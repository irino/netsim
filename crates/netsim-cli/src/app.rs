use clap::{Parser, Subcommand};
use emit_clab::emit_clab_topology;
use emit_zebra::emit_zebra;
use ir::DeviceConfig;
use parse_ios::parse_ios_config;
use parse_junos::parse_junos_config;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use topology::infer_topology;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported vendor '{0}'. Supported vendors: ios, junos")]
    UnsupportedVendor(String),
    #[error("No configuration files found in '{0}'")]
    NoConfigsFound(PathBuf),
    #[error("Conversion error: {0}")]
    Conversion(String),
}

#[derive(Parser, Debug)]
#[command(
    name = "netsim",
    version,
    about = "Network Simulator & Config Translator"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Convert a single vendor configuration to zebra-rs YAML
    Convert {
        /// Vendor type (e.g. 'ios')
        #[arg(long, default_value = "ios")]
        vendor: String,

        /// Path to input configuration file
        #[arg(long, value_name = "FILE")]
        r#in: PathBuf,

        /// Path to output zebra-rs YAML file
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },

    /// Generate complete Containerlab environment and zebra-rs configs from directory
    GenLab {
        /// Directory containing vendor configuration files (.cfg, .conf)
        #[arg(long, value_name = "DIR")]
        configs: PathBuf,

        /// Output directory for zebra-rs YAML configs, topology.clab.yml, and report.md
        #[arg(long, value_name = "DIR")]
        out: PathBuf,

        /// Optional lab name for Containerlab topology
        #[arg(long, default_value = "netsim-lab")]
        lab_name: String,
    },
}

pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Commands::Convert { vendor, r#in, out } => run_convert(&vendor, &r#in, &out),
        Commands::GenLab {
            configs,
            out,
            lab_name,
        } => run_gen_lab(&configs, &out, &lab_name),
    }
}

fn detect_and_parse_config(
    content: &str,
    filename: Option<&str>,
    vendor_hint: Option<&str>,
) -> Result<DeviceConfig, CliError> {
    match vendor_hint {
        Some("ios") => Ok(parse_ios_config(content, filename)),
        Some("junos") => Ok(parse_junos_config(content, filename)),
        Some(v) => Err(CliError::UnsupportedVendor(v.to_string())),
        None => {
            // Auto-detect: if extension is .set or non-empty lines start with 'set ', assume junos
            let is_set_format = content
                .lines()
                .map(|l| l.trim())
                .any(|l| l.starts_with("set "));

            let ext = filename
                .and_then(|f| Path::new(f).extension())
                .and_then(|s| s.to_str());

            if ext == Some("set") || is_set_format {
                Ok(parse_junos_config(content, filename))
            } else {
                Ok(parse_ios_config(content, filename))
            }
        }
    }
}

pub fn run_convert(vendor: &str, in_path: &Path, out_path: &Path) -> Result<(), CliError> {
    if vendor != "ios" && vendor != "junos" {
        return Err(CliError::UnsupportedVendor(vendor.to_string()));
    }

    let content = fs::read_to_string(in_path)?;
    let filename = in_path.file_name().and_then(|s| s.to_str());

    let device = detect_and_parse_config(&content, filename, Some(vendor))?;

    // Print diagnostics to stderr
    for unparsed in &device.unparsed_lines {
        eprintln!(
            "[{}:{}] Unsupported line: '{}' ({})",
            unparsed.file_path.as_deref().unwrap_or("<unknown>"),
            unparsed.line_number,
            unparsed.content,
            unparsed.reason
        );
    }

    let emit_res = emit_zebra(&device)
        .map_err(|e| CliError::Conversion(format!("Failed to emit zebra config: {}", e)))?;

    for warn in &emit_res.warnings {
        eprintln!("[{}] {}", warn.category, warn.message);
    }

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(out_path, emit_res.yaml)?;
    println!(
        "Successfully converted '{}' -> '{}'",
        in_path.display(),
        out_path.display()
    );

    Ok(())
}

pub fn run_gen_lab(configs_dir: &Path, out_dir: &Path, lab_name: &str) -> Result<(), CliError> {
    if !configs_dir.exists() || !configs_dir.is_dir() {
        return Err(CliError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Configs directory '{}' does not exist",
                configs_dir.display()
            ),
        )));
    }

    let mut config_files = Vec::new();
    for entry in fs::read_dir(configs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext == "cfg" || ext == "conf" || ext == "txt" || ext == "set" {
                    config_files.push(path);
                }
            }
        }
    }

    config_files.sort();

    if config_files.is_empty() {
        return Err(CliError::NoConfigsFound(configs_dir.to_path_buf()));
    }

    fs::create_dir_all(out_dir)?;

    let mut devices = Vec::new();
    let mut all_unparsed = Vec::new();

    for cfg_path in &config_files {
        let content = fs::read_to_string(cfg_path)?;
        let filename = cfg_path.file_name().and_then(|s| s.to_str());
        let dev = detect_and_parse_config(&content, filename, None)?;
        for unparsed in &dev.unparsed_lines {
            all_unparsed.push(unparsed.clone());
        }
        devices.push(dev);
    }

    // Infer topology
    let topology = infer_topology(&devices);

    // Emit zebra-rs YAML per device
    let mut all_emit_warnings = Vec::new();
    for dev in &devices {
        let emit_res = emit_zebra(dev)
            .map_err(|e| CliError::Conversion(format!("Failed to emit zebra config: {}", e)))?;
        for w in emit_res.warnings {
            all_emit_warnings.push((dev.hostname.clone(), w));
        }

        let out_file = out_dir.join(format!("{}.zebra.yaml", dev.hostname));
        fs::write(out_file, emit_res.yaml)?;
    }

    // Emit Containerlab topology
    let clab_out = emit_clab_topology(lab_name, &devices, &topology, None).map_err(|e| {
        CliError::Conversion(format!("Failed to emit Containerlab topology: {}", e))
    })?;

    let clab_file = out_dir.join("topology.clab.yml");
    fs::write(clab_file, clab_out.yaml)?;

    // Generate report.md
    let mut report = String::new();
    report.push_str("# netsim Generation Report\n\n");
    report.push_str(&format!("**Lab Name**: `{}`\n", lab_name));
    report.push_str(&format!("**Devices Processed**: {}\n\n", devices.len()));

    report.push_str("## Inferred Links\n\n");
    if topology.links.is_empty() {
        report.push_str("No point-to-point links inferred.\n\n");
    } else {
        for link in &topology.links {
            report.push_str(&format!(
                "- Subnet `{}`: `{}:{}` <-> `{}:{}`\n",
                link.subnet,
                link.endpoint_a.hostname,
                link.endpoint_a.interface_name,
                link.endpoint_b.hostname,
                link.endpoint_b.interface_name
            ));
        }
        report.push('\n');
    }

    if !topology.warnings.is_empty() {
        report.push_str("## Topology Warnings\n\n");
        for w in &topology.warnings {
            report.push_str(&format!("- Subnet `{}`: {}\n", w.subnet, w.message));
        }
        report.push('\n');
    }

    if !all_unparsed.is_empty() {
        report.push_str("## Unparsed / Unsupported Configuration Lines\n\n");
        for u in &all_unparsed {
            report.push_str(&format!(
                "- `{}:{}`: `{}` ({})\n",
                u.file_path.as_deref().unwrap_or("<unknown>"),
                u.line_number,
                u.content,
                u.reason
            ));
        }
        report.push('\n');
    }

    if !all_emit_warnings.is_empty() {
        report.push_str("## Emitter Warnings\n\n");
        for (host, w) in &all_emit_warnings {
            report.push_str(&format!("- `{}` [{}] {}\n", host, w.category, w.message));
        }
        report.push('\n');
    }

    let report_file = out_dir.join("report.md");
    fs::write(report_file, report)?;

    println!(
        "Successfully generated lab '{}' in '{}'",
        lab_name,
        out_dir.display()
    );

    Ok(())
}

use crate::intent::IntentConfig;
use crate::verify::{CommandRunner, LabVerifier, ProcessRunner, VerifyError, VerifyOptions};
use clap::{Parser, Subcommand};
use emit_clab::{emit_clab_topology, EmitClabOptions, RuntimeEngine};
use emit_frr::emit_frr;
use emit_zebra::emit_zebra;
use ir::DeviceConfig;
use parse_eos::parse_eos_config;
use parse_frr::parse_frr_config;
use parse_ios::parse_ios_config;
use parse_iosxr::parse_iosxr_config;
use parse_junos::parse_junos_config;
use reachability::{export_to_batfish, ReachabilitySolver};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use topology::infer_topology;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported vendor '{0}'. Supported vendors: ios, junos, eos, iosxr, frr")]
    UnsupportedVendor(String),
    #[error("Unsupported engine '{0}'. Supported engines: zebra, frr")]
    UnsupportedEngine(String),
    #[error("No configuration files found in '{0}'")]
    NoConfigsFound(PathBuf),
    #[error("Conversion error: {0}")]
    Conversion(String),
    #[error("Static reachability check failed: {0}")]
    CheckFailed(String),
    #[error("Verification error: {0}")]
    Verification(#[from] VerifyError),
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
    /// Convert a single vendor configuration to zebra-rs YAML or FRR config
    Convert {
        /// Vendor type (e.g. 'ios')
        #[arg(long, default_value = "ios")]
        vendor: String,

        /// Target engine (zebra or frr)
        #[arg(long, default_value = "zebra")]
        engine: String,

        /// Path to input configuration file
        #[arg(long, value_name = "FILE")]
        r#in: PathBuf,

        /// Path to output file
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },

    /// Generate complete Containerlab environment and node configs from directory
    GenLab {
        /// Directory containing vendor configuration files (.cfg, .conf)
        #[arg(long, value_name = "DIR")]
        configs: PathBuf,

        /// Output directory for node configs, topology.clab.yml, and report.md
        #[arg(long, value_name = "DIR")]
        out: PathBuf,

        /// Target runtime engine: zebra or frr
        #[arg(long, default_value = "zebra")]
        engine: String,

        /// Optional lab name for Containerlab topology
        #[arg(long, default_value = "netsim-lab")]
        lab_name: String,
    },

    /// Perform fast static reachability checking, subnet consistency analysis, and Batfish export
    Check {
        /// Directory containing vendor configuration files
        #[arg(long, value_name = "DIR")]
        configs: PathBuf,

        /// Optional path to intent assertion file (default: <configs>/intent.yml if present)
        #[arg(long, value_name = "FILE")]
        intent: Option<PathBuf>,

        /// Export Batfish vendor-independent snapshot JSON to this path
        #[arg(long, value_name = "FILE")]
        export_batfish: Option<PathBuf>,
    },

    /// Deploy lab on Containerlab, poll routing convergence, run loopback pings, and assert intent
    #[command(alias = "test")]
    Verify {
        /// Directory containing generated lab artifacts (topology.clab.yml, *.zebra.yaml)
        #[arg(long, value_name = "DIR", default_value = "./lab/")]
        lab_dir: PathBuf,

        /// Optional path to topology.clab.yml file
        #[arg(long, value_name = "FILE")]
        topology: Option<PathBuf>,

        /// Path to intent assertion file (default: <lab-dir>/intent.yml if present)
        #[arg(long, value_name = "FILE")]
        intent: Option<PathBuf>,

        /// Convergence polling timeout in seconds (default: 60)
        #[arg(long, default_value = "60")]
        timeout: u64,

        /// Polling interval in seconds (default: 2)
        #[arg(long, default_value = "2")]
        interval: u64,

        /// Skip containerlab destroy upon completion
        #[arg(long, default_value = "false")]
        skip_destroy: bool,

        /// Execute containerlab and docker commands with sudo
        #[arg(long, default_value = "false")]
        sudo: bool,

        /// Run only static reachability checks without deploying Containerlab
        #[arg(long, default_value = "false")]
        static_only: bool,

        /// Binary path for containerlab
        #[arg(long, default_value = "containerlab")]
        clab_bin: String,

        /// Binary path for docker
        #[arg(long, default_value = "docker")]
        docker_bin: String,
    },
}

pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Commands::Convert {
            vendor,
            engine,
            r#in,
            out,
        } => run_convert(&vendor, &engine, &r#in, &out),
        Commands::GenLab {
            configs,
            out,
            engine,
            lab_name,
        } => run_gen_lab(&configs, &out, &engine, &lab_name),
        Commands::Check {
            configs,
            intent,
            export_batfish: batfish_out,
        } => run_check(&configs, intent.as_deref(), batfish_out.as_deref()),
        Commands::Verify {
            lab_dir,
            topology,
            intent,
            timeout,
            interval,
            skip_destroy,
            sudo,
            static_only,
            clab_bin,
            docker_bin,
        } => {
            let opts = VerifyOptions {
                lab_dir,
                topology_file: topology,
                intent_file: intent,
                timeout: Duration::from_secs(timeout),
                interval: Duration::from_secs(interval),
                skip_destroy,
                sudo,
                static_only,
                clab_bin,
                docker_bin,
            };
            run_verify(&opts, &ProcessRunner)
        }
    }
}

pub fn run_verify(opts: &VerifyOptions, runner: &dyn CommandRunner) -> Result<(), CliError> {
    let verifier = LabVerifier::new(opts, runner);
    verifier.run_pipeline()?;
    println!("==> Verification PASSED successfully!");
    Ok(())
}

fn detect_and_parse_config(
    content: &str,
    filename: Option<&str>,
    vendor_hint: Option<&str>,
) -> Result<DeviceConfig, CliError> {
    let vendor_norm = vendor_hint.map(|v| v.to_ascii_lowercase());
    match vendor_norm.as_deref() {
        Some("ios") | Some("cisco") => Ok(parse_ios_config(content, filename)),
        Some("junos") | Some("juniper") => Ok(parse_junos_config(content, filename)),
        Some("eos") | Some("arista") => Ok(parse_eos_config(content, filename)),
        Some("iosxr") | Some("ios-xr") | Some("xr") => Ok(parse_iosxr_config(content, filename)),
        Some("frr") | Some("sonic") => Ok(parse_frr_config(content, filename)),
        Some(v) => Err(CliError::UnsupportedVendor(v.to_string())),
        None => {
            // Auto-detect:
            // 1. Check filename hints first
            let ext = filename
                .and_then(|f| Path::new(f).extension())
                .and_then(|s| s.to_str());

            if let Some(f) = filename {
                let f_low = f.to_lowercase();
                if ext == Some("set") || f_low.contains("junos") || f_low.contains("juniper") {
                    return Ok(parse_junos_config(content, filename));
                }
                if f_low.contains("xr") || f_low.contains("iosxr") {
                    return Ok(parse_iosxr_config(content, filename));
                }
                if f_low.contains("eos") || f_low.contains("arista") {
                    return Ok(parse_eos_config(content, filename));
                }
                if f_low.contains("frr") || f_low.contains("sonic") {
                    return Ok(parse_frr_config(content, filename));
                }
                if f_low.contains("ios") || f_low.contains("cisco") {
                    return Ok(parse_ios_config(content, filename));
                }
            }

            // 2. If extension is .set or top-level unindented lines start with 'set ', assume junos
            let is_set_format = content
                .lines()
                .filter(|l| {
                    !l.trim().is_empty()
                        && !l.trim().starts_with('#')
                        && !l.trim().starts_with("/*")
                })
                .all(|l| l.starts_with("set "));

            if ext == Some("set") || is_set_format {
                return Ok(parse_junos_config(content, filename));
            }

            // 3. If content contains Cisco IOS XR signatures
            let is_iosxr = content.lines().any(|l| {
                let t = l.trim();
                t.contains("cisco-xr")
                    || t.contains("IOS XR")
                    || t.starts_with("router static")
                    || t.starts_with("prefix-set ")
                    || t == "end-set"
                    || t.starts_with("ipv4 address ")
            });

            if is_iosxr {
                return Ok(parse_iosxr_config(content, filename));
            }

            // 4. If content contains FRRouting / SONiC signatures
            let is_frr = content.lines().any(|l| {
                let t = l.trim();
                t.starts_with("frr version")
                    || t.starts_with("frr defaults")
                    || t.starts_with("ospf router-id")
            });

            if is_frr {
                return Ok(parse_frr_config(content, filename));
            }

            // 5. If content contains Arista EOS signatures
            let is_eos = content.lines().any(|l| {
                let t = l.trim();
                t.contains("EOS-")
                    || t.contains("vEOS")
                    || t.starts_with("transceiver qsfp")
                    || t.starts_with("spanning-tree mode")
                    || t.starts_with("management api")
                    || t.starts_with("service routing protocols model")
            });

            if is_eos {
                Ok(parse_eos_config(content, filename))
            } else {
                Ok(parse_ios_config(content, filename))
            }
        }
    }
}

pub fn run_convert(
    vendor: &str,
    engine: &str,
    in_path: &Path,
    out_path: &Path,
) -> Result<(), CliError> {
    let vendor_lower = vendor.to_ascii_lowercase();
    if vendor_lower != "ios"
        && vendor_lower != "cisco"
        && vendor_lower != "junos"
        && vendor_lower != "juniper"
        && vendor_lower != "eos"
        && vendor_lower != "arista"
        && vendor_lower != "iosxr"
        && vendor_lower != "ios-xr"
        && vendor_lower != "xr"
        && vendor_lower != "frr"
        && vendor_lower != "sonic"
    {
        return Err(CliError::UnsupportedVendor(vendor.to_string()));
    }

    let engine_lower = engine.to_ascii_lowercase();
    let runtime_engine = match engine_lower.as_str() {
        "zebra" | "zebra-rs" => RuntimeEngine::Zebra,
        "frr" | "frrouting" => RuntimeEngine::Frr,
        _ => return Err(CliError::UnsupportedEngine(engine.to_string())),
    };

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

    let output_content = match runtime_engine {
        RuntimeEngine::Zebra => {
            let emit_res = emit_zebra(&device)
                .map_err(|e| CliError::Conversion(format!("Failed to emit zebra config: {}", e)))?;
            for warn in &emit_res.warnings {
                eprintln!("[{}] {}", warn.category, warn.message);
            }
            emit_res.yaml
        }
        RuntimeEngine::Frr => {
            let emit_res = emit_frr(&device)
                .map_err(|e| CliError::Conversion(format!("Failed to emit FRR config: {}", e)))?;
            for warn in &emit_res.warnings {
                eprintln!("[{}] {}", warn.category, warn.message);
            }
            emit_res.conf
        }
    };

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(out_path, output_content)?;
    println!(
        "Successfully converted '{}' -> '{}' (engine: {:?})",
        in_path.display(),
        out_path.display(),
        runtime_engine
    );

    Ok(())
}

pub fn load_devices_from_dir(
    configs_dir: &Path,
) -> Result<(Vec<DeviceConfig>, Vec<ir::UnparsedLine>), CliError> {
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

    Ok((devices, all_unparsed))
}

pub fn run_check(
    configs_dir: &Path,
    intent_override: Option<&Path>,
    batfish_out: Option<&Path>,
) -> Result<(), CliError> {
    println!(
        "==> Running static mathematical reachability checks for '{}'...",
        configs_dir.display()
    );
    let (devices, _) = load_devices_from_dir(configs_dir)?;
    let topology = infer_topology(&devices);
    let solver = ReachabilitySolver::new(&devices, &topology);

    // Consistency check
    let consistency_issues = solver.check_consistency();
    if !consistency_issues.is_empty() {
        eprintln!("Warning: Topology/Subnet consistency issues detected:");
        for issue in &consistency_issues {
            eprintln!("  - {}", issue);
        }
    }

    // Static reachability across all pairs for configured address families
    let has_v4 = devices
        .iter()
        .any(|d| d.interfaces.iter().any(|i| i.ipv4.is_some()));
    let has_v6 = devices
        .iter()
        .any(|d| d.interfaces.iter().any(|i| i.ipv6.is_some()));

    let mut unreachables = Vec::new();
    if has_v4 {
        let v4_matrix = solver.compute_reachability_matrix(false);
        for p in v4_matrix {
            if p.status == reachability::ReachabilityStatus::Unreachable {
                unreachables.push(format!("IPv4 {} -> {}", p.source, p.destination));
            }
        }
    }
    if has_v6 {
        let v6_matrix = solver.compute_reachability_matrix(true);
        for p in v6_matrix {
            if p.status == reachability::ReachabilityStatus::Unreachable {
                unreachables.push(format!("IPv6 {} -> {}", p.source, p.destination));
            }
        }
    }

    if !unreachables.is_empty() {
        return Err(CliError::CheckFailed(format!(
            "Static reachability graph has disconnected components: {}",
            unreachables.join(", ")
        )));
    }

    println!("==> All-pairs static graph reachability PASSED!");

    // Check intent file if present
    let intent_file = intent_override.map(|p| p.to_path_buf()).or_else(|| {
        let candidate = configs_dir.join("intent.yml");
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    });

    if let Some(intent_path) = intent_file {
        println!(
            "==> Statically evaluating intent assertions from '{}'...",
            intent_path.display()
        );
        let intent_cfg = IntentConfig::load_from_file(&intent_path)
            .map_err(|e| CliError::CheckFailed(e.to_string()))?;
        let assertions = intent_cfg
            .compile_assertions()
            .map_err(|e| CliError::CheckFailed(e.to_string()))?;

        for assertion in assertions {
            match assertion {
                crate::intent::Assertion::Ping { from, to } => {
                    let target_str = match to {
                        crate::intent::PingTarget::Ip(ip) => ip,
                        crate::intent::PingTarget::Loopback(r) => r,
                        crate::intent::PingTarget::Router(r) => r,
                    };
                    print!("    static assert: ping {} -> {}... ", from, target_str);
                    match solver.is_target_reachable(&from, &target_str) {
                        Ok(true) => println!("PASSED"),
                        Ok(false) => {
                            println!("FAILED");
                            return Err(CliError::CheckFailed(format!(
                                "Static intent assertion failed: {} cannot reach {}",
                                from, target_str
                            )));
                        }
                        Err(e) => {
                            println!("FAILED ({})", e);
                            return Err(CliError::CheckFailed(format!(
                                "Static intent assertion failed: {}",
                                e
                            )));
                        }
                    }
                }
                crate::intent::Assertion::RouteExists { node, prefix } => {
                    print!("    static assert: route {} has {}... ", node, prefix);
                    if solver.has_route(&node, &prefix) {
                        println!("PASSED");
                    } else {
                        println!("FAILED");
                        return Err(CliError::CheckFailed(format!(
                            "Static route assertion failed: node '{}' has no route to '{}'",
                            node, prefix
                        )));
                    }
                }
            }
        }
        println!("==> All static intent assertions PASSED!");
    }

    // Export Batfish JSON if requested
    if let Some(bf_path) = batfish_out {
        println!(
            "==> Exporting Batfish snapshot to '{}'...",
            bf_path.display()
        );
        let snapshot = export_to_batfish("netsim-snapshot", &devices, &topology);
        let bf_json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| CliError::Conversion(e.to_string()))?;
        if let Some(parent) = bf_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(bf_path, bf_json)?;
        println!("==> Batfish export completed successfully.");
    }

    Ok(())
}

pub fn run_gen_lab(
    configs_dir: &Path,
    out_dir: &Path,
    engine_str: &str,
    lab_name: &str,
) -> Result<(), CliError> {
    let engine_lower = engine_str.to_ascii_lowercase();
    let runtime_engine = match engine_lower.as_str() {
        "zebra" | "zebra-rs" => RuntimeEngine::Zebra,
        "frr" | "frrouting" => RuntimeEngine::Frr,
        _ => return Err(CliError::UnsupportedEngine(engine_str.to_string())),
    };

    let (devices, all_unparsed) = load_devices_from_dir(configs_dir)?;
    fs::create_dir_all(out_dir)?;

    // Infer topology
    let topology = infer_topology(&devices);

    let mut all_emit_warnings = Vec::new();

    match runtime_engine {
        RuntimeEngine::Zebra => {
            for dev in &devices {
                let emit_res = emit_zebra(dev).map_err(|e| {
                    CliError::Conversion(format!("Failed to emit zebra config: {}", e))
                })?;
                for w in emit_res.warnings {
                    all_emit_warnings.push((dev.hostname.clone(), w.category, w.message));
                }

                let out_file = out_dir.join(format!("{}.zebra.yaml", dev.hostname));
                fs::write(out_file, emit_res.yaml)?;
            }
        }
        RuntimeEngine::Frr => {
            for dev in &devices {
                let emit_res = emit_frr(dev).map_err(|e| {
                    CliError::Conversion(format!("Failed to emit FRR config: {}", e))
                })?;
                for w in emit_res.warnings {
                    all_emit_warnings.push((dev.hostname.clone(), w.category, w.message));
                }

                let conf_file = out_dir.join(format!("{}.frr.conf", dev.hostname));
                fs::write(conf_file, emit_res.conf)?;

                let daemons_file = out_dir.join(format!("{}.daemons", dev.hostname));
                fs::write(daemons_file, emit_res.daemons)?;
            }
        }
    }

    // Emit Containerlab topology
    let clab_opts = match runtime_engine {
        RuntimeEngine::Zebra => EmitClabOptions::new_zebra(out_dir.to_string_lossy().to_string()),
        RuntimeEngine::Frr => EmitClabOptions::new_frr(out_dir.to_string_lossy().to_string()),
    };

    let clab_out =
        emit_clab_topology(lab_name, &devices, &topology, Some(clab_opts)).map_err(|e| {
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

    if !topology.lan_segments.is_empty() {
        report.push_str("## Inferred Multi-Access LANs (Bridges)\n\n");
        for (idx, lan) in topology.lan_segments.iter().enumerate() {
            let bridge_name = format!("br-lan{}", idx);
            let ep_strs: Vec<String> = lan
                .endpoints
                .iter()
                .map(|ep| format!("`{}:{}`", ep.hostname, ep.interface_name))
                .collect();
            report.push_str(&format!(
                "- Subnet `{}` via bridge `{}`: {}\n",
                lan.subnet,
                bridge_name,
                ep_strs.join(", ")
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
        for (host, cat, msg) in &all_emit_warnings {
            report.push_str(&format!("- `{}` [{}] {}\n", host, cat, msg));
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

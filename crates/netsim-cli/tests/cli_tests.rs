use netsim_cli::{run, Cli, Commands};
use std::fs;
use std::path::PathBuf;

fn create_temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let unique = format!("netsim_test_{}_{}", name, std::process::id());
    dir.push(unique);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_convert_success_and_diagnostics() {
    let tmp = create_temp_dir("convert_ok");
    let in_file = tmp.join("R1.cfg");
    let out_file = tmp.join("R1.zebra.yaml");

    let cfg_content = r#"
hostname R1
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
unsupported command test
"#;
    fs::write(&in_file, cfg_content).unwrap();

    let cli = Cli {
        command: Commands::Convert {
            vendor: "ios".to_string(),
            r#in: in_file.clone(),
            out: out_file.clone(),
        },
    };

    let res = run(cli);
    assert!(res.is_ok());

    assert!(out_file.exists());
    let emitted = fs::read_to_string(&out_file).unwrap();
    assert!(emitted.contains("hostname: R1"));
    assert!(emitted.contains("- 10.0.0.1/24"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_convert_invalid_vendor_or_missing_file() {
    let tmp = create_temp_dir("convert_err");
    let in_file = tmp.join("missing.cfg");
    let out_file = tmp.join("out.yaml");

    // Missing file
    let cli = Cli {
        command: Commands::Convert {
            vendor: "ios".to_string(),
            r#in: in_file.clone(),
            out: out_file.clone(),
        },
    };
    assert!(run(cli).is_err());

    // Unsupported vendor
    let dummy = tmp.join("dummy.cfg");
    fs::write(&dummy, "hostname R1").unwrap();
    let cli_bad_vendor = Cli {
        command: Commands::Convert {
            vendor: "junos".to_string(),
            r#in: dummy,
            out: out_file,
        },
    };
    assert!(run(cli_bad_vendor).is_err());

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_gen_lab_success_and_report() {
    let tmp = create_temp_dir("gen_lab_ok");
    let configs_dir = tmp.join("configs");
    let out_dir = tmp.join("lab");
    fs::create_dir_all(&configs_dir).unwrap();

    let r1_cfg = r#"
hostname R1
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
router ospf 1
 router-id 1.1.1.1
 network 10.0.0.0 0.0.0.255 area 0
!
unsupported global stmt
"#;
    let r2_cfg = r#"
hostname R2
!
interface GigabitEthernet0/0
 ip address 10.0.0.2 255.255.255.0
 no shutdown
!
router ospf 1
 router-id 2.2.2.2
 network 10.0.0.0 0.0.0.255 area 0
"#;
    fs::write(configs_dir.join("R1.cfg"), r1_cfg).unwrap();
    fs::write(configs_dir.join("R2.cfg"), r2_cfg).unwrap();

    let cli = Cli {
        command: Commands::GenLab {
            configs: configs_dir.clone(),
            out: out_dir.clone(),
            lab_name: "test-topo".to_string(),
        },
    };

    assert!(run(cli).is_ok());

    // Check generated files
    assert!(out_dir.join("R1.zebra.yaml").exists());
    assert!(out_dir.join("R2.zebra.yaml").exists());
    assert!(out_dir.join("topology.clab.yml").exists());
    assert!(out_dir.join("report.md").exists());

    let clab_yaml = fs::read_to_string(out_dir.join("topology.clab.yml")).unwrap();
    assert!(clab_yaml.contains("name: test-topo"));
    assert!(clab_yaml.contains("R1:eth1"));
    assert!(clab_yaml.contains("R2:eth1"));

    let report_md = fs::read_to_string(out_dir.join("report.md")).unwrap();
    assert!(report_md.contains("**Lab Name**: `test-topo`"));
    assert!(report_md.contains("**Devices Processed**: 2"));
    assert!(report_md.contains("Subnet `10.0.0.0/24`"));
    assert!(report_md.contains("unsupported global stmt"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_gen_lab_empty_directory_error() {
    let tmp = create_temp_dir("gen_lab_empty");
    let configs_dir = tmp.join("empty_configs");
    let out_dir = tmp.join("out");
    fs::create_dir_all(&configs_dir).unwrap();

    let cli = Cli {
        command: Commands::GenLab {
            configs: configs_dir,
            out: out_dir,
            lab_name: "test".to_string(),
        },
    };

    assert!(run(cli).is_err());
    let _ = fs::remove_dir_all(&tmp);
}

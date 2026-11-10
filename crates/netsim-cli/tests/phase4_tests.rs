use netsim_cli::{run, Cli, Commands};
use std::fs;
use std::path::{Path, PathBuf};

fn find_examples_dir(name: &str) -> PathBuf {
    let p1 = Path::new("examples").join(name);
    if p1.exists() {
        return p1;
    }
    let p2 = Path::new("../../examples").join(name);
    if p2.exists() {
        return p2;
    }
    panic!("Could not locate examples/{} directory", name);
}

fn create_temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let unique = format!("netsim_phase4_{}_{}", name, std::process::id());
    dir.push(unique);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_phase4_frr_engine_lab_generation() {
    let tmp = create_temp_dir("frr_lab");
    let configs_dir = find_examples_dir("ospf-3routers");
    let out_dir = tmp.join("lab-frr");

    let cli = Cli {
        command: Commands::GenLab {
            configs: configs_dir,
            out: out_dir.clone(),
            engine: "frr".to_string(),
            lab_name: "ospf-frr-lab".to_string(),
        },
    };

    let res = run(cli);
    assert!(
        res.is_ok(),
        "GenLab with FRR engine should succeed: {:?}",
        res.err()
    );

    // Verify FRR config and daemons files generated
    assert!(out_dir.join("R1.frr.conf").exists());
    assert!(out_dir.join("R1.daemons").exists());
    assert!(out_dir.join("R2.frr.conf").exists());
    assert!(out_dir.join("R2.daemons").exists());
    assert!(out_dir.join("R3.frr.conf").exists());
    assert!(out_dir.join("R3.daemons").exists());
    assert!(out_dir.join("topology.clab.yml").exists());

    let r1_conf = fs::read_to_string(out_dir.join("R1.frr.conf")).unwrap();
    assert!(r1_conf.contains("hostname R1"));
    assert!(r1_conf.contains("router ospf"));

    let r1_daemons = fs::read_to_string(out_dir.join("R1.daemons")).unwrap();
    assert!(r1_daemons.contains("zebra=yes"));
    assert!(r1_daemons.contains("ospfd=yes"));

    let clab_yaml = fs::read_to_string(out_dir.join("topology.clab.yml")).unwrap();
    assert!(clab_yaml.contains("image: quay.io/frrouting/frr:10.2.0"));
    assert!(clab_yaml.contains("/etc/frr/frr.conf"));
    assert!(clab_yaml.contains("/etc/frr/daemons"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_phase4_check_subcommand_and_batfish_export() {
    let tmp = create_temp_dir("check_batfish");
    let configs_dir = find_examples_dir("ospf-3routers");
    let batfish_out = tmp.join("batfish-snapshot.json");

    let cli = Cli {
        command: Commands::Check {
            configs: configs_dir,
            intent: None,
            export_batfish: Some(batfish_out.clone()),
        },
    };

    let res = run(cli);
    assert!(res.is_ok(), "Check command should succeed: {:?}", res.err());

    assert!(batfish_out.exists());
    let bf_json = fs::read_to_string(&batfish_out).unwrap();
    assert!(bf_json.contains("\"name\": \"netsim-snapshot\""));
    assert!(bf_json.contains("\"R1\""));
    assert!(bf_json.contains("\"R2\""));
    assert!(bf_json.contains("\"R3\""));
    assert!(bf_json.contains("\"default_vrf\""));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_phase4_convert_to_frr() {
    let tmp = create_temp_dir("convert_frr");
    let in_file = find_examples_dir("ospf-3routers").join("R1.cfg");
    let out_file = tmp.join("R1.frr.conf");

    let cli = Cli {
        command: Commands::Convert {
            vendor: "ios".to_string(),
            engine: "frr".to_string(),
            r#in: in_file,
            out: out_file.clone(),
        },
    };

    let res = run(cli);
    assert!(res.is_ok());

    assert!(out_file.exists());
    let conf = fs::read_to_string(&out_file).unwrap();
    assert!(conf.contains("hostname R1"));
    assert!(conf.contains("router ospf"));
    assert!(conf.contains("interface GigabitEthernet0/0"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_phase4_verify_static_only() {
    let tmp = create_temp_dir("verify_static");
    let in_dir = find_examples_dir("ospf-3routers");
    let lab_dir = tmp.join("lab");

    // 1. Generate lab
    let gen_cli = Cli {
        command: Commands::GenLab {
            configs: in_dir,
            out: lab_dir.clone(),
            engine: "zebra".to_string(),
            lab_name: "test-static-verify".to_string(),
        },
    };
    assert!(run(gen_cli).is_ok());

    // 2. Run verify with static-only
    let verify_cli = Cli {
        command: Commands::Verify {
            lab_dir,
            topology: None,
            intent: None,
            timeout: 10,
            interval: 1,
            skip_destroy: true,
            sudo: false,
            static_only: true,
            clab_bin: "containerlab".to_string(),
            docker_bin: "docker".to_string(),
        },
    };

    let res = run(verify_cli);
    assert!(
        res.is_ok(),
        "Verify static-only should succeed without docker/clab"
    );

    let _ = fs::remove_dir_all(&tmp);
}

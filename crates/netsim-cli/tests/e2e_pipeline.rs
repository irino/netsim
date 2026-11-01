use emit_clab::emit_clab_topology;
use emit_zebra::emit_zebra;
use netsim_cli::{run, Cli, Commands};
use parse_ios::parse_ios_config;
use parse_junos::parse_junos_config;
use std::fs;
use std::path::{Path, PathBuf};
use topology::infer_topology;

fn create_temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let unique = format!("netsim_e2e_{}_{}", name, std::process::id());
    dir.push(unique);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

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

#[test]
fn test_e2e_ospf_3routers_pipeline() {
    let example_dir = find_examples_dir("ospf-3routers");

    // 1. Test programmatic crate-level pipeline
    let r1_content = fs::read_to_string(example_dir.join("R1.cfg")).unwrap();
    let r2_content = fs::read_to_string(example_dir.join("R2.cfg")).unwrap();
    let r3_content = fs::read_to_string(example_dir.join("R3.cfg")).unwrap();

    let r1 = parse_ios_config(&r1_content, Some("R1.cfg"));
    let r2 = parse_ios_config(&r2_content, Some("R2.cfg"));
    let r3 = parse_ios_config(&r3_content, Some("R3.cfg"));

    assert_eq!(r1.hostname, "R1");
    assert_eq!(r2.hostname, "R2");
    assert_eq!(r3.hostname, "R3");

    // Verify OSPF router IDs and interface areas
    assert_eq!(
        r1.ospf.as_ref().unwrap().router_id.unwrap().to_string(),
        "1.1.1.1"
    );
    assert_eq!(
        r2.ospf.as_ref().unwrap().router_id.unwrap().to_string(),
        "2.2.2.2"
    );
    assert_eq!(
        r3.ospf.as_ref().unwrap().router_id.unwrap().to_string(),
        "3.3.3.3"
    );

    let devices = vec![r1, r2, r3];

    // Infer topology
    let topo = infer_topology(&devices);
    assert_eq!(
        topo.links.len(),
        3,
        "Expected 3 point-to-point links between 3 routers"
    );
    assert!(
        topo.warnings.is_empty(),
        "No topology warnings expected for clean /24 p2p links"
    );

    let subnets: Vec<String> = topo.links.iter().map(|l| l.subnet.to_string()).collect();
    assert!(subnets.contains(&"10.0.12.0/24".to_string()));
    assert!(subnets.contains(&"10.0.13.0/24".to_string()));
    assert!(subnets.contains(&"10.0.23.0/24".to_string()));

    // Verify zebra-rs configuration generation
    for dev in &devices {
        let zebra_res = emit_zebra(dev).unwrap();
        assert!(zebra_res
            .yaml
            .contains(&format!("hostname: {}", dev.hostname)));
        assert!(zebra_res.yaml.contains("router:"));
        assert!(zebra_res.yaml.contains("ospf:"));
    }

    // Verify Containerlab topology generation
    let clab_out = emit_clab_topology("ospf-mesh", &devices, &topo, None).unwrap();
    assert!(clab_out.yaml.contains("name: ospf-mesh"));
    assert!(clab_out.yaml.contains("R1:"));
    assert!(clab_out.yaml.contains("R2:"));
    assert!(clab_out.yaml.contains("R3:"));

    // 2. Test CLI end-to-end `gen-lab` execution
    let tmp_out = create_temp_dir("ospf3_cli");
    let cli = Cli {
        command: Commands::GenLab {
            configs: example_dir,
            out: tmp_out.clone(),
            lab_name: "ospf-triangle".to_string(),
        },
    };

    let res = run(cli);
    assert!(res.is_ok(), "CLI gen-lab command must succeed");

    assert!(tmp_out.join("R1.zebra.yaml").exists());
    assert!(tmp_out.join("R2.zebra.yaml").exists());
    assert!(tmp_out.join("R3.zebra.yaml").exists());
    assert!(tmp_out.join("topology.clab.yml").exists());
    assert!(tmp_out.join("report.md").exists());

    let report_content = fs::read_to_string(tmp_out.join("report.md")).unwrap();
    assert!(report_content.contains("**Lab Name**: `ospf-triangle`"));
    assert!(report_content.contains("**Devices Processed**: 3"));
    assert!(report_content.contains("10.0.12.0/24"));
    assert!(report_content.contains("10.0.13.0/24"));
    assert!(report_content.contains("10.0.23.0/24"));

    let clab_yaml = fs::read_to_string(tmp_out.join("topology.clab.yml")).unwrap();
    assert!(clab_yaml.contains("name: ospf-triangle"));

    let _ = fs::remove_dir_all(&tmp_out);
}

#[test]
fn test_e2e_multi_access_lan_pipeline() {
    let tmp_cfg_dir = create_temp_dir("lan_cfg");

    // Create 3 router configs sharing a single /24 multi-access subnet
    let r1_cfg = r#"
hostname r1
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
router ospf 1
 router-id 1.1.1.1
 network 10.0.0.0 0.0.0.255 area 0
"#;
    let r2_cfg = r#"
hostname r2
interface GigabitEthernet0/0
 ip address 10.0.0.2 255.255.255.0
 no shutdown
router ospf 1
 router-id 2.2.2.2
 network 10.0.0.0 0.0.0.255 area 0
"#;
    let r3_cfg = r#"
hostname r3
interface GigabitEthernet0/0
 ip address 10.0.0.3 255.255.255.0
 no shutdown
router ospf 1
 router-id 3.3.3.3
 network 10.0.0.0 0.0.0.255 area 0
"#;

    fs::write(tmp_cfg_dir.join("r1.cfg"), r1_cfg).unwrap();
    fs::write(tmp_cfg_dir.join("r2.cfg"), r2_cfg).unwrap();
    fs::write(tmp_cfg_dir.join("r3.cfg"), r3_cfg).unwrap();

    let tmp_out = create_temp_dir("lan_out");
    let cli = Cli {
        command: Commands::GenLab {
            configs: tmp_cfg_dir.clone(),
            out: tmp_out.clone(),
            lab_name: "lan-test".to_string(),
        },
    };

    let res = run(cli);
    assert!(
        res.is_ok(),
        "CLI gen-lab command must succeed for multi-access LAN"
    );

    assert!(tmp_out.join("r1.zebra.yaml").exists());
    assert!(tmp_out.join("r2.zebra.yaml").exists());
    assert!(tmp_out.join("r3.zebra.yaml").exists());
    assert!(tmp_out.join("topology.clab.yml").exists());
    assert!(tmp_out.join("report.md").exists());

    let report_content = fs::read_to_string(tmp_out.join("report.md")).unwrap();
    assert!(report_content.contains("## Inferred Multi-Access LANs (Bridges)"));
    assert!(report_content.contains("Subnet `10.0.0.0/24` via bridge `br-lan0`"));

    let clab_yaml = fs::read_to_string(tmp_out.join("topology.clab.yml")).unwrap();
    assert!(clab_yaml.contains("br-lan0:"));
    assert!(clab_yaml.contains("kind: bridge"));
    assert!(
        !clab_yaml.contains("image: zebra-rs:latest\n      binds:\n      - ./br-lan0.zebra.yaml")
    );
    assert!(clab_yaml.contains("br-lan0:eth1"));
    assert!(clab_yaml.contains("br-lan0:eth2"));
    assert!(clab_yaml.contains("br-lan0:eth3"));

    let _ = fs::remove_dir_all(&tmp_cfg_dir);
    let _ = fs::remove_dir_all(&tmp_out);
}

#[test]
fn test_e2e_mixed_ios_junos_pipeline() {
    let example_dir = find_examples_dir("mixed-ios-junos");

    // 1. Test programmatic multi-vendor parsing and inference
    let cisco_content = fs::read_to_string(example_dir.join("cisco-r1.cfg")).unwrap();
    let junos_content = fs::read_to_string(example_dir.join("junos-r2.set")).unwrap();

    let r1 = parse_ios_config(&cisco_content, Some("cisco-r1.cfg"));
    let r2 = parse_junos_config(&junos_content, Some("junos-r2.set"));

    assert_eq!(r1.hostname, "cisco-r1");
    assert_eq!(r2.hostname, "junos-r2");

    assert_eq!(
        r1.ospf.as_ref().unwrap().router_id.unwrap().to_string(),
        "1.1.1.1"
    );
    assert_eq!(
        r2.ospf.as_ref().unwrap().router_id.unwrap().to_string(),
        "2.2.2.2"
    );

    let devices = vec![r1, r2];

    // Infer topology across mixed vendor routers
    let topo = infer_topology(&devices);
    assert_eq!(
        topo.links.len(),
        1,
        "Expected 1 point-to-point link between cisco-r1 and junos-r2"
    );
    assert_eq!(topo.links[0].subnet.to_string(), "10.0.0.0/30");
    assert!(topo.warnings.is_empty());

    // Check zebra-rs configuration generation for both vendors
    for dev in &devices {
        let zebra_res = emit_zebra(dev).unwrap();
        assert!(zebra_res
            .yaml
            .contains(&format!("hostname: {}", dev.hostname)));
        assert!(zebra_res.yaml.contains("router:"));
        assert!(zebra_res.yaml.contains("ospf:"));
    }

    // 2. Test CLI end-to-end `gen-lab` execution with auto-detection
    let tmp_out = create_temp_dir("mixed_cli");
    let cli = Cli {
        command: Commands::GenLab {
            configs: example_dir,
            out: tmp_out.clone(),
            lab_name: "mixed-lab".to_string(),
        },
    };

    let res = run(cli);
    assert!(
        res.is_ok(),
        "CLI gen-lab command must succeed for mixed vendor directory"
    );

    assert!(tmp_out.join("cisco-r1.zebra.yaml").exists());
    assert!(tmp_out.join("junos-r2.zebra.yaml").exists());
    assert!(tmp_out.join("topology.clab.yml").exists());
    assert!(tmp_out.join("report.md").exists());

    let report_content = fs::read_to_string(tmp_out.join("report.md")).unwrap();
    assert!(report_content.contains("**Lab Name**: `mixed-lab`"));
    assert!(report_content.contains("**Devices Processed**: 2"));
    assert!(report_content.contains("10.0.0.0/30"));

    let clab_yaml = fs::read_to_string(tmp_out.join("topology.clab.yml")).unwrap();
    assert!(clab_yaml.contains("name: mixed-lab"));
    assert!(clab_yaml.contains("cisco-r1:eth1"));
    assert!(clab_yaml.contains("junos-r2:eth1"));

    let _ = fs::remove_dir_all(&tmp_out);
}

#[test]
fn test_e2e_ebgp_2routers_pipeline() {
    let example_dir = find_examples_dir("ebgp-2routers");

    // 1. Test programmatic crate-level pipeline
    let r1_content = fs::read_to_string(example_dir.join("r1.cfg")).unwrap();
    let r2_content = fs::read_to_string(example_dir.join("r2.cfg")).unwrap();

    let r1 = parse_ios_config(&r1_content, Some("r1.cfg"));
    let r2 = parse_ios_config(&r2_content, Some("r2.cfg"));

    assert_eq!(r1.hostname, "r1");
    assert_eq!(r2.hostname, "r2");

    // Verify BGP and static routes
    let r1_bgp = r1.bgp.as_ref().unwrap();
    assert_eq!(r1_bgp.as_number, 65001);
    assert_eq!(r1_bgp.neighbors[0].remote_as, 65002);
    assert_eq!(r1.static_routes.len(), 1);

    let r2_bgp = r2.bgp.as_ref().unwrap();
    assert_eq!(r2_bgp.as_number, 65002);
    assert_eq!(r2_bgp.neighbors[0].remote_as, 65001);
    assert_eq!(r2.static_routes.len(), 1);

    let devices = vec![r1, r2];

    // Infer topology
    let topo = infer_topology(&devices);
    assert_eq!(
        topo.links.len(),
        1,
        "Expected 1 point-to-point link between r1 and r2"
    );
    assert_eq!(topo.links[0].subnet.to_string(), "10.0.0.0/30");
    assert!(topo.warnings.is_empty());

    // Verify zebra-rs configuration generation
    for dev in &devices {
        let zebra_res = emit_zebra(dev).unwrap();
        assert!(zebra_res.yaml.contains("ip:"));
        assert!(zebra_res.yaml.contains("route:"));
        assert!(zebra_res.yaml.contains("router:"));
        assert!(zebra_res.yaml.contains("bgp:"));
    }

    // 2. Test CLI end-to-end `gen-lab` execution
    let tmp_out = create_temp_dir("ebgp2_cli");
    let cli = Cli {
        command: Commands::GenLab {
            configs: example_dir,
            out: tmp_out.clone(),
            lab_name: "ebgp-p2p".to_string(),
        },
    };

    let res = run(cli);
    assert!(res.is_ok(), "CLI gen-lab command must succeed");

    assert!(tmp_out.join("r1.zebra.yaml").exists());
    assert!(tmp_out.join("r2.zebra.yaml").exists());
    assert!(tmp_out.join("topology.clab.yml").exists());
    assert!(tmp_out.join("report.md").exists());

    let report_content = fs::read_to_string(tmp_out.join("report.md")).unwrap();
    assert!(report_content.contains("**Lab Name**: `ebgp-p2p`"));
    assert!(report_content.contains("**Devices Processed**: 2"));
    assert!(report_content.contains("10.0.0.0/30"));

    let clab_yaml = fs::read_to_string(tmp_out.join("topology.clab.yml")).unwrap();
    assert!(clab_yaml.contains("name: ebgp-p2p"));
    assert!(clab_yaml.contains("r1:eth1"));
    assert!(clab_yaml.contains("r2:eth1"));

    let r1_zebra = fs::read_to_string(tmp_out.join("r1.zebra.yaml")).unwrap();
    assert!(r1_zebra.contains("as: 65001"));
    assert!(r1_zebra.contains("remote-as: 65002"));

    let _ = fs::remove_dir_all(&tmp_out);
}

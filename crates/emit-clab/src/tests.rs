use super::*;
use ir::{DeviceConfig, Interface, Ipv4Prefix};
use topology::infer_topology;

fn make_device(name: &str, ifaces: &[(&str, Option<&str>, bool)]) -> DeviceConfig {
    DeviceConfig {
        hostname: name.to_string(),
        interfaces: ifaces
            .iter()
            .map(|(ifname, ip, enabled)| Interface {
                name: ifname.to_string(),
                description: None,
                enabled: *enabled,
                ipv4: ip.map(|s| s.parse::<Ipv4Prefix>().unwrap()),
                ospf: None,
            })
            .collect(),
        ospf: None,
        unparsed_lines: Vec::new(),
    }
}

#[test]
fn test_single_link_between_two_nodes() {
    let r1 = make_device("r1", &[("GigabitEthernet0/0", Some("10.0.0.1/24"), true)]);
    let r2 = make_device("r2", &[("GigabitEthernet0/0", Some("10.0.0.2/24"), true)]);

    let topo = infer_topology(&[r1.clone(), r2.clone()]);
    let clab = emit_clab_topology("lab-p2p", &[r1, r2], &topo, None).unwrap();

    assert!(clab.yaml.contains("name: lab-p2p"));
    assert!(clab.yaml.contains("r1:"));
    assert!(clab.yaml.contains("r2:"));
    assert!(clab.yaml.contains("endpoints:"));
    assert!(clab.yaml.contains("r1:eth1"));
    assert!(clab.yaml.contains("r2:eth1"));
    assert!(clab.warnings.is_empty());
}

#[test]
fn test_multiple_links_and_deterministic_mapping() {
    let r1 = make_device(
        "r1",
        &[
            ("GigabitEthernet0/0", Some("10.0.1.1/24"), true),
            ("GigabitEthernet0/1", Some("10.0.2.1/24"), true),
            ("Loopback0", Some("1.1.1.1/32"), true),
        ],
    );
    let r2 = make_device("r2", &[("GigabitEthernet0/0", Some("10.0.1.2/24"), true)]);
    let r3 = make_device("r3", &[("GigabitEthernet0/0", Some("10.0.2.2/24"), true)]);

    let topo = infer_topology(&[r1.clone(), r2.clone(), r3.clone()]);
    let clab = emit_clab_topology("multi-lab", &[r1, r2, r3], &topo, None).unwrap();

    // Loopback0 should be excluded from ethX mapping
    let r1_map = clab.mapper.device_mappings("r1");
    assert_eq!(r1_map.get("GigabitEthernet0/0").unwrap(), "eth1");
    assert_eq!(r1_map.get("GigabitEthernet0/1").unwrap(), "eth2");
    assert!(!r1_map.contains_key("Loopback0"));

    let val: serde_yaml::Value = serde_yaml::from_str(&clab.yaml).unwrap();
    let links = val["topology"]["links"].as_sequence().unwrap();
    assert_eq!(links.len(), 2);

    let l1_endpoints: Vec<&str> = links[0]["endpoints"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(l1_endpoints, vec!["r1:eth1", "r2:eth1"]);

    let l2_endpoints: Vec<&str> = links[1]["endpoints"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(l2_endpoints, vec!["r1:eth2", "r3:eth1"]);
}

#[test]
fn test_stable_output_ordering() {
    let r1 = make_device("r1", &[("GigabitEthernet0/0", Some("10.0.0.1/24"), true)]);
    let r2 = make_device("r2", &[("GigabitEthernet0/0", Some("10.0.0.2/24"), true)]);

    let topo = infer_topology(&[r1.clone(), r2.clone()]);
    let clab1 = emit_clab_topology("stable", &[r1.clone(), r2.clone()], &topo, None).unwrap();
    let clab2 = emit_clab_topology("stable", &[r2, r1], &topo, None).unwrap();

    assert_eq!(clab1.yaml, clab2.yaml);
}

#[test]
fn test_unsupported_topology_warning_propagation() {
    let r1 = make_device("r1", &[("eth0", Some("10.0.0.1/24"), true)]);
    let r2 = make_device("r2", &[("eth0", Some("10.0.0.2/24"), true)]);
    let r3 = make_device("r3", &[("eth0", Some("10.0.0.3/24"), true)]);

    let topo = infer_topology(&[r1.clone(), r2.clone(), r3.clone()]);
    let clab = emit_clab_topology("warn-lab", &[r1, r2, r3], &topo, None).unwrap();

    assert_eq!(clab.warnings.len(), 1);
    assert!(clab.warnings[0].contains("shared across 3 interfaces"));
    assert!(!clab.yaml.contains("endpoints:"));
}

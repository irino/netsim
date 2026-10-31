use super::*;
use ir::{DeviceConfig, Interface, Ipv4Prefix};
use topology::infer_topology;

#[test]
fn test_clab_golden() {
    let r1 = DeviceConfig {
        hostname: "r1".to_string(),
        interfaces: vec![Interface {
            name: "GigabitEthernet0/0".to_string(),
            description: Some("Link to r2".to_string()),
            enabled: true,
            ipv4: Some("10.0.0.1/24".parse::<Ipv4Prefix>().unwrap()),
            ospf: None,
        }],
        ospf: None,
        bgp: None,
        static_routes: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let r2 = DeviceConfig {
        hostname: "r2".to_string(),
        interfaces: vec![Interface {
            name: "GigabitEthernet0/0".to_string(),
            description: Some("Link to r1".to_string()),
            enabled: true,
            ipv4: Some("10.0.0.2/24".parse::<Ipv4Prefix>().unwrap()),
            ospf: None,
        }],
        ospf: None,
        bgp: None,
        static_routes: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let topo = infer_topology(&[r1.clone(), r2.clone()]);
    let clab = emit_clab_topology("golden-lab", &[r1, r2], &topo, None).unwrap();

    let expected = std::fs::read_to_string("../../tests/golden/clab_golden.yml")
        .or_else(|_| std::fs::read_to_string("tests/golden/clab_golden.yml"))
        .unwrap();

    assert_eq!(clab.yaml.trim(), expected.trim());
}

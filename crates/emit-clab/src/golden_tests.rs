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
            ipv6: None,
            ospf: None,
            ospfv3: None,
            isis: None,
            vxlan: None,
        }],
        ospf: None,
        ospfv3: None,
        isis: None,
        bgp: None,
        segment_routing: None,
        static_routes: Vec::new(),
        static_routes_v6: Vec::new(),
        prefix_lists: Vec::new(),
        prefix_lists_v6: Vec::new(),
        community_lists: Vec::new(),
        policies: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let r2 = DeviceConfig {
        hostname: "r2".to_string(),
        interfaces: vec![Interface {
            name: "GigabitEthernet0/0".to_string(),
            description: Some("Link to r1".to_string()),
            enabled: true,
            ipv4: Some("10.0.0.2/24".parse::<Ipv4Prefix>().unwrap()),
            ipv6: None,
            ospf: None,
            ospfv3: None,
            isis: None,
            vxlan: None,
        }],
        ospf: None,
        ospfv3: None,
        isis: None,
        bgp: None,
        segment_routing: None,
        static_routes: Vec::new(),
        static_routes_v6: Vec::new(),
        prefix_lists: Vec::new(),
        prefix_lists_v6: Vec::new(),
        community_lists: Vec::new(),
        policies: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let topo = infer_topology(&[r1.clone(), r2.clone()]);
    let clab = emit_clab_topology("golden-lab", &[r1, r2], &topo, None).unwrap();

    let expected = std::fs::read_to_string("../../tests/golden/clab_golden.yml")
        .or_else(|_| std::fs::read_to_string("tests/golden/clab_golden.yml"))
        .unwrap();

    assert_eq!(clab.yaml.trim(), expected.trim());
}

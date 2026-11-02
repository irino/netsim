use super::*;
use ir::{DeviceConfig, Interface, InterfaceOspfConfig, OspfAreaId, OspfProcess};

#[test]
fn test_zebra_golden() {
    let dev = DeviceConfig {
        hostname: "router-golden".to_string(),
        interfaces: vec![
            Interface {
                name: "eth1".to_string(),
                description: Some("Uplink".to_string()),
                enabled: true,
                ipv4: Some("10.0.1.1/24".parse().unwrap()),
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: None,
                    passive: None,
                }),
            },
            Interface {
                name: "eth2".to_string(),
                description: None,
                enabled: true,
                ipv4: Some("192.168.100.1/24".parse().unwrap()),
                ospf: None,
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(std::net::Ipv4Addr::new(1, 1, 1, 1)),
        }),
        bgp: None,
        static_routes: Vec::new(),
        prefix_lists: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let expected_yaml = std::fs::read_to_string("../../tests/golden/zebra_golden.yaml")
        .or_else(|_| std::fs::read_to_string("tests/golden/zebra_golden.yaml"))
        .unwrap();

    let actual_yaml = emit_zebra_yaml(&dev).unwrap();
    assert_eq!(actual_yaml.trim(), expected_yaml.trim());
}

use super::*;
use ir::{
    BgpNeighbor, BgpProcess, DeviceConfig, Interface, InterfaceOspfConfig, Ipv4Prefix, OspfAreaId,
    OspfProcess, StaticRoute,
};
use std::net::Ipv4Addr;

fn make_basic_device() -> DeviceConfig {
    DeviceConfig {
        hostname: "router-1".to_string(),
        interfaces: vec![
            Interface {
                name: "enp0s6".to_string(),
                description: Some("Uplink to Core".to_string()),
                enabled: true,
                ipv4: Some("10.0.0.1/24".parse::<Ipv4Prefix>().unwrap()),
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                }),
            },
            Interface {
                name: "enp0s7".to_string(),
                description: Some("Area 1 interface".to_string()),
                enabled: true,
                ipv4: Some("192.168.1.1/24".parse::<Ipv4Prefix>().unwrap()),
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Ipv4(Ipv4Addr::new(0, 0, 0, 1)),
                    cost: Some(20),
                    passive: Some(true),
                }),
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(1, 1, 1, 1)),
        }),
        bgp: None,
        static_routes: Vec::new(),
        prefix_lists: Vec::new(),
        unparsed_lines: Vec::new(),
    }
}

#[test]
fn test_hostname_generation() {
    let dev = DeviceConfig {
        hostname: "test-node".to_string(),
        interfaces: Vec::new(),
        ospf: None,
        bgp: None,
        static_routes: Vec::new(),
        prefix_lists: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let yaml = emit_zebra_yaml(&dev).unwrap();
    assert!(yaml.contains("hostname: test-node"));
}

#[test]
fn test_multiple_interfaces_and_ips() {
    let dev = make_basic_device();
    let yaml = emit_zebra_yaml(&dev).unwrap();

    assert!(yaml.contains("name: enp0s6"));
    assert!(yaml.contains("address:"));
    assert!(yaml.contains("- 10.0.0.1/24"));
    assert!(yaml.contains("name: enp0s7"));
    assert!(yaml.contains("- 192.168.1.1/24"));
}

#[test]
fn test_ospf_router_id_and_area() {
    let dev = make_basic_device();
    let yaml = emit_zebra_yaml(&dev).unwrap();

    assert!(yaml.contains("router-id: 1.1.1.1"));
    assert!(yaml.contains("area-id: '0'"));
    assert!(yaml.contains("area-id: 0.0.0.1"));
    assert!(yaml.contains("cost: 10"));
    assert!(yaml.contains("cost: 20"));
    assert!(yaml.contains("passive: true"));
}

#[test]
fn test_static_routes_emission() {
    let dev = DeviceConfig {
        hostname: "R-STATIC".to_string(),
        interfaces: Vec::new(),
        ospf: None,
        bgp: None,
        static_routes: vec![
            StaticRoute {
                destination: "0.0.0.0/0".parse().unwrap(),
                next_hop: Ipv4Addr::new(10, 0, 0, 1),
            },
            StaticRoute {
                destination: "192.168.0.0/16".parse().unwrap(),
                next_hop: Ipv4Addr::new(10, 0, 0, 2),
            },
        ],
        prefix_lists: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let yaml = emit_zebra_yaml(&dev).unwrap();
    assert!(yaml.contains("ip:"));
    assert!(yaml.contains("route:"));
    assert!(yaml.contains("destination: 0.0.0.0/0"));
    assert!(yaml.contains("next-hop: 10.0.0.1"));
    assert!(yaml.contains("destination: 192.168.0.0/16"));
    assert!(yaml.contains("next-hop: 10.0.0.2"));
}

#[test]
fn test_bgp_emission() {
    let dev = DeviceConfig {
        hostname: "R-BGP".to_string(),
        interfaces: Vec::new(),
        ospf: None,
        bgp: Some(BgpProcess {
            as_number: 65001,
            router_id: Some(Ipv4Addr::new(1, 1, 1, 1)),
            neighbors: vec![BgpNeighbor {
                remote_address: Ipv4Addr::new(10, 0, 0, 2),
                remote_as: 65002,
            }],
            networks: vec!["192.168.1.0/24".parse().unwrap()],
        }),
        static_routes: Vec::new(),
        prefix_lists: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let yaml = emit_zebra_yaml(&dev).unwrap();
    assert!(yaml.contains("router:"));
    assert!(yaml.contains("bgp:"));
    assert!(yaml.contains("as: 65001"));
    assert!(yaml.contains("router-id: 1.1.1.1"));
    assert!(yaml.contains("peer-address: 10.0.0.2"));
    assert!(yaml.contains("remote-as: 65002"));
    assert!(yaml.contains("- 192.168.1.0/24"));
}

#[test]
fn test_deterministic_yaml_output() {
    let dev = make_basic_device();
    let yaml1 = emit_zebra_yaml(&dev).unwrap();
    let yaml2 = emit_zebra_yaml(&dev).unwrap();

    assert_eq!(yaml1, yaml2);
}

#[test]
fn test_yaml_roundtrip_deserialization() {
    let dev = make_basic_device();
    let yaml = emit_zebra_yaml(&dev).unwrap();

    let deserialized: ZebraConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(deserialized.system.hostname, "router-1");
    assert_eq!(deserialized.interface.len(), 2);
    assert!(deserialized.router.is_some());
}

#[test]
fn test_emit_warnings_for_unsupported_features() {
    let dev = DeviceConfig {
        hostname: "R1".to_string(),
        interfaces: vec![Interface {
            name: "eth0".to_string(),
            description: Some("Disabled test link".to_string()),
            enabled: false,
            ipv4: Some("10.0.0.1/24".parse().unwrap()),
            ospf: None,
        }],
        ospf: Some(OspfProcess {
            process_id: 10,
            router_id: None,
        }),
        bgp: None,
        static_routes: Vec::new(),
        prefix_lists: Vec::new(),
        unparsed_lines: vec![ir::UnparsedLine {
            file_path: Some("R1.cfg".to_string()),
            line_number: 12,
            content: "unsupported command".to_string(),
            reason: "not supported".to_string(),
        }],
    };

    let result = emit_zebra(&dev).unwrap();
    assert_eq!(result.warnings.len(), 4);
    let categories: Vec<&str> = result
        .warnings
        .iter()
        .map(|w| w.category.as_str())
        .collect();
    assert!(categories.contains(&"interface-disabled"));
    assert!(categories.contains(&"interface-description"));
    assert!(categories.contains(&"ospf-process-id"));
    assert!(categories.contains(&"unparsed-line"));
}

use super::*;
use ir::{
    DeviceConfig, Interface, InterfaceOspfConfig, Ipv4Prefix, OspfAreaId, OspfProcess, StaticRoute,
};
use std::net::Ipv4Addr;
use std::str::FromStr;

#[test]
fn test_frr_golden_roundtrip() {
    let dev = DeviceConfig {
        hostname: "golden-frr".to_string(),
        interfaces: vec![
            Interface {
                name: "eth0".to_string(),
                description: Some("Management interface".to_string()),
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("192.168.100.2/24").unwrap()),
                ipv6: None,
                ospf: None,
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "eth1".to_string(),
                description: Some("Peering link".to_string()),
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.1.1/30").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(5),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(10, 0, 1, 1)),
        }),
        ospfv3: None,
        isis: None,
        bgp: None,
        segment_routing: None,
        static_routes: vec![StaticRoute {
            destination: Ipv4Prefix::from_str("0.0.0.0/0").unwrap(),
            next_hop: Ipv4Addr::new(192, 168, 100, 1),
        }],
        static_routes_v6: Vec::new(),
        prefix_lists: Vec::new(),
        prefix_lists_v6: Vec::new(),
        community_lists: Vec::new(),
        policies: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let res = emit_frr(&dev).unwrap();
    assert_eq!(res.warnings.len(), 0);

    let expected_conf_snippets = [
        "hostname golden-frr",
        "interface eth0",
        " description Management interface",
        " ip address 192.168.100.2/24",
        "interface eth1",
        " description Peering link",
        " ip address 10.0.1.1/30",
        " ip ospf area 0",
        " ip ospf cost 5",
        "ip route 0.0.0.0/0 192.168.100.1",
        "router ospf",
        " ospf router-id 10.0.1.1",
    ];

    for snippet in &expected_conf_snippets {
        assert!(
            res.conf.contains(snippet),
            "Expected snippet '{}' not found in frr.conf:\n{}",
            snippet,
            res.conf
        );
    }
}

use super::*;
use ir::{DeviceConfig, Interface, InterfaceOspfConfig, Ipv4Prefix, OspfAreaId, OspfProcess};
use std::net::Ipv4Addr;
use std::str::FromStr;
use topology::infer_topology;

fn create_3node_triangle_network() -> (Vec<DeviceConfig>, topology::Topology) {
    let r1 = DeviceConfig {
        hostname: "R1".to_string(),
        interfaces: vec![
            Interface {
                name: "eth1".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.12.1/24").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "eth2".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.13.1/24").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "Loopback0".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("1.1.1.1/32").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: None,
                    passive: Some(true),
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(1, 1, 1, 1)),
        }),
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
        hostname: "R2".to_string(),
        interfaces: vec![
            Interface {
                name: "eth1".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.12.2/24").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "eth2".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.23.2/24").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "Loopback0".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("2.2.2.2/32").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: None,
                    passive: Some(true),
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(2, 2, 2, 2)),
        }),
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

    let r3 = DeviceConfig {
        hostname: "R3".to_string(),
        interfaces: vec![
            Interface {
                name: "eth1".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.13.3/24").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "eth2".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("10.0.23.3/24").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: Some(10),
                    passive: None,
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
            Interface {
                name: "Loopback0".to_string(),
                description: None,
                enabled: true,
                ipv4: Some(Ipv4Prefix::from_str("3.3.3.3/32").unwrap()),
                ipv6: None,
                ospf: Some(InterfaceOspfConfig {
                    area: OspfAreaId::Number(0),
                    cost: None,
                    passive: Some(true),
                    bfd: None,
                }),
                ospfv3: None,
                isis: None,
                vxlan: None,
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(3, 3, 3, 3)),
        }),
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

    let devices = vec![r1, r2, r3];
    let topo = infer_topology(&devices);
    (devices, topo)
}

#[test]
fn test_graph_and_path_finding() {
    let (devices, topo) = create_3node_triangle_network();
    let solver = ReachabilitySolver::new(&devices, &topo);

    let issues = solver.check_consistency();
    assert!(issues.is_empty(), "Issues found: {:?}", issues);

    // Path R1 -> R3
    let path = solver.find_path("R1", "R3", false).unwrap();
    assert_eq!(path.status, ReachabilityStatus::Reachable);
    assert_eq!(path.cost, Some(10));
    assert_eq!(path.hops, vec!["R1", "R3"]);

    // Reachability matrix
    let matrix = solver.compute_reachability_matrix(false);
    assert_eq!(matrix.len(), 9); // 3x3 pairs
    for p in matrix {
        assert_eq!(p.status, ReachabilityStatus::Reachable);
    }

    // Target reachability
    assert!(solver.is_target_reachable("R1", "3.3.3.3").unwrap());
    assert!(solver.is_target_reachable("R2", "1.1.1.1").unwrap());
    assert!(solver.is_target_reachable("R3", "R2").unwrap());

    // Route existence check
    assert!(solver.has_route("R1", "10.0.23.0/24"));
    assert!(solver.has_route("R2", "1.1.1.1/32"));
    assert!(!solver.has_route("R1", "192.168.99.0/24"));
}

#[test]
fn test_batfish_exporter() {
    let (devices, topo) = create_3node_triangle_network();
    let bf = export_to_batfish("test-snapshot", &devices, &topo);

    assert_eq!(bf.name, "test-snapshot");
    assert_eq!(bf.nodes.len(), 3);
    assert_eq!(bf.topology.len(), 3);

    let r1 = bf.nodes.get("R1").unwrap();
    assert_eq!(r1.hostname, "R1");
    assert_eq!(r1.interfaces.len(), 3);
    assert!(r1.interfaces.contains_key("Loopback0"));

    let json = serde_json::to_string_pretty(&bf).unwrap();
    assert!(json.contains("\"hostname\": \"R1\""));
    assert!(json.contains("\"node1\": \"R1\""));
}

#[test]
fn test_vtep_and_srv6_consistency_checks() {
    let r1 = DeviceConfig {
        hostname: "leaf1".to_string(),
        interfaces: vec![Interface {
            name: "vxlan1".to_string(),
            description: None,
            enabled: true,
            ipv4: None,
            ipv6: None,
            ospf: None,
            ospfv3: None,
            isis: None,
            vxlan: Some(ir::InterfaceVxlanConfig {
                source_interface: Some("Loopback99".to_string()),
                local_ip: None,
                udp_port: Some(4789),
                vni_bindings: vec![ir::VniVlanBinding {
                    vni: 10010,
                    vlan: Some(10),
                }],
                default_vni: None,
            }),
        }],
        ospf: None,
        ospfv3: None,
        isis: None,
        bgp: None,
        segment_routing: Some(ir::SegmentRoutingConfig {
            srv6: Some(ir::SrV6Config {
                enabled: true,
                encapsulation_source_address: None,
                locators: vec![ir::SrV6Locator {
                    name: "LOC1".to_string(),
                    prefix: "fc00:0:1::/48".parse().unwrap(),
                    micro_segment: None,
                }],
            }),
            mpls: None,
        }),
        static_routes: vec![],
        static_routes_v6: vec![],
        prefix_lists: vec![],
        prefix_lists_v6: vec![],
        community_lists: vec![],
        policies: vec![],
        unparsed_lines: vec![],
    };

    let r2 = DeviceConfig {
        hostname: "leaf2".to_string(),
        interfaces: vec![],
        ospf: None,
        ospfv3: None,
        isis: None,
        bgp: None,
        segment_routing: Some(ir::SegmentRoutingConfig {
            srv6: Some(ir::SrV6Config {
                enabled: true,
                encapsulation_source_address: None,
                locators: vec![ir::SrV6Locator {
                    name: "LOC1".to_string(),
                    prefix: "fc00:0:1::/48".parse().unwrap(),
                    micro_segment: None,
                }],
            }),
            mpls: None,
        }),
        static_routes: vec![],
        static_routes_v6: vec![],
        prefix_lists: vec![],
        prefix_lists_v6: vec![],
        community_lists: vec![],
        policies: vec![],
        unparsed_lines: vec![],
    };

    let topo = topology::infer_topology(&[r1.clone(), r2.clone()]);
    let devices = vec![r1, r2];
    let solver = ReachabilitySolver::new(&devices, &topo);
    let issues = solver.check_consistency();
    assert!(issues
        .iter()
        .any(|i| i.contains("VTEP source interface 'Loopback99'")));
    assert!(issues
        .iter()
        .any(|i| i.contains("Duplicate SRv6 locator prefix fc00:0:1::/48")));
}

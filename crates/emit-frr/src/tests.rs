use super::*;
use ir::{
    BgpNeighbor, BgpNeighborV6, BgpPeerGroup, BgpProcess, CommunityList, CommunityListEntry,
    DeviceConfig, Interface, InterfaceIsisConfig, InterfaceOspfConfig, Ipv4Prefix, Ipv6Prefix,
    IsisLevel, IsisProcess, Ospf6Process, OspfAreaId, OspfProcess, PolicyAction, PolicyActions,
    PolicyConditions, PolicyDefinition, PolicyStatement, PrefixLength, PrefixList, PrefixListEntry,
    PrefixRange, StaticRoute, UnparsedLine,
};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

#[test]
fn test_emit_frr_basic() {
    let dev = DeviceConfig {
        hostname: "r1".to_string(),
        interfaces: vec![
            Interface {
                name: "eth1".to_string(),
                description: Some("Link to R2".to_string()),
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
            },
            Interface {
                name: "lo".to_string(),
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
            },
        ],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(1, 1, 1, 1)),
        }),
        ospfv3: None,
        isis: None,
        bgp: None,
        static_routes: vec![StaticRoute {
            destination: Ipv4Prefix::from_str("192.168.1.0/24").unwrap(),
            next_hop: Ipv4Addr::new(10, 0, 12, 2),
        }],
        static_routes_v6: Vec::new(),
        prefix_lists: Vec::new(),
        prefix_lists_v6: Vec::new(),
        community_lists: Vec::new(),
        policies: Vec::new(),
        unparsed_lines: Vec::new(),
    };

    let res = emit_frr(&dev).unwrap();
    assert!(res.conf.contains("hostname r1"));
    assert!(res.conf.contains("interface eth1"));
    assert!(res.conf.contains("description Link to R2"));
    assert!(res.conf.contains("ip address 10.0.12.1/24"));
    assert!(res.conf.contains("ip ospf area 0"));
    assert!(res.conf.contains("ip ospf cost 10"));
    assert!(res.conf.contains("router ospf"));
    assert!(res.conf.contains("ospf router-id 1.1.1.1"));
    assert!(res.conf.contains("ip route 192.168.1.0/24 10.0.12.2"));

    // Check daemons
    assert!(res.daemons.contains("zebra=yes"));
    assert!(res.daemons.contains("ospfd=yes"));
    assert!(res.daemons.contains("staticd=yes"));
    assert!(res.daemons.contains("bgpd=no"));
}

#[test]
fn test_emit_frr_advanced_protocols() {
    let dev = DeviceConfig {
        hostname: "core-router".to_string(),
        interfaces: vec![Interface {
            name: "eth1".to_string(),
            description: None,
            enabled: true,
            ipv4: Some(Ipv4Prefix::from_str("10.0.0.1/30").unwrap()),
            ipv6: Some(Ipv6Prefix::from_str("2001:db8::1/64").unwrap()),
            ospf: None,
            ospfv3: Some(InterfaceOspfConfig {
                area: OspfAreaId::Number(0),
                cost: Some(20),
                passive: None,
                bfd: Some(true),
            }),
            isis: Some(InterfaceIsisConfig {
                level: Some(IsisLevel::Level2),
                metric: Some(15),
                passive: None,
                bfd: Some(true),
            }),
        }],
        ospf: None,
        ospfv3: Some(Ospf6Process {
            process_id: 1,
            router_id: Some(Ipv4Addr::new(10, 0, 0, 1)),
        }),
        isis: Some(IsisProcess {
            net: Some("49.0001.0000.0000.0001.00".to_string()),
            level: Some(IsisLevel::Level2),
            te_router_id: None,
        }),
        bgp: Some(BgpProcess {
            as_number: 65001,
            router_id: Some(Ipv4Addr::new(10, 0, 0, 1)),
            cluster_id: Some(Ipv4Addr::new(10, 0, 0, 1)),
            peer_groups: vec![BgpPeerGroup {
                name: "IBGP-PEERS".to_string(),
                remote_as: Some(65001),
                route_reflector_client: Some(true),
                cluster_id: None,
                send_community: Some("standard".to_string()),
                bfd: Some(true),
            }],
            neighbors: vec![BgpNeighbor {
                remote_address: Ipv4Addr::new(10, 0, 0, 2),
                remote_as: 65001,
                peer_group: Some("IBGP-PEERS".to_string()),
                route_reflector_client: None,
                cluster_id: None,
                send_community: None,
                bfd: None,
            }],
            networks: vec![Ipv4Prefix::from_str("192.168.0.0/16").unwrap()],
            neighbors_v6: vec![BgpNeighborV6 {
                remote_address: Ipv6Addr::from_str("2001:db8::2").unwrap(),
                remote_as: 65001,
                peer_group: None,
                route_reflector_client: Some(true),
                cluster_id: None,
                send_community: Some("both".to_string()),
                bfd: Some(true),
            }],
            networks_v6: vec![Ipv6Prefix::from_str("2001:db8:100::/48").unwrap()],
        }),
        static_routes: Vec::new(),
        static_routes_v6: Vec::new(),
        prefix_lists: vec![PrefixList {
            name: "PL-V4".to_string(),
            entries: vec![PrefixListEntry {
                sequence: 10,
                action: PolicyAction::Permit,
                prefix: Ipv4Prefix::from_str("10.0.0.0/8").unwrap(),
                range: Some(PrefixRange {
                    ge: Some(PrefixLength::new(16).unwrap()),
                    le: Some(PrefixLength::new(24).unwrap()),
                }),
            }],
        }],
        prefix_lists_v6: Vec::new(),
        community_lists: vec![CommunityList {
            name: "COMM-SET".to_string(),
            entries: vec![CommunityListEntry {
                sequence: None,
                action: PolicyAction::Permit,
                communities: vec!["65001:100".to_string(), "65001:200".to_string()],
            }],
        }],
        policies: vec![PolicyDefinition {
            name: "RM-BGP-IN".to_string(),
            statements: vec![PolicyStatement {
                name: "10".to_string(),
                conditions: PolicyConditions {
                    match_prefix_set: Some("PL-V4".to_string()),
                    match_prefix_set_v6: None,
                    match_community: Some("COMM-SET".to_string()),
                },
                actions: PolicyActions {
                    policy_result: PolicyAction::Permit,
                    set_metric: Some(50),
                    set_local_preference: Some(150),
                    set_community: Some("65001:10".to_string()),
                },
            }],
        }],
        unparsed_lines: vec![UnparsedLine {
            file_path: Some("test.cfg".to_string()),
            line_number: 42,
            content: "unsupported custom knob".to_string(),
            reason: "testing warning".to_string(),
        }],
    };

    let res = emit_frr(&dev).unwrap();
    assert_eq!(res.warnings.len(), 1);
    assert_eq!(res.warnings[0].category, "unparsed-line");

    assert!(res.conf.contains("router ospf6"));
    assert!(res.conf.contains("ospf6 router-id 10.0.0.1"));
    assert!(res.conf.contains("router isis netsim"));
    assert!(res.conf.contains("net 49.0001.0000.0000.0001.00"));
    assert!(res.conf.contains("router bgp 65001"));
    assert!(res.conf.contains("neighbor IBGP-PEERS peer-group"));
    assert!(res.conf.contains("neighbor 10.0.0.2 peer-group IBGP-PEERS"));
    assert!(res.conf.contains("neighbor 2001:db8::2 activate"));
    assert!(res
        .conf
        .contains("ip prefix-list PL-V4 seq 10 permit 10.0.0.0/8 ge 16 le 24"));
    assert!(res
        .conf
        .contains("ip community-list standard COMM-SET permit 65001:100 65001:200"));
    assert!(res.conf.contains("route-map RM-BGP-IN permit 10"));
    assert!(res.conf.contains("match ip address prefix-list PL-V4"));
    assert!(res.conf.contains("match community COMM-SET"));
    assert!(res.conf.contains("set metric 50"));
    assert!(res.conf.contains("set local-preference 150"));
    assert!(res.conf.contains("set community 65001:10"));

    // Check daemons
    assert!(res.daemons.contains("zebra=yes"));
    assert!(res.daemons.contains("bgpd=yes"));
    assert!(res.daemons.contains("ospf6d=yes"));
    assert!(res.daemons.contains("isisd=yes"));
    assert!(res.daemons.contains("bfdd=yes"));
}

use crate::parser::parse_frr_config;
use ir::{OspfAreaId, PolicyAction};
use std::net::Ipv4Addr;

#[test]
fn test_parse_frr_basic() {
    let config = r#"
! FRRouting configuration
!
frr version 8.1
frr defaults traditional
!
hostname leaf-01
!
interface eth0
 description Management
 ip address 192.168.100.10/24
 no shutdown
!
interface swp1
 description Uplink to Spine
 ip address 10.0.0.1/30
 ip ospf area 0
 ip ospf cost 10
!
interface lo
 ip address 10.255.0.1/32
 ip ospf passive
!
router ospf
 ospf router-id 10.255.0.1
 network 10.255.0.1/32 area 0
!
ip route 0.0.0.0/0 10.0.0.2
!
"#;

    let dev = parse_frr_config(config, Some("frr.conf"));
    assert_eq!(dev.hostname, "leaf-01");
    assert_eq!(dev.interfaces.len(), 3);

    let swp1 = &dev.interfaces[1];
    assert_eq!(swp1.name, "swp1");
    assert_eq!(swp1.ipv4.as_ref().unwrap().to_string(), "10.0.0.1/30");
    assert_eq!(swp1.ospf.as_ref().unwrap().area, OspfAreaId::Number(0));
    assert_eq!(swp1.ospf.as_ref().unwrap().cost, Some(10));

    let lo = &dev.interfaces[2];
    assert_eq!(lo.name, "lo");
    assert!(lo.is_loopback());
    assert_eq!(lo.ospf.as_ref().unwrap().passive, Some(true));

    let ospf = dev.ospf.as_ref().unwrap();
    assert_eq!(ospf.router_id, Some(Ipv4Addr::new(10, 255, 0, 1)));

    assert_eq!(dev.static_routes.len(), 1);
    assert_eq!(dev.static_routes[0].destination.to_string(), "0.0.0.0/0");
    assert_eq!(dev.static_routes[0].next_hop, Ipv4Addr::new(10, 0, 0, 2));

    assert!(dev
        .unparsed_lines
        .iter()
        .any(|u| u.content.contains("frr version")));
}

#[test]
fn test_parse_frr_bgp_and_prefix_list() {
    let config = r#"
hostname border-gw
!
router bgp 65000
 bgp router-id 10.255.0.2
 neighbor 10.0.0.2 remote-as 65001
 address-family ipv4 unicast
  network 10.255.0.2/32
  network 172.16.0.0/16
 exit-address-family
!
ip prefix-list PFX-ALLOWED seq 10 permit 10.0.0.0/8 ge 16 le 24
ip prefix-list PFX-ALLOWED seq 20 deny 0.0.0.0/0
!
"#;

    let dev = parse_frr_config(config, Some("frr.conf"));
    assert_eq!(dev.hostname, "border-gw");

    let bgp = dev.bgp.as_ref().unwrap();
    assert_eq!(bgp.as_number, 65000);
    assert_eq!(bgp.router_id, Some(Ipv4Addr::new(10, 255, 0, 2)));
    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].remote_address, Ipv4Addr::new(10, 0, 0, 2));
    assert_eq!(bgp.neighbors[0].remote_as, 65001);
    assert_eq!(bgp.networks.len(), 2);
    assert_eq!(bgp.networks[0].to_string(), "10.255.0.2/32");
    assert_eq!(bgp.networks[1].to_string(), "172.16.0.0/16");

    assert_eq!(dev.prefix_lists.len(), 1);
    let pl = &dev.prefix_lists[0];
    assert_eq!(pl.name, "PFX-ALLOWED");
    assert_eq!(pl.entries.len(), 2);
    assert_eq!(pl.entries[0].action, PolicyAction::Permit);
    assert_eq!(pl.entries[1].action, PolicyAction::Deny);
}

#[test]
fn test_parse_frr_ipv6() {
    let input = r#"!
frr version 8.2
hostname frr-v6
!
interface eth1
 description IPv6 uplink
 ipv6 address 2001:db8:1::1/64
 ipv6 ospf6 area 0
 no shutdown
!
ipv6 route 2001:db8:2::/64 2001:db8:1::2
!
router ospf6
 ospf6 router-id 1.1.1.1
 interface eth1 area 0
!
router bgp 65001
 bgp router-id 1.1.1.1
 neighbor 2001:db8:1::2 remote-as 65002
 address-family ipv6 unicast
  network 2001:db8:2::/64
 exit-address-family
!
"#;
    let dev = parse_frr_config(input, Some("frr-v6.conf"));
    assert_eq!(dev.hostname, "frr-v6");
    assert_eq!(dev.interfaces.len(), 1);
    let iface = &dev.interfaces[0];
    assert_eq!(iface.name, "eth1");
    assert_eq!(iface.ipv6.as_ref().unwrap().to_string(), "2001:db8:1::1/64");
    assert!(iface.ospfv3.is_some());
    assert_eq!(
        iface.ospfv3.as_ref().unwrap().area,
        ir::OspfAreaId::Number(0)
    );

    assert_eq!(dev.static_routes_v6.len(), 1);
    assert_eq!(
        dev.static_routes_v6[0].destination.to_string(),
        "2001:db8:2::/64"
    );
    assert_eq!(
        dev.static_routes_v6[0].next_hop.to_string(),
        "2001:db8:1::2"
    );

    assert!(dev.ospfv3.is_some());
    assert_eq!(dev.ospfv3.as_ref().unwrap().process_id, 1);
    assert_eq!(
        dev.ospfv3.as_ref().unwrap().router_id,
        Some(Ipv4Addr::new(1, 1, 1, 1))
    );

    assert!(dev.bgp.is_some());
    let bgp = dev.bgp.as_ref().unwrap();
    assert_eq!(bgp.neighbors_v6.len(), 1);
    assert_eq!(
        bgp.neighbors_v6[0].remote_address.to_string(),
        "2001:db8:1::2"
    );
    assert_eq!(bgp.neighbors_v6[0].remote_as, 65002);
    assert_eq!(bgp.networks_v6.len(), 1);
    assert_eq!(bgp.networks_v6[0].to_string(), "2001:db8:2::/64");
}

#[test]
fn test_parse_frr_ipv6_prefix_list_and_route_map() {
    let cfg = r#"
hostname frr-r1
!
ipv6 prefix-list PL-V6 seq 10 permit 2001:db8:1::/48 ge 64 le 96
ipv6 prefix-list PL-V6 seq 20 deny ::/0 le 128
!
route-map RM-POLICY permit 10
 match ip address prefix-list PL-V4
 set metric 50
 set local-preference 150
!
route-map RM-POLICY deny 20
 match ipv6 address prefix-list PL-V6
!
"#;
    let dev = parse_frr_config(cfg, Some("frr.conf"));
    assert_eq!(dev.prefix_lists_v6.len(), 1);
    assert_eq!(dev.prefix_lists_v6[0].name, "PL-V6");
    assert_eq!(dev.prefix_lists_v6[0].entries.len(), 2);
    assert_eq!(dev.policies.len(), 1);
    let pol = &dev.policies[0];
    assert_eq!(pol.name, "RM-POLICY");
    assert_eq!(pol.statements.len(), 2);
    assert_eq!(pol.statements[0].actions.set_metric, Some(50));
    assert_eq!(pol.statements[0].actions.set_local_preference, Some(150));
    assert_eq!(
        pol.statements[1].actions.policy_result,
        ir::PolicyAction::Deny
    );
}

#[test]
fn test_parse_frr_isis_and_advanced_bgp() {
    let input = r#"
hostname frr-test
!
interface eth0
 ip address 10.0.0.1/24
 ip router isis
 isis metric 15 level-2
 isis bfd
!
interface lo
 ip address 1.1.1.1/32
 ip router isis
 isis passive
!
router isis
 net 49.0001.0001.0001.0001.00
 is-type level-2
!
ip community-list standard COMM_PERMIT permit 65000:100
!
route-map RM_SET_COMM permit 10
 match community COMM_PERMIT
 set community 65000:300
!
router bgp 65000
 bgp router-id 1.1.1.1
 bgp cluster-id 1.1.1.1
 neighbor LEAF peer-group
 neighbor LEAF remote-as 65000
 neighbor LEAF route-reflector-client
 neighbor LEAF send-community
 neighbor LEAF bfd
 neighbor 10.0.0.2 peer-group LEAF
!
"#;
    let config = parse_frr_config(input, None);
    assert_eq!(config.hostname, "frr-test");

    // IS-IS
    let isis = config.isis.expect("IS-IS process should exist");
    assert_eq!(isis.net.as_deref(), Some("49.0001.0001.0001.0001.00"));
    assert_eq!(isis.level, Some(ir::IsisLevel::Level2));

    let eth = config.interfaces.iter().find(|i| i.name == "eth0").unwrap();
    let isis_if = eth.isis.as_ref().unwrap();
    assert_eq!(isis_if.metric, Some(15));
    assert_eq!(isis_if.level, Some(ir::IsisLevel::Level2));
    assert_eq!(isis_if.bfd, Some(true));

    let lo = config.interfaces.iter().find(|i| i.name == "lo").unwrap();
    assert_eq!(lo.isis.as_ref().unwrap().passive, Some(true));

    // BGP
    let bgp = config.bgp.expect("BGP process should exist");
    assert_eq!(bgp.cluster_id, Some("1.1.1.1".parse().unwrap()));
    assert_eq!(bgp.peer_groups.len(), 1);
    let pg = &bgp.peer_groups[0];
    assert_eq!(pg.name, "LEAF");
    assert_eq!(pg.remote_as, Some(65000));
    assert_eq!(pg.route_reflector_client, Some(true));
    assert_eq!(pg.send_community.as_deref(), Some("standard"));
    assert_eq!(pg.bfd, Some(true));

    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].peer_group.as_deref(), Some("LEAF"));

    // Community list & Route Map
    assert_eq!(config.community_lists.len(), 1);
    assert_eq!(config.community_lists[0].name, "COMM_PERMIT");
    assert_eq!(config.policies.len(), 1);
    assert_eq!(
        config.policies[0].statements[0]
            .conditions
            .match_community
            .as_deref(),
        Some("COMM_PERMIT")
    );
    assert_eq!(
        config.policies[0].statements[0]
            .actions
            .set_community
            .as_deref(),
        Some("65000:300")
    );
}

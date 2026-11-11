use crate::parser::parse_iosxr_config;
use ir::{OspfAreaId, PolicyAction};
use std::net::Ipv4Addr;

#[test]
fn test_parse_iosxr_basic_router() {
    let config = r#"!RANCID-CONTENT-TYPE: cisco-xr
!
hostname xr-router1
!
interface Loopback0
 description Router ID
 ipv4 address 10.255.0.1/32
!
interface GigabitEthernet0/0/0/0
 description Link to Core
 ipv4 address 10.0.1.1 255.255.255.252
 no shutdown
!
interface GigabitEthernet0/0/0/1
 shutdown
 ipv4 address 10.0.2.1/30
!
router static
 address-family ipv4 unicast
  172.16.0.0/16 10.0.1.2
 !
!
router ospf 100
 router-id 10.255.0.1
 area 0
  interface Loopback0
   passive enable
  !
  interface GigabitEthernet0/0/0/0
   cost 10
  !
 !
!
"#;

    let dev = parse_iosxr_config(config, Some("xr-router1.cfg"));
    assert_eq!(dev.hostname, "xr-router1");
    assert_eq!(dev.interfaces.len(), 3);

    let lo0 = &dev.interfaces[0];
    assert_eq!(lo0.name, "Loopback0");
    assert_eq!(lo0.ipv4.as_ref().unwrap().to_string(), "10.255.0.1/32");
    assert_eq!(lo0.ospf.as_ref().unwrap().area, OspfAreaId::Number(0));
    assert_eq!(lo0.ospf.as_ref().unwrap().passive, Some(true));

    let gi0 = &dev.interfaces[1];
    assert_eq!(gi0.name, "GigabitEthernet0/0/0/0");
    assert_eq!(gi0.ipv4.as_ref().unwrap().to_string(), "10.0.1.1/30");
    assert!(gi0.enabled);
    assert_eq!(gi0.ospf.as_ref().unwrap().area, OspfAreaId::Number(0));
    assert_eq!(gi0.ospf.as_ref().unwrap().cost, Some(10));

    let gi1 = &dev.interfaces[2];
    assert!(!gi1.enabled);

    assert_eq!(dev.static_routes.len(), 1);
    assert_eq!(
        dev.static_routes[0].destination.to_string(),
        "172.16.0.0/16"
    );
    assert_eq!(dev.static_routes[0].next_hop, Ipv4Addr::new(10, 0, 1, 2));

    let ospf = dev.ospf.as_ref().unwrap();
    assert_eq!(ospf.process_id, 100);
    assert_eq!(ospf.router_id, Some(Ipv4Addr::new(10, 255, 0, 1)));
}

#[test]
fn test_parse_iosxr_bgp_and_prefix_set() {
    let config = r#"!
hostname xr-bgp-r1
!
prefix-set my_prefixes
 10.0.0.0/8 ge 16 le 24,
 192.168.0.0/16
end-set
!
router bgp 65100
 bgp router-id 10.255.0.1
 address-family ipv4 unicast
  network 10.255.0.1/32
 !
 neighbor 10.0.1.2
  remote-as 65200
 !
!
"#;

    let dev = parse_iosxr_config(config, Some("xr-bgp-r1.cfg"));
    assert_eq!(dev.hostname, "xr-bgp-r1");

    let bgp = dev.bgp.as_ref().unwrap();
    assert_eq!(bgp.as_number, 65100);
    assert_eq!(bgp.router_id, Some(Ipv4Addr::new(10, 255, 0, 1)));
    assert_eq!(bgp.networks.len(), 1);
    assert_eq!(bgp.networks[0].to_string(), "10.255.0.1/32");
    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].remote_address, Ipv4Addr::new(10, 0, 1, 2));
    assert_eq!(bgp.neighbors[0].remote_as, 65200);

    assert_eq!(dev.prefix_lists.len(), 1);
    let pl = &dev.prefix_lists[0];
    assert_eq!(pl.name, "my_prefixes");
    assert_eq!(pl.entries.len(), 2);
    assert_eq!(pl.entries[0].action, PolicyAction::Permit);
    assert_eq!(pl.entries[0].prefix.to_string(), "10.0.0.0/8");
    assert_eq!(
        pl.entries[0].range.as_ref().unwrap().ge.unwrap().value(),
        16
    );
    assert_eq!(
        pl.entries[0].range.as_ref().unwrap().le.unwrap().value(),
        24
    );
    assert_eq!(pl.entries[1].prefix.to_string(), "192.168.0.0/16");
}

#[test]
fn test_parse_iosxr_ipv6() {
    let input = r#"!RANCID-CONTENT-TYPE: cisco-xr
!
hostname iosxr-v6
!
interface GigabitEthernet0/0/0/0
 description IPv6 interface
 ipv6 address 2001:db8:1::1/64
 no shutdown
!
router static
 address-family ipv6 unicast
  2001:db8:2::/64 2001:db8:1::2
 !
!
router ospfv3 100
 router-id 1.1.1.1
 area 0
  interface GigabitEthernet0/0/0/0
   cost 20
  !
 !
!
router bgp 65001
 bgp router-id 1.1.1.1
 address-family ipv6 unicast
  network 2001:db8:2::/64
 !
 neighbor 2001:db8:1::2
  remote-as 65002
 !
!
"#;
    let dev = parse_iosxr_config(input, Some("iosxr-v6.cfg"));
    assert_eq!(dev.hostname, "iosxr-v6");
    assert_eq!(dev.interfaces.len(), 1);
    let iface = &dev.interfaces[0];
    assert_eq!(iface.name, "GigabitEthernet0/0/0/0");
    assert_eq!(iface.ipv6.as_ref().unwrap().to_string(), "2001:db8:1::1/64");
    assert!(iface.ospfv3.is_some());
    let ospfv3 = iface.ospfv3.as_ref().unwrap();
    assert_eq!(ospfv3.area, ir::OspfAreaId::Number(0));
    assert_eq!(ospfv3.cost, Some(20));

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
    assert_eq!(dev.ospfv3.as_ref().unwrap().process_id, 100);
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
fn test_parse_iosxr_ipv6_prefix_set_and_route_policy() {
    let cfg = r#"
hostname iosxr-r1
!
prefix-set V6_PFX
  2001:db8:1::/48,
  2001:db8::/32 ge 48 le 64
end-set
!
route-policy RPL-TEST
  if destination in V6_PFX then
    set med 100
    set local-preference 200
    pass
  else
    drop
  endif
end-policy
!
"#;
    let dev = parse_iosxr_config(cfg, Some("iosxr.cfg"));
    assert_eq!(dev.prefix_lists_v6.len(), 1);
    assert_eq!(dev.prefix_lists_v6[0].name, "V6_PFX");
    assert_eq!(dev.prefix_lists_v6[0].entries.len(), 2);
    assert_eq!(dev.policies.len(), 1);
    let pol = &dev.policies[0];
    assert_eq!(pol.name, "RPL-TEST");
    assert_eq!(pol.statements.len(), 2);
    assert_eq!(
        pol.statements[0].actions.policy_result,
        ir::PolicyAction::Permit
    );
    assert_eq!(pol.statements[0].actions.set_metric, Some(100));
    assert_eq!(pol.statements[0].actions.set_local_preference, Some(200));
    assert_eq!(
        pol.statements[1].actions.policy_result,
        ir::PolicyAction::Deny
    );
}

#[test]
fn test_parse_iosxr_isis_and_advanced_bgp() {
    let input = r#"
hostname iosxr-test
!
interface GigabitEthernet0/0/0/0
 ipv4 address 10.0.0.1 255.255.255.0
!
interface Loopback0
 ipv4 address 1.1.1.1 255.255.255.255
!
router isis 100
 is-type level-2-only
 net 49.0001.0001.0001.0001.00
 interface GigabitEthernet0/0/0/0
  point-to-point
  metric 18
  bfd fast-detect
 !
 interface Loopback0
  passive
 !
!
route-policy RPL_COMM
  if community matches-any COMM_SET then
    set community (65000:700)
    pass
  endif
end-policy
!
router bgp 65000
 bgp router-id 1.1.1.1
 cluster-id 1.1.1.1
 neighbor-group SPINE_GRP
  remote-as 65000
  route-reflector-client
  send-community-ebgp
  bfd fast-detect
 !
 neighbor 10.0.0.2
  use neighbor-group SPINE_GRP
 !
!
"#;
    let config = parse_iosxr_config(input, None);
    assert_eq!(config.hostname, "iosxr-test");

    // IS-IS
    let isis = config.isis.expect("IS-IS process should exist");
    assert_eq!(isis.net.as_deref(), Some("49.0001.0001.0001.0001.00"));
    assert_eq!(isis.level, Some(ir::IsisLevel::Level2));

    let eth = config
        .interfaces
        .iter()
        .find(|i| i.name == "GigabitEthernet0/0/0/0")
        .unwrap();
    let isis_if = eth.isis.as_ref().unwrap();
    assert_eq!(isis_if.metric, Some(18));
    assert_eq!(isis_if.bfd, Some(true));

    let lo = config
        .interfaces
        .iter()
        .find(|i| i.name == "Loopback0")
        .unwrap();
    assert_eq!(lo.isis.as_ref().unwrap().passive, Some(true));

    // BGP
    let bgp = config.bgp.expect("BGP process should exist");
    assert_eq!(bgp.cluster_id, Some("1.1.1.1".parse().unwrap()));
    assert_eq!(bgp.peer_groups.len(), 1);
    let pg = &bgp.peer_groups[0];
    assert_eq!(pg.name, "SPINE_GRP");
    assert_eq!(pg.remote_as, Some(65000));
    assert_eq!(pg.route_reflector_client, Some(true));
    assert_eq!(pg.send_community.as_deref(), Some("standard"));
    assert_eq!(pg.bfd, Some(true));

    assert_eq!(bgp.neighbors.len(), 1);
    let nbr = &bgp.neighbors[0];
    assert_eq!(nbr.peer_group.as_deref(), Some("SPINE_GRP"));
    assert_eq!(nbr.remote_as, 65000);
    assert_eq!(nbr.route_reflector_client, Some(true));
    assert_eq!(nbr.bfd, Some(true));

    // RPL community
    assert_eq!(config.policies.len(), 1);
    let stmt = &config.policies[0].statements[0];
    assert_eq!(stmt.conditions.match_community.as_deref(), Some("COMM_SET"));
    assert_eq!(stmt.actions.set_community.as_deref(), Some("65000:700"));
}

#[test]
fn test_parse_iosxr_evpn_vxlan_and_srv6() {
    let input = r#"
hostname xr-leaf1
!
interface Loopback0
 ipv4 address 10.0.0.1 255.255.255.255
!
interface nve1
 source-interface Loopback0
 member vni 10010 vlan 10
 member vni 10020 vlan 20
 vxlan udp-port 4789
!
router bgp 65001
 bgp router-id 10.0.0.1
 neighbor 10.0.0.100
  remote-as 65001
  address-family l2vpn evpn
  !
 !
!
segment-routing
 srv6
  encapsulation
   source-address 2001:db8:1::1
  !
  locators
   locator LOC1
    micro-segment behavior unode
    prefix fc00:0:1::/48
   !
  !
 !
!
"#;

    let config = parse_iosxr_config(input, Some("xr-leaf1.cfg"));
    assert_eq!(config.hostname, "xr-leaf1");

    let nve = config
        .interfaces
        .iter()
        .find(|i| i.name == "nve1")
        .expect("nve1");
    let vxlan = nve.vxlan.as_ref().expect("vxlan");
    assert_eq!(vxlan.source_interface.as_deref(), Some("Loopback0"));
    assert_eq!(vxlan.vni_bindings.len(), 2);
    assert_eq!(vxlan.vni_bindings[0].vni, 10010);
    assert_eq!(vxlan.vni_bindings[0].vlan, Some(10));

    let bgp = config.bgp.expect("bgp");
    let evpn = bgp.evpn.expect("evpn");
    assert!(evpn.enabled);
    assert_eq!(evpn.neighbors.len(), 1);
    assert_eq!(evpn.neighbors[0].peer, "10.0.0.100");

    let sr = config.segment_routing.expect("sr");
    let srv6 = sr.srv6.expect("srv6");
    assert_eq!(
        srv6.encapsulation_source_address,
        Some("2001:db8:1::1".parse().unwrap())
    );
    assert_eq!(srv6.locators.len(), 1);
    assert_eq!(srv6.locators[0].name, "LOC1");
    assert_eq!(srv6.locators[0].prefix, "fc00:0:1::/48".parse().unwrap());
    assert!(srv6.locators[0].micro_segment.as_ref().unwrap().enabled);
}

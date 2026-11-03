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

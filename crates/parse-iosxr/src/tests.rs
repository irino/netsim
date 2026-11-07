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

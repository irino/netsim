use crate::parser::parse_eos_config;
use ir::{OspfAreaId, PolicyAction};
use std::net::Ipv4Addr;

#[test]
fn test_parse_basic_router() {
    let config = r#"
! Command: show running-config
! device: spine1 (vEOS-lab, EOS-4.28.0F)
!
transceiver qsfp default-arm
!
hostname spine1
!
spanning-tree mode mstp
!
interface Ethernet1
   description Uplink to leaf1
   no shutdown
   ip address 10.0.1.1/30
   ip ospf area 0
   ip ospf cost 10
!
interface Loopback0
   ip address 10.255.0.1/32
!
router ospf 1
   router-id 10.255.0.1
   network 10.255.0.1/32 area 0
!
ip route 0.0.0.0/0 10.0.1.2
!
"#;

    let dev = parse_eos_config(config, Some("spine1.cfg"));
    assert_eq!(dev.hostname, "spine1");
    assert_eq!(dev.interfaces.len(), 2);

    let eth1 = &dev.interfaces[0];
    assert_eq!(eth1.name, "Ethernet1");
    assert_eq!(eth1.description.as_deref(), Some("Uplink to leaf1"));
    assert!(eth1.enabled);
    assert_eq!(eth1.ipv4.as_ref().unwrap().to_string(), "10.0.1.1/30");
    assert_eq!(eth1.ospf.as_ref().unwrap().area, OspfAreaId::Number(0));
    assert_eq!(eth1.ospf.as_ref().unwrap().cost, Some(10));

    let lo0 = &dev.interfaces[1];
    assert_eq!(lo0.name, "Loopback0");
    assert_eq!(lo0.ipv4.as_ref().unwrap().to_string(), "10.255.0.1/32");
    assert_eq!(lo0.ospf.as_ref().unwrap().area, OspfAreaId::Number(0));

    let ospf = dev.ospf.as_ref().unwrap();
    assert_eq!(ospf.process_id, 1);
    assert_eq!(ospf.router_id, Some(Ipv4Addr::new(10, 255, 0, 1)));

    assert_eq!(dev.static_routes.len(), 1);
    assert_eq!(dev.static_routes[0].destination.to_string(), "0.0.0.0/0");
    assert_eq!(dev.static_routes[0].next_hop, Ipv4Addr::new(10, 0, 1, 2));

    // Verify unparsed lines include unhandled global lines
    assert!(dev
        .unparsed_lines
        .iter()
        .any(|u| u.content.contains("transceiver qsfp")));
    assert!(dev
        .unparsed_lines
        .iter()
        .any(|u| u.content.contains("spanning-tree mode mstp")));
}

#[test]
fn test_bgp_and_prefix_lists() {
    let config = r#"
hostname leaf1
!
interface Ethernet1
   shutdown
   ip address 10.0.1.2 255.255.255.252
!
router bgp 65001
   router-id 10.255.0.2
   neighbor 10.0.1.1 remote-as 65000
   network 10.255.0.2/32
   network 192.168.1.0 mask 255.255.255.0
!
ip prefix-list PL-BGP seq 10 permit 10.255.0.0/16 ge 24 le 32
ip prefix-list PL-BGP seq 20 deny 0.0.0.0/0
!
"#;

    let dev = parse_eos_config(config, Some("leaf1.cfg"));
    assert_eq!(dev.hostname, "leaf1");
    let eth1 = &dev.interfaces[0];
    assert!(!eth1.enabled);
    assert_eq!(eth1.ipv4.as_ref().unwrap().to_string(), "10.0.1.2/30");

    let bgp = dev.bgp.as_ref().unwrap();
    assert_eq!(bgp.as_number, 65001);
    assert_eq!(bgp.router_id, Some(Ipv4Addr::new(10, 255, 0, 2)));
    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].remote_address, Ipv4Addr::new(10, 0, 1, 1));
    assert_eq!(bgp.neighbors[0].remote_as, 65000);
    assert_eq!(bgp.networks.len(), 2);
    assert_eq!(bgp.networks[0].to_string(), "10.255.0.2/32");
    assert_eq!(bgp.networks[1].to_string(), "192.168.1.0/24");

    assert_eq!(dev.prefix_lists.len(), 1);
    let pl = &dev.prefix_lists[0];
    assert_eq!(pl.name, "PL-BGP");
    assert_eq!(pl.entries.len(), 2);
    assert_eq!(pl.entries[0].sequence, 10);
    assert_eq!(pl.entries[0].action, PolicyAction::Permit);
    assert_eq!(pl.entries[0].prefix.to_string(), "10.255.0.0/16");
    assert_eq!(
        pl.entries[0].range.as_ref().unwrap().ge.unwrap().value(),
        24
    );
    assert_eq!(
        pl.entries[0].range.as_ref().unwrap().le.unwrap().value(),
        32
    );
    assert_eq!(pl.entries[1].sequence, 20);
    assert_eq!(pl.entries[1].action, PolicyAction::Deny);
    assert_eq!(pl.entries[1].prefix.to_string(), "0.0.0.0/0");
}

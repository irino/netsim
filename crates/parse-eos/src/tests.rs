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

#[test]
fn test_parse_eos_ipv6() {
    let input = r#"!
hostname eos-v6
!
interface Ethernet1
   description IPv6 uplink
   no shutdown
   ipv6 address 2001:db8:1::1/64
   ipv6 ospf 1 area 0
!
ipv6 route 2001:db8:2::/64 2001:db8:1::2
!
router ospf3 1
   router-id 1.1.1.1
!
router bgp 65001
   router-id 1.1.1.1
   neighbor 2001:db8:1::2 remote-as 65002
   address-family ipv6
      network 2001:db8:2::/64
   exit-address-family
!
"#;
    let dev = parse_eos_config(input, Some("eos-v6.cfg"));
    assert_eq!(dev.hostname, "eos-v6");
    assert_eq!(dev.interfaces.len(), 1);
    let iface = &dev.interfaces[0];
    assert_eq!(iface.name, "Ethernet1");
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
fn test_parse_eos_ipv6_prefix_list_and_route_map() {
    let cfg = r#"
hostname eos-r1
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
    let dev = parse_eos_config(cfg, Some("eos.cfg"));
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
fn test_parse_eos_isis_and_advanced_bgp() {
    let input = r#"
hostname eos-test
!
interface Ethernet1
   no shutdown
   ip address 10.0.0.1/24
   isis enable
   isis metric 25
   isis bfd
!
interface Loopback0
   ip address 1.1.1.1/32
   isis enable
   isis passive
!
router isis
   net 49.0001.0001.0001.0001.00
   is-type level-2
!
ip community-list COMM_EOS permit 65000:100
!
route-map RM_EOS permit 10
   match community COMM_EOS
   set community 65000:500
!
router bgp 65000
   router-id 1.1.1.1
   cluster-id 1.1.1.1
   neighbor SPINE peer group
   neighbor SPINE remote-as 65000
   neighbor SPINE route-reflector-client
   neighbor SPINE send-community
   neighbor SPINE bfd
   neighbor 10.0.0.2 peer group SPINE
!
"#;
    let config = parse_eos_config(input, None);
    assert_eq!(config.hostname, "eos-test");

    // IS-IS
    let isis = config.isis.expect("IS-IS process should exist");
    assert_eq!(isis.net.as_deref(), Some("49.0001.0001.0001.0001.00"));
    assert_eq!(isis.level, Some(ir::IsisLevel::Level2));

    let eth = config
        .interfaces
        .iter()
        .find(|i| i.name == "Ethernet1")
        .unwrap();
    let isis_if = eth.isis.as_ref().unwrap();
    assert_eq!(isis_if.metric, Some(25));
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
    assert_eq!(pg.name, "SPINE");
    assert_eq!(pg.remote_as, Some(65000));
    assert_eq!(pg.route_reflector_client, Some(true));
    assert_eq!(pg.send_community.as_deref(), Some("standard"));
    assert_eq!(pg.bfd, Some(true));

    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].peer_group.as_deref(), Some("SPINE"));

    // Community list & Route Map
    assert_eq!(config.community_lists.len(), 1);
    assert_eq!(config.community_lists[0].name, "COMM_EOS");
    assert_eq!(config.policies.len(), 1);
    assert_eq!(
        config.policies[0].statements[0]
            .conditions
            .match_community
            .as_deref(),
        Some("COMM_EOS")
    );
    assert_eq!(
        config.policies[0].statements[0]
            .actions
            .set_community
            .as_deref(),
        Some("65000:500")
    );
}

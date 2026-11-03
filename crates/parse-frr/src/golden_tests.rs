use crate::parser::parse_frr_config;

#[test]
fn test_frr_golden_config() {
    let input = r#"!
! FRRouting configuration for SONiC / FRR
!
frr version 8.2.2
frr defaults datacenter
hostname sonic-leaf01
!
interface eth0
 ip address 10.10.10.1/24
 no shutdown
!
interface lo
 ip address 10.255.0.1/32
!
interface Ethernet0
 description Uplink to spine01
 ip address 10.0.1.1/31
 ip ospf area 0
 ip ospf cost 5
!
router ospf
 ospf router-id 10.255.0.1
 network 10.255.0.1/32 area 0
!
router bgp 65100
 bgp router-id 10.255.0.1
 neighbor 10.0.1.0 remote-as 65000
 address-family ipv4 unicast
  network 10.255.0.1/32
 exit-address-family
!
ip route 192.168.0.0/16 10.0.1.0
!
ip prefix-list PL-DC seq 5 permit 10.0.0.0/8 le 24
!
"#;

    let dev = parse_frr_config(input, Some("frr.conf"));
    let json = serde_json::to_string_pretty(&dev).unwrap();

    assert_eq!(dev.hostname, "sonic-leaf01");
    assert_eq!(dev.interfaces.len(), 3);
    assert_eq!(dev.static_routes.len(), 1);
    assert!(dev.ospf.is_some());
    assert!(dev.bgp.is_some());
    assert_eq!(dev.prefix_lists.len(), 1);

    assert!(json.contains("\"hostname\": \"sonic-leaf01\""));
    assert!(json.contains("\"address\": \"10.0.1.1\""));
    assert!(json.contains("\"router-id\": \"10.255.0.1\""));
    assert!(json.contains("\"as-number\": 65100"));
}

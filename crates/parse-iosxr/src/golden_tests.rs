use crate::parser::parse_iosxr_config;

#[test]
fn test_cisco_iosxr_golden_config() {
    let input = r#"!RANCID-CONTENT-TYPE: cisco-xr
!
hostname asr9k-edge
!
interface Loopback0
 description Management Loopback
 ipv4 address 192.168.0.1 255.255.255.255
!
interface GigabitEthernet0/0/0/0
 description Core uplink to P1
 ipv4 address 10.10.10.1/30
 no shutdown
!
interface GigabitEthernet0/0/0/1
 description Core uplink to P2
 ipv4 address 10.10.10.5/30
 shutdown
!
router static
 address-family ipv4 unicast
  10.0.0.0/8 10.10.10.2
 !
!
router ospf 1
 router-id 192.168.0.1
 area 0.0.0.0
  interface Loopback0
   passive enable
  !
  interface GigabitEthernet0/0/0/0
   cost 5
  !
 !
!
router bgp 65000
 bgp router-id 192.168.0.1
 address-family ipv4 unicast
  network 192.168.0.1/32
 !
 neighbor 10.10.10.2
  remote-as 65001
 !
!
prefix-set PFX_IN
 10.0.0.0/8 le 24,
 192.168.0.0/16
end-set
!
"#;

    let dev = parse_iosxr_config(input, Some("asr9k-edge.cfg"));
    let json = serde_json::to_string_pretty(&dev).unwrap();

    assert_eq!(dev.hostname, "asr9k-edge");
    assert_eq!(dev.interfaces.len(), 3);
    assert_eq!(dev.static_routes.len(), 1);
    assert!(dev.ospf.is_some());
    assert!(dev.bgp.is_some());
    assert_eq!(dev.prefix_lists.len(), 1);

    assert!(json.contains("\"hostname\": \"asr9k-edge\""));
    assert!(json.contains("\"address\": \"10.10.10.1\""));
    assert!(json.contains("\"router-id\": \"192.168.0.1\""));
    assert!(json.contains("\"as-number\": 65000"));
}

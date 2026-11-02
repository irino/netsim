use crate::parser::parse_eos_config;

#[test]
fn test_arista_eos_golden_config() {
    let input = r#"!
! Boot configuration generated at 2026-03-30
!
transceiver qsfp default-arm-mode
!
hostname spine-01
!
spanning-tree mode none
!
interface Ethernet1
   description Link to leaf-01
   no shutdown
   ip address 10.0.10.1/31
   ip ospf cost 5
   ip ospf area 0.0.0.0
!
interface Ethernet2
   description Link to leaf-02
   shutdown
   ip address 10.0.10.3/31
   ip ospf area 0
!
interface Loopback0
   description Router ID
   ip address 10.255.1.1/32
!
ip route 172.16.0.0/16 10.0.10.2
ip route 192.168.0.0 255.255.0.0 10.0.10.4
!
router ospf 100
   router-id 10.255.1.1
   network 10.255.1.1/32 area 0
!
router bgp 65000
   router-id 10.255.1.1
   neighbor 10.0.10.2 remote-as 65001
   network 10.255.1.1/32
!
ip prefix-list PFX-IN seq 10 permit 10.0.0.0/8 le 24
!
management api http-commands
   no shutdown
!
end
"#;

    let dev = parse_eos_config(input, Some("spine-01.cfg"));
    let json = serde_json::to_string_pretty(&dev).unwrap();

    // Verify key fields in JSON representation
    assert_eq!(dev.hostname, "spine-01");
    assert_eq!(dev.interfaces.len(), 3);
    assert_eq!(dev.static_routes.len(), 2);
    assert!(dev.ospf.is_some());
    assert!(dev.bgp.is_some());
    assert_eq!(dev.prefix_lists.len(), 1);

    println!("Serialized JSON:\n{}", json);
    assert!(json.contains("\"hostname\": \"spine-01\""));
    assert!(json.contains("\"address\": \"10.0.10.1\""));
    assert!(json.contains("\"prefix-length\": 31"));
    assert!(json.contains("\"router-id\": \"10.255.1.1\""));
    assert!(json.contains("\"as-number\": 65000"));
}

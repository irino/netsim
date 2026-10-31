use super::*;
use ir::OspfAreaId;
use std::net::Ipv4Addr;

#[test]
fn test_parse_empty_and_comments() {
    let cfg = "!\n! Comment line\n   \n";
    let device = parse_ios_config(cfg, Some("empty.cfg"));
    assert_eq!(device.hostname, "router");
    assert!(device.interfaces.is_empty());
    assert!(device.ospf.is_none());
    assert!(device.bgp.is_none());
    assert!(device.static_routes.is_empty());
    assert!(device.unparsed_lines.is_empty());
}

#[test]
fn test_parse_basic_router() {
    let cfg = r#"
hostname R1
!
interface GigabitEthernet0/0
 description Link to R2
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
interface GigabitEthernet0/1
 ip address 192.168.1.1 255.255.255.0
 shutdown
!
"#;

    let device = parse_ios_config(cfg, Some("R1.cfg"));
    assert_eq!(device.hostname, "R1");
    assert_eq!(device.interfaces.len(), 2);

    let if0 = &device.interfaces[0];
    assert_eq!(if0.name, "GigabitEthernet0/0");
    assert_eq!(if0.description.as_deref(), Some("Link to R2"));
    assert!(if0.enabled);
    assert_eq!(if0.ipv4.as_ref().unwrap().to_string(), "10.0.0.1/24");

    let if1 = &device.interfaces[1];
    assert_eq!(if1.name, "GigabitEthernet0/1");
    assert!(!if1.enabled);
    assert_eq!(if1.ipv4.as_ref().unwrap().to_string(), "192.168.1.1/24");

    assert!(device.unparsed_lines.is_empty());
}

#[test]
fn test_invalid_subnet_mask() {
    let cfg = r#"
hostname R_BAD_MASK
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.240.128
"#;

    let device = parse_ios_config(cfg, Some("bad_mask.cfg"));
    assert_eq!(device.interfaces.len(), 1);
    assert!(device.interfaces[0].ipv4.is_none());
    assert_eq!(device.unparsed_lines.len(), 1);
    assert_eq!(device.unparsed_lines[0].line_number, 4);
    assert_eq!(
        device.unparsed_lines[0].content,
        "ip address 10.0.0.1 255.255.240.128"
    );
    assert!(device.unparsed_lines[0]
        .reason
        .contains("invalid IP address mask"));
}

#[test]
fn test_ospf_router_id_and_wildcard_matching() {
    let cfg = r#"
hostname R2
!
interface GigabitEthernet0/0
 ip address 10.1.1.1 255.255.255.0
!
interface GigabitEthernet0/1
 ip address 172.16.2.1 255.255.0.0
!
interface GigabitEthernet0/2
 ip address 192.168.10.1 255.255.255.0
!
router ospf 1
 router-id 2.2.2.2
 network 10.1.0.0 0.0.255.255 area 0
 network 172.16.2.1 0.0.0.0 area 0.0.0.1
!
"#;

    let device = parse_ios_config(cfg, Some("R2.cfg"));
    assert_eq!(device.hostname, "R2");
    assert_eq!(device.interfaces.len(), 3);

    let ospf = device.ospf.as_ref().unwrap();
    assert_eq!(ospf.process_id, 1);
    assert_eq!(ospf.router_id, Some(Ipv4Addr::new(2, 2, 2, 2)));

    // GigabitEthernet0/0 matches network 10.1.0.0 0.0.255.255 area 0
    let if0_ospf = device.interfaces[0].ospf.as_ref().unwrap();
    assert_eq!(if0_ospf.area, OspfAreaId::Number(0));

    // GigabitEthernet0/1 matches network 172.16.2.1 0.0.0.0 area 0.0.0.1
    let if1_ospf = device.interfaces[1].ospf.as_ref().unwrap();
    assert_eq!(if1_ospf.area, OspfAreaId::Ipv4(Ipv4Addr::new(0, 0, 0, 1)));

    // GigabitEthernet0/2 does not match any OSPF network statement
    assert!(device.interfaces[2].ospf.is_none());
}

#[test]
fn test_static_routes_parsing() {
    let cfg = r#"
hostname R_STATIC
ip route 10.0.0.0 255.0.0.0 192.168.1.254
ip route 0.0.0.0 0.0.0.0 192.168.1.1
ip route 172.16.0.0 255.255.240.128 192.168.1.2
"#;

    let device = parse_ios_config(cfg, Some("static.cfg"));
    assert_eq!(device.static_routes.len(), 2);
    assert_eq!(
        device.static_routes[0].destination.to_string(),
        "10.0.0.0/8"
    );
    assert_eq!(
        device.static_routes[0].next_hop,
        Ipv4Addr::new(192, 168, 1, 254)
    );
    assert_eq!(device.static_routes[1].destination.to_string(), "0.0.0.0/0");
    assert_eq!(
        device.static_routes[1].next_hop,
        Ipv4Addr::new(192, 168, 1, 1)
    );

    // One invalid mask static route
    assert_eq!(device.unparsed_lines.len(), 1);
    assert!(device.unparsed_lines[0]
        .reason
        .contains("invalid static route mask"));
}

#[test]
fn test_bgp_parsing() {
    let cfg = r#"
hostname R_BGP
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.252
!
router bgp 65001
 bgp router-id 1.1.1.1
 neighbor 10.0.0.2 remote-as 65002
 network 192.168.1.0 mask 255.255.255.0
!
"#;

    let device = parse_ios_config(cfg, Some("bgp.cfg"));
    assert_eq!(device.hostname, "R_BGP");
    assert!(device.ospf.is_none());

    let bgp = device.bgp.as_ref().unwrap();
    assert_eq!(bgp.as_number, 65001);
    assert_eq!(bgp.router_id, Some(Ipv4Addr::new(1, 1, 1, 1)));
    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].remote_address, Ipv4Addr::new(10, 0, 0, 2));
    assert_eq!(bgp.neighbors[0].remote_as, 65002);
    assert_eq!(bgp.networks.len(), 1);
    assert_eq!(bgp.networks[0].to_string(), "192.168.1.0/24");
    assert!(device.unparsed_lines.is_empty());
}

#[test]
fn test_unparsed_lines_and_line_numbers() {
    let cfg = r#"
hostname R3
service password-encryption
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 ip nbar protocol-discovery
!
router ospf 10
 auto-cost reference-bandwidth 1000
!
"#;

    let device = parse_ios_config(cfg, Some("R3.cfg"));
    assert_eq!(device.unparsed_lines.len(), 3);

    assert_eq!(device.unparsed_lines[0].line_number, 3);
    assert_eq!(
        device.unparsed_lines[0].content,
        "service password-encryption"
    );

    assert_eq!(device.unparsed_lines[1].line_number, 7);
    assert_eq!(
        device.unparsed_lines[1].content,
        "ip nbar protocol-discovery"
    );

    assert_eq!(device.unparsed_lines[2].line_number, 10);
    assert_eq!(
        device.unparsed_lines[2].content,
        "auto-cost reference-bandwidth 1000"
    );
}

#[test]
fn test_malformed_inputs_without_panic() {
    let cfg = r#"
hostname
interface
 ip address invalid ip
router ospf invalid
 router-id invalid
 network invalid
router bgp invalid
 bgp router-id invalid
 neighbor invalid
 network invalid
ip route invalid
"#;

    let device = parse_ios_config(cfg, Some("malformed.cfg"));
    assert!(!device.unparsed_lines.is_empty());
}

#[test]
fn test_non_contiguous_wildcard_matching() {
    let cfg = r#"
hostname R4
!
interface GigabitEthernet0/0
 ip address 10.1.3.1 255.255.255.0
!
router ospf 1
 network 10.1.1.0 0.0.2.255 area 0
!
"#;

    let device = parse_ios_config(cfg, None);
    // 10.1.3.1 with wildcard 0.0.2.255: bit 1 in 3rd octet is ignored (wildcard bit set), so 10.1.3.1 matches 10.1.1.0
    assert!(device.interfaces[0].ospf.is_some());
    assert_eq!(
        device.interfaces[0].ospf.as_ref().unwrap().area,
        OspfAreaId::Number(0)
    );
}

#[test]
fn test_ir_json_serialization() {
    let cfg = r#"
hostname R-JSON
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
!
router ospf 1
 router-id 1.1.1.1
 network 10.0.0.0 0.0.0.255 area 0
!
ip route 192.168.0.0 255.255.0.0 10.0.0.2
"#;

    let device = parse_ios_config(cfg, Some("test.cfg"));
    let json_val = serde_json::to_value(&device).unwrap();

    assert_eq!(json_val["hostname"], "R-JSON");
    assert_eq!(json_val["interfaces"][0]["name"], "GigabitEthernet0/0");
    assert_eq!(json_val["interfaces"][0]["ipv4"]["address"], "10.0.0.1");
    assert_eq!(json_val["interfaces"][0]["ipv4"]["prefix-length"], 24);
    assert_eq!(json_val["interfaces"][0]["ospf"]["area"], 0);
    assert_eq!(json_val["ospf"]["process-id"], 1);
    assert_eq!(json_val["ospf"]["router-id"], "1.1.1.1");
    assert_eq!(
        json_val["static-routes"][0]["destination"]["address"],
        "192.168.0.0"
    );
    assert_eq!(
        json_val["static-routes"][0]["destination"]["prefix-length"],
        16
    );
    assert_eq!(json_val["static-routes"][0]["next-hop"], "10.0.0.2");
}

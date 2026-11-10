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

#[test]
fn test_prefix_list_explicit_and_implicit_sequences() {
    let cfg = r#"
hostname R_PL
ip prefix-list FILTER-BGP description Filter incoming BGP routes
ip prefix-list FILTER-BGP seq 10 permit 10.0.0.0/8 ge 16 le 24
ip prefix-list FILTER-BGP seq 20 deny 0.0.0.0/0 le 32
ip prefix-list AUTO-SEQ permit 172.16.0.0/12
ip prefix-list AUTO-SEQ deny 192.168.0.0/16 ge 24
"#;

    let device = parse_ios_config(cfg, Some("pl.cfg"));
    assert_eq!(device.prefix_lists.len(), 2);

    let pl_auto = device
        .prefix_lists
        .iter()
        .find(|pl| pl.name == "AUTO-SEQ")
        .unwrap();
    assert_eq!(pl_auto.entries.len(), 2);
    assert_eq!(pl_auto.entries[0].sequence, 5);
    assert_eq!(pl_auto.entries[0].action, ir::PolicyAction::Permit);
    assert_eq!(pl_auto.entries[0].prefix.to_string(), "172.16.0.0/12");
    assert!(pl_auto.entries[0].range.is_none());

    assert_eq!(pl_auto.entries[1].sequence, 10);
    assert_eq!(pl_auto.entries[1].action, ir::PolicyAction::Deny);
    assert_eq!(pl_auto.entries[1].prefix.to_string(), "192.168.0.0/16");
    assert_eq!(
        pl_auto.entries[1].range,
        Some(ir::PrefixRange {
            ge: Some(ir::PrefixLength::new(24).unwrap()),
            le: None,
        })
    );

    let pl_filter = device
        .prefix_lists
        .iter()
        .find(|pl| pl.name == "FILTER-BGP")
        .unwrap();
    assert_eq!(pl_filter.entries.len(), 2);
    assert_eq!(pl_filter.entries[0].sequence, 10);
    assert_eq!(pl_filter.entries[0].action, ir::PolicyAction::Permit);
    assert_eq!(pl_filter.entries[0].prefix.to_string(), "10.0.0.0/8");
    assert_eq!(
        pl_filter.entries[0].range,
        Some(ir::PrefixRange {
            ge: Some(ir::PrefixLength::new(16).unwrap()),
            le: Some(ir::PrefixLength::new(24).unwrap()),
        })
    );

    assert_eq!(pl_filter.entries[1].sequence, 20);
    assert_eq!(pl_filter.entries[1].action, ir::PolicyAction::Deny);
    assert_eq!(pl_filter.entries[1].prefix.to_string(), "0.0.0.0/0");
    assert_eq!(
        pl_filter.entries[1].range,
        Some(ir::PrefixRange {
            ge: None,
            le: Some(ir::PrefixLength::new(32).unwrap()),
        })
    );

    assert!(device.unparsed_lines.is_empty());
}

#[test]
fn test_prefix_list_diagnostics_and_errors() {
    let cfg = r#"
hostname R_ERR
ip prefix-list DUP seq 10 permit 10.0.0.0/8
ip prefix-list DUP seq 10 deny 10.1.0.0/16
ip prefix-list BAD-RANGE permit 10.0.0.0/16 ge 12
ip prefix-list BAD-RANGE-2 permit 10.0.0.0/16 ge 28 le 24
ipv6 prefix-list IPV6-BAD permit 2001:db8::/48 ge 32
"#;

    let device = parse_ios_config(cfg, Some("errors.cfg"));
    assert_eq!(device.unparsed_lines.len(), 4);

    // Duplicate sequence
    assert_eq!(device.unparsed_lines[0].line_number, 4);
    assert_eq!(
        device.unparsed_lines[0].content,
        "ip prefix-list DUP seq 10 deny 10.1.0.0/16"
    );
    assert_eq!(
        device.unparsed_lines[0].reason,
        "duplicate sequence number 10 in prefix-list DUP"
    );

    // Malformed range: ge < prefix_len
    assert_eq!(device.unparsed_lines[1].line_number, 5);
    assert_eq!(
        device.unparsed_lines[1].content,
        "ip prefix-list BAD-RANGE permit 10.0.0.0/16 ge 12"
    );
    assert_eq!(
        device.unparsed_lines[1].reason,
        "invalid prefix-list length range: len <= ge <= le"
    );

    // Malformed range: ge > le
    assert_eq!(device.unparsed_lines[2].line_number, 6);
    assert_eq!(
        device.unparsed_lines[2].content,
        "ip prefix-list BAD-RANGE-2 permit 10.0.0.0/16 ge 28 le 24"
    );
    assert_eq!(
        device.unparsed_lines[2].reason,
        "invalid prefix-list length range: len <= ge <= le"
    );

    // IPv6 prefix list bad range
    assert_eq!(device.unparsed_lines[3].line_number, 7);
    assert_eq!(
        device.unparsed_lines[3].content,
        "ipv6 prefix-list IPV6-BAD permit 2001:db8::/48 ge 32"
    );
    assert_eq!(
        device.unparsed_lines[3].reason,
        "invalid prefix-list length range: len <= ge <= le"
    );
}

#[test]
fn test_cisco_ios_ipv6_prefix_list_and_route_map() {
    let cfg = r#"
hostname R1
ipv6 prefix-list V6-PL seq 10 permit 2001:db8::/32 ge 48 le 64
ipv6 prefix-list V6-PL seq 20 deny ::/0 le 128
!
route-map POLICY-1 permit 10
 match ip address prefix-list PL-4
 set metric 100
 set local-preference 200
!
route-map POLICY-1 deny 20
 match ipv6 address prefix-list V6-PL
!
"#;
    let dev = parse_ios_config(cfg, Some("test.cfg"));
    assert_eq!(dev.prefix_lists_v6.len(), 1);
    assert_eq!(dev.prefix_lists_v6[0].name, "V6-PL");
    assert_eq!(dev.prefix_lists_v6[0].entries.len(), 2);
    assert_eq!(dev.prefix_lists_v6[0].entries[0].sequence, 10);
    assert_eq!(
        dev.prefix_lists_v6[0].entries[0].prefix.to_string(),
        "2001:db8::/32"
    );
    assert!(dev.prefix_lists_v6[0].entries[0].range.is_some());
    assert_eq!(dev.policies.len(), 1);
    let pol = &dev.policies[0];
    assert_eq!(pol.name, "POLICY-1");
    assert_eq!(pol.statements.len(), 2);
    assert_eq!(pol.statements[0].name, "10");
    assert_eq!(
        pol.statements[0].conditions.match_prefix_set.as_deref(),
        Some("PL-4")
    );
    assert_eq!(pol.statements[0].actions.set_metric, Some(100));
    assert_eq!(pol.statements[0].actions.set_local_preference, Some(200));
    assert_eq!(
        pol.statements[0].actions.policy_result,
        ir::PolicyAction::Permit
    );
    assert_eq!(pol.statements[1].name, "20");
    assert_eq!(
        pol.statements[1].conditions.match_prefix_set_v6.as_deref(),
        Some("V6-PL")
    );
    assert_eq!(
        pol.statements[1].actions.policy_result,
        ir::PolicyAction::Deny
    );
}

#[test]
fn test_prefix_list_rfc7951_json_serialization() {
    let cfg = r#"
hostname R1
ip prefix-list FILTER-BGP seq 10 permit 10.0.0.0/8 ge 16 le 24
ip prefix-list FILTER-BGP seq 20 deny 0.0.0.0/0 le 32
"#;

    let device = parse_ios_config(cfg, None);
    let json_val = serde_json::to_value(&device).unwrap();

    let expected = serde_json::json!({
        "hostname": "R1",
        "prefix-lists": [
            {
                "name": "FILTER-BGP",
                "entries": [
                    {
                        "sequence": 10,
                        "action": "permit",
                        "prefix": "10.0.0.0/8",
                        "ge": 16,
                        "le": 24
                    },
                    {
                        "sequence": 20,
                        "action": "deny",
                        "prefix": "0.0.0.0/0",
                        "le": 32
                    }
                ]
            }
        ]
    });

    assert_eq!(json_val["hostname"], expected["hostname"]);
    assert_eq!(json_val["prefix-lists"], expected["prefix-lists"]);
}

#[test]
fn test_cisco_ios_ipv6_parsing() {
    let input = "hostname r1-v6\n!\ninterface GigabitEthernet0/0\n description IPv6 uplink\n ipv6 address 2001:db8:1::1/64\n ipv6 ospf 1 area 0\n no shutdown\n!\ninterface GigabitEthernet0/1\n ipv6 address 2001:db8:2::1/64\n ipv6 ospf 1 area 0.0.0.1\n shutdown\n!\nipv6 route 2001:db8:100::/48 2001:db8:1::2\n!\nipv6 router ospf 1\n router-id 1.1.1.1\n!\nrouter bgp 65001\n bgp router-id 1.1.1.1\n neighbor 2001:db8:1::2 remote-as 65002\n address-family ipv6 unicast\n  network 2001:db8:2::/64\n exit-address-family\n!\n";
    let dev = parse_ios_config(input, Some("r1-v6.cfg"));
    assert_eq!(dev.hostname, "r1-v6");
    assert_eq!(dev.interfaces.len(), 2);

    let if0 = &dev.interfaces[0];
    assert_eq!(if0.name, "GigabitEthernet0/0");
    assert_eq!(if0.ipv6.as_ref().unwrap().to_string(), "2001:db8:1::1/64");
    assert!(if0.enabled);
    assert!(if0.ospfv3.is_some());
    assert_eq!(if0.ospfv3.as_ref().unwrap().area, ir::OspfAreaId::Number(0));

    let if1 = &dev.interfaces[1];
    assert_eq!(if1.name, "GigabitEthernet0/1");
    assert_eq!(if1.ipv6.as_ref().unwrap().to_string(), "2001:db8:2::1/64");
    assert!(!if1.enabled);
    assert_eq!(
        if1.ospfv3.as_ref().unwrap().area,
        ir::OspfAreaId::Ipv4(Ipv4Addr::new(0, 0, 0, 1))
    );

    assert_eq!(dev.static_routes_v6.len(), 1);
    assert_eq!(
        dev.static_routes_v6[0].destination.to_string(),
        "2001:db8:100::/48"
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
fn test_parse_ios_isis_and_advanced_bgp() {
    let input = r#"
hostname R-ADV
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 ip router isis
 isis metric 20
 isis bfd
!
interface Loopback0
 ip address 1.1.1.1 255.255.255.255
 ip router isis
 isis passive
!
router isis
 net 49.0001.0001.0001.0001.00
 is-type level-2-only
!
ip community-list standard COMM_TEST permit 65000:100
!
route-map RM_TEST permit 10
 match community COMM_TEST
 set community 65000:200
!
router bgp 65000
 bgp router-id 1.1.1.1
 bgp cluster-id 1.1.1.1
 neighbor IBGP_PEERS peer-group
 neighbor IBGP_PEERS remote-as 65000
 neighbor IBGP_PEERS route-reflector-client
 neighbor IBGP_PEERS send-community
 neighbor IBGP_PEERS fall-over bfd
 neighbor 10.0.0.2 peer-group IBGP_PEERS
!
"#;
    let config = parse_ios_config(input, None);
    assert_eq!(config.hostname, "R-ADV");

    // Check IS-IS
    let isis = config.isis.expect("IS-IS process should be parsed");
    assert_eq!(isis.net.as_deref(), Some("49.0001.0001.0001.0001.00"));
    assert_eq!(isis.level, Some(ir::IsisLevel::Level2));

    let eth = config
        .interfaces
        .iter()
        .find(|i| i.name == "GigabitEthernet0/0")
        .unwrap();
    let isis_if = eth.isis.as_ref().unwrap();
    assert_eq!(isis_if.metric, Some(20));
    assert_eq!(isis_if.bfd, Some(true));

    let lo = config
        .interfaces
        .iter()
        .find(|i| i.name == "Loopback0")
        .unwrap();
    assert_eq!(lo.isis.as_ref().unwrap().passive, Some(true));

    // Check BGP
    let bgp = config.bgp.expect("BGP should be parsed");
    assert_eq!(bgp.cluster_id, Some("1.1.1.1".parse().unwrap()));
    assert_eq!(bgp.peer_groups.len(), 1);
    let pg = &bgp.peer_groups[0];
    assert_eq!(pg.name, "IBGP_PEERS");
    assert_eq!(pg.remote_as, Some(65000));
    assert_eq!(pg.route_reflector_client, Some(true));
    assert_eq!(pg.send_community.as_deref(), Some("standard"));
    assert_eq!(pg.bfd, Some(true));

    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].peer_group.as_deref(), Some("IBGP_PEERS"));

    // Check Community List
    assert_eq!(config.community_lists.len(), 1);
    assert_eq!(config.community_lists[0].name, "COMM_TEST");
    assert_eq!(
        config.community_lists[0].entries[0].communities,
        vec!["65000:100"]
    );

    // Check Policy match and set community
    assert_eq!(config.policies.len(), 1);
    let stmt = &config.policies[0].statements[0];
    assert_eq!(
        stmt.conditions.match_community.as_deref(),
        Some("COMM_TEST")
    );
    assert_eq!(stmt.actions.set_community.as_deref(), Some("65000:200"));
}

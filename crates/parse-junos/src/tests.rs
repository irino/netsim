use super::*;
use ir::OspfAreaId;
use std::net::Ipv4Addr;

#[test]
fn test_parse_empty_and_comments() {
    let cfg = "# Junos configuration\n/* block comment */\n\n";
    let dev = parse_junos_config(cfg, Some("empty.set"));
    assert_eq!(dev.hostname, "router");
    assert!(dev.interfaces.is_empty());
    assert!(dev.ospf.is_none());
    assert!(dev.bgp.is_none());
    assert!(dev.static_routes.is_empty());
    assert!(dev.unparsed_lines.is_empty());
}

#[test]
fn test_parse_hostname_and_interfaces() {
    let cfg = r#"
set system host-name "junos-r1"
set interfaces ge-0/0/0 description "Uplink interface"
set interfaces ge-0/0/0 unit 0 family inet address 10.0.0.1/24
set interfaces ge-0/0/1 unit 0 description "Disabled interface"
set interfaces ge-0/0/1 unit 0 family inet address 192.168.1.1/24
set interfaces ge-0/0/1 unit 0 disable
"#;

    let dev = parse_junos_config(cfg, Some("r1.set"));
    assert_eq!(dev.hostname, "junos-r1");
    assert_eq!(dev.interfaces.len(), 2);

    let if0 = &dev.interfaces[0];
    assert_eq!(if0.name, "ge-0/0/0.0");
    assert_eq!(if0.description.as_deref(), Some("Uplink interface"));
    assert!(if0.enabled);
    assert_eq!(if0.ipv4.as_ref().unwrap().to_string(), "10.0.0.1/24");

    let if1 = &dev.interfaces[1];
    assert_eq!(if1.name, "ge-0/0/1.0");
    assert_eq!(if1.description.as_deref(), Some("Disabled interface"));
    assert!(!if1.enabled);
    assert_eq!(if1.ipv4.as_ref().unwrap().to_string(), "192.168.1.1/24");

    assert!(dev.unparsed_lines.is_empty());
}

#[test]
fn test_parse_ospf() {
    let cfg = r#"
set system host-name junos-ospf
set routing-options router-id 1.1.1.1
set interfaces ge-0/0/0 unit 0 family inet address 10.1.0.1/24
set interfaces ge-0/0/1 unit 0 family inet address 10.2.0.1/24
set protocols ospf area 0.0.0.0 interface ge-0/0/0.0 metric 10
set protocols ospf area 0.0.0.0 interface ge-0/0/1.0 passive
"#;

    let dev = parse_junos_config(cfg, Some("ospf.set"));
    assert_eq!(dev.hostname, "junos-ospf");

    let ospf = dev.ospf.as_ref().unwrap();
    assert_eq!(ospf.process_id, 1);
    assert_eq!(ospf.router_id, Some(Ipv4Addr::new(1, 1, 1, 1)));

    let if0 = &dev.interfaces[0];
    assert_eq!(if0.name, "ge-0/0/0.0");
    let if0_ospf = if0.ospf.as_ref().unwrap();
    assert_eq!(if0_ospf.area, OspfAreaId::Ipv4(Ipv4Addr::new(0, 0, 0, 0)));
    assert_eq!(if0_ospf.cost, Some(10));
    assert_eq!(if0_ospf.passive, None);

    let if1 = &dev.interfaces[1];
    assert_eq!(if1.name, "ge-0/0/1.0");
    let if1_ospf = if1.ospf.as_ref().unwrap();
    assert_eq!(if1_ospf.area, OspfAreaId::Ipv4(Ipv4Addr::new(0, 0, 0, 0)));
    assert_eq!(if1_ospf.passive, Some(true));
}

#[test]
fn test_parse_bgp_and_static_routes() {
    let cfg = r#"
set system host-name junos-bgp
set routing-options router-id 2.2.2.2
set routing-options autonomous-system 65001
set routing-options static route 0.0.0.0/0 next-hop 192.168.1.254
set routing-options static route 172.16.0.0/16 next-hop 10.0.0.2
set protocols bgp group ebgp-peers neighbor 10.0.0.2 peer-as 65002
set policy-options policy-statement adv-routes term 1 from route-filter 192.168.10.0/24 exact
"#;

    let dev = parse_junos_config(cfg, Some("bgp.set"));
    assert_eq!(dev.hostname, "junos-bgp");

    let bgp = dev.bgp.as_ref().unwrap();
    assert_eq!(bgp.as_number, 65001);
    assert_eq!(bgp.router_id, Some(Ipv4Addr::new(2, 2, 2, 2)));
    assert_eq!(bgp.neighbors.len(), 1);
    assert_eq!(bgp.neighbors[0].remote_address, Ipv4Addr::new(10, 0, 0, 2));
    assert_eq!(bgp.neighbors[0].remote_as, 65002);
    assert_eq!(bgp.networks.len(), 1);
    assert_eq!(bgp.networks[0].to_string(), "192.168.10.0/24");

    assert_eq!(dev.static_routes.len(), 2);
    assert_eq!(dev.static_routes[0].destination.to_string(), "0.0.0.0/0");
    assert_eq!(
        dev.static_routes[0].next_hop,
        Ipv4Addr::new(192, 168, 1, 254)
    );
    assert_eq!(
        dev.static_routes[1].destination.to_string(),
        "172.16.0.0/16"
    );
    assert_eq!(dev.static_routes[1].next_hop, Ipv4Addr::new(10, 0, 0, 2));

    assert!(dev.unparsed_lines.is_empty());
}

#[test]
fn test_unparsed_lines_and_malformed() {
    let cfg = r#"
set system host-name junos-err
set unknown-hierarchy command value
invalid line not starting with set
set routing-options router-id invalid
set routing-options static route invalid
"#;

    let dev = parse_junos_config(cfg, Some("err.set"));
    assert_eq!(dev.hostname, "junos-err");
    assert_eq!(dev.unparsed_lines.len(), 4);

    assert_eq!(dev.unparsed_lines[0].line_number, 3);
    assert!(dev.unparsed_lines[0]
        .reason
        .contains("unsupported or unrecognized"));

    assert_eq!(dev.unparsed_lines[1].line_number, 4);
    assert!(dev.unparsed_lines[1]
        .reason
        .contains("line does not start with 'set'"));

    assert_eq!(dev.unparsed_lines[2].line_number, 5);
    assert!(dev.unparsed_lines[2].reason.contains("malformed router-id"));

    assert_eq!(dev.unparsed_lines[3].line_number, 6);
    assert!(dev.unparsed_lines[3]
        .reason
        .contains("malformed static route"));
}

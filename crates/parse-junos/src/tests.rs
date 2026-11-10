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

#[test]
fn test_parse_junos_prefix_lists() {
    let cfg = r#"
set system host-name R_JUNOS_PL
set policy-options prefix-list PL-INTERNAL 10.0.0.0/8
set policy-options prefix-list PL-INTERNAL 172.16.0.0/12
set policy-options prefix-list PL-DEFAULT 0.0.0.0/0
"#;

    let dev = parse_junos_config(cfg, Some("pl.set"));
    assert_eq!(dev.hostname, "R_JUNOS_PL");
    assert_eq!(dev.prefix_lists.len(), 2);

    let pl_default = dev
        .prefix_lists
        .iter()
        .find(|p| p.name == "PL-DEFAULT")
        .unwrap();
    assert_eq!(pl_default.entries.len(), 1);
    assert_eq!(pl_default.entries[0].sequence, 5);
    assert_eq!(pl_default.entries[0].prefix.to_string(), "0.0.0.0/0");
    assert_eq!(pl_default.entries[0].action, ir::PolicyAction::Permit);

    let pl_internal = dev
        .prefix_lists
        .iter()
        .find(|p| p.name == "PL-INTERNAL")
        .unwrap();
    assert_eq!(pl_internal.entries.len(), 2);
    assert_eq!(pl_internal.entries[0].sequence, 5);
    assert_eq!(pl_internal.entries[0].prefix.to_string(), "10.0.0.0/8");
    assert_eq!(pl_internal.entries[1].sequence, 10);
    assert_eq!(pl_internal.entries[1].prefix.to_string(), "172.16.0.0/12");

    assert!(dev.unparsed_lines.is_empty());
}

#[test]
fn test_parse_junos_ipv6() {
    let input = r#"
set system host-name "junos-v6"
set interfaces ge-0/0/0 unit 0 description "IPv6 interface"
set interfaces ge-0/0/0 unit 0 family inet6 address 2001:db8:1::1/64
set routing-options rib inet6.0 static route 2001:db8:2::/64 next-hop 2001:db8:1::2
set protocols ospf3 area 0.0.0.0 interface ge-0/0/0.0 metric 15
set protocols ospf3 area 0.0.0.0 interface ge-0/0/0.0 passive
set protocols bgp group ebgp-v6 neighbor 2001:db8:1::2 peer-as 65002
set protocols bgp local-as 65001
"#;
    let dev = parse_junos_config(input, Some("junos-v6.set"));
    assert_eq!(dev.hostname, "junos-v6");
    assert_eq!(dev.interfaces.len(), 1);

    let iface = &dev.interfaces[0];
    assert_eq!(iface.name, "ge-0/0/0.0");
    assert_eq!(iface.ipv6.as_ref().unwrap().to_string(), "2001:db8:1::1/64");
    assert!(iface.ospfv3.is_some());
    let ospf3 = iface.ospfv3.as_ref().unwrap();
    assert_eq!(ospf3.cost, Some(15));
    assert_eq!(ospf3.passive, Some(true));

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
    assert!(dev.bgp.is_some());
    let bgp = dev.bgp.as_ref().unwrap();
    assert_eq!(bgp.as_number, 65001);
    assert_eq!(bgp.neighbors_v6.len(), 1);
    assert_eq!(
        bgp.neighbors_v6[0].remote_address.to_string(),
        "2001:db8:1::2"
    );
    assert_eq!(bgp.neighbors_v6[0].remote_as, 65002);
}

#[test]
fn test_parse_junos_ipv6_prefix_list_and_policy_statement() {
    let cfg = r#"
set system host-name junos-r1
set policy-options prefix-list PL-V6 2001:db8:1::/48
set policy-options prefix-list PL-V6 2001:db8:2::/48
set policy-options policy-statement PS-BGP term 1 from prefix-list PL-V6
set policy-options policy-statement PS-BGP term 1 then metric 100
set policy-options policy-statement PS-BGP term 1 then local-preference 200
set policy-options policy-statement PS-BGP term 1 then accept
set policy-options policy-statement PS-BGP term 2 then reject
"#;
    let dev = parse_junos_config(cfg, Some("junos.set"));
    assert_eq!(dev.prefix_lists_v6.len(), 1);
    assert_eq!(dev.prefix_lists_v6[0].name, "PL-V6");
    assert_eq!(dev.prefix_lists_v6[0].entries.len(), 2);
    assert_eq!(dev.policies.len(), 1);
    let pol = &dev.policies[0];
    assert_eq!(pol.name, "PS-BGP");
    assert_eq!(pol.statements.len(), 2);
    assert_eq!(pol.statements[0].name, "1");
    assert_eq!(
        pol.statements[0].conditions.match_prefix_set.as_deref(),
        Some("PL-V6")
    );
    assert_eq!(pol.statements[0].actions.set_metric, Some(100));
    assert_eq!(pol.statements[0].actions.set_local_preference, Some(200));
    assert_eq!(
        pol.statements[0].actions.policy_result,
        ir::PolicyAction::Permit
    );
    assert_eq!(pol.statements[1].name, "2");
    assert_eq!(
        pol.statements[1].actions.policy_result,
        ir::PolicyAction::Deny
    );
}

#[test]
fn test_parse_junos_isis_and_advanced_bgp() {
    let input = r#"
set system host-name junos-test
set interfaces ge-0/0/0 unit 0 family inet address 10.0.0.1/24
set interfaces ge-0/0/0 unit 0 family iso address "49.0001.0001.0001.0001.00"
set protocols isis interface ge-0/0/0.0 level 2 metric 20
set protocols isis interface ge-0/0/0.0 bfd-liveness-detection
set protocols isis interface lo0.0 passive
set protocols isis level 1 disable
set policy-options community COMM_TEST members "65000:100"
set policy-options policy-statement PS_TEST term t1 from community COMM_TEST
set policy-options policy-statement PS_TEST term t1 then community set "65000:200"
set policy-options policy-statement PS_TEST term t1 then accept
set protocols bgp local-as 65000
set protocols bgp group RR_CLIENTS type internal
set protocols bgp group RR_CLIENTS cluster 1.1.1.1
set protocols bgp group RR_CLIENTS peer-as 65000
set protocols bgp group RR_CLIENTS bfd-liveness-detection
set protocols bgp group RR_CLIENTS neighbor 10.0.0.2
"#;
    let config = parse_junos_config(input, None);
    assert_eq!(config.hostname, "junos-test");

    // IS-IS
    let isis = config.isis.expect("IS-IS process should exist");
    assert_eq!(isis.net.as_deref(), Some("49.0001.0001.0001.0001.00"));
    assert_eq!(isis.level, Some(ir::IsisLevel::Level2));

    let ge = config
        .interfaces
        .iter()
        .find(|i| i.name == "ge-0/0/0.0")
        .unwrap();
    let isis_if = ge.isis.as_ref().unwrap();
    assert_eq!(isis_if.metric, Some(20));
    assert_eq!(isis_if.level, Some(ir::IsisLevel::Level2));
    assert_eq!(isis_if.bfd, Some(true));

    let lo = config
        .interfaces
        .iter()
        .find(|i| i.name == "lo0.0")
        .unwrap();
    assert_eq!(lo.isis.as_ref().unwrap().passive, Some(true));

    // BGP
    let bgp = config.bgp.expect("BGP process should exist");
    assert_eq!(bgp.cluster_id, Some("1.1.1.1".parse().unwrap()));
    assert_eq!(bgp.peer_groups.len(), 1);
    let pg = &bgp.peer_groups[0];
    assert_eq!(pg.name, "RR_CLIENTS");
    assert_eq!(pg.remote_as, Some(65000));
    assert_eq!(pg.route_reflector_client, Some(true));
    assert_eq!(pg.bfd, Some(true));

    assert_eq!(bgp.neighbors.len(), 1);
    let nbr = &bgp.neighbors[0];
    assert_eq!(nbr.peer_group.as_deref(), Some("RR_CLIENTS"));
    assert_eq!(nbr.route_reflector_client, Some(true));
    assert_eq!(nbr.cluster_id, Some("1.1.1.1".parse().unwrap()));
    assert_eq!(nbr.bfd, Some(true));

    // Community list & Policy statement
    assert_eq!(config.community_lists.len(), 1);
    assert_eq!(config.community_lists[0].name, "COMM_TEST");
    assert_eq!(
        config.community_lists[0].entries[0].communities,
        vec!["65000:100"]
    );

    assert_eq!(config.policies.len(), 1);
    let stmt = &config.policies[0].statements[0];
    assert_eq!(
        stmt.conditions.match_community.as_deref(),
        Some("COMM_TEST")
    );
    assert_eq!(stmt.actions.set_community.as_deref(), Some("65000:200"));
}

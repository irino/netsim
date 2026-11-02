use super::*;
use ir::{DeviceConfig, Interface, Ipv4Prefix};

fn make_device(hostname: &str, interfaces: Vec<Interface>) -> DeviceConfig {
    DeviceConfig {
        hostname: hostname.to_string(),
        interfaces,
        ospf: None,
        bgp: None,
        static_routes: Vec::new(),
        prefix_lists: Vec::new(),
        unparsed_lines: Vec::new(),
    }
}

fn make_iface(name: &str, ip: Option<&str>, enabled: bool) -> Interface {
    Interface {
        name: name.to_string(),
        description: None,
        enabled,
        ipv4: ip.map(|s| s.parse::<Ipv4Prefix>().unwrap()),
        ospf: None,
    }
}

#[test]
fn test_empty_and_single_device_input() {
    let topo_empty = infer_topology(&[]);
    assert!(topo_empty.links.is_empty());
    assert!(topo_empty.warnings.is_empty());

    let dev1 = make_device("R1", vec![make_iface("eth0", Some("10.0.0.1/24"), true)]);
    let topo_single = infer_topology(&[dev1]);
    assert!(topo_single.links.is_empty());
    assert!(topo_single.warnings.is_empty());
}

#[test]
fn test_p2p_link_inference() {
    let dev1 = make_device("R1", vec![make_iface("eth0", Some("10.0.0.1/24"), true)]);
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.0.2/24"), true)]);

    let topo = infer_topology(&[dev1, dev2]);
    assert_eq!(topo.links.len(), 1);
    assert!(topo.warnings.is_empty());

    let link = &topo.links[0];
    assert_eq!(link.subnet.to_string(), "10.0.0.0/24");
    assert_eq!(link.endpoint_a.hostname, "R1");
    assert_eq!(link.endpoint_a.interface_name, "eth0");
    assert_eq!(link.endpoint_b.hostname, "R2");
    assert_eq!(link.endpoint_b.interface_name, "eth0");
}

#[test]
fn test_different_subnets_no_link() {
    let dev1 = make_device("R1", vec![make_iface("eth0", Some("10.0.0.1/24"), true)]);
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.1.1/24"), true)]);

    let topo = infer_topology(&[dev1, dev2]);
    assert!(topo.links.is_empty());
    assert!(topo.warnings.is_empty());
}

#[test]
fn test_different_prefix_lengths_subnet_calculation() {
    let dev1 = make_device("R1", vec![make_iface("eth0", Some("10.0.0.1/24"), true)]);
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.0.2/25"), true)]);

    let topo = infer_topology(&[dev1, dev2]);
    // Since prefix lengths differ (/24 vs /25), network prefixes 10.0.0.0/24 and 10.0.0.0/25 are distinct
    assert!(topo.links.is_empty());
}

#[test]
fn test_shutdown_and_no_ip_excluded() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.0.0.1/24"), false), // shutdown
            make_iface("eth1", None, true),                 // no IP
        ],
    );
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.0.2/24"), true)]);

    let topo = infer_topology(&[dev1, dev2]);
    assert!(topo.links.is_empty());
}

#[test]
fn test_loopback_excluded() {
    let dev1 = make_device(
        "R1",
        vec![make_iface("Loopback0", Some("10.0.0.1/24"), true)],
    );
    let dev2 = make_device("R2", vec![make_iface("lo0", Some("10.0.0.2/24"), true)]);

    let topo = infer_topology(&[dev1, dev2]);
    assert!(topo.links.is_empty());
}

#[test]
fn test_three_routers_on_one_shared_subnet() {
    let dev1 = make_device("R1", vec![make_iface("eth0", Some("10.0.0.1/24"), true)]);
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.0.2/24"), true)]);
    let dev3 = make_device("R3", vec![make_iface("eth0", Some("10.0.0.3/24"), true)]);

    let topo = infer_topology(&[dev1, dev2, dev3]);
    assert!(topo.links.is_empty());
    assert_eq!(topo.lan_segments.len(), 1);
    assert!(topo.warnings.is_empty());

    let lan = &topo.lan_segments[0];
    assert_eq!(lan.subnet.to_string(), "10.0.0.0/24");
    assert_eq!(lan.endpoints.len(), 3);
    assert_eq!(lan.endpoints[0].hostname, "R1");
    assert_eq!(lan.endpoints[0].interface_name, "eth0");
    assert_eq!(lan.endpoints[1].hostname, "R2");
    assert_eq!(lan.endpoints[1].interface_name, "eth0");
    assert_eq!(lan.endpoints[2].hostname, "R3");
    assert_eq!(lan.endpoints[2].interface_name, "eth0");
}

#[test]
fn test_four_or_more_routers_on_one_shared_subnet() {
    let dev1 = make_device("R1", vec![make_iface("eth0", Some("192.168.1.1/24"), true)]);
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("192.168.1.2/24"), true)]);
    let dev3 = make_device("R3", vec![make_iface("eth0", Some("192.168.1.3/24"), true)]);
    let dev4 = make_device("R4", vec![make_iface("eth0", Some("192.168.1.4/24"), true)]);
    let dev5 = make_device("R5", vec![make_iface("eth0", Some("192.168.1.5/24"), true)]);

    let topo = infer_topology(&[dev1, dev2, dev3, dev4, dev5]);
    assert!(topo.links.is_empty());
    assert_eq!(topo.lan_segments.len(), 1);
    assert!(topo.warnings.is_empty());

    let lan = &topo.lan_segments[0];
    assert_eq!(lan.subnet.to_string(), "192.168.1.0/24");
    assert_eq!(lan.endpoints.len(), 5);
}

#[test]
fn test_multiple_independent_multi_access_lans() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.1.0.1/24"), true),
            make_iface("eth1", Some("10.2.0.1/24"), true),
        ],
    );
    let dev2 = make_device(
        "R2",
        vec![
            make_iface("eth0", Some("10.1.0.2/24"), true),
            make_iface("eth1", Some("10.2.0.2/24"), true),
        ],
    );
    let dev3 = make_device(
        "R3",
        vec![
            make_iface("eth0", Some("10.1.0.3/24"), true),
            make_iface("eth1", Some("10.2.0.3/24"), true),
        ],
    );

    let topo = infer_topology(&[dev3, dev1, dev2]);
    assert!(topo.links.is_empty());
    assert_eq!(topo.lan_segments.len(), 2);
    assert!(topo.warnings.is_empty());

    assert_eq!(topo.lan_segments[0].subnet.to_string(), "10.1.0.0/24");
    assert_eq!(topo.lan_segments[0].endpoints.len(), 3);
    assert_eq!(topo.lan_segments[1].subnet.to_string(), "10.2.0.0/24");
    assert_eq!(topo.lan_segments[1].endpoints.len(), 3);
}

#[test]
fn test_router_in_both_p2p_and_multi_access_links() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.0.1.1/24"), true), // p2p with R2
            make_iface("eth1", Some("10.0.2.1/24"), true), // LAN with R2, R3
        ],
    );
    let dev2 = make_device(
        "R2",
        vec![
            make_iface("eth0", Some("10.0.1.2/24"), true),
            make_iface("eth1", Some("10.0.2.2/24"), true),
        ],
    );
    let dev3 = make_device("R3", vec![make_iface("eth0", Some("10.0.2.3/24"), true)]);

    let topo = infer_topology(&[dev1, dev2, dev3]);
    assert_eq!(topo.links.len(), 1);
    assert_eq!(topo.links[0].subnet.to_string(), "10.0.1.0/24");

    assert_eq!(topo.lan_segments.len(), 1);
    assert_eq!(topo.lan_segments[0].subnet.to_string(), "10.0.2.0/24");
    assert_eq!(topo.lan_segments[0].endpoints.len(), 3);
    assert!(topo.warnings.is_empty());
}

#[test]
fn test_multi_access_shutdown_loopback_and_no_ip_excluded() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.0.0.1/24"), true),
            make_iface("eth1", Some("10.0.0.10/24"), false), // shutdown
            make_iface("Loopback0", Some("10.0.0.20/24"), true), // loopback
            make_iface("eth2", None, true),                  // no IP
        ],
    );
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.0.2/24"), true)]);
    let dev3 = make_device("R3", vec![make_iface("eth0", Some("10.0.0.3/24"), true)]);

    let topo = infer_topology(&[dev1, dev2, dev3]);
    assert_eq!(topo.lan_segments.len(), 1);
    assert_eq!(topo.lan_segments[0].endpoints.len(), 3);
    assert!(topo.warnings.is_empty());
}

#[test]
fn test_multi_access_same_device_duplicate_warning() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.0.0.1/24"), true),
            make_iface("eth1", Some("10.0.0.2/24"), true),
        ],
    );
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.0.3/24"), true)]);
    let dev3 = make_device("R3", vec![make_iface("eth0", Some("10.0.0.4/24"), true)]);

    let topo = infer_topology(&[dev1, dev2, dev3]);
    assert!(topo.lan_segments.is_empty());
    assert_eq!(topo.warnings.len(), 1);
    assert!(topo.warnings[0]
        .message
        .contains("multiple interfaces on the same device 'R1'"));
}

#[test]
fn test_multiple_independent_links_and_determinism() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.0.1.1/24"), true),
            make_iface("eth1", Some("10.0.2.1/24"), true),
        ],
    );
    let dev2 = make_device("R2", vec![make_iface("eth0", Some("10.0.1.2/24"), true)]);
    let dev3 = make_device("R3", vec![make_iface("eth0", Some("10.0.2.2/24"), true)]);

    // Pass devices in different order
    let topo1 = infer_topology(&[dev1.clone(), dev2.clone(), dev3.clone()]);
    let topo2 = infer_topology(&[dev3, dev1, dev2]);

    assert_eq!(topo1.links.len(), 2);
    assert_eq!(topo1, topo2);

    assert_eq!(topo1.links[0].subnet.to_string(), "10.0.1.0/24");
    assert_eq!(topo1.links[1].subnet.to_string(), "10.0.2.0/24");
}

#[test]
fn test_duplicate_links_and_same_device_subnet() {
    let dev1 = make_device(
        "R1",
        vec![
            make_iface("eth0", Some("10.0.0.1/24"), true),
            make_iface("eth1", Some("10.0.0.2/24"), true),
        ],
    );

    let topo = infer_topology(&[dev1]);
    assert!(topo.links.is_empty());
    assert_eq!(topo.warnings.len(), 1);
    assert!(topo.warnings[0]
        .message
        .contains("multiple interfaces on the same device"));
}

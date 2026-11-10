use ir::DeviceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use topology::{IpSubnet, Topology};

/// Graph representation of the static network topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkGraph {
    pub nodes: BTreeMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub hostname: String,
    pub loopbacks_v4: Vec<Ipv4Addr>,
    pub loopbacks_v6: Vec<Ipv6Addr>,
    pub interface_ips_v4: Vec<Ipv4Addr>,
    pub interface_ips_v6: Vec<Ipv6Addr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphEdge {
    pub node_a: String,
    pub iface_a: String,
    pub node_b: String,
    pub iface_b: String,
    pub subnet: String,
    pub is_v6: bool,
    pub cost: u32,
}

impl NetworkGraph {
    /// Build network graph from parsed devices and inferred topology.
    pub fn build(devices: &[DeviceConfig], topology: &Topology) -> Self {
        let mut nodes = BTreeMap::new();

        for dev in devices {
            let mut loopbacks_v4 = Vec::new();
            let mut loopbacks_v6 = Vec::new();
            let mut interface_ips_v4 = Vec::new();
            let mut interface_ips_v6 = Vec::new();

            for iface in &dev.interfaces {
                if !iface.enabled {
                    continue;
                }
                if let Some(ref v4) = iface.ipv4 {
                    interface_ips_v4.push(v4.address);
                    if iface.is_loopback() {
                        loopbacks_v4.push(v4.address);
                    }
                }
                if let Some(ref v6) = iface.ipv6 {
                    interface_ips_v6.push(v6.address);
                    if iface.is_loopback() {
                        loopbacks_v6.push(v6.address);
                    }
                }
            }

            loopbacks_v4.sort();
            loopbacks_v6.sort();
            interface_ips_v4.sort();
            interface_ips_v6.sort();

            nodes.insert(
                dev.hostname.clone(),
                GraphNode {
                    hostname: dev.hostname.clone(),
                    loopbacks_v4,
                    loopbacks_v6,
                    interface_ips_v4,
                    interface_ips_v6,
                },
            );
        }

        let mut edges = Vec::new();

        // Point-to-point links
        for link in &topology.links {
            let is_v6 = matches!(link.subnet, IpSubnet::Ipv6(_));
            let cost = 10; // Default link metric

            edges.push(GraphEdge {
                node_a: link.endpoint_a.hostname.clone(),
                iface_a: link.endpoint_a.interface_name.clone(),
                node_b: link.endpoint_b.hostname.clone(),
                iface_b: link.endpoint_b.interface_name.clone(),
                subnet: link.subnet.to_string(),
                is_v6,
                cost,
            });
        }

        // Multi-access LAN segments: create clique of edges between all endpoints
        for lan in &topology.lan_segments {
            let is_v6 = matches!(lan.subnet, IpSubnet::Ipv6(_));
            let cost = 10;
            for i in 0..lan.endpoints.len() {
                for j in (i + 1)..lan.endpoints.len() {
                    let ep_a = &lan.endpoints[i];
                    let ep_b = &lan.endpoints[j];
                    edges.push(GraphEdge {
                        node_a: ep_a.hostname.clone(),
                        iface_a: ep_a.interface_name.clone(),
                        node_b: ep_b.hostname.clone(),
                        iface_b: ep_b.interface_name.clone(),
                        subnet: lan.subnet.to_string(),
                        is_v6,
                        cost,
                    });
                }
            }
        }

        Self { nodes, edges }
    }
}

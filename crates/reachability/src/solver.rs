use crate::graph::NetworkGraph;
use ir::DeviceConfig;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};
use std::net::IpAddr;
use thiserror::Error;
use topology::Topology;

#[derive(Debug, Error)]
pub enum ReachabilityError {
    #[error("Node '{0}' not found in topology graph")]
    NodeNotFound(String),
    #[error("Target IP/prefix '{0}' does not belong to any configured node")]
    TargetNotFound(String),
}

/// Reachability state between two endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReachabilityStatus {
    Reachable,
    Unreachable,
}

/// Result of evaluating reachability between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathResult {
    pub source: String,
    pub destination: String,
    pub status: ReachabilityStatus,
    pub cost: Option<u32>,
    pub hops: Vec<String>,
}

/// Pair state in the priority queue for Dijkstra's shortest path algorithm.
#[derive(Copy, Clone, Eq, PartialEq)]
struct State {
    cost: u32,
    node_idx: usize,
}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: lower cost has higher priority
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Static Reachability Solver analyzing mathematical path existence across nodes.
pub struct ReachabilitySolver<'a> {
    pub devices: &'a [DeviceConfig],
    pub topology: &'a Topology,
    pub graph: NetworkGraph,
}

impl<'a> ReachabilitySolver<'a> {
    pub fn new(devices: &'a [DeviceConfig], topology: &'a Topology) -> Self {
        let graph = NetworkGraph::build(devices, topology);
        Self {
            devices,
            topology,
            graph,
        }
    }

    /// Check basic link and subnet consistency across all devices.
    pub fn check_consistency(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // 1. Check for duplicate IP addresses across active interfaces
        let mut seen_ips = BTreeMap::new();
        for dev in self.devices {
            for iface in &dev.interfaces {
                if !iface.enabled {
                    continue;
                }
                if let Some(ref v4) = iface.ipv4 {
                    let ip = v4.address;
                    if let Some(other) =
                        seen_ips.insert(ip.to_string(), (dev.hostname.clone(), iface.name.clone()))
                    {
                        issues.push(format!(
                            "Duplicate IPv4 address {} configured on {}:{} and {}:{}",
                            ip, other.0, other.1, dev.hostname, iface.name
                        ));
                    }
                }
                if let Some(ref v6) = iface.ipv6 {
                    let ip = v6.address;
                    if let Some(other) =
                        seen_ips.insert(ip.to_string(), (dev.hostname.clone(), iface.name.clone()))
                    {
                        issues.push(format!(
                            "Duplicate IPv6 address {} configured on {}:{} and {}:{}",
                            ip, other.0, other.1, dev.hostname, iface.name
                        ));
                    }
                }
            }
        }

        // 2. Propagate topology warnings
        for w in &self.topology.warnings {
            issues.push(format!("Topology warning for {}: {}", w.subnet, w.message));
        }

        issues
    }

    /// Solve shortest path and reachability between source node and destination node using Dijkstra.
    pub fn find_path(
        &self,
        source: &str,
        destination: &str,
        ipv6: bool,
    ) -> Result<PathResult, ReachabilityError> {
        if !self.graph.nodes.contains_key(source) {
            return Err(ReachabilityError::NodeNotFound(source.to_string()));
        }
        if !self.graph.nodes.contains_key(destination) {
            return Err(ReachabilityError::NodeNotFound(destination.to_string()));
        }

        if source == destination {
            return Ok(PathResult {
                source: source.to_string(),
                destination: destination.to_string(),
                status: ReachabilityStatus::Reachable,
                cost: Some(0),
                hops: vec![source.to_string()],
            });
        }

        // Index nodes for graph algorithms
        let node_names: Vec<String> = self.graph.nodes.keys().cloned().collect();
        let node_indices: BTreeMap<String, usize> = node_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i))
            .collect();

        let src_idx = *node_indices.get(source).unwrap();
        let dst_idx = *node_indices.get(destination).unwrap();

        let n = node_names.len();
        let mut dist: Vec<u32> = vec![u32::MAX; n];
        let mut prev: Vec<Option<usize>> = vec![None; n];
        let mut heap = BinaryHeap::new();

        dist[src_idx] = 0;
        heap.push(State {
            cost: 0,
            node_idx: src_idx,
        });

        // Build adjacency list filtered by address family
        let mut adj: Vec<Vec<(usize, u32)>> = vec![Vec::new(); n];
        for edge in &self.graph.edges {
            if edge.is_v6 == ipv6 {
                if let (Some(&u), Some(&v)) = (
                    node_indices.get(&edge.node_a),
                    node_indices.get(&edge.node_b),
                ) {
                    adj[u].push((v, edge.cost));
                    adj[v].push((u, edge.cost));
                }
            }
        }

        while let Some(State { cost, node_idx }) = heap.pop() {
            if node_idx == dst_idx {
                break;
            }
            if cost > dist[node_idx] {
                continue;
            }

            for &(next_idx, edge_cost) in &adj[node_idx] {
                let next_cost = cost.saturating_add(edge_cost);
                if next_cost < dist[next_idx] {
                    dist[next_idx] = next_cost;
                    prev[next_idx] = Some(node_idx);
                    heap.push(State {
                        cost: next_cost,
                        node_idx: next_idx,
                    });
                }
            }
        }

        if dist[dst_idx] == u32::MAX {
            Ok(PathResult {
                source: source.to_string(),
                destination: destination.to_string(),
                status: ReachabilityStatus::Unreachable,
                cost: None,
                hops: Vec::new(),
            })
        } else {
            let mut hops = Vec::new();
            let mut curr = Some(dst_idx);
            while let Some(idx) = curr {
                hops.push(node_names[idx].clone());
                curr = prev[idx];
            }
            hops.reverse();

            Ok(PathResult {
                source: source.to_string(),
                destination: destination.to_string(),
                status: ReachabilityStatus::Reachable,
                cost: Some(dist[dst_idx]),
                hops,
            })
        }
    }

    /// Compute full all-pairs reachability matrix across all nodes.
    pub fn compute_reachability_matrix(&self, ipv6: bool) -> Vec<PathResult> {
        let mut matrix = Vec::new();
        let node_names: Vec<String> = self.graph.nodes.keys().cloned().collect();

        for src in &node_names {
            for dst in &node_names {
                if let Ok(res) = self.find_path(src, dst, ipv6) {
                    matrix.push(res);
                }
            }
        }

        matrix
    }

    /// Check if target IP address or loopback is reachable from source node.
    pub fn is_target_reachable(
        &self,
        from_node: &str,
        target: &str,
    ) -> Result<bool, ReachabilityError> {
        let is_v6 = target.contains(':');

        // Check if target is a known router name
        if self.graph.nodes.contains_key(target) {
            let path = self.find_path(from_node, target, is_v6)?;
            return Ok(path.status == ReachabilityStatus::Reachable);
        }

        // Try parsing as IP address
        if let Ok(ip) = target.parse::<IpAddr>() {
            for (hostname, node) in &self.graph.nodes {
                let matches = match ip {
                    IpAddr::V4(v4) => {
                        node.loopbacks_v4.contains(&v4) || node.interface_ips_v4.contains(&v4)
                    }
                    IpAddr::V6(v6) => {
                        node.loopbacks_v6.contains(&v6) || node.interface_ips_v6.contains(&v6)
                    }
                };
                if matches {
                    let path = self.find_path(from_node, hostname, is_v6)?;
                    return Ok(path.status == ReachabilityStatus::Reachable);
                }
            }
        }

        Err(ReachabilityError::TargetNotFound(target.to_string()))
    }

    /// Check if a given route prefix exists or is advertised to a node.
    pub fn has_route(&self, node: &str, prefix_str: &str) -> bool {
        let dev = match self.devices.iter().find(|d| d.hostname == node) {
            Some(d) => d,
            None => return false,
        };

        // 1. Directly connected interfaces
        for iface in &dev.interfaces {
            if !iface.enabled {
                continue;
            }
            if let Some(ref v4) = iface.ipv4 {
                if v4.to_string() == prefix_str || v4.network_prefix().to_string() == prefix_str {
                    return true;
                }
            }
            if let Some(ref v6) = iface.ipv6 {
                if v6.to_string() == prefix_str || v6.network_prefix().to_string() == prefix_str {
                    return true;
                }
            }
        }

        // 2. Static routes
        for r in &dev.static_routes {
            if r.destination.to_string() == prefix_str {
                return true;
            }
        }
        for r in &dev.static_routes_v6 {
            if r.destination.to_string() == prefix_str {
                return true;
            }
        }

        // 3. Routes advertised by reachable peers in IGP/BGP
        let is_v6 = prefix_str.contains(':');
        for other_dev in self.devices {
            if other_dev.hostname == node {
                continue;
            }

            // Check if other device originates this prefix
            let mut originates = false;
            for iface in &other_dev.interfaces {
                if !iface.enabled {
                    continue;
                }
                if let Some(ref v4) = iface.ipv4 {
                    if v4.to_string() == prefix_str || v4.network_prefix().to_string() == prefix_str
                    {
                        originates = true;
                        break;
                    }
                }
                if let Some(ref v6) = iface.ipv6 {
                    if v6.to_string() == prefix_str || v6.network_prefix().to_string() == prefix_str
                    {
                        originates = true;
                        break;
                    }
                }
            }

            if !originates {
                if let Some(ref bgp) = other_dev.bgp {
                    if bgp.networks.iter().any(|p| p.to_string() == prefix_str)
                        || bgp.networks_v6.iter().any(|p| p.to_string() == prefix_str)
                    {
                        originates = true;
                    }
                }
            }

            if originates {
                // If other_dev originates and is reachable, route exists in node's RIB
                if let Ok(path) = self.find_path(node, &other_dev.hostname, is_v6) {
                    if path.status == ReachabilityStatus::Reachable {
                        return true;
                    }
                }
            }
        }

        false
    }
}

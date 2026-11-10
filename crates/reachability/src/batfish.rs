use ir::DeviceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use topology::Topology;

/// Batfish Vendor-Independent format representation (subset for topology and reachability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishSnapshot {
    pub name: String,
    pub nodes: BTreeMap<String, BatfishConfiguration>,
    pub topology: Vec<BatfishEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishConfiguration {
    pub hostname: String,
    pub interfaces: BTreeMap<String, BatfishInterface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_vrf: Option<BatfishVrf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishInterface {
    pub name: String,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_address: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf_cost: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf_area: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishVrf {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub static_routes: Vec<BatfishStaticRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bgp_process: Option<BatfishBgpProcess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf_process: Option<BatfishOspfProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishStaticRoute {
    pub prefix: String,
    pub next_hop_ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishBgpProcess {
    pub router_id: Option<String>,
    pub neighbors: BTreeMap<String, BatfishBgpNeighbor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishBgpNeighbor {
    pub remote_as: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishOspfProcess {
    pub router_id: Option<String>,
    pub process_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatfishEdge {
    pub node1: String,
    pub interface1: String,
    pub node2: String,
    pub interface2: String,
}

/// Export devices and inferred topology to Batfish vendor-independent format.
pub fn export_to_batfish(
    snapshot_name: &str,
    devices: &[DeviceConfig],
    topology: &Topology,
) -> BatfishSnapshot {
    let mut nodes = BTreeMap::new();

    for dev in devices {
        let mut ifaces = BTreeMap::new();
        for iface in &dev.interfaces {
            let mut all_addrs = Vec::new();
            if let Some(ref v4) = iface.ipv4 {
                all_addrs.push(v4.to_string());
            }
            if let Some(ref v6) = iface.ipv6 {
                all_addrs.push(v6.to_string());
            }

            let primary = all_addrs.first().cloned();

            let ospf_cost = iface.ospf.as_ref().and_then(|o| o.cost).map(|c| c as u32);
            let ospf_area = iface.ospf.as_ref().map(|o| o.area.to_string());

            ifaces.insert(
                iface.name.clone(),
                BatfishInterface {
                    name: iface.name.clone(),
                    active: iface.enabled,
                    description: iface.description.clone(),
                    primary_address: primary,
                    all_addresses: all_addrs,
                    ospf_cost,
                    ospf_area,
                },
            );
        }

        // Static routes
        let mut static_routes = Vec::new();
        for r in &dev.static_routes {
            static_routes.push(BatfishStaticRoute {
                prefix: r.destination.to_string(),
                next_hop_ip: r.next_hop.to_string(),
            });
        }
        for r in &dev.static_routes_v6 {
            static_routes.push(BatfishStaticRoute {
                prefix: r.destination.to_string(),
                next_hop_ip: r.next_hop.to_string(),
            });
        }

        // BGP
        let bgp_process = dev.bgp.as_ref().map(|b| {
            let mut neighbors = BTreeMap::new();
            for n in &b.neighbors {
                neighbors.insert(
                    n.remote_address.to_string(),
                    BatfishBgpNeighbor {
                        remote_as: n.remote_as,
                        peer_group: n.peer_group.clone(),
                    },
                );
            }
            for n in &b.neighbors_v6 {
                neighbors.insert(
                    n.remote_address.to_string(),
                    BatfishBgpNeighbor {
                        remote_as: n.remote_as,
                        peer_group: n.peer_group.clone(),
                    },
                );
            }
            BatfishBgpProcess {
                router_id: b.router_id.map(|r| r.to_string()),
                neighbors,
            }
        });

        // OSPF
        let ospf_process = dev.ospf.as_ref().map(|o| BatfishOspfProcess {
            router_id: o.router_id.map(|r| r.to_string()),
            process_id: o.process_id.to_string(),
        });

        let vrf = BatfishVrf {
            name: "default".to_string(),
            static_routes,
            bgp_process,
            ospf_process,
        };

        nodes.insert(
            dev.hostname.clone(),
            BatfishConfiguration {
                hostname: dev.hostname.clone(),
                interfaces: ifaces,
                default_vrf: Some(vrf),
            },
        );
    }

    let mut edges = Vec::new();
    for link in &topology.links {
        edges.push(BatfishEdge {
            node1: link.endpoint_a.hostname.clone(),
            interface1: link.endpoint_a.interface_name.clone(),
            node2: link.endpoint_b.hostname.clone(),
            interface2: link.endpoint_b.interface_name.clone(),
        });
    }

    BatfishSnapshot {
        name: snapshot_name.to_string(),
        nodes,
        topology: edges,
    }
}

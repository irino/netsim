use ir::{DeviceConfig, Ipv4Prefix};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Endpoint of a point-to-point link.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LinkEndpoint {
    pub hostname: String,
    pub interface_name: String,
}

/// A point-to-point network link between two device interfaces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Link {
    pub subnet: Ipv4Prefix,
    pub endpoint_a: LinkEndpoint,
    pub endpoint_b: LinkEndpoint,
}

/// Topology inference warning message.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TopologyWarning {
    pub subnet: Ipv4Prefix,
    pub message: String,
}

/// A multi-access LAN segment connecting three or more device interfaces via a shared subnet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LanSegment {
    pub subnet: Ipv4Prefix,
    pub endpoints: Vec<LinkEndpoint>,
}

/// Inferred network topology containing point-to-point links, multi-access LAN segments, and warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Topology {
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lan_segments: Vec<LanSegment>,
    pub warnings: Vec<TopologyWarning>,
}

#[derive(Debug, Clone)]
struct SubnetInterface {
    hostname: String,
    interface_name: String,
}

/// Infer network links and topology warnings from a list of device configurations.
pub fn infer_topology(devices: &[DeviceConfig]) -> Topology {
    let mut subnets: BTreeMap<Ipv4Prefix, Vec<SubnetInterface>> = BTreeMap::new();

    for device in devices {
        for iface in &device.interfaces {
            // Rule 5: Exclude shutdown interfaces and interfaces without IPv4 addresses
            if !iface.enabled {
                continue;
            }
            let ipv4 = match &iface.ipv4 {
                Some(ip) => ip,
                None => continue,
            };

            // Rule 6: Do not infer links between loopback interfaces
            if iface.is_loopback() {
                continue;
            }

            // Rule 1: Calculate network prefix
            let net_prefix = ipv4.network_prefix();

            subnets
                .entry(net_prefix)
                .or_default()
                .push(SubnetInterface {
                    hostname: device.hostname.clone(),
                    interface_name: iface.name.clone(),
                });
        }
    }

    let mut links = Vec::new();
    let mut lan_segments = Vec::new();
    let mut warnings = Vec::new();

    for (subnet, mut candidates) in subnets {
        // Sort candidates deterministically by hostname and interface_name
        candidates.sort_by(|a, b| {
            a.hostname
                .cmp(&b.hostname)
                .then_with(|| a.interface_name.cmp(&b.interface_name))
        });

        // Check if multiple interfaces belong to the same device
        let mut device_seen = std::collections::BTreeSet::new();
        let mut duplicate_device = None;
        for c in &candidates {
            if !device_seen.insert(&c.hostname) {
                duplicate_device = Some(c.hostname.clone());
                break;
            }
        }

        if let Some(host) = duplicate_device {
            warnings.push(TopologyWarning {
                subnet: subnet.clone(),
                message: format!(
                    "Subnet {} has multiple interfaces on the same device '{}'; skipping automatic link inference",
                    subnet, host
                ),
            });
            continue;
        }

        if candidates.len() == 2 {
            let (ep_a, ep_b) = if candidates[0].hostname < candidates[1].hostname
                || (candidates[0].hostname == candidates[1].hostname
                    && candidates[0].interface_name <= candidates[1].interface_name)
            {
                (
                    LinkEndpoint {
                        hostname: candidates[0].hostname.clone(),
                        interface_name: candidates[0].interface_name.clone(),
                    },
                    LinkEndpoint {
                        hostname: candidates[1].hostname.clone(),
                        interface_name: candidates[1].interface_name.clone(),
                    },
                )
            } else {
                (
                    LinkEndpoint {
                        hostname: candidates[1].hostname.clone(),
                        interface_name: candidates[1].interface_name.clone(),
                    },
                    LinkEndpoint {
                        hostname: candidates[0].hostname.clone(),
                        interface_name: candidates[0].interface_name.clone(),
                    },
                )
            };

            links.push(Link {
                subnet,
                endpoint_a: ep_a,
                endpoint_b: ep_b,
            });
        } else if candidates.len() >= 3 {
            let endpoints: Vec<LinkEndpoint> = candidates
                .into_iter()
                .map(|c| LinkEndpoint {
                    hostname: c.hostname,
                    interface_name: c.interface_name,
                })
                .collect();

            lan_segments.push(LanSegment { subnet, endpoints });
        }
    }

    // Rules 8 & 9: Ensure links, lan_segments, and warnings are sorted deterministically and deduplicated
    links.sort();
    links.dedup();

    lan_segments.sort();
    lan_segments.dedup();

    warnings.sort();
    warnings.dedup();

    Topology {
        links,
        lan_segments,
        warnings,
    }
}

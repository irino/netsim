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

/// Inferred network topology containing point-to-point links and warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Topology {
    pub links: Vec<Link>,
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
    let mut warnings = Vec::new();

    for (subnet, mut candidates) in subnets {
        // Sort candidates deterministically by hostname and interface_name
        candidates.sort_by(|a, b| {
            a.hostname
                .cmp(&b.hostname)
                .then_with(|| a.interface_name.cmp(&b.interface_name))
        });

        if candidates.len() == 2 {
            // Check if from two different devices or same device
            if candidates[0].hostname != candidates[1].hostname {
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
            } else {
                warnings.push(TopologyWarning {
                    subnet: subnet.clone(),
                    message: format!(
                        "Subnet {} has multiple interfaces on the same device '{}'; skipping automatic link inference",
                        subnet, candidates[0].hostname
                    ),
                });
            }
        } else if candidates.len() >= 3 {
            // Rule 4: Subnets with 3+ eligible interfaces emit warning and skip automatic link generation
            warnings.push(TopologyWarning {
                subnet: subnet.clone(),
                message: format!(
                    "Subnet {} is shared across {} interfaces; multi-access subnets are skipped in MVP",
                    subnet,
                    candidates.len()
                ),
            });
        }
    }

    // Rules 8 & 9: Ensure links and warnings are sorted deterministically and deduplicated
    links.sort();
    links.dedup();

    warnings.sort();
    warnings.dedup();

    Topology { links, warnings }
}

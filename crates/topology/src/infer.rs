use ir::{DeviceConfig, Ipv4Prefix, Ipv6Prefix};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// An IP subnet (IPv4 or IPv6 prefix).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IpSubnet {
    Ipv4(Ipv4Prefix),
    Ipv6(Ipv6Prefix),
}

impl fmt::Display for IpSubnet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpSubnet::Ipv4(p) => write!(f, "{}", p),
            IpSubnet::Ipv6(p) => write!(f, "{}", p),
        }
    }
}

impl From<Ipv4Prefix> for IpSubnet {
    fn from(p: Ipv4Prefix) -> Self {
        IpSubnet::Ipv4(p)
    }
}

impl From<Ipv6Prefix> for IpSubnet {
    fn from(p: Ipv6Prefix) -> Self {
        IpSubnet::Ipv6(p)
    }
}

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
    pub subnet: IpSubnet,
    pub endpoint_a: LinkEndpoint,
    pub endpoint_b: LinkEndpoint,
}

/// Topology inference warning message.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TopologyWarning {
    pub subnet: IpSubnet,
    pub message: String,
}

/// A multi-access LAN segment connecting three or more device interfaces via a shared subnet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LanSegment {
    pub subnet: IpSubnet,
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
    let mut subnets: BTreeMap<IpSubnet, Vec<SubnetInterface>> = BTreeMap::new();

    for device in devices {
        for iface in &device.interfaces {
            // Rule 5: Exclude shutdown interfaces and interfaces without IPv4 addresses
            if !iface.enabled {
                continue;
            }
            // Rule 6: Do not infer links between loopback interfaces
            if iface.is_loopback() {
                continue;
            }

            if let Some(ref ipv4) = iface.ipv4 {
                let net_prefix = IpSubnet::Ipv4(ipv4.network_prefix());
                subnets
                    .entry(net_prefix)
                    .or_default()
                    .push(SubnetInterface {
                        hostname: device.hostname.clone(),
                        interface_name: iface.name.clone(),
                    });
            }

            if let Some(ref ipv6) = iface.ipv6 {
                let net_prefix = IpSubnet::Ipv6(ipv6.network_prefix());
                subnets
                    .entry(net_prefix)
                    .or_default()
                    .push(SubnetInterface {
                        hostname: device.hostname.clone(),
                        interface_name: iface.name.clone(),
                    });
            }
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

    // Deduplicate dual-stack links connecting the exact same pair of interfaces:
    // If a link between endpoint_a and endpoint_b already exists (e.g. from IPv4),
    // retain the first one (IPv4 before IPv6 due to IpSubnet ordering)
    let mut deduplicated_links: Vec<Link> = Vec::new();
    let mut seen_endpoints = std::collections::BTreeSet::new();

    for link in links {
        let ep_pair = (
            link.endpoint_a.hostname.clone(),
            link.endpoint_a.interface_name.clone(),
            link.endpoint_b.hostname.clone(),
            link.endpoint_b.interface_name.clone(),
        );
        if seen_endpoints.insert(ep_pair) {
            deduplicated_links.push(link);
        }
    }

    // Deduplicate LAN segments connecting the exact same set of endpoints
    let mut deduplicated_lans: Vec<LanSegment> = Vec::new();
    let mut seen_lan_endpoints = std::collections::BTreeSet::new();

    for lan in lan_segments {
        let eps: Vec<(String, String)> = lan
            .endpoints
            .iter()
            .map(|e| (e.hostname.clone(), e.interface_name.clone()))
            .collect();
        if seen_lan_endpoints.insert(eps) {
            deduplicated_lans.push(lan);
        }
    }

    // Rules 8 & 9: Ensure links, lan_segments, and warnings are sorted deterministically and deduplicated
    deduplicated_links.sort();
    deduplicated_lans.sort();

    warnings.sort();
    warnings.dedup();

    Topology {
        links: deduplicated_links,
        lan_segments: deduplicated_lans,
        warnings,
    }
}

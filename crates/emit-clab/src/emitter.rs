use crate::mapper::InterfaceMapper;
use ir::DeviceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use topology::Topology;

#[derive(Debug, Error)]
pub enum EmitClabError {
    #[error("YAML serialization error: {0}")]
    Serialization(#[from] serde_yaml::Error),
    #[error("Missing interface mapping for node '{0}', interface '{1}'")]
    MissingInterfaceMapping(String, String),
}

/// Containerlab node definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClabNode {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub binds: Vec<String>,
}

/// Containerlab brief-format link definition (`endpoints: ["nodeA:eth1", "nodeB:eth2"]`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClabLink {
    pub endpoints: Vec<String>,
}

/// Containerlab topology section (`nodes`, `links`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClabTopologyContent {
    pub nodes: BTreeMap<String, ClabNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ClabLink>,
}

/// Root Containerlab topology definition file (`*.clab.yml`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClabTopology {
    pub name: String,
    pub topology: ClabTopologyContent,
}

/// Output of Containerlab topology generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClabOutput {
    pub yaml: String,
    pub mapper: InterfaceMapper,
    pub warnings: Vec<String>,
}

/// Options for Containerlab topology emission.
#[derive(Debug, Clone)]
pub struct EmitClabOptions {
    pub image: String,
    pub config_dir: String,
}

impl Default for EmitClabOptions {
    fn default() -> Self {
        Self {
            image: "zebra-rs:latest".to_string(),
            config_dir: "./".to_string(),
        }
    }
}

/// Build a `ClabTopology` struct from device configurations and inferred topology.
pub fn build_clab_topology(
    lab_name: &str,
    devices: &[DeviceConfig],
    topology: &Topology,
    options: &EmitClabOptions,
    mapper: &InterfaceMapper,
) -> Result<(ClabTopology, Vec<String>), EmitClabError> {
    let mut warnings = Vec::new();

    // Propagate warnings from topology inference (e.g. multi-access subnets)
    for warn in &topology.warnings {
        warnings.push(format!(
            "[TopologyWarning] {}: {}",
            warn.subnet, warn.message
        ));
    }

    // Build nodes deterministically in BTreeMap sorted by hostname
    let mut nodes = BTreeMap::new();
    let mut sorted_devices: Vec<&DeviceConfig> = devices.iter().collect();
    sorted_devices.sort_by(|a, b| a.hostname.cmp(&b.hostname));

    for dev in sorted_devices {
        let config_path = format!(
            "{}/{}.zebra.yaml",
            options.config_dir.trim_end_matches('/'),
            dev.hostname
        );
        let bind_str = format!("{}:/etc/zebra-rs/zebra.yaml", config_path);

        nodes.insert(
            dev.hostname.clone(),
            ClabNode {
                kind: "linux".to_string(),
                image: Some(options.image.clone()),
                binds: vec![bind_str],
            },
        );
    }

    // Build bridge nodes for multi-access LAN segments
    let mut sorted_lan_segments = topology.lan_segments.clone();
    sorted_lan_segments.sort();

    for lan_idx in 0..sorted_lan_segments.len() {
        let bridge_name = format!("br-lan{}", lan_idx);
        nodes.insert(
            bridge_name,
            ClabNode {
                kind: "bridge".to_string(),
                image: None,
                binds: Vec::new(),
            },
        );
    }

    // Build links deterministically
    let mut links = Vec::new();
    let mut sorted_links = topology.links.clone();
    sorted_links.sort();

    for link in sorted_links {
        let ep_a = &link.endpoint_a;
        let ep_b = &link.endpoint_b;

        let linux_a = mapper
            .get_linux_name(&ep_a.hostname, &ep_a.interface_name)
            .ok_or_else(|| {
                EmitClabError::MissingInterfaceMapping(
                    ep_a.hostname.clone(),
                    ep_a.interface_name.clone(),
                )
            })?;

        let linux_b = mapper
            .get_linux_name(&ep_b.hostname, &ep_b.interface_name)
            .ok_or_else(|| {
                EmitClabError::MissingInterfaceMapping(
                    ep_b.hostname.clone(),
                    ep_b.interface_name.clone(),
                )
            })?;

        let endpoint_str_a = format!("{}:{}", ep_a.hostname, linux_a);
        let endpoint_str_b = format!("{}:{}", ep_b.hostname, linux_b);

        // Normalize link endpoint order for stable output
        let (first, second) = if endpoint_str_a <= endpoint_str_b {
            (endpoint_str_a, endpoint_str_b)
        } else {
            (endpoint_str_b, endpoint_str_a)
        };

        links.push(ClabLink {
            endpoints: vec![first, second],
        });
    }

    // Add links between routers and bridge nodes for multi-access LAN segments
    for (lan_idx, lan) in sorted_lan_segments.iter().enumerate() {
        let bridge_name = format!("br-lan{}", lan_idx);
        let mut sorted_endpoints = lan.endpoints.clone();
        sorted_endpoints.sort();

        for (ep_idx, ep) in sorted_endpoints.iter().enumerate() {
            let linux_name = mapper
                .get_linux_name(&ep.hostname, &ep.interface_name)
                .ok_or_else(|| {
                    EmitClabError::MissingInterfaceMapping(
                        ep.hostname.clone(),
                        ep.interface_name.clone(),
                    )
                })?;

            let endpoint_router = format!("{}:{}", ep.hostname, linux_name);
            let endpoint_bridge = format!("{}:eth{}", bridge_name, ep_idx + 1);

            let (first, second) = if endpoint_router <= endpoint_bridge {
                (endpoint_router, endpoint_bridge)
            } else {
                (endpoint_bridge, endpoint_router)
            };

            links.push(ClabLink {
                endpoints: vec![first, second],
            });
        }
    }

    links.sort();
    links.dedup();

    warnings.sort();
    warnings.dedup();

    let clab = ClabTopology {
        name: lab_name.to_string(),
        topology: ClabTopologyContent { nodes, links },
    };

    Ok((clab, warnings))
}

/// Convert devices and topology into Containerlab topology YAML.
pub fn emit_clab_topology(
    lab_name: &str,
    devices: &[DeviceConfig],
    topology: &Topology,
    options: Option<EmitClabOptions>,
) -> Result<ClabOutput, EmitClabError> {
    let opts = options.unwrap_or_default();
    let mapper = InterfaceMapper::build(devices);
    let (clab_topo, warnings) = build_clab_topology(lab_name, devices, topology, &opts, &mapper)?;

    let yaml = serde_yaml::to_string(&clab_topo)?;

    Ok(ClabOutput {
        yaml,
        mapper,
        warnings,
    })
}

use ir::{DeviceConfig, OspfAreaId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitZebraError {
    #[error("YAML serialization error: {0}")]
    Serialization(#[from] serde_yaml::Error),
}

/// Diagnostic warning for unsupported or unmapped IR features in zebra-rs YAML configuration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EmitWarning {
    pub category: String,
    pub message: String,
}

/// zebra-rs system configuration (`system`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraSystem {
    pub hostname: String,
}

/// zebra-rs IPv4 interface address container (`interface -> ipv4`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraInterfaceIpv4 {
    pub address: Vec<String>,
}

/// zebra-rs interface entry (`interface`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraInterface {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4: Option<ZebraInterfaceIpv4>,
}

/// zebra-rs OSPF interface entry (`router -> ospf -> area -> interface`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraOspfInterface {
    pub name: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive: Option<bool>,
}

/// zebra-rs OSPF area entry (`router -> ospf -> area`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraOspfArea {
    pub area_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interface: Vec<ZebraOspfInterface>,
}

/// zebra-rs OSPF protocol configuration (`router -> ospf`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraOspf {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_id: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub area: Vec<ZebraOspfArea>,
}

/// zebra-rs routing protocols container (`router`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraRouter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf: Option<ZebraOspf>,
}

/// Root configuration representation for zebra-rs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraConfig {
    pub system: ZebraSystem,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interface: Vec<ZebraInterface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<ZebraRouter>,
}

/// Result of emitting a zebra-rs configuration containing the YAML output and explicit warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitResult {
    pub yaml: String,
    pub warnings: Vec<EmitWarning>,
}

/// Convert a vendor-independent `DeviceConfig` into a `ZebraConfig` struct and explicit warnings.
pub fn build_zebra_config(device: &DeviceConfig) -> (ZebraConfig, Vec<EmitWarning>) {
    let mut warnings = Vec::new();

    let system = ZebraSystem {
        hostname: device.hostname.clone(),
    };

    // Interfaces sorted deterministically by name
    let mut interfaces = Vec::new();
    let mut ospf_by_area: BTreeMap<String, Vec<ZebraOspfInterface>> = BTreeMap::new();

    // Sort device interfaces by name for deterministic ordering
    let mut sorted_interfaces = device.interfaces.clone();
    sorted_interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    for iface in sorted_interfaces {
        if !iface.enabled {
            warnings.push(EmitWarning {
                category: "interface-disabled".to_string(),
                message: format!(
                    "Interface '{}' is administratively disabled/shutdown; zebra-rs top-level interface does not have an admin-down state knob, interface omitted or excluded from active routing",
                    iface.name
                ),
            });
        }

        if let Some(ref desc) = iface.description {
            warnings.push(EmitWarning {
                category: "interface-description".to_string(),
                message: format!(
                    "Interface '{}' description '{}' is not representable in zebra-rs interface schema and will be omitted",
                    iface.name, desc
                ),
            });
        }

        let ipv4 = iface.ipv4.as_ref().map(|prefix| ZebraInterfaceIpv4 {
            address: vec![prefix.to_string()],
        });

        interfaces.push(ZebraInterface {
            name: iface.name.clone(),
            ipv4,
        });

        // Collect OSPF interface participation if present
        if let Some(ref ospf_cfg) = iface.ospf {
            let area_key = match &ospf_cfg.area {
                OspfAreaId::Number(n) => n.to_string(),
                OspfAreaId::Ipv4(ip) => ip.to_string(),
            };

            ospf_by_area
                .entry(area_key)
                .or_default()
                .push(ZebraOspfInterface {
                    name: iface.name.clone(),
                    enabled: iface.enabled,
                    cost: ospf_cfg.cost,
                    passive: ospf_cfg.passive,
                });
        }
    }

    if let Some(ref ospf_proc) = device.ospf {
        if ospf_proc.process_id != 1 {
            warnings.push(EmitWarning {
                category: "ospf-process-id".to_string(),
                message: format!(
                    "OSPF process ID {} is not modeled in single-instance zebra-rs 'router ospf' configuration",
                    ospf_proc.process_id
                ),
            });
        }
    }

    // Report unparsed lines preserved from parser
    for unparsed in &device.unparsed_lines {
        warnings.push(EmitWarning {
            category: "unparsed-line".to_string(),
            message: format!(
                "Unparsed line at {}:{}: '{}' ({})",
                unparsed.file_path.as_deref().unwrap_or("<unknown>"),
                unparsed.line_number,
                unparsed.content,
                unparsed.reason
            ),
        });
    }

    let router = if let Some(ref ospf_proc) = device.ospf {
        let mut areas = Vec::new();
        for (area_id, mut ospf_ifaces) in ospf_by_area {
            ospf_ifaces.sort_by(|a, b| a.name.cmp(&b.name));
            areas.push(ZebraOspfArea {
                area_id,
                interface: ospf_ifaces,
            });
        }

        Some(ZebraRouter {
            ospf: Some(ZebraOspf {
                router_id: ospf_proc.router_id,
                area: areas,
            }),
        })
    } else if !ospf_by_area.is_empty() {
        let mut areas = Vec::new();
        for (area_id, mut ospf_ifaces) in ospf_by_area {
            ospf_ifaces.sort_by(|a, b| a.name.cmp(&b.name));
            areas.push(ZebraOspfArea {
                area_id,
                interface: ospf_ifaces,
            });
        }

        Some(ZebraRouter {
            ospf: Some(ZebraOspf {
                router_id: None,
                area: areas,
            }),
        })
    } else {
        None
    };

    warnings.sort();
    warnings.dedup();

    (
        ZebraConfig {
            system,
            interface: interfaces,
            router,
        },
        warnings,
    )
}

/// Convert a vendor-independent `DeviceConfig` into a zebra-rs YAML configuration string.
pub fn emit_zebra_yaml(device: &DeviceConfig) -> Result<String, EmitZebraError> {
    let (zebra_cfg, _) = build_zebra_config(device);
    let yaml = serde_yaml::to_string(&zebra_cfg)?;
    Ok(yaml)
}

/// Convert a vendor-independent `DeviceConfig` into a zebra-rs YAML configuration string with explicit warnings.
pub fn emit_zebra(device: &DeviceConfig) -> Result<EmitResult, EmitZebraError> {
    let (zebra_cfg, warnings) = build_zebra_config(device);
    let yaml = serde_yaml::to_string(&zebra_cfg)?;
    Ok(EmitResult { yaml, warnings })
}

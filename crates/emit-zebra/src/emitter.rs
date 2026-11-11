use ir::{DeviceConfig, OspfAreaId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
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

/// zebra-rs static IPv6 route entry (`ip6 -> route`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraStaticRouteV6 {
    pub destination: String,
    #[serde(rename = "next-hop")]
    pub next_hop: Ipv6Addr,
}

/// zebra-rs IPv6 level configuration container (`ip6`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraIp6 {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<ZebraStaticRouteV6>,
}

/// zebra-rs IPv6 interface address container (`interface -> ipv6`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraInterfaceIpv6 {
    pub address: Vec<String>,
}

/// zebra-rs VXLAN VNI binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraVxlanBinding {
    pub vni: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u16>,
}

/// zebra-rs interface VXLAN / VTEP configuration (`interface -> vxlan`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraInterfaceVxlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_ip: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vni: Vec<ZebraVxlanBinding>,
}

/// zebra-rs BGP EVPN neighbor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraBgpEvpnNeighbor {
    pub peer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector_client: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
}

/// zebra-rs BGP EVPN address-family configuration (`router -> bgp -> evpn`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraBgpEvpn {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advertise_all_vni: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advertise_default_gw: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neighbor: Vec<ZebraBgpEvpnNeighbor>,
}

fn default_true() -> bool {
    true
}

/// zebra-rs SRv6 micro-segment (uSID) parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraSrV6Usid {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_length: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_length: Option<u8>,
}

/// zebra-rs SRv6 locator configuration (`segment-routing -> srv6 -> locators -> locator`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraSrV6Locator {
    pub name: String,
    pub prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub micro_segment: Option<ZebraSrV6Usid>,
}

/// zebra-rs SRv6 configuration (`segment-routing -> srv6`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraSrV6 {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encapsulation_source_address: Option<Ipv6Addr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locator: Vec<ZebraSrV6Locator>,
}

/// zebra-rs Segment Routing container (`segment-routing`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraSegmentRouting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub srv6: Option<ZebraSrV6>,
}

/// zebra-rs static route entry (`ip -> route`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraStaticRoute {
    pub destination: String,
    #[serde(rename = "next-hop")]
    pub next_hop: Ipv4Addr,
}

/// zebra-rs IP level configuration container (`ip`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraIp {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<ZebraStaticRoute>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<ZebraInterfaceIpv6>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vxlan: Option<ZebraInterfaceVxlan>,
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

/// zebra-rs BFD container (`bfd`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraBfdConfig {
    pub enabled: bool,
}

/// zebra-rs Route Reflector container (`route-reflector`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraRouteReflector {
    pub client: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Ipv4Addr>,
}

/// zebra-rs BGP neighbor group / template (`router -> bgp -> neighbor-group`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraBgpNeighborGroup {
    pub name: String,
    #[serde(rename = "peer-as", skip_serializing_if = "Option::is_none")]
    pub peer_as: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector: Option<ZebraRouteReflector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<ZebraBfdConfig>,
}

/// zebra-rs community-set member (`community-set -> members`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraCommunitySet {
    pub name: String,
    pub members: Vec<String>,
}

/// zebra-rs IS-IS interface IPv4 enablement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraIsisInterfaceIpv4 {
    pub enabled: bool,
}

/// zebra-rs IS-IS interface configuration (`router -> isis -> interface`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraIsisInterface {
    pub if_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<ZebraBfdConfig>,
    pub ipv4: ZebraIsisInterfaceIpv4,
}

/// zebra-rs IS-IS protocol configuration (`router -> isis`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraIsis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interface: Vec<ZebraIsisInterface>,
}
/// zebra-rs BGP neighbor configuration (`router -> bgp -> neighbor`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraBgpNeighbor {
    pub peer_address: Ipv4Addr,
    pub remote_as: u32,
    #[serde(rename = "peer-group", skip_serializing_if = "Option::is_none")]
    pub peer_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector: Option<ZebraRouteReflector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<ZebraBfdConfig>,
}

/// zebra-rs BGP IPv6 neighbor configuration (`router -> bgp -> neighbor-v6`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraBgpNeighborV6 {
    pub peer_address: Ipv6Addr,
    pub remote_as: u32,
    #[serde(rename = "peer-group", skip_serializing_if = "Option::is_none")]
    pub peer_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector: Option<ZebraRouteReflector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<ZebraBfdConfig>,
}

/// zebra-rs BGP protocol configuration (`router -> bgp`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraBgp {
    #[serde(rename = "as")]
    pub as_number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_id: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Ipv4Addr>,
    #[serde(rename = "neighbor-group", skip_serializing_if = "Vec::is_empty")]
    pub neighbor_group: Vec<ZebraBgpNeighborGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub neighbor: Vec<ZebraBgpNeighbor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub network: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub neighbor_v6: Vec<ZebraBgpNeighborV6>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub network_v6: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evpn: Option<ZebraBgpEvpn>,
}

/// zebra-rs routing protocols container (`router`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraRouter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf: Option<ZebraOspf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf6: Option<ZebraOspf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isis: Option<ZebraIsis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bgp: Option<ZebraBgp>,
}

/// zebra-rs prefix-set entry (`prefix-set -> prefixes`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraPrefixEntry {
    pub prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ge: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub le: Option<u8>,
}

/// zebra-rs prefix-set container (`prefix-set`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraPrefixSet {
    pub name: String,
    pub prefixes: Vec<ZebraPrefixEntry>,
}

/// zebra-rs match conditions (`policy -> entry -> match`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraPolicyMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_set: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
}

/// zebra-rs numeric set operation (`set: NUM`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraNumericSet {
    pub set: u32,
}

/// zebra-rs set clauses (`policy -> entry -> set`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ZebraPolicySet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub med: Option<ZebraNumericSet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_preference: Option<ZebraNumericSet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
}

/// zebra-rs policy entry (`policy -> entry`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraPolicyEntry {
    pub number: u32,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_: Option<ZebraPolicyMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<ZebraPolicySet>,
}

/// zebra-rs policy container (`policy`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraPolicy {
    pub name: String,
    pub entry: Vec<ZebraPolicyEntry>,
}

/// Root configuration representation for zebra-rs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZebraConfig {
    pub system: ZebraSystem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<ZebraIp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip6: Option<ZebraIp6>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interface: Vec<ZebraInterface>,
    #[serde(rename = "prefix-set", default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_set: Vec<ZebraPrefixSet>,
    #[serde(
        rename = "community-set",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub community_set: Vec<ZebraCommunitySet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy: Vec<ZebraPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<ZebraRouter>,
    #[serde(rename = "segment-routing", skip_serializing_if = "Option::is_none")]
    pub segment_routing: Option<ZebraSegmentRouting>,
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

    // Static routes under `ip -> route`
    let ip = if !device.static_routes.is_empty() {
        let mut routes: Vec<ZebraStaticRoute> = device
            .static_routes
            .iter()
            .map(|r| ZebraStaticRoute {
                destination: r.destination.to_string(),
                next_hop: r.next_hop,
            })
            .collect();
        routes.sort_by(|a, b| {
            a.destination
                .cmp(&b.destination)
                .then_with(|| a.next_hop.cmp(&b.next_hop))
        });
        Some(ZebraIp { route: routes })
    } else {
        None
    };

    // Static IPv6 routes under `ip6 -> route`
    let ip6 = if !device.static_routes_v6.is_empty() {
        let mut routes: Vec<ZebraStaticRouteV6> = device
            .static_routes_v6
            .iter()
            .map(|r| ZebraStaticRouteV6 {
                destination: r.destination.to_string(),
                next_hop: r.next_hop,
            })
            .collect();
        routes.sort_by(|a, b| {
            a.destination
                .cmp(&b.destination)
                .then_with(|| a.next_hop.cmp(&b.next_hop))
        });
        Some(ZebraIp6 { route: routes })
    } else {
        None
    };

    // Interfaces sorted deterministically by name
    let mut interfaces = Vec::new();
    let mut ospf_by_area: BTreeMap<String, Vec<ZebraOspfInterface>> = BTreeMap::new();
    let mut ospf6_by_area: BTreeMap<String, Vec<ZebraOspfInterface>> = BTreeMap::new();

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
        let ipv6 = iface.ipv6.as_ref().map(|prefix| ZebraInterfaceIpv6 {
            address: vec![prefix.to_string()],
        });

        let vxlan = iface.vxlan.as_ref().map(|vx| ZebraInterfaceVxlan {
            source_interface: vx.source_interface.clone(),
            local_ip: vx.local_ip,
            udp_port: vx.udp_port,
            vni: vx
                .vni_bindings
                .iter()
                .map(|b| ZebraVxlanBinding {
                    vni: b.vni,
                    vlan: b.vlan,
                })
                .collect(),
        });

        interfaces.push(ZebraInterface {
            name: iface.name.clone(),
            ipv4,
            ipv6,
            vxlan,
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

        // Collect OSPFv3 interface participation if present
        if let Some(ref ospf_cfg) = iface.ospfv3 {
            let area_key = match &ospf_cfg.area {
                OspfAreaId::Number(n) => n.to_string(),
                OspfAreaId::Ipv4(ip) => ip.to_string(),
            };

            ospf6_by_area
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

    let mut isis_interfaces = Vec::new();
    for iface in &device.interfaces {
        if let Some(ref isis_cfg) = iface.isis {
            isis_interfaces.push(ZebraIsisInterface {
                if_name: iface.name.clone(),
                metric: isis_cfg.metric,
                passive: isis_cfg.passive,
                bfd: isis_cfg.bfd.map(|b| ZebraBfdConfig { enabled: b }),
                ipv4: ZebraIsisInterfaceIpv4 { enabled: true },
            });
        }
    }
    isis_interfaces.sort_by(|a, b| a.if_name.cmp(&b.if_name));

    // Community sets emission
    let mut community_sets = Vec::new();
    for cl in &device.community_lists {
        let mut members = Vec::new();
        for entry in &cl.entries {
            members.extend(entry.communities.clone());
        }
        community_sets.push(ZebraCommunitySet {
            name: cl.name.clone(),
            members,
        });
    }
    community_sets.sort_by(|a, b| a.name.cmp(&b.name));
    // Prefix-set emission
    let mut prefix_sets = Vec::new();
    for pl in &device.prefix_lists {
        let mut prefixes = Vec::new();
        for entry in &pl.entries {
            prefixes.push(ZebraPrefixEntry {
                prefix: entry.prefix.to_string(),
                ge: entry.range.as_ref().and_then(|r| r.ge).map(|p| p.value()),
                le: entry.range.as_ref().and_then(|r| r.le).map(|p| p.value()),
            });
        }
        prefix_sets.push(ZebraPrefixSet {
            name: pl.name.clone(),
            prefixes,
        });
    }

    for pl in &device.prefix_lists_v6 {
        let mut prefixes = Vec::new();
        for entry in &pl.entries {
            prefixes.push(ZebraPrefixEntry {
                prefix: entry.prefix.to_string(),
                ge: entry.range.as_ref().and_then(|r| r.ge).map(|p| p.value()),
                le: entry.range.as_ref().and_then(|r| r.le).map(|p| p.value()),
            });
        }
        prefix_sets.push(ZebraPrefixSet {
            name: pl.name.clone(),
            prefixes,
        });
    }
    prefix_sets.sort_by(|a, b| a.name.cmp(&b.name));

    // Policy emission
    let mut policies = Vec::new();
    for pol in &device.policies {
        let mut entries = Vec::new();
        for stmt in &pol.statements {
            let number = stmt
                .name
                .parse::<u32>()
                .unwrap_or_else(|_| (entries.len() as u32 + 1) * 10);
            let action = match stmt.actions.policy_result {
                ir::PolicyAction::Permit => "permit".to_string(),
                ir::PolicyAction::Deny => "deny".to_string(),
            };

            let match_ps = stmt
                .conditions
                .match_prefix_set
                .clone()
                .or_else(|| stmt.conditions.match_prefix_set_v6.clone());
            let match_comm = stmt.conditions.match_community.clone();

            let match_ = if match_ps.is_some() || match_comm.is_some() {
                Some(ZebraPolicyMatch {
                    prefix_set: match_ps,
                    community: match_comm,
                })
            } else {
                None
            };

            let set = if stmt.actions.set_metric.is_some()
                || stmt.actions.set_local_preference.is_some()
                || stmt.actions.set_community.is_some()
            {
                Some(ZebraPolicySet {
                    med: stmt.actions.set_metric.map(|m| ZebraNumericSet { set: m }),
                    local_preference: stmt
                        .actions
                        .set_local_preference
                        .map(|lp| ZebraNumericSet { set: lp }),
                    community: stmt.actions.set_community.clone(),
                })
            } else {
                None
            };

            entries.push(ZebraPolicyEntry {
                number,
                action,
                match_,
                set,
            });
        }
        entries.sort_by_key(|e| e.number);
        policies.push(ZebraPolicy {
            name: pol.name.clone(),
            entry: entries,
        });
    }
    policies.sort_by(|a, b| a.name.cmp(&b.name));

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

    // OSPF configuration
    let ospf = if let Some(ref ospf_proc) = device.ospf {
        let mut areas = Vec::new();
        for (area_id, mut ospf_ifaces) in ospf_by_area {
            ospf_ifaces.sort_by(|a, b| a.name.cmp(&b.name));
            areas.push(ZebraOspfArea {
                area_id,
                interface: ospf_ifaces,
            });
        }

        Some(ZebraOspf {
            router_id: ospf_proc.router_id,
            area: areas,
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

        Some(ZebraOspf {
            router_id: None,
            area: areas,
        })
    } else {
        None
    };

    // OSPFv3 configuration
    let ospf6 = if let Some(ref ospf6_proc) = device.ospfv3 {
        let mut areas = Vec::new();
        for (area_id, mut ospf_ifaces) in ospf6_by_area {
            ospf_ifaces.sort_by(|a, b| a.name.cmp(&b.name));
            areas.push(ZebraOspfArea {
                area_id,
                interface: ospf_ifaces,
            });
        }

        Some(ZebraOspf {
            router_id: ospf6_proc.router_id,
            area: areas,
        })
    } else if !ospf6_by_area.is_empty() {
        let mut areas = Vec::new();
        for (area_id, mut ospf_ifaces) in ospf6_by_area {
            ospf_ifaces.sort_by(|a, b| a.name.cmp(&b.name));
            areas.push(ZebraOspfArea {
                area_id,
                interface: ospf_ifaces,
            });
        }

        Some(ZebraOspf {
            router_id: None,
            area: areas,
        })
    } else {
        None
    };
    // IS-IS configuration
    let isis = if let Some(ref isis_proc) = device.isis {
        let is_type = match isis_proc.level {
            Some(ir::IsisLevel::Level1) => Some("level-1".to_string()),
            Some(ir::IsisLevel::Level2) => Some("level-2-only".to_string()),
            Some(ir::IsisLevel::Level12) => Some("level-1-2".to_string()),
            None => None,
        };
        Some(ZebraIsis {
            net: isis_proc.net.clone(),
            is_type,
            interface: isis_interfaces,
        })
    } else if !isis_interfaces.is_empty() {
        Some(ZebraIsis {
            net: None,
            is_type: None,
            interface: isis_interfaces,
        })
    } else {
        None
    };

    // BGP configuration
    let bgp = if let Some(ref bgp_proc) = device.bgp {
        let mut peer_groups: Vec<ZebraBgpNeighborGroup> = bgp_proc
            .peer_groups
            .iter()
            .map(|pg| ZebraBgpNeighborGroup {
                name: pg.name.clone(),
                peer_as: pg.remote_as,
                route_reflector: pg.route_reflector_client.map(|c| ZebraRouteReflector {
                    client: c,
                    cluster_id: pg.cluster_id,
                }),
                send_community: pg.send_community.clone(),
                bfd: pg.bfd.map(|b| ZebraBfdConfig { enabled: b }),
            })
            .collect();
        peer_groups.sort_by(|a, b| a.name.cmp(&b.name));

        let mut neighbors: Vec<ZebraBgpNeighbor> = bgp_proc
            .neighbors
            .iter()
            .map(|n| ZebraBgpNeighbor {
                peer_address: n.remote_address,
                remote_as: n.remote_as,
                peer_group: n.peer_group.clone(),
                route_reflector: n.route_reflector_client.map(|c| ZebraRouteReflector {
                    client: c,
                    cluster_id: n.cluster_id,
                }),
                send_community: n.send_community.clone(),
                bfd: n.bfd.map(|b| ZebraBfdConfig { enabled: b }),
            })
            .collect();
        neighbors.sort_by_key(|a| a.peer_address);

        let mut networks: Vec<String> = bgp_proc.networks.iter().map(|p| p.to_string()).collect();
        networks.sort();

        let mut neighbors_v6: Vec<ZebraBgpNeighborV6> = bgp_proc
            .neighbors_v6
            .iter()
            .map(|n| ZebraBgpNeighborV6 {
                peer_address: n.remote_address,
                remote_as: n.remote_as,
                peer_group: n.peer_group.clone(),
                route_reflector: n.route_reflector_client.map(|c| ZebraRouteReflector {
                    client: c,
                    cluster_id: n.cluster_id,
                }),
                send_community: n.send_community.clone(),
                bfd: n.bfd.map(|b| ZebraBfdConfig { enabled: b }),
            })
            .collect();
        neighbors_v6.sort_by_key(|a| a.peer_address);

        let mut networks_v6: Vec<String> =
            bgp_proc.networks_v6.iter().map(|p| p.to_string()).collect();
        networks_v6.sort();

        let evpn = bgp_proc.evpn.as_ref().map(|e| ZebraBgpEvpn {
            enabled: e.enabled,
            advertise_all_vni: e.advertise_all_vni,
            advertise_default_gw: e.advertise_default_gw,
            neighbor: e
                .neighbors
                .iter()
                .map(|n| ZebraBgpEvpnNeighbor {
                    peer: n.peer.clone(),
                    route_reflector_client: n.route_reflector_client,
                    send_community: n.send_community.clone(),
                })
                .collect(),
        });

        Some(ZebraBgp {
            as_number: bgp_proc.as_number,
            router_id: bgp_proc.router_id,
            cluster_id: bgp_proc.cluster_id,
            neighbor_group: peer_groups,
            neighbor: neighbors,
            network: networks,
            neighbor_v6: neighbors_v6,
            network_v6: networks_v6,
            evpn,
        })
    } else {
        None
    };

    let router = if ospf.is_some() || ospf6.is_some() || isis.is_some() || bgp.is_some() {
        Some(ZebraRouter {
            ospf,
            ospf6,
            isis,
            bgp,
        })
    } else {
        None
    };

    warnings.sort();
    warnings.dedup();

    let segment_routing = device.segment_routing.as_ref().and_then(|sr| {
        sr.srv6.as_ref().map(|srv6| ZebraSegmentRouting {
            srv6: Some(ZebraSrV6 {
                enabled: srv6.enabled,
                encapsulation_source_address: srv6.encapsulation_source_address,
                locator: srv6
                    .locators
                    .iter()
                    .map(|loc| ZebraSrV6Locator {
                        name: loc.name.clone(),
                        prefix: loc.prefix.to_string(),
                        micro_segment: loc.micro_segment.as_ref().map(|u| ZebraSrV6Usid {
                            enabled: u.enabled,
                            block_length: u.block_length,
                            node_length: u.node_length,
                        }),
                    })
                    .collect(),
            }),
        })
    });

    (
        ZebraConfig {
            system,
            ip,
            ip6,
            interface: interfaces,
            prefix_set: prefix_sets,
            community_set: community_sets,
            policy: policies,
            router,
            segment_routing,
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

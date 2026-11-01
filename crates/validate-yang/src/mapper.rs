use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use ir::{DeviceConfig, Interface, OspfAreaId};
use serde_json::{json, Value};

/// Standard RFC 7951 YANG JSON documents partitioned by top-level module container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YangInstanceDocuments {
    /// Combined standard RFC 7951 document containing both interfaces and routing,
    /// enabling complete inter-module leafref resolution.
    pub combined_json: String,
    /// RFC 8343 / RFC 8344 `ietf-interfaces:interfaces` document.
    pub interfaces_json: Option<String>,
    /// RFC 8349 NMDA `ietf-routing:routing` document covering OSPF, BGP, and Static Routes.
    pub routing_json: Option<String>,
    /// Uncovered fields and reasons why they are not mapped into the selected IETF YANG models.
    pub uncovered_fields: Vec<String>,
}

/// Infers the IANA interface type from interface name.
fn infer_interface_type(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.starts_with("loopback") || lower.starts_with("lo") {
        "iana-if-type:softwareLoopback"
    } else {
        "iana-if-type:ethernetCsmacd"
    }
}

/// Formats OSPF Area ID as dotted quad string (as required by ietf-ospf area-id-type).
fn format_ospf_area_id(area: &OspfAreaId) -> String {
    match area {
        OspfAreaId::Number(n) => {
            let ip = Ipv4Addr::from(*n);
            ip.to_string()
        }
        OspfAreaId::Ipv4(ip) => ip.to_string(),
    }
}

/// Maps a vendor-independent `DeviceConfig` into standard RFC 7951 YANG JSON instance documents.
pub fn map_device_config_to_yang_json(config: &DeviceConfig) -> YangInstanceDocuments {
    let mut uncovered_fields = Vec::new();
    let mut root_obj = serde_json::Map::new();

    // 1. Map interfaces into ietf-interfaces:interfaces and ietf-ip
    let interfaces_json = if !config.interfaces.is_empty() {
        let mut if_entries = Vec::new();
        for iface in &config.interfaces {
            let mut if_obj = serde_json::Map::new();
            if_obj.insert("name".to_string(), json!(iface.name));
            if_obj.insert("type".to_string(), json!(infer_interface_type(&iface.name)));
            if_obj.insert("enabled".to_string(), json!(iface.enabled));

            if let Some(desc) = &iface.description {
                if_obj.insert("description".to_string(), json!(desc));
            }

            if let Some(ip) = &iface.ipv4 {
                let ipv4_obj = json!({
                    "address": [
                        {
                            "ip": ip.address.to_string(),
                            "prefix-length": ip.prefix_length.value()
                        }
                    ]
                });
                if_obj.insert("ietf-ip:ipv4".to_string(), ipv4_obj);
            }

            if_entries.push(Value::Object(if_obj));
        }

        let if_container = json!({
            "interface": if_entries
        });
        root_obj.insert(
            "ietf-interfaces:interfaces".to_string(),
            if_container.clone(),
        );

        let doc = json!({
            "ietf-interfaces:interfaces": if_container
        });
        Some(serde_json::to_string_pretty(&doc).expect("serialize interfaces json"))
    } else {
        None
    };

    // 2. Map routing protocols and static routes into ietf-routing:routing
    let mut control_plane_protocols = Vec::new();

    // OSPF mapping
    if let Some(ospf) = &config.ospf {
        // Group interfaces by OSPF area
        let mut area_map: BTreeMap<String, Vec<&Interface>> = BTreeMap::new();
        for iface in &config.interfaces {
            if let Some(ospf_cfg) = &iface.ospf {
                let area_str = format_ospf_area_id(&ospf_cfg.area);
                area_map.entry(area_str).or_default().push(iface);
            }
        }

        let mut areas = Vec::new();
        for (area_id, ifaces) in area_map {
            let mut if_list = Vec::new();
            for iface in ifaces {
                let mut if_val = serde_json::Map::new();
                if_val.insert("name".to_string(), json!(iface.name));

                if let Some(ospf_cfg) = &iface.ospf {
                    if let Some(cost) = ospf_cfg.cost {
                        if_val.insert("cost".to_string(), json!(cost));
                    }
                    if let Some(passive) = ospf_cfg.passive {
                        if_val.insert("passive".to_string(), json!(passive));
                    }
                }

                if_list.push(Value::Object(if_val));
            }

            areas.push(json!({
                "area-id": area_id,
                "interfaces": {
                    "interface": if_list
                }
            }));
        }

        let mut ospf_container = serde_json::Map::new();
        if let Some(rid) = ospf.router_id {
            // In RFC 9129 ietf-ospf, configurable router-id is `explicit-router-id`
            ospf_container.insert("explicit-router-id".to_string(), json!(rid.to_string()));
        }
        if !areas.is_empty() {
            ospf_container.insert(
                "areas".to_string(),
                json!({
                    "area": areas
                }),
            );
        }

        control_plane_protocols.push(json!({
            "type": "ietf-ospf:ospfv2",
            "name": ospf.process_id.to_string(),
            "ietf-ospf:ospf": Value::Object(ospf_container)
        }));
    }

    // BGP mapping
    if let Some(bgp) = &config.bgp {
        let mut neighbors = Vec::new();
        for nbr in &bgp.neighbors {
            let mut nbr_obj = serde_json::Map::new();
            nbr_obj.insert(
                "remote-address".to_string(),
                json!(nbr.remote_address.to_string()),
            );
            nbr_obj.insert("peer-as".to_string(), json!(nbr.remote_as));
            neighbors.push(Value::Object(nbr_obj));
        }

        let mut global_obj = serde_json::Map::new();
        global_obj.insert("as".to_string(), json!(bgp.as_number));
        if let Some(rid) = bgp.router_id {
            global_obj.insert("identifier".to_string(), json!(rid.to_string()));
        }

        let mut bgp_obj = serde_json::Map::new();
        bgp_obj.insert("global".to_string(), Value::Object(global_obj));
        if !neighbors.is_empty() {
            bgp_obj.insert(
                "neighbors".to_string(),
                json!({
                    "neighbor": neighbors
                }),
            );
        }

        control_plane_protocols.push(json!({
            "type": "ietf-bgp:bgp",
            "name": format!("bgp-{}", bgp.as_number),
            "ietf-bgp:bgp": Value::Object(bgp_obj)
        }));

        if !bgp.networks.is_empty() {
            uncovered_fields.push(format!(
                "bgp.networks: BGP network advertisement prefix list ({} prefix(es)) is represented via routing-policy in IETF BGP (ietf-bgp / ietf-routing-policy) which is outside current scope",
                bgp.networks.len()
            ));
        }
    }

    // Static routes mapping (NMDA static-routes in control-plane-protocol)
    if !config.static_routes.is_empty() {
        let mut routes = Vec::new();
        for sr in &config.static_routes {
            routes.push(json!({
                "destination-prefix": format!("{}/{}", sr.destination.address, sr.destination.prefix_length),
                "next-hop": {
                    "next-hop-address": sr.next_hop.to_string()
                }
            }));
        }

        control_plane_protocols.push(json!({
            "type": "ietf-routing:static",
            "name": "static-routing",
            "static-routes": {
                "ietf-ipv4-unicast-routing:ipv4": {
                    "route": routes
                }
            }
        }));
    }

    let routing_json = if !control_plane_protocols.is_empty() {
        let rt_container = json!({
            "control-plane-protocols": {
                "control-plane-protocol": control_plane_protocols
            }
        });
        root_obj.insert("ietf-routing:routing".to_string(), rt_container.clone());

        let doc = json!({
            "ietf-routing:routing": rt_container
        });
        Some(serde_json::to_string_pretty(&doc).expect("serialize routing json"))
    } else {
        None
    };

    let combined_json =
        serde_json::to_string_pretty(&Value::Object(root_obj)).expect("serialize combined json");

    // Document uncovered root fields
    uncovered_fields.push(
        "hostname: Router hostname is represented in ietf-system (RFC 7317), not ietf-interfaces or ietf-routing"
            .to_string(),
    );
    if !config.unparsed_lines.is_empty() {
        uncovered_fields.push(format!(
            "unparsed_lines: Meta-diagnostic unparsed syntax lines ({} line(s)) have no IETF YANG representation",
            config.unparsed_lines.len()
        ));
    }

    YangInstanceDocuments {
        combined_json,
        interfaces_json,
        routing_json,
        uncovered_fields,
    }
}

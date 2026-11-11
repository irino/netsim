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
    /// RFC 9067 `ietf-routing-policy:routing-policy` document covering prefix sets.
    pub routing_policy_json: Option<String>,
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

            if let Some(ip) = &iface.ipv6 {
                let ipv6_obj = json!({
                    "address": [
                        {
                            "ip": ip.address.to_string(),
                            "prefix-length": ip.prefix_length.value()
                        }
                    ]
                });
                if_obj.insert("ietf-ip:ipv6".to_string(), ipv6_obj);
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
    if !config.static_routes.is_empty() || !config.static_routes_v6.is_empty() {
        let mut sr_container = serde_json::Map::new();

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
            sr_container.insert(
                "ietf-ipv4-unicast-routing:ipv4".to_string(),
                json!({ "route": routes }),
            );
        }

        if !config.static_routes_v6.is_empty() {
            let mut routes_v6 = Vec::new();
            for sr in &config.static_routes_v6 {
                routes_v6.push(json!({
                    "destination-prefix": format!("{}/{}", sr.destination.address, sr.destination.prefix_length),
                    "next-hop": {
                        "next-hop-address": sr.next_hop.to_string()
                    }
                }));
            }
            sr_container.insert(
                "ietf-ipv6-unicast-routing:ipv6".to_string(),
                json!({ "route": routes_v6 }),
            );
        }

        control_plane_protocols.push(json!({
            "type": "ietf-routing:static",
            "name": "static-routing",
            "static-routes": Value::Object(sr_container)
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

    // 3. Map prefix-lists and policies into ietf-routing-policy:routing-policy
    let has_routing_policy = !config.prefix_lists.is_empty()
        || !config.prefix_lists_v6.is_empty()
        || !config.policies.is_empty();

    let routing_policy_json = if has_routing_policy {
        let mut policy_obj = serde_json::Map::new();

        let mut prefix_sets = Vec::new();
        for pl in &config.prefix_lists {
            let mut prefixes = Vec::new();
            for entry in &pl.entries {
                let mut prefix_obj = serde_json::Map::new();
                prefix_obj.insert("ip-prefix".to_string(), json!(entry.prefix.to_string()));
                let lower = entry
                    .range
                    .as_ref()
                    .and_then(|r| r.ge)
                    .map(|ge| ge.value())
                    .unwrap_or_else(|| entry.prefix.prefix_length.value());
                let upper = entry
                    .range
                    .as_ref()
                    .and_then(|r| r.le)
                    .map(|le| le.value())
                    .unwrap_or_else(|| {
                        if let Some(ref r) = entry.range {
                            if r.ge.is_some() {
                                32
                            } else {
                                entry.prefix.prefix_length.value()
                            }
                        } else {
                            entry.prefix.prefix_length.value()
                        }
                    });

                prefix_obj.insert("mask-length-lower".to_string(), json!(lower));
                prefix_obj.insert("mask-length-upper".to_string(), json!(upper));
                prefixes.push(Value::Object(prefix_obj));
            }

            prefix_sets.push(json!({
                "name": pl.name,
                "mode": "ipv4",
                "prefixes": {
                    "prefix-list": prefixes
                }
            }));
        }

        for pl in &config.prefix_lists_v6 {
            let mut prefixes = Vec::new();
            for entry in &pl.entries {
                let mut prefix_obj = serde_json::Map::new();
                prefix_obj.insert("ip-prefix".to_string(), json!(entry.prefix.to_string()));
                let lower = entry
                    .range
                    .as_ref()
                    .and_then(|r| r.ge)
                    .map(|ge| ge.value())
                    .unwrap_or_else(|| entry.prefix.prefix_length.value());
                let upper = entry
                    .range
                    .as_ref()
                    .and_then(|r| r.le)
                    .map(|le| le.value())
                    .unwrap_or_else(|| {
                        if let Some(ref r) = entry.range {
                            if r.ge.is_some() {
                                128
                            } else {
                                entry.prefix.prefix_length.value()
                            }
                        } else {
                            entry.prefix.prefix_length.value()
                        }
                    });

                prefix_obj.insert("mask-length-lower".to_string(), json!(lower));
                prefix_obj.insert("mask-length-upper".to_string(), json!(upper));
                prefixes.push(Value::Object(prefix_obj));
            }

            prefix_sets.push(json!({
                "name": pl.name,
                "mode": "ipv6",
                "prefixes": {
                    "prefix-list": prefixes
                }
            }));
        }

        if !prefix_sets.is_empty() {
            policy_obj.insert(
                "defined-sets".to_string(),
                json!({
                    "prefix-sets": {
                        "prefix-set": prefix_sets
                    }
                }),
            );
        }

        if !config.policies.is_empty() {
            let mut policy_defs = Vec::new();
            for pol in &config.policies {
                let mut statements = Vec::new();
                for stmt in &pol.statements {
                    let mut stmt_obj = serde_json::Map::new();
                    stmt_obj.insert("name".to_string(), json!(stmt.name));

                    // Conditions
                    let mut cond_obj = serde_json::Map::new();
                    if let Some(ref ps_name) = stmt.conditions.match_prefix_set {
                        cond_obj.insert(
                            "match-prefix-set".to_string(),
                            json!({
                                "prefix-set": ps_name
                            }),
                        );
                    } else if let Some(ref ps_name_v6) = stmt.conditions.match_prefix_set_v6 {
                        cond_obj.insert(
                            "match-prefix-set".to_string(),
                            json!({
                                "prefix-set": ps_name_v6
                            }),
                        );
                    }
                    if !cond_obj.is_empty() {
                        stmt_obj.insert("conditions".to_string(), Value::Object(cond_obj));
                    }

                    // Actions
                    let mut act_obj = serde_json::Map::new();
                    let result_str = match stmt.actions.policy_result {
                        ir::PolicyAction::Permit => "accept-route",
                        ir::PolicyAction::Deny => "reject-route",
                    };
                    act_obj.insert("policy-result".to_string(), json!(result_str));

                    if let Some(metric) = stmt.actions.set_metric {
                        act_obj.insert(
                            "set-metric".to_string(),
                            json!({
                                "metric-modification": "set-metric",
                                "metric": metric
                            }),
                        );
                    }
                    stmt_obj.insert("actions".to_string(), Value::Object(act_obj));

                    statements.push(Value::Object(stmt_obj));
                }

                policy_defs.push(json!({
                    "name": pol.name,
                    "statements": {
                        "statement": statements
                    }
                }));
            }

            policy_obj.insert(
                "policy-definitions".to_string(),
                json!({
                    "policy-definition": policy_defs
                }),
            );
        }

        let policy_container = Value::Object(policy_obj);
        root_obj.insert(
            "ietf-routing-policy:routing-policy".to_string(),
            policy_container.clone(),
        );

        let doc = json!({
            "ietf-routing-policy:routing-policy": policy_container
        });
        Some(serde_json::to_string_pretty(&doc).expect("serialize routing policy json"))
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

    if let Some(ref bgp) = config.bgp {
        if let Some(ref evpn) = bgp.evpn {
            uncovered_fields.push(format!(
                "bgp.evpn: BGP EVPN address family (AFI 25 / SAFI 70) with {} neighbor(s) is modeled under ietf-bgp-common-multiprotocol placeholder",
                evpn.neighbors.len()
            ));
        }
    }

    if let Some(ref sr) = config.segment_routing {
        if let Some(ref srv6) = sr.srv6 {
            uncovered_fields.push(format!(
                "segment_routing.srv6: SRv6 locator configuration with {} locator(s) augments RFC 9020 via SRv6 YANG extensions",
                srv6.locators.len()
            ));
        }
    }

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
        routing_policy_json,
        uncovered_fields,
    }
}

use ir::{
    BgpNeighbor, BgpProcess, DeviceConfig, Interface, InterfaceOspfConfig, Ipv4Prefix, OspfAreaId,
    OspfProcess, StaticRoute, UnparsedLine,
};
use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use winnow::ascii::{space0, space1};
use winnow::combinator::seq;
use winnow::error::{ErrMode, FromExternalError};
use winnow::token::{rest, take_while};
use winnow::ModalResult as PResult;
use winnow::Parser;

fn parse_ipv4(input: &mut &str) -> PResult<Ipv4Addr> {
    let checkpoint = *input;
    let ip_str: &str =
        take_while(1.., |c: char| c.is_ascii_digit() || c == '.').parse_next(input)?;
    match ip_str.parse::<Ipv4Addr>() {
        Ok(ip) => Ok(ip),
        Err(e) => {
            *input = checkpoint;
            Err(ErrMode::from_external_error(input, e))
        }
    }
}

fn parse_ipv4_prefix(input: &mut &str) -> PResult<Ipv4Prefix> {
    let checkpoint = *input;
    let prefix_str: &str =
        take_while(1.., |c: char| c.is_ascii_digit() || c == '.' || c == '/').parse_next(input)?;
    match prefix_str.parse::<Ipv4Prefix>() {
        Ok(prefix) => Ok(prefix),
        Err(e) => {
            *input = checkpoint;
            Err(ErrMode::from_external_error(input, e))
        }
    }
}

fn parse_ospf_area(input: &mut &str) -> PResult<OspfAreaId> {
    space0.parse_next(input)?;
    let checkpoint = *input;

    if input.contains('.') {
        if let Ok(ip) = parse_ipv4.parse_next(input) {
            return Ok(OspfAreaId::Ipv4(ip));
        }
    }

    *input = checkpoint;
    let num_str: &str = take_while(1.., |c: char| c.is_ascii_digit()).parse_next(input)?;
    let num = num_str
        .parse::<u32>()
        .map_err(|e| ErrMode::from_external_error(input, e))?;
    Ok(OspfAreaId::Number(num))
}

#[derive(Default, Debug)]
struct RawInterface {
    name: String,
    description: Option<String>,
    disable: bool,
    ipv4: Option<Ipv4Prefix>,
}

#[derive(Default, Debug)]
struct RawOspfInterface {
    cost: Option<u16>,
    passive: Option<bool>,
}

#[derive(Default, Debug)]
struct RawBgpNeighbor {
    peer_as: Option<u32>,
}

/// Parse Junos set-format configuration into vendor-independent DeviceConfig IR.
pub fn parse_junos_config(input: &str, filename: Option<&str>) -> DeviceConfig {
    let mut hostname = String::from("router");
    let mut interfaces_map: BTreeMap<String, RawInterface> = BTreeMap::new();
    let mut ospf_router_id: Option<Ipv4Addr> = None;
    let mut ospf_areas: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>> = BTreeMap::new();
    let mut has_ospf = false;

    let mut bgp_as: Option<u32> = None;
    let mut bgp_router_id: Option<Ipv4Addr> = None;
    let mut bgp_neighbors: BTreeMap<Ipv4Addr, RawBgpNeighbor> = BTreeMap::new();
    let mut bgp_networks: Vec<Ipv4Prefix> = Vec::new();
    let mut has_bgp = false;

    let mut static_routes = Vec::new();
    let mut unparsed_lines = Vec::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();

        // Skip blank lines
        if line.is_empty() {
            continue;
        }

        // Comments in Junos set format or scripts: '#' or '/* ... */'
        if line.starts_with('#') || line.starts_with("/*") {
            continue;
        }

        // Lines must start with "set "
        if !line.starts_with("set ") {
            unparsed_lines.push(UnparsedLine {
                file_path: filename.map(String::from),
                line_number: line_num,
                content: line.to_string(),
                reason: "line does not start with 'set'".to_string(),
            });
            continue;
        }

        let cmd = line["set ".len()..].trim();

        // 1. Hostname: set system host-name <name>
        if cmd.starts_with("system host-name ") {
            let mut s = cmd;
            let parsed: PResult<(&str, &str, &str)> =
                seq!("system host-name", space1, rest).parse_next(&mut s);
            if let Ok((_, _, h)) = parsed {
                hostname = h.trim().trim_matches('"').to_string();
            } else {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "malformed system host-name command".to_string(),
                });
            }
            continue;
        }

        // 2. Interfaces: set interfaces <name> ...
        if let Some(stripped) = cmd.strip_prefix("interfaces ") {
            let rest_cmd = stripped.trim();
            // Rest format can be:
            // <name> description "..." / <desc>
            // <name> disable
            // <name> unit <unit> description "..."
            // <name> unit <unit> disable
            // <name> unit <unit> family inet address <ip/len>
            let tokens: Vec<&str> = rest_cmd.split_whitespace().collect();
            if tokens.is_empty() {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "malformed interfaces statement".to_string(),
                });
                continue;
            }

            let iface_base = tokens[0];

            if tokens.len() >= 3 && tokens[1] == "unit" {
                let unit = tokens[2];
                let full_name = format!("{}.{}", iface_base, unit);
                let entry =
                    interfaces_map
                        .entry(full_name.clone())
                        .or_insert_with(|| RawInterface {
                            name: full_name,
                            description: None,
                            disable: false,
                            ipv4: None,
                        });

                let sub_tokens = &tokens[3..];
                if sub_tokens.is_empty() {
                    // Just defining unit without properties
                    continue;
                }

                if sub_tokens[0] == "disable" {
                    entry.disable = true;
                } else if sub_tokens[0] == "description" {
                    let desc = rest_cmd
                        .splitn(4, |c: char| c.is_whitespace())
                        .nth(3)
                        .unwrap_or("")
                        .trim();
                    let desc = desc.strip_prefix("description").unwrap_or(desc).trim();
                    entry.description = Some(desc.trim_matches('"').to_string());
                } else if sub_tokens.len() >= 4
                    && sub_tokens[0] == "family"
                    && sub_tokens[1] == "inet"
                    && sub_tokens[2] == "address"
                {
                    let addr_str = sub_tokens[3];
                    let mut s = addr_str;
                    match parse_ipv4_prefix.parse_next(&mut s) {
                        Ok(prefix) => {
                            entry.ipv4 = Some(prefix);
                        }
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid IPv4 address/prefix".to_string(),
                            });
                        }
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "unsupported interface unit configuration".to_string(),
                    });
                }
            } else if tokens.len() >= 2 && tokens[1] == "description" {
                let desc = rest_cmd
                    .split_once(|c: char| c.is_whitespace())
                    .map(|x| x.1)
                    .unwrap_or("")
                    .trim();
                let desc = desc.strip_prefix("description").unwrap_or(desc).trim();
                // Base interface description (if unit 0 interface exists, or apply to base interface)
                let entry = interfaces_map
                    .entry(iface_base.to_string())
                    .or_insert_with(|| RawInterface {
                        name: iface_base.to_string(),
                        description: None,
                        disable: false,
                        ipv4: None,
                    });
                entry.description = Some(desc.trim_matches('"').to_string());
            } else if tokens.len() == 2 && tokens[1] == "disable" {
                let entry = interfaces_map
                    .entry(iface_base.to_string())
                    .or_insert_with(|| RawInterface {
                        name: iface_base.to_string(),
                        description: None,
                        disable: false,
                        ipv4: None,
                    });
                entry.disable = true;
            } else {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "unsupported interface configuration".to_string(),
                });
            }
            continue;
        }

        // 3. Routing options:
        // set routing-options router-id <ip>
        // set routing-options autonomous-system <as>
        // set routing-options static route <dest/len> next-hop <ip>
        if let Some(stripped) = cmd.strip_prefix("routing-options ") {
            let rest_ro = stripped.trim();

            if let Some(stripped_id) = rest_ro.strip_prefix("router-id ") {
                let mut s = stripped_id.trim();
                if let Ok(ip) = parse_ipv4.parse_next(&mut s) {
                    ospf_router_id = Some(ip);
                    bgp_router_id = Some(ip);
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed router-id in routing-options".to_string(),
                    });
                }
            } else if let Some(stripped_as) = rest_ro.strip_prefix("autonomous-system ") {
                let s = stripped_as.trim();
                if let Ok(as_num) = s.parse::<u32>() {
                    bgp_as = Some(as_num);
                    has_bgp = true;
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "invalid autonomous-system number".to_string(),
                    });
                }
            } else if let Some(stripped_sr) = rest_ro.strip_prefix("static route ") {
                let rest_sr = stripped_sr.trim();
                let tokens: Vec<&str> = rest_sr.split_whitespace().collect();
                if tokens.len() >= 3 && tokens[1] == "next-hop" {
                    let mut s_dest = tokens[0];
                    let mut s_nh = tokens[2];
                    let dest_res = parse_ipv4_prefix.parse_next(&mut s_dest);
                    let nh_res = parse_ipv4.parse_next(&mut s_nh);
                    match (dest_res, nh_res) {
                        (Ok(dest), Ok(next_hop)) => {
                            static_routes.push(StaticRoute {
                                destination: dest,
                                next_hop,
                            });
                        }
                        _ => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed static route destination or next-hop"
                                    .to_string(),
                            });
                        }
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed static route statement".to_string(),
                    });
                }
            } else {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "unsupported routing-options command".to_string(),
                });
            }
            continue;
        }

        // 4. Protocols OSPF:
        // set protocols ospf area <area> interface <name> ...
        if let Some(stripped) = cmd.strip_prefix("protocols ospf ") {
            has_ospf = true;
            let rest_ospf = stripped.trim();

            if let Some(stripped_area) = rest_ospf.strip_prefix("area ") {
                let rest_area = stripped_area.trim();
                let mut s = rest_area;
                let area_id_res = parse_ospf_area.parse_next(&mut s);
                if let Ok(area_id) = area_id_res {
                    s = s.trim();
                    if let Some(stripped_iface) = s.strip_prefix("interface ") {
                        let rest_iface = stripped_iface.trim();
                        let tokens: Vec<&str> = rest_iface.split_whitespace().collect();
                        if !tokens.is_empty() {
                            let iface_name = tokens[0].to_string();
                            let iface_entry = ospf_areas
                                .entry(area_id)
                                .or_default()
                                .entry(iface_name)
                                .or_default();

                            let sub_tokens = &tokens[1..];
                            if !sub_tokens.is_empty() {
                                if sub_tokens[0] == "passive" {
                                    iface_entry.passive = Some(true);
                                } else if sub_tokens.len() >= 2 && sub_tokens[0] == "metric" {
                                    if let Ok(cost) = sub_tokens[1].parse::<u16>() {
                                        iface_entry.cost = Some(cost);
                                    } else {
                                        unparsed_lines.push(UnparsedLine {
                                            file_path: filename.map(String::from),
                                            line_number: line_num,
                                            content: line.to_string(),
                                            reason: "invalid OSPF metric cost".to_string(),
                                        });
                                    }
                                } else {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: "unsupported OSPF interface option".to_string(),
                                    });
                                }
                            }
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "missing OSPF interface name".to_string(),
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "unsupported OSPF area option".to_string(),
                        });
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed OSPF area identifier".to_string(),
                    });
                }
            } else {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "unsupported OSPF command".to_string(),
                });
            }
            continue;
        }

        // 5. Protocols BGP:
        // set protocols bgp group <group> neighbor <ip> peer-as <as>
        // set protocols bgp group <group> local-as <as>
        // set protocols bgp group <group> export ... (or other group options)
        // set protocols bgp local-as <as>
        // (networks advertised in BGP or policy)
        // set protocols bgp group <group> neighbor <ip> ...
        if let Some(stripped) = cmd.strip_prefix("protocols bgp ") {
            has_bgp = true;
            let rest_bgp = stripped.trim();

            let tokens: Vec<&str> = rest_bgp.split_whitespace().collect();
            // Handle: group <group> neighbor <ip> peer-as <as>
            if tokens.len() >= 5 && tokens[0] == "group" && tokens[2] == "neighbor" {
                let mut ip_s = tokens[3];
                if let Ok(peer_ip) = parse_ipv4.parse_next(&mut ip_s) {
                    let nbr = bgp_neighbors.entry(peer_ip).or_default();
                    if tokens.len() >= 6 && tokens[4] == "peer-as" {
                        if let Ok(pas) = tokens[5].parse::<u32>() {
                            nbr.peer_as = Some(pas);
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid BGP peer-as number".to_string(),
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "unsupported BGP neighbor property".to_string(),
                        });
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed BGP neighbor address".to_string(),
                    });
                }
            } else if tokens.len() >= 3 && tokens[0] == "group" && tokens[2] == "local-as" {
                if let Ok(las) = tokens[3].parse::<u32>() {
                    bgp_as = Some(las);
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "invalid BGP local-as number".to_string(),
                    });
                }
            } else if tokens.len() >= 2 && tokens[0] == "local-as" {
                if let Ok(las) = tokens[1].parse::<u32>() {
                    bgp_as = Some(las);
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "invalid BGP local-as number".to_string(),
                    });
                }
            } else {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "unsupported BGP command".to_string(),
                });
            }
            continue;
        }

        // 6. Policy options for BGP network announcement (e.g. set policy-options policy-statement ... term ... from route-filter <ip/prefix> exact)
        if let Some(stripped) = cmd.strip_prefix("policy-options ") {
            let rest_po = stripped.trim();
            let tokens: Vec<&str> = rest_po.split_whitespace().collect();
            // Look for route-filter <prefix>
            let mut matched = false;
            for (t_idx, token) in tokens.iter().enumerate() {
                if *token == "route-filter" && t_idx + 1 < tokens.len() {
                    let mut s = tokens[t_idx + 1];
                    if let Ok(prefix) = parse_ipv4_prefix.parse_next(&mut s) {
                        bgp_networks.push(prefix);
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "unsupported policy-options command".to_string(),
                });
            }
            continue;
        }

        // Catch-all unparsed set command
        unparsed_lines.push(UnparsedLine {
            file_path: filename.map(String::from),
            line_number: line_num,
            content: line.to_string(),
            reason: "unsupported or unrecognized Junos command".to_string(),
        });
    }

    // Build finalized interfaces list
    // Check if we need to merge base interface description/disable into sub-interfaces (like ge-0/0/0 -> ge-0/0/0.0)
    // If an interface name doesn't have a unit (no dot) but sub-interfaces with same prefix exist, merge descriptions if needed.
    // However, in Junos, interfaces configured with `set interfaces ge-0/0/0 unit 0 ...` should have name "ge-0/0/0.0" or "ge-0/0/0".
    // If unit is present, full name is `interface.unit`.
    // Let's propagate base interface descriptions / disable flags if any.
    let base_properties: BTreeMap<String, (Option<String>, bool)> = interfaces_map
        .iter()
        .filter(|(k, _)| !k.contains('.'))
        .map(|(k, v)| (k.clone(), (v.description.clone(), v.disable)))
        .collect();

    for (k, v) in &mut interfaces_map {
        if k.contains('.') {
            if let Some(base) = k.split('.').next() {
                if let Some((base_desc, base_disable)) = base_properties.get(base) {
                    if v.description.is_none() && base_desc.is_some() {
                        v.description = base_desc.clone();
                    }
                    if *base_disable {
                        v.disable = true;
                    }
                }
            }
        }
    }

    let all_keys: Vec<String> = interfaces_map.keys().cloned().collect();

    // Filter out interface entries that are purely base containers with child units
    let mut interfaces = Vec::new();
    for (name, raw_iface) in interfaces_map {
        // If an interface has child units defined, e.g. ge-0/0/0.0 exists, and ge-0/0/0 has no IPv4,
        // we shouldn't emit a separate ge-0/0/0.
        let has_child_unit = !name.contains('.')
            && raw_iface.ipv4.is_none()
            && all_keys
                .iter()
                .any(|k| k.starts_with(&format!("{}.", name)));

        if has_child_unit {
            continue;
        }

        // Attach OSPF configuration if this interface participates in any OSPF area
        let mut iface_ospf = None;
        for (area_id, area_ifaces) in &ospf_areas {
            // Check exact name match or match without unit (e.g. area interface ge-0/0/0.0 or ge-0/0/0)
            let matched_cfg = area_ifaces.get(&name).or_else(|| {
                if let Some(base) = name.split('.').next() {
                    area_ifaces.get(base)
                } else {
                    None
                }
            });

            if let Some(ospf_opt) = matched_cfg {
                iface_ospf = Some(InterfaceOspfConfig {
                    area: area_id.clone(),
                    cost: ospf_opt.cost,
                    passive: ospf_opt.passive,
                });
                break;
            }
        }

        interfaces.push(Interface {
            name: raw_iface.name,
            description: raw_iface.description,
            enabled: !raw_iface.disable,
            ipv4: raw_iface.ipv4,
            ospf: iface_ospf,
        });
    }

    // Sort interfaces deterministically by name
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    // Construct OspfProcess if OSPF was configured
    let ospf = if has_ospf || !ospf_areas.is_empty() {
        Some(OspfProcess {
            process_id: 1, // Junos default single instance maps to process 1
            router_id: ospf_router_id,
        })
    } else {
        None
    };

    // Construct BgpProcess if BGP was configured
    let bgp = if has_bgp || !bgp_neighbors.is_empty() {
        let mut neighbors = Vec::new();
        for (peer_addr, nbr_raw) in bgp_neighbors {
            if let Some(peer_as) = nbr_raw.peer_as {
                neighbors.push(BgpNeighbor {
                    remote_address: peer_addr,
                    remote_as: peer_as,
                });
            }
        }
        neighbors.sort_by_key(|n| n.remote_address);
        bgp_networks.sort();
        bgp_networks.dedup();

        Some(BgpProcess {
            as_number: bgp_as.unwrap_or(0),
            router_id: bgp_router_id,
            neighbors,
            networks: bgp_networks,
        })
    } else {
        None
    };

    DeviceConfig {
        hostname,
        interfaces,
        ospf,
        bgp,
        static_routes,
        unparsed_lines,
    }
}

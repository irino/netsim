#[derive(Debug, Default, Clone)]
struct JunosTerm {
    name: String,
    match_prefix_set: Option<String>,
    match_prefix_set_v6: Option<String>,
    match_community: Option<String>,
    action: Option<PolicyAction>,
    set_metric: Option<u32>,
    set_local_preference: Option<u32>,
    set_community: Option<String>,
}

use ir::{
    BgpEvpnConfig, BgpEvpnNeighbor, BgpNeighbor, BgpNeighborV6, BgpPeerGroup, BgpProcess,
    CommunityList, CommunityListEntry, DeviceConfig, Interface, InterfaceIsisConfig,
    InterfaceOspfConfig, InterfaceVxlanConfig, Ipv4Prefix, Ipv6Prefix, Ipv6StaticRoute, IsisLevel,
    IsisProcess, Ospf6Process, OspfAreaId, OspfProcess, PolicyAction, PolicyActions,
    PolicyConditions, PolicyDefinition, PolicyStatement, PrefixList, PrefixListEntry,
    PrefixListEntryV6, PrefixListV6, SegmentRoutingConfig, SrV6Locator, SrV6UsidConfig,
    StaticRoute, UnparsedLine, VniVlanBinding,
};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use winnow::ascii::{space0, space1};
use winnow::combinator::seq;
use winnow::error::{ErrMode, FromExternalError};
use winnow::token::{rest, take_while};
use winnow::ModalResult as PResult;
use winnow::Parser;

fn parse_ipv6(input: &mut &str) -> PResult<Ipv6Addr> {
    let checkpoint = *input;
    let ip_str: &str =
        take_while(1.., |c: char| c.is_ascii_hexdigit() || c == ':').parse_next(input)?;
    match ip_str.parse::<Ipv6Addr>() {
        Ok(ip) => Ok(ip),
        Err(e) => {
            *input = checkpoint;
            Err(ErrMode::from_external_error(input, e))
        }
    }
}

fn parse_ipv6_prefix(input: &mut &str) -> PResult<Ipv6Prefix> {
    let checkpoint = *input;
    let prefix_str: &str = take_while(1.., |c: char| c.is_ascii_hexdigit() || c == ':' || c == '/')
        .parse_next(input)?;
    match prefix_str.parse::<Ipv6Prefix>() {
        Ok(p) => Ok(p),
        Err(e) => {
            *input = checkpoint;
            Err(ErrMode::from_external_error(input, e))
        }
    }
}

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
    ipv6: Option<Ipv6Prefix>,
    vxlan: Option<InterfaceVxlanConfig>,
}

#[derive(Default, Debug)]
struct RawOspfInterface {
    cost: Option<u16>,
    passive: Option<bool>,
}

#[derive(Default, Debug)]
struct RawBgpNeighbor {
    peer_as: Option<u32>,
    peer_group: Option<String>,
    route_reflector_client: Option<bool>,
    cluster_id: Option<Ipv4Addr>,
    send_community: Option<String>,
    bfd: Option<bool>,
}

/// Parse Junos set-format configuration into vendor-independent DeviceConfig IR.
pub fn parse_junos_config(input: &str, filename: Option<&str>) -> DeviceConfig {
    let mut hostname = String::from("router");
    let mut interfaces_map: BTreeMap<String, RawInterface> = BTreeMap::new();
    let mut ospf_router_id: Option<Ipv4Addr> = None;
    let mut ospf_areas: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>> = BTreeMap::new();
    let mut ospfv3_areas: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>> =
        BTreeMap::new();
    let mut has_ospf = false;
    let mut has_ospfv3 = false;

    let mut bgp_as: Option<u32> = None;
    let mut bgp_router_id: Option<Ipv4Addr> = None;
    let mut bgp_cluster_id: Option<Ipv4Addr> = None;
    let mut bgp_peer_groups: BTreeMap<String, BgpPeerGroup> = BTreeMap::new();
    let mut bgp_neighbors: BTreeMap<Ipv4Addr, RawBgpNeighbor> = BTreeMap::new();
    let mut bgp_neighbors_v6: BTreeMap<Ipv6Addr, RawBgpNeighbor> = BTreeMap::new();
    let mut isis_process: Option<IsisProcess> = None;
    let mut isis_interfaces: BTreeMap<String, InterfaceIsisConfig> = BTreeMap::new();
    let mut community_lists_map: BTreeMap<String, Vec<CommunityListEntry>> = BTreeMap::new();
    let mut bgp_networks: Vec<Ipv4Prefix> = Vec::new();
    let mut bgp_networks_v6: Vec<Ipv6Prefix> = Vec::new();
    let mut bgp_evpn_config: Option<BgpEvpnConfig> = None;
    let mut segment_routing: Option<SegmentRoutingConfig> = None;
    let mut vtep_source_interface: Option<String> = None;
    let mut vni_bindings: Vec<VniVlanBinding> = Vec::new();
    let mut has_bgp = false;

    let mut static_routes = Vec::new();
    let mut static_routes_v6 = Vec::new();
    let mut prefix_lists_map: BTreeMap<String, Vec<PrefixListEntry>> = BTreeMap::new();
    let mut prefix_list_last_seq: BTreeMap<String, u32> = BTreeMap::new();
    let mut prefix_lists_v6_map: BTreeMap<String, Vec<PrefixListEntryV6>> = BTreeMap::new();
    let mut prefix_list_v6_last_seq: BTreeMap<String, u32> = BTreeMap::new();
    let mut policy_statements: BTreeMap<String, BTreeMap<String, JunosTerm>> = BTreeMap::new();
    let mut policy_term_orders: BTreeMap<String, Vec<String>> = BTreeMap::new();
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

        // Switch options & VLANs for EVPN / VXLAN
        if let Some(stripped) = cmd.strip_prefix("switch-options ") {
            let tokens: Vec<&str> = stripped.split_whitespace().collect();
            if tokens.len() >= 2 && tokens[0] == "vtep-source-interface" {
                vtep_source_interface = Some(tokens[1].to_string());
            }
            continue;
        }

        if let Some(stripped) = cmd.strip_prefix("vlans ") {
            let tokens: Vec<&str> = stripped.split_whitespace().collect();
            if let Some(vni_idx) = tokens.iter().position(|&t| t == "vni") {
                if vni_idx + 1 < tokens.len() {
                    if let Ok(vni) = tokens[vni_idx + 1].parse::<u32>() {
                        let mut vlan = None;
                        if let Some(vlan_idx) = tokens.iter().position(|&t| t == "vlan-id") {
                            if vlan_idx + 1 < tokens.len() {
                                vlan = tokens[vlan_idx + 1].parse::<u16>().ok();
                            }
                        }
                        vni_bindings.push(VniVlanBinding { vni, vlan });
                    }
                }
            }
            continue;
        }

        if let Some(stripped) = cmd.strip_prefix("protocols evpn ") {
            let tokens: Vec<&str> = stripped.split_whitespace().collect();
            let cfg = bgp_evpn_config.get_or_insert_with(Default::default);
            cfg.enabled = true;
            if tokens.len() >= 2 && tokens[0] == "encapsulation" && tokens[1] == "vxlan" {
                cfg.advertise_all_vni = Some(true);
            } else if tokens.len() >= 2
                && tokens[0] == "default-gateway"
                && tokens[1] == "advertise"
            {
                cfg.advertise_default_gw = Some(true);
            }
            continue;
        }

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
                            ipv6: None,
                            vxlan: None,
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
                } else if sub_tokens.len() >= 4
                    && sub_tokens[0] == "family"
                    && sub_tokens[1] == "iso"
                    && sub_tokens[2] == "address"
                {
                    let net = sub_tokens[3].trim_matches('"').to_string();
                    let proc = isis_process.get_or_insert_with(IsisProcess::default);
                    proc.net = Some(net);
                } else if sub_tokens.len() >= 4
                    && sub_tokens[0] == "family"
                    && sub_tokens[1] == "inet6"
                    && sub_tokens[2] == "address"
                {
                    let addr_str = sub_tokens[3];
                    let mut s = addr_str;
                    match parse_ipv6_prefix.parse_next(&mut s) {
                        Ok(prefix) => {
                            entry.ipv6 = Some(prefix);
                        }
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid IPv6 address/prefix".to_string(),
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
                        ipv6: None,
                        vxlan: None,
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
                        ipv6: None,
                        vxlan: None,
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

            if let Some(stripped_sr) = rest_ro.strip_prefix("segment-routing srv6 ") {
                let sr_cfg = segment_routing.get_or_insert_with(Default::default);
                let srv6_cfg = sr_cfg.srv6.get_or_insert_with(Default::default);
                srv6_cfg.enabled = true;
                let tokens: Vec<&str> = stripped_sr.split_whitespace().collect();
                if tokens.len() >= 3
                    && tokens[0] == "encapsulation"
                    && tokens[1] == "source-address"
                {
                    if let Ok(ip6) = tokens[2].parse::<Ipv6Addr>() {
                        srv6_cfg.encapsulation_source_address = Some(ip6);
                    }
                } else if tokens.len() >= 4 && tokens[0] == "locator" && tokens[2] == "prefix" {
                    let loc_name = tokens[1].to_string();
                    if let Ok(pfx) = tokens[3].parse::<Ipv6Prefix>() {
                        if let Some(loc) = srv6_cfg.locators.iter_mut().find(|l| l.name == loc_name)
                        {
                            loc.prefix = pfx;
                        } else {
                            srv6_cfg.locators.push(SrV6Locator {
                                name: loc_name,
                                prefix: pfx,
                                micro_segment: None,
                            });
                        }
                    }
                } else if tokens.len() >= 3
                    && tokens[0] == "locator"
                    && tokens[2].starts_with("micro-segment")
                {
                    let loc_name = tokens[1].to_string();
                    if let Some(loc) = srv6_cfg.locators.iter_mut().find(|l| l.name == loc_name) {
                        loc.micro_segment = Some(SrV6UsidConfig {
                            enabled: true,
                            block_length: Some(16),
                            node_length: Some(16),
                        });
                    } else {
                        srv6_cfg.locators.push(SrV6Locator {
                            name: loc_name,
                            prefix: "::/0".parse().unwrap(),
                            micro_segment: Some(SrV6UsidConfig {
                                enabled: true,
                                block_length: Some(16),
                                node_length: Some(16),
                            }),
                        });
                    }
                }
                continue;
            }

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
            } else if let Some(stripped_rib) = rest_ro.strip_prefix("rib inet6.0 static route ") {
                let rest_sr = stripped_rib.trim();
                let tokens: Vec<&str> = rest_sr.split_whitespace().collect();
                if tokens.len() >= 3 && tokens[1] == "next-hop" {
                    let mut s_dest = tokens[0];
                    let mut s_nh = tokens[2];
                    let dest_res = parse_ipv6_prefix.parse_next(&mut s_dest);
                    let nh_res = parse_ipv6.parse_next(&mut s_nh);
                    match (dest_res, nh_res) {
                        (Ok(dest), Ok(next_hop)) => {
                            static_routes_v6.push(Ipv6StaticRoute {
                                destination: dest,
                                next_hop,
                            });
                        }
                        _ => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed IPv6 static route destination or next-hop"
                                    .to_string(),
                            });
                        }
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed IPv6 static route statement".to_string(),
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

        // 4. Protocols OSPF3 (OSPFv3):
        // set protocols ospf3 area <area> interface <name> ...
        if let Some(stripped) = cmd.strip_prefix("protocols ospf3 ") {
            has_ospfv3 = true;
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
                            let iface_entry = ospfv3_areas
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
                                            reason: "invalid OSPFv3 metric cost".to_string(),
                                        });
                                    }
                                } else {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: "unsupported OSPFv3 interface option".to_string(),
                                    });
                                }
                            }
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed OSPFv3 interface statement".to_string(),
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "unsupported OSPFv3 area option".to_string(),
                        });
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed OSPFv3 area ID".to_string(),
                    });
                }
            } else {
                unparsed_lines.push(UnparsedLine {
                    file_path: filename.map(String::from),
                    line_number: line_num,
                    content: line.to_string(),
                    reason: "unsupported OSPFv3 command".to_string(),
                });
            }
            continue;
        }

        // Protocols isis: set protocols isis ...
        if let Some(stripped) = cmd.strip_prefix("protocols isis ") {
            let proc = isis_process.get_or_insert_with(IsisProcess::default);
            let rest_isis = stripped.trim();
            let tokens: Vec<&str> = rest_isis.split_whitespace().collect();
            if tokens.len() >= 2 && tokens[0] == "interface" {
                let if_name = tokens[1].to_string();
                let if_cfg = isis_interfaces.entry(if_name).or_default();
                if tokens.len() >= 3 && tokens[2] == "passive" {
                    if_cfg.passive = Some(true);
                } else if tokens.len() >= 5
                    && tokens[2] == "level"
                    && (tokens[3] == "1" || tokens[3] == "2")
                    && tokens[4] == "metric"
                {
                    if let Some(m_str) = tokens.get(5) {
                        if let Ok(m) = m_str.parse::<u32>() {
                            if_cfg.metric = Some(m);
                            if tokens[3] == "1" {
                                if_cfg.level = Some(IsisLevel::Level1);
                            } else {
                                if_cfg.level = Some(IsisLevel::Level2);
                            }
                        }
                    }
                } else if tokens.len() >= 4 && tokens[2] == "metric" {
                    if let Ok(m) = tokens[3].parse::<u32>() {
                        if_cfg.metric = Some(m);
                    }
                } else if tokens.len() >= 3
                    && (tokens[2] == "bfd-liveness-detection" || tokens[2] == "bfd")
                {
                    if_cfg.bfd = Some(true);
                }
            } else if tokens.len() >= 3 && tokens[0] == "level" && tokens[2] == "disable" {
                if tokens[1] == "1" {
                    proc.level = Some(IsisLevel::Level2);
                } else if tokens[1] == "2" {
                    proc.level = Some(IsisLevel::Level1);
                }
            }
            continue;
        }

        // Community set: set policy-options community <name> members <community>
        if let Some(stripped) = cmd.strip_prefix("policy-options community ") {
            let tokens: Vec<&str> = stripped.split_whitespace().collect();
            if tokens.len() >= 3 && tokens[1] == "members" {
                let name = tokens[0].to_string();
                let val = tokens[2].trim_matches('"').to_string();
                community_lists_map
                    .entry(name)
                    .or_default()
                    .push(CommunityListEntry {
                        sequence: None,
                        action: PolicyAction::Permit,
                        communities: vec![val],
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
            if tokens.len() >= 2 && tokens[0] == "group" {
                let gname = tokens[1].to_string();
                let pg = bgp_peer_groups
                    .entry(gname.clone())
                    .or_insert_with(|| BgpPeerGroup {
                        name: gname.clone(),
                        ..Default::default()
                    });
                if tokens.len() >= 4 && tokens[2] == "peer-as" {
                    if let Ok(pas) = tokens[3].parse::<u32>() {
                        pg.remote_as = Some(pas);
                    }
                    continue;
                } else if tokens.len() >= 3 && tokens[2] == "cluster" {
                    if tokens.len() >= 4 {
                        if let Ok(cid) = tokens[3].parse::<Ipv4Addr>() {
                            pg.cluster_id = Some(cid);
                            if bgp_cluster_id.is_none() {
                                bgp_cluster_id = Some(cid);
                            }
                        }
                    }
                    pg.route_reflector_client = Some(true);
                    continue;
                } else if tokens.len() >= 3
                    && (tokens[2] == "bfd-liveness-detection" || tokens[2] == "bfd")
                {
                    pg.bfd = Some(true);
                    continue;
                } else if tokens.len() >= 4 && tokens[2] == "neighbor" {
                    let nbr_str = tokens[3];
                    if let Ok(ip) = nbr_str.parse::<Ipv4Addr>() {
                        let nbr = bgp_neighbors.entry(ip).or_default();
                        nbr.peer_group = Some(gname);
                        if tokens.len() >= 6 && tokens[4] == "peer-as" {
                            if let Ok(pas) = tokens[5].parse::<u32>() {
                                nbr.peer_as = Some(pas);
                            }
                        }
                        continue;
                    } else if let Ok(ip6) = nbr_str.parse::<Ipv6Addr>() {
                        let nbr = bgp_neighbors_v6.entry(ip6).or_default();
                        nbr.peer_group = Some(gname);
                        if tokens.len() >= 6 && tokens[4] == "peer-as" {
                            if let Ok(pas) = tokens[5].parse::<u32>() {
                                nbr.peer_as = Some(pas);
                            }
                        }
                        continue;
                    }
                }
            }
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
                    let mut ip6_s = tokens[3];
                    if let Ok(peer_ip6) = parse_ipv6.parse_next(&mut ip6_s) {
                        let nbr = bgp_neighbors_v6.entry(peer_ip6).or_default();
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

        // 6. Policy options for prefix-list and BGP network announcement and policy-statements:
        // set policy-options prefix-list <name> <prefix>
        // set policy-options policy-statement ... term ... from route-filter <ip/prefix> exact
        // set policy-options policy-statement ... term ... from prefix-list <pl>
        // set policy-options policy-statement ... term ... then accept/reject
        if let Some(stripped) = cmd.strip_prefix("policy-options ") {
            let rest_po = stripped.trim();
            if let Some(stripped_pl) = rest_po.strip_prefix("prefix-list ") {
                let pl_tokens: Vec<&str> = stripped_pl.split_whitespace().collect();
                if pl_tokens.len() >= 2 {
                    let pl_name = pl_tokens[0].to_string();
                    let mut s = pl_tokens[1];
                    if let Ok(prefix) = parse_ipv4_prefix.parse_next(&mut s) {
                        let next_seq = match prefix_list_last_seq.get(&pl_name) {
                            Some(&last) => last + 5,
                            None => 5,
                        };
                        prefix_list_last_seq.insert(pl_name.clone(), next_seq);
                        prefix_lists_map
                            .entry(pl_name)
                            .or_default()
                            .push(PrefixListEntry {
                                sequence: next_seq,
                                action: PolicyAction::Permit,
                                prefix,
                                range: None,
                            });
                    } else {
                        let mut s_v6 = pl_tokens[1];
                        if let Ok(prefix_v6) = parse_ipv6_prefix.parse_next(&mut s_v6) {
                            let next_seq = match prefix_list_v6_last_seq.get(&pl_name) {
                                Some(&last) => last + 5,
                                None => 5,
                            };
                            prefix_list_v6_last_seq.insert(pl_name.clone(), next_seq);
                            prefix_lists_v6_map.entry(pl_name).or_default().push(
                                PrefixListEntryV6 {
                                    sequence: next_seq,
                                    action: PolicyAction::Permit,
                                    prefix: prefix_v6,
                                    range: None,
                                },
                            );
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid prefix-list IP prefix".to_string(),
                            });
                        }
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed prefix-list statement".to_string(),
                    });
                }
            } else if let Some(stripped_ps) = rest_po.strip_prefix("policy-statement ") {
                let tokens: Vec<&str> = stripped_ps.split_whitespace().collect();
                if tokens.len() >= 2 {
                    let ps_name = tokens[0].to_string();
                    let mut term_name = "default".to_string();
                    let mut cursor = 1;

                    if tokens[cursor] == "term" && cursor + 1 < tokens.len() {
                        term_name = tokens[cursor + 1].to_string();
                        cursor += 2;
                    }

                    let term_order = policy_term_orders.entry(ps_name.clone()).or_default();
                    if !term_order.contains(&term_name) {
                        term_order.push(term_name.clone());
                    }

                    let term_map = policy_statements.entry(ps_name.clone()).or_default();
                    let term = term_map
                        .entry(term_name.clone())
                        .or_insert_with(|| JunosTerm {
                            name: term_name.clone(),
                            ..Default::default()
                        });

                    // Parse remaining tokens: from / then
                    let mut matched = false;
                    while cursor < tokens.len() {
                        if tokens[cursor] == "from" && cursor + 1 < tokens.len() {
                            cursor += 1;
                            if tokens[cursor] == "route-filter" && cursor + 1 < tokens.len() {
                                let mut s = tokens[cursor + 1];
                                if let Ok(prefix) = parse_ipv4_prefix.parse_next(&mut s) {
                                    bgp_networks.push(prefix);
                                    matched = true;
                                }
                                cursor += 1;
                            } else if tokens[cursor] == "prefix-list" && cursor + 1 < tokens.len() {
                                term.match_prefix_set = Some(tokens[cursor + 1].to_string());
                                cursor += 1;
                                matched = true;
                            } else if tokens[cursor] == "community" && cursor + 1 < tokens.len() {
                                term.match_community =
                                    Some(tokens[cursor + 1].trim_matches('"').to_string());
                                cursor += 1;
                                matched = true;
                            }
                        } else if tokens[cursor] == "then" && cursor + 1 < tokens.len() {
                            cursor += 1;
                            if tokens[cursor] == "community"
                                && cursor + 2 < tokens.len()
                                && (tokens[cursor + 1] == "set" || tokens[cursor + 1] == "add")
                            {
                                term.set_community =
                                    Some(tokens[cursor + 2].trim_matches('"').to_string());
                                cursor += 2;
                                matched = true;
                            } else if tokens[cursor] == "accept" {
                                term.action = Some(PolicyAction::Permit);
                                matched = true;
                            } else if tokens[cursor] == "reject" {
                                term.action = Some(PolicyAction::Deny);
                                matched = true;
                            } else if tokens[cursor] == "metric" && cursor + 1 < tokens.len() {
                                if let Ok(m) = tokens[cursor + 1].parse::<u32>() {
                                    term.set_metric = Some(m);
                                }
                                cursor += 1;
                                matched = true;
                            } else if tokens[cursor] == "local-preference"
                                && cursor + 1 < tokens.len()
                            {
                                if let Ok(lp) = tokens[cursor + 1].parse::<u32>() {
                                    term.set_local_preference = Some(lp);
                                }
                                cursor += 1;
                                matched = true;
                            }
                        }
                        cursor += 1;
                    }

                    if !matched {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "unsupported policy-statement clause".to_string(),
                        });
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "malformed policy-statement command".to_string(),
                    });
                }
            } else {
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

    for isis_if_name in isis_interfaces.keys() {
        interfaces_map
            .entry(isis_if_name.clone())
            .or_insert_with(|| RawInterface {
                name: isis_if_name.clone(),
                description: None,
                disable: false,
                ipv4: None,
                ipv6: None,
                vxlan: None,
            });
    }
    let all_keys: Vec<String> = interfaces_map.keys().cloned().collect();

    // Filter out interface entries that are purely base containers with child units
    let mut interfaces = Vec::new();
    for (name, raw_iface) in interfaces_map {
        // If an interface has child units defined, e.g. ge-0/0/0.0 exists, and ge-0/0/0 has no IPv4,
        // we shouldn't emit a separate ge-0/0/0.
        let has_child_unit = !name.contains('.')
            && raw_iface.ipv4.is_none()
            && raw_iface.ipv6.is_none()
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
                    bfd: None,
                });
                break;
            }
        }

        // Attach OSPFv3 configuration if this interface participates in any OSPFv3 area
        let mut iface_ospfv3 = None;
        for (area_id, area_ifaces) in &ospfv3_areas {
            let matched_cfg = area_ifaces.get(&name).or_else(|| {
                if let Some(base) = name.split('.').next() {
                    area_ifaces.get(base)
                } else {
                    None
                }
            });

            if let Some(ospf_opt) = matched_cfg {
                iface_ospfv3 = Some(InterfaceOspfConfig {
                    area: area_id.clone(),
                    cost: ospf_opt.cost,
                    passive: ospf_opt.passive,
                    bfd: None,
                });
                break;
            }
        }

        let iface_isis = isis_interfaces.get(&name).cloned().or_else(|| {
            if let Some(base) = name.split('.').next() {
                isis_interfaces.get(base).cloned()
            } else {
                None
            }
        });

        interfaces.push(Interface {
            name: raw_iface.name,
            description: raw_iface.description,
            enabled: !raw_iface.disable,
            ipv4: raw_iface.ipv4,
            ipv6: raw_iface.ipv6,
            ospf: iface_ospf,
            ospfv3: iface_ospfv3,
            isis: iface_isis,
            vxlan: raw_iface.vxlan,
        });
    }

    if vtep_source_interface.is_some() || !vni_bindings.is_empty() {
        let vtep_iface = Interface {
            name: "vtep0".to_string(),
            description: Some("Junos EVPN/VXLAN VTEP".to_string()),
            enabled: true,
            ipv4: None,
            ipv6: None,
            ospf: None,
            ospfv3: None,
            isis: None,
            vxlan: Some(InterfaceVxlanConfig {
                source_interface: vtep_source_interface,
                local_ip: None,
                udp_port: Some(4789),
                vni_bindings,
                default_vni: None,
            }),
        };
        interfaces.push(vtep_iface);
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

    let ospfv3 = if has_ospfv3 || !ospfv3_areas.is_empty() {
        Some(Ospf6Process {
            process_id: 1,
            router_id: ospf_router_id,
        })
    } else {
        None
    };

    // Construct BgpProcess if BGP was configured
    let bgp = if has_bgp
        || !bgp_neighbors.is_empty()
        || !bgp_neighbors_v6.is_empty()
        || !bgp_peer_groups.is_empty()
    {
        let mut neighbors = Vec::new();
        for (peer_addr, nbr_raw) in bgp_neighbors {
            let mut peer_as = nbr_raw.peer_as;
            let mut rr_client = nbr_raw.route_reflector_client;
            let mut cid = nbr_raw.cluster_id;
            let mut sc = nbr_raw.send_community;
            let mut bfd = nbr_raw.bfd;

            if let Some(ref pg_name) = nbr_raw.peer_group {
                if let Some(pg) = bgp_peer_groups.get(pg_name) {
                    if peer_as.is_none() {
                        peer_as = pg.remote_as;
                    }
                    if rr_client.is_none() {
                        rr_client = pg.route_reflector_client;
                    }
                    if cid.is_none() {
                        cid = pg.cluster_id;
                    }
                    if sc.is_none() {
                        sc = pg.send_community.clone();
                    }
                    if bfd.is_none() {
                        bfd = pg.bfd;
                    }
                }
            }

            if let Some(pas) = peer_as {
                neighbors.push(BgpNeighbor {
                    remote_address: peer_addr,
                    remote_as: pas,
                    peer_group: nbr_raw.peer_group,
                    route_reflector_client: rr_client,
                    cluster_id: cid,
                    send_community: sc,
                    bfd,
                });
            }
        }
        neighbors.sort_by_key(|n| n.remote_address);

        let mut neighbors_v6 = Vec::new();
        for (peer_addr, nbr_raw) in bgp_neighbors_v6 {
            let mut peer_as = nbr_raw.peer_as;
            let mut rr_client = nbr_raw.route_reflector_client;
            let mut cid = nbr_raw.cluster_id;
            let mut sc = nbr_raw.send_community;
            let mut bfd = nbr_raw.bfd;

            if let Some(ref pg_name) = nbr_raw.peer_group {
                if let Some(pg) = bgp_peer_groups.get(pg_name) {
                    if peer_as.is_none() {
                        peer_as = pg.remote_as;
                    }
                    if rr_client.is_none() {
                        rr_client = pg.route_reflector_client;
                    }
                    if cid.is_none() {
                        cid = pg.cluster_id;
                    }
                    if sc.is_none() {
                        sc = pg.send_community.clone();
                    }
                    if bfd.is_none() {
                        bfd = pg.bfd;
                    }
                }
            }

            if let Some(pas) = peer_as {
                neighbors_v6.push(BgpNeighborV6 {
                    remote_address: peer_addr,
                    remote_as: pas,
                    peer_group: nbr_raw.peer_group,
                    route_reflector_client: rr_client,
                    cluster_id: cid,
                    send_community: sc,
                    bfd,
                });
            }
        }
        neighbors_v6.sort_by_key(|n| n.remote_address);

        bgp_networks.sort();
        bgp_networks.dedup();
        bgp_networks_v6.sort();
        bgp_networks_v6.dedup();

        let mut evpn = bgp_evpn_config;
        if let Some(ref mut evpn_cfg) = evpn {
            for nbr in &neighbors {
                if !evpn_cfg
                    .neighbors
                    .iter()
                    .any(|n| n.peer == nbr.remote_address.to_string())
                {
                    evpn_cfg.neighbors.push(BgpEvpnNeighbor {
                        peer: nbr.remote_address.to_string(),
                        route_reflector_client: nbr.route_reflector_client,
                        send_community: nbr.send_community.clone(),
                    });
                }
            }
            for nbr_v6 in &neighbors_v6 {
                if !evpn_cfg
                    .neighbors
                    .iter()
                    .any(|n| n.peer == nbr_v6.remote_address.to_string())
                {
                    evpn_cfg.neighbors.push(BgpEvpnNeighbor {
                        peer: nbr_v6.remote_address.to_string(),
                        route_reflector_client: nbr_v6.route_reflector_client,
                        send_community: nbr_v6.send_community.clone(),
                    });
                }
            }
        }

        Some(BgpProcess {
            as_number: bgp_as.unwrap_or(0),
            router_id: bgp_router_id,
            cluster_id: bgp_cluster_id,
            peer_groups: bgp_peer_groups.into_values().collect(),
            neighbors,
            networks: bgp_networks,
            neighbors_v6,
            networks_v6: bgp_networks_v6,
            evpn,
        })
    } else {
        None
    };

    // Finalize prefix lists
    let mut prefix_lists = Vec::new();
    for (name, mut entries) in prefix_lists_map {
        entries.sort_by_key(|e| e.sequence);
        prefix_lists.push(PrefixList { name, entries });
    }

    let mut prefix_lists_v6 = Vec::new();
    for (name, mut entries) in prefix_lists_v6_map {
        entries.sort_by_key(|e: &PrefixListEntryV6| e.sequence);
        prefix_lists_v6.push(PrefixListV6 { name, entries });
    }

    // Finalize policies
    let mut policies = Vec::new();
    for (ps_name, term_order) in policy_term_orders {
        if let Some(terms) = policy_statements.get(&ps_name) {
            let mut statements = Vec::new();
            for t_name in term_order {
                if let Some(term) = terms.get(&t_name) {
                    statements.push(PolicyStatement {
                        name: term.name.clone(),
                        conditions: PolicyConditions {
                            match_prefix_set: term.match_prefix_set.clone(),
                            match_prefix_set_v6: term.match_prefix_set_v6.clone(),
                            match_community: term.match_community.clone(),
                        },
                        actions: PolicyActions {
                            policy_result: term.action.unwrap_or(PolicyAction::Permit),
                            set_metric: term.set_metric,
                            set_local_preference: term.set_local_preference,
                            set_community: term.set_community.clone(),
                        },
                    });
                }
            }
            policies.push(PolicyDefinition {
                name: ps_name,
                statements,
            });
        }
    }

    let mut community_lists = Vec::new();
    for (name, entries) in community_lists_map {
        community_lists.push(CommunityList { name, entries });
    }

    DeviceConfig {
        hostname,
        interfaces,
        ospf,
        ospfv3,
        isis: isis_process,
        bgp,
        segment_routing,
        static_routes,
        static_routes_v6,
        prefix_lists,
        prefix_lists_v6,
        community_lists,
        policies,
        unparsed_lines,
    }
}

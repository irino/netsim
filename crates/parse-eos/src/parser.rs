use ir::{
    BgpNeighbor, BgpNeighborV6, BgpProcess, DeviceConfig, Interface, InterfaceOspfConfig,
    Ipv4Prefix, Ipv6Prefix, Ipv6StaticRoute, Ospf6Process, OspfAreaId, OspfProcess, PolicyAction,
    PrefixLength, PrefixList, PrefixListEntry, PrefixRange, StaticRoute, UnparsedLine,
};
use std::collections::{BTreeMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use winnow::ascii::{space0, space1};
use winnow::combinator::seq;
use winnow::error::{ErrMode, FromExternalError};
use winnow::token::{rest, take_while};
use winnow::ModalResult as PResult;
use winnow::Parser;

#[derive(Debug, Clone)]
struct ParsedOspfNetwork {
    network: Ipv4Addr,
    wildcard: Ipv4Addr,
    area: OspfAreaId,
}

#[derive(Debug)]
enum Context {
    Global,
    Interface(Interface),
    Ospf(u16, Option<Ipv4Addr>, Vec<ParsedOspfNetwork>),
    Ospf6(u16, Option<Ipv4Addr>),
    Bgp {
        as_num: u32,
        router_id: Option<Ipv4Addr>,
        neighbors: Vec<BgpNeighbor>,
        networks: Vec<Ipv4Prefix>,
        neighbors_v6: Vec<BgpNeighborV6>,
        networks_v6: Vec<Ipv6Prefix>,
        in_af_ipv6: bool,
    },
}

/// Convert a subnet mask to a prefix length (0..=32).
fn mask_to_prefix_length(mask: Ipv4Addr) -> Result<PrefixLength, String> {
    let bits = u32::from(mask);
    let leading_ones = bits.leading_ones() as u8;
    if leading_ones == 32 {
        PrefixLength::new(32).map_err(|e| e.to_string())
    } else {
        let expected = if leading_ones == 0 {
            0
        } else {
            !((1u64 << (32 - leading_ones)) - 1) as u32
        };
        if bits == expected {
            PrefixLength::new(leading_ones).map_err(|e| e.to_string())
        } else {
            Err(format!("non-contiguous subnet mask: {}", mask))
        }
    }
}

/// Check if an interface IPv4 address matches an OSPF network statement with a wildcard mask.
fn matches_wildcard(ip: Ipv4Addr, network: Ipv4Addr, wildcard: Ipv4Addr) -> bool {
    let ip_u32 = u32::from(ip);
    let net_u32 = u32::from(network);
    let wild_u32 = u32::from(wildcard);

    (ip_u32 & !wild_u32) == (net_u32 & !wild_u32)
}

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

/// Parse Arista EOS configuration into vendor-independent DeviceConfig IR.
#[allow(clippy::type_complexity)]
pub fn parse_eos_config(input: &str, filename: Option<&str>) -> DeviceConfig {
    let mut hostname = String::from("router");
    let mut interfaces = Vec::new();
    let mut ospf_process: Option<OspfProcess> = None;
    let mut ospf_networks: Vec<ParsedOspfNetwork> = Vec::new();
    let mut ospfv3_process: Option<Ospf6Process> = None;
    let mut bgp_process: Option<BgpProcess> = None;
    let mut static_routes = Vec::new();
    let mut static_routes_v6 = Vec::new();
    let mut prefix_lists_map: BTreeMap<String, Vec<PrefixListEntry>> = BTreeMap::new();
    let mut prefix_list_seq_sets: BTreeMap<String, HashSet<u32>> = BTreeMap::new();
    let mut prefix_list_last_seq: BTreeMap<String, u32> = BTreeMap::new();
    let mut unparsed_lines = Vec::new();

    let mut ctx = Context::Global;

    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();

        // Skip blank lines
        if line.is_empty() {
            continue;
        }

        // Comment lines (!) separate blocks in Arista EOS / Cisco-like configs
        if line.starts_with('!') {
            match ctx {
                Context::Interface(iface) => interfaces.push(iface),
                Context::Ospf(proc_id, router_id, mut nets) => {
                    ospf_process = Some(OspfProcess {
                        process_id: proc_id,
                        router_id,
                    });
                    ospf_networks.append(&mut nets);
                }
                Context::Ospf6(proc_id, router_id) => {
                    ospfv3_process = Some(Ospf6Process {
                        process_id: proc_id,
                        router_id,
                    });
                }
                Context::Bgp {
                    as_num,
                    router_id,
                    neighbors,
                    networks,
                    neighbors_v6,
                    networks_v6,
                    ..
                } => {
                    bgp_process = Some(BgpProcess {
                        as_number: as_num,
                        router_id,
                        neighbors,
                        networks,
                        neighbors_v6,
                        networks_v6,
                    });
                }
                Context::Global => {}
            }
            ctx = Context::Global;
            continue;
        }

        // Explicit block exits
        if line == "exit" || line == "end" {
            if let Context::Bgp {
                ref mut in_af_ipv6, ..
            } = ctx
            {
                if *in_af_ipv6 {
                    *in_af_ipv6 = false;
                    continue;
                }
            }
            match ctx {
                Context::Interface(iface) => interfaces.push(iface),
                Context::Ospf(proc_id, router_id, mut nets) => {
                    ospf_process = Some(OspfProcess {
                        process_id: proc_id,
                        router_id,
                    });
                    ospf_networks.append(&mut nets);
                }
                Context::Ospf6(proc_id, router_id) => {
                    ospfv3_process = Some(Ospf6Process {
                        process_id: proc_id,
                        router_id,
                    });
                }
                Context::Bgp {
                    as_num,
                    router_id,
                    neighbors,
                    networks,
                    neighbors_v6,
                    networks_v6,
                    ..
                } => {
                    bgp_process = Some(BgpProcess {
                        as_number: as_num,
                        router_id,
                        neighbors,
                        networks,
                        neighbors_v6,
                        networks_v6,
                    });
                }
                Context::Global => {}
            }
            ctx = Context::Global;
            continue;
        }

        // Check if an unindented top-level line occurred while in a sub-block
        let is_indented = raw_line.starts_with(' ') || raw_line.starts_with('\t');
        if !is_indented && !matches!(ctx, Context::Global) {
            let is_subcommand = match &ctx {
                Context::Interface(_) => {
                    line.starts_with("description ")
                        || line.starts_with("ip address ")
                        || line.starts_with("ipv6 address ")
                        || line.starts_with("ip ospf ")
                        || line.starts_with("ipv6 ospf ")
                        || line == "shutdown"
                        || line == "no shutdown"
                }
                Context::Ospf(..) => {
                    line.starts_with("router-id ")
                        || line.starts_with("network ")
                        || line.starts_with("passive-interface ")
                }
                Context::Ospf6(..) => line.starts_with("router-id "),
                Context::Bgp { .. } => {
                    line.starts_with("router-id ")
                        || line.starts_with("neighbor ")
                        || line.starts_with("network ")
                        || line.starts_with("address-family ")
                        || line == "exit-address-family"
                }
                Context::Global => unreachable!(),
            };

            if !is_subcommand {
                // Terminate active block and transition to global context
                match ctx {
                    Context::Interface(iface) => interfaces.push(iface),
                    Context::Ospf(proc_id, router_id, mut nets) => {
                        ospf_process = Some(OspfProcess {
                            process_id: proc_id,
                            router_id,
                        });
                        ospf_networks.append(&mut nets);
                    }
                    Context::Ospf6(proc_id, router_id) => {
                        ospfv3_process = Some(Ospf6Process {
                            process_id: proc_id,
                            router_id,
                        });
                    }
                    Context::Bgp {
                        as_num,
                        router_id,
                        neighbors,
                        networks,
                        neighbors_v6,
                        networks_v6,
                        ..
                    } => {
                        bgp_process = Some(BgpProcess {
                            as_number: as_num,
                            router_id,
                            neighbors,
                            networks,
                            neighbors_v6,
                            networks_v6,
                        });
                    }
                    Context::Global => {}
                }
                ctx = Context::Global;
            }
        }

        match &mut ctx {
            Context::Global => {
                if line.starts_with("hostname ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("hostname", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, h)) = parsed {
                        hostname = h.trim().to_string();
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed hostname statement".to_string(),
                        });
                    }
                } else if line.starts_with("interface ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("interface", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, iface_name)) = parsed {
                        ctx = Context::Interface(Interface {
                            name: iface_name.trim().to_string(),
                            description: None,
                            enabled: true,
                            ipv4: None,
                            ipv6: None,
                            ospf: None,
                            ospfv3: None,
                        });
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed interface statement".to_string(),
                        });
                    }
                } else if line.starts_with("router ospf ") {
                    let mut s = line;
                    let parsed_pid: PResult<(&str, &str, &str)> =
                        seq!("router ospf", space1, rest).parse_next(&mut s);

                    if let Ok((_, _, id_str)) = parsed_pid {
                        if let Ok(proc_id) = id_str.trim().parse::<u16>() {
                            ctx = Context::Ospf(proc_id, None, Vec::new());
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid OSPF process ID".to_string(),
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router ospf command".to_string(),
                        });
                    }
                } else if line.starts_with("router ospf3 ") {
                    let mut s = line;
                    let parsed_pid: PResult<(&str, &str, &str)> =
                        seq!("router ospf3", space1, rest).parse_next(&mut s);

                    if let Ok((_, _, id_str)) = parsed_pid {
                        if let Ok(proc_id) = id_str.trim().parse::<u16>() {
                            ctx = Context::Ospf6(proc_id, None);
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid OSPFv3 process ID".to_string(),
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router ospf3 command".to_string(),
                        });
                    }
                } else if line.starts_with("router bgp ") {
                    let mut s = line;
                    let parsed_as: PResult<(&str, &str, &str)> =
                        seq!("router bgp", space1, rest).parse_next(&mut s);

                    if let Ok((_, _, as_str)) = parsed_as {
                        if let Ok(as_num) = as_str.trim().parse::<u32>() {
                            ctx = Context::Bgp {
                                as_num,
                                router_id: None,
                                neighbors: Vec::new(),
                                networks: Vec::new(),
                                neighbors_v6: Vec::new(),
                                networks_v6: Vec::new(),
                                in_af_ipv6: false,
                            };
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid BGP AS number".to_string(),
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router bgp command".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("ipv6 route ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() >= 2 {
                        let mut s_dest = tokens[0];
                        let mut s_nh = tokens[1];
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
                                    reason: "malformed ipv6 route command".to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed ipv6 route command".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("ip route ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() == 2 {
                        // ip route <prefix/len> <next-hop>
                        let mut s_dest = tokens[0];
                        let mut s_nh = tokens[1];
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
                                    reason: "malformed ip route CIDR command".to_string(),
                                });
                            }
                        }
                    } else if tokens.len() >= 3 {
                        // ip route <dest> <netmask> <next-hop>
                        let mut s_dest = tokens[0];
                        let mut s_mask = tokens[1];
                        let mut s_nh = tokens[2];
                        let dest_res = parse_ipv4.parse_next(&mut s_dest);
                        let mask_res = parse_ipv4.parse_next(&mut s_mask);
                        let nh_res = parse_ipv4.parse_next(&mut s_nh);
                        match (dest_res, mask_res, nh_res) {
                            (Ok(dest_ip), Ok(mask), Ok(next_hop)) => {
                                match mask_to_prefix_length(mask) {
                                    Ok(prefix_len) => {
                                        static_routes.push(StaticRoute {
                                            destination: Ipv4Prefix::new(dest_ip, prefix_len),
                                            next_hop,
                                        });
                                    }
                                    Err(reason) => {
                                        unparsed_lines.push(UnparsedLine {
                                            file_path: filename.map(String::from),
                                            line_number: line_num,
                                            content: line.to_string(),
                                            reason: format!(
                                                "invalid static route mask: {}",
                                                reason
                                            ),
                                        });
                                    }
                                }
                            }
                            _ => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed ip route command".to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed ip route command".to_string(),
                        });
                    }
                } else if line.starts_with("ipv6 prefix-list ") {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "IPv6 prefix-lists are outside current IPv4 milestone scope"
                            .to_string(),
                    });
                } else if let Some(pl_tail) = line.strip_prefix("ip prefix-list ") {
                    let tokens: Vec<&str> = pl_tail.split_whitespace().collect();
                    if tokens.is_empty() {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed ip prefix-list command".to_string(),
                        });
                    } else {
                        let name = tokens[0].to_string();
                        prefix_lists_map.entry(name.clone()).or_default();

                        let rest_tokens = &tokens[1..];
                        if rest_tokens.is_empty() {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed ip prefix-list command".to_string(),
                            });
                        } else if rest_tokens[0] == "description" {
                            // ip prefix-list <NAME> description <TEXT>
                        } else {
                            let mut cursor = 0;
                            let mut explicit_seq = None;

                            if rest_tokens.len() >= 2 && rest_tokens[cursor] == "seq" {
                                if let Ok(s_num) = rest_tokens[cursor + 1].parse::<u32>() {
                                    explicit_seq = Some(s_num);
                                    cursor += 2;
                                } else {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: "invalid sequence number in prefix-list"
                                            .to_string(),
                                    });
                                    continue;
                                }
                            }

                            if cursor >= rest_tokens.len() {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "missing action in prefix-list command".to_string(),
                                });
                                continue;
                            }

                            let action = match rest_tokens[cursor] {
                                "permit" => Some(PolicyAction::Permit),
                                "deny" => Some(PolicyAction::Deny),
                                _ => None,
                            };

                            let action = match action {
                                Some(a) => a,
                                None => {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: format!(
                                            "unknown prefix-list action '{}'",
                                            rest_tokens[cursor]
                                        ),
                                    });
                                    continue;
                                }
                            };
                            cursor += 1;

                            if cursor >= rest_tokens.len() {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "missing prefix in prefix-list command".to_string(),
                                });
                                continue;
                            }

                            let prefix_res = Ipv4Prefix::from_str(rest_tokens[cursor]);
                            let prefix = match prefix_res {
                                Ok(p) => p,
                                Err(_) => {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: format!("invalid prefix '{}'", rest_tokens[cursor]),
                                    });
                                    continue;
                                }
                            };
                            cursor += 1;

                            let mut ge_val = None;
                            let mut le_val = None;
                            let mut parse_range_failed = false;

                            while cursor < rest_tokens.len() {
                                if rest_tokens[cursor] == "ge" && cursor + 1 < rest_tokens.len() {
                                    if let Ok(v) = rest_tokens[cursor + 1].parse::<u8>() {
                                        if let Ok(p_len) = PrefixLength::new(v) {
                                            ge_val = Some(p_len);
                                            cursor += 2;
                                            continue;
                                        }
                                    }
                                    parse_range_failed = true;
                                    break;
                                } else if rest_tokens[cursor] == "le"
                                    && cursor + 1 < rest_tokens.len()
                                {
                                    if let Ok(v) = rest_tokens[cursor + 1].parse::<u8>() {
                                        if let Ok(p_len) = PrefixLength::new(v) {
                                            le_val = Some(p_len);
                                            cursor += 2;
                                            continue;
                                        }
                                    }
                                    parse_range_failed = true;
                                    break;
                                } else {
                                    parse_range_failed = true;
                                    break;
                                }
                            }

                            if parse_range_failed {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed prefix-list range parameters".to_string(),
                                });
                                continue;
                            }

                            let range = if ge_val.is_some() || le_val.is_some() {
                                let r = PrefixRange {
                                    ge: ge_val,
                                    le: le_val,
                                };
                                if r.validate(prefix.prefix_length).is_err() {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: "invalid prefix-list length range: len <= ge <= le"
                                            .to_string(),
                                    });
                                    continue;
                                }
                                Some(r)
                            } else {
                                None
                            };

                            let seq_num = match explicit_seq {
                                Some(n) => n,
                                None => {
                                    let next_seq = match prefix_list_last_seq.get(&name) {
                                        Some(&last) => last + 10,
                                        None => 10,
                                    };
                                    next_seq
                                }
                            };

                            let seq_set = prefix_list_seq_sets.entry(name.clone()).or_default();
                            if seq_set.contains(&seq_num) {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: format!(
                                        "duplicate sequence number {} in prefix-list {}",
                                        seq_num, name
                                    ),
                                });
                                continue;
                            }
                            seq_set.insert(seq_num);
                            prefix_list_last_seq.insert(name.clone(), seq_num);

                            prefix_lists_map
                                .entry(name)
                                .or_default()
                                .push(PrefixListEntry {
                                    sequence: seq_num,
                                    action,
                                    prefix,
                                    range,
                                });
                        }
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "unsupported or unrecognized global command".to_string(),
                    });
                }
            }
            Context::Interface(ref mut iface) => {
                if line.starts_with("description ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("description", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, desc)) = parsed {
                        iface.description = Some(desc.trim().to_string());
                    }
                } else if let Some(stripped) = line.strip_prefix("ip address ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() == 1 {
                        // Arista EOS CIDR format: ip address 10.0.0.1/24
                        let mut s = tokens[0];
                        match parse_ipv4_prefix.parse_next(&mut s) {
                            Ok(prefix) => {
                                iface.ipv4 = Some(prefix);
                            }
                            Err(_) => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "invalid IPv4 CIDR address format".to_string(),
                                });
                            }
                        }
                    } else if tokens.len() >= 2 {
                        // Traditional format: ip address 10.0.0.1 255.255.255.0
                        let mut s_ip = tokens[0];
                        let mut s_mask = tokens[1];
                        let ip_res = parse_ipv4.parse_next(&mut s_ip);
                        let mask_res = parse_ipv4.parse_next(&mut s_mask);
                        match (ip_res, mask_res) {
                            (Ok(ip), Ok(mask)) => match mask_to_prefix_length(mask) {
                                Ok(prefix_len) => {
                                    iface.ipv4 = Some(Ipv4Prefix::new(ip, prefix_len));
                                }
                                Err(reason) => {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: format!("invalid IP address mask: {}", reason),
                                    });
                                }
                            },
                            _ => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed ip address command".to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed ip address command".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("ip ospf area ") {
                    // EOS interface level OSPF: ip ospf area <area>
                    let mut s = stripped.trim();
                    match parse_ospf_area.parse_next(&mut s) {
                        Ok(area) => {
                            let curr_cost = iface.ospf.as_ref().and_then(|o| o.cost);
                            let curr_passive = iface.ospf.as_ref().and_then(|o| o.passive);
                            iface.ospf = Some(InterfaceOspfConfig {
                                area,
                                cost: curr_cost,
                                passive: curr_passive,
                            });
                        }
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed ip ospf area command".to_string(),
                            });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("ipv6 address ") {
                    let mut s = stripped.trim();
                    match parse_ipv6_prefix.parse_next(&mut s) {
                        Ok(prefix) => {
                            iface.ipv6 = Some(prefix);
                        }
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid IPv6 address format".to_string(),
                            });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("ipv6 ospf ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() >= 3 && tokens[1] == "area" {
                        let mut s_area = tokens[2];
                        if let Ok(area) = parse_ospf_area.parse_next(&mut s_area) {
                            iface.ospfv3 = Some(InterfaceOspfConfig {
                                area,
                                cost: None,
                                passive: None,
                            });
                        }
                    } else if tokens.len() >= 2 && tokens[0] == "area" {
                        let mut s_area = tokens[1];
                        if let Ok(area) = parse_ospf_area.parse_next(&mut s_area) {
                            iface.ospfv3 = Some(InterfaceOspfConfig {
                                area,
                                cost: None,
                                passive: None,
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed ipv6 ospf command".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("ip ospf cost ") {
                    let cost_str = stripped.trim();
                    if let Ok(cost) = cost_str.parse::<u16>() {
                        if let Some(ref mut o) = iface.ospf {
                            o.cost = Some(cost);
                        } else {
                            iface.ospf = Some(InterfaceOspfConfig {
                                area: OspfAreaId::Number(0),
                                cost: Some(cost),
                                passive: None,
                            });
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "invalid ip ospf cost".to_string(),
                        });
                    }
                } else if line == "shutdown" {
                    iface.enabled = false;
                } else if line == "no shutdown" {
                    iface.enabled = true;
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "unsupported interface command".to_string(),
                    });
                }
            }
            Context::Ospf6(_proc_id, ref mut router_id) => {
                if line.starts_with("router-id ") {
                    let mut s = line;
                    let parsed_rid: PResult<(&str, &str, Ipv4Addr)> =
                        seq!("router-id", space1, parse_ipv4).parse_next(&mut s);

                    if let Ok((_, _, rid)) = parsed_rid {
                        *router_id = Some(rid);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router-id command".to_string(),
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
            }
            Context::Ospf(_proc_id, ref mut router_id, ref mut nets) => {
                if line.starts_with("router-id ") {
                    let mut s = line;
                    let parsed_rid: PResult<(&str, &str, Ipv4Addr)> =
                        seq!("router-id", space1, parse_ipv4).parse_next(&mut s);

                    if let Ok((_, _, rid)) = parsed_rid {
                        *router_id = Some(rid);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router-id command".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("network ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() >= 3 && tokens[1] == "area" {
                        // network <prefix/len> area <area>
                        let mut s_prefix = tokens[0];
                        let mut s_area = tokens[2];
                        let prefix_res = parse_ipv4_prefix.parse_next(&mut s_prefix);
                        let area_res = parse_ospf_area.parse_next(&mut s_area);
                        match (prefix_res, area_res) {
                            (Ok(prefix), Ok(area)) => {
                                let len = prefix.prefix_length.value();
                                let wildcard_u32 = if len >= 32 {
                                    0
                                } else {
                                    (1u64 << (32 - len)) - 1
                                } as u32;
                                let wildcard = Ipv4Addr::from(wildcard_u32);
                                nets.push(ParsedOspfNetwork {
                                    network: prefix.network_address(),
                                    wildcard,
                                    area,
                                });
                            }
                            _ => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed OSPF network CIDR command".to_string(),
                                });
                            }
                        }
                    } else if tokens.len() >= 4 && tokens[2] == "area" {
                        // network <ip> <wildcard> area <area>
                        let mut s_net = tokens[0];
                        let mut s_wild = tokens[1];
                        let mut s_area = tokens[3];
                        let net_res = parse_ipv4.parse_next(&mut s_net);
                        let wild_res = parse_ipv4.parse_next(&mut s_wild);
                        let area_res = parse_ospf_area.parse_next(&mut s_area);
                        match (net_res, wild_res, area_res) {
                            (Ok(net), Ok(wildcard), Ok(area)) => {
                                nets.push(ParsedOspfNetwork {
                                    network: net,
                                    wildcard,
                                    area,
                                });
                            }
                            _ => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed OSPF network command".to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed OSPF network command".to_string(),
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
            }
            Context::Bgp {
                as_num: _,
                ref mut router_id,
                ref mut neighbors,
                ref mut networks,
                ref mut neighbors_v6,
                ref mut networks_v6,
                ref mut in_af_ipv6,
            } => {
                if line.starts_with("address-family ipv6") {
                    *in_af_ipv6 = true;
                } else if line == "exit-address-family" || line == "exit" {
                    *in_af_ipv6 = false;
                } else if line.starts_with("router-id ") {
                    let mut s = line;
                    let parsed_rid: PResult<(&str, &str, Ipv4Addr)> =
                        seq!("router-id", space1, parse_ipv4).parse_next(&mut s);

                    if let Ok((_, _, rid)) = parsed_rid {
                        *router_id = Some(rid);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed bgp router-id command".to_string(),
                        });
                    }
                } else if line.starts_with("neighbor ") && line.contains(" remote-as ") {
                    let mut s = line;
                    let parsed_nbr: PResult<(&str, &str, Ipv4Addr, &str, &str, &str, &str)> = seq!(
                        "neighbor",
                        space1,
                        parse_ipv4,
                        space1,
                        "remote-as",
                        space1,
                        rest
                    )
                    .parse_next(&mut s);

                    if let Ok((_, _, nbr_ip, _, _, _, rem_as_str)) = parsed_nbr {
                        if let Ok(rem_as) = rem_as_str.trim().parse::<u32>() {
                            neighbors.push(BgpNeighbor {
                                remote_address: nbr_ip,
                                remote_as: rem_as,
                            });
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid neighbor remote-as".to_string(),
                            });
                        }
                    } else {
                        let mut s_v6 = line;
                        let parsed_nbr_v6: PResult<(&str, &str, Ipv6Addr, &str, &str, &str, &str)> =
                            seq!(
                                "neighbor",
                                space1,
                                parse_ipv6,
                                space1,
                                "remote-as",
                                space1,
                                rest
                            )
                            .parse_next(&mut s_v6);

                        if let Ok((_, _, nbr_ip_v6, _, _, _, rem_as_str)) = parsed_nbr_v6 {
                            if let Ok(rem_as) = rem_as_str.trim().parse::<u32>() {
                                neighbors_v6.push(BgpNeighborV6 {
                                    remote_address: nbr_ip_v6,
                                    remote_as: rem_as,
                                });
                            } else {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "invalid neighbor remote-as".to_string(),
                                });
                            }
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed neighbor remote-as command".to_string(),
                            });
                        }
                    }
                } else if *in_af_ipv6 && line.starts_with("network ") {
                    let mut s = line;
                    let parsed_net_v6: PResult<(&str, &str, Ipv6Prefix)> =
                        seq!("network", space1, parse_ipv6_prefix).parse_next(&mut s);

                    if let Ok((_, _, prefix)) = parsed_net_v6 {
                        networks_v6.push(prefix);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed BGP IPv6 network command".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("network ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() == 1 {
                        // network <prefix/len>
                        let mut s = tokens[0];
                        match parse_ipv4_prefix.parse_next(&mut s) {
                            Ok(prefix) => {
                                networks.push(prefix);
                            }
                            Err(_) => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "invalid BGP network prefix".to_string(),
                                });
                            }
                        }
                    } else if tokens.len() >= 3 && tokens[1] == "mask" {
                        // network <ip> mask <netmask>
                        let mut s_ip = tokens[0];
                        let mut s_mask = tokens[2];
                        let ip_res = parse_ipv4.parse_next(&mut s_ip);
                        let mask_res = parse_ipv4.parse_next(&mut s_mask);
                        match (ip_res, mask_res) {
                            (Ok(ip), Ok(mask)) => match mask_to_prefix_length(mask) {
                                Ok(prefix_len) => {
                                    networks.push(Ipv4Prefix::new(ip, prefix_len));
                                }
                                Err(reason) => {
                                    unparsed_lines.push(UnparsedLine {
                                        file_path: filename.map(String::from),
                                        line_number: line_num,
                                        content: line.to_string(),
                                        reason: format!("invalid BGP network mask: {}", reason),
                                    });
                                }
                            },
                            _ => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed BGP network command".to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed BGP network command".to_string(),
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
            }
        }
    }

    // Flush any pending context at EOF
    match ctx {
        Context::Interface(iface) => interfaces.push(iface),
        Context::Ospf(proc_id, router_id, mut nets) => {
            ospf_process = Some(OspfProcess {
                process_id: proc_id,
                router_id,
            });
            ospf_networks.append(&mut nets);
        }
        Context::Ospf6(proc_id, router_id) => {
            ospfv3_process = Some(Ospf6Process {
                process_id: proc_id,
                router_id,
            });
        }
        Context::Bgp {
            as_num,
            router_id,
            neighbors,
            networks,
            neighbors_v6,
            networks_v6,
            ..
        } => {
            bgp_process = Some(BgpProcess {
                as_number: as_num,
                router_id,
                neighbors,
                networks,
                neighbors_v6,
                networks_v6,
            });
        }
        Context::Global => {}
    }

    // Resolve OSPF network statements to interface OSPF areas (if not explicitly configured on interface)
    for iface in &mut interfaces {
        if iface.ospf.is_none() {
            if let Some(ref ipv4) = iface.ipv4 {
                for net_stmt in &ospf_networks {
                    if matches_wildcard(ipv4.address, net_stmt.network, net_stmt.wildcard) {
                        iface.ospf = Some(InterfaceOspfConfig {
                            area: net_stmt.area.clone(),
                            cost: None,
                            passive: None,
                        });
                        break;
                    }
                }
            }
        }
    }

    // Finalize prefix-lists: sort entries by sequence number
    let mut prefix_lists = Vec::new();
    for (name, mut entries) in prefix_lists_map {
        entries.sort_by_key(|e| e.sequence);
        prefix_lists.push(PrefixList { name, entries });
    }

    DeviceConfig {
        hostname,
        interfaces,
        ospf: ospf_process,
        ospfv3: ospfv3_process,
        bgp: bgp_process,
        static_routes,
        static_routes_v6,
        prefix_lists,
        unparsed_lines,
    }
}

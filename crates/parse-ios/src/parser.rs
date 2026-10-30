use ir::{
    DeviceConfig, Interface, InterfaceOspfConfig, Ipv4Prefix, OspfAreaId, OspfProcess,
    PrefixLength, UnparsedLine,
};
use std::net::Ipv4Addr;
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

    // Bits where wildcard bit is 0 must match exactly between IP and network statement
    (ip_u32 & !wild_u32) == (net_u32 & !wild_u32)
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

#[allow(clippy::type_complexity)]
pub fn parse_ios_config(input: &str, filename: Option<&str>) -> DeviceConfig {
    let mut hostname = String::from("router");
    let mut interfaces = Vec::new();
    let mut ospf_process: Option<OspfProcess> = None;
    let mut ospf_networks: Vec<ParsedOspfNetwork> = Vec::new();
    let mut unparsed_lines = Vec::new();

    let mut ctx = Context::Global;

    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();

        // Skip blank lines
        if line.is_empty() {
            continue;
        }

        // Comment lines (!) separate blocks in Cisco IOS configs
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
                Context::Global => {}
            }
            ctx = Context::Global;
            continue;
        }

        // Explicit block exits
        if line == "exit" || line == "end" {
            match ctx {
                Context::Interface(iface) => interfaces.push(iface),
                Context::Ospf(proc_id, router_id, mut nets) => {
                    ospf_process = Some(OspfProcess {
                        process_id: proc_id,
                        router_id,
                    });
                    ospf_networks.append(&mut nets);
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
                        || line == "shutdown"
                        || line == "no shutdown"
                }
                Context::Ospf(..) => line.starts_with("router-id ") || line.starts_with("network "),
                Context::Global => unreachable!(),
            };

            if !is_subcommand {
                // Terminate the active block and transition to global context
                match ctx {
                    Context::Interface(iface) => interfaces.push(iface),
                    Context::Ospf(proc_id, router_id, mut nets) => {
                        ospf_process = Some(OspfProcess {
                            process_id: proc_id,
                            router_id,
                        });
                        ospf_networks.append(&mut nets);
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
                            ospf: None,
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
                if line.starts_with("interface ") {
                    // Implicit transition to new interface block
                    let old_iface = std::mem::replace(
                        iface,
                        Interface {
                            name: String::new(),
                            description: None,
                            enabled: true,
                            ipv4: None,
                            ospf: None,
                        },
                    );
                    interfaces.push(old_iface);

                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("interface", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, iface_name)) = parsed {
                        iface.name = iface_name.trim().to_string();
                    } else {
                        ctx = Context::Global;
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed interface statement".to_string(),
                        });
                    }
                } else if line.starts_with("router ospf ") {
                    // Implicit transition to router ospf block
                    let old_iface = std::mem::replace(
                        iface,
                        Interface {
                            name: String::new(),
                            description: None,
                            enabled: true,
                            ipv4: None,
                            ospf: None,
                        },
                    );
                    interfaces.push(old_iface);

                    let mut s = line;
                    let parsed_pid: PResult<(&str, &str, &str)> =
                        seq!("router ospf", space1, rest).parse_next(&mut s);

                    if let Ok((_, _, id_str)) = parsed_pid {
                        if let Ok(proc_id) = id_str.trim().parse::<u16>() {
                            ctx = Context::Ospf(proc_id, None, Vec::new());
                        } else {
                            ctx = Context::Global;
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid OSPF process ID".to_string(),
                            });
                        }
                    } else {
                        ctx = Context::Global;
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router ospf command".to_string(),
                        });
                    }
                } else if line.starts_with("description ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("description", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, desc)) = parsed {
                        iface.description = Some(desc.trim().to_string());
                    }
                } else if line.starts_with("ip address ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, Ipv4Addr, &str, Ipv4Addr)> =
                        seq!("ip address", space1, parse_ipv4, space1, parse_ipv4)
                            .parse_next(&mut s);

                    match parsed {
                        Ok((_, _, ip, _, mask)) => match mask_to_prefix_length(mask) {
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
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed ip address command".to_string(),
                            });
                        }
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
            Context::Ospf(proc_id, ref mut router_id, ref mut nets) => {
                if line.starts_with("interface ") {
                    ospf_process = Some(OspfProcess {
                        process_id: *proc_id,
                        router_id: *router_id,
                    });

                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("interface", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, iface_name)) = parsed {
                        ctx = Context::Interface(Interface {
                            name: iface_name.trim().to_string(),
                            description: None,
                            enabled: true,
                            ipv4: None,
                            ospf: None,
                        });
                    } else {
                        ctx = Context::Global;
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed interface statement".to_string(),
                        });
                    }
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
                            reason: "malformed router-id command".to_string(),
                        });
                    }
                } else if line.starts_with("network ") {
                    let mut s = line;
                    let parsed_net: PResult<(
                        &str,
                        &str,
                        Ipv4Addr,
                        &str,
                        Ipv4Addr,
                        &str,
                        &str,
                        OspfAreaId,
                    )> = seq!(
                        "network",
                        space1,
                        parse_ipv4,
                        space1,
                        parse_ipv4,
                        space1,
                        "area",
                        parse_ospf_area
                    )
                    .parse_next(&mut s);

                    if let Ok((_, _, net, _, wild, _, _, area)) = parsed_net {
                        nets.push(ParsedOspfNetwork {
                            network: net,
                            wildcard: wild,
                            area,
                        });
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
        Context::Global => {}
    }

    // Resolve OSPF network statements to interface OSPF areas
    for iface in &mut interfaces {
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

    DeviceConfig {
        hostname,
        interfaces,
        ospf: ospf_process,
        unparsed_lines,
    }
}

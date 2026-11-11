use ir::{
    BgpEvpnConfig, BgpEvpnNeighbor, BgpNeighbor, BgpNeighborV6, BgpPeerGroup, BgpProcess,
    CommunityList, CommunityListEntry, DeviceConfig, Interface, InterfaceIsisConfig,
    InterfaceOspfConfig, InterfaceVxlanConfig, Ipv4Prefix, Ipv6Prefix, Ipv6PrefixLength,
    Ipv6StaticRoute, IsisLevel, IsisProcess, Ospf6Process, OspfAreaId, OspfProcess, PolicyAction,
    PolicyActions, PolicyConditions, PolicyDefinition, PolicyStatement, PrefixLength, PrefixList,
    PrefixListEntry, PrefixListEntryV6, PrefixListV6, PrefixRange, PrefixRangeV6,
    SegmentRoutingConfig, SrV6Config, SrV6Locator, SrV6UsidConfig, StaticRoute, UnparsedLine,
    VniVlanBinding,
};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use winnow::ascii::{space0, space1};
use winnow::combinator::seq;
use winnow::error::{ErrMode, FromExternalError};
use winnow::token::{rest, take_while};
use winnow::ModalResult as PResult;
use winnow::Parser;

#[derive(Default, Debug, Clone)]
struct RawBgpPeer {
    remote_as: u32,
    peer_group: Option<String>,
    route_reflector_client: Option<bool>,
    cluster_id: Option<Ipv4Addr>,
    send_community: Option<String>,
    bfd: Option<bool>,
}

#[derive(Default, Debug, Clone)]
struct RawIsisInterface {
    metric: Option<u32>,
    level: Option<IsisLevel>,
    passive: Option<bool>,
    bfd: Option<bool>,
}

#[derive(Default, Debug)]
struct RawOspfInterface {
    cost: Option<u16>,
    passive: Option<bool>,
}

#[derive(Debug, Default)]
struct ParsedRpl {
    name: String,
    statements: Vec<PolicyStatement>,
    current_match_ps: Option<String>,
    current_match_ps_v6: Option<String>,
    current_match_community: Option<String>,
    current_med: Option<u32>,
    current_lp: Option<u32>,
    current_set_community: Option<String>,
}

enum Context {
    Global,
    Interface(Interface),
    Ospf {
        proc_id: u16,
        router_id: Option<Ipv4Addr>,
        current_area: Option<OspfAreaId>,
        current_interface: Option<String>,
        areas: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>>,
    },
    OspfV3 {
        proc_id: u16,
        router_id: Option<Ipv4Addr>,
        current_area: Option<OspfAreaId>,
        current_interface: Option<String>,
        areas: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>>,
    },
    Isis {
        net: Option<String>,
        level: Option<IsisLevel>,
        current_interface: Option<String>,
        interfaces: BTreeMap<String, RawIsisInterface>,
    },
    Bgp {
        as_num: u32,
        router_id: Option<Ipv4Addr>,
        cluster_id: Option<Ipv4Addr>,
        current_neighbor: Option<Ipv4Addr>,
        current_neighbor_v6: Option<Ipv6Addr>,
        current_peer_group: Option<String>,
        in_af_ipv4: bool,
        in_af_ipv6: bool,
        in_af_evpn: bool,
        peer_groups: BTreeMap<String, BgpPeerGroup>,
        neighbors: BTreeMap<Ipv4Addr, RawBgpPeer>,
        networks: Vec<Ipv4Prefix>,
        neighbors_v6: BTreeMap<Ipv6Addr, RawBgpPeer>,
        networks_v6: Vec<Ipv6Prefix>,
        evpn: Option<BgpEvpnConfig>,
    },
    SegmentRouting {
        srv6: Option<SrV6Config>,
        in_srv6: bool,
        in_locators: bool,
        current_locator: Option<SrV6Locator>,
    },
    Static {
        in_af_ipv4: bool,
        in_af_ipv6: bool,
    },
    PrefixSet {
        name: String,
        entries: Vec<PrefixListEntry>,
        entries_v6: Vec<PrefixListEntryV6>,
    },
    RoutePolicy(ParsedRpl),
}

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

/// Parse Cisco IOS XR configuration into vendor-independent DeviceConfig IR.
pub fn parse_iosxr_config(input: &str, filename: Option<&str>) -> DeviceConfig {
    let mut hostname = String::from("router");
    let mut interfaces: Vec<Interface> = Vec::new();
    let mut ospf_process: Option<OspfProcess> = None;
    let mut ospf_areas_map: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>> =
        BTreeMap::new();
    let mut ospfv3_process: Option<Ospf6Process> = None;
    let mut ospfv3_areas_map: BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>> =
        BTreeMap::new();
    let mut isis_process: Option<IsisProcess> = None;
    let mut isis_interfaces: BTreeMap<String, RawIsisInterface> = BTreeMap::new();
    let mut bgp_process: Option<BgpProcess> = None;
    let mut segment_routing: Option<SegmentRoutingConfig> = None;
    let community_lists_map: BTreeMap<String, Vec<CommunityListEntry>> = BTreeMap::new();
    let mut static_routes: Vec<StaticRoute> = Vec::new();
    let mut static_routes_v6: Vec<Ipv6StaticRoute> = Vec::new();
    let mut prefix_lists: Vec<PrefixList> = Vec::new();
    let mut prefix_lists_v6: Vec<PrefixListV6> = Vec::new();
    let mut policies: Vec<PolicyDefinition> = Vec::new();
    let mut unparsed_lines: Vec<UnparsedLine> = Vec::new();

    let mut ctx = Context::Global;

    let flush_context =
        |ctx: &mut Context,
         interfaces: &mut Vec<Interface>,
         ospf_process: &mut Option<OspfProcess>,
         ospf_areas_map: &mut BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>>,
         ospfv3_process: &mut Option<Ospf6Process>,
         ospfv3_areas_map: &mut BTreeMap<OspfAreaId, BTreeMap<String, RawOspfInterface>>,
         isis_process: &mut Option<IsisProcess>,
         isis_interfaces: &mut BTreeMap<String, RawIsisInterface>,
         bgp_process: &mut Option<BgpProcess>,
         prefix_lists: &mut Vec<PrefixList>,
         prefix_lists_v6: &mut Vec<PrefixListV6>,
         policies: &mut Vec<PolicyDefinition>,
         segment_routing: &mut Option<SegmentRoutingConfig>| {
            match ctx {
                Context::Interface(iface) => {
                    interfaces.push(iface.clone());
                }
                Context::Ospf {
                    proc_id,
                    router_id,
                    areas,
                    ..
                } => {
                    *ospf_process = Some(OspfProcess {
                        process_id: *proc_id,
                        router_id: *router_id,
                    });
                    for (a_id, if_map) in areas.iter() {
                        let entry = ospf_areas_map.entry(a_id.clone()).or_default();
                        for (if_name, raw_if) in if_map {
                            entry.insert(
                                if_name.clone(),
                                RawOspfInterface {
                                    cost: raw_if.cost,
                                    passive: raw_if.passive,
                                },
                            );
                        }
                    }
                }
                Context::OspfV3 {
                    proc_id,
                    router_id,
                    areas,
                    ..
                } => {
                    *ospfv3_process = Some(Ospf6Process {
                        process_id: *proc_id,
                        router_id: *router_id,
                    });
                    for (a_id, if_map) in areas.iter() {
                        let entry = ospfv3_areas_map.entry(a_id.clone()).or_default();
                        for (if_name, raw_if) in if_map {
                            entry.insert(
                                if_name.clone(),
                                RawOspfInterface {
                                    cost: raw_if.cost,
                                    passive: raw_if.passive,
                                },
                            );
                        }
                    }
                }
                Context::Isis {
                    net,
                    level,
                    interfaces: isis_ifs,
                    ..
                } => {
                    *isis_process = Some(IsisProcess {
                        net: net.clone(),
                        level: *level,
                        te_router_id: None,
                    });
                    isis_interfaces.extend(isis_ifs.clone());
                }

                Context::Bgp {
                    as_num,
                    router_id,
                    cluster_id,
                    peer_groups,
                    neighbors,
                    networks,
                    neighbors_v6,
                    networks_v6,
                    evpn,
                    ..
                } => {
                    let mut nbr_vec = Vec::new();
                    for (addr, raw) in neighbors.iter() {
                        let mut rem_as = raw.remote_as;
                        let mut rr = raw.route_reflector_client;
                        let mut cid = raw.cluster_id;
                        let mut sc = raw.send_community.clone();
                        let mut bfd = raw.bfd;

                        if let Some(ref pg_name) = raw.peer_group {
                            if let Some(pg) = peer_groups.get(pg_name) {
                                if rem_as == 0 {
                                    rem_as = pg.remote_as.unwrap_or(0);
                                }
                                if rr.is_none() {
                                    rr = pg.route_reflector_client;
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

                        nbr_vec.push(BgpNeighbor {
                            remote_address: *addr,
                            remote_as: rem_as,
                            peer_group: raw.peer_group.clone(),
                            route_reflector_client: rr,
                            cluster_id: cid,
                            send_community: sc,
                            bfd,
                        });
                    }
                    nbr_vec.sort_by_key(|n| n.remote_address);
                    let mut nets = networks.clone();
                    nets.sort();
                    nets.dedup();

                    let mut nbr_v6_vec = Vec::new();
                    for (addr, raw) in neighbors_v6.iter() {
                        let mut rem_as = raw.remote_as;
                        let mut rr = raw.route_reflector_client;
                        let mut cid = raw.cluster_id;
                        let mut sc = raw.send_community.clone();
                        let mut bfd = raw.bfd;

                        if let Some(ref pg_name) = raw.peer_group {
                            if let Some(pg) = peer_groups.get(pg_name) {
                                if rem_as == 0 {
                                    rem_as = pg.remote_as.unwrap_or(0);
                                }
                                if rr.is_none() {
                                    rr = pg.route_reflector_client;
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

                        nbr_v6_vec.push(BgpNeighborV6 {
                            remote_address: *addr,
                            remote_as: rem_as,
                            peer_group: raw.peer_group.clone(),
                            route_reflector_client: rr,
                            cluster_id: cid,
                            send_community: sc,
                            bfd,
                        });
                    }
                    nbr_v6_vec.sort_by_key(|n| n.remote_address);
                    let mut nets_v6 = networks_v6.clone();
                    nets_v6.sort();
                    nets_v6.dedup();

                    *bgp_process = Some(BgpProcess {
                        as_number: *as_num,
                        router_id: *router_id,
                        cluster_id: *cluster_id,
                        peer_groups: peer_groups.values().cloned().collect(),
                        neighbors: nbr_vec,
                        networks: nets,
                        neighbors_v6: nbr_v6_vec,
                        networks_v6: nets_v6,
                        evpn: evpn.clone(),
                    });
                }
                Context::Static { .. } => {}
                Context::SegmentRouting {
                    srv6,
                    current_locator,
                    ..
                } => {
                    let mut srv6_cfg = srv6.clone().unwrap_or_default();
                    if let Some(loc) = current_locator.clone() {
                        srv6_cfg.locators.push(loc);
                    }
                    if !srv6_cfg.locators.is_empty()
                        || srv6_cfg.encapsulation_source_address.is_some()
                    {
                        *segment_routing = Some(SegmentRoutingConfig {
                            srv6: Some(srv6_cfg),
                            mpls: None,
                        });
                    }
                }
                Context::PrefixSet {
                    name,
                    entries,
                    entries_v6,
                } => {
                    if !entries.is_empty() {
                        prefix_lists.push(PrefixList {
                            name: name.clone(),
                            entries: entries.clone(),
                        });
                    }
                    if !entries_v6.is_empty() {
                        prefix_lists_v6.push(PrefixListV6 {
                            name: name.clone(),
                            entries: entries_v6.clone(),
                        });
                    }
                }
                Context::RoutePolicy(rpl) => {
                    let mut stmts = rpl.statements.clone();
                    if stmts.is_empty()
                        && (rpl.current_match_ps.is_some()
                            || rpl.current_match_ps_v6.is_some()
                            || rpl.current_match_community.is_some()
                            || rpl.current_med.is_some()
                            || rpl.current_lp.is_some()
                            || rpl.current_set_community.is_some())
                    {
                        stmts.push(PolicyStatement {
                            name: "10".to_string(),
                            conditions: PolicyConditions {
                                match_prefix_set: rpl.current_match_ps.clone(),
                                match_prefix_set_v6: rpl.current_match_ps_v6.clone(),
                                match_community: rpl.current_match_community.clone(),
                            },
                            actions: PolicyActions {
                                policy_result: PolicyAction::Permit,
                                set_metric: rpl.current_med,
                                set_local_preference: rpl.current_lp,
                                set_community: rpl.current_set_community.clone(),
                            },
                        });
                    }
                    policies.push(PolicyDefinition {
                        name: rpl.name.clone(),
                        statements: stmts,
                    });
                }
                Context::Global => {}
            }
            *ctx = Context::Global;
        };

    for (idx, raw_line) in input.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();

        if line.is_empty() {
            continue;
        }

        // Comment lines start with '!'
        if line.starts_with('!') {
            // In IOS XR, '!' inside sub-blocks can either close the innermost block or separate sub-blocks
            match &mut ctx {
                Context::Ospf {
                    current_interface,
                    current_area,
                    ..
                } => {
                    if current_interface.is_some() {
                        *current_interface = None;
                    } else if current_area.is_some() {
                        *current_area = None;
                    } else {
                        flush_context(
                            &mut ctx,
                            &mut interfaces,
                            &mut ospf_process,
                            &mut ospf_areas_map,
                            &mut ospfv3_process,
                            &mut ospfv3_areas_map,
                            &mut isis_process,
                            &mut isis_interfaces,
                            &mut bgp_process,
                            &mut prefix_lists,
                            &mut prefix_lists_v6,
                            &mut policies,
                            &mut segment_routing,
                        );
                    }
                }
                Context::OspfV3 {
                    current_interface,
                    current_area,
                    ..
                } => {
                    if current_interface.is_some() {
                        *current_interface = None;
                    } else if current_area.is_some() {
                        *current_area = None;
                    } else {
                        flush_context(
                            &mut ctx,
                            &mut interfaces,
                            &mut ospf_process,
                            &mut ospf_areas_map,
                            &mut ospfv3_process,
                            &mut ospfv3_areas_map,
                            &mut isis_process,
                            &mut isis_interfaces,
                            &mut bgp_process,
                            &mut prefix_lists,
                            &mut prefix_lists_v6,
                            &mut policies,
                            &mut segment_routing,
                        );
                    }
                }
                Context::Isis {
                    current_interface, ..
                } => {
                    if current_interface.is_some() {
                        *current_interface = None;
                    } else {
                        flush_context(
                            &mut ctx,
                            &mut interfaces,
                            &mut ospf_process,
                            &mut ospf_areas_map,
                            &mut ospfv3_process,
                            &mut ospfv3_areas_map,
                            &mut isis_process,
                            &mut isis_interfaces,
                            &mut bgp_process,
                            &mut prefix_lists,
                            &mut prefix_lists_v6,
                            &mut policies,
                            &mut segment_routing,
                        );
                    }
                }

                Context::Bgp {
                    current_neighbor,
                    current_neighbor_v6,
                    current_peer_group,
                    in_af_ipv4,
                    in_af_ipv6,
                    in_af_evpn,
                    ..
                } => {
                    if current_peer_group.is_some() {
                        *current_peer_group = None;
                    } else if current_neighbor.is_some() {
                        *current_neighbor = None;
                    } else if current_neighbor_v6.is_some() {
                        *current_neighbor_v6 = None;
                    } else if *in_af_ipv4 {
                        *in_af_ipv4 = false;
                    } else if *in_af_ipv6 {
                        *in_af_ipv6 = false;
                    } else if *in_af_evpn {
                        *in_af_evpn = false;
                    } else {
                        flush_context(
                            &mut ctx,
                            &mut interfaces,
                            &mut ospf_process,
                            &mut ospf_areas_map,
                            &mut ospfv3_process,
                            &mut ospfv3_areas_map,
                            &mut isis_process,
                            &mut isis_interfaces,
                            &mut bgp_process,
                            &mut prefix_lists,
                            &mut prefix_lists_v6,
                            &mut policies,
                            &mut segment_routing,
                        );
                    }
                }
                Context::Static {
                    in_af_ipv4,
                    in_af_ipv6,
                } => {
                    if *in_af_ipv4 {
                        *in_af_ipv4 = false;
                    } else if *in_af_ipv6 {
                        *in_af_ipv6 = false;
                    } else {
                        flush_context(
                            &mut ctx,
                            &mut interfaces,
                            &mut ospf_process,
                            &mut ospf_areas_map,
                            &mut ospfv3_process,
                            &mut ospfv3_areas_map,
                            &mut isis_process,
                            &mut isis_interfaces,
                            &mut bgp_process,
                            &mut prefix_lists,
                            &mut prefix_lists_v6,
                            &mut policies,
                            &mut segment_routing,
                        );
                    }
                }
                Context::PrefixSet { .. } => {
                    // Do not flush on '!' inside prefix-set; wait for end-set
                }
                Context::RoutePolicy(..) => {
                    // Do not flush on '!' inside route-policy; wait for end-policy
                }
                Context::SegmentRouting {
                    current_locator,
                    srv6,
                    ..
                } => {
                    if let Some(loc) = current_locator.take() {
                        let cfg = srv6.get_or_insert_with(Default::default);
                        cfg.locators.push(loc);
                    }
                }
                _ => {
                    flush_context(
                        &mut ctx,
                        &mut interfaces,
                        &mut ospf_process,
                        &mut ospf_areas_map,
                        &mut ospfv3_process,
                        &mut ospfv3_areas_map,
                        &mut isis_process,
                        &mut isis_interfaces,
                        &mut bgp_process,
                        &mut prefix_lists,
                        &mut prefix_lists_v6,
                        &mut policies,
                        &mut segment_routing,
                    );
                }
            }
            continue;
        }

        if line == "end-set" {
            if let Context::PrefixSet {
                name,
                entries,
                entries_v6,
            } = &ctx
            {
                if !entries.is_empty() {
                    prefix_lists.push(PrefixList {
                        name: name.clone(),
                        entries: entries.clone(),
                    });
                }
                if !entries_v6.is_empty() {
                    prefix_lists_v6.push(PrefixListV6 {
                        name: name.clone(),
                        entries: entries_v6.clone(),
                    });
                }
            }
            ctx = Context::Global;
            continue;
        }

        if line == "end-policy" {
            if let Context::RoutePolicy(rpl) = &ctx {
                let mut stmts = rpl.statements.clone();
                if stmts.is_empty()
                    && (rpl.current_match_ps.is_some()
                        || rpl.current_match_ps_v6.is_some()
                        || rpl.current_med.is_some()
                        || rpl.current_lp.is_some())
                {
                    stmts.push(PolicyStatement {
                        name: "10".to_string(),
                        conditions: PolicyConditions {
                            match_prefix_set: rpl.current_match_ps.clone(),
                            match_prefix_set_v6: rpl.current_match_ps_v6.clone(),
                            match_community: rpl.current_match_community.clone(),
                        },
                        actions: PolicyActions {
                            policy_result: PolicyAction::Permit,
                            set_metric: rpl.current_med,
                            set_local_preference: rpl.current_lp,
                            set_community: rpl.current_set_community.clone(),
                        },
                    });
                }
                policies.push(PolicyDefinition {
                    name: rpl.name.clone(),
                    statements: stmts,
                });
            }
            ctx = Context::Global;
            continue;
        }

        if line == "exit" || line == "end" {
            flush_context(
                &mut ctx,
                &mut interfaces,
                &mut ospf_process,
                &mut ospf_areas_map,
                &mut ospfv3_process,
                &mut ospfv3_areas_map,
                &mut isis_process,
                &mut isis_interfaces,
                &mut bgp_process,
                &mut prefix_lists,
                &mut prefix_lists_v6,
                &mut policies,
                &mut segment_routing,
            );
            continue;
        }

        // Top-level unindented keywords that trigger flush
        let is_indented = raw_line.starts_with(' ') || raw_line.starts_with('\t');
        if !is_indented && !matches!(ctx, Context::Global) {
            let is_top_command = line.starts_with("hostname ")
                || line.starts_with("interface ")
                || line.starts_with("router ospf ")
                || line.starts_with("router bgp ")
                || line.starts_with("router static")
                || line.starts_with("prefix-set ")
                || line.starts_with("route-policy ")
                || line == "segment-routing"
                || line.starts_with("segment-routing ");

            if is_top_command {
                flush_context(
                    &mut ctx,
                    &mut interfaces,
                    &mut ospf_process,
                    &mut ospf_areas_map,
                    &mut ospfv3_process,
                    &mut ospfv3_areas_map,
                    &mut isis_process,
                    &mut isis_interfaces,
                    &mut bgp_process,
                    &mut prefix_lists,
                    &mut prefix_lists_v6,
                    &mut policies,
                    &mut segment_routing,
                );
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
                        let name_str = iface_name.trim().to_string();
                        let is_vxlan = name_str.to_lowercase().starts_with("nve")
                            || name_str.to_lowercase().starts_with("vxlan");
                        ctx = Context::Interface(Interface {
                            name: name_str,
                            description: None,
                            enabled: true,
                            ipv4: None,
                            ipv6: None,
                            ospf: None,
                            ospfv3: None,
                            isis: None,
                            vxlan: if is_vxlan {
                                Some(InterfaceVxlanConfig::default())
                            } else {
                                None
                            },
                        });
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed interface statement".to_string(),
                        });
                    }
                } else if line.starts_with("router ospfv3 ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, &str)> =
                        seq!("router ospfv3", space1, rest).parse_next(&mut s);
                    if let Ok((_, _, proc_str)) = parsed {
                        if let Ok(proc_id) = proc_str.trim().parse::<u16>() {
                            ctx = Context::OspfV3 {
                                proc_id,
                                router_id: None,
                                current_area: None,
                                current_interface: None,
                                areas: BTreeMap::new(),
                            };
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
                            reason: "malformed router ospfv3 command".to_string(),
                        });
                    }
                } else if line.starts_with("router isis") {
                    ctx = Context::Isis {
                        net: None,
                        level: None,
                        current_interface: None,
                        interfaces: BTreeMap::new(),
                    };
                } else if line.starts_with("router ospf ") {
                    let mut s = line;
                    let parsed_pid: PResult<(&str, &str, &str)> =
                        seq!("router ospf", space1, rest).parse_next(&mut s);

                    if let Ok((_, _, id_str)) = parsed_pid {
                        if let Ok(proc_id) = id_str.trim().parse::<u16>() {
                            ctx = Context::Ospf {
                                proc_id,
                                router_id: None,
                                current_area: None,
                                current_interface: None,
                                areas: BTreeMap::new(),
                            };
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
                } else if line.starts_with("router bgp ") {
                    let mut s = line;
                    let parsed_as: PResult<(&str, &str, &str)> =
                        seq!("router bgp", space1, rest).parse_next(&mut s);

                    if let Ok((_, _, as_str)) = parsed_as {
                        if let Ok(as_num) = as_str.trim().parse::<u32>() {
                            ctx = Context::Bgp {
                                as_num,
                                router_id: None,
                                cluster_id: None,
                                current_neighbor: None,
                                current_neighbor_v6: None,
                                current_peer_group: None,
                                in_af_ipv4: false,
                                in_af_ipv6: false,
                                in_af_evpn: false,
                                peer_groups: BTreeMap::new(),
                                neighbors: BTreeMap::new(),
                                networks: Vec::new(),
                                neighbors_v6: BTreeMap::new(),
                                networks_v6: Vec::new(),
                                evpn: None,
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
                } else if line.starts_with("router static") {
                    ctx = Context::Static {
                        in_af_ipv4: false,
                        in_af_ipv6: false,
                    };
                } else if let Some(stripped) = line.strip_prefix("prefix-set ") {
                    let name = stripped.trim().to_string();
                    ctx = Context::PrefixSet {
                        name,
                        entries: Vec::new(),
                        entries_v6: Vec::new(),
                    };
                } else if line == "segment-routing" || line.starts_with("segment-routing ") {
                    ctx = Context::SegmentRouting {
                        srv6: None,
                        in_srv6: false,
                        in_locators: false,
                        current_locator: None,
                    };
                } else if let Some(stripped) = line.strip_prefix("route-policy ") {
                    let name = stripped.trim().to_string();
                    ctx = Context::RoutePolicy(ParsedRpl {
                        name,
                        ..Default::default()
                    });
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
                } else if let Some(stripped) = line.strip_prefix("ipv4 address ") {
                    let tail = stripped.trim();
                    let tokens: Vec<&str> = tail.split_whitespace().collect();
                    if tokens.len() == 1 {
                        // ipv4 address 10.0.0.1/30
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
                                    reason: "invalid IPv4 CIDR address".to_string(),
                                });
                            }
                        }
                    } else if tokens.len() >= 2 {
                        // ipv4 address 10.0.0.1 255.255.255.0
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
                                        reason: format!("invalid IPv4 netmask: {}", reason),
                                    });
                                }
                            },
                            _ => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed ipv4 address command".to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed ipv4 address command".to_string(),
                        });
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
                                reason: "invalid IPv6 address".to_string(),
                            });
                        }
                    }
                } else if line == "shutdown" {
                    iface.enabled = false;
                } else if line == "no shutdown" {
                    iface.enabled = true;
                } else if let Some(stripped) = line.strip_prefix("source-interface ") {
                    let cfg = iface.vxlan.get_or_insert_with(Default::default);
                    cfg.source_interface = Some(stripped.trim().to_string());
                } else if let Some(stripped) = line.strip_prefix("member vni ") {
                    let parts: Vec<&str> = stripped.split_whitespace().collect();
                    if let Some(vni_str) = parts.first().copied() {
                        if let Ok(vni) = vni_str.parse::<u32>() {
                            let mut vlan = None;
                            if parts.len() >= 3 && parts[1] == "vlan" {
                                vlan = parts[2].parse::<u16>().ok();
                            }
                            let cfg = iface.vxlan.get_or_insert_with(Default::default);
                            cfg.vni_bindings.push(VniVlanBinding { vni, vlan });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("vxlan udp-port ") {
                    if let Ok(port) = stripped.trim().parse::<u16>() {
                        let cfg = iface.vxlan.get_or_insert_with(Default::default);
                        cfg.udp_port = Some(port);
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "unsupported interface command".to_string(),
                    });
                }
            }
            Context::OspfV3 {
                ref mut router_id,
                ref mut current_area,
                ref mut current_interface,
                ref mut areas,
                ..
            } => {
                if line.starts_with("router-id ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, Ipv4Addr)> =
                        seq!("router-id", space1, parse_ipv4).parse_next(&mut s);
                    if let Ok((_, _, rid)) = parsed {
                        *router_id = Some(rid);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router-id in ospfv3".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("area ") {
                    let mut s = stripped.trim();
                    match parse_ospf_area.parse_next(&mut s) {
                        Ok(area_id) => {
                            *current_area = Some(area_id);
                            *current_interface = None;
                        }
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed OSPFv3 area identifier".to_string(),
                            });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("interface ") {
                    let if_name = stripped.trim().to_string();
                    if let Some(ref area_id) = current_area {
                        areas
                            .entry(area_id.clone())
                            .or_default()
                            .entry(if_name.clone())
                            .or_default();
                        *current_interface = Some(if_name);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "OSPFv3 interface specified outside area block".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("cost ") {
                    if let Ok(cost) = stripped.trim().parse::<u16>() {
                        if let (Some(area_id), Some(if_name)) =
                            (current_area.clone(), current_interface.clone())
                        {
                            areas
                                .entry(area_id)
                                .or_default()
                                .entry(if_name)
                                .or_default()
                                .cost = Some(cost);
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "invalid OSPFv3 cost".to_string(),
                        });
                    }
                } else if line == "passive enable" || line == "passive" {
                    if let (Some(area_id), Some(if_name)) =
                        (current_area.clone(), current_interface.clone())
                    {
                        areas
                            .entry(area_id)
                            .or_default()
                            .entry(if_name)
                            .or_default()
                            .passive = Some(true);
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
            Context::Ospf {
                ref mut router_id,
                ref mut current_area,
                ref mut current_interface,
                ref mut areas,
                ..
            } => {
                if line.starts_with("router-id ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, Ipv4Addr)> =
                        seq!("router-id", space1, parse_ipv4).parse_next(&mut s);
                    if let Ok((_, _, rid)) = parsed {
                        *router_id = Some(rid);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed router-id in ospf".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("area ") {
                    let mut s = stripped.trim();
                    match parse_ospf_area.parse_next(&mut s) {
                        Ok(area_id) => {
                            *current_area = Some(area_id);
                            *current_interface = None;
                        }
                        Err(_) => {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed OSPF area identifier".to_string(),
                            });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("interface ") {
                    let if_name = stripped.trim().to_string();
                    if let Some(ref area_id) = current_area {
                        areas
                            .entry(area_id.clone())
                            .or_default()
                            .entry(if_name.clone())
                            .or_default();
                        *current_interface = Some(if_name);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "OSPF interface specified outside area block".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("cost ") {
                    if let Ok(cost) = stripped.trim().parse::<u16>() {
                        if let (Some(area_id), Some(if_name)) =
                            (current_area.clone(), current_interface.clone())
                        {
                            areas
                                .entry(area_id)
                                .or_default()
                                .entry(if_name)
                                .or_default()
                                .cost = Some(cost);
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "invalid OSPF cost".to_string(),
                        });
                    }
                } else if line == "passive enable" || line == "passive" {
                    if let (Some(area_id), Some(if_name)) =
                        (current_area.clone(), current_interface.clone())
                    {
                        areas
                            .entry(area_id)
                            .or_default()
                            .entry(if_name)
                            .or_default()
                            .passive = Some(true);
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
            Context::Isis {
                ref mut net,
                ref mut level,
                ref mut current_interface,
                ref mut interfaces,
            } => {
                if let Some(stripped) = line.strip_prefix("net ") {
                    *net = Some(stripped.trim().to_string());
                } else if let Some(stripped) = line.strip_prefix("is-type ") {
                    match stripped.trim() {
                        "level-1" => *level = Some(IsisLevel::Level1),
                        "level-2-only" | "level-2" => *level = Some(IsisLevel::Level2),
                        "level-1-2" => *level = Some(IsisLevel::Level12),
                        _ => {}
                    }
                } else if let Some(stripped) = line.strip_prefix("interface ") {
                    let if_name = stripped.trim().to_string();
                    *current_interface = Some(if_name.clone());
                    interfaces.entry(if_name).or_default();
                } else if line == "passive" {
                    if let Some(ref if_name) = current_interface {
                        interfaces.entry(if_name.clone()).or_default().passive = Some(true);
                    }
                } else if let Some(stripped) = line.strip_prefix("metric ") {
                    if let Ok(m) = stripped.trim().parse::<u32>() {
                        if let Some(ref if_name) = current_interface {
                            interfaces.entry(if_name.clone()).or_default().metric = Some(m);
                        }
                    }
                } else if line.starts_with("point-to-point") || line.starts_with("circuit-type") {
                    // Supported
                } else if line.starts_with("bfd fast-detect") || line.starts_with("bfd") {
                    if let Some(ref if_name) = current_interface {
                        interfaces.entry(if_name.clone()).or_default().bfd = Some(true);
                    }
                }
            }
            Context::Bgp {
                ref mut router_id,
                ref mut cluster_id,
                ref mut current_neighbor,
                ref mut current_neighbor_v6,
                ref mut current_peer_group,
                ref mut in_af_ipv4,
                ref mut in_af_ipv6,
                ref mut in_af_evpn,
                ref mut peer_groups,
                ref mut neighbors,
                ref mut networks,
                ref mut neighbors_v6,
                ref mut networks_v6,
                ref mut evpn,
                ..
            } => {
                if let Some(stripped) = line.strip_prefix("cluster-id ") {
                    let mut s = stripped.trim();
                    if let Ok(cid) = parse_ipv4.parse_next(&mut s) {
                        *cluster_id = Some(cid);
                    }
                } else if let Some(stripped) = line.strip_prefix("neighbor-group ") {
                    let gname = stripped.trim().to_string();
                    *current_peer_group = Some(gname.clone());
                    *current_neighbor = None;
                    *current_neighbor_v6 = None;
                    peer_groups
                        .entry(gname.clone())
                        .or_insert_with(|| BgpPeerGroup {
                            name: gname,
                            ..Default::default()
                        });
                } else if let Some(stripped) = line.strip_prefix("use neighbor-group ") {
                    let gname = stripped.trim().to_string();
                    if let Some(ip) = current_neighbor {
                        neighbors.entry(*ip).or_default().peer_group = Some(gname);
                    } else if let Some(ip6) = current_neighbor_v6 {
                        neighbors_v6.entry(*ip6).or_default().peer_group = Some(gname);
                    }
                } else if line.starts_with("route-reflector-client") {
                    if let Some(ref pg_name) = current_peer_group {
                        peer_groups
                            .entry(pg_name.clone())
                            .or_default()
                            .route_reflector_client = Some(true);
                    } else if let Some(ip) = current_neighbor {
                        neighbors.entry(*ip).or_default().route_reflector_client = Some(true);
                    } else if let Some(ip6) = current_neighbor_v6 {
                        neighbors_v6.entry(*ip6).or_default().route_reflector_client = Some(true);
                    }
                } else if line.starts_with("send-community-ebgp")
                    || line.starts_with("send-community")
                {
                    if let Some(ref pg_name) = current_peer_group {
                        peer_groups
                            .entry(pg_name.clone())
                            .or_default()
                            .send_community = Some("standard".to_string());
                    } else if let Some(ip) = current_neighbor {
                        neighbors.entry(*ip).or_default().send_community =
                            Some("standard".to_string());
                    } else if let Some(ip6) = current_neighbor_v6 {
                        neighbors_v6.entry(*ip6).or_default().send_community =
                            Some("standard".to_string());
                    }
                } else if line.starts_with("bfd fast-detect") || line.starts_with("bfd") {
                    if let Some(ref pg_name) = current_peer_group {
                        peer_groups.entry(pg_name.clone()).or_default().bfd = Some(true);
                    } else if let Some(ip) = current_neighbor {
                        neighbors.entry(*ip).or_default().bfd = Some(true);
                    } else if let Some(ip6) = current_neighbor_v6 {
                        neighbors_v6.entry(*ip6).or_default().bfd = Some(true);
                    }
                }
                if line.starts_with("bgp router-id ") {
                    let mut s = line;
                    let parsed: PResult<(&str, &str, Ipv4Addr)> =
                        seq!("bgp router-id", space1, parse_ipv4).parse_next(&mut s);
                    if let Ok((_, _, rid)) = parsed {
                        *router_id = Some(rid);
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed bgp router-id".to_string(),
                        });
                    }
                } else if line.starts_with("address-family ipv4 unicast") {
                    *in_af_ipv4 = true;
                    *in_af_ipv6 = false;
                    *in_af_evpn = false;
                    *current_neighbor = None;
                    *current_neighbor_v6 = None;
                } else if line.starts_with("address-family ipv6 unicast") {
                    *in_af_ipv6 = true;
                    *in_af_ipv4 = false;
                    *in_af_evpn = false;
                    *current_neighbor = None;
                    *current_neighbor_v6 = None;
                } else if line.starts_with("address-family l2vpn evpn")
                    || line.starts_with("address-family evpn")
                {
                    *in_af_evpn = true;
                    *in_af_ipv4 = false;
                    *in_af_ipv6 = false;
                    let cfg = evpn.get_or_insert_with(Default::default);
                    cfg.enabled = true;
                    if let Some(nbr) = current_neighbor {
                        if !cfg.neighbors.iter().any(|n| n.peer == nbr.to_string()) {
                            cfg.neighbors.push(BgpEvpnNeighbor {
                                peer: nbr.to_string(),
                                route_reflector_client: None,
                                send_community: None,
                            });
                        }
                    } else if let Some(nbr6) = current_neighbor_v6 {
                        if !cfg.neighbors.iter().any(|n| n.peer == nbr6.to_string()) {
                            cfg.neighbors.push(BgpEvpnNeighbor {
                                peer: nbr6.to_string(),
                                route_reflector_client: None,
                                send_community: None,
                            });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("neighbor ") {
                    let mut s = stripped.trim();
                    if let Ok(ip) = parse_ipv4.parse_next(&mut s) {
                        *current_neighbor = Some(ip);
                        *current_neighbor_v6 = None;
                        neighbors.entry(ip).or_default();
                    } else {
                        let mut s_v6 = stripped.trim();
                        if let Ok(ip6) = parse_ipv6.parse_next(&mut s_v6) {
                            *current_neighbor_v6 = Some(ip6);
                            *current_neighbor = None;
                            neighbors_v6.entry(ip6).or_default();
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "malformed BGP neighbor address".to_string(),
                            });
                        }
                    }
                } else if let Some(stripped) = line.strip_prefix("remote-as ") {
                    if let Ok(rem_as) = stripped.trim().parse::<u32>() {
                        if let Some(ref pg_name) = current_peer_group {
                            peer_groups.entry(pg_name.clone()).or_default().remote_as =
                                Some(rem_as);
                        } else if let Some(ref nbr_ip) = current_neighbor {
                            neighbors.entry(*nbr_ip).or_default().remote_as = rem_as;
                        } else if let Some(ref nbr_ip6) = current_neighbor_v6 {
                            neighbors_v6.entry(*nbr_ip6).or_default().remote_as = rem_as;
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "invalid BGP remote-as".to_string(),
                        });
                    }
                } else if let Some(stripped) = line.strip_prefix("network ") {
                    if *in_af_ipv6 {
                        let mut s = stripped.trim();
                        match parse_ipv6_prefix.parse_next(&mut s) {
                            Ok(prefix) => {
                                networks_v6.push(prefix);
                            }
                            Err(_) => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed BGP IPv6 network prefix".to_string(),
                                });
                            }
                        }
                    } else {
                        let mut s = stripped.trim();
                        match parse_ipv4_prefix.parse_next(&mut s) {
                            Ok(prefix) => {
                                networks.push(prefix);
                            }
                            Err(_) => {
                                unparsed_lines.push(UnparsedLine {
                                    file_path: filename.map(String::from),
                                    line_number: line_num,
                                    content: line.to_string(),
                                    reason: "malformed BGP network prefix".to_string(),
                                });
                            }
                        }
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
            Context::Static {
                ref mut in_af_ipv4,
                ref mut in_af_ipv6,
            } => {
                if line.starts_with("address-family ipv4 unicast") {
                    *in_af_ipv4 = true;
                    *in_af_ipv6 = false;
                } else if line.starts_with("address-family ipv6 unicast") {
                    *in_af_ipv6 = true;
                    *in_af_ipv4 = false;
                } else if *in_af_ipv4 {
                    // Lines like: 10.0.0.0/24 192.168.1.1
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if tokens.len() >= 2 {
                        let mut s_prefix = tokens[0];
                        let mut s_nh = tokens[1];
                        let prefix_res = parse_ipv4_prefix.parse_next(&mut s_prefix);
                        let nh_res = parse_ipv4.parse_next(&mut s_nh);
                        match (prefix_res, nh_res) {
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
                                    reason:
                                        "unsupported or malformed static route destination/nexthop"
                                            .to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed static route line".to_string(),
                        });
                    }
                } else if *in_af_ipv6 {
                    // Lines like: 2001:db8:2::/64 2001:db8:1::2
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if tokens.len() >= 2 {
                        let mut s_prefix = tokens[0];
                        let mut s_nh = tokens[1];
                        let prefix_res = parse_ipv6_prefix.parse_next(&mut s_prefix);
                        let nh_res = parse_ipv6.parse_next(&mut s_nh);
                        match (prefix_res, nh_res) {
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
                                    reason:
                                        "unsupported or malformed IPv6 static route destination/nexthop"
                                            .to_string(),
                                });
                            }
                        }
                    } else {
                        unparsed_lines.push(UnparsedLine {
                            file_path: filename.map(String::from),
                            line_number: line_num,
                            content: line.to_string(),
                            reason: "malformed IPv6 static route line".to_string(),
                        });
                    }
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "unsupported router static command".to_string(),
                    });
                }
            }
            Context::SegmentRouting {
                ref mut srv6,
                ref mut in_srv6,
                ref mut in_locators,
                ref mut current_locator,
            } => {
                if line == "srv6" {
                    *in_srv6 = true;
                    if srv6.is_none() {
                        *srv6 = Some(SrV6Config {
                            enabled: true,
                            encapsulation_source_address: None,
                            locators: Vec::new(),
                        });
                    }
                } else if line == "locators" {
                    *in_locators = true;
                } else if let Some(stripped) = line.strip_prefix("source-address ") {
                    if let Ok(ip6) = stripped.trim().parse::<Ipv6Addr>() {
                        let cfg = srv6.get_or_insert_with(Default::default);
                        cfg.encapsulation_source_address = Some(ip6);
                    }
                } else if let Some(stripped) = line.strip_prefix("locator ") {
                    if let Some(prev) = current_locator.take() {
                        let cfg = srv6.get_or_insert_with(Default::default);
                        cfg.locators.push(prev);
                    }
                    let loc_name = stripped.trim().to_string();
                    *current_locator = Some(SrV6Locator {
                        name: loc_name,
                        prefix: "::/0".parse().unwrap(),
                        micro_segment: None,
                    });
                } else if let Some(stripped) = line.strip_prefix("prefix ") {
                    let parts: Vec<&str> = stripped.split_whitespace().collect();
                    if let Some(pfx_str) = parts.first().copied() {
                        if let Ok(pfx) = pfx_str.parse::<Ipv6Prefix>() {
                            if let Some(ref mut loc) = current_locator {
                                loc.prefix = pfx;
                                if parts.iter().any(|&p| p == "micro-segment" || p == "unode") {
                                    loc.micro_segment = Some(SrV6UsidConfig {
                                        enabled: true,
                                        block_length: Some(16),
                                        node_length: Some(16),
                                    });
                                }
                            }
                        }
                    }
                } else if line.starts_with("micro-segment ") {
                    if let Some(ref mut loc) = current_locator {
                        loc.micro_segment = Some(SrV6UsidConfig {
                            enabled: true,
                            block_length: Some(16),
                            node_length: Some(16),
                        });
                    }
                } else if line == "!" || line == "exit" {
                    if let Some(prev) = current_locator.take() {
                        let cfg = srv6.get_or_insert_with(Default::default);
                        cfg.locators.push(prev);
                    }
                }
            }
            Context::PrefixSet {
                ref mut entries,
                ref mut entries_v6,
                ..
            } => {
                let clean_entry = line.trim_end_matches(',');
                let tokens: Vec<&str> = clean_entry.split_whitespace().collect();
                if !tokens.is_empty() {
                    let mut s = tokens[0];
                    if let Ok(prefix) = parse_ipv4_prefix.parse_next(&mut s) {
                        let mut ge_val = None;
                        let mut le_val = None;
                        let mut cursor = 1;
                        let mut parse_range_failed = false;

                        while cursor < tokens.len() {
                            if tokens[cursor] == "ge" && cursor + 1 < tokens.len() {
                                if let Ok(v) = tokens[cursor + 1].parse::<u8>() {
                                    if let Ok(pl) = PrefixLength::new(v) {
                                        ge_val = Some(pl);
                                        cursor += 2;
                                        continue;
                                    }
                                }
                                parse_range_failed = true;
                                break;
                            } else if tokens[cursor] == "le" && cursor + 1 < tokens.len() {
                                if let Ok(v) = tokens[cursor + 1].parse::<u8>() {
                                    if let Ok(pl) = PrefixLength::new(v) {
                                        le_val = Some(pl);
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
                                reason: "malformed prefix-set range parameters".to_string(),
                            });
                        } else {
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
                                        reason: "invalid prefix-set range: len <= ge <= le"
                                            .to_string(),
                                    });
                                    continue;
                                }
                                Some(r)
                            } else {
                                None
                            };

                            let seq_num = ((entries.len() + 1) * 10) as u32;
                            entries.push(PrefixListEntry {
                                sequence: seq_num,
                                action: PolicyAction::Permit,
                                prefix,
                                range,
                            });
                        }
                    } else {
                        let mut s_v6 = tokens[0];
                        if let Ok(prefix) = parse_ipv6_prefix.parse_next(&mut s_v6) {
                            let mut ge_val = None;
                            let mut le_val = None;
                            let mut cursor = 1;
                            let mut parse_range_failed = false;

                            while cursor < tokens.len() {
                                if tokens[cursor] == "ge" && cursor + 1 < tokens.len() {
                                    if let Ok(v) = tokens[cursor + 1].parse::<u8>() {
                                        if let Ok(pl) = Ipv6PrefixLength::new(v) {
                                            ge_val = Some(pl);
                                            cursor += 2;
                                            continue;
                                        }
                                    }
                                    parse_range_failed = true;
                                    break;
                                } else if tokens[cursor] == "le" && cursor + 1 < tokens.len() {
                                    if let Ok(v) = tokens[cursor + 1].parse::<u8>() {
                                        if let Ok(pl) = Ipv6PrefixLength::new(v) {
                                            le_val = Some(pl);
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
                                    reason: "malformed prefix-set range parameters".to_string(),
                                });
                            } else {
                                let range = if ge_val.is_some() || le_val.is_some() {
                                    let r = PrefixRangeV6 {
                                        ge: ge_val,
                                        le: le_val,
                                    };
                                    if r.validate(prefix.prefix_length).is_err() {
                                        unparsed_lines.push(UnparsedLine {
                                            file_path: filename.map(String::from),
                                            line_number: line_num,
                                            content: line.to_string(),
                                            reason: "invalid prefix-set range: len <= ge <= le"
                                                .to_string(),
                                        });
                                        continue;
                                    }
                                    Some(r)
                                } else {
                                    None
                                };

                                let seq_num = ((entries_v6.len() + 1) * 10) as u32;
                                entries_v6.push(PrefixListEntryV6 {
                                    sequence: seq_num,
                                    action: PolicyAction::Permit,
                                    prefix,
                                    range,
                                });
                            }
                        } else {
                            unparsed_lines.push(UnparsedLine {
                                file_path: filename.map(String::from),
                                line_number: line_num,
                                content: line.to_string(),
                                reason: "invalid prefix in prefix-set".to_string(),
                            });
                        }
                    }
                }
            }
            Context::RoutePolicy(ref mut rpl) => {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("if destination in ") {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if !parts.is_empty() {
                        let ps_name = parts[0].to_string();
                        rpl.current_match_ps = Some(ps_name);
                        if trimmed.contains("pass") || trimmed.contains("done") {
                            let seq = ((rpl.statements.len() + 1) * 10) as u32;
                            rpl.statements.push(PolicyStatement {
                                name: seq.to_string(),
                                conditions: PolicyConditions {
                                    match_prefix_set: rpl.current_match_ps.take(),
                                    match_prefix_set_v6: rpl.current_match_ps_v6.take(),
                                    match_community: rpl.current_match_community.take(),
                                },
                                actions: PolicyActions {
                                    policy_result: PolicyAction::Permit,
                                    set_metric: rpl.current_med.take(),
                                    set_local_preference: rpl.current_lp.take(),
                                    set_community: rpl.current_set_community.take(),
                                },
                            });
                        } else if trimmed.contains("drop") {
                            let seq = ((rpl.statements.len() + 1) * 10) as u32;
                            rpl.statements.push(PolicyStatement {
                                name: seq.to_string(),
                                conditions: PolicyConditions {
                                    match_prefix_set: rpl.current_match_ps.take(),
                                    match_prefix_set_v6: rpl.current_match_ps_v6.take(),
                                    match_community: rpl.current_match_community.take(),
                                },
                                actions: PolicyActions {
                                    policy_result: PolicyAction::Deny,
                                    set_metric: rpl.current_med.take(),
                                    set_local_preference: rpl.current_lp.take(),
                                    set_community: rpl.current_set_community.take(),
                                },
                            });
                        }
                    }
                } else if let Some(rest) = trimmed.strip_prefix("set community ") {
                    rpl.current_set_community = Some(
                        rest.trim()
                            .trim_matches('(')
                            .trim_matches(')')
                            .trim()
                            .to_string(),
                    );
                } else if let Some(rest) = trimmed
                    .strip_prefix("if community matches-any ")
                    .or_else(|| trimmed.strip_prefix("community matches-any "))
                {
                    let comm_name = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    rpl.current_match_community = Some(comm_name);
                } else if let Some(rest) = trimmed.strip_prefix("set med ") {
                    if let Ok(v) = rest.trim().parse::<u32>() {
                        rpl.current_med = Some(v);
                    }
                } else if let Some(rest) = trimmed.strip_prefix("set local-preference ") {
                    if let Ok(v) = rest.trim().parse::<u32>() {
                        rpl.current_lp = Some(v);
                    }
                } else if trimmed == "pass" || trimmed == "done" {
                    let seq = ((rpl.statements.len() + 1) * 10) as u32;
                    rpl.statements.push(PolicyStatement {
                        name: seq.to_string(),
                        conditions: PolicyConditions {
                            match_prefix_set: rpl.current_match_ps.take(),
                            match_prefix_set_v6: rpl.current_match_ps_v6.take(),
                            match_community: rpl.current_match_community.take(),
                        },
                        actions: PolicyActions {
                            policy_result: PolicyAction::Permit,
                            set_metric: rpl.current_med.take(),
                            set_local_preference: rpl.current_lp.take(),
                            set_community: rpl.current_set_community.take(),
                        },
                    });
                } else if trimmed == "drop" {
                    let seq = ((rpl.statements.len() + 1) * 10) as u32;
                    rpl.statements.push(PolicyStatement {
                        name: seq.to_string(),
                        conditions: PolicyConditions {
                            match_prefix_set: rpl.current_match_ps.take(),
                            match_prefix_set_v6: rpl.current_match_ps_v6.take(),
                            match_community: rpl.current_match_community.take(),
                        },
                        actions: PolicyActions {
                            policy_result: PolicyAction::Deny,
                            set_metric: rpl.current_med.take(),
                            set_local_preference: rpl.current_lp.take(),
                            set_community: rpl.current_set_community.take(),
                        },
                    });
                } else if trimmed == "endif" || trimmed == "else" {
                    // control flow markers
                } else {
                    unparsed_lines.push(UnparsedLine {
                        file_path: filename.map(String::from),
                        line_number: line_num,
                        content: line.to_string(),
                        reason: "unsupported route-policy statement".to_string(),
                    });
                }
            }
        }
    }

    // Flush any pending context at EOF
    flush_context(
        &mut ctx,
        &mut interfaces,
        &mut ospf_process,
        &mut ospf_areas_map,
        &mut ospfv3_process,
        &mut ospfv3_areas_map,
        &mut isis_process,
        &mut isis_interfaces,
        &mut bgp_process,
        &mut prefix_lists,
        &mut prefix_lists_v6,
        &mut policies,
        &mut segment_routing,
    );

    for iface in &mut interfaces {
        if let Some(isis_if) = isis_interfaces.get(&iface.name) {
            iface.isis = Some(InterfaceIsisConfig {
                level: isis_if.level,
                metric: isis_if.metric,
                passive: isis_if.passive,
                bfd: isis_if.bfd,
            });
        }
    }

    // Attach OSPF configuration to interfaces from ospf_areas_map
    for iface in &mut interfaces {
        for (area_id, area_ifaces) in &ospf_areas_map {
            if let Some(raw_ospf) = area_ifaces.get(&iface.name) {
                iface.ospf = Some(InterfaceOspfConfig {
                    area: area_id.clone(),
                    cost: raw_ospf.cost,
                    passive: raw_ospf.passive,
                    bfd: None,
                });
                break;
            }
        }
        for (area_id, area_ifaces) in &ospfv3_areas_map {
            if let Some(raw_ospf) = area_ifaces.get(&iface.name) {
                iface.ospfv3 = Some(InterfaceOspfConfig {
                    area: area_id.clone(),
                    cost: raw_ospf.cost,
                    passive: raw_ospf.passive,
                    bfd: None,
                });
                break;
            }
        }
    }

    let mut community_lists = Vec::new();
    for (name, entries) in community_lists_map {
        community_lists.push(CommunityList { name, entries });
    }

    DeviceConfig {
        hostname,
        interfaces,
        ospf: ospf_process,
        ospfv3: ospfv3_process,
        isis: isis_process,
        bgp: bgp_process,
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

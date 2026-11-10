use ir::{DeviceConfig, IsisLevel, OspfAreaId, PolicyAction};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitFrrError {
    #[error("FRR configuration emission error: {0}")]
    Format(#[from] std::fmt::Error),
}

/// Diagnostic warning for unsupported or unmapped IR features in FRR configuration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FrrEmitWarning {
    pub category: String,
    pub message: String,
}

/// Result of emitting an FRR configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrrEmitResult {
    pub conf: String,
    pub daemons: String,
    pub warnings: Vec<FrrEmitWarning>,
}

/// Generate FRR `daemons` file content based on protocols active in `DeviceConfig`.
pub fn emit_frr_daemons(device: &DeviceConfig) -> String {
    let zebra = "yes";
    let mut bgpd = "no";
    let mut ospfd = "no";
    let mut ospf6d = "no";
    let mut isisd = "no";
    let mut staticd = "no";
    let mut bfdd = "no";

    if !device.static_routes.is_empty() || !device.static_routes_v6.is_empty() {
        staticd = "yes";
    }

    if device.ospf.is_some() || device.interfaces.iter().any(|i| i.ospf.is_some()) {
        ospfd = "yes";
    }

    if device.ospfv3.is_some() || device.interfaces.iter().any(|i| i.ospfv3.is_some()) {
        ospf6d = "yes";
    }

    if device.isis.is_some() || device.interfaces.iter().any(|i| i.isis.is_some()) {
        isisd = "yes";
    }

    if let Some(ref bgp) = device.bgp {
        bgpd = "yes";
        if bgp.neighbors.iter().any(|n| n.bfd == Some(true))
            || bgp.neighbors_v6.iter().any(|n| n.bfd == Some(true))
            || bgp.peer_groups.iter().any(|pg| pg.bfd == Some(true))
        {
            bfdd = "yes";
        }
    }

    if device
        .interfaces
        .iter()
        .any(|i| i.ospf.as_ref().and_then(|o| o.bfd) == Some(true))
        || device
            .interfaces
            .iter()
            .any(|i| i.isis.as_ref().and_then(|is| is.bfd) == Some(true))
    {
        bfdd = "yes";
    }

    format!(
        "zebra={}\nbgpd={}\nospfd={}\nospf6d={}\nripd=no\nripngd=no\nisisd={}\npimd=no\nldpd=no\nnhrpd=no\neigrpd=no\nbabeld=no\nsharpd=no\npbrd=no\nbfdd={}\nfabricd=no\nvrrpd=no\npathd=no\nstaticd={}\n",
        zebra, bgpd, ospfd, ospf6d, isisd, bfdd, staticd
    )
}

/// Convert a vendor-independent `DeviceConfig` into FRR `frr.conf` syntax.
pub fn emit_frr_conf(device: &DeviceConfig) -> (String, Vec<FrrEmitWarning>) {
    use std::fmt::Write;
    let mut out = String::new();
    let mut warnings = Vec::new();

    // Propagate unparsed lines
    for unparsed in &device.unparsed_lines {
        warnings.push(FrrEmitWarning {
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

    // Header & System
    writeln!(out, "frr version 10.2").unwrap();
    writeln!(out, "frr defaults datacenter").unwrap();
    writeln!(out, "hostname {}", device.hostname).unwrap();
    writeln!(out, "service integrated-vtysh-config").unwrap();
    writeln!(out, "!").unwrap();

    // Interfaces sorted deterministically by name
    let mut sorted_interfaces = device.interfaces.clone();
    sorted_interfaces.sort_by(|a, b| a.name.cmp(&b.name));

    for iface in &sorted_interfaces {
        writeln!(out, "interface {}", iface.name).unwrap();
        if let Some(ref desc) = iface.description {
            writeln!(out, " description {}", desc).unwrap();
        }
        if let Some(ref v4) = iface.ipv4 {
            writeln!(out, " ip address {}", v4).unwrap();
        }
        if let Some(ref v6) = iface.ipv6 {
            writeln!(out, " ipv6 address {}", v6).unwrap();
        }

        // Interface OSPFv2
        if let Some(ref ospf) = iface.ospf {
            match &ospf.area {
                OspfAreaId::Number(n) => writeln!(out, " ip ospf area {}", n).unwrap(),
                OspfAreaId::Ipv4(ip) => writeln!(out, " ip ospf area {}", ip).unwrap(),
            }
            if let Some(cost) = ospf.cost {
                writeln!(out, " ip ospf cost {}", cost).unwrap();
            }
            if ospf.passive == Some(true) {
                writeln!(out, " ip ospf passive").unwrap();
            }
            if ospf.bfd == Some(true) {
                writeln!(out, " ip ospf bfd").unwrap();
            }
        }

        // Interface OSPFv3
        if let Some(ref ospf6) = iface.ospfv3 {
            match &ospf6.area {
                OspfAreaId::Number(n) => writeln!(out, " ipv6 ospf6 area {}", n).unwrap(),
                OspfAreaId::Ipv4(ip) => writeln!(out, " ipv6 ospf6 area {}", ip).unwrap(),
            }
            if let Some(cost) = ospf6.cost {
                writeln!(out, " ipv6 ospf6 cost {}", cost).unwrap();
            }
            if ospf6.passive == Some(true) {
                writeln!(out, " ipv6 ospf6 passive").unwrap();
            }
            if ospf6.bfd == Some(true) {
                writeln!(out, " ipv6 ospf6 bfd").unwrap();
            }
        }

        // Interface IS-IS
        if let Some(ref isis) = iface.isis {
            writeln!(out, " ip router isis netsim").unwrap();
            if let Some(metric) = isis.metric {
                match isis.level {
                    Some(IsisLevel::Level1) => {
                        writeln!(out, " isis metric {} level-1", metric).unwrap()
                    }
                    Some(IsisLevel::Level2) => {
                        writeln!(out, " isis metric {} level-2", metric).unwrap()
                    }
                    _ => writeln!(out, " isis metric {}", metric).unwrap(),
                }
            }
            if isis.passive == Some(true) {
                writeln!(out, " isis passive").unwrap();
            }
            if isis.bfd == Some(true) {
                writeln!(out, " isis bfd").unwrap();
            }
        }

        if !iface.enabled {
            writeln!(out, " shutdown").unwrap();
        }

        writeln!(out, "!").unwrap();
    }

    // Static Routes
    let mut sorted_static_v4 = device.static_routes.clone();
    sorted_static_v4.sort_by(|a, b| {
        a.destination
            .cmp(&b.destination)
            .then_with(|| a.next_hop.cmp(&b.next_hop))
    });
    for r in &sorted_static_v4 {
        writeln!(out, "ip route {} {}", r.destination, r.next_hop).unwrap();
    }
    if !sorted_static_v4.is_empty() {
        writeln!(out, "!").unwrap();
    }

    let mut sorted_static_v6 = device.static_routes_v6.clone();
    sorted_static_v6.sort_by(|a, b| {
        a.destination
            .cmp(&b.destination)
            .then_with(|| a.next_hop.cmp(&b.next_hop))
    });
    for r in &sorted_static_v6 {
        writeln!(out, "ipv6 route {} {}", r.destination, r.next_hop).unwrap();
    }
    if !sorted_static_v6.is_empty() {
        writeln!(out, "!").unwrap();
    }

    // Router OSPFv2
    let has_ospf_ifaces = device.interfaces.iter().any(|i| i.ospf.is_some());
    if device.ospf.is_some() || has_ospf_ifaces {
        let proc_id = device.ospf.as_ref().map(|o| o.process_id).unwrap_or(1);
        if proc_id == 1 {
            writeln!(out, "router ospf").unwrap();
        } else {
            writeln!(out, "router ospf {}", proc_id).unwrap();
        }
        if let Some(ref proc) = device.ospf {
            if let Some(rid) = proc.router_id {
                writeln!(out, " ospf router-id {}", rid).unwrap();
            }
        }
        writeln!(out, "!").unwrap();
    }

    // Router OSPFv3
    let has_ospfv3_ifaces = device.interfaces.iter().any(|i| i.ospfv3.is_some());
    if device.ospfv3.is_some() || has_ospfv3_ifaces {
        writeln!(out, "router ospf6").unwrap();
        if let Some(ref proc) = device.ospfv3 {
            if let Some(rid) = proc.router_id {
                writeln!(out, " ospf6 router-id {}", rid).unwrap();
            }
        }
        for iface in &sorted_interfaces {
            if let Some(ref ospf6) = iface.ospfv3 {
                match &ospf6.area {
                    OspfAreaId::Number(n) => {
                        writeln!(out, " interface {} area {}", iface.name, n).unwrap()
                    }
                    OspfAreaId::Ipv4(ip) => {
                        writeln!(out, " interface {} area {}", iface.name, ip).unwrap()
                    }
                }
            }
        }
        writeln!(out, "!").unwrap();
    }

    // Router IS-IS
    let has_isis_ifaces = device.interfaces.iter().any(|i| i.isis.is_some());
    if device.isis.is_some() || has_isis_ifaces {
        writeln!(out, "router isis netsim").unwrap();
        if let Some(ref proc) = device.isis {
            if let Some(ref net) = proc.net {
                writeln!(out, " net {}", net).unwrap();
            }
            match proc.level {
                Some(IsisLevel::Level1) => writeln!(out, " is-type level-1").unwrap(),
                Some(IsisLevel::Level2) => writeln!(out, " is-type level-2-only").unwrap(),
                Some(IsisLevel::Level12) => writeln!(out, " is-type level-1-2").unwrap(),
                None => {}
            }
        }
        writeln!(out, "!").unwrap();
    }

    // Router BGP
    if let Some(ref bgp) = device.bgp {
        writeln!(out, "router bgp {}", bgp.as_number).unwrap();
        if let Some(rid) = bgp.router_id {
            writeln!(out, " bgp router-id {}", rid).unwrap();
        }
        if let Some(cid) = bgp.cluster_id {
            writeln!(out, " bgp cluster-id {}", cid).unwrap();
        }

        // Peer groups
        let mut sorted_pgs = bgp.peer_groups.clone();
        sorted_pgs.sort_by(|a, b| a.name.cmp(&b.name));
        for pg in &sorted_pgs {
            writeln!(out, " neighbor {} peer-group", pg.name).unwrap();
            if let Some(rem_as) = pg.remote_as {
                writeln!(out, " neighbor {} remote-as {}", pg.name, rem_as).unwrap();
            }
            if pg.route_reflector_client == Some(true) {
                writeln!(out, " neighbor {} route-reflector-client", pg.name).unwrap();
            }
            if let Some(ref sc) = pg.send_community {
                writeln!(out, " neighbor {} send-community {}", pg.name, sc).unwrap();
            }
            if pg.bfd == Some(true) {
                writeln!(out, " neighbor {} bfd", pg.name).unwrap();
            }
        }

        // Neighbors (IPv4)
        let mut sorted_nbrs = bgp.neighbors.clone();
        sorted_nbrs.sort_by_key(|n| n.remote_address);
        for n in &sorted_nbrs {
            if let Some(ref pg) = n.peer_group {
                writeln!(out, " neighbor {} peer-group {}", n.remote_address, pg).unwrap();
            }
            if n.remote_as != 0 {
                writeln!(
                    out,
                    " neighbor {} remote-as {}",
                    n.remote_address, n.remote_as
                )
                .unwrap();
            }
            if n.route_reflector_client == Some(true) {
                writeln!(out, " neighbor {} route-reflector-client", n.remote_address).unwrap();
            }
            if let Some(ref sc) = n.send_community {
                writeln!(out, " neighbor {} send-community {}", n.remote_address, sc).unwrap();
            }
            if n.bfd == Some(true) {
                writeln!(out, " neighbor {} bfd", n.remote_address).unwrap();
            }
        }

        // Neighbors (IPv6)
        let mut sorted_nbrs_v6 = bgp.neighbors_v6.clone();
        sorted_nbrs_v6.sort_by_key(|n| n.remote_address);
        for n in &sorted_nbrs_v6 {
            if let Some(ref pg) = n.peer_group {
                writeln!(out, " neighbor {} peer-group {}", n.remote_address, pg).unwrap();
            }
            if n.remote_as != 0 {
                writeln!(
                    out,
                    " neighbor {} remote-as {}",
                    n.remote_address, n.remote_as
                )
                .unwrap();
            }
            if n.route_reflector_client == Some(true) {
                writeln!(out, " neighbor {} route-reflector-client", n.remote_address).unwrap();
            }
            if let Some(ref sc) = n.send_community {
                writeln!(out, " neighbor {} send-community {}", n.remote_address, sc).unwrap();
            }
            if n.bfd == Some(true) {
                writeln!(out, " neighbor {} bfd", n.remote_address).unwrap();
            }
        }

        // Address Family IPv4 Unicast
        let has_v4_networks = !bgp.networks.is_empty();
        if has_v4_networks {
            writeln!(out, " address-family ipv4 unicast").unwrap();
            let mut sorted_nets = bgp.networks.clone();
            sorted_nets.sort();
            for net in &sorted_nets {
                writeln!(out, "  network {}", net).unwrap();
            }
            writeln!(out, " exit-address-family").unwrap();
        }

        // Address Family IPv6 Unicast
        let has_v6_networks = !bgp.networks_v6.is_empty();
        let has_v6_neighbors = !bgp.neighbors_v6.is_empty();
        if has_v6_networks || has_v6_neighbors {
            writeln!(out, " address-family ipv6 unicast").unwrap();
            for n in &sorted_nbrs_v6 {
                writeln!(out, "  neighbor {} activate", n.remote_address).unwrap();
            }
            let mut sorted_nets_v6 = bgp.networks_v6.clone();
            sorted_nets_v6.sort();
            for net in &sorted_nets_v6 {
                writeln!(out, "  network {}", net).unwrap();
            }
            writeln!(out, " exit-address-family").unwrap();
        }

        writeln!(out, "!").unwrap();
    }

    // Community Lists
    let mut sorted_comms = device.community_lists.clone();
    sorted_comms.sort_by(|a, b| a.name.cmp(&b.name));
    for cl in &sorted_comms {
        for entry in &cl.entries {
            let act = match entry.action {
                PolicyAction::Permit => "permit",
                PolicyAction::Deny => "deny",
            };
            writeln!(
                out,
                "ip community-list standard {} {} {}",
                cl.name,
                act,
                entry.communities.join(" ")
            )
            .unwrap();
        }
    }
    if !sorted_comms.is_empty() {
        writeln!(out, "!").unwrap();
    }

    // IPv4 Prefix Lists
    let mut sorted_pls = device.prefix_lists.clone();
    sorted_pls.sort_by(|a, b| a.name.cmp(&b.name));
    for pl in &sorted_pls {
        let mut entries = pl.entries.clone();
        entries.sort_by_key(|e| e.sequence);
        for entry in &entries {
            let act = match entry.action {
                PolicyAction::Permit => "permit",
                PolicyAction::Deny => "deny",
            };
            let mut line = format!(
                "ip prefix-list {} seq {} {} {}",
                pl.name, entry.sequence, act, entry.prefix
            );
            if let Some(ref range) = entry.range {
                if let Some(ge) = range.ge {
                    line.push_str(&format!(" ge {}", ge));
                }
                if let Some(le) = range.le {
                    line.push_str(&format!(" le {}", le));
                }
            }
            writeln!(out, "{}", line).unwrap();
        }
    }
    if !sorted_pls.is_empty() {
        writeln!(out, "!").unwrap();
    }

    // IPv6 Prefix Lists
    let mut sorted_pls_v6 = device.prefix_lists_v6.clone();
    sorted_pls_v6.sort_by(|a, b| a.name.cmp(&b.name));
    for pl in &sorted_pls_v6 {
        let mut entries = pl.entries.clone();
        entries.sort_by_key(|e| e.sequence);
        for entry in &entries {
            let act = match entry.action {
                PolicyAction::Permit => "permit",
                PolicyAction::Deny => "deny",
            };
            let mut line = format!(
                "ipv6 prefix-list {} seq {} {} {}",
                pl.name, entry.sequence, act, entry.prefix
            );
            if let Some(ref range) = entry.range {
                if let Some(ge) = range.ge {
                    line.push_str(&format!(" ge {}", ge));
                }
                if let Some(le) = range.le {
                    line.push_str(&format!(" le {}", le));
                }
            }
            writeln!(out, "{}", line).unwrap();
        }
    }
    if !sorted_pls_v6.is_empty() {
        writeln!(out, "!").unwrap();
    }

    // Route Maps (Policies)
    let mut sorted_policies = device.policies.clone();
    sorted_policies.sort_by(|a, b| a.name.cmp(&b.name));
    for pol in &sorted_policies {
        let mut stmts = pol.statements.clone();
        stmts.sort_by_key(|s| s.name.parse::<u32>().unwrap_or(10));
        for stmt in &stmts {
            let seq = stmt.name.parse::<u32>().unwrap_or(10);
            let act = match stmt.actions.policy_result {
                PolicyAction::Permit => "permit",
                PolicyAction::Deny => "deny",
            };
            writeln!(out, "route-map {} {} {}", pol.name, act, seq).unwrap();
            if let Some(ref ps) = stmt.conditions.match_prefix_set {
                writeln!(out, " match ip address prefix-list {}", ps).unwrap();
            }
            if let Some(ref ps6) = stmt.conditions.match_prefix_set_v6 {
                writeln!(out, " match ipv6 address prefix-list {}", ps6).unwrap();
            }
            if let Some(ref comm) = stmt.conditions.match_community {
                writeln!(out, " match community {}", comm).unwrap();
            }
            if let Some(med) = stmt.actions.set_metric {
                writeln!(out, " set metric {}", med).unwrap();
            }
            if let Some(lp) = stmt.actions.set_local_preference {
                writeln!(out, " set local-preference {}", lp).unwrap();
            }
            if let Some(ref comm) = stmt.actions.set_community {
                writeln!(out, " set community {}", comm).unwrap();
            }
            writeln!(out, "!").unwrap();
        }
    }

    warnings.sort();
    warnings.dedup();

    (out, warnings)
}

/// Emit both `frr.conf` and `daemons` configuration.
pub fn emit_frr(device: &DeviceConfig) -> Result<FrrEmitResult, EmitFrrError> {
    let (conf, warnings) = emit_frr_conf(device);
    let daemons = emit_frr_daemons(device);
    Ok(FrrEmitResult {
        conf,
        daemons,
        warnings,
    })
}

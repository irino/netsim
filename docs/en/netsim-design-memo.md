# Network Simulator (netsim) Design Memo v0.2

Created: 2026-10-26 / v0.2 Updated: 2026-10-27
Concept: Parse vendor network configurations (Cisco IOS, Junos, Arista EOS, Cisco IOS XR, FRRouting, etc.), translate them into zebra-rs configuration, and run realistic routing simulations (emulations) on Containerlab.

Changes in v0.2: Adopted the policy of establishing the Intermediate Representation (IR) on standard models (IETF YANG) as a formal decision. Prioritized zebra-rs as the execution engine. Clarified the scope of reusing Holo: Holo is not used as an execution engine, but rather as a reference for authoritative YANG models, schema validation, and modular structure (Section 2.5).

---

## 1. Overall Architecture

```
[Vendor Configs (IOS / Junos / EOS / IOS XR / FRR)]  (Multiple devices)
        │
        ▼  Parsers (Per-vendor, Rust with winnow)
[In-house IR: Vendor-Independent Model]  <-- serde structs, exportable to JSON
        │
        ├──▼ Topology Inference (Link & Multi-access LAN extraction)
        ├──▼ zebra-rs YAML Emitter ──→ Each node's zebra.yaml
        └──▼ Containerlab Emitter ──→ topology.clab.yml
        │
        ▼  containerlab deploy
[zebra-rs containers establish realistic protocol adjacencies & route exchange]
        │
        ▼  Verification
  Inspect state with show ip ospf neighbor / show ip route / ping
```

Key points:
- Translation and topology inference are separate modules.
- Rather than a mathematical abstraction, this is live daemon emulation; the result reflects genuine protocol adjacencies and route convergence, with the trade-off that features not implemented in zebra-rs cannot be emulated.

## 2. Design Decisions

### 2.1 An Intermediate Representation (IR) is Essential (Highest Priority)
Avoid direct translation from vendor syntax to zebra-rs. Like Batfish's multi-stage pipeline ("vendor-specific model -> vendor-independent model"), netsim utilizes an IR. This decouples parsing from code generation: adding a new vendor requires only adding a new parser, and supporting a new target platform requires only adding a new emitter.

### 2.2 Translation Target is zebra-rs YAML
- zebra-rs configuration is declarative; CLI, YAML, and JSON represent alternative views of the same underlying YANG tree and can round-trip cleanly. Machine-generated configuration targets YAML and is applied idempotently via `vtyctl apply -f zebra.yaml`.
- zebra-rs runs as a single process/binary, matching the container 1-process model cleanly. In Containerlab, each node runs under `kind: linux` mounting the generated `zebra.yaml` into the zebra-rs container.
- Note: YAML keys and hierarchy correspond to zebra-rs schema definitions.

### 2.3 Batfish as Specification and Test Reference, Not a Full Port
Batfish grammars (ANTLR4) and test configurations serve as reference specifications for supported syntax. A complete port would take years and is out of scope; only targeted subsets are implemented in Rust. ANTLR's unofficial Rust target is not used.

### 2.4 Do Not Discard Unsupported Lines Silently
Parsers must collect unhandled lines and blocks as an unsupported lines report (`UnparsedLine`: filename, line number, line content, and reason). Real-world configurations often contain unsupported statements; without explicit diagnostics, the fidelity of the simulation cannot be judged.

### 2.5 Align with Standard Models (IETF YANG) / Scope of Holo Reuse [v0.2 Decision]
- While remaining in-house `serde` structs, the IR aligns its structure, naming, and JSON serialization with IETF YANG models: `ietf-interfaces`, `ietf-ip`, `ietf-routing`, `ietf-ospf` (RFC 9129), and `ietf-routing-policy` (RFC 9067). Maintain mapping tables (vendor command | IR | IETF path | zebra-rs). OpenConfig is retained only as an optional column for future I/O adapters, not as the primary IR schema.
- **zebra-rs is prioritized** as the execution engine (due to its native focus on SRv6, EVPN, L3VPN, etc.). Note: Holo is also a single process (`holod`) communicating via an internal event bus (IBus / Tokio channels); process architecture does not differentiate them.
- Reusing Holo is strictly limited to three aspects:
  1. **Authoritative YANG Models**: Embedded IETF YANG modules in `holo-yang` serve as references for IR design and schema validation.
  2. **Schema Validation**: Automated tests use `yang3` (libyang bindings) to validate IR JSON output against standard IETF models, detecting schema drift.
  3. **Architectural Patterns**: Protocol-specific crate modularity and northbound structure serve as design references.
- What is NOT reused from Holo: generated types from northbound code generation (which require callback/lifetime assumptions not suitable for an independent IR) and the `holod` runtime itself.

## 3. IR Data Models (Core Subset)

```rust
pub struct DeviceConfig {
    pub hostname: String,
    pub interfaces: Vec<Interface>,
    pub ospf: Option<OspfProcess>,
    pub ospf6: Option<Ospf6Process>,
    pub bgp: Option<BgpProcess>,
    pub static_routes: Vec<StaticRoute>,
    pub ipv6_static_routes: Vec<Ipv6StaticRoute>,
    pub prefix_lists: Vec<PrefixList>,
    pub unparsed_lines: Vec<UnparsedLine>,
}

pub struct Interface {
    pub name: String,
    pub description: Option<String>,
    pub ipv4: Option<Ipv4AddrMask>,
    pub ipv6: Vec<Ipv6Prefix>,
    pub shutdown: bool,
    pub ospf_area: Option<OspfAreaId>,
    pub ospf_cost: Option<u16>,
    pub ospf_passive: Option<bool>,
}

pub struct OspfProcess {
    pub process_id: u16,
    pub router_id: Option<Ipv4Addr>,
}
```

Design notes:
- Vendor network statements (such as Cisco IOS `network 10.0.0.0 0.0.0.255 area 0`) are semantically resolved at parse time to the matching interfaces and stored in `Interface.ospf_area`, matching Junos `protocols ospf area 0 interface ge-0/0/0` and keeping the IR vendor-independent.
- Interface names are preserved in vendor format in the IR; mapping to Containerlab container interfaces (`eth1`, `eth2`, ...) is handled deterministically during emission.
- Field names and hierarchy mirror IETF YANG paths.

## 4. Topology Inference

- Calculate subnet network addresses from interface IP addresses and masks across all devices; interfaces in the same subnet form link candidates.
- Point-to-point links: Exactly 2 interfaces sharing a subnet.
- Multi-access LAN segments: 3 or more interfaces sharing a subnet. Modeled by instantiating a Containerlab `kind: bridge` node (`br-lan0`, `br-lan1`, etc.) and connecting each participating router interface to the bridge. If multiple interfaces on the same device share a subnet, emit a warning and skip ambiguous inference.
- Dual-stack deduplication: IPv4 and IPv6 subnets on the same physical link between peers are merged into a single Containerlab connection.
- Shutdown, unaddressed, and loopback interfaces are excluded from link generation.

## 5. Parser Strategy: winnow over pest

| Feature | pest | winnow |
|---|---|---|
| Approach | Grammar file (`.pest`, PEG) with code generator | Combinator library composed in Rust functions (fork of nom) |
| Output | Generic parse tree (Pair tree); manual AST conversion required | Functions directly return typed IR/AST structures |
| Best For | Strictly declarative languages and clean DSLs | Procedural logic, state/indentation handling, and rich error recovery |

Why winnow is chosen for vendor configurations:
1. Cisco IOS uses indentation-sensitive blocks not easily expressed in pure PEG grammars. winnow allows a two-stage approach: grouping indented blocks and parsing lines with modular combinators.
2. Error recovery (preserving unrecognized lines in `UnparsedLine` and continuing) is trivial with line-oriented combinators, whereas grammar generators often abort or require complex recovery rules.
3. Bypasses intermediate parse trees, constructing IR structs directly.
4. Set-format Junos (`set protocols ...`) is line-oriented and maps cleanly to winnow combinators.

## 6. MVP Scope and Implemented Milestones

### Phase 0: Spike & Verification
- Validated two-node zebra-rs container deployment on Containerlab (`kind: linux`).
- Verified YAML configuration roundtrip and fixed `emit-zebra` output conventions.

### Phase 1: MVP Core
- Cisco IOS parser: `hostname`, `interface` (IP, description, shutdown), `router ospf` (router-id, network statements).
- IR models and subnet-based topology inference.
- Emitters: `emit-zebra` and `emit-clab`.
- Unparsed lines diagnostics report.

### Phase 2: Static Routing & BGP Basics
- Static IPv4/IPv6 routes and BGP basics (`router bgp`, `address-family`, neighbors, network statements).

### Phase 3: Multi-Vendor Expansion
- Added Junos (`crates/parse-junos`), Arista EOS (`crates/parse-eos`), Cisco IOS XR (`crates/parse-iosxr`), and FRRouting / SONiC (`crates/parse-frr`).
- Multi-access LAN bridge inference and Containerlab bridge nodes.
- Automated IETF YANG validation with `yang3` and `holo-yang` in `crates/validate-yang`.
- End-to-end IPv4 and IPv6 dual-stack support across all parsers, IR, topology inference, and emitters.

## 7. Cargo Workspace Structure

```
netsim/
├── crates/
│   ├── ir/             # Vendor-independent models (serde, no external parser dependencies)
│   ├── parse-ios/      # Cisco IOS parser (winnow)
│   ├── parse-junos/    # Junos set-format parser (winnow)
│   ├── parse-eos/      # Arista EOS parser (winnow)
│   ├── parse-iosxr/    # Cisco IOS XR parser (winnow)
│   ├── parse-frr/      # FRRouting / SONiC parser (winnow)
│   ├── topology/       # Topology and link/LAN inference
│   ├── emit-zebra/     # zebra-rs YAML configuration emitter
│   ├── emit-clab/      # Containerlab topology emitter
│   ├── netsim-cli/     # Unified CLI (convert, gen-lab)
│   └── validate-yang/  # Automated IETF YANG schema validation
├── tests/              # Golden tests and integration pipelines
└── examples/           # Multi-vendor sample configurations
```

Unified CLI interface:
```bash
netsim convert --vendor <ios|junos|eos|iosxr|frr> --in <FILE> --out <FILE>
netsim gen-lab --configs <DIR> --out <DIR> [--lab-name <NAME>]
```

## 8. Testing Strategy

- Golden Tests: Input config snippets and expected IR / YAML pairs checked via `cargo test`.
- Diagnostics Regression: Unparsed line counts and contents are checked in snapshot tests to guarantee parser improvements do not regress.
- Schema Validation: RFC 7951 JSON generated from IR is validated against pinned IETF YANG models using `libyang` (`yang3`).
- End-to-end integration tests execute full pipeline conversions across multi-vendor networks (up to 5 vendors).

## 9. Operational Considerations

- Emulation Fidelity: The fidelity of the emulation matches zebra-rs protocol capabilities. Unsupported runtime knobs are surfaced through explicit `EmitWarning` diagnostics.
- Licensing: zebra-rs is AGPLv3. netsim remains an independent translation tool.
- Interface Mappings: Vendor interface names are mapped deterministically to container interfaces (`eth1`, `eth2`, ...) and must stay aligned with `zebra.yaml`.

---

## 10. Proposed Roadmap for Future Iterations

### Phase 1: Routing Policy & Route Maps / Policy Statements (Next Priority)
Currently, prefix lists are parsed into the IR for several vendors, but `emit-zebra` outputs an `EmitWarning` because zebra-rs routing policy schema integration is deferred.

- **Routing Policy IR & IPv6 Prefix Lists**:
  - Model `PolicyDefinition` / `RouteMap` in `crates/ir` conforming to `ietf-routing-policy` (RFC 9067).
  - Extend `PrefixList` support to IPv6 (`ipv6 prefix-list` on Cisco IOS, Arista EOS, FRR, and Junos `prefix-list` under `inet6`).
- **Vendor Policy Parsers**:
  - Cisco IOS / Arista EOS / FRR: `route-map <NAME> {permit|deny} <SEQ>` (`match ip address prefix-list`, `set metric`, `set local-preference`, etc.).
  - Cisco IOS XR: `route-policy <NAME>` (RPL language).
  - Junos: `policy-options policy-statement <NAME> term <TERM>` (`from prefix-list`, `then accept/reject`).
- **zebra-rs Policy Emitter**:
  - Map IR route policies into zebra-rs top-level `prefix-set` and `policy` YAML structures.

### Phase 2: Automated Lab Deployment & Convergence Verification (`netsim verify`)
Currently, `netsim gen-lab` generates artifacts, but deploying with Containerlab and checking convergence requires manual CLI commands.

- **Automated Emulation Runner**:
  - Add a subcommand (e.g., `netsim verify --lab-dir ./lab/` or `netsim test`):
    - Executes `containerlab deploy -t topology.clab.yml`.
    - Polls `vtyctl show ip route` / `vtyctl show ipv6 route` via Docker/container exec until routing protocols converge or timeout occurs.
    - Runs data-plane ping tests across inferred device loopbacks (both IPv4 & IPv6).
    - Automatically executes `containerlab destroy`.
- **Reachability Matrix & Intent Assertion**:
  - Allow users to specify an `intent.yml` (e.g., `assert: ping r1 -> r5 loopback`) to validate that network emulation meets operational intent.

### Phase 3: IS-IS Protocol & Advanced Routing Knobs
- **IS-IS Routing Protocol**:
  - Many service provider and modern data center topologies use IS-IS as the IGP instead of OSPF.
  - Add IS-IS models in `crates/ir` (`IsisProcess`, Level-1/Level-2, NET / ISO system ID, interface metrics).
  - Extend vendor parsers and zebra-rs YAML emission for IS-IS.
- **Advanced BGP Enhancements**:
  - BGP Route Reflectors (`neighbor ... route-reflector-client`).
  - BGP Communities / Extended Communities (`send-community`, `community-list`).
  - BGP Peer Groups / Templates for scalable configuration parsing.
  - BFD (Bidirectional Forwarding Detection) integration aligned with `ietf-bfd`.

### Phase 4: Multi-Engine & Hybrid Verification Backends
- **Alternative Runtime Engine Emitters**:
  - While zebra-rs is the primary target, adding an `emit-frr` backend would allow running emulation labs with upstream FRR container images for features not yet implemented in zebra-rs.
- **Hybrid Static + Emulation Verification**:
  - Fast static mathematical reachability checking prior to Containerlab deployment (e.g., exporting to Batfish vendor-independent format or a lightweight reachability graph solver).

### Phase 5: Overlay Networking & Modern Fabrics (EVPN / SRv6)
- **VXLAN / EVPN L2/L3 Fabrics**:
  - Parsing and modeling VTEP / NVE interfaces and BGP EVPN (AFI 25 / SAFI 70).
- **Segment Routing (SRv6 / SR-MPLS)**:
  - Aligning with zebra-rs's core strengths in SRv6 micro-segments (uSID) and programmable data planes.

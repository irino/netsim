# netsim

A Rust-based multi-vendor network configuration translator and network emulation generator.

## Project Workspace Structure

- `crates/ir`: Vendor-independent Intermediate Representation data models (`DeviceConfig`, `Interface`, `OspfProcess`, `UnparsedLine`, `Ipv4Prefix`, `PrefixLength`, `OspfAreaId`) aligned with IETF YANG models.
- `crates/parse-ios`: Cisco IOS configuration parser built with `winnow`.
- `crates/parse-junos`: Junos set-format configuration parser built with `winnow`.
- `crates/parse-eos`: Arista EOS configuration parser built with `winnow`.
- `crates/parse-iosxr`: Cisco IOS XR configuration parser built with `winnow`.
- `crates/parse-frr`: FRRouting / SONiC configuration parser built with `winnow`.
- `crates/topology`: Network topology inference engine calculating point-to-point links and warnings across devices.
- `crates/emit-zebra`: zebra-rs YAML configuration emitter conforming to authoritative zebra-rs YANG/YAML schema.
- `crates/emit-clab`: Containerlab topology generator emitting `*.clab.yml` definitions with deterministic interface mappings.
- `crates/netsim-cli`: Unified command-line interface providing `convert` and `gen-lab` subcommands.
- `crates/validate-yang`: Automated IETF YANG schema validation and drift testing engine backed by `libyang` (`yang3` and `holo-yang`).
- `examples/`: Synthetic example configurations demonstrating multi-device network topologies (e.g., `examples/ospf-3routers`).

## Step-by-Step Usage Guide

### Prerequisites

- **Rust toolchain**: 1.80+ (`rustup default stable`)
- **Containerlab**: (optional, for deploying emulation environments) [Install guide](https://containerlab.dev/install/)
- **Docker**: (optional, required by Containerlab)
- **zebra-rs container image**: `zebra-rs:latest` built or pulled locally.

---

### 1. Translating a Single Configuration (`convert`)

To translate a single Cisco IOS or Junos configuration into a declarative zebra-rs YAML configuration, use the `convert` subcommand:

#### Cisco IOS Example:
```bash
cargo run --bin netsim -- convert \
  --vendor ios \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.zebra.yaml
```

#### Junos (set-format) Example:
```bash
cargo run --bin netsim -- convert \
  --vendor junos \
  --in examples/mixed-ios-junos/junos-r2.set \
  --out junos-r2.zebra.yaml
```

#### Arista EOS Example:
```bash
cargo run --bin netsim -- convert \
  --vendor eos \
  --in examples/mixed-3vendors/arista-r3.cfg \
  --out arista-r3.zebra.yaml
```

#### Cisco IOS XR Example:
```bash
cargo run --bin netsim -- convert \
  --vendor iosxr \
  --in examples/mixed-4vendors/iosxr-r4.cfg \
  --out iosxr-r4.zebra.yaml
```

#### FRRouting / SONiC Example:
```bash
cargo run --bin netsim -- convert \
  --vendor frr \
  --in examples/mixed-5vendors/frr-r5.cfg \
  --out frr-r5.zebra.yaml
```

**Options**:
- `--vendor <ios|junos|eos|iosxr|frr>`: Input vendor configuration format (`ios`, `junos`, `eos`, `iosxr`, or `frr`; default: `ios`).
- `--in <FILE>`: Path to the input vendor configuration file.
- `--out <FILE>`: Destination path for the generated zebra-rs YAML configuration.

Any unparsed commands or emitter warnings are printed to `stderr` with line numbers and reasons.

---

### 2. Generating Complete Emulation Labs (`gen-lab`)

The `gen-lab` command processes a folder containing multiple router configurations (`.cfg`, `.set`, `.conf`, or `.txt`), automatically identifies the vendor for each file, infers point-to-point links between subnets, emits all node configurations, builds a Containerlab topology definition, and produces a diagnostic report.

#### Running on Homogeneous or Mixed-Vendor Topologies:

```bash
# 3-router Cisco IOS OSPF triangle
cargo run --bin netsim -- gen-lab \
  --configs ./examples/ospf-3routers/ \
  --out ./lab-ospf/ \
  --lab-name ospf-lab

# 2-router Cisco IOS eBGP peering
cargo run --bin netsim -- gen-lab \
  --configs ./examples/ebgp-2routers/ \
  --out ./lab-ebgp/ \
  --lab-name ebgp-lab

# Mixed Cisco IOS and Junos multi-vendor topology
cargo run --bin netsim -- gen-lab \
  --configs ./examples/mixed-ios-junos/ \
  --out ./lab-mixed/ \
  --lab-name mixed-lab

# Mixed 3-vendor Cisco IOS, Junos, and Arista EOS topology
cargo run --bin netsim -- gen-lab \
  --configs ./examples/mixed-3vendors/ \
  --out ./lab-mixed/ \
  --lab-name mixed-lab

# Mixed 5-vendor Cisco IOS, Junos, Arista EOS, Cisco IOS XR, and FRRouting topology
cargo run --bin netsim -- gen-lab \
  --configs ./examples/mixed-5vendors/ \
  --out ./lab-mixed/ \
  --lab-name mixed-lab
```

**Options**:
- `--configs <DIR>`: Directory containing router configuration files.
- `--out <DIR>`: Output directory for generated artifacts.
- `--lab-name <NAME>`: Name of the Containerlab topology (defaults to `netsim-lab`).

#### Generated Output Structure:
```
lab-mixed/
├── cisco-r1.zebra.yaml       # zebra-rs YAML configuration for cisco-r1
├── junos-r2.zebra.yaml       # zebra-rs YAML configuration for junos-r2
├── topology.clab.yml         # Containerlab topology file
└── report.md                 # Diagnostic markdown report
```

---

### 3. Deploying and Verifying with Containerlab

Once the lab files are generated, deploy the environment and inspect live routing protocols:

#### Step 1: Deploy the Lab
```bash
sudo containerlab deploy -t lab-mixed/topology.clab.yml
```

#### Step 2: Verify Protocol Adjacency & Routing State
Use `docker exec` to access `vtyctl` within any router container:

```bash
# Inspect OSPF neighbors on cisco-r1:
docker exec -it clab-mixed-lab-cisco-r1 vtyctl show ip ospf neighbor

# Inspect routing tables:
docker exec -it clab-mixed-lab-cisco-r1 vtyctl show ip route

# Inspect OSPF state on junos-r2:
docker exec -it clab-mixed-lab-junos-r2 vtyctl show ip ospf interface
```

#### Step 3: Test Data-Plane Connectivity
```bash
# Ping junos-r2 loopback (2.2.2.2) from cisco-r1:
docker exec -it clab-mixed-lab-cisco-r1 ping -c 3 2.2.2.2
```

#### Step 4: Destroy the Lab
```bash
sudo containerlab destroy -t lab-mixed/topology.clab.yml
```

---

### 4. Automated Deployment & Verification (`verify` / `test`)

The `verify` command (aliased to `test`) automates the entire lifecycle:
1. Deploys the Containerlab topology (`containerlab deploy -t topology.clab.yml`).
2. Polls `vtyctl show ip route` / `vtyctl show ipv6 route` on all nodes until routing protocols converge (or timeout).
3. Executes automated data-plane ping tests across all inferred device loopbacks (both IPv4 & IPv6).
4. Validates operational assertions defined in `intent.yml` (e.g. `assert: ping r1 -> r5 loopback`).
5. Automatically tears down the lab (`containerlab destroy -t topology.clab.yml --cleanup`).

#### Example Usage:
```bash
# Verify generated lab in ./lab-ospf/ with automated convergence check and loopback pings
cargo run --bin netsim -- verify --lab-dir ./lab-ospf/

# Verify with custom timeout and an explicit intent file
cargo run --bin netsim -- verify \
  --lab-dir ./lab-ospf/ \
  --intent ./examples/ospf-3routers/intent.yml \
  --timeout 90

# Retain lab after verification (skip containerlab destroy)
cargo run --bin netsim -- verify --lab-dir ./lab-ospf/ --skip-destroy
```

#### Intent Assertion Specification (`intent.yml`):
```yaml
# Single assertion shorthand:
assert: ping R1 -> R3 loopback

# Multiple assertions:
assertions:
  - ping R2 -> R1 loopback
  - ping R3 -> R2 loopback
  - ping R1 -> 1.1.1.1
  - route R1 has 10.0.23.0/24
```

## IETF YANG Schema Validation & Drift Testing (`crates/validate-yang`)

The project aligns its vendor-independent IR (`crates/ir`) with standard IETF YANG data models. `crates/validate-yang` provides automated validation against pinned upstream YANG schemas using `yang3` (libyang bindings) and `holo-yang`:

### Pinned Upstream YANG Modules & Revisions
- `ietf-interfaces@2018-02-20` (RFC 8343: Interface Management)
- `ietf-ip@2018-02-22` (RFC 8344: IP Management)
- `ietf-routing@2018-03-13` (RFC 8349 NMDA: Routing Management)
- `ietf-ipv4-unicast-routing@2018-03-13` (RFC 8349 NMDA: IPv4 Unicast Routing)
- `ietf-ospf@2022-10-19` (RFC 9129: OSPF Protocol)
- `ietf-bgp@2023-07-05` (BGP-4 Model)
- `iana-if-type@2017-01-19` (RFC 7224: IANA Interface Types)
- `iana-routing-types@2018-10-29` (RFC 8294: IANA Routing Types)

All modules are vendored/embedded through `holo-yang`, avoiding unauthenticated or flaking external network calls during build and test execution.

### IR to RFC 7951 Mapping & Limitations
`validate_device_config` maps netsim IR instances into standard YANG JSON:
- **Interfaces**: Mapped to `ietf-interfaces:interfaces/interface[]` with `name`, `type` (`iana-if-type`), `enabled`, `description`, and `ietf-ip:ipv4/address[]`.
- **OSPF**: Mapped under `ietf-routing:routing/control-plane-protocols/control-plane-protocol[type='ietf-ospf:ospfv2']` with `explicit-router-id` and `areas/area[]/interfaces/interface[]`.
- **BGP**: Mapped under `ietf-routing:routing/control-plane-protocols/control-plane-protocol[type='ietf-bgp:bgp']` with `global` (`as`, `identifier`) and `neighbors/neighbor[]` (`remote-address`, `peer-as`).
- **Static Routes**: Mapped under `ietf-routing:routing/control-plane-protocols/control-plane-protocol[type='ietf-routing:static']` with `static-routes/ietf-ipv4-unicast-routing:ipv4/route[]` (`destination-prefix`, `next-hop/next-hop-address`).
- **Uncovered Fields / Limitations**:
  - `hostname`: Modeled under `ietf-system` (RFC 7317), not `ietf-interfaces` or `ietf-routing`.
  - `bgp.networks`: BGP prefix origination/export in IETF BGP is governed by `ietf-routing-policy` (RFC 9067), which is outside current parser and IR scope.
  - `unparsed_lines`: Diagnostic syntax preservation metadata without a direct IETF YANG equivalent.

### Schema Drift Tests
The test suite in `crates/validate-yang` validates:
1. Valid IR conversions against golden samples (`device_config_full`, `cisco_ios_basic`, `junos_basic`, multi-protocol configurations).
2. Schema constraint failures (invalid IPv4 addresses, out-of-range prefix lengths $>32$, invalid OSPF area-ids).
3. Mandatory missing node detection (`name`, `type`).
4. Detection of schema drift or unexpected data model mutations.

## CI & Quality Checks

This repository uses GitHub Actions CI to ensure code quality. Every pull request targeting `main` and pushes to `main` must pass the automated checks:

- `cargo fmt --all -- --check` (Code formatting)
- `cargo test --workspace` (Unit, Golden, and Integration tests)
- `cargo clippy --workspace --all-targets -- -D warnings` (Clippy lint checks)

### Running Checks Locally

Contributors can run the same checks locally prior to submitting pull requests:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Supported Cisco IOS Syntax (`crates/parse-ios`)

The initial Cisco IOS parser supports:

- **Hostname**: `hostname <NAME>`
- **Interfaces**:
  - `interface <NAME>` (e.g., `interface GigabitEthernet0/0`)
  - Interface description: `description <TEXT>`
  - IPv4 address and subnet mask: `ip address <A.B.C.D> <NETMASK>`
  - Shutdown / No shutdown: `shutdown`, `no shutdown`
- **OSPF Protocol**:
  - Process creation: `router ospf <PROCESS_ID>`
  - Router ID: `router-id <A.B.C.D>`
  - Network statements: `network <ADDRESS> <WILDCARD> area <AREA>` (supports integer and dotted-quad area IDs, as well as non-contiguous wildcard mask bitwise matching).

Unrecognized or unsupported lines are preserved in `UnparsedLine` structs recording filename, 1-based line number, original content, and reason.

## Supported Arista EOS Syntax (`crates/parse-eos`)

The Arista EOS parser supports:

- **Hostname**: `hostname <NAME>`
- **Interfaces**:
  - `interface <NAME>` (e.g., `interface Ethernet1`, `interface Loopback0`)
  - Description: `description <TEXT>`
  - IPv4 address in CIDR or subnet mask format: `ip address <A.B.C.D/LEN>` or `ip address <A.B.C.D> <NETMASK>`
  - Admin state: `shutdown`, `no shutdown`
  - Interface-level OSPF: `ip ospf area <AREA>`, `ip ospf cost <COST>`
- **Static Routes**:
  - CIDR and mask syntax: `ip route <DEST/LEN> <NEXT_HOP>` and `ip route <DEST> <NETMASK> <NEXT_HOP>`
- **OSPF Protocol**:
  - Process creation: `router ospf <PROCESS_ID>`
  - Router ID: `router-id <A.B.C.D>`
  - Network statements: `network <PREFIX/LEN> area <AREA>` and `network <ADDRESS> <WILDCARD> area <AREA>`
- **BGP Protocol**:
  - Process creation: `router bgp <AS>`
  - Router ID: `router-id <A.B.C.D>`
  - Neighbor: `neighbor <A.B.C.D> remote-as <AS>`
  - Network: `network <PREFIX/LEN>` and `network <ADDRESS> mask <NETMASK>`
- **Prefix Lists**:
  - `ip prefix-list <NAME> [seq <SEQ>] {permit|deny} <PREFIX/LEN> [ge <GE>] [le <LE>]`

Unrecognized or unsupported lines are preserved in `UnparsedLine` structs recording filename, 1-based line number, original content, and reason.

## Supported Cisco IOS XR Syntax (`crates/parse-iosxr`)

The Cisco IOS XR parser supports:

- **Hostname**: `hostname <NAME>`
- **Interfaces**:
  - `interface <NAME>` (e.g., `interface GigabitEthernet0/0/0/0`, `interface Loopback0`)
  - Description: `description <TEXT>`
  - IPv4 address in CIDR or netmask format: `ipv4 address <A.B.C.D/LEN>` or `ipv4 address <A.B.C.D> <NETMASK>`
  - Admin state: `shutdown`, `no shutdown`
- **Static Routes**:
  - `router static` with `address-family ipv4 unicast` destinations and next-hops: `<DEST/LEN> <NEXT_HOP>`
- **OSPF Protocol**:
  - `router ospf <PROCESS_ID>`
  - Router ID: `router-id <A.B.C.D>`
  - Hierarchical areas: `area <AREA>`
  - Nested interfaces within area: `interface <NAME>` (`cost <COST>`, `passive enable`)
- **BGP Protocol**:
  - `router bgp <AS>`
  - Router ID: `bgp router-id <A.B.C.D>`
  - Peer configuration: `neighbor <A.B.C.D>` (`remote-as <AS>`)
  - Network advertisements: `address-family ipv4 unicast` (`network <PREFIX/LEN>`)
- **Prefix Sets**:
  - `prefix-set <NAME>` with prefix entries (`<PREFIX/LEN> [ge <GE>] [le <LE>]`) terminated by `end-set`

Unrecognized or unsupported lines are preserved in `UnparsedLine` structs recording filename, 1-based line number, original content, and reason.

## Supported FRRouting / SONiC Syntax (`crates/parse-frr`)

The FRRouting / SONiC parser supports:

- **Hostname**: `hostname <NAME>`
- **Interfaces**:
  - `interface <NAME>` (e.g., `interface eth0`, `interface swp1`, `interface lo`)
  - Description: `description <TEXT>`
  - IPv4 address: `ip address <A.B.C.D/LEN>` or `ip address <A.B.C.D> <NETMASK>`
  - Admin state: `shutdown`, `no shutdown`
  - Interface-level OSPF: `ip ospf area <AREA>`, `ip ospf cost <COST>`, `ip ospf passive`
- **Static Routes**:
  - `ip route <DEST/LEN> <NEXT_HOP>` and `ip route <DEST> <NETMASK> <NEXT_HOP>`
- **OSPF Protocol**:
  - `router ospf`
  - Router ID: `ospf router-id <A.B.C.D>` or `router-id <A.B.C.D>`
  - Network statements: `network <PREFIX/LEN> area <AREA>` and `network <ADDRESS> <WILDCARD> area <AREA>`
- **BGP Protocol**:
  - `router bgp <AS>`
  - Router ID: `bgp router-id <A.B.C.D>`
  - Neighbor: `neighbor <A.B.C.D> remote-as <AS>`
  - Network: `network <PREFIX/LEN>` (top-level or within `address-family ipv4 unicast`)
- **Prefix Lists**:
  - `ip prefix-list <NAME> [seq <SEQ>] {permit|deny} <PREFIX/LEN> [ge <GE>] [le <LE>]`

Unrecognized or unsupported lines are preserved in `UnparsedLine` structs recording filename, 1-based line number, original content, and reason.

## Supported Junos Syntax (`crates/parse-junos`)

The Junos set-format parser supports:

- **Hostname**: `set system host-name <NAME>`
- **Interfaces**:
  - `set interfaces <NAME> description <TEXT>`
  - `set interfaces <NAME> disable`
  - `set interfaces <NAME> unit <UNIT> description <TEXT>`
  - `set interfaces <NAME> unit <UNIT> disable`
  - `set interfaces <NAME> unit <UNIT> family inet address <IP/LEN>`
- **Routing Options**:
  - Router ID: `set routing-options router-id <A.B.C.D>`
  - Autonomous System: `set routing-options autonomous-system <AS>`
  - Static Routes: `set routing-options static route <DEST/LEN> next-hop <NEXT_HOP>`
- **OSPF Protocol**:
  - Interface participation: `set protocols ospf area <AREA> interface <NAME>`
  - Interface options: `metric <COST>`, `passive`
- **BGP Protocol**:
  - Peer configuration: `set protocols bgp group <GROUP> neighbor <A.B.C.D> peer-as <AS>`
  - Local AS: `set protocols bgp [group <GROUP>] local-as <AS>`
  - Route export filters: `set policy-options policy-statement ... term ... from route-filter <PREFIX> exact`

Unrecognized or unsupported lines are preserved in `UnparsedLine` structs recording filename, 1-based line number, original content, and reason.

## zebra-rs YAML Configuration Emitter (`crates/emit-zebra`)

`crates/emit-zebra` converts `ir::DeviceConfig` into zebra-rs declarative YAML configurations based on the schema documented at https://zebra.rs/:

- **System**:
  - `system.hostname`
- **Interfaces**:
  - `interface[].name`
  - `interface[].ipv4.address[]` (`A.B.C.D/prefix`)
- **OSPFv2**:
  - `router.ospf.router-id`
  - `router.ospf.area[].area-id`
  - `router.ospf.area[].interface[].name`
  - `router.ospf.area[].interface[].enabled`
  - `router.ospf.area[].interface[].cost`
  - `router.ospf.area[].interface[].passive`

### Schema Restrictions & Diagnostics
zebra-rs binds configuration to existing kernel devices and models protocol participation declaratively:
- **Interface descriptions**: Not representable in the top-level zebra-rs `interface` schema; emitted as diagnostic `interface-description` warnings.
- **Shutdown / Admin-Down state**: zebra-rs top-level interfaces do not have an administrative-down state knob; emitted as diagnostic `interface-disabled` warnings.
- **Multiple OSPF process IDs**: zebra-rs configures single-instance OSPF under `router.ospf`; process IDs $\ne 1$ emit `ospf-process-id` warnings.
- **Unparsed lines**: All unparsed lines from upstream parsers are propagated through `EmitWarning` diagnostics so no configuration is silently discarded.

## Containerlab Topology Emitter (`crates/emit-clab`)

`crates/emit-clab` produces Containerlab `*.clab.yml` topology definition files:
- Emits nodes under `kind: linux` mounting each node's generated `zebra.yaml` configuration.
- Deterministically maps vendor interfaces (e.g. `GigabitEthernet0/0`) to container Linux interfaces (`eth1`, `eth2`, ...) alphabetically while excluding loopback interfaces.
- Interconnects inferred point-to-point links using brief endpoint syntax `endpoints: ["nodeA:eth1", "nodeB:eth2"]`.
- Retains deterministic ordering for nodes and links.
- Supports multi-access LAN segments (where 3 or more router interfaces share a subnet) by creating Containerlab `kind: bridge` nodes (`br-lan0`, `br-lan1`, etc.) and connecting all participating routers.
- Explicitly surfaces topology inference warnings (e.g., multiple interfaces on the same device sharing a subnet).

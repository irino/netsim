# netsim

A Rust-based multi-vendor network configuration translator and network emulation generator.

## Project Workspace Structure

- `crates/ir`: Vendor-independent Intermediate Representation data models (`DeviceConfig`, `Interface`, `OspfProcess`, `UnparsedLine`, `Ipv4Prefix`, `PrefixLength`, `OspfAreaId`) aligned with IETF YANG models.
- `crates/parse-ios`: Cisco IOS configuration parser built with `winnow`.
- `crates/parse-junos`: Junos set-format configuration parser built with `winnow`.
- `crates/topology`: Network topology inference engine calculating point-to-point links and warnings across devices.
- `crates/emit-zebra`: zebra-rs YAML configuration emitter conforming to authoritative zebra-rs YANG/YAML schema.
- `crates/emit-clab`: Containerlab topology generator emitting `*.clab.yml` definitions with deterministic interface mappings.
- `crates/netsim-cli`: Unified command-line interface providing `convert` and `gen-lab` subcommands.
- `examples/`: Synthetic example configurations demonstrating multi-device network topologies (e.g., `examples/ospf-3routers`).

## CLI Usage (`crates/netsim-cli`)

### 1. `convert`
Converts a single vendor configuration file into a zebra-rs YAML configuration:

```bash
cargo run --bin netsim -- convert --vendor ios --in R1.cfg --out R1.zebra.yaml
```

Options:
- `--vendor <ios|junos>`: Input vendor configuration format (defaults to `ios`).
- `--in <FILE>`: Path to the input vendor configuration.
- `--out <FILE>`: Path to the destination zebra-rs YAML configuration.

### 2. `gen-lab`
Processes a directory of vendor configuration files, infers the interconnecting topology, and outputs all zebra-rs configuration files, a Containerlab topology definition, and a diagnostic markdown report:

```bash
cargo run --bin netsim -- gen-lab --configs ./examples/ospf-3routers/ --out ./lab/ --lab-name ospf-lab
```

Outputs generated in `--out`:
- `<hostname>.zebra.yaml`: Node configuration for each parsed router.
- `topology.clab.yml`: Containerlab topology file with brief point-to-point interface bindings.
- `report.md`: Markdown summary detailing processed devices, inferred links, topology warnings, and unparsed/unsupported configuration lines.

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
- Explicitly surfaces topology inference warnings (e.g., multi-access shared subnets with 3+ endpoints).

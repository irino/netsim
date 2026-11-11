# netsim

A Rust-based multi-vendor network configuration translator, topology synthesizer, static verification engine, and Containerlab network emulation orchestrator.

## Documentation

- **English User Guide & Samples**: [`docs/en/how-to-use.md`](docs/en/how-to-use.md)
- **Japanese User Guide (使い方ガイド・サンプル解説)**: [`docs/ja/how-to-use-ja.md`](docs/ja/how-to-use-ja.md)
- **Design Memo (EN)**: [`docs/en/netsim-design-memo.md`](docs/en/netsim-design-memo.md)
- **Design Memo (JA)**: [`docs/ja/netsim-design-memo-ja.md`](docs/ja/netsim-design-memo-ja.md)
- **Samples Directory**: [`examples/`](examples/README.md)

---

## Project Workspace Structure

- `crates/ir`: Vendor-independent Intermediate Representation data models (`DeviceConfig`, `Interface`, `OspfProcess`, `IsisProcess`, `PrefixList`, `PolicyDefinition`, `InterfaceVxlanConfig`, `SegmentRoutingConfig`, `SrV6Config`) aligned with IETF YANG models.
- `crates/parse-ios`: Cisco IOS configuration parser built with `winnow`.
- `crates/parse-junos`: Junos set-format configuration parser built with `winnow`.
- `crates/parse-eos`: Arista EOS configuration parser built with `winnow`.
- `crates/parse-iosxr`: Cisco IOS XR configuration parser built with `winnow`.
- `crates/parse-frr`: FRRouting / SONiC configuration parser built with `winnow`.
- `crates/topology`: Network topology inference engine calculating point-to-point links and multi-access LAN bridge segments across devices.
- `crates/emit-zebra`: zebra-rs YAML configuration emitter conforming to authoritative zebra-rs YANG/YAML schema.
- `crates/emit-frr`: FRRouting (`frr.conf` and `daemons`) configuration emitter for multi-engine container emulation.
- `crates/reachability`: Static reachability graph solver, consistency analyzer, intent assertion engine, and Batfish vendor-independent format exporter.
- `crates/emit-clab`: Containerlab topology generator emitting `*.clab.yml` definitions with deterministic interface mappings and multi-engine support.
- `crates/netsim-cli`: Unified command-line interface providing `convert`, `gen-lab`, `check`, and `verify` (alias: `test`) subcommands.
- `crates/validate-yang`: Automated IETF YANG schema validation and drift testing engine backed by `libyang` (`yang3` and `holo-yang`).
- `examples/`: Synthetic example configurations demonstrating multi-device network topologies (e.g., `examples/ospf-3routers`, `examples/mixed-5vendors`, `examples/mixed-5vendors-ipv6`).

---

## Step-by-Step Usage Guide

### Prerequisites

- **Rust toolchain**: 1.80+ (`rustup default stable`)
- **C Compiler & CMake**: `gcc` / `clang`, `cmake`
- **PCRE2 development library**: `libpcre2-dev` (Debian/Ubuntu) or `pcre2-devel` (RHEL/CentOS)
- **Containerlab**: (optional, for deploying emulation environments) [Install guide](https://containerlab.dev/install/)
- **Docker**: (optional, required by Containerlab)
- **Container images**: `quay.io/frrouting/frr:10.2.0` or `zebra-rs:latest`

---

### 1. Translating a Single Configuration (`convert`)

To translate a single vendor configuration file into a target engine configuration (`zebra` YAML or `frr.conf`):

```bash
# Cisco IOS to zebra-rs YAML
cargo run --bin netsim -- convert \
  --vendor ios \
  --engine zebra \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.zebra.yaml

# Cisco IOS to FRRouting configuration
cargo run --bin netsim -- convert \
  --vendor ios \
  --engine frr \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.frr.conf

# Junos set-format
cargo run --bin netsim -- convert \
  --vendor junos \
  --in examples/mixed-ios-junos/junos-r2.set \
  --out junos-r2.zebra.yaml

# Arista EOS
cargo run --bin netsim -- convert \
  --vendor eos \
  --in examples/mixed-3vendors/arista-r3.cfg \
  --out arista-r3.zebra.yaml

# Cisco IOS XR
cargo run --bin netsim -- convert \
  --vendor iosxr \
  --in examples/mixed-4vendors/iosxr-r4.cfg \
  --out iosxr-r4.zebra.yaml

# FRRouting / SONiC
cargo run --bin netsim -- convert \
  --vendor frr \
  --in examples/mixed-5vendors/frr-r5.cfg \
  --out frr-r5.zebra.yaml
```

**Options**:
- `--vendor <ios|junos|eos|iosxr|frr>`: Input vendor configuration format (default: `ios`).
- `--engine <zebra|frr>`: Target runtime engine (`zebra` or `frr`; default: `zebra`).
- `--in <FILE>`: Path to the input vendor configuration file.
- `--out <FILE>`: Destination path for the generated configuration file.

---

### 2. Generating Complete Emulation Labs (`gen-lab`)

The `gen-lab` command processes a folder containing multiple router configurations (`.cfg`, `.set`, `.conf`, or `.txt`), automatically identifies the vendor for each file, infers point-to-point links and shared multi-access LAN segments, emits all node configurations, builds a Containerlab topology definition, and produces a diagnostic report.

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

# Mixed 5-vendor Cisco IOS, Junos, Arista EOS, Cisco IOS XR, and FRRouting topology
cargo run --bin netsim -- gen-lab \
  --configs ./examples/mixed-5vendors/ \
  --out ./lab-mixed5/ \
  --engine frr \
  --lab-name mixed-lab
```

**Generated Output Structure**:
```
lab-mixed5/
├── arista-r3_frr.conf
├── arista-r3_daemons
├── cisco-r1_frr.conf
├── cisco-r1_daemons
├── topology.clab.yml         # Containerlab topology file
└── report.md                 # Diagnostic markdown report
```

---

### 3. Static Mathematical Reachability Checking (`check`)

Run fast static mathematical reachability checking, subnet consistency analysis, VTEP/SRv6 verification, and Batfish snapshot export prior to Containerlab deployment:

```bash
# Check static reachability and validate intent assertions across configurations:
cargo run --bin netsim -- check --configs ./examples/ospf-3routers/

# Check with Batfish vendor-independent format export:
cargo run --bin netsim -- check \
  --configs ./examples/ospf-3routers/ \
  --export-batfish ./batfish-snapshot.json
```

---

### 4. Automated Deployment & Verification (`verify` / `test`)

The `verify` command (aliased to `test`) automates the entire lifecycle:
1. Deploys the Containerlab topology (`containerlab deploy -t topology.clab.yml`).
2. Polls `vtyctl show ip route` / `vtysh -c 'show ip route'` on all nodes until routing protocols converge (or timeout).
3. Executes automated data-plane ping tests across all inferred device loopbacks (both IPv4 & IPv6).
4. Validates operational assertions defined in `intent.yml` (e.g. `assert: ping r1 -> r5 loopback`).
5. Automatically tears down the lab (`containerlab destroy -t topology.clab.yml --cleanup`).

```bash
# Verify generated lab in ./lab-ospf/ with automated convergence check and loopback pings
cargo run --bin netsim -- verify --lab-dir ./lab-ospf/ --sudo

# Run static-only verification without deploying containers
cargo run --bin netsim -- verify --lab-dir ./lab-ospf/ --static-only
```

---

## Intent Assertion Specification (`intent.yml`)

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

---

## Supported Vendors and Syntax Overview

- **Cisco IOS**: Interfaces (IPv4/IPv6, VXLAN), OSPFv2, OSPFv3, IS-IS, BGP (peer groups, route reflectors, EVPN), SRv6 locators, prefix-lists, community-lists, route-maps.
- **Junos**: Set format syntax (`interfaces ... family inet/inet6/iso`, `routing-options static route`, `protocols ospf/ospf3/isis/bgp`, EVPN/VXLAN, policy-statements).
- **Arista EOS**: Interfaces (IPv4/IPv6, Vxlan), OSPFv2/OSPFv3, IS-IS, BGP, EVPN, SRv6 locators, prefix-lists, community-lists, route-maps.
- **Cisco IOS XR**: Hierarchical interface blocks, OSPF areas/interfaces, IS-IS, BGP EVPN, SRv6 locators and uSID, prefix-sets, RPL route-policies.
- **FRRouting / SONiC**: Interfaces, OSPFv2/OSPF6, IS-IS, BGP, EVPN, SRv6, prefix-lists, community-lists, route-maps.

---

## CI & Quality Checks

Run the automated test suite locally:
```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

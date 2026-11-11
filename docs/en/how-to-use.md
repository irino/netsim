# How to Use netsim: User Guide & Examples

`netsim` is a Rust-based multi-vendor network configuration translator, topology synthesizer, static verification engine, and Containerlab emulation orchestrator. It parses diverse vendor configuration files into an IETF YANG-aligned Intermediate Representation (IR), infers inter-device link topology, and emits configuration artifacts for either `zebra-rs` or `FRRouting` (FRR) running inside Containerlab network emulation containers.

---

## Table of Contents

1. [Prerequisites & Installation](#prerequisites--installation)
2. [Architecture Overview](#architecture-overview)
3. [CLI Commands Reference](#cli-commands-reference)
   - [`netsim convert`](#1-netsim-convert)
   - [`netsim gen-lab`](#2-netsim-gen-lab)
   - [`netsim check`](#3-netsim-check)
   - [`netsim verify` (alias: `test`)](#4-netsim-verify-alias-test)
4. [Runtime Engines: zebra-rs vs. FRR](#runtime-engines-zebra-rs-vs-frr)
5. [Intent-Driven Testing (`intent.yml`)](#intent-driven-testing-intentyml)
6. [Supported Vendors and Features](#supported-vendors-and-features)
7. [Comprehensive Walkthrough of Sample Labs](#comprehensive-walkthrough-of-sample-labs)
   - [Sample 1: `ospf-3routers`](#sample-1-ospf-3routers)
   - [Sample 2: `ebgp-2routers`](#sample-2-ebgp-2routers)
   - [Sample 3: `mixed-ios-junos`](#sample-3-mixed-ios-junos)
   - [Sample 4: `mixed-3vendors`](#sample-4-mixed-3vendors)
   - [Sample 5: `mixed-4vendors`](#sample-5-mixed-4vendors)
   - [Sample 6: `mixed-5vendors`](#sample-6-mixed-5vendors)
   - [Sample 7: `mixed-5vendors-ipv6`](#sample-7-mixed-5vendors-ipv6)
8. [Troubleshooting & Diagnostics](#troubleshooting--diagnostics)

---

## Prerequisites & Installation

### Build Tools & Libraries
- **Rust Toolchain**: 1.80+ (`rustup default stable`)
- **C Compiler & CMake**: `gcc` / `clang`, `cmake` (required for bundled `libyang`)
- **PCRE2 Development Library**: `libpcre2-dev` (Ubuntu/Debian) or `pcre2-devel` (RHEL/CentOS)
- **Git**

Install dependencies on Debian/Ubuntu:
```bash
sudo apt-get update && sudo apt-get install -y build-essential cmake libpcre2-dev
```

### Emulation Tools (Optional, for Live Containerlab Deployment)
- **Containerlab**: [Installation Guide](https://containerlab.dev/install/) (e.g. `curl -sL https://containerlab.dev/setup | sudo bash -s "all"`)
- **Docker**: Engine version 20.10+
- **Container Images**:
  - For FRR: `quay.io/frrouting/frr:10.2.0` (pulled automatically by Docker)
  - For zebra-rs: `zebra-rs:latest` (built locally from [zebra-rs repository](https://github.com/zebra-rs/zebra-rs))

### Building netsim
```bash
# Clone the repository
git clone https://github.com/irino/netsim.git
cd netsim

# Build release binary
cargo build --release --bin netsim

# The compiled binary is located at target/release/netsim
# Or run directly with cargo:
cargo run --bin netsim -- --help
```

---

## Architecture Overview

```
 Vendor Configurations (Cisco IOS, Junos, Arista EOS, IOS XR, FRR)
                             │
                             ▼
              [winnow Vendor Parsers]
                             │
                             ▼
         [IETF YANG-Aligned Rust IR (crates/ir)]
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
[Topology Inference]   [Static Reachability]  [Config Emitters]
  - P2P Subnet Links     - Subnet & IP Check    - emit-zebra (YAML)
  - LAN Bridge Nodes     - VTEP / SRv6 Check    - emit-frr (frr.conf)
        │                - Batfish JSON Export  - emit-clab (topology.clab.yml)
        ▼                    │                    │
 [Containerlab Lab] ◄────────┴────────────────────┘
        │
        ▼
 [netsim verify / test]
  - Automated deployment (`containerlab deploy`)
  - Route convergence polling (`vtyctl` / `vtysh`)
  - Loopback ping matrix & operational intent validation (`intent.yml`)
  - Automatic teardown (`containerlab destroy`)
```

---

## CLI Commands Reference

`netsim` provides 4 primary subcommands:

### 1. `netsim convert`

Converts a single router configuration file from any supported vendor into a target engine configuration (`zebra` YAML or `frr.conf`).

#### Syntax
```bash
netsim convert --in <FILE> --out <FILE> [--vendor <ios|junos|eos|iosxr|frr>] [--engine <zebra|frr>]
```

#### Arguments
- `--in <FILE>`: Path to input configuration file.
- `--out <FILE>`: Path to output configuration file.
- `--vendor <VENDOR>`: Vendor syntax of the input file. Default: `ios`. Valid options: `ios`, `junos`, `eos`, `iosxr`, `frr`.
- `--engine <ENGINE>`: Target runtime emulation engine. Default: `zebra`. Valid options: `zebra`, `frr`.

#### Example
```bash
# Translate Cisco IOS to zebra-rs YAML
cargo run --bin netsim -- convert \
  --vendor ios \
  --engine zebra \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.zebra.yaml

# Translate Cisco IOS to FRRouting configuration
cargo run --bin netsim -- convert \
  --vendor ios \
  --engine frr \
  --in examples/ospf-3routers/R1.cfg \
  --out R1.frr.conf
```

---

### 2. `netsim gen-lab`

Processes a folder containing configurations from multiple routers (even across mixed vendors), automatically detects each file's vendor syntax, infers point-to-point and multi-access LAN topologies, emits configuration files for all devices, generates a Containerlab `topology.clab.yml`, and produces a comprehensive Markdown diagnostic report.

#### Syntax
```bash
netsim gen-lab --configs <DIR> --out <DIR> [--engine <zebra|frr>] [--lab-name <NAME>]
```

#### Arguments
- `--configs <DIR>`: Directory containing vendor configuration files (`.cfg`, `.set`, `.conf`, `.txt`).
- `--out <DIR>`: Output directory where node configurations, `topology.clab.yml`, and `report.md` will be written.
- `--engine <ENGINE>`: Target runtime engine. Default: `zebra`. Options: `zebra` or `frr`.
- `--lab-name <NAME>`: Name of the Containerlab topology. Default: `netsim-lab`.

#### Example
```bash
cargo run --bin netsim -- gen-lab \
  --configs ./examples/mixed-5vendors/ \
  --out ./lab-mixed5/ \
  --engine frr \
  --lab-name mixed5-frr
```

#### Output Directory Structure
For `--engine zebra`:
```
lab-mixed5/
├── arista-r3.zebra.yaml
├── cisco-r1.zebra.yaml
├── frr-r5.zebra.yaml
├── iosxr-r4.zebra.yaml
├── junos-r2.zebra.yaml
├── topology.clab.yml
└── report.md
```

For `--engine frr`:
```
lab-mixed5/
├── arista-r3_frr.conf
├── arista-r3_daemons
├── cisco-r1_frr.conf
├── cisco-r1_daemons
├── topology.clab.yml
└── report.md
```

---

### 3. `netsim check`

Executes fast, mathematical static verification without needing Docker or Containerlab. It checks for:
- Duplicate IP addresses and duplicate subnet assignments.
- Point-to-point subnet prefix length consistency.
- VTEP NVE source interface existence and consistency.
- SRv6 locator prefix collisions and reachability.
- Operational intent assertions (`intent.yml`) evaluated via Dijkstra shortest paths on the inferred topology graph.
- Optional Batfish vendor-independent snapshot export.

#### Syntax
```bash
netsim check --configs <DIR> [--intent <FILE>] [--export-batfish <FILE>]
```

#### Arguments
- `--configs <DIR>`: Directory containing configuration files.
- `--intent <FILE>`: Optional path to an operational intent assertion file (defaults to `<configs>/intent.yml` if present).
- `--export-batfish <FILE>`: Optional path to export Batfish vendor-independent snapshot JSON.

#### Example
```bash
# Check static reachability and intent for 3-router OSPF
cargo run --bin netsim -- check \
  --configs ./examples/ospf-3routers/ \
  --intent ./examples/ospf-3routers/intent.yml \
  --export-batfish ./batfish-ospf.json
```

---

### 4. `netsim verify` (alias: `test`)

Automates end-to-end emulation verification:
1. Deploys the generated Containerlab topology (`sudo containerlab deploy -t topology.clab.yml`).
2. Polls protocol convergence on all nodes (using `vtyctl show ip route` for zebra-rs or `vtysh -c 'show ip route'` for FRR) until all loopbacks and subnets are learned or timeout expires.
3. Performs an automated ping mesh across all discovered device loopback addresses (IPv4 & IPv6).
4. Evaluates operational intent assertions from `intent.yml`.
5. Tears down and cleans up the container lab automatically (`sudo containerlab destroy -t topology.clab.yml --cleanup`).

#### Syntax
```bash
netsim verify [OPTIONS]
# or:
netsim test [OPTIONS]
```

#### Options
- `--lab-dir <DIR>`: Directory containing generated lab artifacts (`topology.clab.yml`, node configs). Default: `./lab/`.
- `--topology <FILE>`: Optional direct path to `topology.clab.yml`.
- `--intent <FILE>`: Path to custom intent assertions file (defaults to `<lab-dir>/intent.yml` if present).
- `--timeout <SECONDS>`: Maximum seconds to poll for routing convergence. Default: `60`.
- `--interval <SECONDS>`: Seconds between polling attempts. Default: `2`.
- `--skip-destroy`: Retain the Containerlab environment after test completion (useful for debugging).
- `--sudo`: Prepend `sudo` to containerlab and docker executions.
- `--static-only`: Perform only static graph analysis without deploying containers.

#### Example
```bash
# Generate lab
cargo run --bin netsim -- gen-lab \
  --configs ./examples/ospf-3routers/ \
  --out ./lab-ospf/ \
  --engine frr

# Copy intent file
cp ./examples/ospf-3routers/intent.yml ./lab-ospf/intent.yml

# Run automated verification with sudo
cargo run --bin netsim -- verify \
  --lab-dir ./lab-ospf/ \
  --sudo \
  --timeout 90
```

---

## Runtime Engines: zebra-rs vs. FRR

`netsim` supports two target execution engines for emulation:

| Feature | `zebra-rs` (`--engine zebra`) | `FRRouting` (`--engine frr`) |
|---|---|---|
| **Configuration Format** | Declarative YAML (`zebra.yaml`) | vtysh syntax (`frr.conf` + `daemons`) |
| **Container Image** | `zebra-rs:latest` | `quay.io/frrouting/frr:10.2.0` |
| **CLI Management Tool** | `vtyctl` (`vtyctl show ip route`, etc.) | `vtysh` (`vtysh -c 'show ip route'`) |
| **Interface Binding** | Kernel Linux interfaces (`eth1`, `eth2`) | Kernel Linux interfaces (`eth1`, `eth2`) |
| **BGP Support** | Neighbor groups, eBGP/iBGP, EVPN, communities | Full BGP, peer-groups, EVPN L2VPN, route reflectors |
| **OSPF Support** | OSPFv2, OSPFv3 | OSPFv2, OSPFv3 |
| **IS-IS Support** | IS-IS Level-1 / Level-2 | IS-IS Level-1 / Level-2 |
| **Overlay / Modern Fabrics** | VXLAN, BGP EVPN, SRv6 locators & uSID | VXLAN, BGP EVPN, SRv6 locators & uSID |

To choose your engine during lab generation:
```bash
# Target zebra-rs (default)
cargo run --bin netsim -- gen-lab --configs ./examples/mixed-5vendors/ --out ./lab-zebra/ --engine zebra

# Target FRRouting
cargo run --bin netsim -- gen-lab --configs ./examples/mixed-5vendors/ --out ./lab-frr/ --engine frr
```

---

## Intent-Driven Testing (`intent.yml`)

You can define operational assertions in an `intent.yml` file placed in the configuration directory or passed to `--intent`.

### Assertion Syntax

```yaml
# Single assertion shorthand:
assert: ping R1 -> R3 loopback

# Multiple assertions list:
assertions:
  # Ping from node name to another node's loopback interface
  - ping R1 -> R2 loopback
  - ping R2 -> R3 loopback

  # Ping directly to an explicit IP address
  - ping R1 -> 1.1.1.1
  - ping R2 -> 10.0.12.1

  # Assert that a router's routing table contains a specific subnet
  - route R1 has 10.0.23.0/24
  - route R3 has 1.1.1.1/32
```

Both `netsim check` (statically) and `netsim verify` (live in containers) automatically parse and evaluate these assertions.

---

## Supported Vendors and Features

`netsim` includes parsers built with `winnow` for 5 major network OS families:

### 1. Cisco IOS (`crates/parse-ios`)
- Hostname: `hostname <NAME>`
- Interfaces: physical, Loopback, NVE (VXLAN), IPv4 (`ip address`), IPv6 (`ipv6 address`), `shutdown` / `no shutdown`, OSPF cost/area, IS-IS metric.
- Routing Protocols: OSPFv2 (`router ospf`), OSPFv3 (`router ospfv3`), IS-IS (`router isis`), BGP (`router bgp`), static routes.
- Advanced: BGP peer-groups, route reflectors, EVPN (`address-family l2vpn evpn`), Segment Routing SRv6 locators, prefix-lists, community-lists, route-maps.

### 2. Junos (`crates/parse-junos`)
- Format: Junos `set` syntax.
- Hostname: `set system host-name <NAME>`
- Interfaces: `interfaces <IF> unit <UNIT> family inet / inet6 / iso address <IP/LEN>`.
- Protocols: OSPF (`protocols ospf`), OSPF3 (`protocols ospf3`), IS-IS (`protocols isis`), BGP (`protocols bgp`), static routes (`routing-options static route`).
- Advanced: BGP groups, route reflectors, EVPN/VXLAN (`switch-options vtep-source-interface`, `vlans ... vxlan vni`), SRv6 locators, policy-statements, prefix-lists.

### 3. Arista EOS (`crates/parse-eos`)
- Hostname: `hostname <NAME>`
- Interfaces: `interface Ethernet...`, `interface Vxlan1`, IPv4/IPv6 addresses (CIDR and subnet mask format), admin state, interface-level OSPF/IS-IS.
- Routing Protocols: `router ospf`, `router ospfv3`, `router isis`, `router bgp`, static routes.
- Advanced: BGP peer-groups, EVPN address family, SRv6 locators, prefix-lists, community-lists, route-maps.

### 4. Cisco IOS XR (`crates/parse-iosxr`)
- Hostname: `hostname <NAME>`
- Interfaces: hierarchical interface blocks, IPv4/IPv6 CIDR addresses, admin state.
- Routing Protocols: `router ospf` with hierarchical areas and interfaces, `router isis`, `router bgp` with `address-family ipv4/ipv6/l2vpn evpn`, `router static`.
- Advanced: Prefix sets (`prefix-set ... end-set`), Route Policy RPL (`route-policy ... end-policy`), SRv6 locators and uSID.

### 5. FRRouting / SONiC (`crates/parse-frr`)
- Hostname: `hostname <NAME>`
- Interfaces: `interface <NAME>`, `ip address`, `ipv6 address`, VXLAN configuration.
- Routing Protocols: `router ospf`, `router ospf6`, `router isis`, `router bgp`, static routes.
- Advanced: BGP peer-groups, EVPN L2VPN, SRv6, prefix-lists, community-lists, route-maps.

---

## Comprehensive Walkthrough of Sample Labs

The repository includes 7 pre-configured sample network topologies under `examples/`:

### Sample 1: `ospf-3routers`
- **Path**: `examples/ospf-3routers`
- **Vendors**: 3x Cisco IOS (`R1.cfg`, `R2.cfg`, `R3.cfg`)
- **Topology**: Triangle interconnection.
  - `R1`: `10.0.12.1/24` (to R2), `10.0.13.1/24` (to R3), Loopback `1.1.1.1/32`
  - `R2`: `10.0.12.2/24` (to R1), `10.0.23.2/24` (to R3), Loopback `2.2.2.2/32`
  - `R3`: `10.0.13.3/24` (to R1), `10.0.23.3/24` (to R2), Loopback `3.3.3.3/32`
- **Protocol**: OSPFv2 Area 0.
- **Run Commands**:
  ```bash
  # Check static reachability and intent
  cargo run --bin netsim -- check --configs examples/ospf-3routers/

  # Generate FRR lab
  cargo run --bin netsim -- gen-lab --configs examples/ospf-3routers/ --out lab-ospf/ --engine frr
  ```

---

### Sample 2: `ebgp-2routers`
- **Path**: `examples/ebgp-2routers`
- **Vendors**: 2x Cisco IOS (`r1.cfg`, `r2.cfg`)
- **Topology**: Point-to-point eBGP peering over `10.0.0.0/30`.
  - `r1`: AS 65001, advertises `192.168.1.0/24`, static route `172.16.0.0/16`.
  - `r2`: AS 65002, advertises `192.168.2.0/24`, static route `172.16.0.0/16`.
- **Run Commands**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/ebgp-2routers/ --out lab-ebgp/
  ```

---

### Sample 3: `mixed-ios-junos`
- **Path**: `examples/mixed-ios-junos`
- **Vendors**: Cisco IOS (`cisco-r1.cfg`) & Junos set format (`junos-r2.set`)
- **Topology**: 2-node point-to-point link (`10.0.0.0/30`) running OSPFv2 Area 0.
  - `cisco-r1`: `10.0.0.1/30`, Loopback `1.1.1.1/32`
  - `junos-r2`: `10.0.0.2/30`, Loopback `2.2.2.2/32`
- **Run Commands**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-ios-junos/ --out lab-mixed/
  ```

---

### Sample 4: `mixed-3vendors`
- **Path**: `examples/mixed-3vendors`
- **Vendors**: Cisco IOS (`cisco-r1`), Junos (`junos-r2`), Arista EOS (`arista-r3`)
- **Topology**: 3-router ring running OSPFv2 Area 0 across `/30` links.
  - `cisco-r1` <-> `arista-r3`: `10.0.12.0/30`
  - `cisco-r1` <-> `junos-r2`: `10.0.13.0/30`
  - `junos-r2` <-> `arista-r3`: `10.0.23.0/30`
- **Run Commands**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-3vendors/ --out lab-3v/
  ```

---

### Sample 5: `mixed-4vendors`
- **Path**: `examples/mixed-4vendors`
- **Vendors**: Cisco IOS (`cisco-r1`), Junos (`junos-r2`), Arista EOS (`arista-r3`), Cisco IOS XR (`iosxr-r4`)
- **Topology**: 4-router multi-vendor OSPF mesh.
  - `iosxr-r4` connects to `arista-r3` over `10.0.34.0/30`.
- **Run Commands**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-4vendors/ --out lab-4v/
  ```

---

### Sample 6: `mixed-5vendors`
- **Path**: `examples/mixed-5vendors`
- **Vendors**: Cisco IOS, Junos, Arista EOS, Cisco IOS XR, FRRouting / SONiC (`frr-r5`)
- **Topology**: 5-vendor OSPFv2 network.
  - `frr-r5` connects to `iosxr-r4` over `10.0.45.0/30`.
- **Run Commands**:
  ```bash
  cargo run --bin netsim -- gen-lab --configs examples/mixed-5vendors/ --out lab-5v/ --engine frr
  ```

---

### Sample 7: `mixed-5vendors-ipv6`
- **Path**: `examples/mixed-5vendors-ipv6`
- **Vendors**: Cisco IOS, Junos, Arista EOS, Cisco IOS XR, FRRouting (Dual Stack IPv4 + IPv6)
- **Topology**: 5-vendor dual-stack network with IPv6 unicast addresses (`2001:db8:...`) and OSPFv3 / dual-stack routing.
- **Run Commands**:
  ```bash
  cargo run --bin netsim -- check --configs examples/mixed-5vendors-ipv6/
  cargo run --bin netsim -- gen-lab --configs examples/mixed-5vendors-ipv6/ --out lab-5v-v6/
  ```

---

## Troubleshooting & Diagnostics

### Viewing Unparsed Configuration Statements
`netsim` guarantees that **no configuration is silently discarded**. Any unsupported command is logged in `report.md` with:
- Source file name
- 1-based line number
- Original command content
- Specific reason for skipping

Example excerpt from `report.md`:
```markdown
## Unparsed Configuration Lines
- `router-1.cfg:42`: `ip ipsec transform-set TS esp-aes esp-sha-hmac` (IPsec configuration is not supported)
```

### Checking Container State in Live Emulation
If running a live Containerlab lab:
```bash
# Check running containers
sudo containerlab inspect -t lab-dir/topology.clab.yml

# Check zebra-rs routing state
docker exec -it clab-netsim-lab-R1 vtyctl show ip route
docker exec -it clab-netsim-lab-R1 vtyctl show ip ospf neighbor

# Check FRRouting state
docker exec -it clab-netsim-lab-R1 vtysh -c 'show ip route'
docker exec -it clab-netsim-lab-R1 vtysh -c 'show ip ospf neighbor'
```

### Static-Only Verification
If Containerlab or Docker is unavailable on your machine, you can still execute static intent assertions without container deployment:
```bash
cargo run --bin netsim -- verify --lab-dir ./lab-dir/ --static-only
```

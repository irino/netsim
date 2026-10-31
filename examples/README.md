# netsim Examples & Usage Guide

This directory contains sample network configurations demonstrating the Phase 1 workflow of `netsim`.

## 1. Available Examples

### `ospf-3routers`
A 3-router triangle topology (`R1`, `R2`, `R3`) interconnected via point-to-point `/24` IPv4 subnets running OSPFv2 (Area 0).

- **Topology Summary**:
  - `R1`: Loopback0 (`1.1.1.1/32`), GigabitEthernet0/0 (`10.0.12.1/24` -> R2), GigabitEthernet0/1 (`10.0.13.1/24` -> R3)
  - `R2`: Loopback0 (`2.2.2.2/32`), GigabitEthernet0/0 (`10.0.12.2/24` -> R1), GigabitEthernet0/1 (`10.0.23.2/24` -> R3)
  - `R3`: Loopback0 (`3.3.3.3/32`), GigabitEthernet0/0 (`10.0.13.3/24` -> R1), GigabitEthernet0/1 (`10.0.23.3/24` -> R2)
- **Routing Protocol**: OSPFv2 Process 1 (Router ID matching Loopback IP, network statements covering connected interfaces and loopbacks).

---

## 2. Required Input Files

Each device configuration file (e.g. `.cfg`, `.conf`, or `.txt`) must follow supported Cisco IOS syntax:
- **Hostname**: `hostname <NAME>`
- **Interfaces**:
  - Loopback and physical interfaces: `interface <NAME>`
  - Description: `description <TEXT>`
  - IPv4 address and subnet mask: `ip address <A.B.C.D> <NETMASK>`
  - Admin state: `no shutdown` (or `shutdown`)
- **OSPF Process**:
  - `router ospf <PROCESS_ID>`
  - `router-id <A.B.C.D>`
  - `network <ADDRESS> <WILDCARD> area <AREA>`

---

## 3. CLI Commands & Workflow

The `netsim` CLI (`crates/netsim-cli`) provides two main subcommands: `convert` and `gen-lab`.

### Workflow 1: Convert a Single Router Configuration
To convert an individual Cisco IOS configuration file into a declarative zebra-rs YAML configuration:

```bash
cargo run --bin netsim -- convert --vendor ios --in examples/ospf-3routers/R1.cfg --out R1.zebra.yaml
```

**Options**:
- `--vendor <VENDOR>`: Vendor input format (currently supports `ios`).
- `--in <FILE>`: Path to input vendor configuration file.
- `--out <FILE>`: Path to destination zebra-rs YAML configuration file.

### Workflow 2: Generate Complete Lab Environment (`gen-lab`)
To process a directory of vendor configurations, automatically infer point-to-point interconnecting links, emit zebra-rs YAML files for every router, generate a Containerlab topology file, and produce a diagnostic summary report:

```bash
cargo run --bin netsim -- gen-lab --configs ./examples/ospf-3routers/ --out ./lab-output/ --lab-name ospf-lab
```

**Options**:
- `--configs <DIR>`: Directory containing vendor configuration files (`.cfg`, `.conf`, `.txt`).
- `--out <DIR>`: Output directory where generated artifacts will be written.
- `--lab-name <NAME>`: Name of the Containerlab topology (defaults to `netsim-lab`).

---

## 4. Expected Generated Files (`gen-lab`)

Running `gen-lab` on `./examples/ospf-3routers/` generates the following files in the output directory (e.g., `./lab-output/`):

1. **Node Configurations**:
   - `R1.zebra.yaml`
   - `R2.zebra.yaml`
   - `R3.zebra.yaml`
   - *Description*: Declarative YAML configurations conforming to the zebra-rs schema, containing hostname, loopback/physical interface IPv4 addresses, and OSPFv2 router-id, areas, and interfaces.
2. **Containerlab Topology Definition**:
   - `topology.clab.yml`
   - *Description*: Containerlab topology file (`kind: linux`) that binds each router container to its corresponding `zebra.yaml` configuration via bind mounts, alphabetically maps physical interfaces (e.g., `GigabitEthernet0/0` -> `eth1`), and connects endpoints according to inferred point-to-point links.
3. **Diagnostic Report**:
   - `report.md`
   - *Description*: Markdown report detailing processed devices, inferred links, topology warnings, unparsed configuration lines, and emitter warnings.

---

## 5. Basic End-to-End Simulation Workflow

1. **Translate**: Run `netsim gen-lab` on your vendor configuration directory.
2. **Deploy**: Deploy the generated Containerlab topology:
   ```bash
   sudo containerlab deploy -t lab-output/topology.clab.yml
   ```
3. **Verify**: Access container shells or run zebra-rs CLI (`vtyctl`) to check OSPF neighbor adjacencies (`show ip ospf neighbor`), routing tables (`show ip route`), and reachability (`ping`).
4. **Destroy**: Clean up the container lab when finished:
   ```bash
   sudo containerlab destroy -t lab-output/topology.clab.yml
   ```

---

## 6. Unsupported Configuration Features & Current Limitations

- **Parser Limitations (Cisco IOS Phase 1)**:
  - Only `hostname`, `interface` (Loopback/GigabitEthernet/Ethernet with `description`, `ip address`, `shutdown`/`no shutdown`), and `router ospf` (`router-id`, `network` statement) are parsed.
  - All other configuration commands (e.g. ACLs, static routes, BGP, route-maps, prefix-lists, spanning-tree) are skipped and reported as `UnparsedLine` diagnostics.
- **zebra-rs Emitter Limitations**:
  - **Interface Descriptions**: Not supported in top-level zebra-rs interface schema; emitted as diagnostic `interface-description` warnings.
  - **Admin-Down State**: zebra-rs does not have an explicit administrative shutdown knob; emitted as `interface-disabled` warnings.
  - **Multiple OSPF Processes**: zebra-rs supports a single OSPF instance; process IDs other than `1` emit `ospf-process-id` warnings.
- **Topology Inference Limitations**:
  - Point-to-point links (subnets with exactly 2 participating device interfaces) are fully inferred.
  - Shared multi-access segments (subnets with 3 or more device interfaces) trigger a topology warning in `report.md`.
  - Loopbacks and unnumbered/unconfigured interfaces are ignored during topology inference.

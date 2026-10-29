# netsim

A Rust-based multi-vendor network configuration translator and network emulation generator.

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

# Mixed 5-Vendor IPv6 Dual-Stack Example Network

This directory contains synthetic configurations for a 5-vendor topology running dual-stack IPv4 and IPv6 (OSPFv2 and OSPFv3) across all 5 supported network OS families: Cisco IOS, Junos, Arista EOS, Cisco IOS XR, and FRRouting / SONiC.

## Topology Summary

Every router is configured with both IPv4 and IPv6 unicast addresses (`2001:db8:...` /64 subnets and /128 loopbacks) and runs OSPFv2 (Area 0) alongside OSPFv3:

- **cisco-r1**:
  - Loopback0: `1.1.1.1/32`, `2001:db8::1/128`
  - Gi0/0 (to arista-r3): `10.0.12.1/30`, `2001:db8:12::1/64`
  - Gi0/1 (to junos-r2): `10.0.13.1/30`, `2001:db8:13::1/64`
- **junos-r2**:
  - lo0.0: `2.2.2.2/32`, `2001:db8::2/128`
  - ge-0/0/0.0 (to cisco-r1): `10.0.13.2/30`, `2001:db8:13::2/64`
  - ge-0/0/1.0 (to arista-r3): `10.0.23.2/30`, `2001:db8:23::2/64`
- **arista-r3**:
  - Loopback0: `3.3.3.3/32`, `2001:db8::3/128`
  - Ethernet1 (to cisco-r1): `10.0.12.2/30`, `2001:db8:12::2/64`
  - Ethernet2 (to junos-r2): `10.0.23.1/30`, `2001:db8:23::1/64`
  - Ethernet3 (to iosxr-r4): `10.0.34.1/30`, `2001:db8:34::1/64`
- **iosxr-r4**:
  - Loopback0: `4.4.4.4/32`, `2001:db8::4/128`
  - Gi0/0/0/0 (to arista-r3): `10.0.34.2/30`, `2001:db8:34::2/64`
  - Gi0/0/0/1 (to frr-r5): `10.0.45.1/30`, `2001:db8:45::1/64`
- **frr-r5**:
  - lo: `5.5.5.5/32`, `2001:db8::5/128`
  - eth1 (to iosxr-r4): `10.0.45.2/30`, `2001:db8:45::2/64`

## Usage

### 1. Static Reachability Check (IPv4 & IPv6)
```bash
cargo run --bin netsim -- check --configs examples/mixed-5vendors-ipv6/
```

### 2. Generate Containerlab Topology (zebra-rs)
```bash
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-5vendors-ipv6/ \
  --out lab-5v-v6/ \
  --lab-name mixed-5v-v6
```

### 3. Generate Containerlab Topology (FRRouting)
```bash
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-5vendors-ipv6/ \
  --out lab-5v-v6-frr/ \
  --engine frr \
  --lab-name mixed-5v-v6-frr
```

### 4. Deploy with Containerlab
```bash
sudo containerlab deploy -t lab-5v-v6-frr/topology.clab.yml
```

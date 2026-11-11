# Mixed 5-Vendor Example Network (Cisco IOS, Junos, Arista EOS, Cisco IOS XR, FRRouting)

This directory contains synthetic configurations for a 5-vendor topology running OSPFv2 (Area 0) across all 5 major network OS families supported by `netsim`.

## Topology

```
             +--------------------+
             |      cisco-r1      |
             |    (Cisco IOS)     |
             | Loopback0: 1.1.1.1 |
             +--------------------+
             /                    \
  Gi0/0:    /                      \    Gi0/1:
 10.0.12.1/30                       \  10.0.13.1/30
           /                        \
          /                          \
         /                            \
+--------------------+        +--------------------+
|     arista-r3      |        |      junos-r2      |
|    (Arista EOS)    | Eth2---|      (Junos)       |
| Loopback0: 3.3.3.3 |        |   lo0.0: 2.2.2.2   |
+--------------------+        +--------------------+
         |
         | Ethernet3: 10.0.34.1/30
         |
         | Gi0/0/0/0: 10.0.34.2/30
+--------------------+
|     iosxr-r4       |
|  (Cisco IOS XR)    |
| Loopback0: 4.4.4.4 |
+--------------------+
         |
         | Gi0/0/0/1: 10.0.45.1/30
         |
         | eth1: 10.0.45.2/30
+--------------------+
|      frr-r5        |
| (FRRouting / SONiC)|
|   lo: 5.5.5.5/32   |
+--------------------+
```

- **cisco-r1**: Cisco IOS router, OSPF Area 0, router-id `1.1.1.1`.
- **junos-r2**: Junos router (set format), OSPF Area 0, router-id `2.2.2.2`.
- **arista-r3**: Arista EOS switch/router, OSPF Area 0, router-id `3.3.3.3`.
- **iosxr-r4**: Cisco IOS XR router, OSPF Area 0, router-id `4.4.4.4`.
- **frr-r5**: FRRouting / SONiC router (`frr.conf`), OSPF Area 0, router-id `5.5.5.5`.

## Usage

### 1. Static Reachability Check
```bash
cargo run --bin netsim -- check --configs examples/mixed-5vendors/
```

### 2. Generate Containerlab Topology (zebra-rs)
```bash
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-5vendors/ \
  --out lab-5v/ \
  --lab-name mixed-5v
```

### 3. Generate Containerlab Topology (FRRouting)
```bash
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-5vendors/ \
  --out lab-5v-frr/ \
  --engine frr \
  --lab-name mixed-5v-frr
```

### 4. Deploy with Containerlab
```bash
sudo containerlab deploy -t lab-5v-frr/topology.clab.yml
```

# Mixed 3-Vendor Example Network (Cisco IOS, Junos, Arista EOS)

This directory contains synthetic configurations for a 3-vendor ring topology running OSPFv2 (Area 0) over `/30` point-to-point links.

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
|    (Arista EOS)    |        |      (Junos)       |
| Loopback0: 3.3.3.3 |--------|   lo0.0: 2.2.2.2   |
+--------------------+ Eth2   +--------------------+
                      10.0.23.1/30   10.0.23.2/30 (ge-0/0/1.0)
```

- **cisco-r1**: Cisco IOS, Loopback0 `1.1.1.1/32`, Gi0/0 `10.0.12.1/30`, Gi0/1 `10.0.13.1/30`, OSPF Area 0.
- **junos-r2**: Junos set format, lo0.0 `2.2.2.2/32`, ge-0/0/0.0 `10.0.13.2/30`, ge-0/0/1.0 `10.0.23.2/30`, OSPF Area 0.
- **arista-r3**: Arista EOS, Loopback0 `3.3.3.3/32`, Ethernet1 `10.0.12.2/30`, Ethernet2 `10.0.23.1/30`, OSPF Area 0.

## Usage

### 1. Static Reachability Check
```bash
cargo run --bin netsim -- check --configs examples/mixed-3vendors/
```

### 2. Generate Containerlab Topology (zebra-rs)
```bash
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-3vendors/ \
  --out lab-3v/ \
  --lab-name mixed-3v
```

### 3. Generate Containerlab Topology (FRRouting)
```bash
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-3vendors/ \
  --out lab-3v-frr/ \
  --engine frr \
  --lab-name mixed-3v-frr
```

### 4. Deploy with Containerlab
```bash
sudo containerlab deploy -t lab-3v/topology.clab.yml
```

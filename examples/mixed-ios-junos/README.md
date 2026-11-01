# Mixed Multi-Vendor Example Network (Cisco IOS & Junos)

This directory contains synthetic configurations for a multi-vendor topology consisting of one Cisco IOS router and one Junos router running OSPFv2 (Area 0).

## Topology

```
+-------------------+                    +-------------------+
|     cisco-r1      | GigabitEthernet0/0 |     junos-r2      |
|    (Cisco IOS)    |--------------------|      (Junos)      |
|    10.0.0.1/30    |                    |    10.0.0.2/30    |
+-------------------+                    +-------------------+
```

- **cisco-r1**: Loopback0 (`1.1.1.1/32`), GigabitEthernet0/0 (`10.0.0.1/30`), OSPF router-id `1.1.1.1`.
- **junos-r2**: lo0.0 (`2.2.2.2/32`), ge-0/0/0.0 (`10.0.0.2/30`), OSPF router-id `2.2.2.2`.

## Converting and Generating Lab

Generate zebra-rs configuration files and Containerlab topology:

```bash
cargo run --bin netsim -- gen-lab --configs examples/mixed-ios-junos/ --out lab-mixed/ --lab-name mixed-lab
```

Deploying Containerlab:

```bash
sudo clab deploy -t lab-mixed/topology.clab.yml
```

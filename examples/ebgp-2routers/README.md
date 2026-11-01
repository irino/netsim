# 2-Router eBGP Example Network

This directory contains synthetic Cisco IOS configurations for an eBGP 2-router peering scenario.

## Topology

```
+-------------+                     +-------------+
|     r1      | GigabitEthernet0/0  |     r2      |
|  AS 65001   |---------------------|  AS 65002   |
| 10.0.0.1/30 |                     | 10.0.0.2/30 |
+-------------+                     +-------------+
```

- **r1**: BGP AS 65001, router-id 1.1.1.1, advertises prefix `192.168.1.0/24`.
- **r2**: BGP AS 65002, router-id 2.2.2.2, advertises prefix `192.168.2.0/24`.
- Both routers have a static route for `172.16.0.0/16`.

## Converting and Generating Lab

Generate zebra-rs configuration files and Containerlab topology:

```bash
cargo run --bin netsim -- gen-lab --configs examples/ebgp-2routers --out lab-ebgp/
```

Deploying Containerlab:

```bash
sudo clab deploy -t lab-ebgp/topology.clab.yml
```

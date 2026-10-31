# OSPF 3-Router Triangle Example (`ospf-3routers`)

This directory contains sample Cisco IOS configurations for 3 routers (`R1`, `R2`, `R3`) arranged in a triangle topology running OSPFv2.

## Files
- `R1.cfg`: Configuration for router R1
- `R2.cfg`: Configuration for router R2
- `R3.cfg`: Configuration for router R3

## Quick Usage with netsim CLI

### Generate Lab Environment
```bash
cargo run --bin netsim -- gen-lab --configs ./examples/ospf-3routers/ --out ./lab-output/ --lab-name ospf-triangle
```

### Convert Single Router
```bash
cargo run --bin netsim -- convert --vendor ios --in ./examples/ospf-3routers/R1.cfg --out ./R1.zebra.yaml
```

For detailed documentation, input requirements, generated artifacts, workflow, and limitations, see the [Examples README](../README.md).

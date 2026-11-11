# netsim Examples & Sample Topologies

This directory contains pre-configured synthetic multi-device network configurations demonstrating the features of `netsim`.

For complete end-to-end documentation on how to use `netsim`, refer to:
- **English User Guide**: [`docs/en/how-to-use.md`](../docs/en/how-to-use.md)
- **Japanese User Guide (使い方ガイド)**: [`docs/ja/how-to-use-ja.md`](../docs/ja/how-to-use-ja.md)

---

## 1. Available Sample Topologies

| Sample Directory | Devices & Vendors | Protocols & Topologies | Documentation |
|---|---|---|---|
| [`ospf-3routers`](ospf-3routers/) | 3x Cisco IOS (`R1`, `R2`, `R3`) | OSPFv2 (Area 0) triangle topology with operational `intent.yml` | [README](ospf-3routers/README.md) |
| [`ebgp-2routers`](ebgp-2routers/) | 2x Cisco IOS (`r1`, `r2`) | eBGP peering (AS 65001 - AS 65002) with static routes | [README](ebgp-2routers/README.md) |
| [`mixed-ios-junos`](mixed-ios-junos/) | Cisco IOS + Junos | 2-node `/30` point-to-point OSPFv2 Area 0 link | [README](mixed-ios-junos/README.md) |
| [`mixed-3vendors`](mixed-3vendors/) | Cisco IOS + Junos + Arista EOS | 3-router ring topology with `/30` P2P links running OSPFv2 | [README](mixed-3vendors/README.md) |
| [`mixed-4vendors`](mixed-4vendors/) | Cisco IOS + Junos + Arista EOS + Cisco IOS XR | 4-router multi-vendor OSPFv2 mesh | [README](mixed-4vendors/README.md) |
| [`mixed-5vendors`](mixed-5vendors/) | Cisco IOS + Junos + Arista EOS + Cisco IOS XR + FRRouting | 5-vendor multi-device OSPFv2 network | [README](mixed-5vendors/README.md) |
| [`mixed-5vendors-ipv6`](mixed-5vendors-ipv6/) | 5 Vendors (Cisco IOS, Junos, Arista EOS, Cisco IOS XR, FRR) | Dual-Stack IPv4 + IPv6 network running OSPFv2 and OSPFv3 | [README](mixed-5vendors-ipv6/README.md) |

---

## 2. Common Workflows on Samples

### Static Verification (`check`)
Perform mathematical reachability analysis, subnet and IP conflict detection, and intent validation across any sample directory without launching containers:

```bash
cargo run --bin netsim -- check --configs examples/ospf-3routers/
cargo run --bin netsim -- check --configs examples/mixed-5vendors-ipv6/
```

### Lab Generation (`gen-lab`)
Generate Containerlab definitions (`topology.clab.yml`) and configuration files for all devices:

```bash
# Generate zebra-rs configurations
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-5vendors/ \
  --out lab-mixed/ \
  --lab-name mixed5-lab

# Generate FRRouting configurations
cargo run --bin netsim -- gen-lab \
  --configs examples/mixed-5vendors/ \
  --out lab-frr/ \
  --engine frr \
  --lab-name mixed5-frr
```

### Automated Verification (`verify` / `test`)
Deploy the lab in Containerlab, wait for route convergence, verify loopback pings, and clean up:

```bash
cargo run --bin netsim -- verify \
  --lab-dir lab-frr/ \
  --sudo \
  --timeout 90
```

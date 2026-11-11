use netsim_cli::{run, Cli, Commands};
use std::fs;
use std::path::PathBuf;

fn create_temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let unique = format!("netsim_phase5_{}_{}", name, std::process::id());
    dir.push(unique);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_phase5_evpn_and_srv6_conversion() {
    let tmp = create_temp_dir("phase5_convert");
    let cfg_file = tmp.join("iosxr-leaf1.cfg");
    let out_zebra = tmp.join("leaf1.zebra.yaml");
    let out_frr = tmp.join("leaf1.frr.conf");

    let eos_config = r#"
hostname leaf1
!
interface Ethernet1
   no shutdown
   ip address 10.0.0.1/30
!
interface Vxlan1
   vxlan source-interface Loopback0
   vxlan udp-port 4789
   vxlan vlan 10 vni 10010
   vxlan vlan 20 vni 10020
!
router bgp 65001
   router-id 10.0.0.1
   neighbor 10.0.0.100 remote-as 65001
   address-family evpn
      neighbor 10.0.0.100 activate
!
segment-routing
   srv6
      encapsulation
         source-address 2001:db8:1::1
      locators
         locator LOC1
            prefix fc00:0:1::/48
            micro-segment behavior unode
!
"#;
    fs::write(&cfg_file, eos_config).unwrap();

    // 1. Convert to zebra YAML
    let cli_zebra = Cli {
        command: Commands::Convert {
            vendor: "eos".to_string(),
            engine: "zebra".to_string(),
            r#in: cfg_file.clone(),
            out: out_zebra.clone(),
        },
    };
    let res_z = run(cli_zebra);
    assert!(
        res_z.is_ok(),
        "Conversion to zebra should succeed: {:?}",
        res_z.err()
    );

    let zebra_yaml = fs::read_to_string(&out_zebra).unwrap();
    assert!(zebra_yaml.contains("name: Vxlan1"));
    assert!(zebra_yaml.contains("vni: 10010"));
    assert!(zebra_yaml.contains("udp-port: 4789"));
    assert!(zebra_yaml.contains("peer: 10.0.0.100"));
    assert!(zebra_yaml.contains("segment-routing:"));
    assert!(zebra_yaml.contains("prefix: fc00:0:1::/48"));
    assert!(zebra_yaml.contains("block-length: 16"));

    // 2. Convert to FRR syntax
    let cli_frr = Cli {
        command: Commands::Convert {
            vendor: "eos".to_string(),
            engine: "frr".to_string(),
            r#in: cfg_file,
            out: out_frr.clone(),
        },
    };
    let res_f = run(cli_frr);
    assert!(
        res_f.is_ok(),
        "Conversion to frr should succeed: {:?}",
        res_f.err()
    );

    let frr_conf = fs::read_to_string(&out_frr).unwrap();
    assert!(frr_conf.contains("interface Vxlan1"));
    assert!(frr_conf.contains("vxlan id 10010"));
    assert!(frr_conf.contains("address-family l2vpn evpn"));
    assert!(frr_conf.contains("neighbor 10.0.0.100 activate"));
    assert!(frr_conf.contains("segment-routing"));
    assert!(frr_conf.contains("srv6"));
    assert!(frr_conf.contains("prefix fc00:0:1::/48 micro-segment behavior unode"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_phase5_evpn_fabric_lab_generation() {
    let tmp = create_temp_dir("phase5_lab");
    let configs_dir = tmp.join("configs");
    let out_dir = tmp.join("lab-out");
    fs::create_dir_all(&configs_dir).unwrap();

    // Leaf 1 (Cisco IOS XR)
    let xr_cfg = r#"
hostname leaf1
!
interface Loopback0
 ipv4 address 10.0.0.1 255.255.255.255
!
interface GigabitEthernet0/0/0/0
 ipv4 address 10.10.10.1 255.255.255.252
 no shutdown
!
interface nve1
 source-interface Loopback0
 member vni 10010
!
router bgp 65000
 bgp router-id 10.0.0.1
 neighbor 10.10.10.2
  remote-as 65000
  address-family l2vpn evpn
  !
 !
!
segment-routing
 srv6
  encapsulation
   source-address 2001:db8:1::1
  !
  locators
   locator LOC1
    micro-segment behavior unode
    prefix fc00:0:1::/48
   !
  !
 !
!
"#;
    fs::write(configs_dir.join("iosxr-leaf1.cfg"), xr_cfg).unwrap();

    // Leaf 2 (Arista EOS)
    let eos_cfg = r#"
hostname leaf2
!
interface Loopback0
 ip address 10.0.0.2/32
!
interface Ethernet1
 no shutdown
 ip address 10.10.10.2/30
!
interface Vxlan1
 vxlan source-interface Loopback0
 vxlan vlan 10 vni 10010
!
router bgp 65000
 router-id 10.0.0.2
 neighbor 10.10.10.1 remote-as 65000
 address-family evpn
  neighbor 10.10.10.1 activate
!
segment-routing
 srv6
  encapsulation
   source-address 2001:db8:2::1
  locators
   locator LOC2
    prefix fc00:0:2::/48
    micro-segment behavior unode
!
"#;
    fs::write(configs_dir.join("arista-leaf2.cfg"), eos_cfg).unwrap();

    // Generate Containerlab topology with zebra-rs engine
    let cli = Cli {
        command: Commands::GenLab {
            configs: configs_dir.clone(),
            out: out_dir.clone(),
            engine: "zebra".to_string(),
            lab_name: "evpn-fabric".to_string(),
        },
    };
    let res = run(cli);
    assert!(
        res.is_ok(),
        "GenLab for Phase 5 should succeed: {:?}",
        res.err()
    );

    assert!(out_dir.join("topology.clab.yml").exists());
    assert!(out_dir.join("leaf1.zebra.yaml").exists());
    assert!(out_dir.join("leaf2.zebra.yaml").exists());

    let leaf1_yaml = fs::read_to_string(out_dir.join("leaf1.zebra.yaml")).unwrap();
    assert!(leaf1_yaml.contains("hostname: leaf1"));
    assert!(leaf1_yaml.contains("name: nve1"));
    assert!(leaf1_yaml.contains("vni: 10010"));
    assert!(leaf1_yaml.contains("peer: 10.10.10.2"));
    assert!(leaf1_yaml.contains("segment-routing:"));
    assert!(leaf1_yaml.contains("block-length: 16"));

    let leaf2_yaml = fs::read_to_string(out_dir.join("leaf2.zebra.yaml")).unwrap();
    assert!(leaf2_yaml.contains("hostname: leaf2"));
    assert!(leaf2_yaml.contains("name: Vxlan1"));
    assert!(leaf2_yaml.contains("vni: 10010"));
    assert!(leaf2_yaml.contains("peer: 10.10.10.1"));
    assert!(leaf2_yaml.contains("segment-routing:"));
    assert!(leaf2_yaml.contains("block-length: 16"));

    let _ = fs::remove_dir_all(&tmp);
}

use std::fs;

use ir::{BgpNeighbor, BgpProcess, DeviceConfig, Interface, Ipv4Prefix, PrefixLength, StaticRoute};

use crate::context::{create_yang_context, PINNED_MODULES};
use crate::validator::{validate_device_config, validate_yang_json, ValidationError};

#[test]
fn test_pinned_modules_metadata_and_revisions() {
    assert_eq!(PINNED_MODULES.len(), 8);
    let module_names: Vec<&str> = PINNED_MODULES.iter().map(|m| m.name).collect();
    assert!(module_names.contains(&"iana-if-type"));
    assert!(module_names.contains(&"iana-routing-types"));
    assert!(module_names.contains(&"ietf-interfaces"));
    assert!(module_names.contains(&"ietf-ip"));
    assert!(module_names.contains(&"ietf-routing"));
    assert!(module_names.contains(&"ietf-ipv4-unicast-routing"));
    assert!(module_names.contains(&"ietf-ospf"));
    assert!(module_names.contains(&"ietf-bgp"));

    // Verify pinned revisions
    for m in PINNED_MODULES {
        assert!(
            !m.revision.is_empty(),
            "Revision cannot be empty for {}",
            m.name
        );
        assert!(
            m.source_url.starts_with("https://"),
            "Source URL must be https for {}",
            m.name
        );
    }
}

#[test]
fn test_valid_device_config_full_golden() {
    let json_str = fs::read_to_string("../../tests/golden/device_config_full.json")
        .expect("read device_config_full.json");
    let device_config: DeviceConfig =
        serde_json::from_str(&json_str).expect("deserialize device_config_full");

    let docs = validate_device_config(&device_config).expect("validation should succeed");
    assert!(docs.interfaces_json.is_some());
    assert!(docs.routing_json.is_some());
    assert!(!docs.uncovered_fields.is_empty());
}

#[test]
fn test_valid_cisco_ios_basic_golden() {
    let json_str = fs::read_to_string("../../tests/golden/cisco_ios_basic.json")
        .expect("read cisco_ios_basic.json");
    let device_config: DeviceConfig =
        serde_json::from_str(&json_str).expect("deserialize cisco_ios_basic");

    let docs = validate_device_config(&device_config).expect("validation should succeed");
    assert!(docs.interfaces_json.is_some());
    assert!(docs.routing_json.is_some());
}

#[test]
fn test_valid_junos_basic_golden() {
    let json_str =
        fs::read_to_string("../../tests/golden/junos_basic.json").expect("read junos_basic.json");
    let device_config: DeviceConfig =
        serde_json::from_str(&json_str).expect("deserialize junos_basic");

    let docs = validate_device_config(&device_config).expect("validation should succeed");
    assert!(docs.interfaces_json.is_some());
    assert!(docs.routing_json.is_some());
}

#[test]
fn test_valid_device_with_bgp_and_static_routes() {
    let config = DeviceConfig {
        hostname: "border-router".to_string(),
        interfaces: vec![
            Interface {
                name: "GigabitEthernet0/0".to_string(),
                description: Some("Uplink to ISP".to_string()),
                enabled: true,
                ipv4: Some(Ipv4Prefix::new(
                    "203.0.113.1".parse().unwrap(),
                    PrefixLength::new(30).unwrap(),
                )),
                ospf: None,
            },
            Interface {
                name: "Loopback0".to_string(),
                description: Some("Router ID Loopback".to_string()),
                enabled: true,
                ipv4: Some(Ipv4Prefix::new(
                    "198.51.100.1".parse().unwrap(),
                    PrefixLength::new(32).unwrap(),
                )),
                ospf: None,
            },
        ],
        ospf: None,
        bgp: Some(BgpProcess {
            as_number: 65001,
            router_id: Some("198.51.100.1".parse().unwrap()),
            neighbors: vec![BgpNeighbor {
                remote_address: "203.0.113.2".parse().unwrap(),
                remote_as: 65002,
            }],
            networks: vec![Ipv4Prefix::new(
                "198.51.100.0".parse().unwrap(),
                PrefixLength::new(24).unwrap(),
            )],
        }),
        static_routes: vec![StaticRoute {
            destination: Ipv4Prefix::new("0.0.0.0".parse().unwrap(), PrefixLength::new(0).unwrap()),
            next_hop: "203.0.113.2".parse().unwrap(),
        }],
        prefix_lists: vec![],
        unparsed_lines: vec![],
    };

    let docs = validate_device_config(&config).expect("validation should succeed");
    assert!(docs.interfaces_json.is_some());
    assert!(docs.routing_json.is_some());
}

#[test]
fn test_invalid_interface_missing_mandatory_name_fails() {
    let ctx = create_yang_context();
    let bad_json = r#"{
      "ietf-interfaces:interfaces": {
        "interface": [
          {
            "type": "iana-if-type:ethernetCsmacd",
            "enabled": true
          }
        ]
      }
    }"#;

    let res = validate_yang_json(&ctx, bad_json, "ietf-interfaces");
    assert!(
        res.is_err(),
        "Missing interface key/name must fail validation"
    );
    if let Err(ValidationError::LibyangError { message, .. }) = res {
        assert!(
            message.contains("Missing")
                || message.contains("mandatory")
                || message.contains("key")
                || message.contains("name")
        );
    }
}

#[test]
fn test_invalid_interface_missing_type_fails() {
    let ctx = create_yang_context();
    let bad_json = r#"{
      "ietf-interfaces:interfaces": {
        "interface": [
          {
            "name": "eth0",
            "enabled": true
          }
        ]
      }
    }"#;

    let res = validate_yang_json(&ctx, bad_json, "ietf-interfaces");
    assert!(
        res.is_err(),
        "Missing interface mandatory type must fail validation"
    );
}

#[test]
fn test_invalid_ip_format_fails() {
    let ctx = create_yang_context();
    let bad_json = r#"{
      "ietf-interfaces:interfaces": {
        "interface": [
          {
            "name": "GigabitEthernet0/0",
            "type": "iana-if-type:ethernetCsmacd",
            "ietf-ip:ipv4": {
              "address": [
                {
                  "ip": "999.1.2.3",
                  "prefix-length": 24
                }
              ]
            }
          }
        ]
      }
    }"#;

    let res = validate_yang_json(&ctx, bad_json, "ietf-interfaces");
    assert!(
        res.is_err(),
        "Invalid IP string 999.1.2.3 must fail validation"
    );
}

#[test]
fn test_invalid_prefix_length_out_of_range_fails() {
    let ctx = create_yang_context();
    let bad_json = r#"{
      "ietf-interfaces:interfaces": {
        "interface": [
          {
            "name": "GigabitEthernet0/0",
            "type": "iana-if-type:ethernetCsmacd",
            "ietf-ip:ipv4": {
              "address": [
                {
                  "ip": "10.0.0.1",
                  "prefix-length": 35
                }
              ]
            }
          }
        ]
      }
    }"#;

    let res = validate_yang_json(&ctx, bad_json, "ietf-interfaces");
    assert!(res.is_err(), "Prefix length 35 (> 32) must fail validation");
}

#[test]
fn test_ospf_validation_and_invalid_area_id_fails() {
    let ctx = create_yang_context();

    // Valid combined JSON
    let valid_ospf_json = r#"{
      "ietf-interfaces:interfaces": {
        "interface": [
          {
            "name": "GigabitEthernet0/0",
            "type": "iana-if-type:ethernetCsmacd"
          }
        ]
      },
      "ietf-routing:routing": {
        "control-plane-protocols": {
          "control-plane-protocol": [
            {
              "type": "ietf-ospf:ospfv2",
              "name": "100",
              "ietf-ospf:ospf": {
                "explicit-router-id": "10.0.0.1",
                "areas": {
                  "area": [
                    {
                      "area-id": "0.0.0.0",
                      "interfaces": {
                        "interface": [
                          {
                            "name": "GigabitEthernet0/0",
                            "cost": 10,
                            "passive": false
                          }
                        ]
                      }
                    }
                  ]
                }
              }
            }
          ]
        }
      }
    }"#;
    assert!(validate_yang_json(&ctx, valid_ospf_json, "ietf-routing").is_ok());

    // Invalid area-id (not a dotted quad string in ietf-ospf)
    let invalid_area_json = r#"{
      "ietf-routing:routing": {
        "control-plane-protocols": {
          "control-plane-protocol": [
            {
              "type": "ietf-ospf:ospfv2",
              "name": "100",
              "ietf-ospf:ospf": {
                "areas": {
                  "area": [
                    {
                      "area-id": "not-an-ip-or-dotted-quad"
                    }
                  ]
                }
              }
            }
          ]
        }
      }
    }"#;
    assert!(validate_yang_json(&ctx, invalid_area_json, "ietf-routing").is_err());
}

#[test]
fn test_schema_drift_detection_on_unexpected_node() {
    let ctx = create_yang_context();

    // Simulating schema drift: unexpected node injected into ietf-interfaces:interfaces
    let drifted_json = r#"{
      "ietf-interfaces:interfaces": {
        "interface": [
          {
            "name": "eth0",
            "type": "iana-if-type:ethernetCsmacd",
            "unexpected-drift-leaf": 12345
          }
        ]
      }
    }"#;

    let res = validate_yang_json(&ctx, drifted_json, "ietf-interfaces");
    assert!(
        res.is_err(),
        "Schema drift with unexpected field must fail validation"
    );
}

#[test]
fn test_schema_drift_detection_on_unexpected_routing_protocol() {
    let ctx = create_yang_context();

    let drifted_json = r#"{
      "ietf-routing:routing": {
        "control-plane-protocols": {
          "control-plane-protocol": [
            {
              "type": "non-existent-module:unknown-protocol",
              "name": "test"
            }
          ]
        }
      }
    }"#;

    let res = validate_yang_json(&ctx, drifted_json, "ietf-routing");
    assert!(
        res.is_err(),
        "Unknown identity/protocol must fail validation"
    );
}

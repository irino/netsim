use super::*;

#[test]
fn test_prefix_length_valid() {
    assert_eq!(PrefixLength::new(0).unwrap().value(), 0);
    assert_eq!(PrefixLength::new(24).unwrap().value(), 24);
    assert_eq!(PrefixLength::new(32).unwrap().value(), 32);
}

#[test]
fn test_prefix_length_invalid() {
    let err = PrefixLength::new(33).unwrap_err();
    assert_eq!(err, IrError::InvalidPrefixLength(33));

    let err2 = PrefixLength::new(255).unwrap_err();
    assert_eq!(err2, IrError::InvalidPrefixLength(255));
}

#[test]
fn test_ipv4_prefix_parsing() {
    let prefix: Ipv4Prefix = "192.168.1.1/24".parse().unwrap();
    assert_eq!(prefix.address, std::net::Ipv4Addr::new(192, 168, 1, 1));
    assert_eq!(prefix.prefix_length, PrefixLength::new(24).unwrap());
    assert_eq!(prefix.to_string(), "192.168.1.1/24");

    assert!("192.168.1.1/33".parse::<Ipv4Prefix>().is_err());
    assert!("invalid".parse::<Ipv4Prefix>().is_err());
}

#[test]
fn test_serde_serialization_deserialization() {
    let config = DeviceConfig {
        hostname: "R1".to_string(),
        interfaces: vec![Interface {
            name: "eth0".to_string(),
            description: Some("Test interface".to_string()),
            enabled: true,
            ipv4: Some("10.0.0.1/24".parse().unwrap()),
            ospf: Some(InterfaceOspfConfig {
                area: OspfAreaId::Number(0),
                cost: Some(10),
                passive: Some(false),
            }),
        }],
        ospf: Some(OspfProcess {
            process_id: 1,
            router_id: Some(std::net::Ipv4Addr::new(1, 1, 1, 1)),
        }),
        bgp: Some(BgpProcess {
            as_number: 65001,
            router_id: Some(std::net::Ipv4Addr::new(1, 1, 1, 1)),
            neighbors: vec![BgpNeighbor {
                remote_address: std::net::Ipv4Addr::new(10, 0, 0, 2),
                remote_as: 65002,
            }],
            networks: vec!["10.0.0.0/24".parse().unwrap()],
        }),
        static_routes: vec![StaticRoute {
            destination: "192.168.0.0/16".parse().unwrap(),
            next_hop: std::net::Ipv4Addr::new(10, 0, 0, 2),
        }],
        prefix_lists: vec![PrefixList {
            name: "FILTER-BGP".to_string(),
            entries: vec![
                PrefixListEntry {
                    sequence: 10,
                    action: PolicyAction::Permit,
                    prefix: "10.0.0.0/8".parse().unwrap(),
                    range: Some(PrefixRange {
                        ge: Some(PrefixLength::new(16).unwrap()),
                        le: Some(PrefixLength::new(24).unwrap()),
                    }),
                },
                PrefixListEntry {
                    sequence: 20,
                    action: PolicyAction::Deny,
                    prefix: "0.0.0.0/0".parse().unwrap(),
                    range: Some(PrefixRange {
                        ge: None,
                        le: Some(PrefixLength::new(32).unwrap()),
                    }),
                },
            ],
        }],
        unparsed_lines: vec![UnparsedLine {
            file_path: Some("R1.cfg".to_string()),
            line_number: 10,
            content: "some unknown line".to_string(),
            reason: "unsupported statement".to_string(),
        }],
    };

    let json = serde_json::to_string_pretty(&config).unwrap();
    let deserialized: DeviceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}

#[test]
fn test_prefix_range_validation() {
    let p_len = PrefixLength::new(16).unwrap();

    // Valid cases
    let range1 = PrefixRange {
        ge: Some(PrefixLength::new(16).unwrap()),
        le: Some(PrefixLength::new(24).unwrap()),
    };
    assert!(range1.validate(p_len).is_ok());

    let range2 = PrefixRange {
        ge: None,
        le: Some(PrefixLength::new(24).unwrap()),
    };
    assert!(range2.validate(p_len).is_ok());

    let range3 = PrefixRange {
        ge: Some(PrefixLength::new(20).unwrap()),
        le: None,
    };
    assert!(range3.validate(p_len).is_ok());

    let range_none = PrefixRange { ge: None, le: None };
    assert!(range_none.validate(p_len).is_ok());

    // Invalid: ge < prefix_len
    let range_ge_small = PrefixRange {
        ge: Some(PrefixLength::new(12).unwrap()),
        le: None,
    };
    let err1 = range_ge_small.validate(p_len).unwrap_err();
    assert!(matches!(err1, IrError::InvalidPrefixRange(_)));
    assert!(err1
        .to_string()
        .contains("cannot be smaller than prefix length"));

    // Invalid: le < prefix_len
    let range_le_small = PrefixRange {
        ge: None,
        le: Some(PrefixLength::new(12).unwrap()),
    };
    let err2 = range_le_small.validate(p_len).unwrap_err();
    assert!(matches!(err2, IrError::InvalidPrefixRange(_)));

    // Invalid: ge > le
    let range_ge_gt_le = PrefixRange {
        ge: Some(PrefixLength::new(28).unwrap()),
        le: Some(PrefixLength::new(24).unwrap()),
    };
    let err3 = range_ge_gt_le.validate(p_len).unwrap_err();
    assert!(matches!(err3, IrError::InvalidPrefixRange(_)));
    assert!(err3.to_string().contains("cannot be greater than le"));
}

#[test]
fn test_prefix_list_serialization_shape() {
    let dev = DeviceConfig {
        hostname: "R1".to_string(),
        interfaces: vec![],
        ospf: None,
        bgp: None,
        static_routes: vec![],
        prefix_lists: vec![PrefixList {
            name: "FILTER-BGP".to_string(),
            entries: vec![
                PrefixListEntry {
                    sequence: 10,
                    action: PolicyAction::Permit,
                    prefix: "10.0.0.0/8".parse().unwrap(),
                    range: Some(PrefixRange {
                        ge: Some(PrefixLength::new(16).unwrap()),
                        le: Some(PrefixLength::new(24).unwrap()),
                    }),
                },
                PrefixListEntry {
                    sequence: 20,
                    action: PolicyAction::Deny,
                    prefix: "0.0.0.0/0".parse().unwrap(),
                    range: Some(PrefixRange {
                        ge: None,
                        le: Some(PrefixLength::new(32).unwrap()),
                    }),
                },
            ],
        }],
        unparsed_lines: vec![],
    };

    let json_val = serde_json::to_value(&dev).unwrap();
    let expected = serde_json::json!({
        "hostname": "R1",
        "prefix-lists": [
            {
                "name": "FILTER-BGP",
                "entries": [
                    {
                        "sequence": 10,
                        "action": "permit",
                        "prefix": "10.0.0.0/8",
                        "ge": 16,
                        "le": 24
                    },
                    {
                        "sequence": 20,
                        "action": "deny",
                        "prefix": "0.0.0.0/0",
                        "le": 32
                    }
                ]
            }
        ]
    });
    assert_eq!(json_val, expected);
}

#[test]
fn test_golden_minimal() {
    let content = std::fs::read_to_string("../../tests/golden/device_config_minimal.json")
        .or_else(|_| std::fs::read_to_string("tests/golden/device_config_minimal.json"))
        .unwrap();
    let config: DeviceConfig = serde_json::from_str(&content).unwrap();
    assert_eq!(config.hostname, "router-minimal");
    assert!(config.interfaces.is_empty());
    assert!(config.ospf.is_none());
    assert!(config.bgp.is_none());
    assert!(config.static_routes.is_empty());
    assert!(config.prefix_lists.is_empty());
    assert!(config.unparsed_lines.is_empty());
}

#[test]
fn test_golden_full() {
    let content = std::fs::read_to_string("../../tests/golden/device_config_full.json")
        .or_else(|_| std::fs::read_to_string("tests/golden/device_config_full.json"))
        .unwrap();
    let config: DeviceConfig = serde_json::from_str(&content).unwrap();
    assert_eq!(config.hostname, "router-1");
    assert_eq!(config.interfaces.len(), 2);
    assert_eq!(config.interfaces[0].name, "GigabitEthernet0/0");
    assert_eq!(
        config.interfaces[0].ipv4,
        Some("10.0.0.1/24".parse().unwrap())
    );
    assert_eq!(
        config.interfaces[0].ospf,
        Some(InterfaceOspfConfig {
            area: OspfAreaId::Number(0),
            cost: Some(10),
            passive: None,
        })
    );
    assert_eq!(
        config.ospf,
        Some(OspfProcess {
            process_id: 1,
            router_id: Some(std::net::Ipv4Addr::new(1, 1, 1, 1)),
        })
    );
    assert!(config.prefix_lists.is_empty());
    assert_eq!(config.unparsed_lines.len(), 1);
}

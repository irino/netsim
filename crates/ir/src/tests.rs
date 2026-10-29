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
fn test_golden_minimal() {
    let content = std::fs::read_to_string("../../tests/golden/device_config_minimal.json")
        .or_else(|_| std::fs::read_to_string("tests/golden/device_config_minimal.json"))
        .unwrap();
    let config: DeviceConfig = serde_json::from_str(&content).unwrap();
    assert_eq!(config.hostname, "router-minimal");
    assert!(config.interfaces.is_empty());
    assert!(config.ospf.is_none());
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
    assert_eq!(config.unparsed_lines.len(), 1);
}

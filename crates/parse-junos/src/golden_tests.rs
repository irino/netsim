use super::*;

#[test]
fn test_junos_golden_config() {
    let cfg = std::fs::read_to_string("../../tests/golden/junos_basic.set")
        .or_else(|_| std::fs::read_to_string("tests/golden/junos_basic.set"))
        .unwrap();
    let json_expected = std::fs::read_to_string("../../tests/golden/junos_basic.json")
        .or_else(|_| std::fs::read_to_string("tests/golden/junos_basic.json"))
        .unwrap();

    let device = parse_junos_config(&cfg, Some("junos_basic.set"));
    let expected_device: ir::DeviceConfig = serde_json::from_str(&json_expected).unwrap();

    assert_eq!(device, expected_device);
}

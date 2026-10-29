use super::*;

#[test]
fn test_cisco_ios_golden_config() {
    let cfg = std::fs::read_to_string("../../tests/golden/cisco_ios_basic.cfg")
        .or_else(|_| std::fs::read_to_string("tests/golden/cisco_ios_basic.cfg"))
        .unwrap();
    let json_expected = std::fs::read_to_string("../../tests/golden/cisco_ios_basic.json")
        .or_else(|_| std::fs::read_to_string("tests/golden/cisco_ios_basic.json"))
        .unwrap();

    let device = parse_ios_config(&cfg, Some("cisco_ios_basic.cfg"));
    let expected_device: ir::DeviceConfig = serde_json::from_str(&json_expected).unwrap();

    assert_eq!(device, expected_device);
}

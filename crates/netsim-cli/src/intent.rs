use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("I/O error reading intent file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse intent YAML: {0}")]
    ParseYaml(#[from] serde_yaml::Error),
    #[error("Invalid assertion syntax: '{0}' ({1})")]
    InvalidAssertion(String, String),
}

/// Target of a ping assertion: either a specific IP/address string, or the named router's loopback/interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PingTarget {
    Loopback(String), // e.g. "r5 loopback" or router's default loopback
    Router(String),   // e.g. "r5" (resolves to primary loopback or IP)
    Ip(String),       // e.g. "1.1.1.1" or "2001:db8::1"
}

/// Intent assertion item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    /// Ping assertion: `ping <from> -> <target>`
    Ping { from: String, to: PingTarget },
    /// Route table assertion: `route <node> has <prefix>`
    RouteExists { node: String, prefix: String },
}

/// Representation of the intent file (`intent.yml`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentConfig {
    /// Single assertion shorthand (e.g. `assert: ping r1 -> r5 loopback`)
    #[serde(default)]
    pub r#assert: Option<String>,

    /// List of string assertion shorthands (e.g. `assertions: ["ping r1 -> r5 loopback", ...]`)
    #[serde(default)]
    pub assertions: Vec<String>,

    /// Structured assertions list
    #[serde(default)]
    pub tests: Vec<Assertion>,
}

impl IntentConfig {
    /// Load and parse intent config from a YAML file.
    pub fn load_from_file(path: &Path) -> Result<Self, IntentError> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }

    /// Parse intent config from YAML string.
    pub fn load_from_str(content: &str) -> Result<Self, IntentError> {
        let raw: IntentConfig = serde_yaml::from_str(content)?;
        Ok(raw)
    }

    /// Resolve all assertions into concrete `Assertion` objects.
    pub fn compile_assertions(&self) -> Result<Vec<Assertion>, IntentError> {
        let mut list = self.tests.clone();

        if let Some(ref single) = self.r#assert {
            list.push(parse_assertion_string(single)?);
        }

        for item in &self.assertions {
            list.push(parse_assertion_string(item)?);
        }

        Ok(list)
    }
}

/// Parse string assertions like:
/// - `ping r1 -> r5 loopback`
/// - `ping r1 -> r5`
/// - `ping r1 -> 1.1.1.1`
/// - `route r1 has 10.0.0.0/24`
pub fn parse_assertion_string(input: &str) -> Result<Assertion, IntentError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(IntentError::InvalidAssertion(
            input.to_string(),
            "assertion string is empty".to_string(),
        ));
    }

    if let Some(rest) = trimmed.strip_prefix("ping ") {
        // Expected format: <from> -> <target>
        let parts: Vec<&str> = rest.split("->").map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(IntentError::InvalidAssertion(
                input.to_string(),
                "expected format 'ping <from> -> <to>'".to_string(),
            ));
        }

        let from = parts[0].to_string();
        if from.is_empty() {
            return Err(IntentError::InvalidAssertion(
                input.to_string(),
                "source node cannot be empty".to_string(),
            ));
        }

        let to_str = parts[1];
        let target = if let Some(router) = to_str.strip_suffix("loopback") {
            let r = router.trim();
            if r.is_empty() {
                return Err(IntentError::InvalidAssertion(
                    input.to_string(),
                    "missing router name before 'loopback'".to_string(),
                ));
            }
            PingTarget::Loopback(r.to_string())
        } else if to_str.parse::<std::net::IpAddr>().is_ok() {
            PingTarget::Ip(to_str.to_string())
        } else {
            PingTarget::Router(to_str.to_string())
        };

        Ok(Assertion::Ping { from, to: target })
    } else if let Some(rest) = trimmed.strip_prefix("route ") {
        // Expected format: <node> has <prefix>
        let parts: Vec<&str> = rest.split(" has ").map(|s| s.trim()).collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(IntentError::InvalidAssertion(
                input.to_string(),
                "expected format 'route <node> has <prefix>'".to_string(),
            ));
        }

        Ok(Assertion::RouteExists {
            node: parts[0].to_string(),
            prefix: parts[1].to_string(),
        })
    } else {
        Err(IntentError::InvalidAssertion(
            input.to_string(),
            "unrecognized assertion type (supported: 'ping ...', 'route ...')".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ping_string() {
        let a1 = parse_assertion_string("ping r1 -> r5 loopback").unwrap();
        assert_eq!(
            a1,
            Assertion::Ping {
                from: "r1".to_string(),
                to: PingTarget::Loopback("r5".to_string()),
            }
        );

        let a2 = parse_assertion_string("ping r1 -> r2").unwrap();
        assert_eq!(
            a2,
            Assertion::Ping {
                from: "r1".to_string(),
                to: PingTarget::Router("r2".to_string()),
            }
        );

        let a3 = parse_assertion_string("ping r1 -> 1.1.1.1").unwrap();
        assert_eq!(
            a3,
            Assertion::Ping {
                from: "r1".to_string(),
                to: PingTarget::Ip("1.1.1.1".to_string()),
            }
        );

        let a4 = parse_assertion_string("ping r1 -> 2001:db8::1").unwrap();
        assert_eq!(
            a4,
            Assertion::Ping {
                from: "r1".to_string(),
                to: PingTarget::Ip("2001:db8::1".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_route_string() {
        let a1 = parse_assertion_string("route r1 has 10.0.0.0/24").unwrap();
        assert_eq!(
            a1,
            Assertion::RouteExists {
                node: "r1".to_string(),
                prefix: "10.0.0.0/24".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_invalid_string() {
        assert!(parse_assertion_string("").is_err());
        assert!(parse_assertion_string("invalid something").is_err());
        assert!(parse_assertion_string("ping r1").is_err());
        assert!(parse_assertion_string("route r1").is_err());
    }

    #[test]
    fn test_yaml_parsing_and_compile() {
        let yaml = r#"
assert: ping r1 -> r5 loopback
assertions:
  - ping r2 -> r3
  - route r3 has 10.0.0.0/24
"#;
        let cfg = IntentConfig::load_from_str(yaml).unwrap();
        let compiled = cfg.compile_assertions().unwrap();
        assert_eq!(compiled.len(), 3);
        assert_eq!(
            compiled[0],
            Assertion::Ping {
                from: "r1".to_string(),
                to: PingTarget::Loopback("r5".to_string()),
            }
        );
        assert_eq!(
            compiled[1],
            Assertion::Ping {
                from: "r2".to_string(),
                to: PingTarget::Router("r3".to_string()),
            }
        );
        assert_eq!(
            compiled[2],
            Assertion::RouteExists {
                node: "r3".to_string(),
                prefix: "10.0.0.0/24".to_string(),
            }
        );
    }
}

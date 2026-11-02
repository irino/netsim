use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;
use thiserror::Error;

/// Error type for IR validation and parsing operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IrError {
    #[error("invalid prefix length {0}, must be between 0 and 32")]
    InvalidPrefixLength(u8),
    #[error("invalid IPv4 prefix format: '{0}'")]
    InvalidIpv4Prefix(String),
    #[error("invalid prefix range: {0}")]
    InvalidPrefixRange(String),
}

/// Validated IPv4 prefix length (0..=32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefixLength(u8);

impl PrefixLength {
    pub fn new(len: u8) -> Result<Self, IrError> {
        if len <= 32 {
            Ok(Self(len))
        } else {
            Err(IrError::InvalidPrefixLength(len))
        }
    }

    pub fn value(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for PrefixLength {
    type Error = IrError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PrefixLength> for u8 {
    fn from(prefix: PrefixLength) -> Self {
        prefix.0
    }
}

impl fmt::Display for PrefixLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for PrefixLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for PrefixLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let len = u8::deserialize(deserializer)?;
        PrefixLength::new(len).map_err(de::Error::custom)
    }
}

/// IPv4 address and prefix length pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Ipv4Prefix {
    pub address: Ipv4Addr,
    pub prefix_length: PrefixLength,
}

impl Ipv4Prefix {
    pub fn new(address: Ipv4Addr, prefix_length: PrefixLength) -> Self {
        Self {
            address,
            prefix_length,
        }
    }

    /// Calculate the network address for this prefix.
    pub fn network_address(&self) -> Ipv4Addr {
        let ip_u32 = u32::from(self.address);
        let len = self.prefix_length.value();
        let mask = if len == 0 {
            0
        } else {
            !((1u64 << (32 - len)) - 1) as u32
        };
        Ipv4Addr::from(ip_u32 & mask)
    }

    /// Returns the network prefix (network address + prefix length).
    pub fn network_prefix(&self) -> Ipv4Prefix {
        Ipv4Prefix {
            address: self.network_address(),
            prefix_length: self.prefix_length,
        }
    }
}

impl FromStr for Ipv4Prefix {
    type Err = IrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(IrError::InvalidIpv4Prefix(s.to_string()));
        }
        let address = parts[0]
            .parse::<Ipv4Addr>()
            .map_err(|_| IrError::InvalidIpv4Prefix(s.to_string()))?;
        let len = parts[1]
            .parse::<u8>()
            .map_err(|_| IrError::InvalidIpv4Prefix(s.to_string()))?;
        let prefix_length = PrefixLength::new(len)?;
        Ok(Ipv4Prefix {
            address,
            prefix_length,
        })
    }
}

impl fmt::Display for Ipv4Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix_length)
    }
}

impl<'de> Deserialize<'de> for Ipv4Prefix {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct Ipv4PrefixFields {
            address: Ipv4Addr,
            prefix_length: PrefixLength,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Ipv4PrefixHelper {
            Str(String),
            Obj(Ipv4PrefixFields),
        }

        match Ipv4PrefixHelper::deserialize(deserializer)? {
            Ipv4PrefixHelper::Str(s) => s.parse::<Ipv4Prefix>().map_err(de::Error::custom),
            Ipv4PrefixHelper::Obj(fields) => Ok(Ipv4Prefix {
                address: fields.address,
                prefix_length: fields.prefix_length,
            }),
        }
    }
}

/// OSPF Area Identifier, representable as a 32-bit integer or an IPv4 dotted-quad address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OspfAreaId {
    Number(u32),
    Ipv4(Ipv4Addr),
}

impl fmt::Display for OspfAreaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OspfAreaId::Number(n) => write!(f, "{}", n),
            OspfAreaId::Ipv4(ip) => write!(f, "{}", ip),
        }
    }
}

/// OSPF configuration for an interface.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InterfaceOspfConfig {
    pub area: OspfAreaId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive: Option<bool>,
}

/// Network interface configuration aligned with ietf-interfaces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Interface {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4: Option<Ipv4Prefix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf: Option<InterfaceOspfConfig>,
}

impl Interface {
    /// Check if this interface is a loopback interface based on standard naming conventions.
    pub fn is_loopback(&self) -> bool {
        let name_lower = self.name.to_lowercase();
        name_lower.starts_with("loopback")
            || name_lower.starts_with("lo")
            || name_lower.starts_with("lo0")
    }
}

fn default_true() -> bool {
    true
}

/// OSPF process configuration aligned with ietf-routing / ietf-ospf.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct OspfProcess {
    pub process_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_id: Option<Ipv4Addr>,
}

/// Static IPv4 route configuration aligned with ietf-routing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StaticRoute {
    pub destination: Ipv4Prefix,
    pub next_hop: Ipv4Addr,
}

/// BGP neighbor peer configuration aligned with ietf-bgp.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BgpNeighbor {
    pub remote_address: Ipv4Addr,
    pub remote_as: u32,
}

/// BGP process configuration aligned with ietf-routing / ietf-bgp.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BgpProcess {
    pub as_number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_id: Option<Ipv4Addr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neighbors: Vec<BgpNeighbor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<Ipv4Prefix>,
}

/// Action for a routing policy or prefix-list entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyAction {
    Permit,
    Deny,
}

/// Prefix length range constraint for prefix matching (ge, le).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ge: Option<PrefixLength>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub le: Option<PrefixLength>,
}

impl PrefixRange {
    /// Validate prefix-range constraints against the prefix length:
    /// - If `ge` is set, `prefix_length <= ge <= 32`.
    /// - If `le` is set, `prefix_length <= le <= 32`.
    /// - If both `ge` and `le` are set, `ge <= le`.
    pub fn validate(&self, prefix_len: PrefixLength) -> Result<(), IrError> {
        let p_len = prefix_len.value();
        if let Some(ge) = self.ge {
            if ge.value() < p_len {
                return Err(IrError::InvalidPrefixRange(format!(
                    "ge {} cannot be smaller than prefix length {}",
                    ge.value(),
                    p_len
                )));
            }
        }
        if let Some(le) = self.le {
            if le.value() < p_len {
                return Err(IrError::InvalidPrefixRange(format!(
                    "le {} cannot be smaller than prefix length {}",
                    le.value(),
                    p_len
                )));
            }
        }
        if let (Some(ge), Some(le)) = (self.ge, self.le) {
            if ge.value() > le.value() {
                return Err(IrError::InvalidPrefixRange(format!(
                    "ge {} cannot be greater than le {}",
                    ge.value(),
                    le.value()
                )));
            }
        }
        Ok(())
    }
}

/// Serialize `Ipv4Prefix` as a string (`10.0.0.0/8`).
pub mod ipv4_prefix_str {
    use super::Ipv4Prefix;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(prefix: &Ipv4Prefix, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&prefix.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Ipv4Prefix, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<Ipv4Prefix>().map_err(de::Error::custom)
    }
}

/// A single entry in an IPv4 prefix-list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixListEntry {
    pub sequence: u32,
    pub action: PolicyAction,
    #[serde(with = "ipv4_prefix_str")]
    pub prefix: Ipv4Prefix,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub range: Option<PrefixRange>,
}

/// An IPv4 prefix list containing ordered entries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixList {
    pub name: String,
    pub entries: Vec<PrefixListEntry>,
}

/// Unparsed or unsupported configuration line metadata.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UnparsedLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub line_number: usize,
    pub content: String,
    pub reason: String,
}

/// Root vendor-independent device configuration representation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DeviceConfig {
    pub hostname: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<Interface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf: Option<OspfProcess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bgp: Option<BgpProcess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_routes: Vec<StaticRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_lists: Vec<PrefixList>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unparsed_lines: Vec<UnparsedLine>,
}

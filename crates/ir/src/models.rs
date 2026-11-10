use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};
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
    #[error("invalid IPv6 prefix length {0}, must be between 0 and 128")]
    InvalidIpv6PrefixLength(u8),
    #[error("invalid IPv6 prefix format: '{0}'")]
    InvalidIpv6Prefix(String),
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

/// Validated IPv6 prefix length (0..=128).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ipv6PrefixLength(u8);

impl Ipv6PrefixLength {
    pub fn new(len: u8) -> Result<Self, IrError> {
        if len <= 128 {
            Ok(Self(len))
        } else {
            Err(IrError::InvalidIpv6PrefixLength(len))
        }
    }

    pub fn value(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Ipv6PrefixLength {
    type Error = IrError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Ipv6PrefixLength> for u8 {
    fn from(prefix: Ipv6PrefixLength) -> Self {
        prefix.0
    }
}

impl fmt::Display for Ipv6PrefixLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Ipv6PrefixLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Ipv6PrefixLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let len = u8::deserialize(deserializer)?;
        Ipv6PrefixLength::new(len).map_err(de::Error::custom)
    }
}

/// IPv6 address and prefix length pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Ipv6Prefix {
    pub address: Ipv6Addr,
    pub prefix_length: Ipv6PrefixLength,
}

impl Ipv6Prefix {
    pub fn new(address: Ipv6Addr, prefix_length: Ipv6PrefixLength) -> Self {
        Self {
            address,
            prefix_length,
        }
    }

    /// Calculate the network address for this IPv6 prefix.
    pub fn network_address(&self) -> Ipv6Addr {
        let ip_u128 = u128::from(self.address);
        let len = self.prefix_length.value();
        let mask = if len == 0 {
            0
        } else if len >= 128 {
            u128::MAX
        } else {
            !((1u128 << (128 - len)) - 1)
        };
        Ipv6Addr::from(ip_u128 & mask)
    }

    /// Returns the network prefix (network address + prefix length).
    pub fn network_prefix(&self) -> Ipv6Prefix {
        Ipv6Prefix {
            address: self.network_address(),
            prefix_length: self.prefix_length,
        }
    }
}

impl FromStr for Ipv6Prefix {
    type Err = IrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(IrError::InvalidIpv6Prefix(s.to_string()));
        }
        let address = parts[0]
            .parse::<Ipv6Addr>()
            .map_err(|_| IrError::InvalidIpv6Prefix(s.to_string()))?;
        let len = parts[1]
            .parse::<u8>()
            .map_err(|_| IrError::InvalidIpv6Prefix(s.to_string()))?;
        let prefix_length = Ipv6PrefixLength::new(len)?;
        Ok(Ipv6Prefix {
            address,
            prefix_length,
        })
    }
}

impl fmt::Display for Ipv6Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix_length)
    }
}

impl<'de> Deserialize<'de> for Ipv6Prefix {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct Ipv6PrefixFields {
            address: Ipv6Addr,
            prefix_length: Ipv6PrefixLength,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Ipv6PrefixHelper {
            Str(String),
            Obj(Ipv6PrefixFields),
        }

        match Ipv6PrefixHelper::deserialize(deserializer)? {
            Ipv6PrefixHelper::Str(s) => s.parse::<Ipv6Prefix>().map_err(de::Error::custom),
            Ipv6PrefixHelper::Obj(fields) => Ok(Ipv6Prefix {
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

/// IS-IS level for an interface or process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IsisLevel {
    Level1,
    Level2,
    Level12,
}

/// IS-IS configuration for an interface.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InterfaceIsisConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<IsisLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<bool>,
}

/// IS-IS routing process configuration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IsisProcess {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<IsisLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub te_router_id: Option<Ipv4Addr>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<bool>,
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
    pub ipv6: Option<Ipv6Prefix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospf: Option<InterfaceOspfConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ospfv3: Option<InterfaceOspfConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isis: Option<InterfaceIsisConfig>,
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

/// OSPFv3 process configuration aligned with ietf-routing / ietf-ospf.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Ospf6Process {
    pub process_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_id: Option<Ipv4Addr>,
}

pub type OspfV3Process = Ospf6Process;

/// Static IPv4 route configuration aligned with ietf-routing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StaticRoute {
    pub destination: Ipv4Prefix,
    pub next_hop: Ipv4Addr,
}

/// Static IPv6 route configuration aligned with ietf-routing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Ipv6StaticRoute {
    pub destination: Ipv6Prefix,
    pub next_hop: Ipv6Addr,
}

/// BGP peer group / template configuration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BgpPeerGroup {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_as: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector_client: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<bool>,
}

/// BGP neighbor peer configuration aligned with ietf-bgp.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BgpNeighbor {
    pub remote_address: Ipv4Addr,
    pub remote_as: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector_client: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<bool>,
}

/// BGP IPv6 neighbor peer configuration aligned with ietf-bgp.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BgpNeighborV6 {
    pub remote_address: Ipv6Addr,
    pub remote_as: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reflector_client: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfd: Option<bool>,
}

/// BGP process configuration aligned with ietf-routing / ietf-bgp.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BgpProcess {
    pub as_number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_id: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Ipv4Addr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peer_groups: Vec<BgpPeerGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neighbors: Vec<BgpNeighbor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<Ipv4Prefix>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neighbors_v6: Vec<BgpNeighborV6>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks_v6: Vec<Ipv6Prefix>,
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

/// Prefix length range constraint for IPv6 prefix matching (ge, le).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixRangeV6 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ge: Option<Ipv6PrefixLength>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub le: Option<Ipv6PrefixLength>,
}

impl PrefixRangeV6 {
    /// Validate prefix-range constraints against the IPv6 prefix length:
    /// - If `ge` is set, `prefix_length <= ge <= 128`.
    /// - If `le` is set, `prefix_length <= le <= 128`.
    /// - If both `ge` and `le` are set, `ge <= le`.
    pub fn validate(&self, prefix_len: Ipv6PrefixLength) -> Result<(), IrError> {
        let p_len = prefix_len.value();
        if let Some(ge) = self.ge {
            if ge.value() < p_len {
                return Err(IrError::InvalidPrefixRange(format!(
                    "ge {} cannot be smaller than IPv6 prefix length {}",
                    ge.value(),
                    p_len
                )));
            }
        }
        if let Some(le) = self.le {
            if le.value() < p_len {
                return Err(IrError::InvalidPrefixRange(format!(
                    "le {} cannot be smaller than IPv6 prefix length {}",
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

/// Serialize `Ipv6Prefix` as a string (`2001:db8::/32`).
pub mod ipv6_prefix_str {
    use super::Ipv6Prefix;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(prefix: &Ipv6Prefix, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&prefix.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Ipv6Prefix, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<Ipv6Prefix>().map_err(de::Error::custom)
    }
}

/// A single entry in an IPv4 prefix-list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixListEntry {
    pub sequence: u32,
    pub action: PolicyAction,
    #[serde(with = "ipv4_prefix_str")]
    pub prefix: Ipv4Prefix,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub range: Option<PrefixRange>,
}

impl<'de> Deserialize<'de> for PrefixListEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct RawEntry {
            sequence: u32,
            action: PolicyAction,
            #[serde(with = "ipv4_prefix_str")]
            prefix: Ipv4Prefix,
            #[serde(flatten)]
            range: Option<PrefixRange>,
        }

        let raw = RawEntry::deserialize(deserializer)?;
        let range = raw.range.filter(|r| r.ge.is_some() || r.le.is_some());

        Ok(PrefixListEntry {
            sequence: raw.sequence,
            action: raw.action,
            prefix: raw.prefix,
            range,
        })
    }
}

/// An IPv4 prefix list containing ordered entries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixList {
    pub name: String,
    pub entries: Vec<PrefixListEntry>,
}

/// A single entry in an IPv6 prefix-list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixListEntryV6 {
    pub sequence: u32,
    pub action: PolicyAction,
    #[serde(with = "ipv6_prefix_str")]
    pub prefix: Ipv6Prefix,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub range: Option<PrefixRangeV6>,
}

impl<'de> Deserialize<'de> for PrefixListEntryV6 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct RawEntryV6 {
            sequence: u32,
            action: PolicyAction,
            #[serde(with = "ipv6_prefix_str")]
            prefix: Ipv6Prefix,
            #[serde(flatten)]
            range: Option<PrefixRangeV6>,
        }

        let raw = RawEntryV6::deserialize(deserializer)?;
        let range = raw.range.filter(|r| r.ge.is_some() || r.le.is_some());

        Ok(PrefixListEntryV6 {
            sequence: raw.sequence,
            action: raw.action,
            prefix: raw.prefix,
            range,
        })
    }
}

/// An IPv6 prefix list containing ordered entries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PrefixListV6 {
    pub name: String,
    pub entries: Vec<PrefixListEntryV6>,
}

/// A single entry in a BGP community-list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CommunityListEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
    pub action: PolicyAction,
    pub communities: Vec<String>,
}

/// Named BGP community-list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CommunityList {
    pub name: String,
    pub entries: Vec<CommunityListEntry>,
}
/// Conditions for a routing policy statement aligned with RFC 9067 (`ietf-routing-policy`).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyConditions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_prefix_set: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_prefix_set_v6: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_community: Option<String>,
}

/// Actions for a routing policy statement aligned with RFC 9067 (`ietf-routing-policy`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyActions {
    pub policy_result: PolicyAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_metric: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_local_preference: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_community: Option<String>,
}

impl Default for PolicyActions {
    fn default() -> Self {
        Self {
            policy_result: PolicyAction::Permit,
            set_metric: None,
            set_local_preference: None,
            set_community: None,
        }
    }
}

/// A statement/term in a routing policy definition aligned with RFC 9067 (`ietf-routing-policy`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyStatement {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default_conditions")]
    pub conditions: PolicyConditions,
    #[serde(default)]
    pub actions: PolicyActions,
}

fn is_default_conditions(cond: &PolicyConditions) -> bool {
    cond.match_prefix_set.is_none() && cond.match_prefix_set_v6.is_none()
}

/// A routing policy definition (route-map / policy-statement / route-policy) aligned with RFC 9067 (`ietf-routing-policy`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyDefinition {
    pub name: String,
    pub statements: Vec<PolicyStatement>,
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
    pub ospfv3: Option<Ospf6Process>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isis: Option<IsisProcess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bgp: Option<BgpProcess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_routes: Vec<StaticRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_routes_v6: Vec<Ipv6StaticRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_lists: Vec<PrefixList>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_lists_v6: Vec<PrefixListV6>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub community_lists: Vec<CommunityList>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<PolicyDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unparsed_lines: Vec<UnparsedLine>,
}

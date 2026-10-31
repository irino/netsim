use ir::DeviceConfig;
use std::collections::BTreeMap;

/// Interface mapper that deterministically maps vendor interface names to Linux interface names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterfaceMapper {
    /// Mapping of (hostname, vendor_interface_name) -> linux_interface_name (e.g. "eth1")
    mappings: BTreeMap<(String, String), String>,
}

impl InterfaceMapper {
    /// Build interface mapper from a slice of devices.
    /// Excludes loopback interfaces (lo/loopback remain lo inside container).
    /// Physical/data interfaces are sorted alphabetically by vendor interface name and assigned eth1, eth2, ...
    pub fn build(devices: &[DeviceConfig]) -> Self {
        let mut mappings = BTreeMap::new();

        for dev in devices {
            let mut ifaces: Vec<&String> = dev
                .interfaces
                .iter()
                .filter(|iface| !iface.is_loopback())
                .map(|iface| &iface.name)
                .collect();

            // Deterministic sort
            ifaces.sort();
            ifaces.dedup();

            for (idx, vendor_name) in ifaces.into_iter().enumerate() {
                let linux_name = format!("eth{}", idx + 1);
                mappings.insert((dev.hostname.clone(), vendor_name.clone()), linux_name);
            }
        }

        Self { mappings }
    }

    /// Look up the Linux interface name for a device and vendor interface name.
    pub fn get_linux_name(&self, hostname: &str, vendor_name: &str) -> Option<&str> {
        self.mappings
            .get(&(hostname.to_string(), vendor_name.to_string()))
            .map(|s| s.as_str())
    }

    /// Return all mapped interfaces for a given device.
    pub fn device_mappings(&self, hostname: &str) -> BTreeMap<String, String> {
        let mut res = BTreeMap::new();
        for ((host, vendor), linux) in &self.mappings {
            if host == hostname {
                res.insert(vendor.clone(), linux.clone());
            }
        }
        res
    }
}

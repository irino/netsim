use crate::intent::{Assertion, IntentConfig, PingTarget};
use emit_clab::ClabTopology;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Topology file '{0}' does not exist")]
    TopologyNotFound(PathBuf),
    #[error("Failed to parse Containerlab topology YAML '{0}': {1}")]
    ClabParseError(PathBuf, serde_yaml::Error),
    #[error("Failed to parse zebra YAML '{0}': {1}")]
    ZebraParseError(PathBuf, serde_yaml::Error),
    #[error("Deployment failed: {0}")]
    DeployFailed(String),
    #[error("Destruction failed: {0}")]
    DestroyFailed(String),
    #[error("Routing convergence timeout after {0:?}: {1}")]
    ConvergenceTimeout(Duration, String),
    #[error("Data-plane ping test failed: {0}")]
    PingFailed(String),
    #[error("Intent assertion failed: {0}")]
    IntentFailed(String),
    #[error("Command execution error: {0}")]
    Exec(String),
}

/// Output of an executed external command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status_code == 0
    }
}

/// Trait abstracting command execution (containerlab, docker, etc.) to allow unit/mock testing.
pub trait CommandRunner: Send + Sync {
    fn run_command(&self, program: &str, args: &[&str]) -> Result<CommandOutput, VerifyError>;
}

/// Real command runner invoking `std::process::Command`.
#[derive(Debug, Default, Clone)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run_command(&self, program: &str, args: &[&str]) -> Result<CommandOutput, VerifyError> {
        let output = Command::new(program).args(args).output().map_err(|e| {
            VerifyError::Exec(format!("Failed to execute '{} {:?}': {}", program, args, e))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status_code = output.status.code().unwrap_or(-1);

        Ok(CommandOutput {
            status_code,
            stdout,
            stderr,
        })
    }
}

/// Configuration options for `netsim verify`.
#[derive(Debug, Clone)]
pub struct VerifyOptions {
    pub lab_dir: PathBuf,
    pub topology_file: Option<PathBuf>,
    pub intent_file: Option<PathBuf>,
    pub timeout: Duration,
    pub interval: Duration,
    pub skip_destroy: bool,
    pub sudo: bool,
    pub clab_bin: String,
    pub docker_bin: String,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            lab_dir: PathBuf::from("."),
            topology_file: None,
            intent_file: None,
            timeout: Duration::from_secs(60),
            interval: Duration::from_secs(2),
            skip_destroy: false,
            sudo: false,
            clab_bin: "containerlab".to_string(),
            docker_bin: "docker".to_string(),
        }
    }
}

/// Helper struct for partially deserializing zebra-rs YAML configs to extract loopbacks and addresses.
#[derive(Debug, Deserialize)]
struct ZebraYamlInterface {
    name: String,
    #[serde(default)]
    ipv4: Option<ZebraYamlIpv4>,
    #[serde(default)]
    ipv6: Option<ZebraYamlIpv6>,
}

#[derive(Debug, Deserialize)]
struct ZebraYamlIpv4 {
    #[serde(default)]
    address: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ZebraYamlIpv6 {
    #[serde(default)]
    address: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ZebraYamlRoot {
    #[serde(default)]
    interface: Vec<ZebraYamlInterface>,
}

/// Discovered device network information from lab directory.
#[derive(Debug, Clone, Default)]
pub struct DeviceNetworkInfo {
    pub hostname: String,
    pub container_name: String,
    pub loopback_v4: Option<IpAddr>,
    pub loopback_v6: Option<IpAddr>,
    pub all_ips_v4: Vec<IpAddr>,
    pub all_ips_v6: Vec<IpAddr>,
}

/// Inspect lab directory and parse `topology.clab.yml` and `<hostname>.zebra.yaml` files.
pub fn inspect_lab(
    lab_dir: &Path,
    topology_override: Option<&Path>,
) -> Result<(ClabTopology, BTreeMap<String, DeviceNetworkInfo>), VerifyError> {
    let topo_path = match topology_override {
        Some(p) => p.to_path_buf(),
        None => lab_dir.join("topology.clab.yml"),
    };

    if !topo_path.exists() {
        return Err(VerifyError::TopologyNotFound(topo_path));
    }

    let topo_content = std::fs::read_to_string(&topo_path)?;
    let clab: ClabTopology = serde_yaml::from_str(&topo_content)
        .map_err(|e| VerifyError::ClabParseError(topo_path.clone(), e))?;

    let lab_name = &clab.name;
    let mut devices = BTreeMap::new();

    for (node_name, node_def) in &clab.topology.nodes {
        // Skip bridges (which are not routing daemons / zebra-rs containers)
        if node_def.kind == "bridge" {
            continue;
        }

        let container_name = format!("clab-{}-{}", lab_name, node_name);
        let mut dev_info = DeviceNetworkInfo {
            hostname: node_name.clone(),
            container_name,
            loopback_v4: None,
            loopback_v6: None,
            all_ips_v4: Vec::new(),
            all_ips_v6: Vec::new(),
        };

        let zebra_file = lab_dir.join(format!("{}.zebra.yaml", node_name));
        if zebra_file.exists() {
            let zebra_content = std::fs::read_to_string(&zebra_file)?;
            if let Ok(zebra_root) = serde_yaml::from_str::<ZebraYamlRoot>(&zebra_content) {
                for iface in zebra_root.interface {
                    let is_loopback = iface.name.to_lowercase().starts_with("lo");

                    if let Some(v4) = iface.ipv4 {
                        for addr_str in v4.address {
                            if let Some(ip_str) = addr_str.split('/').next() {
                                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                                    dev_info.all_ips_v4.push(ip);
                                    if is_loopback && dev_info.loopback_v4.is_none() {
                                        dev_info.loopback_v4 = Some(ip);
                                    }
                                }
                            }
                        }
                    }

                    if let Some(v6) = iface.ipv6 {
                        for addr_str in v6.address {
                            if let Some(ip_str) = addr_str.split('/').next() {
                                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                                    dev_info.all_ips_v6.push(ip);
                                    if is_loopback && dev_info.loopback_v6.is_none() {
                                        dev_info.loopback_v6 = Some(ip);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        devices.insert(node_name.clone(), dev_info);
    }

    Ok((clab, devices))
}

/// Verification runner executing deployment, polling, loopback pings, intent assertions, and destroy.
pub struct LabVerifier<'a> {
    options: &'a VerifyOptions,
    runner: &'a dyn CommandRunner,
}

impl<'a> LabVerifier<'a> {
    pub fn new(options: &'a VerifyOptions, runner: &'a dyn CommandRunner) -> Self {
        Self { options, runner }
    }

    fn exec(&self, program: &str, args: &[&str]) -> Result<CommandOutput, VerifyError> {
        if self.options.sudo {
            let mut sudo_args = vec![program];
            sudo_args.extend_from_slice(args);
            self.runner.run_command("sudo", &sudo_args)
        } else {
            self.runner.run_command(program, args)
        }
    }

    /// Deploy containerlab topology.
    pub fn deploy(&self, topo_file: &Path) -> Result<(), VerifyError> {
        let topo_str = topo_file.to_string_lossy();
        let args = ["deploy", "-t", &topo_str];
        println!("==> Deploying Containerlab topology: {}", topo_str);
        let out = self.exec(&self.options.clab_bin, &args)?;
        if !out.success() {
            return Err(VerifyError::DeployFailed(format!(
                "exit code: {}\nstderr: {}",
                out.status_code, out.stderr
            )));
        }
        println!("==> Deployment completed successfully.");
        Ok(())
    }

    /// Destroy containerlab topology.
    pub fn destroy(&self, topo_file: &Path) -> Result<(), VerifyError> {
        let topo_str = topo_file.to_string_lossy();
        let args = ["destroy", "-t", &topo_str, "--cleanup"];
        println!("==> Destroying Containerlab topology: {}", topo_str);
        let out = self.exec(&self.options.clab_bin, &args)?;
        if !out.success() {
            return Err(VerifyError::DestroyFailed(format!(
                "exit code: {}\nstderr: {}",
                out.status_code, out.stderr
            )));
        }
        println!("==> Lab teardown completed.");
        Ok(())
    }

    /// Query route table of a container using `vtyctl show ip route` or `vtyctl show ipv6 route`.
    pub fn get_routes(&self, container: &str, ipv6: bool) -> Result<String, VerifyError> {
        let sub = if ipv6 { "ipv6" } else { "ip" };
        let args = ["exec", container, "vtyctl", "show", sub, "route"];
        let out = self.exec(&self.options.docker_bin, &args)?;
        if !out.success() {
            return Err(VerifyError::Exec(format!(
                "vtyctl route query failed for container '{}': {}",
                container, out.stderr
            )));
        }
        Ok(out.stdout)
    }

    /// Check if target IP / prefix is in the routing table output.
    pub fn has_route(route_output: &str, target: &str) -> bool {
        for line in route_output.lines() {
            let trimmed = line.trim();
            if trimmed.contains(target) {
                return true;
            }
        }
        false
    }

    /// Poll until all expected loopback addresses appear in all nodes' routing tables or timeout occurs.
    pub fn wait_for_convergence(
        &self,
        devices: &BTreeMap<String, DeviceNetworkInfo>,
    ) -> Result<(), VerifyError> {
        println!(
            "==> Waiting for routing protocol convergence (timeout: {:?}, interval: {:?})...",
            self.options.timeout, self.options.interval
        );

        let start = Instant::now();

        // Collect all target loopbacks across devices
        let all_v4_loopbacks: Vec<(String, IpAddr)> = devices
            .iter()
            .filter_map(|(host, info)| info.loopback_v4.map(|ip| (host.clone(), ip)))
            .collect();

        let all_v6_loopbacks: Vec<(String, IpAddr)> = devices
            .iter()
            .filter_map(|(host, info)| info.loopback_v6.map(|ip| (host.clone(), ip)))
            .collect();

        loop {
            let mut all_converged = true;
            let mut pending_summary = Vec::new();

            for (hostname, dev) in devices {
                // Check IPv4 loopbacks from peers
                if !all_v4_loopbacks.is_empty() {
                    match self.get_routes(&dev.container_name, false) {
                        Ok(v4_routes) => {
                            for (peer_host, peer_ip) in &all_v4_loopbacks {
                                if peer_host != hostname {
                                    let ip_s = peer_ip.to_string();
                                    if !Self::has_route(&v4_routes, &ip_s) {
                                        all_converged = false;
                                        pending_summary.push(format!(
                                            "{} missing IPv4 route to {} ({})",
                                            hostname, peer_host, ip_s
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            all_converged = false;
                            pending_summary
                                .push(format!("{} error reading IPv4 routes: {}", hostname, e));
                        }
                    }
                }

                // Check IPv6 loopbacks from peers
                if !all_v6_loopbacks.is_empty() {
                    match self.get_routes(&dev.container_name, true) {
                        Ok(v6_routes) => {
                            for (peer_host, peer_ip) in &all_v6_loopbacks {
                                if peer_host != hostname {
                                    let ip_s = peer_ip.to_string();
                                    if !Self::has_route(&v6_routes, &ip_s) {
                                        all_converged = false;
                                        pending_summary.push(format!(
                                            "{} missing IPv6 route to {} ({})",
                                            hostname, peer_host, ip_s
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            all_converged = false;
                            pending_summary
                                .push(format!("{} error reading IPv6 routes: {}", hostname, e));
                        }
                    }
                }
            }

            if all_converged {
                println!(
                    "==> Routing protocols converged successfully in {:.2?}s!",
                    start.elapsed()
                );
                return Ok(());
            }

            if start.elapsed() >= self.options.timeout {
                return Err(VerifyError::ConvergenceTimeout(
                    self.options.timeout,
                    pending_summary.join("; "),
                ));
            }

            sleep(self.options.interval);
        }
    }

    /// Execute ping from a container to target IP address.
    pub fn ping(
        &self,
        from_container: &str,
        target_ip: &str,
        count: u32,
    ) -> Result<(), VerifyError> {
        let count_str = count.to_string();
        let args = ["exec", from_container, "ping", "-c", &count_str, target_ip];
        let out = self.exec(&self.options.docker_bin, &args)?;
        if !out.success() {
            return Err(VerifyError::PingFailed(format!(
                "Ping from '{}' to '{}' failed (code: {}): {}",
                from_container, target_ip, out.status_code, out.stderr
            )));
        }
        Ok(())
    }

    /// Perform full data-plane mesh ping across all inferred loopbacks.
    pub fn verify_loopback_mesh(
        &self,
        devices: &BTreeMap<String, DeviceNetworkInfo>,
    ) -> Result<(), VerifyError> {
        println!("==> Running data-plane loopback ping tests...");

        // IPv4 loopback pings
        let v4_nodes: Vec<(&String, &DeviceNetworkInfo)> = devices
            .iter()
            .filter(|(_, d)| d.loopback_v4.is_some())
            .collect();

        for (src_host, src_dev) in &v4_nodes {
            for (dst_host, dst_dev) in &v4_nodes {
                if src_host != dst_host {
                    let target_ip = dst_dev.loopback_v4.unwrap().to_string();
                    print!(
                        "    ping (IPv4) {} -> {} ({})... ",
                        src_host, dst_host, target_ip
                    );
                    match self.ping(&src_dev.container_name, &target_ip, 3) {
                        Ok(()) => println!("OK"),
                        Err(e) => {
                            println!("FAILED");
                            return Err(e);
                        }
                    }
                }
            }
        }

        // IPv6 loopback pings
        let v6_nodes: Vec<(&String, &DeviceNetworkInfo)> = devices
            .iter()
            .filter(|(_, d)| d.loopback_v6.is_some())
            .collect();

        for (src_host, src_dev) in &v6_nodes {
            for (dst_host, dst_dev) in &v6_nodes {
                if src_host != dst_host {
                    let target_ip = dst_dev.loopback_v6.unwrap().to_string();
                    print!(
                        "    ping (IPv6) {} -> {} ({})... ",
                        src_host, dst_host, target_ip
                    );
                    match self.ping(&src_dev.container_name, &target_ip, 3) {
                        Ok(()) => println!("OK"),
                        Err(e) => {
                            println!("FAILED");
                            return Err(e);
                        }
                    }
                }
            }
        }

        println!("==> All data-plane loopback ping tests PASSED!");
        Ok(())
    }

    /// Evaluate intent assertions from `intent.yml`.
    pub fn verify_intent(
        &self,
        intent: &IntentConfig,
        devices: &BTreeMap<String, DeviceNetworkInfo>,
    ) -> Result<(), VerifyError> {
        let assertions = intent
            .compile_assertions()
            .map_err(|e| VerifyError::IntentFailed(e.to_string()))?;

        if assertions.is_empty() {
            return Ok(());
        }

        println!(
            "==> Validating operational intent ({} assertion(s))...",
            assertions.len()
        );

        for assertion in assertions {
            match assertion {
                Assertion::Ping { from, to } => {
                    let src_dev = devices.get(&from).ok_or_else(|| {
                        VerifyError::IntentFailed(format!(
                            "Source node '{}' not found in lab",
                            from
                        ))
                    })?;

                    let target_ip = match to {
                        PingTarget::Ip(ip) => ip,
                        PingTarget::Loopback(dst_node) => {
                            let dst_dev = devices.get(&dst_node).ok_or_else(|| {
                                VerifyError::IntentFailed(format!(
                                    "Target node '{}' not found in lab",
                                    dst_node
                                ))
                            })?;
                            dst_dev
                                .loopback_v4
                                .map(|ip| ip.to_string())
                                .or_else(|| dst_dev.loopback_v6.map(|ip| ip.to_string()))
                                .ok_or_else(|| {
                                    VerifyError::IntentFailed(format!(
                                        "Target node '{}' has no inferred loopback address",
                                        dst_node
                                    ))
                                })?
                        }
                        PingTarget::Router(dst_node) => {
                            let dst_dev = devices.get(&dst_node).ok_or_else(|| {
                                VerifyError::IntentFailed(format!(
                                    "Target node '{}' not found in lab",
                                    dst_node
                                ))
                            })?;
                            dst_dev
                                .loopback_v4
                                .map(|ip| ip.to_string())
                                .or_else(|| dst_dev.loopback_v6.map(|ip| ip.to_string()))
                                .or_else(|| dst_dev.all_ips_v4.first().map(|ip| ip.to_string()))
                                .or_else(|| dst_dev.all_ips_v6.first().map(|ip| ip.to_string()))
                                .ok_or_else(|| {
                                    VerifyError::IntentFailed(format!(
                                        "Target node '{}' has no IP addresses",
                                        dst_node
                                    ))
                                })?
                        }
                    };

                    print!("    assert: ping {} -> {}... ", from, target_ip);
                    match self.ping(&src_dev.container_name, &target_ip, 3) {
                        Ok(()) => println!("PASSED"),
                        Err(e) => {
                            println!("FAILED");
                            return Err(VerifyError::IntentFailed(format!(
                                "Assertion failed: ping {} -> {}: {}",
                                from, target_ip, e
                            )));
                        }
                    }
                }
                Assertion::RouteExists { node, prefix } => {
                    let dev = devices.get(&node).ok_or_else(|| {
                        VerifyError::IntentFailed(format!("Node '{}' not found in lab", node))
                    })?;

                    let is_v6 = prefix.contains(':');
                    let routes = self.get_routes(&dev.container_name, is_v6)?;
                    print!("    assert: route {} has {}... ", node, prefix);
                    if Self::has_route(&routes, &prefix) {
                        println!("PASSED");
                    } else {
                        println!("FAILED");
                        return Err(VerifyError::IntentFailed(format!(
                            "Route assertion failed: node '{}' missing route to '{}'",
                            node, prefix
                        )));
                    }
                }
            }
        }

        println!("==> Operational intent assertions PASSED!");
        Ok(())
    }

    /// Full end-to-end verification pipeline:
    /// 1. Inspect lab directory
    /// 2. Deploy Containerlab topology
    /// 3. Poll routing convergence
    /// 4. Run loopback mesh ping
    /// 5. Validate intent assertions
    /// 6. Destroy lab
    pub fn run_pipeline(&self) -> Result<(), VerifyError> {
        let topo_path = match &self.options.topology_file {
            Some(p) => p.clone(),
            None => self.options.lab_dir.join("topology.clab.yml"),
        };

        let (_clab, devices) = inspect_lab(&self.options.lab_dir, Some(&topo_path))?;

        // Check if intent file is specified or default intent.yml exists in lab_dir
        let intent_path = match &self.options.intent_file {
            Some(p) => Some(p.clone()),
            None => {
                let candidate = self.options.lab_dir.join("intent.yml");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            }
        };

        let intent_config = if let Some(ref p) = intent_path {
            println!("==> Loading intent file: {}", p.display());
            Some(
                IntentConfig::load_from_file(p)
                    .map_err(|e| VerifyError::IntentFailed(e.to_string()))?,
            )
        } else {
            None
        };

        // Step 1: Deploy
        self.deploy(&topo_path)?;

        // Wrap execution in a closure so cleanup destroy is always called on error
        let execution_result = (|| -> Result<(), VerifyError> {
            // Step 2: Route convergence polling
            self.wait_for_convergence(&devices)?;

            // Step 3: Loopback data-plane ping mesh
            self.verify_loopback_mesh(&devices)?;

            // Step 4: Intent assertions
            if let Some(ref intent) = intent_config {
                self.verify_intent(intent, &devices)?;
            }

            Ok(())
        })();

        // Teardown unless skip_destroy is set
        if !self.options.skip_destroy {
            let destroy_res = self.destroy(&topo_path);
            if let Err(e) = destroy_res {
                eprintln!("Warning: Error during lab teardown: {}", e);
                if execution_result.is_ok() {
                    return Err(e);
                }
            }
        } else {
            println!("==> Skipping lab destroy (--skip-destroy was specified).");
        }

        execution_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Mock command runner recording calls and producing mock responses.
    #[derive(Default)]
    struct MockCommandRunner {
        pub calls: Mutex<Vec<(String, Vec<String>)>>,
        pub poll_counter: AtomicUsize,
        pub simulate_timeout: bool,
        pub simulate_ping_fail: bool,
        pub simulate_route_missing: bool,
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, program: &str, args: &[&str]) -> Result<CommandOutput, VerifyError> {
            let mut recorded = self.calls.lock().unwrap();
            let args_vec: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            recorded.push((program.to_string(), args_vec));

            // Deploy
            if args.contains(&"deploy") {
                return Ok(CommandOutput {
                    status_code: 0,
                    stdout: "deployed".to_string(),
                    stderr: String::new(),
                });
            }

            // Destroy
            if args.contains(&"destroy") {
                return Ok(CommandOutput {
                    status_code: 0,
                    stdout: "destroyed".to_string(),
                    stderr: String::new(),
                });
            }

            // Route polling: vtyctl show ip/ipv6 route
            if args.contains(&"show")
                && (args.contains(&"ip") || args.contains(&"ipv6"))
                && args.contains(&"route")
            {
                let call_idx = self.poll_counter.fetch_add(1, Ordering::SeqCst);
                if self.simulate_timeout {
                    // Return empty route table
                    return Ok(CommandOutput {
                        status_code: 0,
                        stdout: "Codes: K - kernel, C - connected, S - static, O - OSPF\n"
                            .to_string(),
                        stderr: String::new(),
                    });
                }

                // Simulate delayed convergence: first 2 calls missing, then converged
                if call_idx < 2 {
                    return Ok(CommandOutput {
                        status_code: 0,
                        stdout: "Codes: K - kernel, C - connected\nC 10.0.12.0/24 is directly connected\n".to_string(),
                        stderr: String::new(),
                    });
                }

                if self.simulate_route_missing && args.contains(&"10.99.99.0/24") {
                    return Ok(CommandOutput {
                        status_code: 0,
                        stdout: "Codes: O - OSPF\nO 1.1.1.1/32 via 10.0.12.1\nO 2.2.2.2/32 via 10.0.12.2\n".to_string(),
                        stderr: String::new(),
                    });
                }

                return Ok(CommandOutput {
                    status_code: 0,
                    stdout: "Codes: O - OSPF\nO 1.1.1.1/32 via 10.0.12.1\nO 2.2.2.2/32 via 10.0.12.2\nO 10.99.99.0/24\nO 2001:db8:1::1/128\nO 2001:db8:2::2/128\n".to_string(),
                    stderr: String::new(),
                });
            }

            // Ping
            if args.contains(&"ping") {
                if self.simulate_ping_fail {
                    return Ok(CommandOutput {
                        status_code: 1,
                        stdout: String::new(),
                        stderr: "100% packet loss".to_string(),
                    });
                }
                return Ok(CommandOutput {
                    status_code: 0,
                    stdout: "3 packets transmitted, 3 received, 0% packet loss".to_string(),
                    stderr: String::new(),
                });
            }

            Ok(CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn setup_test_lab(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();

        let clab = r#"
name: test-mock-lab
topology:
  nodes:
    r1:
      kind: linux
      image: zebra-rs:latest
    r2:
      kind: linux
      image: zebra-rs:latest
  links:
  - endpoints: ["r1:eth1", "r2:eth1"]
"#;
        std::fs::write(dir.join("topology.clab.yml"), clab).unwrap();

        let r1_zebra = r#"
system:
  hostname: r1
interface:
- name: eth1
  ipv4:
    address:
    - 10.0.12.1/24
- name: Loopback0
  ipv4:
    address:
    - 1.1.1.1/32
  ipv6:
    address:
    - 2001:db8:1::1/128
"#;
        std::fs::write(dir.join("r1.zebra.yaml"), r1_zebra).unwrap();

        let r2_zebra = r#"
system:
  hostname: r2
interface:
- name: eth1
  ipv4:
    address:
    - 10.0.12.2/24
- name: Loopback0
  ipv4:
    address:
    - 2.2.2.2/32
  ipv6:
    address:
    - 2001:db8:2::2/128
"#;
        std::fs::write(dir.join("r2.zebra.yaml"), r2_zebra).unwrap();
    }

    #[test]
    fn test_inspect_lab_extracts_nodes_and_loopbacks() {
        let tmp = std::env::temp_dir().join(format!("inspect_lab_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        setup_test_lab(&tmp);

        let (clab, devices) = inspect_lab(&tmp, None).unwrap();
        assert_eq!(clab.name, "test-mock-lab");
        assert_eq!(devices.len(), 2);

        let r1 = devices.get("r1").unwrap();
        assert_eq!(r1.hostname, "r1");
        assert_eq!(r1.container_name, "clab-test-mock-lab-r1");
        assert_eq!(r1.loopback_v4.unwrap().to_string(), "1.1.1.1");
        assert_eq!(r1.loopback_v6.unwrap().to_string(), "2001:db8:1::1");

        let r2 = devices.get("r2").unwrap();
        assert_eq!(r2.hostname, "r2");
        assert_eq!(r2.container_name, "clab-test-mock-lab-r2");
        assert_eq!(r2.loopback_v4.unwrap().to_string(), "2.2.2.2");
        assert_eq!(r2.loopback_v6.unwrap().to_string(), "2001:db8:2::2");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_pipeline_success_with_mock_runner() {
        let tmp = std::env::temp_dir().join(format!("pipeline_ok_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        setup_test_lab(&tmp);

        let intent_yaml = r#"
assert: ping r1 -> r2 loopback
assertions:
  - ping r2 -> r1
  - route r1 has 10.99.99.0/24
"#;
        std::fs::write(tmp.join("intent.yml"), intent_yaml).unwrap();

        let opts = VerifyOptions {
            lab_dir: tmp.clone(),
            topology_file: None,
            intent_file: None,
            timeout: Duration::from_secs(10),
            interval: Duration::from_millis(50),
            skip_destroy: false,
            sudo: false,
            clab_bin: "containerlab".to_string(),
            docker_bin: "docker".to_string(),
        };

        let mock_runner = MockCommandRunner::default();
        let verifier = LabVerifier::new(&opts, &mock_runner);

        let res = verifier.run_pipeline();
        assert!(res.is_ok(), "Pipeline should succeed: {:?}", res.err());

        // Check calls
        let calls = mock_runner.calls.lock().unwrap();
        let programs: Vec<&str> = calls.iter().map(|(p, _)| p.as_str()).collect();
        assert!(programs.contains(&"containerlab"));
        assert!(programs.contains(&"docker"));

        // Verify deploy was called
        assert!(calls
            .iter()
            .any(|(p, args)| p == "containerlab" && args.contains(&"deploy".to_string())));
        // Verify destroy was called
        assert!(calls
            .iter()
            .any(|(p, args)| p == "containerlab" && args.contains(&"destroy".to_string())));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_pipeline_timeout_handling() {
        let tmp = std::env::temp_dir().join(format!("pipeline_timeout_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        setup_test_lab(&tmp);

        let opts = VerifyOptions {
            lab_dir: tmp.clone(),
            topology_file: None,
            intent_file: None,
            timeout: Duration::from_millis(150),
            interval: Duration::from_millis(50),
            skip_destroy: false,
            sudo: false,
            clab_bin: "containerlab".to_string(),
            docker_bin: "docker".to_string(),
        };

        let mock_runner = MockCommandRunner {
            simulate_timeout: true,
            ..Default::default()
        };

        let verifier = LabVerifier::new(&opts, &mock_runner);
        let res = verifier.run_pipeline();
        assert!(res.is_err());
        match res.unwrap_err() {
            VerifyError::ConvergenceTimeout(dur, msg) => {
                assert_eq!(dur, Duration::from_millis(150));
                assert!(msg.contains("missing IPv4 route"));
            }
            other => panic!("Unexpected error: {:?}", other),
        }

        // Even on timeout, destroy must be executed!
        let calls = mock_runner.calls.lock().unwrap();
        assert!(calls
            .iter()
            .any(|(p, args)| p == "containerlab" && args.contains(&"destroy".to_string())));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_pipeline_ping_failure() {
        let tmp = std::env::temp_dir().join(format!("pipeline_ping_fail_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        setup_test_lab(&tmp);

        let opts = VerifyOptions {
            lab_dir: tmp.clone(),
            topology_file: None,
            intent_file: None,
            timeout: Duration::from_secs(10),
            interval: Duration::from_millis(50),
            skip_destroy: false,
            sudo: false,
            clab_bin: "containerlab".to_string(),
            docker_bin: "docker".to_string(),
        };

        let mock_runner = MockCommandRunner {
            simulate_ping_fail: true,
            ..Default::default()
        };

        let verifier = LabVerifier::new(&opts, &mock_runner);
        let res = verifier.run_pipeline();
        assert!(res.is_err());
        match res.unwrap_err() {
            VerifyError::PingFailed(msg) => {
                assert!(msg.contains("packet loss"));
            }
            other => panic!("Unexpected error: {:?}", other),
        }

        // Lab cleanup destroy still executed on ping failure
        let calls = mock_runner.calls.lock().unwrap();
        assert!(calls
            .iter()
            .any(|(p, args)| p == "containerlab" && args.contains(&"destroy".to_string())));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

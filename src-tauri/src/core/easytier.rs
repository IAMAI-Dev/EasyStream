// EasyStream - GPL-3.0-or-later
// EasyTier core + CLI v2.6.4 orchestration, subnet overlap detection,
// restricted runtime config generation, and 2s JSON peer/route polling.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::core::credentials::{
    delete_secret, read_secret, save_secret, secure_delete_file, write_restricted_runtime_file,
};
use crate::core::process_mgr::ProcessManager;
use crate::state::{
    app_data_dir, AddressMode, CommandError, ErrorCode, HostReadiness, JoinNetworkRequest,
    PeerInfo, RoomSnapshot, RoomStage, RouteDiagnosticEntry, RoutePath, SubnetWarning,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ROOM_SECRET_CRED_KEY: &str = "room_secret";

pub struct EasyTierManager {
    process_mgr: Arc<ProcessManager>,
    snapshot: Arc<RwLock<RoomSnapshot>>,
    peers: Arc<RwLock<Vec<PeerInfo>>>,
    routes: Arc<RwLock<Vec<RouteDiagnosticEntry>>>,
    core_pid: Arc<Mutex<Option<u32>>>,
    runtime_config_path: Arc<Mutex<Option<PathBuf>>>,
    snapshot_seq: Arc<Mutex<u64>>,
    last_virtual_ip: Arc<Mutex<Option<String>>>,
}

impl EasyTierManager {
    pub fn new(process_mgr: Arc<ProcessManager>) -> Self {
        let mut initial = RoomSnapshot::default();
        if let Ok(Some((_, _))) = read_secret(ROOM_SECRET_CRED_KEY) {
            initial.has_saved_secret = true;
        }
        Self {
            process_mgr,
            snapshot: Arc::new(RwLock::new(initial)),
            peers: Arc::new(RwLock::new(Vec::new())),
            routes: Arc::new(RwLock::new(Vec::new())),
            core_pid: Arc::new(Mutex::new(None)),
            runtime_config_path: Arc::new(Mutex::new(None)),
            snapshot_seq: Arc::new(Mutex::new(0)),
            last_virtual_ip: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_room_snapshot(&self) -> RoomSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.clone()
    }

    pub async fn get_routes(&self) -> Vec<RouteDiagnosticEntry> {
        self.routes.read().await.clone()
    }

    pub async fn next_seq(&self) -> u64 {
        let mut seq = self.snapshot_seq.lock().await;
        *seq += 1;
        *seq
    }

    pub fn resolve_binaries(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        let core = self
            .process_mgr
            .resolve_component_binary("easytier", "easytier-core.exe");
        let cli = self
            .process_mgr
            .resolve_component_binary("easytier", "easytier-cli.exe");
        (core, cli)
    }

    /// Join or create an EasyTier room (idempotent if already in the exact same room).
    pub async fn join_network(
        &self,
        req: JoinNetworkRequest,
        rpc_port: u16,
    ) -> Result<RoomSnapshot, CommandError> {
        let room_name = req.room_name.trim().to_string();
        let room_secret = req.room_secret.trim().to_string();
        if room_name.is_empty() || room_secret.len() < 6 {
            return Err(CommandError::new(
                ErrorCode::InvalidArgument,
                "房间名称不能为空，且房间密钥长度至少为 6 位。",
            ));
        }

        // Idempotency check
        {
            let cur = self.snapshot.read().await;
            if cur.stage == RoomStage::InRoom
                && cur.room_name.as_deref() == Some(&room_name)
                && self.core_pid.lock().await.is_some()
            {
                return Ok(cur.clone());
            }
        }

        // Clean up any previous session first
        let _ = self.leave_network().await;

        let (core_path, cli_path) = self.resolve_binaries();
        let core_bin = core_path.ok_or_else(|| {
            CommandError::new(
                ErrorCode::ComponentMissing,
                "未找到 easytier-core.exe，请先运行 scripts/fetch-upstream.ps1 或安装完整包。",
            )
        })?;
        if cli_path.is_none() {
            return Err(CommandError::new(
                ErrorCode::ComponentMissing,
                "未找到 easytier-cli.exe，无法读取组网状态。",
            ));
        }

        let hostname = req
            .device_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "EasyStream-PC".to_string())
            });

        let relay_nodes: Vec<String> = if req.relay_nodes.is_empty() {
            vec!["tcp://public.easytier.cn:11010".to_string()]
        } else {
            req.relay_nodes
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        // Check local LAN/VPN subnet overlap against default 10.144.144.0/24 or configured static CIDR
        let target_cidr = match &req.address_mode {
            AddressMode::Dhcp => "10.144.144.0/24".to_string(),
            AddressMode::Static { ipv4_cidr } => ipv4_cidr.clone(),
        };
        let subnet_warnings = detect_subnet_overlaps(&target_cidr);

        {
            let mut snap = self.snapshot.write().await;
            snap.stage = RoomStage::Joining;
            snap.room_id = Some(uuid::Uuid::new_v4().to_string());
            snap.room_name = Some(room_name.clone());
            snap.local_hostname = hostname.clone();
            snap.rpc_port = rpc_port;
            snap.relay_nodes = relay_nodes.clone();
            snap.address_mode = req.address_mode.clone();
            snap.subnet_warnings = subnet_warnings.clone();
            snap.last_error = None;
        }

        // Persist room secret in Windows Credential Manager if requested
        if req.remember_secret {
            if let Err(e) = save_secret(ROOM_SECRET_CRED_KEY, &room_name, &room_secret) {
                tracing::warn!("Failed to store room secret in Credential Manager: {e}");
            } else {
                self.snapshot.write().await.has_saved_secret = true;
            }
        } else {
            let _ = delete_secret(ROOM_SECRET_CRED_KEY);
            self.snapshot.write().await.has_saved_secret = false;
        }

        // Write restricted TOML config (ACL-protected to current user + SYSTEM)
        let toml_content = build_easytier_toml(
            &room_name,
            &room_secret,
            &hostname,
            &req.address_mode,
            &relay_nodes,
        );
        let runtime_dir = app_data_dir().join("runtime");
        let cfg_path = write_restricted_runtime_file(&runtime_dir, "easytier-room.toml", &toml_content)
            .map_err(|e| CommandError::new(ErrorCode::NetworkJoinFailed, e))?;

        *self.runtime_config_path.lock().await = Some(cfg_path.clone());

        let rpc_addr = format!("127.0.0.1:{rpc_port}");
        let args = vec![
            "-c".to_string(),
            cfg_path.to_string_lossy().to_string(),
            "--rpc-portal".to_string(),
            rpc_addr,
        ];

        let pid = self
            .process_mgr
            .spawn_owned("easytier-core", &core_bin, &args, None, &[], true, true)
            .map_err(|e| {
                CommandError::new(ErrorCode::NetworkJoinFailed, "启动 EasyTier 核心进程失败")
                    .with_detail(e)
            })?;

        *self.core_pid.lock().await = Some(pid);

        // Wait briefly for RPC portal readiness (up to 4s)
        let mut ready = false;
        for _ in 0..8 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Some(exit_code) = self.process_mgr.try_wait_owned(pid) {
                let _ = self.cleanup_runtime_file().await;
                *self.core_pid.lock().await = None;
                let err = CommandError::new(
                    ErrorCode::NetworkJoinFailed,
                    format!("EasyTier 进程启动后立即退出（退出码 {exit_code}），请检查 TUN 驱动权限或房间配置。"),
                );
                let mut snap = self.snapshot.write().await;
                snap.stage = RoomStage::Error;
                snap.last_error = Some(err.clone());
                return Err(err);
            }
            if self.poll_once().await.is_ok() {
                ready = true;
                break;
            }
        }

        let mut snap = self.snapshot.write().await;
        snap.stage = RoomStage::InRoom;
        if !ready {
            tracing::info!("EasyTier core started; waiting for peer/DHCP convergence");
        }
        Ok(snap.clone())
    }

    pub async fn leave_network(&self) -> Result<(), CommandError> {
        if let Some(pid) = self.core_pid.lock().await.take() {
            let _ = self.process_mgr.stop_owned(pid);
        }
        self.cleanup_runtime_file().await;
        *self.last_virtual_ip.lock().await = None;
        self.peers.write().await.clear();
        self.routes.write().await.clear();

        let mut snap = self.snapshot.write().await;
        snap.stage = RoomStage::Disconnected;
        snap.room_id = None;
        snap.local_virtual_ip = None;
        snap.nat_type = "Unknown".to_string();
        snap.last_error = None;
        Ok(())
    }

    async fn cleanup_runtime_file(&self) {
        if let Some(path) = self.runtime_config_path.lock().await.take() {
            secure_delete_file(&path);
        }
    }

    pub async fn load_saved_room_secret(&self) -> Option<(String, String)> {
        read_secret(ROOM_SECRET_CRED_KEY).ok().flatten()
    }

    /// Poll `easytier-cli -p 127.0.0.1:<port> -o json peer` and `route`.
    /// Returns true if the local virtual IP changed since last poll.
    pub async fn poll_once(&self) -> Result<bool, String> {
        let (rpc_port, stage) = {
            let snap = self.snapshot.read().await;
            (snap.rpc_port, snap.stage.clone())
        };
        if matches!(stage, RoomStage::Disconnected) {
            return Ok(false);
        }

        // Verify process is still alive
        if let Some(pid) = *self.core_pid.lock().await {
            if let Some(code) = self.process_mgr.try_wait_owned(pid) {
                *self.core_pid.lock().await = None;
                let mut snap = self.snapshot.write().await;
                snap.stage = RoomStage::Error;
                snap.last_error = Some(CommandError::new(
                    ErrorCode::NetworkJoinFailed,
                    format!("EasyTier 核心进程意外退出 (code={code})"),
                ));
                return Err(format!("easytier-core exited with code {code}"));
            }
        }

        let (_, cli_opt) = self.resolve_binaries();
        let cli_bin = cli_opt.ok_or_else(|| "easytier-cli.exe not found".to_string())?;
        let rpc_addr = format!("127.0.0.1:{rpc_port}");

        let peer_json = run_cli_json(&cli_bin, &rpc_addr, "peer").await?;
        let route_json = run_cli_json(&cli_bin, &rpc_addr, "route").await.ok();

        let parsed_peers = parse_peer_items(&peer_json);
        let parsed_routes = route_json
            .as_ref()
            .map(parse_route_items)
            .unwrap_or_default();

        let route_map: HashMap<String, &ParsedRouteRow> = parsed_routes
            .iter()
            .filter(|r| !r.ipv4.is_empty())
            .map(|r| (strip_cidr(&r.ipv4), r))
            .collect();

        let mut local_ip: Option<String> = None;
        let mut local_nat = "Unknown".to_string();
        let mut remote_peers: Vec<PeerInfo> = Vec::new();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let existing_peers: HashMap<String, PeerInfo> = self
            .peers
            .read()
            .await
            .iter()
            .map(|p| (p.peer_id.clone(), p.clone()))
            .collect();

        for row in parsed_peers {
            if row.cost.eq_ignore_ascii_case("Local") {
                if !row.ipv4.is_empty() {
                    local_ip = Some(row.ipv4.clone());
                }
                if !row.nat_type.is_empty() {
                    local_nat = row.nat_type.clone();
                }
                continue;
            }

            // Section 3.3: filter infrastructure/public server nodes & nodes without virtual IPv4
            if row.hostname.starts_with("PUBLIC_SERVER")
                || row.hostname.starts_with("public_server")
                || row.ipv4.trim().is_empty()
                || row.ipv4 == "-"
            {
                continue;
            }

            let clean_ip = strip_cidr(&row.ipv4);
            let route_entry = route_map.get(&clean_ip).copied();
            let path = determine_route_path(&row.cost, route_entry);
            let rtt_ms = parse_rtt_ms(&row.lat_ms);
            let loss_rate = parse_loss_rate(&row.loss_rate);

            let peer_key = if !row.id.is_empty() {
                row.id.clone()
            } else {
                clean_ip.clone()
            };

            let prev = existing_peers.get(&peer_key);
            let ip_changed = prev.map(|p| p.virtual_ip != clean_ip).unwrap_or(false);

            let (readiness, host_reachable, stream_available, paired, host_unique_id, available_apps, selected_app) =
                if let Some(p) = prev.filter(|_| !ip_changed) {
                    (
                        p.readiness.clone(),
                        p.host_reachable,
                        p.stream_available,
                        p.paired,
                        p.host_unique_id.clone(),
                        p.available_apps.clone(),
                        p.selected_app.clone(),
                    )
                } else {
                    (
                        HostReadiness::InRoom,
                        false,
                        false,
                        false,
                        None,
                        Vec::new(),
                        None,
                    )
                };

            remote_peers.push(PeerInfo {
                peer_id: peer_key,
                hostname: row.hostname,
                virtual_ip: clean_ip,
                rtt_ms,
                loss_rate,
                path,
                tunnel_proto: row.tunnel_proto,
                nat_type: row.nat_type,
                readiness,
                host_reachable,
                stream_available,
                paired,
                host_unique_id,
                available_apps,
                selected_app,
                last_seen_ms: now_ms,
            });
        }

        let mut virtual_ip_changed = false;
        {
            let mut last_ip = self.last_virtual_ip.lock().await;
            if *last_ip != local_ip {
                *last_ip = local_ip.clone();
                virtual_ip_changed = true;
            }
        }

        {
            let mut snap = self.snapshot.write().await;
            if let Some(ip) = local_ip {
                snap.local_virtual_ip = Some(ip);
            }
            snap.nat_type = local_nat;
            if snap.stage == RoomStage::Joining {
                snap.stage = RoomStage::InRoom;
            }
        }

        *self.peers.write().await = remote_peers;
        *self.routes.write().await = parsed_routes
            .into_iter()
            .map(|r| RouteDiagnosticEntry {
                ipv4: r.ipv4,
                hostname: r.hostname,
                next_hop_ipv4: r.next_hop_ipv4,
                next_hop_hostname: r.next_hop_hostname,
                path_len: r.path_len,
                path_latency_ms: r.path_latency,
                version: r.version,
            })
            .collect();

        Ok(virtual_ip_changed)
    }

    pub async fn update_peer_probe(
        &self,
        peer_id: &str,
        host_reachable: bool,
        stream_available: bool,
        paired: bool,
        busy: bool,
        host_unique_id: Option<String>,
        identity_changed: bool,
        available_apps: Vec<String>,
    ) {
        let mut peers = self.peers.write().await;
        if let Some(p) = peers.iter_mut().find(|p| p.peer_id == peer_id) {
            p.host_reachable = host_reachable;
            p.stream_available = stream_available;
            p.paired = paired;
            p.host_unique_id = host_unique_id;
            if !available_apps.is_empty() {
                p.available_apps = available_apps;
                if p.selected_app.is_none() {
                    p.selected_app = p
                        .available_apps
                        .iter()
                        .find(|a| a.eq_ignore_ascii_case("Desktop"))
                        .cloned()
                        .or_else(|| p.available_apps.first().cloned());
                }
            }
            p.readiness = if identity_changed {
                HostReadiness::IdentityChanged
            } else if busy {
                HostReadiness::HostBusy
            } else if paired && stream_available {
                HostReadiness::StreamReady
            } else if host_reachable {
                HostReadiness::NeedsPairing
            } else {
                HostReadiness::HostUnreachable
            };
        }
    }
}

async fn run_cli_json(
    cli_bin: &PathBuf,
    rpc_addr: &str,
    subcommand: &str,
) -> Result<serde_json::Value, String> {
    let cli = cli_bin.clone();
    let rpc = rpc_addr.to_string();
    let sub = subcommand.to_string();

    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(&cli);
        cmd.args(["-p", &rpc, "-o", "json", &sub]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run easytier-cli {sub}: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("easytier-cli {sub} failed: {stderr}"));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str::<serde_json::Value>(&stdout)
            .map_err(|e| format!("Invalid JSON from easytier-cli {sub}: {e}"))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[derive(Debug, Default)]
struct ParsedPeerRow {
    ipv4: String,
    hostname: String,
    cost: String,
    lat_ms: String,
    loss_rate: String,
    tunnel_proto: String,
    nat_type: String,
    id: String,
}

#[derive(Debug, Default)]
struct ParsedRouteRow {
    ipv4: String,
    hostname: String,
    next_hop_ipv4: String,
    next_hop_hostname: String,
    path_len: i32,
    path_latency: i32,
    version: String,
}

fn parse_peer_items(val: &serde_json::Value) -> Vec<ParsedPeerRow> {
    let arr = if let Some(a) = val.as_array() {
        a.clone()
    } else if let Some(obj) = val.as_object() {
        // Multi-instance wrapper or single object
        if let Some(res) = obj.get("result").and_then(|v| v.as_array()) {
            res.clone()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    arr.iter()
        .map(|item| ParsedPeerRow {
            ipv4: json_str(item, &["ipv4", "cidr"]),
            hostname: json_str(item, &["hostname"]),
            cost: json_str(item, &["cost"]),
            lat_ms: json_str(item, &["lat_ms", "lat(ms)"]),
            loss_rate: json_str(item, &["loss_rate", "loss"]),
            tunnel_proto: json_str(item, &["tunnel_proto", "tunnel"]),
            nat_type: json_str(item, &["nat_type", "NAT"]),
            id: json_str(item, &["id", "peer_id"]),
        })
        .collect()
}

fn parse_route_items(val: &serde_json::Value) -> Vec<ParsedRouteRow> {
    let arr = if let Some(a) = val.as_array() {
        a.clone()
    } else if let Some(res) = val.get("result").and_then(|v| v.as_array()) {
        res.clone()
    } else {
        Vec::new()
    };

    arr.iter()
        .map(|item| ParsedRouteRow {
            ipv4: json_str(item, &["ipv4"]),
            hostname: json_str(item, &["hostname"]),
            next_hop_ipv4: json_str(item, &["next_hop_ipv4"]),
            next_hop_hostname: json_str(item, &["next_hop_hostname"]),
            path_len: item.get("path_len").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            path_latency: item
                .get("path_latency")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32,
            version: json_str(item, &["version"]),
        })
        .collect()
}

fn json_str(val: &serde_json::Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(v) = val.get(*k) {
            if let Some(s) = v.as_str() {
                return s.to_string();
            }
            if !v.is_null() {
                return v.to_string();
            }
        }
    }
    String::new()
}

fn strip_cidr(ip_or_cidr: &str) -> String {
    ip_or_cidr
        .split('/')
        .next()
        .unwrap_or(ip_or_cidr)
        .trim()
        .to_string()
}

fn determine_route_path(cost: &str, route: Option<&ParsedRouteRow>) -> RoutePath {
    if let Some(r) = route {
        if r.next_hop_ipv4.eq_ignore_ascii_case("DIRECT") || r.path_len == 1 {
            return RoutePath::Direct;
        }
        if r.path_len > 1 || (!r.next_hop_ipv4.is_empty() && r.next_hop_ipv4 != "-") {
            return RoutePath::Relay;
        }
    }
    let c = cost.trim().to_ascii_lowercase();
    if c == "p2p" || c == "direct" || c == "1" {
        RoutePath::Direct
    } else if c.starts_with("relay") || c == "2" || c == "3" {
        RoutePath::Relay
    } else {
        RoutePath::Unknown
    }
}

fn parse_rtt_ms(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed == "—" {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0)
}

fn parse_loss_rate(raw: &str) -> Option<f64> {
    let trimmed = raw.trim().trim_end_matches('%');
    if trimmed.is_empty() || trimmed == "-" {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Build an EasyTier v2.6.4 TOML config with minimal required flags and no exit-node or subnet proxy exposure.
pub fn build_easytier_toml(
    room_name: &str,
    room_secret: &str,
    hostname: &str,
    address_mode: &AddressMode,
    relay_nodes: &[String],
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("instance_name = \"easystream\""));
    lines.push(format!("hostname = \"{}\"", escape_toml(hostname)));

    match address_mode {
        AddressMode::Dhcp => {
            lines.push("dhcp = true".to_string());
        }
        AddressMode::Static { ipv4_cidr } => {
            lines.push("dhcp = false".to_string());
            lines.push(format!("ipv4 = \"{}\"", escape_toml(ipv4_cidr)));
        }
    }

    lines.push("listeners = [\"tcp://0.0.0.0:11010\", \"udp://0.0.0.0:11010\"]".to_string());
    lines.push(String::new());
    lines.push("[network_identity]".to_string());
    lines.push(format!("network_name = \"{}\"", escape_toml(room_name)));
    lines.push(format!("network_secret = \"{}\"", escape_toml(room_secret)));

    for node in relay_nodes {
        lines.push(String::new());
        lines.push("[[peer]]".to_string());
        lines.push(format!("uri = \"{}\"", escape_toml(node)));
    }

    lines.push(String::new());
    lines.push("[flags]".to_string());
    lines.push("default_protocol = \"udp\"".to_string());
    lines.push("enable_encryption = true".to_string());
    lines.push("enable_exit_node = false".to_string());
    lines.push("no_tun = false".to_string());
    lines.push("latency_first = true".to_string());
    lines.push("mtu = 1380".to_string());

    lines.join("\n")
}

fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Inspect local Windows IPv4 interfaces and warn if the virtual subnet overlaps with a local LAN/VPN route.
pub fn detect_subnet_overlaps(virtual_cidr: &str) -> Vec<SubnetWarning> {
    let mut warnings = Vec::new();
    let Some((virt_net, virt_mask)) = parse_cidr(virtual_cidr) else {
        return warnings;
    };

    #[cfg(windows)]
    {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } | ForEach-Object { \"$($_.InterfaceAlias)|$($_.IPAddress)/$($_.PrefixLength)\" }",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.trim().split('|').collect();
                if parts.len() != 2 {
                    continue;
                }
                let alias = parts[0];
                if alias.to_ascii_lowercase().contains("easytier") {
                    continue;
                }
                if let Some((local_net, local_mask)) = parse_cidr(parts[1]) {
                    if cidrs_overlap(virt_net, virt_mask, local_net, local_mask) {
                        warnings.push(SubnetWarning {
                            local_interface: alias.to_string(),
                            local_cidr: parts[1].to_string(),
                            virtual_cidr: virtual_cidr.to_string(),
                            suggestion: "检测到虚拟网段与本机物理网卡或已有 VPN 网段重叠，建议在高级设置中改用静态网段（如 10.199.199.x/24）。".to_string(),
                        });
                    }
                }
            }
        }
    }

    warnings
}

fn parse_cidr(cidr: &str) -> Option<(u32, u32)> {
    let mut parts = cidr.trim().split('/');
    let ip: Ipv4Addr = parts.next()?.parse().ok()?;
    let prefix: u32 = parts.next().unwrap_or("24").parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let mask = if prefix == 0 {
        0
    } else {
        !0u32 << (32 - prefix)
    };
    Some((u32::from(ip) & mask, mask))
}

fn cidrs_overlap(net_a: u32, mask_a: u32, net_b: u32, mask_b: u32) -> bool {
    let common_mask = mask_a & mask_b;
    (net_a & common_mask) == (net_b & common_mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cidr_overlap() {
        let (a, ma) = parse_cidr("10.144.144.0/24").unwrap();
        let (b, mb) = parse_cidr("10.144.144.12/24").unwrap();
        assert!(cidrs_overlap(a, ma, b, mb));

        let (c, mc) = parse_cidr("192.168.1.10/24").unwrap();
        assert!(!cidrs_overlap(a, ma, c, mc));
    }

    #[test]
    fn builds_valid_easytier_toml() {
        let toml = build_easytier_toml(
            "room-123",
            "secret-xyz",
            "MyPC",
            &AddressMode::Dhcp,
            &["tcp://public.easytier.cn:11010".to_string()],
        );
        assert!(toml.contains("dhcp = true"));
        assert!(toml.contains("enable_exit_node = false"));
        assert!(toml.contains("network_name = \"room-123\""));
    }
}

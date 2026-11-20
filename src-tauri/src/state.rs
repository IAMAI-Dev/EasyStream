// EasyStream - GPL-3.0-or-later
// Shared domain models, snapshots, events, error codes, and application state.

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::core::easytier::EasyTierManager;
use crate::core::moonlight::MoonlightManager;
use crate::core::process_mgr::ProcessManager;
use crate::core::session::SessionManager;
use crate::core::sunshine::SunshineManager;

/// Structured, recoverable error codes matching Section 6 of the specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    NetworkJoinFailed,
    NoReachablePath,
    Unauthorized,
    HostBusy,
    AppMissing,
    EncodeOrCaptureFailed,
    PairingTimeout,
    PairingRejected,
    PairingInvalidPin,
    HostIdentityMismatch,
    ComponentMissing,
    SunshineConflict,
    PermissionRequired,
    SubnetOverlap,
    OperationCancelled,
    InvalidArgument,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
    pub recoverable: bool,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let recoverable = !matches!(code, ErrorCode::InternalError);
        Self {
            code,
            message: message.into(),
            detail: None,
            recoverable,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AddressMode {
    Dhcp,
    Static {
        #[serde(rename = "ipv4Cidr", alias = "ipv4_cidr")]
        ipv4_cidr: String,
    },
}

impl Default for AddressMode {
    fn default() -> Self {
        AddressMode::Dhcp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinNetworkRequest {
    pub room_name: String,
    pub room_secret: String,
    #[serde(default)]
    pub address_mode: AddressMode,
    #[serde(default)]
    pub relay_nodes: Vec<String>,
    #[serde(default)]
    pub remember_secret: bool,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoomStage {
    Disconnected,
    Joining,
    InRoom,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubnetWarning {
    pub local_interface: String,
    pub local_cidr: String,
    pub virtual_cidr: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSnapshot {
    pub room_id: Option<String>,
    pub room_name: Option<String>,
    pub stage: RoomStage,
    pub local_virtual_ip: Option<String>,
    pub local_hostname: String,
    pub nat_type: String,
    pub rpc_port: u16,
    pub relay_nodes: Vec<String>,
    pub address_mode: AddressMode,
    pub subnet_warnings: Vec<SubnetWarning>,
    pub last_error: Option<CommandError>,
    pub has_saved_secret: bool,
}

impl Default for RoomSnapshot {
    fn default() -> Self {
        let local_hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "EasyStream-PC".to_string());
        Self {
            room_id: None,
            room_name: None,
            stage: RoomStage::Disconnected,
            local_virtual_ip: None,
            local_hostname,
            nat_type: "Unknown".to_string(),
            rpc_port: 15888,
            relay_nodes: vec!["tcp://public.easytier.cn:11010".to_string()],
            address_mode: AddressMode::Dhcp,
            subnet_warnings: Vec::new(),
            last_error: None,
            has_saved_secret: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoutePath {
    Direct,
    Relay,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HostReadiness {
    /// Device is visible in the EasyTier virtual room, not yet confirmed as Sunshine host.
    InRoom,
    /// Host GameStream port responded; requires pairing authorization.
    NeedsPairing,
    /// Host is paired and ready to stream.
    StreamReady,
    /// Host is currently serving another active stream.
    HostBusy,
    /// Previously trusted host's unique identity changed; requires re-confirmation.
    IdentityChanged,
    /// Peer is visible in room but Sunshine port is unreachable or disabled.
    HostUnreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub peer_id: String,
    pub hostname: String,
    pub virtual_ip: String,
    pub rtt_ms: Option<f64>,
    pub loss_rate: Option<f64>,
    pub path: RoutePath,
    pub tunnel_proto: String,
    pub nat_type: String,
    pub readiness: HostReadiness,
    pub host_reachable: bool,
    pub stream_available: bool,
    pub paired: bool,
    pub host_unique_id: Option<String>,
    pub available_apps: Vec<String>,
    pub selected_app: Option<String>,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeersUpdatedEvent {
    pub room_id: String,
    pub snapshot_seq: u64,
    pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDiagnosticEntry {
    pub ipv4: String,
    pub hostname: String,
    pub next_hop_ipv4: String,
    pub next_hop_hostname: String,
    pub path_len: i32,
    pub path_latency_ms: i32,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPairingRequest {
    pub request_id: String,
    pub pairing_id: String,
    pub client_name: String,
    pub source_address: String,
    pub valid: bool,
    pub received_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedClient {
    pub client_id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayOutputInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub is_primary: bool,
    pub is_virtual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDisplayState {
    pub supported_driver_detected: bool,
    pub driver_name: Option<String>,
    pub virtual_output_active: bool,
    pub managed_by_easystream: bool,
    pub has_physical_display: bool,
    pub guidance: String,
}

impl Default for VirtualDisplayState {
    fn default() -> Self {
        Self {
            supported_driver_detected: false,
            driver_name: None,
            virtual_output_active: false,
            managed_by_easystream: false,
            has_physical_display: true,
            guidance: "当前存在物理显示器，可直接进行有屏串流。".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub enabled: bool,
    pub sunshine_installed: bool,
    pub sunshine_version: Option<String>,
    pub sunshine_owned_by_easystream: bool,
    pub sunshine_conflict_detected: bool,
    pub sunshine_conflict_message: Option<String>,
    pub service_running: bool,
    pub management_port: u16,
    pub cert_fingerprint: Option<String>,
    pub active_inbound_sessions: u32,
    pub pending_pairings: Vec<PendingPairingRequest>,
    pub trusted_clients: Vec<TrustedClient>,
    pub capture_displays: Vec<DisplayOutputInfo>,
    pub virtual_display: VirtualDisplayState,
    pub firewall_scoped: bool,
    pub last_error: Option<CommandError>,
}

impl Default for HostStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            sunshine_installed: false,
            sunshine_version: None,
            sunshine_owned_by_easystream: false,
            sunshine_conflict_detected: false,
            sunshine_conflict_message: None,
            service_running: false,
            management_port: 47990,
            cert_fingerprint: None,
            active_inbound_sessions: 0,
            pending_pairings: Vec::new(),
            trusted_clients: Vec::new(),
            capture_displays: Vec::new(),
            virtual_display: VirtualDisplayState::default(),
            firewall_scoped: false,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StreamPresetId {
    Saver720p30,
    Balanced1080p60,
    HighRefresh1440p120,
}

impl Default for StreamPresetId {
    fn default() -> Self {
        StreamPresetId::Balanced1080p60
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamPreset {
    pub id: StreamPresetId,
    pub label: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: String,
    pub display_mode: String,
}

impl StreamPreset {
    pub fn presets() -> Vec<Self> {
        vec![
            StreamPreset {
                id: StreamPresetId::Saver720p30,
                label: "省流 720p 30FPS".to_string(),
                description: "5 Mbps · 适合弱网或中继链路".to_string(),
                width: 1280,
                height: 720,
                fps: 30,
                bitrate_kbps: 5000,
                codec: "H.264".to_string(),
                display_mode: "fullscreen".to_string(),
            },
            StreamPreset {
                id: StreamPresetId::Balanced1080p60,
                label: "标准 1080p 60FPS".to_string(),
                description: "15 Mbps · 桌面办公与常规游戏推荐".to_string(),
                width: 1920,
                height: 1080,
                fps: 60,
                bitrate_kbps: 15000,
                codec: "H.264".to_string(),
                display_mode: "fullscreen".to_string(),
            },
            StreamPreset {
                id: StreamPresetId::HighRefresh1440p120,
                label: "高刷候选 1440p 120FPS".to_string(),
                description: "40 Mbps · 需直连高带宽与硬编解码支持".to_string(),
                width: 2560,
                height: 1440,
                fps: 120,
                bitrate_kbps: 40000,
                codec: "H.264".to_string(),
                display_mode: "fullscreen".to_string(),
            },
        ]
    }

    pub fn by_id(id: &StreamPresetId, display_mode: &str, codec: &str) -> Self {
        let mut preset = Self::presets()
            .into_iter()
            .find(|p| p.id == *id)
            .unwrap_or_else(|| Self::presets()[1].clone());
        if !display_mode.is_empty() {
            preset.display_mode = display_mode.to_string();
        }
        if !codec.is_empty() {
            preset.codec = codec.to_string();
        }
        preset
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionStage {
    Idle,
    ProbingHost,
    PairingWaitingUser,
    ListingApps,
    LaunchingPlayer,
    PlayerOpened,
    Reconnecting,
    Disconnecting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub operation_id: Option<String>,
    pub room_id: Option<String>,
    pub peer_id: Option<String>,
    pub peer_name: Option<String>,
    pub peer_ip: Option<String>,
    pub stage: SessionStage,
    pub pin_display: Option<String>,
    pub preset: StreamPreset,
    pub selected_app: Option<String>,
    pub route_path: RoutePath,
    pub downgrade_hint: Option<String>,
    pub error: Option<CommandError>,
    pub started_at_ms: Option<i64>,
    pub pairing_wait_ms: Option<u64>,
    pub player_pid: Option<u32>,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            operation_id: None,
            room_id: None,
            peer_id: None,
            peer_name: None,
            peer_ip: None,
            stage: SessionStage::Idle,
            pin_display: None,
            preset: StreamPreset::by_id(&StreamPresetId::Balanced1080p60, "fullscreen", "H.264"),
            selected_app: None,
            route_path: RoutePath::Unknown,
            downgrade_hint: None,
            error: None,
            started_at_ms: None,
            pairing_wait_ms: None,
            player_pid: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_preset: StreamPresetId,
    pub display_mode: String,
    pub video_codec: String,
    pub relay_nodes: Vec<String>,
    pub remember_room_secret: bool,
    pub device_name: String,
    pub sunshine_base_port: u16,
    pub easytier_rpc_port: u16,
    pub last_room_name: Option<String>,
    pub selected_apps_by_host: HashMap<String, String>,
    pub trusted_host_identities: HashMap<String, String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let device_name = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "EasyStream-PC".to_string());
        Self {
            default_preset: StreamPresetId::Balanced1080p60,
            display_mode: "fullscreen".to_string(),
            video_codec: "H.264".to_string(),
            relay_nodes: vec!["tcp://public.easytier.cn:11010".to_string()],
            remember_room_secret: false,
            device_name,
            sunshine_base_port: 47989,
            easytier_rpc_port: 15888,
            last_room_name: None,
            selected_apps_by_host: HashMap::new(),
            trusted_host_identities: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatusItem {
    pub id: String,
    pub name: String,
    pub pinned_version: String,
    pub license: String,
    pub found: bool,
    pub path: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullSnapshot {
    pub app_version: String,
    pub room: RoomSnapshot,
    pub peers: Vec<PeerInfo>,
    pub routes: Vec<RouteDiagnosticEntry>,
    pub snapshot_seq: u64,
    pub host: HostStatus,
    pub session: SessionSnapshot,
    pub settings: AppSettings,
    pub presets: Vec<StreamPreset>,
    pub components: Vec<ComponentStatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecidePairingRequest {
    pub request_id: String,
    pub approve: bool,
    pub pin: Option<String>,
    pub client_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecidePairingResult {
    pub request_id: String,
    pub approved: bool,
    pub status: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPeerRequest {
    pub peer_id: String,
    pub preset_id: StreamPresetId,
    pub display_mode: Option<String>,
    pub app_name: Option<String>,
    #[serde(default)]
    pub confirm_identity_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPeerResponse {
    pub operation_id: String,
    pub reused_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedRoomCredentials {
    pub room_name: String,
    pub room_secret: String,
}

pub struct AppState {
    pub process_mgr: Arc<ProcessManager>,
    pub easytier: Arc<EasyTierManager>,
    pub sunshine: Arc<SunshineManager>,
    pub moonlight: Arc<MoonlightManager>,
    pub session: Arc<SessionManager>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub recent_logs: Arc<Mutex<Vec<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        let settings = load_settings_from_disk().unwrap_or_default();
        let process_mgr = Arc::new(ProcessManager::new());
        let easytier = Arc::new(EasyTierManager::new(Arc::clone(&process_mgr)));
        let sunshine = Arc::new(SunshineManager::new(
            Arc::clone(&process_mgr),
            settings.sunshine_base_port,
        ));
        let moonlight = Arc::new(MoonlightManager::new(Arc::clone(&process_mgr)));
        let session = Arc::new(SessionManager::new());

        Self {
            process_mgr,
            easytier,
            sunshine,
            moonlight,
            session,
            settings: Arc::new(RwLock::new(settings)),
            recent_logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn append_log(&self, line: String) {
        let sanitized = crate::core::credentials::redact_secrets(&line);
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let entry = format!("[{ts}] {sanitized}");
        let mut logs = self.recent_logs.lock().await;
        logs.push(entry);
        if logs.len() > 500 {
            let drain = logs.len() - 500;
            logs.drain(0..drain);
        }
    }
}

pub fn app_data_dir() -> std::path::PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = base.join("EasyStream");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn settings_file_path() -> std::path::PathBuf {
    app_data_dir().join("settings.json")
}

pub fn load_settings_from_disk() -> Option<AppSettings> {
    let path = settings_file_path();
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_settings_to_disk(settings: &AppSettings) -> Result<(), String> {
    let path = settings_file_path();
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod address_mode_tests {
    use super::AddressMode;
    use serde_json::{from_value, json, to_value};

    #[test]
    fn static_address_uses_frontend_field_and_reads_previous_spelling() {
        let canonical = json!({"static": {"ipv4Cidr": "10.77.1.2/24"}});
        for input in [canonical.clone(), json!({"static": {"ipv4_cidr": "10.77.1.2/24"}})] {
            let mode: AddressMode = from_value(input).unwrap();
            assert_eq!(to_value(mode).unwrap(), canonical);
        }
        assert_eq!(to_value(AddressMode::Dhcp).unwrap(), json!("dhcp"));
    }
}

// EasyStream - GPL-3.0-or-later
// Outbound connection session coordinator: operationId lifecycle, concurrency control,
// stale-callback rejection, pairing wait timing, and downgrade hints.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::state::{
    CommandError, ErrorCode, RoutePath, SessionSnapshot, SessionStage, StreamPreset,
};

pub struct SessionManager {
    snapshot: Arc<RwLock<SessionSnapshot>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(SessionSnapshot::default())),
        }
    }

    pub async fn get_snapshot(&self) -> SessionSnapshot {
        self.snapshot.read().await.clone()
    }

    /// Begin a new outbound operation, or reuse an active one targeting the same peer.
    /// Returns `(operation_id, reused_existing)`.
    pub async fn begin_operation(
        &self,
        room_id: Option<String>,
        peer_id: &str,
        peer_name: &str,
        peer_ip: &str,
        preset: StreamPreset,
        route_path: RoutePath,
    ) -> Result<(String, bool), CommandError> {
        let mut snap = self.snapshot.write().await;

        let active = !matches!(
            snap.stage,
            SessionStage::Idle | SessionStage::Failed | SessionStage::Disconnecting
        );

        if active {
            if snap.peer_id.as_deref() == Some(peer_id) {
                if let Some(op_id) = snap.operation_id.clone() {
                    return Ok((op_id, true));
                }
            }
            return Err(CommandError::new(
                ErrorCode::HostBusy,
                "当前已有正在进行中的连接或串流会话，请先断开后再切换设备。",
            ));
        }

        let op_id = uuid::Uuid::new_v4().to_string();
        let downgrade_hint = if route_path == RoutePath::Relay && preset.bitrate_kbps > 5000 {
            Some("当前通过中继节点转发，如出现卡顿或丢帧，建议切换为“省流 720p 30FPS (5 Mbps)”预设。".to_string())
        } else {
            None
        };

        *snap = SessionSnapshot {
            operation_id: Some(op_id.clone()),
            room_id,
            peer_id: Some(peer_id.to_string()),
            peer_name: Some(peer_name.to_string()),
            peer_ip: Some(peer_ip.to_string()),
            stage: SessionStage::ProbingHost,
            pin_display: None,
            preset,
            selected_app: None,
            route_path,
            downgrade_hint,
            error: None,
            started_at_ms: Some(chrono::Utc::now().timestamp_millis()),
            pairing_wait_ms: None,
            player_pid: None,
        };

        Ok((op_id, false))
    }

    /// Check if `operation_id` is still the current active operation (discards stale callbacks).
    pub async fn is_current(&self, operation_id: &str) -> bool {
        let snap = self.snapshot.read().await;
        snap.operation_id.as_deref() == Some(operation_id)
            && !matches!(snap.stage, SessionStage::Disconnecting | SessionStage::Idle)
    }

    pub async fn set_pairing_waiting(&self, operation_id: &str, pin: &str) -> bool {
        let mut snap = self.snapshot.write().await;
        if snap.operation_id.as_deref() != Some(operation_id) {
            return false;
        }
        snap.stage = SessionStage::PairingWaitingUser;
        snap.pin_display = Some(pin.to_string());
        true
    }

    pub async fn set_listing_apps(
        &self,
        operation_id: &str,
        pairing_wait_ms: Option<u64>,
    ) -> bool {
        let mut snap = self.snapshot.write().await;
        if snap.operation_id.as_deref() != Some(operation_id) {
            return false;
        }
        snap.stage = SessionStage::ListingApps;
        snap.pin_display = None;
        if pairing_wait_ms.is_some() {
            snap.pairing_wait_ms = pairing_wait_ms;
        }
        true
    }

    pub async fn set_launching_player(&self, operation_id: &str, app_name: &str) -> bool {
        let mut snap = self.snapshot.write().await;
        if snap.operation_id.as_deref() != Some(operation_id) {
            return false;
        }
        snap.stage = SessionStage::LaunchingPlayer;
        snap.selected_app = Some(app_name.to_string());
        true
    }

    pub async fn set_player_opened(&self, operation_id: &str, pid: u32) -> bool {
        let mut snap = self.snapshot.write().await;
        if snap.operation_id.as_deref() != Some(operation_id) {
            return false;
        }
        snap.stage = SessionStage::PlayerOpened;
        snap.player_pid = Some(pid);
        true
    }

    pub async fn set_failed(&self, operation_id: &str, err: CommandError) -> bool {
        let mut snap = self.snapshot.write().await;
        if snap.operation_id.as_deref() != Some(operation_id) {
            return false;
        }
        snap.stage = SessionStage::Failed;
        snap.pin_display = None;
        snap.player_pid = None;
        snap.error = Some(err);
        true
    }

    pub async fn update_route_hint(&self, path: RoutePath, loss_rate: Option<f64>) {
        let mut snap = self.snapshot.write().await;
        if !matches!(
            snap.stage,
            SessionStage::PlayerOpened | SessionStage::LaunchingPlayer
        ) {
            return;
        }
        snap.route_path = path.clone();
        let high_loss = loss_rate.map(|l| l >= 2.0).unwrap_or(false);
        if (path == RoutePath::Relay || high_loss) && snap.preset.bitrate_kbps > 5000 {
            snap.downgrade_hint = Some(
                "检测到当前为中继路由或链路存在明显丢包，建议断开后切换至“省流 720p 30FPS (5 Mbps)”以提升流畅度。"
                    .to_string(),
            );
        } else if path == RoutePath::Direct && !high_loss {
            snap.downgrade_hint = None;
        }
    }

    /// Mark session disconnecting and return `(player_pid, peer_ip)` to clean up.
    pub async fn begin_disconnect(&self, operation_id: Option<&str>) -> (Option<u32>, Option<String>) {
        let mut snap = self.snapshot.write().await;
        if let Some(op) = operation_id {
            if snap.operation_id.as_deref() != Some(op) {
                return (None, None);
            }
        }
        let pid = snap.player_pid.take();
        let ip = snap.peer_ip.clone();
        snap.stage = SessionStage::Disconnecting;
        snap.pin_display = None;
        (pid, ip)
    }

    pub async fn finish_disconnect(&self) {
        let mut snap = self.snapshot.write().await;
        *snap = SessionSnapshot::default();
    }
}

// EasyStream - GPL-3.0-or-later
// Sanitized diagnostic report export (Section 6 & 7).
// Strictly excludes room secrets, admin passwords, PINs, and private keys.

use std::path::PathBuf;
use serde::Serialize;

use crate::core::credentials::redact_secrets;
use crate::state::{
    app_data_dir, AppState, ComponentStatusItem, HostStatus, PeerInfo, RoomSnapshot,
    RouteDiagnosticEntry, SessionSnapshot,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SanitizedDiagnosticBundle {
    generated_at: String,
    app_version: String,
    os: String,
    arch: String,
    room: RoomSnapshot,
    peers: Vec<PeerInfo>,
    routes: Vec<RouteDiagnosticEntry>,
    host: HostStatus,
    session: SessionSnapshot,
    components: Vec<ComponentStatusItem>,
    logs: Vec<String>,
}

pub fn collect_component_status(state: &AppState) -> Vec<ComponentStatusItem> {
    let (et_core, et_cli) = state.easytier.resolve_binaries();
    let ml = state.moonlight.resolve_binary();
    let ss_dir = state.sunshine.find_sunshine_install_dir();
    let ss_msi = state.sunshine.find_staged_msi();

    vec![
        ComponentStatusItem {
            id: "easytier-core".to_string(),
            name: "EasyTier Core".to_string(),
            pinned_version: "v2.6.4".to_string(),
            license: "LGPL-3.0".to_string(),
            found: et_core.is_some(),
            path: et_core.map(|p| p.to_string_lossy().to_string()),
            note: "虚拟组网核心与 TUN 转发".to_string(),
        },
        ComponentStatusItem {
            id: "easytier-cli".to_string(),
            name: "EasyTier CLI".to_string(),
            pinned_version: "v2.6.4".to_string(),
            license: "LGPL-3.0".to_string(),
            found: et_cli.is_some(),
            path: et_cli.map(|p| p.to_string_lossy().to_string()),
            note: "回环 RPC 状态与路由 JSON 查询".to_string(),
        },
        ComponentStatusItem {
            id: "sunshine".to_string(),
            name: "Sunshine Host".to_string(),
            pinned_version: "v2026.914.233613".to_string(),
            license: "GPL-3.0".to_string(),
            found: ss_dir.is_some() || ss_msi.is_some(),
            path: ss_dir
                .or(ss_msi)
                .map(|p| p.to_string_lossy().to_string()),
            note: "被控端桌面捕获、硬件编码与输入注入".to_string(),
        },
        ComponentStatusItem {
            id: "moonlight".to_string(),
            name: "Moonlight Qt".to_string(),
            pinned_version: "v6.2.0".to_string(),
            license: "GPL-3.0".to_string(),
            found: ml.is_some(),
            path: ml.map(|p| p.to_string_lossy().to_string()),
            note: "控制端硬解码播放器与键鼠/手柄回传".to_string(),
        },
    ]
}

pub async fn export_sanitized_bundle(state: &AppState) -> Result<PathBuf, String> {
    let room = state.easytier.get_room_snapshot().await;
    let peers = state.easytier.get_peers().await;
    let routes = state.easytier.get_routes().await;
    let host = state.sunshine.get_status().await;
    let mut session = state.session.get_snapshot().await;
    // Never include transient pairing PIN in diagnostic snapshots
    session.pin_display = session.pin_display.map(|_| "[REDACTED_PIN]".to_string());

    let logs: Vec<String> = state
        .recent_logs
        .lock()
        .await
        .iter()
        .map(|l| redact_secrets(l))
        .collect();

    let bundle = SanitizedDiagnosticBundle {
        generated_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        room,
        peers,
        routes,
        host,
        session,
        components: collect_component_status(state),
        logs,
    };

    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;
    let scrubbed_json = redact_secrets(&json);

    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let out_dir = app_data_dir().join("diagnostics");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_path = out_dir.join(format!("easystream-diag-{ts}.json"));
    std::fs::write(&out_path, scrubbed_json).map_err(|e| e.to_string())?;
    Ok(out_path)
}

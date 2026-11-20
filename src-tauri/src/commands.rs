// EasyStream - GPL-3.0-or-later
// Typed Tauri commands exposed to the React frontend (Section 6 contract).

use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::core::credentials::{generate_high_entropy_secret, generate_room_name};
use crate::core::diagnostics::{collect_component_status, export_sanitized_bundle};
use crate::core::moonlight::MoonlightManager;
use crate::state::{
    save_settings_to_disk, AppSettings, AppState, CommandError, ConnectPeerRequest,
    ConnectPeerResponse, DecidePairingRequest, DecidePairingResult, ErrorCode, FullSnapshot,
    GeneratedRoomCredentials, HostStatus, JoinNetworkRequest, RoomSnapshot, SessionSnapshot,
    StreamPreset, TrustedClient,
};

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Arc<AppState>>) -> Result<FullSnapshot, CommandError> {
    build_full_snapshot(state.inner()).await
}

pub async fn build_full_snapshot(state: &Arc<AppState>) -> Result<FullSnapshot, CommandError> {
    let room = state.easytier.get_room_snapshot().await;
    let peers = state.easytier.get_peers().await;
    let routes = state.easytier.get_routes().await;
    let host = state.sunshine.get_status().await;
    let session = state.session.get_snapshot().await;
    let settings = state.settings.read().await.clone();
    let components = collect_component_status(state);

    Ok(FullSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        room,
        peers,
        routes,
        snapshot_seq: 0,
        host,
        session,
        settings,
        presets: StreamPreset::presets(),
        components,
    })
}

#[tauri::command]
pub async fn generate_room_credentials() -> Result<GeneratedRoomCredentials, CommandError> {
    Ok(GeneratedRoomCredentials {
        room_name: generate_room_name(),
        room_secret: generate_high_entropy_secret(24),
    })
}

#[tauri::command]
pub async fn load_saved_room(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<GeneratedRoomCredentials>, CommandError> {
    Ok(state
        .easytier
        .load_saved_room_secret()
        .await
        .map(|(room_name, room_secret)| GeneratedRoomCredentials {
            room_name,
            room_secret,
        }))
}

#[tauri::command]
pub async fn join_network(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: JoinNetworkRequest,
) -> Result<RoomSnapshot, CommandError> {
    let rpc_port = state.settings.read().await.easytier_rpc_port;
    state
        .append_log(format!("正在加入 EasyTier 房间: {}", request.room_name))
        .await;

    let snap = match state.easytier.join_network(request.clone(), rpc_port).await {
        Ok(room) => room,
        Err(err) => {
            let _ = app.emit("room-updated", state.easytier.get_room_snapshot().await);
            return Err(err);
        }
    };

    {
        let mut settings = state.settings.write().await;
        settings.last_room_name = Some(request.room_name);
        settings.remember_room_secret = request.remember_secret;
        if !request.relay_nodes.is_empty() {
            settings.relay_nodes = request.relay_nodes;
        }
        if let Some(dev) = request.device_name {
            if !dev.trim().is_empty() {
                settings.device_name = dev;
            }
        }
        let _ = save_settings_to_disk(&settings);
    }

    let _ = app.emit("room-updated", &snap);
    Ok(snap)
}

#[tauri::command]
pub async fn leave_network(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), CommandError> {
    state.append_log("正在断开当前串流并离开房间...".to_string()).await;

    // Orderly teardown: 1. Disconnect active outbound session, 2. Stop host if enabled, 3. Leave room
    let _ = disconnect_internal(&app, state.inner(), None).await;
    if state.sunshine.get_status().await.enabled {
        let _ = state.sunshine.toggle_host_service(false, "").await;
    }
    state.easytier.leave_network().await?;

    let snap = state.easytier.get_room_snapshot().await;
    let _ = app.emit("room-updated", &snap);
    Ok(())
}

#[tauri::command]
pub async fn toggle_host_service(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enable: bool,
) -> Result<HostStatus, CommandError> {
    state
        .append_log(if enable {
            "正在开启被控端（允许远程控制）...".to_string()
        } else {
            "正在关闭被控端...".to_string()
        })
        .await;

    let alias = if enable {
        let room = state.easytier.get_room_snapshot().await;
        let ip = room.local_virtual_ip.as_deref()
            .and_then(|ip| ip.split('/').next())
            .and_then(|ip| ip.parse().ok())
            .ok_or_else(|| CommandError::new(ErrorCode::NoReachablePath, "请先加入房间并等待 EasyTier 分配虚拟 IPv4。"))?;
        crate::core::helper_ipc::room_interface_alias(ip)
            .map_err(|e| CommandError::new(ErrorCode::NoReachablePath, e))?
    } else {
        String::new()
    };
    let status = state.sunshine.toggle_host_service(enable, &alias).await?;
    let _ = app.emit("host-status-updated", &status);
    Ok(status)
}

#[tauri::command]
pub async fn decide_pairing(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: DecidePairingRequest,
) -> Result<DecidePairingResult, CommandError> {
    let (status, message) = state
        .sunshine
        .decide_pairing(
            &request.request_id,
            request.approve,
            request.pin.as_deref(),
            request.client_name.as_deref(),
        )
        .await?;

    state
        .append_log(format!(
            "被控端配对审批 {}: {} ({message})",
            request.request_id,
            if request.approve { "允许" } else { "拒绝" }
        ))
        .await;

    let host = state.sunshine.get_status().await;
    let _ = app.emit("host-status-updated", &host);

    Ok(DecidePairingResult {
        request_id: request.request_id,
        approved: request.approve,
        status,
        message,
    })
}

#[tauri::command]
pub async fn list_trusted_clients(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TrustedClient>, CommandError> {
    state.sunshine.list_trusted_clients().await
}

#[tauri::command]
pub async fn revoke_client(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    client_id: String,
) -> Result<bool, CommandError> {
    let ok = state.sunshine.revoke_client(&client_id).await?;
    state
        .append_log(format!("已撤销已授权设备 {client_id} (ok={ok})"))
        .await;
    let host = state.sunshine.get_status().await;
    let _ = app.emit("host-status-updated", &host);
    Ok(ok)
}

#[tauri::command]
pub async fn connect_to_peer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: ConnectPeerRequest,
) -> Result<ConnectPeerResponse, CommandError> {
    let peers = state.easytier.get_peers().await;
    let peer = peers
        .into_iter()
        .find(|p| p.peer_id == request.peer_id)
        .ok_or_else(|| {
            CommandError::new(
                ErrorCode::NoReachablePath,
                "未在当前房间找到目标设备，请等待设备列表刷新。",
            )
        })?;

    let settings = state.settings.read().await.clone();
    let display_mode = request
        .display_mode
        .as_deref()
        .unwrap_or(&settings.display_mode);
    let preset = StreamPreset::by_id(&request.preset_id, display_mode, &settings.video_codec);
    let room_id = state.easytier.get_room_snapshot().await.room_id;

    let (op_id, reused) = state
        .session
        .begin_operation(
            room_id,
            &peer.peer_id,
            &peer.hostname,
            &peer.virtual_ip,
            preset.clone(),
            peer.path.clone(),
        )
        .await?;

    if reused {
        return Ok(ConnectPeerResponse {
            operation_id: op_id,
            reused_existing: true,
        });
    }

    emit_session(&app, state.inner()).await;

    let state_clone = Arc::clone(state.inner());
    let app_clone = app.clone();
    let op_clone = op_id.clone();
    let requested_app = request.app_name.clone();
    let confirm_identity_change = request.confirm_identity_change;

    tokio::spawn(async move {
        run_connect_workflow(
            app_clone,
            state_clone,
            op_clone,
            peer.peer_id,
            peer.hostname,
            peer.virtual_ip,
            preset,
            requested_app,
            confirm_identity_change,
        )
        .await;
    });

    Ok(ConnectPeerResponse {
        operation_id: op_id,
        reused_existing: false,
    })
}

async fn run_connect_workflow(
    app: AppHandle,
    state: Arc<AppState>,
    op_id: String,
    peer_id: String,
    peer_name: String,
    peer_ip: String,
    preset: StreamPreset,
    requested_app: Option<String>,
    confirm_identity_change: bool,
) {
    let base_port = state.settings.read().await.sunshine_base_port;
    let probe = state
        .moonlight
        .probe_host_serverinfo(&peer_ip, base_port)
        .await;

    if !state.session.is_current(&op_id).await {
        return;
    }

    if !probe.reachable {
        let err = CommandError::new(
            ErrorCode::NoReachablePath,
            format!("无法连接到 {peer_name} ({peer_ip}:{base_port})，请确认对方已开启“允许远程控制”且网络可达。"),
        );
        state.session.set_failed(&op_id, err).await;
        emit_session(&app, &state).await;
        return;
    }

    if probe.busy {
        let err = CommandError::new(
            ErrorCode::HostBusy,
            "目标主机正在进行另一场串流会话（主机忙），首版仅支持单路入站串流。",
        );
        state.session.set_failed(&op_id, err).await;
        emit_session(&app, &state).await;
        return;
    }

    // Verify trusted host uniqueid identity (Section 3.4)
    if let Some(ref uid) = probe.unique_id {
        let mut settings = state.settings.write().await;
        if let Some(expected) = settings.trusted_host_identities.get(&peer_id) {
            if expected != uid && !confirm_identity_change {
                let err = CommandError::new(
                    ErrorCode::HostIdentityMismatch,
                    "目标主机的配对身份标识已发生变化（可能重装过系统或更换了主机），请确认后再重新连接。",
                );
                drop(settings);
                state.session.set_failed(&op_id, err).await;
                emit_session(&app, &state).await;
                return;
            }
        }
        settings
            .trusted_host_identities
            .insert(peer_id.clone(), uid.clone());
        let _ = save_settings_to_disk(&settings);
    }

    // Step 1-3: If not paired yet, generate 4-digit PIN and run Moonlight.exe pair
    let mut pairing_wait_ms: Option<u64> = None;
    let initial_apps = state.moonlight.list_apps(&peer_ip).await;
    if !state.session.is_current(&op_id).await {
        return;
    }
    let needs_pairing = matches!(&initial_apps, Err(err) if err.code == ErrorCode::Unauthorized);
    if needs_pairing {
        let pin = MoonlightManager::generate_pairing_pin();
        state.session.set_pairing_waiting(&op_id, &pin).await;
        emit_session(&app, &state).await;
        state
            .append_log(format!(
                "发起首次配对至 {peer_name} ({peer_ip})，等待被控端录入短码..."
            ))
            .await;

        let pair_start = std::time::Instant::now();
        let pair_pid = match state.moonlight.start_pairing(&peer_ip, &pin) {
            Ok(pid) => pid,
            Err(err) => {
                state.session.set_failed(&op_id, err).await;
                emit_session(&app, &state).await;
                return;
            }
        };

        // Wait up to 90s for user on host to enter PIN, checking cancellation every 350ms
        let mut paired_ok = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while std::time::Instant::now() < deadline {
            if !state.session.is_current(&op_id).await {
                let _ = state.process_mgr.stop_owned(pair_pid);
                return;
            }
            if let Some(exit_code) = state.process_mgr.try_wait_owned(pair_pid) {
                paired_ok = exit_code == 0;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        }

        if !paired_ok {
            let _ = state.process_mgr.stop_owned(pair_pid);
            let err = CommandError::new(
                ErrorCode::PairingTimeout,
                "配对未完成（可能超时、短码错误或被被控端拒绝），请重试。",
            );
            state.session.set_failed(&op_id, err).await;
            emit_session(&app, &state).await;
            return;
        }
        pairing_wait_ms = Some(pair_start.elapsed().as_millis() as u64);
    }

    if !state.session.is_current(&op_id).await {
        return;
    }

    // Step 4: Enumerate published apps via Moonlight.exe list <ip> --csv
    state
        .session
        .set_listing_apps(&op_id, pairing_wait_ms)
        .await;
    emit_session(&app, &state).await;

    let app_result = if needs_pairing {
        state.moonlight.list_apps(&peer_ip).await
    } else {
        initial_apps
    };
    let apps = match app_result {
        Ok(apps) if apps.is_empty() => {
            let err = CommandError::new(ErrorCode::AppMissing, "主机未发布任何可串流应用。");
            state.session.set_failed(&op_id, err).await;
            emit_session(&app, &state).await;
            return;
        }
        Ok(a) => a,
        Err(err) => {
            state.session.set_failed(&op_id, err).await;
            emit_session(&app, &state).await;
            return;
        }
    };

    if !state.session.is_current(&op_id).await {
        return;
    }

    let saved_app = state
        .settings
        .read()
        .await
        .selected_apps_by_host
        .get(&peer_id)
        .cloned();

    let chosen_app = requested_app
        .filter(|a| apps.iter().any(|x| x == a))
        .or_else(|| saved_app.filter(|a| apps.iter().any(|x| x == a)))
        .or_else(|| apps.iter().find(|a| a.eq_ignore_ascii_case("Desktop")).cloned())
        .unwrap_or_else(|| apps[0].clone());

    {
        let mut settings = state.settings.write().await;
        settings
            .selected_apps_by_host
            .insert(peer_id.clone(), chosen_app.clone());
        let _ = save_settings_to_disk(&settings);
    }

    state.session.set_launching_player(&op_id, &chosen_app).await;
    emit_session(&app, &state).await;

    match state.moonlight.start_stream(&peer_ip, &chosen_app, &preset) {
        Ok(pid) => {
            state.session.set_player_opened(&op_id, pid).await;
            state
                .append_log(format!(
                    "已打开 Moonlight 播放器 (pid={pid}) -> {peer_name} [{chosen_app}] {}x{}@{}fps {}kbps",
                    preset.width, preset.height, preset.fps, preset.bitrate_kbps
                ))
                .await;
            emit_session(&app, &state).await;
        }
        Err(err) => {
            state.session.set_failed(&op_id, err).await;
            emit_session(&app, &state).await;
        }
    }
}

#[tauri::command]
pub async fn disconnect(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    operation_id: Option<String>,
) -> Result<(), CommandError> {
    disconnect_internal(&app, state.inner(), operation_id.as_deref()).await
}

pub async fn disconnect_internal(
    app: &AppHandle,
    state: &Arc<AppState>,
    operation_id: Option<&str>,
) -> Result<(), CommandError> {
    let (pid, peer_ip) = state.session.begin_disconnect(operation_id).await;
    emit_session(app, state).await;

    if let Some(pid) = pid {
        let _ = state.process_mgr.stop_owned(pid);
    }
    if let Some(ip) = peer_ip {
        state.moonlight.quit_remote_app(&ip).await;
    }

    state.session.finish_disconnect().await;
    emit_session(app, state).await;
    Ok(())
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<AppSettings, CommandError> {
    {
        let mut guard = state.settings.write().await;
        *guard = settings.clone();
        save_settings_to_disk(&guard)
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
    }
    let snap = build_full_snapshot(state.inner()).await?;
    let _ = app.emit("snapshot-updated", &snap);
    Ok(settings)
}

#[tauri::command]
pub async fn export_diagnostics(state: State<'_, Arc<AppState>>) -> Result<String, CommandError> {
    let path = export_sanitized_bundle(state.inner())
        .await
        .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_logs(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, CommandError> {
    Ok(state.recent_logs.lock().await.clone())
}

async fn emit_session(app: &AppHandle, state: &Arc<AppState>) {
    let session: SessionSnapshot = state.session.get_snapshot().await;
    let _ = app.emit("session-updated", &session);
}

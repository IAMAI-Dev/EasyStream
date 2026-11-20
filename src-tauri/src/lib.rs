// EasyStream - GPL-3.0-or-later
// Tauri v2 application entry, system tray lifecycle, 2s background state loop,
// and orderly resource teardown.

pub mod commands;
pub mod core;
pub mod state;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

use crate::commands::{
    connect_to_peer, decide_pairing, disconnect, export_diagnostics, generate_room_credentials,
    get_logs, get_snapshot, join_network, leave_network, list_trusted_clients,
    load_saved_room, revoke_client, toggle_host_service, update_settings,
};
use crate::state::{AppState, PeersUpdatedEvent, RoomStage, SessionStage};

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("easystream=info,warn")
        .try_init();

    let app_state = Arc::new(AppState::new());

    tauri::Builder::default()
        .manage(Arc::clone(&app_state))
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            generate_room_credentials,
            load_saved_room,
            join_network,
            leave_network,
            toggle_host_service,
            decide_pairing,
            list_trusted_clients,
            revoke_client,
            connect_to_peer,
            disconnect,
            update_settings,
            export_diagnostics,
            get_logs,
        ])
        .setup(move |app| {
            setup_tray(app.handle())?;
            start_background_monitor(app.handle().clone(), Arc::clone(&app_state));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Section 5: closing main window minimizes/hides to system tray
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running EasyStream");
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示 EasyStream 主窗口", true, None::<&str>)?;
    let stop_host_item =
        MenuItem::with_id(app, "stop_host", "立即停止被控", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 EasyStream", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &stop_host_item, &quit_item])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("EasyStream — 跨网远程桌面与游戏串流")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
            "stop_host" => {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<Arc<AppState>>();
                    let status = state.sunshine.toggle_host_service(false, "").await;
                    if let Ok(s) = status {
                        let _ = app_handle.emit("host-status-updated", &s);
                    }
                });
            }
            "quit" => {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<Arc<AppState>>();
                    perform_clean_exit(&app_handle, state.inner()).await;
                    app_handle.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

async fn perform_clean_exit(app: &AppHandle, state: &Arc<AppState>) {
    let _ = crate::commands::disconnect_internal(app, state, None).await;
    if state.sunshine.was_started_by_easystream().await
        || state.sunshine.get_status().await.enabled
    {
        let _ = state.sunshine.toggle_host_service(false, "").await;
    }
    let _ = state.easytier.leave_network().await;
    state.process_mgr.stop_all_owned();
}

fn start_background_monitor(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut next_app_refresh = std::time::Instant::now();
        loop {
            ticker.tick().await;

            // 1. Check if active Moonlight player exited on its own
            let session_snap = state.session.get_snapshot().await;
            if session_snap.stage == SessionStage::PlayerOpened {
                if let Some(pid) = session_snap.player_pid {
                    if state.process_mgr.try_wait_owned(pid).is_some() {
                        state.session.finish_disconnect().await;
                        let updated = state.session.get_snapshot().await;
                        let _ = app.emit("session-updated", &updated);
                    }
                }
            }

            // 2. Poll EasyTier peers & routes every 2s when in room
            let room_snap = state.easytier.get_room_snapshot().await;
            if matches!(room_snap.stage, RoomStage::InRoom | RoomStage::Reconnecting) {
                if let Ok(_ip_changed) = state.easytier.poll_once().await {
                    let peers = state.easytier.get_peers().await;
                    let (base_port, trusted_map) = {
                        let s = state.settings.read().await;
                        (s.sunshine_base_port, s.trusted_host_identities.clone())
                    };

                    let refresh_apps = std::time::Instant::now() >= next_app_refresh;
                    if refresh_apps {
                        next_app_refresh = std::time::Instant::now() + std::time::Duration::from_secs(30);
                    }
                    for peer in &peers {
                        let probe = state
                            .moonlight
                            .probe_host_serverinfo(&peer.virtual_ip, base_port)
                            .await;
                        let identity_changed = match (&probe.unique_id, trusted_map.get(&peer.peer_id))
                        {
                            (Some(actual), Some(expected)) => actual != expected,
                            _ => false,
                        };
                        let (paired, apps) = if !probe.reachable || identity_changed {
                            (None, Vec::new())
                        } else if !probe.busy && (refresh_apps || peer.host_unique_id != probe.unique_id) {
                            match state.moonlight.list_apps(&peer.virtual_ip).await {
                                Ok(apps) => (Some(true), apps),
                                Err(e) if e.code == crate::state::ErrorCode::Unauthorized => (Some(false), Vec::new()),
                                Err(_) => (None, Vec::new()),
                            }
                        } else {
                            let known = if peer.paired { Some(true) }
                                else if peer.readiness == crate::state::HostReadiness::NeedsPairing { Some(false) }
                                else { None };
                            (known, peer.available_apps.clone())
                        };

                        state
                            .easytier
                            .update_peer_probe(
                                &peer.peer_id,
                                probe.reachable,
                                paired == Some(true) && !apps.is_empty() && !probe.busy,
                                paired,
                                probe.busy,
                                probe.unique_id,
                                identity_changed,
                                apps,
                            )
                            .await;

                        if session_snap.peer_id.as_deref() == Some(&peer.peer_id) {
                            state
                                .session
                                .update_route_hint(peer.path.clone(), peer.loss_rate)
                                .await;
                        }
                    }

                }
                let updated_peers = state.easytier.get_peers().await;
                let updated_room = state.easytier.get_room_snapshot().await;
                let seq = state.easytier.next_seq().await;
                let _ = app.emit("room-updated", &updated_room);
                let _ = app.emit(
                    "network-peers-updated",
                    &PeersUpdatedEvent {
                        room_id: updated_room.room_id.unwrap_or_default(),
                        snapshot_seq: seq,
                        peers: updated_peers,
                    },
                );
            }

            // 3. Refresh local Sunshine host status & pending pairings
            let host_status = state.sunshine.refresh_status().await;
            let _ = app.emit("host-status-updated", &host_status);
        }
    });
}

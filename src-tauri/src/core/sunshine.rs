// EasyStream - GPL-3.0-or-later
// Sunshine v2026.914.233613 local management API, ownership verification,
// on-demand MSI installation, pairing approval/cancellation, and client revocation.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::core::credentials::{generate_high_entropy_secret, read_secret, save_secret};
use crate::core::display_mgr::{enumerate_active_displays, inspect_virtual_display_state};
use crate::core::helper_ipc::{invoke_privileged_operation, HelperOperation};
use crate::core::process_mgr::{
    query_windows_service, sunshine_ownership_marker_path, ProcessManager, WindowsServiceState,
};
use crate::state::{
    CommandError, ErrorCode, HostStatus, PendingPairingRequest, TrustedClient,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SUNSHINE_ADMIN_CRED_KEY: &str = "sunshine_admin";
const SUNSHINE_CERT_PIN_KEY: &str = "sunshine_cert_sha256";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SunshineOwnershipMarker {
    installed_by: String,
    version: String,
    install_dir: String,
    installed_at: String,
}

#[derive(Debug, Deserialize)]
struct SunshinePendingPairingsResp {
    #[serde(default)]
    pairings: Vec<SunshinePendingItem>,
}

#[derive(Debug, Deserialize)]
struct SunshinePendingItem {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    address: String,
}

#[derive(Debug, Deserialize)]
struct SunshineClientsResp {
    named_certs: Vec<SunshineNamedCert>,
    #[serde(default)]
    status: bool,
}

#[derive(Debug, Deserialize)]
struct SunshineNamedCert {
    uuid: String,
    #[serde(default)]
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct SunshineStatusResp {
    #[serde(default)]
    status: bool,
}

pub struct SunshineManager {
    process_mgr: Arc<ProcessManager>,
    status: Arc<RwLock<HostStatus>>,
    base_port: Arc<Mutex<u16>>,
    started_by_easystream: Arc<Mutex<bool>>,
    host_toggle: Mutex<()>,
}

impl SunshineManager {
    pub fn new(process_mgr: Arc<ProcessManager>, base_port: u16) -> Self {
        let mut initial = HostStatus::default();
        initial.management_port = base_port.saturating_add(1);
        Self {
            process_mgr,
            status: Arc::new(RwLock::new(initial)),
            base_port: Arc::new(Mutex::new(base_port)),
            started_by_easystream: Arc::new(Mutex::new(false)),
            host_toggle: Mutex::new(()),
        }
    }

    pub async fn get_status(&self) -> HostStatus {
        self.status.read().await.clone()
    }

    pub async fn was_started_by_easystream(&self) -> bool {
        *self.started_by_easystream.lock().await
    }

    pub fn find_sunshine_install_dir(&self) -> Option<PathBuf> {
        let candidates = [
            PathBuf::from(r"C:\Program Files\Sunshine"),
            PathBuf::from(r"C:\Program Files (x86)\Sunshine"),
        ];
        for dir in candidates {
            if dir.join("sunshine.exe").is_file() {
                return Some(dir);
            }
        }
        None
    }

    pub fn find_staged_msi(&self) -> Option<PathBuf> {
        if let Some(p) = self
            .process_mgr
            .resolve_component_binary("sunshine", "sunshine-windows-installer.msi")
        {
            return Some(p);
        }
        if !cfg!(debug_assertions) {
            return None;
        }
        // Scan resources/sunshine/*.msi
        if let Ok(cwd) = std::env::current_dir() {
            for base in [
                cwd.join("src-tauri").join("resources").join("sunshine"),
                cwd.join("resources").join("sunshine"),
            ] {
                if let Ok(entries) = std::fs::read_dir(&base) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().and_then(|s| s.to_str()) == Some("msi") {
                            return Some(p);
                        }
                    }
                }
            }
        }
        None
    }

    /// Refresh installation, ownership, Windows service, display outputs, and (if running) API state.
    pub async fn refresh_status(&self) -> HostStatus {
        let install_dir = self.find_sunshine_install_dir();
        let installed = install_dir.is_some();
        let svc_state = query_windows_service("SunshineService");
        let service_running = svc_state == WindowsServiceState::Running;
        let marker_exists = sunshine_ownership_marker_path().is_file();
        let owned = installed && marker_exists;
        let conflict = installed && !marker_exists;

        let displays = enumerate_active_displays();
        let virtual_display = inspect_virtual_display_state(&displays);

        let cert_fp = install_dir
            .as_deref()
            .and_then(read_sunshine_cert_fingerprint);

        let cert_changed = matches!(
            read_secret(SUNSHINE_CERT_PIN_KEY),
            Ok(Some((_, saved))) if cert_fp.as_ref() != Some(&saved)
        );

        let conflict_message = if conflict {
            Some("检测到系统已存在非 EasyStream 管理的 Sunshine 安装。为避免覆盖您的现有配置，默认仅作为控制端运行；如需由 EasyStream 管理，请先确认接管或卸载原实例。".to_string())
        } else {
            None
        };

        let mut pending_pairings = Vec::new();
        let mut trusted_clients = Vec::new();
        let mut last_error = None;

        if cert_changed {
            last_error = Some(
                CommandError::new(
                    ErrorCode::HostIdentityMismatch,
                    "检测到本机 Sunshine TLS 证书已变更，请重新确认本机被控实例归属。",
                ),
            );
        }

        let enabled = self.status.read().await.enabled;
        if service_running && enabled && owned && !cert_changed {
            if let Ok(client) = self.build_local_https_client(install_dir.as_deref()) {
                if let Ok(creds) = self.get_or_init_admin_creds().await {
                    if let Ok(pairings) = self.fetch_pending_pairings_http(&client, &creds).await {
                        pending_pairings = pairings;
                    }
                    if let Ok(clients) = self.fetch_clients_http(&client, &creds).await {
                        trusted_clients = clients;
                    }
                }
            }
        }

        let base_port = *self.base_port.lock().await;
        let mut status = self.status.write().await;
        status.sunshine_installed = installed;
        status.sunshine_version = install_dir
            .as_deref()
            .and_then(read_sunshine_version)
            .or_else(|| {
                if installed {
                    Some("installed".to_string())
                } else {
                    None
                }
            });
        status.sunshine_owned_by_easystream = owned;
        status.sunshine_conflict_detected = conflict;
        status.sunshine_conflict_message = conflict_message;
        status.service_running = service_running;
        status.management_port = base_port.saturating_add(1);
        status.cert_fingerprint = cert_fp;
        status.pending_pairings = pending_pairings;
        status.trusted_clients = trusted_clients;
        status.capture_displays = displays;
        status.virtual_display = virtual_display;
        if last_error.is_some() {
            status.last_error = last_error;
        }
        status.clone()
    }

    /// Install the staged Sunshine x64 MSI on-demand and record EasyStream ownership marker.
    async fn install_sunshine_msi(&self) -> Result<HostStatus, CommandError> {
        if self.find_sunshine_install_dir().is_some() {
            return Ok(self.refresh_status().await);
        }

        let msi_path = self.find_staged_msi().ok_or_else(|| {
            CommandError::new(
                ErrorCode::ComponentMissing,
                "未找到 Sunshine x64 MSI 安装包（resources/sunshine/*.msi），请先运行 scripts/fetch-upstream.ps1 下载上游组件。",
            )
        })?;

        #[cfg(windows)]
        {
            let msi_str = msi_path.to_string_lossy().to_string();
            let output = tokio::task::spawn_blocking(move || {
                let manifest: serde_json::Value =
                    serde_json::from_str(include_str!("../../resources/manifest.json"))
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let expected = manifest["components"]["sunshine"]["sha256"].as_str();
                let digest = hex::encode(Sha256::digest(std::fs::read(&msi_path)?));
                if expected != Some(digest.as_str()) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Sunshine MSI 校验失败，请重新安装 EasyStream。",
                    ));
                }
                std::process::Command::new("msiexec")
                    .args(["/i", &msi_str, "/qn", "/norestart", "REMOVE=CM_C_firewall"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
            })
            .await
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e.to_string()))?
            .map_err(|e| {
                CommandError::new(ErrorCode::PermissionRequired, format!("启动 msiexec 失败: {e}"))
            })?;

            let code = output.status.code().unwrap_or(-1);
            if !matches!(code, 0 | 3010) {
                return Err(CommandError::new(
                    ErrorCode::PermissionRequired,
                    format!("Sunshine MSI 安装失败（退出码 {code}），静默安装通常需要管理员权限。"),
                ));
            }
        }

        let install_dir = self
            .find_sunshine_install_dir()
            .ok_or_else(|| CommandError::new(ErrorCode::ComponentMissing, "Sunshine MSI 已退出，但未找到安装后的程序。"))?;
        self.write_ownership_marker(&install_dir, "v2026.914.233613")
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;

        Ok(self.refresh_status().await)
    }

    fn write_ownership_marker(&self, install_dir: &Path, version: &str) -> Result<(), String> {
        let marker_path = sunshine_ownership_marker_path();
        if let Some(parent) = marker_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let marker = SunshineOwnershipMarker {
            installed_by: "EasyStream".to_string(),
            version: version.to_string(),
            install_dir: install_dir.to_string_lossy().to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string_pretty(&marker).map_err(|e| e.to_string())?;
        std::fs::write(marker_path, json).map_err(|e| e.to_string())
    }

    /// Toggle host remote control service (Section 4 & Section 6).
    pub async fn toggle_host_service(
        &self,
        enable: bool,
        easytier_iface: &str,
    ) -> Result<HostStatus, CommandError> {
        let _guard = self.host_toggle.lock().await;
        let mut current = self.refresh_status().await;

        if enable {
            if current.sunshine_conflict_detected {
                return Err(CommandError::new(
                    ErrorCode::SunshineConflict,
                    "检测到非 EasyStream 管理的 Sunshine 实例，已拒绝自动接管以保护现有配置。",
                ));
            }

            let displays = enumerate_active_displays();
            let has_capturable = displays.iter().any(|d| d.width > 0 && d.height > 0);
            if !has_capturable {
                return Err(CommandError::new(
                    ErrorCode::EncodeOrCaptureFailed,
                    "未检测到可捕获的活动显示输出（物理屏或已启用虚拟屏），无法开启被控。",
                ));
            }

            if !current.sunshine_installed {
                current = self.install_sunshine_msi().await?;
                if !current.sunshine_owned_by_easystream {
                    return Err(CommandError::new(ErrorCode::SunshineConflict, "无法确认新安装的 Sunshine 由 EasyStream 管理。"));
                }
            }

            // The upstream MSI registers an auto-starting service. Apply our
            // configuration before starting it, and keep it off across reboot.
            #[cfg(windows)]
            if !current.enabled {
                let output = std::process::Command::new("sc.exe")
                    .args(["config", "SunshineService", "start=", "demand"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                    .map_err(|e| CommandError::new(ErrorCode::PermissionRequired, e.to_string()))?;
                if !output.status.success() {
                    return Err(CommandError::new(
                        ErrorCode::PermissionRequired,
                        "无法将受管 Sunshine 服务设置为按需启动。",
                    ));
                }
                crate::core::process_mgr::stop_windows_service("SunshineService")
                    .map_err(|e| CommandError::new(ErrorCode::PermissionRequired, e))?;
                for _ in 0..20 {
                    if query_windows_service("SunshineService") == WindowsServiceState::Stopped {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                current.service_running =
                    query_windows_service("SunshineService") != WindowsServiceState::Stopped;
                if current.service_running {
                    return Err(CommandError::new(ErrorCode::HostBusy, "请等待 Sunshine 服务结束后重试。"));
                }
            }

            let install_dir = self.find_sunshine_install_dir().ok_or_else(|| {
                CommandError::new(ErrorCode::ComponentMissing, "Sunshine 安装目录不存在")
            })?;
            let base_port = *self.base_port.lock().await;

            // Configure hardened sunshine.conf: upnp=disabled, origin_web_ui_allowed=pc
            ensure_hardened_sunshine_conf(&install_dir, base_port)
                .map_err(|e| CommandError::new(ErrorCode::PermissionRequired, e))?;

            // Scope Windows Firewall to EasyTier interface only
            match invoke_privileged_operation(HelperOperation::ConfigureFirewall {
                easytier_interface_alias: easytier_iface.to_string(),
                sunshine_base_port: base_port,
            })
            .await
            {
                Ok(resp) if resp.ok => {
                    self.status.write().await.firewall_scoped = true;
                }
                Ok(resp) => {
                    return Err(CommandError::new(ErrorCode::PermissionRequired, resp.message));
                }
                Err(e) => {
                    return Err(CommandError::new(ErrorCode::PermissionRequired, e));
                }
            }

            // Start SunshineService if not already running
            if !current.service_running {
                invoke_privileged_operation(HelperOperation::StartOwnedSunshineService)
                    .await
                    .map_err(|e| CommandError::new(ErrorCode::PermissionRequired, e))?;
                *self.started_by_easystream.lock().await = true;

                // Wait briefly for HTTPS API readiness
                for _ in 0..10 {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    if find_sunshine_cert_bytes(&install_dir).is_some()
                        && tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, base_port.saturating_add(1))).await.is_ok()
                    {
                        break;
                    }
                }
            }

            if let Err(err) = self.ensure_sunshine_credentials_initialized(&install_dir).await {
                if !current.service_running {
                    if let Ok(resp) = invoke_privileged_operation(HelperOperation::StopOwnedSunshineService).await {
                        if resp.ok {
                            *self.started_by_easystream.lock().await = false;
                        }
                    }
                }
                let mut status = self.status.write().await;
                status.enabled = false;
                status.last_error = Some(err.clone());
                return Err(err);
            }

            self.status.write().await.enabled = true;
            self.status.write().await.last_error = None;
        } else {
            // Disabling host: close active sessions, stop service if owned/started by us
            self.status.write().await.enabled = false;
            if let Some(install_dir) = self.find_sunshine_install_dir() {
                if let Ok(client) = self.build_local_https_client(Some(&install_dir)) {
                    if let Ok(creds) = self.get_or_init_admin_creds().await {
                        let _ = self.close_active_app_http(&client, &creds).await;
                    }
                }
            }

            if !current.sunshine_conflict_detected && current.service_running {
                let _ =
                    invoke_privileged_operation(HelperOperation::StopOwnedSunshineService).await;
                *self.started_by_easystream.lock().await = false;
            }
        }

        Ok(self.refresh_status().await)
    }

    pub async fn decide_pairing(
        &self,
        pairing_id: &str,
        approve: bool,
        pin: Option<&str>,
        client_name: Option<&str>,
    ) -> Result<(bool, String), CommandError> {
        let install_dir = self.find_sunshine_install_dir().ok_or_else(|| {
            CommandError::new(ErrorCode::ComponentMissing, "Sunshine 未安装")
        })?;
        let client = self
            .build_local_https_client(Some(&install_dir))
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
        let creds = self.get_or_init_admin_creds().await?;

        // Verify request is still pending in GET /api/pin (Section 4 step 2)
        let pending = self.fetch_pending_pairings_http(&client, &creds).await?;
        if !pending.iter().any(|p| p.pairing_id == pairing_id) {
            return Err(CommandError::new(
                ErrorCode::PairingTimeout,
                "该配对请求已过期、已取消或无效，请在控制端重新发起连接。",
            ));
        }

        let mgmt_port = *self.base_port.lock().await + 1;
        let url = format!("https://127.0.0.1:{mgmt_port}/api/pin");

        if !approve {
            let body = serde_json::json!({ "pairing_id": pairing_id });
            let resp = client
                .delete(&url)
                .basic_auth(&creds.0, Some(&creds.1))
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    CommandError::new(ErrorCode::InternalError, format!("取消配对请求失败: {e}"))
                })?;
            let parsed: SunshineStatusResp = resp.json().await.unwrap_or(SunshineStatusResp {
                status: false,
            });
            let _ = self.refresh_status().await;
            return Ok((parsed.status, "已拒绝该设备的配对请求。".to_string()));
        }

        let pin_str = pin.unwrap_or("").trim();
        if pin_str.len() != 4 || !pin_str.chars().all(|c| c.is_ascii_digit()) {
            return Err(CommandError::new(
                ErrorCode::PairingInvalidPin,
                "请输入控制端显示的 4 位数字短码。",
            ));
        }

        let name = client_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("EasyStream-Client");

        // Section 4 step 3: POST /api/pin with { pairing_id, pin, name }
        let body = serde_json::json!({
            "pairing_id": pairing_id,
            "pin": pin_str,
            "name": name,
        });
        let resp = client
            .post(&url)
            .basic_auth(&creds.0, Some(&creds.1))
            .timeout(std::time::Duration::from_secs(30))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                CommandError::new(ErrorCode::PairingTimeout, format!("提交配对 PIN 超时或失败: {e}"))
            })?;

        if !resp.status().is_success() {
            let status_code = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(CommandError::new(
                ErrorCode::PairingInvalidPin,
                format!("Sunshine 拒绝配对 ({status_code}): {err_text}"),
            ));
        }

        let parsed: SunshineStatusResp = resp.json().await.unwrap_or(SunshineStatusResp {
            status: false,
        });
        let _ = self.refresh_status().await;
        if parsed.status {
            Ok((true, "设备配对成功，已加入已授权设备列表。".to_string()))
        } else {
            Err(CommandError::new(
                ErrorCode::PairingInvalidPin,
                "短码校验失败或控制端已超时取消，请核对短码后重试。",
            ))
        }
    }

    pub async fn list_trusted_clients(&self) -> Result<Vec<TrustedClient>, CommandError> {
        let install_dir = self.find_sunshine_install_dir().ok_or_else(|| {
            CommandError::new(ErrorCode::ComponentMissing, "Sunshine 未安装")
        })?;
        let client = self
            .build_local_https_client(Some(&install_dir))
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
        let creds = self.get_or_init_admin_creds().await?;
        let clients = self.fetch_clients_http(&client, &creds).await?;
        self.status.write().await.trusted_clients = clients.clone();
        Ok(clients)
    }

    pub async fn revoke_client(&self, client_id: &str) -> Result<bool, CommandError> {
        let install_dir = self.find_sunshine_install_dir().ok_or_else(|| {
            CommandError::new(ErrorCode::ComponentMissing, "Sunshine 未安装")
        })?;
        let client = self
            .build_local_https_client(Some(&install_dir))
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
        let creds = self.get_or_init_admin_creds().await?;
        let mgmt_port = *self.base_port.lock().await + 1;

        // Unpair by persistent client UUID (Section 4 step 5)
        let url = format!("https://127.0.0.1:{mgmt_port}/api/clients/unpair");
        let resp = client
            .post(&url)
            .basic_auth(&creds.0, Some(&creds.1))
            .json(&serde_json::json!({ "uuid": client_id }))
            .send()
            .await
            .map_err(|e| {
                CommandError::new(ErrorCode::InternalError, format!("撤销设备失败: {e}"))
            })?;

        let parsed: SunshineStatusResp = resp.json().await.unwrap_or(SunshineStatusResp {
            status: false,
        });

        // Explicitly close running app/session if needed so old session terminates immediately
        let _ = self.close_active_app_http(&client, &creds).await;
        let _ = self.refresh_status().await;
        Ok(parsed.status)
    }

    async fn get_or_init_admin_creds(&self) -> Result<(String, String), CommandError> {
        if let Some((user, pass)) = read_secret(SUNSHINE_ADMIN_CRED_KEY)
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))? {
            if !user.is_empty() && !pass.is_empty() {
                return Ok((user, pass));
            }
        }
        let user = "easystream_admin".to_string();
        let pass = generate_high_entropy_secret(24);
        save_secret(SUNSHINE_ADMIN_CRED_KEY, &user, &pass)
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
        Ok((user, pass))
    }

    async fn ensure_sunshine_credentials_initialized(
        &self,
        install_dir: &Path,
    ) -> Result<(), CommandError> {
        let pem = find_sunshine_cert_bytes(install_dir)
            .ok_or_else(|| CommandError::new(ErrorCode::HostIdentityMismatch, "未找到受管 Sunshine 的本机证书。"))?;
        let fingerprint = hex::encode(Sha256::digest(&pem));
        let saved = read_secret(SUNSHINE_CERT_PIN_KEY)
            .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
        if let Some((_, expected)) = &saved {
            if expected != &fingerprint {
                return Err(CommandError::new(ErrorCode::HostIdentityMismatch, "Sunshine 证书已变更，请核实受管实例。"));
            }
        }
        let client = Self::pinned_client(&pem)
            .map_err(|e| CommandError::new(ErrorCode::HostIdentityMismatch, e))?;
        let creds = self.get_or_init_admin_creds().await?;
        let mgmt_port = self.base_port.lock().await.saturating_add(1);
        let check_url = format!("https://127.0.0.1:{mgmt_port}/api/clients/list");
        let check = client.get(&check_url).basic_auth(&creds.0, Some(&creds.1))
            .send().await.map_err(|e| CommandError::new(ErrorCode::Unauthorized, e.to_string()))?;
        if check.status().is_success() {
            check.json::<SunshineClientsResp>().await
                .map_err(|e| CommandError::new(ErrorCode::Unauthorized, e.to_string()))?;
        } else {
            let first_setup = check.status() == reqwest::StatusCode::TEMPORARY_REDIRECT
                && check.headers().get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok()) == Some("/welcome");
            if !first_setup {
                return Err(CommandError::new(ErrorCode::Unauthorized, format!("Sunshine 管理接口返回 {}", check.status())));
            }
            let pw_url = format!("https://127.0.0.1:{mgmt_port}/api/password");
            let resp = client.post(pw_url)
                .json(&serde_json::json!({
                    "currentUsername": "", "currentPassword": "",
                    "newUsername": creds.0, "newPassword": creds.1, "confirmNewPassword": creds.1,
                }))
                .send().await.and_then(|r| r.error_for_status())
                .map_err(|e| CommandError::new(ErrorCode::Unauthorized, e.to_string()))?;
            let result: SunshineStatusResp = resp.json().await
                .map_err(|e| CommandError::new(ErrorCode::Unauthorized, e.to_string()))?;
            if !result.status {
                return Err(CommandError::new(ErrorCode::Unauthorized, "Sunshine 拒绝初始化管理凭据。"));
            }
            self.fetch_clients_http(&client, &creds).await?;
        }
        if saved.is_none() {
            save_secret(SUNSHINE_CERT_PIN_KEY, "local", &fingerprint)
                .map_err(|e| CommandError::new(ErrorCode::InternalError, e))?;
        }
        Ok(())
    }

    fn build_local_https_client(&self, install_dir: Option<&Path>) -> Result<Client, String> {
        let pem = install_dir.and_then(find_sunshine_cert_bytes).ok_or("Sunshine 本机证书不可用")?;
        let (_, expected) = read_secret(SUNSHINE_CERT_PIN_KEY)?.ok_or("Sunshine 证书尚未绑定")?;
        if hex::encode(Sha256::digest(&pem)) != expected {
            return Err("Sunshine 证书与已保存的指纹不符".into());
        }
        Self::pinned_client(&pem)
    }

    fn pinned_client(pem: &[u8]) -> Result<Client, String> {
        Client::builder().use_preconfigured_tls(super::sunshine_tls::pinned_config(pem)?)
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(6))
            .no_proxy().build().map_err(|e| e.to_string())
    }

    async fn fetch_pending_pairings_http(
        &self,
        client: &Client,
        creds: &(String, String),
    ) -> Result<Vec<PendingPairingRequest>, CommandError> {
        let mgmt_port = *self.base_port.lock().await + 1;
        let url = format!("https://127.0.0.1:{mgmt_port}/api/pin");
        let resp = client
            .get(&url)
            .basic_auth(&creds.0, Some(&creds.1))
            .send()
            .await
            .map_err(|e| {
                CommandError::new(
                    ErrorCode::InternalError,
                    format!("请求 Sunshine GET /api/pin 失败: {e}"),
                )
            })?;

        if !resp.status().is_success() {
            return Err(CommandError::new(
                ErrorCode::Unauthorized,
                format!("Sunshine 管理接口认证失败 ({})", resp.status()),
            ));
        }

        let body: SunshinePendingPairingsResp = resp.json().await.map_err(|e| {
            CommandError::new(ErrorCode::InternalError, format!("解析 /api/pin 失败: {e}"))
        })?;
        let now_ms = chrono::Utc::now().timestamp_millis();

        Ok(body
            .pairings
            .into_iter()
            .map(|p| PendingPairingRequest {
                request_id: p.id.clone(),
                pairing_id: p.id,
                client_name: if p.name.is_empty() {
                    "未知控制端".to_string()
                } else {
                    p.name
                },
                source_address: p.address,
                valid: true,
                received_at_ms: now_ms,
            })
            .collect())
    }

    async fn fetch_clients_http(
        &self,
        client: &Client,
        creds: &(String, String),
    ) -> Result<Vec<TrustedClient>, CommandError> {
        let mgmt_port = *self.base_port.lock().await + 1;
        let url = format!("https://127.0.0.1:{mgmt_port}/api/clients/list");
        let resp = client
            .get(&url)
            .basic_auth(&creds.0, Some(&creds.1))
            .send()
            .await
            .map_err(|e| {
                CommandError::new(
                    ErrorCode::InternalError,
                    format!("请求 Sunshine GET /api/clients/list 失败: {e}"),
                )
            })?;

        if !resp.status().is_success() {
            return Err(CommandError::new(
                ErrorCode::Unauthorized,
                format!("获取已授权设备失败 ({})", resp.status()),
            ));
        }

        let body: SunshineClientsResp = resp.json().await.map_err(|e| {
            CommandError::new(
                ErrorCode::InternalError,
                format!("解析 /api/clients/list 失败: {e}"),
            )
        })?;

        let _ = body.status;
        Ok(body
            .named_certs
            .into_iter()
            .map(|c| TrustedClient {
                client_id: c.uuid,
                name: if c.name.is_empty() {
                    "已配对客户端".to_string()
                } else {
                    c.name
                },
                enabled: c.enabled,
            })
            .collect())
    }

    async fn close_active_app_http(
        &self,
        client: &Client,
        creds: &(String, String),
    ) -> Result<(), String> {
        let mgmt_port = *self.base_port.lock().await + 1;
        let url = format!("https://127.0.0.1:{mgmt_port}/api/apps/close");
        let _ = client
            .post(&url)
            .basic_auth(&creds.0, Some(&creds.1))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn ensure_hardened_sunshine_conf(install_dir: &Path, base_port: u16) -> Result<(), String> {
    let conf_path = install_dir.join("config").join("sunshine.conf");
    if let Some(parent) = conf_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 Sunshine 配置目录失败: {e}"))?;
    }

    let existing = std::fs::read_to_string(&conf_path).unwrap_or_default();
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|l| {
            let trimmed = l.trim();
            !trimmed.starts_with("upnp")
                && !trimmed.starts_with("origin_web_ui_allowed")
                && !trimmed.starts_with("port")
        })
        .map(str::to_string)
        .collect();

    lines.push("upnp = disabled".to_string());
    lines.push("origin_web_ui_allowed = pc".to_string());
    lines.push(format!("port = {base_port}"));

    std::fs::write(&conf_path, lines.join("\n") + "\n")
        .map_err(|e| format!("写入 sunshine.conf 失败（可能需要管理员权限）: {e}"))
}

fn find_sunshine_cert_bytes(install_dir: &Path) -> Option<Vec<u8>> {
    let candidates = [
        install_dir.join("config").join("credentials").join("cacert.pem"),
        install_dir.join("config").join("cacert.pem"),
    ];
    for p in candidates {
        if let Ok(bytes) = std::fs::read(&p) {
            return Some(bytes);
        }
    }
    None
}

fn read_sunshine_cert_fingerprint(install_dir: &Path) -> Option<String> {
    let bytes = find_sunshine_cert_bytes(install_dir)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(hex::encode(hasher.finalize()))
}

fn read_sunshine_version(install_dir: &Path) -> Option<String> {
    let ver_file = install_dir.join("version.txt");
    if let Ok(v) = std::fs::read_to_string(ver_file) {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

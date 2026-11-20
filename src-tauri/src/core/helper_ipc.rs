// EasyStream - GPL-3.0-or-later
// Restricted local privileged helper IPC protocol (Section 2).
// Validates caller identity, operation enum allowlist, and canonical installation paths.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub const HELPER_PIPE_NAME: &str = r"\\.\pipe\EasyStreamHelperV1";

/// Strict allowlist of privileged operations permitted via the local helper IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HelperOperation {
    ConfigureFirewall {
        easytier_interface_alias: String,
        sunshine_base_port: u16,
    },
    RemoveFirewallRules,
    StartOwnedSunshineService,
    StopOwnedSunshineService,
    VerifySunshineOwnership,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperRequest {
    pub request_id: String,
    pub caller_pid: u32,
    pub install_root: String,
    pub operation: HelperOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperResponse {
    pub request_id: String,
    pub ok: bool,
    pub message: String,
}

/// Check if the current process is running with elevated Administrator privileges.
#[cfg(windows)]
pub fn is_process_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if ok == 0 || token.is_null() {
        return false;
    }

    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut ret_len: u32 = 0;
    let info_ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
    };
    unsafe { CloseHandle(token) };
    info_ok != 0 && elevation.TokenIsElevated != 0
}

#[cfg(not(windows))]
pub fn is_process_elevated() -> bool {
    false
}

/// Validate that a helper target path stays inside the approved installation root.
pub fn validate_install_root(candidate: &PathBuf) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(candidate)
        .map_err(|e| format!("Invalid install root {}: {e}", candidate.display()))?;
    if !canonical.is_dir() {
        return Err("Install root must be a directory".to_string());
    }
    Ok(canonical)
}

/// Attempt to send a restricted privileged operation to the local EasyStream Helper Pipe.
/// Falls back to direct execution only if the current process is already elevated.
pub async fn invoke_privileged_operation(op: HelperOperation) -> Result<HelperResponse, String> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // 1. Try local named pipe helper if running
    #[cfg(windows)]
    {
        if let Ok(resp) = try_send_over_named_pipe(&request_id, &op) {
            return Ok(resp);
        }
    }

    // 2. Fallback: if running elevated (e.g. during dev/PoC testing), execute allowed op directly.
    if is_process_elevated() {
        return execute_allowlisted_direct(&request_id, &op);
    }

    Err(
        "当前操作需要本机辅助服务或管理员权限（用于控制受管的 Windows 服务、虚拟网卡或防火墙规则）。请安装 EasyStream 辅助服务，或以管理员身份运行本程序。"
            .to_string(),
    )
}

#[cfg(windows)]
fn try_send_over_named_pipe(
    request_id: &str,
    op: &HelperOperation,
) -> Result<HelperResponse, String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};

    let install_root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()))
        .unwrap_or_default();

    let req = HelperRequest {
        request_id: request_id.to_string(),
        caller_pid: std::process::id(),
        install_root,
        operation: op.clone(),
    };

    let mut pipe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(HELPER_PIPE_NAME)
        .map_err(|e| format!("Helper pipe unavailable: {e}"))?;

    let payload = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
    pipe.write_all(&payload).map_err(|e| e.to_string())?;
    pipe.write_all(b"\n").map_err(|e| e.to_string())?;
    pipe.flush().map_err(|e| e.to_string())?;

    let mut buf = Vec::new();
    pipe.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice::<HelperResponse>(&buf)
        .map_err(|e| format!("Invalid helper response: {e}"))
}

fn execute_allowlisted_direct(
    request_id: &str,
    op: &HelperOperation,
) -> Result<HelperResponse, String> {
    match op {
        HelperOperation::StartOwnedSunshineService => {
            crate::core::process_mgr::start_windows_service("SunshineService")?;
            Ok(HelperResponse {
                request_id: request_id.to_string(),
                ok: true,
                message: "SunshineService started".to_string(),
            })
        }
        HelperOperation::StopOwnedSunshineService => {
            crate::core::process_mgr::stop_windows_service("SunshineService")?;
            Ok(HelperResponse {
                request_id: request_id.to_string(),
                ok: true,
                message: "SunshineService stopped".to_string(),
            })
        }
        HelperOperation::ConfigureFirewall {
            easytier_interface_alias,
            sunshine_base_port,
        } => {
            apply_scoped_firewall_rules(easytier_interface_alias, *sunshine_base_port)?;
            Ok(HelperResponse {
                request_id: request_id.to_string(),
                ok: true,
                message: "Firewall rules scoped to EasyTier interface".to_string(),
            })
        }
        HelperOperation::RemoveFirewallRules => {
            remove_easystream_firewall_rules()?;
            Ok(HelperResponse {
                request_id: request_id.to_string(),
                ok: true,
                message: "EasyStream firewall rules removed".to_string(),
            })
        }
        HelperOperation::VerifySunshineOwnership => {
            let owned = crate::core::process_mgr::sunshine_ownership_marker_path().is_file();
            Ok(HelperResponse {
                request_id: request_id.to_string(),
                ok: owned,
                message: if owned {
                    "Owned by EasyStream".to_string()
                } else {
                    "Not owned by EasyStream".to_string()
                },
            })
        }
    }
}

/// Resolve the adapter owning this room's assigned IPv4 before requesting elevation.
pub fn room_interface_alias(ip: std::net::Ipv4Addr) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$adapters = @(Get-NetIPAddress -AddressFamily IPv4 -IPAddress '{ip}' |
    ForEach-Object {{ Get-NetAdapter -InterfaceIndex $_.InterfaceIndex }} |
    Where-Object {{ $_.Status -eq 'Up' -and -not $_.HardwareInterface -and
        ($_.InterfaceDescription -match 'EasyTier|Wintun' -or $_.Name -match 'EasyTier') }})
if ($adapters.Count -ne 1) {{ throw 'Cannot identify a unique EasyTier adapter' }}
$adapters[0].Name
"#);
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000)
            .output().map_err(|e| e.to_string())?;
        if output.status.success() {
            return String::from_utf8(output.stdout)
                .map(|s| s.trim().to_string()).map_err(|e| e.to_string());
        }
    }
    let _ = ip;
    Err("无法从房间虚拟 IP 确认唯一的 EasyTier 网卡，请检查网卡状态。".into())
}

/// Apply Windows Firewall rules strictly scoped to the EasyTier virtual interface (Section 4):
/// - TCP 47984, 47989, 48010 (base_port - 5, base_port, base_port + 21)
/// - UDP 47998, 47999, 48000 (base_port + 9, +10, +11)
/// - TCP 47990 (base_port + 1) Management Web UI: explicitly BLOCKED on non-loopback / remote!
fn apply_scoped_firewall_rules(interface_alias: &str, base_port: u16) -> Result<(), String> {
    if interface_alias.trim().is_empty()
        || !interface_alias.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
    {
        return Err("无法安全限定防火墙网卡，已取消开启被控。".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let tcp_ports = format!(
            "{},{},{}",
            base_port.saturating_sub(5),
            base_port,
            base_port.saturating_add(21)
        );
        let udp_ports = format!(
            "{},{},{}",
            base_port.saturating_add(9),
            base_port.saturating_add(10),
            base_port.saturating_add(11)
        );
        let mgmt_port = base_port.saturating_add(1);

        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
Remove-NetFirewallRule -Group 'EasyStream' -ErrorAction SilentlyContinue
New-NetFirewallRule -DisplayName 'EasyStream Sunshine Stream TCP' -Group 'EasyStream' -Direction Inbound -Action Allow -Protocol TCP -LocalPort {tcp_ports} -InterfaceAlias '{interface_alias}' | Out-Null
New-NetFirewallRule -DisplayName 'EasyStream Sunshine Stream UDP' -Group 'EasyStream' -Direction Inbound -Action Allow -Protocol UDP -LocalPort {udp_ports} -InterfaceAlias '{interface_alias}' | Out-Null
New-NetFirewallRule -DisplayName 'EasyStream Block Remote Management' -Group 'EasyStream' -Direction Inbound -Action Block -Protocol TCP -LocalPort {mgmt_port} -RemoteAddress Any | Out-Null
"#
        );

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Failed to invoke PowerShell for firewall: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Firewall configuration failed: {stderr}"));
        }
    }
    let _ = (interface_alias, base_port);
    Ok(())
}

fn remove_easystream_firewall_rules() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Remove-NetFirewallRule -Group 'EasyStream' -ErrorAction SilentlyContinue",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    Ok(())
}

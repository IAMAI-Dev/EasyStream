// EasyStream - GPL-3.0-or-later
// Moonlight Qt v6.2.0 orchestration: GameStream serverinfo probe, PIN pairing,
// CSV app enumeration, discrete-argv stream launch, and process monitoring.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use rand::Rng;
use regex::Regex;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::core::process_mgr::ProcessManager;
use crate::state::{CommandError, ErrorCode, StreamPreset};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct HostProbeResult {
    pub reachable: bool,
    pub unique_id: Option<String>,
    pub paired: bool,
    pub busy: bool,
    pub hostname: Option<String>,
}

pub struct MoonlightManager {
    process_mgr: Arc<ProcessManager>,
}

impl MoonlightManager {
    pub fn new(process_mgr: Arc<ProcessManager>) -> Self {
        Self { process_mgr }
    }

    pub fn resolve_binary(&self) -> Option<PathBuf> {
        self.process_mgr
            .resolve_component_binary("moonlight", "Moonlight.exe")
    }

    /// Generate a 4-digit pairing PIN preserving leading zeros (Section 4 step 1).
    pub fn generate_pairing_pin() -> String {
        let num = rand::rngs::OsRng.gen_range(0..10000u16);
        format!("{num:04}")
    }

    /// Probe target host via standard GameStream HTTP /serverinfo (base port 47989).
    /// Returns reachability, host uniqueid (for identity verification), PairStatus, and busy state.
    pub async fn probe_host_serverinfo(
        &self,
        host_ip: &str,
        base_port: u16,
    ) -> HostProbeResult {
        let url = format!("http://{host_ip}:{base_port}/serverinfo");
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(1600))
            .no_proxy()
            .build()
        {
            Ok(c) => c,
            Err(_) => {
                return HostProbeResult {
                    reachable: false,
                    unique_id: None,
                    paired: false,
                    busy: false,
                    hostname: None,
                }
            }
        };

        let resp = match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => {
                return HostProbeResult {
                    reachable: false,
                    unique_id: None,
                    paired: false,
                    busy: false,
                    hostname: None,
                }
            }
        };

        let body = resp.text().await.unwrap_or_default();
        let unique_id = extract_xml_tag(&body, "uniqueid");
        let pair_status = extract_xml_tag(&body, "PairStatus")
            .map(|s| s == "1")
            .unwrap_or(false);
        let current_game = extract_xml_tag(&body, "currentgame")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let state = extract_xml_tag(&body, "state").unwrap_or_default();
        let hostname = extract_xml_tag(&body, "hostname");

        let busy = current_game != 0 || state.contains("SUNSHINE_SERVER_BUSY");

        HostProbeResult {
            reachable: true,
            unique_id,
            paired: pair_status,
            busy,
            hostname,
        }
    }

    /// Spawn `Moonlight.exe pair <host_ip> --pin <pin>` with discrete argv and sensitive_args=true.
    /// Returns the owned child PID so the session coordinator can wait or cancel it.
    pub fn start_pairing(&self, host_ip: &str, pin: &str) -> Result<u32, CommandError> {
        let bin = self.resolve_binary().ok_or_else(|| {
            CommandError::new(
                ErrorCode::ComponentMissing,
                "未找到 Moonlight.exe（resources/moonlight/Moonlight.exe），请先运行 scripts/fetch-upstream.ps1。",
            )
        })?;
        let work_dir = bin.parent();

        let args = vec![
            "pair".to_string(),
            host_ip.to_string(),
            "--pin".to_string(),
            pin.to_string(),
        ];

        self.process_mgr
            .spawn_owned("moonlight-pair", &bin, &args, work_dir, &[], true, true)
            .map_err(|e| {
                CommandError::new(ErrorCode::InternalError, "启动 Moonlight 配对进程失败")
                    .with_detail(e)
            })
    }

    /// Run `Moonlight.exe list <host_ip> --csv` to enumerate published applications.
    pub async fn list_apps(&self, host_ip: &str) -> Result<Vec<String>, CommandError> {
        let bin = self.resolve_binary().ok_or_else(|| {
            CommandError::new(
                ErrorCode::ComponentMissing,
                "未找到 Moonlight.exe，无法获取主机应用列表。",
            )
        })?;
        let work_dir = bin.parent().map(PathBuf::from);
        let ip = host_ip.to_string();

        let output = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&bin);
            cmd.args(["list", &ip, "--csv"]);
            if let Some(dir) = work_dir {
                cmd.current_dir(dir);
            }
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.output()
        })
        .await
        .map_err(|e| CommandError::new(ErrorCode::InternalError, e.to_string()))?
        .map_err(|e| {
            CommandError::new(
                ErrorCode::InternalError,
                format!("执行 Moonlight list 失败: {e}"),
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            let combined = format!("{stdout}\n{stderr}");
            if combined.to_lowercase().contains("pair")
                || combined.to_lowercase().contains("certificate")
            {
                return Err(CommandError::new(
                    ErrorCode::Unauthorized,
                    "设备尚未完成配对或主机配对凭证已失效，请重新配对。",
                ));
            }
            return Err(CommandError::new(
                ErrorCode::AppMissing,
                format!("获取主机应用列表失败: {}", stderr.trim()),
            ));
        }

        let apps = parse_moonlight_csv_apps(&stdout);
        if apps.is_empty() {
            return Err(CommandError::new(
                ErrorCode::AppMissing,
                "主机未发布任何可串流应用（至少需要 Desktop）。",
            ));
        }
        Ok(apps)
    }

    /// Launch `Moonlight.exe stream <host_ip> <app> --resolution WxH --fps F --bitrate K --display-mode M --video-codec C`
    /// as a visible native Moonlight player window bound to EasyStream's Job Object.
    pub fn start_stream(
        &self,
        host_ip: &str,
        app_name: &str,
        preset: &StreamPreset,
    ) -> Result<u32, CommandError> {
        let bin = self.resolve_binary().ok_or_else(|| {
            CommandError::new(
                ErrorCode::ComponentMissing,
                "未找到 Moonlight.exe，无法启动串流播放器。",
            )
        })?;
        let work_dir = bin.parent();

        let resolution = format!("{}x{}", preset.width, preset.height);
        let fps = preset.fps.to_string();
        let bitrate = preset.bitrate_kbps.to_string();

        let args = vec![
            "stream".to_string(),
            host_ip.to_string(),
            app_name.to_string(),
            "--resolution".to_string(),
            resolution,
            "--fps".to_string(),
            fps,
            "--bitrate".to_string(),
            bitrate,
            "--display-mode".to_string(),
            preset.display_mode.clone(),
            "--video-codec".to_string(),
            preset.codec.clone(),
        ];

        self.process_mgr
            .spawn_owned("moonlight-stream", &bin, &args, work_dir, &[], false, false)
            .map_err(|e| {
                CommandError::new(ErrorCode::InternalError, "启动 Moonlight 播放器失败")
                    .with_detail(e)
            })
    }

    /// Send `Moonlight.exe quit <host_ip>` to cleanly end a remote running app if needed.
    pub async fn quit_remote_app(&self, host_ip: &str) {
        let Some(bin) = self.resolve_binary() else {
            return;
        };
        let work_dir = bin.parent().map(PathBuf::from);
        let ip = host_ip.to_string();

        let _ = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&bin);
            cmd.args(["quit", &ip]);
            if let Some(dir) = work_dir {
                cmd.current_dir(dir);
            }
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.output();
        })
        .await;
    }
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"<{tag}>(.*?)</{tag}>");
    let re = Regex::new(&pattern).ok()?;
    re.captures(xml)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Parse `Moonlight.exe list <host> --csv` output lines.
/// Moonlight `--csv` prints lines like `"Desktop",12345` or `Desktop` depending on verbose flag.
pub fn parse_moonlight_csv_apps(stdout: &str) -> Vec<String> {
    let mut apps = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip header if present
        if trimmed.eq_ignore_ascii_case("Name,ID") || trimmed.eq_ignore_ascii_case("\"Name\",\"ID\"") {
            continue;
        }
        let name = if let Some(rest) = trimmed.strip_prefix('"') {
            rest.split('"').next().unwrap_or("").trim().to_string()
        } else {
            trimmed.split(',').next().unwrap_or("").trim().to_string()
        };
        if !name.is_empty() && !apps.contains(&name) {
            apps.push(name);
        }
    }
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_4_digit_pin_with_leading_zeros() {
        for _ in 0..50 {
            let pin = MoonlightManager::generate_pairing_pin();
            assert_eq!(pin.len(), 4);
            assert!(pin.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn parses_csv_app_list() {
        let sample = "\"Desktop\",1001\n\"Steam Big Picture\",1002\n";
        let apps = parse_moonlight_csv_apps(sample);
        assert_eq!(apps, vec!["Desktop", "Steam Big Picture"]);
    }
}

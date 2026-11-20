// EasyStream - GPL-3.0-or-later
// Active display output enumeration (Win32 QueryDisplayConfig) and Phase-2
// optional signed virtual display driver detection & safe topology restore (Section 5).

use crate::state::{DisplayOutputInfo, VirtualDisplayState};

/// Query active capturable display paths via Windows QueryDisplayConfig API.
#[cfg(windows)]
pub fn enumerate_active_displays() -> Vec<DisplayOutputInfo> {
    use windows_sys::Win32::Devices::Display::{
        GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO, QDC_ONLY_ACTIVE_PATHS,
    };

    let mut path_count: u32 = 0;
    let mut mode_count: u32 = 0;
    let res = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if res != 0 || path_count == 0 {
        return Vec::new();
    }

    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> =
        vec![unsafe { std::mem::zeroed() }; path_count as usize];
    let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> =
        vec![unsafe { std::mem::zeroed() }; mode_count as usize];

    let query_res = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    };
    if query_res != 0 {
        return Vec::new();
    }

    let mut outputs = Vec::new();
    for (idx, path) in paths.iter().take(path_count as usize).enumerate() {
        let mode_idx = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
        let (width, height, is_primary) = if mode_idx < mode_count as usize
            && modes[mode_idx].infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
        {
            let src = unsafe { modes[mode_idx].Anonymous.sourceMode };
            (
                src.width,
                src.height,
                src.position.x == 0 && src.position.y == 0,
            )
        } else {
            (0, 0, idx == 0)
        };

        let refresh_hz = if path.targetInfo.refreshRate.Denominator > 0 {
            (path.targetInfo.refreshRate.Numerator as f64
                / path.targetInfo.refreshRate.Denominator as f64)
                .round() as u32
        } else {
            60
        };

        // DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED (16) or INDIRECT_VIRTUAL (17)
        let tech = path.targetInfo.outputTechnology;
        let is_virtual = tech == 16 || tech == 17;

        outputs.push(DisplayOutputInfo {
            id: format!("display-{}-{}", path.sourceInfo.id, path.targetInfo.id),
            name: if is_virtual {
                format!("虚拟显示器 #{}", idx + 1)
            } else {
                format!("显示器 #{}", idx + 1)
            },
            width,
            height,
            refresh_hz,
            is_primary,
            is_virtual,
        });
    }

    outputs
}

#[cfg(not(windows))]
pub fn enumerate_active_displays() -> Vec<DisplayOutputInfo> {
    Vec::new()
}

/// Detect whether any supported, signed Indirect Display Driver (virtual display) is installed.
/// EasyStream never bundles unsigned Microsoft IddSample drivers (Section 5).
pub fn inspect_virtual_display_state(displays: &[DisplayOutputInfo]) -> VirtualDisplayState {
    let has_physical = displays.iter().any(|d| !d.is_virtual && d.width > 0);
    let virtual_active = displays.iter().any(|d| d.is_virtual && d.width > 0);
    let detected_driver = detect_signed_vdd_driver();

    let guidance = if has_physical {
        "已检测到可捕获的物理显示器，首版推荐直接使用物理屏串流。".to_string()
    } else if virtual_active {
        "当前无物理显示器，已通过虚拟显示器提供可捕获画面源。".to_string()
    } else if let Some(ref drv) = detected_driver {
        format!("当前无可捕获物理显示器，已检测到已签名虚拟显示驱动（{drv}），可在第二阶段按需启用 1080p60 虚拟输出。")
    } else {
        "未检测到活动物理显示器，也未检测到受支持的已签名虚拟显示驱动。请连接显示器/HDMI欺骗器，或单独安装受支持的已签名虚拟显示驱动（如 Virtual Display Driver / SudoVDA）。".to_string()
    };

    VirtualDisplayState {
        supported_driver_detected: detected_driver.is_some(),
        driver_name: detected_driver,
        virtual_output_active: virtual_active,
        managed_by_easystream: false,
        has_physical_display: has_physical,
        guidance,
    }
}

#[cfg(windows)]
fn detect_signed_vdd_driver() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let known = [
        "Virtual Display Driver",
        "Parsec Virtual Display",
        "SudoMaker Virtual Display",
        "MttVDD",
    ];
    for line in stdout.lines() {
        let trimmed = line.trim();
        for k in &known {
            if trimmed.to_lowercase().contains(&k.to_lowercase()) {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn detect_signed_vdd_driver() -> Option<String> {
    None
}

# EasyStream Installer & Privilege Architecture

## Packaging Targets
Tauri v2 (`src-tauri/tauri.conf.json`) builds Windows x64 `.msi` (WiX) and `.exe` (NSIS) installers.

## Component Layout in Installed Root
- `easystream.exe` — Main Tauri v2 desktop application (runs as the logged-in desktop user).
- `resources/easytier/` — `easytier-core.exe`, `easytier-cli.exe`, `wintun.dll` (`v2.6.4`, LGPL-3.0).
- `resources/moonlight/` — Official `Moonlight Qt v6.2.0` portable directory with Qt plugins & DLLs (GPL-3.0).
- `resources/sunshine/` — Official `Sunshine v2026.914.233613` x64 MSI installer (GPL-3.0), installed on demand when the user enables Host mode.
- `resources/manifest.json` — Pinned version, asset filename, architecture, and SHA-256 records.

## Restricted Local Privilege Model
- WebView and `Moonlight.exe` always run under the logged-in user token.
- Operations requiring elevation (starting/stopping `SunshineService`, configuring EasyTier-scoped Windows Firewall rules, on-demand MSI install) are executed via `core/helper_ipc.rs` using a strict `HelperOperation` allowlist over `\\.\pipe\EasyStreamHelperV1` (or direct elevated token during PoC/dev testing).
- When Sunshine is installed by EasyStream, an ownership marker is written to `%ProgramData%\EasyStream\sunshine-owner.json`. If an unowned Sunshine installation already exists, EasyStream refuses to overwrite its config and allows controller-only operation.

## Upgrade & Uninstall Expectations
- Upgrading EasyStream preserves `%LOCALAPPDATA%\EasyStream\settings.json`, Windows Credential Manager secrets, and Sunshine pairing state.
- Uninstalling EasyStream removes `EasyStream` firewall rule group (`Remove-NetFirewallRule -Group 'EasyStream'`) and stops any EasyStream-owned processes.

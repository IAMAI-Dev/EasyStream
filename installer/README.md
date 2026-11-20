# EasyStream Installer & Privilege Architecture

## Packaging Targets
Tauri v2 builds one per-machine Windows x64 NSIS `.exe`. The installer includes the WebView2 Evergreen offline installer. `pnpm build:windows` stages the pinned upstream components before bundling.

## Component Layout in Installed Root
- `easystream.exe` — Main Tauri v2 desktop application (runs as the logged-in desktop user).
- `resources/easytier/` — `easytier-core.exe`, `easytier-cli.exe`, `wintun.dll`, `Packet.dll` and `WinDivert64.sys` from the pinned Windows x64 release.
- `resources/moonlight/` — Official `Moonlight Qt v6.2.0` portable directory with Qt plugins & DLLs (GPL-3.0).
- `resources/sunshine/` — Official `Sunshine v2026.914.233613` x64 MSI installer (GPL-3.0), installed on demand when the user enables Host mode.
- `resources/manifest.json` — Pinned version, asset filename, architecture, and SHA-256 records.

## Restricted Local Privilege Model
- The packaged application requests elevation at launch for the current host-service and EasyTier TUN operations. Developer builds can be run from an elevated terminal for integration testing. The privileged helper protocol remains available for a future split into an unprivileged UI and a privileged process.
- First use of **Allow remote control** verifies the bundled MSI and installs Sunshine locally. EasyStream configures the owned service for on-demand startup and scopes its media rules to the EasyTier adapter.
- When Sunshine is installed by EasyStream, an ownership marker is written to `%ProgramData%\EasyStream\sunshine-owner.json`. If an unowned Sunshine installation already exists, EasyStream refuses to overwrite its config and allows controller-only operation.

## Upgrade & Uninstall Expectations
- Upgrading EasyStream preserves `%LOCALAPPDATA%\EasyStream\settings.json`, Windows Credential Manager secrets, and Sunshine pairing state.
- Uninstalling EasyStream removes `EasyStream` firewall rule group (`Remove-NetFirewallRule -Group 'EasyStream'`) and stops any EasyStream-owned processes.

# EasyStream Sidecar Binaries

Run `.\scripts\fetch-upstream.ps1` from the repository root to download and verify the pinned upstream Windows x64 binaries:

- `easytier-core-x86_64-pc-windows-msvc.exe` (or `resources/easytier/easytier-core.exe`)
- `easytier-cli-x86_64-pc-windows-msvc.exe` (or `resources/easytier/easytier-cli.exe`)
- `resources/moonlight/Moonlight.exe` (full official portable Qt directory with plugins & DLLs)
- `resources/sunshine/` (official Sunshine x64 MSI installer for on-demand host installation)

When building a bundled Tauri release with `externalBin`, target-triple suffixed binaries in this directory are picked up automatically. During local development or portable runs, EasyStream searches in order:
1. Application resource directory (`resources/easytier/`, `resources/moonlight/`)
2. Sidecar binaries directory (`binaries/`)
3. ProgramData / LocalAppData managed component directory (`%LOCALAPPDATA%\EasyStream\components\`)
4. System PATH / standard install paths (e.g. `C:\Program Files\Sunshine\`)

# Third-Party Notices & Upstream Component Provenance

EasyStream itself is licensed under the **GNU General Public License v3.0 (GPLv3)** (see [LICENSE](./LICENSE)).

EasyStream orchestrates and aggregates the following upstream open-source components as separate executables/installers without altering their streaming protocols. Binary releases of EasyStream MUST retain all upstream copyrights, licenses, SHA-256 verification records, and corresponding source code download references.

---

## Pinned Candidate Versions (PoC / v0.1 Baseline)

| Component | Version / Tag | Architecture | License | Upstream Repository & Source |
| --- | --- | --- | --- | --- |
| **EasyTier** (`easytier-core`, `easytier-cli`) | `v2.6.4` | Windows x64 (`x86_64`) | LGPL-3.0 | https://github.com/EasyTier/EasyTier/releases/tag/v2.6.4 |
| **Sunshine** (Host Service MSI) | `v2026.914.233613` | Windows x64 (`x86_64`) | GPL-3.0 | https://github.com/SunshineStream/Sunshine/releases/tag/v2026.914.233613 |
| **Moonlight Qt** (Portable Client) | `v6.2.0` | Windows x64 (`x86_64`) | GPL-3.0 | https://github.com/moonlight-stream/moonlight-qt/releases/tag/v6.2.0 |

---

## Corresponding Source Offer (GPLv3 / LGPL-3.0)

In accordance with Sections 6 of GPLv3 and 4 of LGPL-3.0:
1. Full source code for EasyStream, including all build scripts (`scripts/`, `installer/`, `src/`, `src-tauri/`), is distributed in this repository.
2. Unmodified upstream source tarballs for the exact pinned tags listed above are available from their official GitHub releases and archived alongside any binary release of EasyStream.
3. Exact asset filenames and SHA-256 checksums are recorded in `src-tauri/resources/manifest.json` during component staging via `scripts/fetch-upstream.ps1`.

---

## Runtime & Driver Redistribution Notes

- **Qt / FFmpeg / SDL / OpenSSL**: Redistributed strictly as part of the official Moonlight Qt `v6.2.0` portable bundle under their respective licenses (LGPL/GPL/Apache/MIT).
- **Wintun / Virtual Network Adapter**: Provided by EasyTier (`v2.6.4`) under its upstream distribution terms.
- **Virtual Display Driver (Phase 2 / Headless)**: EasyStream does NOT bundle unsigned or sample drivers (such as Microsoft IddSample). Users who require headless operation select a separately installed, WHQL/EV-signed virtual display driver.
- **Sunshine Virtual HID Driver**: Treated as an optional upstream component per Sunshine's Windows installation documentation. Baseline keyboard and mouse streaming is verified independently.

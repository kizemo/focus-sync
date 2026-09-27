# Focus Sync — Sigma File Manager Extension

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build](https://github.com/kizemo/focus-sync/actions/workflows/build.yml/badge.svg)](https://github.com/kizemo/focus-sync/actions)

Listary-style global focus sync for [Sigma File Manager](https://github.com/aleksey-hoffman/sigma-file-manager).

When you Alt-Tab from any Windows app's file dialog (Chrome downloads, Word "Save As", VS Code "Open Folder", 钉钉 / 飞书 / 微信 send-file flows) to Sigma and browse to a new folder, the dialog auto-jumps to that new folder on Alt-Tab back.

## How it works

```
┌─────────────────────────────┐    onPathChange     ┌───────────────────────┐
│   Sigma File Manager        │ ──────────────────► │ focus-sync-sidecar.exe│
│   ├─ extension: focus-sync  │   POST /set_path   │ (Windows UI Automation)│
│   └─ spawns sidecar + pushes│                     └───────────────────────┘
│      current folder         │                              │
└─────────────────────────────┘                              │ UIA
                                                              ▼
                                            ┌──────────────────────────┐
                                            │ Chrome / Word / VS Code / │
                                            │ 钉钉 / 飞书 / 微信 ...    │
                                            └──────────────────────────┘
```

The Sigma FM extension listens to `sigma.context.onPathChange`, and on each change POSTs the new folder to `focus-sync-sidecar.exe` (a local HTTP server). The sidecar uses Windows UI Automation to detect the foreground file dialog and write the path to its filename input.

## Architecture

- **Monorepo**:
  - `extension/` — Sigma FM extension (TypeScript, ES module, bundled to `dist/index.js`)
  - `sidecar/` — Windows UI Automation daemon (Rust, MIT-licensed port from PathWrap + Apache-2.0 fg_bypass from QwenLM)
  - `.github/workflows/` — CI build + Release
- **Distribution**: GitHub Releases host `focus-sync-sidecar-windows-x64.zip` + `package.json`. Sigma FM's host downloads + verifies SHA256 integrity from the manifest.

### Why two artifacts (extension + sidecar)?

- Sigma FM extensions are TypeScript running in a sandboxed webview. They **cannot** call Windows COM APIs directly.
- Windows UI Automation needs COM STA + UIAutomationCore — only a native Win32 process can do that.
- So the extension orchestrates (events, commands, lifecycle) and the sidecar handles the actual HWND / ValuePattern work via HTTP IPC.
- See `docs/spec.md` for the full architecture, including the design trade-offs.

## Install

### From GitHub Release (recommended)

1. Open Sigma File Manager (>= v2.2.0)
2. Extensions → "Add from URL" → paste: `https://github.com/kizemo/focus-sync/releases/latest/download/package.json`
3. Sigma auto-downloads `focus-sync-sidecar.exe`, verifies SHA256, installs to `%APPDATA%\com.sigma-file-manager.app\binaries\focus-sync-sidecar\0.2.0\`
4. Restart Sigma — toolbar shows "Focus Sync" dropdown

### From marketplace (when published)

Sigma FM's [sfm-marketplace](https://github.com/sigma-hub/sfm-marketplace/wiki) will list this extension with name `Focus Sync`.

### From source (dev)

```bash
git clone https://github.com/kizemo/focus-sync
cd focus-sync
# Build sidecar binary
cd sidecar && cargo build --release && cd ..
# Build extension bundle
cd extension && npm install && npm run build && cd ..
# Symlink into Sigma FM's extensions dir (Windows)
# %APPDATA%\com.sigma-file-manager.app\extensions\link\kizemo.focus-sync → ../focus-sync/extension
```

## Usage

| Action | Result |
|---|---|
| Open any file dialog in any Windows app | Sidecar detects, reads current path, registers dialog |
| In Sigma, navigate to a new folder | Extension pushes new path via HTTP `POST /set_path` → sidecar writes to all registered dialogs |
| Toolbar → Focus Sync → "Sync Now" | Manually re-push current path (useful after navigating in another app) |
| Toolbar → Focus Sync → "Enable / Disable" | Toggle the feature on/off (sidecar stays running; setting persists) |

## Limitations

- **Windows only** — UI Automation is Windows-specific. macOS / Linux are out of scope.
- **Chromium-class hosts** (Chrome / Edge / Brave / etc.) — covered by `EnableWindow(FALSE)` RAII guard ported from QwenLM (measured 0/507 z-drops on Chromium dialogs).
- **UWP / XAML-class hosts** — currently NOT covered (Phase 2c). Will work for `钉钉 / 飞书 / 微信` which are NOT UWP, but NOT for Excel / Photos / OneDrive dialogs.
- **Manual verification recommended** per target app before production use (no headless GUI test possible from CI).

## Source attribution

`sidecar/` source is a port from two open-source projects:

| File | Origin | License |
|---|---|---|
| `sidecar/src/uia_inject.rs` | [`inaku-Gyan/PathWrap/src/os/dialog.rs`](https://github.com/inaku-Gyan/PathWrap) | MIT |
| `sidecar/src/uia_event.rs` | [`inaku-Gyan/PathWrap/src/os/monitor.rs`](https://github.com/inaku-Gyan/PathWrap) | MIT |
| `sidecar/src/fg_bypass.rs` | [`QwenLM/qwen-code/.../fg_bypass.rs`](https://github.com/QwenLM/qwen-code) | Apache-2.0 |

License headers preserved at the top of each ported file. See `sidecar/README.md`.

## Build & release

- **CI**: `.github/workflows/build.yml` — builds `focus-sync-sidecar.exe`, bundles extension, runs HTTP smoke test on every push.
- **Release**: `.github/workflows/release.yml` — on `v*.*.*` tag push: builds, zips, computes SHA256, updates manifest integrity, creates GitHub Release with `focus-sync-sidecar-windows-x64.zip` + `package.json` attached.

To cut a release locally:

```bash
git tag v0.2.0 && git push origin v0.2.0
# GitHub Actions builds + publishes automatically
```

## License

This project: MIT.

Sigma File Manager integration: respects [GPL-3.0-or-later](https://github.com/aleksey-hoffman/sigma-file-manager/blob/main/LICENSE.md) of the host.

Sidecar source: MIT (with Apache-2.0 for `fg_bypass.rs`).
# Sigma File Manager + Focus Sync — Custom Installer

This bundle packages the kizemo fork of Sigma File Manager (`v2.2.0`) **with the kizemo/focus-sync extension pre-installed**, including the `focus-sync-sidecar.exe` UI Automation binary. The installer is a standard Tauri NSIS bundle (same packaging upstream aleksey-hoffman uses); the extension is deployed via a custom NSIS `POSTINSTALL` hook.

After running the installer, the Focus Sync plugin is already registered in Sigma FM. You just open the app, go to **Extensions**, find **Focus Sync**, and click **Enable** — no manual file copying, no downloads, fully offline-ready.

## What's inside

```
release/
├── Sigma-File-Manager-2.2.0-focus-1-setup.exe    ← the installer (17 MB)
├── installer/
│   └── Sigma File Manager_2.2.0_x64-setup.exe     ← Tauri NSIS (identical copy)
├── extension/
│   ├── package.json                               ← extension manifest
│   ├── dist/index.js                              ← bundled TS
│   ├── locales/{en,zh-CN}.json                    ← i18n
│   └── bin/focus-sync-sidecar.exe                 ← sidecar binary (2.9 MiB)
└── README.md                                      ← this file
```

## How to use

### Just install

1. Run `Sigma-File-Manager-2.2.0-focus-1-setup.exe`
2. Confirm the UAC prompt (`perMachine` install requires admin; lands in `%PROGRAMFILES%\Sigma File Manager\`)
3. **No need to uninstall the previous version** — the installer auto-detects the existing install path (registry or filesystem probe) and reuses the same folder. Tauri NSIS overwrites in place.
4. The installer:
   - Writes Sigma File Manager to `$INSTDIR\` (default: `%PROGRAMFILES%\Sigma File Manager\`)
   - Writes the focus-sync extension to `$INSTDIR\extensions\kizemo.focus-sync\`
   - Writes `focus-sync-sidecar.exe` to `$INSTDIR\bin\focus-sync-sidecar\0.2.0\`
   - `POSTINSTALL` hook runtime-copies these to `%APPDATA%\com.sigma-file-manager.app\extensions\` and `...\binaries\`
5. **Launch Sigma File Manager**
6. **Extensions** menu → find **Focus Sync** → click **Enable**
7. The toolbar shows a new **Focus Sync** dropdown:
   - **Sync Now** — push Sigma's current folder to all open file dialogs
   - **Enable / Disable** — toggle (sidecar stays running)

### What to test

- Open **Save As** in Word / Chrome / VS Code
- In Sigma, browse to a new folder
- Alt-Tab back to the dialog — it should jump to the new folder

See `meta-repo/docs/superpowers/test-plans/2026-09-27-listary-focus-sync-manual-checklist.md` for the full 7-app GUI verification matrix.

## How it works (technical)

### NSIS `installerHooks` — what Tauri 2 calls them

Tauri 2.x NSIS bundling supports a custom `installerHooks` file (referenced from `tauri.conf.json`'s `bundle.windows.nsis.installerHooks`) that defines four macros called from the installer's section flow:

| Macro | When it fires | What we use it for |
|---|---|---|
| `NSIS_HOOK_PREINSTALL` | Before file copy | Detect existing Sigma FM install (registry or filesystem) and override `$INSTDIR` |
| `NSIS_HOOK_POSTINSTALL` | After file copy | Copy extension files from `$INSTDIR` to `%APPDATA%` |
| `NSIS_HOOK_PREUNINSTALL` | Before file removal | (no-op; Tauri already kills the running app) |
| `NSIS_HOOK_POSTUNINSTALL` | After file removal | Clean up extension + sidecar directories in `%APPDATA%` |

The hooks file lives at `sigma-file-manager/src-tauri/installer/hooks.nsh`.

### Smart-upgrade: detecting the old install path

The `PREINSTALL` hook tries three sources in order:

1. **Tauri registry key**: `HKLM\SOFTWARE\[WOW6432Node\]Microsoft\Windows\CurrentVersion\Uninstall\com.sigma-file-manager.app_is1\InstallLocation` (this fork)
2. **Legacy makensis registry key**: `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Sigma File Manager_is1\InstallLocation` (upstream AppName)
3. **Filesystem probe**: `%PROGRAMFILES%\Sigma File Manager\Sigma File Manager\Sigma File Manager.exe` → nested layout → `%PROGRAMFILES%\Sigma File Manager\Sigma File Manager.exe` → flat layout → `%PROGRAMFILES%\Sigma File Manager.exe` → root install

The first match wins. If none match, the installer falls back to Tauri's default `$PROGRAMFILES\Sigma File Manager\`.

### Runtime file copy (POSTINSTALL)

NSIS's `File` command reads source files at **compile time** and embeds them in the installer's payload — it can't reference runtime variables like `$INSTDIR`. To copy files *from* `$INSTDIR` *to* `%APPDATA%`, the hook uses `CopyFiles /SILENT` which is a runtime operation.

### Why pre-deploy the sidecar at the shared path?

Sigma FM's extension runtime, on first enable, calls:

```ts
const existingSharedPath = await invoke<string | null>('get_shared_binary_path', {
  binaryId: 'focus-sync-sidecar',
  version: '0.2.0',
});
```

If a file exists at `%APPDATA%\com.sigma-file-manager.app\binaries\focus-sync-sidecar\0.2.0\focus-sync-sidecar.exe`, Sigma FM **reuses it without re-downloading**. So pre-deploying at this path gives offline support for free.

### Extension manifest

The `extension/package.json` declares:

```json
"binaries": [{
  "id": "focus-sync-sidecar",
  "version": "0.2.0",
  "assets": [{
    "platform": "windows",
    "downloadUrl": "https://github.com/kizemo/focus-sync/releases/download/v0.2.0/focus-sync-sidecar-windows-x64.zip",
    "integrity": "sha256:...",
    ...
  }]
}]
```

Sigma FM uses this to (a) know what binaries the extension needs, (b) fall back to download if pre-deploy is missing, (c) verify integrity of downloads.

## Reproducing this build

```bash
# From meta-repo root.

# 1. Stage focus-sync extension (manifest + dist + locales) under release/extension/.
mkdir -p release/extension/{dist,locales,bin}
cp "../focus-sync/extension/package.json" release/extension/
cp "../focus-sync/extension/dist/index.js" release/extension/dist/
cp "../focus-sync/extension/dist/index.js.map" release/extension/dist/
cp "../focus-sync/extension/locales/"*.json release/extension/locales/

# 2. Stage focus-sync-sidecar.exe from focus-sync v0.2.0 release.
gh release download v0.2.0 --pattern 'focus-sync-sidecar-windows-x64.zip' -D /tmp/sidecar-dl --repo kizemo/focus-sync
unzip -o /tmp/sidecar-dl/focus-sync-sidecar-windows-x64.zip -d release/extension/bin/

# 3. Build Tauri NSIS (rustc 1.90 + Cargo via rsproxy.cn mirror — see handoff-2026-09-29-build-success.md).
cd sigma-file-manager
export PATH="/c/Users/Duanyi/.rustup/toolchains/1.90.0-x86_64-pc-windows-msvc/bin:$PATH"
export RUSTUP_DIST_SERVER=https://rsproxy.cn RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
npm install
npx tauri build --bundles nsis

# 4. Stage installer into release/.
cd ..
cp "sigma-file-manager/src-tauri/target/release/bundle/nsis/Sigma File Manager_2.2.0_x64-setup.exe" release/installer/
cp "sigma-file-manager/src-tauri/target/release/bundle/nsis/Sigma File Manager_2.2.0_x64-setup.exe" release/Sigma-File-Manager-2.2.0-focus-1-setup.exe
```

## Limitations

- **Windows only** — Tauri NSIS installer + UIA spike.
- **No auto-update channel** — when focus-sync v0.2.0 ships, the user has to re-run this installer to upgrade (or use the GitHub release of focus-sync directly + Sigma FM's `Add from URL`).
- **Single Sigma FM fork pin** — this bundles the specific build at `sigma-file-manager/src-tauri/target/release/bundle/nsis/Sigma File Manager_2.2.0_x64-setup.exe`. Rebuilds require a fresh inner installer.
- **Admin required** — `installMode: perMachine` shows a UAC prompt; the installer lands in `%PROGRAMFILES%`.

## Where things come from

| Component | Source | Version |
|---|---|---|
| Sigma FM NSIS | `kizemo/sigma-file-manager` fork (built `src-tauri/target/release/bundle/nsis/`) | 2.2.0 (recovery branch) |
| Installer hooks (NSH) | `sigma-file-manager/src-tauri/installer/hooks.nsh` | (this commit) |
| Extension code | `kizemo/focus-sync` | 0.2.0 |
| Extension dist | Built from `focus-sync/extension/` via `npm run build` | 0.2.0 |
| focus-sync-sidecar.exe | `kizemo/focus-sync/releases/download/v0.2.0/focus-sync-sidecar-windows-x64.zip` | 0.2.0 |

## License

- Sigma FM fork: GPL-3.0-or-later (inherited from upstream)
- focus-sync extension: MIT
- focus-sync-sidecar.exe: MIT (with Apache-2.0 for `fg_bypass.rs`)

See `focus-sync/LICENSE` for full attribution.

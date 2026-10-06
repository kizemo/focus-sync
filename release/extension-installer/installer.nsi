; ============================================================================
; kizemo.focus-sync 0.2.0 — Standalone NSIS Installer
; ============================================================================
; This installer is a one-click solution to register the Focus Sync extension
; with Sigma File Manager. It:
;   1. Copies extension files to %APPDATA%\com.sigma-file-manager.app\
;   2. Registers the extension in user-extensions.json (the storage Sigma FM
;      reads from at startup to populate its "已安装" tab).
;
; IMPORTANT — Before running this installer:
;   - CLOSE Sigma File Manager completely (right-click taskbar icon → Quit).
;   - Sigma FM uses a one-shot startup bootstrap. If it's running, its
;     in-memory state has stale data and will overwrite user-extensions.json
;     on its next save, undoing this install.
;
; After install completes:
;   - Open Sigma File Manager
;   - Ctrl+Shift+X → Installed tab → Focus Sync should be listed
;   - Toggle it on; toolbar should show Sync Now / Enable-Disable items
; ============================================================================

Unicode true
SetCompressor /FINAL /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"

; ---------------------------------------------------------------------------
; Metadata
; ---------------------------------------------------------------------------
!define PRODUCT_NAME      "Focus Sync Extension"
!define PRODUCT_VERSION   "0.2.0"
!define PRODUCT_PUBLISHER  "kizemo"
!define PRODUCT_ID         "kizemo.focus-sync"
!define PRODUCT_EXTDIR     "kizemo.focus-sync"
!define PRODUCT_SIDECAR    "focus-sync-sidecar"
!define PRODUCT_SIDECAR_VER "0.2.0"
!define SIGMA_APP_DIR      "com.sigma-file-manager.app"
!define SIGMA_USERDATA     "$APPDATA\${SIGMA_APP_DIR}"

; ---------------------------------------------------------------------------
; Custom vars (must be declared before first use)
; ---------------------------------------------------------------------------
Var SIGMAWASRUNNING

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "kizemo.focus-sync-${PRODUCT_VERSION}-setup.exe"

; Installer metadata lives in a stable local dir (not %APPDATA% to avoid
; colliding with Sigma FM's own state).
InstallDir "$LOCALAPPDATA\Programs\kizemo.focus-sync"
InstallDirRegKey HKCU "Software\${PRODUCT_PUBLISHER}\${PRODUCT_ID}" "InstallDir"

RequestExecutionLevel user
ShowInstDetails show
ShowUninstDetails show
BrandingText "${PRODUCT_NAME} ${PRODUCT_VERSION}"

!define MUI_ABORTWARNING
; MUI defaults for icon (omit MUI_ICON/MUI_UNICON to use built-ins).

; ---------------------------------------------------------------------------
; Pages
; ---------------------------------------------------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TITLE "Focus Sync installed"
!define MUI_FINISHPAGE_TEXT    "Focus Sync has been registered with Sigma File Manager.$\r$\n$\r$\nNext steps:$\r$\n1. Open Sigma File Manager$\r$\n2. Press Ctrl+Shift+X to open Extensions$\r$\n3. The Installed tab should now list Focus Sync$\r$\n4. Toggle it on and try Save-As dialog focus sync$\r$\n$\r$\nIf Focus Sync does not appear, see README.md in the installer directory."
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_LANGUAGE "SimpChinese"

; ---------------------------------------------------------------------------
; Helper: detect if Sigma FM is running. Sets $0 to "running" or "stopped".
; ---------------------------------------------------------------------------
Function CheckSigmaFMRunning
    ; Use $$p so NSIS preprocessor doesn't substitute it (literal $p for PowerShell).
    nsExec::ExecToLog 'powershell -NoProfile -Command "$$p = Get-Process -Name sigma-file-manager -ErrorAction SilentlyContinue; if ($$p) { exit 0 } else { exit 1 }"'
    Pop $0
    ; $0 is exit code: 0 = running, 1 = stopped
    StrCmp $0 "0" 0 +2
        StrCpy $0 "running"
        Goto +2
        StrCpy $0 "stopped"
FunctionEnd

; ---------------------------------------------------------------------------
; Helper: kill Sigma FM gracefully via kill-sigma.ps1 in $PLUGINSDIR.
; Returns $0 = 0 on success (clean exit OR force-kill), 1 on hard failure.
; ---------------------------------------------------------------------------
Function KillSigmaFM
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\kill-sigma.ps1"'
    Pop $0
FunctionEnd

; ---------------------------------------------------------------------------
; Helper: re-launch Sigma FM via reopen-sigma.ps1 in $PLUGINSDIR.
; Returns $0 = 0 on success, 1 if binary not found / failed to start.
; ---------------------------------------------------------------------------
Function ReopenSigmaFM
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\reopen-sigma.ps1"'
    Pop $0
FunctionEnd

; ---------------------------------------------------------------------------
; Installation
; ---------------------------------------------------------------------------
Section "Install Focus Sync Extension" SecInstall
    SectionIn RO

    DetailPrint "==> [Focus Sync Installer] v0.3.0 (Scheduled Task mode) BEGIN"

    ; ---- Stage 0: Stage PowerShell helpers in $PLUGINSDIR (NSIS temp) ----
    DetailPrint "==> Stage 0: Preparing helper scripts in $PLUGINSDIR..."
    SetOutPath $PLUGINSDIR
    File "kill-sigma.ps1"
    File "reopen-sigma.ps1"

    ; ---- Stage 1: If Sigma FM is running, kill it gracefully + reopen later ----
    ; Round-2 of class fix (2026-09-30 09:55 follow-up): replace HARD-BLOCK
    ; dialog with auto-kill + auto-reopen. Reason: Sigma FM holds an
    ; in-memory copy of user-extensions.json and silently overwrites the
    ; on-disk file on next save. We must kill it before install so
    ; register.ps1's write survives. User explicitly preferred auto-kill
    ; over the manual-close popup.
    StrCpy $SIGMAWASRUNNING "0"
    DetailPrint "==> Stage 1: Checking if Sigma FM is running..."
    Call CheckSigmaFMRunning
    ${If} $0 == "running"
        DetailPrint "==> Sigma FM is running. Sending WM_CLOSE + waiting for graceful exit..."
        Call KillSigmaFM
        Pop $0
        ${If} $0 == "0"
            DetailPrint "==> Sigma FM exited gracefully."
        ${Else}
            DetailPrint "==> WARNING: Sigma FM force-killed (did not respond to WM_CLOSE within 10s)."
        ${EndIf}
        StrCpy $SIGMAWASRUNNING "1"
    ${Else}
        DetailPrint "==> Sigma FM is not running (good)."
    ${EndIf}

    ; ---- Stage 1.5 (v0.3.0): Kill legacy v0.2.0 spawn-mode sidecar. ----
    ; v2 review V3.5: precise PowerShell path filter (NOT wildcard "*kizemo*").
    DetailPrint "==> Stage 1.5: Killing legacy v0.2.0 spawn-mode sidecar (precise path filter)..."
    nsExec::ExecToLog 'powershell -NoProfile -Command "Get-Process focus-sync-sidecar -ErrorAction SilentlyContinue | Where-Object { $_.Path -like ''*\Sigma File Manager\tools\focus-sync-sidecar.exe'' -or $_.Path -like ''*\kizemo.focus-sync\bin\focus-sync-sidecar.exe'' } | Stop-Process -Force"'
    Pop $0
    ; Non-zero exit is fine

    ; ---- Stage 2: Create installer metadata dir ----
    DetailPrint "==> Stage 2: Creating installer metadata dir at $INSTDIR..."
    SetOutPath "$INSTDIR"
    File "register.ps1"
    File "unregister.ps1"
    File "README.md"

    ; ---- Stage 2.5 (v0.3.0): Bundle PS scripts for Scheduled Task creation.
    ; v2 review S2: use NSIS `File` directive (NOT `!include` which is a
    ; preprocessor directive). PS files go to $INSTDIR\bin\ alongside
    ; the sidecar binary, so the install stage 9 below can invoke them
    ; via PowerShell.
    ; ----
    SetOutPath "$INSTDIR\bin"
    File "register-scheduled-task.ps1"
    File "unregister-scheduled-task.ps1"
    File "..\extension\bin\focus-sync-sidecar.exe"

    ; ---- Stage 3: Copy extension files to %APPDATA% ----
    DetailPrint "==> Stage 3: Copying extension files to ${SIGMA_USERDATA}\extensions\${PRODUCT_EXTDIR}\"
    SetOutPath "${SIGMA_USERDATA}\extensions\${PRODUCT_EXTDIR}"
    File "..\extension\package.json"

    SetOutPath "${SIGMA_USERDATA}\extensions\${PRODUCT_EXTDIR}\dist"
    File "..\extension\dist\index.js"
    File "..\extension\dist\index.js.map"

    SetOutPath "${SIGMA_USERDATA}\extensions\${PRODUCT_EXTDIR}\locales"
    File "..\extension\locales\en.json"
    File "..\extension\locales\zh-CN.json"

    ; ---- Stage 4 (v0.3.0): REMOVED — Sigma FM no longer spawns sidecar. ----
    ; The sidecar binary lives at $INSTDIR\bin\focus-sync-sidecar.exe and is
    ; launched by Windows Task Scheduler (stage 5 below), NOT by Sigma FM.

    ; ---- Stage 5: Register in user-extensions.json via PowerShell ----
    DetailPrint "==> Stage 5: Registering extension in user-extensions.json..."
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\register.ps1"'

    ; ---- Stage 6 (v0.3.0): Write registry key + register Scheduled Task ----
    DetailPrint "==> Stage 6: Writing SidecarPath registry key (HKCU\SOFTWARE\kizemo\focus-sync\)..."
    WriteRegStr HKCU "SOFTWARE\kizemo\focus-sync" "SidecarPath" "$INSTDIR\bin\focus-sync-sidecar.exe"

    DetailPrint "==> Stage 6.5: Registering Scheduled Task 'KizemoFocusSync'..."
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\bin\register-scheduled-task.ps1" -SidecarPath "$INSTDIR\bin\focus-sync-sidecar.exe"'
    Pop $0
    ${If} $0 != "0"
        DetailPrint "==> WARNING: register-scheduled-task.ps1 exited with code $0"
    ${EndIf}

    ; ---- Stage 7: Write uninstaller ----
    DetailPrint "==> Stage 7: Writing uninstaller..."
    WriteUninstaller "$INSTDIR\uninst.exe"

    ; ---- Stage 8: Re-open Sigma FM if we killed it at Stage 1 ----
    ${If} $SIGMAWASRUNNING == "1"
        DetailPrint "==> Stage 8: Re-opening Sigma File Manager..."
        Call ReopenSigmaFM
        Pop $0
        ${If} $0 == "0"
            DetailPrint "==> Sigma FM re-opened."
        ${Else}
            DetailPrint "==> WARNING: Could not re-open Sigma FM automatically."
            DetailPrint "    User must launch manually from the install directory."
        ${EndIf}
    ${EndIf}

    DetailPrint "==> [Focus Sync Installer] END"
    DetailPrint "    Extension files:    ${SIGMA_USERDATA}\extensions\${PRODUCT_EXTDIR}"
    DetailPrint "    Sidecar binary:     $INSTDIR\bin\focus-sync-sidecar.exe (perUser)"
    DetailPrint "    PS scripts:         $INSTDIR\bin\register-scheduled-task.ps1"
    DetailPrint "                        $INSTDIR\bin\unregister-scheduled-task.ps1"
    DetailPrint "    Registry:           HKCU\SOFTWARE\kizemo\focus-sync\SidecarPath"
    DetailPrint "    Scheduled Task:     KizemoFocusSync (AtLogOn, --service mode)"
    DetailPrint "    user-extensions.json updated (with isLocal: true)."
    DetailPrint "    Sigma FM was running at start: $SIGMAWASRUNNING (1 = killed+reopened, 0 = not running)"
SectionEnd

; ---------------------------------------------------------------------------
; Uninstaller
; ---------------------------------------------------------------------------
Section "Uninstall"
    DetailPrint "==> [Focus Sync Uninstaller] BEGIN"

    ; Remove extension files
    DetailPrint "==> Removing extension files..."
    RMDir /r "${SIGMA_USERDATA}\extensions\${PRODUCT_EXTDIR}"
    ; Legacy cleanup: round 7 placed sidecar under binaries\ not extensions\<id>\bin\
    RMDir /r "${SIGMA_USERDATA}\binaries\${PRODUCT_SIDECAR}"

    ; Unregister from user-extensions.json
    DetailPrint "==> Unregistering from user-extensions.json..."
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\unregister.ps1"'
    Pop $0
    ${If} $0 != "0"
        DetailPrint "==> WARNING: PowerShell unregister failed (exit $0)."
    ${EndIf}

    ; ---- Stage v0.3.0: Unregister Scheduled Task + kill orphan sidecar. ----
    DetailPrint "==> Unregistering Scheduled Task and killing orphan sidecar..."
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\bin\unregister-scheduled-task.ps1"'
    Pop $0

    ; Delete SidecarPath registry key
    DetailPrint "==> Deleting SidecarPath registry key..."
    DeleteRegKey HKCU "SOFTWARE\kizemo\focus-sync"

    ; Remove installer metadata dir
    DetailPrint "==> Removing installer metadata..."
    RMDir /r "$INSTDIR"

    DetailPrint "==> [Focus Sync Uninstaller] END"
SectionEnd

; ---------------------------------------------------------------------------
; Uninstaller init (write uninstaller.exe to known path)
; ---------------------------------------------------------------------------
Function un.onInit
    ; Detect if Sigma FM is running
    nsExec::ExecToLog 'powershell -NoProfile -Command "$$p = Get-Process -Name sigma-file-manager -ErrorAction SilentlyContinue; if ($$p) { exit 0 } else { exit 1 }"'
    Pop $0
    ${If} $0 == "0"
        ; Sigma FM is running — confirm user wants to continue
        MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON2 "Sigma FM is currently running.$\r$\n$\r$\nUninstall cannot fully clean user-extensions.json while Sigma FM holds the in-memory state.$\r$\n$\r$\nContinue anyway?$\r$\n(You should close Sigma FM first for a clean uninstall)"
        ; MessageBox sets $0 to 6 (YES) or 7 (NO)
        ${If} $0 == "7"
            Abort
        ${EndIf}
    ${EndIf}
FunctionEnd
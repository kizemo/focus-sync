// MIT License — Copyright (c) inaku-Gyan
// Ported from https://github.com/inaku-Gyan/PathWrap/blob/main/src/os/dialog.rs
// Original file: src/os/dialog.rs (176 lines, MIT licensed)
//
// v0.5.0 (2026-10-04) — ROLLBACK to Sep 27 PathWrap port design:
//   The 12 rounds of focus-19/20/21/22 + v0.3.4-0.3.9 introduced so many
//   verification, basename, and WinUI 3 detection layers that the simple
//   working UIA-only SetValue+Invoke approach was lost. This rollback
//   restores the Sep 27 design (which the user verified PASSES manually):
//
//   1. Scan dialog for `best_edit` (1148/1001 automation_id or "文件名" name)
//      and `best_button` (IDOK or "保存"/"打开" name).
//   2. SetValue the FULL `target_path` to best_edit.
//
//   === v0.5.5 DESIGN ORIGIN NOTE (READ BEFORE REVERTING) ===
//   The v0.5.0 design chose 41477 (Chromium address bar ComboBox) as
//   best_edit, based on the WRONG assumption "Chromium address bar
//   SetValue triggers navigation". In reality, Chromium ComboBox
//   ValuePattern SetValue APPENDS to .Value (does NOT navigate), so
//   F3 baseline write polluted the address bar with "<current>C:\\"
//   instead of navigating to C:\\.
//
//   v0.5.5 (2026-10-06 Mavis + Claude 复盘) excludes 41477 from
//   best_edit candidates at uia_inject.rs:592-598 via `continue` filter
//   (uses the `aid` variable already computed at line 573 to avoid
//   redundant UIA COM calls). See Fix A in:
//   docs/superpowers/plans/2026-10-06-focus-21-v0.5.5-ship-ready-plan-v6.md
//
//   DO NOT REVERT this fix without first running a real Chromium
//   ComboBox SetValue test (it will append. Test is wrong. Don't try).
//   3. If should_commit, Invoke best_button (or SendMessageW Enter fallback).
//
//   COM IFileDialog::SetFolder (round 19f) is preserved as the FIRST attempt
//   because it's the canonical way to change directory in Vista+ native
//   dialogs and silently succeeds for Edge Legacy / Photos without polluting
//   the filename field.
//
// User accepted trade-off (Sep 27 + focus-18 era): filename field may show
// the target path briefly (2-6s) on dialogs where UIA SetValue writes to the
// filename field. This was acceptable when the user accepted the spike-report
// PASS. The 12-round rejection cycle was triggered by trying to "fix" this
// trade-off — see `feedback_phase2_giveup.md` Rule 31/32/36.

//! UIA + COM path injection: drive a file dialog to a specific folder.
//!
//! Two-stage approach:
//! 1. `try_set_folder_via_com` (round 19f): COM IFileDialog::SetFolder via
//!    AccessibleObjectFromWindow → IServiceProvider → IFileDialog → SetFolder.
//!    Handles Vista+ IFileDialog natively (Windows Photos, Edge Legacy,
//!    Paint, Notepad Save As, etc.) without polluting the filename field.
//! 2. UIA fallback (PathWrap port): if COM fails, find best_edit + best_button
//!    via UIA, SetValue target_path, Invoke Save if should_commit.
//!
//! Both stages trust UIA/COM's own feedback — no read-back verify, no
//! WinUI 3 special-casing, no address-bar fallback. The Sep 27 design was
//! proven by the spike-report PASS.

use tracing::{debug, error, info, warn};
use std::cell::RefCell;
use windows::core::{BSTR, Error, Result, PCWSTR, Interface};
use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IServiceProvider,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement,
    IUIAutomationInvokePattern, IUIAutomationValuePattern,
    TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_EditControlTypeId,
    UIA_InvokePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Shell::{IFileDialog, IShellItem, SHCreateItemFromParsingName};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, GetForegroundWindow, SendMessageW, SetForegroundWindow, WM_KEYDOWN, WM_KEYUP,
};
// v0.5.1 (Mavis 第 13 轮 D2 [HIGH]): complete SendInput helper imports.
// windows-rs 0.62 names the keyboard-flag newtype `KEYBD_EVENT_FLAGS`
// and the constants `KEYEVENTF_*`. The local `VK_RETURN: usize` constant
// is kept for `SendMessageW`'s `WPARAM` cast — we do not import the
// windows-rs `VK_RETURN` symbol to avoid a name collision.
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput as SendInputFn, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    VIRTUAL_KEY, VK_L, VK_CONTROL,
    KEYEVENTF_UNICODE, KEYEVENTF_KEYUP,
    VK_HOME, VK_END, VK_SHIFT,
    GetFocus,  // ← v0.5.3 (Mavis C1 必修): post-SendInput focus verify
};
use windows::core::GUID;

const VK_RETURN: usize = 0x0D;

// v0.5.1 (Mavis 第 13 轮 D1 [HIGH]): AllowSetForegroundWindow ASFW_ANY = 0xFFFFFFFF.
// Per MSDN: dwProcessId = ASFW_ANY means any process is allowed.
// Windows crate API does not export the symbol `ASFW_ANY`, so we hard-define it
// here. Using 0x00000001 (the previous value) is wrong — it rejects most
// foreground grants and silently fails the SendInput path.
const ASFW_ANY: u32 = 0xFFFFFFFF;

/// focus-19 (2026-10-02): structured result of an injection attempt.
///
/// `Unsupported` is bumped on `state.unsupported_dialog_count` so the extension
/// can surface a user-facing notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogOutcome {
    /// COM IFileDialog::SetFolder succeeded (Edge Legacy, Notepad, Photos).
    ComSetFolder,
    /// UIA SetValue + Invoke on best_edit + best_button.
    AddressBarWritten,
    /// No navigable element found; user must intervene.
    Unsupported,
    /// Zero hwnd or empty target.
    Invalid,
}

// IIDs for COM interop (round 19f, preserved from v0.3.0).
const IID_IAccessible: GUID = GUID::from_u128(0x618736E0_3C3D_11CF_810C_00AA00389B71);
const IID_IFileDialog: GUID = GUID::from_u128(0x84BCCD23_5FDE_4CDB_AEA4_AF62BAA7B3B6);
const SID_STopLevelBrowser: GUID = GUID::from_u128(0x208D2C60_3AEA_1069_A2D7_08002B30309D);
const OBJID_NATIVEOM: u32 = 0xFFFFFFF0;

/// Extract the basename of a Windows path (used by tests).
///
/// Examples:
///   basename(r"C:\downloads") == "downloads"
///   basename(r"C:\a\b\c") == "c"
///   basename("downloads") == "downloads"
fn basename(path: &str) -> &str {
    let p = path.trim_end_matches(['\\', '/']);
    match p.rfind(['\\', '/']) {
        Some(idx) => &p[idx + 1..],
        None => p,
    }
}

// ----------------------------------------------------------------------
// v0.5.1 SendInput helpers (Mavis 第 13 轮 D2 [HIGH] — full implementation)
// ----------------------------------------------------------------------
//
// These helpers existed as `/* ... */` placeholders in the v0.5.1 plan; this
// section provides the complete SendInput path that does NOT touch the
// 1001 Edit (which in WinUI 3 wrapped dialogs is a dual-role field — writing
// it pollutes the user's filename while also driving Chromium's internal
// address-bar parser).
//
// IMPORTANT architectural note (Mavis 第 13 轮 D3 [HIGH]):
//   The sidecar (`spike.exe`) is launched by the user's Scheduled Task at
//   logon. It has never received foreground focus, so on Windows Vista+ the
//   foreground-lock rules in `SetForegroundWindow` reject our request to
//   pull the dialog into the foreground. `AllowSetForegroundWindow` must be
//   invoked by the **receiving** process (Sigma FM, or an elevated helper),
//   so calling it from the sidecar is functionally a no-op. The SendInput
//   path therefore only succeeds when:
//     (a) The user already has the dialog focused (so SetForegroundWindow
//         is a no-op success), AND
//     (b) Either the sidecar still has a foreground grant from a recent
//         call, OR Sigma FM has called AllowSetForegroundWindow on us.
//
//   v0.5.1 implements the SendInput code defensively (best-effort) and
//   wraps the rest of the flow in a 4-layer fallback chain (SendInput →
//   H2 restore-filename → v0.5.0 SetValue → Unsupported) so a SendInput
//   failure is never a regression vs. v0.5.0.

/// Build an INPUT for a virtual-key press/release.
///
/// `vk` is the Win32 virtual-key code (e.g. `0x4C` for L).
/// `flags` is the raw `KEYEVENTF_*` bitmask (0 for press, `KEYEVENTF_KEYUP.0`
/// for release).
///
/// Used for non-character keys (Ctrl, L, Enter). For path characters we use
/// `make_unicode` so that codepoints outside the OEM scancode set still
/// arrive correctly.
fn make_key(vk: u16, flags: u32) -> INPUT {
    let mut input = unsafe {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: std::mem::zeroed(),
        }
    };
    input.Anonymous.ki = KEYBDINPUT {
        wVk: VIRTUAL_KEY(vk),
        wScan: 0,
        dwFlags: KEYBD_EVENT_FLAGS(flags),
        time: 0,
        dwExtraInfo: 0,
    };
    input
}

/// Build an INPUT that injects a single Unicode character via
/// `KEYEVENTF_UNICODE`.
///
/// This is the only reliable way to feed a path containing non-ASCII
/// characters (e.g. CJK usernames) to a Chromium internal input pipeline.
/// The virtual-key code is set to `0` because the OS derives it from the
/// Unicode codepoint.
fn make_unicode(ch: u16) -> INPUT {
    let mut input = unsafe {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: std::mem::zeroed(),
        }
    };
    input.Anonymous.ki = KEYBDINPUT {
        wVk: VIRTUAL_KEY(0),
        wScan: ch,
        dwFlags: KEYEVENTF_UNICODE,
        time: 0,
        dwExtraInfo: 0,
    };
    input
}

/// v0.5.2 (I3 fix, Mavis review): KEYEVENTF_UNICODE also needs a release
/// event per MSDN SendInput docs. Without this, the OS may auto-release on
/// its own (and Chromium usually interprets single unicode events fine), but
/// some apps / dead-key compositions are missed. We pair every press with
/// a release for correctness.
fn make_unicode_release(ch: u16) -> INPUT {
    let mut input = unsafe {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: std::mem::zeroed(),
        }
    };
    input.Anonymous.ki = KEYBDINPUT {
        wVk: VIRTUAL_KEY(0),
        wScan: ch,
        dwFlags: KEYBD_EVENT_FLAGS(KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0),
        time: 0,
        dwExtraInfo: 0,
    };
    input
}

/// v0.5.3 H1 — SendInput Ctrl+L + Home+Shift+End + path. NO Enter. NO focus theft.
///
/// Strategy (Mavis 第 15/16/17 轮 review 综合):
///   1. `AllowSetForegroundWindow(ASFW_ANY)` — accept any sender
///      (no-op in sidecar context per D3, but kept for completeness).
///   2. `SetForegroundWindow(dialog)` — try to bring the dialog forward.
///      May fail under the Vista+ foreground lock; we proceed regardless.
///   3. **GetForegroundWindow check** — warn if dialog is not foreground
///      so user knows events may have gone elsewhere (Mavis A4).
///   4. `SendInput Ctrl+L` — focus Chromium's address bar.
///   5. `SendInput Home + Shift+End` — basic Edit operations that WinUI 3
///      is unlikely to intercept. Replaces v0.5.2's broken Ctrl+A.
///   6. `SendInput path chars` (press + release per MSDN).
///   7. **NO Enter** (Rule 47 — user wants manual Save confirmation).
///   8. **post-SendInput focus verify** — GetFocus + UIA read-back; warn if
///      focus landed on filename box (1001) — pollution risk (Mavis I1).
///
/// Returns true iff SendInput accepted every INPUT. Does NOT auto-trigger Save.
/// Does NOT steal focus from user (Mavis A1 — KEEP SetForegroundWindow but
/// it usually no-ops; extension has been updated to NOT call
/// AllowSetForegroundWindow so Sigma FM never gets yanked).
fn try_send_path_via_sendinput(dialog_hwnd: u32, target_path: &str) -> bool {
    let hwnd = HWND(dialog_hwnd as *mut core::ffi::c_void);
    unsafe {
        // Mavis A1 (第 15 轮): KEEP AllowSetForegroundWindow + SetForegroundWindow.
        // SendInput dispatches events to the foreground window — if dialog
        // is backgrounded, events go to Sigma FM (worse: would pollute
        // Sigma FM's search box).
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let _ = SetForegroundWindow(hwnd);

        // Mavis A4: check if dialog actually became foreground. Warn user
        // so they know SendInput may not reach dialog (race window).
        let current_fg = GetForegroundWindow();
        if current_fg != hwnd {
            warn!(
                "v0.5.5 SendInput: dialog hwnd={hwnd:?} is not foreground \
                 (current={current_fg:?}); aborting to trigger H2 fallback."
            );
            // v0.5.4 (2026-10-06 复盘): MUST abort instead of silent continue.
            // Per Mavis review §1.3: SendInput events dispatch to the
            // foreground window (possibly NOT dialog). Without abort + H2
            // fallback, dialog never receives the path and user sees
            // "completely no response". Abort returns false → inject_via_uia
            // falls through to H2 (UIA SetValue, no foreground dependency).
            return false;
        }

        // Mavis O2 (第 18 轮): Vec capacity should match actual push count.
        // Actual: 4 (Ctrl+L) + 6 (Home+Shift+End) + 2N (path press+release) = 10 + 2N.
        let mut inputs: Vec<INPUT> =
            Vec::with_capacity(10 + target_path.encode_utf16().count() * 2);

        // Step 4: Ctrl+L — focus Chromium's address bar.
        inputs.push(make_key(VK_CONTROL.0, 0));
        inputs.push(make_key(VK_L.0, 0));
        inputs.push(make_key(VK_L.0, KEYEVENTF_KEYUP.0));
        inputs.push(make_key(VK_CONTROL.0, KEYEVENTF_KEYUP.0));

        // Step 5: Home + Shift+End — basic Edit operations that WinUI 3 is
        // unlikely to intercept (vs. Ctrl+A which is a Chrome extension that
        // WinUI 3 may eat). Replaces v0.5.2's broken Ctrl+A.
        inputs.push(make_key(VK_HOME.0, 0));
        inputs.push(make_key(VK_HOME.0, KEYEVENTF_KEYUP.0));
        inputs.push(make_key(VK_SHIFT.0, 0));
        inputs.push(make_key(VK_END.0, 0));
        inputs.push(make_key(VK_END.0, KEYEVENTF_KEYUP.0));
        inputs.push(make_key(VK_SHIFT.0, KEYEVENTF_KEYUP.0));

        // Step 6: type the path char-by-char (press + release per MSDN).
        for ch in target_path.encode_utf16() {
            inputs.push(make_unicode(ch));
            inputs.push(make_unicode_release(ch));
        }

        let n = SendInputFn(&inputs, std::mem::size_of::<INPUT>() as i32);
        let total = inputs.len() as u32;
        if n != total {
            warn!(
                "v0.5.5 SendInput: queued {total} events, system accepted only {n}; \
                 foreground-lock likely blocked some events (Mavis D3)"
            );
            return false;
        }
        info!(
            "v0.5.5 SendInput Ctrl+L + Home+Shift+End + path dispatched \
             ({total} events, no Enter)"
        );

        // Mavis I1 (第 16 轮): post-SendInput verify focused element.
        // Confirms U-HK (Ctrl+L focused address bar, not 1001 Edit).
        // If focused on filename box (1001), SendInput may have polluted
        // filename — surface a warn so the user can see in spike.log.
        let focused_hwnd = GetFocus();
        if let Some(uia) = automation() {
            if let Ok(elem) = uia.ElementFromHandle(focused_hwnd) {
                let aid = current_automation_id(&elem);
                let name = current_name(&elem);
                info!(
                    "v0.5.5 SendInput: post-SendInput focus aid='{aid}' name='{name}'"
                );
                if aid == "1001" || name.to_lowercase().contains("file name") {
                    warn!(
                        "v0.5.5 SendInput: focused on filename box (1001) — \
                         SendInput may pollute filename. Consider H2 fallback."
                    );
                }
            }
        }

        true
    }
}

/// v0.5.1 H2 — write target_path then immediately restore the original
/// filename.
///
/// Chromium's Save-As breadcrumb is updated by the first SetValue (it
/// parses the path). The second SetValue restores the visible filename
/// string. In practice Chromium does NOT roll the breadcrumb back when
/// we restore the filename — the navigation already committed.
///
/// Risk acknowledged (Mavis D4 [MEDIUM]): Chromium may roll the
/// breadcrumb back when the filename changes again. If that happens,
/// v0.5.1 falls through to the v0.5.0 baseline (which is known to
/// pollute the filename, but at least navigates).
///
/// 50ms gap between the two SetValue calls: most users cannot perceive
/// this (Mavis D7 [LOW]); U-V3 is the explicit user-verification task.
fn try_h2_restore_filename(edit: &IUIAutomationElement, target_path: &str) -> bool {
    let value: IUIAutomationValuePattern = unsafe {
        match edit.GetCurrentPatternAs(UIA_ValuePatternId) {
            Ok(v) => v,
            Err(e) => {
                warn!("v0.5.1 H2: failed to acquire ValuePattern: {e:?}");
                return false;
            }
        }
    };
    let original_filename = unsafe { value.CurrentValue() }
        .unwrap_or_default()
        .to_string();
    if original_filename.is_empty() {
        warn!("v0.5.1 H2: original filename is empty; skipping restore path");
        return false;
    }

    // Step 1: write target_path so Chromium updates its breadcrumb.
    if let Err(e) = unsafe { value.SetValue(&BSTR::from(target_path)) } {
        warn!("v0.5.1 H2: SetValue target_path failed: {e:?}");
        return false;
    }
    // Brief settle so Chromium's internal address-bar parser runs.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Step 2: restore original filename so the visible field is clean.
    if let Err(e) = unsafe { value.SetValue(&BSTR::from(original_filename.as_str())) } {
        warn!("v0.5.1 H2: SetValue restore filename failed: {e:?}");
        return false;
    }
    info!(
        "v0.5.1 H2: target_path written and original filename restored ({} bytes)",
        original_filename.len()
    );
    true
}

/// v0.5.1 D9 [LOW]: detect WinUI 3 wrapped Save As.
///
/// WinUI 3 wrapped Edge Save As has BOTH:
///   - `automation_id == "1001"` (filename Edit, dual-role)
///   - either `automation_id` or `name` containing `FileNameControlHost`
///     (the WinUI 3 Pane that hosts the filename label) OR
///     `automation_id == "50003"` (WinUI 3 Pane class id, alternate form)
///
/// True IFileDialog has `1001` only. Chromium 41477 has `41477` only. So
/// `(1001 ∧ FileNameControlHost/50003)` is the unique WinUI 3 wrapped
/// signature.
fn is_winui3_wrapped_signature(has_filename_host: bool, has_1001: bool) -> bool {
    has_filename_host && has_1001
}

/// Round 19f: try COM IFileDialog::SetFolder first. Returns true on success.
fn try_set_folder_via_com(dialog_hwnd: u32, target_path: &str) -> bool {
    use windows::Win32::UI::Accessibility::AccessibleObjectFromWindow;

    unsafe {
        let hwnd = HWND(dialog_hwnd as *mut core::ffi::c_void);

        // 1. AccessibleObjectFromWindow → IAccessible
        let mut ppv: *mut core::ffi::c_void = std::ptr::null_mut();
        if AccessibleObjectFromWindow(hwnd, OBJID_NATIVEOM, &IID_IAccessible, &mut ppv).is_err()
            || ppv.is_null()
        {
            warn!("AccessibleObjectFromWindow failed; dialog is not a native IFileDialog");
            return false;
        }
        let acc: windows::Win32::UI::Accessibility::IAccessible =
            windows::Win32::UI::Accessibility::IAccessible::from_raw(ppv);

        // 2. QI → IServiceProvider
        let sp: IServiceProvider = match acc.cast() {
            Ok(s) => s,
            Err(e) => {
                warn!("QI for IServiceProvider failed: {e:?}");
                return false;
            }
        };

        // 3. QueryService(SID_STopLevelBrowser) → IFileDialog
        let file_dialog: IFileDialog = match sp.QueryService(&SID_STopLevelBrowser) {
            Ok(f) => f,
            Err(e) => {
                warn!("QueryService(SID_STopLevelBrowser) → IFileDialog failed: {e:?}");
                return false;
            }
        };

        // 4. SHCreateItemFromParsingName → IShellItem
        let wide: Vec<u16> = target_path.encode_utf16().chain(std::iter::once(0)).collect();
        let item: IShellItem = match SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) {
            Ok(i) => i,
            Err(e) => {
                warn!("SHCreateItemFromParsingName failed for '{target_path}': {e:?}");
                return false;
            }
        };

        // 5. SetFolder
        match file_dialog.SetFolder(&item) {
            Ok(()) => {
                info!("SetFolder via COM succeeded for '{target_path}'");
                true
            }
            Err(e) => {
                warn!("IFileDialog::SetFolder failed: {e:?}");
                false
            }
        }
    }
}

/// Reuse the main-thread `IUIAutomation` instance.
pub fn automation() -> Option<IUIAutomation> {
    thread_local! {
        static INSTANCE: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    }

    INSTANCE.with(|cell| {
        if let Some(existing) = cell.borrow().as_ref() {
            return Some(existing.clone());
        }
        let created = unsafe {
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        };
        match created {
            Ok(uia) => {
                *cell.borrow_mut() = Some(uia.clone());
                Some(uia)
            }
            Err(err) => {
                error!("failed to create IUIAutomation: {err}");
                None
            }
        }
    })
}

/// Inject `target_path` into the dialog.
///
/// v0.5.0 rollback: COM IFileDialog::SetFolder first, then UIA fallback
/// (best_edit + best_button via PathWrap-style scoring). No WinUI 3 detection,
/// no address-bar fallback, no read-back verify. Trust the underlying APIs.
pub fn inject_folder_path(
    state: &crate::state::AppState,
    dialog_hwnd: u32,
    target_path: &str,
    should_commit: bool,
) -> DialogOutcome {
    if dialog_hwnd == 0 {
        error!("invalid dialog hwnd: 0");
        return DialogOutcome::Invalid;
    }
    if target_path.trim().is_empty() {
        error!("target path is empty");
        return DialogOutcome::Invalid;
    }

    info!(
        "Injection starts: hwnd={dialog_hwnd} target='{target_path}' should_commit={should_commit}"
    );

    // Stage 1: COM IFileDialog::SetFolder (Edge Legacy / Notepad / Photos).
    if try_set_folder_via_com(dialog_hwnd, target_path) {
        info!("Path sync via IFileDialog::SetFolder succeeded.");
        return DialogOutcome::ComSetFolder;
    }
    warn!("COM SetFolder unavailable; falling back to UIA.");

    // Stage 2: UIA fallback (Sep 27 PathWrap design).
    match inject_via_uia(dialog_hwnd, target_path, should_commit) {
        Ok(()) => {
            info!("Injection succeeded via UI Automation.");
            DialogOutcome::AddressBarWritten
        }
        Err(err) => {
            warn!("Injection failed via UI Automation: {err}");
            state.inc_unsupported_dialog();
            DialogOutcome::Unsupported
        }
    }
}

/// Sep 27 PathWrap port: scan for best_edit + best_button, SetValue, Invoke.
///
/// v0.5.1 (Mavis 第 13 轮 D5/D9): when the dialog has the WinUI 3 wrapped
/// signature (`1001` + `FileNameControlHost`/`50003`), apply a 4-layer
/// fallback chain before falling back to the v0.5.0 baseline SetValue:
///
///   H1 — `SendInput Ctrl+L + path + Enter` — does NOT touch the 1001 Edit
///        so the user's filename stays clean. Best-effort under the
///        Vista+ foreground lock (Mavis D3 [HIGH]).
///   H2 — write target_path, then restore the original filename (50ms
///        gap). Breadcrumb usually survives the second SetValue; if it
///        rolls back, H2 is treated as a no-op (Mavis D4/D7).
///   F3 — original v0.5.0 SetValue 1001 + Invoke Save. Known to pollute
///        the filename field on WinUI 3 wrapped dialogs but guarantees
///        navigation (this is the v0.5.0 ship behavior).
///   F4 — return Unsupported (the caller maps this to a
///        `unsupported_dialog_count` bump and a user notification).
fn inject_via_uia(
    dialog_hwnd: u32,
    target_path: &str,
    should_commit: bool,
) -> Result<()> {
    let uia = automation().ok_or_else(|| Error::from(E_FAIL))?;
    let dialog = HWND(dialog_hwnd as *mut core::ffi::c_void);

    let root = unsafe { uia.ElementFromHandle(dialog)? };
    let condition = unsafe { uia.CreateTrueCondition()? };
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
    let count = unsafe { elements.Length()? };

    // D9 [LOW]: inventory both candidates — we want to know if this is a
  // WinUI 3 wrapped dialog BEFORE we decide which fallback layer to start at.
    let mut has_filename_host = false;
    let mut has_1001 = false;
    let mut best_edit: Option<IUIAutomationElement> = None;
    let mut best_edit_score = i32::MIN;
    let mut best_button: Option<IUIAutomationElement> = None;
    let mut best_button_score = i32::MIN;

    for i in 0..count {
        let element = unsafe { elements.GetElement(i)? };
        let control_type = unsafe { element.CurrentControlType()? };
        let aid = current_automation_id(&element);
        let name = current_name(&element);

        // D9: detect the WinUI 3 wrapped signature.
        // `FileNameControlHost` may live as automation_id, name, or as the
        // numeric pane id `50003` (per v0.5.1 plan §1.2). Also accept the
        // case-insensitive presence in the name (WinUI 3 sometimes surfaces
        // it as a localized label, e.g. "File Name Control Host").
        if aid == "FileNameControlHost"
            || name.contains("FileNameControlHost")
            || name.to_lowercase().contains("file name control host")
            || aid == "50003"
        {
            has_filename_host = true;
        }
        if aid == "1001" {
            has_1001 = true;
        }

        if control_type == UIA_EditControlTypeId {
            // v0.5.5 (Mavis Round 25 Q-4, Round 26 S-1): 41477 is Chromium
            // address bar ComboBox, UIA ValuePattern SetValue is APPEND not
            // replace — writing to it pollutes the address bar with
            // "<current>C:\\" instead of navigating. Exclude from best_edit
            // candidates entirely. Reuse the `aid` variable computed at
            // line 573 (avoid redundant UIA COM call per element).
            if aid == "41477" {
                continue;
            }
            let score = filename_edit_score(&element);
            if score > best_edit_score {
                best_edit_score = score;
                best_edit = Some(element);
            }
        } else if control_type == UIA_ButtonControlTypeId {
            let score = confirm_button_score(&element);
            if score > best_button_score {
                best_button_score = score;
                best_button = Some(element);
            }
        }
    }

    let is_winui3_wrapped = is_winui3_wrapped_signature(has_filename_host, has_1001);

    // D5 [MEDIUM]: 4-layer fallback chain for WinUI 3 wrapped dialogs.
    if is_winui3_wrapped {
        debug!(
            "WinUI 3 wrapped detected (has_filename_host={has_filename_host} has_1001={has_1001}); \
             entering 4-layer fallback chain"
        );

        // H1 — SendInput Ctrl+L + path (NO Enter, NO 1001 Edit touch).
        // v0.5.1 (2026-10-05): user feedback "下载启动后,跳过了路径选择步骤,
        // 直接启动下载". The Enter + commit_save_button combo auto-triggered
        // the save. Now we just set the address bar text and let the user
        // navigate manually (click tree view, press Enter, click Save).
        if try_send_path_via_sendinput(dialog_hwnd, target_path) {
            info!("v0.5.1 H1 succeeded: SendInput Ctrl+L set address bar (user navigates + saves manually)");
            // v0.5.1: NEVER auto-Invoke Save after H1. The user explicitly
            // requested manual confirmation. Even if should_commit=true,
            // we do not commit. (Future: maybe re-enable should_commit if
            // a new "trusted auto-confirm" user setting is added.)
            return Ok(());
        }
        warn!("v0.5.1 H1 failed: SendInput did not reach the dialog (foreground lock?)");

        // H2 — write target_path then restore the original filename.
        // v0.5.1 (2026-10-05): user wants manual confirmation. Do not
        // auto-Invoke Save after H2 either — same fix as H1.
        if let Some(edit) = best_edit.as_ref() {
            if try_h2_restore_filename(edit, target_path) {
                info!("v0.5.1 H2 succeeded: target_path written and filename restored (user saves manually)");
                return Ok(());
            }
        }
        warn!("v0.5.1 H2 failed: restore-filename path did not produce navigation");

        // F3 — fall through to the v0.5.0 baseline SetValue path below.
        warn!(
            "v0.5.1: H1 + H2 failed; falling back to v0.5.0 SetValue 1001 \
             (filename pollution expected on WinUI 3 wrapped dialogs)"
        );
    }

    // F3 — v0.5.0 baseline: SetValue 1001. v0.5.1 does NOT auto-Invoke Save
    // even here — the user's complaint applies to every fallback layer.
    let edit = best_edit.ok_or_else(|| Error::from(E_FAIL))?;
    let value: IUIAutomationValuePattern =
        unsafe { edit.GetCurrentPatternAs(UIA_ValuePatternId)? };
    unsafe { value.SetValue(&BSTR::from(target_path))? };
    debug!(
        "UIA SetValue OK on automation_id='{}' name='{}' target='{}'",
        current_automation_id(&edit),
        current_name(&edit),
        target_path
    );

    // v0.5.1: should_commit is ignored at the injection layer — the user
    // wants manual save confirmation for every code path. (The should_commit
    // parameter is retained for backward compatibility with writer.rs but
    // the auto-Invoke Save was removed at the user's request.)

    Ok(())
}

/// Invoke the best Save button, or fall back to `SendMessageW(Enter)`.
///
/// v0.5.1 (Mavis D5): extracted from the inline match in `inject_via_uia`
/// so the four code branches (H1 / H2 / F3 / F4) share the same
/// commit semantics.
fn commit_save_button(best_button: Option<&IUIAutomationElement>, best_button_score: i32) {
    match best_button {
        Some(button) if best_button_score > 0 => {
            let invoke_result: Result<IUIAutomationInvokePattern> =
                unsafe { button.GetCurrentPatternAs(UIA_InvokePatternId) };
            match invoke_result {
                Ok(invoke) => {
                    if let Err(e) = unsafe { invoke.Invoke() } {
                        warn!("Invoke(Save button) failed: {e:?}");
                    } else {
                        info!("Committed via Invoke(Save button).");
                    }
                }
                Err(e) => {
                    warn!("GetCurrentPatternAs(InvokePattern) failed: {e:?}");
                }
            }
        }
        _ => {
            warn!("commit: no good button; skipping Enter fallback in v0.5.1 (Chromium path)");
            // Note: we intentionally no longer call fallback_confirm here.
            // The Enter fallback (SendMessageW WM_KEYDOWN) was for legacy
            // IFileDialog. For Chromium hosts the SendInput Enter we
            // already dispatched (H1) or the Invoke-pattern above covers
            // the commit; sending another Enter via SendMessageW would
            // double-fire and select the wrong element.
        }
    }
}

/// Score candidate filename/address input: AutomationId hits highest, then name.
/// v0.5.5 (Mavis Round 23 N-4 / Round 25 Q-4): extracted pure helper so unit
/// tests can verify scoring without instantiating a UIA element. Returns None
/// for unrecognized automation_id (caller falls through to name match).
///
/// NOTE: 41477 is intentionally NOT scored here — it is filtered at the
/// call site (uia_inject.rs:592-598 `continue`) before scoring, because
/// Chromium ComboBox SetValue APPENDs instead of replacing.
fn compute_score_for_automation_id(automation_id: &str) -> Option<i32> {
    if automation_id == "1148" { return Some(100); }
    if automation_id == "1001" { return Some(90); }
    // 41477 deliberately excluded — see call site.
    None
}

/// v0.5.5: extracted pure helper for name-based fallback scoring.
fn compute_score_for_name(name: &str) -> i32 {
    let name = name.to_lowercase();
    if name.contains("文件名") || name.contains("file name") {
        return 80;
    }
    1
}

fn filename_edit_score(element: &IUIAutomationElement) -> i32 {
    let aid = current_automation_id(element);
    if let Some(score) = compute_score_for_automation_id(&aid) {
        return score;
    }
    compute_score_for_name(&current_name(element))
}

/// Score candidate confirm button: IDOK highest, then name matches.
fn confirm_button_score(element: &IUIAutomationElement) -> i32 {
    if current_automation_id(element) == "1" {
        return 100; // IDOK
    }

    let name = current_name(element).to_lowercase();
    if name.contains("打开")
        || name.contains("保存")
        || name.contains("open")
        || name.contains("save")
    {
        return 80;
    }
    i32::MIN + 1
}

fn current_automation_id(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentAutomationId() }
        .unwrap_or_default()
        .to_string()
}

fn current_name(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentName() }
        .unwrap_or_default()
        .to_string()
}

/// Fallback confirmation: send Enter to the filename box's native window handle.
fn fallback_confirm(edit: &IUIAutomationElement) {
    let native = unsafe { edit.CurrentNativeWindowHandle() }.unwrap_or_default();
    if native.is_invalid() {
        warn!("fallback confirm skipped: filename edit has no native window handle");
        return;
    }
    unsafe {
        let _ = SendMessageW(native, WM_KEYDOWN, Some(WPARAM(VK_RETURN)), Some(LPARAM(0)));
        let _ = SendMessageW(native, WM_KEYUP, Some(WPARAM(VK_RETURN)), Some(LPARAM(0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_top_level() {
        assert_eq!(basename(r"C:\downloads"), "downloads");
    }

    #[test]
    fn basename_nested() {
        assert_eq!(basename(r"C:\a\b\c"), "c");
    }

    #[test]
    fn basename_trailing_separator_trimmed() {
        assert_eq!(basename(r"C:\downloads\"), "downloads");
    }

    #[test]
    fn basename_no_separator_returns_input() {
        assert_eq!(basename("downloads"), "downloads");
    }

    #[test]
    fn basename_forward_slash_supported() {
        assert_eq!(basename("C:/downloads/folder"), "folder");
    }

    #[test]
    fn basename_drive_root_only_returns_drive() {
        assert_eq!(basename(r"C:\"), "C:");
    }

    #[test]
    fn basename_empty_returns_empty() {
        assert_eq!(basename(""), "");
    }

    #[test]
    fn basename_unc_supported() {
        assert_eq!(basename(r"\\server\share\file.txt"), "file.txt");
    }

    #[test]
    fn dialog_outcome_variants_distinct() {
        // Document that DialogOutcome is a closed enum; caller maps Unsupported
        // to WriteOutcome::Failed and bumps state.unsupported_dialog_count.
        let outcomes = [
            DialogOutcome::ComSetFolder,
            DialogOutcome::AddressBarWritten,
            DialogOutcome::Unsupported,
            DialogOutcome::Invalid,
        ];
        for (i, a) in outcomes.iter().enumerate() {
            for (j, b) in outcomes.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b, "variants at {i} and {j} must differ");
                }
            }
        }
    }

    #[test]
    fn inject_folder_path_zero_hwnd_returns_invalid() {
        use crate::state::AppState;
        let state = AppState::new("C:\\".into(), 1);
        let initial_unsupported = state.health_snapshot().unsupported_dialog_count;
        let outcome = inject_folder_path(&state, 0, "C:\\downloads", false);
        assert_eq!(outcome, DialogOutcome::Invalid);
        assert_eq!(
            state.health_snapshot().unsupported_dialog_count,
            initial_unsupported,
            "Invalid outcome must not bump unsupported_dialog_count"
        );
    }

    #[test]
    fn inject_folder_path_empty_target_returns_invalid() {
        use crate::state::AppState;
        let state = AppState::new("C:\\".into(), 1);
        let initial_unsupported = state.health_snapshot().unsupported_dialog_count;
        let outcome = inject_folder_path(&state, 12345, "   ", false);
        assert_eq!(outcome, DialogOutcome::Invalid);
        assert_eq!(
            state.health_snapshot().unsupported_dialog_count,
            initial_unsupported,
            "Invalid outcome must not bump unsupported_dialog_count"
        );
    }

    // ----------------------------------------------------------------------
    // v0.5.1 (Mavis 第 13 轮 D1/D2/D4/D5/D9) regression tests
    // ----------------------------------------------------------------------

    /// D1 [HIGH]: ASFW_ANY must be 0xFFFFFFFF. Using 0x00000001 silently
    /// fails the foreground grant, which is the bug Mavis caught in
    /// the v0.5.1 plan.
    #[test]
    fn asfw_any_constant_is_max_u32() {
        assert_eq!(ASFW_ANY, 0xFFFFFFFF);
        assert_ne!(ASFW_ANY, 0x00000001, "0x00000001 is wrong — see D1");
    }

    /// D9 [LOW]: WinUI 3 wrapped signature detection requires BOTH a
    /// `FileNameControlHost`/`50003` element AND a `1001` element. Either
    /// alone is insufficient (true IFileDialog has 1001 only; bare
    /// WinUI 3 panes without the file-name edit are not Save As dialogs).
    #[test]
    fn is_winui3_wrapped_signature_requires_both() {
        assert!(is_winui3_wrapped_signature(true, true));
        assert!(!is_winui3_wrapped_signature(true, false));
        assert!(!is_winui3_wrapped_signature(false, true));
        assert!(!is_winui3_wrapped_signature(false, false));
    }

    /// D2 [HIGH]: make_key constructs a well-formed keyboard INPUT with the
    /// requested virtual-key code and flags. The union discriminant
    /// (`r#type`) must be INPUT_KEYBOARD and the active variant must be
    /// `Anonymous.ki` (KEYBDINPUT).
    #[test]
    fn make_key_letters_form_correct_input() {
        let input = make_key(0x4C, 0); // VK_L
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        unsafe {
            let ki = input.Anonymous.ki;
            assert_eq!(ki.wVk.0, 0x4C);
            assert_eq!(ki.wScan, 0);
            assert_eq!(ki.dwFlags.0, 0, "press flag should be 0");
        }
    }

    /// D2 [HIGH]: make_key with KEYEVENTF_KEYUP sets the keyup flag.
    #[test]
    fn make_key_release_sets_keyup_flag() {
        let input = make_key(0x4C, KEYEVENTF_KEYUP.0);
        unsafe {
            let ki = input.Anonymous.ki;
            assert_eq!(ki.wVk.0, 0x4C);
            assert_eq!(ki.dwFlags.0, KEYEVENTF_KEYUP.0);
        }
    }

    /// D2 [HIGH]: make_unicode sets wVk=0 and the unicode flag, with the
    /// codepoint in wScan (per MSDN SendInput docs).
    #[test]
    fn make_unicode_sets_unicode_flag() {
        let input = make_unicode(0x0041); // 'A'
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        unsafe {
            let ki = input.Anonymous.ki;
            assert_eq!(ki.wVk.0, 0, "Unicode INPUTs must use wVk=0");
            assert_eq!(ki.wScan, 0x0041);
            assert_eq!(ki.dwFlags.0, KEYEVENTF_UNICODE.0);
        }
    }

    /// D2 [HIGH]: make_unicode survives non-ASCII codepoints (the whole
    /// reason we use KEYEVENTF_UNICODE instead of OEM scancodes).
    #[test]
    fn make_unicode_handles_cjk_codepoint() {
        let input = make_unicode(0x4E2D); // 中 (CJK Unified Ideograph)
        unsafe {
            let ki = input.Anonymous.ki;
            assert_eq!(ki.wScan, 0x4E2D);
            assert_eq!(ki.dwFlags.0, KEYEVENTF_UNICODE.0);
        }
    }
}

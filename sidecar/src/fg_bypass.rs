// Apache License, Version 2.0 — Copyright QwenLM contributors
// Ported from https://github.com/QwenLM/qwen-code/blob/main/packages/cua-driver/rust/crates/platform-windows/src/uia/fg_bypass.rs
// Original file: fg_bypass.rs (129 lines, Apache-2.0 licensed)
//
// Adaptations from original:
// - windows 0.58 → 0.62: `HWND::is_null()` semantics preserved (raw pointer cast `0 as *mut _`
//   is null in either version); 0.62 uses `HWND::is_invalid()` for HWND validation
// - Dropped `crate::input::is_xaml_host_hwnd` (QwenLM internal). Per handoff §6.3:
//   "sidecar 实际场景中飞书/钉钉/微信 不是 UWP" — UWP/XAML branch removed
// - Local minimal `is_chromium_target_window(hwnd) -> bool`:
//   `get_class_name(hwnd).starts_with("Chrome_WidgetWin_")`
// - `host_hwnd: isize` → `host_hwnd: u32` (sidecar convention)
// - `run_with_uwp_bypass` → `run_with_bypass` (no UWP branch)
//
// Empirical evidence (QwenLM comment, 2026-05-24):
//   - Baseline UIA Invoke against UWP Calculator: 91% foreground drops
//   - With this bypass: 0/507 z-drops across Calculator, Clock, Settings
//   - Chromium / Electron (`Chrome_WidgetWin_*`): 7/8 background ax-bg stole focus
//     before bypass → 0 z-drops after.
//
// See handoff §6.3 for detailed adaptation notes.

//! Foreground-steal bypass for Chromium / Electron hosts during UIA `Invoke` calls.
//!
//! Chromium hosts (`Chrome_WidgetWin_*`) self-foreground during UIA `InvokePattern.Invoke`
//! (their Invoke handler calls `SetForegroundWindow(self)`, stealing focus from the user).
//!
//! Wrapping the call in `EnableWindow(host, FALSE) / call / EnableWindow(host, TRUE)` silently
//! suppresses the self-activation while letting the UIA pattern call still execute — UIA pattern
//! delivery uses the kernel accessibility channel, not the input queue gated by `EnableWindow`.
//!
//! Non-Chromium classic Win32 apps don't exhibit this bug. The bypass is therefore gated on
//! `is_chromium_target_window` and is a no-op for other hosts.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, GetClassNameW};

/// RAII guard that disables a window on construction and restores its
/// previous enabled-state on Drop. Always re-arms on Drop even if the
/// wrapped action panics.
pub struct DisabledHwndGuard {
    hwnd: HWND,
    was_enabled: bool,
    armed: bool,
}

impl DisabledHwndGuard {
    /// Disable `hwnd` for the lifetime of the guard. No-op for null HWND.
    pub fn disable(hwnd: HWND) -> Self {
        if hwnd.0.is_null() {
            return Self {
                hwnd,
                was_enabled: false,
                armed: false,
            };
        }
        // `EnableWindow` returns nonzero iff the window was *previously
        // disabled* — invert to get the "was enabled" state we want to
        // restore at Drop time.
        let was_disabled = unsafe { EnableWindow(hwnd, false).as_bool() };
        Self {
            hwnd,
            was_enabled: !was_disabled,
            armed: true,
        }
    }
}

impl Drop for DisabledHwndGuard {
    fn drop(&mut self) {
        if self.armed {
            unsafe {
                let _ = EnableWindow(self.hwnd, self.was_enabled);
            }
        }
    }
}

/// Wrap an activation closure (`InvokePattern::Invoke`) in a Chromium foreground-steal bypass.
///
/// `host_hwnd` is the top-level HWND of the window containing the target UIA element.
/// When it identifies as a Chromium host (`is_chromium_target_window`), the HWND is disabled
/// for the duration of `action`. For non-Chromium hosts the closure runs unmodified — those
/// don't self-foreground via the input-queue path that `EnableWindow` gates.
pub fn run_with_bypass<T>(host_hwnd: u32, action: impl FnOnce() -> T) -> T {
    let _guard = make_guard(host_hwnd);
    action()
}

fn make_guard(host_hwnd: u32) -> Option<DisabledHwndGuard> {
    if host_hwnd == 0 {
        return None;
    }
    // Chromium/Electron hosts (`Chrome_WidgetWin_*`) self-foreground during UIA pattern
    // handling: their UIA `InvokePattern.Invoke` handler reaches the browser's focus path
    // and calls `SetForegroundWindow(self)`, stealing focus from the user's window on a
    // *background* click. `WS_EX_NOACTIVATE` (the injection path's `NoActivateGuard`) does
    // NOT stop an explicit self-`SetForegroundWindow`, but the `EnableWindow` shield does:
    // a *disabled* top-level cannot be made the foreground window, while the UIA Invoke
    // still lands (delivered over the kernel accessibility channel, not the input queue).
    let shielded = is_chromium_target_window(host_hwnd);
    if !shielded {
        return None;
    }
    let h = HWND(host_hwnd as *mut core::ffi::c_void);
    // Defensive: walk up to the root in case the caller handed us a child
    // HWND inside the Chromium host's HWND tree.
    let root = unsafe { GetAncestor(h, GA_ROOT) };
    let target = if root.0.is_null() { h } else { root };
    Some(DisabledHwndGuard::disable(target))
}

/// Minimal Chromium host detection by window class name prefix.
fn is_chromium_target_window(hwnd: u32) -> bool {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    let class = get_class_name(h);
    class.starts_with("Chrome_WidgetWin_")
}

fn get_class_name(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf);
        match usize::try_from(len) {
            Ok(n) if n > 0 => String::from_utf16_lossy(&buf[..n]),
            _ => String::new(),
        }
    }
}
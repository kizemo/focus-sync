//! Path writer: UIA `ValuePattern::SetValue` + `InvokePattern::Invoke` (via uia_inject),
//! wrapped in `fg_bypass::run_with_bypass` for Chromium hosts.
//!
//! Strategy: `uia_inject::inject_folder_path` handles the UIA work; we add the
//! foreground-bypass shield for Chromium targets before the UIA call.

use tracing::info;

#[derive(Debug)]
pub enum WriteOutcome {
    UiaSetValue,
    SendInputFallback,
    Failed(String),
}

/// Write `target` to the file dialog at `hwnd_isize`. Wraps the UIA injection in
/// `fg_bypass::run_with_bypass` — Chromium hosts are temporarily disabled during
/// the Invoke call to prevent focus theft from the user.
pub fn write_path(hwnd_isize: isize, target: &str) -> WriteOutcome {
    if hwnd_isize == 0 {
        return WriteOutcome::Failed("hwnd is 0".into());
    }
    let hwnd_u32 = hwnd_isize as u32;
    info!("write_path: hwnd={hwnd_isize} target='{target}'");

    // The fg_bypass shield is a no-op for non-Chromium hosts (see fg_bypass::is_chromium_target_window).
    // For Chromium hosts it wraps the UIA Invoke in EnableWindow(FALSE) so the host can't steal focus.
    crate::fg_bypass::run_with_bypass(hwnd_u32, || {
        crate::uia_inject::inject_folder_path(hwnd_u32, target);
    });

    // Distinguish strategy: uia_inject logs internally; we assume UiaSetValue succeeded if no error returned.
    // For the sidecar's first integration we mark UiaSetValue as default. Detailed strategy tracking
    // happens when uia_inject returns Result<Strategy>.
    WriteOutcome::UiaSetValue
}
// MIT License — Copyright (c) inaku-Gyan
// Ported from https://github.com/inaku-Gyan/PathWrap/blob/main/src/os/dialog.rs
// Original file: src/os/dialog.rs (176 lines, MIT licensed)
//
// Adaptations from original:
// - `dialog_hwnd: isize` → `dialog_hwnd: u32` (spike convention per old plan)
// - `HWND(hwnd_isize as *mut core::ffi::c_void)` — windows 0.62 real construction
// - `BSTR::from(target_path)` — direct (PathWrap already used this)
// - Score functions (`filename_edit_score` / `confirm_button_score`): ported verbatim
// - `fallback_confirm`: kept `SendMessageW(native, WM_KEYDOWN, ...)` — NOT SendInput
//
// See handoff §6.1 for detailed adaptation notes.

//! UIA path injection: drive a file dialog to a specific folder via UI Automation.
//!
//! Compared to legacy implementations (CDM messages / WM_SETTEXT / per-char keyboard
//! simulation + hard sleep), the UIA approach:
//! - Locates the filename input box directly and writes the full path via `ValuePattern::SetValue`;
//! - Then clicks the default "Open/Save" button via `InvokePattern::Invoke`;
//! - Fully synchronous cross-process COM calls — no sleep, no key simulation,
//!   does not steal focus, and reliable on modern `IFileDialog`.

use tracing::{error, info, warn};
use std::cell::RefCell;
use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationValuePattern, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_EditControlTypeId, UIA_InvokePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_KEYDOWN, WM_KEYUP};
use windows::core::{BSTR, Error, Result};

const VK_RETURN: usize = 0x0D;

/// Inject `target_path` into a file dialog and trigger confirmation.
/// Logs only — failure does not panic.
pub fn inject_folder_path(dialog_hwnd: u32, target_path: &str) {
    if dialog_hwnd == 0 {
        error!("invalid dialog hwnd: 0");
        return;
    }
    if target_path.trim().is_empty() {
        error!("target path is empty");
        return;
    }

    info!("Injection starts: hwnd={dialog_hwnd} target='{target_path}'");
    match inject_via_uia(dialog_hwnd, target_path) {
        Ok(()) => info!("Injection succeeded via UI Automation."),
        Err(err) => warn!("Injection failed via UI Automation: {err}"),
    }
}

/// Reuse the main-thread `IUIAutomation` instance (COM interfaces are `!Send`,
/// thread-local fits the constraint perfectly).
fn automation() -> Option<IUIAutomation> {
    thread_local! {
        static INSTANCE: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    }

    INSTANCE.with(|cell| {
        if let Some(existing) = cell.borrow().as_ref() {
            return Some(existing.clone());
        }
        // Caller must have initialized the main thread as STA (OleInitialize).
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

fn inject_via_uia(dialog_hwnd: u32, target_path: &str) -> Result<()> {
    let uia = automation().ok_or_else(|| Error::from(E_FAIL))?;
    let dialog = HWND(dialog_hwnd as *mut core::ffi::c_void);

    let root = unsafe { uia.ElementFromHandle(dialog)? };
    let condition = unsafe { uia.CreateTrueCondition()? };
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition)? };
    let count = unsafe { elements.Length()? };

    let mut best_edit: Option<IUIAutomationElement> = None;
    let mut best_edit_score = i32::MIN;
    let mut best_button: Option<IUIAutomationElement> = None;
    let mut best_button_score = i32::MIN;

    for i in 0..count {
        let element = unsafe { elements.GetElement(i)? };
        let control_type = unsafe { element.CurrentControlType()? };

        if control_type == UIA_EditControlTypeId {
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

    let edit = best_edit.ok_or_else(|| Error::from(E_FAIL))?;
    let value: IUIAutomationValuePattern = unsafe { edit.GetCurrentPatternAs(UIA_ValuePatternId)? };
    unsafe { value.SetValue(&BSTR::from(target_path))? };

    // Prefer clicking the default button; fall back to sending Enter to the filename box.
    match best_button {
        Some(button) if best_button_score > 0 => {
            let invoke: IUIAutomationInvokePattern =
                unsafe { button.GetCurrentPatternAs(UIA_InvokePatternId)? };
            unsafe { invoke.Invoke()? };
        }
        _ => fallback_confirm(&edit),
    }

    Ok(())
}

/// Score candidate filename/address input: AutomationId hits highest, then name.
fn filename_edit_score(element: &IUIAutomationElement) -> i32 {
    let automation_id = current_automation_id(element);
    if automation_id == "1148" {
        return 100; // Modern IFileDialog filename combo box
    }
    if automation_id == "1001" {
        return 90; // Legacy dialog filename Edit
    }

    let name = current_name(element).to_lowercase();
    if name.contains("文件名") || name.contains("file name") {
        return 80;
    }
    // Still a candidate, lowest priority (could be address bar / search box).
    1
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
    i32::MIN + 1 // Button with no matching text — not a default confirm target
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

/// Fallback confirmation: send Enter to the filename box's native window handle
/// (no sleep, no SendInput).
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
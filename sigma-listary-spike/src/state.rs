//! In-memory state: current Sigma path + active file dialog registry + health tracking.
//!
//! Per handoff §6.5: `current_path` is `Mutex<String>` (not `String`),
//! accessed via `set_current_path(&self, p)` + `get_current_path(&self) -> String`.
//!
//! v0.3.0 (focus-sync Scheduled Task migration): added health tracking fields
//! surfaced via `/health` HTTP endpoint for Sigma FM extension monitoring:
//! - `startup_time: Instant`        — uptime calculation
//! - `last_ping_received_ts: AtomicU64` — last `/set_path` (Unix millis, 0 if never)
//! - `last_dialog_event_ts: AtomicU64`  — last `DialogEvent::Opened` (Unix millis)
//! - `uia_healthy: AtomicBool`           — set false if UIA self-check fails 3x
//! - `session_id: u32`                   — set once at startup (ProcessIdToSessionId)

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct DialogInfo {
    pub hwnd: u32,
    pub app: String,
    pub last_known_path: String,
    pub write_strategy: Option<String>,
}

#[derive(Debug)]
pub struct AppState {
    pub current_path: Mutex<String>,
    pub registry: Mutex<HashMap<u32, DialogInfo>>,

    // Health tracking (v0.3.0)
    pub startup_time: Instant,
    pub last_ping_received_ts: AtomicU64,
    pub last_dialog_event_ts: AtomicU64,
    pub uia_healthy: AtomicBool,
    pub session_id: u32,

    // focus-18: per-dialog single-shot commit dedup.
    // Semantics: try_mark_committed(hwnd) -> true if first time (may commit),
    //            false if already committed (skip Invoke).
    // clear_committed(hwnd) -> on DialogEvent::Closed, allow next dialog commit.
    pub committed_hwnds: Mutex<HashSet<u32>>,

    // focus-19 (2026-10-02): counter of "dialogs we could not navigate".
    // Incremented by uia_inject::inject_folder_path when the dialog is
    // neither a native IFileDialog (COM SetFolder succeeded) nor a Chromium
    // Save As (41477 address bar). Typical cause: Edge Win11 22H2+ WinUI
    // Save As, which is unsupported by the current detection logic.
    //
    // Surfaced via /health so the extension can poll and notify the user
    // (deep-analysis §4.2 Option B + §8 task 5). Note: count is monotonic
    // for the life of the process; we do NOT rate-limit here. The extension
    // can throttle notifications by tracking last-seen count.
    pub unsupported_dialog_count: AtomicU64,

    // v0.5.5 (Mavis Round 21 B-1 + Round 23 N-1 + Round 25 Q-5): boolean
    // flag tracking whether extension has pushed at least one path. Until
    // this is true, opened_write_back MUST skip — otherwise it'd write the
    // init sentinel "C:\\" to every new dialog (race condition observed in
    // spike.log 06:41:12 etc.). Using a boolean flag instead of string
    // equality (e.g. `current != "C:\\"`) avoids false negatives when user
    // legitimately navigates to "C:\\" root.
    pub first_push_received: AtomicBool,
}

impl AppState {
    pub fn new(initial_path: String, session_id: u32) -> Self {
        Self {
            current_path: Mutex::new(initial_path),
            registry: Mutex::new(HashMap::new()),
            startup_time: Instant::now(),
            last_ping_received_ts: AtomicU64::new(0),
            last_dialog_event_ts: AtomicU64::new(0),
            uia_healthy: AtomicBool::new(true), // optimistic; self-check may flip
            session_id,
            committed_hwnds: Mutex::new(HashSet::new()),
            unsupported_dialog_count: AtomicU64::new(0),
            first_push_received: AtomicBool::new(false),
        }
    }

    /// focus-18: atomically "mark + report whether first time". Returns true if
    /// `hwnd` has never been committed (this caller MAY commit), returns false
    /// if already committed (skip commit). HashSet::insert returns bool:
    /// `true` = newly inserted, `false` = already present.
    pub fn try_mark_committed(&self, hwnd: u32) -> bool {
        let mut set = self.committed_hwnds.lock().unwrap_or_else(|e| e.into_inner());
        set.insert(hwnd)
    }

    /// focus-18: called from DialogEvent::Closed. Clears the commit mark for
    /// `hwnd`, allowing the next dialog (or re-spawn of the same hwnd) to commit.
    pub fn clear_committed(&self, hwnd: u32) {
        let mut set = self.committed_hwnds.lock().unwrap_or_else(|e| e.into_inner());
        set.remove(&hwnd);
    }

    pub fn set_current_path(&self, p: String) {
        let mut guard = self.current_path.lock().unwrap_or_else(|e| e.into_inner());
        *guard = p;
        // Track that we received a ping — used by /health.
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.last_ping_received_ts.store(ts, Ordering::Relaxed);
        // v0.5.5 (Mavis Round 21 B-1 + Round 23 N-1): defense in depth.
        // Always mark first_push_received when path is set, in case future
        // callers skip the explicit mark_first_push_received() call.
        // Idempotent: store(true) on an already-true flag is a no-op.
        self.first_push_received.store(true, Ordering::SeqCst);
    }

    /// v0.5.5: idempotent mark of first push. Called from http_server
    /// set_path early-return branch (path_unchanged) where set_current_path
    /// is not invoked.
    pub fn mark_first_push_received(&self) {
        self.first_push_received.store(true, Ordering::SeqCst);
    }

    /// v0.5.5: query for opened_write_back guard and /health exposure.
    pub fn has_first_push_received(&self) -> bool {
        self.first_push_received.load(Ordering::SeqCst)
    }

    /// v0.5.5 (Mavis Round 25 N-4): single source of truth for "should we
    /// write state.current_path to this new dialog?". Production code in
    /// main.rs:352 and unit tests both call this function, so test
    /// changes if production logic changes.
    pub fn should_opened_write_back(&self, initial_path: &str, current_path: &str) -> bool {
        !initial_path.is_empty()
            && initial_path != current_path
            && self.has_first_push_received()
    }

    pub fn get_current_path(&self) -> String {
        let guard = self.current_path.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }

    pub fn register_dialog(&self, info: DialogInfo) {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.insert(info.hwnd, info);
        // Track dialog event for /health.
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.last_dialog_event_ts.store(ts, Ordering::Relaxed);
    }

    pub fn remove_dialog(&self, hwnd: u32) -> Option<DialogInfo> {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.remove(&hwnd)
    }

    pub fn active_dialogs(&self) -> Vec<DialogInfo> {
        let reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.values().cloned().collect()
    }

    /// UIA self-check health setter. Called periodically by main loop.
    pub fn set_uia_healthy(&self, healthy: bool) {
        self.uia_healthy.store(healthy, Ordering::Relaxed);
    }

    /// focus-19 (2026-10-02): bump the unsupported-dialog counter. Called by
    /// `uia_inject::inject_folder_path` when the dialog type is neither
    /// IFileDialog (COM) nor Chromium-41477 (UIA). The counter is
    /// monotonic — there is no decrement path. The extension polls this
    /// via `/health` to surface a notification to the user.
    pub fn inc_unsupported_dialog(&self) {
        self.unsupported_dialog_count
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot for /health endpoint.
    pub fn health_snapshot(&self) -> HealthSnapshot {
        HealthSnapshot {
            uptime_secs: self.startup_time.elapsed().as_secs(),
            last_ping_received_ts: self.last_ping_received_ts.load(Ordering::Relaxed),
            last_dialog_event_ts: self.last_dialog_event_ts.load(Ordering::Relaxed),
            active_dialogs: self.active_dialogs().len(),
            uia_healthy: self.uia_healthy.load(Ordering::Relaxed),
            session_id: self.session_id,
            unsupported_dialog_count: self.unsupported_dialog_count.load(Ordering::Relaxed),
            // v0.5.5 (Mavis Round 21 B-4): expose first_push_received for
            // extension UX (toolbar can show "⚠️ Sigma FM 未推送" if false).
            first_push_received: self.first_push_received.load(Ordering::SeqCst),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthSnapshot {
    pub uptime_secs: u64,
    pub last_ping_received_ts: u64,
    pub last_dialog_event_ts: u64,
    pub active_dialogs: usize,
    pub uia_healthy: bool,
    pub session_id: u32,
    /// focus-19: monotonic counter of dialogs that could not be navigated
    /// (WinUI Save As / unknown types). Surfaced via `/health` for the
    /// extension's user-facing notification.
    pub unsupported_dialog_count: u64,
    /// v0.5.5 (Mavis Round 21 B-4): boolean exposure for extension UX.
    /// Extension reads this to detect "sidecar is waiting for PUSH" and
    /// show user a "Sync Now?" notification.
    pub first_push_received: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn register_and_remove() {
        let state = AppState::new("C:\\".into(), 1);
        state.register_dialog(DialogInfo {
            hwnd: 1,
            app: "chrome".into(),
            last_known_path: "C:\\Users\\foo".into(),
            write_strategy: None,
        });
        assert_eq!(state.active_dialogs().len(), 1);

        let removed = state.remove_dialog(1).unwrap();
        assert_eq!(removed.app, "chrome");
        assert_eq!(state.active_dialogs().len(), 0);
    }

    #[test]
    fn current_path_updates() {
        let state = AppState::new("C:\\".into(), 1);
        assert_eq!(state.get_current_path(), "C:\\");
        state.set_current_path("D:\\new".into());
        assert_eq!(state.get_current_path(), "D:\\new");
    }

    #[test]
    fn health_snapshot_initial() {
        let state = AppState::new("C:\\".into(), 2);
        let snap = state.health_snapshot();
        assert_eq!(snap.session_id, 2);
        assert_eq!(snap.active_dialogs, 0);
        assert_eq!(snap.last_ping_received_ts, 0);
        assert_eq!(snap.last_dialog_event_ts, 0);
        assert!(snap.uia_healthy); // default true
        assert!(snap.uptime_secs < 5);
    }

    #[test]
    fn set_current_path_records_ping_ts() {
        let state = AppState::new("C:\\".into(), 1);
        state.set_current_path("D:\\".into());
        let snap = state.health_snapshot();
        assert!(snap.last_ping_received_ts > 0, "ping ts should be recorded");
    }

    #[test]
    fn register_dialog_records_event_ts() {
        let state = AppState::new("C:\\".into(), 1);
        state.register_dialog(DialogInfo {
            hwnd: 1,
            app: "chrome".into(),
            last_known_path: "".into(),
            write_strategy: None,
        });
        let snap = state.health_snapshot();
        assert!(snap.last_dialog_event_ts > 0);
        assert_eq!(snap.active_dialogs, 1);
    }

    #[test]
    fn set_uia_healthy_toggles() {
        let state = AppState::new("C:\\".into(), 1);
        assert!(state.health_snapshot().uia_healthy);
        state.set_uia_healthy(false);
        assert!(!state.health_snapshot().uia_healthy);
        state.set_uia_healthy(true);
        assert!(state.health_snapshot().uia_healthy);
    }

    #[test]
    fn uptime_advances() {
        let state = AppState::new("C:\\".into(), 1);
        let s1 = state.health_snapshot().uptime_secs;
        std::thread::sleep(Duration::from_millis(50));
        let s2 = state.health_snapshot().uptime_secs;
        assert!(s2 >= s1);
    }

    // focus-18: per-dialog single-shot dedup.
    #[test]
    fn try_mark_committed_returns_true_first_time() {
        let state = AppState::new("C:\\".into(), 1);
        assert!(state.try_mark_committed(12345));
        assert!(state.try_mark_committed(67890)); // different hwnd also first-time
    }

    #[test]
    fn try_mark_committed_returns_false_subsequent_calls() {
        let state = AppState::new("C:\\".into(), 1);
        assert!(state.try_mark_committed(12345));
        assert!(!state.try_mark_committed(12345));
        assert!(!state.try_mark_committed(12345));
    }

    #[test]
    fn clear_committed_resets_state() {
        let state = AppState::new("C:\\".into(), 1);
        assert!(state.try_mark_committed(12345));
        assert!(!state.try_mark_committed(12345));
        state.clear_committed(12345);
        assert!(state.try_mark_committed(12345)); // reset -> can commit again
    }

    // focus-19 (2026-10-02): unsupported_dialog_count is monotonic and
    // surfaced via /health for the extension's notification path.
    #[test]
    fn unsupported_count_starts_at_zero() {
        let state = AppState::new("C:\\".into(), 1);
        assert_eq!(state.health_snapshot().unsupported_dialog_count, 0);
    }

    #[test]
    fn inc_unsupported_dialog_is_monotonic() {
        let state = AppState::new("C:\\".into(), 1);
        state.inc_unsupported_dialog();
        state.inc_unsupported_dialog();
        state.inc_unsupported_dialog();
        assert_eq!(state.health_snapshot().unsupported_dialog_count, 3);
        // No decrement path: 100 inc's → count == 100.
        for _ in 0..97 {
            state.inc_unsupported_dialog();
        }
        assert_eq!(state.health_snapshot().unsupported_dialog_count, 100);
    }
}
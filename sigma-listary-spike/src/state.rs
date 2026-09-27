//! In-memory state: current Sigma path + active file dialog registry.
//!
//! Per handoff §6.5: `current_path` is `Mutex<String>` (not `String`),
//! accessed via `set_current_path(&self, p)` + `get_current_path(&self) -> String`.

use std::collections::HashMap;
use std::sync::Mutex;

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
}

impl AppState {
    pub fn new(initial_path: String) -> Self {
        Self {
            current_path: Mutex::new(initial_path),
            registry: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_current_path(&self, p: String) {
        let mut guard = self.current_path.lock().unwrap_or_else(|e| e.into_inner());
        *guard = p;
    }

    pub fn get_current_path(&self) -> String {
        let guard = self.current_path.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }

    pub fn register_dialog(&self, info: DialogInfo) {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.insert(info.hwnd, info);
    }

    pub fn remove_dialog(&self, hwnd: u32) -> Option<DialogInfo> {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.remove(&hwnd)
    }

    pub fn active_dialogs(&self) -> Vec<DialogInfo> {
        let reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_remove() {
        let state = AppState::new("C:\\".into());
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
        let state = AppState::new("C:\\".into());
        assert_eq!(state.get_current_path(), "C:\\");
        state.set_current_path("D:\\new".into());
        assert_eq!(state.get_current_path(), "D:\\new");
    }
}
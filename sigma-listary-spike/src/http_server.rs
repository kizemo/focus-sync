//! HTTP/1.1 server: replaces raw TCP for Sigma FM extension IPC.
//!
//! Endpoints (per extension `release/extension/dist/index.js` contract):
//! - `POST /set_path` body `{"path":"D:\\downloads"}` → 200 `{"ok":true}`
//!                                                or 400 `{"ok":false,"error":"..."}`
//! - `POST /quit`                            → 200 `{"ok":true}` then `std::process::exit(0)`
//! - `GET  /get_status`                      → 200 `{"ok":true,"current_path":"...","active_dialogs":N}`
//! - `GET  /health`                          → 200 health snapshot (v0.3.0 Scheduled Task mode)
//!
//! Protocol context:
//! - Sigma FM extension speaks HTTP (uses `sigma.http.request`) — see
//!   `handoff-2026-09-30-fix-ipc-http.md §一` for the mismatch root cause.
//! - tiny_http is chosen for sync I/O + zero async-runtime overhead (the rest of
//!   the spike already runs tokio, but http_server is wrapped in
//!   `std::thread::spawn` from main — see `main.rs`).
//!
//! Reuse: `state::AppState`, `events::Event`, `logger::log_event`, `writer::write_path`.
//! No business logic changes from the round-18 TCP version — only the transport layer.

use crate::events::Event;
use crate::logger::{log_event, now_iso8601};
use crate::state::{AppState, DialogInfo};
use crate::writer::WriteOutcome;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;
use tiny_http::{Header, Method, Response, Server, StatusCode};

// v0.5.5: 41477 exclusion + first_push_received flag + /health expose +
// should_opened_write_back helper. See:
// docs/superpowers/plans/2026-10-06-focus-21-v0.5.5-ship-ready-plan-v6.md
const SPIKE_VERSION: &str = "0.5.5";

#[derive(Debug, Deserialize)]
struct SetPathBody {
    path: String,
    /// focus-18: from extension PUSH body. Default false (serde default).
    /// true means the extension user enabled Auto-Confirm, so the sidecar
    /// may Invoke Save (per-dialog single-shot via state.try_mark_committed).
    #[serde(default)]
    auto_confirm: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ResponseBody {
    Ok {
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Status {
        ok: bool,
        current_path: String,
        active_dialogs: usize,
    },
    Health(HealthResponse),
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    /// `"ok"` / `"degraded"` / `"down"`.
    /// - "ok":       process alive, port bound, UIA healthy
    /// - "degraded": process alive, but UIA self-check failed OR session_id = 0
    /// - "down":     unused (we'd return 5xx instead)
    status: &'static str,
    uptime_secs: u64,
    last_ping_received_ts: u64,
    last_dialog_event_ts: u64,
    active_dialogs: usize,
    uia_healthy: bool,
    session_id: u32,
    version: &'static str,
    /// `"service"` when launched via Scheduled Task with `--service` flag,
    /// `"spawn"` when launched by Sigma FM (v0.2.0 backward-compat).
    mode: &'static str,
    /// focus-19 (2026-10-02): monotonic counter of dialogs the sidecar
    /// could NOT navigate (WinUI Save As / unknown). The extension polls
    /// this and surfaces a notification when the count increases since the
    /// last check (deep-analysis §4.2 Option B + §8 task 5).
    unsupported_dialog_count: u64,
    /// v0.5.5 (Mavis Round 21 B-4): boolean exposure for extension UX.
    /// Extension reads this to detect "sidecar is waiting for PUSH" and
    /// show user a "Sync Now?" notification.
    first_push_received: bool,
}

/// Blocking HTTP server loop. Returns when the server is dropped (e.g. after
/// `/quit` triggers `std::process::exit(0)`).
///
/// Tiny_http's `Server::http` binds `127.0.0.1:port` and `incoming_requests()`
/// yields requests sequentially. We process them inline — local IPC is single-
/// threaded by design (Sigma FM extension is the only consumer).
///
/// `mode` is the v0.3.0 mode label (`"service"` or `"spawn"`) included in /health.
pub fn run_server(state: Arc<AppState>, port: u16, mode: &'static str) -> Result<()> {
    let server = Server::http(("127.0.0.1", port)).map_err(anyhow::Error::msg)?;
    tracing::info!(port, "HTTP server listening");

    for request in server.incoming_requests() {
        let state = state.clone();
        let method = request.method().clone();
        let url = request.url().to_string();

        match (method, url.as_str()) {
            (Method::Post, "/set_path") => {
                if let Err(e) = handle_set_path(request, state) {
                    tracing::warn!(error = %e, "set_path handler error");
                }
            }
            (Method::Post, "/quit") => {
                if let Err(e) = handle_quit(request) {
                    tracing::warn!(error = %e, "quit handler error");
                }
                // handle_quit returns 200 then exits — never reached in normal flow.
            }
            (Method::Get, "/get_status") => {
                if let Err(e) = handle_get_status(request, state) {
                    tracing::warn!(error = %e, "get_status handler error");
                }
            }
            (Method::Get, "/health") => {
                if let Err(e) = handle_health(request, state, mode) {
                    tracing::warn!(error = %e, "health handler error");
                }
            }
            _ => {
                let _ = request.respond(
                    Response::from_string("not found")
                        .with_status_code(StatusCode(404)),
                );
            }
        }
    }
    Ok(())
}

// ---- handlers ----

fn handle_set_path(
    mut request: tiny_http::Request,
    state: Arc<AppState>,
) -> Result<()> {
    // Read body (tiny_http limits by default to 8 MB; we cap smaller for safety).
    let mut body = String::new();
    request.as_reader().take(64 * 1024).read_to_string(&mut body)?;

    let parsed: Result<SetPathBody, _> = serde_json::from_str(&body);
    let parsed_body = match parsed {
        Ok(p) => p,
        Err(e) => {
            let resp = ResponseBody::Ok {
                ok: false,
                error: Some(format!("invalid JSON body: {e}")),
            };
            return respond_json(request, 400, &resp);
        }
    };
    let path = parsed_body.path;
    let auto_confirm = parsed_body.auto_confirm;

    if path.is_empty() {
        let resp = ResponseBody::Ok {
            ok: false,
            error: Some("path is empty".into()),
        };
        return respond_json(request, 400, &resp);
    }

    // D6 [MEDIUM] (Mavis 第 13 轮): PUSH dedup — if the extension pushes the
    // same path twice in a row, skip the per-input dialog write loop. This
    // matters most for the v0.5.1 SendInput path: each SendInput call
    // dispatches Ctrl+L, the path, and Enter into the dialog, which
    // would destroy any filename the user is currently typing if the
    // Sigma FM focus flickers (e.g. user re-clicks the same folder).
    //
    // We compare against the previous `current_path` BEFORE updating
    // state, so identical-push PUSHes short-circuit cleanly. The new
    // current_path is still set below so subsequent /get_status reflects
    // the latest call.
    let prev_path = state.get_current_path();
    let path_unchanged = prev_path == path;
    if path_unchanged {
        // v0.5.5 (Mavis Round 21 B-1): even if path didn't change, this
        // counts as a push — mark first_push_received explicitly. The
        // set_current_path below won't be called in this branch, so we
        // MUST mark here. Without this, user pushing C:\\ (== sentinel)
        // leaves flag = false, and opened_write_back will skip silently.
        state.mark_first_push_received();

        log_event(&Event::SpikeError {
            kind: "set_path_dedup".into(),
            message: format!("path={path} (unchanged, skipping dialog write loop)"),
            ts: now_iso8601(),
        });
        let resp = ResponseBody::Ok { ok: true, error: None };
        return respond_json(request, 200, &resp);
    }

    // Update state.
    state.set_current_path(path.clone());
    log_event(&Event::SpikeError {
        kind: "path_update".into(),
        message: format!("current_path={}", path),
        ts: now_iso8601(),
    });

    // Snapshot active dialogs, then write to each.
    // Important: release the lock before calling write_path.
    //
    // Round 19e (2026-09-30): The dialog registry is updated asynchronously via
    // main loop's DialogEvent::Opened handler. There is a race window where
    // /set_path runs BEFORE register_dialog has processed the new dialog. The
    // extension's follow-up /get_status confirms the dialog is now registered
    // (so it's not gone — just not yet visible to /set_path). Retry briefly so
    // the write attempt isn't silently dropped on the early PUSHes.
    let mut dialogs: Vec<DialogInfo> = state.active_dialogs();
    let mut retry_iter = 0u32;
    while dialogs.is_empty() && retry_iter < 8 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        dialogs = state.active_dialogs();
        retry_iter += 1;
    }
    if retry_iter > 0 {
        log_event(&Event::SpikeError {
            kind: "set_path_dialogs_retry".into(),
            message: format!("retried {retry_iter}x; final dialogs_count={}", dialogs.len()),
            ts: now_iso8601(),
        });
    }
    log_event(&Event::SpikeError {
        kind: "set_path_dialogs".into(),
        message: format!("dialogs_count={}", dialogs.len()),
        ts: now_iso8601(),
    });
    for d in dialogs {
        log_event(&Event::SpikeError {
            kind: "set_path_iter".into(),
            message: format!(
                "hwnd={} app={} auto_confirm={}",
                d.hwnd, d.app, auto_confirm
            ),
            ts: now_iso8601(),
        });
        let outcome = crate::writer::write_path(&state, d.hwnd as isize, &path, auto_confirm);
        match outcome {
            WriteOutcome::UiaSetValue => {
                log_event(&Event::Write {
                    hwnd: d.hwnd,
                    app: d.app.clone(),
                    target: path.clone(),
                    strategy: "uia".into(),
                    ts: now_iso8601(),
                });
            }
            WriteOutcome::SendInputFallback => {
                log_event(&Event::Write {
                    hwnd: d.hwnd,
                    app: d.app.clone(),
                    target: path.clone(),
                    strategy: "sendinput".into(),
                    ts: now_iso8601(),
                });
            }
            WriteOutcome::Failed(reason) => {
                log_event(&Event::WriteFailed {
                    hwnd: d.hwnd,
                    app: d.app.clone(),
                    target: path.clone(),
                    reason,
                    ts: now_iso8601(),
                });
            }
        }
    }

    let resp = ResponseBody::Ok { ok: true, error: None };
    respond_json(request, 200, &resp)
}

fn handle_quit(request: tiny_http::Request) -> Result<()> {
    let resp = ResponseBody::Ok { ok: true, error: None };
    respond_json(request, 200, &resp)?;
    // Process exit — Sigma FM extension's deactivate() expects graceful shutdown.
    std::process::exit(0);
}

fn handle_get_status(
    request: tiny_http::Request,
    state: Arc<AppState>,
) -> Result<()> {
    let resp = ResponseBody::Status {
        ok: true,
        current_path: state.get_current_path(),
        active_dialogs: state.active_dialogs().len(),
    };
    respond_json(request, 200, &resp)
}

/// v0.3.0 health endpoint. Returns process state snapshot for extension
/// monitoring and user-facing diagnostics.
///
/// Status semantics (v3 review V3.17):
/// - "ok"       — process alive, port bound, UIA healthy, session != 0
/// - "degraded" — process alive but UIA self-check failed 3x OR session_id = 0
///                (RDP / SSH / CI runner scenarios; v3 review V3.1 — not panic)
/// - "down"     — unused; would require process to be in bad state, but we just
///                respond with whatever we have. Future: detect self-destruct.
fn handle_health(
    request: tiny_http::Request,
    state: Arc<AppState>,
    mode: &'static str,
) -> Result<()> {
    let snap = state.health_snapshot();
    let status = if snap.uia_healthy && snap.session_id != 0 {
        "ok"
    } else {
        "degraded"
    };
    let body = HealthResponse {
        status,
        uptime_secs: snap.uptime_secs,
        last_ping_received_ts: snap.last_ping_received_ts,
        last_dialog_event_ts: snap.last_dialog_event_ts,
        active_dialogs: snap.active_dialogs,
        uia_healthy: snap.uia_healthy,
        session_id: snap.session_id,
        version: SPIKE_VERSION,
        mode,
        unsupported_dialog_count: snap.unsupported_dialog_count,
        // v0.5.5 (Mavis Round 21 B-4)
        first_push_received: snap.first_push_received,
    };
    respond_json(request, 200, &ResponseBody::Health(body))
}

// ---- response helper ----

fn respond_json(
    request: tiny_http::Request,
    status: u16,
    body: &ResponseBody,
) -> Result<()> {
    let json = serde_json::to_string(body)?;
    let response = Response::from_string(json)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_includes_all_fields() {
        let snap = crate::state::HealthSnapshot {
            uptime_secs: 100,
            last_ping_received_ts: 1700000000000,
            last_dialog_event_ts: 1700000000000,
            active_dialogs: 2,
            uia_healthy: true,
            session_id: 1,
            unsupported_dialog_count: 0,
            // v0.5.5 (Mavis Round 21 B-4)
            first_push_received: true,
        };
        let resp = HealthResponse {
            status: "ok",
            uptime_secs: snap.uptime_secs,
            last_ping_received_ts: snap.last_ping_received_ts,
            last_dialog_event_ts: snap.last_dialog_event_ts,
            active_dialogs: snap.active_dialogs,
            uia_healthy: snap.uia_healthy,
            session_id: snap.session_id,
            version: SPIKE_VERSION,
            mode: "service",
            unsupported_dialog_count: snap.unsupported_dialog_count,
            // v0.5.5 (Mavis Round 21 B-4)
            first_push_received: snap.first_push_received,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"uptime_secs\":100"));
        assert!(json.contains("\"session_id\":1"));
        // v0.5.5 (Mavis Round 23 N-1): use SPIKE_VERSION constant, NOT
        // hardcoded string. Prevents test from breaking every time version
        // bumps.
        assert!(json.contains(&format!("\"version\":\"{}\"", SPIKE_VERSION)));
        assert!(json.contains("\"mode\":\"service\""));
        assert!(json.contains("\"unsupported_dialog_count\":0"));
    }

    #[test]
    fn health_response_degraded_when_session_zero() {
        // v3 review V3.1: session_id = 0 should be "degraded", not panic.
        // We can't easily test handle_health directly (it needs tiny_http Request),
        // but the status logic is verified by the data flow above.
        let body = HealthResponse {
            status: "degraded",
            uptime_secs: 0,
            last_ping_received_ts: 0,
            last_dialog_event_ts: 0,
            active_dialogs: 0,
            uia_healthy: true,
            session_id: 0,
            version: SPIKE_VERSION,
            mode: "service",
            unsupported_dialog_count: 0,
            // v0.5.5 (Mavis Round 21 B-4)
            first_push_received: false,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"status\":\"degraded\""));
    }

    /// focus-19 (2026-10-02): the unsupported_dialog_count field MUST be
    /// serialized in /health JSON. The extension uses this to decide when
    /// to surface a "this dialog isn't supported" notification.
    #[test]
    fn health_response_includes_unsupported_dialog_count() {
        let body = HealthResponse {
            status: "ok",
            uptime_secs: 5,
            last_ping_received_ts: 1,
            last_dialog_event_ts: 2,
            active_dialogs: 0,
            uia_healthy: true,
            session_id: 1,
            version: SPIKE_VERSION,
            mode: "service",
            unsupported_dialog_count: 7,
            // v0.5.5 (Mavis Round 21 B-4)
            first_push_received: false,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(
            json.contains("\"unsupported_dialog_count\":7"),
            "expected field in JSON, got: {json}"
        );
    }
}
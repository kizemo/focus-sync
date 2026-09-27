//! HTTP server: localhost JSON-over-HTTP for Sigma FM extension.
//!
//! Endpoints (all on `127.0.0.1:<port>`):
//! - `POST /set_path`  body: `{"path": "C:\\Users\\foo"}` → 200 `{"ok":true}`
//! - `GET  /get_status`                                  → 200 `{"ok":true,"current_path":"...","active_dialogs":N}`
//! - `POST /quit`                                         → 200 `{"ok":true}` then process exits
//!
//! Why HTTP instead of TCP:
//! - Sigma FM extensions can declare `http` host permission but not raw TCP socket.
//! - `sigma.http.request({url: "http://127.0.0.1:37421/..."})` is the official API.
//! - HTTP gives us free request/response framing, no need for line-JSON parsing.
//!
//! STA threading:
//! - The HTTP server runs on the same single-threaded tokio runtime as the TCP server
//!   (per handoff §5.2 — COM STA on main thread). axum with `current_thread` runtime
//!   handles one request at a time cooperatively, which is fine for our traffic.

use crate::events::Event;
use crate::logger::{log_event, now_iso8601};
use crate::state::{AppState, DialogInfo};
use crate::writer::WriteOutcome;
use anyhow::Result;
use axum::{
    extract::State as AxumState,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
enum _CommandJson {
    #[serde(rename = "set_path")]
    SetPath { path: String },
}

#[derive(Debug, Deserialize)]
struct SetPathBody {
    path: String,
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    ok: bool,
    current_path: String,
    active_dialogs: usize,
}

async fn handle_set_path(
    AxumState(state): AxumState<Arc<AppState>>,
    Json(body): Json<SetPathBody>,
) -> impl IntoResponse {
    let path = body.path;
    state.set_current_path(path.clone());
    log_event(&Event::SidecarError {
        kind: "path_update".into(),
        message: format!("current_path={}", path),
        ts: now_iso8601(),
    });

    // Snapshot dialogs, then write to each (release lock before write_path call).
    let dialogs: Vec<DialogInfo> = state.active_dialogs();
    for d in dialogs {
        let outcome = crate::writer::write_path(d.hwnd as isize, &path);
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

    (StatusCode::OK, Json(OkResponse { ok: true, error: None }))
}

async fn handle_get_status(AxumState(state): AxumState<Arc<AppState>>) -> impl IntoResponse {
    let resp = StatusResponse {
        ok: true,
        current_path: state.get_current_path(),
        active_dialogs: state.active_dialogs().len(),
    };
    (StatusCode::OK, Json(resp))
}

async fn handle_quit() -> impl IntoResponse {
    log_event(&Event::SidecarError {
        kind: "shutdown_http".into(),
        message: "quit via HTTP".into(),
        ts: now_iso8601(),
    });
    // Spawn exit so we can flush the response first.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        std::process::exit(0);
    });
    (StatusCode::OK, Json(OkResponse { ok: true, error: None }))
}

async fn handle_health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"ok": true, "service": "focus-sync-sidecar"})))
}

pub async fn run_server(state: Arc<AppState>, port: u16) -> Result<()> {
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/get_status", get(handle_get_status))
        .route("/set_path", post(handle_set_path))
        .route("/quit", post(handle_quit))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(port, "HTTP server listening");

    axum::serve(listener, app).await?;
    Ok(())
}
//! TCP server: line-based JSON command protocol on `127.0.0.1:<port>`.
//!
//! Commands:
//! - `{"cmd":"set_path","path":"C:\\Users\\foo"}` → updates state.current_path,
//!   then writes `path` to all active dialogs in the registry.
//! - `{"cmd":"get_status"}` → returns current_path + active_dialogs count.
//! - `{"cmd":"quit"}` → server returns ok then signals shutdown.

use crate::events::Event;
use crate::logger::{log_event, now_iso8601};
use crate::state::{AppState, DialogInfo};
use crate::writer::WriteOutcome;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
enum Command {
    #[serde(rename = "set_path")]
    SetPath { path: String },
    #[serde(rename = "get_status")]
    GetStatus,
    #[serde(rename = "quit")]
    Quit,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
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
}

pub async fn run_server(state: Arc<AppState>, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    tracing::info!(port, "TCP server listening");

    loop {
        let (socket, _addr) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, state).await {
                tracing::warn!(error = %e, "connection error");
            }
        });
    }
}

async fn handle_connection(mut socket: TcpStream, state: Arc<AppState>) -> Result<()> {
    let (read_half, mut write_half) = socket.split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(()); // EOF
        }

        let cmd: Command = match serde_json::from_str(line.trim()) {
            Ok(c) => c,
            Err(e) => {
                let resp = Response::Ok {
                    ok: false,
                    error: Some(e.to_string()),
                };
                let s = serde_json::to_string(&resp)?;
                write_half.write_all(format!("{}\n", s).as_bytes()).await?;
                continue;
            }
        };

        match cmd {
            Command::SetPath { path } => {
                state.set_current_path(path.clone());
                log_event(&Event::SpikeError {
                    kind: "path_update".into(),
                    message: format!("current_path={}", path),
                    ts: now_iso8601(),
                });

                // Snapshot active dialogs, then write to each.
                // Important: release the lock before calling write_path (no lock held across .await).
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

                let resp = Response::Ok { ok: true, error: None };
                let s = serde_json::to_string(&resp)?;
                write_half.write_all(format!("{}\n", s).as_bytes()).await?;
            }
            Command::GetStatus => {
                let resp = Response::Status {
                    ok: true,
                    current_path: state.get_current_path(),
                    active_dialogs: state.active_dialogs().len(),
                };
                let s = serde_json::to_string(&resp)?;
                write_half.write_all(format!("{}\n", s).as_bytes()).await?;
            }
            Command::Quit => {
                let resp = Response::Ok { ok: true, error: None };
                let s = serde_json::to_string(&resp)?;
                write_half.write_all(format!("{}\n", s).as_bytes()).await?;
                std::process::exit(0);
            }
        }
    }
}
//! Sidecar binary entry point — full integration.
//!
//! Wiring:
//! - `tokio::select!` over three sources:
//!   1. `tcp_server::run_server` — accepts control commands (set_path/get_status/quit)
//!   2. `uia_event::start_monitor` (in `spawn_blocking`) — emits dialog events via sync mpsc
//!   3. Ctrl-C signal — graceful shutdown
//!
//! Per handoff §6.5: no lock held across `select!` return boundary.

use anyhow::Result;
use clap::Parser;
use sidecar::config::parse_config;
use sidecar::events::Event;
use sidecar::logger::{log_event, now_iso8601};
use sidecar::state::{AppState, DialogInfo};
use sidecar::{tcp_server, uia_event};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tokio::signal;

#[derive(Parser, Debug)]
#[command(name = "focus-sync-sidecar", about = "Listary-style global focus sync sidecar")]
struct Cli {
    #[arg(long, default_value_t = 37421)]
    port: u16,
    #[arg(long, default_value = "C:\\")]
    initial_path: String,
    #[arg(long, value_delimiter = ',')]
    app_whitelist: Vec<String>,
    /// IPC mode: tcp (legacy, line-JSON) or http (Sigma FM extension).
    #[arg(long, value_enum, default_value_t = IpcMode::Http)]
    ipc: IpcMode,
    /// Skip UIA monitor + COM init. Useful for CI smoke tests where the runner
    /// has no real desktop session (no UIAutomationCore).
    #[arg(long)]
    no_uia: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum IpcMode {
    Tcp,
    Http,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Initialize tracing (so UIA errors are visible).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sidecar=debug")),
        )
        .try_init();

    let cli = Cli::parse();
    let cfg = parse_config(
        cli.port,
        cli.initial_path.clone(),
        cli.app_whitelist,
        cli.no_uia,
    )?;

    log_event(&Event::SidecarError {
        kind: "startup".into(),
        message: format!("sidecar starting on port {}", cfg.port),
        ts: now_iso8601(),
    });

    let state = Arc::new(AppState::new(cfg.initial_path.clone()));
    let port = cfg.port;

    // Bridge: sync mpsc (PathWrap monitor thread) → tokio mpsc (main select).
    let (sync_tx, sync_rx) = mpsc::channel::<Option<uia_event::DialogInfo>>();
    let (async_tx, mut async_rx) = tokio::sync::mpsc::channel::<Option<uia_event::DialogInfo>>(64);

    // Forward sync → async in a small blocking thread.
    std::thread::spawn(move || {
        while let Ok(msg) = sync_rx.recv() {
            if async_tx.blocking_send(msg).is_err() {
                break;
            }
        }
    });

    // Spawn the UIA monitor (blocking) in a blocking task.
    // start_monitor is `pub fn` (sync); needs spawn_blocking.
    // Skip when --no-uia (CI smoke tests).
    if !cfg.no_uia {
        // COM init as STA — required for UIA. Must happen before any UIA call.
        // Safe to call multiple times (subsequent calls return RPC_E_CHANGED_MODE).
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        let sync_tx_clone = sync_tx.clone();
        tokio::task::spawn_blocking(move || {
            // The monitor uses `ctx.request_repaint` semantics; in sidecar we just no-op the notify.
            // For first integration we pass a no-op Arc<tokio::sync::Notify>.
            let notify = Arc::new(tokio::sync::Notify::new());
            uia_event::start_monitor(sync_tx_clone, notify);
        });
    } else {
        tracing::warn!("UIA monitor disabled (--no-uia); sidecar runs HTTP server only");
    }

    // Spawn IPC server (TCP legacy OR HTTP for Sigma FM extension).
    let state_for_ipc = state.clone();
    let ipc_handle = match cli.ipc {
        IpcMode::Tcp => tokio::spawn(async move {
            if let Err(e) = tcp_server::run_server(state_for_ipc, port).await {
                tracing::warn!(error = %e, "tcp server exited");
            }
        }),
        IpcMode::Http => tokio::spawn(async move {
            if let Err(e) = sidecar::http_server::run_server(state_for_ipc, port).await {
                tracing::warn!(error = %e, "http server exited");
            }
        }),
    };

    // Main select: dialog events + Ctrl-C.
    tokio::select! {
        _ = signal::ctrl_c() => {
            log_event(&Event::SidecarError {
                kind: "shutdown".into(),
                message: "ctrl_c received".into(),
                ts: now_iso8601(),
            });
        }
        Some(dialog_opt) = async_rx.recv() => {
            match dialog_opt {
                Some(info) => {
                    let hwnd_u32 = info.hwnd as u32;
                    let class = classify_app(info.hwnd);
                    log_event(&Event::DialogDetected {
                        hwnd: hwnd_u32,
                        app: class.clone(),
                        ts: now_iso8601(),
                    });

                    // 2. Try Read (UIA ValuePattern read on filename box).
                    match sidecar::reader::read_path(info.hwnd) {
                        Ok(path) => {
                            log_event(&Event::Read {
                                hwnd: hwnd_u32,
                                app: class.clone(),
                                path: path.clone(),
                                strategy: "uia".into(),
                                ts: now_iso8601(),
                            });
                            // 3. Register for write tracking with last_known_path.
                            state.register_dialog(DialogInfo {
                                hwnd: hwnd_u32,
                                app: class.clone(),
                                last_known_path: path,
                                write_strategy: None,
                            });
                        }
                        Err(e) => {
                            log_event(&Event::SidecarError {
                                kind: "read_failed".into(),
                                message: format!("hwnd={hwnd_u32} app={class} err={e}"),
                                ts: now_iso8601(),
                            });
                            // Still register so writes can still be attempted.
                            state.register_dialog(DialogInfo {
                                hwnd: hwnd_u32,
                                app: class.clone(),
                                last_known_path: String::new(),
                                write_strategy: None,
                            });
                        }
                    }

                    // 4. Try Write (sync, UIA SetValue + Invoke, wrapped in fg_bypass).
                    let target = state.get_current_path();
                    if !target.is_empty() {
                        match sidecar::writer::write_path(info.hwnd, &target) {
                            sidecar::writer::WriteOutcome::UiaSetValue => {
                                log_event(&Event::Write {
                                    hwnd: hwnd_u32,
                                    app: class.clone(),
                                    target: target.clone(),
                                    strategy: "uia".into(),
                                    ts: now_iso8601(),
                                });
                            }
                            sidecar::writer::WriteOutcome::SendInputFallback => {
                                log_event(&Event::Write {
                                    hwnd: hwnd_u32,
                                    app: class.clone(),
                                    target: target.clone(),
                                    strategy: "sendinput".into(),
                                    ts: now_iso8601(),
                                });
                            }
                            sidecar::writer::WriteOutcome::Failed(reason) => {
                                log_event(&Event::WriteFailed {
                                    hwnd: hwnd_u32,
                                    app: class.clone(),
                                    target: target.clone(),
                                    reason,
                                    ts: now_iso8601(),
                                });
                            }
                        }
                    }
                }
                None => {
                    // Dialog closed — clean registry (best effort, no lock held across select).
                    log_event(&Event::SidecarError {
                        kind: "dialog_closed".into(),
                        message: "monitor reported dialog close".into(),
                        ts: now_iso8601(),
                    });
                }
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(u64::MAX / 4)) => {
            // unreachable — placeholder so select has at least one branch that doesn't borrow state.
        }
    }

    // Graceful shutdown.
    ipc_handle.abort();
    log_event(&Event::SidecarError {
        kind: "shutdown".into(),
        message: "sidecar exiting cleanly".into(),
        ts: now_iso8601(),
    });
    Ok(())
}

/// Minimal app classifier from HWND class name.
fn classify_app(hwnd: isize) -> String {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    if hwnd == 0 {
        return "unknown".into();
    }
    let h = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetClassNameW(h, &mut buf);
        if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            "unknown".into()
        }
    }
}
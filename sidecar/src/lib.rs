//! Sidecar library — re-exports for unit testing.
//!
//! See `main.rs` for the binary entry point and
//! `docs/spec.md` for full design (in the parent repo).
//!
//! **Source attribution** (see `../README.md` for full licenses):
//! - `uia_inject.rs` — ported from `inaku-Gyan/PathWrap/src/os/dialog.rs` (MIT)
//! - `uia_event.rs`  — ported from `inaku-Gyan/PathWrap/src/os/monitor.rs` (MIT)
//! - `fg_bypass.rs`  — ported from `QwenLM/qwen-code/.../fg_bypass.rs` (Apache-2.0)

pub mod config;
pub mod detector;
pub mod events;
pub mod fg_bypass;
pub mod http_server;
pub mod logger;
pub mod reader;
pub mod state;
pub mod tcp_server;
pub mod uia_event;
pub mod uia_inject;
pub mod writer;
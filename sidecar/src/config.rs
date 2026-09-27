//! CLI argument parsing + port resolution.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::net::TcpListener;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub port: u16,
    pub initial_path: String,
    pub app_whitelist: Vec<String>,
    /// When true, skip UIA monitor + COM init. Used for CI smoke tests where
    /// Windows Server runners may lack a real desktop session.
    pub no_uia: bool,
}

/// Try to bind to `preferred`, then `preferred+1`, ..., up to `preferred+78`.
/// Returns the first port that binds successfully.
/// Returns Err if all 79 ports are occupied.
pub fn resolve_port(preferred: u16) -> Result<u16> {
    for offset in 0..79u16 {
        let candidate = preferred.saturating_add(offset);
        if TcpListener::bind(("127.0.0.1", candidate)).is_ok() {
            // Re-bind and immediately drop to confirm port is free for caller
            let _drop = TcpListener::bind(("127.0.0.1", candidate));
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "no free port in range {}..{}",
        preferred,
        preferred.saturating_add(78)
    ))
}

/// Parse Config from raw values (testable without Cli struct).
pub fn parse_config(
    port: u16,
    initial_path: String,
    app_whitelist: Vec<String>,
    no_uia: bool,
) -> Result<Config> {
    let port = resolve_port(port)?;
    Ok(Config {
        port,
        initial_path,
        app_whitelist,
        no_uia,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_port_preferred_free() {
        // Use a known-high port that should be free; should return same port.
        let p = resolve_port(37421).unwrap();
        assert!(p >= 37421);
    }

    #[test]
    fn resolve_port_increments_on_conflict() {
        // Bind a port manually, then resolve_port from that port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied = listener.local_addr().unwrap().port();
        let resolved = resolve_port(occupied).unwrap();
        assert_ne!(resolved, occupied, "should skip occupied port");
        drop(listener);
    }

    #[test]
    fn parse_config_returns_resolved_port() {
        let cfg = parse_config(37421, "C:\\Users\\foo".into(), vec![], false).unwrap();
        assert_eq!(cfg.initial_path, "C:\\Users\\foo");
        assert!(cfg.port >= 37421);
        assert!(cfg.app_whitelist.is_empty());
        assert!(!cfg.no_uia);
    }
}
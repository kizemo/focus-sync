//! JSON-line logger. Each event is one line of JSON on stdout,
//! suitable for aggregation by `verify_target_matrix.ps1`.

use crate::events::Event;
use std::io::Write;
use std::sync::Mutex;
use std::time::SystemTime;

static STDOUT_LOCK: Mutex<()> = Mutex::new(());

/// Returns current UTC time in ISO 8601 format (e.g. "2026-09-27T12:34:56Z").
pub fn now_iso8601() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    format_iso8601_utc(secs)
}

fn format_iso8601_utc(unix_secs: u64) -> String {
    let days = unix_secs / 86400;
    let secs_of_day = unix_secs % 86400;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

/// Serialize event as one-line JSON and write to stdout, followed by newline.
pub fn log_event(event: &Event) {
    let _guard = STDOUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let json = serde_json::to_string(event).expect("Event serialization never fails");
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", json);
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_iso8601_format() {
        let s = now_iso8601();
        // ISO 8601: 2026-09-27T12:34:56Z
        assert!(s.contains("T"));
        assert!(s.ends_with("Z"));
    }

    #[test]
    fn format_iso8601_utc_known_value() {
        // 2026-09-27T00:00:00Z = 1790467200
        let s = format_iso8601_utc(1790467200);
        assert_eq!(s, "2026-09-27T00:00:00Z");
    }

    #[test]
    fn log_event_does_not_panic() {
        let evt = Event::ForegroundChanged {
            hwnd: 1,
            ts: now_iso8601(),
        };
        log_event(&evt); // smoke test, output captured by test framework
    }
}
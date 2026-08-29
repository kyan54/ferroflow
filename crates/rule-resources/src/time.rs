//! Hand-rolled UTC RFC3339 timestamp formatting, mirrored (not shared via a
//! cross-crate dependency) from `core_manager::history`'s `now_rfc3339` --
//! see that module's doc comment for why this avoids pulling in a
//! `chrono`/`time` dependency for one timestamp. Duplicated rather than
//! depending on `core-manager` from this crate to keep `rule-resources`
//! decoupled from the proxy-lifecycle crate (nothing else in this crate
//! needs anything else `core-manager` provides).

use std::time::{SystemTime, UNIX_EPOCH};

/// Formats `SystemTime::now()` as a UTC RFC3339 timestamp with 1-second
/// resolution, e.g. `"2024-01-15T10:30:00Z"`.
pub fn now_rfc3339() -> String {
    format_rfc3339(SystemTime::now())
}

fn format_rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86400) as i64;
    let time_of_day = secs % 86400;
    let (hour, minute, second) = (time_of_day / 3600, (time_of_day % 3600) / 60, time_of_day % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Converts a day count since the Unix epoch (1970-01-01) into a
/// (year, month, day) civil calendar date -- Howard Hinnant's well-known
/// `civil_from_days` algorithm (<http://howardhinnant.github.io/date_algorithms.html>),
/// transcribed directly, same as `core_manager::history`'s copy.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_rfc3339_known_epoch_values() {
        assert_eq!(format_rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            format_rfc3339(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)),
            "2023-11-14T22:13:20Z"
        );
    }
}

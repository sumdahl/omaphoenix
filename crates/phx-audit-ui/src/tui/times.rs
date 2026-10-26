//! Local time formatting without a timezone crate: the UTC offset is asked
//! of `date` once.

use std::process::Command;

use phx_audit::store::now;

/// The local UTC offset in seconds, asked of `date` once.
// ponytail: one offset per session; a DST change mid-session shifts times by an hour until restart.
fn utc_offset() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        let z = Command::new("date")
            .arg("+%z")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
        let z = z.unwrap_or_default();
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let digits = z.trim_start_matches(['+', '-']);
        let h: i64 = digits.get(..2).and_then(|v| v.parse().ok()).unwrap_or(0);
        let m: i64 = digits.get(2..4).and_then(|v| v.parse().ok()).unwrap_or(0);
        sign * (h * 3600 + m * 60)
    })
}

const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Formats epoch seconds in local time. Supports `%Y %m %d %-d %H %M %a %b`.
pub fn local_time(epoch: u64, fmt: &str) -> String {
    let t = i64::try_from(epoch).unwrap_or(0) + utc_offset();
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let (y, mo, d) = civil(days);
    let wday = (days + 4).rem_euclid(7); // 1970-01-01 was a Thursday
    let idx = |v: i64| usize::try_from(v).unwrap_or(0);
    fmt.replace("%Y", &y.to_string())
        .replace("%m", &format!("{mo:02}"))
        .replace("%-d", &d.to_string())
        .replace("%d", &format!("{d:02}"))
        .replace("%H", &format!("{:02}", secs / 3600))
        .replace("%M", &format!("{:02}", secs % 3600 / 60))
        .replace("%a", WD.get(idx(wday)).unwrap_or(&""))
        .replace("%b", MON.get(idx(mo - 1)).unwrap_or(&""))
}

/// (year, month, day) from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "5m ago" style.
pub fn ago(epoch: u64) -> String {
    let s = now().saturating_sub(epoch);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86_400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_371), (2025, 10, 10));
        assert_eq!(super::civil(11_016), (2000, 2, 29));
    }
}

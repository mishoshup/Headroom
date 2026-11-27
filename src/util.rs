use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write_bp(f: &mut fmt::Formatter<'_>, bp: i64) -> fmt::Result {
    let whole = bp / 100;
    let frac = bp % 100;
    if frac == 0 {
        write!(f, "{whole}")
    } else if frac % 10 == 0 {
        write!(f, "{whole}.{}", frac / 10)
    } else {
        write!(f, "{whole}.{frac:02}")
    }
}

/// Percentage clamped to 0..100, formatted without an allocation.
pub struct Pct(pub i64);

impl fmt::Display for Pct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_bp(f, self.0.clamp(0, 10_000))
    }
}

/// Seconds -> "1d 5h", "3h 12m", "59m", formatted without an allocation.
pub struct Duration(pub i64);

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.0.max(0);
        let d = secs / 86_400;
        let h = (secs % 86_400) / 3_600;
        let m = (secs % 3_600) / 60;
        if d > 0 {
            write!(f, "{d}d {h}h")
        } else if h > 0 {
            write!(f, "{h}h {m}m")
        } else {
            write!(f, "{m}m")
        }
    }
}

/// Basis points -> compact percent text. 4286 -> "42.86", 4500 -> "45".
pub fn format_pct(bp: i64) -> String {
    format!("{}", Pct(bp))
}

/// Seconds -> "1d 5h", "3h 12m", "59m".
pub fn duration(secs: i64) -> String {
    format!("{}", Duration(secs))
}

/// JSON percentage (f64) -> basis points.
pub fn f64_to_bp(v: f64) -> Option<i64> {
    if v.is_finite() && v >= 0.0 {
        Some((v * 100.0).round() as i64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_compact() {
        assert_eq!(duration(90_061), "1d 1h");
        assert_eq!(duration(3_600), "1h 0m");
    }
}

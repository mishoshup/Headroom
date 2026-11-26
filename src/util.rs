use std::time::{SystemTime, UNIX_EPOCH};

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Basis points -> compact percent text. 4286 -> "42.86", 4500 -> "45".
pub fn format_pct(bp: i64) -> String {
    let bp = bp.clamp(0, 10_000);
    let whole = bp / 100;
    let frac = bp % 100;
    if frac == 0 {
        format!("{whole}")
    } else if frac % 10 == 0 {
        format!("{whole}.{}", frac / 10)
    } else {
        format!("{whole}.{frac:02}")
    }
}

/// Seconds -> "1d 5h", "3h 12m", "59m".
pub fn duration(secs: i64) -> String {
    let secs = secs.max(0);
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// JSON percentage (f64) -> basis points.
pub fn f64_to_bp(v: f64) -> Option<i64> {
    if v.is_finite() && v >= 0.0 {
        Some((v * 100.0).round() as i64)
    } else {
        None
    }
}

use serde::Deserialize;
use std::env;
use std::io::Read;

use crate::config::Config;
use crate::credit;
use crate::pace::{self, Pace};
use crate::render::{self, Snapshot, WindowView};
use crate::state::{State, Window};
use crate::style::Theme;
use crate::util::{f64_to_bp, unix_now};

#[derive(Deserialize, Default)]
struct Input {
    #[serde(default)]
    model: Option<Model>,
    #[serde(default)]
    context_window: Option<ContextWindow>,
    #[serde(default)]
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct Model {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct ContextWindow {
    #[serde(default)]
    used_percentage: Option<f64>,
}

#[derive(Deserialize)]
struct RateLimits {
    #[serde(default)]
    five_hour: Option<RateWindow>,
    #[serde(default)]
    seven_day: Option<RateWindow>,
}

#[derive(Deserialize)]
struct RateWindow {
    #[serde(default)]
    used_percentage: Option<f64>,
    #[serde(default)]
    resets_at: Option<f64>,
}

pub fn run(cfg: &Config) -> String {
    let now = unix_now();
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return "Claude".into();
    }
    let input: Input = match serde_json::from_str(&raw) {
        Ok(input) => input,
        Err(_) => return "Claude".into(),
    };

    let model = input
        .model
        .and_then(|m| m.display_name)
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "Claude".into());
    let ctx_bp = input
        .context_window
        .and_then(|c| c.used_percentage)
        .and_then(f64_to_bp);

    let five = input
        .rate_limits
        .as_ref()
        .and_then(|r| r.five_hour.as_ref())
        .and_then(to_window);
    let week = input
        .rate_limits
        .as_ref()
        .and_then(|r| r.seven_day.as_ref())
        .and_then(to_window);

    let previous = State::load(&cfg.state);
    let previous_captured = previous.as_ref().map(|s| s.captured_at);
    let five = merge_window(
        five,
        previous.as_ref().and_then(|s| s.five_hour),
        now,
        previous_captured,
        cfg.stale_after,
    );
    let week = merge_window(
        week,
        previous.as_ref().and_then(|s| s.seven_day),
        now,
        previous_captured,
        cfg.stale_after,
    );

    let mut state = State {
        captured_at: now,
        five_hour: five,
        seven_day: week,
        ..State::default()
    };
    pace::refresh_state(&mut state, cfg.week_days, now);
    let _ = state.save(&cfg.state);

    let pace = match state.seven_day.and_then(|window| window.resets_at) {
        Some(reset) => {
            let (extra, _) = credit::active_bp(cfg, now, reset, state.day);
            Pace::compute(cfg, &state, now, extra)
        }
        None => None,
    };

    let snapshot = Snapshot {
        model,
        ctx_bp,
        five: state.five_hour.map(|w| WindowView {
            used_bp: w.used_bp,
            resets_at: w.resets_at,
        }),
        week: state.seven_day.map(|w| WindowView {
            used_bp: w.used_bp,
            resets_at: w.resets_at,
        }),
    };

    let width = env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .saturating_sub(4)
        .max(40);
    let theme = Theme::new(cfg.color, cfg.ascii);
    let mut output = render::dashboard(&snapshot, pace.as_ref(), &theme, now, width);
    // Each blank visual row is a non-whitespace glyph so Claude Code keeps it.
    for _ in 0..cfg.bottom_margin {
        output.push('\n');
        output.push('\u{2800}');
    }
    output
}

fn to_window(window: &RateWindow) -> Option<Window> {
    let used_bp = f64_to_bp(window.used_percentage?)?;
    let resets_at = window
        .resets_at
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|v| v as i64);
    Some(Window { used_bp, resets_at })
}

/// A statusline payload may briefly omit a window. Keep the previous value if
/// it is still fresh and its reset is still in the future.
fn merge_window(
    new: Option<Window>,
    old: Option<Window>,
    now: u64,
    previous_captured: Option<u64>,
    stale_after: u64,
) -> Option<Window> {
    if let Some(window) = new {
        return Some(window);
    }
    let window = old?;
    let reset = window.resets_at?;
    let captured = previous_captured?;
    if reset > now as i64 && now.saturating_sub(captured) <= stale_after {
        Some(window)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_missing_windows() {
        let input: Input = serde_json::from_str(r#"{"model":{"display_name":"Sonnet"}}"#).unwrap();
        assert!(input.rate_limits.is_none());
        assert_eq!(input.model.unwrap().display_name.unwrap(), "Sonnet");
    }

    #[test]
    fn reads_float_percentages_and_epochs() {
        let input: Input = serde_json::from_str(
            r#"{"rate_limits":{"seven_day":{"used_percentage":45.5,"resets_at":1796094000}}}"#,
        )
        .unwrap();
        let w = to_window(input.rate_limits.unwrap().seven_day.as_ref().unwrap()).unwrap();
        assert_eq!(w.used_bp, 4_550);
        assert_eq!(w.resets_at, Some(1_796_094_000));
    }
}

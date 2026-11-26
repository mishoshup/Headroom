use std::fmt::Write as _;

use crate::pace::Pace;
use crate::style::{Accent, Theme};
use crate::util::{Duration, Pct, Ratio};

const LABEL_WIDTH: usize = 8;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub model: String,
    pub ctx_bp: Option<i64>,
    pub five: Option<WindowView>,
    pub week: Option<WindowView>,
}

#[derive(Clone, Copy, Debug)]
pub struct WindowView {
    pub used_bp: i64,
    pub resets_at: Option<i64>,
}

/// Render the status line into one pre-sized buffer. No intermediate line or
/// span strings are allocated.
pub fn dashboard(
    snapshot: &Snapshot,
    pace: Option<&Pace>,
    theme: &Theme,
    now: u64,
    width: usize,
) -> String {
    let mut out = String::with_capacity(512);
    let sep = if theme.ascii() { " | " } else { " · " };

    theme.write_dim(&mut out, "Usage");
    theme.write_faint(&mut out, sep);
    theme.write_bold_color(&mut out, Accent::Cyan, &snapshot.model);
    if let Some(ctx) = snapshot.ctx_bp {
        theme.write_faint(&mut out, sep);
        theme.write_dim(&mut out, "context ");
        let accent = usage_accent(ctx);
        theme.start_color(&mut out, accent);
        if theme.ascii() {
            let _ = write!(out, "{}%", Pct(ctx));
        } else {
            let _ = write!(out, "{} {}%", context_circle(ctx), Pct(ctx));
        }
        theme.reset(&mut out);
    }
    out.push('\n');

    let cells = width.saturating_sub(38).clamp(10, 28);

    if let Some(five) = snapshot.five {
        write_window_row(&mut out, "5-hour", five, cells, theme, now);
        out.push('\n');
    }
    if let Some(week) = snapshot.week {
        write_window_row(&mut out, "weekly", week, cells, theme, now);
        out.push('\n');
    }
    if let Some(p) = pace {
        write_today_row(&mut out, p, cells, theme);
        out.push('\n');
        if p.credit_bp > 0 {
            write_borrow_row(&mut out, p, theme);
            out.push('\n');
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn write_label(out: &mut String, theme: &Theme, label: &str) {
    theme.write_dim(out, label);
    for _ in label.len()..LABEL_WIDTH {
        out.push(' ');
    }
}

fn write_window_row(
    out: &mut String,
    label: &str,
    window: WindowView,
    cells: usize,
    theme: &Theme,
    now: u64,
) {
    let accent = usage_accent(window.used_bp);
    write_label(out, theme, label);
    write_bar(out, window.used_bp, cells, theme, accent);
    out.push_str("  ");
    theme.start_color(out, accent);
    let _ = write!(out, "{:>6}%", Pct(window.used_bp));
    theme.reset(out);
    if let Some(reset) = window.resets_at {
        if reset > now as i64 {
            out.push_str("  ");
            theme.start_color(out, Accent::Muted);
            let _ = write!(out, "resets in {}", Duration(reset - now as i64));
            theme.reset(out);
        }
    }
}

fn write_today_row(out: &mut String, pace: &Pace, cells: usize, theme: &Theme) {
    // 100% = today's allowance exactly used; 120% = overspent by 20% of it.
    let used_of_allowance = if pace.ceiling_bp > 0 {
        pace.used_bp.saturating_mul(10_000) / pace.ceiling_bp
    } else {
        0
    };
    let accent = if used_of_allowance >= 10_000 {
        Accent::Red
    } else if used_of_allowance >= 9_000 {
        Accent::Yellow
    } else {
        Accent::Green
    };
    let delta = pace.used_bp - pace.ceiling_bp;

    write_label(out, theme, "today");
    write_bar(out, used_of_allowance, cells, theme, accent);
    out.push_str("  ");
    theme.start_color(out, accent);
    let _ = write!(out, "{:>6}%", Ratio(used_of_allowance));
    theme.reset(out);
    out.push_str("  ");

    if delta > 0 {
        theme.start_color(out, Accent::Red);
        let _ = write!(out, "{}% extra of weekly", Pct(delta));
        theme.reset(out);
    } else {
        theme.start_color(out, Accent::Green);
        let _ = write!(out, "{}% left of weekly", Pct(-delta));
        theme.reset(out);
    }
}

fn write_borrow_row(out: &mut String, pace: &Pace, theme: &Theme) {
    for _ in 0..LABEL_WIDTH {
        out.push(' ');
    }
    out.push_str("  ");
    theme.start_color(out, Accent::Cyan);
    let _ = write!(out, "+{}% borrowed", Pct(pace.credit_bp));
    theme.reset(out);
    theme.write_faint(out, " · ");
    theme.start_color(out, Accent::Muted);
    let _ = write!(out, "allowance {}% of weekly", Pct(pace.ceiling_bp));
    theme.reset(out);
}

fn write_bar(out: &mut String, bp: i64, cells: usize, theme: &Theme, accent: Accent) {
    let bp = bp.clamp(0, 10_000);
    let filled = ((bp * cells as i64 + 5_000) / 10_000) as usize;
    let (full, empty) = if theme.ascii() {
        ('#', '-')
    } else {
        ('▰', '▱')
    };

    theme.start_color(out, accent);
    for _ in 0..filled {
        out.push(full);
    }
    theme.reset(out);

    theme.start_color(out, Accent::Faint);
    for _ in 0..(cells - filled) {
        out.push(empty);
    }
    theme.reset(out);
}

fn usage_accent(bp: i64) -> Accent {
    if bp >= 8_000 {
        Accent::Red
    } else if bp >= 6_000 {
        Accent::Yellow
    } else {
        Accent::Green
    }
}

fn context_circle(bp: i64) -> &'static str {
    match bp.clamp(0, 10_000) {
        0 => "○",
        1..=2_500 => "◔",
        2_501..=5_000 => "◑",
        5_001..=7_500 => "◕",
        _ => "●",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ColorMode;

    fn theme() -> Theme {
        Theme::new(ColorMode::None, false)
    }

    fn pace() -> Pace {
        Pace {
            day: 4,
            week_days: 7,
            used_bp: 6_000,
            allowance_bp: 5_716,
            credit_bp: 2_000,
            ceiling_bp: 7_716,
            over: false,
            next_epoch: 2_000,
            window_end: 3_000,
        }
    }

    #[test]
    fn renders_plain_language_rows() {
        let snap = Snapshot {
            model: "Sonnet".into(),
            ctx_bp: Some(4_240),
            five: Some(WindowView {
                used_bp: 1_200,
                resets_at: Some(3_000),
            }),
            week: Some(WindowView {
                used_bp: 6_000,
                resets_at: Some(3_000),
            }),
        };
        let out = dashboard(&snap, Some(&pace()), &theme(), 1_000, 80);
        assert!(out.starts_with("Usage · Sonnet · context "));
        assert!(out.contains("42.4%"));
        assert!(out.contains('◑'));
        assert!(out.contains("5-hour"));
        assert!(out.contains("weekly"));
        assert!(out.contains("today"));
        assert!(out.contains("77.76%"));
        assert!(out.contains("17.16% left of weekly"));
        assert!(out.contains("+20% borrowed"));
        assert!(out.contains("allowance 77.16% of weekly"));
        assert!(out.contains("resets in 33m"));
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn ascii_mode_uses_ascii_glyphs() {
        let t = Theme::new(ColorMode::None, true);
        let out = dashboard(
            &Snapshot {
                model: "Claude".into(),
                ctx_bp: None,
                five: None,
                week: Some(WindowView {
                    used_bp: 4_500,
                    resets_at: None,
                }),
            },
            None,
            &t,
            1_000,
            80,
        );
        assert!(out.contains('#'));
        assert!(out.contains('-'));
        assert!(!out.contains('▰'));
    }
}

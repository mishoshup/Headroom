use crate::pace::Pace;
use crate::style::{Accent, Theme};
use crate::util::{duration, format_pct, format_ratio};

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

pub fn dashboard(
    snapshot: &Snapshot,
    pace: Option<&Pace>,
    theme: &Theme,
    now: u64,
    width: usize,
) -> String {
    let mut lines = Vec::new();
    let sep = if theme.ascii() { " | " } else { " · " };

    let mut header = format!(
        "{}{}{}",
        theme.dim("Usage"),
        theme.faint(sep),
        theme.bold_color(Accent::Cyan, &snapshot.model)
    );
    if let Some(ctx) = snapshot.ctx_bp {
        header.push_str(&theme.faint(sep));
        header.push_str(&theme.dim("context "));
        let accent = usage_accent(ctx);
        if theme.ascii() {
            header.push_str(&theme.color(accent, &format!("{}%", format_pct(ctx))));
        } else {
            header.push_str(&theme.color(
                accent,
                &format!("{} {}%", context_circle(ctx), format_pct(ctx)),
            ));
        }
    }
    lines.push(header);

    let cells = width.saturating_sub(38).clamp(10, 28);

    if let Some(five) = snapshot.five {
        lines.push(window_row("5-hour", five, cells, theme, now));
    }
    if let Some(week) = snapshot.week {
        lines.push(window_row("weekly", week, cells, theme, now));
    }
    if let Some(p) = pace {
        let (row, borrow_row) = today_row(p, cells, theme);
        lines.push(row);
        if let Some(borrow_row) = borrow_row {
            lines.push(borrow_row);
        }
    }

    lines.join("\n")
}

fn window_row(label: &str, window: WindowView, cells: usize, theme: &Theme, now: u64) -> String {
    let accent = usage_accent(window.used_bp);
    let mut out = String::new();
    out.push_str(&theme.dim(&format!("{label:<LABEL_WIDTH$}")));
    out.push_str(&bar(window.used_bp, cells, theme, accent));
    out.push_str("  ");
    out.push_str(&theme.bold_color(
        accent,
        &format!("{:>6}", format!("{}%", format_pct(window.used_bp))),
    ));
    if let Some(reset) = window.resets_at {
        if reset > now as i64 {
            out.push_str(&format!(
                "  {}",
                theme.dim(&format!("resets in {}", duration(reset - now as i64)))
            ));
        }
    }
    out
}

fn today_row(pace: &Pace, cells: usize, theme: &Theme) -> (String, Option<String>) {
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

    let mut out = String::new();
    out.push_str(&theme.dim(&format!("{:<LABEL_WIDTH$}", "today")));
    out.push_str(&bar(used_of_allowance, cells, theme, accent));
    out.push_str("  ");
    out.push_str(&theme.bold_color(
        accent,
        &format!("{:>6}", format!("{}%", format_ratio(used_of_allowance))),
    ));
    out.push_str("  ");
    if delta > 0 {
        out.push_str(&theme.color(
            Accent::Red,
            &format!("{}% extra of weekly", format_pct(delta)),
        ));
    } else {
        out.push_str(&theme.color(
            Accent::Green,
            &format!("{}% left of weekly", format_pct(-delta)),
        ));
    }

    let borrow_row = if pace.credit_bp > 0 {
        Some(format!(
            "{}  {} {}",
            theme.dim(&format!("{:<LABEL_WIDTH$}", "")),
            theme.color(
                Accent::Cyan,
                &format!("+{}% borrowed", format_pct(pace.credit_bp))
            ),
            theme.dim(&format!(
                "· allowance {}% of weekly",
                format_pct(pace.ceiling_bp)
            ))
        ))
    } else {
        None
    };
    (out, borrow_row)
}

fn bar(bp: i64, cells: usize, theme: &Theme, accent: Accent) -> String {
    let bp = bp.clamp(0, 10_000);
    let filled = ((bp * cells as i64 + 5_000) / 10_000) as usize;
    let (full, empty) = if theme.ascii() {
        ('#', '-')
    } else {
        ('▰', '▱')
    };
    let fill = full.to_string().repeat(filled);
    let pad = empty.to_string().repeat(cells - filled);
    format!("{}{}", theme.color(accent, &fill), theme.faint(&pad))
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

/// Small circular gauge for the context window.
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
        assert!(out.contains("◑"));
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

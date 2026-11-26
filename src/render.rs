use crate::pace::Pace;
use crate::style::{Accent, Theme};
use crate::util::{duration, format_pct};

const LABEL_WIDTH: usize = 10;

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
        theme.dim(sep),
        theme.bold_color(Accent::Cyan, &snapshot.model)
    );
    if let Some(ctx) = snapshot.ctx_bp {
        header.push_str(&theme.dim(sep));
        header.push_str(&theme.dim("ctx "));
        header.push_str(&theme.color(usage_accent(ctx), &format!("{}%", format_pct(ctx))));
    }
    lines.push(header);

    // Grid first, legend second: the compact /context dashboard shape.
    let cells = width.saturating_sub(38).clamp(10, 20);

    if let Some(five) = snapshot.five {
        lines.push(window_row("5h", five, cells, theme, now));
    }
    if let Some(week) = snapshot.week {
        lines.push(window_row("7d", week, cells, theme, now));
    }
    if let Some(p) = pace {
        lines.push(pace_row(p, cells, theme));
    }

    lines.join("\n")
}

fn window_row(label: &str, window: WindowView, cells: usize, theme: &Theme, now: u64) -> String {
    let accent = usage_accent(window.used_bp);
    let mut out = String::new();
    out.push_str(&bar(window.used_bp, cells, theme, accent));
    out.push_str("  ");
    out.push_str(&theme.dim(&format!("{label:<LABEL_WIDTH$}")));
    out.push_str("  ");
    out.push_str(&theme.bold_color(
        accent,
        &format!("{:>7}", format!("{}%", format_pct(window.used_bp))),
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

fn pace_row(pace: &Pace, cells: usize, theme: &Theme) -> String {
    let ratio = if pace.ceiling_bp > 0 {
        (pace.used_bp.saturating_mul(10_000) / pace.ceiling_bp).clamp(0, 10_000)
    } else {
        0
    };
    let accent = if pace.over {
        Accent::Red
    } else if ratio >= 9_000 {
        Accent::Yellow
    } else {
        Accent::Green
    };

    let label = format!("pace d{}/{}", pace.day, pace.week_days);
    let mut out = String::new();
    out.push_str(&bar(ratio, cells, theme, accent));
    out.push_str("  ");
    out.push_str(&theme.dim(&format!("{label:<LABEL_WIDTH$}")));
    out.push_str("  ");
    out.push_str(&theme.bold_color(
        accent,
        &format!("{:>7}", format!("{}%", format_pct(pace.ceiling_bp))),
    ));
    out.push_str(&theme.dim(" cap"));
    out.push_str("  ");

    if pace.over {
        out.push_str(&theme.color(
            Accent::Red,
            &format!("{} over", format_pct(pace.used_bp - pace.ceiling_bp)),
        ));
    } else {
        out.push_str(&theme.color(
            Accent::Green,
            &format!("{} left", format_pct(pace.ceiling_bp - pace.used_bp)),
        ));
    }
    if pace.credit_bp > 0 {
        out.push_str(&theme.color(
            Accent::Cyan,
            &format!("  +{} borrowed", format_pct(pace.credit_bp)),
        ));
    }
    out
}

fn bar(bp: i64, cells: usize, theme: &Theme, accent: Accent) -> String {
    let bp = bp.clamp(0, 10_000);
    let filled = ((bp * cells as i64 + 5_000) / 10_000) as usize;
    let (full, empty) = if theme.ascii() {
        ('#', '-')
    } else {
        ('█', '░')
    };
    let fill = full.to_string().repeat(filled);
    let pad = empty.to_string().repeat(cells - filled);
    format!("{}{}", theme.color(accent, &fill), theme.dim(&pad))
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
    fn renders_header_and_rows_without_ansi() {
        let snap = Snapshot {
            model: "Sonnet".into(),
            ctx_bp: Some(4_240),
            five: Some(WindowView {
                used_bp: 1_200,
                resets_at: Some(1_500),
            }),
            week: Some(WindowView {
                used_bp: 6_000,
                resets_at: Some(3_000),
            }),
        };
        let out = dashboard(&snap, Some(&pace()), &theme(), 1_000, 80);
        assert!(out.starts_with("Usage · Sonnet · ctx 42.4%"));
        assert!(out.contains("5h"));
        assert!(out.contains("12%"));
        assert!(out.contains("pace d4/7"));
        assert!(out.contains("77.16% cap"));
        assert!(out.contains("+20 borrowed"));
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

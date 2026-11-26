use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::time::Duration;

use crate::config::{Config, FiveMode};
use crate::credit;
use crate::pace::Pace;
use crate::state::{State, Window};
use crate::util::{duration, format_pct, unix_now};

pub enum Decision {
    Allow,
    Deny(String),
}

pub enum PromptDecision {
    Allow(String),
    Block(String),
}

#[derive(Deserialize)]
struct PromptInput {
    #[serde(default)]
    prompt: Option<String>,
}

pub fn pretool(cfg: &Config) -> Decision {
    let now = unix_now();
    let Some(state) = State::load(&cfg.state) else {
        return Decision::Allow;
    };
    if !state.is_fresh(now, cfg.stale_after) {
        return Decision::Allow;
    }

    if let Some(five) = state.five_hour {
        let active = five
            .resets_at
            .map(|reset| (now as i64) < reset)
            .unwrap_or(true);
        if active && five.used_bp >= cfg.five_limit_bp {
            match cfg.five_mode {
                FiveMode::Block => return Decision::Deny(five_reason(cfg, &five, now)),
                FiveMode::Wait => {
                    let Some(reset) = five.resets_at else {
                        return Decision::Deny(five_reason(cfg, &five, now));
                    };
                    let nap = reset - now as i64 + cfg.reset_buffer as i64;
                    if nap <= 0 {
                        let _ = fs::remove_file(&cfg.state);
                        return Decision::Allow;
                    }
                    if nap > cfg.max_sleep as i64 {
                        return Decision::Deny(format!(
                            "usage-pacer: the 5-hour window is at {}%; the reset is {} away, longer than the configured maximum wait of {}. The call is denied instead of sleeping partially. Retry after the reset, or set PACER_FIVE_HOUR_MODE=block.",
                            format_pct(five.used_bp),
                            duration(nap),
                            duration(cfg.max_sleep as i64)
                        ));
                    }
                    eprintln!(
                        "usage-pacer: 5h at {}%; sleeping {nap}s for the reset.",
                        format_pct(five.used_bp)
                    );
                    std::thread::sleep(Duration::from_secs(nap as u64));
                    let _ = fs::remove_file(&cfg.state);
                    return Decision::Allow;
                }
            }
        }
    }

    if cfg.pacing {
        if let Some(week) = state.seven_day {
            if let Some(reset) = week.resets_at {
                if let Some(base) = Pace::compute(cfg, week.used_bp, reset, now, 0) {
                    let (extra, _) = credit::active_bp(cfg, now, reset, base.day);
                    if let Some(pace) = Pace::compute(cfg, week.used_bp, reset, now, extra) {
                        if pace.over {
                            return Decision::Deny(weekly_reason(&pace, now));
                        }
                    }
                }
            }
        }
    }

    Decision::Allow
}

pub fn prompt(cfg: &Config) -> PromptDecision {
    let mut raw = String::new();
    let parsed = std::io::stdin().read_to_string(&mut raw).ok();
    let prompt = parsed
        .and_then(|_| serde_json::from_str::<PromptInput>(&raw).ok())
        .and_then(|p| p.prompt);

    let now = unix_now();
    let Some(state) = State::load(&cfg.state) else {
        return PromptDecision::Allow(String::new());
    };
    if !state.is_fresh(now, cfg.stale_after) {
        return PromptDecision::Allow(String::new());
    }

    let mut parts: Vec<String> = Vec::new();

    if let Some(five) = state.five_hour {
        let active = five
            .resets_at
            .map(|reset| (now as i64) < reset)
            .unwrap_or(true);
        if active {
            let mut seg = format!("5h {}%", format_pct(five.used_bp));
            if let Some(reset) = five.resets_at {
                seg.push_str(&format!(" (resets in {})", duration(reset - now as i64)));
            }
            parts.push(seg);
        }
    }

    if let Some(week) = state.seven_day {
        let mut seg = format!("7d {}%", format_pct(week.used_bp));
        if let Some(reset) = week.resets_at {
            if (now as i64) < reset {
                seg.push_str(&format!(" (resets in {})", duration(reset - now as i64)));
            }
        }
        parts.push(seg);

        if cfg.pacing {
            if let Some(reset) = week.resets_at {
                if let Some(base) = Pace::compute(cfg, week.used_bp, reset, now, 0) {
                    let (extra, _) = credit::active_bp(cfg, now, reset, base.day);
                    if let Some(pace) = Pace::compute(cfg, week.used_bp, reset, now, extra) {
                        let credit_note = if pace.credit_bp > 0 {
                            format!(", +{}% borrowed", format_pct(pace.credit_bp))
                        } else {
                            String::new()
                        };
                        if pace.over {
                            parts.push(format!(
                                "pace day {}/{}: {}% used vs {}% ceiling{} (OVER by {} pts)",
                                pace.day,
                                pace.week_days,
                                format_pct(pace.used_bp),
                                format_pct(pace.ceiling_bp),
                                credit_note,
                                format_pct(pace.used_bp - pace.ceiling_bp)
                            ));
                            if cfg.block_prompts && is_human(prompt.as_deref()) {
                                return PromptDecision::Block(block_reason(&pace, now));
                            }
                        } else {
                            parts.push(format!(
                                "pace day {}/{}: ceiling {}%{}, {} pts left",
                                pace.day,
                                pace.week_days,
                                format_pct(pace.ceiling_bp),
                                credit_note,
                                format_pct(pace.ceiling_bp - pace.used_bp)
                            ));
                        }
                    }
                }
            }
        }
    }

    if parts.is_empty() {
        return PromptDecision::Allow(String::new());
    }
    PromptDecision::Allow(format!(
        "[usage-pacer] {}. Keep steps small as the ceiling approaches.",
        parts.join("; ")
    ))
}

fn is_human(prompt: Option<&str>) -> bool {
    match prompt {
        Some(p) => !p.starts_with('/') && !p.starts_with('!'),
        None => false,
    }
}

fn five_reason(cfg: &Config, five: &Window, now: u64) -> String {
    let reset = five
        .resets_at
        .map(|r| format!(" Resets in {}.", duration(r - now as i64)))
        .unwrap_or_default();
    format!(
        "usage-pacer: the 5-hour window is at {}% of a {}% ceiling.{reset} Stop and checkpoint; tools stay denied until the window rolls over.",
        format_pct(five.used_bp),
        format_pct(cfg.five_limit_bp)
    )
}

fn weekly_reason(pace: &Pace, now: u64) -> String {
    let credit_note = if pace.credit_bp > 0 {
        format!(" (+{}% borrowed)", format_pct(pace.credit_bp))
    } else {
        String::new()
    };
    format!(
        "usage-pacer: weekly usage is {}%, over the day-{} ceiling of {}%{credit_note}. Pause until {}. Do not retry tools; summarise progress and tell the user when work can resume.",
        format_pct(pace.used_bp),
        pace.day,
        format_pct(pace.ceiling_bp),
        next_event(pace, now)
    )
}

fn block_reason(pace: &Pace, now: u64) -> String {
    let credit_note = if pace.credit_bp > 0 {
        format!(" (+{}% borrowed)", format_pct(pace.credit_bp))
    } else {
        String::new()
    };
    format!(
        "usage-pacer: weekly usage {}% is over the day-{} ceiling of {}%{credit_note}. Work resumes when {}. Borrow more with `usage-pacer borrow <pct>`, raise PACER_DAILY_PERCENT, or set PACER_PACING=off (restart Claude Code after changing the environment).",
        format_pct(pace.used_bp),
        pace.day,
        format_pct(pace.ceiling_bp),
        next_event(pace, now)
    )
}

fn next_event(pace: &Pace, now: u64) -> String {
    let secs = (pace.next_epoch - now as i64).max(0);
    if pace.next_epoch >= pace.window_end {
        format!("the weekly window resets in {}", duration(secs))
    } else {
        format!("the next allowance unlocks in {}", duration(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_human_prompts_can_be_blocked() {
        assert!(is_human(Some("do the thing")));
        assert!(!is_human(Some("/usage")));
        assert!(!is_human(Some("!ls")));
        assert!(!is_human(None));
    }
}

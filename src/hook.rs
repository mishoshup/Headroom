use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::time::Duration;

use crate::config::{Config, FiveMode};
use crate::credit;
use crate::pace::Pace;
use crate::state::{State, Window};
use crate::util::{duration, format_pct, unix_now};

#[cfg(windows)]
const BORROW_BIN: &str = r"%USERPROFILE%\.claude\headroom\headroom.exe";
#[cfg(not(windows))]
const BORROW_BIN: &str = "~/.claude/headroom/headroom";

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

#[derive(Deserialize)]
struct ToolEnvelope {
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<ToolPayload>,
}

#[derive(Deserialize)]
struct ToolPayload {
    #[serde(default)]
    command: Option<String>,
}

fn stdin_string() -> String {
    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    raw
}

/// Only the binary's own maintenance commands are exempt from the guard.
/// Reject shell metacharacters and require the first token to be the headroom
/// binary, so this cannot be used to smuggle another command.
fn is_headroom_command(command: &str) -> bool {
    let command = command.trim();
    if command.is_empty()
        || command.chars().any(|c| {
            matches!(
                c,
                '\n' | '\r' | ';' | '&' | '|' | '<' | '>' | '`' | '$' | '(' | ')'
            )
        })
    {
        return false;
    }

    let mut parts = command.split_whitespace();
    let Some(exe) = parts.next() else {
        return false;
    };
    let Some(subcommand) = parts.next() else {
        return false;
    };
    let exe = exe.trim_matches(|c| c == '"' || c == '\'');
    let exe = exe.to_ascii_lowercase();

    (exe.ends_with("headroom") || exe.ends_with("headroom.exe"))
        && matches!(subcommand, "borrow" | "setup" | "doctor")
}

fn is_exempt_tool_call(raw: &str) -> bool {
    if !raw.contains("headroom") {
        return false;
    }
    let Ok(envelope) = serde_json::from_str::<ToolEnvelope>(raw) else {
        return false;
    };
    if envelope.tool_name.as_deref() != Some("Bash") {
        return false;
    }
    envelope
        .tool_input
        .and_then(|payload| payload.command)
        .as_deref()
        .map(is_headroom_command)
        .unwrap_or(false)
}

pub fn pretool(cfg: &Config) -> Decision {
    let raw = stdin_string();
    if is_exempt_tool_call(&raw) {
        return Decision::Allow;
    }

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
                            "headroom: the 5-hour window is at {}%; the reset is {} away, longer than the configured maximum wait of {}. The call is denied instead of sleeping partially. Retry after the reset, or set HEADROOM_FIVE_HOUR_MODE=block.",
                            format_pct(five.used_bp),
                            duration(nap),
                            duration(cfg.max_sleep as i64)
                        ));
                    }
                    eprintln!(
                        "headroom: 5h at {}%; sleeping {nap}s for the reset.",
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
        if let Some(base) = Pace::compute(cfg, &state, now, 0) {
            let (extra, _) = credit::active_bp(cfg, now, base.window_end, base.day);
            let pace = Pace::compute(cfg, &state, now, extra).unwrap_or(base);
            if pace.exhausted {
                return Decision::Deny(exhausted_reason(&pace, now));
            }
        }
    }

    Decision::Allow
}

pub fn prompt(cfg: &Config) -> PromptDecision {
    let raw = stdin_string();
    let prompt = serde_json::from_str::<PromptInput>(&raw)
        .ok()
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
            if let Some(base) = Pace::compute(cfg, &state, now, 0) {
                let (extra, _) = credit::active_bp(cfg, now, base.window_end, base.day);
                let pace = Pace::compute(cfg, &state, now, extra).unwrap_or(base);
                let credit_note = if pace.credit_bp > 0 {
                    format!(", +{}% borrowed", format_pct(pace.credit_bp))
                } else {
                    String::new()
                };
                if pace.exhausted {
                    parts.push(format!(
                        "today: 0% left{}; next allowance {}",
                        credit_note,
                        next_event(&pace, now)
                    ));
                    if cfg.block_prompts && is_human(prompt.as_deref()) {
                        return PromptDecision::Block(block_reason(&pace, now));
                    }
                } else {
                    let next = if pace.days_after > 0 {
                        format!(
                            "next {} days {}%/day",
                            pace.days_after,
                            format_pct(pace.future_per_day_bp)
                        )
                    } else {
                        "last day of the week".into()
                    };
                    parts.push(format!(
                        "today: {}% left{}; {}",
                        format_pct(pace.left_bp),
                        credit_note,
                        next
                    ));
                }
            }
        }
    }

    if parts.is_empty() {
        return PromptDecision::Allow(String::new());
    }
    PromptDecision::Allow(format!(
        "[headroom] {}. Keep steps small as the ceiling approaches.",
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
        "headroom: the 5-hour window is at {}% of a {}% ceiling.{reset} Stop and checkpoint; tools stay denied until the window rolls over.",
        format_pct(five.used_bp),
        format_pct(cfg.five_limit_bp)
    )
}

fn exhausted_reason(pace: &Pace, now: u64) -> String {
    let credit_note = if pace.credit_bp > 0 {
        format!(" (+{}% borrowed)", format_pct(pace.credit_bp))
    } else {
        String::new()
    };
    if pace.used_bp >= crate::pace::BP_MAX {
        format!(
            "headroom: the weekly budget is exhausted at {}%. It resets in {}. Do not retry tools; summarise progress and tell the user when work can resume.",
            format_pct(pace.used_bp),
            duration(pace.window_end - now as i64)
        )
    } else {
        format!(
            "headroom: today's allowance is used ({}% cap{credit_note}, {}% weekly used). Next allowance unlocks in {}. Do not retry tools; summarise progress and tell the user when work can resume. Borrow with `{BORROW_BIN} borrow <pct>`.",
            format_pct(pace.ceiling_bp),
            format_pct(pace.used_bp),
            duration(pace.next_epoch - now as i64)
        )
    }
}

fn block_reason(pace: &Pace, now: u64) -> String {
    let credit_note = if pace.credit_bp > 0 {
        format!(" (+{}% borrowed)", format_pct(pace.credit_bp))
    } else {
        String::new()
    };
    if pace.used_bp >= crate::pace::BP_MAX {
        format!(
            "headroom: the weekly budget is exhausted at 100%. It resets in {}. Nothing can be borrowed; work resumes after the reset.",
            duration(pace.window_end - now as i64)
        )
    } else {
        format!(
            "headroom: today's allowance is used ({}% cap{credit_note}, {}% weekly used). Work resumes when {}. To unblock now, run `{BORROW_BIN} borrow <pct>`.",
            format_pct(pace.ceiling_bp),
            format_pct(pace.used_bp),
            next_event(pace, now)
        )
    }
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

    #[test]
    fn headroom_commands_are_exempt() {
        assert!(is_headroom_command("~/.claude/headroom/headroom borrow 20"));
        assert!(is_headroom_command(
            r#""C:\Users\me\.claude\headroom\headroom.exe" borrow clear"#
        ));
        assert!(is_headroom_command(
            "/Users/me/.claude/plugins/cache/mishoshup/headroom/1.1.2/bin/headroom setup --force"
        ));
        assert!(is_headroom_command("headroom doctor"));
    }

    #[test]
    fn other_commands_are_not_exempt() {
        assert!(!is_headroom_command("ls"));
        assert!(!is_headroom_command("echo headroom borrow 20"));
        assert!(!is_headroom_command("headroom borrow 20; rm -rf /"));
        assert!(!is_headroom_command("headroom borrow 20 && rm -rf /"));
        assert!(!is_headroom_command("headroom borrow 20 | cat"));
        assert!(!is_headroom_command("$(headroom borrow 20)"));
        assert!(!is_headroom_command("headroom statusline"));
    }
}

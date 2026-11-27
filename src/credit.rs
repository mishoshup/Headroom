use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::{pct_to_bp, Config};
use crate::pace::{Pace, BP_MAX};
use crate::state::State;
use crate::util::{duration, format_pct, unix_now};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Day,
    Week,
}

/// A temporary credit added to today's allowance. Stored outside the usage
/// snapshot so statusline refreshes never clobber it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credit {
    pub window_reset: i64,
    pub day: u32,
    pub extra_bp: i64,
    pub expires_at: i64,
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: u64,
}

impl Credit {
    pub fn load(path: &Path) -> Option<Self> {
        let raw = fs::read(path).ok()?;
        serde_json::from_slice(&raw).ok()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let raw = serde_json::to_vec(self)?;
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        fs::write(&tmp, raw)?;
        #[cfg(windows)]
        let _ = fs::remove_file(path);
        fs::rename(&tmp, path)
    }

    pub fn clear(path: &Path) -> std::io::Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Credits are day-scoped now. `week` credits from older versions are only
    /// honoured on their creation day, then expire naturally.
    pub fn active_for(&self, now: u64, window_reset: i64, day: u32) -> bool {
        now < self.expires_at.max(0) as u64 && self.window_reset == window_reset && self.day == day
    }
}

/// Active credit for the current window/day, if any.
pub fn active_bp(cfg: &Config, now: u64, window_reset: i64, day: u32) -> (i64, Option<Credit>) {
    match Credit::load(&cfg.credit) {
        Some(c) if c.active_for(now, window_reset, day) => (c.extra_bp.max(0), Some(c)),
        _ => (0, None),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Applied {
    pub added_bp: i64,
    pub total_bp: i64,
    pub ceiling_bp: i64,
    pub left_bp: i64,
    pub days_after: u32,
    pub future_per_day_bp: i64,
    pub expires_at: i64,
}

/// Add `add_bp` to today's credit. Borrowing is additive and capped by the
/// amount still left for the days after today.
pub fn apply(
    cfg: &Config,
    state: &State,
    now: u64,
    add_bp: i64,
    note: Option<String>,
) -> Result<Applied, String> {
    if add_bp <= 0 {
        return Err("borrow must be greater than 0%".into());
    }
    if add_bp > BP_MAX {
        return Err("borrow cannot exceed 100% of the weekly limit".into());
    }

    let base = Pace::compute(cfg, state, now, 0)
        .ok_or("the weekly window has already rolled over; wait for the next statusline tick")?;
    let (existing_bp, existing) = active_bp(cfg, now, base.window_end, base.day);
    let total_bp = existing_bp + add_bp;
    let max_total = (BP_MAX - base.base_cap_bp).max(0);
    if total_bp > max_total {
        return Err(format!(
            "cannot borrow +{}%: today's allowance is {}% and only {}% remains for the rest of the week",
            format_pct(add_bp),
            format_pct(base.base_cap_bp),
            format_pct(max_total)
        ));
    }

    let expires_at = base.next_epoch;
    let note = note.or_else(|| existing.as_ref().and_then(|c| c.note.clone()));
    let credit = Credit {
        window_reset: base.window_end,
        day: base.day,
        extra_bp: total_bp,
        expires_at,
        scope: Scope::Day,
        note,
        created_at: now,
    };
    credit
        .save(&cfg.credit)
        .map_err(|e| format!("could not save credit: {e}"))?;

    let ceiling_bp = (base.base_cap_bp + total_bp).clamp(0, BP_MAX);
    Ok(Applied {
        added_bp: add_bp,
        total_bp,
        ceiling_bp,
        left_bp: (ceiling_bp - base.used_bp).max(0),
        days_after: base.days_after,
        future_per_day_bp: base.future_per_day_bp,
        expires_at,
    })
}

pub fn cli(cfg: &Config, args: &[String]) -> Result<String, String> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "clear" | "off" | "reset" => {
            Credit::clear(&cfg.credit).map_err(|e| format!("could not clear credit: {e}"))?;
            Ok("headroom: borrow cleared; today's normal allowance applies again.".into())
        }
        "status" | "show" => status_text(cfg),
        "help" | "--help" | "-h" => Err(usage()),
        raw => {
            let add_bp = pct_to_bp(raw).ok_or_else(|| format!("'{raw}' is not a percentage"))?;
            if add_bp <= 0 {
                return Err("borrow must be greater than 0%".into());
            }
            if add_bp > BP_MAX {
                return Err("borrow cannot exceed 100% of the weekly limit".into());
            }

            let mut note = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--note" => {
                        i += 1;
                        note = args.get(i).cloned();
                        if note.is_none() {
                            return Err("--note needs a value".into());
                        }
                    }
                    other => return Err(format!("unknown option '{other}'\n{}", usage())),
                }
                i += 1;
            }

            let now = unix_now();
            let state = State::load(&cfg.state).ok_or(
                "no usage snapshot yet; start Claude Code once so the statusline can capture it",
            )?;
            let base = Pace::compute(cfg, &state, now, 0)
                .ok_or("the weekly window is not available in the snapshot; run headroom doctor")?;
            let (existing_bp, _) = active_bp(cfg, now, base.window_end, base.day);
            let applied = apply(cfg, &state, now, add_bp, note)?;

            let mut out = format!(
                "headroom: added +{}% for today",
                format_pct(applied.added_bp)
            );
            if applied.total_bp != applied.added_bp {
                out.push_str(&format!(
                    " (total +{}% borrowed)",
                    format_pct(applied.total_bp)
                ));
            }
            out.push_str(&format!(
                ".\ntoday's allowance: {}% -> {}%.\n{}% left today.\n",
                format_pct(base.base_cap_bp + existing_bp),
                format_pct(applied.ceiling_bp),
                format_pct(applied.left_bp)
            ));
            if applied.days_after > 0 {
                out.push_str(&format!(
                    "next {} days {}%/day.\n",
                    applied.days_after,
                    format_pct(applied.future_per_day_bp)
                ));
            } else {
                out.push_str("last day of the week.\n");
            }
            out.push_str(&format!(
                "expires in {}.",
                duration(applied.expires_at - now as i64)
            ));
            Ok(out)
        }
    }
}

pub fn status_text(cfg: &Config) -> Result<String, String> {
    let now = unix_now();
    let Some(state) = State::load(&cfg.state) else {
        return Ok(format!(
            "headroom: no usage snapshot yet at {}. Start Claude Code once, then run headroom doctor.",
            cfg.state.display()
        ));
    };
    let Some(base) = Pace::compute(cfg, &state, now, 0) else {
        return Ok(no_weekly_message(&state, now));
    };
    let (extra, credit) = active_bp(cfg, now, base.window_end, base.day);
    let ceiling = (base.base_cap_bp + extra).clamp(0, BP_MAX);
    let left = (ceiling - base.used_bp).max(0);

    let mut out = String::new();
    match credit {
        Some(c) => out.push_str(&format!(
            "headroom: +{}% borrowed today (expires in {}).\n",
            format_pct(extra),
            duration(c.expires_at - now as i64)
        )),
        None => out.push_str("headroom: no active borrow.\n"),
    }
    out.push_str(&format!(
        "weekly usage {}%.\ntoday: {}% left.\n",
        format_pct(base.used_bp),
        format_pct(left)
    ));
    if base.days_after > 0 {
        out.push_str(&format!(
            "next {} days {}%/day.",
            base.days_after,
            format_pct(base.future_per_day_bp)
        ));
    } else {
        out.push_str("last day of the week.");
    }
    Ok(out)
}

fn usage() -> String {
    "usage: headroom borrow <percent> [--note text]\n\
     \x20      headroom borrow status\n\
     \x20      headroom borrow clear"
        .into()
}

fn no_weekly_message(state: &State, now: u64) -> String {
    let age = now.saturating_sub(state.captured_at);
    let five = state
        .five_hour
        .map(|window| {
            format!(
                "5h {}%{}",
                format_pct(window.used_bp),
                window
                    .resets_at
                    .map(|reset| format!(", resets in {}", duration(reset - now as i64)))
                    .unwrap_or_default()
            )
        })
        .unwrap_or_else(|| "no windows".into());
    format!(
        "headroom: the weekly window is not in the latest snapshot (captured {age}s ago; {five}). Wait for the next statusline tick or run headroom doctor."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credit(scope: Scope, day: u32, expires: i64) -> Credit {
        Credit {
            window_reset: 1_000,
            day,
            extra_bp: 2_000,
            expires_at: expires,
            scope,
            note: None,
            created_at: 0,
        }
    }

    #[test]
    fn credit_expires_and_matches_day() {
        let c = credit(Scope::Day, 3, 500);
        assert!(c.active_for(400, 1_000, 3));
        assert!(!c.active_for(400, 1_000, 4));
        assert!(!c.active_for(500, 1_000, 3));
        assert!(!c.active_for(400, 999, 3));
    }

    #[test]
    fn old_week_credit_is_day_scoped_now() {
        let c = credit(Scope::Week, 3, 1_000);
        assert!(c.active_for(400, 1_000, 3));
        assert!(!c.active_for(400, 1_000, 4));
    }
}

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::{pct_to_bp, Config};
use crate::pace::Pace;
use crate::state::State;
use crate::util::{duration, format_pct, unix_now};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Day,
    Week,
}

/// A temporary credit added to the pace ceiling. Stored outside the usage
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

    pub fn active_for(&self, now: u64, window_reset: i64, day: u32) -> bool {
        now < self.expires_at.max(0) as u64
            && self.window_reset == window_reset
            && (self.scope == Scope::Week || self.day == day)
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
    pub scope: Scope,
    pub expires_at: i64,
}

/// Add `add_bp` to the active credit and persist it. Borrowing is additive:
/// repeating it stacks until the weekly limit is reached.
pub fn apply(
    cfg: &Config,
    now: u64,
    reset: i64,
    base: &Pace,
    add_bp: i64,
    want_week: bool,
    note: Option<String>,
) -> Result<Applied, String> {
    if add_bp <= 0 {
        return Err("borrow must be greater than 0%".into());
    }
    if add_bp > 10_000 {
        return Err("borrow cannot exceed 100% of the weekly limit".into());
    }

    let (existing_bp, existing) = active_bp(cfg, now, reset, base.day);
    let total_bp = (existing_bp + add_bp).clamp(0, 10_000);
    let scope = if want_week
        || existing
            .as_ref()
            .map(|c| c.scope == Scope::Week)
            .unwrap_or(false)
    {
        Scope::Week
    } else {
        Scope::Day
    };

    let mut expires_at = match scope {
        Scope::Week => reset,
        Scope::Day => base.next_epoch,
    };
    if let Some(c) = &existing {
        expires_at = expires_at.max(c.expires_at);
    }

    let note = note.or_else(|| existing.as_ref().and_then(|c| c.note.clone()));
    let credit = Credit {
        window_reset: reset,
        day: base.day,
        extra_bp: total_bp,
        expires_at,
        scope,
        note,
        created_at: now,
    };
    credit
        .save(&cfg.credit)
        .map_err(|e| format!("could not save credit: {e}"))?;

    Ok(Applied {
        added_bp: add_bp,
        total_bp,
        ceiling_bp: (base.ceiling_bp + total_bp).clamp(0, 10_000),
        scope,
        expires_at,
    })
}

pub fn cli(cfg: &Config, args: &[String]) -> Result<String, String> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "clear" | "off" | "reset" => {
            Credit::clear(&cfg.credit).map_err(|e| format!("could not clear credit: {e}"))?;
            Ok("usage-pacer: override cleared; the normal daily pace applies again.".into())
        }
        "status" | "show" => status_text(cfg),
        "help" | "--help" | "-h" => Err(usage()),
        raw => {
            let add_bp = pct_to_bp(raw).ok_or_else(|| format!("'{raw}' is not a percentage"))?;
            if add_bp <= 0 {
                return Err("borrow must be greater than 0%".into());
            }
            if add_bp > 10_000 {
                return Err("borrow cannot exceed 100% of the weekly limit".into());
            }

            let mut want_week = false;
            let mut note = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--week" | "--until-reset" => want_week = true,
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
            let week = state
                .seven_day
                .ok_or("the weekly window is not exposed for this account")?;
            let reset = week
                .resets_at
                .ok_or("the snapshot has no weekly reset time")?;
            let base = Pace::compute(cfg, week.used_bp, reset, now, 0).ok_or(
                "the weekly window has already rolled over; wait for the next statusline tick",
            )?;
            let previous =
                (base.ceiling_bp + active_bp(cfg, now, reset, base.day).0).clamp(0, 10_000);
            let applied = apply(cfg, now, reset, &base, add_bp, want_week, note)?;

            let scope_text = match applied.scope {
                Scope::Week => "the rest of the weekly window",
                Scope::Day => "today",
            };
            let mut out = format!(
                "usage-pacer: added +{}% for {scope_text}",
                format_pct(applied.added_bp)
            );
            if applied.total_bp != applied.added_bp {
                out.push_str(&format!(
                    " (total +{}% borrowed).\n",
                    format_pct(applied.total_bp)
                ));
            } else {
                out.push_str(".\n");
            }
            out.push_str(&format!(
                "today's limit: {}% -> {}%.\n",
                format_pct(previous),
                format_pct(applied.ceiling_bp)
            ));
            let delta = week.used_bp - applied.ceiling_bp;
            if delta > 0 {
                out.push_str(&format!(
                    "You are still {}% over today.\n",
                    format_pct(delta)
                ));
            } else {
                out.push_str(&format!(
                    "You now have {}% left today.\n",
                    format_pct(-delta)
                ));
            }
            out.push_str(&format!(
                "Expires in {}. Future days keep their normal limits, so unused allowance repays the borrow.",
                duration(applied.expires_at - now as i64),
            ));
            Ok(out)
        }
    }
}

pub fn status_text(cfg: &Config) -> Result<String, String> {
    let now = unix_now();
    let state = State::load(&cfg.state)
        .ok_or("no usage snapshot yet; start Claude Code once so the statusline can capture it")?;
    let week = state
        .seven_day
        .ok_or("the weekly window is not exposed for this account")?;
    let reset = week
        .resets_at
        .ok_or("the snapshot has no weekly reset time")?;
    let base = Pace::compute(cfg, week.used_bp, reset, now, 0)
        .ok_or("the weekly window has already rolled over")?;
    let (extra, credit) = active_bp(cfg, now, reset, base.day);

    let Some(c) = credit else {
        let delta = week.used_bp - base.ceiling_bp;
        return Ok(if delta > 0 {
            format!(
                "usage-pacer: no active override. Today's limit is {}%; you are {}% over. Weekly usage {}%.",
                format_pct(base.ceiling_bp),
                format_pct(delta),
                format_pct(week.used_bp)
            )
        } else {
            format!(
                "usage-pacer: no active override. Today's limit is {}%; {}% left. Weekly usage {}%.",
                format_pct(base.ceiling_bp),
                format_pct(-delta),
                format_pct(week.used_bp)
            )
        });
    };

    let scope_text = match c.scope {
        Scope::Week => "rest of week",
        Scope::Day => "today",
    };
    let limit = (base.ceiling_bp + extra).clamp(0, 10_000);
    let delta = week.used_bp - limit;
    let standing = if delta > 0 {
        format!("{}% over", format_pct(delta))
    } else {
        format!("{}% left", format_pct(-delta))
    };
    Ok(format!(
        "usage-pacer: +{}% borrowed for {scope_text} (expires in {}).\n\
         Today's limit is {}%; {standing}. Weekly usage {}%{}.",
        format_pct(extra),
        duration(c.expires_at - now as i64),
        format_pct(limit),
        format_pct(week.used_bp),
        c.note.map(|n| format!(", note: {n}")).unwrap_or_default()
    ))
}

fn usage() -> String {
    "usage: usage-pacer borrow <percent> [--week] [--note text]\n\
     \x20      usage-pacer borrow status\n\
     \x20      usage-pacer borrow clear"
        .into()
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
    fn day_credit_expires_and_matches_day() {
        let c = credit(Scope::Day, 3, 500);
        assert!(c.active_for(400, 1_000, 3));
        assert!(!c.active_for(400, 1_000, 4));
        assert!(!c.active_for(500, 1_000, 3));
        assert!(!c.active_for(400, 999, 3));
    }

    #[test]
    fn week_credit_ignores_day() {
        let c = credit(Scope::Week, 3, 1_000);
        assert!(c.active_for(400, 1_000, 5));
        assert!(!c.active_for(1_000, 1_000, 5));
    }
}

use crate::config::Config;
use crate::state::{State, Window};

pub const BP_MAX: i64 = 10_000;
const DAY_SECS: i64 = 86_400;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pace {
    pub day: u32,
    pub week_days: u32,
    pub used_bp: i64,
    /// Today's allowance before any borrow, fixed at the start of the day.
    pub base_cap_bp: i64,
    pub credit_bp: i64,
    /// Today's cap including borrow.
    pub ceiling_bp: i64,
    /// ceiling - used, never below zero.
    pub left_bp: i64,
    /// Days after today.
    pub days_after: u32,
    /// Remaining weekly budget divided over the days after today.
    pub future_per_day_bp: i64,
    pub exhausted: bool,
    pub next_epoch: i64,
    pub window_end: i64,
}

/// Day number in the weekly window, anchored at `resets_at`.
pub fn day_number(resets_at: i64, now: u64, week_days: u32) -> Option<u32> {
    if resets_at <= now as i64 {
        return None;
    }
    let week_s = week_days as i64 * DAY_SECS;
    let start = resets_at - week_s;
    let elapsed = (now as i64 - start).clamp(0, week_s - 1);
    Some((elapsed / DAY_SECS + 1).clamp(1, week_days as i64) as u32)
}

/// Persist today's fixed allowance. It is recomputed only when the day or
/// weekly window changes, so spending during the day cannot move the cap.
pub fn refresh_state(state: &mut State, week_days: u32, now: u64) {
    let Some(week) = state.seven_day else {
        clear_day(state);
        return;
    };
    let Some(reset) = week.resets_at else {
        clear_day(state);
        return;
    };
    let Some(day) = day_number(reset, now, week_days) else {
        clear_day(state);
        return;
    };
    if state.day == day
        && state.week_days == week_days
        && state.day_reset == reset
        && state.day_cap_bp > 0
    {
        return;
    }

    let days_left = week_days.saturating_sub(day) + 1;
    let remaining = (BP_MAX - week.used_bp).clamp(0, BP_MAX);
    state.day = day;
    state.day_reset = reset;
    state.week_days = week_days;
    state.day_cap_bp = (week.used_bp + remaining / days_left as i64).clamp(0, BP_MAX);
}

fn clear_day(state: &mut State) {
    state.day = 0;
    state.day_cap_bp = 0;
    state.day_reset = 0;
    state.week_days = 0;
}

impl Pace {
    pub fn compute(cfg: &Config, state: &State, now: u64, credit_bp: i64) -> Option<Pace> {
        let week: Window = state.seven_day?;
        let reset = week.resets_at?;
        let day = day_number(reset, now, cfg.week_days)?;
        let used_bp = week.used_bp.clamp(0, BP_MAX);
        let days_left = cfg.week_days.saturating_sub(day) + 1;
        let days_after = cfg.week_days.saturating_sub(day);

        let stored = state.day == day
            && state.week_days == cfg.week_days
            && state.day_reset == reset
            && state.day_cap_bp > 0;
        let base_cap_bp = if stored {
            state.day_cap_bp.clamp(0, BP_MAX)
        } else {
            let remaining = (BP_MAX - used_bp).clamp(0, BP_MAX);
            (used_bp + remaining / days_left as i64).clamp(0, BP_MAX)
        };

        let credit_bp = credit_bp.clamp(0, BP_MAX);
        let ceiling_bp = (base_cap_bp + credit_bp).clamp(0, BP_MAX);
        let left_bp = (ceiling_bp - used_bp).max(0);
        let future_per_day_bp = if days_after > 0 {
            (BP_MAX - used_bp).max(0) / days_after as i64
        } else {
            0
        };
        let week_s = cfg.week_days as i64 * DAY_SECS;
        let start = reset - week_s;
        let next_epoch = (start + day as i64 * DAY_SECS).min(reset);

        Some(Pace {
            day,
            week_days: cfg.week_days,
            used_bp,
            base_cap_bp,
            credit_bp: ceiling_bp.saturating_sub(base_cap_bp),
            ceiling_bp,
            left_bp,
            days_after,
            future_per_day_bp,
            exhausted: left_bp == 0 || used_bp >= BP_MAX,
            next_epoch,
            window_end: reset,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ColorMode, FiveMode};
    use std::path::PathBuf;

    fn cfg(week_days: u32) -> Config {
        Config {
            state: PathBuf::new(),
            credit: PathBuf::new(),
            week_days,
            stale_after: 900,
            five_limit_bp: 9_800,
            five_mode: FiveMode::Block,
            reset_buffer: 60,
            max_sleep: 19_800,
            pacing: true,
            block_prompts: true,
            color: ColorMode::None,
            ascii: false,
            bottom_margin: 0,
        }
    }

    fn state(used_bp: i64, day_cap_bp: i64, day: u32, reset: i64) -> State {
        State {
            captured_at: NOW,
            seven_day: Some(Window {
                used_bp,
                resets_at: Some(reset),
            }),
            day,
            day_cap_bp,
            day_reset: reset,
            week_days: 7,
            ..State::default()
        }
    }

    const NOW: u64 = 1_000_000;
    const DAY: i64 = 86_400;

    #[test]
    fn new_week_starts_with_an_even_share() {
        let c = cfg(7);
        let reset = NOW as i64 + 7 * DAY;
        let s = state(0, 1_429, 1, reset);
        let p = Pace::compute(&c, &s, NOW, 0).unwrap();
        assert_eq!(p.day, 1);
        assert_eq!(p.base_cap_bp, 1_429);
        assert_eq!(p.left_bp, 1_429);
        assert_eq!(p.future_per_day_bp, 1_666); // 10000 / 6
    }

    #[test]
    fn used_budget_redistributes_to_remaining_days() {
        let c = cfg(7);
        let reset = NOW as i64 + 7 * DAY;
        let s = state(200, 1_429, 1, reset);
        let p = Pace::compute(&c, &s, NOW, 0).unwrap();
        assert_eq!(p.day, 1);
        assert_eq!(p.left_bp, 1_229);
        assert_eq!(p.future_per_day_bp, 1_633); // 9800 / 6
    }

    #[test]
    fn day_cap_is_fixed_within_the_day() {
        let c = cfg(7);
        let reset = NOW as i64 + 5 * DAY;
        let s = state(6_200, 6_800, 3, reset);
        let p = Pace::compute(&c, &s, NOW, 0).unwrap();
        assert_eq!(p.base_cap_bp, 6_800);
        assert_eq!(p.left_bp, 600);
        assert_eq!(p.future_per_day_bp, 950); // 3800 / 4
    }

    #[test]
    fn borrow_adds_to_todays_allowance_only() {
        let c = cfg(7);
        let reset = NOW as i64 + 5 * DAY;
        let s = state(6_000, 6_800, 3, reset);
        let p = Pace::compute(&c, &s, NOW, 2_000).unwrap();
        assert_eq!(p.base_cap_bp, 6_800);
        assert_eq!(p.credit_bp, 2_000);
        assert_eq!(p.ceiling_bp, 8_800);
        assert_eq!(p.left_bp, 2_800);
        assert_eq!(p.future_per_day_bp, 1_000); // 4000 / 4
    }

    #[test]
    fn cap_is_clamped_to_the_weekly_limit() {
        let c = cfg(7);
        let reset = NOW as i64 + DAY;
        let s = state(9_000, 9_800, 6, reset);
        let p = Pace::compute(&c, &s, NOW, 5_000).unwrap();
        assert_eq!(p.ceiling_bp, 10_000);
        assert_eq!(p.left_bp, 1_000);
    }

    #[test]
    fn exhausted_when_todays_cap_is_used() {
        let c = cfg(7);
        let reset = NOW as i64 + 4 * DAY;
        let s = state(6_800, 6_800, 4, reset);
        let p = Pace::compute(&c, &s, NOW, 0).unwrap();
        assert_eq!(p.left_bp, 0);
        assert!(p.exhausted);
    }

    #[test]
    fn expired_window_returns_none() {
        let c = cfg(7);
        let s = state(0, 1_429, 1, NOW as i64 - 1);
        assert!(Pace::compute(&c, &s, NOW, 0).is_none());
    }

    #[test]
    fn refresh_state_recomputes_on_a_new_day() {
        let reset = NOW as i64 + 5 * DAY;
        let mut s = State {
            captured_at: NOW,
            seven_day: Some(Window {
                used_bp: 6_000,
                resets_at: Some(reset),
            }),
            day: 2,
            day_cap_bp: 2_857,
            day_reset: reset,
            week_days: 7,
            ..State::default()
        };
        refresh_state(&mut s, 7, NOW);
        assert_eq!(s.day, 3);
        assert_eq!(s.day_cap_bp, 6_800);
    }
}

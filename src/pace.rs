use crate::config::Config;

pub const BP_MAX: i64 = 10_000;
const DAY_SECS: i64 = 86_400;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pace {
    pub day: u32,
    pub week_days: u32,
    pub used_bp: i64,
    pub allowance_bp: i64,
    pub credit_bp: i64,
    pub ceiling_bp: i64,
    pub over: bool,
    pub next_epoch: i64,
    pub window_end: i64,
}

impl Pace {
    /// Weekly pace for `now`. Day N gets N days of allowance (unused
    /// allowance carries forward), plus grace and any active borrow credit.
    pub fn compute(
        cfg: &Config,
        used_bp: i64,
        resets_at: i64,
        now: u64,
        credit_bp: i64,
    ) -> Option<Pace> {
        if resets_at <= now as i64 {
            return None;
        }

        let week_s = cfg.week_days as i64 * DAY_SECS;
        let start = resets_at - week_s;
        let elapsed = (now as i64 - start).clamp(0, week_s - 1);
        let day = (elapsed / DAY_SECS + 1).clamp(1, cfg.week_days as i64) as u32;

        let allowance = (cfg.daily_bp() * day as i64).clamp(0, BP_MAX);
        let credit = credit_bp.max(0);
        let ceiling = (allowance + cfg.grace_bp + credit).clamp(0, BP_MAX);
        let next = (start + day as i64 * DAY_SECS).min(resets_at);

        Some(Pace {
            day,
            week_days: cfg.week_days,
            used_bp,
            allowance_bp: allowance,
            credit_bp: credit.min(BP_MAX),
            ceiling_bp: ceiling,
            over: used_bp > ceiling,
            next_epoch: next,
            window_end: resets_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ColorMode, FiveMode};
    use std::path::PathBuf;

    fn cfg(week_days: u32, daily_bp: Option<i64>, grace_bp: i64) -> Config {
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
            daily_bp,
            target_bp: 10_000,
            grace_bp,
            color: ColorMode::None,
            ascii: false,
            bottom_margin: 1,
        }
    }

    const NOW: u64 = 1_000_000;
    const DAY: i64 = 86_400;

    #[test]
    fn day_number_and_ceiling_follow_the_window() {
        let c = cfg(7, Some(1_429), 0);
        let reset = NOW as i64 + 3 * DAY + DAY / 2; // 3.5 days left -> day 4

        let p = Pace::compute(&c, 5_000, reset, NOW, 0).unwrap();
        assert_eq!(p.day, 4);
        assert_eq!(p.allowance_bp, 5_716);
        assert_eq!(p.ceiling_bp, 5_716);
        assert!(!p.over);
    }

    #[test]
    fn credit_raises_only_the_ceiling() {
        let c = cfg(7, Some(1_429), 0);
        let reset = NOW as i64 + 3 * DAY + DAY / 2;
        let p = Pace::compute(&c, 6_000, reset, NOW, 2_000).unwrap();
        assert_eq!(p.allowance_bp, 5_716);
        assert_eq!(p.credit_bp, 2_000);
        assert_eq!(p.ceiling_bp, 7_716);
        assert!(!p.over);
    }

    #[test]
    fn ceiling_is_clamped_to_the_weekly_limit() {
        let c = cfg(7, Some(5_000), 500);
        let reset = NOW as i64 + DAY + 1;
        let p = Pace::compute(&c, 9_000, reset, NOW, 5_000).unwrap();
        assert_eq!(p.ceiling_bp, 10_000);
    }

    #[test]
    fn expired_window_returns_none() {
        let c = cfg(7, None, 0);
        assert!(Pace::compute(&c, 0, NOW as i64 - 1, NOW, 0).is_none());
    }

    #[test]
    fn round_target_splits_evenly() {
        let c = cfg(7, None, 0);
        assert_eq!(c.daily_bp(), 1_429);
    }
}

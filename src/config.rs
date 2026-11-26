use std::env;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiveMode {
    Block,
    Wait,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub state: PathBuf,
    pub credit: PathBuf,
    pub week_days: u32,
    pub stale_after: u64,
    pub five_limit_bp: i64,
    pub five_mode: FiveMode,
    pub reset_buffer: u64,
    pub max_sleep: u64,
    pub pacing: bool,
    pub block_prompts: bool,
    pub daily_bp: Option<i64>,
    pub target_bp: i64,
    pub grace_bp: i64,
    pub color: ColorMode,
    pub ascii: bool,
    pub bottom_margin: u32,
}

impl Config {
    pub fn from_env() -> Self {
        Self::load(true)
    }

    /// Hooks do not render anything, so skip colour/terminal detection and the
    /// bottom-margin environment reads on the per-tool-call path.
    pub fn for_hooks() -> Self {
        Self::load(false)
    }

    fn load(render: bool) -> Self {
        let home = env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let pacer_home = env::var_os("HEADROOM_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude").join("headroom"));
        let state = env::var_os("HEADROOM_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| pacer_home.join("state"));
        let credit = env::var_os("HEADROOM_CREDIT")
            .map(PathBuf::from)
            .unwrap_or_else(|| pacer_home.join("credit.json"));

        let week_days = env_u64("HEADROOM_WEEK_DAYS", 7).clamp(1, 31) as u32;
        let stale_after = env_u64("HEADROOM_STALE_AFTER", 900);
        let five_limit_bp = env_bp("HEADROOM_FIVE_HOUR_LIMIT").unwrap_or(9_800);
        let five_mode = match env::var("HEADROOM_FIVE_HOUR_MODE")
            .unwrap_or_default()
            .as_str()
        {
            "wait" | "sleep" => FiveMode::Wait,
            _ => FiveMode::Block,
        };
        let reset_buffer = env_u64("HEADROOM_RESET_BUFFER", 60);
        let max_sleep = env_u64("HEADROOM_MAX_SLEEP", 19_800);
        let pacing = env_bool("HEADROOM_PACING", true);
        let block_prompts = env_bool("HEADROOM_BLOCK_PROMPTS", true);
        let daily_bp = env_bp("HEADROOM_DAILY_PERCENT");
        let target_bp = env_bp("HEADROOM_WEEKLY_TARGET").unwrap_or(10_000);
        let grace_bp = env_bp("HEADROOM_PACING_GRACE").unwrap_or(0);
        let (color, ascii, bottom_margin) = if render {
            let color = detect_color();
            let ascii = env_bool("HEADROOM_ASCII", false)
                || env::var("TERM").map(|t| t == "dumb").unwrap_or(false);
            let bottom_margin = env_u64("HEADROOM_BOTTOM_MARGIN", 1).min(3) as u32;
            (color, ascii, bottom_margin)
        } else {
            (ColorMode::None, false, 0)
        };

        Self {
            state,
            credit,
            week_days,
            stale_after,
            five_limit_bp,
            five_mode,
            reset_buffer,
            max_sleep,
            pacing,
            block_prompts,
            daily_bp,
            target_bp,
            grace_bp,
            color,
            ascii,
            bottom_margin,
        }
    }

    pub fn daily_bp(&self) -> i64 {
        self.daily_bp.unwrap_or_else(|| {
            let days = self.week_days as i64;
            (self.target_bp + days / 2) / days
        })
    }
}

pub fn pct_to_bp(raw: &str) -> Option<i64> {
    let v: f64 = raw.trim().parse().ok()?;
    if !v.is_finite() || v < 0.0 {
        return None;
    }
    Some((v * 100.0).round() as i64)
}

fn env_bp(name: &str) -> Option<i64> {
    env::var(name).ok().and_then(|v| pct_to_bp(&v))
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no" | ""
        ),
        Err(_) => default,
    }
}

fn detect_color() -> ColorMode {
    if env::var_os("NO_COLOR")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return ColorMode::None;
    }

    match env::var("HEADROOM_COLOR")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "off" | "never" | "0" | "false" | "no" => return ColorMode::None,
        "16" | "ansi16" => return ColorMode::Ansi16,
        "256" | "ansi256" => return ColorMode::Ansi256,
        "truecolor" | "24bit" => return ColorMode::TrueColor,
        _ => {}
    }

    if env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
        return ColorMode::None;
    }
    let colorterm = env::var("COLORTERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return ColorMode::TrueColor;
    }
    let term = env::var("TERM").unwrap_or_default();
    if term.contains("256color") {
        ColorMode::Ansi256
    } else {
        ColorMode::Ansi16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_percentages() {
        assert_eq!(pct_to_bp("45"), Some(4500));
        assert_eq!(pct_to_bp("14.29"), Some(1429));
        assert_eq!(pct_to_bp(" 0.5 "), Some(50));
        assert_eq!(pct_to_bp("-1"), None);
        assert_eq!(pct_to_bp("junk"), None);
    }

    #[test]
    fn default_daily_is_rounded() {
        let cfg = Config {
            state: PathBuf::new(),
            credit: PathBuf::new(),
            week_days: 7,
            stale_after: 900,
            five_limit_bp: 9800,
            five_mode: FiveMode::Block,
            reset_buffer: 60,
            max_sleep: 19800,
            pacing: true,
            block_prompts: true,
            daily_bp: None,
            target_bp: 10_000,
            grace_bp: 0,
            color: ColorMode::None,
            ascii: false,
            bottom_margin: 1,
        };
        assert_eq!(cfg.daily_bp(), 1429);
    }
}

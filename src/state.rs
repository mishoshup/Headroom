use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Window {
    pub used_bp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub captured_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<Window>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day: Option<Window>,
}

impl State {
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

    pub fn is_fresh(&self, now: u64, stale_after: u64) -> bool {
        now.saturating_sub(self.captured_at) <= stale_after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_tolerates_missing_windows() {
        let state = State {
            captured_at: 1_795_000_000,
            five_hour: None,
            seven_day: Some(Window {
                used_bp: 4_500,
                resets_at: Some(1_796_000_000),
            }),
        };
        let raw = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.captured_at, state.captured_at);
        assert_eq!(back.seven_day.unwrap().used_bp, 4_500);
        assert!(back.five_hour.is_none());
    }

    #[test]
    fn freshness_is_inclusive_and_saturating() {
        let state = State {
            captured_at: 100,
            ..State::default()
        };
        assert!(state.is_fresh(1_000, 900));
        assert!(!state.is_fresh(1_001, 900));
        assert!(state.is_fresh(50, 900));
    }
}

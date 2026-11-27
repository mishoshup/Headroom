use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::credit::Credit;
use crate::state::State;
use crate::util::{duration, format_pct, unix_now};

pub fn run(cfg: &Config) -> Result<(), String> {
    let now = unix_now();

    println!("headroom {}", env!("CARGO_PKG_VERSION"));
    match env::current_exe() {
        Ok(path) => println!("binary: {}", path.display()),
        Err(error) => println!("binary: unavailable ({error})"),
    }
    println!("state: {}", cfg.state.display());
    println!("credit: {}", cfg.credit.display());

    match State::load(&cfg.state) {
        Some(state) => {
            println!("snapshot age: {}s", now.saturating_sub(state.captured_at));
            print_window("five_hour", state.five_hour, now);
            print_window("seven_day", state.seven_day, now);
        }
        None => println!("snapshot: missing"),
    }

    match Credit::load(&cfg.credit) {
        Some(credit) => println!(
            "credit: +{}% for {:?}, expires in {}",
            format_pct(credit.extra_bp),
            credit.scope,
            duration(credit.expires_at - now as i64)
        ),
        None => println!("credit: none"),
    }

    let settings = settings_path();
    println!("settings: {}", settings.display());
    match fs::read(&settings)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
    {
        Some(value) => {
            let command = value
                .get("statusLine")
                .and_then(|line| line.get("command"))
                .and_then(Value::as_str)
                .unwrap_or("(none)");
            println!("statusLine: {command}");
        }
        None => println!("statusLine: unable to read settings"),
    }

    let mut overrides: Vec<(String, String)> = env::vars()
        .filter(|(key, _)| key.starts_with("HEADROOM_"))
        .collect();
    overrides.sort();
    if overrides.is_empty() {
        println!("overrides: none");
    } else {
        for (key, value) in overrides {
            println!("override: {key}={value}");
        }
    }

    Ok(())
}

fn print_window(name: &str, window: Option<crate::state::Window>, now: u64) {
    match window {
        Some(window) => println!(
            "{name}: {}% used, resets in {}",
            format_pct(window.used_bp),
            window
                .resets_at
                .map(|reset| duration(reset - now as i64))
                .unwrap_or_else(|| "unknown".into())
        ),
        None => println!("{name}: absent"),
    }
}

fn settings_path() -> PathBuf {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
        .join("settings.json")
}

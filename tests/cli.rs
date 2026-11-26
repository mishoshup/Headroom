use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn home(name: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("usage-pacer-{name}-{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_state(home: &Path, captured: u64, five: Option<(i64, i64)>, week: Option<(i64, i64)>) {
    let mut value = json!({ "captured_at": captured });
    if let Some((used, reset)) = five {
        value["five_hour"] = json!({ "used_bp": used, "resets_at": reset });
    }
    if let Some((used, reset)) = week {
        value["seven_day"] = json!({ "used_bp": used, "resets_at": reset });
    }
    fs::write(home.join("state"), serde_json::to_vec(&value).unwrap()).unwrap();
}

fn run(home: &Path, args: &[&str], stdin: Option<&str>, envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_usage-pacer"));
    command
        .args(args)
        .env("PACER_HOME", home)
        .env("PACER_COLOR", "off")
        .env_remove("NO_COLOR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = command.spawn().unwrap();
    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn statusline_payload(five_reset: i64, week_reset: i64) -> String {
    format!(
        r#"{{"model":{{"display_name":"Sonnet"}},"context_window":{{"used_percentage":42.4}},"rate_limits":{{"five_hour":{{"used_percentage":12,"resets_at":{five_reset}}},"seven_day":{{"used_percentage":45,"resets_at":{week_reset}}}}}}}"#
    )
}

#[test]
fn weekly_pace_allows_and_denies() {
    let home = home("weekly");
    let reset = now() as i64 + 3 * 86_400 + 43_200; // day 4

    write_state(&home, now(), None, Some((5_000, reset)));
    let out = run(&home, &["pretool"], None, &[]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());

    write_state(&home, now(), None, Some((6_000, reset)));
    let out = run(&home, &["pretool"], None, &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(r#""permissionDecision":"deny""#));
    assert!(text.contains("day-4 ceiling"));

    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        parsed["hookSpecificOutput"]["hookEventName"],
        serde_json::json!("PreToolUse")
    );
    assert_eq!(
        parsed["hookSpecificOutput"]["permissionDecision"],
        serde_json::json!("deny")
    );
    assert!(parsed["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .contains("weekly usage"));
}

#[test]
fn custom_daily_percent_and_grace_apply() {
    let home = home("custom");
    let reset = now() as i64 + 3 * 86_400 + 43_200;

    write_state(&home, now(), None, Some((4_500, reset)));
    let out = run(&home, &["pretool"], None, &[("PACER_DAILY_PERCENT", "10")]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("deny"));

    let out = run(&home, &["pretool"], None, &[("PACER_PACING_GRACE", "6")]);
    assert!(out.stdout.is_empty());
}

#[test]
fn five_hour_ceiling_and_stale_state() {
    let home = home("five");
    let reset = now() as i64 + 3_600;

    write_state(&home, now(), Some((9_900, reset)), None);
    let out = run(&home, &["pretool"], None, &[]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("5-hour window"));

    let out = run(
        &home,
        &["pretool"],
        None,
        &[("PACER_FIVE_HOUR_MODE", "block")],
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("deny"));

    write_state(&home, now() - 1_000, Some((9_900, reset)), None);
    let out = run(&home, &["pretool"], None, &[]);
    assert!(out.stdout.is_empty());
}

#[test]
fn wait_mode_denies_when_reset_exceeds_max_wait() {
    let home = home("wait-cap");
    let reset = now() as i64 + 3_600;
    write_state(&home, now(), Some((9_900, reset)), None);

    let out = run(
        &home,
        &["pretool"],
        None,
        &[("PACER_FIVE_HOUR_MODE", "wait"), ("PACER_MAX_SLEEP", "1")],
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(r#""permissionDecision":"deny""#));
    assert!(text.contains("maximum wait"));
}

#[test]
fn prompt_blocks_and_slash_commands_pass() {
    let home = home("prompt");
    let reset = now() as i64 + 3 * 86_400 + 43_200;
    write_state(&home, now(), None, Some((6_000, reset)));

    let out = run(&home, &["prompt"], Some(r#"{"prompt":"do work"}"#), &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage-pacer"));

    let out = run(&home, &["prompt"], Some(r#"{"prompt":"/usage"}"#), &[]);
    assert!(out.status.success());

    let out = run(
        &home,
        &["prompt"],
        Some(r#"{"prompt":"do work"}"#),
        &[("PACER_BLOCK_PROMPTS", "0")],
    );
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("OVER"));
}

#[test]
fn statusline_writes_state_and_borrow_lifts_the_ceiling() {
    let home = home("borrow");
    let week_reset = now() as i64 + 3 * 86_400 + 43_200;
    let five_reset = now() as i64 + 3_600;

    write_state(&home, now(), None, Some((4_500, week_reset)));
    let out = run(&home, &["borrow", "20"], None, &[]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("added +20%"));

    let out = run(&home, &["borrow", "5"], None, &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("total +25%"));
    assert!(text.contains("82.16%"));

    let payload = statusline_payload(five_reset, week_reset);
    let out = run(&home, &["statusline"], Some(&payload), &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Sonnet"));
    assert!(text.contains("today"));
    assert!(text.contains("54.77%"));
    assert!(text.contains("37.16% left of weekly"));
    assert!(text.contains("+25% borrowed"));

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.join("state")).unwrap()).unwrap();
    assert_eq!(state["seven_day"]["used_bp"], 4_500);
    assert_eq!(state["seven_day"]["resets_at"], week_reset);

    let out = run(&home, &["borrow", "clear"], None, &[]);
    assert!(out.status.success());
    let out = run(&home, &["statusline"], Some(&payload), &[]);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("borrowed"));
}

#[test]
fn self_test_passes() {
    let home = home("selftest");
    let out = run(&home, &["self-test"], None, &[]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("self-test passed"));
}

#[test]
fn ui_falls_back_to_status_without_a_tty() {
    let home = home("ui-fallback");
    let reset = now() as i64 + 3 * 86_400 + 43_200;
    write_state(&home, now(), None, Some((4_500, reset)));

    let out = run(&home, &[], None, &[]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("no active override"));

    let out = run(&home, &["borrow"], None, &[]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("no active override"));
}

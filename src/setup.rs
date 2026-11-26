use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::util::unix_now;

/// Wire the status line into Claude Code's settings without requiring jq.
pub fn run(args: &[String]) -> Result<(), String> {
    let force = args.iter().any(|arg| arg == "--force" || arg == "-f");
    let refresh = env::var("HEADROOM_REFRESH")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(60);

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let config_dir = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let settings_path = config_dir.join("settings.json");

    let exe = env::current_exe().map_err(|e| format!("could not resolve the binary path: {e}"))?;
    let exe_str = exe.to_string_lossy();
    let command = format!("{exe_str} statusline");

    let mut settings: Value = if settings_path.exists() {
        let raw = fs::read(&settings_path)
            .map_err(|e| format!("could not read {}: {e}", settings_path.display()))?;
        serde_json::from_slice(&raw)
            .map_err(|e| format!("{} is not valid JSON: {e}", settings_path.display()))?
    } else {
        json!({})
    };
    if !settings.is_object() {
        return Err(format!(
            "{} is not a JSON object; refusing to edit it",
            settings_path.display()
        ));
    }

    let existing = settings
        .get("statusLine")
        .and_then(|value| value.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if !existing.is_empty() && existing != command && !force {
        println!("headroom: a different statusLine is already configured:");
        println!("  {existing}");
        println!("headroom: leaving it alone. Re-run with --force to replace it, or set:");
        println!(
            "  \"statusLine\": {{\"type\": \"command\", \"command\": \"{command}\", \"refreshInterval\": {refresh}}}"
        );
        return Ok(());
    }

    let backup = if settings_path.exists() {
        let backup = settings_path.with_extension(format!("json.bak.{}", unix_now()));
        fs::copy(&settings_path, &backup).map_err(|e| {
            format!(
                "could not back up {} to {}: {e}",
                settings_path.display(),
                backup.display()
            )
        })?;
        Some(backup)
    } else {
        None
    };

    settings["statusLine"] = json!({
        "type": "command",
        "command": command,
        "refreshInterval": refresh
    });

    if let Some(dir) = settings_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let tmp = settings_path.with_extension(format!("json.tmp.{}", std::process::id()));
    let raw = serde_json::to_vec_pretty(&settings)
        .map_err(|e| format!("could not serialise settings: {e}"))?;
    fs::write(&tmp, raw).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    #[cfg(windows)]
    let _ = fs::remove_file(&settings_path);
    fs::rename(&tmp, &settings_path).map_err(|e| {
        format!(
            "could not replace {} with {}: {e}",
            settings_path.display(),
            tmp.display()
        )
    })?;

    println!(
        "headroom: statusline wired to {} (refresh {refresh}s)",
        exe.display()
    );
    if let Some(backup) = backup {
        println!("headroom: settings backup at {}", backup.display());
    }
    Ok(())
}

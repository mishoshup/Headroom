mod config;
mod credit;
mod hook;
mod pace;
mod render;
mod state;
mod statusline;
mod style;
mod util;

use serde_json::json;
use std::process::ExitCode;

use config::Config;
use hook::{Decision, PromptDecision};

fn main() -> ExitCode {
    let cfg = Config::from_env();
    let command = std::env::args().nth(1).unwrap_or_default();

    match command.as_str() {
        "statusline" => {
            println!("{}", statusline::run(&cfg));
            ExitCode::SUCCESS
        }
        "pretool" => {
            if let Decision::Deny(reason) = hook::pretool(&cfg) {
                let out = json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": reason
                    }
                });
                println!("{out}");
            }
            ExitCode::SUCCESS
        }
        "prompt" => match hook::prompt(&cfg) {
            PromptDecision::Allow(context) => {
                if !context.is_empty() {
                    println!("{context}");
                }
                ExitCode::SUCCESS
            }
            PromptDecision::Block(message) => {
                eprintln!("{message}");
                ExitCode::from(2)
            }
        },
        "borrow" | "override" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            match credit::cli(&cfg, &args) {
                Ok(message) => {
                    println!("{message}");
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::FAILURE
                }
            }
        }
        "self-test" => self_test(),
        "help" | "--help" | "-h" => {
            println!(
                "usage: usage-pacer <statusline|pretool|prompt|borrow|self-test>\n\
                 \x20      usage-pacer borrow <percent> [--week] [--note text]\n\
                 \x20      usage-pacer borrow status\n\
                 \x20      usage-pacer borrow clear"
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: usage-pacer <statusline|pretool|prompt|borrow|self-test>");
            ExitCode::from(64)
        }
    }
}

fn self_test() -> ExitCode {
    let checks = [
        (util::format_pct(4_286), "42.86"),
        (util::format_pct(4_500), "45"),
        (util::format_pct(14_200), "100"),
        (util::duration(90_061), "1d 1h"),
        (util::duration(3_600), "1h 0m"),
        (util::duration(59), "0m"),
    ];
    for (got, want) in checks {
        if got != want {
            eprintln!("self-test failed: got '{got}', want '{want}'");
            return ExitCode::FAILURE;
        }
    }
    println!("self-test passed");
    ExitCode::SUCCESS
}

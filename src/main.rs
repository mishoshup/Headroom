mod config;
mod credit;
mod hook;
mod pace;
mod render;
mod state;
mod statusline;
mod style;
mod ui;
mod util;

use serde_json::json;
use std::process::ExitCode;

use config::Config;
use hook::{Decision, PromptDecision};

fn main() -> ExitCode {
    let command = std::env::args().nth(1).unwrap_or_default();

    match command.as_str() {
        "" | "ui" => run_ui(&Config::from_env()),
        "statusline" => {
            println!("{}", statusline::run(&Config::from_env()));
            ExitCode::SUCCESS
        }
        "pretool" => {
            if let Decision::Deny(reason) = hook::pretool(&Config::for_hooks()) {
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
        "prompt" => match hook::prompt(&Config::for_hooks()) {
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
            if args.is_empty() {
                return run_ui(&Config::from_env());
            }
            match credit::cli(&Config::for_hooks(), &args) {
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
                "usage: usage-pacer [ui]              interactive borrow panel\n\
                 \x20      usage-pacer statusline       statusline renderer\n\
                 \x20      usage-pacer borrow <pct>     borrow headroom non-interactively\n\
                 \x20      usage-pacer borrow status    show the active credit\n\
                 \x20      usage-pacer borrow clear     remove the credit\n\
                 \x20      usage-pacer self-test        maths sanity checks"
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: usage-pacer [ui|statusline|borrow|self-test]");
            ExitCode::from(64)
        }
    }
}

fn run_ui(cfg: &Config) -> ExitCode {
    match ui::run(cfg) {
        Ok(Some(message)) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
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

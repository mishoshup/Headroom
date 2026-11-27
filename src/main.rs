mod config;
mod credit;
mod doctor;
mod hook;
mod pace;
mod render;
mod setup;
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
        "setup" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            match setup::run(&args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("headroom: {message}");
                    ExitCode::FAILURE
                }
            }
        }
        "doctor" => match doctor::run(&Config::for_hooks()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("headroom: {message}");
                ExitCode::FAILURE
            }
        },
        "self-test" => self_test(),
        "help" | "--help" | "-h" => {
            println!(
                "usage: headroom [ui]              interactive borrow panel\n\
                 \x20      headroom statusline       statusline renderer\n\
                 \x20      headroom borrow <pct>     borrow headroom non-interactively\n\
                 \x20      headroom borrow status    show the active credit\n\
                 \x20      headroom borrow clear     remove the credit\n\
                 \x20      headroom setup            wire the statusline into settings.json\n\
                 \x20      headroom doctor           show paths, snapshot and configuration\n\
                 \x20      headroom self-test        maths sanity checks"
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: headroom [ui|statusline|borrow|setup|doctor|self-test]");
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

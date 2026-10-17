//! macsetup: a terminal dashboard for a macOS setup engine.
//!
//! This is the v0.0.1 stub: it finds the engine and checks its API version.

mod config;
mod engine;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            println!("macsetup {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            println!(
                "macsetup {}\n\nTerminal dashboard for a macOS setup engine.\n\n\
                 Usage: macsetup [--version | --help]\n\n\
                 The engine is found via $MACSETUP_REPO, then `repo = \"…\"` in\n\
                 ~/.config/macsetup/config.toml, then `chezmoi source-path`.",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("macsetup: unknown argument '{other}' (try --help)");
            ExitCode::from(2)
        }
        None => run(),
    }
}

fn run() -> ExitCode {
    let (repo, source) = match config::find_repo(&config::Inputs::from_env()) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("macsetup: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("engine: {} (from {})", repo.display(), source.describe());
    match engine::api(&repo) {
        Ok((raw, info)) => {
            println!("{raw}");
            println!("engine API {}, dashboard expects {}", info.api, engine::API);
            if info.api == engine::API {
                ExitCode::SUCCESS
            } else {
                eprintln!("macsetup: API mismatch; update whichever side is older");
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("macsetup: {e}");
            ExitCode::FAILURE
        }
    }
}

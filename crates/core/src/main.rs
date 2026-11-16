//! Binary entrypoint for `nexdeck-agent`.

use std::process::ExitCode;

use nexdeck_core::{AGENT_VERSION, AgentConfig, CoreAgent};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") if args.next().is_none() => {
            println!("nexdeck-agent {AGENT_VERSION}");
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") if args.next().is_none() => {
            println!("NexDeck Core Agent {AGENT_VERSION}");
            println!("Usage: nexdeck-agent [--version | --help | --check-config]");
            ExitCode::SUCCESS
        }
        Some("--check-config") if args.next().is_none() => {
            match CoreAgent::new(AgentConfig::default()) {
                Ok(_) => {
                    println!("nexdeck-agent {AGENT_VERSION}: default configuration valid");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("Configuration error: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        None => {
            println!(
                "NexDeck Core Agent v{AGENT_VERSION} (Stage 0 foundation; runtime loop planned for Stage 1)"
            );
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown or unexpected argument: {other}");
            eprintln!("Try 'nexdeck-agent --help' for usage.");
            ExitCode::from(2)
        }
    }
}

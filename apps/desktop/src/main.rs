//! Entrypoint for the pure-Rust NexDeck Desktop Manager (`apps/desktop`).
//!
//! Per ADR-0002, this runs as a separate Windows 11 GUI process communicating
//! with `nexdeck-agent` over local IPC/API.

use std::process::ExitCode;

use nexdeck_api::ApiPermission;

const DESKTOP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") if args.next().is_none() => {
            println!("nexdeck-desktop {DESKTOP_VERSION}");
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") if args.next().is_none() => {
            println!("NexDeck Desktop Manager {DESKTOP_VERSION}");
            println!("Usage: nexdeck-desktop [--version | --help]");
            ExitCode::SUCCESS
        }
        None => {
            let _required = [
                ApiPermission::ReadMetrics,
                ApiPermission::InteractPlugins,
                ApiPermission::ExecuteAllowedAction,
            ];
            println!(
                "NexDeck Desktop Manager v{DESKTOP_VERSION} (Stage 0 skeleton; pure-Rust UI planned in Stage 1-3)"
            );
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown or unexpected argument: {other}");
            eprintln!("Try 'nexdeck-desktop --help' for usage.");
            ExitCode::from(2)
        }
    }
}

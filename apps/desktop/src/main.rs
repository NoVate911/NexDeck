//! Pure-Rust Desktop Manager GUI entrypoint for NexDeck.

mod state;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use nexdeck_core::storage::StoragePaths;

use crate::state::DesktopManagerState;
use crate::ui::NexDeckDesktopApp;

const DESKTOP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
enum DesktopCliCommand {
    Version,
    Help,
    LaunchGui { config_dir: Option<PathBuf> },
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(DesktopCliCommand::Version) => {
            println!("nexdeck-desktop {DESKTOP_VERSION}");
            ExitCode::SUCCESS
        }
        Ok(DesktopCliCommand::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(DesktopCliCommand::LaunchGui { config_dir }) => launch_gui(config_dir),
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("Try 'nexdeck-desktop --help' for usage.");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: &[String]) -> Result<DesktopCliCommand, String> {
    if args.is_empty() {
        return Ok(DesktopCliCommand::LaunchGui { config_dir: None });
    }

    match args[0].as_str() {
        "--version" | "-V" if args.len() == 1 => Ok(DesktopCliCommand::Version),
        "--help" | "-h" if args.len() == 1 => Ok(DesktopCliCommand::Help),
        "--config-dir" if args.len() == 2 => Ok(DesktopCliCommand::LaunchGui {
            config_dir: Some(PathBuf::from(&args[1])),
        }),
        other => Err(format!("Unknown or unexpected argument: {other}")),
    }
}

fn print_help() {
    println!("NexDeck Desktop Manager {DESKTOP_VERSION}");
    println!("Usage:");
    println!("  nexdeck-desktop [--config-dir <dir>]");
    println!("  nexdeck-desktop --version | --help");
}

fn launch_gui(config_dir: Option<PathBuf>) -> ExitCode {
    let paths = match StoragePaths::resolve(config_dir.as_deref()) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("Failed to resolve NexDeck config directory: {err}");
            return ExitCode::FAILURE;
        }
    };

    let state = match DesktopManagerState::load(paths) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("Failed to load NexDeck state: {err}");
            return ExitCode::FAILURE;
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("NexDeck Desktop {DESKTOP_VERSION}"))
            .with_inner_size([1160.0, 740.0])
            .with_min_inner_size([880.0, 560.0]),
        ..Default::default()
    };

    match eframe::run_native(
        "NexDeck Desktop",
        native_options,
        Box::new(move |cc| Ok(Box::new(NexDeckDesktopApp::new(cc, state)))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Desktop GUI failed to start: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parser_supports_headless_flags_and_config_override() {
        assert_eq!(
            parse_args(&[]),
            Ok(DesktopCliCommand::LaunchGui { config_dir: None })
        );
        assert_eq!(
            parse_args(&["--version".to_owned()]),
            Ok(DesktopCliCommand::Version)
        );
        assert_eq!(parse_args(&["-h".to_owned()]), Ok(DesktopCliCommand::Help));
        assert_eq!(
            parse_args(&["--config-dir".to_owned(), "example-dir".to_owned()]),
            Ok(DesktopCliCommand::LaunchGui {
                config_dir: Some(PathBuf::from("example-dir")),
            })
        );
        assert!(parse_args(&["--unknown".to_owned()]).is_err());
    }
}

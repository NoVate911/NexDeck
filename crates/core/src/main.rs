//! Binary entrypoint for `nexdeck-agent`.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use nexdeck_monitoring::default_collector;
use nexdeck_transport::PROTOCOL_VERSION;

use nexdeck_core::autostart::{
    install_current_exe_autostart, query_autostart, uninstall_autostart,
};
use nexdeck_core::process_metrics::{current_process_memory, format_bytes_mib};
use nexdeck_core::runtime::{RuntimeOptions, spawn_agent_runtime};
use nexdeck_core::storage::{AgentSettings, StoragePaths};
use nexdeck_core::{AGENT_VERSION, AgentConfig, CoreAgent, TARGET_IDLE_RAM_BUDGET_BYTES};

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Version,
    Help,
    CheckConfig {
        config_dir: Option<PathBuf>,
    },
    Daemon {
        config_dir: Option<PathBuf>,
        bind_override: Option<SocketAddr>,
        pair_on_start: bool,
        pair_ttl_secs: u64,
    },
    InstallAutostart {
        bind_override: Option<SocketAddr>,
    },
    UninstallAutostart,
    AutostartStatus,
    MeasureRam,
    Summary,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(cmd) => match execute(cmd) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Error: {err}");
                ExitCode::FAILURE
            }
        },
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("Try 'nexdeck-agent --help' for usage.");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: &[String]) -> Result<CliCommand, String> {
    if args.is_empty() {
        return Ok(CliCommand::Summary);
    }

    match args[0].as_str() {
        "--version" | "-V" if args.len() == 1 => Ok(CliCommand::Version),
        "--help" | "-h" if args.len() == 1 => Ok(CliCommand::Help),
        "--uninstall-autostart" if args.len() == 1 => Ok(CliCommand::UninstallAutostart),
        "--autostart-status" if args.len() == 1 => Ok(CliCommand::AutostartStatus),
        "--measure-ram" if args.len() == 1 => Ok(CliCommand::MeasureRam),
        "--check-config" => parse_check_config(&args[1..]),
        "--install-autostart" => parse_install_autostart(&args[1..]),
        "--daemon" => parse_daemon(&args[1..]),
        "--pair" => Err(
            "Standalone --pair requires --daemon (local IPC pairing lands in Stage 3).\n\
             Run: nexdeck-agent --daemon --pair"
                .to_owned(),
        ),
        other => Err(format!("Unknown or unexpected argument: {other}")),
    }
}

fn parse_check_config(rest: &[String]) -> Result<CliCommand, String> {
    let mut config_dir = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--config-dir" => {
                let value = next_flag_value(rest, &mut i, "--config-dir")?;
                config_dir = Some(PathBuf::from(value));
            }
            other => return Err(format!("Unexpected argument for --check-config: {other}")),
        }
        i += 1;
    }
    Ok(CliCommand::CheckConfig { config_dir })
}

fn parse_install_autostart(rest: &[String]) -> Result<CliCommand, String> {
    let mut bind_override = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--bind" => {
                let value = next_flag_value(rest, &mut i, "--bind")?;
                let parsed: SocketAddr = value
                    .parse()
                    .map_err(|err| format!("Invalid --bind socket address '{value}': {err}"))?;
                bind_override = Some(parsed);
            }
            other => {
                return Err(format!(
                    "Unexpected argument for --install-autostart: {other}"
                ));
            }
        }
        i += 1;
    }
    Ok(CliCommand::InstallAutostart { bind_override })
}

fn parse_daemon(rest: &[String]) -> Result<CliCommand, String> {
    let mut config_dir = None;
    let mut bind_override = None;
    let mut pair_on_start = false;
    let mut pair_ttl_secs = 300_u64;
    let mut i = 0;

    while i < rest.len() {
        match rest[i].as_str() {
            "--config-dir" => {
                let value = next_flag_value(rest, &mut i, "--config-dir")?;
                config_dir = Some(PathBuf::from(value));
            }
            "--bind" => {
                let value = next_flag_value(rest, &mut i, "--bind")?;
                let parsed: SocketAddr = value
                    .parse()
                    .map_err(|err| format!("Invalid --bind socket address '{value}': {err}"))?;
                bind_override = Some(parsed);
            }
            "--pair" => {
                pair_on_start = true;
            }
            "--ttl" => {
                let value = next_flag_value(rest, &mut i, "--ttl")?;
                let parsed: u64 = value
                    .parse()
                    .map_err(|err| format!("Invalid --ttl seconds '{value}': {err}"))?;
                if parsed == 0 {
                    return Err("--ttl must be greater than zero".to_owned());
                }
                pair_ttl_secs = parsed;
            }
            other => return Err(format!("Unexpected argument for --daemon: {other}")),
        }
        i += 1;
    }

    Ok(CliCommand::Daemon {
        config_dir,
        bind_override,
        pair_on_start,
        pair_ttl_secs,
    })
}

fn next_flag_value<'a>(
    args: &'a [String],
    index: &mut usize,
    flag: &str,
) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("Missing value after {flag}"))
}

fn execute(cmd: CliCommand) -> Result<(), String> {
    match cmd {
        CliCommand::Version => {
            println!("nexdeck-agent {AGENT_VERSION}");
            Ok(())
        }
        CliCommand::Help => {
            print_help();
            Ok(())
        }
        CliCommand::Summary => {
            println!("NexDeck Core Agent v{AGENT_VERSION} (protocol v{PROTOCOL_VERSION})");
            println!("Run 'nexdeck-agent --daemon' to start the background WSS service,");
            println!("or 'nexdeck-agent --help' for all CLI options.");
            Ok(())
        }
        CliCommand::CheckConfig { config_dir } => run_check_config(config_dir.as_deref()),
        CliCommand::Daemon {
            config_dir,
            bind_override,
            pair_on_start,
            pair_ttl_secs,
        } => run_daemon(
            config_dir.as_deref(),
            bind_override,
            pair_on_start,
            pair_ttl_secs,
        ),
        CliCommand::InstallAutostart { bind_override } => {
            let registered =
                install_current_exe_autostart(bind_override).map_err(|err| err.to_string())?;
            println!("Autostart registered: {registered}");
            Ok(())
        }
        CliCommand::UninstallAutostart => {
            let removed = uninstall_autostart().map_err(|err| err.to_string())?;
            if removed {
                println!("Removed NexDeckAgent autostart entry.");
            } else {
                println!("No NexDeckAgent autostart entry was registered.");
            }
            Ok(())
        }
        CliCommand::AutostartStatus => {
            match query_autostart().map_err(|err| err.to_string())? {
                Some(command) => println!("Autostart: enabled ({command})"),
                None => println!("Autostart: disabled"),
            }
            Ok(())
        }
        CliCommand::MeasureRam => run_measure_ram(),
    }
}

fn print_help() {
    println!("NexDeck Core Agent {AGENT_VERSION}");
    println!("Usage:");
    println!(
        "  nexdeck-agent --daemon [--bind <ip:port>] [--config-dir <dir>] [--pair] [--ttl <secs>]"
    );
    println!("  nexdeck-agent --check-config [--config-dir <dir>]");
    println!("  nexdeck-agent --install-autostart [--bind <ip:port>]");
    println!("  nexdeck-agent --uninstall-autostart");
    println!("  nexdeck-agent --autostart-status");
    println!("  nexdeck-agent --measure-ram");
    println!("  nexdeck-agent --version | --help");
}

fn run_check_config(config_dir: Option<&std::path::Path>) -> Result<(), String> {
    if config_dir.is_none()
        && std::env::var_os("LOCALAPPDATA").is_none()
        && std::env::var_os("XDG_DATA_HOME").is_none()
        && std::env::var_os("HOME").is_none()
    {
        CoreAgent::new(AgentConfig::default()).map_err(|err| err.to_string())?;
        println!("nexdeck-agent {AGENT_VERSION}: default configuration valid");
        return Ok(());
    }

    let paths = StoragePaths::resolve(config_dir).map_err(|err| err.to_string())?;
    let settings = if paths.config_path().exists() {
        paths
            .load_or_init_settings()
            .map_err(|err| err.to_string())?
    } else {
        AgentSettings::default()
    };
    let bind = settings.validate().map_err(|err| err.to_string())?;
    println!(
        "nexdeck-agent {AGENT_VERSION}: configuration valid (bind={bind}, poll_ms={}, data_dir={})",
        settings.agent.poll_interval_ms,
        paths.root_dir().display()
    );
    Ok(())
}

fn run_measure_ram() -> Result<(), String> {
    let mut agent = CoreAgent::new(AgentConfig::default()).map_err(|err| err.to_string())?;
    if let Ok(mut collector) = default_collector()
        && let Ok(snapshot) = collector.collect()
    {
        agent.record_snapshot(snapshot);
    }

    match current_process_memory() {
        Some(stats) => {
            println!("Process memory: {stats}");
            println!(
                "Target idle budget: {}",
                format_bytes_mib(TARGET_IDLE_RAM_BUDGET_BYTES as u64)
            );
            Ok(())
        }
        None => {
            println!("Process memory metrics are unavailable on this OS target.");
            Ok(())
        }
    }
}

fn run_daemon(
    config_dir: Option<&std::path::Path>,
    bind_override: Option<SocketAddr>,
    pair_on_start: bool,
    pair_ttl_secs: u64,
) -> Result<(), String> {
    let paths = StoragePaths::resolve(config_dir).map_err(|err| err.to_string())?;
    let mut settings = paths
        .load_or_init_settings()
        .map_err(|err| err.to_string())?;
    let bind_addr = match bind_override {
        Some(addr) => {
            let ip_text = addr.ip().to_string();
            if !settings.san_hosts.iter().any(|host| host == &ip_text) {
                settings.san_hosts.push(ip_text);
            }
            addr
        }
        None => settings.validate().map_err(|err| err.to_string())?,
    };

    let devices = paths.load_devices().map_err(|err| err.to_string())?;
    let tls = paths
        .load_or_create_tls_identity(&settings.san_hosts)
        .map_err(|err| err.to_string())?;
    let collector = default_collector().map_err(|err| err.to_string())?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to build Tokio runtime: {err}"))?;

    rt.block_on(async move {
        let options = RuntimeOptions {
            bind_addr,
            san_hosts: settings.san_hosts.clone(),
            agent: settings.agent.clone(),
        };
        let agent = spawn_agent_runtime(options, collector, tls, Some(paths.clone()), devices)
            .await
            .map_err(|err| err.to_string())?;

        println!(
            "NexDeck Core Agent v{AGENT_VERSION} listening on wss://{}/ws",
            agent.local_addr()
        );
        println!("TLS SHA-256 fingerprint: {}", agent.fingerprint_sha256());
        println!("Data directory: {}", paths.root_dir().display());

        if pair_on_start {
            let advertised_host = agent.local_addr().ip().to_string();
            let payload = agent
                .start_pairing(advertised_host, pair_ttl_secs)
                .await
                .map_err(|err| err.to_string())?;
            let json = serde_json::to_string_pretty(&payload)
                .map_err(|err| format!("failed to encode pairing payload: {err}"))?;
            println!(
                "Pairing enabled (PIN: {}, TTL: {}s):",
                payload.pin, pair_ttl_secs
            );
            println!("{json}");
        }

        if let Some(stats) = current_process_memory() {
            println!("Startup memory: {stats}");
        }
        println!("Press Ctrl+C to shut down.");

        tokio::signal::ctrl_c()
            .await
            .map_err(|err| format!("failed awaiting Ctrl+C signal: {err}"))?;

        println!("Shutting down NexDeck Core Agent...");
        agent.shutdown().await.map_err(|err| err.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn parses_cli_flags_cleanly() {
        assert_eq!(parse_args(&[]), Ok(CliCommand::Summary));
        assert_eq!(
            parse_args(&["--version".to_owned()]),
            Ok(CliCommand::Version)
        );
        assert_eq!(parse_args(&["--help".to_owned()]), Ok(CliCommand::Help));
        assert_eq!(
            parse_args(&["--measure-ram".to_owned()]),
            Ok(CliCommand::MeasureRam)
        );
        assert_eq!(
            parse_args(&[
                "--daemon".to_owned(),
                "--bind".to_owned(),
                "127.0.0.1:9445".to_owned(),
                "--pair".to_owned(),
                "--ttl".to_owned(),
                "120".to_owned(),
            ]),
            Ok(CliCommand::Daemon {
                config_dir: None,
                bind_override: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9445)),
                pair_on_start: true,
                pair_ttl_secs: 120,
            })
        );
    }

    #[test]
    fn rejects_pair_without_daemon_and_zero_ttl() {
        assert!(parse_args(&["--pair".to_owned()]).is_err());
        assert!(parse_args(&["--daemon".to_owned(), "--ttl".to_owned(), "0".to_owned()]).is_err());
        assert!(parse_args(&["--unknown".to_owned()]).is_err());
    }
}

mod alert;
mod app;
mod collector;
mod config;
mod export;
mod state;
mod ui;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::app::App;
use crate::config::Config;
use crate::state::SharedState;

#[derive(Parser, Debug)]
#[command(
    name = "resource-monitor",
    version,
    about = "TUI system resource monitor"
)]
struct Cli {
    /// Path to a TOML config file (overrides the XDG default).
    #[arg(long)]
    config: Option<PathBuf>,

    /// Enable verbose logging (equivalent to RUST_LOG=debug).
    #[arg(long)]
    debug: bool,

    /// Enable GPU monitoring (macOS Apple Silicon: spawns `sudo powermetrics`).
    /// You'll be prompted to authenticate with sudo before the TUI starts.
    #[arg(long)]
    gpu: bool,

    /// Expose a Prometheus `/metrics` endpoint at the given address.
    /// Disabled by default to keep idle resource usage low.
    /// Example: `--prometheus 127.0.0.1:9091`.
    #[arg(long, value_name = "ADDR:PORT")]
    prometheus: Option<SocketAddr>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.debug)?;

    let config = Config::load(cli.config.as_deref()).context("failed to load config")?;
    let alert_rules = config
        .alert_rules()
        .context("validating alert rules in config")?;
    tracing::info!(
        sample_interval_ms = config.sample_interval_ms,
        history_capacity = config.history_capacity,
        ui_tick_ms = config.ui_tick_ms,
        alert_count = alert_rules.len(),
        "loaded config"
    );

    let gpu_enabled = if cli.gpu { ensure_gpu_prereqs() } else { false };

    let state: SharedState = Arc::new(state::State::new(config.history_capacity));

    let exporter = if let Some(addr) = cli.prometheus {
        match export::Exporter::start(state.clone(), addr) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("Could not start Prometheus exporter on {addr}: {e}");
                None
            }
        }
    } else {
        None
    };

    let mut app = App::new(state, config, gpu_enabled, alert_rules);
    let result = app.run();
    drop(exporter); // graceful shutdown of the /metrics server
    result
}

fn init_tracing(debug: bool) -> Result<()> {
    let default_level = if debug { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing init: {e}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn ensure_gpu_prereqs() -> bool {
    use std::process::Command;
    eprintln!("GPU monitoring needs sudo to run powermetrics. Authenticating...");
    match Command::new("sudo").arg("-v").status() {
        Ok(s) if s.success() => true,
        Ok(_) => {
            eprintln!("sudo authentication failed. Continuing without GPU.");
            false
        }
        Err(e) => {
            eprintln!("Could not invoke sudo ({e}). Continuing without GPU.");
            false
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_gpu_prereqs() -> bool {
    eprintln!("--gpu is currently macOS-only (Apple Silicon). Continuing without GPU.");
    false
}

//! Container stats via the `docker` CLI.
//!
//! Phase 4 v1 deliberately avoids pulling in `bollard` + `tokio` just for
//! the Docker API. We shell out to `docker stats --no-stream --format json`
//! from a dedicated poller thread; the sampler reads cached results so the
//! ~150ms docker call doesn't bottleneck the 1Hz tick.
//!
//! cgroup-based PID grouping (Linux) and a real Docker API client live in
//! Phase 4.5 and beyond — see TODO.md.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{CollectCtx, Collector};
use crate::state::ContainerSnapshot;

/// How often the poller hits `docker stats`. Fast enough that container
/// changes show up within a couple of seconds, slow enough to keep the
/// docker daemon load negligible.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// If the poller hasn't refreshed in this long, treat the cached state as
/// stale and clear it (likely the docker daemon went away).
const STALE_AFTER: Duration = Duration::from_secs(10);

#[derive(Default, Clone)]
struct PollerState {
    available: bool,
    containers: Vec<ContainerSnapshot>,
    last_update: Option<Instant>,
}

pub struct ContainerCollector {
    state: Arc<Mutex<PollerState>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ContainerCollector {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(PollerState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let s = state.clone();
        let sh = shutdown.clone();
        let handle = thread::Builder::new()
            .name("container-poller".into())
            .spawn(move || poller_loop(s, sh))
            .expect("spawn container poller");
        Self {
            state,
            shutdown,
            handle: Some(handle),
        }
    }
}

impl Default for ContainerCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ContainerCollector {
    fn name(&self) -> &'static str {
        "container"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let s = self.state.lock().expect("container state poisoned").clone();
        let fresh = s
            .last_update
            .is_some_and(|t| Instant::now().duration_since(t) < STALE_AFTER);
        let available = s.available && fresh;
        ctx.snapshot
            .set("container.available", if available { 1.0 } else { 0.0 });
        ctx.snapshot
            .set("container.count", s.containers.len() as f64);
        ctx.snapshot.containers = if available { s.containers } else { Vec::new() };
        Ok(())
    }
}

impl Drop for ContainerCollector {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn poller_loop(state: Arc<Mutex<PollerState>>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::SeqCst) {
        let started = Instant::now();
        match query_docker() {
            Ok(containers) => {
                let mut s = state.lock().expect("container state poisoned");
                s.containers = containers;
                s.available = true;
                s.last_update = Some(Instant::now());
            }
            Err(e) => {
                tracing::debug!(error = %e, "docker stats query failed");
                let mut s = state.lock().expect("container state poisoned");
                s.available = false;
                s.containers.clear();
                // Don't update last_update so the staleness check trips and
                // the UI can distinguish "docker dead" from "no containers".
            }
        }
        let elapsed = started.elapsed();
        sleep_chunked(POLL_INTERVAL.saturating_sub(elapsed), &shutdown);
    }
}

fn sleep_chunked(total: Duration, shutdown: &AtomicBool) {
    let chunk = Duration::from_millis(100);
    let mut remaining = total;
    while remaining > Duration::ZERO {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        let step = remaining.min(chunk);
        thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}

fn query_docker() -> Result<Vec<ContainerSnapshot>> {
    let output = Command::new("docker")
        .args(["stats", "--no-stream", "--format", "{{json .}}"])
        .output()
        .context("running `docker stats`")?;
    if !output.status.success() {
        anyhow::bail!("docker stats exited {:?}", output.status.code());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Some(c) => out.push(c),
            None => tracing::debug!(line = %line, "unparseable docker line"),
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct DockerStatsRow<'a> {
    #[serde(rename = "Name")]
    name: &'a str,
    #[serde(rename = "CPUPerc")]
    cpu_perc: &'a str,
    #[serde(rename = "MemUsage")]
    mem_usage: &'a str,
    #[serde(rename = "MemPerc")]
    mem_perc: &'a str,
    #[serde(rename = "NetIO")]
    net_io: &'a str,
}

fn parse_line(line: &str) -> Option<ContainerSnapshot> {
    let row: DockerStatsRow = serde_json::from_str(line).ok()?;
    Some(ContainerSnapshot {
        name: row.name.to_string(),
        cpu_percent: parse_percent(row.cpu_perc).unwrap_or(0.0),
        mem_bytes: split_first_bytes(row.mem_usage).unwrap_or(0),
        mem_percent: parse_percent(row.mem_perc).unwrap_or(0.0),
        net_rx_bytes: split_first_bytes(row.net_io).unwrap_or(0),
        net_tx_bytes: split_second_bytes(row.net_io).unwrap_or(0),
    })
}

fn parse_percent(s: &str) -> Option<f64> {
    s.trim().trim_end_matches('%').parse().ok()
}

/// "49.7MiB / 15.6GiB" → 49.7MiB
fn split_first_bytes(s: &str) -> Option<u64> {
    let first = s.split('/').next()?;
    parse_bytes(first.trim())
}

/// "49.7MiB / 15.6GiB" → 15.6GiB
fn split_second_bytes(s: &str) -> Option<u64> {
    let mut parts = s.split('/');
    parts.next()?;
    parse_bytes(parts.next()?.trim())
}

/// Parse a string like `49.7MiB`, `1.2GB`, `512B` into bytes.
fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (num_part, unit) = s.split_at(split);
    let num: f64 = num_part.parse().ok()?;
    let mult = match unit {
        "B" => 1.0,
        "kB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "TB" => 1e12,
        "KiB" => 1024.0,
        "MiB" => 1024.0_f64.powi(2),
        "GiB" => 1024.0_f64.powi(3),
        "TiB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((num * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_percent_strips_suffix() {
        assert_eq!(parse_percent("0.04%"), Some(0.04));
        assert_eq!(parse_percent("100%"), Some(100.0));
        assert_eq!(parse_percent(" 12.5% "), Some(12.5));
        assert_eq!(parse_percent("nope"), None);
    }

    #[test]
    fn parse_bytes_units() {
        assert_eq!(parse_bytes("512B"), Some(512));
        assert_eq!(parse_bytes("1kB"), Some(1_000));
        assert_eq!(parse_bytes("1MiB"), Some(1024 * 1024));
        assert_eq!(
            parse_bytes("1.5GiB"),
            Some((1.5 * 1024.0 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(parse_bytes("garbage"), None);
        assert_eq!(parse_bytes("1.2ZB"), None);
    }

    #[test]
    fn split_first_bytes_takes_lhs_of_slash() {
        assert_eq!(
            split_first_bytes("49.7MiB / 15.6GiB"),
            Some((49.7 * 1024.0 * 1024.0) as u64)
        );
    }

    #[test]
    fn split_second_bytes_takes_rhs_of_slash() {
        assert_eq!(split_second_bytes("4.8kB / 0B"), Some(0));
    }

    #[test]
    fn parse_line_full_row() {
        let raw = r#"{"BlockIO":"148kB / 0B","CPUPerc":"0.04%","Container":"a1b2c3","ID":"a1b2c3","MemPerc":"0.31%","MemUsage":"49.7MiB / 15.6GiB","Name":"my-svc","NetIO":"4.8kB / 0B","PIDs":"5"}"#;
        let c = parse_line(raw).unwrap();
        assert_eq!(c.name, "my-svc");
        assert!((c.cpu_percent - 0.04).abs() < 1e-9);
        assert_eq!(c.mem_bytes, (49.7 * 1024.0 * 1024.0) as u64);
        assert!((c.mem_percent - 0.31).abs() < 1e-9);
        assert_eq!(c.net_rx_bytes, 4_800);
        assert_eq!(c.net_tx_bytes, 0);
    }

    #[test]
    fn parse_line_rejects_garbage() {
        assert!(parse_line("not json").is_none());
    }
}

//! Container stats via the Docker API (`bollard`).
//!
//! Phase 4.5 carryover: replaces the Phase 4 `docker stats` subprocess
//! with a typed async client. Same shape on the consumer side — the
//! sampler reads cached `Vec<ContainerSnapshot>` from a poller thread,
//! so the rest of the app is unchanged. The poller thread now drives a
//! current-thread tokio runtime so we can `await` bollard calls without
//! introducing a global runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::models::ContainerStatsResponse;
use bollard::query_parameters::{ListContainersOptionsBuilder, StatsOptionsBuilder};
use futures_util::StreamExt;

use super::{CollectCtx, Collector};
use crate::state::ContainerSnapshot;

/// How often the poller hits the Docker daemon. Fast enough that container
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
            .spawn(move || poller_thread(s, sh))
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

/// The poller thread builds a current-thread tokio runtime, then loops
/// blocking on `poll_once` until shutdown.
fn poller_thread(state: Arc<Mutex<PollerState>>, shutdown: Arc<AtomicBool>) {
    // Be explicit about the features we need so we're not load-bearing on
    // bollard/hyper transitively unifying tokio features:
    //   net  — bollard talks to /var/run/docker.sock
    //   io   — implicit via net; hyper drives request bodies
    //   time — internal timeouts in hyper / bollard
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "container poller: tokio runtime build failed");
            return;
        }
    };

    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = %e, "docker connect failed; container panel disabled");
            return;
        }
    };

    while !shutdown.load(Ordering::SeqCst) {
        let started = Instant::now();
        match runtime.block_on(poll_once(&docker)) {
            Ok(containers) => {
                let mut s = state.lock().expect("container state poisoned");
                s.containers = containers;
                s.available = true;
                s.last_update = Some(Instant::now());
            }
            Err(e) => {
                tracing::debug!(error = %e, "docker poll failed");
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

/// One poll cycle: list running containers and fetch a one-shot stats
/// snapshot for each. `stream: false` makes the stats stream emit a single
/// item then complete.
async fn poll_once(docker: &Docker) -> Result<Vec<ContainerSnapshot>> {
    let opts = ListContainersOptionsBuilder::default().all(false).build();
    let containers = docker
        .list_containers(Some(opts))
        .await
        .context("list_containers")?;

    let mut out = Vec::with_capacity(containers.len());
    for c in containers {
        let Some(full_id) = c.id.clone() else {
            continue;
        };
        let name = c
            .names
            .as_ref()
            .and_then(|n| n.first().cloned())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| full_id.chars().take(12).collect());

        let stats_opts = StatsOptionsBuilder::default().stream(false).build();
        let mut stream = docker.stats(&full_id, Some(stats_opts));
        let stats = match stream.next().await {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                tracing::debug!(id = %full_id, error = %e, "stats fetch failed");
                continue;
            }
            None => continue,
        };

        out.push(snapshot_from_stats(&full_id, &name, &stats));
    }
    Ok(out)
}

/// Compute the same numbers `docker stats` shows, from one bollard sample.
fn snapshot_from_stats(
    full_id: &str,
    name: &str,
    stats: &ContainerStatsResponse,
) -> ContainerSnapshot {
    let cpu_percent = compute_cpu_percent(stats);
    let mem_bytes = stats
        .memory_stats
        .as_ref()
        .and_then(|m| m.usage)
        .unwrap_or(0);
    let mem_limit = stats
        .memory_stats
        .as_ref()
        .and_then(|m| m.limit)
        .unwrap_or(0);
    let mem_percent = if mem_limit > 0 {
        (mem_bytes as f64 / mem_limit as f64) * 100.0
    } else {
        0.0
    };
    let (rx, tx) = stats
        .networks
        .as_ref()
        .map(|m| {
            m.values().fold((0u64, 0u64), |(rx, tx), n| {
                (rx + n.rx_bytes.unwrap_or(0), tx + n.tx_bytes.unwrap_or(0))
            })
        })
        .unwrap_or((0, 0));

    ContainerSnapshot {
        id: full_id.chars().take(12).collect(),
        name: name.to_string(),
        cpu_percent,
        mem_bytes,
        mem_percent,
        net_rx_bytes: rx,
        net_tx_bytes: tx,
    }
}

/// Standard `docker stats` CPU% formula:
/// `(cpu_delta / system_delta) * online_cpus * 100`. Returns 0.0 if either
/// delta is missing or zero (very first sample on a container).
fn compute_cpu_percent(stats: &ContainerStatsResponse) -> f64 {
    let cpu = match &stats.cpu_stats {
        Some(c) => c,
        None => return 0.0,
    };
    let pre = match &stats.precpu_stats {
        Some(p) => p,
        None => return 0.0,
    };
    let cpu_total = cpu
        .cpu_usage
        .as_ref()
        .and_then(|u| u.total_usage)
        .unwrap_or(0);
    let pre_total = pre
        .cpu_usage
        .as_ref()
        .and_then(|u| u.total_usage)
        .unwrap_or(0);
    let sys = cpu.system_cpu_usage.unwrap_or(0);
    let pre_sys = pre.system_cpu_usage.unwrap_or(0);

    let cpu_delta = cpu_total.saturating_sub(pre_total) as f64;
    let sys_delta = sys.saturating_sub(pre_sys) as f64;
    let online_cpus_raw = cpu
        .online_cpus
        .map(|n| n as usize)
        .or_else(|| {
            cpu.cpu_usage
                .as_ref()
                .and_then(|u| u.percpu_usage.as_ref().map(|v| v.len()))
        })
        .unwrap_or(1);
    cpu_percent_from_deltas(cpu_delta, sys_delta, online_cpus_raw as f64)
}

/// Pure-math separation so the formula is unit-testable without building
/// the full bollard `ContainerStatsResponse` shape.
fn cpu_percent_from_deltas(cpu_delta: f64, sys_delta: f64, online_cpus: f64) -> f64 {
    if sys_delta <= 0.0 || cpu_delta <= 0.0 {
        return 0.0;
    }
    (cpu_delta / sys_delta) * online_cpus * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_percent_zero_when_either_delta_zero() {
        assert_eq!(cpu_percent_from_deltas(0.0, 1000.0, 4.0), 0.0);
        assert_eq!(cpu_percent_from_deltas(100.0, 0.0, 4.0), 0.0);
    }

    #[test]
    fn cpu_percent_zero_when_negative_deltas() {
        // saturating_sub upstream forces these to 0 in real data, but the
        // pure fn should still bottom out cleanly if called directly.
        assert_eq!(cpu_percent_from_deltas(-1.0, 1000.0, 4.0), 0.0);
        assert_eq!(cpu_percent_from_deltas(100.0, -1.0, 4.0), 0.0);
    }

    #[test]
    fn cpu_percent_classic_formula() {
        // 10% of one core × 4 cores = 40%
        assert_eq!(cpu_percent_from_deltas(100.0, 1000.0, 4.0), 40.0);
        // single-core fully busy
        assert_eq!(cpu_percent_from_deltas(1000.0, 1000.0, 1.0), 100.0);
    }
}

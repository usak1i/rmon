//! Prometheus `/metrics` exporter.
//!
//! Opt-in via `--prometheus <addr:port>`. When enabled, App spawns a
//! dedicated `prometheus-exporter` thread driving a current-thread
//! tokio runtime + axum. The handler reads `Snapshot::numeric` from the
//! same `SharedState` the UI reads and renders each entry as a gauge
//! line. No global runtime; tokio is already in the dep tree thanks to
//! the bollard upgrade.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::sync::{Notify, oneshot};

use crate::state::SharedState;

pub struct Exporter {
    shutdown: Arc<Notify>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Exporter {
    /// Spawn the exporter thread + runtime listening on `addr`. Blocks
    /// briefly on a oneshot from the worker so a failed `bind` (e.g.
    /// port in use) surfaces here rather than a silent
    /// connection-refused at scrape time.
    pub fn start(state: SharedState, addr: SocketAddr) -> Result<Self> {
        let shutdown = Arc::new(Notify::new());
        let shutdown_for_thread = shutdown.clone();
        let (bind_tx, bind_rx) = oneshot::channel::<Result<()>>();
        let handle = thread::Builder::new()
            .name("prometheus-exporter".into())
            .spawn(move || run(state, addr, shutdown_for_thread, bind_tx))
            .context("spawning exporter thread")?;
        match bind_rx.blocking_recv() {
            Ok(Ok(())) => Ok(Self {
                shutdown,
                handle: Some(handle),
            }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err(anyhow::anyhow!(
                "exporter thread exited before reporting bind result"
            )),
        }
    }
}

impl Drop for Exporter {
    fn drop(&mut self) {
        self.shutdown.notify_one();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(
    state: SharedState,
    addr: SocketAddr,
    shutdown: Arc<Notify>,
    bind_tx: oneshot::Sender<Result<()>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            let _ = bind_tx.send(Err(anyhow::anyhow!("tokio runtime build: {e}")));
            return;
        }
    };
    runtime.block_on(serve(state, addr, shutdown, bind_tx));
}

async fn serve(
    state: SharedState,
    addr: SocketAddr,
    shutdown: Arc<Notify>,
    bind_tx: oneshot::Sender<Result<()>>,
) {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            let _ = bind_tx.send(Err(anyhow::anyhow!("bind {addr}: {e}")));
            return;
        }
    };
    let _ = bind_tx.send(Ok(()));
    tracing::info!(%addr, "prometheus exporter listening on /metrics");

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.notified().await;
        })
        .await;
    if let Err(e) = result {
        tracing::warn!(error = %e, "exporter server error");
    }
}

/// Build a fully-formed Response so axum's String-body default doesn't
/// also append its own `text/plain; charset=utf-8` and leave the
/// scraper with two `content-type` headers.
async fn metrics_handler(State(state): State<SharedState>) -> impl IntoResponse {
    let body = render_metrics(&state);
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

/// Render the latest snapshot as Prometheus text-format gauge lines.
/// Each `MetricKey` is sanitised + prefixed with `rmon_` to avoid
/// collisions with other exporters scraped onto the same Prometheus.
fn render_metrics(state: &SharedState) -> String {
    let mut out = String::new();
    state.with_view(|view| {
        let Some(snap) = view.current else { return };
        let mut entries: Vec<(&str, f64)> = snap
            .numeric
            .iter()
            .map(|(k, v)| (k.0.as_str(), *v))
            .collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(b.0));

        let mut last_name = String::new();
        for (key, value) in entries {
            let Some(name) = sanitize(key) else {
                continue;
            };
            let full = format!("rmon_{name}");
            if full != last_name {
                let _ = writeln!(out, "# TYPE {full} gauge");
                last_name = full.clone();
            }
            let _ = writeln!(out, "{full} {value}");
        }
    });
    out
}

/// Make a `MetricKey` like `cpu.core.0` valid as a Prometheus metric
/// name: dots → underscores, dashes / slashes → underscores, drop
/// names that don't fit `[a-zA-Z_:][a-zA-Z0-9_:]*`. Returns None if
/// the input is empty or starts with a digit.
fn sanitize(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        let mapped = match c {
            '.' | '-' | '/' => '_',
            c if c.is_ascii_alphanumeric() || c == '_' || c == ':' => c,
            _ => return None,
        };
        if i == 0 && !(mapped.is_ascii_alphabetic() || mapped == '_' || mapped == ':') {
            return None;
        }
        out.push(mapped);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_preserves_alnum() {
        assert_eq!(sanitize("cpu_total").as_deref(), Some("cpu_total"));
        assert_eq!(sanitize("colon:ok").as_deref(), Some("colon:ok"));
    }

    #[test]
    fn sanitize_translates_dots() {
        assert_eq!(sanitize("cpu.core.0").as_deref(), Some("cpu_core_0"));
        assert_eq!(
            sanitize("net.eth0.rx_bps").as_deref(),
            Some("net_eth0_rx_bps")
        );
    }

    #[test]
    fn sanitize_translates_dashes_and_slashes() {
        assert_eq!(
            sanitize("disk.mount-point/var").as_deref(),
            Some("disk_mount_point_var"),
        );
    }

    #[test]
    fn sanitize_rejects_empty_and_leading_digit() {
        assert_eq!(sanitize(""), None);
        assert_eq!(sanitize("0bad"), None);
    }

    #[test]
    fn sanitize_rejects_non_ascii() {
        assert_eq!(sanitize("cpu.°C"), None);
    }
}

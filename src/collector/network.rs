use anyhow::Result;

use super::{CollectCtx, Collector};
use crate::state::NetworkSnapshot;

/// Per-interface network throughput from sysinfo. Loopback interfaces are
/// hidden by default. Emits:
/// - `net.<iface>.rx_bps` / `net.<iface>.tx_bps`
/// - `net.<iface>.total_rx_bytes` / `net.<iface>.total_tx_bytes`
/// - aggregate `net.total.rx_bps` / `net.total.tx_bps` (sum across non-loopback)
pub struct NetworkCollector;

impl NetworkCollector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for NetworkCollector {
    fn name(&self) -> &'static str {
        "network"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let elapsed_secs = ctx.system.last_refresh_elapsed.as_secs_f64().max(0.001);

        ctx.snapshot.networks.clear();
        let mut total_rx = 0.0_f64;
        let mut total_tx = 0.0_f64;

        for (name, data) in ctx.system.networks.list() {
            if is_loopback(name) {
                continue;
            }
            let rx_bps = data.received() as f64 / elapsed_secs;
            let tx_bps = data.transmitted() as f64 / elapsed_secs;
            total_rx += rx_bps;
            total_tx += tx_bps;

            ctx.snapshot.set(format!("net.{name}.rx_bps"), rx_bps);
            ctx.snapshot.set(format!("net.{name}.tx_bps"), tx_bps);
            ctx.snapshot.set(
                format!("net.{name}.total_rx_bytes"),
                data.total_received() as f64,
            );
            ctx.snapshot.set(
                format!("net.{name}.total_tx_bytes"),
                data.total_transmitted() as f64,
            );

            ctx.snapshot.networks.push(NetworkSnapshot {
                interface: name.to_string(),
                rx_bps,
                tx_bps,
                total_rx_bytes: data.total_received(),
                total_tx_bytes: data.total_transmitted(),
            });
        }

        // Sort by combined throughput descending so busiest sits at the top.
        ctx.snapshot.networks.sort_by(|a, b| {
            (b.rx_bps + b.tx_bps)
                .partial_cmp(&(a.rx_bps + a.tx_bps))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ctx.snapshot.set("net.total.rx_bps", total_rx);
        ctx.snapshot.set("net.total.tx_bps", total_tx);
        Ok(())
    }
}

fn is_loopback(name: &str) -> bool {
    matches!(name, "lo" | "lo0") || name.starts_with("lo:")
}

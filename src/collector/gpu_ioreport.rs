//! Sudo-less Apple Silicon GPU collector.
//!
//! Drives an `IoReportGpuSampler` (private IOReport framework) and emits
//! `gpu.usage` derived from per-P-state residency counters. No
//! frequency or power keys: a per-chip frequency table isn't exposed
//! uniformly, and power is already covered by the Sensors panel via
//! the IOReport Energy Model sampler.
//!
//! The corresponding sudo-based path lives in `gpu.rs` (powermetrics).

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use super::platform::IoReportGpuSampler;
use super::{CollectCtx, Collector};

/// Match `GpuCollector`'s freshness window: if no successful sample in
/// this long, stop emitting numerics so the panel falls back to its
/// empty state instead of pinning a stale reading.
const STALE_AFTER: Duration = Duration::from_secs(5);

pub struct GpuIoReportCollector {
    sampler: IoReportGpuSampler,
    last_usage: Option<f64>,
    last_update: Option<Instant>,
}

impl GpuIoReportCollector {
    pub fn try_new() -> Result<Self> {
        let sampler = IoReportGpuSampler::new()
            .ok_or_else(|| anyhow!("could not subscribe to IOReport \"GPU Stats\" group"))?;
        Ok(Self {
            sampler,
            last_usage: None,
            last_update: None,
        })
    }
}

impl Collector for GpuIoReportCollector {
    fn name(&self) -> &'static str {
        "gpu_ioreport"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        if let Some(usage) = self.sampler.sample() {
            self.last_usage = Some(usage);
            self.last_update = Some(Instant::now());
        }
        let fresh = self
            .last_update
            .is_some_and(|t| Instant::now().duration_since(t) < STALE_AFTER);
        if !fresh {
            return Ok(());
        }
        if let Some(u) = self.last_usage {
            ctx.snapshot.set("gpu.usage", u);
        }
        Ok(())
    }
}

use anyhow::Result;

use super::platform;
use super::{CollectCtx, Collector};

/// Reads platform sensors (temperature, fan, battery) and writes both the
/// structured `Snapshot::sensors` list and per-reading numeric series under
/// `sensor.<category>.<name>` so they can drive sparklines.
///
/// On macOS, also owns an optional `IoReportSampler` for power readings
/// derived from the private IOReport framework. Stays `None` if the
/// subscription couldn't be created (e.g. macOS version mismatch).
pub struct SensorsCollector {
    #[cfg(target_os = "macos")]
    ioreport: Option<platform::IoReportSampler>,
}

impl SensorsCollector {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            ioreport: platform::IoReportSampler::new(),
        }
    }
}

impl Default for SensorsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for SensorsCollector {
    fn name(&self) -> &'static str {
        "sensors"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        // `mut` is only used on macOS (the IOReport sampler appends).
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut readings = platform::read_sensors();

        #[cfg(target_os = "macos")]
        if let Some(sampler) = self.ioreport.as_mut() {
            readings.extend(sampler.sample());
        }

        for r in &readings {
            ctx.snapshot
                .set(format!("sensor.{}.{}", r.category, r.name), r.value);
        }
        ctx.snapshot.sensors = readings;

        let batteries = platform::read_batteries();
        for b in &batteries {
            ctx.snapshot
                .set(format!("battery.{}.percent", b.name), b.percent);
            if let Some(m) = b.time_remaining_minutes {
                ctx.snapshot
                    .set(format!("battery.{}.minutes", b.name), m as f64);
            }
        }
        ctx.snapshot.batteries = batteries;
        Ok(())
    }
}

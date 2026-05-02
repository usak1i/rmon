use anyhow::Result;

use super::platform;
use super::{CollectCtx, Collector};

/// Reads platform sensors (temperature, fan, battery) and writes both the
/// structured `Snapshot::sensors` list and per-reading numeric series under
/// `sensor.<category>.<name>` so they can drive sparklines.
pub struct SensorsCollector;

impl SensorsCollector {
    pub fn new() -> Self {
        Self
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
        let readings = platform::read_sensors();
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

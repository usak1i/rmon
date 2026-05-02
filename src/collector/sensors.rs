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
        Ok(())
    }
}

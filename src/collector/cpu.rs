use anyhow::Result;

use super::{CollectCtx, Collector};

/// Emits:
/// - `cpu.total`             — system-wide CPU% (mean of cores)
/// - `cpu.core.<N>`          — per-core CPU%
/// - `cpu.freq.<N>`          — per-core frequency in MHz
/// - `cpu.load.{1,5,15}`     — load average (Unix)
pub struct CpuCollector;

impl CpuCollector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CpuCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for CpuCollector {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let cpus = ctx.system.system.cpus();
        if cpus.is_empty() {
            return Ok(());
        }

        let mut total = 0.0_f32;
        for (idx, cpu) in cpus.iter().enumerate() {
            let usage = cpu.cpu_usage();
            total += usage;
            ctx.snapshot.set(format!("cpu.core.{idx}"), usage as f64);
            ctx.snapshot
                .set(format!("cpu.freq.{idx}"), cpu.frequency() as f64);
        }
        let mean = total / cpus.len() as f32;
        ctx.snapshot.set("cpu.total", mean as f64);

        let load = sysinfo::System::load_average();
        ctx.snapshot.set("cpu.load.1", load.one);
        ctx.snapshot.set("cpu.load.5", load.five);
        ctx.snapshot.set("cpu.load.15", load.fifteen);

        Ok(())
    }
}

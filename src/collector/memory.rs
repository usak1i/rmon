use anyhow::Result;

use super::{CollectCtx, Collector};

/// Emits:
/// - `mem.total_bytes`
/// - `mem.used_bytes`
/// - `mem.available_bytes`
/// - `mem.swap_total_bytes`
/// - `mem.swap_used_bytes`
pub struct MemoryCollector;

impl MemoryCollector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for MemoryCollector {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let s = &ctx.system.system;
        ctx.snapshot.set("mem.total_bytes", s.total_memory() as f64);
        ctx.snapshot.set("mem.used_bytes", s.used_memory() as f64);
        ctx.snapshot
            .set("mem.available_bytes", s.available_memory() as f64);
        ctx.snapshot
            .set("mem.swap_total_bytes", s.total_swap() as f64);
        ctx.snapshot
            .set("mem.swap_used_bytes", s.used_swap() as f64);
        Ok(())
    }
}

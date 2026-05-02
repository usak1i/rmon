use anyhow::Result;

use super::{CollectCtx, Collector};
use crate::state::DiskSnapshot;

/// Populates `Snapshot::disks` with one entry per mounted volume and emits
/// aggregated `disk.total_bytes` / `disk.used_bytes` numerics for sparklines.
pub struct DiskCollector;

impl DiskCollector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DiskCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for DiskCollector {
    fn name(&self) -> &'static str {
        "disk"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let mut total = 0_u64;
        let mut available = 0_u64;
        ctx.snapshot.disks.clear();
        for d in ctx.system.disks.list() {
            let entry = DiskSnapshot {
                mount_point: d.mount_point().to_string_lossy().into_owned(),
                fs_type: d.file_system().to_string_lossy().into_owned(),
                total_bytes: d.total_space(),
                available_bytes: d.available_space(),
            };
            total = total.saturating_add(entry.total_bytes);
            available = available.saturating_add(entry.available_bytes);
            ctx.snapshot.disks.push(entry);
        }
        let used = total.saturating_sub(available);
        ctx.snapshot.set("disk.total_bytes", total as f64);
        ctx.snapshot.set("disk.used_bytes", used as f64);
        Ok(())
    }
}

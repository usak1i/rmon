mod cpu;
mod disk;
#[cfg(target_os = "macos")]
mod gpu;
mod memory;
mod network;
mod platform;
mod process;
mod sensors;
pub mod system;

pub use cpu::CpuCollector;
pub use disk::DiskCollector;
#[cfg(target_os = "macos")]
pub use gpu::GpuCollector;
pub use memory::MemoryCollector;
pub use network::NetworkCollector;
pub use process::ProcessCollector;
pub use sensors::SensorsCollector;
pub use system::SystemSource;

use anyhow::Result;

use crate::state::Snapshot;

/// Per-tick context handed to every collector. The sampler refreshes
/// `system` once before walking the registry; collectors only read.
pub struct CollectCtx<'a> {
    pub snapshot: &'a mut Snapshot,
    pub system: &'a SystemSource,
}

/// A pluggable source of metrics. Implementations write into
/// `ctx.snapshot` — both numeric series via `Snapshot::set` and structured
/// lists (processes, disks).
pub trait Collector: Send {
    fn name(&self) -> &'static str;
    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()>;
}

/// Owns the configured set of collectors and drives them each tick.
pub struct Registry {
    collectors: Vec<Box<dyn Collector>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            collectors: Vec::new(),
        }
    }

    pub fn register(&mut self, collector: Box<dyn Collector>) {
        tracing::debug!(name = collector.name(), "registering collector");
        self.collectors.push(collector);
    }

    pub fn sample_all(&mut self, system: &mut SystemSource, snapshot: &mut Snapshot) {
        system.refresh();
        let mut ctx = CollectCtx { snapshot, system };
        for c in self.collectors.iter_mut() {
            if let Err(e) = c.sample(&mut ctx) {
                tracing::warn!(collector = c.name(), error = %e, "collector sample failed");
            }
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

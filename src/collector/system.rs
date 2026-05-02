use std::time::{Duration, Instant};

use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, System, Users,
};

/// Owned wrapper around the sysinfo handles the sampler refreshes once per
/// tick. Held by the sampler thread (single-threaded). Collectors borrow it
/// immutably via `CollectCtx`.
pub struct SystemSource {
    pub system: System,
    pub disks: Disks,
    pub networks: Networks,
    pub users: Users,
    /// Time elapsed between the previous refresh and the most recent one.
    /// Defaults to 1s on the very first call. Collectors divide network
    /// byte deltas by this to produce per-second rates.
    pub last_refresh_elapsed: Duration,
    last_refresh_at: Option<Instant>,
}

impl SystemSource {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything()),
        );
        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();
        let users = Users::new_with_refreshed_list();
        Self {
            system,
            disks,
            networks,
            users,
            last_refresh_elapsed: Duration::from_secs(1),
            last_refresh_at: None,
        }
    }

    /// Refresh everything Phase 1 + 2 needs. Called once per sampling tick
    /// before any collector reads from `system` / `disks` / `networks`.
    pub fn refresh(&mut self) {
        let now = Instant::now();
        self.last_refresh_elapsed = self
            .last_refresh_at
            .map(|t| now - t)
            .unwrap_or(Duration::from_secs(1));

        self.system
            .refresh_cpu_specifics(CpuRefreshKind::everything());
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything(),
        );
        self.disks.refresh(true);
        self.networks.refresh(true);

        self.last_refresh_at = Some(now);
    }
}

impl Default for SystemSource {
    fn default() -> Self {
        Self::new()
    }
}

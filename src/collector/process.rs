use std::collections::HashMap;

use anyhow::Result;
use sysinfo::ProcessStatus;

use super::cgroup;
use super::{CollectCtx, Collector};
use crate::state::ProcessSnapshot;

/// Populates `Snapshot::processes`. Also emits `process.count` and the sum
/// `process.cpu_total` for sparklines.
///
/// Holds a per-PID cache of cgroup container IDs since `/proc/<pid>/cgroup`
/// doesn't change for the life of a process — re-reading every tick for
/// hundreds of PIDs would be wasteful.
pub struct ProcessCollector {
    cgroup_cache: HashMap<u32, Option<String>>,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self {
            cgroup_cache: HashMap::new(),
        }
    }
}

impl Default for ProcessCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ProcessCollector {
    fn name(&self) -> &'static str {
        "process"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let processes = ctx.system.system.processes();
        ctx.snapshot.processes.clear();
        ctx.snapshot.processes.reserve(processes.len());

        let mut cpu_total = 0.0_f32;
        let mut next_cache: HashMap<u32, Option<String>> = HashMap::with_capacity(processes.len());

        for proc in processes.values() {
            let pid = proc.pid().as_u32();
            let user = proc
                .user_id()
                .and_then(|uid| ctx.system.users.get_user_by_id(uid))
                .map(|u| u.name().to_string())
                .unwrap_or_else(|| "?".to_string());

            let command = if !proc.cmd().is_empty() {
                proc.cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                proc.name().to_string_lossy().into_owned()
            };

            let cpu = proc.cpu_usage();
            cpu_total += cpu;

            let container_id = self
                .cgroup_cache
                .get(&pid)
                .cloned()
                .unwrap_or_else(|| cgroup::read_pid_container(pid));
            next_cache.insert(pid, container_id.clone());

            ctx.snapshot.processes.push(ProcessSnapshot {
                pid,
                user,
                cpu_percent: cpu,
                memory_bytes: proc.memory(),
                command,
                status: status_char(proc.status()),
                run_time_secs: proc.run_time(),
                container_id,
            });
        }

        // GC: drop cache entries for PIDs that aren't in the current
        // snapshot. next_cache is the new full set.
        self.cgroup_cache = next_cache;

        ctx.snapshot
            .set("process.count", ctx.snapshot.processes.len() as f64);
        ctx.snapshot.set("process.cpu_total", cpu_total as f64);
        Ok(())
    }
}

fn status_char(s: ProcessStatus) -> char {
    match s {
        ProcessStatus::Run => 'R',
        ProcessStatus::Sleep => 'S',
        ProcessStatus::Idle => 'I',
        ProcessStatus::Zombie => 'Z',
        ProcessStatus::Stop | ProcessStatus::Suspended => 'T',
        ProcessStatus::Dead => 'X',
        ProcessStatus::Tracing => 't',
        ProcessStatus::Waking => 'W',
        ProcessStatus::Wakekill => 'K',
        ProcessStatus::Parked => 'P',
        ProcessStatus::LockBlocked => 'L',
        ProcessStatus::UninterruptibleDiskSleep => 'D',
        ProcessStatus::Unknown(_) => '?',
    }
}

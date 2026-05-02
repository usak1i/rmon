use anyhow::Result;
use sysinfo::ProcessStatus;

use super::{CollectCtx, Collector};
use crate::state::ProcessSnapshot;

/// Populates `Snapshot::processes`. Also emits `process.count` and the sum
/// `process.cpu_total` for sparklines.
pub struct ProcessCollector;

impl ProcessCollector {
    pub fn new() -> Self {
        Self
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
        for proc in processes.values() {
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

            ctx.snapshot.processes.push(ProcessSnapshot {
                pid: proc.pid().as_u32(),
                user,
                cpu_percent: cpu,
                memory_bytes: proc.memory(),
                command,
                status: status_char(proc.status()),
                run_time_secs: proc.run_time(),
            });
        }

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

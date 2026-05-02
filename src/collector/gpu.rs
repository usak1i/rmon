//! macOS GPU monitoring by parsing `sudo powermetrics --samplers gpu_power`.
//!
//! Caller must have a cached sudo timestamp (e.g. via `sudo -v`) before
//! constructing this collector — once the TUI is in raw mode, sudo's
//! password prompt is unreachable.
//!
//! IOReport-based path that avoids sudo entirely is tracked under Phase 3
//! "Path B" in TODO.md.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{CollectCtx, Collector};

/// If the reader thread hasn't pushed an update in this long, treat the
/// stats as stale (powermetrics likely died) and stop emitting numerics so
/// the GPU panel falls back to its empty state.
const STALE_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Clone)]
struct GpuStats {
    usage: Option<f64>,
    freq_mhz: Option<f64>,
    power_mw: Option<f64>,
    last_update: Option<Instant>,
}

pub struct GpuCollector {
    state: Arc<Mutex<GpuStats>>,
    child: Child,
}

impl GpuCollector {
    /// Probe sudo, spawn powermetrics, and start the parser thread.
    /// Returns `Err` (and does not spawn anything) if sudo isn't authenticated.
    pub fn try_new() -> Result<Self> {
        let probe = Command::new("sudo")
            .args(["-n", "true"])
            .status()
            .context("running `sudo -n true` probe")?;
        if !probe.success() {
            anyhow::bail!("sudo timestamp not cached; run `sudo -v` first");
        }

        let mut cmd = Command::new("sudo");
        cmd.args(["powermetrics", "--samplers", "gpu_power", "-i", "1000"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null());
        // Put sudo + powermetrics in their own process group so we can kill
        // the whole subtree on Drop. Without this, killing `sudo` orphans
        // `powermetrics` to launchd and it keeps running.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = cmd.spawn().context("spawning powermetrics")?;

        let stdout = child
            .stdout
            .take()
            .context("powermetrics stdout pipe missing")?;
        let state = Arc::new(Mutex::new(GpuStats::default()));
        let state_thread = state.clone();
        thread::Builder::new()
            .name("gpu-reader".into())
            .spawn(move || reader_loop(stdout, state_thread))
            .context("spawning gpu reader thread")?;

        Ok(Self { state, child })
    }
}

impl Collector for GpuCollector {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        let stats = self.state.lock().expect("gpu state poisoned").clone();
        let fresh = stats
            .last_update
            .is_some_and(|t| Instant::now().duration_since(t) < STALE_AFTER);
        if !fresh {
            return Ok(());
        }
        if let Some(u) = stats.usage {
            ctx.snapshot.set("gpu.usage", u);
        }
        if let Some(f) = stats.freq_mhz {
            ctx.snapshot.set("gpu.freq_mhz", f);
        }
        if let Some(p) = stats.power_mw {
            ctx.snapshot.set("gpu.power_mw", p);
        }
        Ok(())
    }
}

impl Drop for GpuCollector {
    fn drop(&mut self) {
        // Kill the entire process group rooted at sudo's PID so powermetrics
        // (sudo's child) doesn't get reparented to launchd. Falls back to a
        // single-process kill if the group send fails for any reason.
        let pid = self.child.id() as libc::pid_t;
        unsafe {
            if libc::kill(-pid, libc::SIGKILL) != 0 {
                let _ = self.child.kill();
            }
        }
        let _ = self.child.wait();
    }
}

fn reader_loop(stdout: ChildStdout, state: Arc<Mutex<GpuStats>>) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let Ok(line) = line else { return };
        let mut delta = ParseDelta::default();
        if parse_line(&line, &mut delta) {
            let mut s = state.lock().expect("gpu state poisoned");
            if let Some(v) = delta.usage {
                s.usage = Some(v);
            }
            if let Some(v) = delta.freq_mhz {
                s.freq_mhz = Some(v);
            }
            if let Some(v) = delta.power_mw {
                s.power_mw = Some(v);
            }
            s.last_update = Some(Instant::now());
        }
    }
}

#[derive(Default)]
struct ParseDelta {
    usage: Option<f64>,
    freq_mhz: Option<f64>,
    power_mw: Option<f64>,
}

fn parse_line(line: &str, out: &mut ParseDelta) -> bool {
    let line = line.trim();

    if let Some(rest) = strip_residency(line)
        && let Some(v) = first_number(rest)
    {
        out.usage = Some(v);
        return true;
    }
    if let Some(rest) = strip_frequency(line)
        && let Some(v) = first_number(rest)
    {
        out.freq_mhz = Some(v);
        return true;
    }
    if let Some(rest) = line.strip_prefix("GPU Power:")
        && let Some(v) = first_number(rest)
    {
        out.power_mw = Some(v);
        return true;
    }
    false
}

fn strip_residency(line: &str) -> Option<&str> {
    line.strip_prefix("GPU HW active residency:")
        .or_else(|| line.strip_prefix("GPU active residency:"))
}

fn strip_frequency(line: &str) -> Option<&str> {
    line.strip_prefix("GPU HW active frequency:")
        .or_else(|| line.strip_prefix("GPU active frequency:"))
}

/// First numeric token in `s`, allowing trailing punctuation (e.g. `12.34%`
/// or `444 MHz`). Returns `None` when there is no parseable number.
fn first_number(s: &str) -> Option<f64> {
    let tok = s.split_whitespace().next()?;
    let cleaned: String = tok
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    cleaned.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hw_active_residency() {
        let mut d = ParseDelta::default();
        assert!(parse_line(
            "GPU HW active residency: 12.34% (444 MHz: 12.34%)",
            &mut d,
        ));
        assert_eq!(d.usage, Some(12.34));
    }

    #[test]
    fn parses_legacy_active_residency() {
        let mut d = ParseDelta::default();
        assert!(parse_line("GPU active residency: 0.00%", &mut d));
        assert_eq!(d.usage, Some(0.0));
    }

    #[test]
    fn parses_active_frequency() {
        let mut d = ParseDelta::default();
        assert!(parse_line("GPU active frequency: 444 MHz", &mut d));
        assert_eq!(d.freq_mhz, Some(444.0));
    }

    #[test]
    fn parses_gpu_power() {
        let mut d = ParseDelta::default();
        assert!(parse_line("GPU Power: 87 mW", &mut d));
        assert_eq!(d.power_mw, Some(87.0));
    }

    #[test]
    fn ignores_section_headers() {
        let mut d = ParseDelta::default();
        assert!(!parse_line("**** GPU usage ****", &mut d));
        assert!(d.usage.is_none() && d.freq_mhz.is_none() && d.power_mw.is_none());
    }

    #[test]
    fn ignores_idle_residency_line() {
        let mut d = ParseDelta::default();
        assert!(!parse_line("GPU idle residency: 87.66%", &mut d));
        assert!(d.usage.is_none());
    }
}

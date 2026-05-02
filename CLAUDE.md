# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project context

`resource-monitor` is a Rust TUI system monitor (htop-style) for **macOS + Linux**. It is being built in deliberate phases — see `/Users/han/.claude/plans/rust-htop-cpu-memory-disk-frolicking-meadow.md` for the full design plan and phase breakdown, plus `TODO.md` in this repo for the active roadmap.

**Done**: Phase 0 (scaffolding) · Phase 1 (CPU/Mem/Disk/Process collectors, four-panel TUI with sparklines, dark theme, focus/sort/search/kill/help keys) · Phase 2 (Network + Sensors, six-panel layout) · Phase 3 (macOS Apple Silicon GPU via `sudo powermetrics`, opt-in via `--gpu`, conditional 7-panel layout).

**Next**: Phase 2.5 carryovers (macOS thermal via IOReport, Linux connection counts) or Phase 4 (Container awareness via Docker socket + cgroups).

### Phase 3 GPU specifics
The GPU collector is **opt-in** (`--gpu`), **macOS-only**, and gated behind sudo. Flow:
1. `main.rs::ensure_gpu_prereqs` runs `sudo -v` *before* entering raw mode so the password prompt is reachable.
2. The sampler thread tries `GpuCollector::try_new` which probes `sudo -n true`, then spawns `sudo powermetrics --samplers gpu_power -i 1000` with stdout piped.
3. A dedicated `gpu-reader` thread parses `GPU HW active residency` / `GPU active frequency` / `GPU Power` lines into a `Mutex<GpuStats>`.
4. `GpuCollector::sample` snapshots that mutex into `gpu.usage` / `gpu.freq_mhz` / `gpu.power_mw` numerics.
5. `Drop for GpuCollector` SIGKILLs the powermetrics child; the reader thread exits when stdout closes.

Failure modes are *non-fatal*: sudo-not-cached, spawn-failed, parser-saw-nothing all degrade to "GPU panel shows `waiting for powermetrics…`" rather than crashing. IOReport-based no-sudo path is tracked in TODO.md.

Differentiators planned beyond htop: historical sparkline charts, modern theme, container/cgroup awareness, alert rules, and a Prometheus `/metrics` exporter. Explicit non-goals: Windows, GUI/Web UI, multi-machine view, record/replay.

## Common commands

```bash
cargo run                   # debug build, run TUI; press q or Ctrl-C to quit
cargo run --release         # smoother sampling/UI
cargo run -- --config <path> --debug   # custom TOML config + RUST_LOG=debug to stderr

cargo test                          # unit tests (currently in src/state/history.rs)
cargo test push_appends_and_caps    # run a single test by substring

# CI gate (run all three before declaring a phase done):
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Smoke-testing the TUI without a real terminal

`cargo run` requires a TTY (raw mode + alt screen). To verify cleanly from a non-interactive shell, wrap with `script` to provide a PTY:

```bash
(sleep 0.8; printf 'q') | script -q /tmp/rmon.log target/release/resource-monitor
```

Exit code 0 + an alt-screen entry/exit pair (`?1049h` / `?1049l`) in the log means terminal restoration works.

## Architecture

### Three-thread model

1. **Sampler thread** (`app::sampler_loop`) — runs every `config.sample_interval_ms` (default 1000ms). Builds a fresh `Snapshot`, walks the `Registry` calling `Collector::sample(&mut Snapshot)` on each, then `State::commit(snapshot)` which atomically replaces `current` and pushes every numeric value into its history ring buffer.
2. **UI thread** (main, `App::event_loop`) — every `config.ui_tick_ms` (default 100ms): non-blocking `crossterm::event::poll`, `terminal.draw(|f| ui::render(f, &state))`. Reads via `state.with_view(|view| ...)`.
3. **Exporter thread** (planned, Phase 6) — tokio + axum serving `/metrics`, reading the same `SharedState`.

All three share one `Arc<State>` (alias `SharedState`); state is guarded by an internal `RwLock`. Sampler is the sole writer; UI and exporter are readers.

### Data flow & key types

- `state::Snapshot` is a flat `HashMap<MetricKey, f64>` plus capture timestamp. Collectors **accumulate into the same Snapshot** rather than returning their own — keeps allocation per tick to one map.
- `MetricKey` naming convention: `<group>.<sub>` (e.g. `cpu.total`, `cpu.core.0`, `mem.used_bytes`, `disk.io.read_bps`). Use this when adding new collectors so the UI/exporter can address series uniformly.
- `state::History` is a per-key `VecDeque<f64>` ring buffer, capacity shared across all series (default 600 → 10 min @ 1Hz). `History::push_from(&Snapshot)` appends every series in lockstep.
- `state::StateView` is the read-side handle exposed inside `with_view` — gives borrowed `current`, `history`, `last_sample_at`, and a monotonic `sample_seq` counter.

### Terminal lifecycle

`app::install_panic_hook` is called before entering raw mode. It chains `disable_raw_mode` + `LeaveAlternateScreen` into the existing panic hook so a crash doesn't leave the user's shell broken. Normal exit goes through `leave_terminal`. Anything that puts the terminal in a special mode (mouse, bracketed paste, etc.) must be undone there too.

### Shutdown

`Arc<AtomicBool>` shared between UI and sampler. UI sets it on `q`/`Ctrl-C`; sampler observes via `sleep_until_or_shutdown` (50ms-chunked sleep) so quit feels instant even at long sampling intervals.

## Conventions

- **`#![allow(dead_code)]` in `src/main.rs`** is intentional Phase 0 scaffolding — the Registry/MetricKey/Snapshot/History APIs are wired but not yet called. Remove the allow once Phase 1 collectors land.
- **Edition 2024** (`Cargo.toml`). `let`-chains are available and used in `app::event_loop`.
- **Errors**: `anyhow::Result` at app boundaries; collectors should `tracing::warn!` on per-sample failures (logged by `Registry::sample_all`) rather than aborting the whole tick.
- **Logs go to stderr with ANSI off** so they don't fight the TUI on the same fd. Use `--debug` or `RUST_LOG=...` to surface them.
- **Phase discipline**: keep collectors decoupled from UI. A new metric is added by registering a `Collector` impl that writes to `Snapshot::set("group.sub", value)` — no UI changes required for the value to start flowing into history. UI changes happen only when you want to *render* the new series.

## Platform-specific work

The plan calls for `/proc` + `procfs` on Linux and Mach/sysctl/IOKit on macOS for higher-precision metrics, behind `#[cfg(target_os = "...")]`. Phase 1 starts with the cross-platform `sysinfo` crate as the baseline; per-platform fast paths come later. Apple Silicon GPU (Phase 3) will spawn `powermetrics` as a child process — needs sudo, gated behind a CLI flag.

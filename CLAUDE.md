# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project context

`resource-monitor` is a Rust TUI system monitor (htop-style) for **macOS + Linux**. It is being built in deliberate phases — see `/Users/han/.claude/plans/rust-htop-cpu-memory-disk-frolicking-meadow.md` for the full design plan and `TODO.md` in this repo for the active roadmap.

**Done**: Phase 0 (scaffolding) · Phase 1 (CPU/Mem/Disk/Process collectors, four-panel TUI with sparklines, dark theme, focus/sort/search/kill/help keys) · Phase 2 (Network + Sensors, six-panel layout) · Phase 3 (macOS Apple Silicon GPU via `sudo powermetrics`, opt-in via `--gpu`, conditional 7-panel layout) · Phase 2.5/3 carryovers (battery status + time-remaining, Linux connection counts, GPU stale-data check, `setpgid` process-group containment for powermetrics) · Phase 4 (Container panel via `docker stats` subprocess in a dedicated poller thread, eight-panel layout) · Phase 4.5 (Linux cgroup PID grouping, Process panel `g` toggle for grouped/flat view) · Phase 5 (TOML alert rules with breach-since tracking; firing tints panel borders, `a` opens an overlay, transitions log + ring the bell).

**Next** (live in `TODO.md`): bollard upgrade (Docker API client, pulls in tokio), macOS thermal/fan via IOReport, or Phase 6 (Prometheus exporter).

Differentiators planned beyond htop: historical sparkline charts, modern theme, container/cgroup awareness, alert rules, and a Prometheus `/metrics` exporter. Explicit non-goals: Windows, GUI/Web UI, multi-machine view, record/replay.

## Project skills

Four project-scoped skills live under `.claude/skills/` and load automatically in Claude Code. Prefer invoking these over re-deriving the steps each session:

- **`rust-phase-gate`** — strict local quality gate (`fmt --check` + `clippy -D warnings` + `test`). Run before declaring any chunk of work done.
- **`tui-smoke-test`** — drives the TUI through `script` PTY so you can verify clean startup/shutdown without a real terminal.
- **`add-collector`** — six-file wiring pattern for a new metric type plus the four common silent-failure modes (forgotten `Registry::register`, missing `SystemSource::refresh`, mistyped `MetricKey`, platform-cfg parser dead-code warnings).
- **`push-and-watch-ci`** — commit (HEREDOC, file-by-name staging, Co-Authored-By), push, and `gh run watch` in the background. CI runs the same gate on `ubuntu-latest` + `macos-latest` matrix.

## Common commands

```bash
cargo run                              # debug build, run TUI; q or Ctrl-C to quit
cargo run --release                    # smoother sampling/UI
cargo run --release -- --gpu           # macOS only: enable GPU panel (sudo)
cargo run -- --config <path> --debug   # custom TOML config + RUST_LOG=debug

cargo test                          # 31 tests: history, format, gpu parser, pmset, hwmon, connections, container parser, cgroup parser
cargo test parses_gpu_power         # run a single test by substring
cargo test connections::tests       # run all tests in one module

# CI gate (mirror of GitHub Actions); see `rust-phase-gate` skill:
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Smoke-testing the TUI without a real terminal

`cargo run` requires a TTY (raw mode + alt screen). From a non-interactive shell, wrap with `script` to provide a PTY:

```bash
(sleep 1.5; printf 'q') | script -q /tmp/rmon.log target/release/resource-monitor
```

Exit code 0 + alt-screen entry/exit (`?1049h` / `?1049l`) in the log means terminal restoration works. Detail in the `tui-smoke-test` skill.

## CI

`.github/workflows/ci.yml` triggers on push and PR against `main`/`master`:

- `fmt` job runs once on Ubuntu (formatting is platform-agnostic)
- `gate` matrix runs `clippy -D warnings` + `test --locked` on `ubuntu-latest` and `macos-latest` because the codebase has cfg-gated paths (`collector/platform/{linux,macos}.rs`, `collector/gpu.rs` macOS-only, `collector/connections.rs` Linux-only) that won't be exercised on a single OS.
- `Swatinem/rust-cache@v2` keeps cached runs in the 1–2 minute range.
- Cancel-in-progress on the same ref so back-to-back pushes don't pile up.

## Architecture

### Three-thread model

1. **Sampler thread** (`app::sampler_loop`) — runs every `config.sample_interval_ms` (default 1000ms). Calls `SystemSource::refresh` once, then walks the `Registry` calling `Collector::sample(&mut CollectCtx)` on each, then `State::commit(snapshot)`.
2. **UI thread** (main, `App::event_loop`) — every `config.ui_tick_ms` (default 100ms): non-blocking `crossterm::event::poll`, `terminal.draw(|f| ui::render(f, &state, &mut ui_state, &theme))`. Reads via `state.with_view(|view| ...)`.
3. **GPU reader thread** (`gpu-reader`, only when `--gpu` on macOS) — owned by `GpuCollector`, parses powermetrics stdout into a `Mutex<GpuStats>`. Killed via `setpgid`-rooted SIGKILL on Drop.
4. **Container poller thread** (`container-poller`) — owned by `ContainerCollector`, runs `docker stats --no-stream --format json` every 2 s into a `Mutex<PollerState>`. Sampler reads cached results so the ~150 ms docker call doesn't bottleneck the 1 Hz tick. Drop signals shutdown + joins.
5. **Exporter thread** (planned, Phase 6) — tokio + axum serving `/metrics`, reading the same `SharedState`.

All threads share one `Arc<State>` (alias `SharedState`); state is guarded by an internal `RwLock`. Sampler is the sole writer; UI and (future) exporter are readers.

### Data flow & key types

- `state::Snapshot` carries:
  - `numeric: HashMap<MetricKey, f64>` — the time-series values that flow into the history ring buffer
  - `processes: Vec<ProcessSnapshot>`, `disks: Vec<DiskSnapshot>`, `networks: Vec<NetworkSnapshot>`, `sensors: Vec<SensorReading>`, `batteries: Vec<BatteryReading>` — point-in-time structured data, **not** historical (lists like processes are too dynamic for series storage)
- `MetricKey` naming convention: `<group>.<sub>` (e.g. `cpu.core.0`, `mem.used_bytes`, `net.eth0.rx_bps`, `sensor.temp.cpu_die`, `gpu.usage`). When adding a collector, follow this so widgets and the future exporter can address series uniformly.
- `state::History` is a per-key `VecDeque<f64>` ring buffer, capacity shared across all series (default 600 → 10 min @ 1Hz). `History::push_from(&Snapshot)` appends every numeric series in lockstep.
- `state::StateView` is the read-side handle exposed inside `with_view` — borrowed `current: Option<&Snapshot>` and `history: &History`. Anything else needs to be added to `StateInner` first and then re-exposed here.

### `CollectCtx` indirection

The `Collector::sample` signature takes `&mut CollectCtx<'_> { snapshot, system: &SystemSource }` rather than just a `&mut Snapshot` so collectors can read shared sysinfo handles (`system.system`, `system.disks`, `system.networks`, `system.users`) and the per-tick elapsed time (`system.last_refresh_elapsed`) used to compute network rates.

### Terminal lifecycle

`app::install_panic_hook` is called before entering raw mode. It chains `disable_raw_mode` + `LeaveAlternateScreen` into the existing panic hook so a crash doesn't leave the user's shell broken. Normal exit goes through `leave_terminal`. Anything that puts the terminal in a special mode (mouse, bracketed paste, etc.) must be undone there too.

### Shutdown

`Arc<AtomicBool>` shared between UI and sampler. UI sets it on `q`/`Ctrl-C`; sampler observes via `sleep_until_or_shutdown` (50ms-chunked sleep) so quit feels instant even at long sampling intervals. `GpuCollector::Drop` then SIGKILLs the powermetrics process group.

## Conventions

- **Edition 2024** (`Cargo.toml`). `let`-chains are in use (`app::event_loop`, `gpu::parse_line`).
- **Errors**: `anyhow::Result` at app boundaries; collectors should `tracing::warn!` on per-sample failures (logged by `Registry::sample_all`) rather than aborting the whole tick.
- **Logs go to stderr with ANSI off** so they don't fight the TUI on the same fd. Use `--debug` or `RUST_LOG=...` to surface them; redirect with `2>/tmp/rmon.log` to view live.
- **Phase discipline**: keep collectors decoupled from UI. A new metric is added by registering a `Collector` that writes to `Snapshot::set("group.sub", value)` — no UI changes needed for the value to flow into history. UI changes happen only when you want to *render* it. Full pattern in the `add-collector` skill.
- **Platform-cfg'd parsers stay testable**: when a parser is only called from `#[cfg(target_os = "linux")]` code, gate it with `#[cfg(any(test, target_os = "linux"))]` so its tests still run on the macOS CI runner. See `collector/connections.rs` for the pattern.
- **No `git add -A`** — stage by name. The repo has no secrets right now but this avoids future accidents (`.env`, large binaries).

## Code review

After every chunk of Rust source changes (`src/**/*.rs` or `Cargo.toml`),
**invoke the `rust-code-reviewer` subagent** before running the local
`rust-phase-gate` and pushing. Pass it:

- The list of files just modified (paths)
- A short summary of the intent of the change (which phase / which carryover)
- Any context the reviewer would otherwise have to re-derive (e.g. "this
  is the parser fn called only from cfg(target_os = "linux") code, see
  the platform-cfg test pattern")

Treat its findings as a peer review:

- Real defects → fix and re-run the gate.
- Style / lint nits → fix unless they conflict with the project conventions
  above; bias toward fixing.
- Missing-test calls → add the test if the function is pure; manual-only
  if it touches I/O.
- Speculative refactor suggestions → push back; this codebase prefers
  YAGNI (see how `parent_pid`, `id`, `captured_at` were dropped and
  re-added on demand rather than kept "just in case").

The skill `push-and-watch-ci` lists this as part of its pre-flight so
the review happens before CI minutes are spent. Skip the review only
for docs-only changes (`*.md`, `TODO.md`, `CLAUDE.md`).

## Platform-specific work

- **Cross-platform baseline**: `sysinfo` covers CPU/Mem/Disk/Process/Network. `SystemSource` owns the `System`, `Disks`, `Networks`, `Users` handles and refreshes them once per tick.
- **Linux-specific** (`collector/platform/linux.rs` + `collector/connections.rs`): hwmon walk for temp/fan, `/sys/class/power_supply/BAT*` for battery status + time-remaining, `/proc/net/{tcp,tcp6,udp,udp6}` for connection state counts.
- **macOS-specific** (`collector/platform/macos.rs` + `collector/gpu.rs`): `pmset -g batt` parser for battery, `sudo powermetrics --samplers gpu_power` subprocess for GPU. Apple Silicon thermal/fan via IOReport private framework is on the roadmap (would also unlock a no-sudo GPU path).

### Phase 3 GPU specifics

The GPU collector is **opt-in** (`--gpu`), **macOS-only**, and gated behind sudo:

1. `main.rs::ensure_gpu_prereqs` runs `sudo -v` *before* entering raw mode so the password prompt is reachable.
2. `GpuCollector::try_new` probes `sudo -n true`, then spawns `sudo powermetrics --samplers gpu_power -i 1000` with `setpgid(0, 0)` via `pre_exec` so sudo + powermetrics share a process group.
3. A dedicated `gpu-reader` thread parses `GPU HW active residency` / `GPU active frequency` / `GPU Power` lines into a `Mutex<GpuStats>`, stamping `last_update`.
4. `GpuCollector::sample` clones the mutex; if `last_update` is older than 5 s the stats are treated as stale and no numerics are emitted (panel falls back to "waiting for powermetrics…").
5. `Drop for GpuCollector` SIGKILLs the entire pgid so powermetrics doesn't outlive a clean shutdown. Caveat: if our process is itself SIGKILL'd the child still leaks (no portable fix on macOS).

Failure modes are *non-fatal*: sudo-not-cached, spawn-failed, parser-saw-nothing all degrade to the empty-state widget rather than crashing.

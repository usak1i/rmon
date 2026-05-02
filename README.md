# resource-monitor

A Rust TUI system monitor for **macOS + Linux** — built to be a more capable
htop with sparklines, modern colours, and (incrementally) GPU / network /
container / Prometheus support. Renders with [`ratatui`].

## Status

Built in numbered phases. Currently shipped:

| Phase | Scope |
|-------|-------|
| 0 | Scaffolding — three-thread sampler/UI/exporter model |
| 1 | CPU / Memory / Disk / Process collectors via `sysinfo`, sparklines, dark theme, focus / sort / search / kill / help keys |
| 2 | Network (per-interface RX/TX) + Sensors (battery on both platforms; Linux hwmon temp/fan) |
| 3 | Apple Silicon GPU via `sudo powermetrics` *or* sudo-less IOReport (opt-in) |
| 4 | Container panel (Docker via `bollard`) + Linux cgroup PID grouping (`g` toggle) |
| 5 | TOML alert rules (`[[alert]]` blocks) — firing tints panel borders, `a` opens an overlay |
| 6 | Apple Silicon CPU/GPU/ANE power via private IOReport (no sudo) + Prometheus `/metrics` exporter |

Roadmap & deferred items live in [`TODO.md`](TODO.md). High-level architecture
notes for contributors are in [`CLAUDE.md`](CLAUDE.md).

## Install / build

Requirements: Rust stable (1.85+ for edition 2024). No other system
dependencies for the core features.

```bash
git clone <repo-url> resource-monitor
cd resource-monitor
cargo build --release
./target/release/resource-monitor
```

If you don't have Rust installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source $HOME/.cargo/env
```

## Run

```bash
cargo run --release                                  # default
cargo run --release -- --gpu                         # macOS Apple Silicon: GPU panel via sudo+powermetrics
cargo run --release -- --gpu=ioreport                # macOS Apple Silicon: GPU panel via IOReport (no sudo)
cargo run --release -- --gpu=off                     # explicitly disable (same as omitting the flag)
cargo run --release -- --config my.toml              # custom config path
cargo run --release -- --debug                       # verbose tracing on stderr
cargo run --release -- --prometheus 127.0.0.1:9091   # expose /metrics
```

`--gpu` accepts `off | powermetrics | ioreport`. Bare `--gpu` is shorthand
for `--gpu=powermetrics` so existing invocations keep working.

`--help` for the full flag list.

## Keys

| Key | Action |
|-----|--------|
| `q` / `Ctrl-C` | Quit |
| `Tab` | Cycle focus between panels |
| `↑` `↓` `PgUp` `PgDn` | Move selection in the Process panel |
| `c` `m` `p` `n` | Sort processes by CPU% / Memory / PID / Name |
| `/` | Start incremental search (Enter accept, Esc cancel) |
| `F9` or `k` | Ask to kill the selected PID; `Enter` confirms (SIGTERM), `Esc` cancels |
| `?` | Toggle help overlay |
| `Esc` | Close help / cancel modal prompt |

## Layout

```
┌─ resource-monitor ─────────────────────────────────────────┐
├─ CPU ──────────┬─ Memory ──────┬─ GPU (--gpu only) ────────┤
│ per-core bars  │ RAM + Swap    │ usage / freq / power      │
│ + sparkline    │ gauges        │                           │
├─ Network ──────┴─────┬─ Sensors ──────────────────────────┤
│ RX/TX per iface      │ temp / fan / battery                │
├─ Disks ──────────────┴────────────────────────────────────┤
│ mounts table                                               │
├─ Processes ───────────────────────────────────────────────┤
│ PID  USER  CPU%  MEM%  STAT  TIME  COMMAND                │
│ ▶ selected row, sortable, searchable, killable             │
└────────────────────────────────────────────────────────────┘
```

When `--gpu` is off (the default), the GPU column collapses and the top row
is split 50/50 between CPU and Memory.

## Config

Loaded from `$XDG_CONFIG_HOME/resource-monitor/config.toml` (Linux) or
`~/Library/Application Support/resource-monitor/config.toml` (macOS), or via
`--config <path>`. Missing file → defaults; malformed file → startup error.

```toml
sample_interval_ms = 1000   # how often the sampler thread runs
ui_tick_ms         = 100    # how often the UI redraws
history_capacity   = 600    # ring-buffer points per metric (1Hz × 600 = 10 min)
```

Theme / alert / Prometheus settings will land in later phases — see
[`TODO.md`](TODO.md).

## Platform notes

### macOS GPU (`--gpu`)

Two backends, picked by the `--gpu` flag:

- `--gpu` / `--gpu=powermetrics` — spawns `sudo powermetrics --samplers
  gpu_power -i 1000`. Emits `gpu.usage` + `gpu.freq_mhz` + `gpu.power_mw`.
  `sudo -v` runs interactively before entering the TUI; on `q` / `Ctrl-C`
  the powermetrics child is SIGKILL'd cleanly via its process group.
- `--gpu=ioreport` — subscribes to the private `IOReport` framework's
  "GPU Stats" group (no sudo). Emits `gpu.usage` only; power is already
  reported under the Sensors panel via the Energy Model sampler, and a
  per-chip frequency table isn't exposed uniformly through IOReport.

Leaving `--gpu` off (the default) hides the GPU panel entirely.

### Linux sensors

Temperatures and fans come from `/sys/class/hwmon/*`. Battery from
`/sys/class/power_supply/BAT*`. No extra setup; no `lm-sensors` daemon
required.

### macOS sensors

Phase 2 ships battery (parsed from `pmset -g batt`). Phase 6 added
Apple Silicon CPU / GPU / ANE *power* readings (Watts) via the private
IOReport framework — no sudo, no powermetrics dependency. Die
temperatures and fan RPMs still need work; see `TODO.md`.

## Prometheus exporter

Pass `--prometheus <addr:port>` to expose a `/metrics` endpoint:

```bash
resource-monitor --prometheus 127.0.0.1:9091
curl -s http://127.0.0.1:9091/metrics | head
# # TYPE rmon_cpu_core_0 gauge
# rmon_cpu_core_0 35.29
# # TYPE rmon_cpu_total gauge
# rmon_cpu_total 18.75
# ...
```

All numeric series in `Snapshot::numeric` are emitted as gauges.
`MetricKey` names get sanitised (dots / dashes / slashes →
underscore) and prefixed with `rmon_`. Off by default to keep idle
resource use low.

A scrape config snippet for `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: rmon
    static_configs:
      - targets: ['127.0.0.1:9091']
    scrape_interval: 5s
```

## Logs

`tracing` writes to **stderr** with ANSI off so it doesn't fight the TUI.
The TUI lives in the alternate screen, so stderr only shows up after exit.
For live logs, redirect: `cargo run -- --debug 2>/tmp/rmon.log`.

## Development

Quality gate before declaring a phase complete:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The repo also ships two project skills under `.claude/skills/` — pick them
up automatically in Claude Code:

- **`rust-phase-gate`** — runs the three commands above with failure handling
- **`tui-smoke-test`** — drives the TUI via `script` PTY to verify clean
  startup / shutdown without a real terminal

## License

Personal project — no license declared yet.

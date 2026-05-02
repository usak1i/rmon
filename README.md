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
| 3 | Apple Silicon GPU via `sudo powermetrics` (opt-in) |

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
cargo run --release                       # default
cargo run --release -- --gpu              # macOS only: enable GPU panel
cargo run --release -- --config my.toml   # custom config path
cargo run --release -- --debug            # verbose tracing on stderr
```

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

GPU monitoring spawns `sudo powermetrics --samplers gpu_power -i 1000` because
Apple gates GPU stats behind root. The flow:

1. Before entering the TUI, `sudo -v` runs interactively so the password
   prompt is reachable. If you've recently authenticated, this is a no-op.
2. The sampler thread spawns powermetrics; a dedicated `gpu-reader` thread
   parses its stdout into the `gpu.usage` / `gpu.freq_mhz` / `gpu.power_mw`
   numerics.
3. On `q` / `Ctrl-C`, the powermetrics child gets SIGKILL'd cleanly.

If you don't want the sudo dance, leave `--gpu` off. An IOReport-based
no-sudo path is on the roadmap.

### Linux sensors

Temperatures and fans come from `/sys/class/hwmon/*`. Battery from
`/sys/class/power_supply/BAT*`. No extra setup; no `lm-sensors` daemon
required.

### macOS sensors

Phase 2 ships battery percentage only (parsed from `pmset -g batt`).
Apple Silicon thermals/fans need the private IOReport framework — tracked
under Phase 2.5 carryovers.

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

# TODO

Roadmap for `resource-monitor`. Full design rationale lives in
`~/.claude/plans/rust-htop-cpu-memory-disk-frolicking-meadow.md`; this file
tracks concrete work to do.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done

**Done so far**
- `[x]` Phase 0 — scaffolding (cargo project, three-thread model, ratatui shell)
- `[x]` Phase 1 — MVP (CPU/Mem/Disk/Process collectors via sysinfo, four-panel TUI with sparklines, dark theme, focus / sort / search / kill / help keys)
- `[x]` Phase 2 — Network (per-interface RX/TX + total sparkline, loopback hidden) + Sensors (battery on both platforms; Linux hwmon temp/fan; macOS thermal deferred), six-panel layout
- `[x]` Phase 3 — Apple Silicon GPU via `sudo powermetrics` (opt-in `--gpu` flag, sudo pre-auth before TUI, dedicated reader thread, hand-rolled parser, conditional 7-panel layout)
- `[x]` Phase 4 v1 — Container panel via `docker stats` subprocess in a dedicated poller thread (2s cache), eight-panel layout (Disk and Container share the disk row)

---

## Phase 2.5 — Carryover from Phase 2

- [x] **Battery time-remaining + status** — `BatteryReading` with status
  enum + estimated minutes. macOS via richer `pmset -g batt` parser; Linux
  via `time_to_empty_now` / `time_to_full_now`.
- [x] **Linux connection counts** — TCP states (ESTABLISHED / LISTEN /
  TIME_WAIT) and UDP totals from `/proc/net/{tcp,tcp6,udp,udp6}`. Surfaced
  as a footer line in the Network widget when present.
- [ ] **macOS thermal/fan via IOReport** — Apple Silicon has no public SMC
  equivalent for die temperatures. Bind to the private `IOReport` framework
  via a small FFI shim. Emit `sensor.temp.cpu_die`, `sensor.fan.<idx>` etc.
  alongside the Linux hwmon path.
- [ ] **AC online indicator** — battery `status` already implies the
  charging state, but a dedicated AC sensor (charging cable plugged but
  battery full) would be honest. Linux `/sys/class/power_supply/AC*/online`,
  macOS the "Now drawing from 'AC Power'" header line in pmset.
- [ ] **Per-process network IO** — sysinfo doesn't expose this; needs eBPF
  on Linux and `nettop` on macOS. Genuinely hard, only worth it if a user
  asks.

### Verification still owed
- [ ] On Linux VM: `iperf3` → confirm RX/TX series move
- [ ] On a Linux box with hwmon: `stress-ng --cpu N` → confirm temperature
  climbs in Sensors panel

---

## Phase 3 — Apple Silicon GPU (carryovers)

Path A (sudo + powermetrics) is shipped. Remaining items:

- [ ] **Path B (IOReport, no sudo)**: bind to the private `IOReport` framework. No sudo, but version-fragile across macOS releases. Gate behind `--gpu=ioreport`.
- [ ] **Additional samplers**: `--samplers ane_power`, `media_power`, `cpu_power` would let us emit `gpu.ane_*`, `gpu.media_*`, package power. Trade-off is more parser surface area.
- [x] **Stale-data check**: GpuStats tracks `last_update`; sample() skips emitting numerics if older than 5 s, so the widget falls back to its empty state instead of a frozen reading.
- [x] **Process-group containment**: `Command::pre_exec` sets `setpgid(0, 0)` so sudo + powermetrics share a pgid; `Drop` kills the whole group so powermetrics doesn't outlive a clean shutdown. Note: this only helps when our process exits cleanly — a SIGKILL of our parent still leaks the child (no portable way to fix on macOS).

### NVIDIA Linux (Phase 3.5)
- [ ] Detect `libnvidia-ml.so` at runtime; if present use `nvml-wrapper` for utilisation/VRAM/temp
- [ ] Same emit prefix (`gpu.<n>.usage` etc.) as Apple path so widgets are uniform
- [ ] AMD via `/sys/class/drm/card*/device/gpu_busy_percent` if no NVML

---

## Phase 4.5 — Container carryovers

Phase 4 v1 ships the docker-CLI poller path. Remaining items from the
original plan:

- [ ] **Linux cgroup PID grouping**: parse `/proc/<pid>/cgroup`, extract
  `docker/...` and `kubepods/.../pod<uuid>` IDs. Lets us group processes
  by container without depending on the docker daemon.
- [ ] **Process panel grouped/flat toggle** (key `C`?): when grouped,
  render processes as a forest indented under their container ID;
  unattributed processes go in a `system` bucket at the top.
- [ ] **bollard upgrade**: replace the docker CLI subprocess with a real
  Docker API client. Cleaner streaming model, faster, types instead of
  string parsing. Cost: pulls in `tokio` for async runtime — defer until
  Phase 6 (Prometheus exporter) needs tokio anyway, so the marginal
  cost is zero.
- [ ] **Image / status / pids columns**: `docker stats` doesn't emit
  these; either add a parallel `docker ps` call or move to bollard for
  unified data.
- [ ] **Container detail keybinding**: `i` → expand selected container
  with logs tail or env / labels; `K` → `docker stop <id>` with
  confirmation. Needs the `id` field re-added to `ContainerSnapshot`.

---

## Phase 5 — Alerts

- [ ] TOML rule schema:
  ```toml
  [[alert]]
  name = "cpu hot"
  metric = "cpu.total"
  op = ">"
  value = 90.0
  duration = "30s"
  severity = "warn"
  ```
- [ ] Evaluator runs after each sample; tracks per-rule "in-breach since" timestamp; fires when breach exceeds `duration`
- [ ] UI: highlight the relevant panel border, optional terminal bell, write to log
- [ ] `a` key opens an alert overlay (active + recently fired, with dismiss)
- [ ] Validate config on startup; surface bad rules with file:line in error output

---

## Phase 6 — Prometheus exporter

- [ ] `--prometheus <addr:port>` CLI flag (off by default — keep idle CPU low)
- [ ] Spawn tokio runtime + axum on the configured address only when flag set
- [ ] `/metrics` endpoint serialises `Snapshot::numeric` as `gauge` lines, plus a stable subset of process / disk fields as labels
- [ ] Add `tokio` and `axum` as **optional** features (`cargo add tokio --optional`) so non-exporter users don't pay the compile cost
- [ ] CI: `promtool check metrics` against the live endpoint
- [ ] Document scrape configuration snippet in README (when a README is requested)

---

## Tech debt / loose ends from earlier phases

- [ ] **Process tree view**: re-add `ProcessSnapshot::parent_pid` (removed in Phase 1 cleanup) and a `t` toggle that renders processes as a forest. Indent children, recompute aggregated CPU%/MEM at parent rows.
- [ ] **Theme**: ship a `Theme::light()` preset and let `[theme]` in TOML override individual colours; expose a `--theme` CLI flag.
- [ ] **Disk IO throughput**: sysinfo doesn't expose system-wide block-device IO. Read `/proc/diskstats` on Linux, IOKit on macOS — emit `disk.<name>.{read,write}_bps`. Land in Phase 2 alongside Network if scope allows; otherwise a Phase 1.5 follow-up.
- [ ] **Search regression scroll**: when search filters down rows, selection clamps to the new last row, but scroll offset can leave selection off-screen. Reset `TableState::offset` when filter set changes.
- [ ] **Per-process kill UX**: confirm dialog currently blocks all keys. Make it modal-but-with-Esc-everywhere (already works) and consider supporting `9` (SIGKILL) as a second-step escalation if SIGTERM is ignored.
- [ ] **Drop the dead `MetricKey::new` allocation** on hot read paths — every `snap.get(...)` allocates a new `String`. Move to `&str` lookups via a borrowed key type once a Phase shows up on the profiler.

---

## CI & release polish

- [ ] GitHub Actions: matrix build on `ubuntu-latest` + `macos-latest`, run the `rust-phase-gate` skill (fmt --check + clippy -D warnings + test)
- [ ] Cache `~/.cargo/registry` and `target/` between runs
- [ ] Pre-commit hook (optional, off by default) that runs `cargo fmt`
- [ ] Release workflow: tag → build static-ish binaries for macOS (universal2) and Linux x86_64/arm64, attach to GitHub Releases
- [ ] Distribution: `cargo install resource-monitor`; consider a Homebrew formula once we have a v0.1.0 tag
- [ ] Demo asciicast or terminalizer GIF for the README (when README is added)

---

## Future / beyond the plan

These are explicit *non-goals* in the current plan but worth revisiting if the
project gets traction:

- [ ] Multi-machine view (SSH/agent fan-out) — would need a wire protocol & permission story
- [ ] Record / replay (capture metrics window → replay for post-mortem)
- [ ] WASM plugin host for custom metrics / panels
- [ ] Web UI mode that shares the collector layer with the TUI
- [ ] Windows support — would require a third platform module and a Windows CI runner

---

## Open questions

- **Binary name**: keep `resource-monitor` or also install as the shorter alias `rmon`? Decide before tagging v0.1.
- **Default sample interval**: 1Hz currently. Bump to 500ms once we have GPU/Net to make sparklines feel live, or keep low for laptop battery?
- **Process default sort**: CPU% (htop default). Worth a `[ui] default_sort = "memory"` config knob?
- **Config schema versioning**: when alerts/themes land, the TOML grows. Add a `version = 1` field now to give us a clean upgrade path?

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
- `[x]` Phase 4.5 — Linux cgroup PID grouping (`/proc/<pid>/cgroup` parser handling cgroup v1, v2 systemd-style docker / cri-containerd), Process panel `g` toggle that renders container header rows + indented children + a final `system` bucket for unattributed PIDs
- `[x]` Phase 5 — TOML alert rules (`[[alert]]` blocks with `op` ∈ `> < >= <=`, `duration`, `severity`); evaluator with breach-since tracking, firing transitions tint panel borders + ring the terminal bell + log via tracing, `a` key opens an overlay listing currently-firing + last 50 transitions
- `[x]` Bollard upgrade — replaced the docker CLI subprocess with bollard 0.20 driven by a current-thread tokio runtime on the `container-poller` thread. Pulls in tokio (`net`/`rt`/`time` only) so Phase 6 Prometheus exporter can reuse it. Pure CPU% formula extracted as testable fn.
- `[x]` IOReport (B.2 partial) — Apple Silicon CPU / GPU / ANE *power* readings (Watts, no sudo) via the private IOReport framework. `build.rs` adds `/usr/lib` to the linker search path so `libIOReport.dylib` resolves on macOS 26. SensorsCollector holds an optional sampler; `power` category surfaces in the Sensors widget. Thermal sensors and the no-sudo GPU usage path stay as carryovers.
- `[x]` Phase 6 — Prometheus `/metrics` exporter, opt-in via `--prometheus <addr:port>`. Dedicated thread driving a current-thread tokio runtime + axum; graceful shutdown via `tokio::sync::Notify`. `MetricKey` → Prometheus name with sanitiser (dots/dashes/slashes → underscore, leading-digit / non-ASCII rejected) + `rmon_` prefix. 5 unit tests on the sanitiser.

---

## Phase 2.5 — Carryover from Phase 2

- [x] **Battery time-remaining + status** — `BatteryReading` with status
  enum + estimated minutes. macOS via richer `pmset -g batt` parser; Linux
  via `time_to_empty_now` / `time_to_full_now`.
- [x] **Linux connection counts** — TCP states (ESTABLISHED / LISTEN /
  TIME_WAIT) and UDP totals from `/proc/net/{tcp,tcp6,udp,udp6}`. Surfaced
  as a footer line in the Network widget when present.
- [ ] **macOS thermal sensors** — IOReport infrastructure is in place
  (`collector/platform/ioreport.rs`), but the channel groups carrying
  CPU die / GPU die temperatures and fan RPMs are gated by chip
  generation (PMP / SMC) and aren't exposed uniformly across Apple
  Silicon. Needs per-chip channel discovery + a fallback story.
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

- [ ] **Path B (IOReport, no sudo)**: extend the existing IOReport
  sampler with a second subscription on the `GPU Stats` group to read
  active vs idle residency. Promote `--gpu` to a value-enum
  (`off|powermetrics|ioreport`) and default to `ioreport` on macOS so
  the sudo prompt is opt-in. The IOReport FFI itself is now in place
  (`collector/platform/ioreport.rs`) — this is mostly channel-name
  matching + state-residency math.
- [ ] **Additional samplers**: `--samplers ane_power`, `media_power`, `cpu_power` would let us emit `gpu.ane_*`, `gpu.media_*`, package power. Trade-off is more parser surface area.
- [x] **Stale-data check**: GpuStats tracks `last_update`; sample() skips emitting numerics if older than 5 s, so the widget falls back to its empty state instead of a frozen reading.
- [x] **Process-group containment**: `Command::pre_exec` sets `setpgid(0, 0)` so sudo + powermetrics share a pgid; `Drop` kills the whole group so powermetrics doesn't outlive a clean shutdown. Note: this only helps when our process exits cleanly — a SIGKILL of our parent still leaks the child (no portable way to fix on macOS).

### NVIDIA Linux (Phase 3.5)
- [ ] Detect `libnvidia-ml.so` at runtime; if present use `nvml-wrapper` for utilisation/VRAM/temp
- [ ] Same emit prefix (`gpu.<n>.usage` etc.) as Apple path so widgets are uniform
- [ ] AMD via `/sys/class/drm/card*/device/gpu_busy_percent` if no NVML

---

## Phase 4 carryovers (post-4.5)

- [x] **Linux cgroup PID grouping**: cgroup v1 + v2 systemd-style docker /
  cri-containerd parsing, six unit tests. PID→container_id cache in
  ProcessCollector to avoid re-reading `/proc/<pid>/cgroup` per tick.
- [x] **Process panel grouped/flat toggle** (`g`): container header rows
  with aggregate CPU/MEM, indented children, `system` bucket for
  unattributed PIDs. Header rows are non-PID (kill on a header is a
  no-op).
- [x] **bollard upgrade**: replaced the docker CLI subprocess with
  bollard 0.20 + a current-thread tokio runtime on the existing poller
  thread. Pure CPU% formula extracted to `cpu_percent_from_deltas` with
  3 unit tests. Same `Vec<ContainerSnapshot>` interface; no UI changes.
- [ ] **Image / status / pids columns**: `docker stats` doesn't emit
  these; either add a parallel `docker ps` call or move to bollard for
  unified data.
- [ ] **Container detail keybinding**: `i` → expand selected container
  with logs tail or env / labels; `K` → `docker stop <id>` with
  confirmation; `Enter` on a container header in grouped mode could
  scope the kill to the whole container.

---

## Phase 5 — Alerts (carryovers)

Phase 5 v1 ships rule definition + evaluator + UI integration. Outstanding:

- [ ] **Alert dismiss**: a way to acknowledge a firing alert so it stops
  tinting the panel border without clearing on its own. Needs persistent
  per-event state in `StateInner`.
- [ ] **Bell quiet config**: some users hate the BEL. Add `bell = false`
  knob to `[alert]` global section (or per-rule).
- [ ] **Severity colour customisation**: theme override for the firing
  border so users can pick yellow vs red themselves.
- [ ] **Word-form ops**: accept `gt` / `ge` / `lt` / `le` in addition to
  symbols, since YAML/TOML reviewers sometimes find symbols look like
  comparison operators in the doc itself.

---

## Phase 6 — Prometheus exporter (carryovers)

Phase 6 v1 ships the basic `/metrics` endpoint. Outstanding:

- [ ] **Process / disk labels**: today only `Snapshot::numeric` flows out
  as gauges. Per-process and per-disk metrics with `pid`/`name`/`mount`
  labels would let users build per-container dashboards. Watch
  cardinality — gate behind `--prometheus-include-processes` if added.
- [ ] **`hostname` label injection**: useful when one Prometheus scrapes
  multiple boxes. Prometheus already adds `instance`; consider whether
  duplicating that into a label is worth it.
- [ ] **`promtool check metrics` in CI**: hit the endpoint from the
  matrix runners and validate format. Today the smoke test only checks
  `200` + a recognisable line. promtool would catch label / type drift.
- [ ] **HELP comments** per metric (currently only `# TYPE`). Optional
  but Prometheus Operators tend to want them.
- [ ] **Optional `tokio` + `axum`**: tokio is a hard dep now (bollard
  needs it), so making axum truly opt-in via cargo features mostly
  saves compile time, not binary size. Defer until someone asks.

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

---
name: add-collector
description: Wire a new system-metric collector into the project. Use when adding a new resource type (network, GPU, sensors, container stats, connection counts, etc.). Walks the six files that need touching and the order to do them in so compile errors don't obscure the real change.
---

# add-collector

The `Collector` trait + `Registry` pattern means a new metric type
touches a predictable set of files. Doing them out of order produces
cascading compile errors that obscure the real change. Follow this
order.

## 1. Decide platform scope

- **Cross-platform** (sysinfo or pure-Rust): no cfg gating.
- **Single-platform** (Linux `/proc`, macOS IOKit/`pmset`): wrap module
  declaration with `#[cfg(target_os = "...")]` in `collector/mod.rs`.
- **Both, with platform-specific implementation**: factor the
  platform-specific bits into `src/collector/platform/{linux,macos}.rs`
  behind a uniform `read_*()` API; the collector itself is
  cross-platform.

## 2. Snapshot types — only if structured output

If the collector emits a *list of items* (processes, disks, network
interfaces, batteries, sensor readings), add a struct to
`src/state/snapshot.rs`:

```rust
#[derive(Debug, Clone)]
pub struct FooSnapshot {
    pub name: String,
    pub value: f64,
    // ...
}
```

Add a `Vec<FooSnapshot>` field to `Snapshot`, default to empty in
`Snapshot::new`, and re-export from `src/state/mod.rs`.

If the collector emits **only numbers** (CPU%, memory bytes,
temperature), skip this step — values flow into `Snapshot::numeric` via
`snapshot.set("foo.<sub>", value)`.

## 3. Collector module

Create `src/collector/<name>.rs`:

```rust
use anyhow::Result;
use super::{CollectCtx, Collector};

pub struct FooCollector;

impl FooCollector {
    pub fn new() -> Self { Self }
}

impl Default for FooCollector {
    fn default() -> Self { Self::new() }
}

impl Collector for FooCollector {
    fn name(&self) -> &'static str { "foo" }
    fn sample(&mut self, ctx: &mut CollectCtx<'_>) -> Result<()> {
        // read from ctx.system or platform module
        ctx.snapshot.set("foo.metric", value);
        Ok(())
    }
}
```

**Naming convention** for emitted metrics: `<group>.<sub>` — e.g.
`cpu.core.0`, `net.eth0.rx_bps`, `sensor.temp.cpu_die`. Widgets read
these via `snap.get("foo.metric")`.

## 4. Wire into Registry

`src/collector/mod.rs`:

```rust
mod foo;
pub use foo::FooCollector;
```

`src/app.rs::sampler_loop`:

```rust
registry.register(Box::new(FooCollector::new()));
```

For platform-conditional collectors (e.g. macOS-only GPU):

```rust
#[cfg(target_os = "macos")]
if some_flag {
    match crate::collector::FooCollector::try_new() {
        Ok(c) => registry.register(Box::new(c)),
        Err(e) => tracing::warn!(error = %e, "foo collector disabled"),
    }
}
```

## 5. Sysinfo refresh — easily forgotten

If you read from `sysinfo::Networks` / `Disks` / `System`, add the
corresponding `.refresh()` call to `SystemSource::refresh`. Forgetting
this causes stale data with no compile error — symptoms are values
that are correct on first sample then never change.

## 6. Widget — only if user-visible

Create `src/ui/widgets/<name>.rs`:

```rust
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use crate::state::StateView;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &StateView,
              theme: &Theme, focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(Span::styled(" Foo ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };
    // actual rendering using snap.get("foo.metric") / snap.foos / etc.
}
```

Then:

- Register module in `src/ui/widgets/mod.rs`
- Add `Panel::Foo` variant to `src/ui/state.rs` and update `Panel::next`
- Slot into `src/ui/mod.rs::render` layout, calling the new widget
- (Optional) Update help overlay in `src/ui/widgets/help.rs` if you
  added new keybindings

## 7. Tests

Pure parsers go in `#[cfg(test)] mod tests` at the bottom of the
collector file. No I/O, just string fixtures → expected output. For
parsers that are only compiled on one platform, add
`#[cfg(any(test, target_os = "linux"))]` so tests still cover them on
the other platform's CI runner.

Live behaviour (does the value match `top`?) is manual verification —
not worth automating per phase.

## 8. Run the gate, then push

`rust-phase-gate` skill locally, then if you added platform-cfg'd code,
the gate may pass on your dev box but fail on the other platform —
push and watch CI via `push-and-watch-ci` skill before declaring done.

## Common gotchas

- **Forgot `Registry::register`**: collector compiles, never runs. The
  metric never appears. Symptom: widget shows "…" forever.
- **Forgot `SystemSource::refresh` field**: data freezes at startup
  values. Symptom: numbers correct once then never change.
- **Widget reads wrong metric key**: typo, no compile error, panel
  silently renders zero. Cross-check the string against `snapshot.set`
  calls.
- **Platform-cfg'd parser causes "unused" warnings on the other
  platform**: gate it with `#[cfg(any(test, target_os = "linux"))]` so
  tests can exercise it on macOS CI even when the caller is Linux-only.
- **New `Panel` variant breaks `Panel::next` cycling**: forgetting to
  add a match arm or to handle a feature-flag-disabled panel makes Tab
  skip into Panel::Cpu unexpectedly.

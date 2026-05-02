---
name: tui-smoke-test
description: Smoke-test a Rust TUI binary headlessly using `script` to provide a PTY, verifying the binary builds, opens an alt screen, runs the event loop, and exits cleanly on `q` without leaving the terminal in raw mode. Use after edits that touch the event loop, terminal setup/teardown, panic hook, or any rendering code path; whenever you need to verify a TUI change without an interactive terminal of your own.
---

# tui-smoke-test

A TUI needs a real TTY for raw mode + alt screen. A non-interactive shell (Claude Code's Bash, CI runner) doesn't have one, so calling the binary directly will either fail or hang. The `script` command on macOS/Linux provides a PTY and lets us drive the binary as if from a terminal.

This skill verifies *liveness and cleanup*, not correctness — it answers "does it open and close without breaking the user's shell?", not "are CPU values right?".

## Build first

```bash
export PATH="$HOME/.cargo/bin:$PATH" && cargo build --release
```

(Debug builds work but cold-start is slower, which makes timing the `q` send fragile.)

## Run it

```bash
(sleep 1.5; printf 'q') | script -q /tmp/rmon-smoke.log target/release/resource-monitor
echo "exit: $?"
```

The `(sleep 1.5; printf 'q')` lets the sampler fire at least once (≥1 second at default 1Hz) and renders ~10 frames before sending `q` over the PTY.

For a binary that takes flags (e.g. `--config`), pass them after the binary path:
```bash
(sleep 1.5; printf 'q') | script -q /tmp/rmon-smoke.log target/release/resource-monitor --debug
```

## What to look for

Inspect `/tmp/rmon-smoke.log`:

- **Exit code 0** (echoed after the pipe)
- Log contains both `[?1049h` (enter alt screen) and `[?1049l` (leave alt screen) — proves the cleanup path ran
- Log contains both `[?25l` (cursor hidden) and `[?25h` (cursor restored)
- After the run, your shell still echoes input normally (raw mode is off)

A quick check:

```bash
grep -q '\[?1049h' /tmp/rmon-smoke.log && grep -q '\[?1049l' /tmp/rmon-smoke.log && echo "alt-screen pair OK"
```

## Common failures

- **Exit code != 0 with no panic**: tracing logs go to stderr → captured in the log. Read it.
- **No `?1049l` in log**: the cleanup didn't run — likely an early `return` in `App::run` skipped `leave_terminal`, or the panic hook is restoring stdout but not the alt screen.
- **Terminal stays broken after exit**: the panic hook is missing or the process exited via SIGKILL (uncatchable). Confirm `app::install_panic_hook` is called *before* `enter_terminal()`.
- **`q` ignored, hangs until SIGINT**: `should_quit` regressed, or `event::poll` is being given `Duration::ZERO` and never reading the key.
- **`script: command not found`**: macOS ships BSD `script`. On Linux: `apt install bsdmainutils` or `util-linux`.

## Cleanup

```bash
rm -f /tmp/rmon-smoke.log
```

## When NOT to use

- For runtime correctness (CPU% matches `top`, sparklines move under load) — that needs interactive verification by the user, or a `--dump`-style headless mode that prints metrics as JSON.
- For input that needs to be observed across multiple frames (typing in a search box, multi-key chord) — the simple `printf` pipeline can't model that. Use a real terminal for those flows.

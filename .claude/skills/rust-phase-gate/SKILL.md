---
name: rust-phase-gate
description: Run the strict Rust quality gate (cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test) before declaring a phase or feature complete. Use when wrapping up implementation work, after fixing bugs, or when the user says "phase done", "final check", "ready to ship", "lint and test", "gate", or asks to verify nothing broke.
---

# rust-phase-gate

This project ships in numbered phases (see `~/.claude/plans/rust-htop-cpu-memory-disk-frolicking-meadow.md`). A phase is "done" only when **all three** of the following pass with the strict settings — never weaken them.

1. `cargo fmt --check` — formatting is canonical
2. `cargo clippy --all-targets -- -D warnings` — warnings are errors
3. `cargo test` — unit + integration tests green

## Run it

If `cargo` is not on PATH (this user's machine doesn't auto-source `~/.cargo/env` in non-interactive shells), prepend the export:

```bash
export PATH="$HOME/.cargo/bin:$PATH" && \
cargo fmt --check && \
cargo clippy --all-targets -- -D warnings && \
cargo test
```

`&&` chains: stop on the first failure so you see the relevant error, not later cascades.

## Failure handling

- **fmt fails** → run `cargo fmt` (without `--check`) to auto-apply, then re-run the gate. Don't hand-format.
- **clippy fails** → read the lint, then either fix the code or, if the lint truly doesn't fit, add `#[allow(clippy::<name>)]` on the smallest possible item with a one-line justification. Do **not** drop `-D warnings`.
- **tests fail** → fix the regression. Never silence with `#[ignore]`. If a test is genuinely obsolete, delete it.
- **Spurious dead-code warnings during scaffolding** → if a scaffold API is wired but not yet called by the consumer that lands in the very next step, a *scoped* `#[allow(dead_code)]` with a comment naming the consumer is acceptable; remove it as soon as the consumer arrives.

## After it passes

- If a binary changed, also invoke the `tui-smoke-test` skill — clippy/test do not catch runtime breakage in the event loop or terminal teardown.
- Update the task list and any plan/CLAUDE.md note that depended on the gate.

## What this skill is not

Not for picking lints to apply, choosing a test runner, or general debugging. It is the final go/no-go check at the end of a chunk of work.

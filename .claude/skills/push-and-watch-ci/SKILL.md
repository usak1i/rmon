---
name: push-and-watch-ci
description: Commit local work, push to GitHub, and watch the CI workflow to completion. Use when finishing a coherent chunk of work (phase, carryover batch, bug fix) — especially when you want CI to verify on platforms you can't run locally (e.g. Linux paths from a macOS dev box). Implies the rust-phase-gate already passed locally.
---

# push-and-watch-ci

Standard end-of-batch flow: commit, push, and watch the GitHub Actions
run *in the background* so the agent stays responsive while CI runs.

## Pre-flight

1. Run `rust-phase-gate` locally first. Pushing red code wastes CI minutes
   and pollutes the run history.
2. Update docs that depend on the work — typically `CLAUDE.md` "Done so
   far" line and `TODO.md` checkboxes. Stale docs make PR diffs noisy.

## Commit

- Use HEREDOC so multi-line messages don't get mangled by shell quoting.
- Title under 70 chars, no `feat:` / `fix:` prefix in this project.
- Body is a bulleted list — one bullet per logically distinct change so
  reviewers can navigate.
- End with the Co-Authored-By line.
- Stage files **by name**, not `git add -A` — avoids accidentally
  including `.env`, large binaries, etc. Run `git status` first to
  confirm the file list.

```bash
git add <files...>
git commit -m "$(cat <<'EOF'
<one-line title>

- bullet 1
- bullet 2

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

## Push + watch

```bash
git push
RUN_ID=$(gh run list --repo <owner>/<repo> --limit 1 \
          --json databaseId --jq '.[0].databaseId')
gh run watch "$RUN_ID" --repo <owner>/<repo> --exit-status
```

Run `gh run watch` via the Bash tool with `run_in_background=true`. The
agent is freed up and gets a notification on completion; success or
failure is conveyed by exit code (because of `--exit-status`). Then
confirm with:

```bash
gh run list --repo <owner>/<repo> --limit 1
```

## Failure handling

- **CI fails on a platform you can't reproduce locally**: pull the
  failing log first via `gh run view <id> --log-failed`. Don't push
  speculative fixes blind — a Linux-only clippy lint, for instance, is
  visible in the log without re-running the whole job.
- **Push rejected (non-fast-forward)**: someone else pushed. `git pull
  --rebase`, re-run the local gate, push again.
- **Force-push to main**: never. The CI safety net only works if history
  is append-only.
- **Push to `main` directly is fine for solo repos** like this one (CI
  is the safety net). For shared repos, use a feature branch + PR.

## When NOT to use

- `cargo test` failing locally → fix that first (`rust-phase-gate`).
- Work isn't a coherent chunk — don't commit half-baked state just to
  get CI verification. Use a scratch branch or stash.
- The change is docs-only and CI doesn't cover docs — `git push` without
  the watch is enough.

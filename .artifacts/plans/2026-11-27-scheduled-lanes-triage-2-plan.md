# Scheduled CI lanes — second triage: plan & record

Date: 2026-11-27. Diagnosis + compressed fixes for the two scheduled lanes still
red after the first triage's re-dispatch (17:41 UTC, 2026-11-27). Follow-up to
`.artifacts/plans/2026-11-27-scheduled-lanes-triage-plan.md`.
Branch: `ci/scheduled-lanes-triage-2` (worktree `taskboard-WT-ci-scheduled-lanes-triage-2`).

## Lane 1 — miri (#1479), run 46043812493

### Root cause

Job `cargo miri (domain — core: data/substrate)` (id 140657927173) fails two
tests in `taskboard-domain --test vocabulary_guards`. Job log:

```
thread 'journal_entries_appears_nowhere_in_the_crate_sources' (1001) panicked at crates/taskboard-domain/tests/vocabulary_guards.rs:143:56:
cargo sets the manifest dir: NotPresent
```

(and the same `NotPresent` at `vocabulary_guards.rs:112:56` for
`no_journal_spelled_code_symbols_outside_the_undo_substrate`).

The file gates itself out of Miri with a **crate-level**
`#![cfg_attr(miri, ignore)]` (line 12) — but the `ignore` attribute is only
valid on functions, so rustc emits
`warning: the 'ignore' attribute cannot be used on crates` and the gate is a
no-op. The guards then run under Miri, where
`std::env::var("CARGO_MANIFEST_DIR")` reads `NotPresent` (Miri isolates the
environment; the workflow's `MIRIFLAGS` forward only
`PROPTEST_DISABLE_FAILURE_PERSISTENCE`/`PROPTEST_CASES`,
`.github/workflows/miri.yml:37-39`). Both tests `expect(...)` on that var and
panic; the third test in the file (pure, no fs/env) passes. They pass in the
ordinary suite because cargo sets the env var there.

### Fix

`crates/taskboard-domain/tests/vocabulary_guards.rs` — remove the invalid
crate-level attribute; move the gate onto the two source-scanning `#[test]`
fns as `#[cfg_attr(miri, ignore)]` (valid on functions, still functions under
miri cfg; verified with `RUSTFLAGS="--cfg miri" cargo check`), and record the
reason in the module docs (the doc already stated the intent — "the suite
reads the crate's own sources from disk, which Miri's isolation scopes out" —
now it names the env-var `NotPresent` and #1479 too). This is the recorded
reason the module always intended, not a new weakening: the guards are fully
enforced by the ordinary suite.

### Verification

Dispatcher dispatches `miri` (workflow_dispatch). Green = all five jobs
success, with `vocabulary_guards` reporting 1 passed / 2 ignored in the
`domain — core: data/substrate` job.

## Lane 2 — mutation testing (#1480), run 46043816740

### Root cause

All four matrix shards fail ~2 min in, in `Run Mutation Testing`:

```
2026-11-27T17:43:14.5039876Z error: failed to run custom build command for `yeslogic-fontconfig-sys v6.0.1`
2026-11-27T17:43:14.5050071Z   called `Result::unwrap()` on an `Err` value: "... Package 'fontconfig' not found ..."
2026-11-27T17:43:14.2851285Z ERROR cargo build failed in an unmutated tree, so no mutants were tested
```

cargo-mutants' baseline build compiles the workspace test binaries, which
reach `taskboard-app`'s slint/pdf-export dependency chain
(`yeslogic-fontconfig-sys` needs `pkg-config fontconfig`). The matrix job and
the `app-pdf-commands` job install only
`lld libdbus-1-dev pkg-config` (mutation-testing.yml:99, :250);
`libfontconfig1-dev` was only installed in the `ui-pure-modules` job (:181) —
and every other CI lane (ci.yml, benchmark.yml) installs it. Not a mutants
version issue, not a cap-kill, not survivors.

### Fix

`.github/workflows/mutation-testing.yml` — add `libfontconfig1-dev` to both
non-UI apt steps (the `mutants` matrix job and `app-pdf-commands`).

### Verification

Dispatcher dispatches `mutation-testing`. Green = the four matrix shards and
`app-pdf-commands` get past the baseline build into actual mutation testing
(first signs: `Running mutants` progress instead of the two-minute
`cargo build failed in an unmutated tree` error); `ui pure modules`
unchanged. Note the shards may now run to completion for the first time —
survivor-shaped exits are a green report outcome per the workflow's own
comment; a genuine cap-kill would be loud and is the dead-man's switch's
business, not this fix's.

## Compression

Both fixes are unambiguous and small (guard gate relocation ≤ ~15 lines;
workflow 2-line addition). Implemented on the branch in standalone commits:
- `fix(ci): give the mutation shards the fontconfig system dep` — the lane
  never reached mutation testing; the baseline workspace build failed on the
  missing `fontconfig.pc` (run 46043816740, job logs of all four matrix
  shards).
- `test(domain): gate the vocabulary guards per-test, not per-crate` — the
  crate-level `#![cfg_attr(miri, ignore)]` is invalid and was a no-op; the
  guards ran under Miri and panicked on `CARGO_MANIFEST_DIR: NotPresent`
  (run 46043812493, job 140657927173).

Local verification done: YAML parse; `cargo nextest run -p taskboard-domain
--test vocabulary_guards` 3/3 pass; `--cfg miri` compile-check of the gated
attribute placement.

Stage 3 (review + PR + gates) is NOT dispatched from here — dispatcher owns
it. Paste-ready stage-3 prompt:

> **Stage 3 — REVIEW (ci/scheduled-lanes-triage-2)**: Review the two commits
> on `ci/scheduled-lanes-triage-2` against
> `.artifacts/plans/2026-11-27-scheduled-lanes-triage-2-plan.md`. Discipline
> (binding — docs/agent_dispatch.org): a subagent is killed after ~600s
> without observable activity. (1) *The coverage trio never runs inside a
> subagent*: your gate is the pre-commit hooks (automatic) plus
> `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe;
> background + poll with useful work between polls if needed); the dispatcher
> runs the full AGENTS §10 suite before ship. *Flock-deferral rule: when the
> gate lock is contended and no other useful work remains, never block —
> commit all landed work, report "gate deferred: lock held by <holder>", and
> end the stage; the dispatcher runs the gate.* (2) *Your scope ends at
> PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks
> --watch`, `mergeStateStatus CLEAN`, teardown, at-merge issue closures, and
> every `gh workflow run`. (3) *Any command expected over ~5 minutes runs
> backgrounded* with short interleaved polls and useful work between polls —
> commit all landed work BEFORE any wait. Large scratch under /var/tmp, never
> /tmp. (4) *Every agent-launched cargo invocation routes through
> `scripts/memory-gate.sh`* — nothing bare. (5) Stage prompts restate this
> discipline verbatim. Out-of-scope findings become `backlog:` issues (AGENTS
> §7; search first). Constraints: never widen a timeout to hide a hang, never
> weaken a test; the vocabulary-guard miri gate is the module-documented
> intent, enforced by the ordinary suite. Worktree:
> `../taskboard-WT-ci-scheduled-lanes-triage-2` (remove it yourself after
> PR-green per AGENTS §8 lifecycle).

## Operator rulings requested

None required: no timeouts moved, no lanes retired. The per-test miri gate
matches the guard file's own documented intent; if the operator prefers the
guards to actually *run* under Miri, the alternative is
`env!("CARGO_MANIFEST_DIR")` (compile-time, miri-safe) plus verifying Miri's
file-IO forwarding — left as a possible follow-up, not done here (risk
without a local miri verification run).

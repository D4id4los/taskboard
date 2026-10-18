# [Report] Chunk 11 stage-3 review — CI & guardrails (`ci/guardrails-bencher-deny`)

- **Date**: 2026-10-18
- **Verdict**: **PASS** — no findings; branch reviewed, gates re-run green,
  PR open and fully green, `mergeStateStatus: CLEAN`.
- **PR**: https://github.com/D4id4los/taskboard/pull/328
  (rebased onto `origin/main` at 76838a1 after the first run went green;
  second CI run equally green; never merged — the user merges on GitHub).

## 1. Functional review (dispatch prompt 11.3 focus points)

- **Lane filter is structural** (D1): the mechanism is the removal of
  `--all-targets` from the test job's nextest invocation — cargo's native
  target selection, not a filter list. A future `[[bench]]` target
  categorically cannot re-enter the lane; nothing to rot. Local proof in
  the worktree: default lane **1022** tests (zero `hot_path`,
  `bench_fixtures` present ×3); `--all-targets` lane **1028** — the delta
  is exactly the six `bench/hot_path` entries. `clippy --all-targets`
  retains bench compile smoke.
- **Bencher fails loudly** (D2): `continue-on-error` is gone from ci.yml
  entirely (grep-verified; the only remaining occurrence in the diff is
  the comment "never re-add continue-on-error here"; mutation-testing.yml
  keeps its by-design one on the sweep Run step, unchanged in spirit).
  CLI install is unconditional (`bencherdev/bencher@v0.6.13` + matching
  `version:` pin) so PRs exercise it. **Double run deduped**: on the
  main-push path exactly one `cargo bench` invocation remains — inside
  `bencher run` (the plain run carries
  `if: github.ref != 'refs/heads/main' || github.event_name != 'push'`,
  the negation of the tracking step's condition). One bench execution per
  event: PR → plain only; main push → Bencher-wrapped only.
- **Deny bans match boundary semantics** (D4): `deny-domain.toml` bans
  the async runtime (tokio/tokio-util/tokio-stream), the net stack
  (reqwest/hyper/hyper-util/tonic), the UI face (slint/slint-build/
  i-slint-core), persistence (sqlx/rusqlite/libsqlite3-sys), `arc-swap`,
  and all five sibling members — the pure-layering-root yardstick of
  AGENTS §2. `deny-ui-slint.toml` bans net/persistence/credential IO
  (keyring, rpassword); tokio deliberately unbanned (mandated channel
  fabric, AGENTS §3). Both scoped with `[graph] exclude-dev = true`.
- **forbid-impossible caveat respected** (D5): ui-slint gets
  `#![deny(unsafe_code)]` — deny, not forbid — and the rewritten NOTE
  records exactly why (generated `allow(unsafe_code)` scopes, E0453 ×36,
  verified-clean provenance) plus that it remains the only crate without
  `forbid`.
- **Mutant upload** (D3): Collect/Upload address `mutants.out/` — the
  path cargo-mutants 27.1.0 actually writes (the old `mutants-out/`
  never existed); upload carries `if-no-files-found: error` (silent
  no-artifact runs can no longer read green); `cargo-mutants@27.1.0`
  pinned as the artifact-name contract; `.gitignore` corrected. The
  scope decision is recorded, and the deferred app one-off exists as
  issue #327 (created by stage 2, referenced in the PR body).
- **Plan conformance**: diff confined to the plan §5 file set; no
  release.yml/miri/coverage-lane edits; commit messages standalone with
  issue references and decision IDs.

## 2. Code-quality mandate

Comments in the workflows state constraints and history-backed reasons
(pin rationale, adapter behavior, fixed output-dir name), not narration.
Naming, commit hygiene, and config minimality (boundary tomls carry
`[graph]`+`[bans]` only, with the `check bans`-only rationale recorded
in-file) all conform. No dead code, no scope creep.

## 3. Gates re-run (stage 3, worktree, cargo-deny 0.20.2 = CI pin)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | green |
| `cargo clippy --workspace --all-targets -- -D warnings` | green |
| `cargo nextest run --workspace --no-tests=warn` | 1022 passed, 28 skipped |
| `cargo test --doc --workspace` | green |
| `cargo deny check` (root) | green (advisories/bans/licenses/sources ok) |
| boundary `check bans` ×2 | both green |
| coverage trio + profraw quarantine | 1022 passed; 2453 profraws valid; report green |
| workflow YAML syntax (both files) | parse ok |

## 4. CI acceptance evidence (the run is the test bed)

Run 39113096083 (post-rebase), job "Performance Benchmarking" — pass,
10m11s. Step log excerpt (job 117633234860):

```
Install Bencher CLI    Downloading Bencher CLI v0.6.13 from: https://github.com/bencherdev/bencher/releases/download/v0.6.13/bencher-v0.6.13-linux-x86-64
Install Bencher CLI    Bencher CLI v0.6.13 installed!
Verify Bencher CLI     bencher 0.6.13
Run Criterion Benchmarks    Run cargo bench --workspace
```

All ten checks pass (Test Execution, Code Coverage, Code Quality,
Security & License Audit — which now includes the two new boundary
steps — MSRV, benchmarking, SonarCloud ×2, codecov, MCP smoke).
`gh pr view 328` → `state=OPEN / mergeable=MERGEABLE /
mergeStateStatus=CLEAN` (after rebasing onto `origin/main`; the first
run had reported BEHIND).

## 5. Post-merge follow-up (recorded, not blocking)

R1 of the plan: `bencher run` is main-push-only by design, so the first
post-merge main push is the live upload evidence. A server-side
prerequisite failure there is the designed loud signal — remediate on
Bencher Cloud, never re-add `continue-on-error`. The first post-merge
weekly mutation run (after Mon 2026-10-19 04:00 UTC) is the
artifact-bearing sweep for #255's guardrail half.

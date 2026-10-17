# [Plan] CI & Guardrails — bench lane, Bencher wiring, mutant upload, deny boundaries

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for remediation-wave-1 dispatch chunk 11
  (branch `ci/guardrails-bencher-deny`, three-stage protocol). CI/config
  work validated by lane-level proofs and local tool runs — no new
  product tests; the CI runs themselves are the test bed (dispatch
  prompt 11.3).
- **Origin** (normative): findings report rows F13-07, F14-03(+F1-04),
  F13-08, F14-02(+F8-11/F1-02/F1-03/F7-09 fold), F7-01, F7-02; agenda
  rows P1-8, J-3, B-5, O-1; agenda-outcomes standing row (a) — as
  materialized in issues **#296, #255, #251, #252**. Owning reports:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md`
  (§5 option (b), §6 F13-07/F13-08),
  `...-theme-test-quality-report.md` (§3.5),
  `...-crate-ui-slint-report.md` (§2.3, §3.1–3.3). Dispatch plan §1/§12.
  Session ruling row (a): **wire Bencher now** — the exit-127 root cause
  is verified (CLI never installed), the token is present, the stale
  TODO's conditionality is void (criterion benches exist).
- **Status**: READY — every decision settled in §4; deviations from
  origin texts are recorded with reasons (D2 pin, D3 flag correction +
  app-one-off deferral, D4 minimal configs, D5 SPDX precedence).

## 1. Scope & acceptance

Four concerns, one branch (dispatch §12):

1. **Bench lane** (#296, F13-07/F14-03): the default
   `cargo nextest run --workspace` CI lane stops executing bench
   harness targets; `tests/bench_fixtures.rs` stays; the benchmark job
   remains the every-PR bench execution.
2. **Bencher graduation** (#296, F13-08, row (a)): install the CLI, drop
   `continue-on-error`, de-duplicate the double `cargo bench` per main
   push; failures loud.
3. **Mutant sweep** (#255, F14-02 fold): upload actually uploads and
   fails the job on empty; the sweep-scope decision recorded (weekly
   run as evidence source; app one-off called).
4. **Deny guardrails** (#251/#252, F7-01/F7-02): `#![deny(unsafe_code)]`
   in ui-slint + NOTE edit; two per-member cargo-deny boundary configs
   + two CI steps.

**Acceptance** (dispatch §12 verbatim intent): default nextest lane no
longer executes bench harness targets (`bench_fixtures.rs` kept);
benchmark job stays the every-PR bench execution; Bencher uploads on
main pushes with the CLI installed and failures loud (step log names
the CLI, not exit 127); mutant upload fixed per the issue; deny configs
enforce domain purity + ui-slint no-unsafe with the two CI steps wired;
all AGENTS §10 gates green.

**Non-goals**: no bench tuning, no new benches, no S1–S6 perf sketches
(their own findings); no workflow restructuring beyond the named steps
(release.yml is chunk 14's; miri.yml, the coverage and MSRV lanes, and
job topology untouched); no ui-slint mutant sweep (deferred by #255's
own decision); no app sweep *execution* (D3: deferred to a successor
issue); no allow-only strict deny variant (report §3.4 rejects it);
no Bencher PR-commentary wiring (`--github-actions`); no AGENTS.md edit
(§10's `cargo nextest run --workspace` already matches the post-change
CI lane — the chunk *removes* the doc/CI divergence rather than adding
one).

## 2. Seam inventory (verified at 53b533c = main)

- `.github/workflows/ci.yml`
  - test job lane `:164-166`:
    `cargo nextest run --workspace --all-targets --no-tests=warn` —
    `--all-targets` is what admits bench targets into the lane.
  - benchmark job `:279-315`: ungated plain `cargo bench --workspace`
    at `:296-301` (with a stale "no-op while no [[bench]] targets
    exist" comment at `:297-300`); Bencher step `:303-315` gated
    main-push-only (`:305`) with `continue-on-error: true` (`:306`,
    stale TODO) and **no CLI install anywhere in the job** — the
    verified exit-127 root cause.
  - deny job `:122-137`: `taiki-e/install-action` `tool: cargo-deny`
    **unversioned** (`:133-135`), then `cargo deny check` (`:137`).
  - check job clippy `:57`: `--all-targets` — keeps benches and the
    example compiling on every PR (the retained rot-protection).
  - coverage lane `:236`: `cargo llvm-cov nextest --all-features
    --workspace` — already the default target set (benches excluded):
    precedent for D1.
- `.github/workflows/mutation-testing.yml`: installs
  `tool: cargo-mutants,nextest` unversioned (`:37-39`); Run step
  `:41-48` (`continue-on-error: true` — by design, survivors are a
  report outcome); Collect `:50-58` cats
  `mutants-out/mutants-out.log` and probes `survivors.txt` — **both
  wrong names**; Upload `:60-66` takes `path: mutants-out/` with the
  default `if-no-files-found: warn` — green-on-empty (F1-03).
- `deny.toml` (root): `[bans] deny = []` at `:94`; the root check is
  untouched by this chunk.
- `.config/nextest.toml`: docker-tier test-group serialization only —
  untouched.
- Bench declaration: exactly one `[[bench]]` — `hot_path`,
  `harness = false`, `crates/taskboard-storage-sqlite/Cargo.toml:29-31`;
  fixture builder `benches/support/mod.rs` is contract-pinned by
  `tests/bench_fixtures.rs` (3 tests, `#[path]` include) — a regular
  integration test, **not** a bench target.
- `crates/taskboard-ui-slint/src/lib.rs`: SPDX `:1`, no crate-level
  lint attribute, NOTE `:9-12` carries the stale forbid rationale.
- `.gitignore:9`: `mutants-out*/` — ignores a path the tool never
  produces.
- One auto-discovered example (`crates/taskboard-sync-nextcloud/
  examples/harvest_deck_fixtures.rs`): `fn main` only, **no tests**
  (verified) — leaving the test lane's build set costs no test
  coverage; its compile smoke stays via clippy `--all-targets`.
- No `.bencher/` config exists; the invocation flags carry everything.

## 3. Measured facts (this machine 2026-10-18; nextest 0.9.148,
cargo-deny 0.20.2, cargo-mutants 27.1.0 — all equal to or pinning the
CI-relevant versions)

- `cargo nextest list --workspace` (default target set): **1022**
  entries; `hot_path` **0**; `bench_fixtures` **3**.
- `cargo nextest list --workspace --all-targets`: **1028** — the delta
  is **exactly** the six `taskboard-storage-sqlite::bench/hot_path
  {apply,load}/{100,1000,10000}` entries; `bench_fixtures` still 3.
- `--all-targets -E 'not(kind(bench))'` ≡ default lane (diff empty) —
  a verified-equivalent fallback, not needed for D1.
- cargo-mutants 27.1.0 CLI (from `--help` + a probe run):
  `-d, --dir` is the **input** crate directory (a probe with
  `-d /tmp/x` made it look for Cargo.toml *there*); `-o, --output`
  means "create `mutants.out` within this directory" — the output dir
  **name is fixed** (`mutants.out`, env `CARGO_MUTANTS_OUTPUT`).
  Consequence: F1-03's prescription "pin `-d mutants-out`"
  misidentifies the flag; the correct fix points Collect/Upload at
  `mutants.out/` (D3). Files observed inside real 27.1.0 output dirs
  (baseline report §mutants): `outcomes.json`, `missed.txt`, plus a
  log; exact log filename is not cheaply re-verifiable without a full
  sweep → the Collect step globs (D3).
- `taiki-e/install-action` TOOLS.md: `cargo-deny` and `cargo-mutants`
  supported with `@version` pins; **`bencher` is not in the list**.
- `bencherdev/bencher` GitHub Action: install-only (no run-wrapping
  inputs beyond `version`); releases attach prebuilt
  `bencher-vX-linux-x86-64` binaries; latest **v0.6.13**. Vendor
  guidance: pin exact versions for self-hosted; on Bencher Cloud
  occasional server-side breaking changes mean exact pins may need
  deliberate bumps.

## 4. Decisions

### D1 — lane filter mechanism: drop `--all-targets` from the test job

New test-lane step text:

```yaml
      - name: Run Tests
        # Default target set (lib+bins+tests): bench targets are the
        # benchmark job's lane (every PR); clippy --all-targets keeps
        # them compiling here. tests/bench_fixtures.rs stays — it is a
        # regular integration test, not a bench target.
        run: cargo nextest run --workspace --no-tests=warn
```

- **Why this mechanism**: native cargo/nextest target selection
  categorically excludes bench (and example) targets — a future
  `[[bench]]` cannot silently re-enter the lane; there is no name list
  to rot (the structural criterion of prompt 11.3). It also makes the
  CI lane byte-identical to AGENTS §10's `cargo nextest run
  --workspace` and to the coverage lane's existing shape.
- **Proof protocol (stage 2, in the worktree)**: run both list commands
  from §3; acceptance = default lane shows `hot_path` absent and
  `bench_fixtures` present (3), `--all-targets` shows exactly +6
  entries, all six being `bench/hot_path`. Cite outputs in the report.
- **Rejected alternatives**: (a) keep `--all-targets` +
  `-E 'not(kind(bench))'` — verified equivalent, but keeps a flag pair
  whose meaning a future maintainer must decode to explain "why are
  benches filtered"; (b) per-target `--exclude` — a hardcoded list
  that rots on the next bench; (c) a committed nextest default-filter
  config — config surface duplicating what target selection does
  natively, and it would change local bare-lane semantics the same way
  while being less discoverable.
- **Side effect, accepted**: the example leaves the test lane's build
  set (it has no tests; clippy `--all-targets` retains its compile
  smoke). Doc-tests lane untouched.

### D2 — Bencher graduation (row (a): wire now)

Benchmark job after the change (only this job's steps after
`Swatinem/rust-cache` are shown; nothing else in ci.yml's benchmark
job changes):

```yaml
      - name: Install Bencher CLI
        # Pinned deliberately (house pin policy, cf. the coverage lane):
        # the CLI's flags and API are this workflow's contract, and an
        # unpinned tool once broke a workflow silently (F1-03 class).
        # Bencher Cloud occasionally ships server changes needing a
        # newer CLI — a skew fails this step loudly, which is the
        # designed posture; bump on purpose.
        uses: bencherdev/bencher@v0.6.13
        with:
          version: 0.6.13

      - name: Verify Bencher CLI
        # PR-visible proof the CLI exists on PATH — the exact failure
        # (command not found, exit 127, swallowed) this chunk fixes.
        run: bencher --version

      - name: Run Criterion Benchmarks
        # Every non-main-push event (PRs, manual dispatch). Benches run
        # exactly once per event: on main pushes they run inside
        # `bencher run` below instead (the former double run).
        if: github.ref != 'refs/heads/main' || github.event_name != 'push'
        run: cargo bench --workspace

      - name: Track Benchmarks with Bencher
        # Record trend data from main pushes only; Bencher needs the API
        # token. The rust_criterion adapter parses criterion's stdout,
        # so the suite runs inside bencher run (--output-format bencher
        # is a nightly-only libtest flag and unneeded on stable).
        if: github.ref == 'refs/heads/main' && github.event_name == 'push'
        env:
          BENCHER_API_TOKEN: ${{ secrets.BENCHER_API_TOKEN }}
        run: |
          bencher run \
            --project taskboard \
            --branch main \
            --testbed ubuntu-latest \
            --adapter rust_criterion \
            "cargo bench --workspace"
```

- **Pinning strategy** (the prompt's open question): action ref
  `@v0.6.13` + `version: 0.6.13`. Rationale: the F1-03 lesson (a
  floating tool broke a workflow silently) and the coverage-lane
  precedent outweigh the vendor's Cloud-float advice; the recorded
  cost is a deliberate bump when Cloud drifts — and post-graduation a
  loud failure is exactly the designed signal (never re-add
  `continue-on-error` to mute it).
- **Dedupe ruling** (the prompt's "which invocation remains on main
  pushes"): the **Bencher-wrapped** invocation remains on main pushes
  (the adapter needs the child's stdout); the plain `cargo bench`
  moves to the negated condition. Event accounting: PR → plain only;
  main push → bencher-wrapped only; `workflow_dispatch` on main →
  plain (tracking records pushes only). One bench execution per event.
- **`continue-on-error` dropped**; stale TODO retired; the stale
  "no-op while no [[bench]] targets exist" comment rewritten (benches
  exist since 2026-10-15).
- **Install is unconditional** so PRs exercise the install path too —
  the previous wiring hid the breakage on PRs entirely (step was
  main-only). The `bencher --version` step gives prompt 11.3 its
  PR-visible CLI evidence; the full `bencher run` is necessarily
  post-merge evidence (main-push-only by design — record in the stage-3
  report; see risk R1).

### D3 — mutant-sweep scope decision (issue #255's decision frame)

1. **Upload fix — in scope, now.** Root cause corrected against the
   real CLI (§3): cargo-mutants always writes `mutants.out/` in the
   workspace root; the workflow addresses `mutants-out/`, a path the
   tool never creates. Fix:
   - Run step: **unchanged invocation** (no `-d`; `-o` would still
     create `<parent>/mutants.out`, so pinning the parent adds
     nothing) — `continue-on-error: true` **stays by design**
     (survivors are a report outcome, not an infra failure; recorded
     residual R6).
   - Collect step: point at `mutants.out/`; cat the log via
     `mutants.out/*.log || true` (exact log filename not re-verifiable
     without a full sweep — the glob is tolerant; missed-report from
     `mutants.out/missed.txt`, the name the baseline report's real runs
     produced; the current `survivors.txt` probe is dead code).
   - Upload step: `path: mutants.out/` + `if-no-files-found: error` —
     a silent no-artifact run can never again read as green.
   - Pin the install: `tool: cargo-mutants@27.1.0,nextest` — the
     pinned mutants version is the artifact-name contract (and makes
     the weekly evidence reproducible); `nextest` stays floating as
     today (no artifact contract in this workflow; recorded).
   - `.gitignore`: replace `mutants-out*/` with `mutants.out/` so
     local sweeps don't dirty trees.
2. **Weekly run as the evidence source — yes.** The weekly job already
   sweeps domain+state; post-reminders coverage comes free (F1-02's
   staleness disposes itself on the next run). Timing: the 2026-10-19
   04:00 UTC run will likely execute **before** this PR merges — that
   run still uploads nothing and its evidence remains console-log
   recoverable exactly as the audit did (`gh run view <id> --log`;
   procedure recorded here). The **first weekly run on post-merge
   main** is the artifact-bearing sweep and the acceptance evidence
   for this half of #255.
3. **Capped app one-off (commands/ + ipc/) — DEFERRED, not executed
   here.** Reasons: it is a multi-hour local measurement campaign over
   the app crate's subprocess-heavy, wall-clock-polling test tiers
   (the sync one-off box: 120 min for 281 mutants; app's cli tiers are
   strictly worse per mutant), orthogonal to this chunk's CI-guardrail
   mechanics; wave 1 contains no app-test-hardening chunk to consume
   its survivor list, so the evidence would age before use — the very
   inversion F14-02 names; the natural owner is the future app-test
   chunk, which should run it fresh at its start (chunk-2 precedent).
   **Bookkeeping**: stage 2 creates the successor issue per AGENTS §7 —
   title `backlog: capped cargo-mutants one-off for taskboard-app
   (commands/ + ipc/ first)`, labels `testing`, `ci`, `p2`, body =
   F8-11 churn facts + the box-style command template from the
   baseline report §mutants + a permalink to this plan; **no
   milestone** (unscheduled). The PR body says `Closes #255` **and**
   names the successor issue so the reviewer sees the split. #255
   still closes here: it tracks the *scope decision*, which this chunk
   makes in full (upload fixed; weekly source chosen; ui deferral
   reaffirmed; app one-off consciously deferred with an owner), and
   its guardrail half lands.
4. **ui-slint sweep (F7-09)**: deferral reaffirmed as already decided
   (post-kiosk-wave) — restated in the successor issue's body as
   context, no new tracking needed (it rides #255's closure text).

### D4 — cargo-deny boundary configs (issue #252; report §3.2 tables verbatim)

Two new files, `.config/deny/deny-domain.toml` and
`.config/deny/deny-ui-slint.toml`. Both start:

```toml
[graph]
exclude-dev = true   # dev-deps stay unjudged; bans target shipped edges
```

`deny-domain.toml` `[bans] deny` (yardstick AGENTS §2 — zero
async/net/UI/persistence; layering root):

| Banned | Why |
|---|---|
| `tokio`, `tokio-util`, `tokio-stream` | async runtime must not enter the pure domain |
| `reqwest`, `hyper`, `hyper-util`, `tonic` | net stack |
| `slint`, `slint-build`, `i-slint-core` | UI framework incl. codegen face |
| `sqlx`, `rusqlite`, `libsqlite3-sys` | persistence |
| `arc-swap` | `ArcSwap<AppState>` is state-engine-owned (AGENTS §2/§3) |
| `taskboard-state`, `taskboard-storage-sqlite`, `taskboard-sync-nextcloud`, `taskboard-ui-slint`, `taskboard-app` | sibling members — domain depends on no other member (the report's optional row: **included**, zero probe cost, pins the workspace layering root) |

`deny-ui-slint.toml` `[bans] deny` (yardstick AGENTS §2 — no IO crates,
presentation only):

| Banned | Why |
|---|---|
| `reqwest`, `hyper`, `hyper-util`, `tonic` | net stack |
| `sqlx`, `rusqlite`, `libsqlite3-sys` | persistence |
| `keyring`, `rpassword` | credential IO is app/actor territory |

Never banned, deliberately: `proptest` (domain's feature-gated normal
dep — visible in the graph even under `exclude-dev`; banning it was
the report's explicit warning) and `tokio` in ui-slint (the mandated
channel fabric, AGENTS §3).

**Recorded deviation from report §3.2's parenthetical**: the configs
are minimal — `[graph]` + `[bans]` only — and do **not** copy the root
license/advisory sections. Reason: the CI steps run `check bans`
exclusively; duplicating the license allow-list into files that never
execute a license check is pure three-way drift surface. The canonical
full check remains root `cargo deny check` (AGENTS §10). Anyone running
a boundary config without a subcommand gets default-policy noise —
acceptable for a CI-scoped single-purpose config.

**CI wiring** (two steps appended to the existing `deny` job, after
`- run: cargo deny check`, commands verbatim from report §3.3):

```yaml
      - run: cargo deny --config .config/deny/deny-domain.toml --manifest-path crates/taskboard-domain/Cargo.toml check bans
      - run: cargo deny --config .config/deny/deny-ui-slint.toml --manifest-path crates/taskboard-ui-slint/Cargo.toml check bans
```

**Tool pin**: change the deny job's install to
`tool: cargo-deny@0.20.2` with a comment (probes verified on 0.20.2;
tested minimum ≥ 0.20 per the issue; deliberate-pin policy). The local
cargo-deny is exactly 0.20.2 — local validation is version-identical
to CI.

**Local validation before any push (stage 2 AND stage 3, mandatory)**:
`cargo deny check` (root) + both boundary `check bans` runs, all green
in the worktree. This is the false-positive gate the prompt demands —
the probe history (report §3.1: own member ok / offender fails /
siblings ok on 0.20.2) says today's manifests pass; if a local run
disagrees, that is a finding to resolve before shipping, never a
config to quietly weaken.

### D5 — ui-slint unsafe deny (issue #251)

`crates/taskboard-ui-slint/src/lib.rs`: insert
`#![deny(unsafe_code)]` as line 2 — the SPDX header stays line 1
(AGENTS §6 header rule outranks the issue's literal "first line";
recorded deviation). NOTE (`:9-12`) rewritten to:

> NOTE: `deny`, not `forbid`: the Slint-generated code carries internal
> `allow(unsafe_code)` scopes that a `forbid` would overrule (E0453 ×36,
> audit chunk 7 §2.3). The crate-level deny coexists with those internal
> allows and fires exactly on hand-written `unsafe` — a future
> hand-written block fails the build unless a local, grep-visible
> `#[allow(unsafe_code)]` is added at the site. This remains the only
> workspace crate without `#![forbid(unsafe_code)]`.

Validation: `cargo clippy --workspace --all-targets -- -D warnings`
(the audit's deny experiment was clean) +
`cargo nextest run -p taskboard-ui-slint`.

## 5. Change list & commit layout (each commit standalone)

| # | Commit (`<type>(<module>): …`) | Files |
|---|---|---|
| 1 | `ci(test): keep bench targets out of the default nextest lane` | `.github/workflows/ci.yml` (test job step + comment) |
| 2 | `ci(benchmark): install the bencher cli and make tracking loud` | `.github/workflows/ci.yml` (benchmark job: install + verify steps, dedupe condition, drop continue-on-error, comment rewrites) |
| 3 | `ci(mutants): upload the real cargo-mutants output and fail on empty` | `.github/workflows/mutation-testing.yml`, `.gitignore` |
| 4 | `fix(ui-slint): deny hand-written unsafe code` | `crates/taskboard-ui-slint/src/lib.rs` |
| 5 | `ci(deny): enforce domain purity and dumb-ui boundaries` | `.config/deny/deny-domain.toml` (new), `.config/deny/deny-ui-slint.toml` (new), `.github/workflows/ci.yml` (deny job: pin + two steps) |

Commit bodies cite the issue IDs and the decision IDs of this plan.
Stage 2 also creates the successor issue (D3.3) — not a commit, GitHub
bookkeeping per AGENTS §7. Stage 3's PR body: summary + `Closes #296`,
`Closes #255`, `Closes #251`, `Closes #252` + the successor-issue
reference from D3. No `release.yml` edits (chunk 14 owns it; dispatch
§1).

## 6. Validation protocol

- **Stage 2 (worktree)**:
  1. Lane proofs (D1): both `cargo nextest list` runs; record the
     counts/asserts from §3.
  2. Deny: root `cargo deny check` + both boundary checks (D4) — green.
  3. `cargo clippy --workspace --all-targets -- -D warnings` green with
     the new deny attribute (D5); `cargo nextest run -p
     taskboard-ui-slint` green.
  4. Full AGENTS §10 gate suite (fmt, clippy, nextest, doc tests, deny,
     coverage trio) green.
  5. `git diff main...HEAD` confined to the five files of §5 (+ two new
     configs); YAML edits pass a syntax check (`ruby -ryaml -e ...` or
     `python3 -c 'import yaml,...'` — or the actionlint binary if
     available; at minimum `python3 -c "import yaml,sys;
     yaml.safe_load(open(sys.argv[1]))" <file>` per workflow).
- **Stage 3 (review + ship)**: re-run 1–5; verify the diff against
  this plan's §4 texts (no scope creep, no restructuring); push, open
  the PR, `gh pr checks --watch` to green; capture the PR-run
  "Performance Benchmarking" log lines showing the install + `bencher
  --version` steps. Post-merge follow-up (recorded in the stage-3
  report, not blocking): the first main-push run is the live Bencher
  acceptance — capture its step log (real CLI output, upload attempt,
  any server-side prerequisite it surfaces).
- **Never**: mute a failing Bencher or deny step to get green; weaken
  a config to pass; edit `release.yml`/`miri.yml`/coverage lanes.

## 7. Risks

- **R1 — Bencher server-side prerequisites are unvalidated** (project
  slug `taskboard`, branch `main`, testbed `ubuntu-latest`, token
  scope): the step has never reached the network. The loud failure on
  the first post-merge main push is **by design** (graduation ruling);
  remediation is server-side (create the project/testbed on Bencher
  Cloud, or fix the slug) — never re-adding `continue-on-error`. The
  PR itself cannot exercise `bencher run` (main-push-only); that
  evidence gap is explicit in §6.
- **R2 — pinned CLI vs Bencher Cloud drift**: vendor says Cloud
  occasionally needs newer CLIs; the pin turns drift into a loud,
  deliberate bump (comment in the workflow states this). Accepted per
  D2 rationale.
- **R3 — cargo-deny false positives**: probe-verified tables + the
  mandatory **local** deny runs before any push (§6). A local failure
  is a finding to diagnose, not a ban to delete.
- **R4 — weekly-run timing**: if chunk 11 merges after Mon 2026-10-19
  04:00 UTC, that run still uploads nothing; console-log recovery
  procedure recorded (D3.2). No action possible from this branch.
- **R5 — mutants artifact-name drift**: the log filename inside
  `mutants.out/` is globbed (`*.log || true`), so Collect degrades
  gracefully; the authoritative guarantee is the dir-level upload with
  `if-no-files-found: error`. The pinned `cargo-mutants@27.1.0` makes
  the contract stable; a future bump re-checks the names (comment in
  the workflow).
- **R6 — residual green-on-crash (mutants)**: a hard cargo-mutants
  crash that already wrote a partial log still reads green (Run's
  `continue-on-error` is by design for survivors and cannot
  distinguish). Diagnosable from the artifact; fixing it means an
  exit-code contract — out of scope (restructuring), recorded here.
- **R7 — `--manifest-path` as a cargo-deny global arg**: the exact
  command shape is probe-verified on 0.20.2 (report §3.1); local
  validation re-proves it on the same version CI will pin.
- **R8 — lane semantics**: `harness = false` bench enumeration under
  nextest `--all-targets` is empirically pinned by §3's measurements
  (six entries with `--all-targets`, zero without); stage 2 re-proves
  in the worktree.

## 8. Stage-2/stage-3 reminders (binding)

- Deviations from this plan are recorded, not improvised; a gate that
  cannot go green is reported, never bypassed.
- The successor issue (D3.3) is created by stage 2 and linked in the
  PR body by stage 3.
- Evidence to capture for the report: nextest lane listings, the three
  deny outputs, the PR's benchmark-job log (install + version), and —
  post-merge — the first main-push Bencher step log.

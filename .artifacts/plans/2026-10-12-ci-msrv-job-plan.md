# [Plan] MSRV CI Job & Float Policy

- **Date**: 2026-10-12
- **Kind**: stage-1 implementation plan (WI-B of the 2026-10-12 backlog
  batch) — CI + docs only, no Rust source changes.
- **Target**: `.github/workflows/ci.yml`,
  `docs/adr/0003-toolchain-edition-and-quality-gates.org`,
  `.artifacts/taskboard-potential-improvements-backlog.md` (final commit).
- **Branch / worktree**: `ci/msrv-job` in `../taskboard-WT-ci-msrv-job`,
  off `main` @ `3ca11ea`.
- **Goal**: make the declared MSRV (`rust-version = "1.96"`,
  `Cargo.toml:15`) an enforced contract instead of a claim: a CI job
  compiles the workspace on exactly that toolchain, and ADR 0003 records
  when and how the number may move.
- **Status**: DRAFT — awaiting user review; stage 2 implements only after
  approval.

---

## 1. Context & Findings (verified 2026-10-12 against `main` @ `3ca11ea`)

1. **The MSRV is declared once and inherited everywhere.**
   `[workspace.package] rust-version = "1.96"` (root `Cargo.toml:15`);
   all six crates say `rust-version.workspace = true`. Nothing verifies
   the claim: an accidental post-1.96 stdlib/stabilization use compiles
   green on CI (all quality jobs ride `dtolnay/rust-toolchain@stable`)
   and fails only on a user's older toolchain.
2. **Local dev floats; the floor doesn't.** `rust-toolchain.toml` pins
   `channel = "stable"` (+ rustfmt/clippy). So the gap between "what CI
   compiles" and "what we promise" grows every six weeks. Note: 1.96 was
   *current* stable when ADR 0003 was written (2026-10-07); the job is
   near-tautological with the stable jobs **today** and becomes a real
   tripwire the moment stable advances past 1.96.
3. **`ci.yml` shape** (all verified in
   `.github/workflows/ci.yml`): workflow-level triggers `push:
   branches: ["main"]` + `pull_request: branches: ["main"]`; one
   workflow-level `concurrency` group with `cancel-in-progress: true`;
   six jobs (`check`, `deny`, `test`, `coverage`, `sonarcloud`,
   `benchmark`). Every compile job installs
   `libdbus-1-dev pkg-config` (keyring → dbus-secret-service →
   `libdbus-sys` probes pkg-config in its build script even under
   `cargo check`) and sets `SQLX_OFFLINE: "true"` (sqlx `query!` macros
   expand during check). No existing job passes `--locked`; the
   `coverage` job already builds `--all-features`.
4. **`dtolnay/rust-toolchain` mechanics** (verified against the action's
   repo today):
   - Branch ref `@1.96` **exists** and installs Rust 1.96.x — its
     `action.yml` hardcodes the toolchain and is rebased per patch
     release (currently `1.96.1`). So the backlog's
     `dtolnay/rust-toolchain@1.96` shorthand is literally usable.
   - README: *"When passing an explicit `toolchain` as an input instead
     of @rev, you'll want to use `dtolnay/rust-toolchain@master`"*.
     A `toolchain: 1.96` input resolves to the newest 1.96.x patch via
     the action's own parse step.
   - The action's final step always prints
     `rustc --version --verbose` — a free audit line proving in the job
     log which compiler actually ran.
5. **Features exist**: the workspace's only features are
   `taskboard-app/tokio-console` (pulls `console-subscriber`/tonic; its
   port binding is runtime-gated per the manifest comment) and
   `taskboard-domain/test-support` (proptest). So `--all-features` is
   meaningful, not a no-op.
6. **No actionlint** anywhere in the repo (no references in
   `.github/`, `scripts/`, no pre-commit config) — YAML validity is
   proven by the PR run itself, not a local linter.
7. **ADR 0003** (`docs/adr/0003-...org`, Accepted 2026-10-07) already
   owns the toolchain decisions and declares "Edition 2024, MSRV
   =1.96=". WI-C owns `miri.yml`; this item must not touch it.

## 2. Design Decisions

| # | Decision | Choice & rationale |
|---|---|---|
| D1 | Toolchain selection | **Read the MSRV from the root `Cargo.toml` at job runtime** (grep step → `dtolnay/rust-toolchain@master` with `toolchain:` input). Single source of truth — the manifest is what users' cargo enforces, so it is what CI must test. A static `@1.96` ref would drift silently if the manifest MSRV were ever *lowered* (job keeps testing 1.96 while users are promised less); the dynamic form cannot drift in either direction. The backlog's `@1.96` shorthand is recorded as correct-but-static; we take the strictly safer variant at the cost of a 3-line step. |
| D2 | Command & flags | `cargo check --workspace --all-features --locked`. `check` suffices (MSRV violations are front-end errors; no codegen time wasted). `--workspace`: all six crates. `--all-features`: `tokio-console` and `test-support` are user-facing compile-time opt-ins — a feature build breaking on MSRV is an MSRV break (matches the `coverage` job's flag set). `--locked`: the job compiles the exact audited `Cargo.lock` tree, which is what an MSRV user gets; it also reds on a stale lockfile here rather than silently re-resolving deps mid-check. Flag set is exactly the dispatch plan's. |
| D3 | Not `--all-targets` | Dev-dependencies/tests are outside the MSRV promise for a pre-1.0 private workspace (users build the binaries; they don't run our test suite). Widening later is a one-flag change if dev-dep MSRV incidents ever appear. |
| D4 | No clippy/rustfmt on this job | Lint and fmt infra deliberately ride current stable (ADR 0003); MSRV job is check-only and requests no components. |
| D5 | cargo-deny does **not** ride along | cargo-deny is a prebuilt binary independent of the Rust toolchain; its existing `deny` job already runs on every PR. Running it under the MSRV job would verify nothing about the MSRV and duplicate work. |
| D6 | Trigger scope | Inherit the workflow-level triggers: **every PR to `main` and every push to `main`**, no path filters, no job-level `if`. MSRV breakage arrives via ordinary PRs (new stdlib API, dependency bump) so the job must be per-PR. Dependabot PRs are included deliberately — a dependency raising its own MSRV above 1.96 reds this job first; that is its highest-value catch. The workflow `concurrency` group already cancels superseded runs. |
| D7 | Policy home | **ADR 0003, as a dated amendment section** (not a new ADR, not `docs/architecture.org`). ADR 0003 owns toolchain/quality-gate decisions and already declares the MSRV; one policy paragraph does not merit an ADR of its own; architecture.org describes structure, not policy history. House precedent: WI-F amends ADR 0004 the same way. |
| D8 | Float policy content | **The MSRV does not float.** It moves only by an explicit bump of `[workspace.package] rust-version` in a dedicated, single-purpose commit that updates ADR 0003 in the same breath. It never tracks "stable minus N". `rust-toolchain.toml` stays on `stable` — dev machines and quality CI ride current stable; the MSRV job is the backstop keeping the declared floor honest. |
| D9 | Job id / name | id `msrv`, display name `MSRV Check`. No version in the name — the manifest owns the number; a "…(1.96)" display string would be cosmetic drift at the next bump. |
| D10 | Job placement & prerequisites | Placed directly after `check` in `ci.yml` (compile-gate family). Mirrors the existing compile jobs: `libdbus-1-dev` + `pkg-config` apt step (build scripts run under `cargo check`), `SQLX_OFFLINE: "true"`, `Swatinem/rust-cache@v2` (its cache key derives from the active rustc, so no cross-toolchain cache poisoning). |
| D11 | Required check | **Recommend the user add "MSRV Check" to the `protect-main` ruleset's required checks after the first green run.** That is a GitHub-UI action, not doable from the repo; until then the job is advisory. Flagged in the stage-3 report. |
| D12 | actionlint | The repo has no actionlint hook today (finding 6); the job's own PR run proves the YAML parses and the refs resolve. Adopting actionlint is a separate decision (see §8). |

## 3. The Job — exact YAML

Added to `.github/workflows/ci.yml` immediately after the `check` job:

```yaml
  msrv:
    name: MSRV Check
    runs-on: ubuntu-latest
    env:
      # Same offline-cache guarantee as the check job: the sqlx query!
      # macros expand during cargo check and must not need a live
      # DATABASE_URL.
      SQLX_OFFLINE: "true"
    steps:
      - uses: actions/checkout@v7

      - name: Install system deps (keyring sync-secret-service links libdbus-1)
        run: sudo apt-get update && sudo apt-get install -y libdbus-1-dev pkg-config

      - name: Read MSRV from the workspace manifest
        # Single source of truth: [workspace.package] rust-version. Fail
        # loudly on an empty parse rather than checking the wrong
        # toolchain silently.
        id: msrv
        run: |
          msrv=$(grep -m1 '^rust-version' Cargo.toml | sed 's/^rust-version *= *"\(.*\)"/\1/')
          test -n "$msrv"
          echo "version=$msrv" >> "$GITHUB_OUTPUT"

      # README: with an explicit toolchain input the action ref is @master.
      # '1.96' resolves to the newest 1.96.x patch release.
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ steps.msrv.outputs.version }}

      - uses: Swatinem/rust-cache@v2

      - name: Check on MSRV
        run: cargo check --workspace --all-features --locked
```

Notes for the implementer:

- Step order matters: apt → read-MSRV → toolchain → rust-cache (the
  cache key must be computed after the MSRV toolchain is default) →
  check. This mirrors the existing jobs' toolchain-then-cache order.
- The dtolnay action's own final step prints
  `rustc --version --verbose` — keep an eye on that line in the run
  log; it is the audit proof (must show `release: 1.96.x`).
- No components, no `timeout-minutes` (neither does any existing job),
  no changes to any other job. `miri.yml` stays untouched (WI-C's).

## 4. ADR 0003 Amendment — exact wording

Appended to `docs/adr/0003-toolchain-edition-and-quality-gates.org`
(after the existing *Consequences* section; the original 2026-10-07
text is history and stays verbatim):

```org
* Amendment (2026-10-12): MSRV verification & float policy

- The MSRV does not float. It moves only by an explicit bump of
  =[workspace.package] rust-version= in a dedicated, single-purpose
  commit that updates this ADR in the same change. It never tracks
  "stable minus N" or any derived channel.
- Acceptable reasons to bump: a dependency or language feature
  requires a newer compiler, or a target platform's support window
  forces it. Convenience alone is not a reason.
- Enforcement: the =MSRV Check= CI job (=.github/workflows/ci.yml=)
  compiles the workspace with the toolchain named by =rust-version=
  (=cargo check --workspace --all-features --locked=) on every PR and
  =main= push, and is intended as a required status check. The job
  reads the version from the root =Cargo.toml=, so the manifest is the
  single source of truth: this ADR records the *why*, the manifest
  records the *what*.
- Division of labor unchanged: =rust-toolchain.toml= stays on =stable=
  — development and the quality-gate CI jobs (fmt, clippy) ride the
  current stable compiler; the MSRV job is the backstop that keeps the
  declared floor honest as stable advances past it.
```

## 5. Validation — the job proves itself on its own PR

1. **Self-validation chain (acceptance path):** stage 3 pushes the
   branch and opens the PR → the `pull_request` trigger runs `ci.yml`
   from the PR merge ref → the new `MSRV Check` job appears among the
   checks and must be green → in the job log, the dtolnay action's
   built-in version step must show `release: 1.96.x` (proving the
   extraction step fed the manifest value into the toolchain install —
   a green job that silently ran stable would otherwise be
   indistinguishable). Green + version line = WI-B's "job is green on
   the PR's own CI run" acceptance criterion, met by construction.
2. **Local pre-checks (stage 2):** YAML sanity by inspection against
   the existing jobs (no actionlint in the repo — D12); optionally
   `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))"`
   if PyYAML is available; extraction step rehearsed locally with the
   same grep/sed pipeline against the real `Cargo.toml`. No Rust code
   changes → the §10 gate suite is unaffected but stage 2 still runs
   it (docs/CI-only diff must leave it green).
3. **Optional negative rehearsal (stage 3, best-effort, only if
   cheap):** on the branch, temporarily set
   `rust-version = "9.9"`, push, watch `MSRV Check` red at the rustup
   install step, then revert before finalizing the PR. This proves the
   plumbing reacts to the manifest. A *true* detection test (code using
   a post-1.96 API) cannot be written today: 1.96 is current stable, so
   nothing newer has stabilized yet — the guard's value accrues as
   stable advances (§1 finding 2). The rehearsal is optional because it
   costs a push cycle; the version-line audit in (1) is the required
   proof.
4. **Post-merge follow-through (user, not the repo):** add `MSRV Check`
   to the required checks in the `protect-main` ruleset (D11), and
   expect the first Dependabot dep bump that needs rustc > 1.96 to be
   caught by exactly this job.

## 6. Commit & bookkeeping plan (stage 2)

Two commits, each standalone (rebase-merge hygiene):

1. `ci(workflows): MSRV check job — compile the workspace on the declared rust-version`
   — the ci.yml job (§3) + the ADR 0003 amendment (§4). One logical
   unit: the job enforces the policy the amendment states; splitting
   them would leave one commit meaning the wrong thing.
2. `docs(backlog): mark MSRV policy and CI job done`
   — the final bookkeeping commit: the
   "[2026-10-07] MSRV policy and CI job" entry (backlog line 97) gets
   ✅ DONE plus a one-line note naming the job and the ADR amendment.

Estimated diff: `ci.yml` +~35 lines, ADR +~20 lines, backlog +~3 lines.
Size S, as triaged.

## 7. Risks

- **`@master` action ref floats** (action code can change under us).
  Accepted: the repo already pins no action by SHA anywhere
  (`checkout@v7`, `dtolnay/rust-toolchain@stable`, `rust-cache@v2` are
  all floating refs), so `@master` adds no new *class* of exposure;
  the `toolchain:` input requires it per the action's README.
  Repo-wide SHA pinning is §8 material, not this item.
- **Patch-train movement**: `toolchain: 1.96` resolves to the newest
  1.96.x patch, so the exact compiler inside the floor moves. This is
  intended (any patch of the floor must compile) and stays auditable
  via the version line in the log.
- **`--locked` can red the job** on a PR with a stale `Cargo.lock` —
  but every other cargo job would red equally at build time; this job
  just fails it earlier and for the right reason (the audited tree is
  part of the MSRV contract).
- **`--all-features` cost**: the console-subscriber/tonic tree adds
  ~1–2 min cold-compile. `rust-cache` absorbs most of it on repeat
  runs. No runtime hazard: the console port binding is runtime-gated
  and `cargo check` never runs the binary anyway.
- **Extraction fragility**: if the root manifest ever stops matching
  `^rust-version = "…"` (formatting, key moved), the `test -n` guard
  reds the job loudly instead of silently checking the wrong toolchain.
- **Advisory until D11 happens**: until the user adds the job to the
  required checks, a red MSRV Check does not block merge. Stage 3's
  report must call this out.
- **WI-C interference**: none expected — disjoint files (`ci.yml` vs
  `miri.yml`); land serially or accept a trivial rebase (dispatch §2).

## 8. Out of scope (backlog candidates, not this item)

- SHA-pinning all third-party actions (repo-wide supply-chain
  hardening).
- Adopting actionlint (local or CI) for workflow YAML.
- Widening the job to `--all-targets` (dev-dependency MSRV coverage).
- Any change to `rust-toolchain.toml` — it stays on `stable` by policy
  (§2 D8).

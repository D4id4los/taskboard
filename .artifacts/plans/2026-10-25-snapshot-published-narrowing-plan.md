# [Plan] Snapshot Published-Half Narrowing (backlog-triage chunk 10)

- **Date**: 2026-10-26 (file name kept per the dispatch prompt's normative
  path; written a day later).
- **Kind**: chunk plan — stage-1 planning record for chunk 10 of the
  backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §11).
  Test-infra only: `crates/taskboard-state/tests/snapshots.rs`, the
  snapshot files, one dev-dep line. **Zero production-code diffs**
  (dispatch non-goal).
- **Branch / worktree**: `test/snapshot-published-narrowing` /
  `../taskboard-WT-test-snapshot-published-narrowing` (doctor exit 0,
  91 G free, floor 25 G).
- **Base**: `88d0ad2` = fetched `origin/main` tip. Deviation from the
  literal "off main": local `main` (a1bc03d) trails origin; the commits
  between are the Kani pilot + UI-mutants CI work — verified not to
  touch the state test suite (`git diff --stat main..origin/main --
  crates/taskboard-state/tests/` is empty; root `Cargo.toml` +6 only),
  so every observation below holds verbatim at the base.
- **Normative origins**: issue #245 (category `testing`, `p2`);
  finding F4-03 of
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-state-report.md`
  (§6 sketch + §7.2); dispatch plan §11 (covers / non-goals /
  acceptance); AGENTS §5 (snapshot directive, "a new test failing can
  be a bug", no-weakening rule).
- **Recorded deviation from the issue numbers**: #245 and dispatch §11
  say "14 files → 7 dumps + 7 tails" (7 scenarios at audit day,
  2026-10-16). Today the suite has **19 scenarios** (the S30–S35
  capture/reminder scenarios landed after the audit) = **38 `.snap`
  files → 19 persisted dumps + 19 tails**. The per-scenario mechanics
  are identical; the dispatch §11 review prompt's "file count exactly
  14 → 14" applies as 38 → 38 under its "unless the plan recorded a
  deviation" clause — this section is that record.
- **Line numbers below are plan-day references** (base `88d0ad2`),
  never load-bearing.

## 0. What the re-grounding established (verified today, all 19 pairs)

The audit's diff claim was re-verified mechanically over **all 19
persisted/published pairs** (not just the audit's s1/s3 spot check),
by splitting each `.snap` into top-level YAML blocks and comparing
order-insensitively after dropping `outbox`, `validators`
(persisted-only) and `last_updated` (published-only):

- The shared families — `boards`, `stacks`, `tasks`, `labels`,
  `reminders`, `sync` — are **byte-identical in 19/19 pairs** except
  one block: `sync` differs in exactly 4 pairs (s3, s4, s6, s10), and
  there **only inside `counters`** (`cycles`/`pushes`/`pulls`/
  `failures`/`offline_episodes`/`last_cycle`). `phase`,
  `last_success`, `pending_ops`, `sidecar`, `features` never diverge.
- Residual pair differences, all accounted for below:
  the header (`expression: "&persisted"` vs `"&*app"`, plus a
  sometimes-present `assertion_line:`), the persisted-only `outbox` +
  `validators` sections, the published-only `last_updated`, and
  **section order** (persisted: `…outbox, validators, sync, reminders`;
  published: `…reminders, sync, last_updated`).

So the audit's "differ only in header and tail" holds, with the one
refinement that the published `sync`'s `counters` overlay is part of
what "tail" must mean — see §1/§2.

## 1. The equality seam, structurally

- `AppState` (`crates/taskboard-domain/src/state.rs:25–45`) =
  `boards, stacks, tasks, labels, reminders, sync, last_updated`.
  `PersistedState`
  (`crates/taskboard-domain/src/persistence.rs:82–102`) =
  `boards, stacks, tasks, labels, outbox, validators, sync, reminders`.
  The four entity maps and `reminders` are the **same types** on both
  sides (`PartialEq` throughout); `sync` is the same `SyncStates`
  newtype on both sides.
- The canonical mapping already exists:
  `From<PersistedState> for AppState`
  (`persistence.rs:104–116`, the boot hydration path) moves the six
  shared families 1:1 and sets `last_updated: None`.
  `EngineCore::from_persisted` (`taskboard-state/src/core.rs:34–51`)
  keeps the whole `PersistedState` as the engine's disk mirror and
  runs `derive_sync_states` on it "exactly like every repository
  `load()` runs it — memory ≡ disk by construction".
- The published view is therefore *defined* as: disk mirror +
  (a) the per-row memory-only `SyncStatus` overlays — `counters`
  (process-lifetime, `core.rs:29–33`: "never persisted, `app()`
  overlays them onto the published rows"), `sidecar` and `features`
  (both documented memory-only at `state.rs:61–72`) — and
  (b) `last_updated` (the live engine stamp).

**"Published equals persisted projection" means**: for the live
published `AppState` and the fake repository's `PersistedState` at
snapshot time,

1. `app.boards == persisted.boards`, likewise `stacks`, `tasks`,
   `labels`, `reminders` — full `PartialEq`;
2. `app.sync` vs `persisted.sync` — equal key sets and, per board,
   equal **stored projection** `(phase, last_success, pending_ops)`;
   `counters`, `sidecar`, `features` are deliberately excluded (the
   documented memory-only overlays, empirically default on the stored
   side in 19/19 pairs);
3. `last_updated` is published-only — pinned by the tail snapshot, not
   by the equality.

Empirical status at base: **this exact predicate holds for all 19
current pairs** (verified in §0), so the dispatch non-goal "must pass
against today's engine unmodified" is pre-satisfied on plan-day
evidence.

## 2. The new `Scenario::snap` shape

In `crates/taskboard-state/tests/snapshots.rs` (the `snap` method,
plan-day lines 110–117), keeping the persisted dump assertion
byte-identical in name, value expression and position semantics:

```rust
/// The persisted whole-state contract dump stays the disk-shape
/// defense (unexpected fields, renames, serde drift all surface
/// here); the published half narrows to a *checked* memory ≡ disk
/// equality plus the memory-only tail.
fn snap(&self, scenario: &str) {
    let app = self.handle.shared_state().load_full();
    let persisted = self.repo.snapshot();
    assert_eq!(app.boards, persisted.boards, "memory ≡ disk: boards");
    assert_eq!(app.stacks, persisted.stacks, "memory ≡ disk: stacks");
    assert_eq!(app.tasks, persisted.tasks, "memory ≡ disk: tasks");
    assert_eq!(app.labels, persisted.labels, "memory ≡ disk: labels");
    assert_eq!(app.reminders, persisted.reminders, "memory ≡ disk: reminders");
    assert_sync_stored(&app.sync, &persisted.sync);
    assert_yaml_snapshot!(format!("{scenario}_persisted_state"), &persisted);
    let tail = PublishedTail { sync: &app.sync, last_updated: app.last_updated };
    assert_yaml_snapshot!(format!("{scenario}_published_tail"), &tail);
}
```

- `assert_sync_stored` (new helper, same file): equal key sets, then
  per board `phase`, `last_success`, `pending_ops` — each with a
  message naming the board and field. The doc comment must cite the
  exclusion's warrant: `SyncStatus.counters/sidecar/features` are
  memory-only by design (`state.rs:56–72`, `core.rs:29–33`), so
  excluding them is the documented projection, **not** a weakening.
- `PublishedTail<'a> { sync: &'a SyncStates, last_updated:
  Option<DateTime<Utc>> }` (new, same file, `#[derive(Serialize)]`):
  field order `sync` then `last_updated` matches the old published
  layout, so migrated files read as a verbatim suffix (§3). Requires
  one [dev-dependencies] line in `crates/taskboard-state/Cargo.toml`:
  `serde.workspace = true` (workspace pin already carries
  `features = ["derive"]`, root `Cargo.toml:70`).
- **Load-bearing pin (permanent)**: a `#[should_panic]` unit test on
  `assert_sync_stored` feeding two `SyncStates` that differ in one
  stored field (and a second case differing only in `counters`, which
  must *not* panic). This is the dispatch acceptance's "a deliberate
  memory/disk divergence would fail it — argue or demo" made
  permanent at the lowest layer; it is a unit test of the new helper,
  not a new snapshot scenario (dispatch non-goal untouched). Stage 2
  additionally runs the one-off negative-control demo of §4.
- Prose to update in the same commit: the module doc
  (`snapshots.rs:12–16`, "captures BOTH the published `AppState` and
  … `PersistedState`") and the `snap` doc comment; check the
  `taskboard-state/src/lib.rs:21` walkthrough line for staleness.
  `docs/testing_strategy.org` and AGENTS §5 make no reference to the
  two-dump shape (grepped) — no doc changes there.
- Import path for `SyncStates` in the test (`taskboard_domain::…`
  re-export vs `state::` module path) is an implementation detail.

## 3. Tail-snapshot format and the file migration (auditable; no insta regeneration)

**`INSTA_UPDATE` / `INSTA_ACCEPT` / `INSTA_FORCE_UPDATE` are
forbidden for this migration.** A regenerated tail is "whatever the
engine produced", which is exactly the unauditable review path the
dispatch rejects. The tails must be *derived from the deleted
published files*, so every diff line is explainable, and insta's
acceptance of the hand-written files then doubles as verification.

Key mechanical fact: in the old published layout the last two
sections are `sync:` then `last_updated:` (serde struct order,
confirmed in all 19 files). Therefore **the tail body is a byte
suffix of the old published file**, starting at its first (and only)
column-0 `sync:` line. Per scenario `s`:

1. `git rm crates/taskboard-state/tests/snapshots/snapshots__{s}_published_app_state.snap`
2. Create `snapshots__{s}_published_tail.snap`:
   ```sh
   { printf -- '---\nsource: crates/taskboard-state/tests/snapshots.rs\nexpression: "&tail"\n---\n'
     git show "HEAD:crates/taskboard-state/tests/snapshots/snapshots__{s}_published_app_state.snap" \
       | sed -n '/^sync:/,$p'; } > snapshots__{s}_published_tail.snap
   ```
   (`HEAD` here = the pre-migration commit; the 4-line header mirrors
   the existing files — `source` verbatim, `expression` matching the
   actual test binding, **no** `assertion_line`, which is optional
   metadata insta accepts in its absence.)
3. Run the suite. insta must accept every hand-written file: green
   run, **zero `*.snap.new` / `*.snap.pending` leftovers** in `git
   status`.

Expected PR diff, fully enumerable: `snapshots.rs` (the §2 change),
`Cargo.toml` (+1 dev-dep line), 19 `D` published files, 19 `A` tail
files, **zero modified persisted files**. insta does not rewrite
passing snapshots, so the persisted files must not appear even though
the persisted assertion's line number moves — a stale `assertion_line`
in their headers is cosmetic and must be left alone. Any persisted
`.snap` modification in the PR is a finding to explain (dispatch §11:
"no snapshot content change may hide inside the migration").

**Stage-3 audit recipe** (the reviewability proof, per scenario):

```sh
diff <(git show <base>:crates/taskboard-state/tests/snapshots/snapshots__{s}_published_app_state.snap \
        | sed -n '/^sync:/,$p') \
      <(tail -n +5 crates/taskboard-state/tests/snapshots/snapshots__{s}_published_tail.snap)
# → empty for all 19 scenarios
```

plus the global checks: `git diff --stat <base> -- 'crates/taskboard-state/tests/snapshots/'`
shows exactly 19 D + 19 A, no `M`; no `*_published_app_state.snap`
survives; no orphan/renamed files; `ls *.snap.new` empty.

## 4. Verification & gates

1. `cargo nextest run -p taskboard-state` (snapshots suite first, then
   the crate) — green with zero pending snapshots.
2. Negative-control demo (one-off, reported by stage 2, **not
   committed**): scratch-edit `snap()` to compare against a mutated
   clone (e.g. remove one task from `persisted` before asserting) →
   suite red at the new assert → revert. Demonstrates the equality is
   load-bearing, complementing the permanent `#[should_panic]` pin.
3. Full AGENTS §10 gate sequence at handoff (fmt, clippy
   `-D warnings`, nextest, doc tests, deny, the three-command coverage
   sequence). No CHANGELOG (test-infra only, dispatch §11).

## 5. Escalation rule if the equality fails against the current engine

Per dispatch §11 ("that is a *finding about the engine*, escalate per
AGENTS §5, do not loosen the assertion") and AGENTS §5 ("a new test
failing can be a bug … do not weaken a correct test"):

1. **STOP.** Do not normalize, exclude, or `cfg`-gate anything beyond
   the §1 projection; do not hand off a red or skipped suite.
2. Triage in order: (a) is the failing scenario's `snap()` reached
   only after its deterministic settle predicate (never a fixed
   wait)? A mid-batch capture can legitimately see the documented
   `Syncing` transient (`persistence.rs:60–67`) — that is a **test
   bug** (fix the settle, not the assertion); (b) if the settle is
   sound, the divergence is a **suspected production bug**: the
   engine published state disagreeing with its own persisted mirror —
   the exact bug class this assertion exists to catch.
3. For a suspected production bug: minimize (one scenario, one field
   family, one command), record the repro in the stage-2 report, and
   file a GitHub issue per AGENTS §7 (labels per convention;
   `flaky-test` + `watchlist` instead if and only if the divergence
   is timing-dependent). Report to the orchestrator; the chunk does
   not proceed until the orchestrator rules (fix here as an explicit
   scope extension, or a separate fix chunk re-based on this one).
4. If a divergence is argued *by design*, that argument must be
   written into the `SyncStatus`/engine docs first, the projection
   updated second, and the doc drift recorded as its own issue — an
   undocumented exclusion encoded silently in tests is the failure
   mode this rule exists to prevent.

## 6. Risks & residual gaps

- **A brand-new published-only `AppState` field is silent** (it would
  once have appeared in the published dump; now neither the equality
  nor the tail mentions it). Accepted by F4-03's design — the
  unexpected-field defense intentionally lives on the persisted half,
  where disk-shape contracts belong. Mitigation: the `snap` doc
  comment states that new `AppState` fields must join the equality or
  the `PublishedTail` explicitly.
- **`pending_ops` is compared against the fake's raw `snapshot()`**,
  which — unlike `load()` (`memory.rs:71–90`) — does not re-derive;
  the stored rows carry the derived projection because
  `append_status_if_changed` (`actor.rs:919–935`) stores the derived
  row. Equal-by-construction today (19/19 pairs), and if the actor
  ever stops storing the derived row, the equality *should* fire —
  that is the memory/disk contract working, noted here so nobody
  "fixes" it by deriving inside the test.
- **YAML byte-format coupling**: the hand-written tails assume
  insta's current YAML rendering (quoting, indentation, BTreeMap key
  order). Insta's first run verifies all 19 at once, so a mismatch
  fails loudly and locally, not in review. A future insta serializer
  change would re-churn the tails — a normal, auditable snapshot
  migration at that point.
- **Worktree sweep hazard** (chunk 3's recorded incident): this plan
  is committed immediately after worktree creation; stage 2/3
  recreate the worktree per dispatch §1 if a parallel doctor took it.

## 7. Stage-2 step list (ordered)

1. Read AGENTS.md, #245, dispatch §11, this plan; recreate the
   worktree if missing (branch exists at `88d0ad2`+1 commit).
2. Edit `snapshots.rs` per §2 (helper + tail struct + `#[should_panic]`
   pin + doc updates); add the serde dev-dep line.
3. Migrate the 38 files per §3 (19 `git rm`, 19 hand-derived tails);
   never use insta update/accept env vars.
4. Run the negative-control demo (§4.2); revert it.
5. State-crate suite green, zero `*.snap.new`; then full §10 gates.
6. Commit granularity: one commit for code+manifest+files is
   acceptable (the suite is green exactly at that commit); if a
   scratch commit happens mid-flight, squash before handoff so no
   red state lands on `main` verbatim.

## 8. Stage-3 review checklist

- [ ] Diff is exactly §3's expected shape (19 D + 19 A + snapshots.rs
      + Cargo.toml; no persisted `.snap` modified).
- [ ] §3 audit recipe returns empty for all 19 scenarios.
- [ ] The equality is load-bearing: `#[should_panic]` pins exist, and
      stage 2's demo result is in the report.
- [ ] The sync exclusion is exactly `{counters, sidecar, features}`
      with the doc-cited warrant — no extra normalization smuggled in.
- [ ] Snapshot names: `*_persisted_state.snap` unchanged;
      `*_published_tail.snap` new; no `*_published_app_state.snap`.
- [ ] Full §10 gates green; PR opened; hand off at PR-green per
      AGENTS §8 (stage 3 closes #245).

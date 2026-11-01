# [Plan] Chunk 2.2 — Audit cleanups: domain + state (#270 + #271)

- **Date**: 2026-10-31 (dispatch-scheduled name; written 2026-11-01)
- **Chunk**: 2.2 of backlog round 2
  (dispatch plan: `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §4 "Chunk 2.2 — audit cleanups: domain + state")
- **Branch**: `chore/audit-cleanups-domain-state`, cut off `origin/main`
  at `84281108` (the fetched tip; local `main` was 3 commits behind).
  Worktree: `../taskboard-WT-chore-audit-cleanups-domain-state`
  (real dir beside the main checkout at
  `/home/viktoria/nextcloud/viktoria/programming/`, `src/` symlink
  created to match the house layout).
- **Covers** (normative): #270 → F3-01/F3-02/F3-03; #271 →
  F4-05/F4-06/F4-08 (+ F4-09, annotate-only per the issue).
  Originating reports: `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-domain-report.md`
  (§3 findings table, §7.3 mutant row), `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-state-report.md`
  (§6, §8).
- **Gate**: chunk 1.9 merged — the dispatch named PR **#555**, stale;
  the work actually landed as **PR #568**
  (`feat/undo-tracking-habit-arms`, MERGED 2026-11-01T16:34Z,
  rebase-merge — the local branch is not an ancestor of `origin/main`,
  the content is). Gate satisfied.
- **Status**: planning complete; every open question resolved in §3
  (D1–D7). Implementation not started.
- **Hard rule** (dispatch acceptance): **zero behavior changes.** The
  suites are the pin: snapshots cover the `SyncStatus` rows, the merge
  pins cover every verdict. A changed merge outcome, status row, or
  test semantic is a bug in this chunk.

---

## 1. Method

- Re-verification ran against `origin/main` @ `84281108` in this
  worktree, finding by finding, with the audited v0.1.0 coordinate
  (`925f50e3`) quoted alongside. Every finding was read at its current
  site; none was taken on faith from the issue text.
- Churn context confirmed: the DR-4/DR-5/undo/reminders waves rebuilt
  exactly these seams. Citations below use the most explanatory
  commits (`5ede7fc4` split `push_exec` into a module dir; `4dc85232`
  engine switch/undo; `97a6ae54` day-boundary roll added `day_tick`;
  the per-board `SyncStatus` rework landed with the multi-board sync
  wave). "Fixed-already" candidates: **none found** — no finding was
  silently repaired since 2026-10-17, so stage 2 has no ✅ comments to
  prepare (dispatch bookkeeping item is a no-op).

## 2. Re-verification table (the core)

| Finding | Verdict | Audited @ v0.1.0 | Current @ `84281108` — evidence |
|---|---|---|---|
| **F3-01** (P3) `push.rs` missing from the lib module map | **drifted further** | `lib.rs:20-26` prose map named 13 of 14 modules, `push.rs` (1 275 lines) the gap | `lib.rs:20-33` prose map names 15 of the now-24 `pub mod` declarations. `push` (1 751 lines) is **still absent**, and the map additionally omits `appdata`, `capabilities`, `find`, `habit`, `journal`, `reconcile`, `tracking`, `undo` (9 missing in total). The private kani-gated `verify` mod is also unnamed |
| **F3-02** (P3) R2/R7/R8 lack rule consts and per-rule verdict events; their verdicts log under `RULE_R4` | **still drifted, wider** | consts `RULE_R1/R3/R4/R5/R6/R9` @ `merge.rs:58-63`; the per-field task verdict debug @ `merge.rs:235` logged under `rule = RULE_R4`; R2 survived only in test comments | consts now `merge.rs:66-74` — **still no R2/R7/R8**; the block gained `RULE_R_DUP` ("R-dup", #150) and `RULE_R_RECUR_DUP` ("R-recur-dup", `merge.rs:1391`), widening the asymmetry. The task per-field umbrella event is now `merge.rs:506-530`, still `rule = RULE_R4`, still carrying the R7 fields (`labels_remote_adds`/`labels_remote_removes`) and the R8 field (`position_remote_wins`) — and grew a `scheduled_*` pair (D-V2). R2 still exists only in test comments (`pipeline.rs:4010`, `:5534`, `command.rs:5222`). Verified: no R7/R8-tagged event exists anywhere in `merge.rs`; note the R4 umbrella is **task-scoped** — bound stack/board pull-side merges log under `RULE_R6` (`merge.rs:1614`, `:1875-1878`) |
| **F3-03** (P3) `MaterializedOp::is_noop` pub, zero production callers, mutant survives | **still drifted** | `push.rs:127-131`; executor pattern-matched `Noop` @ `push_exec.rs:128`; one true-case test caller @ `push.rs:1213-1220` | `is_noop` now `push.rs:164-168` — still `pub const fn` + `#[must_use]`, **still zero production callers**. The executor split into `push_exec/` (`5ede7fc4`): `MaterializedOp::Noop` is matched directly @ `push_exec/execute.rs:69`. Test callers are now **two** (`push.rs:1649`, `:1694`) and **both assert the true case** — the surviving mutant (`is_noop → true`) still survives; no false-case assert exists. Workspace grep: no other callers (the `taskboard-state` hit is a test *name*) |
| **F4-05** (P3) `FlakyRepository` fake duplicated across the engine/reminders suites | **still drifted, signatures diverged** | sonar cluster `engine.rs:53-77 ↔ reminders.rs:48-73`; engine's copy adds `rearm` | engine copy `tests/engine.rs:36-79` — `new(failures)` seeds `InMemoryRepository::with_state(seed_board_state("board"))`, has `rearm`; reminders copy `tests/reminders.rs:38-77` — `new(failures, state: PersistedState)`, **no `rearm`**, doc comment cross-references the engine copy. The `apply` bodies are **byte-identical** (`AtomicU32::try_update` decrement). A struct-literal construction @ `engine.rs:3124-3131` bypasses both constructors (habit-series seed). `tests/common/mod.rs` exists and every suite already declares `mod common` — the hoist's plumbing is pre-built |
| **F4-06** (P3) duplicated `SyncStatus` construction with the derived-field comment | **still drifted, grew** | `actor.rs:639-643` (Failed path) and `:740-744` (`go_offline`); shared comment "`pending_ops: 0, // ignored: derived at apply time`" | Constructions now `actor.rs:842-856` (inside `ingest_sync_report`'s Failed arm, per-board) and `actor.rs:1082-1094` (`go_offline`, per-board) — the **multi-board wave** made `SyncStatus` a per-board row (`core.sync_status(board) -> Option<&SyncStatus>`, `core.rs:109-111`) and grew the carry-over set to `last_success` + `sidecar` + `features` (memory-only fields, domain `state.rs:59-83`) with `..SyncStatus::default()`. The old shared comment is gone; two **diverged** comments replaced it — a 3-line version @ `:845-847` and a "Memory-only fields carry over (see above)." @ `:1086` — the copy-drift marker the finding warned about, back again |
| **F4-08** (P3) positional `Seams`/`TickSeams` tuples invite transposition | **drifted further** | `Seams` 5 fields @ `actor.rs:492-498`; `TickSeams` 4 @ `:767-772`; `EngineParts` named struct @ `:288-296` | The tuples are now **type aliases**, and the family grew to **four**: `type Seams<'a>` = 6 fields @ `actor.rs:605-613` (`notifier` joined) consumed positionally by `execute_command` @ `:620`; `type TickSeams<'a>` = 4 @ `:1132-1140` consumed by `reminder_tick` @ `:1149`; **plus two anonymous inline 4-tuples** — `ingest_sync_report` @ `:748-753` (`repo, ids, clock, notifier`) and `day_tick` @ `:1187-1192` (`repo, ids, clock, system_out`, added by `97a6ae54`). Constructed as positional literals inside `run_loop`'s select arms (`:458-464`, `:563`, and the `execute_command` call @ `:411`). `EngineParts` (named struct, by-name destructure) @ `:318-328` is the style to mirror. The audit's "stops growing a third member" hope failed: it grew two |
| **F4-09** (P3) the shared handle's read-only-ness is convention-enforced | **unchanged — accepted** | `shared_state()` hands out `Arc<ArcSwap<AppState>>` @ `actor.rs:196-200` | Same @ `actor.rs:214-217`. Issue disposition: **no action** — the audit's §2 writer enumeration is the periodic check; enforcement proposals belong to axis O (F7-02). Re-verified the convention still holds (no production `.store` outside the state actor's `publish`) — annotate-only |

## 3. Decisions (every open question resolved)

### D1 — F3-01: repair the whole module map, not just `push`

The issue's normative ask is the `push.rs` entry, but the map has
rotted past it (9 of 24 modules unnamed). Fixing one entry would leave
the map wrong and guarantee the next audit round re-finds F3-01.

- **Edit**: `crates/taskboard-domain/src/lib.rs`, the `//!` module-map
  paragraph (lines 20-33). Extend it so every `pub mod` is named with
  its role, in declaration order where sensible:
  `push` (push planning: `plan_pushes`, `PushGroup`, `MaterializedOp`),
  `appdata` (sidecar document), `capabilities` (probed connector
  capabilities), `find` (entity lookup helpers), `habit` (habit fold),
  `journal` (undo journal shapes), `reconcile` (crash-window duplicate
  reconcile), `tracking` (tracking entry semantics), `undo`
  (inverse synthesis). Stage 2 reads each module's own `//!` header and
  writes a one-clause role per module — the map is orientation, not
  specification.
- The private `verify` mod (kani-gated) gets a trailing prose mention
  ("kani bounded-model harnesses, private"), **not** an intra-doc link
  (it is not a `pub` item; a link would not resolve).
- **Type**: `docs(domain)`. Pure doc comment; zero risk.

### D2 — F3-02: document the umbrella mapping; do NOT add consts/events

The issue allows either. **Decision: document the mapping.** Rationale,
against the alternative:

- `RULE_R2` would be a const with **no emission site**: R2 is the
  quiet rule — a local-only (never-pushed) entity is untouched by the
  merge; there is no verdict to log. Inventing an event for it either
  logs noise per merged task or adds a new event in `pipeline`,
  both of which churn runtime behavior the dispatch forbids.
- Dedicated R7/R8 events would split the single per-field task debug
  event into three, tripling event volume on the hottest merge path.
  The umbrella event already carries their verdicts as typed fields
  (`labels_remote_adds`/`labels_remote_removes` = R7;
  `position_remote_wins` = R8) — the module doc's Logging contract
  ("debug events carry rule verdicts as typed facts") is satisfied
  today; only the *mapping* is undocumented.
- The block's newer members (`RULE_R_DUP`, `RULE_R_RECUR_DUP`) show
  the house pattern: rules with real emission sites get consts.
  Documenting keeps that honesty; dead consts would break it.

**Edit** (two doc comments, zero runtime change):

1. `merge.rs:64-66` — extend the consts-block doc with the full
   tag-vocabulary mapping. Stage 2 writes each rule bullet from that
   const's actual use sites (`grep -n 'rule = RULE_'`); the three
   finding-relevant bullets are fixed verbatim:
   - **R2 — the quiet rule**: a local-only (never-pushed) entity is
     untouched; no code path emits a verdict event (it manifests as the
     pipeline skipping the merge for unbound rows). No const, by design.
   - **R7 (labels)** and **R8 (composite position)** have no dedicated
     events: their per-field verdicts ride the `RULE_R4` task event
     (`labels_remote_adds` / `labels_remote_removes` /
     `position_remote_wins`).
   - Plus: R4 is the **task**-scoped per-field umbrella; bound
     stack/board pull-side merges log under R6.
2. `merge.rs:30-37` (the Logging contract section) — one pointer
   sentence: the rule-tag vocabulary is documented at the consts block,
   including R2's silence and the R7/R8 ride-along.

**Type**: `docs(domain)`.

### D3 — F3-03: demote `is_noop` to test-only + add the false-case assert

Consumption at the executor was the audit's other option; **decision:
demote**. Rationale:

- The `Noop` match arm at `push_exec/execute.rs:69` must stay anyway
  (match totality). Routing the call through `if group.materialized.is_noop()`
  ahead of the match would duplicate the arm's semantics and add a
  second decision site that can drift — artificial consumption, no
  behavior gain. No other production candidate exists (`MaterializedOp`
  is handled nowhere else outside domain's planner and its tests).
- A `pub` predicate with zero production callers is the dead-API
  artifact the audit named. `#[cfg(test)]` removes it from the shipped
  surface while keeping it available to the two inline tests.

**Edits**:

1. `push.rs:163-168`: `#[cfg(test)]` on the inherent method (keep the
   doc comment, `#[must_use]`, `pub const fn` — the cfg hides it from
   non-test builds; the crate's inline test mod keeps compiling).
2. False-case assert, one line in the existing test
   `board_update_materializes_only_for_a_bound_live_board`
   (`push.rs:1652-1700`): the bound board's `UpdateBoard` group is
   already in hand — assert `!…materialized.is_noop()` beside the
   existing `matches!`. With this, `→ true` dies on the false case and
   `→ false` dies on the two true cases: the method is fully
   mutant-pinned.

**Type**: `refactor(domain)`. Behavior: none (cfg-gated method, test-only
assert).

### D4 — F4-05: one `FlakyRepository` in `tests/common/mod.rs`, `rearm` kept

**Edit**: move the fake to `crates/taskboard-state/tests/common/mod.rs`
with the **reminders suite's constructor shape** (the more general
one) plus `rearm`:

```rust
/// Fails the first `failures` applies with `Unavailable`, then
/// delegates — non-mutating on failure, matching transactional reality.
#[derive(Debug)]
pub struct FlakyRepository {
    inner: InMemoryRepository,
    remaining_failures: std::sync::atomic::AtomicU32,
}

impl FlakyRepository {
    pub fn new(failures: u32, state: PersistedState) -> Self { … }
    /// Re-arms the failure injector (the edge-trigger test walks the
    /// fail -> recover -> fail cycle).
    pub fn rearm(&self, failures: u32) { … }
}

impl TaskRepository for FlakyRepository { … } // byte-identical to both copies
```

- Engine call sites adapt: `:709`, `:758`, `:1799` become
  `FlakyRepository::new(<n>, seed_board_state("board"))` (their current
  one-arg constructor seeded exactly this internally — byte-identical
  state); the struct literal @ `:3124-3131` becomes
  `FlakyRepository::new(1, seed_series_state(…))`. Reminders' call
  site `:839` is already in the target shape.
- Both suites consume via `use common::FlakyRepository`; the two local
  copies (structs + impls + duplicated doc comments) are deleted.
- `common/mod.rs` gains the needed imports
  (`taskboard_domain::test_support::InMemoryRepository`,
  `PersistenceAction`, `RepositoryError`, `TaskRepository`, `BoxFuture`)
  behind the module's existing `#![allow(dead_code)]`.
- **Rejected**: a `seeded(failures)` convenience preserving the one-arg
  call — two constructors re-create the constructor divergence this
  finding names.

**Type**: `test(state)`. Behavior: identical apply semantics (bodies
byte-identical); `rearm` semantics kept (the a17 edge-trigger test is
the pin and must stay green unmodified).

### D5 — F4-06: one private `sync_status_action` helper in `actor.rs`

The duplicated construction is bigger than at the audit (per-board
rows, `sidecar`/`features` carry-over) but still differs between the
two sites only in `phase`. **Helper**:

```rust
/// The verdict row a failed/offline transition persists for one bound
/// board: the phase flips; `last_success`, the sidecar health, and the
/// probed features carry over from the current row; the memory-only
/// fields (`counters`) stay at their defaults — `EngineCore::app`
/// overlays them from the live rows, and the storage codec never
/// persists them.
fn sync_status_action(current: Option<&SyncStatus>, phase: SyncPhase) -> SyncStatus {
    SyncStatus {
        phase,
        last_success: current.and_then(|row| row.last_success),
        sidecar: current.map_or(SidecarHealth::default(), |row| row.sidecar),
        features: current.map_or(DeckFeatures::default(), |row| row.features),
        ..SyncStatus::default()
    }
}
```

- Call sites: `actor.rs:844` →
  `PersistenceAction::UpsertSyncStatus(*board, sync_status_action(current, SyncPhase::Failed { last_error: kind }))`;
  `actor.rs:1084` → the `SyncPhase::Offline` analogue. The two
  diverged comments collapse into the helper doc above.
- Naming: the dispatch/issue name is kept (`sync_status_action`) even
  though the helper returns the *row*, not the `PersistenceAction` —
  the board binding lives at the call sites, and the audit's
  `-> PersistenceAction` sketch predates per-board rows. Recorded as a
  deliberate signature deviation.
- Behavior: byte-identical value construction (same fields, same
  defaults, same carry-over); the s-series snapshots pin the stored
  rows.

**Type**: `refactor(state)`.

### D6 — F4-08: named seam structs mirroring `EngineParts`, all four tuples

Acceptance requires "no mixed representation left behind" — and the
tuple family grew to four members. **Replace every positional tuple
parameter with a named struct**, `EngineParts`' style (private struct,
named fields, by-name destructure at the top of the fn):

```rust
/// The command path's injected seams.
struct Seams<'a> {
    repo: &'a Arc<dyn TaskRepository>,
    ids: &'a dyn IdGenerator,
    clock: &'a dyn Clock,
    sync_outs: &'a mut Vec<mpsc::Sender<SyncCommand>>,
    system_out: &'a broadcast::WeakSender<SystemEvent>,
    notifier: &'a mpsc::Sender<NotifierMessage>,
}
/// The report-ingest path's seams.
struct IngestSeams<'a> { repo: &'a Arc<dyn TaskRepository>, ids: &'a dyn IdGenerator,
                         clock: &'a dyn Clock, notifier: &'a mpsc::Sender<NotifierMessage> }
/// The reminder tick's seams.
struct TickSeams<'a> { repo: &'a Arc<dyn TaskRepository>, clock: &'a dyn Clock,
                       system_out: &'a broadcast::WeakSender<SystemEvent>,
                       notifier: &'a mpsc::Sender<NotifierMessage> }
/// The day roll's seams.
struct DaySeams<'a> { repo: &'a Arc<dyn TaskRepository>, ids: &'a dyn IdGenerator,
                      clock: &'a dyn Clock,
                      system_out: &'a broadcast::WeakSender<SystemEvent> }
```

- Existing alias names `Seams`/`TickSeams` are kept; the two anonymous
  tuples join the family as `IngestSeams`/`DaySeams`. Field types are
  copied verbatim from the aliases (`actor.rs:605-613`, `:1132-1140`)
  and the anonymous signatures (`:748-753`, `:1187-1192`).
- Construction: `run_loop`'s select arms build struct literals at the
  four call sites (`execute_command` @ `:411`, `ingest_sync_report` @
  `:458`, `reminder_tick` @ `:563`, `day_tick` just after). Destructure
  sites (`:620`, `:748`, `:1149`, `:1187`) become by-name
  `let Seams { repo, ids, … } = seams;` blocks, matching `run_loop`'s
  `EngineParts` destructure @ `:337`. The two tuple doc comments
  ("The seams tuple: …") are deleted — the struct field names are the
  documentation.
- Per-function shapes are **kept deliberately** (each fn sees only the
  seams it uses — that trimming is the aliases' reason to exist).
  **Rejected**: one shared six-field struct for all four fns — `sync_outs`
  is `&mut`, unused-field noise per fn, and it blurs which fn touches
  which seam.
- `note_storage_failure` (`:1115`, two scalar params) is not a tuple —
  untouched.
- The `#[allow(clippy::too_many_arguments)]` attrs on the affected fns
  may be re-triaged after the change (struct params shrink arities);
  drop an attr only if clippy is clean without it.

**Type**: `refactor(state)`. Behavior: none (types are compile-time
groupings).

### D7 — F4-09: no action, verdict re-recorded

Per the issue: accepted; the audit report's §2 writer enumeration is
the periodic check; enforcement proposals live with axis O (F7-02).
Stage 2 prepares the issue comment text recording today's
re-verification (convention still holds at `84281108`); stage 3 posts.
No diff hunk.

## 4. Implementation order & commit breakdown

One logical commit per finding-cluster, each standalone and
build-green (rebase-merge hygiene, AGENTS §8):

| # | Commit | Finding | Content |
|---|---|---|---|
| 1 | `docs(domain): the module map learns the nine missing modules` | F3-01 | lib.rs prose map extension (D1) |
| 2 | `docs(domain): the rule-tag vocabulary documents the R2/R7/R8 mapping` | F3-02 | consts-block mapping + Logging-contract pointer (D2) |
| 3 | `refactor(domain): is_noop demotes to test-only with a false-case pin` | F3-03 | cfg(test) + false-case assert (D3) |
| 4 | `test(state): the FlakyRepository fake hoists to tests/common` | F4-05 | common hoist, `rearm` kept, both suites consume (D4) |
| 5 | `refactor(state): one sync_status_action helper owns the verdict-row carry-over` | F4-06 | helper + two call sites (D5) |
| 6 | `refactor(state): the seam tuples become named structs` | F4-08 | four named structs, all use sites (D6) |

F4-09: no commit (D7). Suggested order puts the pure-doc commits first
(zero-risk), then the code-adjacent refactors; 4-6 are independent and
may land in any order.

## 5. Verification (stage 2 exit criteria)

- `scripts/run-quality-gates.sh cargo fmt --all -- --check` +
  clippy `-D warnings` + nextest + doc tests + `cargo deny check` +
  the coverage triple — the full §10 block, through the wrapper (this
  box runs parallel agents).
- Zero-behavior pins: the full suite green **unmodified in semantics**
  — s-series snapshots (stored `SyncStatus` rows), the a17
  edge-trigger test (`rearm`), the merge pins (verdict behavior), the
  push-planner tests (`is_noop` true cases). New test code in this
  chunk: the single false-case assert only (D3).
- Grep audits (each rename/move leaves nothing stale):
  - `grep -rn 'is_noop' crates/` → definition + the three in-crate test
    uses only, all under `cfg(test)`;
  - `grep -rn 'FlakyRepository' crates/taskboard-state/` → the common
    module + the consuming use-sites, no local copies;
  - `grep -rnE '(repo, ids, clock|repo, clock,' crates/taskboard-state/src/actor.rs`
    → no positional seam tuples survive; every helper takes a named
    struct;
  - the strings "see above" and "seams tuple" are gone from `actor.rs`.
- `cargo doc` on the domain crate builds warning-free (intra-doc links
  in the extended map all resolve).

## 6. Risks

1. **`Seams` alias → struct of the same name**: the positional
   destructure patterns become by-name; the compiler catches any
   transposition this finding exists to prevent. Low.
2. **`&mut Vec<…>` inside a struct field**: `run_loop` constructs
   `Seams { sync_outs: &mut sync_outs, … }` per command; reborrow
   mechanics are identical to today's tuple construction. Compile-time
   concern only. Low.
3. **cfg(test) demotion as an API change**: `MaterializedOp` stays pub;
   only the predicate hides. Workspace grep shows no external caller;
   the workspace build is the proof. Low.
4. **Snapshot churn**: none expected — D5 constructs byte-identical
   values. If any s-series snapshot changes, that is a behavior change
   and the diff is wrong by definition. Medium consequence, near-zero
   probability.
5. **The plan file's date** (2026-10-31 vs. today's 2026-11-01): kept
   per the dispatch prompt's verbatim path. Noted so no one reads it as
   a dating error.

## 7. Out of scope (recorded, not done)

- The other suites' repository fakes (`tracking.rs`, `undo.rs`,
  `habits.rs` use `InMemoryRepository` or their own local fakes) —
  F4-05 names the `FlakyRepository` pair only.
- New rule consts or events for any rule (D2's alternative), the
  `verify` module's documentation beyond the prose mention, and any
  read-only-handle enforcement (D7 → axis O).

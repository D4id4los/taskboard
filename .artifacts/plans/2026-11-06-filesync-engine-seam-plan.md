# [Plan] The User-Data Engine Seam & the Journal Import (TDD implementation plan)

- **Date**: 2026-11-06
- **Kind**: stage-1 implementation plan — chunk 3 of the DR-8
  implementation dispatch (branch `feat/filesync-engine-seam`, issue
  #631). TDD-first: every commit below names its failing tests before
  the code that satisfies them.
- **Origin** (normative): the accepted design round
  `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  §3 Part E (D-SF11–D-SF13), §5 (the engine test hooks); the
  cross-milestone contract
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  §3 (**binding**); the landed entity's design + record
  (`.artifacts/plans/2026-11-05-journal-review-design-round-plan.md`,
  `.artifacts/plans/2026-11-05-reflection-domain-plan.md`,
  `docs/adr/0021-reflection-family-device-local-roaming-ready.org`);
  the chunk-2 seams it plugs into
  (`.artifacts/plans/2026-11-05-sync-files-nextcloud-plan.md` D-FT11);
  the dispatch plan
  `.artifacts/plans/2026-11-05-filesync-substrate-implementation-dispatch-plan.md` §4.
- **Status**: READY for the stage-2 implementation agent. Decisions
  D-ES1…D-ES9 settle every open question this chunk owns; a deviation
  needs a recorded reason in the stage-2 report.

---

## 1. Gate & verified anchors (verified 2026-11-06 against `origin/main` @ `ccb137a9`)

| Fact | Anchor | Consequence |
|---|---|---|
| **External gate met**: the journal entity landed under DR-7's vocabulary | PR #661 (`feat/reflection-domain`, issue #636 closed) + PR #666 (`feat/reflection-storage-cli`, #637 closed); `crates/taskboard-domain/src/reflection.rs`, migration `0022_reflection_entries.sql`, ADR 0021 | The import has a real target. Vocabulary: `reflection` / `ReflectionEntry` / `ReflectionId` in the domain; the **wire** section name stays `journal` (`taskboard_sync_files::document::JOURNAL_SECTION`) — undo's `journal.rs` names are never touched |
| Entity shape | `reflection.rs:290` `ReflectionEntry { id, cadence, day, window_start, window_end, text, attributes, created_at, updated_at, deleted }`; derives `Serialize, Deserialize` (serde default = unknown keys ignored); `ReflectionAttributes` has hand-written tolerant decode (unknown flags park into `extra_flags`; **ratings decode strictly**) | The wire body *is* the entity minus `id` (the map key), `updated_at` (the row `clock`), `deleted` (the row grammar) |
| State home | `state.rs:47` `AppState.reflection: ReflectionState { entries: BTreeMap<ReflectionId, ReflectionEntry> }`, tombstones in place, `live_entries()` the read seam | The import folds into `entries`; no new state field |
| Persistence | `PersistenceAction::UpsertReflectionEntry(entry)` (`persistence.rs:290`), emitted by `RecordReflection`/`UpdateReflection`/`DeleteReflection` (`command.rs:2683/2747/2810`); storage writes timestamps `to_rfc3339()` (sub-second preserved) | Import persists through the **same** action — no new action, no migration (D-SF19) |
| Board report lane | `SyncReport` (`messages.rs:841`) rides a bare `mpsc::Receiver<SyncReport>` into `run_loop` (`actor.rs:342`, arm `:458`), folded by `ingest_sync_report` (`:781`) → `pipeline::apply_sync_report` (`pipeline.rs:423`); publish `shared.store(...)` (`actor.rs:629`) | `UserDataReport` joins as a **sibling arm** of the same select (D-ES2); `SyncReport` untouched |
| Sidecar-fold clock rule | `merge.rs:677` `if d < l { continue; }` — **document wins ties** (`>=`); rules `merge.rs:797`, time entries `:1095` the same; `Verbatim` never folds | The import guard mirrors it verbatim (D-ES4) |
| `UpdateAppData` | outbox-only `LocalOp::UpdateAppData(TaskId)` (`outbox.rs:81`), enqueued from `command.rs:566-576/715/801` | Import path never reaches the outbox (D-ES6 pin) |
| Files connector seams | `cycle.rs:54` `trait LocalDocument` ("chunk 3 derives it from the published `ArcSwap<AppState>`"); `EmptyDocument` the production impl; `merged_view` = `fold(local ∪ publish_seed ∪ peers)` (`cycle.rs:557`); attach reads all peers (`read_dirty_peers`) **then** create-once-publishes (`cycle.rs:~287-296`); actor has no engine sender (`actor.rs:8`) | Chunk 3 adds an impl + a report sender; the attach already has the fold-first order, it gains the import step between read and publish (D-ES5) |
| Spawn | `bootstrap.rs:157` `files_sync_spawn(...)` → `spawn_files_sync_actor(transport, device_json, Arc::new(EmptyDocument), system, cfg)` (`actor.rs:69`); the engine's published `ArcSwap` is in scope in bootstrap (`:1278` notifier reads it) | Wiring = swap `EmptyDocument` for the state-derived impl + pass the report sender |
| Test machinery | `taskboard-state/tests/snapshots.rs` `Harness` (`snap(scenario)` → `persisted_state` + `published_tail` yaml); `tests/engine.rs` `Harness::spawn` + `eventually()`; `tests/common` `FrozenClock`, `ts()`; `taskboard-sync-files-nextcloud` feature `test-support` exports `FakeFilesTransport` "the chunk-3 convergence journeys reuse it"; `taskboard-sync-files` feature `test-support` exports document strategies | No new test infrastructure beyond a harness lane and a two-engine fixture |
| Clock | `taskboard_domain::Clock` (`clock.rs:17`), engine holds `Arc<dyn Clock>`; the files cycle takes one too | Import stamps nothing (it carries foreign clocks verbatim); the clock seam is only needed by the journeys |

## 2. Scope & non-goals

**In:** the `UserDataReport` type; its channel and engine arm; the pure
domain import fold + its laws; the journal wire codec (entity ↔
`UserRecord`); the state-derived `LocalDocument`; the attach/cycle
emission of the report; bootstrap wiring; insta snapshot journeys; the
two-engine convergence journey; CHANGELOG.

**Out (chunk 4 or later):** surfacing of import rejections beyond a
typed counter + `tracing` finding; divergence alarm; runbooks;
architecture.org; any connector nudge on local edits (the poll cadence
publishes; D-SF8); the reserved `settings`/`proofs` sections; B-full
kinds; any change to `SyncReport`, the board pipeline, the outbox, the
undo journal, or the storage schema.

## 3. Design decisions (D-ES1…D-ES9, normative for stage 2)

### D-ES1 — The `UserDataReport` type (domain, wire-agnostic)

`taskboard-domain` cannot see `taskboard-sync-files` (and must not:
the domain is the dependency root). The report therefore carries
**decoded domain rows**, never `UserRecord`/`Value`:

```rust
// crates/taskboard-domain/src/messages.rs, beside SyncReport
/// Files-substrate actor → engine: the merged per-user sections whose
/// rows differ from what the actor last derived from published state
/// (D-SF11). Never board-shaped; never routes through the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct UserDataReport {
    pub journal: Vec<ImportedReflection>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportedReflection {
    /// A decoded live row; `entry.updated_at` is the wire clock,
    /// `entry.deleted == false`.
    Entry(ReflectionEntry),
    /// A wire tombstone: id + clock only (the wire grammar carries no
    /// body).
    Tombstone { id: ReflectionId, clock: DateTime<Utc> },
}
```

- One field per typed section (v1: `journal`). A future section is a
  new field — the struct is the D-SF11 "`{ sections }`" family; an
  enum-per-section would force one report per section per cycle for
  no gain.
- `Verbatim` rows and undecodable `Entry` bodies **never cross** the
  seam (D-SF11 "`Verbatim` ignored"): the connector drops them from
  the report (they still relay in its published view — chunk 2's
  structural relay is unchanged). The connector counts them
  (`rejected_rows: usize` on the cycle outcome, a `tracing` finding
  `files_sync.import_rejected` — chunk 4 surfaces it).
- `SyncReport` is **not touched** — no variant, no field.

### D-ES2 — The channel join: a sibling lane in the same engine loop

D-SF11 says "on the same connector→engine channel". The board channel
is a bare `mpsc<SyncReport>` shared by registry connectors; the files
actor is deliberately *not* a registry connector (bootstrap
`:1414` comment). Folding `UserDataReport` into that channel would
need a wrapper enum and touch every board connector's sender type —
i.e. `SyncReport`'s lane. Ruling (the recorded reading of D-SF11): **a
second bounded `mpsc::channel::<UserDataReport>(USER_DATA_REPORT_CAPACITY = 4)`
received by a sibling `select!` arm in the same `run_loop`**, beside
`sync_reports.recv()`, with the same `reports_open`-style closed-lane
latch (`user_data_open`). Same actor, same ingest seams, same publish
discipline; ordering between the two lanes is irrelevant (disjoint
state: boards vs `reflection`).

- `spawn_state_engine` gains the receiver as an input. If it takes
  loose arguments, add one; every call site (bootstrap + test
  harnesses) passes it — harnesses keep the sender alive for the
  journey tests and pass a dropped one elsewhere (the latch makes a
  closed lane inert, pinned by a unit test: closing the lane never
  stops the loop).
- Bootstrap creates the pair unconditionally (cheap); the sender goes
  to `files_sync_spawn` only when the substrate spawns; otherwise it
  is dropped and the latch closes the arm.

### D-ES3 — The pure import fold (domain)

```rust
// crates/taskboard-domain/src/reflection.rs (the entity's module)
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReflectionImport {
    pub actions: Vec<PersistenceAction>,  // UpsertReflectionEntry only
    pub applied: usize,
    pub skipped_stale: usize,
    pub skipped_absent_tombstone: usize,
}

/// Folds imported rows into the reflection home under the sidecar
/// clock rule (D-ES4). Pure; mutates `state` only for applied rows.
pub fn import_reflections(state: &mut ReflectionState, rows: &[ImportedReflection]) -> ReflectionImport;
```

Per row, against `local = state.entries.get(id)`:

| Row | Local | Rule | Effect |
|---|---|---|---|
| `Entry(e)` | absent | — | insert `e`; upsert action |
| `Entry(e)` | present | `e.updated_at >= local.updated_at` ∧ `e != *local` | replace; upsert |
| `Entry(e)` | present | otherwise | skip (stale or identical — no action, idempotence) |
| `Tombstone{id,clock}` | present | `clock >= local.updated_at` ∧ `!(local.deleted ∧ local.updated_at == clock)` | `deleted = true`, `updated_at = clock`, body kept (tombstone-in-place, ADR 0021); upsert |
| `Tombstone{..}` | absent | — | **skip** (`skipped_absent_tombstone`): no ghost row is minted — the entity's fields (cadence, day, window) cannot be invented, and the contract forbids placeholder rows. Deliberate leniency documented at the fn: the tombstone keeps winning in every *published* view because the fold relays it, so no peer can resurrect the entry through this device |

- An imported `Entry` for a locally tombstoned row with a newer clock
  **un-deletes** it (later edit beats older tombstone — D-SF5,
  `RuleEntry` semantics).
- Imports **never** touch `created_at` semantics beyond taking the
  winner's row verbatim, never mint ids, never read the clock.

### D-ES4 — The import-vs-local clock guard: `>=`, ties to the document

Mirrors the sidecar folds (`merge.rs:677`, `:797`, `:1095`): an
imported row applies iff its clock is **≥** the local row's
`updated_at`. Why `>=` is *required* here, not merely precedent: the
row the actor reports is the winner of `fold(local ∪ peers)`, whose
equal-clock tie is broken by the content totaliser (`fold.rs`
header). If the engine kept local on ties (`>`), a device whose local
row lost the totaliser would publish the peer's row while keeping its
own — the two devices' state hashes would disagree forever (a
permanent divergence finding in chunk 4). With `>=` both engines adopt
the fold's winner: convergence by construction. The guard's real job
is the **race**: a local edit committed after the cycle read its
`LocalDocument` snapshot carries a later `updated_at` than anything
the report holds, so the stale report row is skipped
(`skipped_stale`) and the next cycle publishes the edit.

### D-ES5 — The attach & cycle ceremony: fold-first, then publish

Both the attach and every steady cycle get one step, inserted between
"peers read" and "own appdata publish":

1. read peers (unchanged);
2. `merged = merged_view(ctx, st)` (unchanged);
3. **import**: `delta = journal_delta(&merged, &ctx.local.document())`
   — the merged rows that differ from the local-derived rows (absent
   locally, or different `UserRecord`); decode each via D-ES7; if
   non-empty **and** its fingerprint (a `StateHash`-style canonical
   hash of the delta rows) differs from `st.last_import_fingerprint`,
   `report_tx.send(UserDataReport { journal }).await`, then record the
   fingerprint. A closed engine lane logs once and the actor keeps
   syncing (publishing still relays everything).
4. publish `merged` (unchanged: create-once at attach, guarded rewrite
   in steady state, skip-PUT by hash).

The fingerprint dedupe exists for the absent-tombstone and
undecodable-row cases, which differ from the local view forever by
design; without it every cycle would re-send a no-op report. The
engine is independently no-op-safe (D-ES6).

The publish does **not** wait for the engine to apply: `merged`
already contains local ∪ peers, so what the device publishes is
identical whether the import has landed yet or not — fold-first is an
ordering of *intent* (peers' history enters the engine before this
device's first document claims a view), and the next cycle's
`LocalDocument` read sees the imported state, which equals `merged`'s
journal ⇒ delta empty ⇒ no report ⇒ hash unchanged ⇒ no PUT (the
steady-state pin).

### D-ES6 — The engine arm: fold, persist, publish-iff-changed; no board ops

```rust
// crates/taskboard-state/src/actor.rs
maybe = user_data.recv(), if user_data_open => match maybe {
    Some(report) => ingest_user_data_report(&mut core, &shared, report, seams),
    None => user_data_open = false,
}
```

`ingest_user_data_report` clones `core.app().reflection`, runs
`import_reflections`, and **iff `applied > 0`**: writes the new
reflection home into the core state, hands `actions` to the same
persistence batch path the command arm uses, and publishes
(`shared.store`). Otherwise it does nothing (no publish, no write).

Pins (the "no board ops" family, D-SF11):
- the outbox is byte-identical before/after any import (no `LocalOp`,
  no `UpdateAppData`);
- the persistence batch contains only `UpsertReflectionEntry`;
- the undo `JournalState` is unchanged (an import is not a user
  command — it must not be undoable, and must not shadow the undo
  vocabulary);
- `SyncReport`-derived state (boards, sync status rows, counters) is
  unchanged.

### D-ES7 — The journal wire codec (in `taskboard-sync-files`)

The transport-agnostic core owns the codec so the folder/LAN
successors reuse it: `taskboard-sync-files` gains a
`taskboard-domain` dependency (pure → pure; already pinned in
`[workspace.dependencies]`) and a `journal` module:

```rust
pub fn encode_row(entry: &ReflectionEntry) -> (String, UserRecord);
pub fn decode_row(key: &str, record: &UserRecord) -> Result<ImportedReflection, RowRejection>;
pub fn journal_section(state: &ReflectionState) -> BTreeMap<String, UserRecord>;
```

- Encode: key = `id` (UUID text); live → `Entry { body: entity
  serialized minus {id, updated_at, deleted}, clock: updated_at,
  author: None }`; tombstoned → `Tombstone { clock: updated_at, author:
  None }` (wire grammar: no body; local keeps its body in place).
- Decode: `Entry` → insert `id`/`updated_at = clock`/`deleted = false`
  into the body map, `serde_json::from_value::<ReflectionEntry>`;
  unknown body keys are ignored by the derive (tolerance — pinned);
  the `author` slot is ignored for import (it ranked in the fold
  already). `Tombstone` → `ImportedReflection::Tombstone`. `Verbatim`,
  a non-UUID key, or a body that fails decode (e.g. a newer build's
  out-of-range rating — ratings decode strictly by DR-7) →
  `RowRejection` (typed enum: `Verbatim`, `BadKey`, `BadBody`),
  never a panic, never a whole-document failure.
- The A1 mint at the trust boundary is `ReflectionId`'s own parse and
  the entity's own serde; no new newtype is needed.

### D-ES8 — The state-derived `LocalDocument` (publish derivation)

```rust
// crates/taskboard-sync-files-nextcloud/src/cycle.rs (beside EmptyDocument)
pub struct PublishedJournal { state: Arc<ArcSwap<AppState>> }
impl LocalDocument for PublishedJournal {
    fn document(&self) -> UserDocument  // state → journal section → document
}
```

`load()` once per call (one coherent snapshot), `journal_section(&
snap.reflection)` over **all** entries (tombstones included — silent
absence would let a peer resurrect a deleted entry), wrapped in the
`DOCUMENT_FORMAT` envelope with no unknown sections. `EmptyDocument`
stays as the test/legacy impl. `arc-swap` is added to the crate from
the workspace pin. Derivation is **D-SF12 materialise-from-state**:
no outbox, no command, no new `LocalOp` lane.

### D-ES9 — Wiring

`spawn_files_sync_actor` gains `report_tx: mpsc::Sender<UserDataReport>`
(threaded into `SyncContext`); `files_sync_spawn` gains the engine's
published `Arc<ArcSwap<AppState>>` and the sender and passes
`Arc::new(PublishedJournal::new(shared))`. Mode-agnostic as before
(D-SF18): a kiosk imports and publishes journals too.

## 4. How the binding contract (v0.2 plan §3) is honoured, line by line

| Contract line | Honoured by |
|---|---|
| §3.1 local-only home: entity + sqlite table, **no appdata sidecar section, no board attachment, no new sync lane**; nothing journal-shaped enqueues `UpdateAppData` | The journal rides the files substrate's own `appdata.json`, never the Deck card sidecar; D-ES6 pins outbox byte-identity and the only action kind; D-ES8 derives from state, no `LocalOp`; the new mpsc lane is an *engine ingest* lane of a non-board actor, not a sync lane in the outbox sense (no queue, no replay, no ops) |
| §3.2 `BTreeMap` home keyed by roaming-family v7 UUID | Wire key = `ReflectionId` text; decode re-mints it via its parse (bad key → `RowRejection::BadKey`) |
| §3.2 embedded per-entry mutation clock | Wire `clock` ≡ `updated_at`, both directions (D-ES7); the guard compares exactly it (D-ES4) |
| §3.2 deletes are tombstone-shaped rows, never silent absence | Encode emits tombstones for every `deleted` row (D-ES8 over all entries); import applies them in place (D-ES3); absent-target tombstones are not dropped from the view — they relay |
| §3.2 hand-written tolerant serde; unknown-field tolerance | `UserRecord` (chunk 1) + `ReflectionAttributes`' tolerant decode + unknown body keys ignored, pinned by a test; undecodable rows reject per row (D-ES7) |
| §3.2 A1 newtypes at the trust boundary | `ReflectionId` parse + `Rating` mint run at decode; nothing un-minted reaches `ImportedReflection` |
| §3.3 vocabulary: never shadow/overload undo `journal` names | Domain types are `ImportedReflection`/`import_reflections`/`ReflectionImport`; the word `journal` appears only as the **wire** section name and the sync-files `journal` module (a different crate, already ruled by chunk 1's `JOURNAL_SECTION`); D-ES6 pins the undo `JournalState` untouched |
| §3.4 per-entry clock LWW union across devices' files; tombstone discipline not optional | The fold (chunk 1) is the union; the engine adopts its winner with the sidecar `>=` rule (D-ES4); proptests below |

## 5. Test plan (TDD-first — the failing tests precede each commit's code)

### Commit 1 — `feat(domain): the user-data report and the reflection import fold`
Failing first (`reflection.rs` tests + proptests, domain `test_support` generators for `ImportedReflection`):
- unit: each row of the D-ES3 table (insert, newer replaces, equal-clock-different-content replaces, equal identical = no action, older skipped, tombstone in place keeps body, newer entry un-deletes, absent tombstone skipped + counted);
- proptest **idempotence**: `import(import(s, r), r)` second pass yields zero actions;
- proptest **order-free**: for a report with unique ids, any permutation gives the same state;
- proptest **monotone clocks**: after import, every row's `updated_at` ≥ its prior value;
- proptest **actions mirror state**: applying `actions` to `PersistedState` equals the resulting home.

### Commit 2 — `feat(sync-files): the journal wire codec`
Failing first: proptest roundtrip `decode(encode(e)) == Entry(e)` for live entries and `Tombstone{id, updated_at}` for deleted ones (over the domain's entry strategy); the encoded body has no `id`/`updated_at`/`deleted` keys (structural); unknown body key tolerated; out-of-range rating → `BadBody`; non-UUID key → `BadKey`; `Verbatim` → `Verbatim`; garbage-row proptest: decode never panics and returns *some* typed outcome (floor, no ceiling); `journal_section` covers every entry incl. tombstones.

### Commit 3 — `feat(state): the user-data lane and its engine arm`
Failing first (`tests/engine.rs` + a new `tests/snapshots.rs` scenario family; harness gains the lane):
- **insta journeys** (`persisted_state` + `published_tail`): `u1_import_into_empty_home`, `u2_import_newer_edit`, `u3_import_tombstone_in_place`, `u4_stale_row_skipped_after_local_edit` (local `UpdateReflection` at T2, report row at T1);
- **no-board-ops pin** (`engine.rs`, structural): outbox equal, undo `JournalState` equal, board/sync-status state equal, persistence batch kinds ⊆ {`UpsertReflectionEntry`};
- no-op report ⇒ no publish (the published `Arc` pointer is unchanged — `Arc::ptr_eq`) and no persistence write;
- closed user-data lane ⇒ the loop keeps serving commands and `SyncReport`s.

### Commit 4 — `feat(sync-files): the state-derived local document and the import step`
Failing first (actor tests over `FakeFilesTransport`, `tokio::time::pause`, a hand-built `ArcSwap<AppState>` + a receiver standing in for the engine):
- attach with a peer's journal ⇒ exactly one `UserDataReport` **received before** the own `appdata.json` create-PUT is recorded (order asserted via a shared sequence: the fake's request log length captured at report receipt);
- steady state after the stand-in applies the report to the `ArcSwap` ⇒ next cycle: no report, no PUT;
- a peer tombstone for an id absent locally ⇒ one report, then none on later cycles (fingerprint dedupe);
- a peer row with a bad body ⇒ not reported, counted, still relayed in the published document;
- local edit between snapshot and report ⇒ covered at the engine (commit 3, u4) — not duplicated here (one behavior, lowest layer);
- `PublishedJournal::document` from a state with a tombstone emits a wire tombstone.

### Commit 5 — `feat(app): the files-sync engine wiring`
Bootstrap passes the receiver to the engine and `PublishedJournal` + sender to the files actor; existing bootstrap tests extended only for routing (the lane exists and the substrate-off path spawns nothing and drops the sender).

### Commit 6 — `test(app): the two-engine journal convergence journey`
`crates/taskboard-app/tests/files_sync_convergence.rs` (dev-dep `taskboard-sync-files-nextcloud/test-support`): **two full engines** (in-memory repo fakes, each with its own `FrozenClock`) + **two files actors** over **one shared** `FakeFilesTransport` (verify it is `Clone`-over-shared-state; if not, add that in this commit — the D-FT12 fake is a model, sharing is its purpose); `tokio::time::pause`, cycles driven by `advance`, terminal state polled with a deadline (no sleeps):
1. A records entry X at T1, B records Y at T1 → both converge to {X, Y};
2. B edits X at T3 (after receiving it) → A converges to B's text;
3. A deletes Y at T4 → B's Y is tombstoned in place;
4. concurrent equal-clock edits of X on A and B at T5 → both hold the **same** winner (whichever the totaliser picks — assert equality, not which one);
5. terminal assertion: both engines' `reflection` homes equal; both published `appdata.json` contents hash-equal (`StateHash`); and on a further quiescent cycle the fake records **zero PUTs** (convergence is stable, not oscillating).

### Commit 7 — `docs(changelog): the user-data engine seam and journal import` + gates green
Full AGENTS §10 suite through `scripts/run-quality-gates.sh`.

### Review pins (stage 3)
(a) `SyncReport` and the board pipeline diff-free; (b) guard is `>=` everywhere in the import, and the reason (D-ES4) is in the fn doc; (c) no ghost rows; (d) no `UpdateAppData`/`LocalOp`/undo touch; (e) report emitted before the own publish at attach; (f) no clock reads in the import path; (g) no text assertions; (h) the convergence journey asserts equality of homes *and* published hashes *and* zero steady-state PUTs.

## 6. Risks

- **Engine spawn-signature churn**: the new receiver touches every
  `spawn_state_engine` call site (bootstrap + harnesses). Mechanical;
  if the count is large, introduce an `EngineLanes` input struct in
  commit 3 rather than a positional argument.
- **Tie-content totaliser vs. entity equality**: the fold's tie-break
  compares wire JSON; the engine's "identical" check compares
  `ReflectionEntry`. A body field the wire carries but the entity
  drops (unknown key from a newer build) makes two rows wire-different
  but entity-equal ⇒ the engine skips, the published view carries the
  newer-build row, hashes still agree (both devices publish the same
  merged view). Acceptable; noted in D-ES7's doc.
- **Undecodable-but-winning rows** (newer build's rating 6): the
  engine keeps its older row while the device relays the newer one —
  local display lags until upgrade. By design (strict ratings, DR-7);
  surfaced via the rejection counter in chunk 4.
- **Absent-target tombstones** are never materialised locally; if the
  tombstoned entry later arrives from a stale offline peer, the fold
  still ranks the tombstone higher (it remains in some peer doc / the
  adopted seed), so no resurrection. After *all* holders of the
  tombstone vanish, an older entry could reappear — the same bound as
  every current-state design; B-full's snapshot floor is the fix.
- **Precision**: sub-second clocks must survive storage and wire
  unchanged or the steady-state "no PUT" fails; storage uses
  `to_rfc3339()` (preserved) and the commit-2 roundtrip + commit-6
  quiescence assertion catch any drift.
- **`taskboard-sync-files` gaining a domain dep** widens the pure
  core's compile graph slightly; alternative (codec in the nextcloud
  crate) recorded and rejected for successor reuse.

## 7. Estimated diff size

~1,300–1,700 lines (M, matching the dispatch): domain report + import
fold ≈ 250 + tests ≈ 300; codec ≈ 150 + tests ≈ 200; engine arm +
lane ≈ 120 + harness/journeys ≈ 250 (+ insta snaps ≈ 200); actor
step + `PublishedJournal` ≈ 150 + tests ≈ 250; bootstrap ≈ 40;
convergence journey ≈ 250.

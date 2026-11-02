# [Plan] Tracker task kind (#492, backlog round 2 chunk 4.2) — stage-1 implementation plan

- **Date**: 2026-11-02 (written into the mandated
  `2026-10-31-tracker-task-kind-plan.md` slot of the round-2 dispatch
  table).
- **Kind**: stage-1 TDD implementation plan for chunk **4.2** of the
  backlog round-2 dispatch. Zero production code in this commit; this
  file is the chunk's entire planning artefact. Implementation is
  stage 2's, review + shipping stage 3's.
- **Origin**: the round-2 dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` (batch
  4, chunk 4.2, branch `feat/tracker-task-kind`); the normative fence
  issue **#492** — *"a never-\"done\" `TaskKind`/flag for tasks like
  'Reading as Hobby' or 'Meditation' that exist to be tracked, not
  completed. Once time entries exist (DR-5 chunks 1–2), this is
  additive field/flag work: the kind suppresses done-affordances and
  done-triggered cascades while keeping tracking fully live. The DR-5
  entity shape reserved the vocabulary"*; the accepted DR-5 design
  round `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  — §7's vocabulary reservation is the binding fence text: *"a
  `kind`/flags field and a `user` field fold onto `TimeEntry` and its
  wire row as default-carrying additions — nothing in v1's schema or
  wire shape precludes either"*, plus the fence table's row naming
  this exact issue. **Scope ruling**: #492 owns the **task-side**
  kind; the **entry-side** kind (`TimeEntryKind`, respite) is chunk
  3.5's, already landed (PR #587). This chunk rides the entry-kind's
  structural precedent everywhere one exists.
- **Normative base**: branch point = `origin/main` @ `9f3508ee`
  (local `main` fast-forwarded to it before the worktree was cut —
  it was ~20 commits behind, which would have silently dropped the
  3.5 precedent). **Chunk gate verified** (2026-11-02): chunk 3.5's
  PR **#587** (`feat/respite-break-reminders`, body *"Closes #493.
  Closes #495."*) is **MERGED** (2026-11-02T21:26:44Z) and its commits
  (`9c0f6391`…`9f3508ee`) are in `origin/main`'s log. Worktree created
  per AGENTS §8 after a passing `scripts/worktree-doctor.sh`
  (exit 0, 222G free / 25G floor).
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§8, D-1…D-18). The three findings
  this planning pass contributes beyond the fence issue are the
  **storage correction** (D-7: the dispatch's "no schema migration
  expected" lane is *refuted* for the task side exactly as 3.5's D-12
  refuted it for the entry side — the `tasks` table is strictly
  columnar, so the kind needs additive `ALTER TABLE`s; the sidecar
  wire fold needs none), the **cascade-enumeration finding** (D-5: the
  done-triggered paths are all *downstream of the plan assembly*, so
  one early typed rejection makes every cascade inert by construction
  — the enumeration still pins each path, but the mechanism is
  ordering, not five separate guards), and the **kiosk finding**
  (D-11: kiosk is passive *by intent, not enforcement* — its board
  cards are the same `TaskCard` with a live done checkbox and its
  keymap is installed unconditionally, so model-side suppression plus
  the router guard cover kiosk with zero kiosk-specific work; the
  engine rejection is the hard backstop).

## 1. Mission & binding constraints

One feature, four layers:

1. **The kind (domain)**: a `TaskKind` datum on tasks whose
   `Tracker` value marks a task as *tracked, not completed*. It roams
   (the sidecar tasks section grows an additive modeled key,
   default-off), it persists (additive sqlite columns), and it is
   edited at the surfaces.
2. **The rejection (engine)**: `SetTaskDone { done: true }` on a
   tracker task is impossible via the public API — a typed
   `CommandError`, asserted with `matches!`, no text. This is the
   acceptance criterion every other suppression leans on.
3. **The suppression (surfaces)**: CLI shows the typed rejection;
   desktop UI *hides* done affordances per surface and offers the
   kind toggle; kiosk inherits both through shared code paths.
4. **The liveness pin (tracking)**: `StartTracking`/`StopTracking`
   and the whole tracked-time surface are untouched on tracker tasks —
   that is the feature's point.

Binding constraints:

- **The DR-5 reservation**: additive, default-carrying folds only.
  Old serialized tasks (state snapshots, journal payloads, sqlite
  rows, sidecar documents) decode as normal (`Standard`) tasks —
  round-trip pinned at every layer. Unknown wire values demote,
  never fail the document (D-S2's materialization rule).
- **The dispatch's hard rules** (chunk 4.2.b): tracking verbs
  unaffected (pinned); the engine rejection typed and `matches!`-ed;
  per-surface suppression per this plan; **migration discipline per
  §1 lane 4** — D-7 records the refutation and the claim procedure
  (next free number **0019** at stage 3's rebase, single-flight).
- **AGENTS §5**: no text assertions; typed outcomes; one behavior one
  test at the lowest layer (domain owns the rejection and the fold;
  state/app/ui own *routing* and *presentation* pins only);
  deterministic; no quality ceilings.
- **AGENTS §2**: `taskboard-ui-slint` stays presentation-only — the
  UI *hides* affordances and dispatches existing commands; it never
  re-legislates the domain rule (the engine owns it).
- **Chunk 3.5's precedent is binding style**: `TimeEntryKind`
  (deliberately an enum, not a bool), the wire default-on-missing
  fold with no silent downgrade, migration 0018's DEFAULT-backfill
  comment conventions. Where this plan cites "the 3.5 recipe" it
  means those landed hunks.

## 2. The seams this plan consumes (all read at `9f3508ee`)

Layer → file:line facts the plan builds on (verify nothing, trust the
base):

- **Entity family**: `Task` (entities.rs:491-539) and its per-field
  `TaskClocks` (entities.rs:280-305); the mergeable-field manifest
  `TASK_FIELDS` (entities.rs:260-273) — "the single source the
  lockstep assertions pin every structure against (#244)"; the epoch
  helper `BoardClocks::epoch()` (entities.rs:401-406, reused by the
  storage clock columns per repo.rs:1542).
- **The done verb and its cascades** (command.rs:1012-1106, the
  `SetTaskDone` arm): idempotence no-op → recur completion spawn or
  bound-suppressed rule clear (D-E1, command.rs:1028-1044) → tracking
  closes at the done stamp (D-T6, `task_tracking_closes`,
  command.rs:1065) → active-occurrence reminder dismissals
  (D-T5/R3.5.5, command.rs:1071-1094) → per-block
  `UpdateAppData` flush-trigger enqueues (command.rs:1066-1103) —
  all assembled into one `CommandPlan` (`applied_task`,
  command.rs:1045). `UpdateTask` (command.rs:940-1010) carries the
  app-lane routing term `app_edit` (command.rs:995-999).
- **The sidecar wire** (`appdata.rs`): the modeled-key manifest
  `MODELED_TASK_FIELD_KEYS` (appdata.rs:43-44, five keys); the
  key-directed `{v, clock}` envelope (`TaskFieldWrite`,
  appdata.rs:201-214); `ModeledValue` + `modeled_key`/`decode_modeled`
  /`encode_modeled` (appdata.rs:221-320) — an unknown *value* under a
  modeled key demotes that field to verbatim, never fails the
  document (appdata.rs:274-320); an unknown *key* demotes likewise
  (appdata.rs:170-186); the overlay builder `task_entry_for`
  (appdata.rs:1284-1335 — effort/importance/urgency emitted always,
  `recur` always, `scheduled` flag-routed); the lockstep pin
  `modeled_keys_are_the_appdata_rows_plus_scheduled`
  (appdata.rs:2325-2381, pins the key set against `TASK_FIELDS` and
  the count `== 5`).
- **The pull fold** (`merge.rs`): `fold_task_app_data`
  (merge.rs:605-711) — per-key guard/doc_wins/clock tuple, guard
  copy-over, LWW adoption. **Merge's native `done` adoption is
  unconditional data truth** (`merged.done = remote.done`,
  merge.rs:485) and fires no cascades — the merge path never plans
  commands.
- **The pending-intent guard** (`pipeline.rs`): `PendingTaskIntent`
  (pipeline.rs:99-116) + the collector's coarse `UpdateAppData` arm
  setting every app flag at once (pipeline.rs:118-170).
- **Undo/journal**: `UndoField` (journal.rs:220-244) and the two
  per-field mirror sites — the watermark content array
  (undo.rs:811-826) and the inverse `TaskChanges` synthesis
  (undo.rs:1320-1360); the journal replans inverse commands through
  `plan_command` (journal.rs module doc), so an inverse that would
  re-complete a tracker task hits the rejection typed.
- **Storage**: the columnar `tasks` table (0001_init.sql:40-62; the
  field-family migrations 0007/0010 are the additive-column recipe;
  0018_respite_break_rules.sql is the DEFAULT-backfill comment
  recipe); `codec.rs` text forms (codec.rs:101-135); the row struct +
  INSERT (repo.rs:90-105, repo.rs:1478) and the decode arms
  (repo.rs:346, repo.rs:443-496).
- **App surfaces**: `TasksCmd::Done` (cli.rs:1367-1373) routing into
  `SetTaskDone` (commands/tasks.rs:283-308); `TaskEditFlags`
  (cli.rs:1311-1346, the #407 at-least-one-flag group); the kebab
  error-kind tables `domain_error_kind` (commands/mod.rs:604-631) and
  `undo_field_kind` (commands/mod.rs:647-665) — both exhaustive
  matches a new variant/field *forces* arms into; the envelope
  reporter (commands/mod.rs:2212-2227). IPC relays `TaskDone`
  (commands/mod.rs:184) to the daemon, which executes the same
  engine path.
- **UI surfaces** (surveyed exhaustively; the numbers are the
  Explore pass's): one shared checkbox widget `done-box`
  (task-card.slint:123-170) consumed by board regular/compact legs,
  today rows, stack detail — no visibility gate exists today; the
  task-view header checkbox (task-view.slint:190-262); the
  notification row's "Mark task done" gated by a per-row model flag
  `can-done` (notification-surface.slint:22, 71-77; models.rs:650) —
  **the codebase's hide-by-flag precedent**; six `SetTaskDone`
  dispatch sites (four checkbox seams + notification + the registry
  route `TaskComplete`/`TaskReopen`, router.rs:376-379/547-552, which
  is how Space/Ctrl+Space/U reach the engine with no widget
  involved); context menus built from `TASK_CONTEXT`
  (menu.rs:164-186) with **empty states maps on object menus**
  (menu.rs:212-215 — nothing greies a card's menu today) and the
  hide precedents `preset_irrelevant` (menu.rs:41-46) and anchor-task
  filtering; decorations `TaskDecoration`/`DecorationKind`
  (task-card.slint:14-31, built in models.rs:1591-1641) — the Recur
  badge (models.rs:1622-1638) is the badge precedent; tracking
  affordances (`can-track-start/stop` task-view buttons,
  status-bar chip, card `Tracking` decoration) are all separate
  machinery this chunk must not disturb; kiosk pins the same
  AppWindow to Board/Today (adapter.rs:3106-3114) with ambient-only
  gates — the card checkbox and keymap are live (adapter.rs:2414-
  2417 "passive by intent"), so suppression must live in shared code.
- **State snapshots**: the persisted-state snapshots serialize
  `AppState` whole — chunk 3.5's identical fold updated all 52
  snapshot files with one line each; the same mechanical update
  rides this chunk's domain commit.

## 3. Domain shape — D-1…D-3

### D-1 — a kind enum, not a bool flag; no newtype, no fallible mint

```rust
/// What a task exists *for*: `Standard` is the completable default;
/// `Tracker` marks a task that exists to be tracked (issue #492) —
/// never "done" (the engine rejects the verb), with tracking fully
/// live. Deliberately an enum, not a bool: the DR-5 §7 reservation
/// names *kind*s (plural, forward-extensible) and a second task-side
/// bool would fork the fold/wire/storage plumbing per flag — the
/// `TimeEntryKind` ruling verbatim (tracking.rs:63-68).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    /// An ordinary, completable card — the default and the
    /// overwhelming case.
    #[default]
    Standard,
    /// A tracked, never-completed card. The marker is data plus one
    /// engine gate (the `SetTaskDone` rejection); every other verb —
    /// edit, move, archive, delete, track, remind — is unchanged.
    Tracker,
}
```

Lives in `entities.rs` beside `Task` (it is a task field, unlike
`TimeEntryKind` which lives in `tracking.rs` beside `TimeEntry`).
Same helper pair as the entry kind: `parse_text_form(&str) ->
Option<Self>` / `const fn text_form(self) -> &'static str`
("standard"/"tracker") — the storage codec's and the clap value
parser's single seam (tracking.rs:87-102 recipe).

**A1 ruling**: no fallible mint, no private-field newtype — a closed
two-arm enum carries no invariant to guard; the *leniency* is
explicit and at the edges (an unknown CLI text is a boundary parse
rejection, never a domain error variant; an unknown wire text demotes
the field to unmodeled, D-3; an unknown storage text is `Corrupted`,
D-7 — local rows are trusted-written, the 0018 comment's ruling).
`Task.kind` gets `#[serde(default)]` so old snapshot/journal JSON
decodes `Standard` (the `TimeEntry.kind` precedent, tracking.rs:59).

### D-2 — the field family: one manifest row, one clock slot, AppData lane

- `Task` gains `pub kind: TaskKind` (documented: never-done under the
  engine's gate; merge may adopt a remote done — D-6 records why that
  is data truth, not a violation).
- `TaskClocks` gains `pub kind: DateTime<Utc>` (the per-field LWW
  slot; local edits stamp it, remote adoption stamps it).
- `TASK_FIELDS` gains `("kind", &["kind"], FieldFamily::AppData)` —
  **AppData, never Native**: Deck has no task-kind card field (the
  connector cannot carry it), and the fold/wire/storage machinery of
  the AppData lane (effort/importance/urgency/recur) is exactly the
  additive reservation DR-5 made. No `RemoteTaskBaseline` change (the
  doc already excludes the sidecar lane, entities.rs:333-336).
- Everything the manifest lockstep drives updates mechanically: the
  appdata modeled-key pin (D-3), `PendingTaskIntent.kind`
  (pipeline.rs:99-116 + the coarse collector arm pipeline.rs:139-146),
  `fold_task_app_data` (D-3), `UpdateTask`'s clock-and-route block
  and `app_edit` term (command.rs:963-999), the undo mirrors (D-4),
  the state/test-support task factories (`kind: TaskKind::default()`
  or explicit `Standard` at every literal construction — the compiler
  enumerates them).

### D-3 — the wire fold: the sixth modeled key, default-off both directions

Mirrors the landed entry-kind recipe (`TimeEntryRecord::Entry.kind`,
appdata.rs:610-730) translated to the task field's envelope:

- `ModeledValue::Kind(Option<TaskKind>)` — the `Option` keeps the
  envelope's clear convention uniform (`null` = an ordinary write
  resetting the task to `Standard`, never a tombstone).
- `modeled_key`/`encode_modeled`/`decode_modeled` gain the `"kind"`
  arm; `MODELED_TASK_FIELD_KEYS` becomes the six-key list. Decode:
  absent key → the field is simply never written (fold leaves the
  local value — default `Standard` for pre-kind tasks); `null` →
  `Kind(None)` (reset); a valid unit name → the kind; **an unknown
  text demotes the field to unmodeled** (appdata.rs:274-320's
  mechanism, verbatim) — never a silent downgrade, never a document
  failure. An old reader receiving a `kind` key demotes it to
  unmodeled and carries it verbatim (appdata.rs:170-186) — the
  old-reader-safety half of the reservation.
- `task_entry_for` (appdata.rs:1284-1335) emits the kind write
  **always**, like `recur` — the kind has no native lane ever, so the
  emission is unconditional (simpler than `scheduled`'s D-V2
  flag-routing; there is no `DeckFeatures` term at all).
- `fold_task_app_data` (merge.rs:605-711) gains the `"kind"` row in
  all three places: the guard tuple (`intent.kind`,
  `local.clocks.kind`), the guard copy-over arm, the adoption arm.
- `sync-nextcloud`'s flush gates gain the kind term:
  `task_has_app_data` (flush.rs:578-590) and the board-level
  any-gate (flush.rs:135-141) both add
  `task.kind != TaskKind::default() || task.clocks.kind !=
  BoardClocks::epoch()` — value presence cannot discriminate (the
  kind is total), so the clock term carries the explicit
  reset-to-standard write, the same role the epoch columns play at
  rest. **No `mapping.rs` change** (no native lane), **no new push
  section** (the tasks section already exists).

## 4. The engine rejection and the cascade enumeration — D-4…D-6

### D-4 — the typed rejection: one guard, before the plan exists

`CommandError` gains:

```rust
/// The target is a tracker task (issue #492): completing it is not
/// a state transition the kind admits — the verb is rejected typed
/// at the boundary, before any plan (and therefore any cascade)
/// exists. Reopening (`done: false`) is untouched: a tracker task
/// is never done, so it idempotence-no-ops exactly as today.
#[error("tracker tasks cannot be done")]
TrackerTaskNotDoneable(TaskId),
```

Placement: in the `SetTaskDone` arm immediately after
`live_task(app, *id)?` (command.rs:1013), **conditioned on
`*done`**:

```rust
if *done && task.kind == TaskKind::Tracker {
    return Err(CommandError::TrackerTaskNotDoneable(*id));
}
```

`done: false` on a tracker task falls through to the idempotence
no-op (command.rs:1014-1016) — a reopen intent against an
always-open task is a no-op, not an error. **The engine owns the
rule; no surface re-legislates it** — the UI guards (D-9) are
affordance hiding for UX, not correctness, and every one of them may
be wrong without breaking the contract.

### D-5 — the cascade non-firing enumeration (the review trap)

Every path `SetTaskDone { done: true }` triggers today on a normal
task, and what makes it inert for a tracker task:

| # | Done-triggered path (3.5/base naming) | Site | Inertness mechanism | Pin |
|---|---|---|---|---|
| 1 | `done` stamp + `clocks.done` write | command.rs:1017-1019 | never reached — the guard returns before `updated` exists | rejection test asserts `Err` **and empty plan** |
| 2 | recur completion spawn / bound-suppressed rule clear (D-E1) | command.rs:1028-1044 | downstream of the guard; a tracker+recur task cannot reach it | covered by the same empty-plan pin (a tracker+recur fixture asserted too) |
| 3 | tracking closes at the done stamp (D-T6) — open entries close | command.rs:1057-1070 | downstream of the guard; **tracker tasks keep entries open forever — that is the feature** | the liveness pin: a tracker task with a running entry + the rejection ⇒ the entry is still open afterwards |
| 4 | active-occurrence reminder dismissals (D-T5/R3.5.5) | command.rs:1071-1094 | downstream of the guard | the empty-plan pin (no `RecordReminderDismissal` actions on a tracker with a live rule fixture) |
| 5 | `UpdateAppData` flush-trigger enqueues (closes/dismissals/recur-clear) | command.rs:1066-1103 | downstream of the guard | the empty-plan pin (no outbox actions at all) |
| 6 | the persistence upsert (`UpsertTask`) | `applied_task`, command.rs:1045 | downstream of the guard | the empty-plan pin |
| 7 | the undo journal entry | journal layer, on the executed plan | `plan_command` never returns a plan; the engine journals plans, not intents | the no-journal pin: `Undo` after the rejected attempt ⇒ `NothingToUndo` |
| 8 | presentation fall-out (today membership, done tinting, section removal) | derived from `task.done` at render | `done` is never set; the projections never see it | not separately pinned (pure derivation of an unset field — testing it would duplicate #1) |

The mechanism is **ordering, not five guards**: the rejection returns
`Err` before `CommandPlan` assembly, and every cascade above lives in
the plan. The enumeration's job in tests is to *prove the ordering
holds per path* with one fixture that maximizes the blast radius
(tracker + recur rule + running entry + due reminder rule) and one
assertion set over the empty plan.

**Undo's inverse path is the one cascade-shaped route that reaches
`plan_command` with `done: true` after conversion** (done → reopen →
convert to tracker → undo): the inverse replans through
`plan_command` (journal.rs module doc) and hits the guard typed. That
is the decided semantics (D-4's ruling applied uniformly): the undo
refuses with `TrackerTaskNotDoneable`, the line keeps its head. Pinned
at the undo layer (D-4's ruling made uniform beats special-casing
`Undo`).

### D-6 — merge posture: remote `done` adoption is data truth

A pre-kind peer (or Deck's own web UI) can set the native `done` on
the card; the pull fold adopts it unconditionally
(merge.rs:485) and **fires no cascades** — the merge path never plans
commands, so paths #2–#7 do not exist there. Decision: **keep
adoption**. Refusing it would leave local-open/remote-done divergence
baked into `remote_baseline` (the difference stops being re-diffable)
— permanent skew is worse than a completed card. The kind's contract
is precisely issue #492's wording: *affordance and cascade
suppression on this device*; cross-version skew is tolerated by the
same old-writer tolerance the DR-5 reservation builds on (nothing
promises old writers honor new semantics). Recorded as deliberate
leniency on the `Task.kind` doc comment. No pin (it is today's
behavior on every task; a tracker-specific pin would pin a limitation
as a contract — AGENTS §5's "no pins of today's behavior" reading).

## 5. Undo/journal coverage — D-7

- `UndoField` gains `Kind` (journal.rs:220-244, after `Recur`,
  before `Archived` — declaration order is the watermark's `Ord`,
  journal.rs:193's "new kinds never reshuffle" rule).
- The watermark content array (undo.rs:811-826) gains
  `(clocks.kind, current.kind, UndoField::Kind)`; the inverse
  synthesis (undo.rs:1320-1360) gains the kind row
  (`pre.kind != current.kind` → `check(...)` +
  `changes.kind = Some(pre.kind)`). The journal pre-image is the
  whole `Task` (boxed), so `pre.kind` exists automatically; old
  journal payloads decode via `#[serde(default)]`.
- `app`'s `undo_field_kind` (commands/mod.rs:647-665) gains `"kind"`
  (compiler-forced).

## 6. Storage — D-8 (the dispatch's no-migration expectation, refuted)

### D-8 — migration 0019 (the 0018 recipe, task-table edition)

The dispatch's §1 lane 4 says no chunk is *expected* to carry a
migration; the expectation is **refuted** exactly as 3.5's D-12
refuted its twin: `tasks` is strictly columnar (0001_init.sql:40-62)
with no spare column, and every prior AppData-lane task field took
the additive-column path (0007, 0010). The sidecar wire fold (D-3)
needs none; the local row does.

```sql
-- 0019_tracker_task_kind: the tracker task kind (backlog round 2,
-- chunk 4.2, issue #492; design D-1). Additive only (ADR 0005), the
-- 0018 recipe verbatim over the 0007 column pair.
--
-- The kind column's DEFAULT 'standard' backfill IS the default-off
-- decode of pre-0019 local data: every existing row reads `Standard`.
-- Stored TEXT is the domain serde unit name; unknown text decodes
-- Corrupted at the codec (local rows are trusted-written — a corrupt
-- enum is a corrupted database, no CHECK re-legislates the enum).
-- The clock column is epoch-defaulted NOT NULL (0007's mechanism) so
-- pre-migration rows read as "never locally written" — every remote
-- stamp / document clock wins bit-for-bit.
ALTER TABLE tasks ADD COLUMN kind    TEXT NOT NULL DEFAULT 'standard';
ALTER TABLE tasks ADD COLUMN ck_kind TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
```

- `codec.rs`: `task_kind_to_text` / `task_kind_from_text` (unit
  names; strict decode — `None` ⇒ `Corrupted` at the caller), the
  `time_entry_kind_*` twins' shape (codec.rs:101-111).
- `repo.rs`: the row struct gains `kind: String, ck_kind: String`
  (repo.rs:90-105's shape), the INSERT gains both columns
  (repo.rs:1478), the read mapping gains
  `kind: decode::kind(...)` + the clock fallback
  `BoardClocks::epoch()` (repo.rs:443-496, 1542's pattern).
- **Numbering claim**: 0019 is the next free number at this chunk's
  base (migrations end at `0018_respite_break_rules`). **Single-flight
  watch item**: sibling 4.3 (`fix/sidecar-cross-board-repair`) is in
  flight right now (its worktree exists with uncommitted changes) and
  is the only plausible sibling carrier — the dispatch's rule is the
  number is *taken at stage 3's rebase*, so stage 3 re-checks
  `migrations/` on the post-rebase base before opening the PR; if
  0019 is taken there, renumber to the next free. Recorded in §12.

## 7. Surfaces — D-9…D-14

### D-9 — CLI: the typed rejection IS the suppression; `--kind` on edit

- **No local pre-check in `tasks done`** (cli.rs:1367-1373,
  commands/tasks.rs:283-308): the verb forwards, the engine rejects,
  the error path renders — one authority. The typed variant flows
  through `domain_error_kind`'s new arm
  (`D::TrackerTaskNotDoneable(_) => "tracker-not-doneable"`,
  commands/mod.rs:604-631, compiler-forced) into the `--json`
  envelope's `kind` field (commands/mod.rs:2212-2227) — a script
  branches on the kebab discriminator, never prose (ADR 0019's typed
  surface convention). IPC: the daemon executes the same path; the
  rejection rides the existing typed-error transport that carries
  `UnknownTask` today (verified in stage 2 — §12 R-4).
- **`TaskEditFlags.kind: Option<TaskKindSpec>`**
  (cli.rs:1311-1346): clap value parser delegating to
  `TaskKind::parse_text_form` (`standard|tracker`; unknown text is a
  clap parse rejection, the 3.5 recipe). Routes into
  `TaskChanges.kind = Some(Some(kind))`.
- **No `Add` flag, no `list` column** — recorded decisions: app-lane
  fields are edit-only at this CLI (effort/importance/urgency/recur
  all are; `CreateTask`'s payload is `{title, stack, order}`,
  messages.rs:23-30), and list marking is presentation sugar this
  chunk defers (the typed error text names the reason on failure;
  discoverability rides the UI badge, D-10). Deferred sugar is a
  candidate follow-up issue, not silent scope (§12).

### D-10 — desktop UI: hide, don't grey; badge, don't re-label

Suppression is **model-flag hiding** (the notification `can-done`
precedent) plus **one router guard** (the keyboard path has no
widget) plus a **menu hide-set**; nothing greys, nothing disables,
no dead affordances remain on any tracker card:

1. **`TaskCardModel`** (task-card.slint:35-52, built in
   `task_card_model` models.rs:1648-1664) gains
   `can-done: bool`; `done-box`'s visibility gates on it
   (task-card.slint:123-170 — the widget is shared, so board
   regular/compact, today rows and stack detail all suppress at
   once). Default `true`; every non-tracker path is byte-identical.
2. **`TaskDetailModel`** (task-view.slint:18-58, pushed
   models.rs:1281-1430) gains the same flag gating the header
   checkbox (task-view.slint:190-262) and its focus/touch scopes;
   the "Completed/Open" status text and done-tinted due chip stay
   data-driven (a tracker task *is* open — they render "Open"
   exactly as today).
3. **Router guard**: `ActionId::TaskComplete`/`TaskReopen`
   (router.rs:376-379, 547-552) check the focused task's kind and
   decline (`NoTarget`-style inert dispatch) when the verb would
   complete a tracker — the only route with no widget between key
   press and engine. `TaskReopen` keeps its existing
   `focused_task_done` gate (router.rs:547-552); the kind check
   only arms the complete side.
4. **Notification rows**: `awaiting_row_buttons`
   (presentation.rs:486-495, pushed models.rs:650) sets
   `can-done = false` for a tracker task's rows — the existing
   per-row flag, one condition.
5. **Context menu**: `open_task_context_menu`
   (adapter.rs:3779-3781) passes a small hide-set into
   `menu::object_entries` (menu.rs:217-222) — for a tracker task it
   omits `TaskComplete`; `TaskReopen` is already never useful and
   stays (dispatch-time no-op semantics unchanged — scope
   discipline: only the impossible verb is hidden). Object menus
   take their first per-task parameter; the empty-states-map pin
   (menu.rs:212-215) gets its doc note updated.
6. **The mark affordance**: one new `ActionId::TaskToggleTracker`
   appended to `TASK_CONTEXT` (menu.rs:164-186) with a registry
   entry modeled on the tracking arms (actions.rs:938-961:
   GLOBAL contexts, `MenuGroup::Task`, rank after the tracking pair,
   no preset binding — pointer-driven, menu-reachable). Dispatch
   reads the task's kind and sends
   `UpdateTask { changes: TaskChanges { kind: Some(Some(inverted)),
   ..Default::default() } }` — **no new domain command** (D-2's
   lanes carry it). The menu item hides for tombstoned tasks the
   same way every object-menu item already only targets live tasks.
   `keymap_snapshots.rs` updates (a registry dump gains a row).
7. **The badge**: `DecorationKind::Tracker` (task-card.slint:14-31)
   built in `field_decorations` (models.rs:1591-1641), visible
   whenever `task.kind == Tracker` — the Recur badge's exact
   mechanism (models.rs:1622-1638), and the kiosk-visible name of
   the kind (D-11). `presentation.rs` owns the pure predicate
   (`tracker_badge_visible`), the Slint file owns the glyph only.
8. **Tracking stays untouched**: `can-track-start/stop`
   (models.rs:1392-1393), the status-bar chip, the card
   `Tracking` decoration, the notification "Start task" button —
   zero hunks; the liveness pin (§9, UI-2) proves it.

**No golden changes**: every fixture task stays `Standard`, so every
board/today/kiosk/task-view golden is byte-identical (the checkbox
visibility flag defaults true; the badge appears only on tracker
fixtures that don't exist). The badge's pin is the decoration-list
model test (§9 UI-3), not pixels. **No new ActionId bindings** ⇒ the
standard/vi presets are untouched except through the registry dump
snapshot.

### D-11 — kiosk: zero dedicated hunks (the finding)

Kiosk renders the same `TaskCard`s pinned to Board/Today
(adapter.rs:3106-3114) — the done-box visibility flag (D-10.1)
suppresses there by construction; the context-menu hide-set and the
router guard are kiosk-agnostic (the keymap is installed
unconditionally, so the router guard is what keeps Space inert on a
kiosk box with a plugged-in keyboard); the badge renders ambiently.
The engine rejection remains the backstop for any path this survey
missed. **No `!kiosk` terms are added** — kiosk work is zero by
design, and one flow-table pin (§9 UI-6) proves the shared path in
kiosk mode (the seconds-gate test's shape, flow_table_tests.rs
:1300-1345).

## 8. Decision record

| # | Question | Decision | Status |
|---|---|---|---|
| D-1 | Enum vs bool flag; validated newtype? | `TaskKind { Standard, Tracker }` kebab enum (the `TimeEntryKind` ruling verbatim); no fallible mint — closed two-arm enum, no invariant; leniency at the edges (clap parse / wire demote / storage `Corrupted`) | decided |
| D-2 | Field family / lane | AppData lane: `TASK_FIELDS` row + `TaskClocks.kind` slot + `#[serde(default)]` on `Task.kind`; never Native (Deck has no such card field); no baseline change | decided |
| D-3 | Wire shape | sixth modeled key `"kind"`, `ModeledValue::Kind(Option<TaskKind>)` (`null` = reset), emitted always (recur's rule), old readers demote verbatim, unknown value demotes the field | decided |
| D-4 | Rejection shape & placement | `CommandError::TrackerTaskNotDoneable(TaskId)`, guarded on `*done` right after `live_task`; reopen stays no-op | decided |
| D-5 | How cascades are pinned inert | ordering (rejection precedes plan assembly) + one max-blast-radius fixture pinning the empty plan per enumerated path (§4 table) | decided |
| D-6 | Merge-adopted remote `done` on a tracker | adopt (data truth); no cascades exist on the merge path; documented leniency on the kind's doc | decided |
| D-7 | Undo coverage & the re-complete-after-convert edge | `UndoField::Kind` + both mirror rows; the inverse replan hits the rejection typed — undo refuses, line keeps its head | decided |
| D-8 | Migration | **0019**, the 0018/0007 recipe verbatim; the dispatch's no-migration expectation refuted (columnar table); number taken at stage 3's rebase, 4.3 watched | decided, with the expectation correction recorded |
| D-9 | CLI posture | no local pre-check (engine-only authority); `--kind` on `tasks edit`; no `Add` flag (app-lane fields are edit-only), no `list` column (deferred sugar, §12); `--json` kind `tracker-not-doneable` | decided |
| D-10 | Desktop suppression mechanism | hide by model flag (`can-done` on both models), one router guard for the keymap, menu hide-set for `TaskComplete`, `TaskToggleTracker` menu action riding `UpdateTask`, `Tracker` decoration badge; tracking affordances untouched | decided |
| D-11 | Kiosk posture | zero dedicated hunks — shared model flags + router guard + badge cover it; one kiosk-mode pin; engine rejection is the backstop | decided |
| D-12 | Scope boundary: which verbs are suppressed | **only done**. Edit/move/archive/delete/label/remind/track all remain valid on tracker tasks (archive ≠ done; deleting a tracker is ordinary; history outlives it per ADR 0017) | decided |
| D-13 | Naming | arms `Standard`/`Tracker` ("todo" rejected: it collides with done semantics; "work" rejected: that's the entry kind's axis); error variant `TrackerTaskNotDoneable`; kebab kinds `standard`/`tracker`/`tracker-not-doneable` | decided |
| D-14 | Proptest? | none — no new pure sorting/scheduling arithmetic exists (the kind is data + one gate); the domain unit matrix + existing property suites cover the fold's LWW (whose arithmetic is already property-pinned upstream) | decided |
| D-15 | Snapshot/golden policy | 52 snapshot files update mechanically in the domain commit; goldens untouched (fixtures stay `Standard`; D-10) | decided |
| D-16 | `SetTaskDone` on an already-done tracker | impossible state by construction (D-4 blocks the entry); if merge skew produces it anyway (D-6), the existing idempotence no-op applies — no special case | decided |
| D-17 | `CaptureTask`/quick capture posture | unchanged — captures are `Standard`; converting is the edit affordance's job (D-9/D-10) | decided |
| D-18 | CHANGELOG | one Unreleased feature bullet: tracker tasks (never-done kind) with the done rejection, the edit flag, and the UI toggle | decided |

## 9. Test catalogue (TDD-first: each phase opens with its failing tests)

Lowest-layer rule applied: the rejection, the fold, the LWW and the
envelope live at domain; state/app/ui pin *routing and presentation*
only. Names are indicative; the shapes are binding.

**Domain (`taskboard-domain`)**

- D1 `set_task_done_on_tracker_is_a_typed_rejection` —
  `matches!(err, CommandError::TrackerTaskNotDoneable(_))`; plan-side:
  the engine helper returns no plan at all (the arm returns `Err`).
- D2 `the_rejection_emits_no_actions_and_no_journal_entry` — the
  max-blast-radius fixture (tracker + recur rule + running entry +
  due reminder rule): no `PersistenceAction`s of any kind; then
  `Undo` ⇒ `NothingToUndo` (cascade-table rows 1–7 in one test — one
  behavior: "the rejection precedes the plan").
- D3 `reopen_intent_on_a_tracker_is_the_accepted_no_op` —
  `SetTaskDone { done: false }` ⇒ `NoOp` outcome.
- D4 `update_task_kind_edits_stamps_and_routes_to_the_app_lane` —
  kind edit stamps `clocks.kind`, emits only the `UpdateAppData`
  enqueue (`app_edit` term), a second edit is LWW-newest.
- D5 `fold_adopts_newer_document_kind` / D6
  `fold_guard_protects_a_pending_kind_write` / D7
  `older_document_kind_loses_the_clock_race` — the
  `fold_task_app_data` triple, the effort/recur tests' shapes.
- D8 `the_overlay_emits_the_kind_write_always` —
  `task_entry_for` carries `kind` + `clocks.kind` for Standard and
  Tracker alike (recur's emission rule).
- D9 `a_pre_kind_document_folds_as_standard` — a document without the
  key never writes the field (absence is not deletion).
- D10 `an_unknown_kind_value_demotes_the_field_verbatim` — decode:
  `"kind": "bogus"` lands in unmodeled, the document never fails;
  `null` decodes the reset write; the unit names decode typed.
- D11 `task_json_without_kind_decodes_standard` — the serde-default
  round-trip (old snapshot + old journal payload shapes).
- D12 `undo_restores_a_kind_edit_and_the_watermark_names_it` — the
  R-U7 watermark + inverse synthesis through `UndoField::Kind`.
- D13 `undo_refuses_to_recomplete_a_tracker_typed` — done → reopen →
  convert → undo ⇒ `TrackerTaskNotDoneable`, head intact (D-5's
  cascade-adjacent edge).
- D14 `tracking_verbs_are_untouched_on_a_tracker_task` —
  Start/Stop on a tracker task plan exactly as on a Standard task
  (the feature's point; the existing tracking suite's fixture
  doubled with `kind: Tracker`).
- D15 `modeled_keys_are_the_appdata_rows_plus_scheduled` — the
  lockstep pin updated: six keys, the structural TASK_FIELDS check,
  every key dispatches its `v:null` envelope (the landed test's
  shape, count 5 → 6).
- Snapshots: the 52 persisted-state files refresh (mechanical).

**State (`taskboard-state`)** — routing only

- S1 `the_engine_surfaces_the_tracker_rejection_typed` —
  `ExecuteError::Rejected(TrackerTaskNotDoneable(_))` crosses the
  actor boundary (one `matches!`).
- S2 `an_old_state_snapshot_without_kinds_decodes` — a pre-kind
  snapshot JSON blob decodes with every task `Standard` (the
  restart-compat pin; the round-trip direction of D11 at the state
  seam).

**Storage (`taskboard-storage-sqlite`)**

- T1 `kind_roundtrips_through_the_row` — Standard + Tracker +
  non-epoch clock, insert → read.
- T2 `pre_0019_rows_decode_standard_with_epoch_clocks` — the
  migration-upgrade pin (the 0018 recipe's test shape).
- T3 `an_unknown_kind_text_is_corrupted` — strict codec decode
  (trusted-written rows).

**Sync (`taskboard-sync-nextcloud`)**

- Y1 `a_kind_only_tracker_task_holds_the_flush_gate` —
  `task_has_app_data` + the board any-gate fire for a task whose only
  app content is the kind write (the D-3 clock term).
- Y2 `the_overlay_carries_the_kind_write` — the built document's
  tasks section contains the `{v, clock}` envelope; union with a
  pre-kind base keeps both facts (the overlay recipe's test shape).

**App (`taskboard-app`)**

- A1 `tasks_edit_kind_marks_and_unmarks` — `--kind tracker` then
  `--kind standard`, typed receipts, state re-read.
- A2 `tasks_done_on_a_tracker_fails_typed` — the command-layer
  `matches!` **and** the `--json` envelope's `kind` discriminator
  equals `"tracker-not-doneable"` (the kebab table is the typed
  surface — a structural fact scripts branch on, not prose).
- A3 `the_rejection_surfaces_over_the_ipc_lane_typed` — the daemon
  path's discriminator survives the relay (R-4's verification
  pinned).

**UI (`taskboard-ui-slint`)**

- U1 `tracker_cards_hide_the_done_box_and_carry_the_badge` —
  `task_card_model`'s flags + decoration list (pure model test).
- U2 `the_tracking_affordances_stay_on_a_tracker_card` —
  `can-track-start/stop` + the Tracking decoration co-exist with
  `can-done = false` (the liveness pin at the model seam).
- U3 `the_task_view_hides_the_header_checkbox_for_a_tracker` —
  `push_task_detail`'s flag pass-through.
- U4 `the_router_declines_complete_on_a_focused_tracker` —
  TaskComplete/TaskReopen dispatch emits no command (the keymap
  path's guard).
- U5 `the_tracker_context_menu_omits_complete_and_offers_the_toggle`
  — the hide-set + `TaskToggleTracker` present; invoking it sends
  the inverted `UpdateTask` kind change (the
  `the_context_menu_focuses_and_routes_the_card` shape,
  journeys.rs:1260-1296).
- U6 `the_done_checkbox_seam_is_inert_in_kiosk_mode` — the shared
  suppression under the kiosk pin (the seconds-gate test's shape,
  flow_table_tests.rs:1300-1345).
- U7 `notification_rows_drop_the_done_button_for_a_tracker` —
  `awaiting_row_buttons`' flag (the
  `notification_surface_buttons_send_exact_commands` shape).

## 10. Phasing & commit plan (one branch, N.1 → N.5 in order)

Every commit standalone (rebase-merge hygiene); tests first within
each phase (red → green), the §9 IDs riding their phase:

| Phase | Scope | Commit subject |
|---|---|---|
| N.1 | D-1…D-5, D-7: `TaskKind`, entity/manifest/clock fold, `TaskChanges.kind`, the engine guard, `fold_task_app_data`/intent/undo mirrors, lockstep updates, snapshots refresh; domain + state tests (D1–D15, S1–S2) | `feat(domain,state): the tracker task kind and the done rejection` |
| N.2 | D-8: migration 0019, codec arms, repo row/INSERT/read, storage tests (T1–T3) | `feat(storage): the tracker kind column` |
| N.3 | D-3's flush gates: `task_has_app_data` + board gate terms, sync tests (Y1–Y2) | `feat(sync): the tracker kind rides the document lane` |
| N.4 | D-9: `--kind` edit flag, the kebab arms, app tests (A1–A3) | `feat(app): the kind edit flag and the typed rejection surface` |
| N.5 | D-10/D-11: model flags, badge, router guard, menu hide-set + toggle action, kiosk pin, UI tests (U1–U7) | `feat(ui-slint): tracker cards lose done and keep tracking` |

CHANGELOG's one bullet rides N.1 (the user-observable contract lands
there); `docs/` untouched (no architecture change — the kind rides
the documented sidecar lane; if review disagrees, a one-line
architecture.org note is the follow-up).

Full gate suite per AGENTS §10 through
`scripts/run-quality-gates.sh` before handoff (stage 2), re-run by
stage 3.

## 11. Risks & mitigations

- **R-1 — migration number race with sibling 4.3** (in flight,
  sync-crate, plausibly migration-carrying): the dispatch's rule is
  the number is taken *at the rebase*; stage 3 lists `migrations/` on
  the post-rebase base and renumbers if 0019 is taken. Low cost,
  recorded here so nobody "fixes" it at stage 2.
- **R-2 — `models.rs` rebase friction with sibling 4.1**
  (`perf/ui-today-virtualisation`, in flight, touches the push
  passes): the dispatch's lane note expects exactly this friction;
  the §2 rebase dance applies. The kind's model additions are small
  and additive, so conflict surfaces are narrow.
- **R-3 — snapshot churn (52 files, one line each)**: mechanical;
  `cargo insta` review; no semantic review needed beyond spot-checking
  `"kind":"standard"` appears per task.
- **R-4 — the typed rejection over IPC**: the relay's typed-error
  transport is exercised by `UnknownTask` today, but A3 pins the new
  variant explicitly; if the transport turns out to flatten variants,
  the finding goes to the user (it would be a pre-existing gap, not
  this chunk's bug) with the pin downgraded to the local path and a
  follow-up issue.
- **R-5 — the undo-refusal semantics (D-5/D-7)**: a user who converts
  then undoes past the conversion meets a typed refusal where the old
  behavior would have re-completed. This is the decided contract
  (the rule is uniform at the engine); if product feedback reverses
  it, the one-line alternative (undo bypasses the guard) is a
  documented policy change at the journal wrapper, not a redesign.
- **R-6 — object-menu plumbing (D-10.5)**: `object_entries` takes its
  first per-task parameter; the empty-states-map pin's doc note must
  not silently become a states-map feature — the hide-set stays
  minimal (one predicate, one consumer).
- **R-7 — the lockstep pins bite broadly** (manifest → modeled keys →
  intent → fold → undo): by design. A missed site fails a structural
  test, not a behavior. The plan enumerates all six consumer sites
  (§2); the compiler enumerates the rest (exhaustive matches at
  `modeled_key`, `decode_modeled`, `domain_error_kind`,
  `undo_field_kind`).

## 12. Size, handoff, coordination

- **Estimated diff**: ~1,400 ± 300 lines including tests —
  domain ~450, storage ~180, sync ~120, app ~170, ui-slint ~450,
  snapshots 52 × ~1. Five commits (N.1–N.5).
- **Coordination**: same-crate lane (§1 lane 3) — 4.2 follows
  3.3/3.5 (both landed); no ui-territory overlap beyond R-2's
  expected rebase friction; the migration claim is stage 3's
  (R-1).
- **Deferred sugar for the issue queue** (not this chunk, candidates
  only if the user wants them): a `tasks list` kind column; a
  task-view tracker status chip; `Add --kind`. Record as follow-up
  issues at close per AGENTS §7 if raised in review.
- **Handoff**: stage 2 continues in
  `../taskboard-WT-feat-tracker-task-kind` on
  `feat/tracker-task-kind` (recreate per AGENTS §8 if missing; the
  branch is unlanded, no `-v2` question). No push, no PR from stage
  2; stage 3 rebases, ships the PR
  (`feat(domain,app): the tracker task kind` per the dispatch), and
  closes #492 with a self-contained comment.

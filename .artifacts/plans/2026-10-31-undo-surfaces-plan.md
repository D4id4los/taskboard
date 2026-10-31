# [Plan] Undo/redo surfaces — CLI verbs + UI reserved-row flip (undo/2, #510) — stage-1 TDD implementation plan

- **Date**: 2026-10-31
- **Kind**: stage-1 chunk plan for dispatch chunk **undo/2** (the
  six-rounds batch dispatch,
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §14.1). TDD-first; no production code in this commit.
- **Branch / worktree**: `feat/undo-surfaces` in
  `../taskboard-WT-feat-undo-surfaces` (off `origin/main` @
  `9e343ddb`).
- **Precondition verified (2026-10-31)**: undo/1 (#509,
  `feat/undo-journal`) is merged — `origin/main` carries the journal
  substrate (`d4c381fc`), the inverse synthesis + `plan_undo`/`plan_redo`
  (`43f2fd9e`), the engine switch + undo e2e suite (`4dc85232`), ADR 0019
  + the ADR 0006 amendment (`5ee43858`), and the redo pushed-boundary fix
  (`e4f4572e`). `StateCommand::Undo`/`Redo`, `CommandOutcome::Undone`/
  `Redone`, the typed refusals, and `AppState.undo: UndoView` all exist.
- **Normative origins**: the undo implementation design round
  `.artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md`
  **§5 chunk 2** (normative, item by item) and **§6** (the test plan);
  the binding architecture
  `.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5 — **D-U5 risk (b)** governs the redo/restore copy); dispatch
  plan §14.1 (covers, acceptance, stage-2/3 prompts).

---

## 1. Mission & binding constraints

Land the second and final half of undo v1: the two **CLI verbs**
(`tasks undo` / `tasks redo`, REPL and daemon socket inherited) and the
**UI flip** (the reserved `AppUndo`/`AppRedo` rows from `Disabled` to
real dispatch, the registry/menu/greying machinery, and the `UndoView`
binding into the adapter's derived state). Everything engine-side this
chunk needs already exists (undo/1); this chunk adds **zero
`StateCommand` arms, zero domain types, zero storage or engine edits**
— undo/2 is not a shape-widener and touches no `taskboard-domain` /
`taskboard-state` / `taskboard-storage-sqlite` production code at all.

Binding constraints (violating any is a review-blocking finding):

- **The UI stays a dumb client** (AGENTS §1): `UndoView` is read from
  the published snapshot; the router emits `StateCommand::Undo`/`Redo`
  through the existing fire-and-forget command channel; toasts render
  typed outcomes — no outcome round-trip, no new envelope variant, no
  new channel (the undo/1 ruling "no new envelope/channel/actor"
  extends verbatim to the surfaces chunk).
- **The CLI only parses/dispatches** (ADR 0009): typed handlers, typed
  receipts, typed refusals; the verbs await `execute` like every verb,
  so REPL and the daemon socket inherit per ADR 0012 — the inheritance
  is structural (`dispatch_line` → `tasks_line`), never re-implemented.
- **D-U5 risk (b) — the copy law**: no user-visible string may promise
  remote identity preservation. Undo of a pushed delete re-POSTs under
  a fresh remote card id (`RestoreTask`'s bound-and-pushed case, ADR
  0019 §11); copy states the *local* fact, never the remote shape.
- **R-U6 — refuse and name, never skip**: the refusal copy names the
  head's label; the UI can never *pre-distinguish* a `NotUndoable` head
  (`UndoView.can_undo` is deliberately `true` for one), so the refusal
  surfaces typed at the CLI/REPL and as a no-op in the UI in v1 (§3
  S-8, risk R-3).
- **AGENTS §5 discipline**: no text assertions (assert
  `matches!`/kinds/structural facts; receipt *label* strings are
  presentation and are never asserted), no sleeps, hermetic boots,
  fixed/stepping clocks only.
- **Keymaps untouched** (dispatch §14.1: "nothing to add — a new
  binding is scope creep"): all three presets already bind the strokes
  (§2 fact 12); the resolved-preset snapshot must not change.

## 2. Verified seam facts (do not re-derive; verified 2026-10-31 against `9e343ddb`)

Symbols are the stable keys; line numbers are this base's.

| # | Fact | Symbol / file | Anchor |
|---|---|---|---|
| 1 | `TasksCmd` is the subcommand enum chunk 2 extends; it ends with `Repeat(RepeatArgs)`; `tasks done` already carries a `--undo` *flag* (reopen = `SetTaskDone{done:false}` — unrelated to the journal) | `TasksCmd` | `taskboard-app/src/cli.rs:1216-1350` |
| 2 | The awaited-verb wrapper to mirror: boot core → run body → print receipt via `render_text_receipt`; error path = `report_to` (exit 1) | `run_mutation` | `taskboard-app/src/commands/mod.rs:2067` |
| 3 | The verb template: one-shot `pub async fn done(config, store, …)` wrapping `with_core`, plus the session seam `pub(crate) async fn done_on(app, …)` executing `StateCommand` on the booted engine | `done` / `done_on` | `taskboard-app/src/commands/tasks.rs:270` / `:282`; `with_core` `:43` |
| 4 | The undo refusals **already have CLI JSON kinds** (landed with undo/1): `nothing-to-undo`, `nothing-to-redo`, `target-not-undoable`, `target-not-invertible`, `undo-target-has-dependents`, `engine-routed` | `domain_error_kind` | `taskboard-app/src/commands/mod.rs:592-596` |
| 5 | The receipts file: per-verb serde projection structs (never domain serde shapes); `TaskDeleteReceipt` is the pre-execute-snapshot precedent, `TrackDeleteReceipt` the bool-honest-repeat precedent | `output.rs` | `taskboard-app/src/commands/output.rs` |
| 6 | Routing: `route_for_ipc`'s `Command::Tasks { .. }` arm is already `IpcOrLocal` — **the new verbs inherit the daemon socket with no new row**; the lane pin test lists whole `Command` groups | `route_for_ipc` | `taskboard-app/src/commands/mod.rs:2002-2045`; unit pins `:2190-2314` |
| 7 | The session router: `dispatch_line` → `Command::Tasks { cmd } => tasks_line(...)`; `tasks_line` matches `TasksCmd` arm by arm — two new arms there give REPL + daemon the verbs for free | `dispatch_line` / `tasks_line` | `commands/mod.rs:1519` / `:1829` |
| 8 | Router: the reserved arm reads `ActionId::AppUndo | ActionId::AppRedo | ActionId::AppHelp => DispatchOutcome::Disabled`; the enabled bit gates *before* the match; `DispatchOutcome::Command` is fire-and-forget through the command mpsc | `ActionRouter::dispatch` / the arm | `taskboard-ui-slint/src/router.rs:332-337`, `:537` |
| 9 | The absorbed-no-op test pins exactly `[AppUndo, AppRedo, AppHelp]` as `Disabled` — shrinks to `[AppHelp]` in the flip | `disabled_actions_are_absorbed_even_with_focus` | `router.rs:886-905` |
| 10 | Registry rows: `AppUndo`/`AppRedo` with `enabled: false`, `contexts: GLOBAL`, ranks 30/40, descriptions that name backlog #356/#353 as "not yet available" — copy rewritten in the flip | `ActionEntry` rows | `taskboard-ui-slint/src/actions.rs:437-456` |
| 11 | The reserved-set pins: `reserved_globals_are_present_and_disabled` + `only_reserved_actions_stay_disabled` + the set helper — the set shrinks to `{AppHelp}`; the registry insta snapshot (`keymap_snapshots__registry.snap`) carries the rows and regenerates | tests + snapshot | `actions.rs:1281-1311`, `:1349-1352`; `tests/snapshots/keymap_snapshots__registry.snap` |
| 12 | Keymaps: all three presets bind the strokes — standard `Ctrl+Z`/`Ctrl+Shift+Z`, emacs `Ctrl+x u`/`Ctrl+g Ctrl+/`, vi `u`/`Ctrl+r` (ContextId::Global / ViNormal). **Nothing to add**; `resolved_preset_keymaps.snap` must not change | presets | `taskboard-ui-slint/src/keymap/presets.rs:65-66`, `:255-256`, `:507-508` |
| 13 | Menu: rows render from the registry (`root_page` quick rows in rank order — AppUndo/AppRedo already asserted present, unchanged); greying is the adapter-supplied `states` override winning over the static `enabled` bit | `row` / `entry_states` consumers | `taskboard-ui-slint/src/menu.rs:63-79` (override at `:75`) |
| 14 | The greying map: an enumerated, closed `BTreeMap<ActionId, bool>` built from `&AppState` + `Flow` at menu open and in the models pass — the `UndoView` binding site | `Adapter::entry_states` | `taskboard-ui-slint/src/adapter.rs:257-287` |
| 15 | The published projection: `UndoView { can_undo, can_redo, head_label: Option<String>, recent: Vec<UndoLabel{ id, label }> }`, window `UNDO_RECENT_WINDOW = 10`, derived at every publish, memory-only; `can_undo` is `true` even for a `NotUndoable` head (R-U6 refuses at planning) | `UndoView` / `UndoView::derive` | `taskboard-domain/src/journal.rs:253-298`, `:42` |
| 16 | The receipts the verbs answer with: `CommandOutcome::Undone { entry: JournalId }` / `Redone { entry: JournalId }` ("the durable receipt; the label lives in the `UndoView`"); refusals `NothingToUndo`, `NothingToRedo`, `TargetNotUndoable(NotUndoableReason)`, `TargetNoLongerInvertible { entity, field }`, `UndoTargetHasDependents { stack }` | outcomes / refusals | `taskboard-domain/src/command.rs:316-324`, `:155-184` |
| 17 | `plan_undo`/`plan_redo` append **no journal entry of their own** (they only `MoveJournalHead`) — so the just-undone/redone entry is always the most recently *appended* entry and sits inside the 10-row `recent` window right after execution | planning | `taskboard-domain/src/undo.rs:406-478`, `:483-523` |
| 18 | The UI integration harness: `Harness` mounts an `AppState` (fields pub — `undo: UndoView` settable), opens menus, asserts commands sent through the adapter | `Harness` / `assert_commands` | `taskboard-ui-slint/tests/journeys.rs:53`, `:141-149`, `:198` |
| 19 | The hermetic CLI test template: standalone tempdir configs, forbidden store, real clap parse, receipts as typed values, refusals via `matches!`/`error_kind()` — "never message text" | H-series | `taskboard-app/tests/cli_habits.rs:1-79` |
| 20 | CHANGELOG already carries the undo/1 engine bullet whose last line hands off to this chunk ("User-facing surfaces … land with chunk undo/2") | Unreleased | `CHANGELOG.md:29-38` |

## 3. The design, decision by decision

### S-1 — The CLI verb shapes: `tasks undo` / `tasks redo`, zero-arg

```rust
/// Undo the last change (the journal line head). Refuses typed when
/// the line is empty, the head is not undoable, or a later write
/// diverged (ADR 0019).
Undo,
/// Redo the last undone change (the head's newest child).
Redo,
```

Both are unit variants on `TasksCmd` (after `Repeat`), zero flags: the
verbs address the journal line, which has no selectors. `Undo`/`Redo`
route through `plan_journaled_command` engine-side (ADR 0019); the CLI
only parses and dispatches.

Naming collision check (recorded): `tasks done --undo` is the reopen
flag on the completion verb — `SetTaskDone { done: false }`, not the
journal. The two coexist; `tasks undo` is the journal verb. No rename,
no alias, no deprecation (D-1).

### S-2 — The receipt: `JournalReceipt`, label best-effort from the published window

One receipt type serves both verbs (the outcomes differ only in the
verb discriminant):

```rust
/// The `tasks undo`/`tasks redo` receipt: the journal entry the verb
/// landed on (the durable receipt, ADR 0019 §7) plus the entry's
/// display label when the published window still carries it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JournalReceipt {
    /// Which verb produced this receipt (`"undo"` | `"redo"`).
    pub verb: JournalVerb,
    /// The journal entry's uuid (the durable id).
    pub entry: String,
    /// The entry's label from the published `UndoView.recent` window;
    /// `None` = outside the window (print the bare uuid — honest, not
    /// an error).
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum JournalVerb { Undo, Redo }
```

Handler shape (both lanes, the `done`/`done_on` template verbatim):

```rust
pub async fn undo(config, store) -> Result<JournalReceipt, CommandError>   // with_core
pub(crate) async fn undo_on(app: &App) -> Result<JournalReceipt, CommandError>
// + redo / redo_on
```

`*_on` executes `StateCommand::Undo` (resp. `Redo`) via
`app.engine.execute(...)`, then loads the published snapshot and
resolves the label:

```rust
let state = app.engine.shared_state().load_full();
let label = state
    .undo
    .recent
    .iter()
    .find(|l| l.id == entry)
    .map(|l| l.label.clone());
```

Why best-effort `Option` instead of a guarantee (D-2): the receipt's
contract is the *entry id* (the domain comment is explicit: "the label
lives in the `UndoView`"); the window is a bounded presentation
projection (10 rows). Fact 17 says the just-touched entry is in-window
right after execution, so `label` is `Some` in every reachable v1 case
— but the type stays total and the `None` path honest rather than
coupling the CLI to window arithmetic the domain does not promise.

Text render (`render_text_undo`, mirroring the delete arm's inline
closure style): `undone "<label>"` / `undone <entry-uuid>` when the
label is absent; `redone …` for redo. Text is UX, never asserted.

### S-3 — Refusals: the existing typed engine-rejection path, plus typed details

Refusals need **zero new `CommandError` variants**: `execute` answers
`Err(ExecuteError::Rejected(domain::CommandError::…))`, the
`From<ExecuteError>` impl boxes it, and the undo kinds landed in
undo/1's `domain_error_kind` (fact 4). The `--json` error envelope
therefore already carries `nothing-to-undo`, `nothing-to-redo`,
`target-not-undoable`, `target-not-invertible`,
`undo-target-has-dependents` — the typed outcomes the origin names.

One small honest extension (D-3): `error_detail()` gains arms for the
two parameterized refusals so the JSON detail names the structural
facts a script can branch on — `TargetNoLongerInvertible { entity,
field }` → `"<entity-uuid> <field-kebab>"`, `UndoTargetHasDependents {
stack }` → the stack uuid, `TargetNotUndoable(reason)` → the reason's
kebab form (a tiny `NotUndoableReason::kind()` helper in the *app*
crate's match, or reuse of an existing projection if one exists).
Details are ids/enums, never prose; the kind stays the contract.

### S-4 — Daemon/REPL inheritance: structural, pinned in the lane tests

`route_for_ipc` needs **no new row** — the `Command::Tasks { .. }`
wildcard already routes `IpcOrLocal` (fact 6), which is exactly ADR
0012's inheritance. What the chunk does instead is *pin* the
inheritance: the two new `Command::Tasks { cmd: TasksCmd::Undo }` /
`Redo` values join the route_for_ipc unit test's `IpcOrLocal` list in
`commands/mod.rs`'s tests (the exhaustive-lane assertion), so a future
split of the `Tasks` wildcard arm cannot silently drop the undo verbs
off the mutation lane. `verb_timeout_secs` gains no row (no
`--timeout-secs` on these verbs). The REPL lane is `tasks_line`'s two
new arms; the daemon lane is the same function through `dispatch_line`
(fact 7) — one implementation, both frontends, the ADR 0012 rule.

### S-5 — The router flip: two arms, one surviving reserved row

`router.rs`'s arm becomes:

```rust
// Reserved (§5.8) — AppHelp is the only disabled row left (D6).
ActionId::PaletteSearch => DispatchOutcome::UiEffect(UiEffect::OpenPalette),
// Undo v1 surfaces (ADR 0019, undo/2): target-free engine verbs —
// availability greys from the published UndoView (entry_states), the
// engine refuses typed when the line cannot serve the verb.
ActionId::AppUndo => DispatchOutcome::Command(StateCommand::Undo),
ActionId::AppRedo => DispatchOutcome::Command(StateCommand::Redo),
ActionId::AppHelp => DispatchOutcome::Disabled,
```

Notes:

- **No `DispatchCtx` field**: the verbs are target-free — unlike every
  other command row there is no `NoTarget` case; the engine-side
  refusals are the only "no" (D-4). The dumb-client pin holds: the
  router emits a `StateCommand`, nothing else.
- The keyboard chord (`Ctrl+Z` etc.) dispatches even when the menu row
  greys — greying is a menu-render fact, not a resolver gate (the
  established `entry_states` semantics). An empty-line Ctrl+Z sends
  `Undo`, the engine refuses, the send path logs-and-drops nothing
  (the *send* succeeds; the rejection is engine-side). UI silence on a
  refused undo is the v1 contract (risk R-3).
- The absorbed-no-op test shrinks to `[AppHelp]` and the two verbs get
  their own dispatch assertions (§5 R-1/R-2).

### S-6 — The registry flip: bits, copy, pins

Each site listed (grep order, all in `actions.rs` unless noted):

1. **`AppUndo` row** (`:437-445`): `enabled: true`; description
   rewritten to the live copy (below); rank 30 and `menu_group: None`
   unchanged (the root-page quick-row order test keeps passing
   untouched).
2. **`AppRedo` row** (`:446-456`): same, rank 40.
3. **`reserved_globals_are_present_and_disabled`** (`:1281-1296`): the
   iterated set shrinks to `[AppHelp]`; the trailing comments update
   (AppUndo/AppRedo left in the undo/2 flip, named).
4. **`reserved_globals_are_present_and_disabled_set`** (`:1349-1352`):
   the helper set shrinks to `{AppHelp}` (drives
   `only_reserved_actions_stay_disabled` — its logic is unchanged and
   now asserts every other row enabled, including the two flipped).
5. **New test** `undo_and_redo_rows_are_enabled_and_global`: both rows
   read `enabled: true`, `contexts == GLOBAL`, ranks 30/40 (§5 A-2).
6. **Snapshot** `tests/snapshots/keymap_snapshots__registry.snap`: the
   one deliberate regeneration pass — the diff must audit as exactly
   the two rows' `enabled` bits and descriptions, nothing else.
   `resolved_preset_keymaps.snap` must **not** change (fact 12: no
   binding edits); a changed preset snapshot is a review-blocking
   finding (keymap scope creep).

The copy law for the two descriptions (D-U5 risk (b) applied *now* —
these strings are the v1 user-visible surface of undo/redo):

- **`AppUndo`** — states the local fact and the refusal behaviour, no
  remote promises: e.g. *"Undoes the last change. A change the server
  already received is undone by a new compensating change; the line
  head that cannot be undone is refused and named."* (final wording is
  stage-2 freedom within the law: no "restores exactly", no identity
  promises, honest about refusals.)
- **`AppRedo`** — the critical one: re-application is a *new edit*;
  restored content may carry fresh remote identity. Must **not**
  contain any promise of preservation ("restores the same card",
  "as it was remotely" are forbidden shapes); must name the new-edit
  nature: e.g. *"Re-applies the last undone change as a new edit.
  Content the server deleted is re-created as a new remote card."*
- The backlog pointers (#353/#356 "not yet available") are dropped —
  the rows are live; the deferrals live in the issues, not the tooltip.

### S-7 — The `UndoView` binding: `entry_states` is the whole binding

`Adapter::entry_states` gains exactly two rows (fact 14's closed map —
the enumerated state-dependent rows only):

```rust
(ActionId::AppUndo, state.undo.can_undo),
(ActionId::AppRedo, state.undo.can_redo),
```

That is the chunk's entire adapter diff. Consequences, all intended:

- The menu rows grey live: an empty journal renders both rows
  disabled; after any accepted mutating command `AppUndo` enables; after
  an undo `AppRedo` enables; engine syncs re-grey an open menu (the map
  is rebuilt in the models pass while the menu holds — fact 13).
- `head_label` and `recent` gain **no UI consumer in v1** (D-5): the
  status bar carries sync/banners only, and inventing a surface for the
  label is scope creep past §14.1. The labels serve the CLI receipt
  (S-2) and the future toast host.
- No `(w)` widening: `UndoView` is consumed exactly as undo/1 landed
  it; the domain/state snapshots are untouched (D-6).

### S-8 — The toast-copy rules: no host in v1, the law recorded

There is **no toast host in the Slint UI** (verified: the only
"toast" strings are the alarm window's and `DELIVERY_CHIP_TEXT`'s
notification-delivery chip; the reminders toast plan and native toast
buttons are fenced behind GUI-review-round-3 #444). The architecture
round itself anticipated this: the capture overlay's undo toast lands
"via `UndoView` **when wired**" (origin plan §4, chunk-4 note).
Building a host here is out of §14.1's scope (D-7).

What undo/2 ships instead is the **copy law** — binding on every
surface this chunk writes (the two registry descriptions, S-6) and on
the toast host when it lands:

1. **Local-fact rule**: undo/redo copy states what happened *locally*
   ("Undone: ⟨label⟩"). Labels come from the journal's
   `command_label` derivation via `UndoView` — never invented at the
   surface.
2. **No-identity-promise rule (D-U5 risk (b))**: no string may claim
   the server sees the same card again. Undo of a pushed delete
   re-POSTs (`RestoreTask`'s pushed case) under a fresh remote id;
   redo across the push boundary is always a new edit. Forbidden
   shapes: "restores", "puts back", "as it was" *in remote terms*.
   Permitted: "re-creates as a new remote card", "as a new edit".
3. **R-U6 refusal rule**: the refusal names the head's label —
   "⟨head label⟩ — not undoable" (the origin's exact example shape),
   never a bare "failed", never a skip. In v1 this copy renders
   nowhere (no host); the CLI carries the typed refusal and the law
   waits, recorded here, for the host.
4. **No-text-assertion corollary**: tests assert the *derivation*
   (which label was chosen, that the refusal path produces the naming
   shape) structurally — never string equality on the copy itself.

### S-9 — CHANGELOG

The Unreleased section gains one `Added` bullet (the surfaces), and
the undo/1 engine bullet's hand-off line ("User-facing surfaces …
land with chunk undo/2") is updated to point at the same release
instead of the chunk name — the reference must not name a chunk that
ceases to exist at release time.

## 4. What does NOT change

- **`taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`
  production code: zero edits.** No `StateCommand` arm, no `UndoView`
  field, no snapshot regeneration in the state crate, no migration.
  The #244 lockstep and the synthesis catalogue are untouched.
- **`plan_command`'s catalogue** and the journal wrapper — undo/1's
  seam, untouchable here.
- **Keymap presets** (`standard`/`emacs`/`vi`): zero rows added or
  edited; the strokes are bound (fact 12). A new binding is scope
  creep (§14.1 stage-3 focus).
- **`menu.rs` structure**: no new rows, no rank changes, no group
  moves — the rows already render from the registry; greying arrives
  through the existing `states` override.
- **`tasks done --undo`**: stays exactly as it is (S-1).
- **The command channel / envelopes / signals**: fire-and-forget
  convention intact; `EngineSignal` untouched; no outcome plumbing.

## 5. Test plan (TDD order — AGENTS §5 discipline throughout)

New-test naming follows the houses' behaviour-phrase style; every
assertion is structural (`matches!`, kinds, ids, enabled bits,
command variants) — **never label/description/message text**.

### UI-slint (`router.rs` tests, `actions.rs` tests, `tests/journeys.rs`)

| # | Test | Asserts |
|---|---|---|
| R-1 | `app_undo_and_redo_dispatch_the_engine_verbs` | `dispatch(AppUndo, any ctx) == Command(StateCommand::Undo)`; `AppRedo` → `Command(Redo)` — focus-independent (no `NoTarget` case exists) |
| R-2 | `disabled_actions_are_absorbed_even_with_focus` (rewrite) | shrinks to `[AppHelp]`; the PaletteSearch/CaptureQuick live-row assertions stay |
| A-1 | `reserved_globals_are_present_and_disabled` (rewrite) | the set is exactly `{AppHelp}`; `contexts == GLOBAL` |
| A-2 | `undo_and_redo_rows_are_enabled_and_global` (new) | both rows `enabled: true`, `contexts == GLOBAL`, ranks 30/40 (order-stable quick rows) |
| A-3 | registry snapshot (one deliberate regen) | diff audits as the two rows' bits + descriptions only; `resolved_preset_keymaps.snap` unchanged |
| J-1 | journeys: `the undo rows grey from the published undo view` | mount `undo: UndoView { can_undo: false, can_redo: false, .. }` → both menu rows render disabled; flip the view to `true`/`true` → both enabled |
| J-2 | journeys: `invoking the undo rows sends the engine verbs` | with the view enabled, invoke each row → `Harness::assert_commands` sees `StateCommand::Undo` / `Redo` (the fire-and-forget pin at the UI boundary) |

The J-series mounts `AppState` with `undo` set directly (the field is
pub; `UndoView`/`UndoLabel` construct literally — fact 18). No engine,
no sleeps; the harness's existing command recording is the assertion
seam.

### App (`tests/cli_undo.rs`, new — the H-series template verbatim)

| # | Test | Asserts |
|---|---|---|
| U-1 | parse pins | `["tasks","undo"]` / `["tasks","redo"]` select `TasksCmd::Undo`/`Redo` through the real clap surface |
| U-2 | `undo_on_an_empty_journal_refuses_nothing_to_undo` | seeded tempdir, `tasks undo` → `Err(Engine(Rejected(NothingToUndo)))` via `matches!`; `error_kind() == "nothing-to-undo"` |
| U-3 | `undo_retracts_the_last_change_and_names_the_entry` | `boards create` + `tasks add`, then undo → `Ok(JournalReceipt { verb: Undo, entry: <uuid>, label: Some(_) })`; the task is gone from the terminal state (a follow-up `tasks list`/`find` read — structural absence, not receipt text) |
| U-4 | `redo_reapplies_the_undone_change` | undo then redo → receipt `{ verb: Redo, .. }`; the task is back (terminal state) |
| U-5 | `redo_without_an_undone_head_refuses_nothing_to_redo` | fresh db → `NothingToRedo` kind |
| U-6 | `undo_refuses_typed_when_a_later_write_diverged` | add task → edit it → undo → undo again (the second undo targets the add whose task was recreated by redo… see the staged shape below) → `TargetNoLongerInvertible` or `NothingToUndo` per the line the sequence lands on; the point is a **typed** refusal kind, asserted by variant |
| U-7 | `the_undo_verbs_route_over_the_daemon_lane` | `route_for_ipc` pins both `Command::Tasks { cmd: TasksCmd::Undo/Redo }` as `IpcOrLocal` (lives in `commands/mod.rs`'s tests with the lane list) |
| U-8 | `the_session_lane_serves_the_undo_verbs` | boot once (`bootstrap`), run `undo_on`/`redo_on` against the live `App` — the REPL/daemon seam exercised directly (no second boot) |

U-6's staged shape (deterministic, no clocks beyond the hermetic boot's
real one — the engine injects its own): create → undo (retracts the
create) → *the journal head is now empty-of-parent;* instead: create →
edit → undo (undoes the edit) → edit again → undo (refuses: the edit
entry's watermark diverged — the interposed second edit touched the
same fields). Assert the variant, not the message.

Refusal-surface note: U-2/U-5/U-6 assert `error_kind()` strings —
those are the stable JSON discriminators (the established typed
contract, H-series precedent), not message text.

### Explicitly no tests for

- The registry/menu **copy** (S-6 wording) — text; the law is the
  plan's, the rows' structure is A-2's.
- `command_label` strings (domain, already landed) and
  `JournalReceipt.label`'s *content* — `label: Some(_)` presence only.
- The engine's undo semantics (watermark, pushed-edit rule, retention)
  — undo/1's suite owns them; this chunk re-tests none of it
  (one-behaviour-one-test-lowest-layer).

## 6. Implementation order (stage-2 guidance)

Every commit standalone-buildable and gate-clean; subjects
`<type>(<module>): …`.

1. **`feat(app): the tasks undo and redo verbs`** — `TasksCmd`
   variants; `JournalReceipt`/`JournalVerb` in `output.rs`;
   `undo`/`undo_on`/`redo`/`redo_on` in `commands/tasks.rs`; the two
   `dispatch` arms + two `tasks_line` arms; the `route_for_ipc` pin
   rows; `error_detail` arms (S-3); `cli_undo.rs` U-1…U-8. TDD:
   the parse/refusal tests first (they fail to compile without the
   variants — the Rust-native red), then the handlers.
2. **`feat(ui-slint): the undo and redo rows dispatch`** — registry
   bits + copy (S-6), router arms (S-5), the rewritten + new tests
   (R-1/R-2, A-1/A-2), the one deliberate registry-snapshot
   regeneration with the diff audit (A-3).
3. **`feat(ui-slint): the undo view greys the menu rows`** —
   `entry_states` rows (S-7), journeys J-1/J-2.
4. **`docs(changelog): the undo surfaces entry`** — S-9 (may fold into
   3 if the diff stays clean; the Unreleased entry must exist by the
   last commit either way).

Then AGENTS §10 gates through
`scripts/run-quality-gates.sh` from the worktree; the state crate's
insta corpus must show **zero** diff (the not-a-widener proof).

## 7. Decision record (every open question, resolved)

| # | Question | Decision | Why / what was rejected |
|---|---|---|---|
| D-1 | `tasks undo` vs the existing `tasks done --undo` | Both stay; `--undo` is the reopen flag, `tasks undo` the journal verb; no rename/alias | The flag predates the journal (chunk-10 shape); renaming it breaks scripters for no ambiguity that exists in practice |
| D-2 | Receipt label: guaranteed or `Option`? | `Option<String>`, best-effort from `UndoView.recent` lookup by id | The domain's contract is the entry id ("the label lives in the `UndoView`"); fact 17 makes `Some` the reachable case, but the type stays total without coupling to window arithmetic |
| D-3 | Refusal details in the JSON envelope | Add `error_detail` arms for `TargetNoLongerInvertible`/`UndoTargetHasDependents`/`TargetNotUndoable` (ids + kebab reason) | The kinds exist; the details make refusals scriptable; details are structural, never prose |
| D-4 | A `DispatchCtx` target for the verbs? | None — target-free `Command` outcomes, no `NoTarget` case | The journal line is app-global; availability is the greying map + engine refusals; a ctx field would invent a target that does not exist |
| D-5 | UI consumers for `head_label`/`recent`? | None in v1; `entry_states` is the whole `UndoView` binding | §14.1's binding scope; a label surface (status bar) is new UI real estate the chunk does not budget |
| D-6 | Any `UndoView` widening (e.g. `head_undoable`)? | No — consume as landed | undo/2 is not a shape-widener in the wave table; a widening would force the 19-pair insta regen and a domain PR's worth of review for a v1 UI that cannot act on it |
| D-7 | A toast host in this chunk? | No host. The copy law (S-8) + registry descriptions ship; the R-U6/redo toast strings land with the #444-fenced toast work | No host exists (§2 fact-check); §14.1's "toast labels" are the copy rules + the live copy surfaces; building a host is new UI subsystem scope |
| D-8 | Undo-refusal UX in the Slint UI v1 | Enabled row + engine-side refusal, silently absorbed (logged); named honestly in the row description | The published view deliberately cannot pre-distinguish a `NotUndoable` head (undo/1's R-U6 choice); without outcome plumbing the UI cannot know — D-6/D-7 keep it that way in v1 |
| D-9 | `route_for_ipc` row for the new verbs? | No new row — the `Command::Tasks` wildcard already routes `IpcOrLocal`; pin both verbs in the lane unit test instead | The inheritance is structural (ADR 0012); the pin converts it from accidental to guarded |
| D-10 | Zero-arg vs flag-bearing verbs (`tasks undo --all`…) | Zero-arg unit variants; no flags | v1's line is linear undo/redo; selectors/branch-picking are #353's v2 scope |

## 8. Risks

- **R-1 — Registry snapshot breadth**: the regen touches exactly two
  rows' bits + descriptions; any other hunk in the audit (or any
  change to `resolved_preset_keymaps.snap`) means a keymap/registry
  edit slipped in — review-blocking.
- **R-2 — Concurrent ui-slint drift**: `actions.rs`/`journeys.rs` are
  the GUI-review rounds' favourite files; if another UI branch lands
  mid-flight, expect the §8 rebase dance (small, conflict-shaped like
  the habit round's).
- **R-3 — UI refusal silence**: a refused undo (empty line via chord,
  or `NotUndoable` head) is invisible in the UI v1 (D-7/D-8). Honest
  per the published view's contract, but a real-usage observation to
  carry into the #444 toast-host ruling — record it in the PR body.
- **R-4 — The label `Option` surprise**: if a future change makes the
  just-touched entry fall outside `recent` (window shrink, eviction
  retiming), receipts degrade to bare uuids — typed total, but watch
  the undo/1 `UNDO_RECENT_WINDOW` if it ever moves.
- **R-5 — Fire-and-forget damping**: Ctrl+Z held down enqueues a
  burst of `Undo`s; the engine serializes them (sequential loop), so
  the line unwinds N deep — correct but surprising; no debounce in v1
  (matches every other held-chord verb repeat behaviour).

## 9. Estimated diff size

| Slice | Contents | Size |
|---|---|---|
| `taskboard-app` | 2 CLI variants, 4 handler fns, receipt + verb enum, 4 dispatch arms, routing pins, `error_detail` arms | ~+140 |
| `taskboard-app/tests` | `cli_undo.rs` (U-1…U-8) | ~+230 |
| `taskboard-ui-slint` | registry rows + copy, router arms, `entry_states` rows, test rewrites + new pins | ~+90 / −40 |
| `taskboard-ui-slint/tests` | journeys J-1/J-2 + snapshot regen | ~+90 |
| `CHANGELOG.md` | the surfaces bullet + hand-off line fix | ~+12 |
| **Total** | | **≈ +560 / −45 (S/M overall)** |

Dispatch ruled M (UI M, CLI S); the mature reserved-row infrastructure
(facts 8–14) lands this at the low end — the rows, the greying map, and
the lane tests all predate the chunk.

## 10. References

- Origin plans: `.artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md`
  (§5 chunk 2 normative; §6 tests; §9 risks — risk 2 is the copy law),
  `.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5; §3.5 scope; the "when wired" toast note).
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (lane table), §14.1 (this chunk's covers/acceptance/prompts).
- Landed base: ADR 0019 (`docs/adr/0019-*.org`), the undo/1 stage-1
  plan `.artifacts/plans/2026-10-30-undo-journal-plan.md`.
- Issues: #510 (this chunk), #176 (umbrella — closes with this chunk's
  PR), #353–#357 (v2 family, untouched), #444 (the fenced toast-host
  family — where the R-U6/redo toast strings land).

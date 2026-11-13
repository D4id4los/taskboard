# [Plan] GUI Review Round 7 — Analysis & Solutions

- **Date**: 2026-11-13
- **Status**: STAGE-1 PLAN (analysis + proposed solutions; no code). Written
  from the user's three round-7 review observations. After acceptance this
  plan chunks into a dispatch plan per `.artifacts/templates/dispatch-plan.md`.
- **Origin**: the user's GUI review round 7 observation list (2026-11-13):
  recurring-task creation, habit creation, and the Files-section board roster
  with per-account sync badges and actions.
- **Verified against**: `main` @ `64efd33c`. All file:line anchors were read
  at that SHA; expect drift by implementation time — re-verify anchors, the
  findings themselves are stable.
- **Related issues**: #722 (closed — the landed Files-boards surface this
  round reshapes), #723 (open — multi-lane files sync; this round keeps the
  single-lane posture), #718 (open — transport DELETE/GC; receives the
  stop-sync leftover), #750 (open — adoption-state badges; rides the same
  row grammar), #502–#506 (closed — the DR-6 recurring/habits waves whose
  engine work every finding below stands on).
- **New backlog issues filed by this plan**: #755 (stop-sync subtree
  leftover, cross-linked to #718) and #756 (Deck→Deck board move).

---

## 1. Scope

Three observations, three code areas. The headline finding shared by all
three: **the engine, domain, and storage layers already implement every
capability the user asks for — what is missing is exclusively the GUI
surface**. No `taskboard-domain`, `taskboard-state`, or
`taskboard-storage-sqlite` schema/state changes are required by any
solution; the work is `taskboard-ui-slint` presentation/adapter plus two
small `taskboard-app` request arms and one pure-function extraction into
the domain crate (D-7.1).

| # | Observation | Verified root cause | Primary anchors |
|---|---|---|---|
| R7-1 | Tasks are missing a GUI setting to create repeated tasks | Recurrence is fully implemented end-to-end — the typed grammar (`RecurRule`/`RecurPattern`/`RecurBound`, `recur.rs:123-701`), the `Task.recur` field (`entities.rs:779`), the sparse setter `TaskChanges.recur: Option<Option<RecurRule>>` (`messages.rs:676`), engine stamping (`command.rs:1072-1076`), storage (`0010_recur_field_family.sql`), sidecar sync (`flush.rs:693`), completion spawn (`command.rs:1134-1159`), the CLI (`tasks repeat`, `commands/repeat.rs:306-360`), and a read/manage UI row (badge + pause/resume/convert, `task-view.slint:726-831`). **No UI site constructs `TaskChanges { recur: … }`** (verified over every `TaskChanges` construction in the UI crate) — setting a rule is CLI-only. At-create scheduling is excluded by a recorded ruling (the capture overlay's header, `capture-overlay.slint:2-6`), so the setting belongs in the task detail view | `crates/taskboard-domain/src/recur.rs`; `crates/taskboard-ui-slint/ui/views/task-view.slint:726-831`; `crates/taskboard-app/src/commands/repeat.rs:306-360`; `src/models.rs:2161-2181` |
| R7-2 | The GUI has no way of creating habits | Habits are a complete entity family (DR-6 chunks 5–6): domain (`habit.rs`, `HabitTarget`/`NonZeroCount`), engine verbs (`CreateHabit { board, title, target }`, `messages.rs:461-515`), storage (`0017_habit_trackers.sql`), sync lanes, undo arms, and a Today-view section with +1 / make-up / reorder / convert affordances (`habits.slint`). **The section has no add affordance** (header is a static caption, `habits.slint:63-76`), the action registry has no create verb, and creation is `taskboard habits add` CLI-only (`commands/habits.rs:259-300`). `CreateHabit` carries the target at create (no optional fields, D-H7a), so a create form needs title + target only; the board comes from the section itself (rows are already scoped to one board, `models.rs:742-744`) | `crates/taskboard-ui-slint/ui/compositions/habits.slint:30-157`; `crates/taskboard-domain/src/messages.rs:461-515`; `crates/taskboard-app/src/commands/habits.rs:259-300`; `crates/taskboard-ui-slint/src/actions.rs:286-305` |
| R7-3 | The Files section lacks a per-account board roster with sync badges and per-row actions | The Files card today emits only files-bound boards as **title-only rows with no actions** — and duplicates that list under *every* Nextcloud account card (`adapter.rs:4867-4883` inside the per-card block). The no-action posture is a recorded round-6 ruling ("adoption is automatic, disconnect is a non-goal", `settings-view.slint:1065-1069`) that this round's request explicitly overturns. The only local→files path is Deck-first-then-move (the user's exact complaint): `PublishBoard` targets Deck only (`requests.rs:412-416`), `MoveBoardToFiles` refuses non-Deck-bound boards (`ui_requests.rs:1520-1528`). Badge data is *mostly* already UI-side: `ConnectorInfo` carries `(server_url, username, backend)` (`requests.rs:51-58`) and `SettingsSnapshot.files_sync_anchor` is the resolved anchor fact (`requests.rs:250`); what is missing is the **binding channel tag on `BoardInfo`** (`requests.rs:65-70` has only `bound_connector: Option<String>`) — and the structural-detection law (D-BF4: files-ness is `binding.board.as_files().is_some()`, never a connector-name match) forbids deriving it from the name | `crates/taskboard-ui-slint/src/adapter.rs:4764-4998`; `crates/taskboard-ui-slint/src/requests.rs:51-79,425-428`; `crates/taskboard-ui-slint/ui/views/settings-view.slint:1016-1086`; `crates/taskboard-app/src/ui_requests.rs:1506-1565`; `crates/taskboard-app/src/bootstrap.rs:103,120-134`; `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs:162-237,561` |

## 2. Solutions (design decisions)

### D-7.1 — A recurrence-rule editor in the task detail view (R7-1)

The recorded ruling stands: creation stays title-only (capture overlay,
`BeginCreate`); the recurrence *setting* lives in the task detail view,
where the recur row already renders.

- **Shared rule assembly becomes domain truth (the only sub-UI change in
  this round).** The CLI's `assemble_rule` (`commands/repeat.rs:306-360`)
  encodes the minting laws — keep the existing `series`/`continued_from`
  when a rule already exists (D-S4), anchor =
  `series_reference_date(task, day, now)`, weekday/day-of-month defaults
  derived from the anchor, `paused_until` preserved. Extract those laws
  into `taskboard-domain` (e.g. `recur::assemble_rule(&Task, pattern
  parts, bound, now, day) -> Result<RecurRule, InvalidRecur>` — pure,
  zero deps) and refactor the CLI verb to call it. One behavior, one
  implementation, one test family (§5 rules); the UI mints through the
  same function so GUI and CLI cannot drift.
- **New composition** `ui/compositions/recur-editor.slint`, mounted as a
  window-level overlay (the date-picker precedent) with its own
  `ContextId::RecurEditor` (mask semantics like `TrackEntries`: the
  form's fields ride `ViInsert` masking; commit/cancel verbs live in the
  overlay context; Escape still resolves cleanly):
  - **Pattern**: `SegmentedControl` Daily / Weekly / Monthly
    (`RecurPattern`'s three variants).
  - **Daily**: an "every N days" numeric field (`EveryDays::new`, ≥1).
  - **Weekly**: interval field + a row of seven weekday toggles
    (`WeekdaySet`, non-empty; pre-checked from the anchor's weekday —
    the assembly law's default made visible).
  - **Monthly**: `MonthlyAnchor` as a pair — "on day" (1–31,
    `DayOfMonth::new`) or "on the Nth weekday" (`Dropdown` for
    `NthWeek` 1st..5th + weekday dropdown).
  - **Bound**: `SegmentedControl` Never / Until / Count (`RecurBound`);
    Until reuses the due-editor's date parser (`due_edit::parse`,
    `YYYY-MM-DD` or `+Nd`); Count is a positive-integer field
    (`OccurrenceCount::new`).
  - **Live echo**: a caption renders the draft's assembled
    `rule.text_form()`; an invalid draft renders the typed
    `InvalidRecur` variant as a hint (presentation-side mapping — tests
    assert the mapping, never hint text).
- **Mount affordances (click ≡ key)**: task detail gains an
  "Add repeat…" affordance for rule-less tasks (adjacent to the due/sched
  rows) and the existing recur row gains an "Edit repeat…" control beside
  the pattern echo — both IconButton-with-label, walk-ordered,
  hover-tipped (the row's stated doctrine, `task-view.slint:728-730`).
  New `ActionId::SeriesEdit` ("series.edit") in the `TaskDetail` context;
  free keys chosen per preset (default/vi/emacs) keeping the preset
  totality invariant tests green; keymap-generated hints pick it up
  automatically. Kiosk: rides the task-detail editing gate like every
  other action.
- **Adapter plumbing**: the draft buffer lives adapter-side (the
  due-editor precedent — buffers + facts globals, no transient UI state
  in Slint). **Commit** dispatches `StateCommand::UpdateTask { changes:
  TaskChanges { recur: Some(Some(rule)), ..Default::default() } }`;
  **"Stop repeating"** (an armed-confirm strip inside the editor, the
  convert precedent) dispatches `Some(None)` — the detach semantic.
  Everything downstream is already live and tested: engine clock/author
  stamping, the `UpdateAppData` sidecar lane, completion spawn, the
  card badge, pause/resume/convert.
- **Tests**: domain assembly unit tests + a proptest (mirror the CLI
  verb's existing tests, moved with the function); presentation
  draft→rule mapping tests; an adapter seam test asserting the exact
  command dispatched (the `banner_actions_send_exact_commands` shape);
  keymap totality tests already cover the binding.

### D-7.2 — Create habits from the Today section (R7-2)

- **Section header affordance**: `HabitsSection`'s header row
  (`habits.slint:63-76`) gains an add `IconButton` (plus icon, label
  "Add habit…") gated by `editing-visible` exactly like the row actions
  (D-T16: kiosk renders rows, never actions). New callback
  `create-asked()` up through `today-view.slint` / `app-window.slint`.
- **Inline create form**: mounted below the header while armed — the
  stack-create inline-field precedent, extended with target picks:
  title `TippedField` (insert mode), target `SegmentedControl`
  (Per day / Per week / Per month), quota numeric field
  (`NonZeroCount::new`, default 1 — the count the CLI's target phrases
  accept), confirm/cancel IconButtons. Confirm dispatches
  `StateCommand::CreateHabit { board, title, target }` with **board =
  the section's own board** (the model pass already scopes the section
  to one board, `models.rs:742-744` — no picker needed); when no board
  is active the section renders no rows and the affordance does not
  mount. Create carries no optional fields (D-H7a stands); retargeting
  later rides the existing `UpdateHabit`.
- **Action registry**: new `ActionId::HabitCreate` ("habit.create"),
  section-scoped group in the `Today` context; the mouse affordance and
  the keyboard binding route through the same action id; free keys per
  preset chosen at implementation (registry-generated hints stay
  honest); stale-arm no-ops follow the one-shot target-field law.
- **Tests**: adapter seam test asserting the exact `CreateHabit`
  command (board resolution + minted target); presentation test for the
  gated mount flag; the engine path is already snapshot-covered (s45
  family) — no new engine tests.

### D-7.3 — The Files section becomes the per-account board roster (R7-3)

**Taxonomy decision — channel-relative.** Within an account card's Files
section, "this Connection" = *that card's files lane*, which is live only
while the card is the resolved anchor and `[files_sync] enabled` holds
(`files_on`, `adapter.rs:4841-4842`). A board bound through any Deck
connector — including the same account's — reads "Synced via other
Connection". The single-lane posture (#723) stands: non-anchor accounts
render the full roster with the files actions disabled (the Files row's
existing restart caption explains why); true multi-lane stays #723.

- **Snapshot widening (one field)**: `BoardInfo` gains
  `bound_channel: Option<BoardChannel>` (`enum BoardChannel { Deck,
  Files }`), filled app-side in `current_connections` from
  `binding.board.as_deck()/as_files()` — structural detection per D-BF4,
  never a name match. `ConnectorInfo` (already has
  server/username/backend) + `files_sync_anchor` make the account join
  and is-this-anchor fold pure presentation work; no other plumbing.
- **Badge fold** (presentation, sibling of `deck_bound_boards`): new
  enum `FilesBoardBadge { ThisConnection, LocalOnly, OtherConnection }`
  with a tone map — ThisConnection → `Tone.success`, LocalOnly →
  `Tone.neutral`, OtherConnection → `Tone.info` — rendered by the
  existing `Badge` widget (`ui/widgets/badge.slint`; alpha-tinted
  backgrounds are built in). Labels verbatim from the request:
  "Synced via this Connection" / "No Sync · Local Only" / "Synced via
  other Connection". No new `Tone` variants (adding one is a theme
  decision; the closed enum's three states are covered).
- **Sort fold**: per account — ThisConnection, then LocalOnly, then
  OtherConnection; within a group by (title, id) for determinism (no
  engine-map-order leakage). A pure fold; kiosk-safe by construction.
- **Row grammar**: the files-board loop (`adapter.rs:4867-4883`) is
  replaced: every live, non-deleted (non-archived: show them flagged
  archived — visibility of archived boards is the roster's job too)
  board emits once **per Nextcloud account card** as
  `ConnectionItem { kind: files-board }` carrying badge text/tone and
  action label/enabled — strings and flags computed Rust-side, the UI
  stays dumb. Each row gains one `SettingsButton` whose label is the
  contextually sensible action; it joins the walk like Remove/Refresh,
  so keyboard parity in the Settings context (Nav next/prev + invoke,
  `presets.rs:232-236`) is free.
- **Actions**:
  - **ThisConnection → "Stop Sync"**: new
    `UiRequest::StopBoardFilesSync { board, reply }` → app arm guards
    (board live, binding Files-typed; anything else → typed refusal)
    then `StateCommand::UnbindBoard` on the engine handle; reply = the
    fresh `ConnectionsSnapshot` (the standard refresh discipline). This
    **supersedes round-6 D-6's "disconnect is a non-goal" ruling** —
    the row comment records the overturn with this plan as authority.
    Known honest edge, filed as backlog (§4): unbind leaves the remote
    `boards/<uuid>/` subtree in place; fold-first discovery defers
    adoption for unbound boards (`board_cycle.rs:208-212`) so nothing
    re-adopts it, but the bytes linger until #718's DELETE-verb GC
    exists (filed as #755).
  - **OtherConnection → "Switch to Sync via this Connection"**: enabled
    only on the anchor card with the lane on; reuses
    `UiRequest::MoveBoardToFiles` verbatim (the ceremony, `BusyDot`,
    `PendingKind::Move`, typed error fold all exist). The
    `FilesMovePicker` dropdown **retires** — the roster is the picker:
    one affordance per row replaces the aggregate dropdown; remove the
    picker component, its copy global, and the `move_choices` table.
  - **LocalOnly → "Sync via this Connection"**: new
    `UiRequest::PublishBoardToFiles { board, reply }` → app arm guards:
    files spawn decision resolvable (`files_sync_spawn_decision`), board
    live, binding `None` (Deck-bound → typed `BindRefused` pointing at
    Switch; Files-bound → the row would have offered Stop). Ceremony =
    the move ceremony's final assign step —
    `BindBoard { remote: RemoteBoardId::files(board_uuid), connector:
    FILES_SYNC_CONNECTOR_NAME }` (`bootstrap.rs:103`) — reusing the
    shared engine-handle run pattern (the publish-board arm
    precedent). The files actor's board lanes re-derive their board set
    from `BoardOwnershipMap` every cycle, so the bound board is served
    on the next tick and its remote subtree is published (fold-first
    adoption covers pre-existing subtrees, `board_cycle.rs:561`). **A
    local board reaches the account's Files without ever touching Deck** —
    the user's scenario, closed. Restart-pending enable (config written,
    actor not yet running) succeeds as a state change with the badge
    flipping on bind (board truth) and the lane catching up next boot —
    the same posture the On/Off toggle's caption already documents.
- **Tests**: presentation fold tests over a synthetic AppState ×
  snapshot matrix (anchor card / non-anchor card / lane off: badge,
  sort, action label, enabled flag — asserted as enums/flags, never
  label text); adapter roster-push tests (one row per board per card);
  app-arm tests for the two new requests (typed refusals + the planned
  `UnbindBoard`/`BindBoard` + snapshot reply), mirroring the move arm's
  existing tests. No engine changes → engine suites untouched.

## 3. Accessibility ledger (both requests demand mouse + keyboard parity)

All three solutions route every new affordance through the action layer
("click ≡ key" doctrine), so parity is structural, not per-feature:

- **D-7.1**: `SeriesEdit` action id in `TaskDetail`; the editor overlay
  is a mask context whose fields join the walk and ride `ViInsert`;
  commit/cancel are in-context verbs; Escape resolves via the global
  law. Hint projections (kbd-hint widgets) pick the binding up from the
  registry — never hand-written.
- **D-7.2**: `HabitCreate` action id in `Today`; the inline form's
  controls are walk-ordered; title field commits on Return (the create-row
  precedent), Escape cancels through the action layer.
- **D-7.3**: roster rows live in the Settings context where every enabled
  control is a tab stop (the item list's stated rule,
  `settings-view.slint:975`); the per-row action button is a
  `SettingsButton` and inherits walk + invoke; no new flyouts, so no new
  focus choreography.

## 4. Non-goals and filed follow-ups

- **No at-create scheduling** in quick capture or the inline create rows —
  the recorded ruling stands; the editor is the setting.
- **No multi-account files lanes** (#723): non-anchor accounts get
  disabled actions with honest captions, not lanes.
- **Filed — stop-sync subtree leftover (#755)**: unbinding a files board
  leaves the remote subtree until GC exists (#718 carries the DELETE
  verb; #755 records the marker-or-GC design question).
- **Filed — Deck→Deck board move (#756)**: a board bound to Deck
  connector A cannot switch to Deck connector B (no ceremony; only
  Deck→Files exists). Out of scope; the roster marks such boards
  "Synced via other Connection" truthfully.
- **No new `Tone` variants**, no theme changes.
- **No habit target editing UI** beyond create (the CLI's `habits
  update` covers it; not requested).
- **No engine/domain/state/storage schema changes** anywhere in this
  round; D-7.1's domain extraction is a pure-function move plus the CLI
  refactor onto it.

## 5. Proposed chunking (post-acceptance dispatch input)

| Chunk | Contents | Branch seed | Size |
|---|---|---|---|
| 1 — habit create | D-7.2 (UI + adapter + registry only) | `feat/ui-r7-habit-create` | S–M |
| 2 — recurrence editor | D-7.1 (domain extraction + CLI refactor + UI) | `feat/ui-r7-recur-editor` | M–L |
| 3 — files roster | D-7.3 (snapshot field + presentation folds + adapter + two app arms + picker retirement) | `feat/ui-r7-files-board-roster` | M |

Chunks are independent (disjoint files; each off `main`, one PR per
chunk per the dispatch protocol). Chunk 2 is the only one that steps
below the UI crate (a pure-function extraction + the CLI verb refactored
onto it — still no schema or message change). Suggested order: 1 → 2 → 3
(smallest first de-risks the shared registry/facts machinery before the
larger editor lands). Quality gates run through
`scripts/run-quality-gates.sh` per AGENTS.md §10; hand-off at PR-open +
green checks; the worktree is removed at hand-off.

# [Plan] Time Tracking Subsystem — Design Round (open entries, board-section roaming, track & export surfaces)

- **Date**: 2026-10-26
- **Kind**: design-round doc — the next round of the backlog-triage
  dispatch's §14 design-round queue
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md:979-1008`,
  after DR-1..DR-3 landed and DR-4 `docs/sync-merge-policy-design` entered
  flight): **DR-5 on issue #210**, the roadmap's Wave-2 "focus &
  reflection" anchor feature. Design only — no production code in this
  commit.
- **Origin**: issue **#210** (normative: the `TimeEntry { id, task, start,
  end, user?, kind flags }` sketch, Start/Stop/Adjust commands, running
  timer as runtime intent, export as pure projections, the follow-on
  fences); the future-directions analysis §3.3 + Wave-2 row
  (`.artifacts/reports/2026-10-14-future-directions-analysis-report.md:117-124,
  295`); DR-1's R3.5 cross-link (#210 comment 2026-10-25: the reminder
  notification surface gains **Start Task** carrying implicit dismissal,
  riding `DismissReminder`); the in-tree reservation
  `crates/taskboard-ui-slint/src/adapter.rs:3300-3304`.
- **Session rulings (2026-10-26, recorded in §4)**: **core subsystem
  only** (follow-ons fenced to §7 issues with vocabulary reserved);
  entries **roam via the appdata sidecar**; the section is
  **board-level** — tracked-time history **outlives task deletion**;
  the running timer **is the open entry** (`end: null`) and roams with
  it (the *synced intent* ruling — a deliberate deviation from the
  2026-10-14 sketch, §3-A); **single open entry globally**; **task done
  / task deleted** close it (no runaway cap); UI v1 = **chip +
  affordances** (totals section and detail history become backlog
  issues); **manual log/adjust/delete in v1**.
- **Branch / worktree**: `docs/time-tracking-design-round` in
  `../taskboard-WT-docs-time-tracking-design` (off `origin/main` @
  `4686af8`).
- **Status**: **PROPOSED** — awaiting user review. Acceptance opens the
  implementation dispatch round over §7's chunk issues (the §14
  DR-3 precedent: a follow-up dispatch plan chunks them).

---

## 1. Context & verified anchors (do not re-derive; verified 2026-10-26 against the round's base `origin/main` @ `4686af8`)

Symbols are the stable keys; line numbers are the base's.

| Fact | Anchor | Evidence |
|---|---|---|
| The Task shape is field-family-complete and soft-deleting | domain `entities.rs` | `Task` :401-440 (title, description, duedate, scheduled, done, stack, order, labels, effort/importance/urgency, archived, **deleted**, `TaskClocks` :277-303); `DeleteTask` sets `deleted = true`, cascades rule deletes, keeps the binding until push confirms (command.rs :612-637) |
| Nested sub-states are the AppState pattern | domain `state.rs`, `reminders.rs`, `persistence.rs` | `AppState` :24-45 (boards/stacks/tasks/labels/**reminders: ReminderState** :38/sync/last_updated); `ReminderState` :408-430 (maps + sets); mirrored on `PersistedState` (persistence.rs :82-102) |
| Determinism-by-construction is pinned at the state doc | domain `state.rs` | :4-8 — `BTreeMap<Id, Entity>` everywhere for deterministic serialization/merge convergence; a time-entry map follows |
| Commands are planned pure, `now` passed as data | domain `command.rs`, `clock.rs` | `plan_command(app, outbox, command, ids, defaults, now) -> CommandPlan` :322-329; `CommandPlan { actions, outcome }` :47-53; typed `CommandError` :59-116; receipt enum `CommandOutcome` :120-198 (`NoOp` :195 the idempotent-repeat precedent); crate rule "nothing calls `Utc::now()`" (clock.rs :4-9) |
| `SetTaskDone` is the implicit-side-effect precedent | domain `command.rs` | :546-591 — stamps done + clocks, **auto-emits dismissals for active occurrences**, enqueues `UpdateAppData`; `StartTracking` reuses exactly this machinery (§3-B) |
| Ids: one macro, two families | domain `ids.rs`, `idgen.rs` | `local_id!` :13-39 (Uuid newtypes); roaming family `BoardId/StackId/TaskId/LabelId/ReminderRuleId` :57-60 vs **device-local** `AdhocAlarmId` :61-65 ("never synced"); v7 mint only in the planner via `IdGenerator` (idgen.rs :16-65) |
| Persistence is per-entity action batches | domain `persistence.rs`, storage `repo.rs` | `PersistenceAction` :177-237; one transaction, fixed FK order, **exhaustive match, no wildcard** — a new variant breaks the storage crate's build by design (repo.rs :853-878); engine applies to storage **before** interpreting into memory (actor.rs :641, core.rs :58) |
| The sidecar substrate is the D1 answer | domain `appdata.rs`, sync `anchor.rs`/`flush.rs` | per-board JSON attachment `taskboard-appdata.json` on the `"[taskboard] app-data"` anchor card (anchor.rs :27-36); `AppDataSections { tasks, reminder-rules, dismissals, unknown }` :65-84 (reserved names `"proofs"`/`"settings"` :60-64); flush runs **between the cycle's pull and its report**, one PUT (flush.rs :146, ladder :385-398) |
| Sidecar merge is embedded-clock LWW union, additive verbatim relay | domain `appdata.rs` | `union(base, overlay)` :578-651 — newer `clock` wins, ties → overlay, unknown additive; `RuleEntry { Rule, Tombstone, Verbatim }` :346-360 the row-enum shape (tombstones compete by clock, `Verbatim` never ordered) |
| The new-section recipe exists, twice-proven | domain `appdata.rs`, `merge.rs`, `pipeline.rs` | the dismissals wave's 8 steps: entry struct with hand-written tolerant serde (:447-495), `clock()` row helper (:511-520), `Option<BTreeMap>` section (:79-80), per-key union wired into `union()` (:618-632), `prune_absent_cards` arm (:755-758), overlay builder (:856-882), tombstone derivation with the **truncated-view guard** (:893-909), `fold_*` in merge.rs (:522) called from the pipeline fold pass (:668-728, rules before dismissals :712-717) |
| `prune_absent_cards` strips **card-keyed** sections only | domain `appdata.rs` | :733-760 — filters `tasks`/`reminder-rules`/`dismissals` by present-card set; a board-level section keyed by entry UUID is invisible to it (the load-bearing fact behind the §4 history ruling) |
| Document-side lane GC has a precedent | domain `appdata.rs` | `lane_gc_scheduled` :917-925 strips a modeled lane from the document while the connector owns it natively — the shape §3-D reuses for retention aging |
| Sidecar writes ride the outbox | domain `outbox.rs`, `push.rs` | `LocalOp::UpdateAppData(TaskId)` :59; ops **coalesce per board into one flush group** (push.rs :371-377, :419-429); `MaterializedOp::FlushAppData { board }` :136-139 (content-free — flush materializes from state); note the stale doc comment at outbox.rs :55-59 ("nothing enqueues it yet" — six call sites do; chunk 2 may fix the comment in passing) |
| Card→task resolution exists for folds | domain `merge.rs` | `fold_reminder_rules` :437 imports the card-keyed rules section — the card-ref mapping §3-D's fold reuses |
| Migrations: next free is contested | storage `migrations/` | 0001-0009 present; **DR-3 chunk 2 claims 0010** (`2026-10-26-reminders-surface-pair-design-round-plan.md` D-I4) — this round's chunk 2 takes **0011** unless timing says otherwise (recorded collision note) |
| No export, no tracking, no user identity exists | workspace-wide | zero CSV/JSON-export surface (CLI verb list cli.rs :29-159 has none); zero `TimeEntry`/`StartTracking`/stopwatch symbols; no `user`/`actor` field anywhere in domain — the only username is config-layer (`ConnectorCfg.username`, config.rs :243; accessor :1104) |
| Day windows are pure and preference-fed | domain `days.rs`, app `config.rs` | `day_window(now, day_start_time, offset)` :41-56; `today_sections` :107-150; `[preferences] day_start_time` default `"00:00"` (config.rs :331-357), parsed once at the app layer (ui.rs :241-254) — consumers receive `(day_start_time, offset)` as data |
| CLI additive verbs have a two-lane recipe | app `commands/timers.rs` | one-shot wrappers via `with_core` + session `*_on` seams (timers.rs :25-199); `collapse()` the single definition site (:289-308); typed receipts in `output.rs`; state read back from the published snapshot, never echoed (:73-100) |
| IPC relays raw argv — new verbs need no protocol change | app `main.rs`, `ipc/server.rs` | `route_for_ipc` is exhaustive — the compiler forces every new verb to pick a lane (commands/mod.rs :1832-1871); daemon is the single parse authority (server.rs :515-599) |
| The engine loop owns its tick; the notifier is a separate consumer | state `actor.rs`, app `bootstrap.rs` | 4-arm biased select incl. the reminder tick arm :540-552; `spawn_notifier` :871 — §3's "no ticking task anywhere" holds: elapsed is always derived at read |
| UI rows with buttons live on banner/notification surfaces only | ui-slint | `push_notifications` :376-410 (kiosk model-side gate :385-389); per-row buttons at notification-surface.slint :49-81 → one `StateCommand` per button (adapter.rs :3305-3368); **cards carry no per-row buttons** (task-card.slint :49-91) — the context menu is the card's affordance surface (r2 actions work) |
| The Start Task button is reserved in-tree | ui-slint `adapter.rs` | :3300-3304 — "No Start Task button — deferred to the time-tracking subsystem (issue #210), which is charged with adding it and carrying the implicit-dismissal semantic" |
| Repaint gives minute-granularity time text for free | ui-slint `lib.rs` | 100 ms driver drains signals; refresh fires on dirty **or wall-minute boundary** (:356-422) — a minute-bucketed elapsed chip needs no repaint change |
| Every wave got its ADR | `docs/adr/` | 0013 reminders, 0014 attach, 0015 slint, 0016 kiosk — this round's chunk 1 writes **ADR 0017** |

Out-of-scope fences that stay binding: the follow-on family of #210 —
tracker task type, respite marking, per-user attribution (Wave 4),
break reminders, pomodoro mode — is **fenced to §7's backlog issues**,
not designed here (the session's scope ruling); DR-4
(`docs/sync-merge-policy-design`, in flight) owns `merge.rs`/
`pipeline.rs` **task-field merge semantics** (#128-#133) — this round's
fold follows the landed additive-section pattern and must not touch
DR-4's subject matter; DR-3 chunk 2 owns migration **0010**; GUI review
round 3 owns the OS-delivery/visual plumbing (this round's notification
button rides the existing surface, it does not repair it); the runaway
auto-close cap was **considered and declined for v1** (session ruling —
runaways are fixed by hand via `track adjust`; revisit only with real
poisoned-statistics evidence).

## 2. Scope & non-goals

**In scope (design only):** the `TimeEntry` entity and its
`TrackingState` home (Part A); the command family and planner
semantics — start/stop/log/adjust/delete, implicit dismissal,
done/deleted close, single-global invariant (Part B); persistence —
migration, codec, actions (Part C); roaming — the board-level
`time-entries` sidecar section, union, tombstones, retention aging,
fold, flush trigger (Part D); the surfaces — CLI `track` family + the
`export` verb, the UI chip/card/detail/notification affordances, the
kiosk fence (Part E); decision record §4; test hooks §5; chunk mapping
§7.

**Non-goals:** any production code; tracker task type / respite /
attribution / break reminders / pomodoro (§7 fences); a today-view
totals section and task-detail entry history (session ruling: backlog
issues, not v1); statistics/charts/PDF (Wave 2 follow-ons ride the
entries later); pomodoro or any clock-driven auto-behavior (no ticking
task exists in this design); a `[tracking]` config section (constants
named in code; knobs fenced); changing DR-3's or DR-4's subject matter;
sub-second elapsed rendering (minute granularity v1).

---

## 3. Design

### Part A — The entity and its state home (D-T1–D-T3)

- **D-T1 — `TimeEntry` is a settled historical record with an open
  mode.**

  ```rust
  pub struct TimeEntry {
      pub id: TimeEntryId,          // local_id!, Uuid v7 — ROAMING family
      pub task: TaskId,             // reference survives soft delete
      pub task_title: String,       // snapshot at start (see below)
      pub start: DateTime<Utc>,
      pub end: Option<DateTime<Utc>>,   // None = open = the running timer
      pub created_at: DateTime<Utc>,
      pub updated_at: DateTime<Utc>,    // the LWW mutation clock
  }
  ```

  The **open entry is the running timer** (the session's synced-intent
  ruling). This deliberately deviates from the 2026-10-14 sketch
  ("the running timer stays app-runtime state, persisted only as an
  in-flight intent; a clock-driven app task stamps the end") — recorded
  reason: the daemon+IPC architecture landed since (ADR 0009/0012) — a
  CLI `track start` relays to the daemon and exits, so the intent must
  survive process death as durable state anyway; and an open entry
  **is** a settled write in the sketch's own sense — a row that exists,
  whose end field is unterminated. The sketch's actual invariant
  ("engine truth remains settled writes"; no clock-driven stamping
  task) is preserved exactly: **nothing ticks** — elapsed is always
  derived at read (`now − start`), and the end is stamped only by a
  command (stop, done, deleted-close, adjust) at planner `now`, from
  any device.
- **D-T2 — `task_title` is an immutable snapshot taken at start.** A
  time tracker's history must stay readable after the task is renamed
  or deleted; the entry freezes the title it was started under
  (display/export read the snapshot, never a live join). Renames do
  not rewrite history — intended, recorded.
- **D-T3 — the state home: `TrackingState`, nested like
  `ReminderState`.** New domain module `tracking.rs`;
  `TrackingState { entries: BTreeMap<TimeEntryId, TimeEntry> }`;
  mounted as `AppState.tracking` and mirrored on `PersistedState`
  (the reminders precedent, §1). **No `running` field** — the open set
  is derived (`entries.values().filter(|e| e.end.is_none())`); one
  source of truth. `TimeEntryId` joins the roaming id family (§1 ids
  anchor): the wire key is the entry UUID, exactly as rule UUIDs are
  the rules section's remote identity (D-S9 precedent). A1 discipline
  (AGENTS §2): span validity (`end > start` whenever end is known) is
  enforced at the trust boundary — the planner rejects with a typed
  error; no derive crates; leniency boundaries documented at the type.

### Part B — Commands and planner semantics (D-T4–D-T8)

- **D-T4 — the verb set.** Five new `StateCommand` arms (messages.rs):

  | Command | Payload | Receipt (CommandOutcome) |
  |---|---|---|
  | `StartTracking` | `{ task: TaskId }` | `TrackingStarted { entry, closed: Vec<TimeEntryId> }` |
  | `StopTracking` | `{}` | `TrackingStopped { entries: Vec<TimeEntry> }`; none open → `NoOp` |
  | `LogTime` | `{ task: TaskId, start, end }` | `TimeLogged { entry }` |
  | `AdjustTimeEntry` | `{ id, start: Option<_>, end: Option<_> }` | `EntryAdjusted { entry }`; no field → `NoOp` |
  | `DeleteTimeEntry` | `{ id: TimeEntryId }` | `EntryDeleted { id }`; repeat → `NoOp` |

  Typed errors reuse the existing families: `UnknownTask`,
  `UnknownTimeEntry` (new), `InvalidSpan` (new: `end ≤ start`, or
  `--from` after `--to` at the CLI layer's parse). Errors are typed
  outcomes — no text assertions anywhere downstream (AGENTS §5).

- **D-T5 — `StartTracking` closes, then opens, and settles the task's
  notifications.** Planner order: resolve task (`UnknownTask`) →
  **close every open entry** at `now` (the single-global ruling:
  starting anywhere stops what any device left running — under LWW a
  remote close is simply the newer write) → mint the new entry
  (`start = now`, snapshot title) → **emit dismissals for the task's
  active reminder occurrences** — the exact `SetTaskDone` machinery
  (§1 anchor), so the notification surface's Start Task button needs
  no second command (the #210 cross-link's implicit-dismissal semantic
  lands in the planner, the Mark Done precedent) → enqueue
  `UpdateAppData(task)` for every touched task. Starting a task that
  already has *the* open entry is an idempotent `NoOp` (the DeleteTask
  repeat precedent).
- **D-T6 — `StopTracking` closes all; done and deleted close theirs.**
  `StopTracking` closes every open entry at `now` (single-global means
  that is the one; the loop is the honest shape) — the stop-from-any-
  device ruling. Two existing planners grow side-effects:
  `SetTaskDone` closes the done task's open entries at the done stamp;
  `DeleteTask` likewise (no orphaned timer on a dead task — session
  ruling). Both already enqueue `UpdateAppData`; the close rides along.
- **D-T7 — `LogTime` is the retroactive entry; `AdjustTimeEntry` can
  close but never reopen; `DeleteTimeEntry` is global.** `LogTime`
  creates a **closed** entry from an explicit span (validated
  `end > start`); it never touches the open set (logging yesterday's
  forgotten block while today's timer runs is legal — distinct
  entries). `AdjustTimeEntry` changes fields (`None` = unchanged, both
  `None` = `NoOp`); the resulting span must validate; **setting an end
  on an open entry closes it** (a manual close with a specific time —
  the forgotten-timer remedy, replacing the declined runaway cap);
  **clearing an end is not expressible** — history editing must not be
  able to resurrect a running timer (the payload's `Option` is the
  change-mask, never a value). `DeleteTimeEntry` removes the row and
  drives a wire tombstone (D-T12) — deletion is **global**: it roams
  and removes the row on every device (deliberate deletes propagate;
  retention aging does not — asymmetric by design).
- **D-T8 — the single-global invariant is soft, and that is recorded.**
  Two devices starting concurrently (before either pull sees the
  other's open entry) union into **two open entries** — the merge
  cannot prevent it, and no reader breaks: the chip shows the
  most-recently-started open entry (tie-break: entry id), `track
  status` lists all, and the next start or stop **anywhere** closes
  all. The invariant every writer maintains locally ("at most one open
  after I act") converges to globally-zero-or-one at the next write;
  the properties pin exactly this (§5). No device identity exists or
  is introduced (attribution stays fenced, §7).

### Part C — Persistence (D-T9)

- **Migration 0011** (0010 is DR-3 chunk 2's; the implementing chunk
  re-verifies the next free number at its base and records any
  collision):

  ```sql
  CREATE TABLE time_entries (
      entry_id   TEXT PRIMARY KEY NOT NULL,
      task_id    TEXT NOT NULL,
      task_title TEXT NOT NULL,
      start      TEXT NOT NULL,
      end        TEXT,                -- NULL = open
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
  );
  ```

  The 0008/0009 recipe: hyphenated-UUID TEXT ids, RFC 3339 TEXT
  stamps, no FKs (soft-deleted tasks keep their rows; entries outlive
  them by design). New `PersistenceAction::UpsertTimeEntry` /
  `DeleteTimeEntry` (the exhaustive storage match forces the arms —
  §1); codec text fns + `decode_time_entry` (repo.rs); no separate
  intent table — **the open row is the intent** (crash-safe by
  construction: daemon restart rehydrates the open entry and the
  timer "resumes" because it never stopped).

### Part D — Roaming: the board-level sidecar section (D-T10–D-T13)

- **D-T10 — a board-level `"time-entries"` section keyed by entry
  UUID.** New `AppDataSections` field
  `time_entries: Option<BTreeMap<String, TimeEntryRecord>>` (serde
  rename `"time-entries"`, default + skip-if-none — the recipe's step
  3). The row enum follows `RuleEntry`:

  ```rust
  enum TimeEntryRecord {
      Entry { task: Option<String>, title: String, start, end: Option<_>, created, clock },
      Tombstone { clock },
      Verbatim(Value),
  }
  ```

  Wire shape (additive; old devices relay the section verbatim —
  mixed fleets fold nothing, lose nothing):

  ```json
  "time-entries": {
    "0192…": { "task": "412", "title": "Write chapter 3",
               "start": "…", "end": "…|null", "created": "…", "clock": "…" },
    "0193…": { "deleted": true, "clock": "…" }
  }
  ```

  `task` is the decimal remote card id. **Unbound tasks are never
  emitted** (`task.remote == None` → device-local entry; a null-task
  wire row folds as unresolvable and is skipped) — an entry without a
  card has no identity another device could resolve.
- **D-T11 — union is per-key clock LWW; open→closed is just a newer
  write.** `union_time_entries` (recipe step 4): newer `clock` wins,
  ties → overlay, `Verbatim` never ordered, tombstones compete by
  clock (a delete after an adjust wins; a later-dated live row beats
  an older tombstone — the `RuleEntry` semantics unchanged). The
  remote stop of an open entry arrives as `Entry` with `end` set and a
  newer `clock` — no special case. `prune_absent_cards` gains **no
  arm** for this section (board-level keys are invisible to it — §1:
  the load-bearing fact): history survives task deletion, which is
  the ruling's point.
- **D-T12 — deletion roams via derived tombstones; aging does not.**
  Overlay builder `time_entries_for(state, board, now)` emits live
  entries of card-bound tasks; **tombstone derivation reuses the
  dismissals wave's truncated-view guard**: tombstone only
  base-present live entries that are **within the retention window
  and whose card resolves locally** — a foreign entry this device
  cannot resolve is neither imported nor deleted (a narrower view can
  never erase another device's history; unresolvable entries age out
  by D-T13 instead). Fold (`fold_time_entries`, merge.rs — recipe
  step 8, called after `fold_reminder_dismissals` in the pipeline
  pass): resolve `task` card → `TaskId` (the `fold_reminder_rules`
  mapping), apply per-entry LWW against local `updated_at`, import
  live rows, apply tombstones newer than the local row as **local
  removal** (deletes are global), skip unresolvable.
- **D-T13 — retention aging is document-side, wire-only, constant.**
  `TRACKING_SYNC_RETENTION_DAYS = 180` (named constant; the
  `[tracking]` config knob is fenced): at flush,
  `lane_gc_old_time_entries(document, now)` strips entries whose
  `start` (and tombstones whose `clock`) fell out of the window —
  the `lane_gc_scheduled` shape — and the overlay builder filters by
  the same window, so an aged entry is never re-emitted and the union
  cannot resurrect it. **The local DB never drops history**: aging is
  a wire-compaction concern (attachment size and per-flush upload
  cost), not a data-retention policy; `export` reads full local
  history. Mixed-fleet note: a pre-round device relays the section
  verbatim (its overlay is built by old code, without the filter) —
  a stale device re-uploading an aged entry is possible until it
  updates; the next round-device flush re-ages it. Accepted, recorded
  (same one-version tolerance every section already has).
- **Flush trigger:** every tracking mutation enqueues
  `LocalOp::UpdateAppData(TaskId)` (task-scoped, coalesces per board —
  §1); soft-deleted tasks keep local rows, so the op stays resolvable
  for post-deletion adjusts/deletes. Chunk 2 fixes the stale
  outbox.rs :55-59 doc comment in passing (one line, in-scope: it
  documents this exact op's enqueue reality).

### Part E — Surfaces: CLI, export, UI (D-T14–D-T18)

- **D-T14 — the CLI family: `taskboard track …`** (the timers
  two-lane recipe: one-shot wrappers + session `*_on` seams + one
  `collapse()` definition site):

  ```
  track start <task-selector>          # closes what runs, opens, settles notifications
  track stop                           # closes all open entries
  track status                         # open entry (if any) + elapsed + tracked-today total
  track list [--task <sel>] [--since <date>]
  track log <task> (--for <dur> | --from <ts> --to <ts>)
  track adjust <entry-id> [--start <ts>] [--end <ts>]
  track delete <entry-id>
  ```

  Selectors reuse the task-resolution machinery; entry ids resolve
  like `resolve_alarm` (exact uuid → unique ≥8-char prefix). `status`'s
  tracked-today total uses the `[preferences] day_start_time` window
  (the app-layer parse seam, §1) with entries attributed by
  **splitting spanning entries at the day-window boundary** — a pure
  projection (`tracked_in_window(entries, window)` in `tracking.rs`);
  entries are stored whole and never physically split (aggregation is
  a read, the days.rs no-derived-bit rule). Durations render as
  `1h 30m` phrases (human) and whole seconds (machine columns).
  Receipts typed (output.rs); `route_for_ipc`: `Track` → `IpcOrLocal`
  (mutating — the daemon's engine holds the open entry; a one-shot
  local boot would orphan its intent when the process exits — the
  timers precedent routes identically).
- **D-T15 — `taskboard export`**: a new top-level verb — pure
  read-side projection to **stdout**:

  ```
  taskboard export [--format csv|json] [--since <date>] [--until <date>]
                   [--task <sel>] [--board <sel>]
  ```

  Default CSV (spreadsheet-first; columns: `entry_id, task_id,
  task_title, start, end, duration_seconds`); `json` emits an array of
  entry objects. Filters are inclusive on entry start. Raw payload on
  stdout — export **is** data, so it does not ride the `--json`
  receipt envelope (recorded; the journal/statistics exports of Wave 2
  reuse this verb). Lane: `IpcOrLocal`; the daemon relays captured
  stdout bytes verbatim (main.rs `relay`). No file writing, no
  clipboard — pipes are the contract.
- **D-T16 — the UI v1 surface: a status chip, card affordances via
  the context menu, a detail button, and the reserved Start Task
  button.**
  - **Status chip** (`status-bar.slint`): while an open entry exists —
    task title + elapsed phrase (`12m`) + a **Stop** IconButton
    dispatching `StopTracking` (the banner-row one-command pattern).
    Minute-bucketed elapsed renders free on the wall-minute repaint
    (§1) — no repaint-driver change. **Kiosk: the chip renders
    read-only (no button)** — the model-side gate empties the action,
    the passive display stays (a kiosk showing what is being worked
    on is the appliance's win); tracking commands are desktop/REPL
    surface.
  - **Task cards**: the affordance is a **context-menu arm**
    (Start/Stop tracking) plus a small *tracking* badge on the card
    whose task holds the open entry — **cards keep their no-per-row-
    buttons rule** (§1; the context menu is the card's affordance
    surface by the r2 actions design). The badge is pure derivation
    from state.
  - **Task detail** (`task-view.slint`): a Start/Stop button beside
    the State cell.
  - **Notification surface**: the **Start Task** button on
    task-anchored rows (anchor-aware visibility — no button on Adhoc
    rows, the DR-3 chunk 3 pattern); it routes the **same**
    `ActionId::TaskTrackStart` with `focused_task` set — implicit
    dismissal lives in the planner (D-T5), so the button dispatches
    exactly one command, the Mark Done precedent. The adapter.rs
    :3300-3304 reservation resolves.
  - **Action layer growth is minimal**: `ActionId::TaskTrackStart`,
    `TaskTrackStop`; router arms map to the two commands; no UI-held
    timer state (elapsed is derived per frame from the ArcSwap
    snapshot; ADR 0015's dumb-client rule untouched).
- **D-T17 — pure presentation functions**, in `presentation.rs` (the
  `notification_phrase` precedent): `open_elapsed_phrase(start, now)`
  (minute buckets), chip visibility, badge visibility, kiosk gate —
  all unit-tested with structural assertions, no slint run loop.
- **D-T18 — ADR 0017** ("time tracking — settled open entries,
  single-global intent, board-section roaming") is written by chunk 1
  (the wave-ADR convention, §1): it records the synced-intent
  deviation from the 2026-10-14 sketch (D-T1's reason), the
  open-entry model, the retention aging policy, and the fence list.

---

## 4. Decision summary

| ID | Question | Decision | Status |
|---|---|---|---|
| D-T1 | Running-timer home | **The open entry (`end: null`) IS the timer** — engine state, persisted, roaming; nothing ticks, elapsed always derived; deliberate deviation from the 2026-10-14 app-runtime sketch, reason recorded (daemon+IPC landed; open row is a settled write in the sketch's own sense) | **RULED (2026-10-26 session)** |
| D-T2 | History readability | `task_title` snapshot at start, immutable — history survives rename and deletion | recorded |
| D-T3 | State home | `TrackingState { entries }` nested like `ReminderState`; no `running` field (derived); `TimeEntryId` joins the roaming id family; A1 span validation at the planner | recorded |
| D-T4 | Verb set | Start/Stop/LogTime/Adjust/DeleteTimeEntry + typed receipts/errors (`UnknownTimeEntry`, `InvalidSpan`); idempotent repeats → `NoOp` | recorded |
| D-T5 | Start semantics | Close-all-open → open → **implicit dismissal of the task's active occurrences** (SetTaskDone machinery — the #210 cross-link lands planner-side) → UpdateAppData | recorded |
| D-T6 | Auto-close | `StopTracking` closes all; `SetTaskDone` and `DeleteTask` close the task's open entries; **runaway cap declined** (adjust is the remedy; revisit on evidence) | **RULED (2026-10-26 session)** |
| D-T7 | Manual entries | `LogTime` (closed, span-validated), `AdjustTimeEntry` (can close, can never reopen), `DeleteTimeEntry` (global, tombstoned) — **in v1** | **RULED (2026-10-26 session)** |
| D-T8 | Concurrency invariant | Single open entry **globally**; merge may transiently produce two (concurrent starts) — readers show the newest, the next write closes all; properties pin convergence; no device identity introduced | **RULED (2026-10-26 session)** |
| D-T9 | Persistence | Migration 0011 (0010 is DR-3's — collision note), `time_entries` table, open row = crash-safe intent, two new PersistenceActions | recorded |
| D-T10 | Sync lane | Entries roam via the appdata sidecar, **board-level `"time-entries"` section** keyed by entry UUID — history **outlives task deletion** (no prune arm); unbound-task entries stay device-local | **RULED (2026-10-26 session)** |
| D-T11 | Wire merge | Per-key clock LWW (RuleEntry semantics: tombstones compete, verbatim never ordered); remote stop = newer write, no special case | recorded |
| D-T12 | Deletion vs truncated views | Derived tombstones guarded (within-window ∧ card-resolvable); deletes are global; unresolvable foreign entries neither import nor delete | recorded |
| D-T13 | Document growth | Retention aging at flush (`lane_gc_scheduled` shape), constant 180d, overlay filtered by the same window; local history never drops; config knob fenced; stale-device re-emit tolerated one version | recorded |
| D-T14 | CLI | `track start/stop/status/list/log/adjust/delete` (timers two-lane recipe); `Track` → IpcOrLocal; tracked-today splits spanning entries at the day-window boundary (pure projection) | recorded |
| D-T15 | Export | New `export` verb — stdout, CSV default / JSON, inclusive start-date filters, raw payload (no receipt envelope); journal/statistics exports reuse it | recorded |
| D-T16 | UI v1 | Status chip (Stop button; kiosk read-only), card context-menu arm + tracking badge (cards keep no per-row buttons), detail Start/Stop, notification **Start Task** (one command; implicit dismissal planner-side); ActionIds `TaskTrackStart/Stop` only | **RULED (2026-10-26 session)** |
| D-T17 | Presentation purity | Elapsed phrase, chip/badge visibility, kiosk gate as pure fns in presentation.rs; minute granularity v1 | recorded |
| D-T18 | ADR | Chunk 1 writes ADR 0017 recording the model + fences | recorded |
| — | Round scope | **Core subsystem only**; tracker type, respite, attribution, break reminders, pomodoro, totals section, detail history → §7 fence issues with vocabulary reserved (`kind`-flags, `user` left additive) | **RULED (2026-10-26 session)** |

## 5. Test-strategy hooks (for the implementation chunks)

- **Domain properties (chunk 1/2)**: union convergence over arbitrary
  document pairs including open→closed transitions, tombstone-vs-live
  clock races, and verbatim rows (the rules/dismissals property shape);
  the single-global convergence property — after any `StopTracking`
  or `StartTracking` plan applied to any state, **zero** open entries
  remain (stop) / exactly one (start), and the union of two
  single-global states converges to ≤1 open at the next write;
  `tracked_in_window` splits spanning entries exactly once at each
  boundary (proptest over windows/entries); title-snapshot immutability
  under rename.
- **Planner (chunk 1)**: every verb on `FixedClock`-supplied `now` —
  receipts carry the stamped values (typed assertions, no text);
  `InvalidSpan` rejection shapes; `NoOp` repeats; Start's dismissal
  emission mirrors `SetTaskDone`'s test set; done/deleted-close side
  effects.
- **Engine (chunk 1)**: start → open row visible in the published
  snapshot; stop stamps and publishes; **crash-resume**: apply-to-
  storage-then-interpret means a fresh engine over the same repo sees
  the open entry and `track status` reports it running (deterministic
  — no sleeps, join/predicate waits only).
- **Storage (chunk 2)**: migration-upgrade test (empty table folds);
  codec roundtrips incl. `end: null`; actions in one transaction.
- **Sidecar (chunk 2)**: `time_entries_for` window filter + unbound
  skip; tombstone derivation guard (a truncated view never deletes —
  the dismissals fail-closed property extended); `lane_gc_old` strips
  and the overlay never re-emits; fold import/tombstone/skip paths.
- **CLI/app (chunk 3)**: `cli_track.rs` in the `cli_timers.rs` shape —
  hermetic tempdir boots, typed receipts, IPC relay leg, `--json`
  envelopes; export golden CSV/JSON against constructed fixtures
  (golden files are data, not labels — no text assertions on error
  paths); `--for`/`--from/--to` parse outcomes typed.
- **UI (chunk 4)**: pure-fn units (elapsed phrase buckets, chip/badge
  visibility, anchor-aware Start visibility, kiosk gate); action
  routing asserts the exact `StateCommand` dispatched; golden visuals
  extend per the existing harness; no TTY, no run loop in units.
- **Gates**: insta snapshots grow the tracking section (persisted
  dumps only — the chunk-10 narrowed shape); `cargo deny` untouched
  (no new deps — rodio stays DR-3's addition, nothing here); no log/UI
  text assertions anywhere (AGENTS §5).

## 6. Risks & mitigations

- **Document growth on a shared flush lane**: dozens of entries/day on
  one board's attachment; mitigated by D-T13 (180d window, both
  document-side and overlay-side) and measured cheaply (entry count ×
  ~140 bytes); if a board still outgrows it, the fenced config knob
  and per-board retention are the follow-up, not a schema change.
- **Transient double-open under concurrent starts** (D-T8): bounded by
  the next write closing all; readers pick the newest; the state is
  observable only between two sync cycles — recorded honestly in ADR
  0017 rather than papered over with device identity (which stays
  fenced to attribution).
- **Clock skew on remote stop**: the stopping device's `now` stamps
  the end; LWW orders by mutation clock, not span sanity — a skewed
  stop can produce a slightly wrong duration, never an invalid one
  (span validation still applies to adjusts; synced rows fold as-is,
  the same tolerance every clock-carrying section already has).
- **Stale-device re-emit of aged entries** (D-T13 mixed-fleet note):
  one-version tolerance, self-healing on the next round-device flush;
  identical in kind to every additive section's old-relay behavior.
- **History-editing foot-guns**: adjust can close but never reopen
  (D-T7) — the dangerous direction is unexpressible in the payload;
  deletes are global and tombstoned, so a fat-fingered `track delete`
  roams — the CLI takes an explicit id (no wildcard), and undo-v2
  (#176 family) is the eventual safety net, noted in its fence.
- **Chip on the kiosk implies tracking state is kiosk-visible**:
  intended (passive display of the running entry is appliance value);
  the gate keeps actions desktop-side; no tracking command is
  dispatchable from a kiosk surface.
- **Migration number collision** with DR-3 chunk 2 (both round plans
  written against 0009): implementing chunks re-verify the next free
  number at their own base; sequenced 0010 → 0011 by the dispatch
  round if both are in flight.

## 7. Chunk mapping → the issues this round opens

Implementation chunks (the follow-up dispatch round chunks these once
this plan is accepted — the §14 rule):

| Chunk | Scope | Size | Issue |
|---|---|---|---|
| 1 — domain & engine | D-T1–D-T8, D-T18 (entity, TrackingState, verbs, planner semantics incl. implicit dismissal + done/deleted-close, projections, properties, ADR 0017) | M | `backlog: DR-5 chunk 1 — time tracking domain & engine (open entries, single-global intent)` |
| 2 — storage & roaming | D-T9–D-T13 (migration, codec, actions; sidecar section, union, tombstone guard, retention aging, fold, flush trigger) | M/L | `backlog: DR-5 chunk 2 — time entry storage & sidecar roaming (board-level section, retention aging)` |
| 3 — CLI & export | D-T14–D-T15 (`track` family, receipts, IPC lane, `export` verb, tracked-today projection) | M | `backlog: DR-5 chunk 3 — track CLI family & export verb` |
| 4 — UI | D-T16–D-T17 (status chip, card context arm + badge, detail button, notification Start Task, kiosk gate, actions) | M | `backlog: DR-5 chunk 4 — tracking UI (status chip, affordances, Start Task button)` |

Sequencing: 1 → 2 → (3 ∥ 4). Chunk 3 needs 1 (verbs) and reads full
local history (2's table for durability, but a thin in-memory repo
carries the tests until 2 lands — the dispatch round decides whether 3
gates on 2 for its durability legs); chunk 4 needs 1 for end-to-end
tests and rides whatever storage exists at its base.

Fence issues (out-of-scope by the session's scope ruling; each carries
a permalink to this plan):

| Fence | Issue |
|---|---|
| Today-view "Tracked" totals section + day total | `backlog: today-view tracked-time totals section (rides DR-5 entries)` |
| Task-detail entry history list (adjust/delete affordances) | `backlog: task-detail time-entry history list (rides DR-5 entries)` |
| Tracker task type (never-"done" `TaskKind`/flag) | `backlog: tracker task type — never-done tasks over time entries` |
| Respite marking (break-activity flag consumed by stats) | `backlog: respite marking on time entries (break-activity kind)` |
| Per-user attribution for shared team decks (Wave 4) | `backlog: per-user attribution on time entries (team decks, Wave 4)` |
| Break reminders (tracked-duration trigger on the reminders pipeline) | `backlog: break reminders driven by tracked-work duration` |
| Pomodoro focus mode + concentration overlay | `backlog: pomodoro focus mode & concentration overlay` |

Vocabulary reserved for the fences (kept additive by this round's
shape): a `kind`/flags field and a `user` field fold onto `TimeEntry`
and its wire row as default-carrying additions — nothing in v1's
schema or wire shape precludes either (the entry map is keyed by id,
not by a closed field tuple).

## 8. References

- Mandate: issue #210 (the round's subject; its comment thread carries
  the DR-1 R3.5 Start Task cross-link) and the backlog-triage dispatch
  plan §14 (the design-round queue this round extends).
- Origin analysis:
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §3.3 (time tracking & focus), the Wave-2 row (§6), and backlog entry
  4 (§8).
- Substrate lineage: DR-1's sidecar design
  (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  §4, and its stage-2 doc's additive-section precedents) — this round's
  Part D is that pattern's third application; DR-3
  (`.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`)
  for the chunk-issue convention, the anchor-aware notification-row
  pattern, and migration-0010 (the collision note).
- In-tree seams: `adapter.rs:3300-3304` (the Start Task reservation),
  `appdata.rs` D-S* decisions (the sidecar contract this round
  extends), ADRs 0004/0006/0007/0013/0015 (sync policy, engine loop,
  sync cycle, reminders precedent, UI rules).
- Adjacent in flight: DR-4 `docs/sync-merge-policy-design` (owns
  task-field merge semantics; no overlap — this round's fold is purely
  additive-section), DR-3's four chunk issues (#474-#477).

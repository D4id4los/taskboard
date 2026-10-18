# [Plan] Daily-Driver Wave — Views & Finding Design (round doc 4 of 5)

- **Date**: 2026-10-19
- **Kind**: design doc for the daily-driver design round, question 4
  (framing plan §5.4). Designs the three "views & finding" features —
  the today view (#346), the inbox stack + quick capture (#347), and
  fuzzy search with `tasks find` + the jump-to palette (#348) — each
  point with re-verified anchors, exactly one recommendation, and a
  decision-table row (§5). This doc is the round's doc 4; the user's
  decision session rules §5 before the wave implementation plan is
  written.
- **Origin**: framing plan
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`
  (§5 Q4 normative; §2 scope rows for #346/#347/#348). Normative
  inputs: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §3.7 (Today view :170; Inbox capture :174; Fuzzy search :175; the
  kiosk rotation row :173), `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`
  (R1 component discipline :32–50; R4 keyboard journeys :88–104; R9
  responsive/kiosk degeneration :203–219; §3 feature-surface rows
  :241–255), and `.artifacts/reports/2026-10-14-adhd-support-research-report.md`
  — **principles as input, not a feature** (framing §5.4, restating
  D6; the principles this doc leans on are cited by id: P1, P2, P4,
  P7, P8, P9, P10, P13).
- **Round context**: docs 1–3 are on this branch, recommendations
  ACCEPTED (doc 1 sidecar @ `2b1a657`; doc 2 field family @ `a8edfdb`
  + rulings `f236c0b`; doc 3 reminders surfaces @ `d5567b3` +
  rulings `75c42c9`). This doc designs **against doc 2's ruled
  shapes** (`scheduled: Option<DateTime<Utc>>` native-lane, the
  five-level enums, `[preferences] day_start_time`, `days.rs::
  day_window`, the D-F5 phrase/decoration seams), consumes doc 1 §7's
  ruled anchor-task hiding as a binding input, and stays clear of
  doc 3's banner/toast surfaces (D-R4: banner unchanged).
- **Status**: recommendations **PROPOSED** — open for the user's
  decision session. Zero production code; this file is the chunk's
  entire diff.

---

## 1. Mission & binding constraints

Land the wave's three user-facing finding/planning surfaces so that
each is (a) driven by the settled data plane (doc 2's fields +
day-boundary contract), (b) a thin client over pure domain functions
(AGENTS §1/§2 — no business logic in the UI), (c) keyboard-complete
(R4), component-disciplined (R1), kiosk-degenerated (R9), and (d)
shaped by the ADHD principles as design input (D6) without turning
any principle into a feature of its own.

Binding, not re-arguable:

- **Doc 2's ruled interfaces**: `Task.scheduled:
  Option<DateTime<Utc>>` on the native `startdate` lane (D-F2 branch
  A, ruled); `Task.done: Option<DateTime<Utc>>` (entities.rs :176)
  as the done-today source; `[preferences] day_start_time` +
  `days.rs::day_window(now, NaiveTime, FixedOffset)` with the
  consumers contract — app bootstrap resolves the pair once, UI
  consumers receive `(day_start_time, offset)` explicitly, nothing
  reads figment mid-flight (D-F6 §3.6 :684–695); the D-F5 row-model
  phrase helpers and the `TaskCardModel.decorations` slot (models.rs
  :336) as the decoration carrier.
- **Doc 1 §7 (ruled 2026-10-18)**: hide the anchor task — a
  presentation-layer exact-title filter excluding it from *lists,
  counts, the today view, and search* — and render the per-board
  format-fence warning on the status surface. The companion CLI
  guard is doc 1's chunk, not this doc's.
- **Doc 3 D-R4 (ruled 2026-10-19)**: the overdue banner is
  unchanged; the today view's overdue section must not duplicate the
  banner's `snooze_target` machinery (presentation.rs :221–231).
- **Fields stay dumb** (D-F1): nothing in this doc derives priority,
  sorts by importance/urgency, or scores effort-fit. The today view
  orders by *time*, not by judgement.
- **Multi-board world** (accepted design Q8 §3.8): `ViewState.board:
  Option<BoardId>` is the board-selection home; board-scoped reads
  route through it; `select_board` resets stack/task focus. This doc
  states per surface whether it is board-scoped — and the today view
  deliberately is not, on desktop (D-V3).
- **R1/R4/R9** as binding GUI constraints: new surfaces are
  compositions over existing primitives (no view-local restyling);
  every verb is an action with a stable id; the scripted journey
  (tests/journeys.rs :181 — "the ten core verbs in one continuous
  session") grows a leg per new surface; each surface states its
  kiosk/compact degeneration.
- **No new dependencies** (AGENTS §4): the fuzzy matcher is
  hand-rolled in the domain crate; no fuzzy-matcher crate, no new
  config sections (P7 — constrain by default).
- **TDD** (AGENTS §5): the domain cores (`today_sections`, the
  matcher) land as proptest targets with the code; UI routing is
  pinned by router/view-state tests and the journey; no text
  assertions anywhere (typed outcomes and structural facts only).

## 2. Verified baseline (symbols are the stable keys; lines are this chunk's)

Worktree base: branch `docs/daily-driver-design-round` @ `75c42c9`
(= `main` `b809809` + docs 1–3; the code delta from the framing's
`07088ee` is the multi-board implementation round, in flight per
framing §7 — anchors below were re-read on this branch; where the
multi-board chunks rewrite a seam, the accepted design's symbol is
named alongside).

| Seam | Symbol (file) | Lines | Notes |
|---|---|---|---|
| Action registry | `ActionId`/`REGISTRY` (ui `actions.rs`) | 92–120 / 206+ | 26 variants; **`CaptureQuick` :99 (row :249) and `PaletteSearch` :100 (row :256) already exist, reserved-disabled** with `Category::Capture`/`Search` (:63/:64); `AppSettings`/`AppHelp` reserved too |
| Contexts | `ContextId` (ui `actions.rs`) | 26–50 | Global/Board/StackDetail/TaskDetail/Menu/ViNormal/ViInsert — the chain vocabulary |
| Router | `ActionRouter::dispatch` (ui `router.rs`) | 101–164 | pure, exhaustive; `DispatchOutcome::{Command, UiEffect, Disabled, NoTarget}` :66–78; reserved arms :156–162 |
| Dispatch ctx | `DispatchCtx` (ui `router.rs`) | 84–92 | `focused_task`/`focused_stack`/`reminder_target` — the target seam a window-carrying action extends |
| View cycle | `ViewId`/`ViewState` (ui `view_state.rs`) | 22–28 / 59–75 | Board → StackDetail → TaskDetail, subject validation in `enter` :249–284; `step_view` :232–245 skips subjectless views |
| Flows | `Flow` (ui `view_state.rs`) | 33–46 | exclusive: Create/CreateStack/EditTitle/Move; one `buffer` mirror :74; `commit_edit` :460–500 emits `StateCommand::CreateTask { title, stack, order }`; `discard_edit` :503 the only non-committing exit |
| Stack-create precedent | `Flow::CreateStack` + `ActionId::StackCreate` | view_state :449–453; actions :107/:311 | PR #334's structure-editing precedent — a new flow + action + registry row is the established ceremony |
| Model push | `push`/`IdTables` (ui `models.rs`) | 101–129 / 34–65 | one deterministic pass, indices-back; `TaskCardModel.decorations` :336 carries `[]` today (D21) — the D-F5 decoration slot |
| Presentation | `due_sets`/`due_kind`/`due_phrase` (ui `presentation.rs`) | 50–59 / 69–81 / 117–138 | set-membership over **domain** functions — the layering template; `banner_rows` :199; `live_board` :250 |
| Repaint driver | `start_repaint_driver` (ui `lib.rs`) + `REPAINT_CADENCE_MS` (ui `adapter.rs`) | 167–200 / 36 | repeating timer, 100 ms cadence, **wall-minute boundary detection** — the day-rollover mechanism exists |
| Overlay pattern | `ActionMenu` mount (ui `app-window.slint`) | 143 | `if (KeyboardState.menu-open): ActionMenu` — conditional-mount overlay + global state; the palette/capture sibling pattern |
| Journey pin | `tests/journeys.rs` | 181–185 | "the ten core verbs in one continuous session" — R4's acceptance artefact |
| Domain temporal fns | `overdue_tasks`/`due_soon`/`filter_sorted` (domain `reminders.rs`) | 420–424 / 431–442 / 453–457 | canonical `(order, id)` sort; the shape `today_sections` mirrors |
| Task eligibility | `is_live`/`is_open` (domain `entities.rs`) | 197 / 203 | the predicates every section/matcher reuses |
| Command vocab | `StateCommand` (domain `messages.rs`) | 20–101 | `CreateTask` :22–29, `UpdateTask` :31–36 (sparse `TaskChanges`) — the verbs capture/re-commit compose |
| CLI verbs | `TasksCmd` (app `cli.rs`) | 347–458 | List/Add/Done/Move/Edit/Delete/Label/Unlabel/Remind; `tasks find`'s siblings |
| Selector | `resolve_task` (app `commands/tasks.rs`) | 492–536 | ADR 0009 cascade: exact uuid → unique ≥8-prefix → exact title; typed ambiguity errors — the anti-model for fuzzy (search ranks, never errors) |
| REPL | `dispatch_line` (app `repl.rs`) | 610 | one handler path feeds CLI + REPL — `tasks find` lands in both for free |
| Output rows | `TaskRow` (app `commands/output.rs`) | 89–108 | the list-row projection `FindRow` mirrors |
| Kiosk switch | `AppMode` (app `config.rs`) | 85–95 | Daemon/Desktop/**Kiosk** — the mode D-R13's `auto` resolves on; the R9 degeneration hook |
| Board selection | Q8 §3.8 (multi-board design) | :970–1070 | `ViewState.board` + `select_board`; kiosk inherits compact degeneration |

## 3. The design, point by point

### Part A — The today view (#346)

#### 3.1 D-V1 — Membership: four sections as one pure domain function

**Anchors.** Framing §5.4 ("driven by the scheduled field +
day-boundary preference"); issue #346 ("tasks due/scheduled for
today"); doc 2 D-F6's `day_window` contract (:655–680) and its
consumers contract (:684–695); the domain temporal-function family
(`overdue_tasks` reminders.rs :420, `due_soon` :431 — collection-side,
canonically sorted, pure); `due_sets`' layering (presentation does
set-membership over domain outputs, presentation.rs :50–59 — domain
owns the concept, UI maps); P2 ("task cards in the today view show
time-of-day shape"), P4 ("the today view render[s] scheduled blocks
as a day shape"), P9 (overdue triage as recovery), P8 ("'done today'
counts on today/overview surfaces").

**Recommendation D-V1.** One pure function in doc 2's `days.rs`
(the module D-F6 creates for `day_window`), returning the whole
sectioned day so every consumer — GUI view, later CLI verb, later
PDF export — reads one definition:

```rust
// domain days.rs — illustration, not a diff.
/// The today view's four sections, first-match placed: a live task
/// appears in at most one section (Overdue > Scheduled > Due >
/// Done). Overdue/due use `duedate`; scheduled uses doc 2's
/// `scheduled`; done-today uses `done ∈ window`.
pub struct TodaySections {
    pub overdue: Vec<TaskId>,
    pub scheduled: Vec<TaskId>,   // `scheduled ∈ [start, end)`, by (scheduled, order, id)
    pub due_today: Vec<TaskId>,   // `duedate ∈ [start, now)` — see below
    pub done_today: Vec<TaskId>,  // `done ∈ [start, end)`, by (done, id)
}

#[must_use]
pub fn today_sections(
    tasks: &BTreeMap<TaskId, Task>,
    now: DateTime<Utc>,
    window: (DateTime<Utc>, DateTime<Utc>), // day_window's output
) -> TodaySections;
```

- **Section semantics** (all reuse `is_live`/`is_open` where open
  applies, entities.rs :197/:203):
  1. **Overdue** — `overdue_tasks(tasks, now)` (reused verbatim,
     reminders.rs :420): `duedate < now`. The missed-day recovery
     home (P9: "overdue triage is a reschedule flow"; the banner
     stays the *nagging* surface — D-R4).
  2. **Scheduled** — `scheduled ∈ [start, end)`, ordered by
     `(scheduled, order, id)`: the day shape (P4). A task scheduled
     earlier today but already past stays in the section — the day
     shape shows what was placed, done or not (P2).
  3. **Due today** — `duedate ∈ [now, end)`: the deadline lands
     inside the remaining day. (Lower bound `now`, not `start`: a
     duedate between `start` and `now` is already **overdue** and
     lives in section 1 — one row per task, no duplication.)
  4. **Done today** — `done ∈ [start, end)`, ordered by `(done, id)`
     ascending (the day's story in order; P8's visible accumulation
     — the section header carries the count).
- **First-match placement is part of the contract**: a task both
  scheduled and due today renders once, in Scheduled, with its due
  phrase on the row (the `due_phrase` decoration, D-F5); a completed
  scheduled task moves to Done. The function returns ids only —
  callers read entities from the map (the `overdue_tasks` shape).
- **Why domain, why one function**: the sectioning is temporal
  semantics, exactly what `overdue_tasks`/`due_soon` already are;
  dedup/first-match must not be re-derived per frontend (the
  three-frontend rule AGENTS §1). A `Vec<TaskId>` per section keeps
  it serde-free and snapshot-friendly.
- **Anchor task**: not filtered here — doc 1 §7 rules the filter
  presentation-layer (see §7, reconciliation item 5). In practice
  the anchor carries no scheduled/duedate, so it never lands in a
  section on its own; the surfaces still apply the exact-title
  filter to counts (belt-and-braces, per the ruling's wording).
- **Proptest targets** (land with the function): partition (every
  live task appears in ≤1 section); window bounds (`start ≤ t < end`
  membership flips when `now` steps past `start` — D-F6's own
  proptest pattern :693–695); first-match (a task with both
  `scheduled` and `duedate` in-window is in `scheduled` only);
  `done`-placement (completing a scheduled task moves it to
  `done_today`, out of `scheduled`).

**Constraints conformance**: pure domain data + chrono; no vocabulary
change; consumers get the window explicitly (D-F6's contract). No
importance/urgency/effort input anywhere (fields stay dumb).

#### 3.2 D-V2 — Placement: a first-class view, the desktop default

**Anchors.** `ViewId` cycle + subject validation (view_state.rs
:22–28, :232–284); `ContextId` per view (:186–193); the registry's
context-tagged rows (actions.rs :184–198 — menu-availability
contexts); R4's acceptance bar ("a scripted keyboard journey
performs every core verb: navigate boards/stacks/tasks, edit,
complete, move, **search**, invoke menus" — GUI plan :99–101);
P10 ("today view as the default desktop surface"); future-directions
§3.7 ("The single highest-value desktop view"); `EmptyState`
primitive (ui/widgets/empty-state.slint) for the nothing-today case.

**Recommendation D-V2.** `ViewId::Today` joins the one view cycle as
a **subjectless, always-enterable** member, and is the **default
start view on Desktop**:

- **Cycle**: `Today → Board → StackDetail → TaskDetail → Today`
  (`step_view`'s order array grows one entry; `enter` for Today
  returns `true` unconditionally — no subject to validate, the
  empty-board precedent inverted). `nav.focus-board` remains the
  one-keystroke escape to the kanban centrepiece.
- **Default**: `ViewState::new()` starts at `Today`. Evidence: P10's
  build direction is explicit ("today view as the default desktop
  surface"); the wave is named *daily-driver*; §3.7 calls it the
  highest-value desktop view. The daemon has no UI and the kiosk's
  surface is the kiosk wave's to own (D-V3), so the default change
  is desktop-only. The session may amend to keep Board-default for
  continuity — the cycle membership is the load-bearing half.
- **Direct jump**: one new global action `ViewToday` (`view.today`,
  label "Go to today", category Navigation, contexts Global,
  enabled) — P10's "reachable in one action from anywhere".
  Dispatch: `UiEffect::FocusToday` (a sibling of `FocusBoard`),
  resolved by the adapter to `view = Today` + `RefreshNow`.
- **Context**: `ContextId::Today` ("today") joins the enum; the
  task-verb registry rows (`task.complete`/`task.edit`/`task.move`/
  `task.delete`, actions.rs :313–339) add `Today` to their contexts
  so their menus offer the verbs there; `ViewState::context()` maps
  the new view.
- **Rows are the existing primitive**: each section renders
  `task-card` instances (R1; the GUI plan's own "task-list
  decorations belong to the task-card primitive" :246) with the
  D-F5 decorations slot (models.rs :336) carrying the scheduled
  time-of-day / due phrases; a new composition
  `today-view.slint` (L2, sibling of board-view.slint) lays out
  section headers + card lists + the done-count caption; empty
  sections collapse; an all-empty day renders `EmptyState` with a
  capture hint (P1's loop-closing nudge — copy is implementation
  polish, the structural fact is the primitive reuse).
- **R4 journey leg**: with a scheduled and an overdue task seeded,
  the journey performs `view.today`, walks rows with
  `nav.next-item`, completes one (`task.complete`), re-commits the
  overdue one (D-V4), and escapes with `nav.focus-board` — added to
  tests/journeys.rs as a continuous session like the existing ten
  (:181–185).

**Constraints conformance**: one `ViewId` variant + one action + one
context — the established registry/flow ceremony (the stack-create
precedent, PR #334); no new state shapes; `view.today` performs no
engine command (dumb-client).

#### 3.3 D-V3 — Scope: cross-board on desktop, board-pinned on kiosk (the R9 statement)

**Anchors.** Multi-board Q8 (§3.8 :970–1070): `ViewState.board` is
the selection home, `select_board` resets focus, "every board-scoped
read that today flows through view_state.rs:198 flows through
`self.board` instead", kiosk inherits compact degeneration; R9
(:203–219 — breakpoint/container behaviour, "which views collapse to
what"); `AppMode::Kiosk` (config.rs :93); framing §7.5 ("The
today-view design states its kiosk degeneration either way");
future-directions §3.7's kiosk rotation row (:173 — the later
display-mode machine); P10 (one system, always visible).

**Recommendation D-V3.** The today view is **cross-board on
desktop**: `today_sections` runs over the whole tasks map (it
already does — it takes the map, not a board), rows carry a stack
title always and a board title only when more than one live board
exists (a presentation helper decides badge visibility —
deterministic state-derived, not config, P7). Kiosk degeneration,
stated per R9:

- **Kiosk**: the same view code renders **only the focused board's
  rows** (`ViewState.board` filter applied presentation-side — the
  one deliberate departure from "the kiosk shows what desktop
  shows", justified below), compact layout (single column, no board
  badges, no input chrome — the Q8 compact path already drops
  chrome), done-count and overdue section still render (glanceable
  status). The view is **rotation-ready**: the kiosk wave's
  display-mode state machine (§3.7 :173) may add Today as a target
  without touching this design. No kiosk-specific code lands in
  this wave beyond the filter rule.
- **Why cross-board on desktop**: a day is not board-scoped — the
  user's 9:00 dentist task lives in Personal and the 14:00 review
  in Work, and the whole value of the view (P10 "one system") is
  seeing the one day. The domain function is board-agnostic by
  construction; board-scoping would be an extra UI filter bought at
  every refresh. **Selection-follows-focus across boards**: walking
  or activating a today row retargets `ViewState.board` to the
  row's board (Q8's `select_board` mechanics) — so `task.move`'s
  target cycle (view_state.rs :531–547, which walks the focused
  board's stacks) and detail navigation stay board-local and need
  no cross-board move semantics (doc 1 §7 keeps cross-board repair
  out of wave as #350).
- **`IdTables`**: grows `today_tasks: Vec<TaskId>` — the four
  sections concatenated in display order; the focus walk's domain
  on the Today view is that vector (the `reading_order` precedent,
  view_state.rs :207–213). No per-board re-keying (Q8's ruling).

**Constraints conformance**: one deliberate, flagged departure from
Q8's board-scoped-read routing (recorded in §7 item 6); kiosk rule
is a presentation filter, no new mode machinery.

#### 3.4 D-V4 — The now mechanism and the overdue re-commit verb

**Anchors.** The repaint driver (lib.rs :167–200 — repeating timer,
`REPAINT_CADENCE_MS` = 100 ms, **wall-minute boundary detection**
"the reminders §7 time repaint"); `models::push` takes `now` as a
parameter and is called by that timer (models.rs :101–108);
`DispatchCtx` (router.rs :84–92) as the target-carrying seam;
`StateCommand::UpdateTask` + `TaskChanges.duedate`'s double option
(messages.rs :31–36, :217–222 — a `Some(Some(_))` write stamps the
clock, D-F1's clearing-is-a-writing rule); P9 ("reschedule flows
without guilt copy — 'move to today?', never 'you failed'"),
P5 (honest urgency: real deadlines made present, no fake scarcity),
P2 (externalise time).

**Recommendation D-V4.**

- **Day rollover rides the existing timer**: the today view holds
  no clock of its own. The repaint driver already detects the
  wall-minute boundary and refreshes; `push` recomputes
  `day_window(now, …)` and `today_sections(…)` with the fresh
  `now`, so the view rolls over within one cadence (≤100 ms) of
  the boundary crossing. No new timer, no engine event — the
  reminders §7 mechanism reused verbatim. (The window itself is
  computed in the adapter from the `(day_start_time, offset)` pair
  it holds per D-F6's consumers contract; `push` receives the
  window like it receives `now` — time is data.)
- **One new action for the missed day**: `TaskDueToday`
  (`task.due-today`, label "Due today (re-commit)", category Task,
  contexts `[Today]`, enabled) — the P9 mechanism made concrete.
  Dispatch needs the window end, so `DispatchCtx` grows
  `day_window: Option<(DateTime<Utc>, DateTime<Utc>)>` (the
  adapter sets it per dispatch, exactly as it sets
  `reminder_target` today); the router maps
  `TaskDueToday` → `Command(UpdateTask { changes: TaskChanges {
  duedate: Some(Some(window_end)), ..default } })` on the focused
  task, `NoTarget` without focus or window. Semantics: re-commit
  the missed deadline to the end of the current day — an honest
  new deadline (P5), a write that stamps the `duedate` clock
  (D-F1), and confirmation-free like every task verb (undo is
  reserved #349; recovery is `task.edit`). The overdue banner
  keeps snooze/mute (D-R4 — the banner is reminder-side; this is
  planning-side; no `snooze_target` logic is duplicated).
- **Done-today count**: the Done section header renders its count
  (a number next to the caption — P8's "accumulation is real work,
  made visible"); no streaks, no gamification (X3/X4 rejected
  principles).

**Constraints conformance**: one action, one ctx field, zero
timers; the command reuses the sparse-edit verb untouched.

### Part B — Inbox stack + quick capture (#347)

#### 3.5 D-V5 — The designated-stack convention and the `CaptureTask` command

**Anchors.** Future-directions §3.7 ("a designated stack + a capture
verb, core-side cost 'tiny'"); P1 ("writing to the §3.7 inbox stack
(designated stack + capture verb)… the capture surface accepts one
line, no required field, triage happens later (P13)"); the anchor
card's title-convention precedent (doc 1 — "creation, title
convention", §header :26); `StateCommand::CreateTask`/`CreateStack`
(messages.rs :22–29/:58–64); Q8's board-resolving commands (DF-1:
board-scoped creation resolves its board post-multi-board); the
engine as sole mutator (AGENTS §2 — the state engine processes
commands).

**Recommendation D-V5.** The convention is a **per-board stack
titled exactly `Inbox`**, and the verb is a **dedicated engine
command**, not frontend glue:

```rust
// domain messages.rs — illustration.
/// Quick-capture: append `title` to `board`'s Inbox stack,
/// creating the stack (appended, canonical order) when the board
/// has none. The one ceremony site every capture surface
/// dispatches (#347).
CaptureTask { title: String, board: BoardId },
```

- **Resolution rule (pure, in the engine's handler)**: the board's
  live stacks titled exactly `Inbox`, canonical `(order, id)` order,
  first wins; none → create (`CreateStack` semantics: append order)
  then append the task (`CreateTask` semantics: append order). The
  engine is single-actor sequential, so the two-step is ordered —
  no race window. Deterministic-when-multiple (canonical first) is
  the right failure shape for capture: **capture never errors on
  the happy path** (P1 — seconds, zero decisions), unlike
  `resolve_task`'s typed ambiguity errors (commands/tasks.rs
  :492–536), which exist to protect *destructive* targeting.
- **Why a command, not caller-side create-then-add**: the
  convention must hold identically for the GUI overlay, the CLI
  verb, the REPL, and the future IPC/Thunderbird capture surfaces
  (future-directions §3.5); each would otherwise reimplement
  resolve-or-create. One command = one ceremony site = the #244
  lesson applied to a verb.
- **Title validation stays at the edges** (the standing split:
  view_state.rs :463–465 "the CLI owns title validation, the UI
  dispatches only well-formed commands" — the CLI handler rejects
  an empty title with a typed `CommandError`; the UI's
  `commit_edit` already refuses empty buffers).
- **Sync**: the Inbox is a native stack and captured tasks are
  native cards — capture rides the proven `duedate`-carrying lane
  (D-F2's point 2) and is **independent of sidecar health**;
  nothing about the inbox appears in the app-data document.
- **Engine tests** (snapshot, per the insta discipline): fresh
  board → Inbox created + task appended; existing Inbox → appended
  only; two Inbox-titled stacks → canonical-first, no error;
  captured task is open, unordered fields unset.

**Constraints conformance**: one `StateCommand` variant (board-
explicit, DF-1-shaped); no schema, no config, no document keys.

#### 3.6 D-V6 — The capture surfaces (and the answer to doc 2's at-create question)

**Anchors.** `CaptureQuick` reserved in the registry (actions.rs
:99/:249–255 — "capture.quick", GLOBAL contexts, waiting since
chunk 2); the GUI brief §3 ("a quick-capture surface (global hotkey
→ small input, no navigation)… the capture hotkey is an action (R5)
available in every context" :251–253); the overlay pattern
(app-window.slint :143); `Flow` exclusivity + the buffer mirror
(view_state.rs :33–75); `begin_create`'s insert-mode opening
(:441–445) and `commit_edit`'s empty-refusal (:466–498); P1
("capturable in seconds, from any context, with a title alone —
structure is optional and deferred"), P13 ("triage happens later");
doc 2 §7's deferred question ("`tasks add` flag parity … revisit if
quick-capture UX (#347) wants at-create scheduling" :799–801);
REPL/CLI shared path (repl.rs :610).

**Recommendation D-V6.**

- **GUI**: flip `CaptureQuick` to enabled; dispatch → a new
  `UiEffect::BeginCapture`; the interpretation table opens a new
  flow `Flow::Capture` (exclusive with all flows — beginning
  capture ends any in-progress flow, the existing XOR rule
  view_state.rs :1170–1183) with `SetMode(Insert)` + cleared
  buffer, exactly `begin_create`'s opening. A new composition
  `capture-overlay.slint` (L2, sibling of `action-menu.slint`,
  conditional-mount like app-window.slint :143) renders one text
  field + a hint line — nothing else (no navigation, GUI brief
  :252). Commit semantics, the one deliberate difference from the
  create row: **Enter commits and keeps the overlay open with a
  cleared buffer; Esc discards-and-closes; Enter on an empty buffer
  closes** — the rapid-fire braindump shape (P1; Marvin's
  Braindump, ADHD report §3.1). Commit emits
  `StateCommand::CaptureTask { title, board: ViewState.board }`
  (focused board; `NoTarget`-shaped inertness on a boardless
  session — capture needs a board to land in).
- **CLI/REPL**: one new top-level verb `taskboard capture <TITLE>
  [--board <BOARD>]` (default: the bound board, the `--stack`
  omission precedent of `tasks add`, cli.rs :354–362). The REPL
  dispatches the same line through `dispatch_line` (repl.rs :610)
  — P1's "cheapest first landing" is the REPL one-liner.
- **The at-create-scheduling ruling (doc 2 §7's question, answered:
  no)**: capture takes a title and nothing else — no `--scheduled`,
  no flags, no overlay fields. The Inbox's whole point is deferred
  structure (P1: "structure is optional and deferred"; P3: triage
  applies structure; P13: triage *is* the review step). Field entry
  happens at triage through the D-F5 edit flags. The separate
  `tasks add` flag-parity backlog entry is unaffected.
- **Kiosk (R9)**: no capture surface — the kiosk is passive and
  keyboardless; the action stays enabled (an operator keyboard
  works, Q8's stance) but no chrome advertises it.

**Constraints conformance**: one flow variant + one effect + one
composition + one CLI verb; the command from D-V5; zero new state
machines (the flow XOR rule already governs).

#### 3.7 D-V7 — Triage UX

**Anchors.** P13 ("their core interaction is triage-and-forgive"…
the end-of-day review "walks: inbox triage (P1) → misses
rescheduled without ceremony (P9) → tomorrow's boxes placed (P4)");
P3 ("splitting a task into steps must be a cheap, first-class verb"
— subtasks are W3, noted not built); the move flow's target cycle
(view_state.rs :531–547 — one keypress per target stack);
`StackDetail` as the single-stack focus view (view_state.rs :255–267,
models.rs :201–228); the toolbar composition (ui/compositions/
toolbar.slint); P10 (the system stays visible).

**Recommendation D-V7.** Triage is **not a new view family** — it is
`StackDetail` opened on the Inbox stack, plus one jump action and
one visibility chip:

- **`ViewInbox`** (`view.inbox`, label "Go to inbox", category
  Navigation, contexts Global, enabled): enters `StackDetail` with
  subject = the focused board's Inbox stack (the D-V5 resolution
  rule, reused read-side); inert when the board has none (an empty
  inbox is triage-done — the action quietly no-ops, P9's
  nothing-wrong-here shape; creating the stack on navigation would
  violate read-write separation).
- **The triage loop is existing verbs**: `nav.next-item` walks
  inbox rows; `task.move` cycles target stacks (the built-in ring,
  view_state.rs :531–547 — the GTD "file it" motion at one
  keypress per candidate); `task.complete` for two-second items;
  `task.edit` opens the detail where the D-F5 flags apply structure
  (`--scheduled`-class edits through the action layer). The
  end-of-day review (W2) will drive this same loop with prompts —
  nothing here pre-empts it.
- **Inbox count chip**: the toolbar gains a badge with the count of
  live, open tasks in the focused board's Inbox (a presentation
  helper `inbox_count(state, board)` — set-membership over the D-V5
  rule, the `due_sets` template). Desktop only; hidden in compact
  (the Q8 chrome-dropping path) and not rendered on kiosk. This is
  P10's loop-closure (capturing is confident when the inbox is
  glanceably non-empty) at the cost of one pure helper + one badge
  — severable polish if the session trims.
- **R4 journey leg**: with a captured task seeded, the journey
  performs `view.inbox`, walks to the row, `task.move` + target
  cycle + confirm, and the inbox count chip reads zero after the
  move (a structural assertion on the model, not text).

**Constraints conformance**: no new views (R1 — StackDetail
reused); one action; the count is a presentation read.

### Part C — Fuzzy search (#348)

#### 3.8 D-V8 — The ranking core: `find.rs` in the domain, proptest-first

**Anchors.** Issue #348 ("a pure ranking/matching function over
`AppState` (proptest target)"); future-directions §3.7 :175 ("Pure
domain ranking function (proptest target) + UI palette… No
prerequisites — can land any time"); AGENTS §5 (proptest for pure
domain logic; no quality ceilings — floors on garbage input only);
the domain's purity rule (clock.rs :11 discipline — no environment;
`find` takes everything as data); P7 (constrain by default — one
matching algorithm, no tuning knobs); the crate-boundary rule
(domain: zero IO/UI/runtime, AGENTS §2).

**Recommendation D-V8.** A new `taskboard-domain` module `find.rs`
— hand-rolled, dependency-free (no fuzzy-matcher crate; AGENTS §4
stack discipline), pure, total, deterministic:

```rust
// domain find.rs — illustration.
/// What a hit points at (the palette/CLI resolve it against state).
pub enum SearchTarget { Task(TaskId), Stack(StackId), Board(BoardId) }

/// Match quality, declaration order = rank order (best first).
pub enum MatchQuality { Exact, Prefix, WordStart, Substring, Subsequence }

pub struct SearchHit { pub target: SearchTarget, pub quality: MatchQuality }

/// Case-folded fuzzy search over live boards, stacks, and tasks
/// (tasks include done ones — finding yesterday's item is real;
/// tombstones and archived entities are never hits). Title-only in
/// v1. Empty/whitespace query → no hits. Deterministic total order:
/// (quality, title length, canonical id) — the sort key, not a
/// magic score number.
#[must_use]
pub fn find(state: &AppState, query: &str) -> Vec<SearchHit>;
```

- **Matching**: case-folded (`to_lowercase`, std-only; the
  deterministic-simple choice), subsequence-based with
  classification — an exact (folded) title equality is `Exact`; a
  title starting with the query is `Prefix`; a match starting at a
  word boundary (after whitespace/punctuation) is `WordStart`; a
  contiguous substring is `Substring`; a spread subsequence is
  `Subsequence`. The class set is ordered (declaration-order `Ord`,
  the D-F1 enum-ordering precedent) and the full ranking key is
  the tuple `(quality, title_len, canonical_id)` — **no numeric
  score exists to tune or flake** (P7; proptest-friendly totality).
- **Scope**: boards/stacks live-and-unarchived; tasks `is_live()`
  (done included — the hit carries the id; surfaces render doneness
  with the existing `DueKind::Done` machinery, presentation.rs
  :70). Title-only (description search is backlog — it would swamp
  ranking before there is a reason to care). **The anchor task is
  not excluded here**: doc 1 §7 rules the hiding
  presentation-layer (§7 item 5 — the surfaces filter, the domain
  stays convention-blind).
- **Type limits are caller-side**: the function interleaves kinds
  in pure rank order; `--type` (CLI) and palette filters select by
  `SearchTarget` kind — one ranking, many lenses (the `due_sets`
  layering instinct applied to search).
- **Proptest suite (the issue's named target)**, properties pinned
  at the floor (no ceilings, AGENTS §5): determinism (equal inputs
  → equal outputs, input-map insertion order irrelevant — BTreeMap
  canonicality); case-fold invariance (query case never changes
  the hit set); the empty-query floor (no hits); exact-title
  invariant (a title equal to the folded query is a hit of quality
  `Exact` and ranks first); class monotonicity (every substring
  match is also a subsequence — classes are refinements, so the
  classifier returns the best class); growth monotonicity (adding
  an entity never removes another entity's hit); type-filter
  partition (filtered output = unfiltered output restricted);
  totality (no two distinct entities share the full key — the id
  tiebreak guarantees it; the proptest pins the sort is strict).

**Constraints conformance**: pure data in, `Vec` out; no clocks, no
env, no IO; the module is liftable unchanged into any consumer (the
presentation.rs extraction-seam doctrine applied at birth).

#### 3.9 D-V9 — `tasks find` in the CLI/REPL

**Anchors.** Issue #348 ("surfaced as `tasks find` in the CLI/REPL");
`TasksCmd`'s shape (cli.rs :347–458 — read verbs like `List` are
pure reads); `TaskRow` (output.rs :89–108) as the row-projection
precedent; `resolve_task`'s cascade (commands/tasks.rs :492) as the
**anti-model**: resolution *errors* on ambiguity, search *ranks* —
they compose (find surfaces candidates; resolve consumes a chosen
one); the REPL shared dispatch (repl.rs :610).

**Recommendation D-V9.** `TasksCmd::Find`:

```
taskboard tasks find <QUERY> [--type task|stack|board] [--limit N]
```

- Pure read, exit 0 always on a well-formed query (no results is an
  empty table, not an error — a query, not a failure; the `List`
  precedent). `--type` names the `SearchTarget` kinds; `--limit`
  truncates after ranking (scripting convenience; the default
  prints all). The REPL dispatches the identical line.
- Output: a `FindRow` projection (kind, id, title, context —
  `stack › board` for tasks, `board` for stacks) rendered by the
  existing table style; the row fields are the contract, layout is
  polish (the D-F5 `TaskRow` stance). The ids make the verb a
  scripting bridge: `tasks find "repo" --type task` feeds
  `tasks done <id>` pipes.
- Handler: `find(state, query)` + optional kind filter + row
  assembly — no engine round-trip (reads over the `ArcSwap`
  snapshot like every read verb).

**Constraints conformance**: app-layer growth only; one clap
variant; no engine vocabulary.

#### 3.10 D-V10 — The jump-to palette in the GUI

**Anchors.** `PaletteSearch` reserved (actions.rs :100/:256–262 —
"palette.search", GLOBAL, waiting); the GUI brief §3 ("an
overlay-style search palette driven by actions; results jump to
items" :254–255); R4's acceptance bar naming **search** a core verb
(:99–101); the ActionMenu overlay pattern (app-window.slint :143 —
conditional mount + `KeyboardState` global + the menu-context chain
flip); the menu cursor actions (actions.rs :357–384); Q8's
`select_board`/focus mechanics for jump targets; P10 ("the search
palette and command layer keep the whole system one action away"),
P6 (novelty through content — the palette makes *content*
navigation the dopamine path, not app-hopping).

**Recommendation D-V10.**

- **Open**: flip `PaletteSearch` to enabled; dispatch → a new
  `UiEffect::OpenPalette`; the interpretation table opens
  `Flow::Palette { selected: usize }` with `SetMode(Insert)` +
  cleared buffer (the `begin_create` opening). While the flow
  holds, the keymap chain's head is a new **`ContextId::Palette`**
  (the menu-open chain-flip precedent — the same mechanism, one
  more context).
- **The overlay**: a new composition `search-palette.slint` (L2,
  ActionMenu's sibling, conditional-mount) — a text field, a result
  list of `ListRow`s (title + type badge + context line), and a
  selection highlight. **The palette calls the domain `find`** on
  the current snapshot for every buffer change (the adapter's
  keystroke mirror already lands per keystroke; board-scale data
  needs no debounce in v1 — no speculative machinery, P7) and maps
  hits to rows presentation-side (the dumb-client rule: the UI
  never reimplements ranking). Results cap at 8 rows + overflow
  count (the `BANNER_ROW_CAP` precedent, models.rs :30);
  `IdTables` gains `palette: Vec<SearchTarget>` — indices back, ids
  never cross the seam (the D2 discipline).
- **Cursor**: the `menu.*` cursor actions gain `Palette` in their
  registry contexts (modal-list cursors are generic; relabeling
  "menu entry" → "list entry" is polish, not new vocabulary — P7).
  Presets bind arrows/C-n/C-p in the palette context (the preset
  table is the tunable; the design fixes existence and resolvability,
  the stack-create preset precedent :80/:165).
- **Jump-to (Activate)**: task hit → `select_board(task's board)` +
  `focused_task` + view `Board` (the card's `emphasis` renders,
  models.rs :333); stack hit → `select_board` + view `StackDetail`
  subject the stack; board hit → `select_board` + view `Board`.
  The flow always ends; mode returns to Normal. Esc/`MenuDismiss`
  closes without effect (the `discard_edit` shape). The anchor-task
  exact-title filter applies to the result rows presentation-side
  (doc 1 §7's ruling names search explicitly).
- **Kiosk (R9)**: no palette surface — passive display, no
  keyboard; the action stays enabled for operators. Nothing
  kiosk-specific lands.
- **R4 journey leg**: the journey performs `palette.search`, types
  a seeded needle, `menu.next`, `menu.invoke`, and asserts the
  focused task/board switched to the hit (structural assertions on
  view state, the existing journey idiom :181–185).

**Constraints conformance**: one flow variant, one context, one
composition, registry-context widening on four rows; ranking stays
in the domain; no new channels or state homes.

## 4. Coherence map

- **One day definition**: every "today" question routes through
  `day_window` (doc 2 D-F6) — the sections (D-V1), the re-commit
  deadline (D-V4), and later the PDF export and plan-next-day all
  ask the same window; nothing stores a derived "is_today" bit, so
  nothing derived can desync (doc 2 §4's closing rule, inherited).
- **One text mechanism**: capture and palette ride the existing
  `Flow` + `buffer` mirror + insert-mode discipline (D-V6/D-V10) —
  the create row, the title editor, capture, and the palette query
  are four flows of one mechanism, not four input systems.
- **One ranking**: `find` is the only matcher (D-V8); the palette
  (D-V10) and `tasks find` (D-V9) are projections of it; `resolve_task`
  stays the exact-inverse (errors, never ranks) for destructive
  targeting — search proposes, resolution disposes.
- **One inbox ceremony**: `CaptureTask` (D-V5) is the single
  resolve-or-create site; the GUI overlay, CLI verb, and
  `view.inbox`'s read-side resolution all reuse the one rule.
- **Banner vs today**: the overdue banner is reminder-side
  (snooze/mute, D-R4 unchanged); the today view's overdue section
  is planning-side (complete / re-commit / detail). Two surfaces,
  one truth (`overdue_tasks`), zero shared policy.
- **Board scoping, stated once**: board-scoped surfaces (board
  view, stack detail, inbox, move targets) route through
  `ViewState.board` (Q8); the today view is cross-board on desktop
  with selection-follows-focus retargeting (D-V3); the palette and
  capture are board-*landing* (jump selects the hit's board;
  capture lands in the focused board).
- **Kiosk degeneration, consolidated (R9)**: kiosk renders no
  capture overlay, no palette, no inbox chip (passive, keyboardless
  — Q8's stance keeps verbs operator-available); the today view
  degenerates to the focused board's rows in compact layout and is
  rotation-ready for the kiosk wave's display-mode machine. All
  three rules are presentation filters; no kiosk code paths.

## 5. Decision record — one recommendation each (PROPOSED)

| ID | Point | Recommendation (exactly one) | Status |
|---|---|---|---|
| D-V1 | Today membership | `days.rs::today_sections(tasks, now, window)` — four first-match sections (Overdue / Scheduled / Due-today / Done-today), ids-only, one definition for all consumers; proptests: partition, bounds, first-match, done-move | PROPOSED |
| D-V2 | Today placement | `ViewId::Today` in the cycle `Today → Board → StackDetail → TaskDetail`; **default Desktop start view**; `view.today` global action; `ContextId::Today`; rows are `task-card` + decorations slot; journey leg added | PROPOSED |
| D-V3 | Today scope + kiosk | Cross-board on desktop (stack title always, board badge when >1 live board; selection-follows-focus retargets `ViewState.board`); kiosk = focused-board subset, compact, passive, rotation-ready | PROPOSED |
| D-V4 | Now + missed day | Rollover rides the existing minute-boundary repaint (no new timer); `task.due-today` re-commit action (`EditTask`-shaped `UpdateTask { duedate: Some(Some(window_end)) }`) via `DispatchCtx.day_window`; done-today count caption | PROPOSED |
| D-V5 | Inbox convention + verb | Per-board stack titled exactly `Inbox`, canonical-first resolution, created on demand; **`StateCommand::CaptureTask { title, board }`** as the one engine-side ceremony; edges validate titles; native-lane sync | PROPOSED |
| D-V6 | Capture surfaces | Flip `capture.quick`; `Flow::Capture` + `capture-overlay.slint` (Enter commits-and-continues, Esc closes, empty Enter closes; lands in `ViewState.board`'s inbox); CLI/REPL `capture <TITLE> [--board]`; **at-create scheduling ruled out** (answers doc 2 §7); no kiosk surface | PROPOSED |
| D-V7 | Triage UX | No new view: `StackDetail(Inbox)` + `view.inbox` jump action + toolbar inbox-count chip (desktop); the triage loop is existing verbs (move ring / complete / edit flags); journey leg | PROPOSED |
| D-V8 | Ranking core | Domain `find.rs`: case-folded subsequence matcher with ordered `MatchQuality` classes, key `(quality, title_len, id)` — no numeric score; title-only, live+done tasks, no anchor filtering (presentation-side per doc 1 §7); the proptest suite is the issue's named target | PROPOSED |
| D-V9 | `tasks find` | `TasksCmd::Find { query, --type, --limit }` — pure read, exit 0, `FindRow` projection (kind/id/title/context); REPL shares the line | PROPOSED |
| D-V10 | Jump-to palette | Flip `palette.search`; `Flow::Palette` + `ContextId::Palette` (chain flip, menu-cursor actions widened); `search-palette.slint` composition calling domain `find` per keystroke, 8-row cap, `IdTables.palette` indices-back; jump-to retargets board/focus per hit kind; no kiosk surface; journey leg | PROPOSED |

## 6. Consequences for the wave implementation plan

Against framing §6's chunk sketch (advisory sizes there, mapped
here):

- **Chunk 9 (fuzzy search — M, confirmed)**: D-V8 (find.rs +
  proptests), D-V9 (CLI/REPL), D-V10 (palette). Lands fully
  independent of the field family (issue #348's "no prerequisites"
  holds after design: nothing in Part C reads `scheduled` or the
  window). Registry/keymap snapshot corpora regenerate (the
  deliberate insta pass, DF-9's rule).
- **Chunk 10 (today view + inbox capture — M–L, confirmed)**:
  D-V1–D-V7, **gated on chunks 1–2** (the fields and
  `day_window` — framing's dependency spine "2 + 1 → 10" holds).
  Contains: `today_sections` + proptests, the view/context/action
  growth, `CaptureTask` engine snapshots, the two overlays'
  compositions, the CLI verb, the journey legs, golden-visual
  regeneration for the new view.
- **Test-strategy hooks** (for the implementation chunks, per
  AGENTS §5): domain proptests as designed; engine snapshots for
  `CaptureTask` only (re-commit routes an existing verb — router
  test, not engine test); view_state/router unit tests for the new
  flows/actions/context; the three journey legs; registry snapshot
  regen counted as the one deliberate pass per chunk. No fixed
  sleeps anywhere (the rollover is proven by calling `push` with a
  stepped `now` — time is data); no text assertions (chip counts,
  hit orders, and focus targets asserted structurally).

## 7. Assumptions borrowed from the round's docs (reconciliation list)

Each item is a shape this doc consumed from a sibling doc's ruled
design; if the implementation chunk lands it differently, reconcile
there first:

1. **`scheduled` + its phrase helpers (doc 2 D-F1/D-F2/D-F5)**:
   today rows and the day shape read `Task.scheduled` and the
   D-F5 `scheduled_phrase`-class helpers by role; exact helper
   names are doc 2's chunk-5 output. Consumers are lane-agnostic
   (they read the field, never the sync lane), so D-F2's branch
   ruling does not propagate here.
2. **The window contract (doc 2 D-F6)**: `day_window(now,
   NaiveTime, FixedOffset)`'s signature, fixed-offset DST ruling,
   and consumers contract are consumed verbatim — the adapter holds
   `(day_start_time, offset)`, computes the window, passes it into
   `push` and `DispatchCtx` like `now`. If D-F6's chunk changes the
   tuple's type, D-V1/D-V4 follow mechanically.
3. **The decorations slot (models.rs :336, D21 + doc 2 D-F5)**:
   today rows assume field decorations land in
   `TaskCardModel.decorations`; this doc adds no second decoration
   carrier.
4. **Doc 2 §7's at-create question — answered here (D-V6: no)**:
   record the answer against that backlog entry when the wave
   lands, so the "`tasks add` parity" item stays the only open
   flags question.
5. **The anchor-task filter's constant home (doc 1 §7)**: the
   ruling is a *presentation-layer* exact-title filter over lists,
   counts, the today view, and search — but the UI crate cannot
   depend on the sync crate (AGENTS §2), so the anchor-title
   constant must be exported from a crate both see (domain-side
   pure const is the natural home). Doc 1's implementation chunks
   own the placement; this doc's surfaces consume it by role
   ("the anchor exact-title filter"). **Flagged for the
   implementation round** — the likely reconciliation point.
6. **Q8's board-scoped-read routing (multi-board §3.8)**: D-V3
   deliberately departs for the desktop today view (cross-board by
   design, board-pinned on kiosk) and extends
   selection-follows-focus to retarget `ViewState.board` from
   today rows and palette jumps. The multi-board implementation
   chunks should not "fix" this as a missed routing site.
7. **Doc 3 D-R4's banner boundary**: the today view's overdue
   section adds planning verbs only; `snooze_target`
   (presentation.rs :221–231) stays the banner's private machinery.
   If doc 3's reminder center (#203, deferred) later absorbs
   overdue triage, D-V4's re-commit action is the verb it calls.
8. **`TaskChanges.duedate`'s double option (doc 2 D-F1)**: the
   re-commit's `Some(Some(window_end))` relies on the
   clearing-is-a-write/clock-stamp contract; unchanged by this doc.

## 8. Backlog candidates (out-of-scope ideas; the user promotes)

- `tasks today` CLI verb + PDF export of the day's list
  (future-directions §3.7's PDF row is W2; both become trivial over
  `today_sections`).
- Time-ruler/day-shape rendering for Scheduled (P2's deeper form —
  a timeline layout, not list sections).
- Description and label text in `find`; a `label:`-style typed
  query grammar.
- Palette quality-of-life: recent-jumps section, debounce at
  cloud-scale data volumes (both premature at board scale, P7).
- Capture-overlay session count ("N captured" — P8 polish).
- Kiosk rotation policy adding Today as a display mode (the kiosk
  wave's state machine owns it; this doc only keeps the view
  rotation-ready).
- `tasks add` field-flag parity (doc 2 §7's separate open item —
  unaffected by D-V6's capture ruling).

## 9. References

- Daily-driver framing plan (normative §5 Q4; §2 scope; §6 chunks;
  §7 sequencing + the kiosk-statement instruction):
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`.
- Round doc 2, field family + preferences (ACCEPTED; shapes
  consumed): `.artifacts/plans/2026-10-18-field-family-design-plan.md`
  — D-F1 §3.1, D-F5 §3.5, D-F6 §3.6, §6's doc-4 note, §7.
- Round doc 1, app-data sidecar (ACCEPTED; §7's ruled doc-4 input:
  anchor hiding + fence warning):
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`.
- Round doc 3, reminders surfaces (ACCEPTED; D-R4 banner boundary,
  D-R13 kiosk auto-resolution):
  `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md`.
- Future-directions analysis §3.7/§6 (effort rows, prerequisites):
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`.
- GUI design requirements (R1/R4/R9 binding; §3 surface shapes; §5
  acceptance criteria): `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`.
- ADHD support research (principles P1–P14 as input; §6 rejected
  principles): `.artifacts/reports/2026-10-14-adhd-support-research-report.md`.
- Multi-board detailed design Q8 (board selection; kiosk
  degeneration): `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md` §3.8.
- Issues: #346 (today view), #347 (inbox + capture), #348 (fuzzy
  search) — milestone `wave-daily-driver`.
- Code anchors @ `75c42c9` (§2's table; symbols are the stable keys).

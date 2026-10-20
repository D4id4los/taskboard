# [Plan] Multi-board Chunk 7 — UI Board Dimension: View State, Switcher Actions, Board Bar

- **Date**: 2026-10-20
- **Kind**: stage-1 TDD-first implementation plan (dispatch plan §8,
  prompt 7.1) for chunk 7 of the multi-board implementation round.
- **Branch / worktree**: `feat/ui-board-switcher` at
  `../taskboard-WT-feat-ui-board-switcher` (created off
  `origin/main` = `885f72c`, after chunk 5's PR #375 — verified
  merged; chunk 6's PR #376 also already merged, so this chunk
  bases on a main that carries the whole spine through chunk 6).
- **Normative sources** (MD-7 rank): implementation plan §2 DF-2
  (the UI selection home), DF-8 (whole — every UI integration
  point), DF-9 (the badge read note in chunk 1's row) / §3 chunk 7 /
  §4 / §5 / §6; design §3.8 in full ( ViewState.board, the three
  ActionIds + registry rows, the keymap rule, `select_board`'s
  contract, the IdTables no-re-keying argument, board-bar under R1,
  compact under R9, the keyboard-journey leg, the badge interplay)
  and §3.2's badge paragraphs (option (a): the badge reads the
  *selected* board's status); GUI requirements R1/R4/R9
  (`.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`);
  dispatch plan §1 (execution model, MD-1…MD-9) and §8 (chunk 7's
  slice, acceptance, traps). Deviations from prose are recorded in
  §9 with reasons; nothing here deviates from a design §5 ACCEPTED
  decision.
- **Line numbers below are plan-day references** (MD-8); symbols and
  finding IDs are the stable keys. Every site was re-located on
  2026-10-20 against `885f72c` in the worktree.

## 1. Scope

**In** (DF-8 whole): `ViewState.board: Option<BoardId>` with the
seed rule (deterministic first-live-non-archived, seeding **only**);
`ViewState::select_board` mirroring `select_stack`; the
next/previous walk over the canonical board order (same order as
`boards list`) with the no-wrap rule; three `ActionId`s —
`BoardNext` (`board.next`), `BoardPrevious` (`board.previous`),
`BoardSelect` (`board.select`) — with registry rows (contexts
`[Board, StackDetail, TaskDetail] + Global`, category navigation),
router dispatch, and keymap entries that exist and resolve in the
`Board` + `ViNormal` contexts of every preset; the `push` builder
derives its board argument from `ViewState.board` with **no
`IdTables` re-keying**; the new `board-bar.slint` composition (one
segment per live board, selected state, compact hides the strip)
mounted in `board-layout.slint`, with segment clicks routed through
the action layer; the badge's selected-board read (upgrading chunk
1's aggregate fold read); replacement of every chunk-2 UI interim
fill; the scripted keyboard-journey board leg (two boards seeded,
next → second board's stacks re-rendered, previous back); the kiosk
compact degeneration (inherited); the two deliberate UI-crate
documentation-dump regenerations (keymap snapshots, golden PNGs).

**Out** (non-goals, implementation plan §3 chunk 7 + dispatch §8):
no engine command from switching — the dumb-client rule
(`FlowOutcome::Command` is never produced by
`select_board`/`next_board`/`previous_board`; a "helpful" sync nudge
on switch is a finding); no config-pinned fixed-board kiosk
(backlog); no UI board create/archive actions (backlog;
`stack.create` is the precedent, growth is post-wave); no
`IdTables` re-keying (the design's rejected double-refresh); no
per-board `IdTables` instances; no changes outside
`taskboard-ui-slint` beyond the CHANGELOG line; no new dependencies
(pure presentation layer — engine untouched, topology untouched).

**Dumb-client rule (absolute, AGENTS §1/§2)**: board switching is
pure view state, exactly like stack switching. It reads the
`ArcSwap` snapshot, mutates only `ViewState`, and returns only
`RefreshNow`-class outcomes. The router's board rows never produce
`DispatchOutcome::Command`.

## 2. Re-verified seam table (2026-10-20, base 885f72c)

| Seam | Site (plan-day line) | Role in this chunk |
|---|---|---|
| `ViewState` | `view_state.rs:59–75` | Gains `board: Option<BoardId>` (D1). |
| `select_stack` | `view_state.rs:380–384` | The mirror contract for `select_board` (set selection → reset focus to first task → `[RefreshNow]`). |
| `stack_ids` (assoc fn) | `view_state.rs:194–199` | Becomes an instance method reading `self.board`; today it consumes `live_board`. |
| `reconcile` | `view_state.rs:586–628` | Gains the seed + dead-selection repair (D1). |
| CreateStack interim fill | `view_state.rs:474–488` (INTERIM marker) | Replaced by the `self.board` read (D6). |
| Interim tests | `view_state.rs:974–988`, `1020–1040` (INTERIM markers) | Replaced (D6, §7). |
| `live_board` | `presentation.rs:250–252` | Survives as the seeding rule **only**; joined by `live_boards` (D3). |
| `sync_badge` | `presentation.rs:159–171` (fold read, comment "the selected-board read is chunk 7's") | Signature gains the board; reads the board's row (D8). |
| `SyncStates` | domain `state.rs:101–172` (`get`, `aggregate`) | Read-only consumer; `get(board) -> Option<&SyncStatus>`; derivation materializes a default row per known board (`state.rs:221–223`). |
| `ActionId` / `ActionEntry` / `REGISTRY` / `GLOBAL` / `VIEW_NAV` | `actions.rs:92–120`, `177–204`, `206–399`, `193`, `194–198` | Three variants + rows + a new contexts const (D4). Coverage test `ALL` list at `actions.rs:427–455` grows. |
| Router | `router.rs:101–164` (`dispatch`), `DispatchCtx:84–92`, `UiEffect:29–63` | Three rows; `DispatchCtx.board_target`; three `UiEffect` variants (D4). |
| Shell | `shell.rs:75–115` (`apply`'s exhaustive match) | The new effects join the no-op arm (shell records nothing for view intents). |
| Presets | `keymap/presets.rs:62–186` | Six new rows (two per preset); `keymap/mod.rs:426–459` core-verb list grows (D5). |
| Interpretation table | `adapter.rs:205–292` (`interpret`; `leaving` set at `:209–212`) | Board effects join `leaving`; three default arms (D4). |
| `route()` ctx assembly | `adapter.rs:121–139` | `board_target` one-shot consumption, mirroring `reminder_target`. |
| `select_column` click seam | `adapter.rs:561–573` | The structural mirror for the board-bar segment seam (index → id table → view-state method). |
| BeginCreateStack guard | `adapter.rs:257–265` (last non-seed `live_board` read) | Moves into `begin_create_stack` (D6 — adapter ends with zero `live_board` reads). |
| `push` / `push_chrome` / `push_status_bar` | `models.rs:101–129`, `133–147`, `268–319` (fold reads at `:277`, `:286`; `live_board` at `:110`; assoc `stack_ids` at `:111`) | Board argument from `vs.board()`; bar model push; badge/recency per-board (D7/D8). |
| `IdTables` | `models.rs:34–65` | Grows `board_ids: Vec<BoardId>` (an added D2 index table — **not** a re-keying; the content tables stay single-board-per-refresh). |
| `board-layout.slint` | `ui/compositions/board-layout.slint:26–56` (top-level if-branches) | Strip mount (D7): top level becomes a `VerticalLayout` hosting `BoardBar` above the kanban in the non-compact branch only. |
| `board-view.slint` / `app-window.slint` | `views/board-view.slint:18–57`, `ui/app-window.slint:55–193` | Bar model property + `board-bar-segment-activated(int)` callback threaded window → view → layout. |
| Sibling compositions | `toolbar.slint`, `status-bar.slint` | The L2 pattern the new `board-bar.slint` follows (properties in, callbacks out, layout only). |
| Compact switcher segments | `board-layout.slint:115–154` | The segment rendering pattern (tab role, selected state, `TouchArea` click) the bar mirrors one level up. |
| Keymap snapshots | `tests/keymap_snapshots.rs` (registry + resolved-preset dumps) | One deliberate `insta` regeneration pass (D10). |
| Golden visuals | `tests/golden_visuals.rs` (6 PNGs at `tests/goldens/`) | 4 non-compact regen; 2 compact **must hold** (D10). |
| Journey harness | `tests/journeys.rs` (`Harness::press` at `:92–111` resolves through `menu::entries`), `tests/support/{driver,fake_engine,fixture,guard}.rs` | The board leg's substrate; fixture grows a two-board state (D9). |
| Lib assoc-fn test uses | `lib.rs:1253`, `lib.rs:1316` (`ViewState::reading_order` / `ViewState::stack_ids`) | Migrate to the instance methods (mechanical). |
| CLI `boards list` order | `crates/taskboard-app/src/commands/boards.rs:69` (`entries.sort_by_key(|e| e.id)`) | The canonical order the walk must match: board-id ascending = `state.boards` `BTreeMap` order. |

**Precondition record**: `git fetch origin --prune`;
`gh pr list --state merged --limit 5` shows #375
(`feat/app-connector-registry`, 2026-10-20T12:00:24Z) and #376
(`feat/app-cli-connector-verbs`, 2026-10-20T15:37:49Z) merged;
`scripts/worktree-doctor.sh` exited 0 (free 144G, floor 25G). The
worktree was created off `origin/main` = `885f72c` and the branch's
auto-created `origin/main` upstream was unset (a bare `git push`
must never target `main`).

## 3. Decisions (each resolves an open question; none touches a design §5 ACCEPTED row)

- **D1 — `ViewState.board`'s type, the seed rule's trigger, and
  dead-selection repair.** Type: `board: Option<BoardId>` (`None` =
  fresh install / zero boards), private field + `pub const fn
  board(&self) -> Option<BoardId>` getter. **Trigger**: the seed
  lives in `reconcile` — the refresh-time point where every snapshot
  meets the view state (`adapter::refresh` runs `reconcile` before
  `models::push`, and `mount` performs an immediate first refresh,
  `lib.rs:175`, so every real session holds `Some(board)` from the
  first frame; there is no reachable action-before-first-refresh
  path, so no separate lazy-read seeding is built — one rule, one
  site). The rule: a private `normalize_board(&mut self, state)`
  sets `self.board` from `presentation::live_board(state)` when
  `None`; **and** clears a `Some(id)` that is no longer live
  (`boards.get(id)` missing, `deleted`, or `archived`) back through
  `None` → re-seed. Rationale: the design's "seeded by
  `live_board` when None … survives as the seed only" plus the
  reconcile pattern already applied to stacks/tasks/subjects (a
  board deleted or archived under the session must fall back to the
  first live board, not render a dead board's emptiness). Liveness =
  `live_board`'s own predicate (`!deleted && !archived`), so an
  archived board is unreachable as a selection, which D7's marker
  decision relies on.
- **D2 — `select_board`'s contract, mirroring `select_stack`
  exactly.**
  `pub fn select_board(&mut self, state: &AppState, board: BoardId) -> Vec<FlowOutcome>`:
  sets `self.board = Some(board)`; resets `selected_stack` to the
  new board's first stack in display order (`self.stack_ids(state)`
  under the new board); resets `focused_task` to that stack's first
  task (`first_task_of`); returns `vec![FlowOutcome::RefreshNow]`.
  An empty target board leaves `selected_stack`/`focused_task`
  `None` (the same shape `select_stack` has on a task-less stack).
  **No view/subject reset and no flow handling** — the design names
  exactly three writes, and the stale `subject_stack`/`subject_task`
  of the old board are popped to `Board` by the existing
  refresh-time subject re-validation (their stacks are not in the
  new board's `stack_ids`), pinned by a unit test (§7 T4). No
  liveness guard on the argument: a stale-index click cannot name a
  dead board through a fresh tables pass, and an interleaved
  deletion is repaired by `reconcile` (the `select_stack` precedent
  has no guard either).
- **D3 — next/previous walk + the canonical order helper + no-wrap.**
  New `presentation::live_boards(state) -> Vec<&Board>`: live
  boards (`!deleted && !archived`) in `state.boards` `BTreeMap`
  iteration order — board-id ascending, the exact order the CLI's
  `boards list` renders (`boards.rs:69` sorts by id). `live_board`
  stays as-is (it is `live_boards().first()` by construction) and
  its doc comment narrows to the seeding rule.
  `ViewState::next_board`/`previous_board(&mut self, state) ->
  Vec<FlowOutcome>`: `normalize_board` first (None seeds; a dead
  selection re-seeds), then find the current index in `live_boards`;
  step ±1 **clamped — no wrap** (D7's rule at the board level: the
  ends hold); out-of-range or single-board ⇒ nothing changed ⇒
  `Vec::new()` (no repaint — the focus walk's "ends hold" shape,
  `view_state.rs:366/373`). A step that lands calls `select_board`
  with the neighbour.
- **D4 — the three ActionId rows + router dispatch + the click
  seam.** Variants `BoardNext`/`BoardPrevious`/`BoardSelect` with
  dotted ids `board.next`/`board.previous`/`board.select`; labels
  "Next board" / "Previous board" / "Select board"; a new contexts
  const `GLOBAL_VIEW_NAV: &[ContextId] = &[Global, Board,
  StackDetail, TaskDetail]` (the design's "`VIEW_NAV`-style … +
  `Global`": global-capable like the `nav.*` family, offered in
  every context's menu); `category: Navigation`; `enabled: true`;
  rows placed in the registry after `NavPrevItem` (the navigation
  family's documentation grouping; the dump snapshot regenerates in
  registry order). Router: `BoardNext → UiEffect::BoardNext`,
  `BoardPrevious → UiEffect::BoardPrevious`,
  `BoardSelect → match ctx.board_target { Some(b) =>
  UiEffect::BoardSelect(b), None => NoTarget }` — the
  ReminderSnooze/ReminderMute parameterized-target precedent
  (`router.rs:130–144`): `DispatchCtx` gains
  `board_target: Option<BoardId>`; the adapter holds a one-shot
  `board_target` consumed in `route()` exactly like
  `reminder_target` (`adapter.rs:122–124`). The **click seam**
  (`adapter.rs`, new `install_board_bar_seam`):
  `on_board_bar_segment_activated(index)` resolves
  `tables.board_ids[index]`, sets `adapter.board_target =
  Some(board)`, routes `ActionId::BoardSelect` — every click
  affordance has an action equivalent (§7.1), and the menu/hint
  projections see the verb (R8). Interpretation table: the three
  effects join the `leaving` set (see D5) and gain default arms
  routing to `next_board`/`previous_board`/`select_board`;
  `shell.rs::apply` adds them to the view-intent no-op arm.
- **D5 — board switching mid-flow: the leaving rule.** A board
  switch is the strongest form of view departure: the whole
  content surface unmounts. The three board effects therefore join
  `leaving` (`adapter.rs:209–212`) — an open edit flow commits
  first (against the board being left; the commit runs before
  `select_board` changes `self.board`), a move flow cancels. By
  keyboard this is nearly unreachable (flows hold insert mode;
  `ViInsert` masks the view contexts, so `b`/`B` reach the text
  field), but the **click path** can switch mid-flow, and D9's
  rule "leaving the edit field by effect first commits the buffer"
  applies verbatim. Pinned by a lib-level test (§7 T13). The
  stale-`flow` binding in `interpret` (captured before the
  leaving-commit) routes the board effects through the default
  arms, which is exactly the post-commit clean state.
- **D6 — the `push` builder's board switch, the interim-fill
  removal inventory, and where the empty-board refusal lives.**
  `models::push` and its sub-pushes derive the board from
  `vs.board()`: `push_chrome`'s toolbar title reads
  `state.boards.get(&vs.board()?)` (the **selected** board's title,
  not the first live board's); the stack list comes from the
  instance `vs.stack_ids(&state)`; `stack_ids`/`reading_order`
  become `&self` methods over `self.board` (no seeding inside —
  `reconcile` has already normalized before every push; the two
  lib.rs test uses of the assoc fns migrate mechanically). The
  **CreateStack commit** (`view_state.rs:474–488`) reads
  `self.board` instead of the inline `live_board` fill: the command
  names the selected board; `None` still ends the flow with no
  command (today's empty-board behavior, unchanged). The
  **BeginCreateStack guard** moves from the adapter's
  interpretation table (`adapter.rs:257–265`, the last non-seed
  `live_board` read in the crate) into
  `begin_create_stack(&mut self, state)`, which normalizes then
  refuses (`Vec::new()`) while `board` is `None` — one home for the
  board-existence refusal, and the adapter ends with **zero**
  `presentation::live_board` reads (the stage-3 trap "`live_board`
  surviving as a read path beyond the seed rule" becomes
  structurally impossible outside `view_state`'s normalize + its
  own tests). **Full inventory of replaced reads/fills**: (1)
  `view_state.rs:474–488` interim fill; (2) `view_state.rs:194–199`
  `stack_ids`'s `live_board`; (3) `models.rs:110` chrome title;
  (4) `models.rs:111` assoc `stack_ids`; (5) `models.rs:277` badge
  fold; (6) `models.rs:286` recency aggregate; (7)
  `adapter.rs:261` guard; (8) the two INTERIM-marked tests
  (`view_state.rs:974–988`, `:1020–1040`) — every `INTERIM(multi-
  board-2 → chunk-7)` marker in the crate is gone.
- **D7 — `board-bar.slint` composition, the strip mount, and the
  archived marker.** New `ui/compositions/board-bar.slint` (L2,
  sibling file of toolbar/status-bar per design §3.8; the mount is
  in `board-layout.slint` per the implementation plan §3 chunk 7 —
  the two statements compose as "sibling composition file, mounted
  by the board layout"). Model: `BoardBarModel { segments:
  [BoardSegmentModel], selected: int }` with `BoardSegmentModel {
  title: string }` — defined in `board-bar.slint`, exported, and
  re-exported through `board-view.slint`/`app-window.slint` so
  `include_modules!` exposes it to `models.rs`. Rendering: a
  `HorizontalLayout` of segment `FocusScope`s mirroring the compact
  stack switcher's segments one level up (`board-layout.slint:115–154`):
  `AccessibleRole.tab` + `tab-list`, real tab stops, selected
  state via `Theme.surface-raised` background +
  `Typography.body-emphasized`/`text-primary` (unselected:
  `body`/`text-secondary`), `TouchArea` click → callback
  `segment-activated(index)`. Primitives reused only (R1: no new
  primitive, no view-local restyle — the segment pattern is copied
  *shape*, colours/tokens all from `Theme`). A zero-segment model
  (never-booted install) renders no row (guarded by segment count).
  **Mount**: `board-layout.slint`'s top level becomes a
  `VerticalLayout` — `BoardBar { bar: root.bar;
  segment-activated(i) => root.board-bar-activated(i) }` above the
  kanban, mounted in the **non-compact branch only**; the compact
  branch is unchanged and strip-less (**compact hides the strip —
  keyboard switching still works**; the two 600px goldens holding
  bit-for-bit is the pin). Threading:
  `app-window.slint` gains `in-out property <BoardBarModel>
  board-bar-model` + `pure callback board-bar-segment-activated(int)`,
  passed into `BoardView` (`board-bar` property + callback) into
  `BoardLayout` (`bar` property + `board-bar-activated` callback).
  **Archived marker — recorded deviation from design §3.8's
  parenthetical**: the segment set is live boards only (DF-8, the
  higher-ranked fact), and D1's normalize repairs a selection that
  dies mid-session *before any push*, so an archived board can
  never appear in the strip — a marker would be unreachable dead
  UI (a code-quality-mandate finding). The segment model carries no
  `archived` field; if a later wave ever renders archived boards
  (e.g. for un-archiving), the field and marker are added then.
- **D8 — the badge's selected-board read (upgrading chunk 1's fold
  read) and the recency caption.**
  `presentation::sync_badge(sync: &SyncStates, board: Option<BoardId>)
  -> SyncBadge`: with `Some(board)`, phase + pending read
  `sync.get(board).cloned().unwrap_or_default()` (the derivation
  materializes a default row per known board, so a bound-but-never-
  synced board reads `Idle`/0 — the honest fresh-install display;
  `None` (no boards at all) is `SyncStatus::default()`, identical
  to today's empty-map fold). `models::push_status_bar` calls it
  with `vs.board()`; the **recency caption** switches from
  `state.sync.aggregate().last_success` to the same row's
  `last_success` — the caption describes the badge's freshness and
  follows it per board (the badge interplay: "switching boards
  re-reads the per-board map in the same refresh pass — no extra
  wiring": the refresh already re-derives the badge from the state
  it loads; nothing else moves). The domain `aggregate()`/`aggregate_rows`
  folds stay untouched — they serve the app-side surfaces (daemon
  `status`), not this crate.
- **D9 — the scripted journey leg + the two-board fixture.** New
  fixture `fixture::two_board_state() -> (Fixture, AppState)`:
  board 1 (`BoardId::from_u128(0x1001…)` — deterministic ids so the
  canonical order is fixed: board 1 < board 2 ⇒ board 1 is the
  seed) with stacks "a" (2 tasks) and "b" (1 task); board 2
  (`0x1002…`) with stack "c" (1 task); distinct titles ("board
  one"/"board two") for the toolbar-title assertion; sync rows
  derived idle. New journey test
  `board_switching_walks_boards_by_keyboard` (one behaviour, one
  test): mount → assert `board_shape() == [2, 1]` and
  `toolbar_title == "board one"` (model reads, not label-text
  matching) → `press(ActionId::BoardNext)` → assert
  `board_shape() == [1]` (the second board's stacks re-rendered)
  and the toolbar title switched → `press(ActionId::BoardNext)` →
  **holds at the end** (no wrap; shape unchanged) →
  `press(ActionId::BoardPrevious)` → back to `[2, 1]` /
  "board one" → `assert_commands(&[])` at every step (the
  dumb-client pin). Then one compact leg: `set_window_size(600,
  800)`, assert `compact_active`, `press(ActionId::BoardNext)` →
  shape switches (keyboard switching remains while the strip is
  hidden — the kiosk/compact operator path). No new guard `Verb`
  (navigation carries no ceiling).
- **D10 — the two deliberate UI-crate dump regenerations.** (a)
  `tests/keymap_snapshots.rs`: the registry dump gains three rows
  and the resolved-preset dump gains six bindings — one
  `INSTA_UPDATE=always cargo nextest run -p taskboard-ui-slint`
  pass inside the chunk, hand-reviewed as "the three registry rows
  + the six preset rows, nothing else", recorded in the commit
  message (the file's own convention). The design's "no insta
  churn in the UI crate" is honoured as *no unintended* churn: the
  UI crate's own documentation dumps grow when the registry grows;
  no engine/state-crate snapshot is touched by this chunk (no
  snapshotted domain shape changes). (b)
  `tests/golden_visuals.rs`: the strip changes the regular/expansive
  board render — `TASKBOARD_REGEN_GOLDENS=1` rewrites the 4
  non-compact PNGs (0.5/3.0 × 1000/1500); the run then **fails by
  design** and a compare-only run must show the 2 compact goldens
  (600px) holding untouched — that is the compact-hide pin at the
  pixel tier. The PR's PNG diff is the review artefact.
- **D11 — kiosk: no new wiring.** Kiosk is fullscreen on the same
  window (`lib.rs:150–156`); there is no kiosk-specific breakpoint
  and none is added. The kiosk surface *inherits* the compact
  degeneration by construction: a compact-width kiosk shows no
  strip, a wider one shows it (chrome an operator can use);
  switching stays keyboard-available in both. The design's
  config-pinned fixed-board kiosk stays backlog; force-hiding the
  strip at all kiosk widths would need a kiosk global the design
  does not name — recorded as a backlog candidate in the stage
  report, not built (no-speculative-prep).
- **D12 — keymap preset rows (the tunable).** Default strokes,
  recorded as the draft the preset table is: **standard** —
  `BoardNext`/`b`, `BoardPrevious`/`B` in `Board`; **emacs** — the
  same two strokes in `Board` (printables are otherwise unbound in
  emacs's Board context; the C-x families are untouched); **vi** —
  `BoardNext`/`b`, `BoardPrevious`/`B` in `ViNormal`
  (vim-authentic: `b` is a motion home-row key, and both strokes
  are free in ViNormal today). The contract the chunk fixes is
  existence + resolution (design §3.8): both verbs are added to
  `every_preset_covers_the_core_verbs`' core list (R4 names
  "navigate boards" a core verb), and a new resolution test walks
  every preset asserting `board.next`/`board.previous` resolve in a
  `[Board, ViNormal, Global]` chain (standard/emacs resolve from
  `Board`, vi from `ViNormal`). `board.select` carries **no preset
  stroke** — it is the parameterized click target (the
  ReminderSnooze/ReminderMute precedent: target-carrying actions
  are registry rows and menu entries, never keyboard-bound; its
  keyboard equivalent *is* next/previous), so it is not in the
  core-verb list; a menu invocation without a target absorbs as
  `NoTarget`, exactly like the reminder verbs.
- **D13 — CHANGELOG.** One `Unreleased → Added` line: the UI board
  dimension — board switcher strip + `board.next`/`board.previous`
  keys + per-board sync badge (user-visible wording at stage 2's
  discretion, promoted by chunk 8).

## 4. IdTables: the no-re-keying argument (recorded)

`IdTables` (`models.rs:34–45`) stays a single-board-per-refresh
structure: `stack_ids`/`board_tasks`/`stack_tasks` continue to
describe **the viewed board only** — they are rebuilt in the same
pass that pushes the models ("same thread, rebuilt together",
`models.rs:31–33`), and a board switch is a `RefreshNow` outcome
that triggers exactly that rebuild. The tables gain one field,
`board_ids: Vec<BoardId>` — the D2 index-table rule applied to the
new strip surface (the bar's segment indices resolve against the
pass that pushed them, like `stack_ids` for the switcher), which is
an *added table*, not a re-keying: no map keyed by board, no
per-board table instances, no N−1 invisible boards refreshed per
cycle. The design's rejected alternative (per-view-instance tables
double-refreshing every board each cycle) is not built.

## 5. TDD order (stage 2 executes in this order; every test names its layer)

Unit layer first (`#[cfg(test)]` modules — plain assertions, no
insta), then the wiring, then the integration/journey tier, then
the two dump regenerations last (they document the finished
tables). "One behaviour, one test, lowest layer": the router tests
pin routing only; the view-state tests pin selection semantics; the
journey pins the end-to-end verb.

1. **T1 presentation — `live_boards`** (`presentation.rs` tests):
   filters deleted/archived, returns board-id ascending order
   (deterministic fixture ids, two live + one dead + one archived);
   `live_board` = its first element (the seed rule's determinism).
2. **T2 presentation — `sync_badge` per board** (rewrites the
   `sync_badge_maps_every_phase` shape): two boards with different
   stored rows (Syncing on board 1, Offline on board 2) + derived
   pending ops attributed to board 1 only ⇒ the badge follows the
   *named* board (phase and pending both); `None` ⇒ the default
   badge; an unknown-but-named board ⇒ the default row.
3. **T3 view-state — the seed rule** (`view_state.rs` tests, the
   fixture grows a deterministic second board):
   `ViewState::new().reconcile(&state)` seeds board 1 (lowest id
   live); the archived-lowest case seeds the non-archived board;
   zero boards stays `None` (and `stack_ids` is empty).
4. **T4 view-state — dead-selection repair**: select board 2,
   delete (or archive) it in the state, `reconcile` ⇒ board falls
   back to the first live board; and: from `StackDetail` on board
   1's stack, `select_board(board2)` then `reconcile` pops the view
   to `Board` (the stale subject re-validation — D2).
5. **T5 view-state — `select_board` mirrors `select_stack`**:
   sets `board`, `selected_stack` = new board's first stack,
   `focused_task` = that stack's first task, returns exactly
   `[FlowOutcome::RefreshNow]`; empty target board ⇒ `None`/`None`
   selection with the same single outcome; the returned outcomes
   (and those of T6) contain **no `FlowOutcome::Command`** (the
   dumb-client unit pin).
6. **T6 view-state — the walk + no-wrap**: from the seed,
   `next_board` → board 2 (`[RefreshNow]`); `next_board` again →
   holds (`Vec::new()`, board unchanged); `previous_board` → board
   1; `previous_board` again → holds; single-board state: both
   directions inert; unseeded state with boards present: the walk
   seeds then steps (normalize-first).
7. **T7 view-state — CreateStack names the selected board**
   (replaces `the_create_stack_interim_fill_picks_the_first_live_board`):
   select board 2, open the stack flow, commit ⇒ the
   `CreateStack` command names board 2 with board 2's append
   order; the no-board case (boards empty) still commits nothing
   (supersedes `the_create_stack_flow_commits_nothing_without_a_board`,
   INTERIM markers gone); `begin_create_stack` on a board-less
   state returns `Vec::new()` (the moved refusal).
8. **T8 actions/router/shell**: the registry coverage `ALL` list
   grows by three (exactly once each); router — `BoardNext`/
   `BoardPrevious` route to their effects from the default ctx;
   `BoardSelect` with `ctx.board_target` routes
   `UiEffect::BoardSelect(board)`, without it `NoTarget`; shell —
   the three effects are no-ops returning `true`.
9. **T9 keymap contracts**: `every_preset_covers_the_core_verbs`
   core list + `BoardNext`/`BoardPrevious`; new test — every
   preset resolves both verbs in a `[Board, ViNormal, Global]`
   chain; the existing conflict/prefix-shadow/parse contract tests
   run unchanged over the grown tables (they are the guards that
   the chosen strokes are legal, not pins of the strokes).
10. **T10 models — push reads the selection** (lib.rs §6-tier, on
    the slint test thread): two-board state mounted ⇒ toolbar
    title is board 1's; a `board.next` press (or the direct
    view-state seam) ⇒ title is board 2's, `board_model.stacks`
    re-rendered to board 2's columns, the sync model reads board
    2's row (fixture stores differing phases per board), and the
    `board-bar` model carries two segments with `selected == 1`.
11. **T11 adapter — the click seam + the dumb-client boundary**
    (lib.rs tier): `invoke_board_bar_segment_activated(1)` ⇒ view
    switches (title/model assert) and the command channel stays
    empty; a stale index (out of range) is absorbed silently.
12. **T12 journey — the board leg** (D9's test, in `journeys.rs`).
13. **T13 lib — click-mid-flow commits against the board being
    left** (D5): open the stack-create flow on board 1 (type a
    title), invoke the bar's second segment ⇒ the `CreateStack`
    command for **board 1** is captured at the boundary, the flow
    is closed, and the view shows board 2.
14. **T14 dumps**: the keymap-snapshot regen pass and the golden
    regen/hold runs (D10), executed only after T1–T13 are green.

## 6. Implementation slices (each a standalone commit)

1. `feat(ui): presentation board-order helper + per-board sync badge`
   — T1 + T2 (pure presentation, no consumers switched yet).
2. `feat(ui): ViewState board selection — seed, select_board, the no-wrap walk`
   — D1/D2/D3 + T3–T7; the interim fill and its markers go here;
   `stack_ids`/`reading_order` go instance; the lib.rs assoc-fn
   test uses migrate.
3. `feat(ui): board.next/previous/select actions, router rows, preset keys`
   — D4 + D12 + T8 + T9; `UiEffect` variants, ctx field, shell
   arm, interpret arms (leaving + defaults), the moved
   `begin_create_stack` guard (D6 item 7), adapter `board_target`.
4. `feat(ui): models push from the selected board + the board bar strip`
   — D6/D7 + T10/T11: `models.rs` board switch, `IdTables.board_ids`,
   `board-bar.slint`, the mount + threading, the click seam.
5. `test(ui): the keyboard-journey board leg + click-mid-flow pin`
   — D9 + T12 + T13 + the fixture growth.
6. `chore(ui): regenerate keymap dumps + board goldens for the bar`
   — D10 (both passes, compact-hold verified) + D13's CHANGELOG
   line (or the CHANGELOG rides slice 4 — stage 2's call, kept out
   of the dump commit).

Slice order is also compile order: 2 needs 1's `live_boards`
indirectly (through normalize), 3 needs 2's methods, 4 needs 3's
effects, 5 needs 4's strip. Every slice passes the full gate set
(`fmt --check`, `clippy --workspace --all-targets -D warnings`,
`nextest`, doc tests, `cargo deny`, the CI-identical llvm-cov
three-command sequence + quarantine + `rm -rf
target/llvm-cov-target`).

## 7. Test catalogue (consolidated; plain assertions throughout — no insta in the UI crate beyond the D10 dumps, no text assertions, typed/structural outcomes only)

- presentation: T1 (`live_boards` order/filter + `live_board`
  first-of), T2 (badge per board + None default).
- view_state: T3 (seed rules), T4 (dead-selection repair + subject
  pop), T5 (select_board mirror + no-Command), T6 (walk/no-wrap/
  single-board/seed-then-step), T7 (CreateStack names the selected
  board; no-board refusal; moved `begin_create_stack` guard).
- actions: coverage ×3 (T8).
- router: three rows incl. target/NoTarget split (T8).
- shell: effect no-ops (T8).
- keymap: core-verb growth + per-preset resolution (T9); existing
  conflict/shadow/parse contracts unchanged.
- lib §6-tier: T10 (push selection: title, columns, badge row, bar
  model), T11 (click seam, dumb-client boundary, stale index), T13
  (click-mid-flow commit).
- journeys: T12 (the board leg + compact keyboard leg).
- dumps: T14 (keymap snapshot regen; golden regen + compact hold).
- No engine/state-crate test changes: this chunk touches no
  snapshotted domain shape; the engine corpus does not churn.

## 8. Risks

- **Golden regen masking drift**: a wholesale PNG rewrite could
  hide an unintended layout change. Mitigation: the 2 compact
  goldens must hold bit-for-bit (the strip is genuinely absent
  there), the PR image diff is reviewed, and T10/T12 assert model
  structure independently of pixels.
- **Three-layer markup threading** (window → view → layout → bar):
  naming drift between `board-bar-model`/`bar`/`board-bar-activated`
  properties. Mitigation: slice 4 lands the whole thread in one
  commit; the compiler checks the slint property graph at build
  time.
- **`select_board`'s reliance on reconcile for subject popping**:
  a stage-2 temptation to "helpfully" reset the view inside
  `select_board` would drift from the mirrored contract. T4 pins
  the reconcile path; the stage-3 trap list already names
  "select_board not mirroring select_stack exactly" as a finding.
- **Journey `press()` requires menu visibility**: `Harness::press`
  resolves through `menu::entries`, so the board verbs must be
  menu-offered in the active chain — guaranteed by the
  `Global`-tagged contexts const (D4), asserted implicitly by T12
  (`press` panics if unresolvable).
- **Coverage-test core-list growth is a judgment call** (D12):
  adding board.next/previous to the core list makes every future
  preset carry them — intended (R4 core verb), and the resolution
  test documents the per-preset context style.
- **Estimate risk**: the board-layout top-level restructure (D7)
  touches the one composition with two layout branches; a mistake
  there is caught by the goldens + journeys, both of which exercise
  both branches.

## 9. Deviation record (MD-7)

- **Archived marker omitted from the bar's segments** (D7) —
  deviation from design §3.8's parenthetical "(title, archived
  marker, selected state)". Reason: DF-8 (the higher-ranked fact)
  fixes the segment set at live boards, and D1's refresh-time
  normalize repairs a dead/archived selection before any model
  push, so an archived board is unreachable in the strip — the
  marker would be dead UI (speculative generality, a stage-3
  mandate finding by construction). Recorded here for the stage-3
  reviewer to accept.
- **BeginCreateStack's guard moves from the adapter into
  `view_state::begin_create_stack`** (D6) — a small structural
  deviation from "the adapter resolves effects against the
  view-state model" prose (the guard becomes part of the method the
  table calls). Reason: single home for the board-existence
  refusal + the adapter must end with zero non-seed `live_board`
  reads (the trap list's structural impossibility). The
  interpretation-table shape is otherwise untouched.
- Everything else follows DF-8/design §3.8/§3.2 and the dispatch
  §8 slice verbatim; no design §5 ACCEPTED decision is touched.

## 10. Estimated diff

~650–750 changed lines plus two regenerated dump sets:
`view_state.rs` (+~130 incl. tests), `presentation.rs` (+~60),
`actions.rs` (+~45), `router.rs` (+~35), `shell.rs` (+~4),
`keymap/presets.rs` (+~8), `keymap/mod.rs` (core list, +~4),
`models.rs` (+~75), `adapter.rs` (+~40, −guard), new
`ui/compositions/board-bar.slint` (~80), `board-layout.slint`
(+~35), `board-view.slint` (+~10), `app-window.slint` (+~15),
`lib.rs` (test migrations, ~15), `fixture.rs` (+~45),
`journeys.rs` (+~95), 2 keymap snapshot files regenerated, 4
golden PNGs regenerated, CHANGELOG (+~8). One new source file
(`board-bar.slint`); no new Rust files; no dependency changes
(`cargo deny` trivially green).

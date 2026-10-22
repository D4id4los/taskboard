# [Plan] GUI Review Round 1, Chunk 2 — Stack column resize with device-local persistence

- **Date**: 2026-10-22
- **Status**: READY — stage-1 plan for dispatch chunk 2
  (`feat/ui-stack-width-persistence`). Precondition gate passed: chunk 1
  is merged into `origin/main` (tip `d470e28`, branch deleted on the
  remote). Worktree `../taskboard-WT-feat-ui-stack-width-persistence`
  created off the freshly fetched `origin/main` after
  `scripts/worktree-doctor.sh` returned green (97 G free, floor 25 G).
- **Origin** (normative input):
  `.artifacts/plans/2026-10-22-gui-review-round1-plan.md` §6
  (D16–D21), §1 facts F7/F8/F13–F16, §10 testing, §14 risks R5/R6;
  the dispatch plan §1/§3 (chunk-2 prompt, acceptance, code-quality
  mandate). Deviations from the design round are recorded in §9 with
  reasons; everything else implements the design verbatim.
- **Scope guard**: `taskboard-ui-slint` + `taskboard-app` (config, ui
  lifecycle) only. The diff over `taskboard-domain`,
  `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud` stays **empty** (acceptance item; stage 3
  asserts it). No domain serde, no `PersistenceAction`, no engine
  commands (D20's explicit rejections).

## 1. Baseline facts (post-chunk-1, anchored in this worktree; do not re-derive)

The design round's §1 anchors predate chunk 1; the line numbers and
two behavioral facts below are the *current* baseline chunk 2 builds
on.

- **F-1 Columns row.** Regular/expansive renders one `VerticalLayout`
  per stack in a single `HorizontalLayout` with `Theme.space-3`
  spacing plus a trailing add-column affordance
  (`ui/compositions/board-layout.slint:97-202`). The per-column loop
  child carries no width state; nothing binds `min-width`/`max-width`/
  `width` on it. `StackColumnModel` is `{title, tasks, create-open}`
  (`board-layout.slint:21-26`). The header `RoleText` already carries
  `overflow: elide` **unconditionally**
  (`board-layout.slint:121-132`, chunk-1 commit `6238f5e`, in-file
  comment deferring "conditional engagement" to chunk 2). The columns
  row is only mounted regular/expansive; compact renders the switcher
  + one column (`board-layout.slint:214-306`) and
  `ui/views/stack-view.slint` is the StackDetail view (single stack,
  no columns).
- **F-2 The window reports its logical size**: `window-w`/`window-h`
  out properties exist (`ui/app-window.slint:107-108`), and the scale
  is readable from `Theme.base-scale` — the clamp math needs no new
  window surface.
- **F-3 Dispatch/try_send pattern.** `Adapter::route` applies
  `DispatchOutcome::Command` via `self.commands.try_send` with a
  warn-and-drop on a full/closed inbox
  (`src/adapter.rs:134-140`); `apply_outcomes` mirrors it
  (`src/adapter.rs:164-189`). `UiEffect`s reach
  `Adapter::apply_effect`/`interpret`
  (`src/adapter.rs:212-365`); the exhaustive router is
  `src/router.rs:114-185`.
- **F-4 Mount is 5-arg** (`src/lib.rs:139-196`):
  `(state, signals, system, commands, settings)`; `UiSettings`
  (`src/lib.rs:51-77`) carries `{scale, keymap, startup, kiosk}`.
  Mount call sites: `crates/taskboard-app/src/ui.rs:294` (desktop) and
  `crates/taskboard-app/src/main.rs:507` (kiosk appliance). The
  journeys harness mounts through the public `mount()`
  (`tests/journeys.rs:42-61`).
- **F-5 View state owns selection.** `ViewState`
  (`src/view_state.rs:58-79`) holds board/subject/focus/selection as
  pure state over `AppState` slices; board selection is
  caller-owned (DF-2) — widths are the same class of UI-local state.
  `reconcile` (`src/view_state.rs:673-716`) is the refresh-time
  normalizer; it must keep leaving the widths map alone (pruning is
  the app forwarder's job, §5 D-6).
- **F-6 Registry/presets shape.** `ActionEntry` carries
  `{id, label, contexts, category, enabled, menu_rank, description,
  menu_group}` (`src/actions.rs:281-315`); the BoardStack group's
  ranks today: BoardNext 60, BoardPrevious 70, BoardSelect 80,
  StackCreate 90. Keymap contract tests are list-driven:
  `every_preset_covers_the_core_verbs`
  (`src/keymap/mod.rs:426-470`), conflict-freeness and prefix-shadow
  passes run over every preset (`src/keymap/mod.rs:498-560`). The
  registry insta snapshot is
  `tests/snapshots/keymap_snapshots__registry.snap`, regenerated with
  `INSTA_UPDATE=always` and recorded in the same commit.
- **F-7 Pointer/drag support on the pinned Slint 1.18.1 — probe
  result (recorded, no throwaway crate needed).** The installed
  sources show `mouse-cursor: ew-resize` is a TouchArea property
  (`i-slint-compiler-1.18.1/tests/syntax/elements/toucharea.slint`),
  and **every builtin tableview implements column resize exactly the
  way this chunk will**: a 10 px-wide TouchArea with
  `mouse-cursor: ew-resize` and
  `moved => if (self.pressed) { adjust(self.mouse-x - self.pressed-x); }`
  (`i-slint-compiler-1.18.1/widgets/fluent/tableview.slint:65-79`).
  `pointer-event(PointerEvent)` with `PointerEventKind.up` /
  `PointerEventButton` works headlessly — chunk 1's card context menu
  rides it (`ui/widgets/task-card.slint:81-87`, journey-pinned); the
  real-pointer identity check is chunk 1's standing Tier-4 item and
  the resize handle inherits it (cosmetic-tier risk only: a missed
  right-button identity cannot affect this chunk — the handle is
  button-agnostic by design, §4 D-2).
- **F-8 Config seams.** `UiSection` = `{scale, keymap}` with
  `#[serde(deny_unknown_fields)]`
  (`crates/taskboard-app/src/config.rs:340-364`); the section-level
  skip-serialization precedent is `WebSection::is_default`
  (`config.rs:92-93`); `uuid` has serde in workspace deps and
  `[kiosk] fixed_board` already stores a `uuid::Uuid`
  (`config.rs:416-427`). The atomic owner-only writer is
  `write_owner_only_atomic` (`config.rs:1162-1186`); the
  comment-preserving overlay writer is connect-crate-private
  `write_config_overlay` (`crates/taskboard-app/src/commands/
  connect.rs:523-588`): read (NotFound → empty) → parse
  `DocumentMut` → mutate a table subtree → atomic write. The write
  target for ceremonies is `AppConfig::write_path(explicit)`
  (`config.rs:762-768`). `tokio` dev-deps in taskboard-app already
  carry `test-util` (hermetic E2E note, its `Cargo.toml`) and
  `#[tokio::test(start_paused = true)]` has a precedent
  (`src/authority.rs:284`).
- **F-9 Testing infrastructure.** Tier-2 journeys run through
  `tests/support` (`FakeEngine::channel`, `SlintDriver`,
  `Guard`); window callbacks are invocable from tests
  (`invoke_<name>` generated methods — the context-menu journey
  drives `board-task-context-requested` that way). Pure-math modules
  with in-file tests + proptests have the `src/tooltip.rs` precedent
  (`src/tooltip.rs:1-60`: no slint types, proptestable).
  No-sleeps rule: debounce tests use
  `#[tokio::test(start_paused = true)]` + `tokio::time::advance`.

## 2. Design D-1 — The widths map in `ViewState` (design D16) and its load path

- **Shape**: `ViewState.stack_widths:
  BTreeMap<BoardId, BTreeMap<StackId, u32>>` — logical px
  (unscaled), absent entry = auto layout (today's rendering). BTreeMap
  for deterministic iteration (snapshot order in `StackWidthsChanged`
  payloads and tests). UI-local forever; never a domain type (DF-2).
- **API** (all `#[must_use]`/pure unless noted):
  - `seed_stack_widths(&mut self, widths:
    BTreeMap<BoardId, BTreeMap<StackId, u32>>)` — called once at
    mount (F-4) with the settings' map.
  - `stack_width(&self, board, stack) -> Option<u32>`.
  - `set_stack_width(&mut self, board, stack, px: u32)` — clamping is
    the caller's duty (the pure math of §4 D-2); the method stores.
  - `stack_widths_of(&self, board) -> Vec<(StackId, u32)>` — the
    full per-board snapshot for `UiRequest::StackWidthsChanged`
    (BTreeMap order).
  - `resize_focused_stack(&mut self, state: &AppState, direction:
      i32, ceiling: u32) -> Vec<FlowOutcome>` — the keyboard verb
    (§4 D-3): resolves target = `focused_stack(state).or
    (selected_stack).or_else(first stack)` (the same resolution
    `create_target_stack` uses for Board view), seeds an absent
    entry at `STACK_WIDTH_DEFAULT_PX`, applies
    `±STACK_WIDTH_STEP_PX`, clamps to `[STACK_WIDTH_MIN_PX,
    ceiling]`, returns `[FlowOutcome::StackWidthsChanged,
    FlowOutcome::RefreshNow]`, or `Vec::new()` when there is no
    stack to address.
- **New `FlowOutcome::StackWidthsChanged`**
  (`src/view_state.rs:52-56` enum gains the variant): the view-state
  → adapter signal "persist the widths now"; the adapter's
  `apply_outcomes` handles it by sending the `UiRequest`
  (§5 D-5). The view state itself never touches a channel.
- **Load path**: `UiSettings` gains
  `stack_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>`
  (serde-native shape, §6 D-7; domain newtypes are deliberately NOT
  given serde impls — that would touch the domain crate). At mount,
  `adapter::install` converts uuid → `BoardId/StackId` via
  `from_uuid` and seeds the fresh `ViewState` before the first
  refresh — widths are applied before the first board render
  (no flash of auto layout to assert either way; the load itself is
  the tested unit). **Seeding keeps unknown boards/stacks** (a
  re-connected board should find its widths again); liveness pruning
  happens once, at write time, app-side (D-6).

## 3. Design D-2 — Mouse resize: geometry, cursor, clamps

- **Pure math module `src/widths.rs`** (the `tooltip.rs` precedent:
  no slint types, in-file tests + proptests):
  - `STACK_WIDTH_MIN_PX: u32 = 96` (design D17's 96–120 band, lower
    bound; card content stays readable and the badge fits).
  - `STACK_WIDTH_DEFAULT_PX: u32 = 280` (the keyboard seed for a
    never-resized column — near the typical auto distribution).
  - `STACK_WIDTH_STEP_PX: u32 = 32` (fixed keyboard step).
  - `clamp_width(requested: i64, min: u32, max: u32) -> u32`
    (saturating).
  - `width_ceiling(window_w: u32, others_min_total: u32,
    chrome_px: u32) -> u32` = `max(STACK_WIDTH_MIN_PX, window_w −
    others_min_total − chrome_px)` — the "remaining-viewport share"
    bound. `others_min_total` = Σ over the *other* live columns of
    their effective minimum (user width if set, else
    `STACK_WIDTH_MIN_PX`); `chrome_px` = the row's fixed furniture
    (spacing `Theme.space-3` per gap + the add-column affordance's
    `Theme.space-6 + Theme.space-2`). The slint-side geometry tokens
    for the handle/furniture stay the markup's; the constants it
    cannot import (`10 px` handle width) are one literal, noted.
  - Proptest invariants: `clamp_width` is monotone in `requested`,
    lands in `[min, max]`, and `width_ceiling ≥ min` whenever
    `window_w ≥ min + chrome` (garbage-input floors are fine).
  - **Units**: all math runs in rendered slint px (winit-logical,
    Theme.base-scale included) — the space `window-w`,
    `wrapper.width`, and drag deltas already share. Conversion to
    stored logical px happens only at the boundary:
    `stored = round(rendered / Theme.base_scale)`,
    `rendered = stored × Theme.base_scale` (the model push of §5).
- **Affordance**: inside the header `Rectangle`
  (`board-layout.slint:108-138`), a right-aligned
  `resize := TouchArea { x: parent.width - self.width; width:
  10px; height: 100%; mouse-cursor: ew-resize; }` — the builtin
  tableview's exact pattern (F-7). Full header height, 10 px hit
  target at the column's right edge (the inter-column `space-3` gap
  visually belongs to it). Button-agnostic: any press starts a drag
  (a dedicated handle cannot conflict with card clicks).
- **Seams** (Rust-authoritative widths; slint never owns a width
  binding during a drag):
  - `pointer-event(event)`: `kind == down` →
    `stack-width-press(column-index, wrapper.width-as-float)`
    (captures the pre-drag base; adapter resolves the stack against
    the tables pass, stores `width_drag: Option<(StackId, f32)>`).
  - `moved`: while pressed →
    `stack-width-drag(column-index, self.mouse-x - self.pressed-x)`
    (the absolute delta since press — idempotent against dropped
    events). The adapter computes
    `clamp_width(base + delta, min_rendered, ceiling_rendered)`,
    writes it via `view_state.set_stack_width`, and refreshes —
    the models push the new width and the column re-renders live.
    Refresh-per-tick is the D1 repaint path at human input scale;
    accepted.
  - `pointer-event`: `kind == up` → `stack-width-release(column-
    index)`: the adapter converts the final rendered width to stored
    px, clears the drag state, and emits
    `UiRequest::StackWidthsChanged { board, widths:
    view_state.stack_widths_of(board) }` via `try_send`
    (drop-on-full, the F-3 convention).
  - Window pass-through: `board-layout` → `board-view` →
    `app-window` gain the three `pure callback`s (the
    `task-context` triple's routing shape,
    `ui/app-window.slint:318-320`), and `adapter::install_view_seams`
    gains `install_stack_width_seams`.
  - A stale column index (tables moved between press and tick) is
    absorbed silently — the `board_target` precedent.
- **The per-column width binding**: the loop child
  (`board-layout.slint:99`) gains conditional layout clamps —
  ```
  min-width:  column.width-px > 0 ? column.width-px * 1px : 0px;
  max-width:  column.width-px > 0 ? column.width-px * 1px : parent.width;
  ```
  (`StackColumnModel` gains `width-px: float`, ≤ 0 = auto — §5).
  With `min == max` the column is exactly that wide; auto columns
  keep the untouched distribution. Never-resized boards push
  `width-px = -1.0` on every column, so the conditional never
  engages — the goldens must pass **unregenerated** (the
  auto-layout regression proof, §7 T-9). `clip: true` on the header
  keeps a shrunken column's content inside its bounds.
- **Cursor probe outcome** (recorded): `ew-resize` cursor ships as
  the primary affordance; no visible grip is added. Real-cursor
  verification joins chunk 1's interactive Tier-4 checklist
  (cosmetic; the drag works regardless).

## 4. Design D-3 — Keyboard resize (design D18, one recorded deviation)

- **Registry rows** (`src/actions.rs`, BoardStack group):
  - `ActionId::StackWider` → `"stack.wider"`, label "Widen stack",
    contexts `&[ContextId::Board]`, `Category::Navigation`,
    `enabled: true`, `menu_group: Some(MenuGroup::BoardStack)`,
    `menu_rank: 100` (StackCreate sits at 90), description stating
    what it does and that the column width is saved per device.
  - `ActionId::StackNarrower` → `"stack.narrower"`, label "Narrow
    stack", same shape, `menu_rank: 110`.
  - The registry-wide `ALL` list in
    `registry_covers_every_action_exactly_once`
    (`src/actions.rs` tests) gains both variants.
- **Router** (`src/router.rs`): `UiEffect::StackWider` /
  `UiEffect::StackNarrower` (pure view intents, the BoardNext
  precedent — never a `DispatchOutcome::Command`); the adapter's
  `interpret` computes the ceiling from `window.get_window_w()` +
  the live column count (state slice + widths map) and routes to
  `view_state.resize_focused_stack` (§2). Compact has no columns:
  the bindings simply do not resolve there (contexts are `Board`-
  binding-context, and the *effect* is inert when the state has no
  stacks; the compact switcher is unaffected).
- **Bindings in all three presets** (design D18's example, made
  concrete; contract tests updated in the same commit):
  - standard: `(StackWider, Board, ["Ctrl+Right"])`,
    `(StackNarrower, Board, ["Ctrl+Left"])` — collision-free (the
    Board context holds only bare keys today).
  - emacs: the same `Ctrl+Right`/`Ctrl+Left` in Board (the design
    names the pair for both presets; the emacs preset already shares
    CUA strokes where no emacs-authentic candidate exists).
  - vi: `(StackWider, ViNormal, ["Ctrl+w", ">"])`,
    `(StackNarrower, ViNormal, ["Ctrl+w", "<"])` — vim's own
    window-resize family; a new `Ctrl+w` prefix sequence works like
    the existing `g`/`d`/`Z` families; `Ctrl+w` alone is unbound, so
    the prefix-shadow pass stays clean.
  - Extend `every_preset_covers_the_core_verbs`'s `core` list with
    both actions and add
    `every_preset_resolves_the_stack_width_verbs_in_the_board_chain`
    (the board-verbs contract's shape, `src/keymap/mod.rs:472-488`).
  - **Registry insta snapshot regenerated in the same commit**
    (`INSTA_UPDATE=always cargo nextest run -p taskboard-ui-slint`,
    noted in the commit message).
- **Recorded deviation (walk)**: D18's parenthetical "headers join
  the focus walk" is **not** implemented. Keyboard resize targets
  the focused/selected stack (§2 resolution); a header click already
  selects its column (`select_column` → `select_stack`), so
  click-header-then-Ctrl+Right is the discovery path. Reason: making
  headers walk stops redefines `nav.next-item`'s domain (D7's
  "the task walk never wraps", the compact switcher-level protocol,
  and a dozen pinned journeys) for marginal gain; the target stack
  is already first-class view state. The header tooltip carries the
  `stack.wider` binding hint (below), which is the design's
  discoverability requirement, met.
- **Header tooltip adopter** (D8/D11/D18): the header TouchArea arms
  the generalized trigger on hover — label = the stack title, hint
  keys = the `stack.wider` chain-visible binding via a new
  `stack-header-hint` projection pushed per refresh
  (`first_binding_display(ActionId::StackWider, chain)`, the
  `board-segment-hint`/`task-card-hint` shape,
  `src/models.rs` push region). No text assertions — structural
  only.

## 5. Design D-4/D-5 — `UiRequest` lane and mount signature (design D21)

- **`UiRequest`** — new UI-crate module `src/requests.rs`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum UiRequest {
      /// Full per-board width snapshot; idempotent (D21).
      StackWidthsChanged { board: BoardId, widths: Vec<(StackId, u32)> },
  }
  ```
  Domain newtypes are fine here (no serde needed — a channel
  payload). Chunk 3 extends this enum; nothing else starts variants
  now.
- **Mount signature** (`src/lib.rs:139`): the parameter list gains
  `requests: mpsc::Sender<UiRequest>` immediately after `commands`.
  `adapter::install` (and `Adapter`) hold the sender; sends use the
  F-3 try_send warn-and-drop convention. `UiHandle` itself does not
  store it (the adapter is the only sender).
- **Mechanical ripple, same commit**: both app call sites
  (`ui.rs:294`, `main.rs:507`), `mount_tests::handover`
  (`src/lib.rs:389-406`), and the journeys harness mount
  (`tests/journeys.rs:45-52`) gain a
  `(requests_tx, requests_rx)` pair; tests drain `requests_rx`
  directly (the FakeEngine pattern, minus the engine).
- **Compact / stack-view**: `ui/views/stack-view.slint` and the
  compact board leg need **no changes** — there are no columns to
  resize; reviewed and recorded as a no-op (the dispatch prompt's
  "stack-view.slint (compact)" read confirmed it).

## 6. Design D-6 — The app-side forwarder: debounce, single owner, merge+prune

New app-crate module `crates/taskboard-app/src/ui_requests.rs`
(the design inventory's "src/ui_requests.rs (or similar)"; SPDX
`MIT OR Apache-2.0`? — no: **app crate is `AGPL-3.0-only`** per
AGENTS §6, same as every file in that crate today).

- **Channel**: `ui_request_channel()` at capacity 16 (human-input
  scale, the `UI_COMMAND_CAPACITY` rationale).
- **Forwarder**:
  ```rust
  pub fn spawn_ui_request_forwarder(
      requests: mpsc::Receiver<UiRequest>,
      config_write_path: PathBuf,
      state: Arc<ArcSwap<AppState>>,
  ) -> JoinHandle<()>
  ```
  Loop (all awaits inside; nothing held across them — R6):
  1. `requests.recv().await` — merge the payload into
     `pending: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>`
     (a board's snapshot **replaces** that board's entry — full-
     snapshot semantics, idempotent under re-send).
  2. Debounce quiet window
     `UI_REQUEST_DEBOUNCE: Duration = 500 ms`: loop
     `tokio::time::timeout(UI_REQUEST_DEBOUNCE, requests.recv())` —
     `Ok(Some(req))` merges and restarts the window; `Ok(None)`
     (channel closed) and `Err(elapsed)` both fall through to the
     flush. Deterministic under
     `#[tokio::test(start_paused = true)]` + `advance` — no sleeps.
  3. **Flush**: prune (below) → merge into the on-disk TOML →
     write. Every error is `tracing::warn!`ed and swallowed — the
     UI's widths live on in `ViewState`, the next event retries;
     the forwarder never panics and never blocks the UI (§8 D-8).
  4. On channel close: one final flush, then log-and-exit (the
     command forwarder's exit shape, `ui.rs:74-80`) — a resize sent
     right before quit still lands.
- **Prune** (pure, unit-tested):
  ```rust
  fn prune_widths(pending, state: &AppState) -> pending'
  ```
  keep a board iff it exists and `!deleted && !archived`; keep a
  stack iff it exists in `state.stacks`, `!deleted`, and
  `stack.board == board` — the `normalize_board`/`stacks_in_board`
  liveness rules, read-only over `state.load()`
  (`app.engine.shared_state()` at spawn; an `ArcSwap` load is
  synchronous and lock-free — **no lock exists to hold across an
  await**, which is the R6 point made structural).
- **Overlay write** (merge + prune in one step):
  ```rust
  fn overlay_stack_widths(path: &Path,
      widths: &BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>)
      -> std::io::Result<()>
  ```
  read → parse `DocumentMut` → **rebuild only the
  `[ui.stack_widths]` subtree** from the pruned map (deterministic
  BTreeMap order; an empty pruned map **removes** the subtree
  instead — no empty husk, and no write at all when nothing was
  ever received) → `write_owner_only_atomic`. Comments, formatting,
  and every other table are `toml_edit`-preserved by construction;
  comments *inside* the `stack_widths` subtree are machine
  territory and are intentionally not preserved (recorded).
  - **Extraction for reuse** ("reuse, do not duplicate"): the
    read(NotFound→empty)→parse skeleton moves to
    `config.rs` as `pub(crate) fn overlay_document(path) ->
    std::io::Result<toml_edit::DocumentMut>` next to
    `write_owner_only_atomic`; connect.rs's `write_config_overlay`
    is rewritten over it (behavior byte-stable — its six tests at
    `connect.rs:718-1025` are the gate and must pass unchanged);
    the widths writer uses the same skeleton. Table mutation stays
    in each writer (they own disjoint subtrees).
- **Single-owner rule (R6, recorded)**: stack-width overlay writes
  are owned by exactly this forwarder task — no other code path
  writes `[ui.stack_widths]`. The REPL ceremonies keep their own
  writer (their subtree is `[nextcloud]`/`[sync]`; the login/logout
  E2E suites stay the gate). The two writers are serialized within
  themselves, read-merge-write over disjoint table subtrees with an
  atomic rename. Residual risk accepted: two *interleaved* writers
  (a width flush racing a `connect` ceremony) can still lose one
  side's keys in a last-writer-wins on the whole file; the window
  is milliseconds on a single-user desktop, no lock is available
  without violating "never hold a lock across `.await`", and the
  design round accepted the two-writer shape explicitly (D21, R6).
- **Config-path plumbing**: the forwarder needs the ceremony write
  target. `main.rs` already holds the explicit `--config` value;
  it computes `let config_write_path =
  AppConfig::write_path(explicit_config_path);` once and passes it
  into `ui::run_desktop(config, config_write_path)` and the kiosk
  appliance's mount block (both app-internal lifecycles; thin
  main-glue, the untested-glue convention applies to the plumbing
  itself, the forwarder carries the tests). `write_path`'s existing
  first-run rules decide where the file lands when no config exists
  — the same place `login` writes.

## 7. Design D-7 — Config serde shape (`UiSection.stack_widths`, design D20)

```rust
/// Device-local stack column widths (round-1 D20): board uuid →
/// stack uuid → width in logical px. Never synced (no domain key);
/// pruned by the UI request forwarder at write time. Skipped when
/// empty so width-less configs serialize unchanged (the `web`
/// precedent).
#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
pub stack_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
```
on `UiSection` (`config.rs:340-364`), which keeps
`#[serde(deny_unknown_fields)]`. File shape:
```toml
[ui.stack_widths."b5a2…-…"]
"9cf1…-…" = 240
"1d0c…-…" = 96
```
- **Defaults**: serde `default` → absent key = empty map; old
  configs parse unchanged (acceptance item, T-11).
- **deny_unknown_fields forward-compat note (recorded)**: the key
  is additive, so *new reads old* works; *old reads new* fails
  decode with the standard unknown-field error — the accepted D20
  posture ("additive-strict"): a downgraded binary refuses the
  config loudly rather than silently ignoring user state; the
  section stays secret-free by construction (typed uuid/u32 keys
  cannot smuggle strings).
- **Value validation**: structural only at the config layer
  (uuid keys and `u32` values are the type system's check). Range
  clamping lives at apply time in the UI (`clamp_width` with the
  live ceiling) — a stale config naming a shrunken monitor cannot
  block boot, and the scale field's fail-fast precedent is *not*
  extended to per-entry values (a single corrupt entry failing the
  whole boot is worse than a clamp; recorded reason for the
  asymmetry).
- `resolve_ui_settings` (`ui.rs:186-215`) copies the map into
  `UiSettings.stack_widths`; `UiSettings::default()` gains the
  empty map. Existing test literals that name `UiSection { scale,
  keymap }` fields exhaustively (`ui.rs:482-504`) gain
  `..Default::default()` or the new field — mechanical, same
  commit.

## 8. Design D-8 — Failure story (unwritable config path)

- Read fails (permissions, path is a directory, corrupt TOML) or
  write fails: `tracing::warn!` with the typed io/parse error,
  **no propagation** — the forwarder task survives and the next
  received request retries the whole read-merge-write.
- The UI never learns of the failure: `ViewState` keeps the widths,
  the session behaves normally, and a restart falls back to auto
  layout (or the last successfully written widths). The UI crate
  cannot crash on this path — it only ever `try_send`s.
- Nothing ever writes a partial file: `write_owner_only_atomic`'s
  temp+rename guarantees the on-disk config is either the old or
  the new full document (F-8).
- The never-crash rule is testable: the forwarder test asserts the
  task outlives a failing write and completes a later good one
  (T-15).

## 9. Recorded deviations from the design round (all with reasons)

1. **Header elide stays unconditional** (D19 wanted "elide only
   under user-set width"): chunk 1 already landed unconditional
   elide for the window-floor requirement (D15; F-1), and
   re-conditionalizing it would regress that acceptance. Chunk 2's
   auto-layout guarantee therefore rides **only** on the width
   binding (never engaged without a map entry), proven by the
   unregenerated goldens + the auto-board journey (T-9/T-10).
   The in-file comment at `board-layout.slint:126-131` is updated
   to name this plan as the resolution.
2. **Headers do not join the focus walk** (D18's parenthetical):
   keyboard resize targets the focused/selected stack instead
   (§4 D-3) — the walk's invariants (D7/D8) and the pinned journeys
   stay untouched; the header tooltip carries the binding hint.
3. **Drag persists on release only; keyboard persists per press**
   (both then debounced app-side): D17's "release persists" is
   kept literally for the mouse; for the keyboard there is no
   release concept, so each step sends a snapshot that the 500 ms
   debounce coalesces. Full per-board snapshots make either
   ordering idempotent (D21).
4. **`UiSettings.stack_widths` carries `uuid::Uuid` keys, not
   domain newtypes**: giving `BoardId/StackId` serde impls would
   break the empty-diff guarantee over `taskboard-domain`; the
   conversion happens once at `adapter::install` (§2).
5. **Keyboard seed width**: a keyboard resize of a never-resized
   column starts from `STACK_WIDTH_DEFAULT_PX` (280) rather than
   the column's actual auto width — the auto width is layout-
   derived and unknown to the pure view state; the mouse path
   *does* capture the real width at press (§3). Recorded as the
   price of keeping widths out of slint query-backs; the jump is
   visible only at the first keyboard resize of a column.

## 10. Test catalogue (TDD order; AGENTS §5 / design §10 / F-9)

Tier-1 pure unit (no slint types) unless marked Tier-2 (journey)
or manual/Tier-4:

| # | Test | Home | Pins |
|---|---|---|---|
| T-1 | `clamp_width`: monotone, `[min, max]`-bounds, saturation (2 proptests + edge units) | `src/widths.rs` | the clamp math (review trap e) |
| T-2 | `width_ceiling`: subtracts others+chrome, floors at min | `src/widths.rs` | viewport share bound |
| T-3 | `resize_focused_stack`: seeds DEFAULT±step on auto; clamps at min/ceiling; emits `[StackWidthsChanged, RefreshNow]`; inert without stacks; targets focused→selected→first | `src/view_state.rs` tests | keyboard semantics (T-3 is the layer rule's lowest home) |
| T-4 | `seed_stack_widths` + `stack_widths_of`: load-back and snapshot order (BTreeMap) | `src/view_state.rs` tests | the restart-apply half |
| T-5 | `push` carries `width-px`: scaled user width on set columns, −1.0 on unset | `src/models.rs` tests | the model sentinel |
| T-6 | registry: two new rows enabled, Board-context, BoardStack group; `ALL` extended; menu BoardStack page contains them after StackCreate | `src/actions.rs` + `src/menu.rs` tests | D18 registry shape |
| T-7 | every preset resolves both verbs in `[Board, ViNormal, Global]`; core-verb list extended; conflict/shadow passes stay green | `src/keymap/mod.rs` tests | bindings in all three presets |
| T-8 | registry insta snapshot regenerated **in the same commit as T-6/T-7's registry change** | `tests/snapshots/keymap_snapshots__registry.snap` | the documentation dump |
| T-9 | goldens pass **unregenerated** (no commit touches `tests/goldens/`) | `tests/golden_visuals.rs` | never-resized boards render byte-identically (auto-layout regression trap b) |
| T-10 | journey `keyboard_resize_steps_clamps_and_requests_persistence`: `press(StackWider/Narrower)` → model `width-px` moves by the scaled step; narrower clamps at min; the requests drain captured the exact typed `StackWidthsChanged{board, widths}` snapshot | `tests/journeys.rs` | keyboard resize end-to-end UI-side |
| T-11 | journey `drag_seam_resizes_live_and_persists_on_release`: `invoke_stack_width_press/drag/release` → live model width, stale index absorbed, exactly one request on release | `tests/journeys.rs` | mouse resize (real-pointer feel is Tier-4) |
| T-12 | journey `never_resized_board_pushes_auto_widths_and_no_requests`: fresh fixture → all `width-px ≤ 0`, requests drain empty after refreshes | `tests/journeys.rs` | auto boards unchanged |
| T-13 | lib test `mount_applies_seeded_widths_before_first_refresh`: `UiSettings` with widths → first pushed model carries them; requests drain empty | `src/lib.rs` mount_tests | the load path (review trap f: a test, not folklore) |
| T-14 | config: stack_widths round-trip (nested table decode/encode); config without the key → empty (old configs parse); unknown `[ui]` key still rejected; default section serializes without the key | `crates/taskboard-app/src/config.rs` tests | serde shape + forward-compat note |
| T-15 | `overlay_stack_widths`: hand-written config with comments + `[nextcloud]` + existing widths → merge preserves comments and unrelated keys byte-for-byte, replaces only the subtree; empty map removes the subtree; failing path (dir-as-file) errors typed without panicking | `crates/taskboard-app/src/ui_requests.rs` tests | overlay merge (review trap c) |
| T-16 | `prune_widths`: dead board/stack dropped, live kept, archived board dropped (AppState fixture) | `src/ui_requests.rs` tests | stale-id pruning cannot eat live entries |
| T-17 | forwarder debounce under `#[tokio::test(start_paused = true)]`: two snapshots for one board + `advance(600 ms)` → exactly one write carrying the merged latest; a late message inside the window extends it; channel close flushes pending before exit; a failing path logs and the task completes a later retry (unwrapped channel, join-based asserts — no sleeps) | `src/ui_requests.rs` tests | debounce determinism (trap d) + failure story |
| T-18 | restart round-trip at the app layer: write widths via the overlay → `AppConfig::load` → `resolve_ui_settings` → `UiSettings.stack_widths` equals the written map (the UI apply half is T-13) | `src/ui_requests.rs` or `src/ui.rs` tests | write → reload → applied |
| T-19 | connect.rs overlay tests green **unchanged** after the `overlay_document` extraction | existing `connect.rs` tests | extraction did not rewrite behavior |
| T-20 | cross-crate guard: `git diff main...HEAD -- crates/taskboard-domain crates/taskboard-state crates/taskboard-storage-sqlite crates/taskboard-sync-nextcloud` is empty | stage-2/3 checklist | the scope guard |

Tier-4 (manual, recorded in the PR body): real-pointer drag feel +
`ew-resize` cursor rendering (chunk-1's interactive checklist
inherits both rows).

**No text assertions anywhere** (R5): widths are typed/structural;
tooltips are asserted by existence/hint-model shape only; a11y
labels stay addresses.

## 11. Commit slicing (TDD-first; every commit standalone-compiling and gate-green-capable)

1. `feat(ui-slint): stack width clamp math` — `src/widths.rs` +
   tests (T-1/T-2).
2. `feat(ui-slint): stack widths in the view state` — ViewState
   map, seed/resize API, `FlowOutcome::StackWidthsChanged` +
   tests (T-3/T-4).
3. `feat(ui-slint): stack.wider and stack.narrower actions` —
   registry rows, router arms, presets bindings, keymap contracts,
   **snapshot regen in this commit** (T-6/T-7/T-8).
4. `feat(ui-slint): column width push, drag handle, header hint` —
   models sentinel (T-5), slint seams + conditional min/max +
   handle + header tooltip, adapter seams, journeys T-10–T-12;
   goldens must pass untouched (T-9).
5. `feat(ui): the UiRequest mount lane` — `src/requests.rs`, mount
   signature, adapter sender, both app call sites + harnesses
   (T-13 rides here or in 4 if the lane lands first — plan order:
   here).
6. `feat(app): ui.stack_widths config section` — serde + tests
   (T-14), `resolve_ui_settings` extension.
7. `feat(app): debounced stack-widths forwarder` —
   `overlay_document` extraction + connect rewrite-over (T-19),
   `ui_requests.rs` (T-15/T-16/T-17/T-18), lifecycle glue
   (`config_write_path` plumbing, both mount-site spawns).

## 12. Risks

- **R6 (overlay writer contention)** — the acceptance posture is
  §6's single-owner rule; the residual whole-file last-writer-wins
  race against REPL ceremonies is recorded there with its
  mitigation boundary. Stage 3 must verify no second writer of
  `[ui.stack_widths]` exists and that the forwarder holds no lock
  (it holds none — it holds only owned maps) across `.await`.
- **Auto-layout regression** — the only structural risk is the
  conditional `min/max-width` binding on the loop child. The gate
  is T-9's unregenerated goldens plus T-12; if they move, the
  binding mechanism is wrong and must be fixed — regenerating
  goldens to absorb this chunk's own layout drift is forbidden
  without a recorded reason (it would mask exactly the regression
  the acceptance names).
- **Mount-signature ripple** — four call sites, all mechanical;
  landing them in one commit (§11 commit 5) keeps every commit
  standalone.
- **Drag-refresh cadence** — a full models push per mouse tick;
  human-input scale on kiosk-sized boards, the established D1
  path. If stage 2 observes jank, the recorded fallback is
  refreshing only the affected column's model — not planned now
  (no speculative generality).
- **R5 (text assertions)** — catalogue is structural/typed
  throughout; stage 3 rejects any label-text assertion.

## 13. Estimated diff size

≈ +900 lines net (UI crate ≈ +480 incl. ~240 test lines; app crate
≈ +420 incl. ~200 test lines; goldens/snapshots binary/yaml except
the one intentional registry-snapshot regen). Files: UI crate 10
sources + 2 slint + 2 test suites; app crate 2 sources + 2 test
homes; connect.rs touched only by the extraction (commit 7).

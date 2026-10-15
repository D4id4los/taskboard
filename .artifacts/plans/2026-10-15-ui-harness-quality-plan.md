# [Plan] UI Test Harness & Quality Gates (GUI Phase 7, Chunk 4)

- **Date**: 2026-10-15
- **Kind**: stage-1 chunk plan (TDD-first implementation spec) for
  `feat/ui-harness-quality` — planning only; no production code in this
  commit.
- **Origin** (normative unless marked): accepted design plan
  `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md` — §7.3 journey
  bar, §8.1–8.2 harness seam, §9.2 keypress guard, §10 tier table, §12
  row 4, §14 risks (journey brittleness, internal-crate churn), §16
  decisions Q5, Q8, Q17, Q19; stress-test report
  `.artifacts/reports/2026-10-14-slint-gui-stress-test-report.md` §3.5
  (testing-backend inventory) and §5 (pin shape); dispatch plan
  `.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  §1/§5, incl. **§1.4 item 5: the `i-slint-backend-testing` dev-pin lands
  in THIS chunk** (the recorded micro-reshaping — chunk 4 asserts pin
  equality with the slint pin).
- **Precondition verified 2026-10-15**: chunks 1–3 are on `main`
  (`56597fc` == `origin/main`; the shell/theme, action-keymap, and
  views/chrome commits all precede it in `main`'s history). The driven
  stack exists: `mount()` → `UiHandle` in
  `crates/taskboard-ui-slint/src/lib.rs`, the root-FocusScope adapter
  (`src/adapter.rs`), the shell/resolver/router/view-state machine, the
  three views + status-bar/banner markup with a11y roles/labels, the
  command `mpsc` boundary, and the app-side forwarder
  (`crates/taskboard-app/src/ui.rs`).
- **Branch / worktree**: `feat/ui-harness-quality` in
  `../taskboard-WT-feat-ui-harness-quality`, off `main` @ `56597fc`.
- **Status**: PROPOSED (this document) — implementation follows per the
  three-stage dispatch protocol.

---

## 0. Scope fence (what this chunk does and does not build)

Builds (design §12 row 4 + dispatch §5):

1. The `i-slint-backend-testing = "=1.18.1"` dev-dependency (exact pin,
   renderer feature for headless snapshots), isolated behind a
   journey-driver module (design §14 churn-isolation).
2. The logical harness: headless journeys with mock time, accessible
   role/label addressing, key injection through the root focus path,
   typed command capture at the forwarder boundary (design §8.1).
3. The full keyboard-journey suite: the ten §7.3 steps as one
   continuous session, plus the cold-start view-graph walk (§8.1
   scriptability rules).
4. The keypress-count guard (§9.2/Q17): counting physical presses from
   the journeys, failing on ceiling exceedance, passing on improvement.
5. The golden tier (§10): board view at scale extremes × breakpoints,
   software renderer, bundled fonts, perceptual-hash tolerance,
   renderer-presence assertion, explicit regeneration procedure.
6. The MCP dev-feature wiring + one CI smoke job + the release-profile
   feature-off assertion (§8.2/Q19).

Does **not** build (dispatch §5 non-goals): no new views or actions; no
driver tooling (the backlog entry "[2026-10-14] LLM/MCP UI-debugging
driver tooling" owns the POST-1.0 wrapper — untouched); no external
compositor capture; no `slint::testing` (does not exist); no Slint
version bump; no ADR/docs amendments (chunk 5 owns
testing_strategy.org's tier table).

**Verified framework facts this plan rests on** (Slint 1.18.1 registry
sources; the stress report's §3.5 inventory, refined where the letter
differed — deviations recorded in §9):

- Public testing-crate surface: `init_no_event_loop()` (mock time, no
  event loop — threading: false), `init_integration_test_with_mock_time()`
  (one-per-process, event loop), `mock_elapsed_time(Duration)`,
  `get_mocked_time()`, the `search_api` (ElementQuery builder:
  `match_accessible_role`, `find_by_accessible_label`, `find_first/
  find_all`; `ElementHandle`: `accessible_label/value/checked/enabled`,
  `accessible_item_selected/item_index`, `invoke_accessible_default_action`,
  `set_accessible_value`, `mock_single_click`, `scroll`), and
  `TestingBackend`/`TestingBackendOptions` (exported unconditionally;
  `renderer_name: Some("software")` embeds a real rasterizer per window
  so `Window::take_snapshot()` works headlessly —
  `testing_backend.rs:160-260`, `create_headless_renderer`).
- The stress report's named injection helpers
  (`send_keyboard_string_sequence` etc.) are **feature-gated**
  (`internal` = Slint-own tests, `ffi` = C++ bindings;
  `testing_backend.rs:81-150`). The public injection path is
  `slint::Window::dispatch_event(slint::platform::WindowEvent::…)` —
  exactly what those helpers call internally, and what this repo's
  chunk 1–3 component tests already exercise. Decision D3 adopts that
  path; the off-label `ffi` feature stays off.
- Modifier-carrying strokes **are** injectable on any backend:
  `WindowEvent::KeyPressed { text: Key::Control.into() }` updates the
  window's tracked modifier state (`i-slint-core/window.rs:1268-1273`)
  and the next character key event carries the modifiers
  (`window.rs:1293-1294`). The chunk-3 note "chords cannot be injected
  modifier-carrying on this backend" described the test helper, not the
  backend — the driver does the modifier dance (D4). This unlocks the
  standard preset's `Ctrl+Q`/`Ctrl+Shift+M` in journeys.
- MCP: the `slint/mcp` feature; the server starts during backend
  selection when `SLINT_MCP_PORT` is set (localhost, origin-validated
  HTTP JSON-RPC; tools include `list_windows`, `get_element_tree`,
  `dispatch_key_event`, `set_element_value`, `take_screenshot` → base64
  PNG; `SLINT_EMIT_DEBUG_INFO=1` gates element ids/type names).
  `SLINT_BACKEND=headless-software` boots the **app itself** on the
  TestingBackend with a real software renderer
  (`i-slint-backend-selector/lib.rs:39-46,130-131`) — no display, no
  GPU, no xvfb.
- `slint::Window::set_size(impl Into<WindowSize>)` is public → drives
  breakpoint widths for goldens; the TestingWindow defaults to 800×600.

---

## 1. Where each tier lives (D1)

| Tier (design §10) | Location | Runner |
|---|---|---|
| Unit (pure Rust) — existing | `src/**` `#[cfg(test)]` (unchanged; NullPlatform lib tests stay) | nextest/`cargo test` |
| Component / journeys | `crates/taskboard-ui-slint/tests/journeys.rs` + `tests/support/` (driver, fake engine, fixture, guard) | nextest (one test binary; process-per-test keeps the one-platform-per-process rule under nextest) |
| Golden visuals | `crates/taskboard-ui-slint/tests/golden_visuals.rs` + `tests/support/golden_compare.rs` + `tests/goldens/*.png` | nextest; regeneration via env var (D8) |
| Pin guard | `crates/taskboard-ui-slint/tests/pin_guard.rs` (asserts the two Slint pins move together) | nextest |
| Harness smoke (MCP) | `crates/taskboard-app/tests/mcp_smoke.rs` (`#[ignore]`d — CI invokes it explicitly) | dedicated CI job (D9) |
| Feature-off assertion | CI steps in `ci.yml` + `release.yml` | non-Rust (cargo tree grep, D10) |

**D1 — placement.** Journeys/goldens live in the UI crate's
integration-test dir (`tests/`), not a new crate: they drive
`taskboard_ui_slint::mount` and need `pub` surface only; a harness crate
would add a workspace member with no production value. The support
modules are `tests/support/*.rs` declared as `mod support;` by each
test binary — compiled per binary, invisible to the shipped library.
The MCP smoke lives in `taskboard-app` because only the app binary
boots the real wiring (backend selector + forwarder + core); it uses
`env!("CARGO_BIN_EXE_taskboard")` to spawn itself with the feature
compiled in (features unify across the `cargo test` invocation that
requests them).

## 2. The dev-pin landing (D2)

In the root `Cargo.toml` `[workspace.dependencies]`, beside the existing
`slint = "1.18.1"` / `slint-build = "1.18.1"` comments:

```toml
# Dev-only testing harness for the UI crate (design §8.1/Q8; internal,
# non-semver — exact pin mandatory, moves with the slint pin on every
# bump). renderer-software is the headless rasterizer behind golden
# snapshots and the MCP screenshot smoke (same crate version the slint
# default features already put in the tree — no new vendors).
i-slint-backend-testing = { version = "=1.18.1", features = ["renderer-software"] }
```

and in `crates/taskboard-ui-slint/Cargo.toml`
`[dev-dependencies]`: `i-slint-backend-testing.workspace = true`.

- `renderer-software` is required for the `supports_headless` cfg that
  gates `TestingBackendOptions::renderer_name` (build.rs cfg_aliases);
  it reuses the same `i-slint-renderer-software` already compiled for
  slint's default features — no new vendored crates, `cargo deny`
  license picture unchanged in kind (the testing crate carries the same
  Slint tri-license as the runtime; extend chunk 1's deny configuration
  entry from `slint` to also name `i-slint-backend-testing` if it is
  listed explicitly rather than by license expression).
- `renderer-skia` stays off (Q10: no skia anywhere).
- **T1 (pin guard)** — `tests/pin_guard.rs`: parse
  `include_str!("../../../Cargo.toml")`, extract the `slint` and
  `i-slint-backend-testing` version reqs, assert version equality (the
  structural fact of Q12's move-together rule; a structural
  manifest assert, not a text assertion on program output). Fails the
  build the day someone bumps one pin without the other.

**D2 — pin mechanics recorded**: exact `=1.18.1`, taken as a
`[workspace.dependencies]` entry with the feature, consumed by the UI
crate's dev-dependencies only (never a normal dependency — the shipped
feature set gains nothing).

## 3. The journey-driver module (D3) — the churn-isolation file

`tests/support/driver.rs` is the **only** file that names
`i_slint_backend_testing`. Everything the journeys need from the
non-semver crate (and from the slint platform APIs used for injection)
sits behind one small trait of our own vocabulary, so a Slint bump
edits one file (design §14):

```rust
// tests/support/driver.rs — the whole file is the seam (design §14).
pub trait JourneyDriver {
    /// Boots the platform: TestingBackend, mock time, no event loop.
    fn init(&mut self);
    /// Advances the mock clock (fires timers incl. the D1 repaint
    /// driver, repeaters, change handlers) — the only clock the
    /// harness ever advances.
    fn advance_time(&mut self, ms: u64);
    /// Injects one stroke through the real key path: modifier-only
    /// KeyPressed events (Key::Control/Shift/Alt/Meta) as needed, then
    /// the character/key event, then modifier releases. Returns the
    /// count of physical presses contributed (always 1 — a stroke is
    /// one press; held modifiers are not presses, §9.2 accounting).
    fn press_stroke(&mut self, stroke: &Stroke) -> u32;
    /// Sets the window's size (physical px; scale factor 1) — drives
    /// the Responsive breakpoint for compact-mode legs and goldens.
    fn set_window_size(&mut self, width: u32, height: u32);
    /// Renders and captures the window (software renderer) as RGBA.
    fn snapshot(&mut self) -> RgbaImage;
    // Accessible-addressing wrappers (ElementQuery under the hood):
    fn find_by_label(&self, label: &str) -> Option<Element>;
    fn find_by_role(&self, role: Role) -> Vec<Element>;
}
pub struct Element { /* label/value/selected/enabled reads, click */ }
```

- `SlintTestingDriver` implements it: `init()` calls
  `i_slint_backend_testing::init_no_event_loop()` **once per process on
  the dedicated slint thread** (the `on_slint_thread` pattern from
  `src/test_platform.rs`, generalised into the support module: a
  long-lived worker thread owns platform + font context; jobs are sent
  over a channel; under nextest's process-per-test each test gets a
  fresh process, under `cargo test`/llvm-cov threads all jobs serialise
  onto the worker). The lib's NullPlatform tests are a different binary
  and stay untouched.
- Mock time (`init_no_event_loop` → `mock_time: true`) means the
  adapter's repaint `Timer` never races wall clock: it fires only when
  the harness calls `advance_time` past `REPAINT_CADENCE_MS` — AGENTS
  §5's no-sleeps rule holds by construction (design §8.1).
- `press_stroke` dispatches `WindowEvent::KeyPressed/KeyReleased` on
  `AppWindow::window()` — the same delivery path (window → focused item
  → root FocusScope capture) real backends use, and the one the gated
  helpers use internally. Shifted printables are dispatched as their
  uppercase text (the established repo convention; `to_stroke`
  normalises case → shift).
- `Element` wraps `ElementHandle` and exposes only the a11y reads the
  journeys assert through (label, value, checked, item-selected) plus
  `click()` (`mock_single_click`) — journeys never touch item-tree
  internals (design §14 journey-brittleness).

**D3 — driver shape**: trait + one impl, injection via the public
`WindowEvent` dispatch with the modifier-state dance, `ffi`/`internal`
features off. *Deviation note (letter vs stress §3.5 inventory)*: the
report listed `send_keyboard_string_sequence`/`send_keyboard_char` as
available; in 1.18.1 they are `internal`/`ffi`-gated. The design's
normative wording ("inject keys through the same root focus path real
users hit", §8.1) is satisfied exactly by the public dispatch path — no
design deviation, only a recorded correction of the inventory's
availability detail.

## 4. The fake engine at the forwarder boundary (D4)

`tests/support/fake_engine.rs`:

```rust
/// The forwarder-boundary fake (design §8.1: a test fake stands in for
/// the engine; AGENTS §5 fakes-over-mocks). Owns the receiver half of
/// the UI command channel — exactly where `spawn_ui_command_forwarder`
/// would sit in the app — plus the ArcSwap it feeds.
pub struct FakeEngine {
    commands: mpsc::Receiver<StateCommand>,
    state: Arc<ArcSwap<AppState>>,
    received: Vec<StateCommand>,
}
impl FakeEngine {
    /// Drains every command that reached the boundary (typed capture).
    pub fn drain(&mut self) -> Vec<StateCommand>; // appends to `received`
    /// Applies drained commands to the state snapshot for journey
    /// continuity (insert/create, mark done, move, delete, retitle …),
    /// then stores + triggers the UI's deterministic refresh.
    pub fn settle(&mut self, ui: &UiHandle);
    pub fn received(&self) -> &[StateCommand];
}
```

- **Assertions** are made on `received()` (typed `StateCommand` values,
  exact variants — the routing-only rule: journeys assert *that* the
  right command reached the boundary, never engine semantics, AGENTS
  §5 "higher layers test routing only").
- **`settle`'s application is fixture continuity, not an oracle**: the
  simplest structural change that makes the next journey step's
  precondition true (e.g. `CreateTask` inserts a `Task` with the given
  title at the given order; `SetTaskDone` flips `done`; `MoveTask`
  moves; `DeleteTask` removes). It is explicitly **not** tested and not
  trusted — the engine's own insta suite owns command semantics. A
  comment in the file states this contract.
- The harness mounts via the public `mount()` with the fake's channel
  halves + broadcast receivers (dropped senders, as the lib tests do) —
  no engine, no app crate dependency from the UI tests.

**D4 — the boundary choice**: the mpsc receiver the app's forwarder
would drain. The app-side `CommandSink` fake (already tested in
`taskboard-app`) is *not* re-tested here — one behaviour, one test,
lowest layer.

## 5. The journey suite (D5) — `tests/journeys.rs`

One support-side `Harness` struct composes driver + fake + fixture:
`press_action(ActionId)` reads the visible context chain (current view
enum, `KeyboardState` globals — menu open, vi insert) exactly as the
adapter's `shell.chain()` would, resolves the action's binding strokes
through the same lookup the generated menu uses (`menu::entries`' hint
path — reuse, not reimplement), and injects each stroke through the
driver, accumulating the per-verb press counts for the guard (§6).

### 5.1 The continuous session (one `#[test]`, the ten §7.3 steps)

Fixture: one board, two stacks ("a" with 2 tasks incl. one overdue,
"b" with 1 task), sync phase `Offline` with `pending_ops = 2` —
`fixture.rs` reuses the §6 component-test fixture shape (extracted to
`tests/support/fixture.rs`; the lib's private copy stays — duplication
across test binaries is the cargo reality, noted here so the review
does not flag it).

| Step | Drives (standard preset) | Terminal-state + focus assertions (typed; labels only as addresses) |
|---|---|---|
| 1 launch | `mount` + first `refresh` | `current_view == Board`; board model: 2 columns, 2+1 cards; first task `emphasis == true` (focus ownership at the model seam, `models.rs:333`); accessible query finds the "Board" region |
| 2 navigate | `nav.next-item` ×2 (Down), `nav.prev-item` (Up); `nav.next-view` ×3 (Ctrl+PageDown) → StackDetail → TaskDetail → wrap to Board; `nav.prev-view` back | after each item move: the `emphasis` bit moved to the expected card (via the id tables' ordering); after each view move: `current_view` equals the expected `View` enum value |
| 3 create | `task.create` (N) → type `"x"` → Return | create row open (`create_open`) then closed; `CreateTask { title: "x", stack: <focused stack>, order }` captured verbatim; after `settle`: 3 cards in the column |
| 4 edit | focus the created task; `task.edit` (Return) → view TaskDetail → type `"y"` → Return | `current_view == TaskDetail` (focus ownership); the title-edit `StateCommand` (exact variant per `view_state::commit_edit`) captured; after `settle` the task-model title reflects it |
| 5 complete | `task.complete` (Space) | `SetTaskDone { id, done: true }` captured |
| 6 move | `task.move` (M) → Down (retarget) → Return (confirm) | flow hint visible mid-flow; `MoveTask { id, stack: <second stack>, order }` captured |
| 7 delete | `task.delete` (Delete) | `DeleteTask { id }` captured; no confirmation step existed (assert: no further command before the next action); after `settle` + refresh the view reconciles off the deleted subject |
| 8 menu | `app.menu` (Ctrl+Shift+M) → `menu.next` (Down) → `menu.invoke` (Return) on a task-verb entry | menu open (`menu_open`), cursor index advanced, menu closed after invoke; the invoked entry's command captured (the re-dispatch path) |
| 9 sync status | read the status bar's sync badge | accessible query finds the sync status element; its state (label/value/`get_sync_model`) matches the fixture's `Offline` + pending 2 — a structural read, no message-text matching beyond the label-as-address rule |
| 10 exit | `app.exit` (Ctrl+Q) | `UiHandle::quit_requests() == 1` (the typed exit outcome — see D6) |

Between steps the harness `drain()`s and asserts **nothing else
arrived** (the "nothing else dispatched" convention of the existing
component tests).

### 5.2 The cold-start view-graph walk (§8.1 scriptability)

A second `#[test]`: from a **cold mount** (fresh process under nextest),
walk the full graph by action only — Board → (activate) StackDetail →
TaskDetail → `nav.next-view` wrap → Board; from every view:
`nav.focus-board` (Alt+1) returns; menu opens and dismisses (Escape);
the edit flow's Escape returns to a stable state (no dead ends); then
**one compact leg**: `set_window_size(600, 800)` → the board's stack
switcher is keyboard-navigable (`nav.next-item` walks it) — the §6.4
"keyboard-complete at every size" claim gets its one journey proof.
Asserts are `current_view`/model-structure/emphasis reads only.

**D5 — suite structure**: the ten steps are **one** `#[test]`
(§7.3's "one continuous session" is the contract; per-step asserts
inside with step-labelled panics), the walk is a second, both in
`tests/journeys.rs`. If a step exposes a real production bug (the
likely candidate: step 4's title-field reachability under the standard
preset — see risks R1), the production code is fixed on the branch with
its own commit; the journey is never weakened (AGENTS §5).

**D6 — the quit seam**: `adapter.quit_requests` is `pub(crate)`; the
journeys are a separate binary. Add a one-method public read accessor
`UiHandle::quit_requests(&self) -> usize` (lib.rs impl block beside
`refresh`). Recorded reason: §7.3 step 10's typed outcome needs a
binary-visible read of the D16 exit-effect counter; it is presentation
state, not new behaviour.

## 6. The keypress-count guard (D6b) — §9.2/Q17

`tests/support/guard.rs`:

```rust
/// §9.2 ceilings — session-approved design contracts copied from the
/// design plan (complete ≤ 2, move ≤ 4, create ≤ 3, edit ≤ 3,
/// menu-invoke ≤ 3), NOT re-derived here. A chord counts as its
/// strokes (Ctrl+x Ctrl+c = 2); held modifiers are not presses.
const CEILINGS: &[(Verb, u32)] = &[ … ];
```

- **Counting**: the harness wraps each journey phase in
  `guard.begin(Verb)` / `guard.end(Verb)`; every `press_stroke` (and
  every typed character in step 3/4's title entry — injected through
  the same `press_stroke` path) increments the open phase's counter.
  The count is therefore *everything the harness injected*, by
  construction equal to the physical presses of a keyboard-only user
  (one event per press; modifier-set events are plumbing and are not
  counted).
- **Fixture convention (load-bearing)**: titles in steps 3/4 are
  **one character** (`"x"`/`"y"`), so `create` counts N + x + Return =
  3 against its ≤ 3 ceiling and `edit` counts Return + y + Return = 3
  against ≤ 3 — the guard sees the true cost including the one data
  key. The standard-preset table this produces: complete 1/≤2, move
  3/≤4, create 3/≤3, edit 3/≤3, menu-invoke 3/≤3 (Ctrl+Shift+M + Down +
  Return).
- **Verdict rule**: fail only when `count > ceiling` (assert with a
  message naming verb, count, ceiling); counts at or below pass — a
  strictly better UI always passes (AGENTS §5 "no quality ceilings"
  interplay, recorded openly in §9.2/Q17: this is a regression ceiling
  on degradation). Counts are also `tracing::info!`-ed so improvements
  are visible in CI logs.
- **T2 (guard unit test, pure)**: the guard logic itself is a pure
  function over `(recorded_counts, ceilings)` — unit-tested in-file
  with exceedance/improvement/exact-boundary cases (cheap, no
  rendering).

**D6b — mechanics**: ceilings live in this plan (design §9.2), carried
in the guard module as a cited constant; the five verbs are exactly the
five §9.2 names (delete/navigation carry no ceiling — not in the
contract; the journeys still *report* their counts for visibility).

## 7. The golden tier (D7) — `tests/golden_visuals.rs`

- **Matrix**: board view, fixture = the *clean* two-stack board (no
  overdue banner, no flow hint — banner labels derive from wall-clock
  relative time and would flake across hour boundaries; banner
  structure stays covered by the §6 component tests). Dimensions:
  scale ∈ {0.5, 3.0} (the config clamp bounds) × breakpoint ∈
  {compact 600, regular 1000, expansive 1500} logical px → **6 goldens**
  (`tests/goldens/board-<scale>-<breakpoint>.png`), window height fixed
  900. Orthogonality note: scale rides `UiSettings.scale` →
  `Theme.base-scale`; breakpoint rides `set_window_size` → the D17
  width push (the Q16 composition, exercised end-to-end).
- **Renderer**: the golden binary initialises the TestingBackend with
  `renderer_name: Some("software")` via the driver (D3's `init` gains a
  `headless_renderer: bool` option; only the golden binary turns it
  on). **Renderer-presence assertion first** (§10/stress §3.5's silent
  fallback warning): the compile-time guarantee is the
  `renderer-software` feature (its absence deletes the field — a
  compile error, not a silent fallback); the runtime assertion is
  `snapshot()` succeeding with dimensions exactly equal to the set
  window size — asserted *before* any golden comparison, with a
  message naming the renderer contract.
- **Determinism**: bundled fonts (registered by `mount`'s
  `register_bundled_fonts`, process-global Once) resolve by family name
  against our static instances; fixture text is ASCII-only (Inter
  covers it; no system-font fallback can differ); mock time means no
  animation/timer state advances between capture and compare; the
  software rasterizer is CPU-deterministic for a pinned version.
- **Comparison** (`tests/support/golden_compare.rs`, pure Rust):
  1. decode golden PNG (`png` dev-dep, workspace-pinned);
  2. assert dimensions equal (exact — structural);
  3. **perceptual hash**: hand-rolled **dHash-64** (box-average
     downscale to 9×8 grayscale → 64 comparison bits) Hamming distance
     ≤ **8** passes; 1–8 passes with a `tracing::warn!` reporting the
     distance (drift visibility); > 8 fails with both hashes in the
     message.
- **Tolerance choice (recorded)**: 8/64. Budget: cross-machine
  anti-aliasing differences of the software rasterizer measure 0–4
  bits in practice; the threshold doubles that headroom. A real
  regression (a card moved across columns, a collapsed decoration, a
  scale step missed) flips ≥ 20 bits — an order of magnitude beyond
  the threshold. The implementation **calibrates before landing**:
  perturb one committed golden (shift a fixture title / flip one
  stack's task count) and record the measured distance in the PR
  description; if measured AA noise exceeds 4 bits on any CI runner,
  the threshold is re-recorded in this file's amendment — never
  silently bumped in code.
- **Regeneration (explicit, reviewed)**:
  `TASKBOARD_REGEN_GOLDENS=1 cargo test -p taskboard-ui-slint --test
  golden_visuals` rewrites the PNGs and fails the run afterwards with a
  "goldens regenerated — review the PNG diff" notice so the operator
  cannot mistake a regen run for a pass. CI never sets the variable;
  the PR's PNG diff **is** the review artefact (GitHub renders the
  images). Documented in the test module's doc comment.
- **T3 (hash unit tests, pure)**: `golden_compare`'s dHash is
  proptested: stable under ±1 LSB per-channel noise on every pixel;
  sensitive to a one-row shift of a filled rectangle (distance >
  threshold). PNG round-trip unit test (encode→decode→equal buffer).

**D7 — goldens**: six PNGs committed as binary artefacts under
`tests/goldens/`; comparison = exact dimensions + dHash-64 ≤ 8;
regeneration env-gated and never CI-blessed (design §10's "explicit
reviewed step").

## 8. The MCP smoke + feature wiring (D8) — Q19

- **Feature**: `crates/taskboard-ui-slint/Cargo.toml` gains

  ```toml
  [features]
  # Dev/testing only (design §8.2/Q19): compiles the embedded MCP server
  # into the slint runtime. Never enabled by default; the CI feature-off
  # assertion (ci.yml/release.yml) guards release builds.
  mcp = ["slint/mcp"]
  ```

- **The smoke** (`crates/taskboard-app/tests/mcp_smoke.rs`,
  `#[ignore]`d): spawns the app binary
  (`env!("CARGO_BIN_EXE_taskboard")`) with a temp `TASKBOARD_CONFIG`
  TOML (`app.mode = "desktop"`, standalone sync backend, temp db) and
  env `SLINT_BACKEND=headless-software`, `SLINT_EMIT_DEBUG_INFO=1`,
  `SLINT_MCP_PORT=<free port>`. The test speaks the MCP server's HTTP
  JSON-RPC surface with a hand-rolled minimal client (`std::net::
  TcpStream`, one POST helper, localhost `Origin` header per the
  server's validation — no new dependencies; the exact endpoint path
  and framing are read from `mcp_server.rs` during implementation and
  the client is 1 screen of code):
  1. `initialize` handshake succeeds;
  2. `list_windows` returns ≥ 1 window (the app mounted);
  3. `take_screenshot` returns a non-empty base64 PNG that decodes
     with the expected dimensions (renderer present — the
     headless-software backend's real rasterizer);
  4. `dispatch_key_event` opens the menu (Ctrl/Shift modifier presses
     + `m`), then a second `take_screenshot` **differs bytewise** from
     the first (changes-after-interaction);
  5. the process is signalled (SIGTERM) and expected to exit before a
     deadline — boot-waits poll the port and the screenshot-readiness
     with a bounded deadline loop (AGENTS §5: poll a predicate with a
     deadline; no fixed sleeps).
- **CI job** (`ci.yml`, new `ui-mcp-smoke` job after "Test Execution"):
  `cargo test -p taskboard-app --features taskboard-ui-slint/mcp
  --test mcp_smoke -- --ignored --nocapture` on ubuntu-latest (no
  display needed — that is the point of the headless backend).
- **Feature-off assertion (D9)**: one step in the `ci.yml` audit job
  and one in `release.yml` before the matrix build:

  ```sh
  ! cargo tree -e features -p taskboard-app | grep -q 'feature "mcp"'
  ```

  (default feature resolution; grep is over cargo's structural output,
  not program messages). Rationale: features are additive and unify
  per-build — the assertion proves the *shipped* feature resolution
  never pulls `slint/mcp`, catching a stray `--all-features` habit or a
  default-feature leak. Note: the existing coverage job runs
  `--all-features` (llvm-cov) — under `--all-features` the mcp feature
  **will** unify in; that is a test-only build, not the release
  profile, and the assertion targets default/release resolution
  (recorded so the review does not read it as a contradiction). The
  MSRV job also runs `--all-features` (compile-only) — same reasoning.

**D8 — wiring scope**: exactly the design's "dev feature flag + one CI
smoke" (§8.2); no daemon/IPC driver lane, no goldens management, no
wrapper tooling (backlog entry stays open).

## 9. Deviations from the design plan (recorded; both are inventory
corrections, not normative changes)

1. **Stress §3.5's injection helpers are feature-gated** in 1.18.1
   (`internal`/`ffi`); the design's §8.1 requirement ("inject keys
   through the same root focus path real users hit") is met via the
   public `WindowEvent` dispatch + modifier-state dance (D3) — the
   identical mechanism the gated helpers implement. The `ffi` feature
   is deliberately left off (off-label for C++ bindings).
2. **Screenshot capture for goldens** uses `Window::take_snapshot()` on
   the TestingBackend's embedded software renderer (public,
   in-process) rather than routing through MCP — MCP is the *runtime*
   visual-debugging lane (§8.2), and the golden tier (§10) only needs
   deterministic capture. The design's tier table says "software
   renderer, headless, bundled fonts" — satisfied exactly.

No other deviations. The journey bar (§7.3), guard ceilings (§9.2),
tier placement (§10), pin discipline (Q8/Q12 + dispatch §1.4 item 5),
and MCP scope (Q19) are implemented as written.

## 10. TDD catalogue (implementation order)

| # | Test | Layer | Gate |
|---|---|---|---|
| T1 | pin guard: the two Slint version reqs in the root manifest are equal | unit (structural manifest parse) | pin equality is mechanical |
| T2 | guard verdict: exceedance fails, exact-boundary and improvement pass; chord/stroke accounting | pure unit | before any journey runs |
| T3 | dHash: noise-stable (proptest ±1 LSB), shift-sensitive (> threshold); PNG round-trip | pure unit + proptest | before goldens |
| T4 | driver smoke: platform init, mount, first refresh, accessible "Board" query resolves, `set_window_size` maps breakpoints | journey-binary helper test | the harness itself boots |
| T5 | the ten-step continuous session (§5.1) with per-step typed asserts | journey | §7.3 |
| T6 | cold-start view-graph walk + compact leg (§5.2) | journey | §8.1 scriptability |
| T7 | keypress counts vs §9.2 ceilings (the T2 verdict run over T5's recorded counts) | journey (guard) | §9.2 |
| T8 | goldens: 6-image matrix, dimension-exact + dHash ≤ 8, renderer-presence first, regen env path | golden | §10 |
| T9 | MCP smoke: handshake, list_windows, non-empty screenshot, changes-after-interaction, clean exit | app integration (CI-invoked) | §8.2 |
| T10 | CI feature-off assertion (cargo tree) | workflow step | release profile hygiene |

TDD order: T2/T3 (pure) → T1 → T4 → T5 (steps land incrementally, one
commit each: driver, fake, session steps 1–2, 3–4, 5–7, 8–10) → T6 →
T7 → T8 → T9/T10 (CI). Every commit standalone (rebase-merge hygiene);
the goldens commit contains only PNGs + the golden test.

## 11. Production-code touch points (exhaustive)

1. `crates/taskboard-ui-slint/src/lib.rs`: `UiHandle::quit_requests()`
   accessor (D6, ~8 lines with docs).
2. Root `Cargo.toml`: the dev-pin entry + `png` workspace pin (§2, D8).
3. `crates/taskboard-ui-slint/Cargo.toml`: dev-dep + `mcp` feature.
4. `.github/workflows/ci.yml`: the smoke job + feature-off step.
5. `.github/workflows/release.yml`: the feature-off step.

Everything else is test-only. No view, action, resolver, or engine
change unless a journey exposes a real bug (risks R1) — then the fix is
its own `fix(ui-slint):` commit with its own test rationale, per
AGENTS §5.

## 12. Risks & mitigations

- **R1 — step 4 (standard-preset title edit) may expose a real focus
  gap** (the TaskDetail title field's keyboard reachability was only
  exercised via callbacks in chunk 3's tests). If the journey cannot
  reach the field with keys, that is a production finding: fix the
  focus hand-off on the branch; never weaken the journey.
- **R2 — dHash threshold calibration**: 8/64 is an informed default;
  the calibration-before-landing procedure (§7) makes the number
  evidence-based; a wrong-low threshold flakes, a wrong-high one blinds
  — the perturbation measurement guards both directions.
- **R3 — zero headroom on create/edit ceilings** (3/≤3): intended
  sensitivity (§9.2 exists to fire on added scaffolding), but it means
  the one-character-title convention is load-bearing; the guard module
  documents it so a future fixture edit does not silently shift the
  contract.
- **R4 — MCP protocol details** (endpoint path, framing, origin
  validation) must be read from `mcp_server.rs` during implementation;
  the hand-rolled client is deliberately minimal so surprises stay
  local to one file.
- **R5 — feature unification**: `--all-features` coverage/MSRV builds
  will compile the mcp feature in; the feature-off assertion targets
  default/release resolution (recorded in D9) so this is test-build
  noise, not a shipped surface.
- **R6 — snapshot readiness**: `take_snapshot` on the headless backend
  may require a resize/draw pass first (the TestingWindow defaults
  800×600); the driver's `snapshot()` performs a no-op size re-set if
  needed — an implementation detail confined to `driver.rs`.
- **R7 — non-ASCII fixture text would break golden determinism**
  (system-font fallback for missing glyphs); ASCII-only rule is stated
  in `fixture.rs`'s docs.
- **R8 — journey brittleness vs widget churn** (design §14): mitigated
  exactly as the design prescribes — assertions at stable seams
  (current_view, model structure, emphasis, KeyboardState, forwarder
  commands, a11y roles/labels as addresses), never per-widget
  internals.

## 13. Estimated diff

- `tests/support/` (driver, fake_engine, fixture, guard,
  golden_compare): ~800–1,000 lines.
- `tests/journeys.rs`: ~450–550 (the session + walk).
- `tests/golden_visuals.rs`, `tests/pin_guard.rs`: ~200.
- `crates/taskboard-app/tests/mcp_smoke.rs`: ~200–250.
- Production touch points (§11): ~30 lines.
- CI: ~50 lines of workflow.
- 6 binary PNGs (≈ 20–60 KB each).
- **Total ≈ 1,700–2,100 lines** — size **M**, consistent with the
  dispatch plan's 5–8 dev-day estimate for chunk 4.

## 14. Acceptance (mirrors dispatch §5)

All ten §7.3 steps + the view-graph walk green in CI, deterministic by
construction (mock time; any `tokio::time::sleep` or
`std::thread::sleep` in the harness is a review finding — boot-waits
are bounded predicate polls); journeys assert routing only (typed
commands at the fake boundary; labels are element addresses); the
keypress guard enforces the §9.2 ceilings and cannot fail on
improvement; goldens cover the scale-extreme × breakpoint matrix behind
the renderer-presence assertion with an explicit regeneration
procedure; the exact pin equals the slint pin (T1); the MCP smoke job
is green and the feature-off assertion holds; gates green (fmt, clippy
`-D warnings`, nextest, doc-tests, deny with the testing crate's
tri-license admitted, llvm-cov); the growth-roadmap "Slint component
tests" exit-criterion line is satisfied by this chunk's journey tier.

# [Plan] Window-geometry persistence (#412) — chunk 2.4, backlog round 2

- **Date**: 2026-10-31 (planned 2026-11-02)
- **Kind**: stage-1 implementation plan for dispatch chunk **2.4**
  (#412, branch `feat/window-geometry-persistence`, batch 2 — the
  UI/persistence lane). TDD-first; no production code in this commit.
- **Origins** (normative, in precedence order):
  issue **#412** (the ask: persist the main window's size/position
  across restarts — the cross-restart counterpart of r1 D15's
  within-session stability);
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  chunk 2.4 (acceptance: geometry survives restart, journey-pinned at
  a typed seam — no real windowing in tests; degenerate/off-screen
  values clamp, pinned; kiosk mode unaffected — fullscreen owns
  geometry, pinned or explicitly out-of-path; CHANGELOG entry; review
  traps: the off-screen/clamp story, the write path must not fight the
  config file's other writers, the kiosk bypass is structural, the UI
  layer renders what it is handed);
  `.artifacts/plans/2026-10-22-gui-review-round1-plan.md` §5 D15
  (within-session stability: the window never resizes itself; no
  `width:`/`height:` binding on the root Window — the invariant this
  feature must not violate) and §13 (this issue's birth: "window
  geometry persistence across restarts — D15 intentionally scopes to
  within-session stability"); §6 D16–D21 (**the storage precedent**:
  stack widths persist device-locally in the config's `[ui]` table —
  D20's rationale is binding for this chunk) and R6 (the single-writer
  rule for overlay subtrees);
  `docs/coding_guidelines.org` §2.1 (the A1 pattern: hand-rolled
  private-field newtype, fallible mint at the trust boundary,
  documented leniency, no validate-and-forget, no derive crates).
- **Branch / worktree**: `feat/window-geometry-persistence` in
  `../taskboard-WT-feat-window-geometry-persistence`, branched off
  `origin/main` @ `a3987d11` (2026-11-02; local `main` is behind and
  branch-protected — the branch was re-pointed at the fetched
  `origin/main` tip; every seam below was read at that base).
- **Gates verified**: chunk 1.3 (#463) shipped as PR #563 (merge
  `013eb200`), chunk 1.6 (#194) shipped as PR #567 (merge
  `95cbe489`) — both merge commits are ancestors of `origin/main`.
- **Status**: **READY** — all seams verified at the branch base; the
  persisted-value API surface was checked against the pinned slint
  1.18.1 sources (`i-slint-core-1.18.1`): `Window::position()` /
  `set_position` / `size()` / `set_size` / `is_maximized` /
  `is_minimized` / `is_fullscreen` / `on_close_requested` all exist
  public; the guard states are window-item properties settable
  platform-independently (pinnable on the headless test platform).

---

## 1. Mission & binding constraints

The desktop window's size and position survive a restart: quit
anywhere, relaunch, the window reopens where the user left it
(subject to §4 D5's clamp). First run (nothing stored), a degenerate
stored value, and every kiosk boot behave exactly as today.

Binding, restated against the read seams:

1. **Device-local by design, never synced** (#412 via the dispatch
   plan): geometry never enters `taskboard-domain`, the engine state,
   the storage sidecar, or the sync payloads. The grep invariant the
   stage-3 reviewer runs — *nothing under `taskboard-domain` touches
   it* — holds by construction: the type lives in the UI crate, the
   raw table in the app crate, and no domain type or command grows a
   geometry field.
2. **The UI crate stays config-free** (crate doc, design §1.2): the
   restore value crosses the mount boundary as a typed
   [`UiSettings`] field (the `stack_widths`/`scale`/`day_start`
   pattern — resolved app-side once, held, never re-read mid-flight);
   the capture value leaves the UI crate as a typed `UiRequest` (the
   D21 lane). No file IO, no config reads under `taskboard-ui-slint`.
3. **D15's invariant survives**: no `width:`/`height:` binding is
   added to the root Window (`app-window.slint:104-111` — preferred
   bindings + min floors stay the no-geometry default). Restoring
   size goes through the OS-window level (`slint::Window::set_size`),
   which the invariant never covered; a code comment at the apply site
   says so, because the invariant comment invites the confusion.
4. **No real windowing in tests** (dispatch acceptance): every new
   test runs on the headless `test_platform` (`NullWindowAdapter`) or
   pure; the winit-reading monitor accessor degrades to a structural
   no-op without a winit window (the request-user-attention
   precedent, lib.rs:451-461). No sleeps anywhere; the debounce is
   deterministic under `tokio::time::pause` (the width-forwarder
   tests' pattern, ui_requests.rs:1182).
5. **AGENTS §5 discipline**: typed outcomes, no text assertions, one
   behavior one test at the lowest layer, no quality ceilings; the
   clamps and the kiosk bypass pins must be load-bearing.
6. **The A1 policy** (coding guidelines §2.1) for the persisted
   value: a hand-rolled private-field newtype with a fallible mint at
   the config-load boundary; deliberate leniency (a degenerate stored
   value defaults, it never blocks UI start — the Q15 rule) documented
   at the type; no validate-and-forget (everything downstream of the
   mint carries the minted type; the write path consumes the minted
   type only).
7. **The single-writer rule extends** (r1 R6): the UI request
   forwarder is the only writer of the new `[ui.window_geometry]`
   subtree, and one flush stays one atomic read-merge-write over the
   whole document (the overlay discipline, ui_requests.rs:192-236).

## 2. Decision ledger

Every open question the dispatch plan left to stage 1, resolved:

- **D1 — Storage home: the `[ui]` config section, via the established
  write path.** A new optional `[ui.window_geometry]` table in
  `taskboard.toml`, written by the existing UI request forwarder's
  read-merge-write overlay (`overlay_document` → `write_config_atomic`).
  **Rejected: a separate state file** — D20's recorded rationale
  applies verbatim: the persisted shape is sync/merge-aware territory
  and a device-local preference does not justify opening it; the
  config file already anticipates device-local settings (F16); kiosk =
  one device per config; and a second persistent artifact would
  duplicate the atomic-write/mode-preservation/single-writer machinery
  (config.rs:1456-1493, R6) for zero benefit. The `[preferences]`
  family and `[ui.stack_widths]` are the in-file precedent for
  "device-local, never synced" tables. Consequence: **zero
  domain/state/storage diff**; the value roams nowhere.
- **D2 — Persisted shape: physical pixels, four integers.**
  `{ x: i64, y: i64, width: i64, height: i64 }` as the raw serde
  table; minted into a UI-crate newtype carrying `x: i32, y: i32,
  width: u32, height: u32`. **Physical**, not logical: physical
  pixels are the unit `Window::position()`/`size()` already report
  (slint 1.18.1 api.rs:547,561), they are scale-factor-independent
  (a logical store would need the store-time scale factor persisted
  beside the rect to be interpretable at restore), and a physical
  rect clamps sanely across monitor changes. Position may be negative
  (left-of/above-primary monitors are legitimate desktop geography —
  rejected positions would corrupt real setups). The raw table keeps
  `i64` so `config print` echoes stored values verbatim and no
  legitimate value trips decode (the `[preferences] day_start_time`
  raw-form precedent).
- **D3 — The mint (`WindowGeometry::new`) rejects degenerate dims;
  off-screen is *not* a mint concern.** `WindowGeometry::new(x, y,
  width, height) -> Option<Self>` (i64 inputs, one constructor serves
  both the config mint and the capture site) rejects: `width < 1`,
  `height < 1`, any dim above an `i32::MAX` ceiling (documented at the
  type: keeps the clamp arithmetic in checked-i32 space), and
  positions outside `i32`. A rejected **stored** value mints to
  `None` ⇒ the hand-over carries `None` ⇒ today's default placement —
  the lenient-read contract documented at the type (the `DeckColor`
  "strict writes, lenient reads" shape; a hostile geometry never
  blocks UI start, Q15). A rejected **captured** value skips the save
  (nothing degenerate is ever written). Off-screen positions are
  *legitimate history* (a monitor unplug between sessions), not
  garbage: the write path minted them from a real window, so they
  pass the mint and are handled where the current monitor is actually
  knowable — the restore-time clamp (D5). The dispatch prompt's
  "off-screen dims rejected at mint and defaulted" collapses, on the
  read seams, into this split: degenerate *dims* at the mint,
  off-screen *restore* at the clamp. Recorded as a deliberate
  deviation-with-reason.
- **D4 — Save trigger: debounced on change, riding the D21 lane.
  Rejected: save-on-graceful-exit.** The exit hooks do not cover the
  field: `on_close_requested` fires only for the OS close button —
  the menu quit (adapter.rs:1601-1602 → `crate::quit()`) and every
  signal exit (SIGTERM/SIGINT via `quit_until_accepted`,
  ui.rs:354-372 → `slint::quit_event_loop()`) bypass it, and `run`
  consumes the `UiHandle` so the app-side tails have no window to
  read (lib.rs:757-773). Debounced-on-change survives every exit
  path: the capture rides the 100 ms repaint tick (a move or resize
  becomes a typed request within one tick; the forwarder's 500 ms
  quiet window coalesces drag bursts — its own docs already speak of
  "a resize burst", ui_requests.rs:39), and the channel-close flush
  lands a change sent right before quit (ui_requests.rs:85-88). The
  worst loss window is the final <500 ms before a SIGKILL — accepted,
  documented in the CHANGELOG entry.
  **Capture guards** (all pinned): never under the kiosk profile
  (D6); never while `is_maximized()` (a maximized window's bounds are
  the workspace's, not the user's normal geometry — skip, keeping the
  last normal rect; v1 never persists the maximized state itself, so
  a maximized-at-exit session reopens unmaximized at the old normal
  rect — recorded as the v1 semantics, an extension candidate, not a
  gap); never while `is_minimized()` (platforms report stale/zero
  geometry minimized); never while `is_fullscreen()` (belt for the
  desktop profile — the kiosk guard is the structural one); and never
  a degenerate snapshot (the mint refuses — which is also what keeps
  the headless tests' default 0×0 silent).
- **D5 — Restore: typed hand-over at mount, position-only clamp
  against the current monitor.** `UiSettings.window_geometry:
  Option<WindowGeometry>`; on the desktop profile `mount` applies
  `set_position(Physical)` + `set_size(Physical)` right after window
  construction (before show). Before applying, the pure
  `WindowGeometry::clamp_to(screen)` guarantees a visibility margin
  strip of the window on the current monitor's rect: fully/partially
  visible (≥ margin in both axes) ⇒ unchanged (never fight the
  user's placement); otherwise the position is pulled to the nearest
  legal spot — **size is never altered** (a window wider than the
  monitor legitimately spanned two monitors; shrinking it would lose
  user state, and the WM enforces the D15 min floors anyway). The
  monitor rect comes from the winit accessor (`current_monitor()`,
  else the first available monitor, else no clamp) — without a winit
  window (headless tests, non-winit backends) the clamp is a
  structural identity no-op, the attention-channel precedent
  (lib.rs:451-461). Correctness of the clamp math is pinned at the
  pure layer with synthetic rects (§5 T2); the mount-level pin
  asserts the hand-over reaches the window adapter.
- **D6 — Kiosk exclusion: structural, twice over.** The kiosk
  appliance boots with `UiSettings.kiosk = true` (main.rs:793) and
  `mount` applies fullscreen in the `if settings.kiosk` arm
  (lib.rs:266-272); the restore becomes the `else if` arm of that
  same chain — one window cannot take both paths, a kiosk boot with a
  stored geometry **cannot** restore it, no lifecycle code needs to
  remember to clear the field. Symmetrically, the capture arm
  checks the adapter's `kiosk` bit (the same hand-over value,
  adapter.rs:113-115) first and returns — the appliance never
  samples, never sends, never writes. Fullscreen owns geometry on
  the appliance; the path is bypassed, not special-cased. Pinned
  both ways (§5 T4).
- **D7 — The request and the write shape.**
  `UiRequest::WindowGeometryChanged { geometry: WindowGeometry }` —
  a full-snapshot value (idempotent under re-send, the width-snapshot
  semantics), taking the **same debounce lane** as
  `StackWidthsChanged` (the forwarder's quiet window restarts on it;
  settings requests keep executing on the spot). The forwarder holds
  a `pending_geometry: Option<WindowGeometry>` beside the widths map
  (latest wins — a burst collapses to the last rect) and one flush
  writes **one document** covering both subtrees (§4 F3): the flush's
  early-return becomes "no widths and no geometry", and the overlay
  applies the widths subtree (existing code) then the geometry
  subtree to the same `DocumentMut` before the single
  `write_config_atomic`. Geometry needs no liveness prune (nothing in
  the engine state can invalidate it). The subtree serializes as an
  inline-style four-key table under `[ui.window_geometry]`; a session
  that never moved anything writes nothing (no key, the
  `skip_serializing_if` + forwarder-empty-flush behavior).
- **D8 — Docs**: one CHANGELOG `Added` entry (dispatch acceptance);
  README.org's annotated config example grows the `[ui.window_geometry]`
  table with one device-local sentence (the config guide's home);
  `docs/architecture.org`'s `[ui]` config-surface line (§659-661)
  names the new table. No ADR — D20 already ruled the
  device-local-config home for exactly this class of value.

## 3. The seams (as read, at base `a3987d11`)

**Save path (today → extended):**

```
slint::Timer (100 ms, lib.rs:482)          the D1 driver tick
  └─ tick closure lib.rs:473-560           drains signals/system/health/replies,
                                           refresh on dirty, alarm pass
       └─ [NEW] adapter.capture_geometry(&window)   tick tail, unconditional
            ├─ kiosk? → return                          (D6, structural)
            ├─ maximized/minimized/fullscreen? → return (D4 guards)
            ├─ Window::position() -> PhysicalPosition   api.rs:547
            ├─ Window::size()      -> PhysicalSize      api.rs:561
            ├─ WindowGeometry::new(..) -> Option        (D3; None = silent skip)
            ├─ last-seen equal? → return                (no request)
            └─ requests.try_send(WindowGeometryChanged) (drop-on-full,
                                                        adapter.rs:583 pattern)
mpsc (UI_REQUEST_CAPACITY = 16)
  └─ spawn_ui_request_forwarder ui_requests.rs:77-119
       ├─ merge into pending_geometry (latest wins)
       ├─ debounce 500 ms (UI_REQUEST_DEBOUNCE; tokio::time::pause tests)
       └─ flush: one overlay_document RMW → write_config_atomic
            └─ [NEW] [ui.window_geometry] subtree applied (D7)
```

**Restore path (today → extended):**

```
run_desktop ui.rs:379
  └─ resolve_ui_settings ui.rs:192-253
       └─ [NEW] ui.window_geometry (raw table) → WindowGeometry::new → None on refuse
            (lenient read; the UiScale::new(..).expect shape is NOT copied —
             geometry's mint is fallible by design, D3)
  └─ mount lib.rs:244-272
       ├─ AppWindow::new()                    preferred bindings = defaults
       ├─ if settings.kiosk → set_fullscreen  (unchanged)
       └─ [NEW] else if let Some(g) → WindowGeometry::clamp_to(current_monitor)
                                                 → set_position + set_size
```

**Test seams:** `NullWindowAdapter` (test_platform.rs) already owns
`size` as a `Cell<PhysicalSize>` set through the public
`Window::set_size` path; the guard states are window-item properties
(`is_maximized`/`is_minimized`/`is_fullscreen` read them;
`set_maximized`/`set_minimized`/`set_fullscreen` write them,
i-slint-core window.rs:2443-2466 — platform-independent, so pinnable
headlessly as-is). **One extension needed**: `NullWindowAdapter`
records `position` (a `Cell<Option<PhysicalPosition>>` +
`WindowAdapter::position`/`set_position` overrides — the trait's
defaults return `None`/no-op, window.rs:101-110), so a test can play
the window manager (set a position, watch the capture) and watch the
restore (mount, read the recorded position). No winit anywhere in
tests.

## 4. The work, in TDD order

**F1 — `taskboard-ui-slint/src/geometry.rs` (new module, `pub`).**
`WindowGeometry` (private fields, `Debug/Clone/Copy/PartialEq/Eq`),
the documented-at-the-type leniency contract, the fallible `new`
(D3), accessors (`x()`, `y()`, `width()`, `height()` — no public
fields, A1), the pure `clamp_to(self, screen: WindowGeometry) ->
Self` with `VISIBILITY_MARGIN_PX: i32 = 80` (a named const; enough
to grab and drag back; exact value is stage-2-tunable but must stay
a const) and checked/saturating i32 arithmetic documented at the fn
(extremes are pinned, T2). No serde impls (the raw table lives
app-side; the width map's "uuid-keyed serde-native shape, the mount
converts once" reasoning, lib.rs:91-96). **Test first**: T1 + T2.

**F2 — the hand-over.** `UiSettings.window_geometry:
Option<WindowGeometry>` (+ `Default::None`); `pub mod geometry` in
lib.rs. App side: `UiSection.window_geometry:
Option<WindowGeometryTable>` — a plain serde struct `{ x, y, width,
height: i64 }` with `deny_unknown_fields`,
`#[serde(default, skip_serializing_if = "Option::is_none")]`
(old configs parse, the T-14 test shape at config.rs:1669-1704);
`resolve_ui_settings` mints via `WindowGeometry::new(..)` and hands
`None` through on refusal (ui.rs:212-248). **Test first**: T5, T6.

**F3 — the save lane.** `UiRequest::WindowGeometryChanged {
geometry: WindowGeometry }` (requests.rs, after `StackWidthsChanged`
— the enum's doc updates: two debounced variants). Forwarder:
`pending_geometry`, the match arm, the flush restructure to one
document/one atomic write (D7) — `overlay_window_geometry(doc, &g)`
as a sibling helper beside `overlay_stack_widths`, both applied to
the same `DocumentMut`. **Test first**: T7, T8, T9.

**F4 — the capture.** `Adapter` grows `last_window_geometry:
Option<WindowGeometry>`; `Adapter::capture_window_geometry(&mut
self, window: &AppWindow)` implements the D4 guard chain + mint +
compare + `try_send` (the `apply_outcomes` width-request shape,
adapter.rs:577-585); the driver tick calls it unconditionally at the
tail (lib.rs, after the alarm pass) — *not* inside `refresh`
(eleven callers with their own semantics; the tick is the one
production sampler, a public `UiHandle::capture_window_geometry()`
mirrors the `drain_*` test-seam pattern for the journeys).
**Test first**: T3.

**F5 — the restore.** `mount`'s `else if` branch (D6) with the
`clamp_to` call and the winit monitor accessor
(`observe_current_monitor_rect(window) -> Option<WindowGeometry>`:
`has_winit_window()` → `with_winit_window(|w|
w.current_monitor().or_else(first-available))` → position+size
minted; any absence ⇒ `None` ⇒ identity clamp), placed beside the
attention accessor it imitates (lib.rs:451-461); the apply-site
comment stating the D15 non-violation (§1.3). `NullWindowAdapter`
position recording (test_platform.rs). **Test first**: T4.

**F6 — docs.** CHANGELOG `Added`; README.org config example +
device-local sentence; architecture.org `[ui]` surface line. (No
code; rides the last commit.)

Commit shape (rebase-merge hygiene, each standalone): the plan's
author suggests `feat(ui-slint): the typed window geometry value`
(F1+F2's UI half), `feat(app): the window-geometry config lane`
(F2's app half + F3), `feat(ui-slint): the geometry capture and
restore` (F4+F5), `docs: the window-geometry config table` (F6) —
stage 2 may re-slice, but every gate-relevant pin lands in the same
commit as the code it pins.

## 5. Test catalogue (TDD-first; no real windowing anywhere)

- **T1 mint accepts/rejects (geometry.rs, pure)** — sane rects
  including negative positions and (0,0) origin mint; `width ≤ 0`,
  `height ≤ 0`, dims over the documented ceiling, positions outside
  i32 refuse (`None`); accessors return the minted values exactly
  (no clamping at mint — the off-screen decision, D3).
- **T2 clamp (geometry.rs, pure)** — the table:
  fully on-screen ⇒ identity; ≥ margin overlap in both axes ⇒
  identity (partial visibility is user geography, never "fixed");
  fully off-screen right/bottom/left/top ⇒ pulled to the margin
  strip; sub-margin overlap (e.g. 10 px) ⇒ pulled in; window larger
  than the screen ⇒ position clamps so the margin strip is visible,
  size untouched (D5); degenerate screen (0×0) ⇒ identity;
  `i32::MAX`-scale inputs ⇒ saturating, never panic. Load-bearing:
  the reviewer inverts the clamp (identity) and watches T2's
  off-screen pins fail.
- **T3 capture (UI crate, `on_slint_thread` + null platform)** —
  desktop mount, public `set_size` to a sane rect + adapter position
  set (the WM role), one `capture_window_geometry()` ⇒ exactly one
  `WindowGeometryChanged` with the minted values; second call
  unchanged ⇒ no request; fresh-mount degenerate (0×0) ⇒ no request
  (the mint's silent skip); each guard set via the public window
  setters (`set_maximized`/`set_minimized`/`set_fullscreen`) ⇒ no
  request; kiosk hand-over ⇒ no request even with a sane live
  geometry (D6's capture side); a changed position alone (size
  equal) ⇒ one request (moves persist too).
- **T4 restore + kiosk bypass (lib mount tests, null platform)** —
  desktop hand-over `Some(g)` ⇒ the adapter's recorded position/size
  equal `g` (no winit ⇒ identity clamp — the fallback pin); `None`
  ⇒ nothing recorded (defaults unchanged); **kiosk hand-over
  `Some(g)` ⇒ `is_fullscreen()` true AND no position/size recorded**
  (the structural bypass pin; load-bearing — reviewer wires the
  restore into the kiosk arm and watches this fail).
- **T5 config round-trip (config.rs tests, T-14 shape)** — a file
  with `[ui.window_geometry]` (incl. a negative position) parses;
  absent key ⇒ `None` (old configs parse); a present key
  serializes; a never-geometry config serializes byte-unchanged.
- **T6 lenient read at the hand-over (ui.rs tests)** — a valid table
  ⇒ `resolve_ui_settings` hands `Some` minted; degenerate dims
  (`width = 0`, absurd `2^40`) ⇒ `None`, and the call still returns
  (never blocks UI start, Q15/D3).
- **T7 forwarder debounce (ui_requests.rs tests, the
  `debounces_bursts_and_flushes_on_close` pattern)** — two geometry
  requests inside the quiet window ⇒ one write carrying the last
  rect; the flush lands on quiet-window elapse and on channel close.
- **T8 overlay write (ui_requests.rs tests)** — a geometry flush
  produces `[ui.window_geometry]` with the four integer cells;
  comments and unrelated tables elsewhere in the file survive
  byte-for-byte (toml_edit); widths + geometry pending together ⇒
  **one** atomic write carrying both subtrees (D7; the R6
  single-document discipline).
- **T9 journey pin (UI crate journeys tier)** — the end-to-end
  seam: a desktop mount with a stored geometry reopens at it, a
  geometry change during the session emits the request (the
  "survives restart" acceptance pinned at the typed seam; the
  restart itself is out of a test's reach — the two halves are each
  pinned, the config file is the join).

## 6. Risks

- **Wayland** (the honest platform gap): `Window::set_position` is a
  documented compositor no-op on Wayland (api.rs:555-556) and
  `position()` may report a placeholder. Consequence: on Wayland the
  restore degrades to compositor-decided placement (size still
  restores) and the capture may persist a placeholder position that
  a later restore also ignores. Benign by construction (the clamp
  and mint keep garbage out of the crash class), but it must be a
  sentence in the CHANGELOG entry and the type docs — not a surprise.
  Dev platform is X11-first (the kiosk runbook); no gate is
  weakened.
- **DPI changes between sessions**: physical pixels restore
  1:1 per monitor; a window moved from a 1× to a 2× monitor appears
  half the logical size. Accepted (D2 records why physical); the
  clamp keeps it on-screen; one user resize re-teaches it.
- **Stray capture requests in existing journeys**: a journey that
  sizes the window via the public `set_size` (golden raster tests)
  will see one `WindowGeometryChanged` arrive on its request
  receiver's first tick. No existing test asserts that channel
  empty (verified by grep at base); stage 2 re-verifies. If one
  surfaces, the fix is draining in the harness — never weakening a
  test.
- **Maximized-state fidelity** varies by WM (lazy reports): the
  guard's worst case is a maximized rect slipping into a save; the
  clamp keeps the restore visible. Cosmetic, self-correcting (the
  next normal-state capture overwrites).
- **The forwarder flush restructure** touches the R6 single-writer
  path (the review trap): one `DocumentMut`, two subtree helpers,
  one `write_config_atomic` — T8 pins the discipline before the
  refactor lands.
- **The `install` argument list** is already at its lint grant
  (`too_many_arguments`): the geometry rides `UiSettings` into
  `mount`, but `install` takes unpacked args — stage 2 either adds
  one more arg under the existing grant or (preferred) passes the
  already-minted `Option<WindowGeometry>` bundled with the widths
  seed. A mechanical decision, recorded in the impl commit.

## 7. Out of scope

Multi-monitor-aware restore (per-monitor geometry), persisting the
maximized state, a "reset window placement" affordance, alarm-window
geometry (the lazily mounted second window stays default), and any
sync/sidecar representation. Each is a backlog candidate if wanted —
none blocks #412's acceptance.

## 8. Estimated diff

~600–700 lines including tests: geometry.rs ~200 (type + clamp +
T1/T2), lib.rs ~40, adapter.rs ~60, requests.rs ~10,
test_platform.rs ~25, lib/journey tests ~150; app side ~200
(config.rs table + tests ~70, ui.rs mint ~15, ui_requests.rs lane +
overlay + tests ~115); docs ~25 (CHANGELOG, README.org,
architecture.org). Zero domain/state/storage lines — the grep
invariant is the empty diff.

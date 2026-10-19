# [Plan] Kiosk Chunk 6 — refresh+draw bench (TDD-first)

- **Date**: 2026-10-20
- **Kind**: stage-1 implementation plan (dispatch plan §7, chunk 6) for
  `feat/perf-refresh-draw-bench` — the S2 refresh+draw bench. TDD-first;
  no implementation in this commit.
- **Origin (normative, MD-7 rank)**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 **KDF-7 row 3**, §3 **K6**, §4–§6); the audit report
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md`
  (§2 budgets table **row 3**, §3 sketch **S2** verbatim — it is the
  design — plus the **§3.7 C2 note** on the 100 ms cadence interaction);
  the dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1/§7 (**MD-5** tier & evidence honesty, MD-1: no round gate, MD-6:
  this path, MD-8: anchors re-located, MD-3: code-quality mandate).
  Deviations from prose are recorded inline with reasons; none touches
  an ACCEPTED ruling.
- **Status**: READY — every open question settled below (§3 decisions
  D1–D13). Base `1bcbb5e` = `origin/main` (fetched 2026-10-20); local
  `main` (`ac77807`) is stale and untouched. **Round state**: chunks
  1–5 of this round have merged (gui verb, kiosk window profile,
  appliance lifecycle, packaging, process harnesses); chunk 6 has no
  predecessor and no stacking (MD-1/MD-2) — `5 ∥ 6 ∥ 7` are
  file-disjoint by construction.

## 1. What this chunk delivers

Three things, per K6 and sketch S2:

1. **The seam extension** — `crates/taskboard-ui-slint/src/test_platform.rs`
   (`NullPlatform`/`NullWindowAdapter`): the `SoftwareRenderer` that is
   today "constructed but never asked to draw" (its own doc contract,
   `test_platform.rs:22-24`) becomes **draw-capable headlessly**: the
   adapter gains a real size, the public resize path, a render
   pass-through into a caller buffer, and a registry so the harness can
   reach the mounted window's adapter. Pinned by new seam unit tests
   (structural facts — no golden images, no visual assertions; K6
   non-goal).
2. **The API verdict recorded** (§1.1): slint 1.18.1 **can** render
   headlessly into a buffer with no windowing system — two independent
   mechanisms, evidence with file paths. The API-blocker rule (K6/MD-5)
   does not fire on the verdict; a functional-scene fallback (D11)
   remains recorded for stage-2 honesty.
3. **The criterion bench** — `crates/taskboard-ui-slint/benches/
   refresh_draw.rs`: the **refresh leg** (`ArcSwap` load →
   `view_state.reconcile` → `models::push` VecModel rebuild —
   `adapter.rs:188-196`) and the **draw leg** (software render into a
   1366×768 buffer), **timed separately**, at board scale N ∈
   {100, 1 000, 10 000} on a deterministic fixture. Plus the CHANGELOG
   Unreleased line. **Closes #300** (stage 3's PR body) with the
   dev-machine numbers recorded in the report and PR body (MD-5:
   references, never gates).

Budget context (row 3, stated not asserted): full-frame repaint
≤ 50 ms / partial ≤ 20 ms @ 1366×768 software renderer on one 1.5 GHz
in-order core — the bench produces **dev-machine references**; the
on-device verdict belongs to the phase-B spike. The C2 note stands
un-adjudicated: composed input→visible latency is floored by the
100 ms repaint cadence (`adapter.rs:35`, `REPAINT_CADENCE_MS`) on any
hardware; this chunk changes no cadence (F13-09).

### 1.1 The API verdict (the blocker question, answered)

**Verdict: YES — headless draw into a buffer works on the pinned slint
1.18.1, by two independent mechanisms.** All paths below are under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (the
checked-out 1.18.1 sources this workspace compiles against; the exact
pin is root `Cargo.toml:38,47`).

**Mechanism A — direct `SoftwareRenderer::render` into a caller-owned
buffer (the bench's mechanism):**

- The module is public and **on by default**: `slint-1.18.1/lib.rs:484-490`
  — `#[cfg(feature = "renderer-software")] pub mod software_renderer {
  pub use i_slint_renderer_software::*; }`, and `renderer-software` is
  part of slint's `default` features (verified live: `cargo tree -p
  taskboard-ui-slint --all-targets -e features -i slint` shows
  `slint feature "renderer-software" ← slint feature "default"`). The
  repo already imports it today: `src/test_platform.rs:11`.
- The draw call: `i-slint-renderer-software-1.18.1/lib.rs:538` —
  `pub fn render(&self, buffer: &mut [impl TargetPixel], pixel_stride:
  usize) -> PhysicalRegion`. Pure CPU rasterisation into a
  **caller-owned** buffer; no windowing system, no GL, no event loop.
- Window binding is automatic: `i-slint-core-1.18.1/src/window.rs:761`
  calls `window_adapter.renderer().set_window_adapter(&window_adapter)`
  at window setup; the software renderer stores the weak adapter
  (`i-slint-renderer-software-1.18.1/lib.rs:1198-1203`) — an adapter
  whose `renderer()` returns a `SoftwareRenderer` (exactly
  `NullWindowAdapter` today, `test_platform.rs:44-46`) needs **zero
  extra wiring** to be drawable.
- Full-frame vs partial is a public knob: `RepaintBufferType`
  (`i-slint-core-1.18.1/partial_renderer.rs:380-397`, re-exported at
  renderer `lib.rs:69`) — **`NewBuffer` is the default** (full window
  always redrawn); `ReusedBuffer` opts into dirty-tracked partial
  rendering when the same buffer is passed each call;
  `set_repaint_buffer_type` is public (renderer `lib.rs:498`). `render`
  returns the drawn `PhysicalRegion` (renderer `lib.rs:222+`;
  `bounding_box_size()`/`iter()` public) — the honest evidence of what
  a frame touched.
- Pixel type: `slint::Rgb8Pixel` (`i-slint-core-1.18.1/graphics/image.rs:162`
  → `api.rs:20` re-export → `slint` via `slint-1.18.1/lib.rs:218`)
  implements `TargetPixel` (`i-slint-renderer-software-1.18.1/
  draw_functions.rs:861`). `PremultipliedRgbaColor` (4 B/px, the
  snapshot path's own format, `draw_functions.rs:874`) is the drop-in
  alternative if 3 B/px proves awkward.
- **Production precedent** (not a testing hack): the linuxkms backend
  renders real frames through exactly this API with its own adapter —
  `i-slint-backend-linuxkms-1.18.1/renderer/sw.rs:218,224,231`
  (`self.renderer.render(buffer, stride)`).

**Mechanism B — `i-slint-backend-testing` + `Window::take_snapshot`
(the golden lane's path; the fallback/cross-check):**

- `TestingBackend` with `renderer_name: Some("software")` embeds the
  same `SoftwareRenderer` in every window adapter it creates
  (`i-slint-backend-testing-1.18.1/testing_backend.rs:195-218`
  `create_window_adapter`; `:733-742` `create_headless_renderer` —
  `"sw"|"software" → SoftwareRenderer::new()`); the dev-pin enables it
  (`renderer-software` feature, root `Cargo.toml:47`).
- `slint::Window::take_snapshot()` (`i-slint-core-1.18.1/src/api.rs:838`)
  is implemented by the software renderer as a **forced full-frame**
  render into a fresh RGBA8 `SharedPixelBuffer`
  (`i-slint-renderer-software-1.18.1/lib.rs:1214-1246`: forces
  `NewBuffer`, renders, un-premultiplies; errors on a 0×0 window).
- **In-repo empirical proof**: the golden visual tier already does
  headless full-frame software renders through this path today —
  `tests/support/driver.rs:154-161` (`snapshot()` → `take_snapshot()`,
  "renderer-presence contract") feeding `tests/golden_visuals.rs`.
  The bench's window at 1366×768 differs from the goldens only in size
  and fixture scale.

**Why not mechanism B for the bench lanes (recorded trade):**
`take_snapshot` is full-frame-only (no partial lane — it forces
`NewBuffer`) and allocates + converts ~8 MB of buffers inside every
call (two 1366×768 buffers), which would sit inside the timed draw
leg. Mechanism A gives both lanes, a pre-allocated steady-state
buffer, and the `PhysicalRegion` evidence. Mechanism B stays as the
stage-2 cross-check (D11): render-once via each mechanism and compare
structurally (same window size, non-empty region) — cheap insurance
that mechanism A's scene is not degenerate.

One platform-coupling fact recorded so stage 2 does not re-derive it:
`mock_elapsed_time` (`testing_backend.rs:50-61`) iterates the testing
backend's **own** window registry (`ALL_TESTING_WINDOWS`) — it does
nothing for windows created on our `NullPlatform`. The bench therefore
cannot borrow the testing backend's "instantiate conditional trees"
helper; D2/D9 explain why that is acceptable (mount's initial refresh
pushes the models; rendering evaluates the tree; linuxkms renders
production frames with no such helper).

### 1.2 Seams re-located at plan time (base `1bcbb5e`, MD-8)

| Anchor (plan-day reference) | Symbol | Verified at `1bcbb5e` |
|---|---|---|
| `test_platform.rs:18-57` | `NullPlatform`, `NullWindowAdapter` (renderer field `:27`), `Platform::create_window_adapter` | unchanged from the audit's citation |
| `test_platform.rs:73` | `on_slint_thread` (worker owns the platform; lib unit tests' entry) | unchanged |
| `test_platform.rs:22-24` | the "never asked to draw (nothing renders)" **doc contract** — no behavioural test pins it (§3 D3) | unchanged |
| `adapter.rs:35` | `REPAINT_CADENCE_MS = 100` (C2/F13-09) | unchanged |
| `adapter.rs:188-196` | `Adapter::refresh` — `sync_view_context` → `state.load()` (`:190`) → `view_state.reconcile` (`:191`) → `models::push` (`:194-195`); `pub(crate)` | unchanged |
| `lib.rs:133` | `pub fn mount(...)` — creates `AppWindow`, installs adapter, starts the repaint timer, first refresh | signature now takes `UiSettings { scale, keymap, startup, kiosk }` (chunk 2 added `kiosk`, `lib.rs:62-65`) |
| `lib.rs:254` | `UiHandle::refresh` — **public**; the bench's refresh-leg driver; delegates to `adapter.refresh` | was `:231` pre-chunk-2 |
| `lib.rs:176-225` | `start_repaint_driver` — the 100 ms repeating timer; **not** measured by the bench (the legs are timer-free by design, D5) | shifted by chunk 2 |
| `models.rs:101-129` | `models::push` — full `VecModel` rebuild per pass (`push_board` builds every column's cards, `:153-198`) | unchanged |
| `ui/compositions/board-layout.slint:52-57` | the board columns are plain `for` in a `VerticalLayout` (inside a `HorizontalLayout`) — **no `ListView` virtualisation**: every card instantiates and renders at every N | — |
| `Cargo.toml:41-50` | dev-deps: `insta`, `uuid`, `proptest`, `serde`, `png`, `i-slint-backend-testing` (+ chunk 5's `tracing-subscriber`) — **no criterion yet** | — |
| root `Cargo.toml:114` | `criterion = { version = "0.8", default-features = false, features = ["cargo_bench_support"] }` — workspace-pinned | — |
| `.github/workflows/ci.yml:337,353` | CI's bench job runs `cargo bench --workspace` inside `bencher run` (rust_criterion adapter) — a new bench target is picked up with **zero workflow edits** | — |

## 2. TDD order (red → green per step; house rules throughout)

All seam tests are unit tests in `src/test_platform.rs`'s own
`#[cfg(test)] mod tests` (the seam's behaviour lives with the seam;
`on_slint_thread` is the runner). Structural assertions only — typed
outcomes and geometry facts, never pixel-image comparisons (the golden
lane owns correctness; K6 non-goal) and never error/log text.

1. **T1 — the adapter registry** (red: no registry exists): after
   `mount(...)` on the null platform, `window_adapter()` returns a live
   adapter whose `window()` is the mounted window (identity of the
   `Window` — compare via `Rc::ptr_eq` on the adapter against a second
   mount creating a second registry entry, or assert the weak upgrades).
2. **T2 — the size seam** (red: `size()` is hard-coded 0×0): a
   `Window::set_size(1366×768)` (the public path,
   `i-slint-core-1.18.1/src/api.rs` `set_size` → the adapter override)
   makes `adapter.size()` report 1366×768; absent a call, the adapter
   stays 0×0 (the existing tests' semantics — the default must not
   change, D3).
3. **T3 — the draw seam, full frame** (red: no render pass-through):
   mounted at 1366×768 with a populated fixture state, a
   `render_into(&mut buffer, 1366)` on a 1366×768 `Rgb8Pixel` buffer
   returns a `PhysicalRegion` whose `bounding_box_size()` is the full
   window (the default `NewBuffer` repaint type draws everything). The
   buffer is caller-allocated; the test also asserts the render did
   touch pixels (some buffer byte differs from the fill value) — a
   degenerate/empty scene fails here, which is exactly the D11
   early-warning.
4. **T4 — the draw seam, dirty tracking** (red: same, exercises the
   `ReusedBuffer` pass-through): after T3's full render, switching the
   adapter's renderer to `ReusedBuffer` and rendering again **with no
   property change** yields a region strictly smaller than the window
   (expected empty — `bounding_box_size()` (0,0) — but pinned as
   `< full area`, the cache-hit contract the partial lane's honesty
   rests on; not an exact-shape pin of renderer internals).
5. **T5 — the bench fixture contract** (red: builder does not exist):
   in `tests/bench_fixture_contract.rs` (a nextest-run target) via
   `#[path]` include of `benches/support/mod.rs` (the storage bench's
   P3/D1 convention): `build_state(n)` yields exactly one live board,
   `STACK_COUNT` stacks, exactly `n` non-deleted tasks round-robinned
   across them, ASCII-only titles, the fixed duedate/done patterns, and
   deterministic ids (no `now_v7()`, no clock reads) — same assertion
   shape as the storage fixture-contract test.
6. **T6 — the bench runs and separates its lanes** (stage-2 smoke, not
   a nextest test): `cargo bench -p taskboard-ui-slint --bench
   refresh_draw -- --quick` (or a saved-baseline run) prints three
   groups — `refresh/{n}`, `draw-full/{n}`, `draw-partial/{n}` — with
   the partial lane reporting the region area alongside (D8). The
   recorded dev-machine numbers (all six cells + region areas) go in
   the stage report + PR body (MD-5).

## 3. Decisions (every open question settled)

### D1 — API verdict & the bench's draw mechanism

Verdict **YES** (§1.1, evidence with file paths). The bench draws via
**mechanism A** (direct `SoftwareRenderer::render` through the extended
seam adapter): both full-frame and partial lanes, pre-allocated buffer
(no allocation inside a timed leg), `PhysicalRegion` as recorded
evidence. **Mechanism B** (testing-backend `take_snapshot`) is kept as
the stage-2 cross-check (render once via each; assert both produce a
full-window, non-degenerate frame) and would become the shipped
full-frame reference under the D11 fallback. No GL/llvmpipe path is
involved anywhere (the stage-3 trap): the software renderer is pure
CPU over a caller buffer — `i-slint-renderer-software-1.18.1/lib.rs`
has no GL dependency, and `i-slint-backend-testing`'s `"software"`
renderer name selects exactly that type (`testing_backend.rs:741`).

### D2 — The seam extension design (what `NullWindowAdapter` gains)

The file stays dev-only, `#[cfg(test)]`-gated in the lib (no public
API, no lib feature, no production-path entanglement — the stage-3
mandate). Changes, all inside `src/test_platform.rs`:

- `size: Cell<PhysicalSize>` field, default `PhysicalSize::new(0, 0)`
  — `size()` returns it (existing tests unaffected, D3).
- `WindowAdapter::set_size(&self, size: WindowSize)` override — the
  public resize path lands here (`api.rs` `Window::set_size` calls the
  adapter's `set_size` directly). The override stores the physical size
  **and dispatches `WindowEvent::Resized` with the logical size**
  (scale factor 1 on this platform), mirroring
  `TestingWindow::set_size` (`testing_backend.rs:512-517`) — that
  dispatch is what resizes the mounted component's root so rendering
  has real geometry. The default trait impl is a no-op
  (`i-slint-core-1.18.1/src/window.rs:122`), so today's behaviour for
  non-callers is unchanged.
- A thread-local **adapter registry**: `create_window_adapter` stores
  the last created `Weak<NullWindowAdapter>`; a
  `pub(crate) fn window_adapter() -> Option<Rc<NullWindowAdapter>>`
  accessor returns the live one (the one-window-per-process reality of
  both the lib tests and the bench; the testing backend keeps the same
  shape via `ALL_TESTING_WINDOWS`).
- Draw pass-throughs on the adapter, thin over the renderer's own
  public API: `render_into(&self, buffer: &mut [impl TargetPixel],
  pixel_stride: usize) -> PhysicalRegion` (delegates to
  `self.renderer.render`, `lib.rs:538`) and
  `set_repaint_buffer_type(&self, RepaintBufferType)` (delegates to
  renderer `lib.rs:498`). Nothing else — no scene inspection, no
  screenshot re-implementation.
- The module doc contract is rewritten honestly: the renderer is
  constructed for **headless draw through the seam's explicit render
  pass-through** (still no implicit draws — nothing renders unless the
  harness calls `render_into`; no event loop; sizes 0×0 until
  `set_size`).

The `take_snapshot` path also becomes available on this platform for
free (the concrete renderer implements the sealed trait method,
renderer `lib.rs:1214`), but the seam does not advertise it — the
golden lane owns snapshots.

### D3 — Which existing tests change, and why that is honest

**None change functionally.** Grep evidence at plan time
(`grep -rn "never asked to draw|never draws|take_snapshot|render_by_line|
render_to" crates/taskboard-ui-slint/ --include="*.rs"`): the only
occurrence of the "never asked to draw" contract is the **doc comment**
`test_platform.rs:22-24`; no test asserts a no-draw behaviour (the
journey/golden binaries use the *testing* backend, not this platform).
What changes is therefore: (a) that doc comment (D2), and (b) nothing
else — the adapter's default size stays 0×0, `set_size` is never called
by existing tests (they drive the markup's `window_width_changed`
callback instead), and `create_window_adapter`'s registry write is
invisible to them. This directly answers the stage-3 trap ("a weakening
of the 'never asked to draw' tests without the recorded evidence"):
there is no such test to weaken, and this section is the recorded
evidence. The four existing `mount_tests`/`font_smoke_tests`/
`keymap_adapter_tests`/`view_component_tests` behaviours all stay
green unmodified.

### D4 — Bench form: a criterion target (not a harness script)

The crate's dev-deps allow criterion (workspace-pinned, root
`Cargo.toml:114`; benches see dev-deps), and the measurement needs
per-leg timing with an N-matrix and unmeasured per-iteration setup —
a script cannot see in-process legs. Registration mirrors the storage
precedent exactly (`crates/taskboard-storage-sqlite/Cargo.toml:27-31`):
`criterion.workspace = true` under `[dev-dependencies]` plus
`[[bench]] name = "refresh_draw", harness = false`. CI/Bencher pick it
up with zero workflow edits (`.github/workflows/ci.yml:337,353` already
run `cargo bench --workspace` inside `bencher run`; the rust_criterion
adapter parses criterion output). Numbers are references, never gates
(MD-5); no CI budget assertion exists or is added (K6's non-goal,
S2's own framing).

### D5 — Refresh-leg instrumentation points

The leg is driven through the **public** `UiHandle::refresh()`
(`lib.rs:254`) — no production code changes, no `pub(crate)` widening.
That method is the entire audit-defined refresh pass
(`adapter.rs:188-196`): `sync_view_context` → `ArcSwap::load()`
(`:190`) → `view_state.reconcile` (`:191`) → `shell.chain()` +
`chrono::Utc::now()` → `models::push` full VecModel rebuild
(`:194-195`, `models.rs:101-129`). The real clock stays in (it is the
production path; its variance is µs-scale against a ms-scale budget).
The 100 ms timer is **not** part of the leg — the bench calls refresh
directly, exactly as the unit tests do; cadence composition is C2's
recorded note, not this chunk's measurement.

### D6 — Draw-leg mechanism

- **Full-frame lane**: default `NewBuffer` repaint type; per bench
  function a pre-allocated `Vec<slint::Rgb8Pixel>` of exactly
  1366×768 (stride 1366) lives outside the measured closure; each
  iteration calls `adapter.render_into(&mut buffer, 1366)` and the
  returned `PhysicalRegion`'s `bounding_box_size()` is captured
  (unmeasured) for the report — a structural guard that the lane
  really measured a full frame.
- **Partial lane**: `set_repaint_buffer_type(ReusedBuffer)`; the same
  buffer persists across iterations (the kiosk steady-state
  framebuffer model — `ReusedBuffer`'s documented contract,
  `partial_renderer.rs:388-391`). Per iteration, **unmeasured setup**
  (criterion `iter_batched`, `BatchSize::PerIteration`): mutate one
  task's `done` flag (rotating index) in a fresh `Arc<AppState>`,
  `ArcSwap::store`, then `UiHandle::refresh()`; **measured routine**:
  one `render_into` + region capture. The reported region area is the
  honest finding-carrier: if `models::push`'s whole-`VecModel`
  replacement dirties the entire board every refresh, the partial lane
  will show full-window regions and near-full-frame times — that is a
  **recorded finding** (feeds C2/F13-09 and the spike's
  event-driven-redraw question), never hidden and never "fixed" by
  weakening the lane.
- `take_snapshot` (mechanism B) is not used inside any timed lane
  (allocation + conversion noise, full-frame only — §1.1 trade).

### D7 — Fixture scale and shape

`benches/support/mod.rs` builds an `AppState` (the type `mount`
consumes) with the storage bench's deterministic conventions
(`crates/taskboard-storage-sqlite/benches/support/mod.rs` is the
reference): `N_SIZES = [100, 1_000, 10_000]`; `STACK_COUNT = 4`
(round-robin); disjoint `Uuid::from_u128` bases per entity kind;
fixed epoch stamps (`EPOCH_SECS` 2024-01-01) — **no `now_v7()`, no
clock reads in the builder**; ASCII-only titles (`Task {i:05} bench
fixture` — the golden lane's R7/R8 rule, so software text layout never
falls back to a system font for a missing glyph); `duedate: i%3` and
`done: i%10` fixed-stamp patterns mirrored from hot_path (realistic
due-phrase work per card; the overdue banner is capped at
`BANNER_ROW_CAP = 3` rows, `models.rs:30` — constant cost, and its
wall-clock-relative phrase *text* is never asserted anywhere). Board +
stacks + tasks only: labels, outbox ops, reminder rules and validators
have no UI-model surface (`task_card_model`, `models.rs:321-338`,
reads none of them) and stay out. The builder is shared with the
fixture-contract test via `#[path]` (storage P3/D1 precedent; a
`tests/` target cannot import a bench target's module any other way).

### D8 — Two-lane timing separation (and the third, partial, lane)

Three criterion bench functions in one group family, never merged into
one number (the stage-3 trap: "one combined refresh+draw number"):

- `refresh/{n}` — `b.iter(|| handle.refresh())`;
- `draw-full/{n}` — mount-then-render steady state as in D6;
- `draw-partial/{n}` — `iter_batched(mutate+store+refresh, render, …)`
  as in D6, region area reported.

Setup (mount, first refresh, `set_size(1366×768)`, buffer alloc,
repaint-type selection) is outside every measured closure. Per-N group
configuration mirrors hot_path's `configure_group`: the 10 000 tier
drops to `sample_size(10)` + `measurement_time(15 s)` (a full-frame
software render over 10 000 cards is expected in the 10² ms range on a
dev machine; the bound keeps the bench minutes-scale). Throughput is
`Throughput::Elements(n)` like hot_path. The window is the row-3
budget's own 1366×768, mounted **windowed** (`UiSettings::default()`,
`kiosk: false`): the budget row is stated in software-renderer terms
at a size, not in fullscreen-kiosk terms, and fullscreen is a no-op on
this adapter anyway (D10).

### D9 — Thread model for the bench process

Slint's platform is process-global and thread-affine. Criterion 0.8
(`default-features = false`, no rayon) runs every bench function and
every measured iteration **on the main thread**, so the bench registers
`NullPlatform` once on the main thread via a `OnceLock` guard called at
the top of each bench function (the same lazy-once shape as
`on_slint_thread`'s `SENDER`, `test_platform.rs:77-91`) and never uses
the worker. `on_slint_thread` itself stays for the lib's unit tests
(and is `#[allow(dead_code)]`-annotated at the bench's `#[path]`
include site, with a comment naming why — the include is the seam's
single source of truth; duplicating the adapter would drift).
**Recorded fallback** (risk R3): if stage 2 observes an affinity panic
(criterion hopping threads in some configuration), the fix is routing
each measured closure through `on_slint_thread` — the mechanism ships
in the same file — plus a control measurement of the channel round-trip
(empty closure through the same path) reported alongside so the
added overhead is visible, not silent.

### D10 — Interactions with the merged chunks

- **Chunk 2 (kiosk profile)**: `UiSettings.kiosk` defaults false; the
  bench mounts windowed. `set_fullscreen(true)` on this adapter is
  inert (no `WindowAdapter` fullscreen surface), so the kiosk flag
  would change nothing measurable — the default is the honest choice
  and one fewer variable.
- **Chunk 5 (first-frame event, JSON logs)**: irrelevant in-process
  (the bench reads no logs; the mount-tail `taskboard_perf` event goes
  to an unset subscriber — a disabled-dispatcher no-op, the hot_path
  D9 note's exact stance).
- **The board markup has no `ListView`** (§1.2 table): every card
  renders at every N — the bench's draw numbers are the true
  whole-board cost, which is the row-3 question. (A future
  virtualisation would change what N means; that is out of scope and
  untouched.)

### D11 — Blocker-fallback plan (moot on the verdict, live for stage 2)

Per K6/MD-5/F13-03: an API-blocked or degenerate draw leg is a
**complete outcome** when evidenced. The verdict says the API is not
blocked, but stage 2 must still prove the *scene* is real. Fallback
trigger: T3 fails or the mechanism-B cross-check diverges (A renders
empty/degenerate while B — the golden-proven path — renders), and the
cause is not fixable within the seam (e.g. conditional trees provably
cannot instantiate under a non-testing platform — considered unlikely
given the linuxkms production precedent, §1.1). Then: ship the refresh
leg + the seam extension + mechanism B's `take_snapshot` number as the
full-frame draw reference, record the exact evidence (failing
assertion + divergence) in the stage report and PR body, and **#300 is
annotated, not closed** (stage 3 posts the one progress comment, MD-4).
What is never acceptable: a silent cut, a lane renamed to hide the
fallback, or a weakened T3.

### D12 — Gates, hygiene, bookkeeping

- Full AGENTS §10 suite. The bench target compiles under
  `cargo clippy --workspace --all-targets -D warnings` (the bench is
  clippy-clean or fixed); `nextest` does not execute bench targets;
  `cargo test --doc` unaffected; llvm-cov sequence unaffected (benches
  are not instrumented targets for nextest coverage).
- New `.rs` files carry `// SPDX-License-Identifier: AGPL-3.0-only`
  (AGPL crate); `unsafe` stays absent (the crate's `deny` stands; the
  seam and bench need none — the software renderer's own internals are
  out of our tree).
- CHANGELOG `Unreleased → Added`: one line for the bench + seam (the
  stage-2 prompt's bookkeeping).
- No tracker edits (MD-4): #300's closure/annotation rides stage 3's
  PR body / comment. No new issues unless stage 2 finds out-of-scope
  gold (then: chunk stage report, the user promotes).
- Commit hygiene: standalone commits, `feat(ui): …` /
  `test(ui): …` / `docs(changelog): …` shapes, rebase-ready (§8).

### D13 — File-by-file change sheet & estimated diff

| File | Change | Est. lines |
|---|---|---|
| `crates/taskboard-ui-slint/src/test_platform.rs` | D2 extension (size cell, `set_size`, registry, render pass-throughs, doc rewrite) + `#[cfg(test)] mod tests` T1–T4 | +130–170 |
| `crates/taskboard-ui-slint/benches/support/mod.rs` | D7 fixture builder (`AppState`, hot_path conventions), shared constants, platform guard (D9), the `#[path]` seam include | +100–130 |
| `crates/taskboard-ui-slint/benches/refresh_draw.rs` | the criterion bench: three lanes × N, group config, region reporting, setup | +160–200 |
| `crates/taskboard-ui-slint/tests/bench_fixture_contract.rs` | T5 contract test over the `#[path]`-included builder | +50–70 |
| `crates/taskboard-ui-slint/Cargo.toml` | `criterion.workspace = true` dev-dep + `[[bench]]` block | +5 |
| `CHANGELOG.md` | Unreleased line | +3–4 |

Estimated total: **≈ +450–580 lines across 6 files**, zero production
behaviour changes (the lib's shipped surface is untouched —
`test_platform.rs` is `#[cfg(test)]`; `Cargo.toml` gains a dev-dep and
a bench target only).

## 4. Implementation order (stage 2)

1. `test_platform.rs` seam extension, TDD: T1 → T2 → T3 → T4, one
   commit per red→green step (or one seam commit with the four tests —
   stage 2's judgement, commits standalone either way).
2. `benches/support/mod.rs` builder + T5 contract test (red → green).
3. `Cargo.toml` registration; `benches/refresh_draw.rs` lanes; T6
   smoke run (`--quick`), then the full recorded run per N.
4. The mechanism-B cross-check (D11): one structural comparison
   inside the bench's setup path or as a one-off dev check recorded in
   the report (not a lane; it is evidence, not a budget number).
5. Full AGENTS §10 gate suite; CHANGELOG line; numbers → report + PR
   body (MD-5). No push, no PR (stage 3 owns those).

## 5. Risks

1. **Conditional trees / change handlers under a non-testing platform**
   (the `mock_elapsed_time` coupling, §1.1): markup `changed` handlers
   (the responsive width push) do not run without a handler pass —
   acceptable *by construction* here: the bench wants the Regular
   breakpoint at 1366 logical px, which is the `Responsive` default;
   the golden lane's compact/expansive cases are not this chunk's
   subject. Rendering itself evaluates the item tree (linuxkms
   precedent). Residual risk: some conditional never instantiates →
   T3's touched-pixels assertion catches it → D11. Likelihood low,
   impact bounded to the fallback.
2. **The partial lane measures full-frame dirt** (`models::push`
   replaces every `ModelRc` each refresh): not a bug — the recorded
   finding (D6), feeding C2/F13-09 and the spike. The lane's honesty
   rests on T4's cache-hit pin, not on the region being small.
3. **Criterion thread affinity**: expected same-thread (D9); fallback
   shipped in the same file with a control measurement. Evidence
   threshold: any "platform used from another thread" panic.
4. **10k-tier runtime**: full-frame renders may be slow; bounded by
   `sample_size(10)`/`measurement_time(15 s)`; if a single iteration
   exceeds seconds, stage 2 records the wall time and (only if
   pathological, e.g. > 5 s/iter) drops the 10k draw tiers to
   `measurement_time(10 s)` with the deviation recorded — never
   silently.
5. **`Rgb8Pixel` ergonomics** (3 B/px slice): if the re-export chain
   misbehaves for `Vec<Rgb8Pixel>` construction, `PremultipliedRgbaColor`
   is the drop-in (D6/§1.1); buffer bytes-per-pixel changes the
   allocation size only, not the lanes' meaning.

## 6. Acceptance checklist (stage 2/3)

- [ ] Seam extension lands with T1–T4 green; **no existing test
      modified** (D3 — if stage 2 believes one must change, that is a
      stop-and-report, not an edit).
- [ ] T5 contract test green; builder deterministic (no clock, no rng).
- [ ] Bench prints three separated lanes × three Ns; region areas
      reported for the draw lanes; numbers recorded (report + PR body).
- [ ] Mechanism-B cross-check recorded (D11) or the fallback executed
      with evidence.
- [ ] Row-3 budget context stated in the bench's module docs; no
      assertion on the numbers anywhere (references, never gates).
- [ ] Full AGENTS §10 gates green; CHANGELOG line present; no tracker
      edits; no push from stage 2.
- [ ] Stage 3: rebase, review `git diff origin/main...HEAD`, re-run
      gates + bench, ship PR — "Closes #300" if the draw leg works,
      else the annotation with evidence (MD-4/MD-5); teardown the
      worktree.

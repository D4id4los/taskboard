# [Plan] Unchanged-refresh guard (#463) — chunk 1.3, backlog round 2

- **Date**: 2026-10-31
- **Kind**: stage-1 implementation plan for dispatch chunk **1.3**
  (#463, branch `perf/ui-unchanged-refresh-guard`, batch 1 — opens
  the render lane §1 lane 1). TDD-first; no production code in this
  commit.
- **Origins** (normative, in precedence order):
  issue **#463** (the ask: O(1) unchanged-guard atop the refresh
  pass; cached `Arc` beside the adapter's cross-refresh state; pin
  via the diff mutation counter *or an equivalent typed seam —
  stage 1 decides*; journeys/keyboard suites untouched);
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §1 lane 1 + chunk 1.3 (acceptance: guard at the top of the refresh
  pass; the unchanged pin; every existing journey/keyboard/golden
  test green **unmodified**; the 100 ms cadence cost at board scale
  drops to ~0 when idle — *argued from the pin, not timed*; review
  traps: compare the **loaded snapshot's** Arc; load-bearing pin;
  mount/first-refresh handled);
  `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`
  §3.7 + §3.7's subject numbers (refresh lane 56.6 µs / 774 µs /
  20.0 ms at N = 100 / 1 000 / 10 000, unchanged since the
  2026-10-26 re-measure — the cost this guard removes from every
  no-op pass).
- **Branch / worktree**: `perf/ui-unchanged-refresh-guard` in
  `../taskboard-WT-perf-ui-unchanged-refresh-guard`, branched off
  `main` = `origin/main` @ `3fb69d1b` (2026-10-31; verified equal —
  every seam below was read at the branch base).
- **Status**: **READY** — all seams verified; the refresh-caller
  audit (§5) is exhaustive over the branch base; two correctness
  findings reshape the issue's bare-`ptr_eq` sketch (§4 D2/D3) while
  keeping its O(1) contract.

---

## 1. Mission & binding constraints

Skip the refresh pass's derive+diff work (`ViewState::reconcile` +
`models::push` + the live-menu re-grey — the 56.6 µs–20 ms lane)
when the loaded `AppState` snapshot is the exact `Arc` the models
already carry. A miss (the engine stores an identical-values state)
costs one wasted pass, never a stale UI — the pointer changes exactly
when the engine publishes.

Binding, restated against the read seams:

1. **The guard compares the *loaded snapshot's* Arc** —
   `self.state.load()` first, `Arc::ptr_eq` against the cache second
   (#463; the dispatch review trap). Never cache the load and diff
   anything values-based: `AppState` has no `PartialEq` contract at
   this seam and must not grow one.
2. **Effect-driven refreshes pass the guard naturally**: every engine
   signal follows a fresh publish — `taskboard-state/src/actor.rs:600`
   `shared.store(Arc::new(core.app()))` then
   `signals.send(EngineSignal::StateUpdated)` — so a signal-driven
   dirty tick always carries a new pointer.
3. **The driver's other dirty sources do NOT move the pointer** and
   are load-bearing today (lib.rs:470-559): the wall-minute boundary
   (time-derived badges, due-soon sets, day window), the system-event
   drain (connector liveness lives in the adapter's
   `SettingsVm.liveness`, adapter.rs:2024-2031), the delivery-health
   watch (the chip push inside `drain_delivery_health`,
   adapter.rs:2066-2075), and the settings-reply drain (the keymap
   live-swap, adapter.rs:2119-2123). The guard must not swallow what
   these exist to deliver (§4 D3/D4).
4. **View-state mutations never move the pointer**: board selection,
   flows, focus, widths, mode, buffers, picker state are all
   adapter-side (`AppState` is pure domain data —
   `taskboard-domain/src/state.rs:25-55`), yet dozens of dispatch
   paths end in a refresh that must re-derive (§5 audit).
5. **The §8/W-SM-c exactness invariant rides every pass**: the due
   editor's buffers never outlive the flow
   (`non-empty ⇔ Flow::EditDate armed`, adapter.rs:608-633). A flow
   can end **without** outcomes (the modal settle,
   `settle_foreign_flow_for_modal`) or without any state change —
   pinned by `flow_table_tests.rs:1008-1016`, where `discard_edit()`
   is followed by a plain refresh that must wipe. The guarded pass
   therefore keeps a cheap tail (§4 D2) — a bare early `return` at
   the top of `refresh` breaks this test.
6. **AGENTS §5 discipline**: typed outcomes, no timing assertions, no
   text assertions, one behavior one test at the lowest layer, no
   quality ceilings; the pin must be load-bearing (dispatch review
   trap — break the guard, watch it fail).
7. **Journeys/keyboard/golden suites untouched** (#463): zero edits
   to existing tests; the guard may only *strengthen* what they
   observe (D9's zero-notification property still holds — a skipped
   pass trivially notifies nothing).

## 2. The refresh pass anatomy (seams read, at base `3fb69d1b`)

```
UiHandle::refresh            lib.rs:671  (public; the driver's tests + external callers)
  └─ Adapter::refresh        adapter.rs:604
       ├─ sync_view_context  adapter.rs:605   (shell.view/focus ← view_state; O(1))
       ├─ state.load()       adapter.rs:606   (ArcSwap load — the Arc this guard caches)
       ├─ reconcile(&state)  adapter.rs:607   (view_state vs state; part of the measured lane)
       ├─ due-buffer wipe    adapter.rs:608-633  (§8 exactness; debug_assert)
       ├─ models::push(...)  adapter.rs:645-657  (THE derive+diff; 56.6 µs–20 ms; sums
       │                                        diff_rows mutations, traces them at
       │                                        models.rs:746 — never stored)
       ├─ menu re-grey       adapter.rs:661-671  (only while the menu holds)
       └─ push_settings      adapter.rs:672   (settings rows fold liveness + replies;
                                               bounded-tiny diff)
```

Callers of the pass (exhaustive; production):

| # | Site | What moved since the last pass | Pointer |
|---|------|-------------------------------|---------|
| 1 | lib.rs:331 — `mount`'s first-frame pass | — (models empty) | cache is `None` |
| 2 | lib.rs:546 — driver tick, signal-drain dirty | engine publish (`actor.rs:600`) | **new** |
| 3 | lib.rs:546 — driver tick, system-event-only dirty | `SettingsVm.liveness` | same |
| 4 | lib.rs:546 — driver tick, delivery-health dirty | chip pushed by the drain itself (adapter.rs:2066) | same |
| 5 | lib.rs:546 — driver tick, **minute boundary** | wall clock (badges/day window/due-soon) | same |
| 6 | lib.rs:673 — `UiHandle::refresh` (tests) | — | either |
| 7 | lib.rs:718 — `observe_connector_episode` | liveness map (+ `push_notice` outside refresh) | same |
| 8 | adapter.rs:552 — `apply_outcomes` `FlowOutcome::RefreshNow` | view machine's re-derive verdict: flow arms/settles, selections, mode flips, width commits, drag teardown, palette jump/close | same |
| 9 | adapter.rs:416 — `intercept` Down-in-field (picker open) | `picker_focus_on_mount` one-shot | same |
| 10 | adapter.rs:2175 — `drain_setting_replies` tail | keymap swap / reply settle | same |
| 11 | adapter.rs:3289 — `on_board_stack_width_resize` (drag) | `view_state.set_stack_width` | same |
| 12 | adapter.rs:4422 — `on_task_date_picker_toggled` (close) | `due_picker.take()` | same |
| 13 | adapter.rs:4549 — `apply_picker_key` | `due_picker` move/step | same |
| 14 | adapter.rs:4578 — `open_due_picker` | `due_picker` arm + focus flag | same |
| 15 | adapter.rs:4643 — `on_banner_row_activated` | `view_state.set_focused` + `reminder_target` | same |

(The live-echo text seam, adapter.rs:4392, deliberately does *not*
refresh — untouched. `run_alarm_pass` and `push_notice` never go
through refresh — untouched.)

The pin seam candidate from #463 — "the diff mutation counter" — is
`diff_rows`' return value (models.rs:273-295), summed per surface
inside `models::push` and only ever **traced** (models.rs:746); it is
not stored anywhere, and under the guard it would read 0 on a skipped
pass *and* on a D9-derived zero-mutation pass — it cannot distinguish
"no work" from "work that mutated nothing". §4 D6 picks the typed
equivalent the issue delegates to stage 1.

## 3. Non-goals

- No change-set plumbing, no dirty-marking, no `PartialEq` on
  `AppState` (#463's architecture-preserving fence).
- No re-engine of the #390 refresh-lane *regression* (56.6 µs vs the
  27.2 µs baseline at N=100 — report §3.7): the guard removes whole
  passes; the per-pass cost curve is that report's separate finding.
- No time-bucket logic inside the guard: the minute boundary stays
  the driver's explicit decision (§4 D4) — one line at the call site,
  no hidden clock reads in `refresh`.
- No `UiHandle`-level public force API: forcing is an adapter-internal
  concern; tests reach the adapter through the existing
  `test_adapter`/rig seams.

## 4. Design decisions

### D1 — The cached Arc lives on the `Adapter`, beside the D2 id tables

```rust
// adapter.rs, Adapter struct (beside `tables`, adapter.rs:193)
/// The last snapshot the derive+diff materialized onto the models
/// (the unchanged-refresh guard, #463): `Some` after every derived
/// pass. `Arc::ptr_eq` against the fresh load ⇒ the models already
/// carry this exact snapshot — the pass's expensive half would be a
/// no-op. A miss (the engine republished identical values) costs one
/// wasted pass, never a stale UI.
last_derived: Option<Arc<AppState>>,
```

- `None` at install; assigned at the **end** of every derived pass
  (after `push_settings` — "materialized" means the work completed).
  The cache holds one extra refcount on the current (or the one
  superseded-but-unobserved) snapshot — no content clone, bounded by
  one pass.
- Rationale for the Adapter (not `UiHandle`): every producer *and*
  consumer of the pass funnels through `Adapter::refresh`
  (lib.rs:546/673/718 all borrow the adapter), and the issue's own
  sketch puts it "beside the adapter's other cross-refresh state".

### D2 — The guard splits `refresh` into a guarded front and a forced body; the cheap tail survives the skip

The issue sketches "return before reconcile/push". The audit (§2)
shows two reasons a bare top-of-body `return` is wrong:

- **The §8 wipe must run on every pass** — a flow can end without
  outcomes (the modal settle) or without a state change, and
  `flow_table_tests.rs:1008-1016` pins exactly that shape against a
  *plain* refresh.
- **The settings surface folds non-state inputs** — liveness
  (callers #3/#7) and settled replies land via `push_settings`;
  swallowing it freezes connector rows.

Shape (the exact condition is two O(1) checks):

```rust
/// The D1 refresh, guarded (#463): the derive+diff half
/// (reconcile, `models::push`, the live-menu re-grey) runs only
/// when the loaded snapshot is not the one the models already
/// carry. The cheap half — the §8 buffer wipe and the settings
/// push — runs on every call: flows can end without outcomes, and
/// the settings rows fold the liveness map and the settled
/// replies, none of which live in the state Arc.
///
/// Invariant for future callers: anything the derive reads that is
/// NOT the state Arc (view_state, shell, keymap, due/picker state,
/// the wall clock) must reach this method via [`Self::refresh_forced`]
/// or behind a state change — the audit table in
/// `.artifacts/plans/2026-10-31-unchanged-refresh-guard-plan.md` §5
/// is the reference classification.
pub(crate) fn refresh(&mut self, window: &AppWindow) {
    let state = self.state.load();
    let unchanged = self
        .last_derived
        .as_ref()
        .is_some_and(|derived| Arc::ptr_eq(derived, &state));
    if unchanged {
        self.enforce_due_buffer_invariant(window);
        self.push_settings(window);
        return;
    }
    self.refresh_forced(window);
}

/// The unguarded D1 pass: reconcile + full re-derive + the cheap
/// tail. Every caller whose inputs moved outside the state Arc
/// calls this directly (the §5 audit's forced set).
pub(crate) fn refresh_forced(&mut self, window: &AppWindow) {
    // — the current `refresh` body, unchanged, except: —
    //  * the wipe block (adapter.rs:615-633) becomes
    //    `self.enforce_due_buffer_invariant(window)` (factored, one
    //    implementation, both paths);
    //  * after `push_settings`, the cache closes the pass:
    //    `self.last_derived = Some(state);`  (`state` is the
    //    `Arc` from the load at adapter.rs:606; `Arc::clone`, not
    //    `load_full` twice).
}
```

`enforce_due_buffer_invariant` is the existing wipe block verbatim
(the `!matches!(Flow::EditDate)` clear of `due_buffer`/`due_echo`/
`due_picker`/`shell.date_picker_grid`/the window global, plus its
`debug_assert`) — factored so both paths share one implementation
and the debug_assert guards both. `sync_view_context` stays at the
top of the forced body only: a guarded pass has, by construction,
an unchanged view_state (every view mutation force-derives, §5), so
the sync would be a no-op there.

The menu re-grey stays inside the forced body: its inputs (state,
flow) are unchanged on a guarded pass, so skipping it is provably a
no-op; on a forced pass it must rebuild (keymap swap, state change
while the menu holds).

### D3 — `apply_outcomes`' `RefreshNow` arm becomes the forced choke point

`FlowOutcome::RefreshNow` is the view machine's own "the models must
re-derive" verdict — every flow arm/settle, selection, mode flip,
width commit, and drag teardown lands in that one arm
(adapter.rs:552). Changing it to `self.refresh_forced(window)`
covers audit rows #8 at a single site. The subsequent engine signal
for the same user action (command → publish → signal) then finds the
new pointer and derives again — exactly today's two-pass shape, no
regression.

### D4 — The driver forces on the minute boundary, guards the rest

```rust
// lib.rs:545-547 becomes:
if dirty {
    if minute_boundary {
        adapter.borrow_mut().refresh_forced(&window);
    } else {
        adapter.borrow_mut().refresh(&window);
    }
}
```

- Minute boundary (row #5): the wall clock moved, the pointer did
  not — the derive must run or every time-derived row freezes until
  the next state change. Today's behavior (boundary ⇒ dirty ⇒ full
  pass) is preserved exactly; the guard adds nothing to its cost.
- Signal tick (row #2): new pointer ⇒ passes the guard naturally.
- System-event tick (row #3) / delivery-health tick (row #4):
  guarded skip is *correct* — the chip is pushed by the drain
  (adapter.rs:2072-2073), the notice by `push_notice`
  (lib.rs:544, runs every tick outside refresh), the liveness rows
  by the cheap tail's `push_settings`.
- The kiosk rotation flip rides `SetMode`/`interpret` → outcomes →
  D3's forced arm (verified: the `SetMode` arms route through
  `interpret`, adapter.rs:912-975) — no separate site.

### D5 — Initial-pass and mount story

`last_derived: None` at install ⇒ the guard's `is_some_and` is false
⇒ the first pass derives. `mount`'s first-frame refresh (lib.rs:331)
is therefore correct with zero new code — the models are empty before
it, and the mount test (lib.rs:329-341) keeps pinning that. The
hand-over's `UiSettings` width seeding (lib.rs:2908, "applied before
the first refresh") is view_state-side and untouched.

### D6 — The pin: a `cfg(test)` derive-pass counter, not the mutation sum

```rust
// Adapter field (the `quit_requests` precedent, adapter.rs:241):
/// The test seam (#463's pin): derive+diff passes executed since
/// install. The guard's contract in one integer — incremented only
/// where `models::push` is about to run.
#[cfg(test)]
pub(crate) derive_passes: usize,
```

Incremented in `refresh_forced` immediately before
`crate::models::push` (the single derive entry). Why not the issue's
"diff mutation counter": it is trace-only (models.rs:746) and
degenerate — 0 on a skipped pass *and* on an unchanged D9 pass, so it
cannot witness "no work happened". The pass counter can: the pin
asserts the integer *stalls* across an unchanged second refresh and
*advances* exactly once per state change. Typed, deterministic,
zero timing. `cfg(test)` keeps it out of the shipped type (the
journeys compile the lib without `cfg(test)` and need nothing here).

### D7 — Naming: `refresh` stays, `refresh_forced` is the new explicit verb

`refresh` keeps its name and signature — every existing caller
compiles untouched, and the *default* semantics (guarded) are the
safe ones for future callers. The forced variant is spelled out at
each audit-mandated site, so a reader of any call site sees which
contract it chose. (Rejected: a `force: bool` parameter — boolean-flag
callsites hide the contract the audit exists to make visible.)

## 5. The refresh-caller audit — final classification

| Row (§2) | Caller | Classification | Action |
|---|---|---|---|
| 1 | `mount` first-frame pass | passes naturally (cache `None`, D5) | none |
| 2 | driver, signal-drain dirty | **effect-driven** — engine publish moved the pointer | stays on `refresh` |
| 3 | driver, system-event-only dirty | **effect-driven** for its actual inputs (liveness) — the cheap tail delivers them | stays on `refresh` |
| 4 | driver, delivery-health dirty | **effect-driven** — the drain pushes the chip itself | stays on `refresh` |
| 5 | driver, minute boundary | **needs a forced pass** — pure time passage | D4's `refresh_forced` |
| 6 | `UiHandle::refresh` (tests) | guarded — the pin's subject | stays on `refresh` |
| 7 | `observe_connector_episode` | **effect-driven** (liveness → cheap tail; notice → `push_notice`) | stays on `refresh` |
| 8 | `apply_outcomes` `RefreshNow` | **needs a forced pass** — the view machine's re-derive verdict | D3's `refresh_forced` |
| 9 | `intercept` picker-focus | **needs a forced pass** — one-shot adapter flag the due push consumes | `refresh_forced` |
| 10 | `drain_setting_replies` tail | **needs a forced pass** — the keymap swap must re-derive menu pages/hints; a changed scale re-pushes widths at the new scale | `refresh_forced` (the explicit `push_settings` at 2174 becomes redundant — refresh_forced ends in it — and is dropped with a one-line note) |
| 11 | `on_board_stack_width_resize` | **needs a forced pass** — view_state width | `refresh_forced` |
| 12 | `on_task_date_picker_toggled` (close) | **needs a forced pass** — picker state | `refresh_forced` |
| 13 | `apply_picker_key` | **needs a forced pass** — picker state | `refresh_forced` |
| 14 | `open_due_picker` | **needs a forced pass** — picker state + focus flag | `refresh_forced` |
| 15 | `on_banner_row_activated` | **needs a forced pass** — focus + one-shot target | `refresh_forced` |

Effect-driven set: rows 1, 2, 3, 4, 6, 7. Needs-forced set: rows 5,
8–10 (three call-site edits: D3's arm, the driver, the reply tail)
plus the six mechanical swaps 9, 11–15. Every forced site is a
direct `refresh(` call → a one-token change; the audit found no
production path that mutates a derive input and reaches the pass any
other way.

## 6. TDD plan (test-first; all in-crate unit tests, `flow_table_tests`-style rig)

The rig pattern (`flow_table_tests.rs:641-674`: `on_slint_thread` +
`mount` + `test_adapter`) owns every pin; no journey changes.

**T1 — `the_guard_skips_the_derive_while_the_snapshot_is_unchanged`**
(the load-bearing pin; write FIRST, watch it fail against the
unguarded code by asserting exact counts).

1. `rig(seed_state())` → mount's first pass ran: `derive_passes == 1`
   (T5's initial-pass story, asserted here for free).
2. `handle.refresh()` → `derive_passes` **still 1** — unchanged
   snapshot, skipped.
3. Store a new `Arc` (the harness's `ArcSwap`):
   `handle.test_state().store(Arc::new(mutated))`,
   `handle.refresh()` → `derive_passes == 2` — a publish derives.
4. `handle.refresh()` again → **still 2**.
5. Store an identical-*values* new `Arc` (clone the pre-mutation
   state), `handle.refresh()` → `derive_passes == 3` — a miss costs
   one wasted pass, never staleness (the issue's own contract,
   asserted).
   Load-bearing proof: delete the guard (refresh always delegates to
   the forced body) → steps 2 and 4 read 2 and 3 → fail.

**T2 — `a_forced_refresh_derives_on_an_unchanged_snapshot`**

After the mount pass, call `adapter.refresh_forced(window)` directly
→ `derive_passes == 2` with the pointer unchanged. Pins D2's other
half: the forced verb really does bypass the cache.

**T3 — `a_view_intent_derives_on_an_unchanged_snapshot`**

Through the production entrance, not the verb: mount-refresh, then
`interpret_for_test(window, UiEffect::ShowTask)` (or a board-select
effect) with the pointer unchanged → the arm's `RefreshNow` forces →
`derive_passes == 2` **and** the affected surface's models actually
changed (typed: the view-context/focused-task accessor or the pushed
row). Pins D3; breaking the `RefreshNow` arm back to plain `refresh`
fails it (the pass stalls and the models never show the target).

**T4 — `an_unchanged_refresh_still_pushes_the_settings_liveness`**

Mount-refresh; inject `SystemEvent::ConnectorLost` via
`observe_connector_episode` (pointer unchanged; the guard skips the
derive) → assert the settings connector row's **status enum** flipped
(`ConnectionRow.status` off the `SettingsModel` rows — typed, the
established connector-episode test shape; never the chip/phrase
text). Pins the cheap tail (D2) and rows #3/#7: the skip must not
freeze the settings surface. Also asserts `derive_passes` stalled —
the liveness delivery provably did not need the derive.

**T5 — `the_first_refresh_derives_from_the_none_cache`**

Folded into T1 step 1 (one behavior, one test): the `None` initial
cache and mount's first-frame pass are the same fact; no separate
mount test.

**T6 — existing suites, unmodified, must stay green** (the
acceptance's strongest clause): `flow_table_tests` (notably
`..._survive_a_refresh` / `..._die_on_a_settle` — the wipe now rides
the cheap tail), the D9 zero-notification pins, the keyboard
journeys, the driver health test (lib.rs:872 — its
drain-then-`handle.refresh()` shape lands on the guarded path; the
chip assertions hold because the drain pushes them). Any red test is
a bug in the guard design, not a test to weaken (AGENTS §5).

Test-order note: T1/T2 compile only after the D1/D6 fields exist —
write the fields + counters as the test scaffold first (red),
then the guard (green).

## 7. Implementation checklist (stage 2, in order)

1. Scaffold: `last_derived` field (D1), `derive_passes` field (D6),
   both init in `install`; T1 + T2 written → red (T1's step-2
   assertion fails against the unguarded body).
2. Factor `enforce_due_buffer_invariant` out of the wipe block
   (adapter.rs:615-633); split `refresh` into guarded front +
   `refresh_forced` body (D2); cache assignment at the forced body's
   end; counter increment before `models::push`. T1, T2 green;
   `flow_table_tests` green.
3. D3 (`RefreshNow` arm), D4 (driver minute arm), row-10 reply tail,
   rows 9/11-15 swaps. T3, T4 green.
4. Doc comments: the two refresh methods (D2's invariant text), the
   driver's minute comment (lib.rs:534), `UiHandle::refresh`
   (lib.rs:668-670 — one sentence on the guard).
5. `docs(changelog)` entry (the undo-surfaces precedent: separate
   commit).
6. Full gate suite through `scripts/run-quality-gates.sh` (AGENTS
   §10) — from the worktree.

## 8. Acceptance (dispatch plan, restated & operationalized)

- Guard at the top of the pass: T1's stall assertions.
- The unchanged pin, load-bearing: T1 + the break-proof in §6.
- Journeys/keyboard/golden green **unmodified**: gate suite diff
  shows zero test edits (T1-T4 are new files/blocks only).
- Idle cadence cost at board scale → ~0, argued from the pin: an
  idle session's dirty ticks (stray signals, liveness episodes,
  health flips) execute load + `ptr_eq` + the bounded-tiny settings
  diff — the 20 ms-at-N=10 000 derive runs only on a publish or the
  minute flip (which today's driver already ran regardless).
- No timing assertions anywhere (AGENTS §5; dispatch acceptance's
  "argued, not timed").

## 9. Risks & traps

1. **A missed force-site freezes its surface until the next publish
   or minute flip.** The audit (§5) is exhaustive at the base; the
   residual risk is *future* callers. Mitigations: the invariant
   sentence on `refresh`'s doc comment (D2) pointing at the audit;
   `refresh` defaulting to guarded is deliberate — a future view
   mutation that forgets the force produces a *visible* staleness in
   journeys within one review cycle, not a latent crash.
2. **The identical-values republish costs a wasted pass** — accepted
   by #463's own framing; T1 step 5 pins it so a future "optimization"
   (values-diffing) must first argue against the typed contract.
3. **`push_settings` on every guarded pass** — bounded-tiny by
   construction (connectors + boards rows; the r2 D9 diff discipline);
   it is the price of delivering liveness/replies without forcing.
   If a future settings surface grows, revisit D2's tail, not the
   guard.
4. **The `last_derived` refcount** extends a superseded snapshot's
   life by at most one pass — no content copy; negligible and
   bounded.
5. **Re-entrancy**: the cache assignment sits at the forced body's
   end, after `models::push` — a (hypothetical, none found) slint
   callback re-entering `refresh` mid-push would see the stale cache
   and derive again, never skip a needed pass. Skip-after-assign is
   the wrong order and is called out here so stage 2 doesn't "tidy"
   it.
6. **Do not "simplify" the wipe out of the guarded tail** — T6's
   flow-table pins (`..._die_on_a_settle`) fail within one step; the
   §8 exactness invariant is load-bearing on both paths (finding §1.5).
7. **The pin must survive review's break-it check**: breaking the
   guard (making `refresh` unconditionally delegate) must fail T1 —
   if a refactor ever makes T1 tautological (e.g. the counter moved
   somewhere the guard bypasses), the pin is dead and must be
   re-homed at the increment site.

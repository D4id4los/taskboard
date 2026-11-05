# [Plan] Chunk 4.2 — the `install_seam!` adjudication & conversion (#595)

- **Date**: 2026-11-03 (dispatch-scheduled name; written 2026-11-05)
- **Chunk**: 4.2 of backlog round 3
  (dispatch plan: `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md`
  §5 "Chunk 4.2 — install_seam! macro (#595)")
- **Branch**: `refactor/install-seam-macro`, cut off the fetched
  `origin/main` at `20f98b3d`. Worktree:
  `../taskboard-WT-refactor-install-seam-macro` (upstream tracking
  unset after `worktree add -b`; the branch must never look
  push-defaulted at `main`).
- **Covers** (normative): #595 → audit finding F7-08
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-crate-ui-slint-report.md`
  §11 axis H — the 13th-seam trigger and the "three concrete
  call-shapes" design premise) and the #275 close comment's deferral
  (the 110-install census, 2026-11-03 @ `5d8c0751`).
- **Gate (verified 2026-11-05)**: round-4 chunk 11's PR **#619**
  (`feat/ui-r4-settings-connections`, MERGED 2026-11-04T20:59:12Z) and
  chunk 4.1's PR **#628** (`feat/gui-settings-editor`, MERGED
  2026-11-04T23:27:51Z) are both in `origin/main`; the only open PRs
  are a docs branch (#633) and two dependabot bumps (#616, #617) —
  `adapter.rs` and the settings surface are quiet. Gate passed.
- **Acceptance** (per the dispatch): either the conversion lands with
  zero behavior change, or this plan's re-adjudication concludes the
  macro loses and #595 closes on the evidence. Both outcomes are a
  completed chunk.

## 1. Method

Read end to end: `crates/taskboard-ui-slint/src/adapter.rs` (all 8149
lines — the mount `install()`, every seam handler, and all 28
`install_*` fns), the audit report §11/§13 (F7-08), #595, the #275
close comment, the dispatch plan §5 chunk 4.2, and the ui-slint test
tier (`tests/journeys.rs`, the probe tests, `tests/icon_mapping.rs`,
`tests/pin_guard.rs`). The census methodology —
`grep -o 'window\.on_[a-zA-Z0-9_]*'` over `adapter.rs` — reproduces
the #275 numbers exactly at `5d8c0751` (110 installs / 26 `install_*`
fns), so current numbers below are directly comparable.

## 2. The re-derived call-shape catalogue

### 2.1 Census at the chunk's base (and the drift from #275)

| Fact | @ `5d8c0751` (#275) | @ `20f98b3d` (this chunk's base) |
|---|---|---|
| `window.on_*` installs in `adapter.rs` | 110 | **163** (all 163 distinct callbacks — no name installed twice) |
| `install_*` fns | 26 | **28** (27 installing + `install_view_seams`, which only delegates; the mount `install()` wires 5 more) |
| Installs outside `adapter.rs` | — | 6 in `lib.rs` (the tooltip trio `on_size_observed`/`on_delay_requested`/`on_dismiss_requested`; the universal-notification trio `on_dismiss`/`on_snooze`/`on_cancel`, already behind a hand-rolled `on_row` combinator) |

The +53 since #275 is the r4 tail waves (the concentration overlay
#496, the habits/recur trio #506, the timer setter D33, the track-form
overlay r4 chunk 10) plus chunks 11 and 4.1 — i.e. the growth is
**ongoing, not residual**: ~30 installs per GUI round is the current
rate. The trigger (a 13th seam) has fired ~12.5× over.

### 2.2 Scaffold variants (the compressible dimension)

Every install is `Rc::clone` + optional `window.as_weak()` + optional
`upgrade()`-else-return + a borrow + the body. The scaffold spellings
actually present, counted over the 163 sites:

| Variant | Count | Exemplars |
|---|---|---|
| weak + upgrade-guard + `borrow_mut()` | 137 | `install_banner_seams`, `install_settings_seams` (all 17), the mount's menu seams |
| weak + upgrade-guard + `borrow_mut()` + **bool return** (guard must `return false`) | 3 | `on_rule_add_custom_accepted`, `on_timers_create_accepted`, `on_track_form_commit_requested` |
| no weak (no window access) + `borrow_mut()` | 11 | the ten buffer mirrors (`on_board_create_edited`, `on_task_title_edited`, `on_capture_text_edited`, …) + `on_timer_setter_insist_picked` |
| no weak + `borrow()` (raw send only) | 4 | `on_stack_task_done_toggled`, `on_today_row_done_toggled`, `on_notification_dismiss_clicked`, `on_notification_done_clicked` |
| `try_borrow_mut()` absorb (the teardown-echo discipline) | 2 | `on_board_card_drag_finished`, `on_text_flow_focus_lost` |
| singular (stay explicit) | 6 | `on_intercept_key` (walk-reentry flag + dropdown ladder + bool), `on_timer_setter_commit_requested` (borrow/borrow_mut interleave + invoke-chain into another seam), the track-form quartet installed from one `for` loop with a shared `handler` closure |

137+3+11+4+2+6 = 163. Supporting counts over the seam region:
142 `as_weak()` mints, 143 upgrade-guards, 50 `usize::try_from`
index-conversion guards (body content, never scaffold), 18
`settle_pointer_conflict` calls.

**Guard-spelling drift is real and observed.** The same
"upgrade-or-bail" guard is spelled four ways in the file: the dominant
`let Some(window) = weak.upgrade() else { return }`; the check-form
`if done_weak.upgrade().is_none() { return; }`
(`on_task_done_toggled`, `adapter.rs:7649`); the unused-binding form
`let Some(_window) = release_weak.upgrade() else …`
(`on_board_stack_width_release`); and the bind-discard form
`if done_weak.upgrade().is_none()` again inside
`install_tracking_seams`. Each is imitation-drift from re-deriving
the same 4 lines ~150 times; none is a bug — but the drift is the
axis-H smell, verifiable today.

### 2.3 Behavior families (the incompressible dimension)

What the body does after the scaffold, counted by dispatch verb over
the seam region (a closure may compose several):

| Family | Verb counts | Notes |
|---|---|---|
| route-action (plain + one-shot-target) | 43 `route()` calls | the dominant family: set `reminder_target`/`board_target`/`timer_target`/… then route (`install_banner_seams`, `install_notification_seams`, `install_toolbar_seam`) |
| raw-send command | 8 | the done checkboxes via `send_toggled_done`, the notification dismiss/done, `commit_date`'s `UpdateTask` |
| view-state machine verb | 29 `apply_outcomes` | drag arms, capture/palette closes, context menus, preset CTA |
| mirror | 12 `set_buffer` | no-window `borrow_mut` mirrors |
| draft + forced refresh | 35 `refresh_forced` | edited-draft fields, setter drafts, disarm arms |
| text-commit funnel | 8 `commit_text` | the seven text flows' Return seams |
| settings VM forward | 17 | all of `install_settings_seams` — one line of body each, the "settings-scale-verb" family #595 names |
| due-editor state family | 14 installs in `install_due_seam` | heterogeneous bodies (parse/sync/commit funnel/picker guards) — uniform scaffold only |
| singular | 6 installs | §2.2's exception row |

The #595-listed shapes all verify: route-action (43), raw-send (8),
settings-scale-verb (17 + 7 general-editor siblings), due-edit (14),
multi-Rc/weak (the paired mints in `install_board_seams`/
`install_stack_seams`/`install_toolbar_seam` — 25 fns mint 2+ handles
side by side). The audit's "three concrete call-shapes" premise
re-derives as **6 scaffold variants × 9 behavior families**. That
premise is dead — but see §3.2: it only ever supported a *generic
wrapper*, not a token macro.

### 2.4 What the journey/golden suite observes

- `tests/journeys.rs` + the probe suites drive **48 distinct seams
  directly** via slint's generated `invoke_*`; the rest are exercised
  markup-mediated (button/field interactions through the testing
  backend) or undriven — roughly 70% of the 163 installs have no
  direct test drive. The suite cannot be the dropped-install net on
  its own.
- Snapshots (`tests/snapshots/`) and goldens (`tests/goldens/`,
  `golden_visuals.rs`) pin the derived models and the rendering; the
  #596 cleanup chunk's protocol ("suite green unmodified, no
  snapshot/golden moved") is the zero-behavior-change precedent this
  chunk reuses.
- No existing test counts installs. The structural-manifest test form
  has two in-repo precedents to copy: `tests/icon_mapping.rs`
  ("a structural manifest assertion over the sources, not a text
  assertion on program output" — totality/existence/no-dead-rows over
  `include_str!` sources) and `tests/pin_guard.rs` (`include_str!`
  over `Cargo.toml`).

## 3. The adjudication

### 3.1 Option space

- **A — do nothing** (update the standing comment's numbers; close
  #595 on the evidence).
- **B — generic wrapper**: one `Fn(&mut Adapter, &AppWindow, Args)`
  wrapper fn, generic over closure arity/return/borrow discipline.
- **C — declarative scaffold macro**: `install_seam!` compresses the
  Rc/weak/upgrade/borrow scaffold; bodies stay verbatim at the call
  site.
- **D — partial conversion** (macro only the uniform families —
  settings forwards + plain routes; keep the rest explicit).

### 3.2 The honest comparison

**Boilerplate saved.** ~6 lines of scaffold per convertible site ×
157 sites ≈ **650 net lines deleted** (~8% of the file), plus the
per-round tax: at ~30 installs/round, ~150 lines/round of ceremony
stop being written and reviewed.

**Macro complexity.** One `macro_rules!` with five populated arms +
docs ≈ 120 lines. Not "small" in the audit's sense — but bounded,
local to `adapter.rs`, and its arms encode *scaffold spellings*, not
type machinery.

**Type-inference costs.** The audit's standing rationale
(`adapter.rs:5970–5972`: "A `Fn(&mut Adapter, &AppWindow, Args)`
wrapper shape is over generics' worth of closure types") is
**re-adjudicated as: correct, and decisive — against Option B**. A
wrapper must be generic over slint's per-callback arity (0–4 args),
return type (unit vs bool), and borrow discipline simultaneously;
each of the 163 sites would instantiate its own bound surface, and
the two `try_borrow_mut` absorbs plus the re-entrancy guard don't fit
the signature at all. **Option C never touches this**: slint's
generated `on_*` methods keep receiving exactly the closure they
always did; the macro is token splicing with zero generic machinery,
zero inference change, zero monomorphization change. The rationale's
objection does not reach the declarative design — the comment gets
replaced by the decision record, not by a wrapper.

**The closure-variety argument.** 6 scaffold variants × 9 body
families. This kills the *single-arm* fantasy but not the macro:
bodies are opaque token soup to a declarative macro; only the
scaffold variants need arms, and five arms cover 157/163 sites (§5).
The variety argument does force two concessions: the 6 singular
installs stay explicit (D3), and Option D (partial) is rejected
because a mixed idiom — some seams macro, some explicit, in one
wiring file — is worse for review than either extreme.

**Safety (the strongest do-nothing fact, stated at full strength).**
The conversion buys **zero hazard reduction**. The borrow checker
already forces the scaffold's shape: a `'static` slint callback
cannot capture `&AppWindow`, so weak-or-nothing is enforced; a
wrong-direction borrow panics loudly in the test tier. No scaffold
bug is recorded in the audit, the plans, or the reports across five
GUI rounds and 163 seams. The duplication is compiler-forced
ceremony, not a hazard — F7-08 itself was filed under axis H
(duplication), P3, effort S. Status quo carries **zero migration
risk**; conversion carries the classic mechanical-rewrite risk whose
catastrophic mode (§7) is a dropped install. The conversion must
therefore win on maintenance economics alone, and only if the risk
budget is provably tiny.

**Verdict: CONVERT (Option C).** The trigger fired 12.5× over; the
audit's action was conditioned on a cheap design ("three concrete
call-shapes"), and while that premise died, its death poisons only
the wrapper design the standing comment already rejected. What the
premise's death actually changed is the arm count: five arms, each
with a recorded population (§5), is still a bounded, reviewable
machine. Meanwhile the economics moved decisively: the seam count
grew 110→163 in two days of r4 chunks and keeps compounding at
~30/round, the guard-drift evidence (§2.2) is the file's own
demonstration that re-deriving 150 copies of one idiom decays, and
the house doctrine ("one mechanism per concern" — the D9 table, the
`commit_date` funnel, `settle_pointer_conflict`, the `apply_effect`
choke point) has 163 unconsolidated copies of one concern standing in
its most-churned file. The near-analogue the audit left as-is (the
keymap preset tables) is materially different: those rows are
*binding facts* — the duplication carries the documentation value.
The scaffold lines carry none; the per-seam design-decision comments
survive conversion verbatim, and the body becomes more visible, not
less. The do-nothing outcome remains legitimate under the dispatch's
acceptance rule — this plan records its best case and why it loses —
but the evidence lands on conversion.

### 3.3 Re-open trigger (recorded, not hidden)

If a future shape ever needs a sixth arm, or per-site customization
beyond the body tokens (e.g. a guard that must run mid-scaffold),
re-adjudicate on the same evidence form. Recorded in the macro's doc
comment (D8) so the trigger survives the chunk.

## 4. Decisions (every open question resolved)

- **D1 — Verdict**: convert; Option C. §3.2. Do-nothing is recorded
  as considered and rejected.
- **D2 — The standing rationale comment**
  (`adapter.rs:5970–5972`) is **replaced** with the decision record:
  what was counted, why the wrapper objection stands but doesn't
  reach the declarative design, pointer to this plan. No stale
  comment survives the chunk.
- **D3 — Exceptions**: 6 installs stay explicit — `on_intercept_key`,
  `on_timer_setter_commit_requested`, and the track-form quartet's
  loop. Reasons: the quartet's shared-`handler` construction would
  have to be restructured to macro-ize it; the other two carry
  borrow-interleaves the arms deliberately don't model. 157 convert.
- **D4 — Scope**: `adapter.rs` only. `lib.rs`'s 6 installs (tooltip,
  universal notifications) are out of scope — #595 names the adapter,
  and the notification trio already sits behind its own `on_row`
  combinator. The `UiHandle::set_view_context`/`set_focused_entities`
  permanent pub seam (F7-04) is not a callback install — untouched.
  The `status_fire`/`rotation_fire` closures are timer fire closures,
  not window callbacks — untouched.
- **D5 — Arm populations** (the dispatch's no-single-occupant rule):
  `mut` ≈ 137, `bare_mut` 11, `bare_ref` 4, `try` 2, `mut-else` 3.
  No `ref` arm (weak+upgrade+`borrow()` has zero occupants — verified
  in §2.2). No speculative arms.
- **D6 — The net lands first** (commit 1, before any conversion): a
  two-sided structural inventory test. Conversion-agnostic: it counts
  `window.on_<name>(` occurrences in the source, so it is blind to
  macro vs explicit and keeps guarding every future chunk. Precedent:
  `icon_mapping.rs`'s documented structural-manifest form (§2.4) —
  this is a source-structure assertion, not a forbidden text
  assertion on program output.
- **D7 — Commit/PR shape**: one PR (one coherent deliverable);
  per-family commits (§6), each standalone-green under the full
  local gate sequence for the crate. PR title per the dispatch:
  `refactor(ui-slint): the install_seam! macro over the adapter
  installs`.
- **D8 — Macro home and visibility**: `macro_rules!` defined in
  `adapter.rs` immediately before the mount `install()` (textual
  order requirement), crate-private, with the §3.3 re-open trigger in
  its doc comment.
- **D9 — CHANGELOG**: none — no user-observable change (the dispatch
  mandates this; the #596 precedent agrees).
- **D10 — Registration order**: conversions are in-place per site;
  callback registration order is preserved exactly (the slint
  callbacks are independent, but order preservation keeps the diff
  reviewable and the "byte-identical" claim honest at the
  construction level).
- **D11 — New warnings**: zero. Bodies that never use the window name
  it `_window` in the macro invocation's arg list (the arg list is
  caller-authored).
- **D12 — Conversion-tracking inventory**: the stage-2 report quotes
  the inventory test as the fn/install count before/after (dispatch
  §5 4.2.b) — the test is the census, not a side-channel document.

## 5. Macro design

### 5.1 Invocation grammar (illustrative — micro-syntax is stage 2's)

```rust
// arm `mut` (the dominant shape):
install_seam!(window, adapter, on_banner_snooze_clicked; (a, window, index); {
    let Some(row) = notification_row(&a, index) else { return };
    a.reminder_target = Some(notification_target(&row));
    a.route(&window, ActionId::ReminderSnooze);
});

// arm `bare_ref` (no window access, immutable borrow):
install_seam!(window, adapter, [bare_ref] on_notification_dismiss_clicked; (a, index); {
    let Some(row) = notification_row(&a, index) else { return };
    a.send(taskboard_domain::StateCommand::DismissReminder { … });
});

// arm `mut-else` (bool-returning seams):
install_seam!(window, adapter, [else false] on_timers_create_accepted; (a, window, label, when_text, alarm_rung); { … });
```

### 5.2 Expansion (the `mut` arm, verbatim today's idiom)

```rust
{
    let adapter = ::std::rc::Rc::clone(adapter);
    let seam_window = window.as_weak();
    window.on_banner_snooze_clicked(move |a, window, index| {
        let Some(window) = seam_window.upgrade() else { return };
        let mut a = adapter.borrow_mut();
        { /* body verbatim */ }
    });
}
```

Hygiene rules:

1. The macro's temporaries (`adapter`, `seam_window`) are
   macro-hygiene-scoped — a body-local `adapter` cannot collide.
2. The arg list `(a, window, index)` is caller-authored idents
   spliced as the closure's parameters — the body's `a`/`window` are
   the closure's own bindings, exactly today's shadowing idiom; write
   `_window` when the body never uses it (D11).
3. The macro adds **no logic** beyond mint + guard + borrow: index
   conversions, settles, refreshes stay in bodies (no hidden
   `usize::try_from`, no hidden `settle_pointer_conflict` — those are
   per-site decisions and stay visible).
4. No arm for a shape with one occupant (D5); the doc comment lists
   each arm's population and the §3.3 trigger.

### 5.3 Arm set (populations from §2.2)

| Arm | Expansion deltas | Population |
|---|---|---|
| `mut` | weak + upgrade-guard(`return`) + `borrow_mut()` | ~137 |
| `[else <expr>]` | as `mut`, guard returns `<expr>` | 3 |
| `[bare_mut]` | no weak; `borrow_mut()` only | 11 |
| `[bare_ref]` | no weak; `borrow()` only | 4 |
| `[try]` | weak + `try_borrow_mut()` absorb (`return` on failure) | 2 |

**Stage-3 amendment (deviation from D5, reviewed 2026-11-05):** a
sixth arm exists — the *non-mut* spelling of the default arm, one
occupant (`on_task_done_toggled`; its body only reads through the
borrow, so the `mut` arm's binding would trip `unused_mut` under
`-D warnings`). D5's no-single-occupant rule stands, but the
alternative was not an arm-count reduction — it was either a
warning-suppression attribute per the mandate's "no new clippy
suppressions" bar or a behavior-adjacent body rewrite (mutating
through a borrow that only reads). The arm carries its reason in its
matcher comment; final arm census: default-mut 136, default non-mut
1, `[else]` 3, `[bare_mut]` 11, `[bare_ref]` 4, `[try]` 2 = 157
macro + 6 explicit = 163.

## 6. Migration order (commit breakdown)

Ordered small→large to prove arms early; each commit standalone-green
(crate test suite + inventory test + no snapshot/golden diff). The
dispatch names the first two as the pilot pair.

1. **The net**: `tests/seam_inventory.rs` — the 163-name inventory,
   two-sided (D6), green on the untouched file.
2. **Macro + pilot**: `install_seam!` definition; convert
   `install_notification_seams` (6: proves `mut` + `bare_ref`) and
   `install_menu_dismiss_seam` (1: proves a singleton fn). Journeys
   green.
3. **Singletons**: `install_board_context_seam`,
   `install_stack_context_seam`, `install_settings_back_seam`,
   `install_task_back_seam`, `install_board_bar_seam`,
   `install_picker_grid_seam`, `install_stack_done_seam`
   (`bare_ref`), `install_body_seams` (2) — 8 fns / 9 installs.
4. **Board/toolbar surfaces**: `install_toolbar_seam` (4),
   `install_stack_create_seams` (4, incl. a `bare_mut` mirror),
   `install_object_context_seams` (8, incl. 3 `bare_mut` mirrors),
   `install_board_seams` (8), `install_stack_seams` (10, incl.
   `bare_mut` mirrors), `install_today_row_seams` (5, incl.
   `bare_ref`) — 6 fns / 39 installs.
5. **Gestures + overlay**: `install_banner_seams` (3),
   `install_habit_seams` (2), `install_focus_seams` (5),
   `install_drag_seams` (4, proves `try`), `install_stack_width_seams`
   (3) — 5 fns / 17 installs.
6. **Task/due editor**: `install_task_seam` (11, proves
   `[else false]`), `install_due_seam` (14) — 2 fns / 25 installs.
7. **Timers + tracking**: `install_timer_seams` (13; the
   setter-commit exception stays explicit → 12 convert),
   `install_tracking_seams` (20; the quartet exception stays explicit
   → 16 convert) — 2 fns / 33 installs, 28 converted / 5 explicit.
8. **Recur + settings**: `install_recur_seams` (11),
   `install_settings_seams` (17, the uniform forward block) — 2 fns /
   28 installs.
9. **The mount + the record**: the four convertible mount seams
   (`on_menu_entry_invoked`, `on_menu_submenu_entry_invoked`,
   `on_menu_level_back`, `on_window_width_changed`); replace the
   standing rationale comment with the decision record (D2). Final
   state: 157 via the macro, 6 explicit, 163 installed — pinned by
   the inventory test.

## 7. Zero-behavior-change protocol (the dropped-install net)

**The catastrophic failure mode** is one `window.on_*` removed and
not re-added: a dead button in the shipped UI that ~70% of the time
no test drives directly (§2.4). The net catches it mechanically:

- **Two-sided inventory test** (`tests/seam_inventory.rs`, commit 1,
  green before any conversion):
  1. *totality*: for every name in the 163-entry `SEAM_INVENTORY`
     const, the source contains `window.on_<name>(` **exactly once**;
  2. *no unlisted installs*: the total count of `window.on_`
     occurrences equals `SEAM_INVENTORY.len()`.

  A dropped install fails (1); an added-but-unlisted install fails
  (2) — the census cannot rot silently in either direction, and
  stage 3's adverse test (delete one install, observe the failure,
  restore) fails loudly on both halves.
- **Per-commit discipline**: each migration commit runs the crate
  suite with `git status` clean on `tests/snapshots/` and
  `tests/goldens/`. **A moved snapshot or golden aborts the chunk**
  — zero-behavior-change means the models and pixels are pinned, so
  any diff there is a conversion bug, bisect to the family.
- **Journey/golden suite byte-identical**: no test file other than
  the new inventory test changes; the #596 protocol ("suite green
  unmodified, no snapshot/golden moved") is the exit bar.
- **Final gate**: the full AGENTS §10 sequence via
  `scripts/run-quality-gates.sh` (fmt, clippy `-D warnings`, nextest,
  doc tests, deny, the coverage trio).

## 8. Stage-2 exit criteria

1. Inventory test green with `SEAM_INVENTORY` at 163; 157 sites
   construct via `install_seam!`, the 6 exceptions explicitly.
2. No production file other than `adapter.rs` differs from the base;
   no test/snapshot/golden file other than the new inventory test.
3. All gates green; no new clippy suppressions or warnings; the
   standing rationale comment is gone, replaced by the decision
   record (D2).
4. Report quotes the inventory: fn count 28 (unchanged), installs
   163 → 163, macro-constructed 157.

## 9. Risks

| Risk | Mitigation |
|---|---|
| Dropped install (catastrophic) | the net, landed first, conversion-agnostic, two-sided; per-commit green (§7) |
| Macro arm subtly diverges from today's idiom (e.g. a hidden guard) | the expansion is 4 lines (§5.2); the pilot commit (2) is reviewed line-by-line against the before-shape before any family follows |
| Borrowck diagnostics degrade inside expansion | arms share one matcher prefix; errors anchor at the call site; bodies are untouched tokens, so most errors still point into the body |
| `unused variable` warnings on window-less bodies | caller-authored arg lists (`_window`), D11; per-commit warning-free |
| Snapshot/golden drift from construction-order change | in-place conversion preserves registration order (D10); protocol aborts on any diff (§7) |
| Rebase friction on the most-churned file | gate passed (both in-flight PRs merged; surface quiet); one PR, land promptly |
| A sixth shape appears mid-conversion | §3.3 trigger: stop, re-adjudicate; do not grow arms ad hoc |

## 10. Out of scope (recorded, not done)

- `lib.rs`'s 6 installs (D4) — future adopters if the macro proves
  out; the notification trio already has its local combinator.
- The `UiHandle` pub seam fns, the timer fire closures, the drag
  payload mint, and all markup-side wiring.
- Any behavior change, any new seam, any test strengthening beyond
  the inventory net — a zero-behavior-change chunk carries none of
  these; ideas go to issues per AGENTS §7.

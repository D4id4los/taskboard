# [Report] UI R4 chunk 4 — hover machinery fixes, stage 2

- **Date**: 2026-11-02
- **Branch**: `fix/ui-r4-hover-fixes` (base `67c0b531`; worktree
  `../taskboard-WT-fix-ui-r4-hover-fixes`). No push, no PR (dispatch
  §ordering: hand-off at branch-ready; the coordinator opens/stacks).
- **Implements**: `.artifacts/plans/2026-11-01-ui-r4-hover-fixes-plan.md`
  (stage-1) against round-4 plan §3.4 (D13/D14/D15) and the dispatch
  plan §1/§5. **Empty diff outside `crates/taskboard-ui-slint`**
  except `scripts/r4-hover-repro.sh` and `.artifacts/` + CHANGELOG —
  i.e. the ui-slint source diff is the only production-code change.

## 1. Commits (each standalone)

1. `4b4aa592` fix(ui-slint): the tooltip panel mounts its parts conditionally
   — the D13 probe (red run recorded below) + the conditional-element fix
   + the CHANGELOG entry.
2. `a5164ae9` feat(ui-slint): the view-identity dismiss choke point —
   `ViewState::view_identity()`, the `apply_effect` diff, the explicit
   modal/ghost arms, the `OpenSettings`/`settle_flows_on_leaving`
   subsumptions, `rotation_tick`, and the structural walk.
3. `d2b0e5a6` test(ui-slint): the dismiss journeys — the five user-facing
   pins (§3.4's 1–5).
4. `2159d11d` test(ui-slint): the hint projections are pinned per preset —
   E1/E2 + the stale `models.rs` comment correction + CHANGELOG note.
5. `1f55156f` chore(scripts): the r4 hover repro script.

Deviation from the plan's 7-commit series (recorded): commits 1–2
merged into one (the plan itself prefers probe+fix in one commit —
"no red commits on the branch"), and the modal/ghost arms landed with
the choke point (commit 2) rather than separately, because the arms
are one-to-three-line edits to the same file and splitting them would
make commit 2's totality claim false without them.

## 2. D13 — the probe's verdict and numbers

The probe (`tests/tooltip_layout_probe.slint` + `..._tests.rs`) hosts
the real `TooltipPanel` plus a hidden one-line caption reference; the
panel's own `tip-width`/`tip-height` mirrors are the geometry oracle.

**Red measurement (before the fix, on `67c0b531`'s panel):**

```
a caption-only tooltip is caption + padding: got 46, expected 23
(hidden rows reserve layout space)
```

Exactly the plan §2.1's predicted mechanism: the hidden description
row reserves **one caption line** (23 − the caption+padding baseline
= 23 px of phantom height ≈ one caption line + spacing). The
keyless hint row's contribution is inside that delta too (its
spacing slot). The §2.1 static analysis (the visibility wrapper stays
a layout cell; the generated layout never sees a visibility term) is
**confirmed by measurement** — the contingency path ("the renderer
returns 0 for empty text") did not fire.

After the fix the four-state matrix is green: caption-only ==
caption line + padding; description-only, keys-only, and both are
strictly ordered by content; keys widen the panel (the r2 D14 width
relation holds unchanged — `the_tooltip_width_includes_the_hint_row_and_keeps_the_cap`
passes untouched, as do the other four existing tooltip pins).

Fix shape (§2.3 verbatim): the description/hint rows are conditional
layout elements; two never-visible off-layout measurers
(`description-natural`, `hint-natural`) re-source the width formula's
natural terms (a conditional element cannot be referenced from
outside its branch). No literal-px geometry was introduced (the
token lint has no waiver hatch — the measurers need none: they are
off-layout, so zero-geometry is the default, not a binding).

## 3. D14 — the total dismiss seam

Landed per §3.2:

- `ViewState::view_identity()` — `(view, board, subject_stack,
  subject_task)`; `focused_task`/`selected_stack` deliberately
  excluded (the over-fire guard; a focus step must not kill a live
  hover tip).
- The `apply_effect` choke point: snapshot at entry, diff at tail,
  dismiss on change.
- Explicit dismisses kept/added with named non-identity reasons:
  `OpenMenu`/`apply_close_menu` (modal takeover, kept),
  `BeginCreate`/`BeginCreateStack` (the ghost's own unmount, added in
  the `Flow::None` begin arm — the arm that owns the flow's arming),
  `OpenPalette`/`BeginCapture` (the modal-takeover half of the r2
  D13 rule, added), `rotation_tick` (view swap outside
  `apply_effect`, added).
- Removed as subsumed: the `OpenSettings` arm's call (the diff covers
  both toggle directions) and `settle_flows_on_leaving`'s dismiss —
  the recorded per-path analysis: its **only** caller is
  `interpret`'s leaving block, and `interpret` is only reached from
  `apply_effect`, so the choke point subsumes it on every path (the
  resolver's `SettleThenRun` re-enters `interpret` inside the same
  pass, before the tail diff).
- The `leaving` match keeps its membership — it drives the settle
  half only (§3.2 point 4).

**The structural walk** (`every_view_swapping_effect_dismisses_...`
in `flow_table_tests.rs`): 46 effect rows, every variant placed in a
wildcard-free `dismiss_class` match (a new variant is a compile
error until placed), each driven through the real `apply_effect` pass
with an armed tooltip. Two honest edges the walk itself surfaced,
resolved per the plan's §6-R2 rule:

1. `BoardPrevious` from the first canonical board is a no-op (the
   board steps don't wrap — "the ends hold"), so its precondition
   seeds the second board. Not a bug: the effect *can* swap, and
   does when a neighbour exists; the diff still catches every actual
   swap.
2. The walk fixture needed an `Inbox`-titled board (`OpenInbox`
   no-ops without one, by design) and a cross-board palette target
   (`ShowTask`/`InvokeSelected` onto the same board would honestly
   preserve the identity).

**The journeys** (all five from §3.4): F14-repro activation, the
inbox jump (the toolbar chip's mouse path — `ViewInbox` is bound in
**no preset**, so no keyboard journey exists; recorded), the palette
invoke across boards, the ghost click, the kiosk flip. All green.
Testing note of record: the chip is a bare `TouchArea` — the
driver's `click_by_label` (the a11y default-action seam) does not
fire it; the journey uses the coordinate click seam.

## 4. D15 — the hints-at-start verdict (F4)

**Verdict: cannot-reproduce on this base; the by-design records
(H-b primary, H-d fixed by D13, H-a/H-c closed by evidence) are the
deliverable.** The Tier-1 live repro did **not** run — Tier-4 detail
of record:

- The machine's only display (`:0`) is the operator's live
  KDE/Wayland session (kwin_wayland + XWayland); injecting synthetic
  pointer motion via XTest would hijack the operator's cursor
  mid-session — declined.
- No isolated server exists (`Xvfb`/`weston` not installed; only
  kwin's own nested-less session).
- Per the dispatch prompt, the verdict is therefore seam-analysis
  with recorded evidence; the repro protocol is committed as
  `scripts/r4-hover-repro.sh` (method of record for an interactive
  machine).

The evidence chain:

- **E1 (H-a closer, green)**:
  `first_frame_hints_match_the_keymap_projection` — at first frame
  the seven chrome hint models (`toolbar_menu_hint`, `nav_back_hint`,
  `board_segment_hint`, `task_card_hint`, `stack_header_hint`,
  `stack_create_hint`, `task_create_hint`) equal
  `first_binding_display(action, mount_chain)` computed independently
  from the preset. Hints are present from the start, projected from
  the real keymap. The masks are asserted exactly `{Menu, Palette}`
  per preset; `ViNormal` never masks — nothing hides the mount chain
  (`[view, ViNormal, Global]`).
- **E2 (H-b, green)**: `projected_hint_bindings_per_preset` — over
  every preset × projected action × `[Board, ViNormal, Global]`, the
  only never-projected pair is `(every preset, BoardSelect)`. The
  board-segment tooltip's keyless hint is **by-design emptiness**
  (D19: a keymap projection never fabricates a chord). The recorded
  presentation decision: **render nothing** (the established
  empty-hint rendering); rendering the action's menu-path hint was
  considered and rejected in the plan (§4.4) — it would make the
  tooltip the only carrier of reachability for an unbound action,
  violating the panel's contract, and menu-path strings are
  hand-written copy. Binding `BoardSelect` for real is a follow-up
  issue (keymap semantics, out of this chunk's scope).
- **H-c closed by code evidence**: the desktop hand-over builds the
  full keymap before mount; the settings-reply swap
  (`apply_keymap_update`) fires only from a settings *save* and ends
  in a forced refresh (the keymap is a non-state derive input). No
  seam exists where a fresh boot's hints change under the user.
- **H-d**: the D13 fix is the fix; the probe's red measurement
  (§2) is the mechanism evidence — the phantom line (exactly one
  caption line) plausibly read as a broken/missing hint row under a
  card tooltip.
- **The stale comment** (`models.rs`, the stack-create hint):
  corrected — `StackCreate` is bound in every shipped preset (emacs:
  `Ctrl+c s`); the "empty under emacs" note predates chunk 3's
  binding. E2 now pins the empty set so the next binding change
  flips a test deliberately.

Follow-up for the reviewer (R3): if F4 was observed on the round's
original build, please re-check against a build containing this
branch — on this base, first-frame hints match the projection
exactly.

## 5. Acceptance checklist (dispatch §5)

- (a) probe first, then the conditional-element fix — done; the red
  measurement is recorded above (§2).
- (b) the structural walk with no wildcard arm — done (§3), 46 rows.
- (c) journeys for all five transitions — done (§3).
- (d) delay/placement untouched (`TOOLTIP_DELAY_MS`, `src/tooltip.rs`
  `place` — zero diff there); the existing pins
  (`a_visible_tooltip_does_not_survive_a_view_switch`,
  `a_menu_row_click_unhovers_the_tooltip`,
  `the_tooltip_width_includes_the_hint_row_and_keeps_the_cap`,
  `an_add_task_ghost_hover_arms_the_tooltip`,
  `an_info_tip_hover_arms_the_generalized_tooltip`) all pass
  unmodified; keymap snapshots untouched; tooltip copy untouched.
- (e) empty diff outside ui-slint (production code) — confirmed
  (§0 preamble).

## 6. Quality gates

Run via `scripts/run-quality-gates.sh` (the exclusive-flock wrapper;
the run queued behind a parallel suite from
`feat/delivery-proof-pruning`, then executed clean). Exit code 0;
per-gate:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | OK |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| `cargo nextest run --workspace` | OK — 2813 tests, 0 failed, 41 skipped |
| `cargo test --doc --workspace` | OK |
| `cargo deny check` | OK — advisories/bans/licenses/sources |
| `cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report` | OK — 2813 passed |
| `scripts/coverage-quarantine-profraw.sh target/llvm-cov-target` | OK |
| `cargo llvm-cov report --lcov` | OK |
| `rm -rf target/llvm-cov-target` (local hygiene) | done |

## 7. Known-limitation notes for review

- The tooltip panel's conditional rows re-mount per content change
  (recorded non-risk, plan R4): content is static while shown.
- `settle_flows_on_leaving` is now a one-line wrapper over
  `settle_flow`; kept as the named leaving rule (the comment carries
  the subsumption analysis). A future cleanup may inline it.
- The engine-event exclusion (a sync-deleted task unmounts an anchor
  through refresh, not an effect) remains recorded-and-excluded
  (plan §3.3); the next pointer movement resolves the tip naturally.

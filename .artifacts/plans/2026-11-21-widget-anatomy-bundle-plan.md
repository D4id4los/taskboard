# [Plan] Widget anatomy bundle — round 4 chunk S8

- **Date**: 2026-11-22
- **Kind**: stage-1 implementation plan — six widget-anatomy backlog
  items (#1096, #1121, #1125, #1126, #1127, #1128), each issue body
  normative. The frame is the O-6 law **read in both directions**:
  base widgets with customization points, no speculative knobs — and
  no speculative *growth* of points whose trigger condition has not
  fired.
- **Origin** (normative): scope report
  `.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`
  §B-S8; dispatch plan
  `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  §1 S8 row + §17 Prompt S8.1; the six issue bodies; the widget
  corpus read fresh from `origin/main` (`a53cab50`) in worktree
  `../taskboard-WT-refactor-widget-anatomy-bundle` on 2026-11-22:
  `ui/widgets/menu-shell.slint`, `labeled-value-row.slint`,
  `anchored-overlay.slint`, `section-header.slint`, `dropdown.slint`
  (the `DropdownTone` enum + `tone-color`), `theme.slint` (the `Tone`
  ladder, `tone-color`/`tone-fill`), `ui/compositions/rule-list.slint`,
  `labels-overlay.slint`, `views/settings-view.slint::LabeledField`,
  `ui/compositions/habits.slint`, `app-window.slint` (the
  `KeyboardState.menu-open` ActionMenu mounts), the flyouts
  (`dropdown-flyout.slint`, `quick-list-flyout.slint`), and the probe
  family (`tests/menu_shell_probe.slint` +
  `tests/probes/menu_shell_probe_tests.rs`).
- **Preconditions verified 2026-11-22**: `scripts/worktree-doctor.sh`
  exit 0 (168G free / 15.8G available); worktree created with
  `git worktree add -b refactor/widget-anatomy-bundle
  ../taskboard-WT-refactor-widget-anatomy-bundle` off `origin/main`
  (`a53cab50`).
- **Status**: READY for stage 2.

## 0. The headline outcome (read this first)

The dispatch prompt frames the chunk as "six refactors", but the O-6
law cuts both ways and **four of the six issues are self-gating
defers**: their own trigger conditions ("if a third consumer
appears") have not fired — building them now would be exactly the
speculative generality the round outlaws. Verified against current
main on 2026-11-22:

| Issue | Verdict | Code this chunk |
|---|---|---|
| #1096 form-row variant | **defer-with-evidence** (trigger: a third label+editor consumer — none exists) | none |
| #1121 hint caption anatomy | **stay re-verified** (zero-delta bar still violated by conversion) | none |
| #1125 host pointer-anchor + fill-to-floor | **defer-with-evidence** (trigger: a third consumer needing either mode — none exists) | none |
| #1126 DropdownTone → Tone.accent | **defer-with-evidence** (trigger: a second accent consumer — none exists) | none |
| #1127 labels roster → form-row primitive | **stay, blocked by #1096's trigger** (the primitive class does not exist) | none |
| #1128 MenuShell `cap-rows` dead knob | **BUILD — remove the speculative point** | yes |

So the chunk is: **one code item (#1128), five evidence-refreshed
stays.** The stays get a dated evidence-refresh comment on their
issues (search-first per §7) recording the 2026-11-22 re-count; the
issues stay OPEN (their triggers may yet fire) — recommend the
dispatcher treat only #1128 as an at-merge closure and leave the five
stays open unless the operator rules otherwise. This is the same
record-and-defer shape the dispatch plan itself prescribes for S9's
capacity shortfall.

## 1. Decisions (all settled; no open questions)

| ID | Decision | Reason |
|---|---|---|
| D-1 | **#1128 build-now**: delete `cap-rows` from `MenuShell`; the height cap `min(row-count·space-6, space-6·6)` becomes unconditional. Delete the `cap-rows: true;` lines at both flyout mounts. Update the widget header comment (it currently documents the knob as "the contract boundary the plan's D-1 recorded" — that recording is superseded by this decision). | Both flyouts pass `true`, which is the default; the ActionMenu is not a shell mount (D-3, still true on main). The knob takes exactly one value — #1128's own no-distinguishing-consumer reading of the §14 law. The O-6 law cuts both ways; this is the removal direction. |
| D-2 | **#1128 probe note (correction to the issue body)**: the issue's "retire the G-2 `cap-rows: false` passthrough probe leg" is stale — the current `menu_shell_probe.slint` never sets `cap-rows` and `menu_shell_probe_tests.rs` G-2 has only the chrome test and `the_shell_caps_at_six_rows` (which reads the default arm). Nothing to retire; stage 2 verifies this on the branch and the cap test keeps passing **unchanged** (the unconditional cap is behavior-identical for every existing mount). The census `docs/ux-surface-census.org` W6 entry (≈line 66–71, "behind =cap-rows=") is updated to the unconditional law. | Corpus-as-oracle: the probe is the oracle for the removal. |
| D-3 | **#1096 defer-with-evidence**: do NOT build a `LabeledFieldRow` variant. Re-count performed 2026-11-22: the label+editor families are still exactly two — settings' `settings-view.slint::LabeledField` (validity, error line, secret, staged buffers) and the habit editor's field rows (labels ride the fields' own `label:`, `habits.slint` "Habit title"/"Quota…"). No third consumer (the capture overlay was the issue's candidate; it never materialized as label+editor). The issue's own trigger ("if a third label+editor consumer appears") governs. | The issue body is normative and self-gating; #951's D-2 ruling stands. Building now would fork the surface with one more speculative family member. |
| D-4 | **#1121 stay**: do not convert the rule-list hint caption to `SectionHeader`. Re-verified on main: the row is still a conditional hint caption with a muted-conditional colour (`muted ? Semantic.warning : text-secondary`, `rule-list.slint` ≈line 78–83) plus a separate conditional muted-indicator row — no header-row anatomy, and the affordances live in the presets row below. Conversion would still change paint (title colour, wrap) and structure, violating the zero-delta bar; the row has gained no inline affordances since. The census deviation entry stands. | Issue body normative; its revisit condition ("the row ever converges on the header-row anatomy" / "gains inline affordances") has not fired. |
| D-5 | **#1125 defer-with-evidence**: do not grow `AnchoredOverlayHost` with `pointer-anchor` / `fill-to-floor` modes. Re-verified: the ActionMenu window mounts (`app-window.slint` ≈lines 1542–1610) remain the only pointer-anchored (`KeyboardState.menu-pointer-anchored` centering law) and fill-to-floor consumer; every other overlay still fits the anchor-rect + content-height contract. A second mode with one consumer is a speculative point — the very thing #1128 removes elsewhere in this chunk. | Issue body normative; its trigger ("a third consumer ever needs pointer anchoring or fill-to-floor") has not fired. |
| D-6 | **#1126 defer-with-evidence**: `DropdownTone` stays a distinct enum; no `Tone.accent` arm. Re-verified on main: the accent arm's consumer set is still exactly the dropdown family (`dropdown.slint` `DropdownTone.accent → Semantic.focus-ring`, consumed by `menu-row.slint::leading-tone` via `DropdownFacts.tone-color`). Growing the shared `Tone` vocabulary (nine arms mapped in `Theme.tone-color`/`tone-fill`) for one consumer is speculative. The census "stays (a different enum, not a Tone ladder; W6 note)" entry stands. | Issue body normative (D-7 of the menu-shell plan); trigger ("a second consumer adopts the accent arm") has not fired. |
| D-7 | **#1127 stay**: the labels roster rows (`labels-overlay.slint`) stay bespoke. The form-row primitive class they would converge onto is exactly #1096's declined variant — with D-3's defer, the destination does not exist. The two-pointer-verb + armed-strip + swatch anatomy is form-row, not menu-row, per the original D-5 ruling; nothing has changed. This issue's revisit is **chained to #1096's trigger** (recorded on both issues). | Issue body normative; no new ruling available. |

## 2. Implementation spec (the one code item — #1128)

Behavior-identical refactor; the existing widget corpus is the oracle.
Files touched (verified paths on `a53cab50`):

1. `crates/taskboard-ui-slint/ui/widgets/menu-shell.slint`
   - Delete `in property <bool> cap-rows: true;` (lines ≈21–25) and
     the comment block above it.
   - `height:` becomes unconditional:
     `min(root.row-count * Theme.space-6, Theme.space-6 * 6);`
   - Rewrite the header comment's height-cap sentence: the cap is the
     shell's unconditional law; a future uncapped consumer (the D-3
     action-menu stay) grows an explicit height mode only when it
     exists (the same defer law as #1125's D-5).
2. `crates/taskboard-ui-slint/ui/widgets/dropdown-flyout.slint`
   — delete `cap-rows: true;` (line ≈37).
3. `crates/taskboard-ui-slint/ui/widgets/quick-list-flyout.slint`
   — delete `cap-rows: true;` (line ≈42).
4. `crates/taskboard-ui-slint/tests/menu_shell_probe.slint` +
   `tests/probes/menu_shell_probe_tests.rs` — expected **no changes**
   (the probe never set `cap-rows`; G-2's cap test reads the default
   arm, which is now the only arm). Stage 2 verifies `grep -rn
   "cap-rows"` over the crate returns nothing post-change.
5. `docs/ux-surface-census.org` — the W6 MenuShell entry: "=min(...)
   behind =cap-rows=" → the unconditional cap; append the #1128
   removal date.

**Tests**: the corpus is the oracle — no new tests. G-2's existing
`the_shell_caps_at_six_rows` (3 rows → 3·space-6, 6 → cap, 9 → cap)
must pass unmodified; the flyout goldens must be byte-identical
(the removed property equalled the default). Per the one-behavior-
one-test law, no duplicated engine/integration coverage.

**Typed outcomes law [L](a)**: the removal is a property deletion;
the observable contract (shell height, chrome paint) is asserted by
the existing probe tests — nothing text-asserted, nothing weakened.

## 3. Gates & mechanics

- Pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run
  --workspace` (flock-safe; background + poll beyond ~5 min; commit
  landed work before any wait). Long-command discipline [D] binding:
  if the gate lock is contended and no useful work remains, commit,
  report "gate deferred: lock held by <holder>", end the stage.
- No UX journey plan needed (dispatch plan S8 row: "no UX plan";
  nothing user-visible changes — the removal is paint-identical).
- Commit: `refactor(ui): remove MenuShell's dead cap-rows knob (#1128)`
  (single commit; the five stays are issue comments + this plan, not
  code).
- Stage 3 opens the PR; issue comments for the five stays are posted
  at stage 3 (search-first), each citing this plan's path + the
  re-verified evidence, per §7's standing authorization.

## 4. Risks

- **Probe goldens drift**: none expected (the deleted property equalled
  its default); if a golden changes, a real behavior delta was found —
  stop and diagnose, do not re-bless.
- **The issue body's stale G-2 claim** (D-2): if stage 2 finds a
  passthrough leg on the branch that this plan's main-side read
  missed, retire it as the issue prescribes and note the delta in the
  stage report — not a plan failure, a main-vs-branch skew.
- **Dispatcher closure-list mismatch**: the dispatch plan's at-merge
  list closes all six issues; this plan recommends closing only
  #1128 (the other five remain open under their own trigger
  conditions). Recorded here so the dispatcher decides explicitly.

## 5. Estimated diff size

S: ~30 changed lines across 4 files (3 code, 1 census doc) + ~15
comment lines on five issues + this plan. The smallest S8 shape
possible under the O-6 law — the chunk's other five items are
deliberate non-builds, which is the law working as intended.

## 6. Backlog issues opened by this stage

None required: no out-of-scope work discovered beyond the six items
themselves. (The stale G-2 sentence inside #1128 is handled as a
comment on that same issue, not a new issue.)

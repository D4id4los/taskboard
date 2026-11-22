# Chunk D1ii Implementation Plan — undo browse parity (picker keyboard + payload labels, CLI history/jump verbs)

- **Date**: 2026-11-22
- **Kind**: per-chunk implementation plan (round-5 backlog reduction,
  undo family, chunk D1ii of two).
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-22-undo-family-journeys-browse-parity.md`
  (this plan maps to its US-1..US-4, W-1..W-8, R-1..R-4, D-1..D-5);
  issues #1155, #1156 (bounded resolution; the deep-tree inspector
  stays open there), #1226, #1227; the round-5 scope report §D wave
  4; the landed UF2 base (`branch_children`, `UndoTo`/`RedoTo`, the
  picker seams, `ActionId::AppUndoBranches`).
- **Status**: DRAFT — the journey plan is GATE-PAUSED pending operator
  acceptance; dispatch may not start until the operator accepts it.
- **Branch**: `feat/undo-browse-parity` (one PR; worktree
  `../taskboard-WT-feat-undo-browse-parity`, created with
  `git worktree add -b` per AGENTS §8 after `scripts/worktree-doctor.sh`).
- **Issues**: #1155, #1226, #1227 (close on merge); #1156 (comment the
  bounded resolution, keep open for the deep-tree inspector).

## 1. Work breakdown

Ordered so every step keeps the tree green:

1. **Domain: the `UndoLabel.payload` widening** (journey plan W-4,
   G2, D-2/D-3): add `command_payload(command: &StateCommand) ->
   Option<String>` beside `command_label` in `journal.rs` — the
   journaled command's own data only (task title, stack name, label
   name, rule summary, …); `None` where the arm has no natural
   subject. Add `payload: Option<String>` to `UndoLabel`; populate in
   both `derive` windows (`recent` and `branch_children` — D-2's one
   projection). `UndoLabel` is `Serialize` — the additive field is
   serde-additive; check whether any consumer pins the struct shape
   byte-stably (#993's pins are over `CommandOutcome`, not
   `UndoLabel` — verify at implementation time; if a pin exists,
   extend it, never break it).
   - Tests (`crates/taskboard-domain/src/journal/tests.rs`):
     `us1_ac3_sibling_rows_carry_distinct_payloads`,
     `us1_ac5_subjectless_arm_derives_none_payload`,
     `us2_s1_both_windows_carry_the_enrichment`; the fence
     `undo/tests.rs::us4_s1_plain_reflex_semantics_unchanged_with_enriched_rows`.
   - Snapshot: the one deliberate insta regeneration pass; corpus
     diffs beyond the additive field are findings.
2. **Slint: the picker cursor** (W-1/W-1b/W-1c, US-1, D-1): route the
   shell's existing `MenuNext`/`MenuPrevious`/`MenuInvoke` ladder to a
   picker-scoped cursor while the picker is mounted (the action menu's
   cursor stays closed then — no conflict). No new ActionIds, no new
   focus model (`MenuRow` rows never hold focus — the family law).
   The invoke arm routes the cursor's row through the existing
   `undo_branch_picked`/`undo_target_picked` callbacks (byte-identical
   dispatch to click — US-4/AC-2 pins it).
   - Journey tests: `us1_s1_picker_mounts_with_cursor_and_payload_rows`,
     `us1_s2_menu_ladder_moves_picker_cursor`,
     `us1_s5_escape_closes_the_picker_without_dispatch`,
     `us1_s3_invoke_selected_dispatches_the_row_verb`,
     `us1_s3_invoke_matches_click_dispatch`,
     `us1_s4_presented_state_reflects_the_landed_head`,
     `us4_s2_click_and_invoke_dispatch_identically` — in
     `tests/journeys/undo_family_browse_parity_journeys.rs`.
3. **Slint: render the payload context** (G2): picker rows render
   label + payload where `payload` is `Some`; label-only for `None`
   (never a blank dash — D-3). Presentation fold test over the model
   mapping (typed, never label text).
4. **CLI: `tasks history [--json]`** (W-5, US-2, D-4): reads the
   published snapshot (no engine command), prints the head / recent /
   branch-children windows with typed ids, labels, payloads;
   `--json` decodes against the widened `UndoLabel`.
   - Tests (`crates/taskboard-app/tests/it_undo_browse_cli.rs`):
     `us2_s1_history_prints_the_three_windows`,
     `us2_s2_json_history_decodes_the_projection`,
     `us2_s3_empty_journal_is_the_typed_empty_fact`.
5. **CLI: `tasks undo to <id/prefix>` / `tasks redo to <id/prefix>`**
   (W-6, US-3): resolve uuid | unique ≥8-char prefix over the bounded
   windows only (what `tasks history` printed — the honest bound,
   R-4); ambiguous prefix refuses typed before dispatch; dispatch the
   exact `UndoTo`/`RedoTo`; refusals map onto the existing
   `domain_error_kind` table (`TargetNotInLine`, unknown-id) — D-5's
   parity-with-GUI law.
   - Tests: `us3_s1_undo_to_dispatches_and_receipts`,
     `us3_s2_redo_to_lands_the_branch_tip`,
     `us3_s3_stale_target_refuses_typed`,
     `us3_s4_ambiguous_prefix_refuses_before_dispatch`.

## 2. Recorded decisions (implementation-level; the journey plan's D-1..D-5 stand)

| ID | Decision | Rationale |
|---|---|---|
| I-1 | The picker cursor lives in the same shell cursor machinery as the action menu, keyed by the picker mount flag — not a parallel cursor struct | One cursor mechanism, two mount scopes; a parallel struct invites the exact drift #1227 documents |
| I-2 | `command_payload` is a sibling function of `command_label`, not a rewrite of it | `command_label`'s consumers (toasts, `head_label`) keep their shape byte-for-byte (the US-4 fence); the D-2 decision needs both |
| I-3 | CLI id resolution matches against the *published projection's* ids (the windows), not the raw journal | The window bound is the product decision (R-4); resolving against the raw journal would silently widen it |
| I-4 | `tasks history` is a read over the published snapshot, routed like the existing status/read verbs — no new `StateCommand`/`UiRequest` | Journeys dispatch nothing for browsing (US-2/S1); a read is not an intent |

## 3. Test & gate plan

- Journey tests: the seven picker tests (§1.2) in the per-plan journey
  file; CLI seam tests: the seven in §1.4–1.5 — typed outcome
  matches, never text.
- Fold/presentation: the four domain tests (§1.1) exact-name
  nominated; the model-mapping fold test for payload rendering.
- Snapshot: the `UndoLabel` widening is the one deliberate insta
  regeneration; the v1 line corpus must otherwise be untouched.
- Kiosk gate: the existing picker-absence probe keeps passing
  (R-2 unchanged); the CLI verbs are platform-free.
- Stage-2 self-check: `scripts/ux-coverage.sh
  .artifacts/plans/2026-11-22-undo-family-journeys-browse-parity.md
  --gate` must exit 0 before handoff (planning-time run: 48 checks,
  0 AMBIGUOUS, exit 1 solely from the pre-landing MISSING rows —
  stage 2 resolves them by landing the wiring and tests).
- Quality gates: `scripts/run-quality-gates.sh` per AGENTS §10;
  memory-gate routing for any heavy single build.

## 4. Handoff

- PR `feat/undo-browse-parity` → `main`; hand off at PR-open +
  `gh pr checks` green (the dispatcher owns the coverage trio, the CI
  watch, teardown, and the at-merge closures).
- On merge: close #1155, #1226, #1227; comment #1156 (the bounded
  browse resolution shipped; the deep-tree inspector stays
  motivation-gated there); run the stage-3 walkthrough protocol (the
  Manual item: payload rows' recognizability for same-verb siblings).
- Sequencing note: D1ii is independent of D1i (different projections
  and surfaces); either order lands clean. If both run in parallel,
  the two branches touch disjoint files except `AppState`'s additive
  fields — rebase the second onto the fetched `origin/main`.

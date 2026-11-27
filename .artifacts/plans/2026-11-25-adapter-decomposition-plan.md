# Adapter Decomposition — Chunk E1 Plan (Round 8, #1349)

Status: **GATE-PAUSED** — this plan is the dispatcher gate. Stage 2
(the mechanical split) dispatches only after the dispatcher confirms
the module map below.

- Branch: `refactor/adapter-decomposition` (worktree
  `../taskboard-WT-refactor-adapter-decomposition`, base `origin/main`
  `28c209da`).
- Normative scope: scope report §E-E1; dispatch plan §10; issue #1349.
- Discipline: mechanical — code moves + visibility/import adjustments
  ONLY. No semantic edits, no public-API changes, no test-case
  changes.

## 1. Subject census (current `main`, `28c209da`)

`crates/taskboard-ui-slint/src/adapter.rs` is **20 067 lines**
(#1349's 18 961-line figures are stale — the file SHRUNK and SHIFTED:
the move-ring family re-homed to domain in #1498 and L1/B1/B4/C2/C3b
edited it). All line ranges below are from a fresh census at current
`main`; stage 2 re-verifies each range immediately before its move
(the file is exclusive-lane-frozen while E1 runs, so drift should be
zero).

Top-level structure:

| Region | Lines | Content |
|---|---|---|
| Header | 1–908 | imports; `StartupWarnings` (64); `CreateFocusHandoff`/`CreateFocusArm` (93/98); `MenuKind`/`MenuSubject` (109/122); `FlowKind`/`Resolve` (134/147); `parse_effort_row` (175); `impl FocusSeam` (255, incl. cfg(test) 404); **`Adapter` struct** (~330–810 area, incl. cfg(test) seams 891) |
| `impl Adapter` A | 908–2598 | sync/models/intercept/route/focus/context machinery |
| `impl Adapter` B (part 1) | 2598–4068 | apply_outcomes, refresh/refresh_forced, timer setter, track form, levels |
| `impl Adapter` B (part 2) | 4068–7215 | settle_flow, walk, interpret + all `interpret_*` helpers, flow_kind/resolve_conflict/resolve |
| `impl Adapter` B (part 3) | 7215–8379 | apply_effect + palette/menu application + habit-menu arming |
| Settings VM block | 8379–8936 | `PendingReply`/`PendingKind`/`ItemAddr` (8379–8472); `SettingsOutcome` (8472); `AutoListBackoff`+`auto_list_suppressed` (8500–8541); `StatusPanelVm` (8541); `SettingsVm` impl (8554/8648); `GeneralField` impl (8724/8742); form mapping free fns (8868–8936) |
| `impl Adapter` C | 8936–12419 | notices/drains/replies/settings application/push_settings (the settings app is 9836–12419) |
| `install` | 12419–12826 | `install` (12419) + cfg(test) seam (12618) |
| Seam free fns | 12826–19359 | ~40 `install_*_seams`/helpers, interleaved small `impl Adapter` blocks (14212, 14320, 15329, 17170, 17287, 17579) |
| Hand-over | 19359–19499 | `impl crate::UiHandle` ×2 (19370, 19436); `paint_files_sync_chip` (19448); `#[cfg(test)] impl Adapter` (19463) |
| Tail test mods | 19499–20067 | 4 `#[cfg(test)] mod`s: effort_parity, auto_list_backoff, menu_label_projection, ui_small_inventory |

## 2. THE MODULE MAP (the gate artifact)

Every line of `adapter.rs` lands in exactly one module below. Ranges
are [start, end) against the census; overlapping-method splits are
named per method. **Per-module ceiling: 3 000 lines** (recorded; the
largest module, `settings.rs`, is ~2 700).

### `adapter/mod.rs` (~1 400 lines) — module root
- Header imports (redistributed per module; each module carries only
  what it uses).
- `StartupWarnings`, `CreateFocusHandoff`, `CreateFocusArm`,
  `MenuKind`, `MenuSubject`, `FlowKind`, `Resolve`, `parse_effort_row`,
  `impl FocusSeam`, the `Adapter` struct — lines 1–908 (imports
  excluded).
- `install` + its cfg(test) seam — 12419–12826.
- Hand-over: both `impl crate::UiHandle` blocks + `paint_files_sync_chip`
  + `#[cfg(test)] impl Adapter` (test_* accessors) — 19359–19499.
- Re-exports preserving the public surface (see §4).

### `adapter/sync.rs` (~1 700) — 908–2598
`impl Adapter` A: entry_states, sync, active_submenu,
sync_undo_picker_models, sync_menu_models, quick_bar_model, menu_model,
intercept, editor_command/editor_scope_arm/special_scope_arms,
disarm_habit_*, sync_quota_popover, set_feedback,
route_selection_verb, schedule_selection, send, route, dismiss_tooltip,
open_focus_site, mask_armed, mint_focus_origin, resolve_anchor,
push_focus_restore, sync_focus_origin, exit_focus_handoff,
finish_mutation, current_chain, chain_with, sync_view_context,
capture_window_geometry.

### `adapter/refresh.rs` (~1 470) — 2598–4068
apply_outcomes_for_test, apply_outcomes, reset_editing_facts,
sync_settings, calendar, today_date, day_ctx, commit_plan_place,
swap_plan_box, compute_seconds_surfaces_active, refresh, quick_time_slots,
refresh_forced, recompose/clear/push_timer_setter, push_track_form,
track-form test accessors, track_form_today, seed_track_form,
commit_track_form(_for_test), reconcile_one_shots, retire_levels.

### `adapter/walk.rs` (~800) — 4068–4865
is_compact, settle_flow, walk_rows, walk_reference, abort_accept/
dismiss/arm, interpret_walk, walk_cursor_task, end_armed_walk_child,
step_walk_row, finish_walk_row, settle_flows_on_leaving, the
`*_for_test`/`test_*` accessor run (4616–4702), responding_flow,
seed_recur_editor, recur_editor_model(_closed).
*Recorded home note:* recur-editor model builders (4715–4865) sit
contiguously here and stay with walk.rs — they are not re-homed to a
recur module (too small to justify the seam).

### `adapter/interpret.rs` (~2 350) — 4865–7215
interpret (4865–6152) and every `interpret_*` helper: push_scroll_page_hint,
page_settings, consume_create_focus_arm, push_focus_scroll_hint,
push_move_scroll_hint, dispatch_traversal, teardown_runtime_drag,
interpret_insert/begin/edit/description/directional/move/lifecycle/
ceremony, activate_rule_cursor, plan_pane_rows, walk_plan_cursor,
begin_plan_place, activate_plan_cursor, control_walk_step,
directional_control_step, settle_foreign_flow_for_modal, flow_kind,
resolve_conflict, resolve.

### `adapter/effects.rs` (~1 160) — 7215–8379
apply_effect (7215–7654), apply_palette_invoke/query, menu_subject_dead,
apply_close_menu, activate_submenu, invoke_entry, set_recur_draft_invalid,
disarm_habit_confirm_staging, arm_outcome_notice, arm_disabled_menu_notice,
open_habit_history_from_subject, habit_edit_subject, arm_no_target_notice,
arm_habit_convert, reproject_habit_menu, habit_menu_states/entries,
habit_move_slots, effective_habit, habit_submenu_page,
open_row_menu_keyboard, open_habit_context_menu, arm_series_convert,
invoke_root_entry, route_menu_action.
*Recorded home note:* the notice-arming trio (arm_outcome_notice /
arm_disabled_menu_notice / arm_no_target_notice) and the habit-menu
helpers are interleaved inside the apply region; they keep their
contiguous home in effects.rs, not notices.rs / a habit module
(ambiguity resolved for contiguity; stage 3 may flag, not move).

### `adapter/notices.rs` (~145) — 8936–9079
record_system_events, push_notice, push_signal_lanes, record_system_event,
drain_delivery_health, drain_files_sync.

### `adapter/replies.rs` (~1 300) — 8379–8472 ∪ 8500–8541 ∪ 9079–9836
`PendingReply`, `PendingKind`, `ItemAddr`; `AutoListBackoff` +
`auto_list_suppressed` (used solely by drain_auto_listings — recorded
home here, not settings_vm.rs); fire_auto_listings, mint_root_entries,
the export-dialog family (9143–9405), drain_auto_listings,
dispatch_status_query, drain_status_reply, status_remediation_*,
export preview/copy family (9630–9802), drain_reconcile_preview.

### `adapter/settings_vm.rs` (~430) — 8472–8500 ∪ 8541–8936
`SettingsOutcome`; `StatusPanelVm`; `SettingsVm` + impl; `GeneralField`
+ impl; preset_index, map_form_field, map_form_kind, to_stroke,
request_scale_anchor.

### `adapter/settings.rs` (~2 700) — 9836–12419 ∪ 14968–14991 ∪ 19097–19359
drain_setting_replies, set_settings_outcome, rotation_tick/rearm/
settings_status_expired, apply_keymap_update, push_settings_validity,
connection_items_model, push_settings, push_general_settings,
settings_field_edited/save/remove/arm/disarm/confirmed/refresh/publish/
files_*/remote_bind/create/disconnect/preset/scale/general_committed/
reminders/focus_*/sink/files_poll/mounted/released, clear_general_draft;
`install_settings_back_seam`; `install_settings_seams`.

### `adapter/seams/` — one file per install family
| Module | Ranges | Lines |
|---|---|---|
| `seams/view.rs` | 12826–12872 `install_view_seams` | ~46 |
| `seams/recur.rs` | 12872–13085 + 13163–13380 (`install_recur_seams`, `install_recur_editor_seams`) | ~430 |
| `seams/export.rs` | 13085–13163 `install_export_seam` | ~78 |
| `seams/habit.rs` | 13380–13737 (`habit_undo_target`, `habit_open_entry`, `install_habit_seams`) + 15006–15309 (`install_habit_editor_seams`) | ~660 |
| `seams/stack_width.rs` | 13737–13842 | ~105 |
| `seams/drag.rs` | 13842–14051 + 14217–14251 (`parse_journal_id`, `drag_target_report` — per-method split from impl 14212) | ~240 |
| `seams/status_panel.rs` | 14051–14138 (`install_status_panel_seam`, `status_panel_data`) | ~87 |
| `seams/toolbar.rs` | 14138–14173 | ~35 |
| `seams/undo.rs` | 14173–14212 + 14291–14304 (`undo_picker_commit`) | ~50 |
| `seams/context.rs` | 14320–14511 (`impl Adapter` context-menu block) + 14511–14559 (`install_board_context_seam`, `install_stack_context_seam`) + 14853–14876 (`open_stack_context`) + 14892–14968 (`install_object_context_seams`) — plus `install_menu_dismiss_seam` (census 14304–14320, the undo.rs→context.rs jump), landed here with its contiguous home in the menu family (called by `install_view_seams`; commit c62ca444) | ~330 |
| `seams/today.rs` | 14559–14853 (`install_today_row_seams`, `focus_tracked_today_row`, `focus_today_row`) | ~294 |
| `seams/navigation.rs` | 14876–14892 (`install_body_seams`) + 14991–15006 (`install_task_back_seam`) | ~31 |
| `seams/shell.rs` | 15309–15329 (switcher flyout free fns) + 15329–15395 (`impl Adapter::switcher_flyout_key`) | ~86 |
| `seams/board.rs` | 15395–15966 (board bar, archived, delete strip, board color, stack create/done, `install_board_seams`, settle_pointer_conflict*, focus_board_card, select_column, commit_text, `install_stack_seams`, activate_stack_row) | ~570 |
| `seams/task.rs` | 15966–16874 `install_task_seam` | ~908 |
| `seams/due.rs` | 16874–17170 (`install_picker_grid_seam`, `install_due_seam`) + 17170–17287 (`impl Adapter` date/description edit) + 17287–17512 (open_due_picker, apply_due_time_edited, sync_buffer_from_picker, settle_date_editor, commit_date, commit_effort_editor) + 14251–14291 (`invoke_picker_row`) | ~640 |
| `seams/banner.rs` | 17512–17579 (`banner_row`, `install_banner_seams`) | ~67 |
| `seams/notification.rs` | 17579–17724 (`impl Adapter` notification verbs) + 17724–17785 (`notification_row`, `slot_importance`, `slot_urgency`, `send_card_track_toggle`, `send_toggled_done`) + 18978–19097 (`install_notification_seams`, `notification_target`) | ~325 |
| `seams/timer.rs` | 17785–18118 (`install_timer_seams`, `apply_time`) | ~333 |
| `seams/tracking.rs` | 18118–18574 (`apply_time` stays here, `install_tracking_seams`) | ~456 |
| `seams/focus.rs` | 18574–18656 `install_focus_seams` | ~82 |
| `seams/review.rs` | 18656–18978 (`install_review_ledger_seams`, `install_review_walk_seams`) | ~322 |

*(apply_time at 18118 belongs to tracking's timer-callback family —
recorded home tracking.rs.)*

### `adapter/tests.rs` (~570) — 19499–20067
The four tail `#[cfg(test)] mod`s moved verbatim into one
`#[cfg(test)]`-gated sibling file (`mod tests;` in mod.rs). No test
code changes; the #1357 in-file-test restructure remains that issue's
follow-up. **Landed deviation (#1508): `auto_list_backoff_tests` instead
landed in `adapter/replies.rs` (commit 2da80494), as a sibling of its
subject type `AutoListBackoff` — the tests assert on its private
fields/methods, which the mapped single-file home would have forced
widening.**

### Completeness check
Union of the ranges above = 1–20067 with no gap and no overlap:
908 | 2598 | 4068 | 4865 | 7215 | 8379/8472/8500/8541/8936 | 9079 |
9836 | 12419 | 12826…19359 (seams table covers every install fn and
every interleaved impl block: 14212–14304 via drag/undo/due per-method
splits; 14320, 15329, 17170, 17287, 17579 in seams) | 19359–19499 |
19499–20067. ✓

## 3. Visibility & import adjustment notes

- The `Adapter` struct and its fields stay **private to the
  `adapter/` tree**: sibling modules access fields directly, so fields
  become `pub(super)` (and `pub(crate)` only where an existing
  accessor already crosses the crate boundary — none known). Method
  visibility: `pub(crate)`/`pub` stays as-is; private methods called
  from sibling modules become `pub(super)`/`pub(in crate::adapter)`.
  This is the mechanical visibility adjustment the discipline allows.
- Free fns/helpers called across siblings get the same
  `pub(super)`-style widening; nothing becomes `pub` outside the crate.
- `install*` functions are called from `mod.rs::install` →
  `pub(super)`.
- Imports: each new module carries its own `use` set, moved (not
  rewritten semantically) from the header; `mod.rs` keeps what the
  root items use. Unused-import pruning that the compiler forces is in
  scope (mechanical).
- `lib.rs` keeps `pub mod adapter;` — the file becomes
  `adapter/mod.rs` (or `adapter.rs` + `adapter/` child dir; stage 2
  uses `adapter/mod.rs`, the Rust-2018 `adapter.rs` + dir spelling is
  equivalent and may be chosen if the compiler layout prefers it —
  either is byte-identical to callers).

## 4. Public-surface preservation

`crate::adapter` externally visible items today: `Adapter`,
`StartupWarnings`, `install`, `SettingsOutcome`, `parse_effort_row`
(pub(crate)). `mod.rs` re-exports all of them (`pub use`) so `lib.rs`
and `tests/` compile unchanged; `UiHandle` hand-over untouched. Proof
of identity: `cargo nextest list` diff is empty; workspace-wide
compile; the full suite + byte-identical golden regen (stage 2/3).

## 5. Ambiguous regions — recorded homes

1. Habit-menu + notice-arming helpers inside the apply region →
   `effects.rs` (contiguity over family purity).
2. Recur-editor model builders (4715–4865) → `walk.rs`.
3. `AutoListBackoff` (8500–8541) → `replies.rs` (sole consumer).
4. impl 14212–14304 mixed helpers → per-method split
   (drag/undo/due) — the only non-contiguous assignment in the map.
5. `apply_time` (18118) → `tracking.rs`.
6. Tail test mods → single `adapter/tests.rs` (not per-family files) —
   #1357 owns any finer restructure. **Landed deviation (#1508):**
   `auto_list_backoff_tests` → `adapter/replies.rs` (commit 2da80494),
   sibling of its subject type — its tests assert on `AutoListBackoff`
   private fields/methods.

## 6. Stage-2 execution order (suggested, one commit per module)

mod.rs skeleton (dir + tests.rs move) → seams/ leaf modules first
(small, self-contained) → settings.rs → replies.rs → notices.rs →
effects.rs → interpret.rs → walk.rs → refresh.rs → sync.rs →
settings_vm.rs → mod.rs final re-exports. After the split:
`cargo nextest list` identity diff, golden byte-identity regen,
clippy/fmt green, line-count-per-module recorded against the 3 000
ceiling.

## 7. Census-context notes

- The prompt's "#1252/#1362-landed guard census" points at
  unrelated issues (a views max-width CI guard and the build.rs probe
  registry) — no adapter-specific guard census exists; the census in
  §1 is the operative one, freshly grepped at `28c209da`.
- #1349's line figures are stale in both directions (18 961 → 20 067
  total; impl/seam anchors shifted); §1 supersedes them.

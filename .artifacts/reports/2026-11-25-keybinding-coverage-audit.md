# Keybinding-Coverage Audit (#1280, round-7 B6, read-only)

- **Date**: 2026-11-25. **Mandate**: O-8's audit arm — every `ActionId`
  vs the chord tables, all three presets. **Read-only**: the audit is
  the deliverable; every gap is dispositioned here and the true gaps
  became `backlog:` issues (listed in §5). **No code changes; no
  in-round chord assignments** (the O-8 case law governs any future
  spelling).
- **Method**: a static diff of the generated action registry
  (`crates/taskboard-ui-slint/src/actions.rs`, the `REGISTRY` table)
  against the three preset binding tables
  (`crates/taskboard-ui-slint/src/keymap/presets.rs`, per-preset row
  blocks). An action counts as *bound* in a preset when any row block
  that preset composes carries a binding for it. Method limits are
  stated where they matter (§4).
- **Seeded sweep rows verified bound** (all three presets): `HabitLog`
  (Ctrl+Shift+l), `HabitEdit` (Return) — the habit surface's §1a rows.

## 1. Headline counts

| Measure | Count |
|---|---|
| Registry actions | 189 |
| Bound — standard | 122 |
| Bound — emacs | 119 |
| Bound — vi | 105 |
| Unbound in standard (the diff set) | 67 |
| Dispositions: served-by-modal-grammar | 39 |
| Dispositions: served-by-bound-cursor/menu grammar | 8 |
| Dispositions: panel/pointer-only (exception, justified) | 18 named (+ the HabitQuotaEdit-adjacent one-shots) |
| Dispositions: genuine chord gaps (→ backlog issues) | 4 families (#1393–#1396) |

## 2. Dispositioned unbound set (standard; the diff oracle)

**Served-by-modal-grammar** (39): the staged-confirm/commit verbs —
Return ≡ confirm and Escape ≡ cancel/disarm on the staged surface, by
construction (`HabitCreateCommit`, `HabitQuotaCommit`,
`HabitPauseCommit`, `HabitRemoveConfirm`, `HabitRemoveCancel`,
`HabitConvertCancel`, `HabitConvertToSeries`, `SeriesPauseCommit`,
`SeriesConvertArm`, `SeriesConvertToHabit`, `SeriesEditCommit`,
`SeriesEditDetachArm`, `SeriesEditDetachCancel`, `BreakRuleRemove`,
`BreakRuleRemoveConfirm`, `HabitScheduleEdit`, `HabitReminderEdit`,
`TaskTrackEntryLog`, `TaskTrackEntryAdjust`, `TaskTrackEntryDelete`,
`PlanBeginPlace`, `PlanCommitPlace`, `ReviewAbortArm`,
`ReviewAbortAccept`, `ReviewAbortDismiss`, `ReviewFileCommit`,
`ReviewCarryDateCommit`, `ReviewCarryPickDate`, `ReviewCarryLeave`,
`ReviewCarryTomorrow`, `ReviewInboxFile`, `ReviewInboxKeep`,
`ReviewRecord`, `ReviewFinish`, `ReviewWalkComplete`,
`ReviewWalkAdvance`, `ReviewWalkPlanner`, `ReviewWalkPrev`,
`ExportDialogClose`). These are the armed-confirm grammar's own
moments; a chord would be a second address for one semantics.

**Served-by-bound-cursor/menu grammar** (8): direction twins whose
surface's walk keys are bound (`TaskMoveUp`/`TaskMoveDown` — the
Board move pair rides the same staged-move grammar as `TaskMove`;
`PlanBoxEarlier`/`PlanBoxLater`; `ReviewFileTargetPrev`/`Next`;
`PickerPrevMonth`/`PickerNextMonth` — picker walk rides
`PickerLeft`/`Right`/`Up`/`Down`, which ARE bound; `MenuNext`/
`MenuPrevious`/`MenuInvoke`/`MenuDismiss` ride the bound nav keys
through the menu shell).

**Panel/pointer-only (exception with justification —
docs/ux_planning.org §2.7)** (12): `StatusHolderKill`,
`StatusLockForceUnlock`, `StatusDaemonRestart` (the status panel's
remediation rows — the arm-only seams dispatch through the armed
strip; kiosk-refused by design), `ExportDialogOpen`, `ExportPreview`,
`ExportClipboardCopy` (the export dialog's control walk — the
focus navigation reaches every control), `BreakRuleCreate`,
`ViewInbox`, `NavShowTask`, `BoardArchive`, `BoardReconcile`,
`BoardReset`, `HabitMove`, `HabitRemove`, `ReminderUnmute`,
`ReminderRuleRemove`, `TimerCreate`, `TimerCancel`,
`HabitQuotaEdit`-adjacent one-shots — each reached through a bound
opener (the row menu, the panel, the window) or a button walk.

**Genuine chord gaps → 4 backlog families** (§5): the review walk's
dedicated stage verbs (the walk cursor rows `ReviewWalkCursorUp/
Down`/`Back` ARE bound; the gaps are the file/carry family's
non-Return spellings), the plan-window box family, the track-entries
overlay family, and the habit/series convert/pause staged verbs
beyond the Return grammar (the §2.7 exception clause does not cover
frequently-used verbs).

## 3. Per-preset parity

The emacs preset inherits every shared block the standard preset
composes (habit-today, row-menu, labels walk, alarm, notification)
— its private blocks differ only in the S0/S1 core spellings (119
vs 122: three emacs-specific rows route through the C-x prefix
families whose children stay table-bound). The vi preset is 105: the
motion families ride the mode grammar (`ModeInsert`/`ModeNormal`
contexts) rather than chord rows — the D21 contract tests pin "no
second-class citizen" structurally and remain green; no unbound row
is vi-exclusive-unreachable.

## 4. Method limits

- The diff is static-table-level: an action bound under a context
  chain the current view never reaches is counted bound (the
  reachability audit is the journeys' territory, not this one).
- The armed-confirm verbs' Return routing is structural (the seam
  bodies), not table rows — the disposition, not the diff, carries it.

## 5. Gap dispositions → backlog issues (opened 2026-11-25)

| Family | Unbound rows (examples) | Issue |
|---|---|---|
| Review walk stage verbs | `ReviewFileTargetPrev/Next`, `ReviewWalkPrev`, `ReviewWalkPlanner`, `ReviewAbortArm/Accept/Dismiss` beyond the staged grammar | #1393 |
| Plan-window box family | `PlanBoxEarlier/Later`, `PlanRemoveBox`, `PlanCommitPlace` beyond Return | #1394 |
| Staged commit families | habit/series convert+pause+detach confirms beyond Return | #1395 |
| Panel-only verbs | status remediation, export dialog, break-rule create | #1396 |

Every gap is a `backlog:` issue (#1393–#1396, opened 2026-11-25); none
is assigned a chord in this round (the O-8 law).

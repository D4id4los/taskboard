# Habit Time Tracking — A3 Dispatch Plan (round 6)

- **Date**: 2026-11-24
- **Kind**: dispatch plan — chunks the accepted round-6 A3 feature
  (issue #1305).
- **Origin**: `.artifacts/plans/2026-11-24-habit-tracking-ux-plan.md`
  (the UX journey plan; its story/step/wiring/ruling IDs are cited
  verbatim below) under the round-6 scope report §A A3
  (`.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`).
  The journey plan must be **accepted by the operator before chunk 1
  dispatches** (AGENTS.md §7 — the plan gates the feature).
- **Status**: DRAFT — awaiting journey-plan acceptance.
- **Branch convention**: one branch and PR per chunk,
  `feat/habit-time-tracking-<slice>`; stacked onto `main`
  (rebase-merge; the §8 rebase dance applies if a stack forms).
- **Sub-agent discipline**: per `docs/agent_dispatch.org` — the
  coverage trio never runs inside a subagent; stages gate on
  pre-commit hooks + flock-wrapped `cargo nextest`; the dispatcher
  verifies each stage's landed state before dispatching the next.

## 1. Feature summary

Habit time tracking, end to end: a new additive habit-side time-entry
fact family (domain), engine reduction with replay-deterministic
clocks, sqlite persistence, an additive sync lane, and the live UI
affordances that repair the dead play button (#1305). All recorded
semantics decisions live in the journey plan §1 D-1..D-8; chunks
implement them, never re-decide.

## 2. P1 slice rule

P1 stories: US-30 (row start), US-31 (menu + chord parity), US-32
(running display), US-33 (persistence + sync durability). The P1
slice is **all four chunks**: a P1 journey is complete only when the
verb dispatches (C1), reduces and survives restart (C2), syncs (C3),
and is affordance-complete with escapes (C4). No chunk order leaves a
P1 step unwired at the end of the slice; chunks may merge out of
order (each is independently green) but the feature is done only when
all four land.

## 3. Chunks

### C1 — domain: the `HabitTimeEntry` fact family

- **Branch**: `feat/domain-habit-time-tracking`
- **Size**: S/M
- **Implements**: journey plan §1 D-1/D-2/D-3/D-4; command delta
  `StartHabitTracking`/`StopHabitTracking`.
- **Scope**:
  - `taskboard-domain`: `HabitTimeEntryId` (the ids.rs mint family),
    `HabitTimeEntry { id, habit, habit_title, start, end: Option,
      created_at, updated_at, by }` (the `TimeEntry` shape, habit
      keyed, no `kind`), `HabitState.time_log:
      BTreeMap<HabitTimeEntryId, HabitTimeEntry>` with
      `#[serde(default)]` (the `day_log` additive law), the pure
      helpers (`is_open`, `duration`, the newest-open pick over habit
      entries), and the two command variants +
      typed outcomes (unknown/tombstoned habit rejection) in
      `messages.rs`.
  - Replay determinism per #986/#1095: every stamp is a pure function
    of (pre-state, command, `now`); stop stamps `end = now`
    unconditionally (total); the single-global close (D-3) closes
    task `TrackingState` open entries too — the reduction composes
    both maps deterministically.
  - The D-2 invariant proptest: `time_log` mutations leave `streak`,
    `adherence`, `log_window`, `statistics_horizon` byte-identical.
  - Changeset/journal plumbing for the new state field (additive).
- **Journey IDs**: US-30/S2, US-31/S3 (reducer halves)
- **Tests**: domain unit + proptest catalogue
  (`habit_time_entry_serde_roundtrip`, `old_state_without_time_log_decodes_empty`,
  `start_closes_task_and_habit_open_entries_at_now`,
  `stop_is_total_and_stamps_now`,
  `time_log_mutations_leave_all_habit_projections_identical_prop`,
  clock-determinism replay unit). Typed outcomes; no text
  assertions; `tokio::time::pause/advance` where async is involved.

### C2 — state engine + storage

- **Branch**: `feat/state-storage-habit-time-tracking`
- **Size**: M
- **Implements**: journey plan D-1 persistence leg; US-33/AC-1.
- **Scope**:
  - `taskboard-state`: reduce the two commands (outcomes, undo
    entries per the undo family's existing tracking precedent,
    snapshot emission through `ArcSwap<AppState>`), insta snapshot
    coverage of the new command family.
  - `taskboard-storage-sqlite`: migration `NNNN_habit_time_entries`
    (the `time_entries` table shape, `habit_id`/`habit_title`
    columns, no foreign keys — history outlives tombstones,
    D-4), persistence actions upsert/load, the additive
    default-absent load law.
- **Journey IDs**: US-32/AC-3 (restart rehydration), US-33/S1
- **Tests**: storage round-trip seam test; rehydration seam test
  (a persisted open entry renders running after reload —
  `us32` token rides the C4 journey file via a `bound-test:`-free
  nomination here: state-level tests assert the engine outcome, the
  UI journey test asserts the render); snapshot tests.

### C3 — sync: the `habit-time-entries` lane

- **Branch**: `feat/sync-habit-time-entries`
- **Size**: M
- **Implements**: journey plan D-6; US-33/AC-2/AC-3.
- **Scope**:
  - `taskboard-sync-nextcloud`: the additive board-level
    `habit-time-entries` wire section (structure mirrors the
    `time-entries` lane, habit-keyed), the retention bound
    (`TRACKING_SYNC_RETENTION_DAYS`, the D-T10/D-H14 precedent),
    `board_has_local_app_data`'s habit-time clause, reconcile/fold
    for peer rows (LWW on `updated_at`), the wire GC beside the
    time-entries GC.
  - Old documents without the section decode unchanged (additive).
- **Journey IDs**: US-33/S2, US-33/S3
- **Tests**: flush-section seam test (typed wire assertion), fold
  test (peer rows land in `time_log`), retention-bound test, old-doc
  decode test.

### C4 — UI: live play control, menu row, chord, display

- **Branch**: `feat/ui-habit-time-tracking`
- **Size**: M
- **Implements**: journey plan W-1..W-5; US-30, US-31, US-32.
- **Scope**:
  - `habits.slint`: connect the card's `track-toggled` to the new
    `habits-track-asked(int)` callback (W-1); the running/elapsed
    chip (W-5).
  - `adapter.rs`: `install_seam!` for the callback +
    `send_habit_track_toggle` (the `send_card_track_toggle` twin
    reading the habit's open-entry state).
  - `actions.rs`/`router.rs`: `ActionId::HabitTrackToggle`
    (`"habit.track-toggle"`), the focused-habit target resolution,
    the keymap preset chord (D-8, §2.7 law).
  - `menu.rs`: the `HABIT_CONTEXT` tracking row (W-2), ordered per
    the destructive-last/menu-grammar laws.
  - `models.rs`: `habit_card` fold sets `track_state` from the open
    habit entry (W-4); today `None` is hardwired.
  - Kiosk: the control renders disabled under `editing-enabled ==
    false`; the chip renders (display is not an edit verb).
- **Journey IDs**: US-30/S1–S3, US-31/S1–S3, US-32/S1–S2
- **Tests**: the journey-test file
  `crates/taskboard-ui-slint/tests/journeys/habit_tracking_journeys.rs`
  with the §8-nominated names; fold tests in `models.rs`; kiosk-gate
  structural probe.
- **Stage-3 mandate**: run
  `scripts/ux-coverage.sh .artifacts/plans/2026-11-24-habit-tracking-ux-plan.md --gate`
  plus the walkthrough protocol for the plan's P1 stories.

## 4. Wiring rows (from the journey plan §6)

W-1..W-5 as tabulated there; each chunk's scope names its rows. The
command-contract delta (journey plan §7) is the derivation hinge: C1
exists because US-30/S1 dispatches `StartHabitTracking`; C4 exists
because W-1/W-2/W-3 need a route.

## 5. Sync-wire delta (summary)

| Section | Status | Retention | Fold law |
|---|---|---|---|
| `habit-time-entries` (new, board-level) | additive | `TRACKING_SYNC_RETENTION_DAYS` (wire GC; sqlite keeps local history) | LWW on `updated_at`, `by` duel rank as F1 |
| `habits` / `habit-log` | untouched | unchanged | unchanged |
| `time-entries` (task lane) | untouched | unchanged | unchanged |

## 6. Non-goal backlog issues

Filed at plan time per the standing authorization; numbers recorded
here and in the final report:

- `backlog:` habit retroactive time-entry valve (`LogTime` twin) —
  #1316
- `backlog:` habit time statistics/report surfaces — #1317
- `backlog:` CLI `habit track` verb — #1318
- `backlog:` habit time entries' Work/Respite kind split (if a
  break-reminder consumer for habits ever exists) — #1319

## 7. Out of scope

Everything not in C1–C4: no `TimeEntry` entity rewrite (O-2), no
O-1 window/streak changes (journey plan D-2), no Review/Settings/CLI
surfaces (journey plan R-2..R-4), no concurrent timers (D-3).

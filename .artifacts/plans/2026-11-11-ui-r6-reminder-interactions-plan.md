# [Plan] UI R6 Chunk 1 — Reminder Interactions (D-6.1 + D-6.2)

- **Date**: 2026-11-11
- **Status**: STAGE-1 PLAN for dispatch-plan chunk 1
  (`.artifacts/plans/2026-11-11-ui-r6-dispatch-plan.md` §2). TDD-first;
  every open question settled with a recorded decision below.
- **Origin (normative)**:
  `.artifacts/plans/2026-11-11-gui-review-round6-plan.md` §2 rows
  R6-1/R6-2 and §3 D-6.1/D-6.2.
- **Branch**: `fix/ui-r6-reminder-interactions` (this worktree).
- **Anchors re-verified** against `main` @ `67a5b03d` (the origin plan's
  anchors had drifted as it predicted; the findings are unchanged). Key
  re-verified anchors:
  - `ui/compositions/status-bar.slint:130-188` — banner row loop, the
    row TouchArea at 164-169 (`clicked` = focus + `row-activated`).
  - `ui/compositions/notification-surface.slint:16-29` —
    `NotificationRowModel`; `:52-60` — the non-interactive `ListRow`.
  - `src/adapter.rs:8336-8376` — `banner_row()` + `install_banner_seams`
    (`on_banner_row_activated` / `on_banner_snooze_clicked` /
    `on_banner_mute_clicked`); `:9197-` `install_notification_seams`;
    `:9285-9296` `notification_target`.
  - `src/adapter.rs:7164-7170` — `on_today_row_activated` (the
    double-click seam precedent: focus, then `route(TaskEdit)`).
  - `ui/widgets/task-card.slint:106-111` — `TouchArea.double-clicked`
    exists and is used (Slint 1.18.1 confirmed in-tree).
  - `src/presentation.rs:652-700` — `BannerRow` / `banner_rows` /
    `snooze_target`; `:706-718` — `NotificationRow`.
  - `src/models.rs:2299-2323` — the `BannerRowModel` build site.
  - `src/router.rs:798` — `ActionId::TaskEdit → UiEffect::Activate`;
    `src/adapter.rs:3236` — Activate → `view_state.activate_focused`
    (`view_state.rs:1643-1650`: sets `ViewId::TaskDetail`, returns
    `RefreshNow`; **empty when no focused task** — the seams must set
    focus first).
  - Tests to keep green: `src/lib.rs:2792-2835`
    `banner_actions_send_exact_commands`, `:2843-`
    `notification_surface_buttons_send_exact_commands`.

## 1. D-6.1 — double-click opens the task view

### D1 — banner row double-click (status-bar.slint)

- Add `callback row-open-requested(int)` beside the existing
  `row-activated(int)` (which keeps its exact semantics: focus + arm,
  unchanged).
- The row TouchArea (status-bar.slint:164-169) gains:

  ```slint
  double-clicked => { root.row-open-requested(row-index); }
  ```

- **Decision (sequencing)**: Slint fires `clicked` on each press-release
  and additionally `double-clicked` on the second; the existing
  `clicked` arm (focus + `row-activated`) therefore runs first and stays
  untouched — the same sequencing `TaskCard` / the today rows already
  live with (`task-card.slint:106-111`). The `double-clicked` arm does
  **not** re-focus or re-arm: `clicked` already did, and both are
  idempotent. Keyboard parity is unchanged (focus → Enter → TaskEdit
  rides `row-activated`'s existing dispatch; no keymap work — #724 stays
  a non-goal).

### D2 — notification row interactivity (notification-surface.slint)

- `NotificationRowModel` gains `can-open: bool`.
- The row's `ListRow` (`row-info`, :55-60) is wrapped in a Rectangle
  carrier with a TouchArea:

  ```slint
  row-box := Rectangle {
      horizontal-stretch: 1;
      background: row-touch.has-hover && row.can-open ? Theme.surface-hover : transparent;
      row-info := ListRow { text: row.title; meta: row.fired-phrase; tone: Tone.info; }
      row-touch := TouchArea {
          enabled: row.can-open;
          double-clicked => { root.row-open-requested(row-index); }
      }
  }
  ```

  plus `callback row-open-requested(int)` on the component.
- **Decision (single click stays inert)**: notification rows have no
  click semantics today and D-6.1 specifies only the open-on-double-
  click; no `row-activated` twin is invented here (no focus semantics
  exist on this surface to reuse — inventing one is out of scope).
  `TouchArea.enabled: row.can-open` makes non-task rows (adhoc alarms)
  neither advertise (hover) nor accept a double-click.

### D3 — adapter seams (adapter.rs)

Mirroring `on_today_row_activated` (:7164) — resolve, focus, route:

```rust
// in install_banner_seams:
install_seam!(window: window.on_banner_row_open_requested, adapter: adapter;
bind mut (a, window), args (index), {
    let Some(banner_row) = banner_row(&a, index) else { return; };
    a.view_state.set_focused(Some(banner_row.task));
    a.reminder_target = Some(banner_row.target);
    a.route(&window, ActionId::TaskEdit);
});
```

```rust
// in install_notification_seams:
install_seam!(window: window.on_notification_row_open_requested, adapter: adapter;
bind mut (a, window), args (index), {
    let Some(row) = notification_row(&a, index) else { return; };
    let Some(task) = row.task else { return; }; // alarm rows never open
    a.view_state.set_focused(Some(task));
    a.route(&window, ActionId::TaskEdit);
});
```

- **Decision (no `refresh_forced`)**: unlike `on_banner_row_activated`
  (which mutates focus state and forces a refresh), the open seams end
  in `ActionId::TaskEdit` → `UiEffect::Activate` →
  `activate_focused` → `FlowOutcome::RefreshNow`, whose apply path
  performs the push. No extra refresh, no new route, no engine command.
- **Decision (notification seam does not arm `reminder_target`)**:
  nothing downstream consumes it on this path; every snooze/mute seam
  arms the one-shot target itself before routing (dispatch-plan review
  focus), so a stale arm would be dead state. The banner seam **does**
  arm it — it mirrors `on_banner_row_activated`'s existing behaviour
  (a click arms the target so a following snooze/mute button click is
  pre-aimed; that is today's D4 contract, preserved verbatim).
- Stale indices stay inert (the existing `banner_row`/`notification_row`
  resolvers already absorb them). A `TaskEdit` with no focused task
  degrades to a no-op (`activate_focused` returns empty).
- **Note**: `set_focused` + route never fires for an alarm row because
  the markup gates on `can-open` **and** the seam re-gates on
  `row.task.is_some()` — belt and braces; the seam test pins the seam
  side (the markup side is not unit-testable).

## 2. D-6.2 — muted/snoozed state truth on the banner rows

### D4 — where snoozes live in AppState (the plan's open question — RESOLVED)

**Journaled snoozes are already retained in `AppState`; no retention
change is needed.** The apply path for `RecordReminderSnooze`
(`taskboard-domain/src/persistence.rs:522-528`) inserts into
`state.reminders.snoozes: BTreeMap<ReminderSnoozeKey, DateTime<Utc>>`
(`reminders.rs:664-681`; key = `(reminder, occurrence)`, value =
`rescheduled_to`). Mutes live in `state.reminders.muted:
BTreeSet<TaskId>`. The banner row's own target
(`presentation.rs:690-700` `snooze_target`) produces exactly the
`(reminder, occurrence)` pair the `SnoozeReminder` command records, so
the lookup key is available on `BannerRow` today. Compaction
(`compact_snooze_chains`, applied post-batch) already prunes settled
chains — the same home the engine prunes deliveries in, as the origin
plan anticipated.

### D5 — presentation model (presentation.rs + models.rs)

- `BannerRow` gains two fields (structural, testable — AGENTS §5 bans
  text assertions, so the *phrase* is never the test oracle):

  ```rust
  pub struct BannerRow {
      pub task: TaskId,
      pub kind: DueKind,
      pub target: ReminderTarget,
      pub muted: bool,                      // task in state.reminders.muted
      pub snoozed_until: Option<DateTime<Utc>>, // active snooze link for (target.reminder, target.occurrence)
  }
  ```

  `banner_rows(state, now)` computes both:
  `muted = state.reminders.muted.contains(&task.id)`;
  `snoozed_until = state.reminders.snoozes.get(&ReminderSnoozeKey {
  reminder: target.reminder, occurrence: target.occurrence
  }).filter(|&&to| to > now).copied()` — an expired link renders as not
  snoozed (the occurrence has effectively re-fired).
- `BannerRowModel` (status-bar.slint:40-43) gains `muted: bool` and
  `snoozed-phrase: string` (empty = not snoozed), mapped at the
  models.rs:2303 build site. New presentation helper
  `snoozed_phrase(until: DateTime<Utc>, tz: &impl TimeZone) -> String`
  rendering `"snoozed until %H:%M"` in local time (the due-phrase
  helpers' formatting shape; wall-clock text lives only in display
  strings, never in assertions — clock discipline §6.1).
- **Decision (model carries bools AND the phrase)**: the Slint model is
  a dumb projection; the bools exist for the tests and for the markup's
  tone switch, the phrase for the meta text. No UI-local state.

### D6 — row rendering (status-bar.slint)

Precedence ladder on the row's `ListRow` (`tone` / `meta`):

1. **muted** → `meta: "reminders muted"`, `tone: Tone.neutral`
   (dimmed). Board truth: the row itself stays visible — the banner's
   mute-blind visibility contract (`presentation.rs:653-655`) is
   untouched; only the row's *description* of itself changes.
2. **snoozed** (not muted) → `meta: row.snoozed-phrase`,
   `tone: Tone.overdue` (still overdue, just re-scheduled).
3. otherwise → today's rendering verbatim.

- **Decision (muted wins over snoozed)**: mute means "stop nagging"
  entirely (`reminders.rs` doc on `muted`) — a muted task's stale snooze
  link must not suggest a future nag. One ladder, no simultaneous
  states.
- **Decision (no new tone/glyph token)**: `Tone.neutral` for the muted
  dimming is an existing token; no theme change. A mute *glyph* is not
  added — the meta copy is the signal, and glyphs on ListRow would
  widen this chunk's blast radius into `list-row.slint` for no
  acceptance gain. (Recorded as deliberate scope; revisit only if the
  user's round-7 review still finds muted rows insufficiently distinct.)
- **Notification-surface Unmute toggle**: D-6.2 marks it "optional
  ride-along, not a requirement" — **Decision: skipped**; filed as a
  backlog issue (see §5) so the idea survives the chunk.

## 3. Test catalogue (TDD order)

All new tests live beside the harnesses they mirror
(`src/lib.rs`, `src/presentation.rs` tests). No `tokio::sleep`, no text
assertions, typed/structural outcomes only.

1. **Seam: banner double-click opens the task view**
   (`banner_double_click_opens_the_task_view`, lib.rs, mirroring
   `banner_actions_send_exact_commands`' harness): `fixture_state()` +
   `handover_with`; `invoke_banner_row_open_requested(0)`; assert the
   structural outcome — `view_state` root is `TaskDetail` with
   `subject_task == overdue_id` (the same observable the today-row
   activation tests use) and no command dispatched on the channel.
2. **Seam: notification row opens only for task anchors**
   (`notification_row_open_gates_on_task_anchor`, lib.rs): the task-rows
   case as (1); the alarm-row case asserts `can-open == false` on the
   pushed model and `invoke_notification_row_open_requested(0)` leaves
   the root view unchanged (structural: still the prior `ViewId`).
3. **Presentation: banner rows reflect muted state**
   (`banner_rows_reflect_muted_tasks`, presentation.rs tests): mutate
   `state.reminders.muted` to contain the overdue task →
   `banner_rows(...)` row `muted == true`; without it, `false`.
4. **Presentation: banner rows reflect an active snooze**
   (`banner_rows_reflect_active_snooze_links`): apply
   `PersistenceAction::RecordReminderSnooze` through
   `apply_actions` (the real journal path, not a hand-inserted map
   entry) with `rescheduled_to` anchored relative to `now` → row
   `snoozed_until == Some(rescheduled_to)`; a link in the past →
   `None`.
5. **Model: the banner row projection carries the state**
   (models.rs tests, beside the existing push tests): from a muted
   state, the pushed `BannerRowModel` has `muted == true` and an empty
   `snoozed-phrase`; from a snoozed state, `muted == false` and a
   **non-empty** `snoozed-phrase` (non-emptiness is the structural
   fact; the text never is). This is the one-behaviour-one-test layer
   check: (3)/(4) own the state→struct truth, (5) owns struct→model
   only.
6. **Unchanged pins**: `banner_actions_send_exact_commands` and
   `notification_surface_buttons_send_exact_commands` must pass
   unmodified — the dispatch contracts do not move. Existing status-bar
   / notification goldens may shift if any golden pins the banner row's
   meta/tone; regenerate via the standard flow and record the regen in
   the commit (the muted/snoozed fixture states are new, so most
   goldens are untouched).

## 4. Non-goals (restated from the dispatch plan §2)

No keymap/keyboard-activation work (#724); no alarm-window changes; no
change to banner visibility semantics (board truth stays); no reply
channels on the commands; no Unmute toggle on the notification surface
(backlog); no engine/persistence changes (snoozes are already retained
— D4).

## 5. Backlog

- New issue: `backlog: notification rows offer an Unmute toggle when the
  task is muted` (the D-6.2 optional ride-along, deferred here) — the
  model would carry `muted` per notification row and the Mute button
  toggles.
- #724 (keyboard activation parity on notification/alarm rows) already
  covers the keyboard gap D-6.1 names — no new issue.

## 6. Risks

- **Double-click sequencing**: the plan relies on Slint's
  clicked-then-double-clicked ordering, which the today rows already
  depend on (TaskCard precedent). Low risk, in-tree precedent.
- **Snooze-key equality**: the banner row's `snooze_target` derives
  `(reminder, occurrence)` from the *current* duedate/rule set. If the
  duedate changed after the snooze, the lookup key no longer matches
  and the row honestly renders unsnoozed — the snooze link applies to
  the occurrence it was recorded against, not to the row. Accepted
  semantics (state truth, not intent recall); no special-casing.
- **Golden drift**: muted/snoozed rows change tone/meta; any golden
  pinning a banner row's rendering shifts. Regen flow applies.
- **Hover carrier on notification rows**: wrapping the ListRow in a
  Rectangle+TouchArea must preserve the HorizontalLayout's stretch
  (`horizontal-stretch: 1` moves to the carrier); a missed transfer
  would compress the row's title column. The seam test asserts
  behaviour, not geometry — a quick manual/route check of the surface
  is part of stage 2's verification.

## 7. Estimated diff

~250–350 lines: status-bar.slint (~+15), notification-surface.slint
(~+20), adapter.rs (~+40), presentation.rs (~+45 incl. helper),
models.rs (~+15), tests (~+150). S–M per the dispatch plan.

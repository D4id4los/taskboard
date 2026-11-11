# [Plan] GUI Review Round 6 — Analysis & Solutions

- **Date**: 2026-11-11
- **Status**: STAGE-1 PLAN (analysis + proposed solutions; no code). Written
  from the user's nine round-6 review observations. After acceptance this
  plan chunks into a dispatch plan per `.artifacts/templates/dispatch-plan.md`.
- **Origin**: the user's GUI review round 6 observation list (2026-11-11),
  plus GitHub issue #721 (fix requested in the list).
- **Verified against**: `main` @ `53ffa1fc`, re-anchored against `main` @
  `ca961b3d` (the boards-over-files wave landed mid-review; §3 D-6.8 reflects
  the landed tree). All file:line anchors were read at those SHAs; expect
  drift by implementation time — re-verify anchors, the findings themselves
  are stable.
- **Revisions**: 2026-11-11 (later same day) — R6-4 re-diagnosed with probe
  evidence (the width-resolver theory was wrong; the real defect is the
  empty-decorations-row layout-info collapse, D-6.4 §3) and D-6.8 phase 2
  reshaped after the DR-9 chunks landed on `main`.
- **Related issues**: #721 (fixed in scope, R6-5), #699 (publish direction,
  R6-9), #686–#689 (DR-9 chunks 3–6, gate R6-8 phase 2).

---

## 1. Scope

Nine observations, four code areas. Each observation below gets a verified
root cause and a proposed solution; §5 proposes the chunk/branch split.
Everything is UI/glue/app-layer except where noted; no domain-model changes
are required by any finding (the domain already carries every message the
solutions dispatch).

## 2. Findings: verified root causes

| # | Observation | Verified root cause | Primary anchors |
|---|---|---|---|
| R6-1 | Double-click on a notification/overdue-reminder row should open the task view | Banner rows have only single-click (`row-activated` → focus+arm); the notification surface's rows are **not interactive at all** (`row-info := ListRow`, no TouchArea). Slint 1.18.1 `TouchArea.double-clicked` exists and is already used by `TaskCard` (the today-view precedent) | `ui/compositions/status-bar.slint:130-169`; `ui/compositions/notification-surface.slint:55-60`; `ui/widgets/task-card.slint:106`; `src/adapter.rs:7088-7093` (the today double-click seam to mirror) |
| R6-2 | Snooze/Mute buttons of overdue reminders "do nothing" | They are **wired and succeed** — seam → `route(ActionId::ReminderSnooze/ReminderMute)` → `StateCommand::SnoozeReminder`/`SetTaskRemindersMuted` → engine planner (guards cannot fire for banner rows: `ReminderRef::Default(live task)`) → persistence. The "nothing" is *perceptual*: (a) banner visibility explicitly ignores mute/snooze ("board truth, not nagging policy", `presentation.rs:649-651`), (b) the banner row model carries no muted/snoozed state, (c) fire-and-forget commands produce no acknowledgement — the click has **zero visible effect** on the surface that owns the buttons | `src/router.rs:549-565`; `src/adapter.rs:8290-8307`; `crates/taskboard-domain/src/command.rs:1983-2059`; `src/presentation.rs:649-697`; tests `src/lib.rs:2792-2843` prove dispatch |
| R6-3 | Stack min column width should be a fixed default, not title-length-derived | `content_min_px(title_chars, scale)` = max(readability floor 120·scale, `chars·9 + 64`) — a documented text-width *estimator*. Title elision support already exists unconditionally on the header (`overflow: elide`, header rect `clip: true`), so the estimator term can be dropped without losing anything | `src/widths.rs:30-54` (estimator + constants, only used here); call site `src/models.rs:1607-1616`; elide `ui/compositions/board-layout.slint:455-469` |
| R6-4 | Task names elided although horizontal space exists | **Verified by probe** (`tests/task_card_title_probe_tests.rs`, reproducible): the title's width is CONSTANT (108 px) across card widths 250→500 px. Root cause: the tertiary decorations row — `if (!root.compact): HorizontalLayout { for decoration in … }` with an **empty** `decorations` list — reports `LayoutInfo { min: 0, pref: 0, max: 0 }` (i-slint-core `box_layout_info`'s empty-layout early return pins `max = min` for stretch-aligned layouts, `layout.rs:1402-1409`), and the text column `VerticalLayout`'s constraint merge (`LayoutInfo::merge`: `max = min(child maxes)`) collapses the whole column's max to it — the column cannot grow, so the title elides at column-padding and the chips overflow via their own min clamp. The r4 D1 `horizontal-stretch: 1` fix could not help: stretch distributes surplus only to cells with `max > size`. Probe-measured column constraints pre-fix: `min=pref=max=166` at any card width | `ui/widgets/task-card.slint:219-241` (the row); solver `~/.cargo/…/i-slint-core-1.18.1/layout.rs:1394-1421, 52-73`; probe `tests/task_card_title_probe_tests.rs` |
| R6-5 | Fix #721 — settings save refuses a config with no `[sync]` section (`PersistRejected`) | `ensure_migrated` maps `migrate_config_file → Ok(false)` to `PersistRejected`, but `Ok(false)` also covers the ordinary "nothing legacy to migrate" cases (no file, no `[nextcloud]` identity keys). The writer backstop then never runs; and the backstop itself (`connectors_item`) refuses a missing `sync.connectors` instead of creating the subtree | `crates/taskboard-app/src/ui_requests.rs:1236-1259`; `crates/taskboard-app/src/migration.rs:193-214`; `crates/taskboard-app/src/connections.rs:99-127, 192-243` |
| R6-6 | Only the first account is offered the Files (file-sync) option | A deliberate contract (D-SF10), not an index bug: the Files item is emitted only for the card whose identity equals `rows.first()`, reflecting the single scalar `[files_sync] enabled`; the files-sync actor runs under the *first* connector's identity only (`specs.first()`) | `src/adapter.rs:4600-4632`; `crates/taskboard-app/src/bootstrap.rs:118-136`; `crates/taskboard-app/src/config.rs:1556-1558`; copy `src/settings_status.rs:261` |
| R6-7 | Deck connection not shown after restart until "Refresh" | `SettingsVm.remotes` ("the last listing per connector") is session-ephemeral and **only ever written by the explicit `ListRemoteBoards` reply** (`drain_setting_replies`); neither `Adapter::install` nor `settings_view_mounted` lists anything. Remote rows — and the bindings rendered *inside* the listing loop — vanish until a listing lands | `src/adapter.rs:3857-3859, 4265-4267, 4655-4683, 5890-5906`; request path `crates/taskboard-app/src/ui_requests.rs:352-355, 835-885` |
| R6-8 | No board list over the Files mechanism; connector rows read "Deck" even with Files on | Two layered facts: (1) the connector rows are hardcoded `ConnectionItemKind::Deck` / `"Deck · " + title` and `ConnectorInfo` carries no backend, so the UI cannot be mechanism-aware; (2) the boards-over-Files **engine work has landed** (DR-9 chunks 1–6 on `main` @ `ca961b3d`: `RemoteBoardId::Files(Uuid)` at `ids.rs:133`, board lanes riding the files actor, persistence, the resumable `boards move --to-files` ceremony, cross-device adoption via `plan_board_adoption`, `taskboard-state/src/actor.rs:1017`) — but **no UI surface exists for it**: `ConnectionItemKind` is still `{account, files, deck, remote}`, the Files item is only the On/Off toggle, and `UiRequest` has no files-boards listing/move variant. Design note: adoption makes files-boards **automatic** — there is no connect step to expose; the missing surface is visibility + the move flow | `src/adapter.rs:4636-4651`; `src/requests.rs:37-41, 154-155, 471-477`; `ui/views/settings-view.slint:71-76, 917`; `crates/taskboard-app/src/cli.rs:269-289`; adoption `crates/taskboard-state/src/actor.rs:1008-1026` |
| R6-9 | Need "publish local board → new board on the server" (reverse of the Deck import) | The whole ceremony **already exists at CLI/REPL level**: `boards publish` → `create_board` REST → `BindBoard` → orphan sweep → outbox drain (`crates/taskboard-app/src/commands/boards.rs:628-874`). Missing is only the UI entry point. #699 tracks exactly this | `crates/taskboard-app/src/cli.rs:317-330`; `crates/taskboard-app/src/commands/boards.rs:628-874`; Deck client `crates/taskboard-sync-nextcloud/src/client.rs:525, 1042, 1110`; UI precedent `src/adapter.rs:5278` (`settings_create_item`) |

## 3. Solutions (design decisions)

### D-6.1 — Reminder rows: double-click opens the task view (R6-1)

Mirror the today-view pattern (`TaskCard`'s `clicked`/`double-clicked` split,
`today-view.slint:356-362`):

- **status-bar.slint**: keep `row-activated(int)` = focus+arm (single click,
  unchanged). Add `callback row-open-requested(int)`, fired from the row
  TouchArea's new `double-clicked =>` arm. A double-click fires `clicked`
  first (focus+arm, idempotent) then `row-open-requested` — the same
  sequencing the today rows already live with.
- **notification-surface.slint**: make the row interactive — wrap the
  `ListRow` in a TouchArea (or add the row's own clicked/double-clicked
  seams). Add `can-open: bool` to `NotificationRowModel` (true iff
  task-anchored; `notification_target` at `adapter.rs:9173-9190` already
  computes the anchor) so non-task rows (alarms) don't advertise
  interactivity.
- **adapter seams**: `on_banner_row_open_requested(index)`:
  resolve `banner_row(index)` → `view_state.set_focused(Some(task))`, arm
  `reminder_target`, `route(&window, ActionId::TaskEdit)` — exactly
  `on_today_row_activated`'s shape (`adapter.rs:7088-7093`). The
  notification twin resolves `notification_row(index)` →
  `notification_target`, and routes only for a `Task` anchor (else no-op).
  `ActionId::TaskEdit` already dispatches `UiEffect::Activate`
  (`router.rs:798`) → `view_state.activate_focused` (`view_state.rs:1434`)
  opens `TaskView` — no new route, no engine command.

*Keyboard parity*: the banner already has it (focus → Enter → `TaskEdit`).
Keyboard activation on notification-surface rows does not exist today and is
out of scope here (backlog issue filed).

### D-6.2 — Snooze/Mute: make the effect visible (R6-2)

No behavioural change to what the buttons dispatch — that part is correct
and tested. The fix is **state-truth reflection** (house style: pushed
models, no transient UI state):

- `BannerRowModel` gains `muted: bool` and `snoozed-phrase: string`.
  - `muted` = the row's task is in `state.reminders.muted`.
  - `snoozed-phrase` = a snooze record for the row's `(reminder,
    occurrence)` rescheduling into the future renders "snoozed until HH:MM"
    (local, the reminder phrases' existing formatting helpers). *Verify
    during implementation where journaled snoozes surface in
    `AppState`* — the persistence apply for `RecordReminderSnooze`
    (`domain/src/persistence.rs:264, 499`) is the seam; if the snooze
    record is not retained in `AppState` today, retain it there (the engine
    already prunes deliveries; the same home).
- **status-bar.slint** row rendering: a muted row renders muted (dimmed
  tone / mute glyph in the meta; copy "reminders muted" instead of the due
  phrase), a snoozed row renders the `snoozed-phrase`. The row itself stays
  — board truth is unchanged by design — but the state change from the
  click is now visible on the row the button belongs to.
- **Notification surface**: `can-mute` already gates the Mute button; if
  the model can cheaply carry the *current* muted state, render the button
  as Unmute (toggle) — optional ride-along, not a requirement.
- The engine's typed rejections (`NoTarget` on a stale index, dead task)
  stay absorbed as today; with reflection in place a *successful* mute is
  distinguishable from a dead click, which was the actual complaint.

Tests: presentation-level (banner rows reflect muted/snoozed state from a
mutated `AppState`), plus the existing `banner_actions_send_exact_commands`
stays green unchanged.

### D-6.3 — Fixed stack minimum, title elides (R6-3)

- `widths.rs`: `content_min_px` loses the estimator term and the
  `title_chars` parameter — `pub fn content_min_px(base_scale: f64) -> u32`
  returning `round(STACK_WIDTH_READABILITY_MIN_PX · scale)` (floored at the
  unscaled constant, preserving today's degenerate-scale behaviour). The
  readability floor (120 logical px, derived from the task card's own
  min-content width) **is** the fixed default minimum the review asks for —
  it is already the standalone floor of `reclamp_width`.
- Delete `HEADING_CHAR_WIDTH_PX` and `HEADER_CHROME_PX` (no other usages —
  verified by grep).
- `models.rs:1607-1616`: stop counting title chars; call
  `content_min_px(scale)` per column (all columns share the value; keep it
  inside the per-column loop or hoist — same result).
- Tests: replace `content_min_grows_with_the_title` (it pins the limitation
  being removed — name the replacement `content_min_is_title_independent`)
  and adjust `content_min_short_header_floors_below_the_old_floor`; the
  `content_min_is_floored_monotone_and_scaled` proptest survives with the
  `chars` strategy dropped.
- `board-layout.slint:455-469`'s comment (elide unconditional, "the
  auto-layout guarantee rides only on the width-px binding") stays true;
  reword only if it references the removed estimator.

### D-6.4 — Title width: sever the empty-decorations-row collapse (R6-4) — FINAL (probe-verified remedy)

The width-resolver theory (needs-aware surplus) is **withdrawn** — the user's
screenshot showed cards whose column was wide enough, and the probe proved the
title is width-frozen by the card itself, not by column allocation.

- **Root cause (probe-verified)**: the always-mounted decorations row — an
  empty stretch-aligned layout — reports `LayoutInfo { min: 0, pref: 0,
  max: 0 }` (i-slint-core `box_layout_info`'s empty-layout early return),
  and the text column's constraint merge (`max = min(child maxes)`)
  collapses the column's max-width to it. Probe-measured pre-fix: the
  column's constraints normalize to `min=pref=max=166` at card widths
  250/340/500, the title frozen at 108 px (84 px chipless).
- **The fix (landed form)**: carry the row in a plain `Rectangle` with an
  explicit `max-width`, severing the merge: a plain Rectangle would
  normally inherit the single-layout-child's constraints (the compiler's
  single-cell lowering — the first wrapper attempt was defeated by it),
  so the cap is stated explicitly (`max-width: 99999px`). The row keeps
  its mount conditions and its phantom-spacing geometry — **zero layout
  delta** against the pre-fix tree except the title fix itself.
- **Why not "mount only with content"** (the first remedy, tried and
  reverted): it removes the phantom spacing slot and **unmasks a latent
  #592 compensation drift** — instrumented (anchor echo): the anchor
  applied the cached 65 px correctly while the witness drifted exactly
  21 px (one chip row): the builtin ListView's post-removal rebase
  consumed 86 px for the removed row against the cached 65 px. The drift
  is real but belongs to the builtin's rebase bookkeeping; with the
  zero-delta remedy it never fires. The hazard (any future change that
  alters today-row heights en masse must re-audit the #592 pins) is
  recorded in #725.
- **Regression probe (committed)**: `tests/task_card_title_probe.slint` +
  `tests/task_card_title_probe_tests.rs` mount the real `TaskCard` at
  controlled widths and pin three structural relations (no font-metric
  pins): the title grows with the card; a wide card shows the whole title
  (resolved ≥ intrinsic); the chipless card grows past the elide floor.
  `TaskCard` exposes `title-width` / `title-preferred-width` (read seams
  for the probe and the automation lane).
- **Pattern audit**: the same empty-repeater-in-layout shape exists at ~14
  other sites (status-bar banner, notification-surface rows, habits,
  timers, track-entries, focus-overlay, action-menu, search-palette,
  stat-bars, dropdown-flyout, date-picker, board-bar, board-layout ×2) —
  filed as #725; only the TaskCard site is fixed in this round.
- The stack-minimum change (R6-3, D-6.3) is independent and unaffected:
  `content_min_px` still sheds its title-length estimator.

### D-6.5 — #721 fix: save into a sync-less config (R6-5)

Two-sided fix, both halves required:

1. **`ensure_migrated` (`ui_requests.rs:1236-1259`)**: `Ok(false)` no
   longer unconditionally rejects. New rule: treat `Ok(false)` as "nothing
   legacy to migrate" and proceed with the raw config *unless* the raw
   config carries a `[nextcloud]` identity husk (either `server_url` or
   `username` set — the same predicate `connectors_item` keys on,
   `connections.rs:109-112`), in which case the typed `PersistRejected`
   corner stands (env-injected identity, partial legacy keys: genuinely
   not a write target). The no-file and empty-config cases fall through to
   the writer, which already creates file and directory.
2. **Writer (`connections.rs:192-243`)**: when `sync`/`connectors` is
   absent, **create** the subtree — mirror `migrate_document_inner`'s
   insertion (`migration.rs:121-128`: `or_insert` a `sync` table, fresh
   `ArrayOfTables`, push the entry). The identity-husk refusal and the
   present-but-scalar `connectors` refusal (`NotAnArray`) both stay.
- Tests: `ensure_migrated` table (no file; file without `[sync]`/`[nextcloud]`
  → proceeds; env-injected identity → `PersistRejected`; legacy full
  identity → migrates then proceeds), `overlay_upsert_connector` from an
  absent file and from a sync-less file (subtree created, comments
  preserved), and a handler-level save happy path on #721's exact repro
  config.

### D-6.6 — Files option on every Nextcloud account (R6-6)

Single files-sync lane stays (one actor, one substrate document) but the
**anchor account becomes a choice**, and every Nextcloud card gets the
control:

- **Config**: `FilesSyncSection` gains `anchor: Option<String>` (connector
  name; absent → first Nextcloud connector = today's behaviour, no config
  churn for existing users). Validation: a set anchor must name an existing
  Nextcloud connector; `enabled` requires a resolvable anchor (replaces the
  default-backend check at `config.rs:1556-1558`).
- **Bootstrap** (`bootstrap.rs:118-136`): resolve the anchor by name
  (fallback first Nextcloud); spawn under that connector's identity.
- **Overlay**: `set_files_sync_anchor(path, name)` beside
  `set_files_sync_enabled` (`settings_overlay.rs:309-316`); new
  `UiRequest::SetFilesSyncConnector { connector, reply }`; snapshot gains
  `files_sync_connector` + the applies/env metadata for the new key.
- **`ConnectorInfo` gains the backend** (`requests.rs:37-41`; app fill at
  `ui.rs:612-623`) — required here to gate the Files row to Nextcloud
  cards, and reused by D-6.8's label fix.
- **UI** (`adapter.rs:4600-4632`): emit the Files item for **every
  Nextcloud account card**. Per-card semantics of the one flag:
  `files_on = enabled && card is the anchor`. Toggling On on a non-anchor
  card re-anchors to it and enables; toggling Off on the anchor disables.
  The card caption names the account ("Files sync mirrors this account's
  Files — takes effect at the next restart."), replacing today's implicit
  default-connection copy (`settings_status.rs:261`).
- **Backlog** (filed): true multi-account files sync (N lanes, N device
  folders, engine lane identity multiplexing) — a deliberate wave, not
  part of this round.

### D-6.7 — Connections visible without pressing Refresh (R6-7)

- **Auto-list at mount**: `settings_view_mounted` (`adapter.rs:5890-5906`)
  fires `UiRequest::ListRemoteBoards` for every connector in `vm.rows`
  without a `remotes` entry, marking the row busy (`PendingKind::ListRemote`
  exists; guard against re-firing while that connector's listing is
  already pending — rapid remounts must not double-fire). Network cost:
  one listing call per connector, deferred to the first settings visit —
  not at boot.
- **Failure path exists**: `PendingReply::Remotes` already lands typed
  outcomes on error (`adapter.rs:4271-4279`) — the auto-list rides it
  unchanged; a failed listing keeps the refresh hint.
- **Bindings survive without a listing**: in `connection_items_model`
  (`adapter.rs:4655-4683`), after the listing loop, emit a `Remote` item
  for each live binding on that connector whose `remote_id` is not in the
  listing — `bound: true`, titled by the bound local board's title (the
  remote title is unknown until a listing lands; the listing refresh
  enriches it). Acceptance: after a restart with a bound board and the
  server unreachable, the settings page still shows the connection and its
  mapping.

### D-6.8 — Backend-aware connector rows; the Files-boards UI surface (R6-8) — REWRITTEN after the DR-9 landing

**Phase 1 (this round)** — stop mislabelling:

- With `ConnectorInfo.backend` in place (D-6.6), the connector row label is
  backend-aware: Nextcloud connectors read "Deck · name"
  (`settings-view.slint:917` becomes `item.backend-word + " · " + title`),
  standalone connectors drop the Deck prefix. Add the backend word to
  `ConnectionItem` (Slint-side string; the UI stays dumb).
- The Files card's caption states what Files does today ("mirrors your
  journal, settings, and your moved boards over the account's Files").

**Phase 2 (wave) — the Files-boards surface, reshaped by the landed design**:

The DR-9 engine work is **done** (chunks 1–6 landed through `ca961b3d`:
files-tagged remote identities, board lanes riding the files actor,
persistence, the resumable `boards move --to-files` ceremony, cross-device
adoption). Adoption removes the connect step the round-6 review expected:
a board published to the Files substrate **arrives automatically** on every
device whose Files lane is enabled (`plan_board_adoption`). What is missing
is the UI visibility layer (tracked as #722):

1. **Files-boards visibility**: a Files card section (or a per-board badge)
   showing which boards ride Files — derivable from the engine's bindings
   (`RemoteBoardId::Files`); no listing request needed for bound boards.
2. **A "Move to Files…" UI flow** per local board, wiring the move ceremony
   (`boards move`, `cli.rs:269-289`) — the GUI has no equivalent today; the
   request + pending/busy states mirror D-6.9's publish pattern.
3. Phase 1's backend-aware labels.

The widget spec rides the landed engine: the interaction model mirrors
`settings_create_item` / `settings_remote_bind_picked` for the move flow;
visibility is a pure projection of engine state.

### D-6.9 — Publish a local board to the server (R6-9)

The ceremony exists; this is a UI surface + one request arm:

- **Request**: `UiRequest::PublishBoard { board: BoardId, connector: String,
  reply }` (requests.rs enum, `:300` region).
- **App arm** (`ui_requests.rs`): build the connector's `DeckClient` (the
  `live_remote_boards` pattern, `:868-885`), then reuse the publish
  ceremony — `publish_on` (`commands/boards.rs:667`) already does
  `create_board` → `BindBoard` → orphan sweep → nudge/wait-reconcile, and
  has a session-core variant for the REPL (`repl.rs:499`); extract the
  shared helper so the UI arm and the CLI verb share it. Guards come free:
  `publish_on` refuses already-bound boards (typed error → the settings
  outcome path).
- **UI**: on each Deck connection card, a "Publish a local board…" row next
  to the existing per-remote actions — opens the board picker that already
  exists for binding (`board-options` is re-derived every push pass,
  `adapter.rs:4699-4704`; unbound boards only, which is exactly the publish
  precondition). New `PendingKind::Publish` + busy flag; reply refreshes
  via the standard `ConnectionsSnapshot` path.
- Closes #699's UI half; comment the issue with this design.

## 4. Non-goals

- No keymap/keyboard-activation changes for notification rows or the alarm
  window (backlog filed).
- No multi-lane files sync (backlog filed).
- No change to banner visibility semantics (board truth stays; only
  *reflection* of mute/snooze state is added).
- No change to the stack-width resolver's allocation semantics (the R6-4
  width-resolver proposal was withdrawn with the probe diagnosis; the
  equal-share/proportional resolver stands).
- No `StackWidthsChanged`/persistence format changes.
- No engine work for Files boards (landed); this plan adds only the UI
  surface over it (D-6.8 phase 2, #722).
- The empty-repeater layout-info audit beyond the TaskCard site (#725).

## 5. Proposed chunking (post-acceptance dispatch input)

| Chunk | Contents | Branch seed | Size |
|---|---|---|---|
| Groundwork — card title width | D-6.4 (the verified severing fix + probe) + this plan + the dispatch plan | `fix/ui-r6-card-title-width` — **this PR, already executed** | S |
| 1 — reminder interactions | D-6.1 + D-6.2 | `fix/ui-r6-reminder-interactions` | S–M |
| 2 — settings save + listing | D-6.5 (#721) + D-6.7 | `fix/app-settings-save-and-listing` | M |
| 3 — files anchor + backend labels | D-6.6 + D-6.8 phase 1 | `feat/ui-files-sync-anchor` | M |
| 4 — publish board | D-6.9 (#699) | `feat/ui-r6-publish-board` | S–M |
| 5 — files-boards UI surface | D-6.8 phase 2 (#722), gated on chunk 3 | `feat/ui-files-boards-surface` | M |
| 6 — empty-repeater audit | #725 (the TaskCard site is already fixed by the groundwork PR) | `fix/ui-slint-empty-repeater-audit` | S–M |

Chunks 1–4 and 6 are independent (all off `main`, disjoint files);
chunk 5 builds on chunk 3's `ConnectorInfo.backend` field and labels.
Default dispatch order: 2 (the user-facing #721 save bug) → 1 → 3 → 4 →
5 → 6. The groundwork PR (fix + probe + plans) merges first so the
dispatch plan's origin references resolve.

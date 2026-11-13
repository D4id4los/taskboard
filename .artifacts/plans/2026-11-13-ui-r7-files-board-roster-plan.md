# [Plan] Chunk 3 — The per-account Files board roster (`feat/ui-r7-files-board-roster`)

- **Date**: 2026-11-13
- **Kind**: stage-1 implementation plan for dispatch chunk 3 of the
  round-7 GUI review remediation. TDD-first: every phase opens with the
  test that fails before the production change.
- **Origin (normative)**: the UX journey plan
  `.artifacts/plans/2026-11-13-r7-files-roster-ux-plan.md` — stories
  US-1..US-4, journey steps US-n/S-k, wiring rows W-1..W-6, rulings
  R-1..R-5, assumptions A-6. Where the UX plan and the design origin
  (`.artifacts/plans/2026-11-13-gui-review-round7-plan.md` §2 D-7.3,
  §3 accessibility ledger, §2 row R7-3) differ, the UX plan's recorded
  rulings (R-n) win — D-7.3 authored them. Single-lane boundary: #723.
  Dispatch frame: `.artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md`
  §1 (chunk table row 3) and §4 (stage-1/2/3 prompts).
- **Status**: READY for stage 2.
- **Baseline**: anchors re-verified against `main` @ `dc290df2`
  (post-UX-plan; the round-6 files-board comment still sits at
  `settings-view.slint:1065-1069`, the title-only loop at
  `adapter.rs:5145-5161`, `current_connections` at
  `ui_requests.rs:1413-1432`, `move_board_to_files` at
  `ui_requests.rs:1506-1542`, `FILES_SYNC_CONNECTOR_NAME` at
  `bootstrap.rs:103`, `files_sync_spawn_decision` at
  `bootstrap.rs:120-134`, `assign_files` at
  `board_move_ceremony.rs:506-516`).
- **No engine/domain/state/storage changes**: `UnbindBoard` and
  `BindBoard` exist; everything here is `taskboard-ui-slint` +
  `taskboard-app`.

## 1. Deliverables × UX-ID coverage map

Every chunk deliverable maps to its wiring row; every P1 story
(US-1..US-4) completes end-to-end including escape paths (the
P1-slice rule — dispatch plan §4's Covers line).

| Deliverable | UX ID | Files |
|---|---|---|
| `BoardInfo.bound_channel: Option<BoardChannel>` filled structurally in `current_connections` | W-1, US-1/S2 | `src/requests.rs`, `taskboard-app/src/ui_requests.rs` |
| `FilesBoardBadge` taxonomy + tone map + verbatim labels + per-account sort/action fold | W-2, US-1/S2–S3 | `src/presentation.rs` |
| Roster loop replacing the title-only files-board rows; per-row `SettingsButton`; `ConnectionItem` field additions; row comment recording the round-6 overturn | W-3, US-1/S2, US-2/S1, US-3/S1, US-4/S1 | `ui/views/settings-view.slint`, `src/adapter.rs`, `ui/app-window.slint` |
| `UiRequest::StopBoardFilesSync` arm (guards → `UnbindBoard` + snapshot reply) | W-4, US-2/S1–S3 | `taskboard-app/src/ui_requests.rs`, `src/requests.rs` |
| `MoveBoardToFiles` reused verbatim from the row button; `FilesMovePicker` retires | W-5, US-3/S1–S2 | `ui/views/settings-view.slint`, `ui/app-window.slint`, `src/adapter.rs` |
| `UiRequest::PublishBoardToFiles` arm (guards → the `BindBoard` files ceremony + snapshot reply) | W-6, US-4/S1–S3 | `taskboard-app/src/ui_requests.rs`, `src/requests.rs`, `bootstrap.rs` (constants/decision reuse) |

Story completeness check (Covers line):

- **US-1** (roster read): W-1+W-2+W-3 — display, badges, sort, archived
  flag, action postures; escape = the standard stale-snapshot refresh
  discipline (US-1/S2 escape, recorded, no new machinery).
- **US-2** (Stop Sync): W-3+W-4 — dispatch, typed guard refusals
  (S2), badge flip + action re-arm (S3), #755 leftover recorded via
  R-4, not hidden.
- **US-3** (Switch): W-3+W-5 — verbatim `MoveBoardToFiles` dispatch,
  typed ceremony refusals, badge flip; the retired picker cannot be
  mis-invoked (deletion is the proof).
- **US-4** (Publish): W-3+W-6 — dispatch, three typed guard refusals
  (S2), the files `BindBoard` ceremony (S3), badge flip, lane
  re-derive next cycle, restart-pending-enable success posture.

## 2. Recorded decisions (resolving A-6 and the open questions)

Each decision names its authority; deviations from the UX origin carry
reasons.

### D1 — A-6 names (callback, seam, handler)

Following the card's `*-clicked(index)` grammar (`remove-clicked`,
`refresh-clicked` precedent):

- Slint callback on the root `SettingsView`:
  `callback files-board-action-clicked(int)`; the row button calls it
  with the item index. Forwarded in `app-window.slint` as
  `settings-files-board-action-clicked(int)` (the
  `settings-move-picked` forwarding precedent,
  `app-window.slint:204,494`).
- Adapter handler: `settings_files_board_action(&mut self, window,
  &AppWindow, item: usize)` — resolves `ItemAddr::FilesBoard { board }`
  from the address table, dispatches per the row's action kind.
- `install_seam!` arm: `window.on_settings_files_board_action`
  (adapter.rs `install_seam!` block, alongside the retired
  `on_settings_move_picked`).

### D2 — A-6 `ConnectionItem` field additions

`ConnectionItem` (settings-view.slint) gains, computed Rust-side (the
UI stays dumb):

- `badge-text: string` — the verbatim label, empty on non-roster kinds.
- `badge-tone: SettingsBadgeTone` — new exported slint enum
  `SettingsBadgeTone { success, neutral, info }`, defined next to
  `ConnectionItemKind`; the row maps it onto the existing `Badge`'s
  `Tone` with a pure three-arm slint function (no new `Tone` variants —
  R-1's theme ruling stands). Precedent: `StatusTone`
  (presentation.rs:5456) is the same Rust-side tone enum shape.
- `action-label: string` — empty = no action button renders.
- `action-enabled: bool`.
- `archived: bool` — the row renders the archived flag caption when
  set.
- `files-channel` is **not** carried: the badge text/tone already
  encode it; the adapter needs the channel only Rust-side (D3's fold
  returns it).

### D3 — A-6 `FilesBoardBadge` enum's home + the fold

`pub enum FilesBoardBadge { ThisConnection, LocalOnly, OtherConnection }`
lives in `crates/taskboard-ui-slint/src/presentation.rs`, sibling of
`deck_bound_boards` (W-2's named home; the enum is Rust-side only —
the UI receives text/tone, never the enum). The per-account fold:

```rust
pub struct FilesBoardRow {
    pub board: BoardId,
    pub badge: FilesBoardBadge,
    pub channel: Option<BoardChannel>,
    pub archived: bool,
    pub action: Option<FilesBoardAction>, // Stop | Switch | Publish
}
pub fn files_board_roster(
    state: &AppState,
    files_on: bool,
) -> Vec<FilesBoardRow>
```

- Inputs: the `AppState` and the card's `files_on` (already computed in
  `connection_items_model`, adapter.rs:5119-5120 — enabled ∧ this card
  holds the resolved anchor). The card join itself stays in the adapter
  loop; the fold is per-card pure.
- Channel: `binding.board.as_files().is_some()` → `Files`;
  `as_deck().is_some()` → `Deck`; else `None` (structural, D-BF4 —
  never a connector-name match; a Files binding's connector is the
  synthetic `files-sync`, so a name match is impossible anyway).
- Badge: `channel None` → `LocalOnly`; `channel Files` → `files_on ?
  ThisConnection : OtherConnection`; `channel Deck` →
  `OtherConnection` (D-7.3: a Deck-bound board — even through the same
  account's Deck connector — reads "other Connection"; R-5 stands).
- Sort: ThisConnection → LocalOnly → OtherConnection; within a group
  by `(title, id)` — determinism over the engine map's iteration
  order. Archived boards stay in their channel group, flagged (D5).
- Action label/enabled (D6's table), computed in the same fold.
- Tone/label maps: `files_board_badge_tone(badge) ->
  SettingsBadgeTone` and `files_board_badge_label(badge) ->
  &'static str` in presentation.rs, unit-pinned exhaustively to the
  verbatim labels ("Synced via this Connection" / "No Sync · Local
  Only" / "Synced via other Connection" — UX plan §1's fixed
  wording).

### D4 — `BoardChannel` shape and home

`pub enum BoardChannel { Deck, Files }` lives in
`crates/taskboard-ui-slint/src/requests.rs` beside `BoardInfo` (it is
snapshot data plumbing, W-1; the UI crate is the snapshot's owner and
the domain crate must stay untouched). `BoardInfo` gains
`bound_channel: Option<BoardChannel>`, filled **app-side** in
`current_connections` (ui_requests.rs:1413) from
`binding.board.as_deck()/as_files()`. Every other `BoardInfo`
construction site must set the field explicitly so the compiler finds
them (grep `BoardInfo {` — `ui_requests.rs` `current_connections`,
`presentation.rs` `deck_bound_boards`/`boards_of`, adapter tests); the
field is `None` where the filler has no binding context (the
selector-board lists), and `deck_bound_boards`/`boards_of` fill it
structurally too since they read the same state. **`deck_bound_boards`
is deleted** by the picker retirement (D8) — its only consumer is
`push_settings`'s move-options pass — so its construction site vanishes
in the same chunk; the fold tests move with the roster fold.

### D5 — Archived-board visibility decision

Archived boards **render**, flagged, per US-1 AC-2 ("visibility of
archived boards is the roster's job"). Recorded specifics the UX plan
leaves open:

- The adapter roster loop skips only `deleted` boards (the current
  loop's `board.archived` skip is dropped — it was the visibility gap).
- Archived rows carry their true channel badge, the `archived` flag,
  and `action-enabled: false` — an archived board cannot be bound or
  unbound (the engine guards read `!archived`; `move_board_to_files`'s
  filter, ui_requests.rs:1517-1519, is the precedent). The disabled
  button is the honest posture; no separate "unarchive" affordance
  (out of scope, not requested).
- `current_connections`' snapshot filter (ui_requests.rs:1418) keeps
  its `!archived` filter — the selector lists stay un-archived-only;
  the roster reads the state directly, not the snapshot's board list.
  (Reason: the selector is a pick-target list; the roster is a
  status display. Divergent filters are intentional, not drift.)

### D6 — The action table (label × posture)

One `SettingsButton` per roster row; label and enabled are Rust-side
computed (US-1/S3). `enabled = files_on ∧ !archived` everywhere; the
label is per badge/channel:

| Channel | Badge | Label | Note |
|---|---|---|---|
| `None` (unbound) | LocalOnly | "Sync via this Connection" | US-4 |
| `Deck` | OtherConnection | "Switch to Sync via this Connection" | US-3 |
| `Files`, anchor+on | ThisConnection | "Stop Sync" | US-2 |
| `Files`, non-anchor or lane off | OtherConnection | "Stop Sync" | disabled — the action that would apply, honestly unavailable (R-3's caption posture; the Files row's restart/env captions above explain the lane state) |

Reason recorded: the UX plan fixes the three *enabled* labels
(§1, journeys US-2/S1, US-3/S1, US-4/S1) but leaves the disabled
label choice open; showing the action that *would* apply (rather than
hiding the button) is the honest-affordance reading of R-3 ("disabled
actions with honest captions, not lanes"). The keyboard walk reaches
only enabled stops regardless (settings-view.slint:975's stated rule).

### D7 — The two new requests: payloads, replies, error variants, sentences

Per the UX plan §7 command-contract delta, verbatim:

```rust
StopBoardFilesSync {
    board: BoardId,
    reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
},
PublishBoardToFiles {
    board: BoardId,
    reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
},
```

- **Stop arm** (`handle_settings` dispatch arm + `stop_board_files_sync`
  fn, mirroring `move_board_to_files`'s shape): guards — board present
  and not deleted → else `Gone`; binding Files-typed
  (`binding.board.as_files().is_some()`) → else `BindRefused`
  (US-2 AC-3: the row grammar can't normally offer it; the arm still
  refuses typed). Effect: `workspace.engine.execute(StateCommand::
  UnbindBoard { board })` (the `detach_connection` precedent,
  ui_requests.rs:1181-1184). **No connector-actor detach step** — the
  files lane is global (single-lane, #723), unlike Deck's
  `detach_connector_named` sweep. Reply: `current_connections`.
- **Publish arm** (`publish_board_to_files` fn): guard order —
  ① `files_sync_spawn_decision(&ensure_migrated(workspace)?)` resolvable
  → else `FilesSyncDisabled` (the move arm's first guard,
  ui_requests.rs:1510-1513 — cheap config check first, and it makes
  US-4 AC-5's restart-pending enable succeed *by construction*: the
  decision is config-derived, not actor-derived); ② board present, not
  deleted, not archived → else `Gone`; ③ binding `None` → else
  `BindRefused` (Deck-bound and Files-bound both refuse typed —
  US-4 AC-4). Ceremony: the move ceremony's final assign step on the
  shared engine handle — `workspace.engine.execute(StateCommand::
  BindBoard { board, remote: RemoteBoardId::files(<files remote id
  minted from the board's uuid>), connector:
  crate::bootstrap::FILES_SYNC_CONNECTOR_NAME.to_string() })`
  (assign_files, board_move_ceremony.rs:506-516, is the pattern; the
  publish-Deck arm is the engine-handle run precedent). The exact
  `RemoteBoardId::files` mint (the files remote id newtype from the
  uuid) is copied from the ceremony's world construction so publish
  and move cannot mint differently. Reply: `current_connections`.
  The files actor re-derives its lanes from `BoardOwnershipMap` next
  cycle — no nudge needed (board_cycle.rs:561).
- **Error variants**: **no new `SettingsError` variants** — `Gone`,
  `BindRefused`, `FilesSyncDisabled` cover every typed refusal both
  arms can produce; `settings_status.rs`'s exhaustive `error_text`
  already carries their sentences and gains no wildcard risk. The
  "Deck-bound → pointing at Switch (US-3)" guidance is delivered
  structurally, not textually: a Deck-bound board's roster row
  *offers* the Switch label (D6), so the only path to the publish
  refusal is a stale snapshot; the sentence stays the existing
  BindRefused one. (Reason: status text is copy pinned by the
  no-text-assertions rule; the row grammar is the pointer.)
- Ok replies render the generic `SettingsOutcome::Ok` sentence — the
  house posture (R5: structure, never per-op prose).

### D8 — The picker retirement's exact deletion list

The roster is the picker; one affordance per row replaces the
aggregate dropdown (W-5). Deletions:

1. `settings-view.slint`: the `FilesMovePicker` component (lines
   ~592-625), its mount in the `files` row block (~1058-1063), the
   root `move-picked(int, int)` callback (~714), the
   `SettingsMoveCopy` global (~473-...), the
   `SettingsModel.move-options` property, and the `SettingsMoveCopy`
   import + the `app-window.slint:918` tooltip reference row (the
   flyout-site-3 keyboard re-entry — retiring the picker retires its
   facts seam; `SettingsBoardFacts` stays for the other pickers).
2. `app-window.slint`: the `settings-move-picked(int, int)` callback
   (line 204) and its forwarding (~494).
3. `adapter.rs`: `SettingsVm.move_choices` (~4242), the
   move-options/move-choices pass in `push_settings` (~5306-5320),
   `settings_move_picked` (~5878-5902), and the
   `window.on_settings_move_picked` `install_seam!` arm (~10326).
4. `presentation.rs`: `deck_bound_boards` — dead after 3 (D4); its
   tests die with it (the roster fold's tests supersede; "one
   behavior, one test" — the eligible-set rule now lives in the
   roster's LocalOnly/Deck-channel arms).
5. **Stays**: `PendingKind::Move`, the `MoveBoardToFiles` request and
   its app arm, `BusyDot` on the Files row, `map_move_error` — the
   ceremony is untouched; only the *entry affordance* retires.

### D9 — Pending/busy model for the two new actions

`PendingKind` gains two attribution variants — `StopFilesSync`,
`PublishFiles` (the enum's comment: attribution only). `SettingsVm`
gains `busy_board: Option<BoardId>` (the `busy_remote` precedent) and
`send_connections_op` gains the board parameter (or the two call sites
set it post-send, matching the existing shape). Busy rendering:

- Stop/Publish pending → the **specific roster row** renders `BusyDot`
  (`pending ∈ {StopFilesSync, PublishFiles} ∧ busy_board == board.id`).
- The move ceremony keeps its existing marker on the Files row
  (`PendingKind::Move ∧ busy_target == files_connector`,
  adapter.rs:5127) — untouched.
- Re-entrancy: `pending.is_some()` absorbs a second click (the
  `settings_move_picked` guard, adapter.rs:5890). During a move's
  pending, row actions across the card stay enabled-but-click-absorbed
  (same posture as Remove/Refresh today) — no new disabling matrix.

### D10 — The row comment recording the overturn

The files-board block in settings-view.slint gains a comment replacing
the round-6 ruling comment (lines 1063-1069):

> Roster rows (round-7 D-7.3): per-board badge + one action button.
> Overturns round-6 D-6's "adoption is automatic, disconnect is a
> non-goal" ruling — authority:
> `.artifacts/plans/2026-11-13-gui-review-round7-plan.md` §2 D-7.3
> (that plan authored the overturn; the UX origin
> `2026-11-13-r7-files-roster-ux-plan.md` §1 carries the authority
> note). The #755 subtree leftover is recorded, not hidden.

Chunk 2's census diff owns the census-side record of the overturn
(UX plan §1 authority note) — this chunk owns the code-side comment.

### D11 — Roster loop placement (adapter)

The roster loop replaces adapter.rs:5138-5161 inside the per-card
`if let Some(files_connector)` block, but **emits for every Nextcloud
card** (the current loop already does — it sits in the card walk; the
UX plan's "duplicated under every account card" complaint was the
title-only duplication, not the per-card roster: US-1 AC-1 wants the
roster per card). Order in the walk: Files row, then its roster rows
(the existing position — the roster is the Files section's body), then
Deck rows. Roster rows keep `ItemAddr::FilesBoard { board }`
addressing (D-7.3 row grammar: ItemAddr kind `ConnectionItem`
files-board); the addr's doc comment ("addressable for a future detach
seam; no action this round") is updated to the now-true statement.

## 3. TDD phases (test first, each phase red → green → commit)

### Phase 1 — the snapshot field (W-1, US-1/S2)

Tests first:

- `taskboard-app/src/ui_requests.rs` tests:
  `us1_s2_bound_channel_drives_badge_fold` — no wait: the app-arm
  tests don't embed step tokens (UX plan §8: "the app-arm tests assert
  exact commands without embedding step tokens"); the *fold* tests
  carry the tokens. App test (plain name, existing suite style):
  `current_connections_fills_bound_channel_structurally` — a deck
  binding fills `Some(Deck)`, a files binding fills `Some(Files)`,
  unbound fills `None`; a connector-name-shaped binding is never
  trusted (D-BF4).
- UI-side: `presentation.rs` unit test
  `board_info_constructors_carry_bound_channel` over the surviving
  constructors.

Then: `BoardChannel` + the `BoardInfo` field (D4); update every
construction site; `current_connections` fills structurally.

### Phase 2 — the badge/tone/sort/action fold (W-2, US-1/S2–S3)

Tests first (presentation.rs `mod tests`, synthetic `AppState`
matrix; enums/flags only, never label text — except the three verbatim
labels pinned exhaustively in the label-map test, which is the copy's
home):

- `us1_s2_badge_tone_and_sort_fold`: the synthetic matrix — files
  board (anchor card: ThisConnection/success; non-anchor:
  OtherConnection/info), deck-bound board (OtherConnection/info),
  unbound board (LocalOnly/neutral); sort ThisConnection →
  LocalOnly → OtherConnection, within-group `(title, id)` ordering
  (two boards whose map order differs from title order prove the
  sort).
- `us1_s2_verbatim_badge_labels_pinned`: the label map over all three
  variants equals the UX plan §1's exact strings (the copy's single
  pinning site).
- `us1_s3_action_label_and_enabled_postures`: per D6's table —
  anchor+on: Stop/Switch/Publish enabled per channel; non-anchor:
  same labels, all disabled; lane off: all disabled; archived: flag
  set, action disabled, badge true to channel.
- `us1_s2_archived_board_stays_in_roster_flagged` (US-1 AC-2).

Then: `FilesBoardBadge`, `FilesBoardAction`, `FilesBoardRow`,
`files_board_roster`, the tone/label maps (D3).

### Phase 3 — the roster rows + callback (W-3, US-1/S2, US-2/S1, US-3/S1, US-4/S1)

Tests first:

- adapter tests (`#[cfg(test)]` mod or the existing adapter test
  home): `us1_s2_roster_emits_one_row_per_board_per_card` — two
  Nextcloud cards ⇒ every live board twice; standalone cards ⇒ zero;
  deleted boards never; the round-6 title-only duplication becomes
  the roster (same count, richer rows).
- `journeys.rs`: `us2_s1_roster_row_action_dispatches` — pressing a
  ThisConnection row's button dispatches `StopBoardFilesSync` (the
  journey-test seam assertion; the exact-command pinning for the arm
  itself lives in the app-arm tests, one layer down).
- `journeys.rs`: `us3_s1_switch_action_dispatches_move_board_to_files`
  and `us4_s1_publish_action_dispatches_publish_board_to_files`.
- `journeys.rs`: `us1_s3_disabled_row_action_absorbs_click` — a
  disabled row's click path never dispatches (the adapter guard).
- inventory tests: update `seam_inventory.rs` /
  `view_mount_inventory.rs` / `focus_coverage.rs` expectations for the
  new callback seam and the retired picker seam (these inventories
  enumerate seams — both directions of churn must be recorded there;
  re-verify the exact inventory mechanics at implementation).

Then: `ConnectionItem` fields + `SettingsBadgeTone` (D2), the roster
loop (D11), the row grammar in settings-view.slint (badge + button +
archived caption + the D10 comment), the callback + forwarding +
handler + `install_seam!` (D1), the busy model (D9).

### Phase 4 — the Stop arm (W-4, US-2)

Tests first (ui_requests.rs test mod, the move-arm test shapes):

- `us2_s2_stop_sync_guards_refuse_typed`: gone board → `Gone`;
  deck-bound board → `BindRefused`; unbound board → `BindRefused`
  (the arm refuses typed even though the row grammar can't offer it).
- `stop_board_files_sync_sends_exact_unbind_command` (seam test,
  plain name per §8): the engine receives exactly
  `StateCommand::UnbindBoard { board }`, and the reply is a fresh
  `ConnectionsSnapshot` reflecting the unbound board.
- `us2_s3` journey (journeys.rs):
  `us2_s3_badge_flips_local_only_and_action_rearms` — after the
  unbind lands, the re-rendered row reads LocalOnly + publish action.

Then: the `StopBoardFilesSync` variant, the dispatch arm,
`stop_board_files_sync` (D7).

### Phase 5 — the Publish arm (W-6, US-4)

Tests first:

- `us4_s2_publish_guards_refuse_deck_bound`: deck-bound →
  `BindRefused`; files-bound → `BindRefused`; spawn decision
  unresolvable (files_sync disabled config) → `FilesSyncDisabled`;
  gone → `Gone`.
- `publish_board_to_files_sends_exact_files_bind_command` (seam
  test): the engine receives exactly
  `StateCommand::BindBoard { remote: RemoteBoardId::files(..),
  connector: FILES_SYNC_CONNECTOR_NAME }`; the reply is the fresh
  snapshot.
- `publish_succeeds_while_files_actor_restart_pending` (US-4 AC-5):
  config with files_sync enabled + a pending (not-yet-booted) lane ⇒
  the bind commits (config-derived guard), badge truth flips on bind.
- `us4_s3` journey (journeys.rs):
  `us4_s3_badge_flips_this_connection_after_publish`.
- The #755 honesty record (US-2 AC-4) is documentation: D10's comment
  + R-4; no assertion (no text assertions).

Then: the `PublishBoardToFiles` variant, the dispatch arm,
`publish_board_to_files` (D7).

### Phase 6 — the picker retirement (W-5, US-3)

Tests first: the retirement's test *churn* is the red state —
`deck_bound_boards`' tests die with the fold; any journey/inventory
test referencing the move picker flips to the row-button path
(`us3_s1_switch_action_dispatches_move_board_to_files` already covers
the surviving behavior; the retired picker cannot be mis-invoked —
deletion is the proof, no test asserts absence).

Then: the D8 deletion list, in order (slint component/copy/callback →
app-window forwarding → adapter vm/handler/seam → the dead fold).
`us3_s2` journey (`badge flips ThisConnection`) is already covered by
the existing move-arm journey tests if any assert the reply refresh —
re-verify; if not, add `us3_s2_badge_flips_this_connection` reading
the post-move re-render.

### Phase 7 — gates + handoff prep

`scripts/run-quality-gates.sh` full suite (fmt, clippy `-D warnings`,
nextest, doc tests, deny, llvm-cov sequence). Stage 3 owns the
walkthrough protocol, `scripts/ux-coverage.sh <ux-plan> --gate`, the
PR, and the worktree teardown.

## 4. Tests that must not break (protected set)

- The move-arm app tests (ui_requests.rs — `move_board_to_files`
  guards, `map_move_error` folds): untouched; only the entry
  affordance retires.
- The settings adapter tests (`connection_items_model` matrix,
  `push_settings` rows): the files-board loop's expectations change
  deliberately (title-only → roster) — those specific assertions are
  *re-authored* to the richer row shape, not weakened; every other
  expectation (account/files/deck/remote rows, busy machine, picker
  options for the remote-bind picker) stands.
- The presentation folds' tests (`account_cards`, `account_status`,
  `boards_of`): stand; `deck_bound_boards`' tests are the sanctioned
  deletion (D8.4).
- Keymap totality, walk, focus-coverage suites: no new `ActionId`
  (the Settings-walk parity argument, UX plan §7's closing note), so
  presets.rs is untouched.

## 5. Risks

- **Seam inventories**: `seam_inventory.rs` /
  `view_mount_inventory.rs` enumerate installed seams and mounts —
  both the new `on_settings_files_board_action` and the removed
  `on_settings_move_picked` must be reflected; a stale inventory is a
  failing gate, not a silent drift (good, but budget for it).
- **`SettingsBoardFacts` flyout machinery**: the picker's window-level
  re-entry path (app-window.slint:914) retires with site 3; verify no
  other picker shares that exact `SettingsBoardFacts` arm (the
  publish picker uses site 2) — partial removal only.
- **`BoardInfo` construction-site churn**: adding a field breaks every
  literal construction; the compiler enumerates them (D4) — no
  Default derive (a silent `None` would hide a missed filler).
- **Files-binding connector identity**: Files bindings carry the
  synthetic `files-sync` connector name, so any accidental
  name-match against card connectors silently yields zero
  ThisConnection badges — the fold's structural channel detection
  (D3) is the guard, and the fold tests include a files binding whose
  connector string is a card connector name to pin D-BF4.
- **Anchor drift while planning**: anchors were re-verified at
  `dc290df2`; re-verify `file:line` at implementation time (the
  findings, not the line numbers, are stable).

## 6. Estimated diff size

~600–800 lines changed across 8 files:
`requests.rs` (+30), `presentation.rs` (+180/−90: new fold + the
retired one), `adapter.rs` (+150/−120: roster loop, handler, busy
model, minus the picker pass), `settings-view.slint` (+90/−80),
`app-window.slint` (+6/−12), `ui_requests.rs` (+140: two arms + tests),
`settings_status.rs` (0 — no new variants), `bootstrap.rs` (0 — reuse
only). Plus inventory/test churn. Stage-2 effort: the two arms are
small; the fold matrix and the adapter/inventory churn carry the bulk.

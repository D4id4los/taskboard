# [Plan] ui-r8 chunk 6 — files sync status (stage-1 implementation plan)

- **Date**: 2026-11-17
- **Kind**: TDD-first chunk implementation plan (dispatch stage 1 —
  planning; no code in this commit).
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-16-files-sync-status-ux-plan.md` (§3
  US-1..US-10, §4 steps, §6 W-1..W-9, §7 deltas, rulings R-1..R-3,
  §8 verification — all normative), chunked by
  `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
  §1 (chunk 6) / §2 (shared execution model, coverage-gate steady
  state). Branch: `fix/ui-r8-files-sync-status`. Round-8 standing
  laws apply verbatim (typed outcomes; `tokio::time::pause/advance`;
  census rides the code commits; `// slint:` comments on every
  touched seam; the C-4 doctrine bar; the focus contract).
- **Worktree**: `../taskboard-WT-fix-ui-r8-files-sync-status`
  (doctor-passed 2026-11-17, 115G free; branched off `origin/main`
  at e7a720dc).

## 1. Verified current-state delta (what is actually left to build)

Reading the seams (all paths under
`crates/taskboard-ui-slint/`, `crates/taskboard-app/src/`,
`crates/taskboard-sync-files-nextcloud/src/`,
`crates/taskboard-domain/src/`) moved several of the origin plan's
premises; the delta below is the honest remainder:

| Origin claim | Seam reality | Consequence |
|---|---|---|
| The chip's warning costume lives in `status-bar.slint` ~356–373 | Confirmed — the block hard-codes `Semantic.warning`/`Tone.warning` for every state except `forked` (danger); the state enum + caption fold live in `presentation.rs:79-105` (`FilesSyncChipState`, `files_sync_chip_caption`), the status→chip fold in `crates/taskboard-app/src/files_sync_surface.rs:23` (`files_sync_chip`, precedence Forked > Diverged > Parked > CycleFailed > Ok), the slint-side mapping in `models.rs:3477` (1:1) | The tone fold needs a home that both the slint and the presentation tests can reach: a pure `files_sync_chip_presentation(state) -> ChipPresentation { tone, flagged }` in `presentation.rs` (§3 D-1); the slint block consumes the model — no business logic in `.slint` |
| N-19: "pending only rises, only `identity.json` lands" | The per-board pending count is the outbox derivation `derive_sync_states` (`domain/state.rs:408-437`, `pending_ops` = outbox ops attributed via `attributed_board`); the files lane drains the same outbox through `drain_board` (`board_cycle.rs:762-955`): `plan_pushes` → entity entries → one chunk `put_create` → `publish_drain_report` (Applied). The operator's symptom (identity lands, chunks never) means the drain stage exits early — three silent exits exist: the non-entity-target warn arm, the `entries.is_empty()` debug arm, and `folded_once` never set (the pull stage failing every tick before drain). Reproduction protocol §3 D-2 | The investigation is test-first (§3 D-2); the fix is expected inside `drain_board`/`plan_pushes` — no contract change (origin §7's expected shape), an amended delta only if the investigation surprises |
| The stalled truth (US-2 AC-3) needs a new state | `FilesSyncStatus` (`sync-files-nextcloud/status.rs:82`) already carries `last_cycle: Option<CycleOutcome>` (Failed variant) — and the pending count is derivable app-side from the engine snapshot's `sync` rows (`pending_ops`). The stalled fact is therefore a **fold**, not a new watch type: `stalled = pending_ops > 0 ∧ last_cycle == Failed` (§3 D-3) | No engine/actor delta for US-2 AC-3; the fold lives with the per-board row model (§3 D-3) |
| The mount auto-list needs a backoff state | The mount auto-list fires one `ListRemoteBoards` per connector on settings mount (`adapter.rs:5660-5700`, round-6 D-6.7); the pending lane is `PendingKind::ListRemote` (`adapter.rs:5128`); failures land through the typed outcome lane | Typed backoff state on the settings VM (§3 D-4); the manual refresh path (`adapter.rs:7072`) overrides |
| Per-key env-lock captions | The snapshot already carries per-key facts: `env_overridden` (a set containing `"files_sync.enabled"` / `"files_sync.anchor"`) and `applies` (restart-required map) — the Files row currently ORs the two keys into one `files_locked` bool (`adapter.rs:6149-6151`) | US-4 is a model split into two bits + caption render; no snapshot delta (§3 D-5) |
| The publish ceremony's full-page disable | `PendingKind::Publish`/`PublishFiles` set a card-level `busy` today (`adapter.rs:6114-6208`) plus page-level disables for the attach-family kinds; the publish phase has no per-row model | The phase rides the addressed connector's row model (§3 D-6); the page-usable guard is a seam test asserting the wholesale-disable flag is absent on the publish path |
| The undo window (US-6) | `MoveStage::DeckRetired` + `board_move_ceremony::undo` (`board_move_ceremony.rs:269-290`) exist; the CLI arm is `BoardsCmd::Move` → `run_on` (`commands/mod.rs:1064-1100`, `--to-deck` fenced `ReverseFenced`); the UI's move arm is `move_board_to_files` (`ui_requests.rs:1528-1565`, pre-IO guards then `run_on`) | New `UiRequest::UndoBoardMove` + app arm on the running engine (§3 D-7); the arm's stage gate is `derive_move_stage`/`current_stage`, typed refusals mapped through the existing `map_move_error` |
| US-7 (#749) re-verification | The r7 roster's Stop Sync landed: `UiRequest::StopBoardFilesSync` + `stop_board_files_sync` arm (`ui_requests.rs:1570-1605`, `Gone`/`BindRefused` guards + `UnbindBoard`) — the r7 files-roster plan's row grammar (2026-11-13-r7-files-roster-ux-plan.md §3 US-2, §6) is the extending surface | Verification work: re-derive the premise, close with evidence or fix residue (§3 D-9) — no planned code |
| US-8 adoption phase | `FilesSyncStatus.moved_boards: BTreeMap<uuid, BoardMoveFinding>` (Adopting/Adopted/Refused) exists; the UI consumes the chip watch only (`files_sync_surface.rs:56` pump, chip-only payload) | The pump widens to a per-board states map (§3 D-8); the Files card rows fold the phase |
| US-9 picker site | The publish picker mounts on flyout **site 2** today (`settings-view.slint:556-559`, "0 the remote rows' pickers, 1 the board bar") | The origin plan's site grammar says site 1 beside the remote rows at the Deck card — a one-line re-anchor + the site table record (§3 D-10) |
| US-10 rebind ceremony | `BindBoard` exists (`ui_requests.rs:1123`); bindings store `remote.board.as_deck()` = the Deck board uuid (`RemoteBoardId`); the r7 R-5 ruling fenced Deck-to-Deck moves to #756. No `RebindBoard` command, no `MoveBoardToDeck` request | The round's one domain delta, spelled in §3 D-11 |
| sync-nextcloud floor/poll interaction | The Retry-After honor (#220/#221/#222: `advertised_floor`, `retry_after_floor` as a backoff floor, `actor.rs:183-438`) and the advertised_floor dedup landed on main | The N-19 fixture must not trip the Deck actor's floor logic (the files crate has its own transport/actor — disjoint); the chip's CycleFailed fold reads `FilesSyncStatus.last_cycle`, which is the *files* actor's outcome — no coupling. Recorded so stage 2 does not "fix" across crates |

Gate inventory at plan time (`scripts/ux-coverage.sh
2026-11-16-files-sync-status-ux-plan.md --gate`): **25 FOUND /
19 MISSING** — all MISSING are the un-built W-2/W-3(action)/W-4/W-6/
W-7/W-9 command tokens and the US-7/US-8/US-9/US-10 step tokens.
Stage 2's steady state: every claimed row FOUND after the commits
below; the `—`-command rows carry their reasons in the journey-steps
column and resolve as FOUND.

## 2. Round-8 standing laws (restated, binding)

Typed outcomes only (AGENTS §5 — no label/log/UI-text assertions);
`tokio::time::pause()/advance()` for any time-dependent test (the
N-19 fixture's polling, the backoff window); census cells
(`docs/ux-surface-census.org`) updated in the same commits as the
code, citing the owning plan's US/W ids; every touched slint seam
carries `// slint: <callback>` (#782 — the status-bar chip block and
the settings rows are pre-framework seams: stage 2 adds the comments
where the gate rows resolve); the C-4 doctrine bar is the regression
bar (grey-within-applicable beside withheld-when-inapplicable — the
Restore affordance withholds outside the window, never greys); the
focus contract (focus-restoration US-1): the publish picker is an
overlay — its re-anchor composes with chunk 2's landed restore seams
(escape/outside-click closes, focus returns to the invoking pick).
Long-command discipline: the coverage trio never runs inside a
subagent — the gate is the pre-commit hooks plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe, ~4–6 min; background + poll with useful work between
polls beyond ~5 minutes, and all landed work committed BEFORE any
wait).

## 3. Recorded decisions

### D-1 — The chip fold table (US-1, W-1)

One pure fold in `crates/taskboard-ui-slint/src/presentation.rs`:

```rust
pub struct ChipPresentation { pub tone: ChipTone, pub flagged: bool }
pub enum ChipTone { Neutral, Warning, Danger }   // maps 1:1 in models.rs
pub fn files_sync_chip_presentation(state: FilesSyncChipState) -> ChipPresentation
```

| State | Tone | Badge ("!") | Caption (unchanged, AC-3) |
|---|---|---|---|
| `Ok` | Neutral (`info` colour, no warning semantics) | **none** | "files-sync running" |
| `CycleFailed` | Warning | yes | "files-sync cycle failing" |
| `Parked` | Warning | yes | "files-sync: peer data held aside" |
| `Diverged` | Warning | yes | "files-sync: devices diverged" |
| `Forked` | Danger | yes | "files-sync forked — writes stopped" |

`status-bar.slint`'s chip block consumes `files-sync-tone` +
`files-sync-flagged` from the model (no per-state conditionals in
slint beyond the tone mapping); no new `Tone` variants (R-2 of the
origin plan's non-goals). The precedence ladder in
`files_sync_surface.rs::files_sync_chip` is unchanged. Tests: the
five-state fold, typed (`presentation.rs::us1_ac1_*`,
`us1_ac2_*`), plus the existing caption tests standing (AC-3).

### D-2 — The N-19 investigation protocol (US-2, W-2; test-first)

**Fixture (hermetic remote)**: reuse/extend
`crates/taskboard-sync-files-nextcloud/src/test_support.rs`'s
in-memory `FilesTransport` stand-in (no network, no sleeps): a
`SyncContext` with the paused clock (`tokio::time::pause()`), a
`state_reader` serving a `PersistedState` with one files-bound board
and an outbox seeded with one op of **each** shape the planner can
see — task update, stack add, label op, board rename, and a
sections-touching op. The harness records every transport call
(path + verb) so the test can assert structural facts (an `ops/`
chunk file exists beside `identity.json`).

**Reproduction test** (`board_cycle.rs::n19_ac1_pending_falls_after_a_cycle`,
written FIRST, committed before the fix): run
`run_board_lanes` until a cycle completes without transport error,
then assert **(a)** the outbox drain report marked the ops Applied
**and** (b) `derive_sync_states`-equivalent attribution of the
remaining outbox is zero **and** (c) the transport listing shows a
`boards/<uuid>/devices/<dev>/ops/NNNN.json` chunk. Today (b)/(c)
fail — that is the reproduction. The assertion of record: *pending
falls on a successful cycle* (typed: zero attributed ops), never a
log/text match.

**Candidate causes to instrument (ranked)**:

1. **H1 — `plan_pushes` mis-targets ops to non-entity targets** on a
   files board (`PushTarget::Document`/`RemoteCard`/`RemoteStack` →
   the warn arm "plan produced a non-entity target on a files board;
   op stays queued", `board_cycle.rs:796-804`): every group skipped,
   `entries` empty, drain publishes nothing — matches the symptom
   exactly (identity.json lands from attach; chunks never). Most
   likely: an attribution/planner drift where
   `PendingOp::owning_board` (drain's filter) and `attributed_board`
   (the pending counter) agree but the group target kind does not.
   Instrument: the fixture's harness asserts every group's target
   kind before the loop.
2. **H2 — `entries.is_empty()` via missing entity rows**: the dirty
   entity is absent from the `persisted` snapshot (a stale
   `read_state` view, or the entity lives outside the tables
   `drain_board` materializes from) → the debug arm "drain found
   nothing publishable; ops stay queued" (`board_cycle.rs:838-841`).
   Instrument: assert each dirty id resolves in the tables.
3. **H3 — `folded_once` never set** (`run_lane` returns after the
   pull stage every tick, `board_cycle.rs:507-523`): a pull-stage
   transport error or a decode path that errors before
   `lane.folded_once = true` keeps drain unreachable. Instrument:
   the fixture fails the first pull once and asserts recovery; check
   `report_pull`'s error paths.
4. **H4 — repeated 412 reconcile failures**: `put_create` 412
   reconciles to a foreign chunk → "ops stay queued" forever
   (`board_cycle.rs:905-925`). Instrument: pre-seed a foreign chunk
   at the next seq in the fixture.
5. **H5 — engine-side accounting gap**: chunks land but the Applied
   pushes in `publish_drain_report` (`board_cycle.rs:1021-1055`) do
   not ack the outbox (the `SyncReport::Completed` router dropping
   files-connector reports) — pending rises even though files land.
   Does not match "only identity.json" alone but composes with H1–H3.
   Instrument: an app-level test routing the report through the
   engine and asserting the outbox shrinks.

The investigation record (typed findings: which stage drops the
writes) lands as a `.artifacts/reports/2026-11-17-n19-files-sync-pending-investigation-report.md`
committed BEFORE the fix commit (§5 sequencing); the fix is expected
as a derivation repair inside `drain_board`/`plan_pushes` — if it
turns out to be a contract repair, the amended delta is recorded in
this plan's §5 follow-up note before the fix lands (origin §7's
gate).

### D-3 — The stalled truth's fold path (US-2 AC-3, W-2)

No new watch type. The per-board row model (the Files card rows +
the settings status fold, `settings_status.rs`) gains:

```rust
pub enum FilesBoardLane { Idle, Syncing, Stalled, Failed }
```

folded purely from the engine snapshot's per-board `SyncStatus`
(`pending_ops > 0`, `phase`) plus the files pump's
`CycleOutcome` (`Failed`): `pending_ops > 0 ∧ last_cycle == Failed →
Stalled`; `phase == Syncing ∧ pending_ops == 0 ∧ failed → Stalled`
(a state may never claim Syncing against a rising count — the
origin plan §5's law). The chip itself stays on the five-state
ladder (Stalled renders through `CycleFailed`'s warning rung — the
chip is lane-level, the row is board-level). Test:
`presentation.rs::us2_ac3_stalled_renders_the_stalled_truth`
(typed: the enum variant, never text).

### D-4 — The auto-list backoff (US-3, W-3)

Typed state on the settings VM (`adapter.rs`):

```rust
struct AutoListBackoff { consecutive_failures: u32, last_failure: Instant }
```

- **Window**: `min(30s * 2^(n-1), 10min)` per connector key
  (n = consecutive failures) — deterministic, pause/advance-tested.
- **Suppression**: the mount auto-list (`adapter.rs:5660`) checks
  the backoff before building `UiRequest::ListRemoteBoards`; inside
  the window no request fires and the connector card renders the
  failed truth with the retry posture (AC-1, request-count seam
  test).
- **Manual override**: the refresh path (`adapter.rs:7072`) clears
  the entry before sending (explicit intent, AC-2); failure re-arms
  (n+1), success clears (AC-3).
- The backoff is in-memory UI-layer state (a display posture, not a
  sync decision — no engine delta).

### D-5 — Per-key env-lock captions (US-4, W-4)

The Files row model (`adapter.rs:6140-6155`) splits the ORed
`files_locked` into `enabled_pinned` /
`anchor_pinned` (from `snapshot.env_overridden`), keeping the
existing typed-refusal posture (`applies` restart bits untouched).
The slint row renders one lock caption per pinned key — the pinned
*set* named, never free text asserted in tests (the presentation
test asserts the bit combination typed). Source of record:
`snapshot.env_overridden` (already per-key, zero snapshot delta).

### D-6 — The publish phase's model path + the page-usable guard (US-5, W-5)

- **Model path**: the pending lane already carries
  `PendingKind::Publish | PublishFiles` + `busy_target`
  (`adapter.rs:6148`, `7169-7170`); the addressed connector's row
  model gains `pending_phase: Option<PublishPhase>` with
  `PublishPhase { Publishing, Adopting, Done, Failed }` — `Adopting`
  is the 60-second budget window (`DEFAULT_MOVE_TIMEOUT`'s publish
  sibling; the adoption nudge phase in the publish choreography).
  The phase fold lives in the adapter's settings fold; the slint row
  renders the phase badge (display-only fold).
- **Page-usable guard**: the full-page disable for the publish path
  is removed — the card-local busy state replaces it; the attach/
  remove family keeps today's posture (out of scope). The seam test
  asserts the wholesale-disable flag is absent when only a publish
  is pending (`us5_ac1_*`), and the phase fold test covers
  Publishing→Adopting→terminal (AC-2's typed-error fold reuses the
  existing outcome tests extended).

### D-7 — The undo request's shape + the arm's stage gate (US-6, W-6)

- **Request**: `UiRequest::UndoBoardMove { board: BoardId, reply:
  oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>> }`
  (the move-family shape — reply is the fresh snapshot so the
  standard drain re-pushes settings).
- **Arm** (`ui_requests.rs`, beside `move_board_to_files`): pre-IO
  guards mirror the move arm's posture (board present & not deleted
  → `Gone`; else derive the stage); the stage gate: only
  `MoveStage::DeckRetired` proceeds — `CommitPublished`/`Committed`
  map to the typed `UndoClosed` class, `NotStarted`/`Published` to
  `NothingToUndo`, a lost restore coordinate to `UndoCoordinatesLost`
  (all through the existing `map_move_error`). Inside the window the
  arm runs the ceremony's own `undo` leg on the running engine (the
  CLI's exact path — the `MoveWorld` on `&EngineHandle`, never a
  reimplemented undo).
- **Surface**: the Files row offers Restore **only** inside the
  window (withheld, not greyed — AC-2; the C-4 bar). The row model
  carries `restore_offerable: bool` folded from the derived stage
  (the derive runs where the row model is built; the engine's
  `shared_state` is the truth source, the same read the move arm
  uses).
- **Focus contract**: Restore is a row action, not an overlay — no
  new overlay site; composed by not touching chunk 2's seams.

### D-8 — The adoption phase's fold (US-8, W-7)

The `files_sync_surface` pump (`files_sync_surface.rs:56`,
chip-only today) widens its payload to a per-board states map:
`files_board_states: BTreeMap<BoardId, FilesBoardState>` where
`FilesBoardState { lane: FilesBoardLane (D-3), adoption:
Option<AdoptionPhase> }` and `AdoptionPhase { Adopting, Adopted,
Refused }` folds 1:1 from `BoardMoveFinding` (`status.rs:68-80` —
Adopting/Adopted/Refused(refusal kind)). The Files card rows fold
the phase into the row model (badge fold, typed test
`us8_ac1_*`). Boards absent from `moved_boards` render the
bound/settled truth (no phase).

### D-9 — US-7 (#749) re-verification protocol

Stage 2 re-derives the premise against the landed r7 roster: every
files-bound row (`binding.board.as_files().is_some()`) renders a
Stop Sync row action (the `settings_stop_files_sync` path,
`adapter.rs:7039-7047`), with the arm's typed guards (`Gone`,
`BindRefused` for non-files rows) documented in the evidence
comment. Outcomes: (a) the premise holds → the evidence comment on
#749, issue closes at stage 3 bookkeeping; (b) a genuine residue (a
files-bound row outside the roster's reach) → the residue gets its
own fix + test under this grammar, committed under US-7. No
pre-planned code; the journey test `us7_ac1_*` records the exercised
path either way.

### D-10 — The picker anchor site (US-9, W-8)

The publish picker re-anchors from flyout **site 2** to **site 1**
(beside the Deck card's remote rows — the site grammar's letter:
site 1 beside the remote rows' site 0; the origin plan §3 US-9's
recorded decision). One-line change in `settings-view.slint:556-559`
+ the site table comment updated; dismissal (escape/outside click)
and focus restore ride chunk 2's landed flyout contract untouched.
Test: `presentation.rs::us9_ac1_the_picker_anchors_at_the_card`
(the anchor site fact) + the site grammar tests standing.

### D-11 — The rebind ceremony's full contract (US-10, W-9 — the round's one domain delta)

**Request**:
`UiRequest::MoveBoardToDeck { board: BoardId, target_connector:
String, reply: oneshot::Sender<Result<ConnectionsSnapshot,
SettingsError>> }` (the move-family snapshot-reply shape; the target
is the connector's name/id as the settings VM addresses connectors
today).

**App arm** (`ui_requests.rs`) — precondition list, in order, each a
typed refusal before any IO:

1. Config migrated + the target connector configured
   (`SettingsError::Gone`-class for an unknown target).
2. Board exists, not deleted/archived (`Gone`).
3. The board's binding is Deck-typed (`BindRefused` otherwise — a
   files-bound or unbound board has no Deck-to-Deck move).
4. The current connector ≠ the target connector (a same-connector
   pick is a typed refusal, not a no-op).
5. The target connector is Deck-typed and its actor lane is live
   (the spawn decision the settings VM already computes for Deck
   cards; a dead lane refuses typed — AC-2's refusal set).

On pass, the arm executes the engine verb and replies with the fresh
snapshot (the roster re-derives — AC-3's badges).

**Engine verb**:
`StateCommand::RebindBoard { board: BoardId, connector: String,
remote: RemoteBoardId }` — atomic re-target: the binding's
`connector` is replaced and `remote` is preserved verbatim
(`remote.board.as_deck()` = the unchanged Deck uuid — the ceremony's
identity-preservation pin, AC-1's test asserts the remote id
unchanged). The engine validates the binding exists and is
Deck-typed (typed refusal otherwise) and journals the change.

**Journal class**: the BindBoard/UnbindBoard family — a single
board-binding journal op (`journal` entity = board, binding-retarget
kind), so the standard undo chain sees one reversible step and no
unowned window exists (the ceremony is atomic; the self-reversing
letter is the arm's refusal posture plus the preserved remote id —
a mis-target re-targets back through the same verb).

**Payload/marker note**: no files-tier marker is written — the
board's Deck subtree is untouched; the Deck connector's next pull
re-reads the same remote board under the new connector. No
`MovedFrom` ceremony, no 60-second budget (that is Deck→Files only).

**CLI twin** (`cli.rs` `BoardsCmd::Move`): extend with
`--to-connection <name>` (Deck→Deck only; `--to-files`/`--undo`
unchanged; `--to-deck` stays fenced `ReverseFenced`). The twin calls
the same precondition list + `RebindBoard` on a bootstrapped engine
(the `run_core` posture). Tests: `us10_ac1_*` (journey + app-arm,
exact commands, remote id preserved), `us10_ac2_*` (typed
refusals), `us10_ac3_*` (badge re-derivation fold).

**Roster row**: "Move to this Connection" mounts on a Deck-bound
board rendered under another Deck account's card (the "Synced via
other Connection" badge's honest state becomes actionable — R-3:
the roster row, never a connector-level affordance); the r7
files-action grammar's twin (no new `ActionId` — the pointer-grade
row grammar).

## 4. Test catalogue (§8 of the origin plan, resolved)

| Test | Layer | Settles |
|---|---|---|
| `us1_ac1_ok_state_renders_neutral_without_the_badge` | presentation fold | D-1 |
| `us1_ac2_warning_states_hold_the_costume` (+ forked danger) | presentation fold | D-1 |
| `n19_ac1_pending_falls_after_a_cycle` | files crate engine test (fixture D-2) | D-2, written first |
| `us2_ac3_stalled_renders_the_stalled_truth` | presentation fold | D-3 |
| `us3_ac1_backoff_suppresses_the_auto_list` | adapter seam (request count, typed) | D-4 |
| `us3_ac2_refresh_overrides_the_backoff` | journey | D-4 |
| `us3_ac3_success_clears_the_backoff` | adapter seam | D-4 |
| `us4_ac1_the_caption_names_the_pinned_key_set` | presentation fold (bit combination) | D-5 |
| `us5_ac1_the_card_renders_the_ceremony_phase` | presentation fold + seam (no wholesale disable) | D-6 |
| `us5_ac2_terminal_phase_clears_busy` | existing outcome tests extended | D-6 |
| `us6_ac1_restore_rebinds_deck_inside_the_window` | journey + app-arm | D-7 |
| `us6_ac2_outside_the_window_no_restore_mounts` | presentation fold (withheld) | D-7 |
| `us7_ac1_stop_sync_exercises_the_landed_row` | journey evidence | D-9 |
| `us8_ac1_the_row_carries_the_adoption_phase` | presentation fold | D-8 |
| `us9_ac1_the_picker_anchors_at_the_card` | presentation fact + site grammar tests | D-10 |
| `us10_ac1_rebind_preserves_the_remote_identity` | journey + app-arm | D-11 |
| `us10_ac2_precondition_refusal_renders_typed` | app-arm | D-11 |
| `us10_ac3_badges_re_derive_after_the_move` | presentation fold | D-11 |

Clock law: the N-19 fixture and the backoff window tests use
`tokio::time::pause()/advance()`; no sleeps anywhere (AGENTS §5).
The stalled-truth fold is clock-free (a pure fold over typed facts).
No text assertions anywhere — typed outcomes and structural facts
only.

## 5. Commit sequencing

**Follow-up note (the D-2 amended-delta gate, recorded 2026-11-17
before the fix landed):** the investigation
(`.artifacts/reports/2026-11-17-n19-files-sync-pending-investigation-report.md`)
refuted H1/H2/H4/H5 and confirmed a *contract* arm rather than the
expected derivation repair: the files drain's refusal to publish
ref-delete groups strands attributed ops forever on a files-bound
board (no Deck actor serves the binding again). The amendment: the
files drain materializes ref-delete groups as tombstone entities
(local-row resolution first, then a files-tagged tombstone minted
from the ref's UUID; a Deck-tagged unresolvable ref stays queued with
the typed warn). The #150-era pin's letter narrows: the pin test is
re-authored to the new contract in the fix commit.

1. `test(files-sync): the N-19 reproduction test` — the fixture +
   the failing reproduction (committed as the investigation's
   opening evidence; the pre-commit hooks + nextest gate expect the
   failing test excluded from the gate run via `--skip` until the
   fix lands, or committed together with the fix if the window is
   short — the report records which).
2. `docs(report): the N-19 files-sync pending investigation` —
   `.artifacts/reports/2026-11-17-n19-files-sync-pending-investigation-report.md`
   (typed findings per D-2's ranked hypotheses, the instrumented
   evidence, the chosen cause).
3. `fix(files-sync): the drain repair` — the fix + the reproduction
   test green (census: `files.sync_pending_display` evidence).
4. `feat(ui): the chip's honest tones` — D-1 (census:
   `files.sync_status_display` minted; `// slint:` on the chip
   block).
5. `feat(ui): the auto-list backoff` — D-4 (census: remote_list
   backoff evidence).
6. `feat(ui): per-key env-lock captions` — D-5.
7. `feat(ui): the card-local publish phase` — D-6 (census: #745).
8. `feat(ui): the move-undo window Restore` — D-7 (census: #748).
9. `test(settings): the #749 Stop Sync re-verification` — D-9
   (+ the evidence comment drafted for stage 3).
10. `feat(ui): the adoption phase on the Files card` — D-8 (census:
    #750).
11. `feat(ui): the publish picker's card anchor` — D-10 (census:
    #746).
12. `feat(app): the Deck-to-Deck rebind ceremony` — D-11 (request +
    arm + `RebindBoard` + CLI twin; census: #756 rows minted).

Each commit stands alone (rebase-merge hygiene, AGENTS §8): build +
meaningful subject + no WIP noise; census cells ride their owning
commits. Gate steady state after commit 12: the origin plan's gate
reports every claimed row FOUND.

## 6. Risks

- **The N-19 cause may be outside the ranked set** (e.g. an engine
  ack path or a storage-layer attribution drift): the protocol's
  instrumentation covers the whole drain path; if the cause is a
  contract repair, the amended delta gate (origin §7) applies before
  the fix lands.
- **H1's planner drift may be shared with the Deck lane** — if the
  fix touches shared planner code (`sync-nextcloud`/domain), the
  Deck-side regression suite runs in the same gate; the fix stays
  inside the files crate unless the evidence forces otherwise.
- **The rebind ceremony's connector identity**: `connector` is
  addressed by name in the settings VM today; the engine verb takes
  the same string — a rename-between-derive-and-execute race
  refuses typed (`Gone`-class) rather than mis-targeting (the arm
  re-reads the config inside the guard block).
- **The publish-phase fold touches the attach-family disable logic**:
  scoped to the publish kinds only; the attach/remove posture is
  regression-pinned by the existing tests.
- **Chunk 2 composition**: the picker re-anchor and any overlay the
  rows build ride chunk 2's landed restore seams — no exit-path
  edits; a conflict there is resolved by rebasing onto landed
  chunk 2, not by forking the grammar.

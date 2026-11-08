# [Plan] The Deck→files move ceremony & mixed-fleet exit (chunk 6, D-BF11–D-BF12)

- **Branch**: `feat/deck-to-files-move` (worktree `../taskboard-WT-feat-deck-to-files-move`)
- **Base**: `origin/main` @ `05dbced3` (chunks 1–5 landed; `feat/boards-over-files` merged)
- **Issue**: #689 (closes #689 and the wave issue #678); milestone `nextcloud-files-sync`
- **Design origins**: `.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`
  Parts F–G (D-BF11, D-BF12), §5 test hooks, §6 risks; ruling **Q4** (v1 ceremony;
  files→Deck fenced); DR-8 runbooks (`docs/files-sync-runbook.org` — the recovery-section
  house style); the connector ceremonies' house style
  (`crates/taskboard-app/src/connector_ceremonies.rs`: named steps, typed-failable,
  honest preludes, step comments narrate the choreography).
- **Type**: plan (stage 1 of the three-stage protocol; no implementation in this chunk's
  stage 1). TDD-first: every step below names its test before its code.

## 0. Verified anchors (verified 2026-11-08 against `origin/main` @ `05dbced3`)

| Fact | Anchor |
|---|---|
| A `boards` verb already exists; `Move` slots into `BoardsCmd` | `taskboard-app/src/cli.rs:66` (`Command::Boards`), `cli.rs:264` (`BoardsCmd::{List, Select, Publish, Use}`) |
| The boards command handlers + error-vocabulary mapping live here | `taskboard-app/src/commands/boards.rs`; `commands/mod.rs:678` (`RemoteBoardInUse => "remote-board-in-use"`) |
| The ownership map is derived from binding rows, never config | `taskboard-domain/src/ownership.rs` (`BoardOwnershipMap::from_state/from_boards/owner_of`); derived at `taskboard-app/src/bootstrap.rs:1283`; orphan guard `BootstrapError::OrphanedConnectorBinding` |
| Second claim = typed refusal | `taskboard-domain/src/command.rs:109` (`CommandError::RemoteBoardInUse`), guard at `command.rs:1869` |
| Retirement = the engine's unbind, with a stranded-ops count | `StateCommand::UnbindBoard { board }` at `command.rs:1940`; `CommandOutcome::Unbound { stranded_ops }`; `count_stranded_ops` at `command.rs:3185` |
| Outbox quiescence is a state query, not a drain | `derive_sync_states` at `taskboard-domain/src/state.rs:405` (per-board `pending_ops`) |
| The final Deck pull is the per-board pull leg | `taskboard-sync-nextcloud/src/actor.rs:411` (`cycle`: push → pull → report, per board); **Deck 404 on pull currently synthesizes a tombstone** (`actor.rs:1172`) — the retirement typing must intercept this |
| The files lane's first publish is the boot-snapshot leg | `taskboard-sync-files/src/board_gc.rs:33-45` (`SNAPSHOT_EVERY_N_CHUNKS`, `snapshot_due(chunks_since, is_boot)`); the lane lives in `taskboard-sync-files-nextcloud/src/board_cycle.rs` (`Lane { pending_boot_snapshot, folded_once, .. }`, `run_board_lanes`) |
| Status vocabulary (one more lane, no new taxonomy) | `taskboard-sync-files-nextcloud/src/status.rs` (`PeerFreshness`, `CycleOutcome`, `ParkKind`, `FilesSyncStatus { forked_paths, parked, diverged_from }` on a `watch`) |
| Dual-tier journey conventions | `taskboard-sync-files-nextcloud/tests/it_docker.rs` + `tests/common/mod.rs` (`TASKBOARD_IT_DOCKER` prefix, `files_transport`); app-side `taskboard-app/tests/it_docker.rs` (`TASKBOARD_IT_DOCKER_TOKEN/URL`, `#[ignore]` skip-never-fail); `.env` per `docs/testing_strategy.org` §8 |
| Runbook house style | `docs/files-sync-runbook.org` (`** Symptom / ** What the app is doing / ** Cause / ** Recovery (matches the code's semantics) / ** What NOT to do / ** Prevention note`) |

## 1. Verb surface

One new `BoardsCmd` variant, in the guided-ceremony style of
`connector_ceremonies.rs`:

```text
boards move <BOARD> --to-files            # the v1 ceremony (Q4)
boards move <BOARD> --to-deck             # exists only to refuse, typed, citing Q4
```

- `BOARD`: existing board selector (same resolution as `boards select/use`).
- `--to-files` is the only live direction; `--to-deck` parses and refuses
  `MoveError::ReverseFenced` (a typed variant whose doc cites Q4). No `--force`,
  no `--dry-run` in v1 — the ceremony itself is the safety (each step prints
  what it did and what undoes it); if a need shows up in review, it is a
  follow-up issue, not a flag.
- While a move is in flight for a board, other verbs that would touch the
  board's binding (`boards use`, `connect` re-claims) see the ordinary
  `RemoteBoardInUse`/`BoardAlreadyBound` refusals — no new lock surface.
- Output: one line per completed step (`move: step 1/4 done — final Deck pull
  converged, outbox quiet (0 pending)` …), then the terminal line. Progress is
  the ceremony log, not a status screen.

**Test (TDD, `tests/cli_verbs.rs` + unit)**: `--to-deck` refuses typed; unknown
board refuses typed; the move verb is registered and dispatches to the
ceremony entry.

## 2. The four steps, their typed states, and the resumable state machine

Each step is typed-failable and idempotent-by-inspection. The machine's state
is **not** persisted as a new blob: it is *derived* from facts the app already
owns (binding rows, `boards.remote` tag, files-subtree existence via the
files tier's identity probe, outbox counts). Crash anywhere ⇒ rerun
`boards move <board> --to-files` and the machine re-derives which step is
next; re-running a completed step is a no-op, never a re-publication with
different content. This is the same "state lives in the world, not in a
checkpoint file" posture the connector ceremonies take.

### Typed states

```text
enum MoveStage {
    NotStarted,          // board bound to Deck, no files subtree
    Pulled,              // step 1 converged: outbox quiet, Deck cache current
    Published,           // step 2 done: files subtree exists w/ snapshot floor
    DeckRetired,         // step 3 done: binding unbound (undo window OPEN)
    Committed,           // step 4 done: Files ownership assigned (undo CLOSED)
}
enum MoveError {
    ReverseFenced,               // --to-deck (Q4)
    OutboxNotQuiescent { pending: u64 },   // step 1 exit guard
    DeckPullFailed(<typed DeckError>),     // step 1
    PublishFailed(<typed files-transport error>), // step 2
    FilesSubtreeForeign,         // step 2 pre: subtree exists w/ another identity
    UnbindRefused(<CommandError>),         // step 3
    AssignRefused(<CommandError>),         // step 4 (RemoteBoardInUse etc.)
}
```

`MoveStage` is a *diagnostic* (what the machine derives), not a stored enum —
no migration, no state-engine additions. The engine-side writes are exactly
two existing commands (`UnbindBoard`, `BindBoard`/the files assignment), so
snapshot-testing the engine covers them; the ceremony orchestration is
app-level and tested with fakes.

### The steps

1. **Final Deck pull + outbox quiescence.** Drive the Deck connector's
   per-board cycle once (`actor.rs:411` seam, app-driven, one shot), then
   guard: `pending_ops(board) == 0` via `derive_sync_states`. Exit typed
   (`OutboxNotQuiescent`) if the user is still editing on this device — the
   ceremony tells them to stop editing and rerun; it never drops ops.
   *Idempotent*: pulling a second time is harmless; the guard is the real gate.
2. **Publish the board subtree.** Ensure the board's files identity
   (`boards.remote = Files(uuid)` assignment happens here on the *binding*
   side via the engine — see the ordering note below), import the folded Deck
   state into the lane, then drive one board-lane cycle with
   `pending_boot_snapshot` set — `snapshot_due(_, is_boot=true)` makes the
   mover's first publication a snapshot: **the converged state is the late-join
   floor** (D-BF11). Pre-guard: if a files subtree already exists at that
   identity that this device didn't write, refuse `FilesSubtreeForeign`
   (adoption, not fork — the 412 create-once discipline stays the transport's).
3. **Retire the Deck binding — loud, typed, undoable-before-commit.** Issue
   `UnbindBoard` (returns `stranded_ops`; assert 0 — step 1 guaranteed it).
   The Deck board on the server is **not deleted** by v1 (the transport has no
   DELETE verb and deletion is the user's DR-8 retirement call, documented in
   the runbook). The undo window lives exactly here (§3).
4. **Assign `Files` in the ownership map.** The assignment is the binding row
   (`Board.remote = Files(id)`, written by the engine — `BindBoard`'s files
   arm), which `BoardOwnershipMap::from_state` picks up at the next bootstrap
   / routing read. Commit point. Reverse-fence check also re-runs here: the
   board must not have re-gained a Deck claim mid-ceremony.

**Ordering note (the review trap):** step 2's identity assignment and step 4's
ownership write are the *same* engine write split in time is NOT the design —
there is exactly one binding write per direction, and the map can never show
two owners transiently because the Deck→Files transition is one row
(`Board.remote`) rewritten under the engine's command lock; the ceremony's
steps 2–4 sequence *orchestration*, not state writes. The plan pins: publish
(transport-visible) happens before retire (binding write), and retire before
the files binding write, so a crash between 2 and 3 leaves the Deck binding
intact (safe to rerun) and a crash between 3 and 4 leaves the board Deck-less
but files-published — rerun completes at step 4.

### Resumability matrix (the stage-2 test list)

| Crash after | State on rerun | Corrupting? |
|---|---|---|
| step 1 mid-pull | Deck binding intact; rerun pulls again | no |
| step 1 guard fails | typed exit; nothing written | no |
| step 2 mid-publish | partial chunks; lane GC/watermark resumes; snapshot floor re-derived | no (chunk/snapshot lifecycle is already crash-safe, chunk 4's pins) |
| step 3 (unbind) before files binding write | board unowned + files subtree present; rerun adopts its own subtree (identity match) and completes step 4 | no |
| step 4 (commit) | `Committed`; rerun reports already-moved | no |

**Tests (TDD)**: for each row, a fake-transport journey that kills the
ceremony at the named seam and reruns it, asserting the end state equals the
no-crash end state (engine snapshot + files document state).

## 3. The undo window's exact boundary

Undo = re-binding to Deck after retirement, and it exists **iff no files
binding write has happened and no other device has adopted the subtree as its
own**. Operationally, v1 pins the boundary at the **commit point (step 4)**:

- **Open**: from completion of step 3 through step 4. `boards move <board>
  --undo` (only meaningful while `MoveStage::DeckRetired` is derived) issues
  `BindBoard` back to the Deck remote (the retired binding's coordinates are
  still in the user's config/known connector) and re-drives one Deck cycle;
  the files subtree published in step 2 is orphaned garbage that the runbook
  tells the user to delete by hand (the transport has no DELETE — same
  posture as step 3).
- **Closed**: the moment step 4's binding write lands. After that, undoing
  means running the fenced reverse ceremony — refused, citing Q4.
- **Why not time-based**: a time window (e.g. 10 minutes) would be a quality
  ceiling on user attention and a race against other devices' adoption cycles;
  the commit point is deterministic and derivable from binding rows.
- **Loudness**: while the window is open, the app's status/findings lane
  shows the board as *moved-pending-commit*; every subsequent `boards` verb
  on that board prints the pending-move line first.

**Tests (TDD)**: undo inside the window restores the Deck binding and a
Deck cycle converges (fake transport); undo after commit refuses typed;
undo of a board that was never in a move refuses typed.

## 4. Retirement typing: what a returning offline device sees

Today a Deck 404 on pull **silently synthesizes a tombstone**
(`sync-nextcloud/src/actor.rs:1172`). After a move, that would fold the moved
board into a deletion on every device that was offline through the ceremony.
This chunk replaces that for the moved case:

- The mover does **not** delete the Deck board (§2 step 3), so a returning
  device's Deck pull still *succeeds* and gets a converged-but-stale board —
  the loud part is on the **routing side**: the device's ownership map
  (`BoardOwnershipMap::holder_of`) sees a `Files` holder for a board it still
  holds a Deck binding for. That mismatch is the new typed finding:
  `Finding::BoardMovedToFiles { board, moved_by, moved_at? }` (fields the
  payload already carries; no new taxonomy — this joins #632's vocabulary on
  `FilesSyncStatus`, one more lane).
- On the finding, the device **adopts fold-first automatically**: drop the
  Deck binding (its own `UnbindBoard`, stranded-ops asserted 0 after folding
  its local pending ops into the files lane), bind `Files`, import via the
  late-join floor (snapshot + chunks), continue. The silent-tombstone path
  (`actor.rs:1172`) keeps its behavior for *genuinely deleted* boards — the
  moved case is distinguished by the ownership mismatch, not by guessing from
  404s. (If a user *does* delete the Deck board before all devices return,
  laggards tombstone — the runbook names this as the "What NOT to do".)
- Adoption is one-way and quiet-on-success: one finding line in the cycle
  report, then ordinary files-board cycling.

**Tests (TDD)**: two-engine journey — device A moves; device B (offline
during the move, holding local edits) comes back: B folds its edits into the
files board, B's Deck binding is gone, no tombstone was synthesized, A and B
converge (the design round's "offline-during-move adoption journey").
Second test: B returns *after* its edits were already pushed to Deck pre-move
quiescence — no duplicates.

## 5. The dual-tier journey design (Tier 3)

One `#[ignore]`-gated journey in `taskboard-app/tests/it_docker.rs` (the
app-level seam is where the ceremony lives), reusing the
`TASKBOARD_IT_DOCKER` conventions:

- **Tier A (real Deck container)**: a board with entries lives in Deck.
- **Tier B (docker files tier)**: the same Nextcloud instance's files
  subtree (the docker tier already hosts both — one `TASKBOARD_IT_DOCKER_URL`).
- **Legs**: (1) two app instances, both on Deck, converge; (2) instance A
  runs the full ceremony pull→publish→retire→assign against the live tier;
  (3) instance B, mid-journey, is held back (no cycle) and returns after —
  the adoption leg of §4; (4) partition invariants asserted throughout: the
  map never shows two owners, the files document folds to the Deck-converged
  state, B's late edits converge; (5) the undo-window leg runs the whole
  ceremony a second time on a second board and undoes at `DeckRetired` —
  Deck convergence restored.
- Skip-never-fail per the house convention; runs in stage 2/3 with the tier
  env, re-pinned via `.env`.

## 6. Runbook docs outline

New `docs/boards-move-runbook.org`, in the DR-8 recovery-section house style
(`** Symptom / What the app is doing / Cause / Recovery / What NOT to do /
Prevention note`), every step naming the code symbol it exercises:

1. **Moving a board from Deck to files** — the four steps, what each prints,
   what to stop doing (editing) during step 1.
2. **The undo window** — open at step 3, closed at step 4; `boards move
   --undo`; deleting the orphaned files subtree by hand afterwards.
3. **A device was offline during the move** — the typed `BoardMovedToFiles`
   finding, automatic fold-first adoption, what "adopted" prints.
4. **Mixed-fleet operation** — Deck boards and files boards coexisting
   (D-BF5's normal); which connector syncs which board; the routing map.
5. **Retiring the Deck board on the server** — the DR-8 device-retirement
   posture applied to a board: only after every device has adopted; what a
   premature delete does (laggard tombstones); trashbin recovery cross-ref
   (`files-sync-runbook.org` §2).
6. **The reverse direction is fenced** — the typed refusal, Q4's citation,
   what would change it (a future round, not a workaround).
7. **Symptom sections** — duplicated by the moved case's failure modes:
   `FilesSubtreeForeign`, `OutboxNotQuiescent`, stranded ops.

Plus: mixed-fleet paragraph in the top-level docs map; CHANGELOG entry under
the unreleased section.

## 7. Status lane & wave-exit bookkeeping this chunk owns

- **Status lane (D-BF12)**: board-move findings join `FilesSyncStatus` on
  #632's vocabulary (one `BTreeMap<BoardId, BoardMoveFinding>`-style field or
  the finding enum extension — stage 2 picks the minimal shape; no new
  taxonomy). CLI `status` gains the moved/adopting lines.
- **Wave exit**: this chunk's PR closes #689 and #678; owns the wave-exit
  report at `.artifacts/reports/2026-11-XX-boards-over-files-wave-exit-report.md`
  (the partition model's operational story + fence hand-offs: org → #184,
  attribution → #494) — written in stage 3 after the journeys pass; the
  exit report, not this plan, is where the operational narrative lives.
- **No ADR** (0022 went with chunk 1); **no migration** (the ceremony writes
  only existing engine commands).

## 8. Implementation order (TDD-first, per test → code)

1. `MoveError` + verb surface + `--to-deck` fence tests → the verb + refusal.
2. `MoveStage` derivation (`derive_move_stage(board, map, subtree-probe)`)
   + resumability matrix tests (fake transport) → the step machine.
3. Undo window tests → `--undo`.
4. `BoardMovedToFiles` finding + offline-adoption two-engine journey tests →
   the routing-side interception + adoption.
5. Dual-tier journey (§5) — enabled last, against the live tier.
6. Runbook docs + CHANGELOG + (stage 3) exit report.

Crates touched: `taskboard-app` (verb, ceremony module `board_move_ceremony.rs`
beside `connector_ceremonies.rs`, commands, tests); `taskboard-domain`
(finding variant(s) on the existing status vocabulary, if the finding enum
lives there — otherwise `taskboard-sync-files-nextcloud/src/status.rs`);
`taskboard-sync-nextcloud` only if the routing-side interception needs a
typed hook at the cycle seam (preferred: none — app-level).

## 9. Risks & mitigations

- **Deck 404 → tombstone race** (the biggest): the moved case is detected by
  ownership mismatch, not 404; the runbook forbids premature server-side
  deletion; the adoption journey pins the ordering (typed-and-loud *then*
  fold-first) — the dispatch plan's named review trap.
- **Two-owner transient**: the map derives from one binding row rewritten
  under the engine's command lock — asserted in every resumability test.
- **Undo after other devices adopted**: post-commit undo refuses (Q4 fence);
  pre-commit undo's orphaned subtree is documented as hand-deletable. A
  device adopting during another's undo window would re-fork — mitigated by
  adoption reading the ownership map *at cycle time*: while undo is possible
  the files binding doesn't exist yet, so non-mover devices have nothing to
  adopt; the window is inherently single-mover.
- **Dual-tier flakiness**: skip-never-fail, house `.env` conventions, the
  docker legs from chunk 5 are the template.
- **Scope creep into deletion**: v1 never deletes the Deck board; the runbook
  owns the retirement step. Any transport DELETE verb idea → new issue.

## 10. Estimated diff size

~900–1,300 lines: ~400–600 production (verb + ceremony module + finding
variant + adoption interception), ~400–600 tests (resumability matrix,
undo, adoption journeys, CLI tests), ~150–250 docs (runbook + CHANGELOG),
plus the stage-3 exit report. Single PR (`feat(app): the Deck-to-files move
ceremony and mixed-fleet exit`), no stacking.

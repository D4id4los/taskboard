# Report — N-19 investigation: the files-sync pending truth

- **Date**: 2026-11-17
- **Kind**: backward-looking investigation record (the files-sync-status
  UX plan US-2 AC-1; chunk plan D-2's protocol).
- **Origin**: the operator's round-8 note N-19 — two files-bound
  boards, the per-board pending count only rising (165 / 8), the
  indicators reading Syncing/Idle, and only `identity.json` per board
  in the Nextcloud folder.
- **Method**: the chunk plan D-2's hermetic protocol — a five-shape
  fixture (task edit, stack add, label op, board rename, sections
  touch) over the scripted in-memory transport, plus the ranked
  hypotheses' instruments. No sleeps; the fixture drives
  `run_board_lanes` directly (no polling loop exists to pause — the
  tick is the test's own await).

## Typed findings

| Hypothesis (D-2 ranking) | Verdict | Evidence |
|---|---|---|
| H1 — `plan_pushes` mis-targets attributed ops to non-entity targets on a files board | **Refuted** | The probe run: all five fixture shapes attribute to the board (`attributed_board`) and plan to entity targets (`Board`/`Stack`/`Label`/`Task`) — no `Document`/`Remote*` group forms in a consistent world (the `UpdateAppData`/`FlushBoardAppData` files routing keys off the same binding the lane filter reads). |
| H2 — `entries.is_empty()` via missing entity rows | **Refuted** (consistent world) | Every planned entity target resolves in the fixture's tables; the healthy fixture publishes one chunk carrying all four dirty entities and settles all five ops `Applied` (`n19_ac1_pending_falls_after_a_cycle`, now the standing guard). |
| H3 — `folded_once` never set (the pull stage failing every tick) | **No permanent stall in the lane runner** | The instrument (`n19_h3_a_failed_pull_recovers_on_the_next_tick`): a staged 503 on the pull's first listing holds the lane open for exactly that tick; the next tick recovers, drains, and settles. A *persistently* failing transport (unreachable server, dead credentials) would hold the lane open indefinitely — identity.json from the one successful attach, nothing else, the engine's per-board rows never updated (stale Syncing/Idle) — the closest match to the operator's "identity only" symptom, but its cause is environmental, not a derivation defect. |
| H4 — repeated create-once 412 reconciles to a foreign chunk | **Refuted** | The seq space is per-(board, device); the reconcile arm already pins the own-device identity (existing tests). Not reachable in the fixture without forcing a foreign writer, which the single-writer contract excludes. |
| H5 — the engine drops files-connector `Completed` reports | **Refuted** | `apply_files_board_snapshot` (domain/pipeline.rs:1777) runs the Deck machinery's push-outcome arm verbatim; the bootstrap fan-in wires `board_reports` into the engine inbox (bootstrap.rs:1536+). |
| **New — the ref-delete family strands on a files-bound board** | **Confirmed — the fix's subject** | `DeleteRemoteCard`/`DeleteRemoteStack` ops attribute to their board for the pending count (`attributed_board` carries the board), but the files drain's group loop treats every `Remote*` target as out-of-lane ("a Deck actor drains it, never this lane") and publishes nothing. On a files-bound board **no Deck actor ever serves that binding again**: the op stays queued forever, the pending count rises cycle over cycle, and the drain exits silently before any chunk — exactly N-19's "pending only rises". The existing pin (`a_ref_delete_op_on_a_files_board_stays_queued`) encoded the Deck-era assumption that the deck connector still owns the board; after a Deck→Files move (or any files-bound life) that assumption is false. |

## The fix shape (amended delta, recorded before the fix lands)

The chunk plan D-2 expected "a derivation repair inside
`drain_board`/`plan_pushes`"; the confirmed cause is the **contract**
arm: the files lane's refusal to publish ref-delete groups. The
amendment (per the plan §5 sequencing gate, recorded here before the
fix commit):

- **The files drain materializes ref-delete groups** instead of
  skipping them: the group loop resolves the referenced entity — the
  local row via the ref's remote binding when one exists, else a
  files-tagged tombstone minted from the ref's own UUID — and relays
  the deletion as an ordinary tombstone entity in the chunk (the
  files tier's fold relays `deleted` rows natively; no new wire
  shape). A Deck-tagged ref with no resolvable local row stays queued
  with the typed warn (genuinely not ours to write).
- **The plan-shape pin's letter narrows accordingly**: "no sidecar" is
  unchanged; the ref-delete arm changes from "publish nothing" to
  "materialize the tombstone". The existing pin test is re-authored
  to pin the new contract (files-tagged refs publish; unresolvable
  Deck-tagged refs stay queued).
- The engine-side accounting needs nothing: the op settles `Applied`
  through the ordinary report lane once the chunk publishes.

## What remains outside this fix (manual confirmation, origin §8)

The operator's live tree can distinguish the two candidate drivers of
the observed "identity.json only" posture: the stranded ref-deletes
(covered by the fix — the pending count falls as tombstone chunks
publish) and a persistently failing pull stage (environmental —
credentials/server reachability; the lane's warn log names it each
tick). The operator's `tracing` capture plus the post-fix pending
count are the recorded manual confirmation step.

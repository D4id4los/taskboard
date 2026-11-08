# Boards-over-Files Wave — Exit Verification Report

- **Date**: 2026-11-09 (stage-3 review day; base `origin/main` @ the
  chunk-5 merge, branch `feat/deck-to-files-move`).
- **Kind**: the wave exit record for the boards-over-files round
  (design plan
  `.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`
  §3 Part G / D-BF12: "exit = live end-to-end + documented ceremony";
  the dispatch plan §1; chunk-6 stage-1 plan §7, which assigns this
  report to stage 3). The operational narrative lives in the runbooks,
  not here — this file records the verdicts and the evidence.
- **Verdict vocabulary**: *met* / *met — with recorded evidence
  limitation* / *not met*.
- **The wave in one line**: six chunks — F1 author-tagged clocks,
  settings/proofs sections, tagged remote identities + partition
  routing, board chunk & snapshot lifecycle, boards riding the files
  substrate end to end, and the Deck→files move ceremony with
  mixed-fleet exit — landing the second sync tier next to Deck under
  one account, with exactly one owner per board at every instant.

## The landed wave — item → evidence → tracker map

| Wave item | Evidence landed | PR | Tracker |
|---|---|---|---|
| Chunk 1 — F1 author-tagged clocks | `taskboard-domain` `TaskAuthors`/`BoardClocks` families, duel order `(clock, author)`, migration 0023 | #709 (11-07) | #684 closed |
| Chunk 2 — settings & proofs per-user sections | `settings`/`proofs` root sections, `UserDataReport` fold arms | #703 (11-07) | #685 closed |
| Chunk 3 — tagged remote identities + partition routing | `Remote*Id` family, `BoardOwnershipMap`, second-claim typed refusal | #711 (11-08) | #686 closed |
| Chunk 4 — board chunk & snapshot lifecycle | `board_chunk`/`board_payload`/`board_gc`/`board_fold` in `taskboard-sync-files` | #717 (11-08) | #687 closed |
| Chunk 5 — boards over files | files-actor board lanes, the no-sidecar routing rule, files ingestion through the ordinary LWW pipeline | #719 (11-08) | #688 closed |
| Chunk 6 — the move ceremony & mixed-fleet exit | `boards move` (`board_move_ceremony.rs`), the commit record (`sync-files/src/board_move.rs`), the sidecar marker (`sync-nextcloud/src/move_marker.rs`), fold-first adoption (`domain/src/board_move.rs`, `board_cycle.rs`), `docs/boards-move-runbook.org` | this PR | #689 (closes with #678) |

## Exit-criterion verdicts (D-BF12)

| Criterion | Evidence | Verdict |
|---|---|---|
| Live end-to-end (the dual-tier journey: two real app instances, real Deck container + real files tier on one Nextcloud; move, undo leg, offline-device adoption, partition invariants) | `taskboard-app/tests/it_docker.rs` `it_nextcloud_docker_move_board_to_files_and_adopt` — run by the stage-3 reviewer against the live tier (`scripts/nextcloud_it_setup.sh up`, Deck 1.19.0), PASS in 35.5 s on 2026-11-09 | *met* |
| The documented ceremony | `docs/boards-move-runbook.org` (all seven planned sections, each step naming its code symbol); the mixed-fleet paragraph in `docs/architecture.org`'s files-sync section; the CHANGELOG entry | *met* |
| Boards over files live end to end (chunks 1–5) | the two-engine convergence suite (`taskboard-app/tests/files_sync_convergence.rs`), the chunk-4 lifecycle laws, the Docker-tier board-cycle legs from chunk 5 | *met* |
| The partition model's operational story | every board has exactly one owner — its `Board.remote` row through `BoardOwnershipMap`; a move walks Deck → unbound → files (two owners are not representable; the runbook §3 states this in user terms); adoption swaps the binding in one engine batch (`plan_board_adoption` + `AdoptMovedBoard`, propped for dangling refs); the state engine drops a Deck report whose binding retired mid-cycle (`EngineCore::retired_deck`) | *met* |
| Fence hand-offs recorded | files→Deck is fenced by ruling Q4, typed as `MoveError::ReverseFenced` (a refusal, not a missing feature) — a future round, not a workaround (`docs/boards-move-runbook.org` §5); the org fence hands to #184 (org-mode sync connector), the attribution fence to #494 (per-user time-entry attribution) — both remain open, unscheduled | *met* |

## Recorded limits (carried, not blockers)

From the chunk-6 adoption amendment §7, all documented in the runbook:

1. **A device without files sync never adopts** — it gets the loud
   `moved_to_files` sidecar finding and syncs a frozen Deck copy until
   `[files_sync]` is enabled (the amendment's decision 1, implemented
   via the mover's Deck-sidecar marker).
2. **Older builds ignore the marker** and would mint a duplicate — the
   runbook's *before the first move* precondition: upgrade every device
   first.
3. **Findings are in-memory** — a one-shot process shows them only in
   its log; durable findings are future work.
4. **`--undo` is reachable only via an interrupted run** (the CLI runs
   retire straight into commit) — kept deliberately (amendment
   decision 3).

## Bookkeeping

- The PR closes #689 and #678; with the merge, the
  `nextcloud-files-sync` milestone's DR-9 items are all closed
  (#626–#631 DR-8, #678, #684–#689 DR-9).
- Stage-3 review findings, fixed on the branch before this report:
  (a) the crash-seam test for a death between retirement and the
  commit record was missing from the ceremony's fake-machine matrix;
  (b) the ceremony mislabeled pull/state-read failures
  (`SyncDisabled` payload, pull errors standing in for state-read
  errors) — now `DeckPullIncomplete` / `StateReadFailed`;
  (c) the CHANGELOG carried two `### Added` sections under one
  `[Unreleased]`; (d) the top-level docs map (`docs/architecture.org`)
  lacked the mixed-fleet paragraph the plan §6 required.
- Gate suite: run by the stage-3 reviewer on the branch
  (`scripts/run-quality-gates.sh`), all green — see the PR's checks
  for the CI-identical confirmation.

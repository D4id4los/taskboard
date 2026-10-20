# Chunk 6 stage-2 report — CLI/REPL connector verbs and per-connector status

- **Date**: 2026-10-20
- **Branch**: `feat/app-cli-connector-verbs` (worktree
  `../taskboard-WT-feat-app-cli-connector-verbs`), base `215750d`
  (chunk 5 merged), 8 commits, **not pushed** (handoff at
  gates-green; the user opens/merges the PR).
- **Contract**: `.artifacts/plans/2026-10-20-multi-board-cli-connector-verbs-plan.md`
  — implemented in full; no ACCEPTED design-record row touched (MD-7
  clean).

## Commits

| Commit | Content |
|---|---|
| `d433970` | feat(domain): command-sourced connector tag on `BindBoard`, binding-scoped refusals, `aggregate_rows` (D6-1/D6-2/D6-4) |
| `0db12c7` | feat(app): the additive per-connector status answer (D6-3/D6-4/D6-7) |
| `1a4432f` | feat(app): `--connector` on the targeting verbs, `resolve_connector`, scoped sync (D6-5/D6-6 sync half) |
| `73fc71b` | feat(app): the REPL session board selection — `boards use` + pre-fill (D6-6) |
| `1e41c8b` | feat(app): completion grows boards, connectors, and the new surface (D6-8) |
| `e0bd660` | test(app): the move-board tier-2 journey + the wire additivity pins (D6-9) |
| `1f8ee41` | chore(changelog): Unreleased lines (D6-12) |
| `5c14443` | docs(state): the spawn doc example drops the removed arg |

## Gate results (AGENTS §10)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | green |
| `cargo clippy --workspace --all-targets -- -D warnings` | green (0 errors) |
| `cargo nextest run --workspace` | green — 1263 passed, 0 failed, 31 skipped (the skipped are the `#[ignore]`d tier tests + env-gated) |
| `cargo test --doc --workspace` | green (after the doc example fix, `5c14443`) |
| `cargo deny check` | green (advisories/bans/licenses/sources ok) |
| Coverage gate, CI-identical three-command sequence | green — 1263 passed under instrumentation; quarantine swept 1520 profraws, all valid; `lcov.info` written; `target/llvm-cov-target` removed |

## Tier-2 record (MD-5)

`it_nextcloud_docker_move_board_between_connectors` **ran and passed
on this runner**: two test-owned instances (`-c6a`/8392, `-c6b`/8393),
108.8 s wall, both instances torn down (verified: no `taskboard-it-*`
containers left). The chunk-5-documented host-kill flakiness did **not**
reproduce this run — one attempt, one pass; no journey weakening, no
skip. The honest MD-5 skip note remains in the test for runners
without the tier context.

## Verification of the acceptance points

- **Strictly additive JSON**: the `sync` field's value shape is the
  all-boards aggregate, byte-pinned; the two new keys append after
  `connector_attached` (order pinned through `to_string`);
  `{"daemon": false}` passes **unmodified**; populated arrays
  round-trip and old shapes decode with `None`s (both directions,
  protocol.rs + status.rs).
- **Attribution derived, never stored**: one derivation
  (`derive_connector_views`) over bindings + the per-board map; the
  tier journey reads it through `StatusView::server`, never re-derives.
- **Interim fill removed**: the repl.rs:1065 INTERIM comment is gone
  (assertion preserved, comment rewritten); the session's board
  resolution is now `current_board` + `prefill_boards`. The ui crate's
  three chunk-7 markers are untouched.
- **Move-board exit criterion**: asserted per board (l1 moves to beta,
  l2 stays under alpha), server-side adoption checked on both
  instances, world built entirely through ceremony verbs (no db
  fixture).
- **Snapshot rule (D6-11)**: zero `.snap` drift, no `.pending`
  snapshots — the corpus passes the same tags explicitly.

## Deviations from the plan (with reasons)

1. **`spawn_state_engine`'s `default_connector` param removed**
   (d433970). The plan said "`EngineParts.default_connector` **stays**:
   the sync-report adopt-new mint still uses it" — but the adopt-new
   mint post-chunk-5 keys off the *reporting connector* on the wire
   (`SyncReport::Completed { connector, .. }` → `apply_sync_report`),
   so the spawn param was dead the moment `plan_command` lost its own
   `default_connector` argument. Removed rather than left as
   `#[allow(dead_code)]`; behavior identical, all spawn sites updated.
2. **`boards::select_prelude`/`create_on`/`unbind_on` and
   `ConnectorSpec` made `pub`** (e0bd660). The journey drives the
   ceremony steps directly (the plan's "no db fixture" mandate); they
   were `pub(crate)`, invisible to an integration test. `select_on`/
   `publish_on` were already `pub` — this completes the seam. No
   semantics change.
3. Minor mechanical deltas: the sync scope helper is split into a pure
   `scoped_sync_status(state, scope)` for hermetic unit testing;
   `boards use` is dispatched as a sync (non-async) session function —
   it has no awaits.

None of these touches an ACCEPTED design-record row; the D6-1 scope
deviation was pre-accepted by the stage-1 plan.

## Traps checked (dispatch §7)

`boards use` intercepted before the immutable-session dispatch and
refused one-shot (`session-only`) and over the IPC Exec lane (code 2);
completion offers boards/connectors only at their own positions
(pinned); the zero-bound connector renders configured-and-idle (unit +
journey); per-connector health is the map-derived fold
(`aggregate_rows`), the authority untouched and still private.

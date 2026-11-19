# Incident report — files-sync board lanes fail `NotFound` forever; outbox pending grows (2026-11-18)

Scope: operator reproduction on the real Nextcloud (cloud.agerekg.at), post-PR-#925.
Read-only investigation; no code, DB, or config was changed.

## Symptom

- Two files-bound boards (Household `01a175c4-aa5c-7525-8751-18c2576d5375`,
  Tech Stuff `01a1da6f-03f3-71c2-81ae-b1a10dde2335`) log, every tick:
  `WARN board lane failed; retried next tick err=NotFound`.
- Once, at startup: `board subtree adopted fold-first remote=<board uuid>`
  and `OwnFileAdopted` for `taskboard/devices/01a1ac07-8f47-72ad-af44-007be019906f/appdata.json` + `info.json`.
- Outbox pending grows (231 ops in the local DB, queued 2026-11-01 → 2026-11-18).
- Status chip neutral; sqlite `sync_status` says `idle` with fresh `last_success`
  and no `last_error` for all three boards.

## Root cause (ranked, with code evidence)

### H1 — CONFIRMED: the board drain PUTs a chunk into an `ops/` collection nobody ever creates; the real server 404s; the fake transport auto-creates, so tests never saw it

1. `attach_board` (`crates/taskboard-sync-files-nextcloud/src/board_cycle.rs:534-568`)
   MKCOLs exactly: `taskboard/boards/`, `taskboard/boards/<uuid>/`,
   `.../devices/`, `.../devices/<self>/`, then create-once PUTs
   `identity.json`. **It never creates `.../devices/<self>/ops/`.**
   A repo-wide grep confirms no production MKCOL targets `ops` anywhere
   (`cycle.rs` ladder: root, `devices/`, `devices/<self>/` only;
   `board_cycle.rs` ladder: the four above). The per-user tier never
   publishes ops chunks (`device_ops_path` is unused in production code),
   so the gap is exclusive to the board drain.
2. `drain_board` (board_cycle.rs:1021-1026) PUTs the chunk create-once at
   `taskboard/boards/<uuid>/devices/<self>/ops/000001.json`. On Sabre/DAV
   (Nextcloud), a PUT whose parent collection does not exist surfaces as
   **404**, which `map_status` maps to `TransportError::NotFound`
   (`crates/taskboard-sync-files-nextcloud/src/client.rs:154`). That is
   the exact `err=NotFound` in the log.
3. Per-tick sequence in `run_lane` (board_cycle.rs:507-528):
   attach is skipped after the first tick (`lane.attached == true`),
   pull succeeds (the `devices/` listing exists), drain 404s →
   `run_board_lanes` logs the WARN and moves on
   (board_cycle.rs:151-156). The failure is **permanent**: no code path
   ever creates `ops/`, so every retry fails identically, forever.
4. Why the tests pass: `FakeFilesTransport::put_create`
   (`crates/taskboard-sync-files-nextcloud/src/test_support.rs:411-438`)
   does **not** enforce parent-collection existence (its `mkcol` does,
   line 313-318 — the asymmetry is the test gap). So the fake lets the
   chunk land in a nonexistent `ops/` dir and the entire drain journey
   tests at board_cycle.rs:1570+ exercise a shape the real server refuses.
5. Why only two boards fail: only those two are files-bound
   (`boards.remote_files_id`, connector `files-sync` in the DB) and have
   outbox ops. "Welcome to Nextcloud Deck!" is Deck-bound
   (connector `cloud-agerekg-at-taskboard-test`) — no files lane at all,
   hence idle/healthy.

### H2 — CONFIRMED (second bug): lane failures are invisible to every status surface

- `run_board_lanes` failure path only `tracing::warn!`s
  (board_cycle.rs:153-156); it returns a count, never an error.
- `FilesSyncStatus` (`crates/taskboard-sync-files-nextcloud/src/status.rs:82-101`)
  has **no per-board lane outcome/error field** — only `last_cycle` for
  the *per-user* cycle, which is healthy. The chip reads that watch →
  honest per its contract, blind to lanes.
- The sqlite `sync_status` rows are upserted by the Deck/engine
  accounting path (`crates/taskboard-storage-sqlite/src/repo.rs:2394`);
  nothing in the files tier writes them. So `idle` + fresh
  `last_success` for the failing boards is real: no surface was told.

### H3 — REJECTED: cross-connector identity / subtree under the other account

- There is exactly one files-sync actor per process; its
  `SyncContext.connector` is the literal `"files-sync"`
  (`crates/taskboard-sync-files-nextcloud/src/actor.rs:292`), and its
  credentials come from the anchor connector — config sets no
  `[files_sync] anchor`, so it resolves to the *first* Nextcloud
  connector in walk order = `cloud-agerekg-at-viktoria`
  (`crates/taskboard-app/src/config.rs:1403-1421`). All lane traffic is
  viktoria's own DAV home.
- The fold-first adoption (412 on the identity create-once PUT,
  board_cycle.rs:560-564) proves the subtree **is visible and
  pre-existing under viktoria's credentials** — if it were not, the
  identity PUT would have succeeded (201) and there would be no
  "adopted fold-first" log line. The `OwnFileAdopted` findings for
  `devices/01a1ac07/…` say the tree was seeded by a *former install*
  (device `01a1ac07…`, the same uuid stored in both boards' remote
  card/stack columns) — same account, older device id. Connector mix
  plays no role.

### H4 — REJECTED: adoption/push path-shape mismatch

Adoption and push both go through `BoardId::from_uuid(remote)` +
`board_identity_path`/`board_ops_path` from the single `layout`
vocabulary; the failing request is not a *different* path, it is the
same path missing its `ops/` parent (H1).

## Reproduction story (matches the log exactly)

1. An earlier install (device `01a1ac07…`, viktoria's account) moved
   Household and Tech Stuff to the files tier: subtrees
   `taskboard/boards/<uuid>/…` with identity.json exist on the server.
2. The current install boots. Per-user attach adopts the former
   install's device files → `OwnFileAdopted` ×2. Board lanes attach:
   identity PUT gets 412 → "board subtree adopted fold-first" ×2.
3. Pull succeeds (empty/peer listings). Drain finds queued ops
   (Household: 1 update_board + 3 flush_board_app_data; Tech Stuff:
   create_task), materializes a chunk, PUTs
   `.../devices/<new-device>/ops/000001.json` → 404 (no `ops/` dir).
4. Every subsequent tick: attach skipped, pull ok, drain 404 → WARN,
   ops never reported `Applied`, outbox never drains, and new local
   edits keep enqueueing → 231 ops and counting. Chip neutral (H2).

## Fix plan

1. **The one-line fix (board lane attach, `board_cycle.rs::attach_board`)**:
   after the `self_dir` MKCOL, add
   `mkcol_exists_ok(transport, &format!("{self_dir}/{}", layout::OPS_DIR_NAME))`.
   Idempotent (exists-ok); on a restart the lane re-attaches
   (`lane.attached` is in-memory only) so deployment + restart heals the
   stuck lanes with no migration. Ops resume draining.
   - Consider the symmetric question for the per-user tier: it does not
     publish ops chunks today, but if `device_ops_path` ever goes live it
     will hit the identical trap — a test pinning the ladder's complete
     dir set is the cheap insurance.
2. **Test gap (must land with the fix)**: make
   `FakeFilesTransport::put_create`/`put_if_match` enforce
   parent-collection existence the way `mkcol` already does (the real
   server 404s on a missing parent; the wiremock tier already pins the
   409-on-missing-parent posture for MKCOL — add the PUT counterpart).
   This turns the existing drain journey tests red without the fix —
   the failing-test-is-a-bug case, fixed on the production side.
   Add: (a) fresh-device-on-adopted-subtree drains successfully (the
   incident scenario); (b) a ladder-completeness test asserting the
   exact MKCOL sequence includes `ops/`.
3. **Honesty fix (H2)**: add a per-board lane outcome to
   `FilesSyncStatus` (e.g. `lanes: BTreeMap<BoardId, LaneOutcome>` with
   a typed last error) published from `run_board_lanes` via the watch,
   and surface failing lanes in the status chip / `sync status`. Optional
   deeper fix: let the files tier upsert `sync_status` rows per files
   board so sqlite stops contradicting the logs — at minimum document
   that `sync_status` is Deck-tier-only.

## Salvageability of the 231 queued ops

**Salvageable — no staleness triage needed.** The drain is
materialize-from-published-state, never op replay
(board_cycle.rs:11-13, 863-1001): each queued op only marks its entity
dirty; the chunk carries the entity's *current* row. Older ops are
naturally subsumed (`plan_pushes` groups by target). Once the fix
deploys and the lane re-attaches, all board-relevant ops drain in one
chunk and report `Applied`. The 4 ops denormalized to Household
(`update_board` + 3 `flush_board_app_data`) and the Tech Stuff
`create_task` are all still accurate statements about current local
state. Nothing should be dropped. (Ops predating the files binding are
harmless for the same reason — H3/H4 rejected, and the materialization
does not care when the op was queued.)

## Open questions for the operator

1. Confirm the former install (device `01a1ac07-8f47-72ad-af44-007be019906f`)
   ran the same account (viktoria). If yes, the server's
   `taskboard/boards/<uuid>/devices/01a1ac07…/ops/` may hold chunks from
   before — the fold already ingests them, no action needed.
2. Was the "Welcome to Nextcloud Deck!" board ever intended to move to
   the files tier? It is Deck-bound to `taskboard_test`; unrelated to
   this incident.
3. Optional server-side confirmation (needs creds): `PROPFIND
   taskboard/boards/01a175c4…/devices/<current-device>/` — expect `ops/`
   absent, which closes H1 empirically. Not required; the code path is
   deterministic.

## Key files

- `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs` (attach ladder 534-568; drain PUT 1021-1073; failure logging 149-160)
- `crates/taskboard-sync-files-nextcloud/src/client.rs` (404→NotFound 150-164)
- `crates/taskboard-sync-files-nextcloud/src/test_support.rs` (fake put_create parent gap 411-438 vs mkcol 295-321)
- `crates/taskboard-sync-files-nextcloud/src/status.rs` (no lane-error surface)
- `crates/taskboard-app/src/config.rs` (anchor resolution 1403-1421)
- Local state: `~/.local/share/taskboard/taskboard.db` (outbox 231 ops; boards bindings; sync_status idle)

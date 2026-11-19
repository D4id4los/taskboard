# Chunk P7 — the files GC verb (#718) + the unbind subtree decision (#755) — Stage-1 Plan

Branch: `feat/files-gc-verb` · Worktree:
`/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-files-gc-verb`
· Base: `origin/main` @ `5d6bd327`.

Normative inputs: dispatch plan
`.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §1/§8; design law D-BF7
(`.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md` §3 Part
D); issues #718 (transport DELETE + owner GC) and #755 (unbind subtree
leftover); the chunk-5 deferral note in
`crates/taskboard-sync-files-nextcloud/src/board_cycle.rs` (module doc, "GC
duty note").

**Status: leg 1 fully specified below; leg 2 is GATE-PAUSED — both options
are fully specified (§4) with a recommendation (§4.3); the dispatcher takes
both to the operator before stage 2 implements leg 2.** Stage 2 may begin
leg 1 immediately; leg 2 rides on top of it either way.

---

## 1. Seams read (evidence base)

| Seam | Where | Fact relied on |
|---|---|---|
| Verb set | `crates/taskboard-sync-files-nextcloud/src/transport.rs` | Closed trait: `mkcol`/`propfind_devices`/`propfind_tree`/`get`/`put_create`/`put_if_match`. Module doc states "readers-never-delete posture enforced by construction (F8)" — the doc comment is part of the change surface. |
| Client impl | `src/client.rs` | `send_with_retries` (retry on the transport class only), `map_status` already maps 404 → `TransportError::NotFound`, `url_for` joins root-inclusive substrate paths. `Method::DELETE` needs no `from_bytes` dance (it is a named method). |
| Fake | `src/test_support.rs` (`FakeFilesTransport`) | In-memory dir/file map, `Effect` staging (`stage`), request recording (`requests()`/`count()`). `impl FilesTransport` at line 306. |
| Tier harnesses | `tests/client_wiremock.rs`, `tests/it_docker.rs`, `tests/it_live.rs`, `tests/common/mod.rs` | wiremock status→typed-error table at client_wiremock.rs:136; it_docker/it_live are `#[ignore]`-gated real-server tiers; `it_live.rs:201-241` already uses a *raw reqwest* DELETE as test scaffolding (F8 note: "the transport's verb set stays closed — readers never delete") — that comment is now stale and must be updated. |
| GC predicate | `crates/taskboard-sync-files/src/board_gc.rs` | `gc_eligible(&BTreeSet<u64>, Option<u64>) -> Vec<u64>` — owner-scoped ∧ watermark-bounded, `None` deletes nothing (proptested P2). Tombstones are payload fields, not ops files — no interaction with whole-file deletion. |
| Snapshot duty | `src/board_cycle.rs` `snapshot_duty` (~line 1100) | Publishes `snapshot-<seq>.json` (put_if_match / put_create), updates `lane.snapshot_etag`, resets `chunks_since_snapshot`. The published snapshot's `seq` **is** the GC watermark. |
| Lane state | `board_cycle.rs` `Lane` | `last_published_seq`, `watermarks` (per-peer ingest cursors — *not* the GC watermark), `snapshot_etag`. No persistent set of own published chunk seqs — see §3.4. |
| Discovery fold-first | `board_cycle.rs` ~208-236 | A subtree whose uuid matches a local board with no files binding is skipped ("adoption deferred to the opt-in door"); `lanes.discovered` remembers it. Nothing silently re-adopts. |
| Unbind path | `taskboard-domain/src/command.rs:1999` (`StateCommand::UnbindBoard`) | Drops the binding row + sync-status row. **No message reaches the files actor** — the actor learns of unbind only via the next tick's `read_state()`: `run_board_lanes` collects `owned` from the ownership map, and the unbound board simply stops being in `owned` while its `Lane` lingers in `lanes.boards` forever (today: stale-lane leak, never pruned). |
| Actor loop | `src/actor.rs` `run` | `select!` over the tick deadline and the broadcast (Shutdown only). No command channel into the actor exists; adding one is app+actor plumbing the round's diff-confinement rule (sync-files crate only) rules out for leg 2. |

## 2. Leg 1 (#718) — the DELETE verb + the owner's post-snapshot GC pass

### 2.1 Verb signature (decision)

```rust
/// `DELETE` outcomes: deleted (2xx) or already absent (404 — the
/// idempotent-delete mapping; a GC re-run after a crash is a no-op,
/// not an error).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted,
    AlreadyAbsent,
}

/// Deletes the file or collection at `path` (a collection delete is
/// recursive per RFC 4918 Depth semantics). Idempotent: a missing
/// target is [`DeleteOutcome::AlreadyAbsent`].
fn delete(&self, path: &str)
    -> impl Future<Output = Result<DeleteOutcome, TransportError>> + Send;
```

- Typed outcomes only ([L](a)): `Deleted` / `AlreadyAbsent` / the existing
  `TransportError` family. No string matching anywhere.
- The trait doc's closed-verb-set paragraph is rewritten: the verb set gains
  exactly one destructive verb, callable **only** from the two call sites
  this chunk adds (the owner GC pass §2.4 and leg 2's retirement §4); the
  pull/fold/publish paths stay delete-free. F8's law is re-stated, not
  dropped: readers still never delete — the *lifecycle duties* do.
- Client impl (`client.rs`): `send_with_retries(Method::DELETE, url, Vec::new,
  None, |raw| match raw.status { 204|200 => Deleted, 404 =>
  AlreadyAbsent (map_status first so 401/403 still type correctly), other =>
  map_status + Server(other) })`. Retry class unchanged.
- Stub impl in `transport.rs` tests gains the verb (the compile-shape test
  drives every verb — extended).

### 2.2 Tier pins (all three, per acceptance)

1. **Client (wiremock)** — extend `tests/client_wiremock.rs`:
   204 → `Deleted`; 404 → `AlreadyAbsent`; 403 → `TransportError::Forbidden`
   (typed, not text); a 5xx after budget exhaustion → `TransportError::Server`.
2. **Fake** — `FakeFilesTransport::delete` removes the seeded file (or the
   whole seeded subtree) from the in-memory map and returns
   `AlreadyAbsent` when absent; honors `Effect::` staging so tests can inject
   transport errors. Record the request in `requests()` (method "DELETE").
3. **Docker/live tier** — extend `tests/it_docker.rs`: seed a file
   (`run_id`-unique path), `delete` → `Deleted`, repeat `delete` →
   `AlreadyAbsent` (the 404-on-missing pin against the real server), and
   PROPFIND confirms the member is gone. Same shape for the live tier only
   if the existing live suite already exercises the verb set (mirror it
   there too — the harness comment at `it_live.rs:201` is updated: DELETE is
   no longer scaffolding-only).

### 2.3 GC pass placement in `snapshot_duty` (decision, recorded ordering)

**The pass runs after a successful snapshot publish, inside the
`Ok(PutOutcome::Created | Updated)` arm, before the `Ok(())` return — and
the watermark is the seq of the snapshot just published.** Rationale for the
ordering, against the stage-3 trap (a) in the dispatch prompt:

- Publish-first is the only safe order. The watermark's meaning is "a
  snapshot subsuming chunks ≤ seq exists on the server". If GC ran before
  the publish and the publish then failed (412 / transport error), the
  server would hold neither the chunks nor the subsuming snapshot — a
  late-joining device with no snapshot floor would replay a hole. Publish
  success *creates* the floor; GC may only consume what the floor covers.
- Crash between publish and GC: harmless — GC is re-derived every cadence
  from the server listing (§2.4); the leftover chunk files are simply
  deleted on a later pass. No durability load-bearing step is skipped.
- Crash between GC and the lane counter resets (`chunks_since_snapshot = 0`
  etc.): also harmless — counters are optimizations (the module doc's own
  law); worst case is one extra snapshot publish (idempotent If-Match
  rewrite).
- The 412 arm (`PreconditionFailed`) skips the pass entirely: no published
  snapshot this cadence ⇒ no watermark advance ⇒ nothing is GC-eligible.

### 2.4 The pass: deriving own chunk seqs honestly (decision)

The lane does **not** carry a persistent set of its own published chunk seqs
(a restart would lose it, and the module doc forbids correctness load on
in-memory cursors). Instead the pass does one `propfind_tree` over this
device's own ops directory
(`boards/<uuid>/devices/<own-device>/ops/`), parses the seq out of each
member name (`NNNNNN.json` → u64 — skip non-parsing names; foreign names are
not ours to read), feeds the set + `Some(seq_just_published)` into
`board_gc::gc_eligible`, and calls `transport.delete` per resulting
`board_ops_path(board, own_device, seq)`. One propfind per snapshot cadence
(boot + every 16 chunks) is negligible.

- **Owner scope is structural**: the listing root is hard-pinned to the
  device dir built from `ctx.identity.device_id` — no foreign device's path
  is ever constructed, let alone deleted (the D-BF7 owner-scoped leg holds
  by construction, test-pinned below).
- **Failure semantics**: a per-file delete error (typed) logs at warn with
  the path and continues to the next seq; the pass never fails the lane —
  eligibility is recomputed from the listing next cadence and the leftover
  file is retried then. A transport-class error on the propfind aborts the
  pass silently-for-this-cadence (same retry story).
- `TransportError::NotFound` from delete maps to `AlreadyAbsent` inside the
  verb — the GC pass treats both outcomes identically (idempotent re-run).

### 2.5 Leg-1 tests (TDD; "a GC test that deletes a tombstone or an unpublished chunk must fail by construction")

In `board_cycle.rs` tests, over the fake transport, one behavior each:

1. After the snapshot publishes with seq N, chunks 1..=N of *this device*
   are deleted; chunk N+1 (published after the snapshot) survives. *(the
   watermark bound — the unpublished chunk's survival is asserted.)*
2. A chunk belonging to a *foreign* device dir (seeded in the fake) is never
   deleted, across repeated cadences. *(owner scope.)*
3. No snapshot published (force the publish to 412 via
   `answer_412_once`) ⇒ zero DELETE requests recorded. *(`None` watermark
   deletes nothing — the pinned leg of `gc_eligible`.)*
4. Tombstone law pin: an ops chunk whose payload carries `deleted` fields
   is deleted exactly like any other (whole-file GC, tombstone fields live
   in snapshots — the inverse pin: a test asserting the tombstone chunk
   *survives* would fail by construction, which is the documented D-BF7
   semantics, not a bug).
5. A delete staged as a transport error leaves the file in place and the
   lane Ok; the next cadence retries it (two-cadence sequence test).
6. Already-absent: a listing naming a file the fake no longer holds
   (crash-between-publish-and-GC analog) ⇒ `AlreadyAbsent`, lane Ok.

Diff surface leg 1: `transport.rs` (+verb, +doc rewrite, +stub),
`client.rs` (+impl, +wiremock pins), `test_support.rs` (+fake delete),
`tests/it_docker.rs` (+the 404 pin), `tests/it_live.rs` (+comment update,
mirror pin), `board_cycle.rs` (+the pass + its tests). Entirely inside
`taskboard-sync-files-nextcloud`.

## 3. Stale-lane leak (discovered while reading the seams — fixed inside leg 2's trigger)

`run_board_lanes` never removes a `Lane` from `lanes.boards` when its board
stops being files-bound (today it just stops being in `owned` and lingers
forever, including its per-peer watermarks and etag caches). Leg 2's trigger
(§4.1/§4.2) prunes the lane as part of whichever retirement path the
operator picks; leg 1 alone leaves this as-is (no behavior change, but the
plan records it so leg 2's diff covers it deliberately rather than
accidentally).

## 4. Leg 2 (#755) — the unbind subtree leftover — GATE-PAUSED (operator chooses)

**Both options are specified in full; the dispatcher takes both to the
operator. Stage 2 implements the recorded choice on top of leg 1.**

### 4.0 Shared trigger seam (both options)

The files actor learns of unbind on its next tick: after collecting `owned`,
diff `lanes.boards` keys against it. A lane whose board is no longer
files-bound is **retiring**: the actor owns the transport and the remote
uuid (the lane's board subtree is `boards/<remote>/`), so the retirement
runs in the actor, per tick, until complete — no new command channel, no
app/engine plumbing, diff confined to the sync-files crate. The engine's
`UnbindBoard` (and undo, `undo.rs:424`) are untouched: unbind is
instantaneous at the state level; the remote bytes drain in the background.

Typed outcome set for the retirement duty (both options): `Retired` /
`AlreadyRetired` / deferred-with-typed-error (retried next tick). Surfaced
on the lane-outcomes watch (`LaneOutcome`) so a stuck retirement is visible,
never silent.

### 4.1 Option A — GC-on-unbind (immediate subtree delete at unbind)

The retiring lane's tick runs a subtree retirement: one
`transport.delete("boards/<uuid>")` (collection delete, recursive — §2.1's
verb already covers collections), then removes the lane + the
`lanes.discovered` entry. Convergence: the discovery fold's
foreign-subtree branch (board_cycle.rs ~208-236) never sees the subtree
again — the bytes are gone, and the unbound-local-board arm already defers
adoption for any local row, so nothing re-adopts during the drain window.

- **Failure semantics**: transport error (or the board disappearing from
  `read_state` mid-tick) ⇒ the retirement is deferred, the lane stays in a
  `retiring: BTreeSet<uuid::Uuid>` set inside `Lanes`, and the next tick
  retries the single DELETE. Typed, idempotent (`AlreadyAbsent` =
  `AlreadyRetired`), no partial state: collection DELETE is all-or-nothing
  server-side. A permanently unreachable server leaves the subtree on the
  server and a visible failed lane outcome — honest, and the Stop Sync
  action's *state* effect is never blocked by it (the engine already
  unbound; the actor's failure is a cleanup finding, not a command
  refusal).
- **Wire format**: none. No new files, no discovery changes, no
  fold/parse code.
- **D-BF7 tension, recorded**: the design law's "tombstones are forever"
  protects a *live* board's delete-duel defense. A subtree retirement
  removes those tombstones, so a stale peer still bound to the same board
  could, on its next push, resurrect content into a fresh subtree. This is
  the same trade the DR-8 runbook's device-retirement procedure already
  accepts — retirement is a user decision, and Stop Sync is per-device
  binding, not a fleet-wide board kill. The residual risk (another device
  still bound, offline through the unbind, later re-publishing) is
  **documented in the module doc** and inherent to unbind-then-delete; it
  is the reason §4.2 exists as the alternative.

### 4.2 Option B — retirement-marker-then-lazy-GC

Analog of the Deck move's `moved_from` record (`board_move.rs`,
`write_move_marker` shape):

1. **Marker write** (the retiring lane's first tick): create-once
   `put_create("boards/<uuid>/retired.json")` — a `RETIREMENT_RECORD_FORMAT
   = 1` payload (`format_version`, `retired_by_device`, `retired_at`). A
   412 (`PreconditionFailed`) means already marked — the marker arm
   proceeds.
2. **Discovery prune rule**: `discover_subtrees` and the bound-lane fold
   read the marker when they touch a `boards/<uuid>/` subtree: a marked
   subtree is never folded, never adopted, and its lane is dropped from
   `lanes.discovered`/`boards`. Fold-first logic at ~208-236 gains one
   early-continue arm (explicitly pinned, not silent).
3. **Lazy GC trigger**: a marked subtree whose local lane is retiring is
   deleted on the retiring lane's tick (same single collection DELETE as
   Option A) — but only **after** the marker round-trips (write observed,
   or create-once 412 observed), so intent is durable on the server before
   the bytes go.
4. **Marker survival**: the marker lives *inside* the subtree, so the
   DELETE consumes it too — the durable intent exists only between marker
   and delete. A peer that folds in exactly that window sees the typed
   retirement and stops; after the delete the subtree is simply absent
   (same end state as Option A). A sibling-path marker
   (`boards/retired/<uuid>.json`, outside the deleted subtree) would
   survive forever, but adds a new top-level layout kind and a permanent
   per-board byte — rejected for v1 (recorded as the alternative if the
   resurrection risk ever bites in practice).
- **Cost/benefit**: strictly more machinery than Option A (new wire record,
  new layout path, discovery arm, codec + round-trip tests, ~150 extra
  lines) in exchange for one narrow gain — the still-bound-stale-peer gets
  a typed retirement signal during the marker-to-delete window instead of
  silently re-publishing into a fresh subtree. Outside that window the
  resurrection risk is identical to Option A.

### 4.3 Recommendation (recorded): **Option A — GC-on-unbind**

Stop Sync's meaning ("stop syncing this board here") plus fold-first
discovery's existing deferral for unbound local boards means no local
reader ever needs the subtree again; Option B's only marginal gain (the
stale-peer signal window) does not justify a new wire format and a
discovery-rule change whose main effect is * prolonging* the subtree's
existence. The unbind leftover is a cleanup duty; idempotent background
deletion with visible typed failure is the honest minimal shape. The
stale-peer hazard is a documented module-doc law either way (and if the
operator wants belt-and-braces, Option B remains implementable later on
top of leg 1 without rework — the trigger seam and the verb are shared).

## 4.4 Addendum (2026-11-19) — the operator's gate decision, as amended

The operator cleared the leg-2 gate choosing **Option A (GC-on-unbind)**
with one binding scope restriction from the dispatcher: **the unbind
cleanup deletes only THIS device's files under the remote board — never
the whole remote board subtree** — because a different client may still
be connected to that board. Concretely, §4.1's single collection
`DELETE "boards/<uuid>"` is replaced by:

- The retiring lane's tick deletes **this device's own directory only**:
  `boards/<remote>/devices/<own-device>/` (one collection DELETE — the
  directory's contents are entirely this device's: `info.json`,
  `ops/NNNNNN.json`, `snapshot-NNNNNN.json`). The owner-scoped law is
  the same one leg 1's GC pass enforces structurally: only paths
  derived from `ctx.identity.device_id` / this device's ops and
  watermarks are ever constructed or deleted; the board's shared bytes
  (`identity.json`, `move.json`, other devices' directories) are never
  touched.
- Failure semantics unchanged from §4.1: a transport error defers the
  retirement into `Lanes::retiring`-equivalent handling (the lane stays,
  the next tick retries the single DELETE), `AlreadyAbsent` is the
  idempotent success, and the outcome surfaces typed on the
  lane-outcomes watch.
- Lane + `lanes.discovered` pruning happens on retirement completion
  exactly as §3 recorded (the stale-lane leak fix rides this trigger).
- The §4.1 "D-BF7 tension" paragraph stands, narrowed: only this
  device's tombstone-bearing chunk files are removed; another device's
  snapshots (which carry the board's tombstone floor) are untouched, so
  a still-bound peer's late-join floor survives the unbind.
- Fail-by-construction test law ([L](c) amendment): a test asserting
  that a foreign device's ops dir or the board's shared subtree survives
  the retirement — and that every recorded DELETE path sits under this
  device's own directory prefix — pins the scope.

## 5. Risks

1. **DELETE deletes remote data** — the stage-3 review's line-by-line
   D-BF7 audit is load-bearing; the eligibility tests (§2.5) are the
   enforcement, not decoration.
2. **Nextcloud collection-DELETE semantics** — recursive Depth delete +
   trashbin: deleted subtrees are user-recoverable (mitigating), but the
   docker tier test must pin the recursive behavior explicitly rather than
   assume it.
3. **wiremock + `Method::DELETE`** — trivial, but the mock's default
   matchers must be told the method; a mock matching any method would hide
   a wrong-verb bug (pin `Method::DELETE` in the mock expectation).
4. **The stale-lane leak (§3)** — leg 2 removes the lane; if the operator
   chose "do nothing" the leak would persist (not an offered option —
   recorded here so its fix never gets lost: it rides leg 2's trigger).
5. **Timing**: leg 2's retirement runs on the tick cadence; "immediate" in
   Option A means next-tick (the dispatch plan's language "at unbind time"
   is interpreted as actor-tick-immediate — a new command channel is
   out of the round's diff-confinement budget; recorded decision).

## 6. Estimated diff size

- Leg 1: ~350–450 lines incl. tests (6 seams + 3 tier pins).
- Leg 2 Option A: ~150–250 lines (trigger diff + retire pass + tests).
- Leg 2 Option B: ~350–450 lines (marker codec + layout + discovery arm +
  tests).

## 7. Open questions — none remaining

Every question raised by the dispatch prompt is resolved above with a
recorded decision: verb signature + outcome enum (§2.1), tier pins (§2.2),
GC placement + ordering + crash windows (§2.3), own-seq derivation + owner
scoping + failure semantics (§2.4), leg-2 trigger seam (§4.0), both leg-2
options fully specified (§4.1/§4.2) with a recommendation (§4.3).

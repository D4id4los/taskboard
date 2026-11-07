# [Plan] Boards over Files — Design Round (B-full: F1 author clocks, the chunk lifecycle, partition routing, the Deck→files move)

- **Date**: 2026-11-07
- **Kind**: design-round doc — **DR-9** (issue #678, milestone
  `nextcloud-files-sync`): B-full, the committed successor to the
  landed B-min substrate. Task/stack/label/board state rides the file
  substrate instead of Deck, **partition-only**. Design only — no
  production code.
- **Origin**: the user's 2026-11-07 session rulings Q1–Q6 (recorded on
  #678 — restated normative in §0 below); the committed-successor
  ruling of 2026-11-04 (options report §9 + the milestone
  description); the backbone report
  `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  (§5.5 chunk/snapshot design, §7.3 F1 — "the one non-negotiable code
  change", §7.4 the vocabulary decision point); the accepted DR-8
  round + its landed substrate (`.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`);
  the coexistence verdict (options report §4.B — stands, not
  re-litigated).
- **Status**: **ACCEPTED** — the user reviewed and approved the round
  as designed (2026-11-07). Implementation is the six chunk issues the
  round opened (§7: #684, #685, #686, #687, #688, #689); the dispatch
  round is
  `.artifacts/plans/2026-11-07-boards-over-files-implementation-dispatch-plan.md`.

## 0. Session rulings in force (2026-11-07, user)

| ID | Ruling |
|---|---|
| Q1 | **Native per-field chunks.** Org-native boards wait for the dedicated far-future org pass and arrive as a **separate sync connection** — no org anything in B-full; the projection idea retires into that pass |
| Q2 | **F1 confirmed** — author-tagged clocks across every roaming clock family, domain-wide, as the first chunk |
| Q3 | **Settings/proofs slice folded in** as an early chunk (∥ F1; closes D5) |
| Q4 | **The Deck→files move ceremony is v1**; files→Deck stays fenced |
| Q5 | **Widen the port** — connector-tagged remote identities; no synthesized Deck-shaped ids |
| Q6 | **Boards activate the reserved §5.5 kinds** (chunks + snapshots + GC); per-user sections stay current-state — mixed kinds by design |

## 1. Context & verified anchors (verified 2026-11-07 against `main` @ `01d4704c`)

| Fact | Anchor | Evidence |
|---|---|---|
| The substrate is live, with the F1 seam already in the fold | `taskboard-sync-files/src/fold.rs` | total order `(rank, clock, author, content)` :4/:108-128; the `author` slot documented `None < Some` ("a B-full-authored row outranks an authorless one") :17-19; `UserRecord::{Entry, Tombstone, Verbatim}` ranks |
| The core crate's modules are the round's extension points | `taskboard-sync-files/src/` | `layout.rs` (the reserved kinds `ops/`, `snapshot-*` tolerated opaquely), `document.rs` (tolerant codecs, reserved section names `settings`/`proofs`), `heartbeat.rs`, `identity.rs`, `ids.rs` (AuthorId), `hash.rs` |
| The transport crate's cycle is the board cycle's template | `taskboard-sync-files-nextcloud/src/` | `actor/client/cycle/device/etag/extract/poll/status/transport`; conditional-PUT discipline + docker-tier pins F3–F6 (characterization report) |
| The per-user lane exists end to end | `main` @ `01d4704c` | `UserDataReport` (domain), the engine's user-data fold arm (state), the journal wire codec (sync-files), the state-derived local document + import step + wiring (app); the two-engine journal convergence journey |
| The clock families are plain `DateTime<Utc>` fields today | domain `entities.rs` | `TaskClocks` :385, `StackClocks` :426, `LabelClocks` :437, `BoardClocks` :525 — no authors; stored via the shared clock text forms (the storage codec's `decode::clock` helper + the stored-families text-form vocabulary) |
| Remote identities are Deck-shaped | domain `ids.rs` | `RemoteCardRef` :246 (+ the Remote* family); `SyncReport::Completed` carries the Deck-typed board snapshot; ADR 0010 anticipated the port promotion |
| The outbox is the board-routing fabric | domain `outbox.rs`, sync `push.rs` | board-attributed `LocalOp`s; `plan_pushes` materializes (`CardShape`-style) and routes per connector; the Deck connector's push pipeline is the regression-critical neighbour |
| The sidecar is per-board and Deck-attached | domain `appdata.rs` | anchor card + per-board sections (`tasks`, `reminder-rules`, `dismissals`, `time-entries`, break rules, habit facts); the Deck-capabilities dual-lane precedent (scheduled: native vs sidecar by flag) is the field-routing model chunk 5 generalizes |
| Partition routing exists as the registry's shape | app `connectors.rs`/`bootstrap.rs` | the multi-board registry: N connector actors, per-board assignment, fan-in reports; two connectors claiming one board is the invariant chunk 3 makes typed-and-loud |
| Counters | `docs/adr/`, storage `migrations/` | next free **ADR 0022** (0021 = reflection family); next free migration **0023** (0022 = reflection entries); implementing chunks re-verify at base |
| The org fence | user ruling Q1 | the dedicated far-future org pass, as a separate sync connection — this round links #184 as the fence's home and touches nothing org |

**Standing fences (binding, not re-litigated)**: connector coexistence
on one board (options report §4.B — partition-only); notify_push; the
LAN/folder-client transports; per-user attribution (#494 — F1 gives
it the seam, the feature stays Wave 4); org markup fields and the org
connector (#184 → the far-future pass); B-min #632's scope.

## 2. Scope & non-goals

**In scope (design only):** F1 — author-tagged clocks on every roaming
family + the total order + neutral Deck-lane carriage (Part A); the
settings/proofs per-user slice (Part B); remote-identity widening +
partition routing (Part C); the chunk/snapshot lifecycle for board
subtrees (Part D); boards riding files — the sync cycle, outbox
routing, native app-only data, board-level sections (Part E); the
Deck→files move ceremony (Part F); decision record §4; test hooks §5;
risks §6; the chunk mapping §7.

**Non-goals:** org anything (Q1's fence); coexistence; reverse moves;
attribution; encryption; notify_push; retiring the Deck connector
(Deck boards are unaffected and first-class); UI beyond the existing
status/connection surfaces' growth; statistics.

## 3. Design

### Part A — F1: author-tagged clocks (D-BF1–D-BF2)

- **D-BF1 — parallel author structs, not packed fields.** Each clock
  family gains a sibling (`TaskAuthors`, `StackAuthors`,
  `LabelAuthors`, `BoardAuthors`) with one `Option<AuthorId>` slot per
  clock, mirroring the clocks' shape. Rationale: the clock structs are
  wire/storage/exhaustive-match load-bearing in dozens of sites — a
  parallel struct keeps every existing match arm compiling, migrates
  additively, and reads as exactly what it is (who last wrote each
  field). `AuthorId` is the substrate's device identity
  (`device.json`'s `device_id` — the `ids.rs` type already exported by
  the core crate); the planner stamps `Some(author)` whenever a
  device identity exists (headless tests use `None` — the fold's
  `None < Some` order keeps those rows honest).
- **D-BF2 — the total order is `(clock, author)` per field.** The
  entity merge (`merge_task` and siblings) resolves duels by
  `(clock, author)` with the entity id as the never-reached residual;
  equal clocks with different authors now converge deterministically
  everywhere — the convergence bug F1 exists to kill. **Storage**: the
  clock text forms extend additively (an optional author suffix on
  the stored form; tolerant parse — old rows decode authorless), one
  migration **0023** adds the `au_` columns the codec backfills as
  absent. **Deck lanes carry authors neutrally**: the sidecar rows
  gain an optional `by` (tolerant, default-absent — old devices
  relay verbatim); the Deck API ignores what it cannot carry.
  Proptest: the two-equal-clocks convergence law that motivated F1
  (two devices, skewed-equal stamps, different edits → one winner on
  every reader), plus the existing duel suite extended with author
  legs.

### Part B — The settings/proofs slice (D-BF3)

The per-user root's reserved sections activate on the current-state
documents: **`settings`** — the roaming subset of user preferences
(the classification is the chunk's stage-1 "design afternoon": which
fields roam vs stay device-local, decided against the D5 notes;
day_start_time and the like roam, machine paths and keyring-adjacent
state never do); **`proofs`** — reminder delivery proofs, riding as
set-union rows (they were designed merge-friendly for exactly this;
the row key is `(rule, occurrence)` and the section is per-user —
per-device facts union to "someone dismissed this"). Engine ingest:
the `UserDataReport` fold grows the two section arms; the
reminder-state read side consumes proofs as it consumes local ones.
No board machinery — the slice is deliberately parallel-safe with
Part A (disjoint files).

### Part C — Remote-identity widening + partition routing (D-BF4–D-BF5)

- **D-BF4 — connector-tagged remote identities.** The `Remote*` id
  family widens to a tagged shape (`Deck(...)` | `Files(...)`), the
  `SyncReport::Completed` snapshot payload becomes backend-tagged,
  and the validator/binding machinery follows the tag — the ADR 0010
  "promote to the honest vocabulary" moment (Q5). **The payoff that
  pays for the refactor**: a files-board's remote identities *are*
  the domain UUIDs — no card-id bindings, no adoption ceremonies, no
  binding tables for files-boards; the Deck ceremony machinery stays
  Deck-tagged and untouched.
- **D-BF5 — partition routing, typed and loud.** The registry's
  board→connector assignment becomes an explicit typed map: each
  board is owned by exactly one connector (`Deck(connection)` |
  `Files`); a second claim is a typed refusal surfaced loudly (the
  invariant is an engine/bootstrap-level check, not a convention).
  Mixed fleets are the norm: some boards Deck, some files, one
  account, per the partition ruling.

### Part D — The chunk/snapshot lifecycle (D-BF6–D-BF7)

- **D-BF6 — board subtrees; the reserved kinds activate.**

  ```
  taskboard/
  ├── identity.json                    # per-user root (B-min, unchanged)
  ├── devices/<id>/{info,appdata}.json # per-user current-state (unchanged, Q6)
  └── boards/
      └── <board-uuid>/
          ├── identity.json            # the board's own folder identity
          └── devices/<device-id>/
              ├── info.json            # heartbeat + last_seq (per board)
              ├── ops/000123.json      # immutable chunks (Q6)
              └── snapshot-00128.json  # the owner's converged fold ≤ 128
  ```

  Chunks carry **materialized entity states** — fields + clocks +
  authors per entity (the backbone §5.5 payload; the `CardShape`
  materialization precedent), never raw op references. Each board's
  lifecycle is independent (own seq space, own snapshots, own GC) —
  membership-free, like the root.
- **D-BF7 — snapshots, GC, late-join.** Snapshot on boot + every N
  chunks (N a named constant); the owner may delete its own chunks ≤
  a published snapshot's seq; **tombstones are forever** (board data
  has delete duels — the resurrection defense is the tombstone, the
  backbone's §7.5 scenarios); late-join = snapshot floor + subsequent
  chunks (the snapshot-floor equivalence law is proptested); device
  retirement reuses the DR-8 runbook. The mixed-kind fold (root
  current-state + board chunks) is what the tolerant core already
  practises — a reader that understands both kinds per subtree, never
  mixed within one.

### Part E — Boards riding files (D-BF8–D-BF10)

- **D-BF8 — the outbox is the routing fabric.** `plan_pushes` routes
  each board's ops to its owning connector; the files connector
  drains its boards' dirty entities into a chunk (materialized at
  drain), reports `PushOutcome::Applied` per op, and the existing
  Deck pipeline is untouched for Deck boards (regression-pinned).
- **D-BF9 — files-board sync reports.** The cycle pulls peers'
  snapshots+chunks, folds the board state, and reports
  `SyncReport::Completed` with the Files-tagged snapshot; the engine
  folds it through the existing per-field LWW pipeline (Part A's
  order) — the merge machinery is reused, not forked. Board-level
  sections (time entries, break rules, habit facts, reminder rules,
  dismissals) ride the same board documents as sections; **app-only
  task fields are ordinary chunk fields** on files-boards (both
  read-sides are ours — the field-routing model collapses to one
  lane).
- **D-BF10 — no sidecar on files-boards.** No anchor card, no
  attachment, no `UpdateAppData` for files-boards — the op routes to
  the files connector as ordinary dirty-entity materialization
  (D-BF8). Deck boards keep the sidecar verbatim. Standalone boards
  may opt into files roaming with no Deck at all — a genuinely new
  capability the widening delivers for free.

### Part F — The Deck→files move ceremony (D-BF11)

A guided verb (`boards move --to-files <board>`), one ceremony, four
steps, each typed and resumable: (1) final Deck pull + quiescent
outbox for the board; (2) publish the board subtree (the moving
device's first snapshot = the converged state); (3) retire the Deck
binding (loud, typed, undoable-before-commit); (4) assign `Files` in
the routing map. Other devices adopt on their next cycle (fold-first,
the journal precedent). A device offline through the move finds the
retired Deck binding typed-and-loud on return and adopts. The reverse
direction is fenced (Q4). Mixed-fleet documentation rides the
ceremony's PR.

### Part G — Status & exit (D-BF12)

Board-cycle status joins the per-user findings on #632's vocabulary
(freshness, divergence, fork) — one more lane, no new taxonomy. The
wave exits when boards-over-files is live end to end, the ceremony is
documented, and the exit report records the partition model's
operational story.

## 4. Decision summary

| ID | Question | Decision | Status |
|---|---|---|---|
| D-BF1 | F1 shape | Parallel author structs per clock family (`TaskAuthors`…), `Option<AuthorId>`, planner-stamped; `None` in headless tests | Q2, ruled |
| D-BF2 | Order + storage | Duel order `(clock, author)`; clock text forms extend additively (tolerant parse); migration 0023 `au_` columns; sidecar rows gain optional `by`; the equal-clocks convergence law proptested | recorded |
| D-BF3 | Settings/proofs | Root sections activate: settings (roam-set classified at stage-1), proofs (set-union rows); `UserDataReport` fold grows the arms | Q3, ruled |
| D-BF4 | Remote identities | Connector-tagged `Remote*` family + backend-tagged report payloads; files remote ids ARE the domain UUIDs (no bindings/ceremonies for files-boards) | Q5, ruled |
| D-BF5 | Routing | Typed board→connector ownership map; second claim = typed loud refusal; mixed fleets normal | recorded |
| D-BF6 | Board payload | `boards/<uuid>/` subtrees; chunks + snapshots (the reserved kinds); materialized-entity payloads | Q1/Q6, ruled |
| D-BF7 | Lifecycle | Snapshot on boot + every N; own-chunk GC ≤ snapshot; tombstones forever; late-join = snapshot floor; per-board independence | Q6, ruled |
| D-BF8 | Push routing | Outbox routes by assignment; files connector materializes chunks at drain; Deck pipeline regression-pinned | recorded |
| D-BF9 | Reports + sections | Files-tagged `SyncReport::Completed`; the existing merge pipeline folds it; board sections ride the board documents; app-only fields are ordinary chunk fields | recorded |
| D-BF10 | Sidecar | None on files-boards (no anchor card, no `UpdateAppData`); Deck boards unchanged; standalone boards may opt in | recorded |
| D-BF11 | Move ceremony | v1: final pull → publish subtree → retire Deck binding (loud, typed) → assign; fold-first adoption; reverse fenced | Q4, ruled |
| D-BF12 | Status/exit | Board lanes on #632's findings vocabulary; exit = live end-to-end + documented ceremony | recorded |
| D-BF13 | ADR | **ADR 0022** (chunk 1): the boards-over-files contract — partition, chunk lifecycle, F1 order, the widened vocabulary | recorded |

## 5. Test-strategy hooks

- **F1 (chunk 1)**: the equal-clocks-different-authors convergence
  law across arbitrary document/overlay pairs; duel-suite extension;
  storage roundtrips authorless→authored; sidecar `by` relayed
  verbatim by an old-reader simulation.
- **Settings/proofs (chunk 2)**: section ingest journeys; set-union
  proofs under duplicate delivery; the roam-set classification pinned
  as data (a test asserting the classified list, not prose).
- **Widening (chunk 3)**: the second-claim refusal journey; a Deck
  board's full sync unchanged (golden regression vs the pre-widening
  pipeline).
- **Lifecycle (chunk 4)**: snapshot-floor equivalence (late-join fold
  == incremental fold) proptested; GC never removes live data;
  tombstone-resurrection scenarios (the backbone §7.5 set); chunk
  fuzz (the parser trigger).
- **Boards + ceremony (chunks 5–6)**: two-engine board convergence
  over the fake transport; a Deck-tier + files-tier **dual-tier
  move-ceremony journey** (real Deck container + the docker files
  tier); the offline-during-move adoption; partition invariants.
- All tiers per `docs/testing_strategy.org`; live Tier-3 re-pins ride
  the `.env` convention.

## 6. Risks & mitigations

- **F1's migration surface** (storage forms, sidecar rows, snapshots):
  every decode site tolerant, one migration, the equal-clocks law as
  the tripwire; staged before any board rides (chunk order enforces
  it).
- **Chunk 5 is L** (the wave's largest): split acceptance per Part E
  sub-part if stage 1 says so; the Deck pipeline's regression pins
  are the safety net while its neighbour changes.
- **Two protocol kinds, one core** (root current-state + board
  chunks): the tolerance is designed-in (DR-8's reserved kinds); the
  fold's kind-dispatch is pinned per subtree, never mixed.
- **The move ceremony's edge cases** (mid-move offline, split-brain
  pull): fold-first adoption + the loud retired-binding typing cover
  the known ones; the ceremony journey runs the dual tier before
  ship.
- **Parallel streams**: v0.2's #623/#624 finish independently (ui/
  app-export territory); #632 precedes chunks 4–6 (status growth);
  GUI-round-5 leftovers are disjoint.

## 7. Chunk mapping → the issues this round opens on acceptance

| Chunk | Scope | Size | Issue |
|---|---|---|---|
| 1 — F1 author clocks | D-BF1–D-BF2 (+ADR 0022): parallel author structs, order, text forms + migration 0023, sidecar `by`, convergence laws | M/L | `backlog: DR-9 chunk 1 — F1 author-tagged clocks (order, storage, sidecar carriage)` |
| 2 — settings/proofs slice | D-BF3: roam-set classification, the two root sections, ingest arms | M | `backlog: DR-9 chunk 2 — settings & proofs per-user sections (D5 closes)` |
| 3 — identity widening + routing | D-BF4–D-BF5: tagged Remote family, tagged report payloads, typed ownership map + refusal | M/L | `backlog: DR-9 chunk 3 — connector-tagged remote identities & partition routing` |
| 4 — chunk/snapshot lifecycle | D-BF6–D-BF7: board subtrees, chunk emission, snapshots, GC, late-join (core crate) | M/L | `backlog: DR-9 chunk 4 — the board chunk & snapshot lifecycle` |
| 5 — boards riding files | D-BF8–D-BF10: outbox routing, files-board cycles + reports, native app-data, no-sidecar | L | `backlog: DR-9 chunk 5 — boards over files (routing, cycles, native app-data)` |
| 6 — the move ceremony | D-BF11–D-BF12: the guided move, retirement typing, adoption, mixed-fleet docs, wave exit | M | `backlog: DR-9 chunk 6 — the Deck→files move ceremony & mixed-fleet exit` |

Sequencing: **1 ∥ 2 → 3 → 4 → 5 → 6** (3 shares domain files with 1;
4 consumes 1's authors; 5 needs 3+4; 6 last). All chunks follow
**#632** (chunks 1–2 may start in parallel with it — disjoint
territory; 4–6 wait for its status vocabulary).

## 8. References

- Issue #678 (the round; the Q1–Q6 rulings in its comments); milestone
  `nextcloud-files-sync`; the DR-8 round + dispatch (the substrate
  this builds on); #632 (the preceding hardening chunk).
- The backbone report §5.5/§7.3/§7.4; the options report §4.B/§9; the
  characterization report (the transport facts the board cycle
  inherits).
- In-tree: `sync-files`/`sync-files-nextcloud` (the extension
  points), `entities.rs` clock families, `ids.rs` Remote family,
  `outbox.rs`/`push.rs` (routing), `appdata.rs` (the sidecar that
  files-boards drop), ADRs 0004/0006/0007/0010/0020/0021.
- The org fence's future home: #184 (the far-future separate
  connection, per Q1).

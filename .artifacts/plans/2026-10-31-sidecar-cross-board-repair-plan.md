# [Plan] Chunk 4.3 — Sidecar cross-board repair (#350)

- **Date**: 2026-10-31 (dispatch-scheduled name; written 2026-11-02)
- **Chunk**: 4.3 of backlog round 2 (dispatch plan:
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` §6
  "Chunk 4.3 — sidecar cross-board repair (#350)"; size M; same-crate
  gate 1.1 → 2.3 → 3.2 → **4.3**)
- **Branch**: `fix/sidecar-cross-board-repair`, cut off `origin/main`
  @ `9f3508ee` (local `main` == `origin/main` at planning time).
  Worktree:
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-sidecar-cross-board-repair`
  (beside the real checkout — `/home/viktoria/src/taskboard` is a
  symlink to `/home/viktoria/nextcloud/viktoria/programming/taskboard`;
  created after `scripts/worktree-doctor.sh` passed — 224 G free,
  floor 25 G; upstream tracking set to `origin/main` by
  `git worktree add -b`, unset before any push).
- **Gate**: chunk 3.2 merged. The dispatch's "#401" is the backlog
  *issue* identifying chunk 3.2 (PRs and issues share GitHub's
  numbering; there is no PR #401). The implementing PR is **#584**
  (`feat/appdata-unreadable-split`), MERGED 2026-11-02T16:58Z; verified
  by merge-commit ancestry: `aa9d415b` ∈ `origin/main`. Gate satisfied.
- **Covers** (normative): GitHub issue **#350** ("app-data sidecar
  repair for cross-board card moves") — the mechanism, the proposed
  approach (sibling-document lookup, migrate with clocks unchanged,
  origin GC drops the old entry), and the partial-coverage honesty
  (single-connector visibility only). Design context: sidecar design
  plan `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`
  §3.8 **D-S8** (per-board document lanes; the accepted limitation this
  chunk repairs) and D-S2/D-S4/D-S7 where the fold/GC/fence machinery
  is defined. Lookup-key soundness: deck-roundtrip strategy report
  `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  §3.2 (attachment storage layout `deck/file-card-{cardId}/{filename}`
  is keyed by card id alone — card ids come from one server-wide
  sequence).
- **Status**: planning complete; every open question resolved in §4
  (D1–D15). Implementation not started.
- **Acceptance** (dispatch): the cross-board move preserves sidecar
  fields when both boards share a connector (integration-pinned at the
  fold boundary); the partial-coverage boundary documented (one
  paragraph); failure isolation pinned; suite green.
- **Hard rules** (from the dispatch prompts + issue):
  1. **Failure isolation is total** — no lookup failure, missing
     sibling, or unreadable document may block or abort the adoption
     path or surface as a sync failure; degradation means *today's
     behavior* (adoption proceeds without the entry).
  2. **Embedded clocks migrate UNCHANGED** — the migrated entry is a
     structural clone of the origin's; a reset clock would re-open
     LWW races. The pin proves clock equality, not "looks migrated".
  3. **Idempotence under repetition** — a second sync cycle after the
     migration must find *nothing* to migrate; a re-migration would
     corrupt clocks or duplicate entries.
  4. **No speculative generality** — the document source is the
     in-memory cache (per D-S4 item 3's ruling) plus the GC-eviction
     reservoir this plan records (D10/D11); no on-demand sibling
     downloads, no persisted state, no config.

---

## 1. Method

Every seam was read at the branch tip (`9f3508ee`). The chunk is M but
its surface is narrow: one new sync-crate module (the lookup + delta
predicate), three small edit clusters in the sync crate's
`actor.rs`/`flush.rs`, **one** deliberately recorded domain change
(the rules-fold re-point, D14), and the docs. The plan's job is to pin
the ordering property the review prompt names — *copy-then-GC* —
because the naive design (migrate from the sibling document cache)
silently loses the source to the origin board's own flush GC in an
ordinary usage pattern (§4.4), and that failure would be permanent and
invisible.

## 2. The defect, precisely

Card X moves from board A to board B (same server). On every device:

1. Board A's cycle pulls a snapshot that no longer lists X. R3
   (`tombstone_task`, domain `merge.rs`) tombstones the local task
   bound to `(A, X)` and clears its remote ref. X's app-data entry
   stays in **A's document** until A's next flush runs
   `prune_absent_cards` (domain `appdata.rs:1220`) — X is absent from
   A's snapshot, so the GC drops the entry from the uploaded document.
2. Board B's cycle pulls a snapshot that lists X. §5 of
   `apply_sync_report` (domain `pipeline.rs:938`) adopts it as a
   **fresh local task** — native fields ride the card entity, but the
   sidecar-carried state does not exist anywhere on B: B's document
   has no entry for X, and A's entry (if not yet GC'd) is invisible to
   B.
3. The §5b sidecar fold (`pipeline.rs:965-1005`) iterates the
   snapshot's cards and resolves each through `task_by_board_card`
   (`pipeline.rs:500`, the post-echo map — a card adopted in the *same*
   cycle is deliberately not in it) against **B's evidence document
   only**; an unresolvable card or an absent entry never folds. Even
   after the repair lands the entry in B's document, the fold applies
   it on the **next** cycle, once the card is bound — the ordinary
   echo cadence, one cycle behind adoption.

So the app-field family, the reminder rules, and the synced
dismissals die with the move; time-entry history stays on A's
document (its section is board-level and never card-pruned — D13
below). Issue #350 repairs the transport lane; local convergence then
rides the existing fold.

## 3. Seam map (what this chunk touches)

| # | Site | Coupling | Change |
|---|---|---|---|
| S1 | `crates/taskboard-sync-nextcloud/src/actor.rs:194` — `LoopState.documents: BTreeMap<RemoteBoardId, CachedAppData>` (in-memory, per-board, last successful ingest; retain prunes boards that left the derived set at `:453`) | the live cache — migration source #1 | read-only consumption + one new sibling map beside it (S2) |
| S2 | `actor.rs` `LoopState` (new field) — `evicted: BTreeMap<RemoteBoardId, AppDataSections>` | the GC-eviction reservoir — migration source #2 (D10/D11) | new; fed from the flush verdict (S5); consumed on migration |
| S3 | `actor.rs:600-663` — the cycle's sidecar block: `sidecar_evidence` → `has_flush_work` → `attempt` gate → `run_flush` | the wiring point | compute the migration fragment between evidence and the work gate; pass it to both |
| S4 | `actor.rs:686-717` — the flush verdict arms (`Applied` inserts the cache at `:697`) | the reservoir feed + consume point | insert evicted into `st.evicted[target]`; consume migrated keys from the origins' reservoirs on success |
| S5 | `crates/taskboard-sync-nextcloud/src/flush.rs:48-66` — `FlushVerdict::{Applied, AppliedUncached}` | the verdict shape | grow an `evicted: AppDataSections` payload (the pre-prune card-keyed entries the snapshot-absence GC dropped this flush) |
| S6 | `flush.rs:159-202` — `has_flush_work` (group queued OR rebuild trigger) | the work gate | third clause: a non-empty migration fragment is work |
| S7 | `flush.rs:214-235` — `run_flush`'s base match (evidence → base; the residual `Unreadable(_) => Idle` fence) | the flush fences | untouched; the migration fragment computation respects the same evidence classes (D5) |
| S8 | `flush.rs:237-375` — the overlay assembly + `:377-393` union/GC chain | the write path | union the migration fragment under the local overlay (`overlay = union(migration_doc, local_overlay)`), then the existing `union(base, overlay)` + lane GCs + `prune_absent_cards` unchanged |
| S9 | `crates/taskboard-sync-nextcloud/src/migrate.rs` (new module) | the lookup + delta predicate | pure functions over domain types; no new domain surface |
| S10 | `crates/taskboard-domain/src/merge.rs:742-812` — `fold_reminder_rules`, the `Some(local) if *clock >= local.updated_at` update arm | the fold | **the one recorded domain change**: re-point `local.task` to the folding card's task when the document wins (D14) |
| S11 | `crates/taskboard-domain/src/appdata.rs:1220-1247` — `prune_absent_cards` | the origin's drop side | **unchanged** — semantics untouched; the reservoir (S2) captures what it drops |
| S12 | `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` §3.8 D-S8 (the accepted-limitation bullet) + `docs/architecture.org:184` ("Accepted limitation: … issue #350 tracks the repair") | the docs | the coverage-boundary paragraph lands here (D15) |

Not touched: `SyncReport`/`AppDataEvidence` shapes, storage/codec,
`SidecarHealth` (the repair is silent transport work — no new health
state), the anchor ceremony, the upload ladder, the self-clobber
guard, the UI.

## 4. The design, point by point

### 4.1 The lookup mechanics

**D1 — Site: actor-side, in the destination board's flush.**
The migration is *transport work*: entries move between documents, and
documents are the actor's to write. The engine never sees sibling
documents (the report carries exactly one board's evidence — D-S4's
shape stands), so the engine cannot perform the lookup. The fragment
is computed once per destination cycle in `cycle` (S3), after
`sidecar_evidence` produced the evidence and before the work gate;
`has_flush_work` and `run_flush` both consume it.

**D2 — When during adoption: concurrent with it, at the transport
layer, never gated by it.** The trigger predicate keys on the
**snapshot's card set** (`present_cards`), not on engine adoption
state. The same cycle whose snapshot first lists X also migrates X's
entry into B's document (the flush runs between pull and report). The
fold applies the entry to the freshly adopted task **one cycle later**
(S2 §2 step 3): cycle N writes the document, cycle N+1's evidence
carries it, the card is bound by then, the ordinary echo fold lands
the fields. This decoupling is deliberate: deferred adoptions (stack
not yet bound, duplicate suppression) must not gate the transport
repair — when the card finally binds, the entry is already waiting.

**D3 — Cacheability: cache-only, plus the reservoir; zero marginal
HTTP.** Enumerating "sibling-board documents the same connector owns"
costs an in-memory iteration over `LoopState.documents` (minus the
target) and `LoopState.evicted` — every owned board's cycle ingests
its own document into the cache each pass, so after one full pass the
actor holds every sibling's last content. The alternatives were
rejected: on-demand sibling downloads would require the sibling's
anchor discovery + snapshot (extra reads every cycle, duplicating the
pull machinery, against D-S8's zero-cost laziness), and persisted
caches re-litigate D-S4 item 3's memory-only ruling. Recorded
consequence (pinned, T17): a destination cycle that runs *before* its
siblings' first ingest of the process (cold cache — fresh actor, or
destination cycling first in pass 1) migrates nothing and converges on
the next full pass. Within a pass, board-id order decides whether the
source is warm; either order converges by pass 2.

**D4 — Lookup-key soundness.** The lookup keys entries by **bare
remote card id across boards**. That is sound because Deck card ids
come from one server-wide sequence: the roundtrip §3.2 measured the
attachment storage layout as `deck/file-card-{cardId}/{filename}` —
keyed by card id *alone*, with no board component. The local ref shape
`(board, card)` (pipeline §5's identity comment) is a scoping
convention, not a uniqueness statement. Two connectors can mint
overlapping id spaces (pipeline §2's N ≥ 2 comment), so the sibling
enumeration is scoped to boards tagged with **this cycle's connector
id** — one id space by construction (`LoopState.documents`/
`st.evicted` only ever hold boards this actor owns). Cross-server
moves have no sibling to consult and are never repaired — the issue's
partial coverage, kept honest (D15).

### 4.2 Failure isolation

**D5 — Destination document unreadable ⇒ the existing fence swallows
the migration too.** The fragment predicate runs only on evidence the
flush may write against — `Document` (base = that document) and
`Absent`/`ForeignBoard` (base = the empty rebuild document). Every
`Unreadable` form suppresses the fragment (empty), so
`has_flush_work` fires only for the already-attempted classes, and
`run_flush`'s residual `Unreadable(_) => Idle` arm (flush.rs:226)
stays the structural no-write fence. Rationale: migrating into a
document we could not read is the blind-overwrite shape D-S7 forbids.
Next cycle retries honestly. Pinned (T16).

**D6 — Sibling missing/unreadable ⇒ degrade to today's behavior.**
"Missing" has exactly two forms under D3's cache-only rule: the
sibling board has no document yet (no anchor anywhere — nothing to
migrate), or its document was never ingested this process (cold cache
— D3's consequence). Both collapse to "the fragment contributes
nothing for that board"; adoption proceeds without the entry, exactly
today's behavior; a later pass repairs when the source warms (T15,
T17). There is no third form: enumeration is pure in-memory map
iteration — infallible by construction, so no *enumeration error* can
exist to surface as a sync failure. The reservoir is likewise
in-memory (D11). No code path introduced by this chunk can fail a
cycle; the only new fallible-looking operation (none) is deliberately
not introduced.

### 4.3 The migration write

**D7 — Composition: the fragment unions *under* the local overlay.**
`migrate::fragment` returns an `AppDataSections` carrying only the
three card-keyed sections (D13). The actor wraps it in a document
envelope stamped `(APPDATA_FORMAT, target)`; `run_flush` computes

```text
overlay = union(migration_doc, local_overlay)   // fragment as base,
                                                // local truth as overlay
merged  = union(base, overlay)                  // the existing step
```

so per-entry clock comparisons keep their existing semantics: equal
clocks resolve to the overlay's side at every level, and the inner
union's overlay is the *local* overlay — local truth wins any collision
of the same card key (the migrated content fills only what local truth
does not already carry). Everything downstream — lane GCs, the
snapshot-absence GC, the upload ladder, the cache refresh — is
untouched. A migrated card that is *also* a local scoped task (only
possible on a move-back plus local edits) keeps the local entry in the
overlay slot; the union's per-field LWW against the destination base
resolves the rest.

**D8 — Byte-for-byte content, clocks unchanged.** The fragment's
entries are structural clones of the origin document's entries —
`TaskEntry`/`RulesEntry`/`DismissalsEntry` as decoded, including
`unmodeled` verbatim content and rule tombstones. No clock is re-stamped,
no value is re-encoded, nothing passes through a local-truth builder
(`task_entry_for` & co. are *not* involved for migrated keys). On the
wire this is JSON *value* equality (the document's own verbatim rule —
re-serialization normalizes formatting); the pin (T12) compares the
uploaded destination document's migrated entry against the origin's
entry by serde value equality, which subsumes clock equality. The
`lane_gc_scheduled` note: on a native-`startdate` connector the lane
GC strips migrated `scheduled` keys from the union exactly as it
strips local ones — correct (the field has a native lane there; D-V2).

**D9 — The predicate is *strictly-newer*, deliberately narrower than
the union's tie rule.** The trigger includes a sibling entry only when
the destination base lacks the card key, or the sibling content is
**strictly newer** than the base's:

- `tasks`: any modeled field with `sibling.clock > base.clock`, or any
  unmodeled key the base entry lacks (additive content has no clocks —
  presence is the comparison);
- `reminder_rules`: per rule row, `sibling.clock > base.clock` (both
  clocked), or the base row is `Verbatim` and the sibling's is not
  (a verbatim base is never displaced — matching
  `union_rules_entry`), or the base lacks the rule key; a `Verbatim`
  sibling never triggers (it can never displace anything);
- `dismissals`: per row, the base lacks the key (append-only set
  union; present rows never retrigger).

Equal clocks never trigger — although `union_task_entry` would let an
equal-clock overlay win, triggering on equality would re-fire every
cycle forever (the post-migration echo holds equal clocks), which is
exactly the non-idempotence the hard rules forbid. This is a recorded
divergence: the *migration trigger* is additive-strictly-newer; the
*flush union* keeps its tie-to-overlay semantics for the writes it
does perform. Pinned (T3, T4, T5).

### 4.4 The GC interplay: copy-then-GC, and the eviction reservoir

**D10 — The hazard.** The naive source (live cache only) dies to the
origin board's own flush. Sequence: X moves A→B; the user edits any
app field on a *remaining* A-task in the same session (ordinary
reorganization); A's next cycle flushes for that op —
`prune_absent_cards` drops X's entry from the uploaded document *and*
the `Applied` verdict replaces `st.documents[A]` with the pruned
document (actor.rs:697-704). When B's cycle then looks for X's entry,
the source is gone — the repair silently never happens, which is the
exact bug class this chunk exists to fix, reintroduced through the GC.
The window is not a race: it is the common ordering (an origin flush
between the move and the destination's migration cycle).

**D11 — The reservoir.** The origin's GC becomes the reservoir's
feeder instead of the repair's enemy: `run_flush` computes the
card-keyed entries the prune dropped (pre-prune union keys minus
`present_cards`, three sections) and returns them on every successful
write verdict (`evicted` on `Applied`/`AppliedUncached`); the actor
extends `st.evicted[target]`. The migration lookup consults, per
sibling board, the live cache first and the reservoir only where the
cache lacks the card (post-GC the cache and the reservoir are
disjoint per card in practice; the order is still pinned, T8).
Properties:

- **GC semantics untouched** (S11): entries leave the wire document
  exactly as before; the reservoir is private migration fuel — never
  uploaded, never folded, invisible outside the lookup.
- **Consumed on success**: when a flush carrying migrated entries
  lands (`Applied`/`AppliedUncached`), the actor removes the migrated
  card keys from the *origins'* reservoirs. On `Idle` (transport
  failure) nothing is consumed — the next cycle retries.
- **Inert entries are harmless**: a reservoir entry is consulted only
  for cards in a *destination snapshot*. A card that was deleted (not
  moved) is in no snapshot — its reservoir entry lingers unread. A
  card that moved back to its origin is excluded from every sibling
  snapshot (a card lives on exactly one board) — its old origin
  reservoir entry is likewise unread. Growth is bounded by
  cross-board moves + deletions per process lifetime (hundreds of
  small entries at kiosk scale); process restart reframes it.
- **Chained moves work**: X moves A→B, later B→C — B's GC evicts X's
  entry into B's reservoir; C's lookup consumes it.

**D12 — No double-migration.** The trigger's predicate is
destination-absence/strictly-newer (D9): after the migration lands,
the destination base holds the entries at the migrated clocks, so the
next cycle's fragment is empty regardless of what the origin's cache
or reservoir still holds (consume-on-success clears the reservoir
anyway). The origin's eventual GC drop is *not* required for
idempotence — it is hygiene: until A's next flush, A's document
carries a stale entry for a card absent from A's snapshot, which is
inert on every device (absence ≠ deletion; the fold never resolves it;
D-S1's "card ids never return" argument covers the move-out case, and
a move-back is a fresh migration by clocks, D9). Pinned (T13).

### 4.5 Scope: which sections migrate

**D13 — The three card-keyed sections; the board-level sections never
move.** `tasks`, `reminder_rules`, `dismissals` migrate — they are
keyed by decimal remote card id and pruned by absent-card GC, so the
move breaks exactly them. `time_entries`, `habits`, `habit_log` are
board-level by design (D-T10/DR-6 §4.5: history and definitions
outlive cards; `prune_absent_cards` never sees them) — a moved card's
time entries stay on the origin board's document, remain
unresolvable-but-preserved there, and age out under the 180-day wire
retention. That is a recorded residual (§7 R4), the same family as
the issue but outside its stated scope ("fields … and reminder
rules"); it is named in the coverage paragraph (D15) so the honesty
stays in one place. Unknown envelope keys and unknown *sections* of
the origin document never migrate (the union's verbatim relay is
per-document, not per-card).

### 4.6 The one domain change: the rules-fold re-point

**D14 — `fold_reminder_rules` re-points `task` when the document
wins.** Without it the repair's rules lane no-ops on the device that
performed the move — the user's own. The mover's rule UUIDs exist
locally under the old (tombstoned) task: `fold_reminder_rules`'s
update arm (`merge.rs:772-779`) matches on the UUID and updates
offset/created/insistence/`updated_at` but never touches `local.task`,
so the rule stays attached to a soft-deleted task (the reminder
scheduler will not meaningfully fire it). The document's card key
*is* the rule's remote scoping (D-S9: the fold adopts the document's
task link), so when the update arm fires (document clock ≥ local),
the local rule follows the folding card's task. The `None` arm already
adopts with the folding task (foreign devices — correct today). The
tombstone arm, the `protected` (self-clobber) early return, and the
`Verbatim` skip are untouched. A re-point fires only when the local
rule's task differs from the folding card's task — exactly the
move-repair shape (both move directions); a rule UUID cannot be
shared across tasks by construction. This is the dispatch's "no new
domain surface unless the plan recorded one" exception, recorded.
Pinned at the lowest layer (T1).

### 4.7 What does not change

`SyncReport`/`AppDataEvidence` shapes; `SidecarHealth` (silent
repair — no badge, no CLI surface); storage migrations and codec
tags; the anchor ceremony, pick rules, and upload ladder; the
self-clobber guard; the snapshot-absence GC's contract; `prune_absent_cards`'
signature; the board-level sections' emission and fold. The sync
crate's dependency boundary holds: `migrate.rs` imports domain *data
types and the existing `union`* only.

## 5. The coverage boundary paragraph — where it lands

**D15 — Two homes, both named, one paragraph of substance.** The
limitation this repair supersedes is recorded in two places today:

1. **Primary: the sidecar design plan's amendment note.** A dated
   amendment (house precedent: the plan's 2026-10-22 amendment block)
   appended to §3.8 **D-S8** — after the "Cross-board card moves lose
   app-fields" bullet — stating: repaired 2026-11 (issue #350, chunk
   4.3); the destination board's flush now migrates the card-keyed
   sections (`tasks`, `reminder-rules`, `dismissals`) from
   sibling-board documents the *same connector* owns (in-memory
   source: the last ingested document plus GC-evicted entries), clocks
   unchanged; coverage is **partial by construction** — cross-server
   moves have no visibility and are never repaired; a concurrent edit
   racing the move can still be lost within one poll interval (§7 R1);
   time-entry history does not follow the card (D13); convergence is
   one cycle behind adoption.
2. **Secondary: `docs/architecture.org`'s sidecar section.** The
   closing sentence "Accepted limitation: a card moved between boards
   loses its app-side fields (the document is per-board; issue #350
   tracks the repair)" is replaced by the one-paragraph boundary
   (same-connector repair, cross-server moves honestly out, the
   residuals above).

Rejected: a new docs page (the boundary is one paragraph; scattering
sidecar truth across documents is how the limitation got two homes in
the first place).

## 6. Test catalogue (TDD — pins first, per cluster)

Layer discipline (AGENTS §5): each behavior pinned once, at its
lowest layer; the wiremock integration pins observe the *wire and
report* surface only (assertion surface is uploaded document content,
request hits, and `SyncReport` payloads — never actor internals). The
dispatch's "integration-pinned at the fold boundary" reads as: the
transport pins prove the document content the fold will consume
(T12/T14), the domain pin proves the fold's one new behavior (T1),
and the existing fold tests prove entry-folding for bound cards —
no engine journey is duplicated hermetically (that composition is
the dockerized e2e tier's job, unchanged).

**Domain (`merge.rs` tests):**

- **T1** `a_document_wins_rule_fold_repoints_a_rule_stuck_on_the_old_task`
  — local rule under task T1, fold entry (clock ≥ `updated_at`) under
  T2 ⇒ `rule.task == T2`, payload adopted, `updated_at` max'd; the
  clock-loses arm keeps T1 (no re-point when local stands); the
  tombstone and `protected` arms unchanged.

**Sync unit — `migrate.rs` (new module's tests):**

- **T2** `the_fragment_carries_entries_whose_card_the_destination_lacks`
  — all three sections, verbatim content and tombstones included.
- **T3** `only_strictly_newer_task_fields_trigger` — equal clock
  excluded (the idempotence edge), older excluded, one newer field
  includes the entry, unmodeled-key absence triggers (T6's twin:
  `unmodeled_entry_content_migrates_when_the_destination_lacks_the_key`).
- **T4** `only_strictly_newer_rule_rows_trigger_and_verbatim_never_displaces`
  — clocked > clocked; verbatim base suppresses a clocked sibling; a
  verbatim sibling never triggers; a missing rule key triggers.
- **T5** `dismissals_are_set_union_missing_rows_migrate_present_rows_never_retrigger`.
- **T7** `entries_for_cards_absent_from_the_destination_snapshot_are_ignored`
  — the snapshot scoping (a sibling entry for a card B never adopted
  contributes nothing).
- **T8** `the_reservoir_shadows_only_where_the_live_cache_lacks_the_card`
  — cache-first order per board.

**Sync unit — `flush.rs` tests:**

- **T9** `a_nonempty_migration_fragment_is_flush_work_without_ops_or_rebuild`
  — the third clause alone wakes the gate; an empty fragment changes
  nothing (every existing gate pin must stay green untouched).
- **T10** `local_overlay_wins_the_card_slot_over_migrated_content` —
  the D7 composition's collision case through `run_flush`'s overlay
  assembly.
- **T11** `the_applied_verdict_carries_the_gc_evicted_card_entries` —
  pre-prune keys minus `present_cards`, present cards never evicted,
  tombstones included, board-level sections never reported.

**Integration — `tests/actor_wiremock.rs` (the existing two-board +
sidecar fixture modules; the pins live beside the F-series):**

- **T12** `the_cross_board_move_migrates_the_entry_between_two_owned_boards`
  — the both-boards-one-connector pin: board 42's document carries
  card 500's entry (modeled field + rule + unmodeled verbatim key),
  43's document lacks it; both boards bound to one connector, 42's
  snapshot without 500, 43's with it, 42 cycling first (cache warm);
  one nudge ⇒ 43's attachment write carries `tasks["500"]` and
  `reminder_rules["500"]` **serde-value-equal** to 42's document's
  entries (clocks byte-identical — hard rule 2's proof), 42 receives
  no write.
- **T13** `a_second_cycle_finds_nothing_to_migrate` — the idempotence
  pin, continuing T12's actor: the second nudge performs **zero**
  attachment writes (and zero downloads — the flush cache refresh fed
  the metadata cache).
- **T14** `the_gc_evicted_entry_still_migrates_from_the_reservoir` —
  the copy-then-GC pin: cycle 1 gives board 42 flush work (an
  `UpdateAppData` op on another 42-task) so its GC evicts 500's entry
  *and* the verdict replaces 42's cache; cycle 2: 43's write still
  carries the entry (reservoir-sourced); cycle 3: nothing.
- **T15** `a_missing_sibling_degrades_to_todays_adoption` — only
  board 43 bound, no sibling documents anywhere: the cycle completes
  (`Completed`, evidence unchanged), no migration write, no failure —
  degradation pin.
- **T16** `an_unreadable_destination_document_suppresses_the_migration`
  — 43's download fails (transport ⇒ `Unreadable(Transport)`): no
  flush attempt, no write, cycle still `Completed` (the D-S7 fence
  composes; migration adds no new failure surface).
- **T17** `a_cold_cache_converges_on_the_next_full_pass` — fresh
  actor, 43 cycling before 42's first ingest: pass 1 migrates nothing,
  pass 2 migrates (the D3 cacheability consequence, pinned so the
  one-pass delay is a contract, not an accident).

## 7. Risks & residuals

- **R1 — Fleet-level GC race (residual, documented in D15).** Another
  device may flush the origin document (GC'ing the entry) before this
  device's actor ever ingests it; the entry is then gone from the
  wire fleet-wide and this device's reservoir never sees it. The
  window is bounded by the poll interval and requires a concurrent
  edit+move from different devices; the failure mode equals the
  pre-repair status quo for the concurrently-edited fields. No
  protocol change is proposed (fixing it means tombstone-or-forward
  semantics in the document — a design-round question, not this
  chunk's).
- **R2 — Reservoir growth**: bounded by moves+deletions per process
  lifetime; consumed on migration; inert entries never consulted
  (D11). Accepted; kiosk-scale numbers are bytes.
- **R3 — One-cycle convergence**: fields appear on the adopted task
  one cycle after the move is observed (the echo cadence, D2).
  Accepted; the same delay already applies to every freshly created
  card's entry.
- **R4 — Time-entry history does not follow the card** (D13): rows
  stay on the origin document until the 180-day retention ages them
  out; the origin's fold cannot resolve them after the move. Named in
  the coverage paragraph; a future design question if it matters
  (board-level rows keyed by card id *could* be re-homed, but that is
  a schema decision, not a repair).
- **R5 — `has_flush_work`'s signature grows again** (already
  `#[allow(clippy::too_many_arguments)]`): the allow stands; the
  alternative (bundling the gate inputs into a struct) is a refactor
  with no behavior, deliberately not taken here.
- **R6 — Predicate/union tie divergence** (D9) is deliberate and
  pinned; the review stage's "trace the flow twice" trap is answered
  by T13 (zero writes on cycle 2) and T3 (equal clocks never
  trigger).

## 8. Estimated diff

≈ 600–700 lines, ~60% tests:

| File | Kind | ≈ lines |
|---|---|---|
| `crates/taskboard-sync-nextcloud/src/migrate.rs` | new (logic + unit tests) | 220 |
| `crates/taskboard-sync-nextcloud/src/actor.rs` | LoopState field + retain, fragment wiring, verdict arms | 60 |
| `crates/taskboard-sync-nextcloud/src/flush.rs` | verdict payload, gate clause, overlay composition, evicted delta + tests | 150 |
| `crates/taskboard-domain/src/merge.rs` | the re-point + T1 | 50 |
| `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs` | T12–T17 | 260 |
| docs (design-plan amendment, architecture.org, CHANGELOG) | D15 + one reliability entry | 40 |

No storage migration, no codec change, no report-shape change.

## 9. Stage-2 execution order

1. T1 pin-first → the re-point (S10). Commit: standalone.
2. `migrate.rs` pins (T2–T8) → the fragment + predicates (S9).
3. `flush.rs` pins (T9–T11) → gate clause, overlay composition,
   evicted delta (S5/S6/S8).
4. `actor.rs` wiring: reservoir field + retain, fragment computation,
   verdict-arm feed + consume (S2/S3/S4).
5. Integration pins T12–T17.
6. Docs: the D-S8 amendment, the architecture.org sentence, the
   CHANGELOG reliability entry (`### Fixed` under `[Unreleased]`).
7. Full gate suite via `scripts/run-quality-gates.sh` (fmt, clippy
   `-D warnings`, nextest, doc tests, deny, the llvm-cov sequence with
   quarantine + trailing rm). Coverage: every new line is exercised by
   the pins above; the quarantine step covers the wiremock binaries.
8. No push, no PR (stage 3 ships).

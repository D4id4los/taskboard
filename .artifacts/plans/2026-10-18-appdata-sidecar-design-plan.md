# [Plan] Daily-Driver Wave — App-Data Sidecar Design (round doc 1 of 5)

- **Date**: 2026-10-18
- **Kind**: design doc for the daily-driver design round, question 1
  (framing plan §5.1). Designs the app-data sidecar end to end —
  anchor ceremony, document schema, merge primitive, actor-cycle
  integration, outbox op, self-clobber mirror, failure fences,
  per-board documents across N connectors, reminder-rules section —
  against the accepted multi-board world. Each point carries
  re-verified anchors, exactly one recommendation, and a recorded
  decision; §6 is the decision record, with the sub-points left open
  for the user's session named explicitly (framing D-D6).
- **Origin**: framing plan
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`
  (§5 Q1 normative; §2 scope incl. #207 and #202-rules). Normative
  inputs: `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  (§4 recommendation and §5 merge-policy implications **BINDING**; §3
  wire evidence; §6 per-field verdicts; §7 risks) and the accepted
  multi-board design
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  with its implementation plan's design facts DF-1…DF-10
  (`2026-10-18-multi-board-wave-implementation-plan.md` §2).
- **Status**: recommendations **ACCEPTED** — the user's decision
  session (2026-10-18) accepted all of §6's recommendations and the
  rider, adding two requirements (the format-fence UI warning, recorded
  into D-S2/D-S4/D-S7; hide + CLI guard for the anchor task, §7) and
  one disposition (the cross-board move repair leaves §8 for tracked
  issue #350). Record in §6. The one tracker edit (that issue) was
  ordered by the ruling; otherwise zero production code — this file
  is the chunk's entire diff.
  **Amended 2026-10-22** (ruled): the transport is version-seamed via
  a Deck capability probe, and `scheduled`'s lane becomes dynamic
  (native when the server supports it, sidecar fallback otherwise).
  This supersedes the "the document's tasks section never gains a
  `scheduled` key" statements here (D-S2, §7 doc-2 interface) *in
  fallback mode*. Normative addendum:
  `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`
  (evidence: `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`).

---

## 1. Mission & binding constraints

Implement the D1 answer: app-only fields (and reminder rules) sync day
one through a **board-anchored attachment sidecar** — one JSON document
per board, stored as one Deck file attachment on a designated anchor
card, merged per key on document-embedded clocks, treated as transport
while local state stays truth. The transport client surface already
shipped (framing §3); this doc designs everything above it.

Binding, not re-arguable (roundtrip §4/§5; framing §5.1):

- One board-anchored document (not per-card documents) — granularity
  is settled (roundtrip §4 table).
- Document keys are `(board, remote card id)`; local `TaskId` UUIDs
  never appear in the document.
- Two disjoint clock domains: native fields merge per-field LWW on
  Deck's entity stamp; app fields merge **only** on the clocks
  embedded per key in the document, never on the card stamp (every
  attachment write advances it — measured, roundtrip §3.3).
- Absence ≠ tombstone; deletion is an explicit tombstone winning only
  by its own timestamp; **the document never feeds R3**.
- Flush is read-modify-write of the union, never a blind overwrite.
- The sidecar upload appends after the cycle's card pushes, before the
  cycle ends, such that its stamp bump lands in the *next* cycle's
  pull; ops queued after `read_at` are protected by the self-clobber
  guard.
- Failed upload keeps ops retryable; failed/partial download folds
  nothing (evidence-then-verdict).
- The v1 fence on reminder data and the native path stays intact —
  the document is a *new* lane, not a widening of the native one.

Multi-board conformance (DF-1…DF-10 binding context): the design
targets the post-multi-board world — per-board `SyncStates` (DF-9),
`ConnectorId` mint chain (DF-3), connector-owned board sets derived
from tagged bindings (DF-1/DF-3), per-board cycles with
O(boards + changed cards) cost (DF-5 #157 guardrail), the topology
invariant (DF-10). Every section below closes with its DF conformance
note.

## 2. Verified baseline (symbols are the stable keys; lines are this chunk's)

Worktree base: branch `docs/daily-driver-design-round` @ `b809809`
(= `origin/main` tip at chunk start; the delta from the framing's
`070885c` is docs-only — the multi-board implementation dispatch
plan). Locating method: `grep -n` for the symbol, then `sed -n` for
bodies.

| Seam | Symbol (file) | This chunk's lines | Notes |
|---|---|---|---|
| Attachment content surface | `DeckClient::{attachments, upload_attachment, download_attachment, update_attachment_content, delete_attachment, restore_attachment}` (sync `client.rs`) | 610 / 645 / 689 / 737 / 778 / 816 | the D1 transport, landed; `update_attachment_content` is the app-level POST route with stable-filename in-place replace |
| Multipart payload | `AttachmentUpload` + `multipart_form` (sync `client.rs`) | 216 / 231 | part names `type`=`deck_file` + `file`; update form adds `data`; filename = server-stored name; no rename |
| Wire-model fix | `ExtendedData.size` `#[serde(alias = "filesize")]` (sync `model.rs`) | 378 | the roundtrip §3.2 wire-drift item — already fixed and fixture-pinned |
| Card carries attachments | `Card.attachments: Vec<Attachment>` + `attachment_count` (sync `model.rs`) | 138 / 140 | the card *detail* read inlines full attachment metadata (roundtrip §3.2); the stacks listing does not |
| Cycle order | `cycle` (sync `actor.rs`) | 298 | `read_at` stamped at 315 *before* `read_state` (316); push 364–406 (before pull — the R3 hazard, ADR 0007); pull 408–448; report 453–461 |
| Per-card detail refresh | the bound-card loop (sync `actor.rs`) | 712–732 | unconditional `client.card` for every *bound* card each cycle; `LocalCardStamps` built at 414–421 |
| Native merge | `merge_task` (domain `merge.rs`) | 170 | per-field LWW via `remote_wins(remote.last_modified, local.clocks.X)` (`remote_wins` :71) |
| R3 / R9a | `tombstone_task` / `adopt_task_after_push` (domain `merge.rs`) | 268 / 291 | R3 fires on listing absence, observation-stamped; R9a takes echo values as-is, clocks `max(local, echo)` |
| Self-clobber guard | `PendingTaskIntent` / `pending_task_intents` / `keep_task_intent` (domain `pipeline.rs`) | 92 / 104 / 158 | protection keyed `op.queued_at > read_at`; `keep_task_intent` copies protected **values and their clocks** back |
| Engine fold order | `apply_sync_report` (domain `pipeline.rs`) | 376 | push outcomes (405) → `task_by_board_card` post-echo identity map (425) → board (431) → stacks → labels → tasks → presence reconciliation |
| Reminder fences | the two `v1:` comments (domain `pipeline.rs`) | 733–735 / 893–895 | "no reminder data enters the sync path / the push path — carried through" |
| Outbox | `LocalOp` / `PendingOp.queued_at` / module doc (domain `outbox.rs`) | 22 / 64 / :5 | id-scoped, field-less; "every Deck write is full-send anyway" |
| Push planning | `PushGroup {target, materialized, primary, subsumed}` / `plan_pushes` / `EntityTables` (domain `push.rs`) | 229 / 317 / 214 | coalescing + dependency ordering live here |
| Report shape | `SyncReport::{Completed, Failed}` (domain `messages.rs`) | 292 | `Completed {snapshot, validators, pushes, read_at}`; already per-board snapshots (DF-1) |
| Rules model | `ReminderRule` (domain `reminders.rs`) | 109 | `id`/`task`/`offset`/`created_at`; the `created_at` doc (117–119) reserves additive write clocks for rule sync — this design consumes that reservation |
| Rules state | `ReminderState` (domain `reminders.rs`) + `AppState.reminders` (domain `state.rs`) | 211 / 37 | `rules` by id; deliveries/snoozes/muted stay out of scope (D5) |
| Rule ids | `ReminderRuleId` (domain `ids.rs`) | 59 | a UUID newtype (per-device mint; §3.9 makes them document-stable) |
| Op bookkeeping | `PersistenceAction::{CompleteOp, FailOp}` (domain `persistence.rs`) | 191 area | FailOp = dead-letter; retryable = *no* action (op stays queued) |
| Validators | `ValidatorKey` (domain `persistence.rs`) | 123 | `Boards` / `Stacks(board)` / `ArchivedStacks(board)` — HTTP-header validators only |
| Storage | migrations 0001–0005; op-tag table + legacy-tag rejection (storage `codec.rs`) | — :401–409 | 0006 is multi-board's (in flight); this wave's migrations number from 0007; 0004's `reminder_rules` has no clock column yet |

## 3. The design, point by point

### 3.1 D-S1 — The anchor-card ceremony

**Anchors.** Card creation/listing client surface (`DeckClient::
create_card` / `card` / `stacks`, sync `client.rs` :984 / :564 / :522);
the pull's card population (sync `actor.rs` :669–687, active +
archived listings merged); the per-bound-card detail refresh
(:712–732); Deck card payload keys incl. `archived` and `lastModified`
(roundtrip §3.1 key list); soft-delete visibility of attachments
(roundtrip §3.2: tombstoned entries stay listed, `deletedAt` stamped;
the card's `attachmentCount` drops while the inline array keeps the
entry); the 409-duplicate rule per (card, filename) and the
no-rename constraint (roundtrip §3.2).

**Decision D-S1 (ceremony).** The anchor is a *convention-named*
card, discovered statelessly every cycle — no config entry, no
persisted anchor identity, no engine command.

- **Constants** (sync-crate `const`s, one home each):
  - Anchor card title: **`[taskboard] app-data`** (exact string;
    scan is exact-match, no normalization — deliberately unlike
    `ReconcileByTitle`'s trim+casefold, because the convention string
    is machine-owned).
  - Attachment filename: **`taskboard-appdata.json`**; MIME
    `application/json` (`AttachmentUpload`, client.rs:216).
  - Both constants are stable forever: the filename can never change
    (no rename exists, roundtrip §3.2), and a title change would
    orphan every fleet's documents at once.
- **Discovery (every cycle, per board)**: scan the pulled snapshot's
  card set (active + archived listings — actor.rs:669–687 already
  merges both) for exact title matches. Candidates are then
  *confirmed* by a card-detail read showing an attachment with the
  reserved filename (live or tombstoned — both count; the detail read
  is one `client.card` GET, and for a *bound* anchor it is the same
  GET the detail-refresh loop at :712–732 already performs — capture
  `detail.attachments` there; a dedicated read is needed only while
  the anchor card is not yet a bound local task, i.e. discovery and
  adoption cycles). **Pick rule among confirmed carriers: newest
  attachment `lastModified`, tie-break lowest remote card id** —
  deterministic across devices reading the same metadata, and
  self-healing after re-anchor races (§ loss taxonomy below).
- **Creation (lazy)**: an anchor is created only when a flush has
  document work (sidecar ops pending, or rebuild triggered) and no
  confirmed carrier exists. The actor creates it directly via
  `create_card` (title convention, first stack by `order`, last
  position) — actor-side, *not* an engine `CreateTask`: the anchor is
  connector infrastructure, and the next pull adopts it as a local
  task through the ordinary remote-card path (R6), which is also what
  makes it a bound card that the detail refresh then covers.
- **Adoption of a title match that carries no attachment**: if there
  is exactly one unconfirmed title match and no confirmed carrier,
  the ceremony **adopts it** — uploads the document attachment onto
  it — rather than creating a second card. This closes the
  interrupted-ceremony loop (create card → upload fails → next cycle
  must not create another); the accepted edge is that a user-made
  card with exactly the convention title gains one attachment. With
  multiple unconfirmed matches and no carrier, the ceremony creates
  its own card (never touches ambiguous user cards).
- **Permission**: anchor creation needs card-write permission on the
  board for the connector's API user. A read-only connector never
  creates; it still folds whatever document another device maintains
  (D-S7). No elevation, no config.
- **Loss taxonomy and recovery** (detection rides the discovery scan;
  every row recovers without data loss because local state is truth):
  - **Card deleted / moved to another board / renamed by a user /
    archived** → the scan finds no confirmed carrier (an archived card
    leaves the stacks listing — roundtrip §7; a moved card leaves
    this board's snapshot entirely) → **re-anchor**: create a fresh
    anchor card and **rebuild flush** (upload *all* local app-data
    and rules, not just dirty entries). The stale document (on the
    deleted/moved/renamed card) is inert: card ids never return, and
    the fold never reads it again. Never unarchive, never rename
    back, never delete anything — the ceremony does not fight user
    actions in Deck's UI.
  - **Attachment tombstoned on the confirmed anchor** (user deleted
    the file in Deck's UI) → **document loss with anchor intact**:
    `restore_attachment` (un-delete; content stamp preserved,
    roundtrip §3.2) then `update_attachment_content` with the rebuilt
    document. Restore-then-replace is used because create would hit
    the 409 duplicate rule against the tombstoned (card, filename)
    pair — create-against-a-tombstone is unmeasured and the design
    must not depend on it.
  - **Attachment absent on an adopted/confirmed anchor** → plain
    upload (the create path).
  - **Residual duplicate carriers** (create race between two devices,
    or a user unarchiving an old anchor later): the newest-stamp pick
    keeps every device on one carrier; the losers linger as ordinary
    cards carrying stale documents. Data is safe under either pick —
    folding an older document is a clock no-op against newer local
    state, and the next flush re-uploads the union to the winner.
    Best-effort cleanup of loser cards is deliberately **not**
    included (risk/benefit poor; backlog §8).
- **Deck-UX cost, accepted**: one card with one attachment per board
  (the roundtrip §4 criterion). The anchor card also appears as a
  local task (the pull adopts it); hiding it in the UI is a
  presentation choice for doc 4 (views), not sync state — recorded as
  a cross-doc requirement in §7.

**Bandwidth**: steady state the anchor read rides the already-paid
per-bound-card detail refresh (zero marginal GETs — better than the
roundtrip §3.4 pricing of one dedicated ~1.5 KB read); content GET
only when the metadata pair changed; content PUT only on flushes with
document work. Cold start (actor restart) re-downloads the document
once per board (§3.4 D-S4).

**DF conformance**: pure actor/client work over the existing per-board
cycle; no channel, no engine vocabulary, no topology. **Topology
untouched (DF-10).**

### 3.2 D-S2 — Document schema: format version + section registry

**Anchors.** The codec conventions (domain serde types, rfc3339
timestamps, string map keys — `ReminderSnoozeKey`'s JSON-key
serialization, reminders.rs:148–185; storage codec's tag discipline);
the D5 generalization requirement (roundtrip §4 consequences; §7 open
question "the document needs a format-version field and a section
registry from day one"); the wave scope fence (framing §2: task-kind /
respite / recurrence fields reserved but not landed; proofs/settings
are D5).

**Decision D-S2 (schema).** The document is a versioned envelope over
a section registry; everything the reader does not understand is
*preserved verbatim* on re-upload. Types live in a new domain module
(`appdata.rs`; pure serde data + the pure functions of §3.3/§3.4 —
AGENTS §2 compliant, serde/chrono only).

Envelope (illustration):

```json
{
  "format": 1,
  "board": 2,
  "sections": {
    "tasks":           { "<card-id>": TaskEntry,  ... },
    "reminder-rules":  { "<card-id>": RulesEntry, ... }
  }
}
```

- **`format`**: integer, starts at 1. Semantics: *additive changes
  never bump* — new sections, new field keys, new entry shapes inside
  a known section are all format-1 evolution (old devices preserve
  them via the rule below). A bump happens only on an incompatible
  *envelope* change (e.g. renaming `sections` or changing the entry
  clock encoding). **A device reading `format` greater than its own:
  folds nothing and flushes nothing** — its sidecar ops strand
  honestly (per-board `pending_ops` climbs; D-S7) until the device is
  upgraded. This is the forward fence that makes a future bump
  non-destructive. **Ruled addition (2026-10-18): the stranding must
  be loud.** The fence stands, but silence does not — the evidence
  carries the reason end-to-end (the typed enum of D-S4), the engine
  surfaces a per-board sidecar health from it, and the UI badge and
  the CLI `status` render it as an actionable warning: *update the
  app on this device to the version your other devices use*.
  Climbing `pending_ops` alone is not a sufficient signal.
- **`board`**: the remote board id the document belongs to. **Fold
  fence:** a document whose `board` ≠ the cycling board is ignored by
  the fold (it rode in on a moved card); the flush may replace its
  content with this board's rebuilt document (update-in-place), since
  the foreign content is stale-by-move.
- **`sections`**: string-keyed map — the registry. Day-one sections:
  `tasks` (the app-field family, one entry per card) and
  `reminder-rules` (§3.9). **Reserved, documented, not emitted**:
  `proofs` and `settings` (D5 substrate, Wave 4 — roundtrip §4/§6),
  and field-key space `task-kind`, `respite`, `recurrence` inside
  `tasks` (Waves 2–3 — framing §2). Reserving means: the names are
  documented here so no wave invents a colliding meaning; nothing
  serializes them yet.
- **`TaskEntry`** (in `tasks`, keyed by decimal card id):

  ```json
  { "fields": { "effort": { "v": 900, "clock": "…" },
                "scheduled": { "v": null, "clock": "…" } } }
  ```

  Each field key maps to `{v, clock}` — value is the domain field's
  serde encoding (shapes owned by doc 2, the field-family design),
  `clock` the per-field write clock (rfc3339). **A `null` value is a
  cleared field, not a tombstone** — clearing effort writes
  `{v: null, clock: now}`, which beats older values by clock exactly
  like any write. The tasks section therefore carries **no
  tombstones at all**.
- **`RulesEntry`** (in `reminder-rules`, keyed by decimal card id):

  ```json
  { "rules": { "<rule-uuid>": { "offset": -900, "created": "…", "clock": "…" },
               "<rule-uuid>": { "deleted": true, "clock": "…" } } }
  ```

  Rule *membership* needs explicit tombstones (`{deleted, clock}`):
  absence of a rule from a (possibly truncated) document must never
  delete it locally (§3.3). A tombstone wins against a rule whose own
  clock is older; loses against a newer re-create.
- **Unknown-content preservation (the D5 guarantee)**: on
  read-modify-write, sections, field keys, and rule entries the
  writer does not model are carried through byte-for-value verbatim —
  the union (§3.4) is *additive over everything it does not
  understand* and never drops it. This is precisely what makes the
  Wave-4 `proofs` section and the Wave-2/3 field keys non-breaking:
  a v1 device in a mixed fleet is a transparent relay for newer
  devices' sections.
- **Local materialization rule**: local state keeps only what it
  understands and can resolve (fields with slots in `TaskClocks` /
  `Task`; rules whose card resolves to a local task — §3.3).
  Unresolvable or unknown entries are *not* errors: they are dropped
  from local state and preserved in the document by the next union.
  The document is transport; local state is truth (roundtrip §4).
- **GC (bounded growth)**: the flush prunes entries (both sections,
  tombstones included) whose card id is absent from this cycle's
  *complete* snapshot — the same evidence R3 trusts (the snapshot's
  completeness contract, messages.rs `RemoteBoardSnapshot` doc).
  Entries for present cards are never pruned by absence.

**DF conformance**: pure domain data types + storage-agnostic codecs;
`AppState` grows nothing for the document itself (fields/rules land
per docs 2/3). **Engine-additive only; topology untouched.**

### 3.3 D-S3 — The merge primitive

**Anchors.** `merge_task` (domain merge.rs:170) and its per-field
clock discipline (`TaskClocks`, entities.rs:49 — the shape to mirror,
roundtrip §5); the fold position requirement (roundtrip §5: "slots
with the task merge, *after* `merge_task` and *after*
`keep_task_intent` has re-applied protected fields"); R3's input
(`tombstone_task`, merge.rs:268 — remote listing absence only);
`task_by_board_card` (pipeline.rs:425, the post-echo identity map);
`adopt_task_after_push` (merge.rs:291 — R9a's values-as-is,
clocks-max shape).

**Decision D-S3 (fold).** A new pure function, a `merge_task`
sibling — **`fold_task_app_data`** — plus a rules counterpart, both
in domain `merge.rs` (mirroring its clock discipline):

```rust
// Illustration of the contract, not a diff.
pub fn fold_task_app_data(
    merged: &mut Task,          // post merge_task + keep_task_intent
    local: &Task,               // pre-merge local truth (guard source)
    entry: &TaskEntry,          // document entry for this card
    intent: &PendingTaskIntent, // the self-clobber guard, widened (§3.6)
)
```

- **Clock rule, per field key**: document clock ≥ local clock →
  take the document value, clock = `max(local, document)`; local
  clock strictly newer → local stands. This single rule subsumes the
  echo case (roundtrip §5: after our own upload the next cycle reads
  back our union — clocks equal, values identical, idempotent) and
  the concurrent-writer case (a newer device's entry wins by its own
  embedded clock, never by card stamp). The card's `last_modified`
  is **never read** by this function (the two clock domains stay
  disjoint — measured §3.3 of the roundtrip: every sidecar write
  advances the card stamp, so it carries no per-field information).
- **Tombstones**: only the rules section has them (D-S2). A rule
  tombstone removes the local rule iff its clock > the rule's clock;
  a rule whose clock is newer stands (re-create after delete). The
  tasks section's `null` values are ordinary writes.
- **Absence**: a key absent from the entry, an entry absent for a
  card, a missing/failed document — none of these is a deletion.
  Deletion semantics exist only as explicit tombstones (rules) and
  the flush GC's snapshot-absence prune (D-S2).
- **R3 fence**: the fold contributes **no presence facts**. Presence
  reconciliation (R3) continues to run exclusively on the snapshot's
  card set; `fold_task_app_data` is *querying* `task_by_board_card`,
  never feeding it. An entry whose card resolves to no local task is
  simply not folded (preserved in the document; re-folded if the task
  is adopted later).
- **Position in `apply_sync_report`** (pipeline.rs:376): immediately
  after the task merge loop (and thus after each task's
  `merge_task` + `keep_task_intent`, satisfying the binding §5
  ordering) — one pass over the document's `tasks` section resolving
  cards through `task_by_board_card`, then one pass over
  `reminder-rules` updating `AppState.reminders` (§3.9). Entries for
  tasks the presence reconciliation is about to tombstone are folded
  first and die with the task — no ordering hazard (disjoint
  concerns).
- **Signature note**: the guard flags arrive as a parameter, and the
  guard copy for app fields happens *inside* the fold (per field:
  intent flag set → copy local value **and its clock** over the fold
  result — §3.6). This keeps the binding §5 wording exactly — "the
  intent-keep step copies the app-field values and their
  document-embedded clocks back over the fold result" — with one
  guard data structure for both families.

**DF conformance**: a pure function beside existing pure functions;
engine-internal; no vocabulary or channel changes. **Topology
untouched.**

### 3.4 D-S4 — Actor-cycle integration

**Anchors.** The cycle's verified order (sync actor.rs: `read_at`
:315 before `read_state` :316; push :364–406 before pull — the R3
hazard; pull :408–448; report :453–461); `pull_snapshot` :535
(all-or-nothing, three conditional reads); the per-bound-card detail
refresh :712–732; `SyncReport::Completed {snapshot, validators,
pushes, read_at}` (messages.rs:295) — already per-board (DF-1);
conditional-read validators `Validators`/`Fetch` (client.rs:269/281);
**no HTTP validators exist on attachment content** (roundtrip §3.2:
no ETag/Last-Modified on the content route, no 304 path); change
detection therefore rides attachment metadata `lastModified` +
`filesize` (roundtrip §3.2/§3.4); every attachment write advances the
card's entity stamp (roundtrip §3.3).

**Decision D-S4 (integration).** Per board cycle, the document rides
the existing push→pull→report spine in four additions:

1. **Evidence alongside the snapshot.** The document is downloaded
   *during the pull phase* (after `pull_snapshot` returns, using the
   anchor discovery of D-S1) and travels as a **new field on
   `SyncReport::Completed`** — `app_data: AppDataEvidence`, a typed
   enum (ruled 2026-10-18 for the D-S2 warning requirement — a bare
   `Option` would swallow the reason):

   - `Document(Box<AppDataDocument>)` — evidence to fold (boxed; a
     document dwarfs the other fields; messages.rs's own
     snapshot-boxing precedent);
   - `Absent` — nothing to read (no anchor, no attachment,
     tombstoned attachment): the normal state, surfaced as silence;
   - `Unreadable(AppDataUnreadable)` — `FormatTooNew { format }` /
     `Transport` / `Decode` / `ForeignBoard` (D-S7 enumerates the
     fences).

   The engine folds `Document` evidence inside `apply_sync_report`
   per D-S3 and derives a per-board sidecar health from the variants
   (an additive status the UI badge and CLI `status` read;
   `FormatTooNew` renders as the actionable update warning, D-S2;
   `Absent` is silence). `SyncReport::Failed` never carries document
   evidence — a failed cycle folds nothing. This is *new evidence
   beside `RemoteBoardSnapshot`*, exactly the binding §5 shape — the
   snapshot itself stays native-only.
2. **Anchor metadata via the card-detail read.** The anchor card's
   attachment metadata (id, `lastModified`, `filesize`, `deletedAt`)
   comes from the card detail read — for a bound anchor, the same
   `client.card` GET the detail-refresh loop (:712–732) already
   performs every cycle (capture `detail.attachments` there; zero
   marginal requests); a dedicated single read covers the unbound /
   discovery / adoption cases (D-S1). Card detail 304s would also
   gate it correctly (any attachment write advances the card stamp,
   roundtrip §3.3 — a 304 card implies unchanged attachments), but
   the refresh loop is unconditional today and the design does not
   add conditional reads it does not need.
3. **Change detection by `lastModified`/`filesize`.** The actor keeps
   in `LoopState` (in-memory only — **no** new `ValidatorKey` kind:
   attachment content has no HTTP validators, roundtrip §3.2, so the
   persisted validator table is the wrong home) the pair
   `(attachment.lastModified, filesize)` plus the parsed document
   from the last ingest. Metadata equal → skip the content GET and
   reuse the cached document; differ (or cold start) →
   `download_attachment`. Cost ceiling: one document GET per board
   per process start (~15–25 KB at 100 cards, roundtrip §3.4) and
   one per foreign change — negligible, and it buys a stateless,
   restart-safe design (the fold is idempotent, so a redundant GET is
   harmless). The known theoretical blind spot (same-size,
   same-second rewrite — roundtrip §7) is accepted for one document
   per board.
4. **Flush upload after the cycle's card pushes — read-modify-write
   union.** The document group is planned by `plan_pushes` with the
   card groups (D-S5) but **executes after the pull and before the
   report is sent** (between actor.rs:448 and :453). This is the only
   position satisfying all three binding §5 clauses at once: *after
   the cycle's card pushes*, *before the cycle ends*, and *the stamp
   bump lands in the next cycle's pull* (the snapshot was already
   assembled pre-upload; an upload before the pull would put our own
   bump in the same cycle's evidence). The upload content is
   **union(cached/downloaded document, local entries for the group's
   tasks as of `read_at`) — never a blind overwrite** (roundtrip §5):
   per key the newer embedded clock wins; unknown sections/keys are
   preserved verbatim (D-S2); the snapshot-absence GC applies (D-S2).
   Local entries are taken **full-send** (the whole entry for each
   task in the group — the outbox module's own rule, outbox.rs:5).
   Tasks unresolved at upload time (create failed, still unbound)
   are omitted — their ops stay queued and ride the next cycle.
   On success the group's outcomes are appended to `pushes` (ops
   complete via the ordinary `PushOutcome` path); after the upload,
   `LoopState`'s cached metadata is refreshed from the upload
   response so the next cycle's comparison does not re-GET our own
   write.

   *Rebuild mode*: when triggered (D-S1 loss taxonomy — re-anchor,
   restore, adoption, or document absent while local app-data
   exists), the union's local side is **all** local entries, not just
   the group's tasks; the pending sidecar ops complete with it.

**DF conformance**: additions inside the per-board cycle the
multi-board world already defines (per-connector cycles over
connector-owned boards — DF-3/DF-5); `SyncReport` widens additively
(one optional field) on its existing mpsc lane. **Topology
untouched.**

### 3.5 D-S5 — The outbox op shape

**Anchors.** `LocalOp` (domain outbox.rs:22) — id-scoped and
field-less by design ("`TaskClocks` already knows which fields
changed, and every Deck write is full-send anyway", module doc :5);
`op_targets_task` (:71); `PushGroup {target, materialized, primary,
subsumed}` + `plan_pushes` (push.rs:229/317 — coalescing and
dependency ordering); the roundtrip §5 push-side ruling ("a sidecar
push op follows that shape (e.g. `UpdateAppData(TaskId)`), coalesced
by the connector into the document at flush"); storage codec op-tag
table + legacy-tag rejection (codec.rs:401–409); `EntityTables`
(push.rs:214 — the owning-board resolution DF-1 names).

**Decision D-S5 (op).** One new arm:

```rust
pub enum LocalOp {
    // …existing arms…
    /// App-data document entry for a task is stale: fields and/or
    /// reminder rules. Full-send at flush; coalesces per board into
    /// the single document upload.
    UpdateAppData(TaskId),
}
```

- **Enqueue sites**: the engine command handlers for the field family
  (doc 2's verbs — `SetEffort`-class commands enqueue it exactly
  where they'd enqueue `UpdateTask`) and the reminder-rule commands
  (`SetReminderRule`/`ClearReminderRules` and doc 3's N-rule
  add/remove verbs — **this is the rule-sync lane; no `UpdateTask` is
  ever enqueued for reminder data**, keeping the v1 fence, §3.9).
- **One op, both sections, deliberately coarse**: a field-only edit
  and a rule-only edit produce the same op for the task. Rationale:
  the flush is one full-send document PUT per board regardless of op
  granularity, so granularity buys nothing on the wire; it only
  sharpens the self-clobber guard, where coarse over-protection is
  the conservative direction (a one-cycle adoption delay, converging
  next cycle). Coarser still (board-scoped `UpdateAppData(BoardId)`)
  was rejected: the guard would protect *every* task's fields on any
  single task's edit — over-protection across tasks is not
  conservative, it delays unrelated adoptions for no savings.
- **Coalescing**: `plan_pushes` folds all of a board's pending
  `UpdateAppData` ops into **one** `PushGroup` (target = the board's
  document; `primary`/`subsumed` per the existing shape) — one PUT
   per flush. Dependency ordering: the group executes after
   `CreateTask` groups (entries key on remote card ids, which exist
   only post-echo — the same create-before-update discipline the
   planner already enforces for native updates).
- **Board resolution** for `pending_ops` accounting: through the
   entity graph like `UpdateTask` (DF-1: task ops resolve the owning
   board via `EntityTables`); `op_targets_task` grows the arm; the
   codec grows the tag `update_app_data` (additive — no legacy tag is
   renamed, so codec.rs's one-representation rule holds).
- **Completion**: success → the group's `PushOutcome` completes
   primary + subsumed (`CompleteOp`); transport failure → **no
   outcome** (the ops stay queued and retry — §3.7); permanent 4xx
   rejection of the document group (e.g. the anchor is uncreatable
   for a permission reason that will not self-heal) dead-letters as
   counted `FailOp`s, mirroring the native push's bad-request path
   (pipeline.rs `apply_push_report` dead-lettering).

**DF conformance**: one additive `LocalOp` arm, id-scoped like its
siblings; zero diff to the board-addressed ops (DF-1's "outbox ops:
zero diff" stands — this is a new arm, not a reshaping). **Engine
vocabulary additive; topology untouched.**

### 3.6 D-S6 — The self-clobber mirror

**Anchors.** `PendingTaskIntent` (pipeline.rs:92 — one bool per
protected field), `pending_task_intents` (:104 — the op→flag mapping,
keyed `op.queued_at > read_at`), `keep_task_intent` (:158 — **copies
protected values and their write clocks back** over the merge
result); the binding requirement (roundtrip §5: "the intent-keep
step copies the app-field values *and their document-embedded
clocks* back over the fold result"); the guard's boundary convention
(`queued_at == read_at` is pre-read — pipeline.rs:3828 test pin).

**Decision D-S6 (mirror).** The guard widens in lockstep with the
field family (the #244 seam — doc 2 owns the manifest test; this doc
fixes the sidecar's side of the contract):

- `PendingTaskIntent` grows one bool per app field **plus one
  `reminder_rules` flag** (per task). `pending_task_intents` grows
  the arm: `LocalOp::UpdateAppData(t)` with `queued_at > read_at`
  sets **all** the task's app-field flags and its rules flag (coarse
  by design, D-S5; the boundary convention is inherited verbatim —
  `queued_at == read_at` is seen by the cycle and unprotected).
- The copy happens **inside `fold_task_app_data`** (D-S3): per field,
  intent flag set → the fold result takes the local value *and the
  local clock* (both the `TaskClocks` slot for fields; the rule's own
  write clock for rules), overriding whatever the document evidence
  said. For rules, the mirror copies the task's local rule set and
  their clocks over the fold result. This is the exact native shape
  (`keep_task_intent`'s value+clock copy) transposed onto the
  document fold, positioned *after* it per the binding §5 wording —
  one guard struct, two application sites (native fields in
  `keep_task_intent`, app fields in the fold).
- **Why the clock copy matters here**: the next cycle's fold will see
  the just-flushed document echo (clocks equal → idempotent). If the
  guard copied only values, a *failed* flush followed by a concurrent
  remote write would leave the local clock stale and the next fold
  would adopt the remote value over the protected local intent one
  cycle late — the exact self-clobber the native guard prevents
  (pipeline.rs:83–89 doc), mirrored.

**DF conformance**: engine-internal widening of an engine-internal
struct; the lockstep manifest test (doc 2) pins the four structures
agreeing. **Topology untouched.**

### 3.7 D-S7 — Failure fences

**Anchors.** The evidence-then-verdict principle (ADR 0007; the
push-only composition `apply_push_report`, pipeline.rs:819, exists so
a failed cycle can never tombstone from an empty snapshot — roundtrip
§5); `PersistenceAction::{CompleteOp, FailOp}` (persistence.rs:191
area); the binding fences (roundtrip §5: "a failed document upload
keeps its ops queued (retryable …) and a failed/partial document
download folds nothing (no tombstones from a document we could not
read)"); the cycle's failure paths (actor.rs: `GroupOutcome::Aborted`
:386–403 — landed outcomes travel, no pull; pull failure :432–447 —
`Failed` report with landed pushes).

**Decision D-S7 (fences).** Each failure mode, its detection, and its
fence:

| Failure | Fence |
|---|---|
| Document **download** fails (transport) | No evidence: `app_data: None` in the report — **folds nothing**. The cycle itself still completes (the snapshot and card pushes are unaffected evidence). **No upload either** — read-modify-write without the read is a blind overwrite, which is forbidden; the sidecar ops stay queued and retry next cycle. |
| Document **decode** fails (truncated body, corrupt JSON) | Same as transport: fold nothing, upload nothing, ops wait. A *persistently* corrupt document strands app-data sync for that board — honest stranding visible in `pending_ops`. Recovery runbook (documented, manual): delete the attachment in Deck's UI → the tombstone path (D-S1) fires restore + rebuild from local truth. |
| **Format too new** (`format >` ours) | Fold nothing, upload nothing (a relay we cannot parse must not be re-encoded), ops strand until the device upgrades (D-S2). **Surfaced, not silent** (ruled 2026-10-18): the reason reaches the engine through the typed evidence (D-S4) and renders as the per-board "update this app" warning in the UI badge and CLI `status`. |
| **Foreign board key** | Fold nothing from it; the flush may replace the content (D-S2's moved-card fence). |
| Document **upload** fails (transport) | The document group emits no outcome — its ops **stay queued** (retryable; no `CompleteOp`, no `FailOp`). The cycle still reports `Completed` with its valid snapshot and document evidence: the snapshot was assembled pre-upload, so no ordering hazard exists (unlike a native push abort, which must skip the pull — actor.rs:386's rationale does not apply post-pull). |
| Upload **permanently rejected** (4xx that will not self-heal) | Dead-letter the group's ops as counted `FailOp`s (the native bad-request precedent), typed log. |
| **Anchor creation** fails (permission) | No anchor, no document lane: fold-only operation if another device maintains a document; otherwise sidecar ops strand honestly (per-board `pending_ops` climbs — the ADR 0010 standalone precedent for stranding). Retryable; the ceremony re-attempts when a flush next has work. |
| **Anchor card itself** deleted mid-cycle | Discovery is per-cycle and stateless; the next cycle's scan re-anchors + rebuilds (D-S1). No persisted anchor identity exists to corrupt. |
| `SyncReport` send fails after upload | Ops stay queued (the engine never saw the outcomes); the next cycle re-uploads the union — idempotent by clocks. Same recovery as a native push in that position. |

The table's invariant: **evidence gates every verdict** — the fold
consumes only a decoded, format-compatible, board-matching document;
the flush consumes only a document this cycle actually read; ops move
only on outcomes the engine saw.

**DF conformance**: fences live inside the existing report/outbox
mechanics; no new channel or persistence action kind. **Topology
untouched.**

### 3.8 D-S8 — Per-board documents across N connectors

**Anchors.** The multi-board world: per-board `SyncStates` keyed by
`BoardId` with connector attribution derived from the tagged binding
(design §3.2, DF-9); the connector-owned board set derived per cycle
from bindings (design §3.3, DF-1/DF-3 — the resume-path
generalisation); per-connector cycles iterating boards sequentially
at O(boards + changed cards) (DF-5 #157 guardrail); the binding's
singularity (design §3.2: board → connector is a *function* — a
board syncs through exactly one connector); config `[[sync.
connectors]]` (DF-6).

**Decision D-S8 (per-board).** One anchor + one document **per
board**, full stop:

- **Keying**: the document's identity is the board (D-S2's `board`
  field; the filename/title constants are per-card but the card is
  per-board). The document never spans boards; entries key on that
  board's card ids.
- **Cycle scoping**: all document work (discovery, download, fold
  evidence, upload) happens inside *that board's* cycle — the
  per-board unit the multi-board actor already iterates. Per-board
  flush ordering is therefore simply "within the board's own cycle,
  after its card pushes, before its report" (D-S4); **no cross-board
  document ordering exists or is needed** — boards are independent
  documents on independent anchors.
- **One connector per board** (binding singularity): no
  two-connectors-write-one-document race exists by construction. Two
  *devices* behind two connectors sharing one *Deck user account*
  remain concurrent writers — handled by the union + embedded-clock
  rules (roundtrip §4/§7: the residual loss window converges on the
  next flush because absence is not deletion).
- **A connector without an anchor** (the framing's explicit
  question): boards with no app-data anywhere get **no anchor and no
  document** — creation is lazy (D-S1), so a connector whose boards
  have no sidecar ops pays exactly zero sidecar cost (no extra
  requests, no UI pollution). A connector that cannot write
  (read-only board share) never anchors; it folds whatever a
  writable peer maintains (D-S7). A connector whose boards are
  standalone/unbound has no cycles at all.
- **Cross-board card moves lose app-fields**: a card moved to another
  board leaves this board's snapshot (R3 tombstones the local task)
  and enters the other board's documentless — its app-fields and
  rules lived in the *origin* board's document and do not follow.
  This is an accepted, recorded limitation of board-scoped documents
  (the flip side of the roundtrip §4 granularity win): local truth on
  the moving *device* still holds the fields until its task is
  tombstoned; other devices drop them with the R3. A reconcile-style
  repair is out of scope (backlog §8). Surfaced to the user session
  (§6).

**DF conformance**: no cross-connector coordination, no registry
entries, no routing — the document rides the per-board cycles the
accepted design already defines. **Topology untouched (DF-10).**

### 3.9 D-S9 — The reminder-rules section

**Anchors.** `ReminderRule` (reminders.rs:109) with the `created_at`
doc's reservation (:117–119: "Per-field write clocks are added
additively when a connector demands rule sync; v1 needs only the
creation stamp") — this design is that demand; `ReminderState.rules`
by id (:213); the v1 fences (pipeline.rs:733 "no reminder data enters
the sync path", :893 "no reminder data enters the push path");
roundtrip §6's verdict ("Reminder rules … N rules per task as
document keys; gives reminders their first sync lane while
`pipeline.rs` keeps them out of the native path … The v1 fence stays
intact"); framing D-D2 (#202 rules half in-wave, proofs → D5).

**Decision D-S9 (rules section).** N rules per task as document keys
under `reminder-rules` (D-S2's `RulesEntry`):

- **Identity**: the document rule key is the `ReminderRuleId` UUID
  itself — device-minted at creation (ids.rs:59), becoming
  cross-device-stable through the document (the fold adopts the
  document's id verbatim as the local id). This preserves the
  domain's id-keyed model exactly, unlike the task map (which keys on
  remote card ids because cards have a remote-native identity — rules
  have none; the document *is* their remote identity). The task link
  rides the entry's card key: flush resolves `rule.task: TaskId` →
  remote card via the binding (the same resolution as field entries);
  fold resolves back through `task_by_board_card`.
- **Clocks**: `ReminderRule` grows one additive write clock (the
  reserved slot; a `clock`/`updated_at` field landed by the
  rules-section chunk — the domain-side cost of consuming the
  reservation, additive column in the wave's storage migration,
  0007+). Every rule mutation (create, and doc 3's edit/remove
  verbs) stamps it; the document entry carries it; the fold and the
  mirror use it (D-S3/D-S6).
- **Deletes are tombstones**: removing a rule writes
  `{deleted: true, clock: now}` (D-S2) — absence never deletes
  (another device's stale, truncated, or racing document must not
  resurrect a deliberate removal). `ClearReminderRules` (and doc 3's
  remove verb) emits one tombstone per removed rule.
- **The v1 fence stays intact — precisely restated**: *no reminder
  data enters the native merge or push path*. The native snapshot,
  `merge_task`, and `UpdateTask` pushes continue to never carry
  reminder data (the two pipeline.rs fence comments are amended, not
  deleted, by the implementing chunk: "no reminder data enters the
  *native* sync/push path; rules ride the app-data document lane").
  Rule mutations enqueue `UpdateAppData(TaskId)` (D-S5) — the
  document is a **new lane**, which is exactly what the fence was
  holding the door open for.
- **Scope fence (framing D-D2)**: only the *rules* half syncs.
  `deliveries` (proofs), `snoozes`, and `muted` stay device-local —
  proofs are the D5 substrate's section (reserved name, D-S2);
  snoozes/muted are per-device interaction state. They remain
  persisted per-device (0004's tables) and untouched by the fold.

**DF conformance**: `AppState.reminders` widens its update path inside
the engine fold only; no new messages. **Topology untouched.**

## 4. Live-tier risk (Deck 1.17.5) — stated explicitly

The D1 wire evidence was measured on **tier-2 Nextcloud 35.0.1 /
Deck 1.19.0**; the live production tier is **Deck 1.17.5**. The
availability verdict transfers by monotonicity (a surface absent on
1.17.5 could not appear in 1.19.0's code without being there —
roundtrip §3.1), but the **behavioral** verdicts did not transfer and
were not probed on the live tier (roundtrip §7, the named follow-up):
multipart field names, the app-level POST update route's exact
semantics, the 409 duplicate rule, soft-delete/restore behavior. The
1.19.0 changelog itself fixes "broken upload of attachment via api"
(#8156, #8219) — direct evidence this surface moved inside the
1.17.5→1.19.0 window, in unknown directions for our paths.

Where this design leans on those verdicts:

- **The flush path's critical edge is the app-level POST update
  route** (client.rs `update_attachment_content` :737) — not in
  Deck's documented API spec (roundtrip §7). If it misbehaves on
  1.17.5, every flush after the first upload fails → sidecar ops
  strand honestly (D-S7) with no data loss (local truth), but the
  sidecar is effectively write-dead on the live tier.
- The client surface landed with the D1 wire traps contract-pinned
  (framing §3) — those pins encode **1.19.0** behavior.

**Requirement on the implementation round** (a gate, not a backlog
item): before or with the first sidecar implementation chunk, run the
named live-tier probe (or a contract-fixture run against a dockerized
**1.17.5**) covering at minimum: upload part names, create-409,
update-in-place with stable filename, soft delete + restore, and the
no-validators assumption. The design's create-409→update ladder
(D-S1) already avoids every filename-changing flow (renames 500,
roundtrip §3.2); the probe closes the remainder. Until it passes, the
live tier's sidecar is expected-degraded (fold works — reads are the
boring half; flush reliability unknown), and that expectation is the
honest thing to tell the user session.

**Ruled (2026-10-18):** the gate's form is the **dockerized Deck
1.17.5 contract-fixture run** — reproducible, in-repo, CI-able — not
a probe against the live production server.

## 5. Coherence map

- **One identity chain**: document keys are `(board, remote card id)`
  everywhere — entries, guard resolution, flush keying, GC pruning.
  Local `TaskId`/device internals never serialize (the D-6 rule);
  the one exception-by-design is `ReminderRuleId`, whose UUID *is*
  the document key because rules have no remote-native identity
  (D-S9).
- **One clock discipline**: every app-data verdict anywhere (fold,
  guard mirror, flush union, echo idempotence) is
  document-embedded-clock LWW; the card stamp is never consulted for
  app data (D-S3). The two domains meet only in the cycle's
  ordering (D-S4).
- **One guard**: `PendingTaskIntent` protects both families — native
  fields via `keep_task_intent`, app fields/rules inside
  `fold_task_app_data` — with the same `queued_at > read_at` key and
  the same value+clock copy shape (D-S6).
- **One upload, full-send**: the flush is one document PUT per board
  per cycle with work; entries are full-send; the union preserves
  everything unrecognized (D-S2/D-S4/D-S5).
- **Conformance to the binding inputs**: every roundtrip §5 clause is
  traceable to a section above (fold position D-S3, flush position
  D-S4, fences D-S7, guard D-S6, op shape D-S5, identity D-S2/D-S9,
  R3 fence D-S3); DF-10 holds in every section's note.

## 6. Decision record — recommendations + open points

| ID | Point | Recommendation (exactly one) | Status |
|---|---|---|---|
| D-S1 | Anchor ceremony | Convention-named card `[taskboard] app-data`, attachment `taskboard-appdata.json`; stateless per-cycle discovery (exact-title scan + detail-read confirmation, newest-stamp pick); lazy actor-side creation; single-unconfirmed-match adoption; loss taxonomy with re-anchor + rebuild flush; restore-then-replace for tombstoned attachments; never fights user actions | **ACCEPTED (ruled 2026-10-18)** |
| D-S2 | Document schema | Format-versioned envelope (`format: 1`) over a string-keyed section registry (`tasks`, `reminder-rules`; reserved `proofs`/`settings`/field-key space); unknown content preserved verbatim; `null` = cleared field, tombstones only in rules; format-too-new = fold-and-flush nothing | **ACCEPTED (ruled 2026-10-18)** + ruled addition: the fence is loud (per-board UI/CLI update warning) |
| D-S3 | Merge primitive | `fold_task_app_data` (merge_task sibling) + rules fold: embedded-clock LWW, clock = max, absence never deletes, never feeds R3, folds after `merge_task`/`keep_task_intent` via `task_by_board_card` | **ACCEPTED (ruled 2026-10-18)** — follows binding §5 mechanically |
| D-S4 | Actor-cycle integration | Document evidence as the typed `Completed.app_data: AppDataEvidence` enum; anchor metadata from the (already-paid) card-detail read; `LoopState`-cached `(lastModified, filesize)` change detection; upload between pull and report as read-modify-write union | **ACCEPTED (ruled 2026-10-18)** — incl. the `LoopState`-only cache (ruling 5) and the typed evidence enum (ruled addition to D-S2) |
| D-S5 | Outbox op | `LocalOp::UpdateAppData(TaskId)`, one op for fields+rules, coalesced per board into one document group, codec tag `update_app_data` | **ACCEPTED (ruled 2026-10-18)** |
| D-S6 | Self-clobber mirror | `PendingTaskIntent` grows per-app-field + `reminder_rules` flags; `UpdateAppData` after `read_at` sets them; the fold copies local values **and embedded clocks** back over the fold result | **ACCEPTED (ruled 2026-10-18)** — follows binding §5 |
| D-S7 | Failure fences | The §3.7 table: failed download/decode/format/foreign → fold nothing + no upload; failed upload → ops stay queued, cycle still `Completed`; permanent 4xx → counted dead-letters; anchor uncreatable → honest stranding; format-too-new surfaced loudly | **ACCEPTED (ruled 2026-10-18)** |
| D-S8 | Per-board × N connectors | One anchor + document per board, all work inside the board's own cycle, no cross-board ordering; lazy (zero cost without sidecar ops); read-only connectors fold-only; cross-board moves lose app-fields (accepted limitation) | **ACCEPTED (ruled 2026-10-18)** — limitation accepted for the wave; repair tracked as issue #350, unscheduled |
| D-S9 | Reminder-rules section | Rules as document keys keyed by the rule UUID (document-stable identity); one additive rule write clock; explicit tombstones for deletes; fence restated: nothing reminder-shaped enters the *native* path; proofs/snoozes/muted stay local | **ACCEPTED (ruled 2026-10-18)** |
| — | Live-tier risk | Deck 1.17.5 behavioral re-probe is a **gate on the first implementation chunk** (§4); until then the live tier's flush reliability is unknown and the sidecar is expected-degraded there | **ACCEPTED (ruled 2026-10-18)** — form ruled: the dockerized 1.17.5 contract-fixture run |

**Decision session record (2026-10-18)** — the user ruled on every
open item (framing D-D6's list, this doc's entries):

1. Convention constants: **accepted as designed** (`[taskboard]
   app-data` / `taskboard-appdata.json`; compile-time, never config).
2. Adoption rule: **confirmed** — adopt a card of the exact same
   name when one exists and no confirmed carrier does (D-S1).
3. Cross-board move limitation: **accepted for this wave**; the
   repair is tracked as issue #350 (`reliability`), deliberately
   unscheduled — not in `wave-daily-driver`.
4. Format fence: **accepted, with an added requirement** — the
   stranding must surface a clear UI/CLI warning telling the user to
   update the app on this device to the version their other devices
   use (typed through D-S2/D-S4/D-S7).
5. Document cache: **memory-only accepted** (`LoopState`; one
   document GET per board per restart, no new persisted kind).
6. Anchor task in the UI: **hide + app-layer CLI guard** — binding
   direction for doc 4 and the implementation chunks (§7).

Rider: the live-tier probe gate runs as the **dockerized Deck 1.17.5
contract-fixture** form (§4).

## 7. Consequences for the wave implementation plan

Chunk mapping against the framing §6 sketch (advisory there,
confirmed here): chunks 3–4 own this design; the interfaces its
siblings depend on:

- **Doc 2 (field family)**: owns `Task`/`TaskClocks`/`TaskChanges`
  field shapes, the #244 lockstep manifest test (which must now also
  name `PendingTaskIntent`'s app-field flags and the
  `UpdateAppData` enqueue sites), the storage columns (0007+), and
  the **native-`startdate` ruling** — which decides whether a
  `scheduled` key ever appears in the `tasks` section (this schema is
  agnostic: a native ruling simply means no sidecar key for it).
- **Doc 3 (reminders surfaces)**: the N-rule verbs are the enqueue
  sites for `UpdateAppData`; its rule-edit semantics define which
  mutations stamp the rule clock (D-S9).
- **Doc 4 (views)**: binding input (ruled 2026-10-18): **hide the
  anchor task** — a presentation-layer exact-title filter excluding
  it from lists, counts, the today view, and search — and **render
  the per-board format-fence warning** (D-S2's update-this-app
  message) on the status surface. The companion **app-layer CLI
  guard** (a typed refusal of rename/delete verbs targeting a task
  whose title equals the anchor constant — app layer, never engine)
  lands with the sidecar implementation chunks, not doc 4.
- **Implementation chunks**: (3) document codec + anchor ceremony +
  connector integration = D-S1/D-S2 + D-S4's actor side; (4) fold +
  engine integration + flush + fences + rules section = D-S3/D-S5/
  D-S6/D-S7/D-S9. Storage touches: codec tag (chunk 3), rule clock
  column + field columns ride docs 2/3's additive migration (0007+).
  The live-tier probe gates chunk 3 (§4).
- **Snapshot corpus**: `SyncReport` widening rewrites report
  fixtures; `AppState` gains nothing document-shaped — the field
  family's snapshots are doc 2's regeneration passes (DF-9's rule:
  the widening chunk owns its one deliberate insta pass).

## 8. Backlog candidates (out-of-scope ideas; the user promotes)

- Best-effort cleanup of duplicate/loser anchor cards after races
  (D-S1 declines it in-wave).
- Cross-board move repair (reconcile app-data by origin-board
  document lookup at adoption) — **tracked as issue #350** (ruled
  2026-10-18: the D-S8 limitation is accepted for the wave; the
  repair is unscheduled).
- Conditional (304-gated) anchor detail reads once the per-card
  refresh itself goes conditional (#157's path).
- Per-connector or per-board sidecar opt-out config (a user who
  refuses the anchor card's Deck-UX presence trades away app-field
  sync).
- Persisted document-stamp cache (D-S4.3's alternative) if restarts
  prove frequent enough to matter at kiosk scale.
- Document compression — payloads are trivial (roundtrip §3.4);
  revisit only if the D5 sections change the arithmetic.

## 9. References

- Roundtrip strategy report (normative):
  `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  — §4 (recommendation), §5 (merge-policy implications, BINDING),
  §3 (wire evidence), §6 (per-field verdicts), §7 (risks).
- Daily-driver framing plan (normative §5 Q1):
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`.
- Multi-board detailed design (ACCEPTED) +
  implementation plan DF-1…DF-10:
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`,
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`.
- Landed transport plan:
  `.artifacts/plans/2026-10-15-sync-nextcloud-attachment-content-plan.md`.
- Reminders design:
  `.artifacts/plans/2026-10-12-reminders-design-plan.md`,
  `2026-10-13-reminders-domain-model-plan.md`.
- ADR 0007 (evidence-then-verdict), ADR 0010 (standalone-first /
  connector vocabulary), ADR 0012 (detach semantics).
- Code anchors @ `b809809` (§2's table; symbols are the stable keys).

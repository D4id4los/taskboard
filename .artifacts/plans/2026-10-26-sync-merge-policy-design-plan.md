# [Plan] Sync Merge-Policy Family — Design (DR-4)

- **Date**: 2026-10-26
- **Kind**: design round deliverable — DR-4 of the 2026-10-25 backlog
  triage dispatch (§14: "sync merge-policy family"). One merge-policy
  design for the four issues that share `merge.rs`/`pipeline.rs`
  semantics — #128 (field-level granularity), #129 (OR-set labels),
  #130 (resurrect-as-new-card), #133 (clock skew) — with #150
  (duplicate-create reconciliation) designed alongside because it
  reads the same merge/pipeline surfaces. Zero production code; this
  file is the round's entire diff, plus the implementation issues
  opened alongside it.
- **Origin**: issues #128, #129, #130, #133, #150; the dispatch
  round's ruling "one round, one agent: these share merge.rs /
  pipeline.rs semantics and must never be parallelized across agents"
  binds the follow-up round's chunk order (§6).
- **Normative inputs** (all verified against `origin/main` @
  `2991f65`):
  - ADR 0004 (sync conflict policy: per-field LWW, the R1–R9 rules,
    the 2026-10-11/18/25 board amendments) — the contract this round
    amends, never rewrites.
  - `crates/taskboard-domain/src/merge.rs` (the per-entity algebra),
    `pipeline.rs` (`apply_sync_report`, the fixed-order composition,
    the self-clobber guards), `entities.rs` (`Task`/`TaskClocks`),
    `remote.rs` (`RemoteTask`, `RemoteIndex`), `outbox.rs`
    (`LocalOp`), `reconcile.rs` (`plan_reconcile`, the bind
    ceremony's normalized-title survivorship pass).
  - `crates/taskboard-sync-nextcloud/src/actor.rs` (the injected
    `Clock` seam), `model.rs` (the card DTO already parses
    `created_at`), `mapping.rs` (which drops it).
  - `crates/taskboard-storage-sqlite/migrations/0001…0009`;
    ADR 0005's additive-migration policy.
  - Audit evidence: the v0.1.0 findings report (F10-01, F11-01,
    F11-02) and the chunk-11 §skew comparison-site map C1–C9 with
    interleavings (i)–(iii)
    (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-time-dst-report.md`
    §skew; chunk 10's W1/W2 crash-window sizing in
    `2026-10-16-v0.1.0-audit-theme-sync-persistence-report.md`).
  - The originating decision record:
    `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md`
    §7.4 (the granularity trap) and §12 (the backlog entries this
    round consumes).
- **Branch / worktree**: `docs/sync-merge-policy-design` in
  `../taskboard-WT-docs-sync-merge-policy-design`, off `origin/main`
  @ `2991f65`.
- **Status**: PROPOSED — the five direction forks (§4, F-128 …
  F-150b) are documented at decision-ready depth with a
  recommendation each and are **not yet ruled on**; the user's plan
  review is the ruling pass. The implementation issues opened with
  this plan carry the same flags in their bodies; a ruling that flips
  a fork edits the issue body at plan review — this plan stays the
  single source.

---

## 1. Mission & binding constraints

One coherent merge-policy family for the offline-first sync path,
resolving four accepted-by-design data-loss classes and one
data-pollution class, without breaking what ADR 0004 got right:
deterministic, total, clock-free merge primitives; the engine (not
the actor) executes the policy; the snapshot-completeness contract;
the fixed processing order; push-echo adoption; the self-clobber
guards. Binding constraints, non-negotiable:

- **Purity**: merge primitives never read clocks, generate ids, or
  perform IO. Every new comparison input (baselines, membership
  clocks, `created_at`) is injected data living on the entities or
  the report.
- **Additive storage** (ADR 0005): new columns are NULL/epoch
  defaulted so pre-migration rows keep today's behavior bit-for-bit,
  pinned by upgrade tests (the migration-0002 precedent).
- **Test discipline** (AGENTS §5): typed outcomes, no log-text
  assertions, proptest for the pure algebra, `currently_pins_…`
  naming for tolerated limitations, no upper bounds on success.
- **Type-driven A1**: new state that carries invariants (membership
  tombstones, the resurrect predicate's inputs) gets hand-rolled
  types with the invariant at the constructor, not boolean soup.
- **Never parallelized**: the follow-up round's chunks run strictly
  one agent at a time (§6); several touch the same `merge_task` /
  pipeline sections.

The round's one-sentence summary: **the merge keeps learning what the
remote actually did** — a persisted remote baseline tells it which
fields really changed (#128), which label memberships were added or
removed (#129), and whether a card disappearance should kill or
re-create (#130) — while skew stays documented-and-pinned (#133) and
the crash-window duplicate gets recognized as our own (#150).

## 2. Verified baseline — numbered facts

All against `origin/main` @ `2991f65`; line numbers from that tree.

1. `TaskClocks` carries twelve per-field write clocks
   (`entities.rs:278-312`); `Task.remote_seen: Option<DateTime<Utc>>`
   is the entity-level last-observed remote stamp
   (`entities.rs:429-432`). There is **no** persisted record of the
   last-observed remote *values* — `remote_seen` is a timestamp only.
2. `merge_task` applies the remote value to each field independently
   iff `remote.last_modified > clocks.<field>` (strict; ties keep
   local) — `merge.rs:195-254`. The labels branch merges the whole
   set under one clock (`merge.rs:238-242`, R7). Position is one
   composite field (`merge.rs:225-235`, R8).
3. The granularity trap is unit-pinned as designed behavior
   (`granularity_trap_remote_entity_touch_overwrites_local_title`,
   merge.rs tests; ADR 0004's 2026-10-25 amendment corrects the
   enforcement citation to this tier). #128 changes this contract.
4. The pipeline gates each entity merge on
   `remote.last_modified > remote_seen` (R4 fast path;
   `pipeline.rs:613-616` for tasks, `:537-540` stacks,
   `:572-575` labels); unchanged remotes never reach the merge.
5. R3 absence tombstoning (cards) is unconditional on local edit
   state: bound-but-absent ⇒ `tombstone_task(local, now)` + cancel
   the task's pending ops (`pipeline.rs:745-767`). ADR 0004 §Deletion
   documents delete-wins and backlogs resurrect-as-new-card (#130).
6. R5 resurrect: a local tombstone clears iff the remote write is
   strictly newer than `clocks.deleted` (`merge.rs:250-254`); the
   pending `DeleteTask` the cycle saw is dropped
   (`pipeline.rs:637-652`).
7. Self-clobber guards (`PendingTaskIntent` et al.) protect fields
   with outbox ops queued after `read_at`
   (`pipeline.rs:94-238`) — a different race than #128's
   granularity; both survive this round unchanged.
8. Labels are stored as a bare join table `task_labels(task, label)`
   PK (`migrations/0001_init.sql:80-85`) — no per-membership state,
   no clocks. Local verbs `AssignLabel`/`UnassignLabel` exist
   (`outbox.rs:47-49`).
9. Deck's card JSON carries `created_at` epoch seconds; the DTO
   parses it (`model.rs:118`) and the mapping to `RemoteTask` drops
   it — `RemoteTask` has no creation time (`remote.rs`, the struct at
   the file's task section). `RemoteStack` likewise (verify at
   implementation; the stacks DTO is expected to match).
10. **F11-01 is done**: the sync actor's every wall stamp flows
    through the injected `Arc<dyn Clock>`
    (`SyncActorConfig.clock`, `actor.rs:56-60`; commit `29b1acb`,
    2026-10-17, in main). The state engine reads its own injected
    clock per message (`taskboard-state/src/actor.rs:257`).
    Issue #133's "first work item" is stale.
11. The skew exposure is the audit's C1–C5 map: all LWW comparisons
    are cross-machine wall clock; the motivating self-echo case is
    half-mitigated by R9 (values adopted unconditionally; only
    clocks advance via `max`), leaving interleaving (i)-residual
    (second local edit within the skew window loses to a
    real-time-earlier remote write) and (ii) the resurrect flip (a
    remote write that preceded the local delete resurrects under
    skew). Seconds-resolution ties keep local (iii) — safe-leaning,
    contract, not debt.
12. The duplicate-create window (W1, audit-corrected): crash after
    the create POST completes, before the landing batch's storage
    commit; the outbox retains `CreateTask`; the next cycle re-POSTs
    (push-first, ADR 0007) → two cards. The same window duplicates
    stacks. W2: `boards publish` crashes after `create_board` POST
    before `BindBoard` commits → orphan remote board; runbook says
    delete it in the Deck UI (ADR 0010:118-120).
13. `reconcile.rs` already implements normalized-title (`trim` +
    case-fold) survivorship for the bind ceremony: local-only row
    survives, adopts the bound row's ref, the duplicate neutralizes
    as a ref-less tombstone, create-ops cancel on survivors
    (`reconcile.rs:1-60`, `plan_reconcile`). Reusable vocabulary for
    #150.
14. `SystemEvent` is a closed six-variant enum on the broadcast lane
    (`messages.rs:377-395`); adding a variant is internal-additive
    (not IPC wire). The UI is a dumb client that reads state/events.
15. Migrations shipped: 0001–0009. The next two this round needs are
    0010 (chunk G) and 0011 (chunk L), both additive.
16. Deck offers no write-side preconditions, no per-field stamps, no
    client-supplied ids or idempotency keys; card writes are
    full-send PUTs (ADR 0004 §Context, verified again by the
    2026-10-22 transport characterization). Every mitigation here is
    client-side inference over entity-granular observations.

## 3. The design, decision by decision

### D-1 — #128: the remote baseline and changed-field inference

**Problem** (fact 2 + 3): a remote entity-touch that is merely newer
than a local field edit overwrites that field with its unchanged
remote value. The remote side is entity-granular; today we cannot
tell *which* fields a remote write touched.

**Design**: persist, per task, the **remote baseline** — the field
values of the remote snapshot as last observed — and gate every
remote adoption on a **changed-field diff** against it.

- **D-1a — the record.** `Task` grows
  `remote_baseline: Option<RemoteTaskBaseline>`, a plain data record
  (no invariants — not an A1 type) mirroring the native-lane remote
  fields in *remote vocabulary*: `title`, `description`, `duedate`,
  `scheduled`, `done`, `position (RemoteStackId, order)`,
  `labels (BTreeSet<RemoteLabelId>)`, `archived`. Not covered:
  `effort`/`importance`/`urgency` (sidecar lane, own embedded clocks
  — different domain, D-S3) and `deleted` (cards are presence-based,
  R3). Remote ids are stored raw, not pre-resolved: resolution is
  per-cycle through `RemoteIndex`, and a stored local resolution
  would go stale when boards re-bind.
- **D-1b — the merge rule.** In `merge_task`, per field `f`:

  ```
  apply_remote(f) := remote_wins(ts_r, clocks.f)
                     && baseline_changed(f)
  baseline_changed(f) := baseline.is_none()
                         || remote.f != baseline.f
  ```

  A field the remote did not change keeps the local value **and its
  clock** (the clock is the write time of the *current value*; an
  unapplied observation advances nothing). Applied fields take the
  remote value and stamp `ts_r` as today. `remote_seen := ts_r`
  always, and `remote_baseline := Some(incoming snapshot)` always —
  the merge observes the remote wholly even when it adopts
  partially. A missing baseline (first observation of a pre-migration
  row) falls back to today's blanket per-field LWW for that cycle and
  establishes the baseline — behavior bit-for-bit, then converges.
- **D-1c — every adoption point refreshes the baseline.** R1
  adoption (`adopt_remote_task`), the merge (D-1b), and the R9 echo
  (`adopt_task_after_push`) all set the baseline from their remote
  input — the echo's post-normalization values are the server truth
  and become the diff reference for the next pull. The tombstone
  paths clear it with the binding (`remote = None` ⇒ baseline =
  `None`).
- **D-1d — the known blind spot, documented.** A remote *revert*
  that exactly restores the baseline value is indistinguishable from
  "no change" and will not land while a local edit holds the field.
  Deterministic, bounded (the next genuine remote change to the field
  lands), and vanishingly rare — recorded in ADR 0004's amendment as
  the accepted residue, alongside the existing self-clobber guard
  (which this does not subsume — different race, fact 7).
- **D-1e — scope (⟂ F-128).** *Recommended: tasks only.* Boards,
  stacks, and labels suffer the same hazard (a remote title touch can
  clobber a local color edit) but with 2–4 scalar fields each and no
  long-form content; the mechanism generalizes mechanically (one
  baseline record per entity kind) and the design records that path
  without building it. The alternative (all entity kinds now) widens
  two migrations and the mutant surface for little observed pain.
- **D-1f — conflict surfacing is out.** The issue's idea 3
  (user-visible kept-copy/current-copy conflicts for contested
  fields) needs an `AppState` conflict surface + UI design of its
  own and an interaction policy for kiosk/unattended sessions. Ruled
  out this round with reasons; if wanted later it is a new backlog
  entry that builds on the baseline (the baseline diff is exactly the
  "contested" detector it would need).
- **Storage (chunk G, migration 0010).** `tasks.remote_baseline TEXT
  NULL` holding the serde-JSON of the record (serde_json is already
  in the storage crate's tree — verify at implementation; a codec
  round-trip test pins the format). NULL ⇒ fallback behavior;
  the upgrade test pins a pre-migration row merging identically
  before/after.
- **Tests.** The anti-clobber invariant becomes a proptest: for any
  baseline-equipped task and any remote snapshot whose diff touches
  only field set S, the merged task's fields outside S are unchanged
  (clocks included). Plus: the pinned granularity-trap test flips to
  pin the *new* behavior (the trap closes); the revert blind spot
  gets a `currently_pins_`-named pin; idempotence and convergence
  properties re-run unchanged (they compare whole states, not
  field-application paths).
- **ADR 0004**: one dated amendment — the Decision's
  per-field-LWW sentence gains the changed-field gate; the
  "documented limitation — granularity data loss" paragraph is
  amended (the loss class closes for tasks; blind spot D-1d and the
  boards/stacks/labels remainder stay documented).

### D-2 — #129: OR-set labels (per-membership add-wins)

**Problem** (fact 8): whole-set LWW — two devices adding different
labels concurrently lose one side's addition entirely.

**Design**: label assignments become per-membership state with
clocks; the set merges as an OR-set biased add-wins-on-tie.

- **D-2a — the membership model (A1-shaped).** Per (task, label):

  ```
  MembershipState := Added { at: DateTime<Utc> }
                   | Removed { at: DateTime<Utc> }   // persists — the membership's tombstone
  ```

  On `Task`, the live set and the clocks live in one map:
  `label_memberships: BTreeMap<LabelId, MembershipState>` with a
  `live_labels()` accessor; the bare `labels: BTreeSet<LabelId>`
  field retires (reader sweep in the chunk; the UI reads the
  accessor). Removed rows are never deleted — forgetting a removal
  lets a stale remote re-add flap (the classic OR-set requirement).
- **D-2b — local verbs.** `AssignLabel` stamps `Added { now }`,
  `UnassignLabel` stamps `Removed { now }` (the ops already exist,
  fact 8; only their state effect changes). A re-add after a remove
  overwrites with the newer `Added`.
- **D-2c — remote events via the D-1 baseline.** At merge, the
  remote label set diffs against `baseline.labels` (remote ids):
  observed remote adds stamp `Added { ts_r }`, observed removes stamp
  `Removed { ts_r }`. An unchanged remote set emits no events — Deck
  cache-lag echoes of our own writes are invisible by construction.
  This is why chunk L builds on chunk G.
- **D-2d — the merge rule.** Per membership, local state `l` vs
  remote event `r`:

  | local | remote event | outcome |
  |---|---|---|
  | — | `Added{t}` | added |
  | `Added{a}` | `Added{t}` | added; clock `max(a,t)` |
  | `Added{a}` | `Removed{t}` | removed iff `t > a`, else the add stands |
  | `Removed{r}` | `Added{t}` | added iff `t >= r`, else stays removed |
  | `Removed{r}` | `Removed{t}` | removed; clock `max(r,t)` |

  Concurrent two-device *adds* both survive (the everyday loss
  closes). A remove beats an add only when strictly newer — the
  issue's wording. Cross-machine stamp order still decides a true
  concurrent add-vs-remove pair; that residue is the same skew class
  as #133 (documented, not fixable without concurrency knowledge
  Deck cannot give).
- **D-2e — label-entity death dominates.** A label absent from the
  board listing is entity-dead (`tombstone_label`, existing): all its
  memberships across tasks force-remove, and a *concurrent local
  assignment* to it dies with the entity — the assignment cannot
  exist server-side (the push would 404). Documented dominance rule;
  no zombie labels.
- **Storage (chunk L, migration 0011).** `task_labels` grows
  `state TEXT NOT NULL DEFAULT 'added'` and `ck_membership TEXT NOT
  NULL DEFAULT <epoch>`; removed memberships keep their row with
  `state = 'removed'`. Existing rows backfill as fresh adds (epoch
  clock) — pre-migration behavior bit-for-bit, upgrade-test-pinned.
  The codec writes/reads the full map; `ON DELETE CASCADE` stays
  (task/label hard-delete still sweeps rows — removal remembrance is
  only needed while either entity lives).
- **Push side.** The card PUT's label set = `live_labels()` mapped
  through `RemoteIndex` — unchanged wire shape; Deck never sees
  membership clocks.
- **Tests.** Proptest over random add/remove sequences on both
  "devices" (logical clocks): commutativity of the per-membership
  merge, add-survival under concurrent adds, remove-needs-newer; the
  entity-death dominance pin; the migration upgrade pin.
- **ADR 0004**: dated amendment — the "Labels merge as one field
  (whole-set LWW)" paragraph and R7's doc comment gain the OR-set
  rule and its skew residue.

### D-3 — #130: resurrect-as-new-card on delete/edit contention

**Problem** (fact 5): a remote card delete during offline local
edits always wins; the local edits are lost.

**Design**: delete still wins — *unless the local task holds unsynced
user work*, in which case the content survives as a re-created card.

- **D-3a — the predicate.** `has_unsynced_work(task) :=
  max(native field clocks) > remote_seen || any reminder rule of the
  task has updated_at > remote_seen`. Field clocks cover content
  edits (an edit stamps at command time; a drained push's echo maxes
  clocks to the echo stamp, so *synced* work compares `<=`); the
  rules clause covers sidecar-lane user data. Pure, derived from
  entity state — no new persisted flag, no A1 minting needed.
- **D-3b — the policy.** In the R3 branch (pipeline §6), for a
  bound-but-absent task:

  - `has_unsynced_work` **false** ⇒ tombstone exactly as today (the
    99% case: delete a synced card remotely, it disappears locally;
    the ADR's zombie fear stays dead).
  - `has_unsynced_work` **true** ⇒ **resurrect as new card**: keep
    the task live, clear `remote`/`remote_seen`/baseline, cancel the
    task's *other* pending ops, and enqueue exactly one
    `CreateTask(task)` — the existing op **is** the recreate op the
    issue asks for (the materialized create full-sends the card
    body; a create-then-update pair would be redundant). The next
    push mints a fresh remote card; the echo rebinds.
  - No loop: the new card is a fresh remote id; a *second* remote
    delete of it while again offline-edited resurrects again — one
    new card per contention episode, each justified by unsynced
    work.
- **D-3c — notification.** A new `SystemEvent::TaskResurrected(
  TaskId)` rides the broadcast when the resurrect branch fires
  (fact 14: internal-additive). The GUI/CLI surface it as a toast/
  status line — the card "reappearing" on the Deck side is the
  policy working, and the user deserves the sentence. No interactive
  prompt anywhere: kiosk and daemon sessions must never block on a
  conflict dialog.
- **D-3d — scope edges, ruled.** The stack-cascade tombstones
  (pipeline §6b: cards inside a remotely-deleted stack) keep
  tombstone-wins even under unsynced edits — the card's position
  target is gone, its stack is dead, and resurrecting would require
  fabricating a new stack (a different policy question, recorded as
  the documented limitation). R5 (a *remote write* newer than the
  local tombstone) is untouched — that is adoption, not contention.
- **D-3e — config (⟂ F-130 sub-fork, only if wanted).** A
  `[sync] delete_contention = "resurrect" | "tombstone"` escape
  hatch is one config read at the pipeline seam; recommended
  *against* (YAGNI until a user asks; the semantic is strictly
  data-preserving), included here so the ruling can flip it cheaply.
- **Tests.** Engine-level snapshot (insta S-series style): the four
  scenes — synced-delete disappears; unsynced-edit delete
  resurrects + `CreateTask` re-enqueued + other ops cancelled +
  event emitted; resurrect-then-delete-again loops once per episode;
  cascade contention stays tombstoned. Property: a task with
  `has_unsynced_work` never reaches `deleted: true` through R3.
- **ADR 0004**: dated amendment — the §Deletion "observed remote
  absence always beats concurrent local edits" sentence gains the
  unsynced-work carve-out and the notification contract.

### D-4 — #133: skew — seam done, pins now, guard designed-not-built

**Facts** (10, 11): both clock seams are injected (F11-01 landed
2026-10-17); the remaining exposure is interleavings (i)-residual and
(ii); the audit's standing recommendation keeps any behavior change
behind the observed-symptom trigger.

- **D-4a — bookkeeping first.** Chunk S posts the verification note
  on #133 (F11-01 done, `29b1acb`; the entry's first work item
  satisfied) and the issue stays open with the trigger documented —
  closing it is the archive act, premature while the guard design
  has no home.
- **D-4b — the skew test-pack (built now).** Pure-seam tests with
  deliberately skewed stamps, no behavior change:
  - interleaving (i)-residual: a second local edit inside the echo
    skew window loses to a real-time-earlier remote write — named
    `…_currently_pins_skew_echo_shadow_residual` (AGENTS §5: reads
    as debt, per the naming rule);
  - interleaving (ii): the resurrect flip both directions (skewed
    remote write resurrects a just-deleted card; skewed local
    tombstone eats a fresh remote change) —
    `…_currently_pins_skew_resurrect_flip`;
  - seconds-tie (iii): same-second remote loses to the sub-second
    local edit — this one is *contract* (ADR 0004's tie-break), a
    plain test name, no `currently_pins`;
  - one actor-tier hermetic test demonstrating the `Clock` seam:
    a wiremock cycle with the injected fake clock set ahead/behind
    by δ, asserting the actor's stamps came from the seam (the
    audit appendix's "hermetic suites run on the real clock" no
    longer constrains skew tests).
- **D-4c — the Δ-band guard, designed to drop-in depth, not built.**
  At the two resurrect-decision comparisons — R5 in `merge_task`
  (`ts_r > clocks.deleted`) and the D-3 resurrect predicate's
  mirror — the guarded form is `ts_r > clocks.deleted + N` with
  `N = [sync] skew_band_secs` (default 2): within the band, treat as
  tie ⇒ keep-local (tombstone stands / delete holds). Costs when
  enabled: a genuine remote edit inside N seconds after a local
  delete fails to resurrect until the next write (bounded,
  self-healing); fixes: interleaving (ii)'s first direction only —
  the mirror direction (local clock behind) needs remote-age bias,
  which stays rejected (picks N blind, trades one flip for another).
  Enabled **only** when real-server symptoms match interleavings
  (i)-residual/(ii) — the trigger both the issue and the audit keep
  standing. The design text above is the complete implementation
  spec; building it is a one-comparison change plus tests plus the
  config surface.
- **Tests**: D-4b is the test chunk; D-4c's tests are specified but
  written only if/when the guard is triggered.
- **ADR 0004**: no amendment (no decision changes); the §Consequences
  skew sentence already points at the documented behavior.

### D-5 — #150: duplicate-create crash-window reconciliation

**Problem** (facts 9, 12): a crash in the POST-complete →
storage-commit window leaves the `CreateTask` op in the outbox; the
replay POSTs a second card; the pull then R1-adopts the first card as
a *new local task* — a duplicate locally and remotely. Same window
duplicates stacks; `boards publish` has the sibling W2 orphan.

**Design**: recognize our own duplicate at pull time and collapse it,
triggered only when an observed duplicate exists.

- **D-5a — substrate.** `RemoteTask` grows `created_at:
  DateTime<Utc>` (the DTO already parses it, fact 9; the mapping's
  epoch sentinel rule applies — `raw <= 0 → MIN_UTC`); `RemoteStack`
  likewise if the DTO carries it (verify at implementation; if a
  stack DTO lacks it, stacks fall back to title+window only).
- **D-5b — detection (pull-side pass, after the task-merge loop,
  before R3).** A remote card `U` headed for R1 adoption is a
  **crash-window duplicate of ours** iff all hold:
  1. some local task `T` has the same *normalized title*
     (`reconcile.rs`'s `normalized_title` — the house vocabulary,
     fact 13) and the same resolved stack;
  2. `T` carries a pending `CreateTask` op **or** took its create
     echo this cycle (the crash signature: the op survived, or its
     echo just landed);
  3. `U.created_at` and the bound card's `created_at` both fall in
     `[op.queued_at − 60s, now]` (the first POST happens shortly
     after the op queued; the tolerance covers the crash-restart
     gap; the window is deliberately generous — a false positive
     requires a *same-title, same-stack, content-equal* card created
     within a minute of our own create);
  4. content equality on the pushed shape (title, description,
     duedate, done) — the duplicate is byte-ours.
- **D-5c — survivorship: keep the older card.** The local task
  rebinds to the **older** remote card (the first POST — it owns any
  remote-side history the crash window could not have, e.g. an
  immediate comment), `remote_seen := U.last_modified`; the
  **newer** card (usually the one this cycle's echo just bound) gets
  a remote delete: enqueue the existing delete path for its ref —
  the push is 404-tolerant by construction. The R1 adoption of `U`
  is suppressed (no local duplicate row is ever created). No config:
  a duplicate we created seconds apart is never worth keeping.
- **D-5d — stacks.** Identical mechanism at the stack level
  (normalized title + board + window + create-op signature); the
  survivor keeps the older stack id, the newer gets a delete push.
- **D-5e — boards W2 (⟂ F-150b).** *Recommended: adopt-by-title
  sweep in the publish ceremony.* After `BindBoard` commits, the
  ceremony lists the connector's boards once; a second board with
  the same normalized title created inside the ceremony window is
  the orphan — bind the older (the just-bound one wins identity
  only if it is older; otherwise rebind + delete the newer through
  the client), one client call + one compare. The alternative —
  runbook-only per ADR 0010:118 — stays the documented floor if the
  ruling prefers zero ceremony surface. The orphan-board case is
  user-visible pollution (an unowned board in their Deck), which is
  why the sweep earns its place.
- **D-5f — honesty surface.** `tracing` (typed facts, rule tag R-dup)
  only: the duplicate was ours, invisible in practice; no
  `SystemEvent`, no UI churn. The Deck web UI may briefly show two
  cards; the collapse is the reconciliation working.
- **Tests.** Wiremock journey: create push succeeds → simulated crash
  (drop the landing batch) → restart → replay POST → duplicate
  injected server-side → pull reconciles (rebind older, delete push
  for newer, zero duplicate local rows). Unit tier: the detection
  predicate's four clauses pinned separately, including the
  near-miss negatives (same title different stack; same title
  outside window; content mismatch). Stack variant. Property: the
  pass is a no-op whenever no pending/just-pushed create exists
  ("trigger only on observed duplicate" is structural, not
  configured).
- **ADR 0007** gains a dated note (the W1 window is now reconciled
  at pull; the window itself remains — Deck has no idempotency
  keys); ADR 0010's W2 runbook sentence is amended if F-150b rules
  the sweep in.

### D-6 — where things live (the family on one page)

| Change | Domain | Storage | Sync actor | Pipeline | UI |
|---|---|---|---|---|---|
| baseline (#128) | `Task.remote_baseline` + `RemoteTaskBaseline` | migration 0010: `tasks.remote_baseline` | — | merge gating | — |
| OR-set labels (#129) | `label_memberships` map + `MembershipState` | migration 0011: `task_labels.state`/`ck_membership` | — | R7 rewrite, rides baseline diff | reads `live_labels()` |
| resurrect (#130) | predicate fn + `SystemEvent::TaskResurrected` | — | — | R3 branch + op re-plan | toast/status line |
| skew (#133) | — | — | — (seam done) | — (tests only) | — |
| dup-reconcile (#150) | `RemoteTask.created_at` + detection fn | — | maps `created_at` | pull pass after §5 | — |

The sync actor stays a translator: only #150 touches it (one mapping
field). Everything else is domain + pipeline + storage, per ADR
0004's "the engine executes the merge".

### D-7 — docs amendments

ADR 0004: three dated amendments (D-1 granularity, D-2 labels, D-3
resurrect carve-out), amendment-not-rewrite. ADR 0007: the W1 note
(D-5). ADR 0010: the W2 sentence, fork-dependent. ADR 0005 needs
nothing (the migrations are ordinary additive). If the follow-up
round changes test-tier vocabulary (a new docker-journey for the
reconcile collapse), `docs/testing_strategy.org` gains the tier note
in that chunk, not here.

### D-8 — testability of the family

All new logic is pure: baselines, membership maps, skewed stamps, and
`created_at` are injected data; the merge algebra stays Miri- and
mutants-friendly (the mutant targets grow: the baseline-diff gate,
the membership outcome table, the resurrect predicate, the duplicate
detection clauses — each with its zero-expected-survivors set). The
`currently_pins_` naming rule carries the tolerated limitations
(D-1d blind spot, D-4b interleavings) so they read as debt, per
AGENTS §5.

## 4. Decision record — the ruling forks

| Fork | Question | Recommendation | Rejected alternatives |
|---|---|---|---|
| **F-128** | Baseline-diff granularity: scope | **Tasks only** now; boards/stacks/labels recorded as the mechanical extension path | All entity kinds this round (2 more migrations, little observed pain); design-only/trigger-gated (the P2 audit verdict already schedules it consciously) |
| **F-129** | OR-set labels: build or hold | **Build** per-membership add-wins on top of the baseline | Keep whole-set LWW (the concurrent-add loss stays the everyday case); approximate add-wins without persisted remove-clocks (re-add flapping — rejected on correctness) |
| **F-130** | Delete-vs-edit contention policy | **Automatic resurrect + `SystemEvent`**, no prompt (kiosk/daemon must not block) | Config-gated default-on (YAGNI); keep delete-wins (the accepted loss stays the P2 the audit scheduled) |
| **F-133** | Skew: what to build now | **Test-pack + drop-in guard spec; guard stays symptom-triggered** | Build the Δ-band guard now (false-keep window with no observed symptom); close outright (the trigger has no documented home) |
| **F-150** | Duplicate resolution direction | **Auto-collapse, keep older, delete our duplicate** (cards + stacks) | Report-only (leaves local + remote pollution); defer |
| **F-150b** | Boards W2 orphan | **Adopt-by-title sweep in the publish ceremony** | Runbook-only (ADR 0010's current text — stays the floor if overruled) |

One process note for the ruling: F-128's "design-only" and F-129's
"hold" variants keep their issues open trigger-gated; every other
fork flip changes only its own chunk's interior, not the chunk map.

## 5. Risks

- **Baseline drift vs server normalization** (D-1c): any adoption
  path that forgets to refresh the baseline turns the next pull's
  normalization delta into a spurious "remote changed field" —
  benign under LWW (the value lands anyway) but noisy for the diff
  statistics; the R9 echo path is the one to watch. Mitigation: the
  proptest includes a push→echo→pull round trip.
- **The membership-map reader sweep** (D-2a) is the widest mechanical
  churn in the family (every `task.labels` reader). Mitigation: the
  accessor keeps the read shape; the sweep is mechanical, chunk L's
  own commit.
- **Resurrect pushes content the remote just lost** (D-3): the
  policy is honest — someone deleted the card while this device held
  unsynced edits, and the edit wins — but a user who *meant* the
  delete on the other client sees the card return. The notification
  is the mitigation; a config hatch exists as F-130's rejected
  alternative, cheap to add later.
- **Duplicate false positives** (D-5b): the four-clause conjunction
  is deliberately strict; the residual risk is a genuinely-twin card
  (same title, stack, content, minute) misread as ours and deleted
  remotely. Assessed acceptable: the crash signature (pending/echoed
  create) cannot exist without our own create in the window.
- **Chunk serialization** (§6): the dispatch note forbids
  parallelism; a stacked-PR dance (AGENTS §8) is available if the
  round wants review overlap — the plan recommends plain serial
  dispatch.

## 6. Implementation chunks (the follow-up round)

Five chunks, **strictly serial** (one agent at a time — the dispatch
ruling); S first (it pins the semantics D then changes), G before L
(L's remote events ride G's baseline). Sizes assume the §4
recommendations; a fork flip resizes at most its own chunk.

| Chunk | Scope | Closes | Size |
|---|---|---|---|
| **S — skew pins** | D-4b test-pack (interleavings (i)/(ii) as `currently_pins_`, (iii) as contract, one Clock-seam hermetic demo), #133 verification comment (F11-01 done) | #133 stays open with the trigger documented | S |
| **G — granularity baseline** | D-1: `RemoteTaskBaseline` + migration 0010 + merge gating + proptest + ADR 0004 amendment; the granularity-trap pin flips to the closed trap | #128 | M/L |
| **L — OR-set labels** | D-2: membership map + migration 0011 + verbs + baseline-diff events + push mapping + reader sweep + ADR amendment | #129 | L |
| **D — delete contention** | D-3: predicate + R3 branch + op re-plan + `TaskResurrected` + engine snapshots + ADR amendment | #130 | M |
| **R — duplicate reconcile** | D-5: `created_at` mapping + detection/survivorship pass + delete-duplicate push + stacks + boards-W2 (per F-150b) + wiremock crash journey + ADR 0007/0010 notes | #150 | M |

The origin issues stay open until their chunk's PR lands (house
archive rule). The implementation issues opened with this plan carry
the §4 fork flags in their bodies.

## 7. References

- Issues: #128, #129, #130, #133, #150; dispatch §14
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md`).
- ADRs 0004, 0005, 0007, 0010.
- Audit: F10-01/F11-01/F11-02
  (`.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`);
  chunk 11 §skew C1–C9 + interleavings
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-time-dst-report.md`);
  chunk 10 W1/W2 windows
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-sync-persistence-report.md`).
- Origin decisions:
  `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md`
  §7.4 (granularity trap), §12 (backlog entries).
- House design-round precedents: DR-2
  (`.artifacts/plans/2026-10-26-signal-handling-design-plan.md`),
  DR-3 (`.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`,
  landed as `eb69a3a`/`2991f65`).

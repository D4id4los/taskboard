# The Habit Events Wire Chunk — Stage-1 Plan (dispatch chunk 4)

- **Date**: 2026-11-28
- **Kind**: stage-1 implementation plan (planning agent; no production
  code in this commit).
- **Branch**: `feat/sync-habit-events-wire`, worktree
  `/home/viktoria/src/taskboard-WT-feat-sync-habit-events-wire`,
  **stacked base** `origin/feat/storage-habit-event-migration`
  (19ee5ba2 — chunk 3, PR #1564, review-passed, awaiting the
  operator's merge). The dispatcher rebases this branch onto
  `origin/main` after #1564 merges (a clean rebase — identical
  commits); nothing in this plan depends on `main` state.
- **Origin**: design
  `.artifacts/plans/2026-11-27-habit-event-log-design.md` §0 Rulings
  D, E (all A–I operator-accepted) and §4; granularity audit
  `.artifacts/reports/2026-11-27-source-of-truth-granularity-audit.md`
  §F-1 items 2–3 and §3's wire-section sweep; dispatch plan
  `.artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md`
  §1/§5; UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md` US-4/US-5
  fold halves (tokens `us4_s2`, `us5_s3`, `us5_ac2`); chunk-3 plan
  `.artifacts/plans/2026-11-28-habit-event-migration-plan.md`
  (R2 — the cross-device inflation consequence this chunk owns on
  the wire side).
- **Status**: READY FOR STAGE 2 (pending the §3 Rulings-requested
  defaults, which stage 2 adopts unless the operator overrules).

---

## 0. Recorded reality (re-located against the worktree, 19ee5ba2)

All design anchors re-verified in this worktree; the line numbers
below supersede the design's `9fc0d5ee` citations:

| Seam | Where |
|---|---|
| `APPDATA_FORMAT = 1`, the additive-never-bumps envelope law | `crates/taskboard-domain/src/appdata.rs:30–33` |
| `AppDataSections` (sections record; `habit_log` field at `:115–116`, `habit_time_entries` at `:133–141`, `unknown` flatten at `:142–144`) | `appdata.rs:100–144` |
| `HabitLogRecord` enum + the tolerant decoder with the `deleted`-key fence (`!map.contains_key("deleted")` at `:1283`) | `appdata.rs:1224–1306` |
| `Serialize for HabitLogRecord` (emit-only-when-set additive law) | `appdata.rs:1288–1320` |
| Document-level union call (the `habit_log` arm at `:1631–1640`) | `appdata.rs:1620–1650` |
| `union_habit_log` (row-clock LWW degenerating to set-union; Verbatim never displaced) | `appdata.rs:1805–1831` |
| `HABIT_LOG_SYNC_RETENTION_DAYS = 180` + `habit_log_cutoff` (local-date vocabulary, D-H8) | `appdata.rs:2624–2646` |
| `habit_log_for` (the fail-closed emission filter: habit exists locally, `board == board`, in retention) | `appdata.rs:2690–2726` |
| `lane_gc_old_habit_log` (wire-only; Verbatim and unparsable-key rows pass) | `appdata.rs:2733–2756` |
| `lane_gc_old_habit_time_entries` (the wire-only GC precedent, instant-bounded) | `appdata.rs:2524–2560` |
| `HabitState.events: BTreeMap<HabitEventId, HabitEvent>` (chunk 2/3 landed; `log`/`day_log` still present — deleted in chunk 5) | `crates/taskboard-domain/src/habit.rs:858` |
| `HabitEvent`/`HabitEventKind`/`HabitEventTombstone` (tombstone derives `Ord` over `(at, by)`; `deleted` default-absent) | `crates/taskboard-domain/src/habit_event.rs:149–225` |
| `legacy_habit_event_id` + `LEGACY_HABIT_EVENT_NAMESPACE` (chunk 3's UUIDv5 recipe — the single source this chunk reuses) | `habit_event.rs:87–143` |
| `fold_habit_log` (the `(clock, by)` tuple duel this chunk retires on the new section; the D-H6 no-habit-existence-check posture) | `crates/taskboard-domain/src/merge.rs:1503–1561` |
| Pipeline fold plumbing (`folds.fold_habit_log` called at `pipeline.rs:1026` and `:1997`; the `fold_habit_log` builder at `:1558–1570` pushing `PersistenceAction::RecordHabitLog`) | `pipeline.rs:1010–1035, 1540–1600, 1980–2010` |
| `PersistenceAction::UpsertHabitEvent` (chunk 3; apply arm at `persistence.rs:699`) | `persistence.rs:519` |
| Deck flush: import `:28–29`, builder closure `:173`, `overlay_log` `:439–445`, overlay sections `:485`, GC chain `:498–503` | `crates/taskboard-sync-nextcloud/src/flush.rs` |
| Files lanes: `habit_log` builder call `:1673–1679`, sections literal `:1706–1712`, `habit_time_entries: None` boundary (`#1363` — untouched) | `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs:1650–1730` |
| Deck wire corpus: `f21_habit_and_log_roam_through_the_sidecar_alone` (asserts the uploaded `habit-log` section), seed `repo_with_habit_flush_work` | `crates/taskboard-sync-nextcloud/tests/suite/actor_wiremock.rs:4344–4433` |
| Deck suite sections literal with `habit_log: None` | `crates/taskboard-sync-nextcloud/tests/suite/e2e_hermetic.rs:1272–1273` |
| Files connector: no habit corpus yet (only `board_cycle.rs` builds the section; its in-crate `mod tests` at `:1733`) | `crates/taskboard-sync-files-nextcloud/` |
| Domain corpus: union/GC convergence tests (`union_habit_log_is_row_lww_with_overlay_ties` at `:6221`, the base/overlay/GC convergence at `:6477–6520`, the fold round-trip at `:6643–6712`) | `appdata.rs` `mod tests` |

---

## 1. Decisions (recorded; defaults adopted)

### D-1 — the section: name, field, envelope

`AppDataSections` gains, additive and default-absent:

```rust
/// Board-level habit completion/skip facts (the event-log design §4.1,
/// #1519 chunk 4), keyed by the event UUID string. Additive:
/// documents without the key decode unchanged (the default-absent
/// law); old readers file it under `unknown` verbatim and never fold
/// it. **Never bumps `APPDATA_FORMAT`** — the envelope's own law.
#[serde(rename = "habit-events", default, skip_serializing_if = "Option::is_none")]
pub habit_events: Option<BTreeMap<String, HabitEventRecord>>,
```

No `APPDATA_FORMAT` bump. A test pins that a v1-format document
carrying a `habit-events` section round-trips byte-stably and that an
old-reader simulation (decode into a struct without the field via the
`unknown` flatten) relays it verbatim.

### D-2 — `HabitEventRecord`: the tolerant decoder grammar

Mirrors `HabitLogRecord`'s grammar exactly (the mint-strict /
decode-lenient posture, never a document failure):

```rust
pub enum HabitEventRecord {
    /// The modeled row. Identity (the map key) is the event uuid;
    /// the habit id rides the row.
    Event {
        habit: String,              // habit uuid wire form
        kind: HabitEventKind,       // the domain enum (tagged "kind")
        at: DateTime<Utc>,
        by: Option<AuthorId>,       // default-absent
        deleted: Option<HabitEventTombstone>, // default-absent
    },
    Verbatim(Value),
}
```

Decoder law (the `HabitLogRecord` device verbatim): read the raw
`Value`; accept as `Event` only if the raw object's key set is exactly
the modeled set (`habit`, `kind`, `window` — present iff the kind is
`skip`/`unskip` —, `at`, and optionally `by`, `deleted`), every value
parses, and **no unmodeled key is present**. Anything else — a future
`{"retracted": ...}` row included, an `id` key (identity is the map
key; a row carrying one is a foreign shape), an unparsable `at`,
a `window` on a `completion` — demotes to `Verbatim`. `Serialize`
emits only the set fields (`by`/`deleted` only when `Some`) — the
additive emit-only-when-set law. The `deleted` key is *modeled* here
(unlike `HabitLogRecord`, where it was the fence): the fence this
time is "any key outside the modeled set".

Reusing `HabitEventKind`'s domain serde directly (already tagged,
already tolerant of nothing — its decode failure demotes via the
outer fence) keeps one grammar for the kind, not two.

### D-3 — the section key grammar: event uuid; habit rides the row

Key = the event uuid string (`HabitEventId`'s lowercase hyphenated
`Display`; decode via `HabitEventId::parse`). The habit id rides the
row, so the GC and the fold are habit-agnostic (the fold's
habit-existence posture is per-row at *derivation* time, never at
fold/import time — D-H6 preserved). Rows keyed by foreign habits are
droppable wholesale by GC because GC keys on the row's own `at`.

### D-4 — the fold: `fold_habit_events`, union-by-uuid, the tombstone max law

In `merge.rs` (replacing `fold_habit_log` as the *events* fold; the
legacy fold survives as D-6):

```rust
pub struct HabitEventFoldChanges { pub upserts: Vec<HabitEvent> }

pub fn fold_habit_events(
    habits: &mut HabitState,
    rows: &BTreeMap<String, HabitEventRecord>,
) -> HabitEventFoldChanges
```

- Per key: `Verbatim` never folds; an unparseable key never folds
  (fail-closed, the `lane_gc_old_habit_log` posture).
- A locally-absent uuid adopts its row. A locally-present uuid is the
  *same fact* — the only competition is the tombstone slot:
  **adopt iff `row.deleted > local.deleted` under `Option<
  HabitEventTombstone>`'s derived `Ord`** (`None < Some`; `Some`
  compares `(at, then by)` — the `(at, by)` max law). Consequences,
  each pinned by a test:
  - a tombstone-less row can never displace a tombstoned one —
    **no resurrection**;
  - idempotent under re-fold (equal tombstones are identical rows);
  - **no clock arbitration anywhere**: `at` and `by` are never
    compared, `habits.log` is never consulted, no `(clock, by)` duel
    survives on this section (stage 3 greps for one).
- D-H6 posture preserved verbatim: **no habit-existence check in the
  fold** — orphan facts import (persisted via
  `PersistenceAction::UpsertHabitEvent`), are analytically inert
  (`participates()` already re-checks membership per derivation), and
  age out under GC like any row.
- Pipeline plumbing: a `folds.fold_habit_events(Option<&rows>)`
  sibling of `fold_habit_log` at `pipeline.rs`, called right after the
  legacy fold at both call sites (`:1026`, `:1997`), pushing
  `UpsertHabitEvent` per change.

### D-5 — the document union: `union_habit_events`

The `union_habit_log` shape with the arbitration swapped: per key,
absent-in-base adopts the overlay; both present with equal rows keeps
base; both present differing **only** in the tombstone slot keeps the
greater `deleted` (same derived-`Ord` max — union and fold state one
law); `Verbatim` on either side never displaces base and is never
ordered against. No clock comparison at all (the
`union_habit_log` overlay-tie leniency has no successor shape — there
is nothing to tie). Wired into the document-level union as the
`habit_events` arm beside the `habit_log` arm (`appdata.rs:1631`).

### D-6 — the legacy `habit-log` read-side shim (Ruling D1)

The read side stays, re-pointed: `merge.rs`'s legacy fold becomes
`fold_legacy_habit_log(habits, rows) -> HabitEventFoldChanges` —
it **synthesizes events and no longer writes `habits.log`**:

- `Verbatim` rows and unparsable keys never fold (posture verbatim).
- A parsed row `Entry { count, clock, by, skip }` for habit `h`,
  window date `d = key.window.as_date()`:
  - `skip == true` → one `Skip { window }` mark event,
    `id = legacy_habit_event_id(h, d, clock, None)`, `at = clock`,
    `by`;
  - `count >= 1` (and not skip) → `count` `Completion` events,
    ordinals `0..count`, `id = legacy_habit_event_id(h, d, clock,
    Some(ordinal))`, `at = clock`, `by`;
  - `count == 0`, `skip == false` → nothing (row-absence expresses
    exactly; the D24 fence's successor posture).
- The synthesized events fold into `habits.events` under the same
  D-4 union law (so a shim re-import after the events arrived via the
  `habit-events` section is idempotent — identical uuids).
- **Single-source invariant**: the shim calls chunk 3's
  `legacy_habit_event_id` through the domain function, imports the
  `LEGACY_HABIT_EVENT_NAMESPACE` — never re-declares the recipe.
  One recipe, two call sites (the sqlite remap, the wire shim).
  Pinning test: **`legacy_shim_synthesizes_the_same_ids_as_the_chunk3_remap`**
  — the same legacy row shape fed through the shim yields the exact
  ids chunk 3's remap tests assert (the shared-recipe assertion's
  wire twin; cross-chunk determinism, the dispatch acceptance's
  explicit item).
- Ruling A1 attribution rides free: the synthesized event stamps at
  the row clock, so the heatmap (chunk 5) renders migrated
  Week/Month completions on the row-clock day (US-4 AC-2's docs
  label).

### D-7 — the emission cut-over (Ruling D1's writer half)

Both flush paths stop emitting the legacy section **this chunk**:

- New builder in `appdata.rs`:
  `habit_events_for(events: &BTreeMap<HabitEventId, HabitEvent>,
  habits: &BTreeMap<HabitId, Habit>, board, today, author) ->
  BTreeMap<String, HabitEventRecord>` — the `habit_log_for`
  fail-closed filter verbatim: habit exists locally, `habit.board ==
  board`, `event.at.date_naive() >= habit_event_cutoff(today)`
  (D-8's boundary); key = the event uuid string; `by` filled
  `event.by.or(author)`.
- Deck `flush.rs`: `overlay_log` → `overlay_events`; the overlay's
  `sections.habit_log` becomes `None` (constant — the field stays
  until chunk 5 deletes it) and `habit_events` fills; the GC chain
  gains `lane_gc_old_habit_events` **and keeps**
  `lane_gc_old_habit_log` (base documents from old peers still carry
  legacy sections that must age out).
- Files `board_cycle.rs`: the `habit_log` builder call becomes
  `habit_events_for`; `habit_log: None`; `habit_events: Some(...)`
  when non-empty. **`habit_time_entries: None` is untouched** (#1363).
- `habit_log_for` itself survives (a `pub` domain function, still
  referenced by its tests) until chunk 5 deletes it with the rest of
  the legacy mirror (#1542). No connector calls it after this chunk.

Mixed-fleet semantics, stated plainly (Ruling D1's accepted cost):
new peers never emit `habit-log`; an old peer receiving a new peer's
document files `habit-events` under `unknown` and folds nothing from
it — so during the transition an old peer's habit-log view of a new
peer's *new* logging goes stale (it still sees legacy sections from
other old peers and still holds its local state; nothing corrupts,
it just stops updating). Old peers age their legacy rows out under
the 180 d GC. The fleet converges within ≤ 180 days of full upgrade.
New peers still fold a received legacy section (D-6), so old → new
direction stays fully fresh.

### D-8 — GC and retention (Ruling E1)

- `HABIT_LOG_SYNC_RETENTION_DAYS` →
  **`HABIT_EVENT_SYNC_RETENTION_DAYS: i64 = 180`** (one constant; the
  legacy GC shares it — same window, one law).
- `habit_log_cutoff(today)` → **`habit_event_cutoff(today)`** (same
  local-date vocabulary, same saturation). The boundary comparison is
  **date-based**: `event.at.date_naive() >= cutoff` — symmetric with
  the legacy window-date comparison (D-H8's vocabulary), tz-free in
  the domain crate, and it makes the legacy and events lanes age out
  in the same daily step (a legacy row and the events synthesized
  from it leave the wire the same day).
- `lane_gc_old_habit_events(document, today)` — the
  `lane_gc_old_habit_log` shape: `Verbatim` rows pass; an `Event` row
  whose key parses keeps iff `at.date_naive() >= cutoff`; a row whose
  key does not parse passes (cannot date it → cannot strip it →
  cannot forge an undeletable deletion the union could not
  resurrect). Tombstones ride their event's row — a tombstone is
  GC'd exactly when its event is; there is no separate tombstone
  store. An emptied section becomes `None`. **Wire-only**: nothing
  deletes a local event (the `lane_gc` posture verbatim).
- Retention law restated: retention ≥ the fold horizon on every peer
  (all peers run the same 180 d), so a GC'd row cannot be resurrected
  by a peer still holding it — the only rows a peer holds past the
  GC's boundary are its *local* rows, which are wire-invisible
  already (the builder filters them out with the same boundary).

### D-9 — the R2 cross-device inflation consequence, pinned as ruled

Chunk 3's R2, owned on the wire side by this chunk: identical legacy
facts with **differing clocks** on two devices (possible when a
legacy `habit-log` row won its LWW duel differently per device) mint
**distinct** v5 ids for "the same" completions; the union keeps both
and the derived count is inflated until the ≤ 180 d age-out removes
both legacy row sets. This is Ruling D1's accepted transitional
semantics, not a defect. Pinned by
`r2_legacy_clock_drift_currently_pins_count_inflation_until_age_out`
(named per AGENTS §5's debt convention — it reads as a ruled
consequence): two documents, same habit/window/ordinal, clocks
differing by one second → union keeps two events → derived count 2;
and its converging twin: after the GC boundary passes both rows, the
union holds neither (no partial age-out).

### D-10 — what does *not* happen here

No UI/CLI (chunk 5); no deletion of `habit_log_for` /
`fold_legacy_habit_log` / `HabitState.log` (chunk 5, #1542); the
`habit_time_entries: None` files boundary untouched (#1363);
`APPDATA_FORMAT` never bumps; no change to the `habits` or
`habit-time-entries` sections; no skip-posture derivation changes
(chunk 1 landed them); the tombstone can never be *removed* by a wire
row (`None` never beats `Some`) — un-tombstoning is a local
command-path fact only (Ruling H's reconcile), never a merge outcome.

---

## 2. TDD test plan (write first, in this order)

Every test typed-assertion only, no text assertions, no sleeps;
fixtures under the existing in-crate/in-suite helpers; the gate runs
per the long-command discipline. Names are contract — stage 3 looks
for them.

**In `merge.rs` tests** (the fold):

1. `us5_ac2_two_devices_same_window_log_union_keeps_both` — the S1
   headline: two events, distinct uuids, same habit/window — union
   keeps both; derived count 2 (the #1094 G-Counter question's
   dissolution pin).
2. `us5_s3_union_keeps_mark_and_log` — the UX W-9 token: a
   completion and a skip mark from different devices both survive
   the fold.
3. `fold_habit_events_tombstone_newest_at_by_wins` — two tombstones,
   `(at, by)` max wins; `None < Some`.
4. `fold_habit_events_tombstone_is_idempotent_under_refold` — fold
   the same rows twice; the second pass produces zero changes.
5. `fold_habit_events_never_resurrects_a_tombstoned_event` — a
   tombstone-less row for a locally-tombstoned uuid changes nothing.
6. `fold_habit_events_orphan_facts_import_inertly` (D-H6) — events
   for an unknown habit fold in, are persisted, and no projection
   reads them.
7. `fold_habit_events_verbatim_never_folds` and
   `fold_habit_events_unparseable_key_never_folds`.
8. `fold_habit_events_has_no_clock_arbitration` — same uuid, same
   tombstone-absence, `at`/`by` differing (a structurally impossible
   wire, constructed in the test) → no adoption, no panic: identity
   is the uuid, nothing else competes.

**In `merge.rs` tests** (the legacy shim, D-6):

9. `legacy_shim_synthesizes_the_same_ids_as_the_chunk3_remap` — the
   cross-chunk determinism pin (chunk 3's recipe assertions' wire
   twin; same row → same v5 ids).
10. `legacy_shim_count_row_mints_ordinal_events_at_row_clock` —
    count 3 → three Completion events, ordinals 0–2, `at` = row
    clock (A1).
11. `legacy_shim_skip_row_mints_one_mark_without_ordinal` and
    `legacy_shim_count0_row_mints_nothing`.
12. `legacy_shim_never_writes_habits_log` — `habits.log` unchanged
    after the shim.
13. `legacy_shim_refold_after_events_section_is_idempotent` — the
    same facts arriving via both sections collapse to one event set.

**In `appdata.rs` tests** (codec, union, builder, GC):

14. `us4_s2_verbatim_never_folds` — the UX W-10 token at the codec:
    a foreign `habit-events` row decodes `Verbatim`, survives union,
    GC, and a re-serialize round trip verbatim.
15. `habit_event_record_decoder_fences_unmodeled_keys` — the fence
    grammar: `id` present, unknown key present, `window` on
    completion, unparsable `at` → each `Verbatim`; the modeled set
    with optional keys absent → `Event`; null `by`/`deleted` behave
    as absent.
16. `habit_events_section_is_additive_default_absent` — decode a
    document without the key; `APPDATA_FORMAT` unchanged; an
    old-reader simulation relays the section verbatim through
    `unknown`.
17. `union_habit_events_tombstone_max_no_clock` — the D-5 law incl.
    the Verbatim-never-displaced arm.
18. `habit_events_for_fail_closed_filter` — foreign-board, unknown
    habit, and pre-cutoff rows never emit; `by` fills from the
    author argument.
19. `lane_gc_old_habit_events_boundary_and_fail_closed` — the E1
    boundary: at the cutoff date the row keeps, one day before it
    drops; Verbatim and unparsable-key rows pass; emptied section
    → `None`; **the document GC never touches local state**.
20. `gc_tombstone_ages_out_with_its_event` — a tombstoned row past
    the boundary leaves the document whole (tombstone + event).
21. `legacy_and_events_lanes_age_out_in_the_same_step` — a legacy
    row and its synthesized event (same clock day) both leave at the
    same GC pass.
22. The existing convergence shape re-run for the new section (the
    `:6477–6520` base/overlay/GC convergence test's `habit-events`
    twin).

**Cross-device consequence pins**:

23. `r2_legacy_clock_drift_currently_pins_count_inflation_until_age_out`
    and its converging twin (D-9).

**Connector churn**:

24. Deck `actor_wiremock.rs` `f21_habit_and_log_roam_through_the_sidecar_alone`
    — re-pointed: the uploaded document carries `habit-events` (rows
    keyed by event uuid, `habit` riding the row) and carries **no**
    `habit-log` key at all; the D-C9 fence assertions stay; the
    seed (`repo_with_habit_flush_work`) provides
    `habits.events`.
25. Deck: a received legacy `habit-log` base document flows through
    the flush union + GC chain untouched-emission-wise (new peer
    never re-emits it, the GC still bounds it) — the D-7
    keep-`lane_gc_old_habit_log` pin at the connector level.
26. Files `board_cycle.rs` in-crate corpus: the builder emits
    `habit-events` and no `habit-log`; `habit_time_entries` stays
    `None`.
27. `flush.rs` in-crate tests (`:1328` eviction shape, `:1608`
    retention constant use) re-pointed to the new names.

**Wire-safety catalogue** — every catalogue row above is one of
tests 1–23 or an explicit stage-3 construction duty (§5 stage-3
prompt); recorded outcomes in the stage-3 report:
concurrent log/log → both (T1); concurrent log vs adjust-tombstone →
tombstoned excluded, row kept (T3/T5); "tombstone vs a later
completion with the same id" → impossible by construction (ids mint
once; the test pins the tombstone-less-cannot-displace law, T5);
GC'd tombstone meeting a live peer copy → impossible within
retention (both peers GC on the same deterministic `at` boundary —
T19/T20; a peer's local rows are wire-invisible already); legacy vs
events section → v5/v7 namespaces cannot collide, identical facts
collapse by id (T9/T13); verbatim re-import → verbatim relay
(T14).

---

## 3. Rulings requested (the operator approves this chunk's merge
personally — the wire-visible facts, in plain words)

Each item names the decision this plan's stage 2 will otherwise
adopt. None blocks dispatch; the operator can overrule any item and
only the affected tests change.

1. **A new wire section appears in synced documents.** Documents
   synced between devices gain a `habit-events` section (one row per
   habit completion or skip mark). Old app versions ignore it
   safely (they carry it along untouched, understanding nothing).
   The document format version does **not** change — this is the
   project's standing rule for purely additive sections.
   *Default: adopt, per the accepted design §4.1.*
2. **Devices stop sending the old habit-log section immediately.**
   The old `habit-log` section (per-window counts) is no longer
   written by this build; it is still *read* if another (older)
   device sends one, converted on arrival into the new event shape
   using the exact same deterministic id recipe as the local
   database migration. Consequence while a mixed fleet exists: an
   old device stops seeing *new* habit completions from new devices
   until it is upgraded (its own data is unaffected and nothing
   corrupts); the stale window ends within 180 days of full upgrade.
   *Default: adopt — Ruling D1, operator-accepted 2026-11-27;
   restated here because it is the user-visible transition cost.*
3. **Known transitional double-count.** If the same old completion
   reached two devices with slightly different recorded times (a
   pre-existing quirk of the old merge), the two copies become two
   distinct events and a count may read one too high until the
   180-day age-out removes the old-format rows. Accepted as the
   ruled transitional cost; pinned by a test so it is documented
   behavior, not a hidden bug.
   *Default: adopt — chunk-3 plan risk R2 + Ruling D1.*
4. **Retention window unchanged, re-pointed.** The 180-day wire
   retention stays 180 days; it now keys on each event's own
   completion instant (calendar-date comparison, same daily step as
   the old lane) instead of the old row's window date. Deletion
   tombstones age out together with the event they stamp. Local
   history is never trimmed by any of this.
   *Default: adopt — Ruling E1.*
5. **Deletions are append-only on the wire.** A corrected/undone
   completion is a deletion stamp riding the event row (newest
   stamp wins); a device that never saw the stamp cannot resurrect
   the deleted event. There is no way to *un*-delete through the
   wire — undo of an adjust restores the pre-image locally, and the
   wire row simply carries whatever tombstone state results.
   *Default: adopt — Ruling C2's wire half.*

## 4. Risks

- **R-A — the mixed-fleet transition (R2/D-7, user-visible)**: old
  peers' habit-log views go stale for new peers' writes until
  upgrade (≤ 180 d to full convergence). Stated in §3 item 2; no
  mitigation designed (single-user fleet upgrades fast — the
  design's assumption).
- **R-B — the R2 inflation (§3 item 3)**: bounded by the legacy
  rows' 180 d age-out; the derived-count inflation cannot exceed the
  legacy row counts and never affects local data. Pinned, named as
  debt.
- **R-C — codec fence over-strictness**: the exact-key-set fence
  demotes any future additive key on an event row to `Verbatim`
  (which then never folds and eventually GCs? — **no**: Verbatim rows
  pass GC untouched, so a future version's richer rows relay forever
  on old peers, exactly the `HabitLogRecord` posture). The cost is
  one decode grammar per additive key (a future chunk widens the
  fence deliberately).
- **R-D — test churn is the diff's majority**: the Deck f21 journey
  and the domain corpus carry the legacy section shape; every such
  fixture update must cite the cut-over in its commit body, or
  stage 3 flags it.
- **R-E — stacked-base rebase**: clean by construction (chunk 4's
  commits touch none of chunk 3's lines except importing
  `legacy_habit_event_id`); the dispatcher rebases after #1564
  merges; `git diff origin/feat/storage-habit-event-migration...HEAD`
  is the review scope until then.

## 5. Stage prompts (paste-ready, verbatim)

The long-command discipline text that both prompts carry word for
word:

> **Long-command discipline** (binding; `docs/agent_dispatch.org`): a
> subagent is killed after ~600s without observable activity, so long
> silent commands kill working agents. (1) *The coverage trio never
> runs inside a subagent*: a stage-2/stage-3 agent's gate is the
> pre-commit hooks (automatic) plus
> `scripts/run-quality-gates.sh cargo nextest run --workspace`
> (flock-safe; background + poll with useful work between polls if
> needed); the dispatcher runs the full AGENTS §10 suite between
> stage 2's landing and stage 3's ship. *Flock-deferral rule: when
> the gate lock is contended and no other useful work remains, never
> block — commit all landed work, report "gate deferred: lock held
> by <holder>", and end the stage; the dispatcher runs the gate.*
> (2) *Stage 3's scope ends at PR-open*: push, open the PR, report —
> the dispatcher owns `gh pr checks --watch`, `mergeStateStatus
> CLEAN`, teardown, and at-merge issue closures. (3) *Any command
> expected over ~5 minutes runs backgrounded* with short interleaved
> polls and useful work between polls — commit all landed work
> BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4)
> **Every agent-launched cargo invocation routes through
> `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts
> restate this discipline verbatim.* **Never commit with
> `--no-verify` or any hook bypass.** **Never let a test or binary
> touch the user's real taskboard data** (isolated config/data dir
> under /var/tmp; never `~/.local/share/taskboard` or
> `~/.config/taskboard`). **The required SonarCloud quality gate
> fails a PR whose NEW code has more than 3 % duplicated lines** —
> plan test scaffolding in existing test-support modules, table-driven
> where possible. If you run `scripts/ux-coverage.sh`, pass
> `--root <your worktree>`.

### Prompt 4.2 — stage 2, IMPLEMENTATION

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the habit event-log dispatch ("wire codec + fold + both connectors", branch feat/sync-habit-events-wire).

**Stacked base**: this branch sits on origin/feat/storage-habit-event-migration (chunk 3, PR #1564 — review-passed, merge pending). Work in /home/viktoria/src/taskboard-WT-feat-sync-habit-events-wire (branch feat/sync-habit-events-wire). If the worktree is missing, recreate it with an explicit start point: git fetch origin --prune; git worktree add -b feat/sync-habit-events-wire ../taskboard-WT-feat-sync-habit-events-wire origin/feat/storage-habit-event-migration — NEVER off the stale local main, NEVER on a landed branch name. Recreate with -v2 only if the branch name has landed. Work ONLY by absolute paths inside that worktree; never modify /home/viktoria/src/taskboard or any other taskboard-WT-* directory.

**Long-command discipline** (binding; docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh — nothing bare. (5) Stage prompts restate this discipline verbatim. Never commit with --no-verify or any hook bypass. Never let a test or binary touch the user's real taskboard data (isolated config/data dir under /var/tmp; never ~/.local/share/taskboard or ~/.config/taskboard). The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines — plan test scaffolding in existing test-support modules, table-driven where possible. If you run scripts/ux-coverage.sh, pass --root <your worktree>.

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0/§4, the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§5, the UX journey plan .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md (US-4/US-5 fold halves, tokens us4_s2/us5_s3/us5_ac2), and the stage-1 plan .artifacts/plans/2026-11-28-habit-events-wire-plan.md in the worktree. The plan is normative; its §1 decisions D-1..D-10 and §2 test list (names are contract) are your spec. Any deviation is a reported deviation, never a silent one; a plan defect goes back to the dispatcher, it is not freelanced.
2. Implement exactly per the plan, TDD (each §2 test written before the code that makes it pass), every commit standalone (correct build, meaningful <type>(<module>) subject, no WIP). Suggested commit sequence: (a) the codec + section field + decoder fence tests; (b) the fold + union + legacy shim (imports chunk 3's legacy_habit_event_id — never re-declares the recipe); (c) the builder + GC + constant rename; (d) the emission cut-over in both connectors + pipeline plumbing; (e) the fixture churn. Hard rules: APPDATA_FORMAT unchanged (a bump here is a bug — the section is additive); tolerant-decoder discipline (never a decode failure; Verbatim demotion); network IO only in the two connector crates (AGENTS §2); no sleeps; typed assertions only (no text assertions); fixture corpora updated deliberately with the cut-over cited in the commit body; no quality ceilings; the R2 pin test is named exactly r2_legacy_clock_drift_currently_pins_count_inflation_until_age_out.
3. Run the fast gate per the long-command discipline (pre-commit hooks; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls; commit BEFORE waits; scripts/memory-gate.sh on every cargo call; coverage trio never runs here; flock deferral = commit + "gate deferred" + end).
4. scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate --root . — this chunk's rows (us5_s3, us4_s2 fold tests) must read FOUND; report the honest steady state.
5. No push, no PR. Never push to or merge main. Creating/commenting backlog issues is permitted and expected (AGENTS §7).
6. Backlog issues per AGENTS §7 standing authorization (search first with gh issue list --search); list numbers.

Report: per-gate results, the §2 test list with pass evidence, the R2 pin's recorded outcome, deviations, backlog issue numbers.
```

### Prompt 4.3 — stage 3, REVIEW

```text
You are the stage-3 REVIEW agent for chunk 4 of the habit event-log dispatch (branch feat/sync-habit-events-wire).

**Stacked base**: until the dispatcher says #1564 merged, review scope is git diff origin/feat/storage-habit-event-migration...HEAD in /home/viktoria/src/taskboard-WT-feat-sync-habit-events-wire; after the dispatcher's rebase notice, it is git diff origin/main...HEAD. Recreate the worktree per the stage-2 prompt's start-point rule if missing (-v2 if landed). Work ONLY by absolute paths inside the worktree; never modify /home/viktoria/src/taskboard or any other taskboard-WT-* directory.

**Long-command discipline** (binding; docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh — nothing bare. (5) Stage prompts restate this discipline verbatim. Never commit with --no-verify or any hook bypass. Never let a test or binary touch the user's real taskboard data (isolated config/data dir under /var/tmp; never ~/.local/share/taskboard or ~/.config/taskboard). The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines — plan test scaffolding in existing test-support modules, table-driven where possible. If you run scripts/ux-coverage.sh, pass --root <your worktree>.

1. Read AGENTS.md, the design §0/§4, the dispatch plan §1/§5 (the code-quality mandate applies), the UX journey plan, and the stage-1 plan .artifacts/plans/2026-11-28-habit-events-wire-plan.md; review the full branch diff.
2. Functional focus, named from the origin: (a) the union fold has NO clock arbitration anywhere — grep for any surviving (clock, by) duel on the new section; identical uuids are the same fact; (b) the S1 headline pinned: a two-device same-window log test proves both increments survive (us5_ac2/us5_s3); (c) the tombstone max law is (at, by) newest-wins and idempotent under re-fold; (d) GC is wire-only (a local row is never deleted — the lane_gc posture); the E1 cutoff boundary test exists; (e) the legacy shim's ids equal chunk 3's remap ids for the same facts (the cross-chunk determinism test present and asserting real equality, not a tautology); (f) both connectors emit the new section and no legacy section; the files habit_time_entries None boundary untouched; (g) APPDATA_FORMAT untouched; (h) unknown-section relay still honest (an old-reader simulation test present).
3. **Wire-safety review duty (try to break the fold)**: actively construct — or verify a catalogue test constructs — each of: concurrent log/log on two devices (same window); concurrent log vs adjust/tombstone; a tombstone vs a later completion carrying the same id; a GC'd tombstone meeting a peer that still holds the event; a legacy habit-log section meeting a habit-events section; verbatim re-import through a full union+GC+re-serialize round trip. Each needs a test with a recorded outcome in your report — a case you can construct that loses an event, resurrects a tombstoned event, or double-counts is a BLOCKER fixed on the branch (a new test + the fix), never a report note. The one accepted exception is the R2 clock-drift inflation, which must appear exactly as the named currently-pins test, not as an unrecorded finding.
4. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5 (typed assertions, no text assertions, deterministic by construction, one behavior one test at the lowest layer); (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crates; the fold never leaves the domain crate; the UI is untouched); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch (amend/reword, no duplicate-subject fixups).
5. Re-run the fast gate per the long-command discipline (backgrounded flock-wrapped nextest; scripts/memory-gate.sh everywhere; commit before waits; flock deferral). The coverage trio never runs in this session.
6. **UX completeness mandate**: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate --root .; execute the walkthrough protocol (docs/ux_planning.org) over the US-4 and US-5 fold halves (US-4/S2 sync cycle, US-5/S3 merge) with evidence — the four questions per step; BLOCKER/MAJOR fixed on the branch, MINOR to backlog issues.
7. Ship: rebase per the dispatcher's current base notice if instructed, push, open the PR (base main; if the stacked base is not yet merged, follow the dispatcher's stacking instruction verbatim), report the URL — no CI watch. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-sync-habit-events-wire; confirm via git worktree list. (If the dispatcher holds the PR for #1564's merge, stop at push-ready and report instead.)
8. Backlog issues per AGENTS §7 standing authorization (search first); list numbers.

Report: verdict, findings, fixes, the wire-safety catalogue with recorded outcomes, PR URL (or push-ready state), walkthrough verdict, worktree-removed confirmation. Never merge; never touch main.
```

---

## 6. Estimated diff size

- `appdata.rs`: ~+280 prod (field, record codec, union, builder, GC,
  constant rename) + ~+520 tests; `merge.rs`: ~+140 prod (events
  fold, legacy shim repoint) + ~+340 tests; `pipeline.rs`: ~+45;
- `flush.rs`: ~+50 (cut-over + GC chain) + ~+120 test churn;
  `board_cycle.rs`: ~+35 + ~+90 corpus;
- `actor_wiremock.rs` f21: ~+60 churn; `e2e_hermetic.rs`: 1 line;
  fixture JSON: none (no static `habit-log` corpus exists).
- Net ~ **+1500 / −150**, tests ≈ 65 % of it (SonarCloud 3 %
  duplication gate: table-driven fences, shared builder helpers in
  the existing test modules).

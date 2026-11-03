# [Plan] Break-rule roaming (#588, backlog round 3 chunk 1.2) — stage-1 implementation plan

- **Date**: 2026-11-03 (the round-3 dispatch table's chunk 1.2 slot).
- **Kind**: stage-1 TDD implementation plan for chunk **1.2** of the
  backlog round-3 dispatch. Zero production code in this commit; this
  file is the chunk's entire planning artefact. Implementation is
  stage 2's, review + shipping stage 3's.
- **Origin**: the round-3 dispatch plan
  `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md`
  (batch 1, chunk 1.2, branch `feat/break-rule-roaming`); the
  normative fence issue **#588** (the recipe: board-level
  `break-rules` sidecar section, dismissals wire admitting
  `break:<uuid>` keys, the proptest carve-out); the origin plan
  `.artifacts/plans/2026-10-31-respite-break-reminders-plan.md`
  (**D-15** — "device-local v1 … coherent roaming … is a
  section-recipe-sized one" — is the fence this chunk dissolves; its
  **R-2/R-4** notes are normative: the double-fire guard and the
  orphaned-suppression-row bounds are untouched here); the section
  recipe's design lineage: DR-5
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  §1/§3-D (the twice-proven new-section recipe, the board-level
  section mechanics) and DR-1's roam-set ruling
  (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`,
  2026-10-25: rule-ref dismissals roam fleet-wide; snoozes/mutes stay
  per-device; *section-level, fail-closed* asymmetry shape).
- **Normative base**: branch point = `origin/main` @ `c21c0e9c` (the
  worktree's base; verified `8221d27a` "the at/in reminder kind" is an
  ancestor of that base, so the At-shaped `RuleEntry` rows and
  **migration `0020`** are landed facts this plan reads as given).
  **Chunk gate verified** (2026-11-03): batch 1 has no predecessor
  gates; 1.2 is the **first** chunk in the domain/state+sync hard lane
  (2.1/2.2/2.3 gate on its merge, not vice versa). Worktree created
  per the lifecycle rules: `scripts/worktree-doctor.sh` exit 0 (217 G
  free, floor 25 G; one llvm-cov run skipped as active), then
  `git worktree add -b feat/break-rule-roaming
  ../taskboard-WT-feat-break-rule-roaming origin/main`.
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§7, D-1…D-16). The findings this
  planning pass contributes beyond the issue text are the **board
  scoping ruling** (D-2: break rules are board-scoped rows like habits
  — the wire never carries the board, the fold stamps it, D-H11's
  exact device), the **migration correction** (D-14: the dispatch's
  "take 0020" is stale — 0020 is landed by the At/in kind; this chunk
  takes **0021**), and the **reserved-entry home for break
  dismissals** (D-9: the per-card `dismissals` section gains a
  board-level reserved entry key `"break"`, because break rules have
  no card to hang a dismissal entry on).

## 1. Mission & binding constraints

Three moves, one lane, sharing the landed sidecar substrate:

1. **The id-family move**: `BreakRuleId` leaves the device-local
   family and joins the roaming family — the rule UUID becomes the
   wire key of a synced section, exactly as `ReminderRuleId` (D-S9)
   and `TimeEntryId`/`HabitId` (D-T10, D-C9) already are.
2. **The `break-rules` sidecar section**: a board-level section so a
   rule created on one device is available on every device bound to
   the board — the reminder-rules row grammar (LWW + derived
   tombstones) instantiated at board level (the habits/time-entries
   placement mechanics).
3. **The dismissals-wire widening**: break-ref dismissals sync, so a
   desktop dismissal silences the kiosk that shares the board — the
   DR-1 roam-set ruling's fleet-wide-suppression semantic extended to
   the break family, via a grammar widening that old peers
   fail-closed-skip by construction.

Binding constraints (every one verified against the tree at the base):

- **The evaluation semantics are untouched** (dispatch non-goal;
  origin plan R-2): the trigger fn, the crossing-occurrence key, the
  respite reset, the set-union suppression guard, and the existing
  evaluator arms keep their shapes — every respite/break pin from the
  3.5 suite must stay green *unmodified*. The only evaluation-adjacent
  deltas are flush triggers (outbox ops), which the evaluator never
  reads.
- **The fail-closed card filter is a contract** (dispatch hard rule):
  `dismissals_entry_for`'s emission filter
  (appdata.rs:1460-1489) keeps `Default ⇒ None | Adhoc ⇒ None |
  Break ⇒ None` verbatim — break dismissal rows ride the *reserved
  board-level entry*, never a card entry. Only the filter's doc
  comment changes (it currently claims break rows "can never reach the
  document" — false after this chunk; §3 D-11).
- **Recipe verbatim, not hand-rolled** (dispatch stage-3 focus): the
  section follows the reminder-rules recipe's row grammar
  (`RuleEntry`'s modeled/tombstone/verbatim tripartite shape,
  default-on-missing decode, unmodelable-demotes-never-fails) at the
  board-level placement the habits/time-entries sections already
  established (flat UUID-keyed map, no `prune_absent_cards` arm,
  board-level fold position, `FlushBoardAppData` trigger, fold-stamped
  board). Every deviation candidate the seams surfaced is recorded as
  a declined alternative in §7.
- **Old peers must not need to exist**: the widening's compat story is
  that the *old* grammar already fail-closed-skips every key it cannot
  parse (`parse_dismissal_wire_key`, appdata.rs:1024-1034 — strict
  UUID split on `|`), and unknown top-level sections relay verbatim
  (the D5 relay rule). No grammar version bump, no `APPDATA_FORMAT`
  bump, no capability flag (the DR-1 §4(a) ruling).
- **Type-driven A1 policy** (AGENTS §2): the carve-out is **typed
  family membership** — the proptest generates typed ids per family
  and asserts per-family wire fate; no string allowlist. `BreakRuleId`
  keeps its own newtype; it never borrows `ReminderRuleId`'s identity
  or grammar (D-1).
- **Dumb-client UI**: zero ui-slint behavior change (the anchors/refs
  the presentation matches are unchanged; the only ui-crate diff is a
  test-fixture literal gaining the new struct field).
- **No new dependencies**: `cargo deny` untouched; everything is
  std + chrono + serde + uuid already in the tree.
- **Non-goals (fences)**: no entry-roaming change (already coherent,
  D-T10); no respite-evaluation semantics change (R-2); no snooze or
  mute roaming (DR-1 ruling: per-device, revisit on demonstrated
  need); no `UpdateBreakRule` verb (the no-rule-edit-verbs fence
  stands — a different threshold re-creates the rule, which is a new
  UUID and a fresh clock); no settle-leg extension (dismissing a break
  occurrence never deletes the rule — the At-rule settle is a Rule-ref
  semantic); no retention/aging lane (the section is config-sized — a
  handful of small rows — unlike `time-entries`' 180-day history
  window); no `[reminders]` or `[sync]` config knob; no UI change; no
  schema change beyond D-14's one additive column.

## 2. The id-family move — D-1…D-3

### D-1 — `BreakRuleId` joins the roaming family as its own typed newtype; the Rule family's grammar is NOT the home

**Decision**: `BreakRuleId` moves (documentation + wire participation,
below) to the roaming family, keeping its **distinct newtype** and
gaining a **distinct wire-key grammar** (`break:`-prefixed keys for
dismissals; bare-UUID map keys for the rule rows themselves). It does
**not** move into the `ReminderRuleId` family or adopt the Rule
family's bare-UUID dismissal-key grammar.

Reasons, each anchored in the verified tree:

1. **The families share one macro; membership is enforcement, not
   typing.** `local_id!` (ids.rs:13-39) generates the identical struct
   shape for both families; the device-local/roaming split lives in
   (a) the doc comments and grouping in ids.rs, (b) wire
   participation, (c) the fail-closed filters and the proptest. The
   "move" is therefore: reposition the definition beside the roaming
   ids with an updated doc (ids.rs:72-76 today reads "device-local —
   never synced, the `AdhocAlarmId` precedent"), update every
   device-local doc that cites D-5 (reminders.rs:299, :458-464, :619-622,
   :633; appdata.rs:1477-1479; flush.rs:92-94), and let the wire
   grammar + tests carry the enforcement. There is no type-system
   cliff to fall off — which is exactly why the carve-out property
   (D-15) is load-bearing.
2. **The bare-UUID grammar is the old-peer fail-closed mechanism.**
   Old peers skip `break:` dismissal keys *because*
   `parse_dismissal_wire_key` strict-parses a bare UUID
   (appdata.rs:1028) and `break:…` fails that parse — the widening
   needs zero old-peer cooperation (D-10). Collapsing break keys into
   the bare grammar would make an old peer decode a break dismissal as
   a *Rule*-ref dismissal for a rule it may not have — silently wrong,
   the exact hazard the prefix exists to prevent.
3. **The typed separation keeps every filter typed.** The emission
   filter matches `ReminderRef` arms; the proptest generates per-family
   typed ids; the wire key fn is typed
   (`break_dismissal_wire_key(BreakRuleId, occurrence)`). One shared id
   type would force string-dispatch at the boundary — the AGENTS §2
   anti-pattern.

The standing consequence, recorded honestly (the DR-1 consequence
list grows by one family): break *deliveries* stay per-device facts
(the R3 two-fact split — fire receipts never roam, reminders.rs:482-486);
break *snoozes* stay per-device (DR-1 ruling); break *dismissals* and
the *rules* now roam. A kiosk without the section (pre-upgrade peer)
stays silent; a desktop dismissal now stops every upgraded peer on the
board.

### D-2 — break rules are board-scoped rows: `BreakRule.board: BoardId`, the habits device

**Decision**: `BreakRule` gains `board: BoardId` — the `Habit` device
verbatim (habit.rs; 0017's `board_id TEXT NOT NULL`; `CreateHabit`
payload carries the board; `board_flush(stored.board, …)`; `habits_for`
filters `habit.board == board`; `fold_habits` stamps adopted rows with
the document's locally-bound board — **no board rides the wire**
(D-H11)). The wire row therefore carries no board key; scoping is a
property of the document a row lives in.

**Declined alternative** (recorded so review need not re-litigate):
board-less rows replicated into *every* board's document. Declined
because (a) it is precisely the "hand-rolled variant" the dispatch's
review stage is charged to reject — no in-tree section replicates rows
across documents; (b) a board-less row cannot key its
`FlushBoardAppData` trigger (the planner would have to enqueue one op
per known board); (c) delete-propagation degenerates into
tombstone-everywhere with a stale-document re-adoption window per
board (the break rule would transiently resurrect and could fire once
before the next fold kills it). The board-scoped device has none of
these: one row, one document, one trigger key.

The multi-board semantics, recorded: a rule rides the board its
creator bound it to; devices bound to that board converge; break
*evaluation* remains global over all tracked entries (unchanged) — the
board field scopes *roaming*, never *firing*. The `breaks add` CLI
resolves the board exactly as `habits add` does (D-8).

### D-3 — the payload gains `board: Option<BoardId>` with `#[serde(default)]`; the planner rejects `None` typed

`CreateBreakRule { board: Option<BoardId>, label, threshold,
insistence }`, `#[serde(default)]` on the field. Reason: the undo
journal stores the verbatim command (journal.rs:44) and the journal
snapshot corpus must keep decoding after the field lands (the
StartTracking-`kind` additive-field precedent, origin plan D-3; a
required `BoardId` would make every pre-landing journal entry fail
PersistedState decode — the corrupt-state class). Redo safety is
already settled in the tree: `plan_redo` rejects `NotUndoable` targets
before re-planning (undo.rs:588-590), and break-rule commands are
`NotUndoable(ReminderSurface)` (undo.rs:145 arm) — an old entry can
only ever be *read*, never re-planned, so `None` is a decode ghost with
no live path. The planner still owes totality (`plan_command_is_total`):
`None` → a new typed `CommandError::BreakRuleBoardMissing` variant
(name bikesheddable at review; the shape — a unit variant in the
existing `CommandError` enum — is not). Live surfaces always send
`Some` (D-8). `DeleteBreakRule` needs no payload change (it resolves
the row and reads `row.board`); `DismissReminder` needs none (it
already resolves the Break-ref rule row for existence — command.rs:1954-1960
— and reads its board for the flush trigger, D-7).

## 3. The `break-rules` sidecar section — D-4…D-8, D-11…D-14

### D-4 — section shape: board-level flat map, the `RuleEntry` row grammar

```rust
// AppDataSections gains (serde rename "break-rules", default,
// skip_serializing_if = "Option::is_none" — recipe step 3):
/// Board-level break-rule definitions, keyed by the `BreakRuleId`
/// UUID string — the rule's remote identity (D-S9's device). Like
/// `time_entries`/`habits`, the keys are not card ids: rules outlive
/// any card, so `prune_absent_cards` must never see the section.
pub break_rules: Option<BTreeMap<String, BreakRuleEntry>>,
```

The section is a **flat** UUID-keyed map (the `habits`/`time_entries`
shape), not the nested per-card `RulesEntry { rules, unknown }` shape —
that nesting exists only because reminder-rules rows hang off *cards*;
board-level sections have no card hop. The **row enum** mirrors
`RuleEntry` (appdata.rs:400-509) field-for-field, renamed to keep the
families distinct:

```rust
pub enum BreakRuleEntry {
    /// `{ "label": str, "threshold_seconds": i64 > 0, "created":
    /// rfc3339, "clock": rfc3339, "insistence": "normal"|"alarm" }`.
    Rule {
        label: String,
        threshold: WorkThreshold,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
        insistence: Insistence,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — membership needs
    /// explicit tombstones (absence from a possibly-truncated
    /// document must never delete locally — the `RuleEntry` ruling).
    Tombstone { clock: DateTime<Utc> },
    /// A shape this version does not model: preserved verbatim,
    /// never folded (D-S2's materialization rule).
    Verbatim(Value),
}
```

Decode rules, each the recipe's rule verbatim and each pinned by test:

- `deleted: true` + parseable clock → `Tombstone`; a tombstone with an
  unparseable clock demotes to `Verbatim`.
- A modeled row requires: `label` a String, `threshold_seconds` an
  i64 the `WorkThreshold` mint accepts (**a zero/negative threshold is
  domain-inconstructible and demotes the whole row to `Verbatim`** —
  the `decode_offset` positivity ruling, appdata.rs:522-530), `created`
  and `clock` rfc3339, `insistence` **default-on-missing** to `Normal`
  (the recipe's `decode_insistence_default_on_missing`,
  appdata.rs:511-520) with present-but-unmodelable demoting the row
  (never a silent downgrade).
- Anything else → `Verbatim(raw)`; decode never fails the document.
- Emit always writes every key; `insistence` emits `text_form()`, the
  0013 stored-text convention.

### D-5 — the write clock: `clock` rides the wire; no `updated_at` field locally

The wire row carries both `created` and `clock` (recipe-verbatim shape,
the growth path stays additive), but the local `BreakRule` keeps
`created_at` as its only stamp — **no `updated_at` field is added**.
The origin plan's D-4 ruled "no `updated_at`: no rule-edit verbs, and
device-local rows never compete under LWW"; this chunk overturns the
*device-local* premise, so rows now compete — but the only verbs are
create and delete, so the write clock **is** the creation stamp:
emission sets `clock: rule.created_at`, and for every row this version
can produce `clock == created` (an adopted row re-emits the same
clock it folded — the union's tie-to-overlay resolves to identical
content; no churn). A future edit-verb chunk grows the field and the
wire already has the slot; until then a dead local field (plus a
`DEFAULT`-less `ALTER` backfill problem it would force, D-14) buys
nothing. Documented at the type: the `clock` key is competition
metadata; v1's only writer stamps creation.

### D-6 — merge: per-key embedded-clock LWW union, the `union_habits` shape

`union_break_rules(&mut merged, overlay_rows)` beside
`union_habits`/`union_time_entries` (appdata.rs:1097-1126): newer
`clock` wins, equal clocks → overlay (the local side being flushed),
`Verbatim` never ordered against a clocked row, tombstones compete by
clock (a delete after a create wins; a re-created row is a *new UUID* —
see D-8). The `union()` match gains the `break_rules` arm (the
compiler forces it). Unknown-section relay needs no code: an old peer
sees `"break-rules"` as an unknown section and carries it in `unknown`
verbatim — pinned by the existing relay property shape, extended with
a break-rules leg.

### D-7 — flush wiring: board-level overlay, derived tombstones, the lazy-gate clause

In `flush.rs` (the sync crate):

- `FlushCtx` gains `break_rules: &'a BTreeMap<BreakRuleId, BreakRule>`
  (the `entries`/`habits` field precedent, flush.rs:96-101); the
  construction sites follow the compiler.
- The board-level overlay assembly (flush.rs:347-397) gains, beside
  the habit builders:
  `break_rules_for(ctx.break_rules, ctx.board)` → the section's live
  rows (board-scoped, D-2), then
  `derive_break_rule_tombstones(base_rows, ctx.break_rules, stamp)`
  → base-present live rows missing locally become
  `Tombstone { clock: stamp }` — the `derive_rule_tombstones` shape
  (appdata.rs:1500-1516) with its truncated-view guard carried over:
  only base-*present* rows tombstone, so a narrowed foreign view can
  never delete; the stamp is the scoped op stamp (the group primary's
  `queued_at`, or `read_at` in a rebuild — the time-entries
  derivation's stamp discipline, flush.rs:353-377).
- **Deletion model**: hard-delete + base-driven tombstone derivation
  (the reminder-rules recipe), **not** the habit soft-delete model.
  Break rules have no local tombstone state, no edit clocks, and
  `remove_break_rule` already cascades ref-matched rows
  (reminders.rs:836-842); a deleted rule leaves no local row to emit
  from, so the tombstone must derive from the base document. The
  habits' "deletions roam forever" property holds identically: the
  tombstone passes through every subsequent flush untouched (the
  derivation only ever *adds* tombstones; existing ones are never
  re-stamped).
- **The D12 lazy gate** (`board_has_local_app_data`, flush.rs:121-167)
  gains the break clause beside the habit-definitions clause:
  `break_rules.values().any(|rule| rule.board == board)` — a board
  whose only sidecar content is break rules still grows its anchor.
  The signature grows the `break_rules` parameter; the compiler
  sweeps `has_flush_work` and the call sites.

### D-8 — surfaces: the planner, the flush triggers, the CLI board

- `CreateBreakRule` plans `UpsertBreakRule(row)` **+
  `board_flush(board, ids, now)`** — the D-H9 board-level trigger
  (command.rs:541-552), enqueued unconditionally (duplicate triggers
  are harmless bookkeeping; coalescing is push-side). Replaces the
  "no outbox op, the rule never leaves this machine" comment.
- `DeleteBreakRule` plans the cascade it already plans **+
  `board_flush(stored.board, …)`** (the tombstone derives at flush
  from the removal op's stamp, D-7).
- `DismissReminder`'s `Break` arm records the dismissal (unchanged)
  **and enqueues `board_flush(rule.board, …)`** — the arm already
  resolves the row for existence; the `task = None` it sets stays
  (no card-scoped op). The settle leg stays Rule-ref-only
  (command.rs:1984-1992): dismissing a break occurrence never deletes
  the rule (fence §1).
- CLI `breaks add` gains `--board <selector>` (optional;
  `resolve_board(app, flag)` — commands/mod.rs:724-760 — supplies the
  single-board default and the `AmbiguousBoard` refusal, the
  `habits add` device byte-for-byte). `breaks remove`/`list` and every
  receipt stay unchanged (`BreakRuleCreated { rule: rule.id }` is
  id-only). `route_for_ipc`: no new verbs, no lane change.

### D-11 — the fold: board-level LWW, guard-immune, board-stamped adoption

`fold_break_rules(break_rules: &mut BTreeMap<BreakRuleId, BreakRule>,
rows: &BTreeMap<String, BreakRuleEntry>, board: BoardId) ->
BreakRuleFoldChanges` in merge.rs (the `fold_habits` body with the
`fold_reminder_rules` comparison):

- Live row with `clock >= local.created_at` → overwrite
  label/threshold/insistence/created_at (doc wins pull-side ties — the
  `fold_reminder_rules` comparison, the mirror of the flush's
  tie-to-overlay); unknown UUID → adopt with `board = board` (the
  fold stamps it — **no board rides the wire**, D-H11's device);
  older → local stands.
- `Tombstone` removes a local row iff `clock > local.created_at`
  (strictly newer; a re-created rule is a new UUID, D-8); tombstones
  for unknown ids leave no trace.
- `Verbatim` never folds; an unparseable map key is not an identity
  and is skipped (kept in the document by the union's additivity).
- **Guard-immune**: no `protected` parameter — the D-S6 intent flag
  protects local *edits* during the copy-over collapse window; break
  rules have no edit verbs and no task, so no intent flag exists to
  consult (the dismissals/habits precedent).
- Return shape: `BreakRuleFoldChanges { upserts, deletes }` (the
  `RuleFoldChanges` pattern) — the pipeline turns them into the
  **existing** `PersistenceAction::UpsertBreakRule`/`DeleteBreakRule`
  (chunk 3.5's actions; no new persistence vocabulary), applying the
  same deltas to the copy-on-write memory view so batch and view
  cannot drift.

Pipeline position (pipeline.rs:958-1062): a new board-level arm in the
§5b sidecar fold, **after the habit folds** (order: per-card
rules → per-card dismissals → time-entries → habits/habit-log →
**break-rules** → break-dismissals), fed by
`document.sections.break_rules`; a `folded_break_rules`
copy-on-write slot over `current.reminders.break_rules` joins
`folded_habits`/`folded_tracking` (pipeline.rs:964-975); the actions
join the batch (pipeline.rs:1374-1377) and the folded state joins the
merged view (pipeline.rs:1427-1435). The foreign-board fence rides the
existing document consumers (a document folds only under its own
board's cycle — the `AppDataDocument.board` field is already gated by
the evidence plumbing; nothing new).

### D-14 — storage: migration **0021**, one additive column

**Numbering correction, recorded**: the dispatch's lane rule 5 says
"takes the next free number (**0020**)" — stale at this base:
`0020_reminder_rule_kind.sql` is landed (the At/in kind, `8221d27a`,
an ancestor of `origin/main` @ `c21c0e9c`). This chunk takes **0021**.
**One migration-carrying chunk in flight** (lane rule 5): chunk 1.3
(codec text-form refactor) carries none — verified at planning time;
stage 2 re-verifies at its base.

```sql
-- 0021_break_rule_board: break rules become board-scoped roaming rows
-- (#588, round 3 chunk 1.2; design D-2/D-14). Additive only (ADR 0005).
-- The backfill hands pre-0021 device-local rows to the device's sole
-- bound board: the fleet this feature has existed on (it landed
-- 2026-11-02) is single-board by deployment. A zero-board device's
-- rows keep '' and fail the codec's UUID parse (trusted-written
-- local rows: a corrupt id is a corrupted database, the 0008 rule) —
-- recorded leniency, not a silent path: such rows can only exist on
-- a device that never bound a board, which cannot have flushed them.
ALTER TABLE break_rules ADD COLUMN board_id TEXT NOT NULL DEFAULT '';
UPDATE break_rules
   SET board_id = (SELECT board_id FROM boards LIMIT 1)
 WHERE board_id = '';
```

(Exact `boards` id column per `0001_init`; stage 2 pins the query
against the schema.) Codec: `board_id` hyphenated-UUID TEXT, the 0008
conventions unchanged; `load()` reads it; `UpsertBreakRule` writes it.
No other storage delta: the suppression tables already carry `break:`
text-form refs in their TEXT columns (the origin plan D-13), the
section is appdata (no sqlite involvement), and the
`ReminderState`/`PersistedState` nesting mirrors wholesale.

## 4. The dismissals-wire widening — D-9, D-10, D-12

### D-9 — the reserved board-level entry: break dismissal rows live in the `dismissals` section under the reserved key `"break"`

The `dismissals` section is per-card (`BTreeMap<String,
DismissalsEntry>` keyed by decimal card id), and a break rule has no
card — so the widening needs a home *inside* the section's map for a
board-level entry. **Decision**: the reserved, never-a-card entry key
`"break"` (constant `BREAK_DISMISSALS_ENTRY_KEY: &str = "break"`
beside the reserved section names, appdata.rs:60-72):

- Deck card ids are decimal strings; `"break"` can never collide.
- The per-card fold loop (pipeline.rs:978-1021) looks up only card
  keys of present tasks — the reserved entry is invisible to it by
  construction (the time-entries/habits invisibility fact).
- The row keys inside the entry are the widened grammar:
  `"break:<uuid>|<rfc3339 occurrence>"` — `break_dismissal_wire_key(
  BreakRuleId, occurrence)` / `parse_break_dismissal_wire_key(&str)
  -> Option<(BreakRuleId, DateTime<Utc>)>`, the
  `dismissal_wire_key` pair (appdata.rs:1012-1034) with the prefix
  arm. Injective for the same reason (a UUID is hex+dashes, rfc3339 is
  digits/punct, and the `break:` prefix is fixed text).
- The row payload is the unchanged `DismissalEntry`
  (`{ "dismissed_at": rfc3339 }`; dismissals are permanent for their
  occurrence — no tombstones, the D-9 discipline).

**Old-peer compat story (the dispatch's "grammar version/compat"
question, resolved)**: no version bump and no old-peer change are
needed, because the old grammar was already total-fail-closed over
unknown key shapes — an old peer's `parse_dismissal_wire_key` returns
`None` on every `break:`-prefixed key (strict UUID parse fails on the
prefix; pinned today at appdata.rs:3020-3022, which gains the explicit
`"break:…"` rejection line), its fold ignores the row while the codec
relays it verbatim, and its per-card union merges the reserved entry
additively (the section union is keyed by map key —
`union_dismissals_entry` merges the inner rows of whatever keys both
sides carry). The one real old-peer hazard is `prune_absent_cards`: an
old peer's flush strips the reserved entry (its `keep` filters to
present card keys, appdata.rs:1335-1338). Recorded as the mixed-fleet
one-version tolerance (R-1): self-healing, because every round peer
re-emits break dismissals from local state at its next flush (the
overlay builder), and a round peer's `prune_absent_cards` gains the
preserve arm (D-12).

### D-10 — fold and emission for break dismissals

- **Fold** `fold_break_dismissals(dismissals: &mut
  BTreeSet<ReminderDismissal>, entry: &DismissalsEntry) -> Vec<ReminderDismissal>`
  (merge.rs, the `fold_reminder_dismissals` body with the break parse):
  additive, first-write-wins on the `(reminder, occurrence)` key, the
  rows become `ReminderRef::Break(id)` dismissals, **guard-immune**, and
  **no rule-existence check** — exactly like the Rule-ref fold imports
  rows for rules it has never seen (the D-9 fold-order property: a rule
  adopted in a later cycle is immediately suppressed by the
  already-folded dismissal; an orphan fact is analytically inert).
  Verbatim rows never fold; unparseable keys are ignored without being
  dropped. Pipeline position: the board-level arm immediately after
  the break-rules fold, reading
  `document.sections.dismissals.as_ref().and_then(|e|
  e.get(BREAK_DISMISSALS_ENTRY_KEY))`; folded rows join the existing
  `RecordReminderDismissal` action lane (pipeline.rs:1012-1020's
  collection, moved beside the new arm). The **withdraw direction is
  free**: `withdrawals_for` is ref-generic
  (actor.rs:1347-1360) — a folded break dismissal withdraws the local
  toast with no new code, and the existing
  cross-site derivation ("covers all three dismissal write sites")
  covers the fourth.
- **Emission** `break_dismissals_entry_for(dismissals, break_rules,
  board) -> DismissalsEntry`: the local set's **Break-ref rows only**,
  each keyed by `break_dismissal_wire_key`, filtered to rules that
  exist locally **with `rule.board == board`** — the orphan + scoping
  filter mirroring `dismissals_entry_for`'s `rules.get(&id).filter(…
  rule.task == task)` (appdata.rs:1468-1469). (The orphan filter is
  belt-and-braces — `remove_break_rule` cascades break dismissals
  locally — but it keeps the property shape identical to the recipe and
  scopes the lane to the flushing board.) The flush inserts the entry
  under the reserved key only when non-empty (the
  `overlay_dismissals` gate, flush.rs:343-345).
- **The per-card filter is untouched in code**: `dismissals_entry_for`
  keeps `Break(_) => None` (§1 contract). Its **doc comment changes**
  (a stale comment is a review finding): the fail-closed arm's reason
  becomes "break dismissals ride the board-level reserved entry
  (`BREAK_DISMISSALS_ENTRY_KEY`), never a card entry — a card entry is
  scoped to the card's task's rules" (the D-15 device-local claim is
  false as of this chunk and must not survive in prose).
- Doc comments updated for the same reason: `ReminderRef::Break`
  (reminders.rs:299), `BreakRule` (:458-464), `ReminderState.dismissals`
  (:619-622), `ReminderState.break_rules` (:633),
  `ReminderDismissal` (:504-511 — "Only `Rule`-ref rows ever
  serialize" becomes "only `Rule`- and `Break`-ref rows"),
  `DismissalsEntry`'s key doc (appdata.rs:560-564).

### D-12 — prune: the preserve arm

`prune_absent_cards` (appdata.rs:1313-1340) keeps stripping the three
card-keyed sections by present-card set; the dismissals `keep` gains
exactly one carve-out: an entry whose key is
`BREAK_DISMISSALS_ENTRY_KEY` survives regardless of `present_cards`
(and the section survives if the reserved entry alone remains — the
`!kept.is_empty()` gate already covers it). `break_rules` gains **no
arm** (board-level section — the load-bearing D-T10 fact, now in its
third application). Pinned by test both ways: the reserved entry
survives a prune whose present-cards exclude it; a `break-rules`
section is invisible to the prune for arbitrary present-card sets.

## 5. The proptest carve-out — D-15

**Decision**: the existing `no_local_ids_reach_the_dismissals_wire`
(appdata.rs:4038-4099) is **extended in place** — one property, both
builders, the break leg's assertion target moves. It stays typed
(per-family generated ids, per-family assertions; no string
allowlist), minimal (one builder + a handful of assertions), and
commented with the constraint (the dispatch hard rule):

- **Card-entry leg (unchanged assertions)**: for arbitrary local
  dismissal sets — Default/Adhoc/Break rows included — the card
  builder's wire content contains no local `TaskId`, no
  `AdhocAlarmId`, no `BreakRuleId`, no `default:`/`alarm:`/`break:`
  key form. (The existing property's body; the fail-closed contract
  pin, byte-identical.)
- **Reserved-entry leg (the carve-out)**: the reserved-entry builder
  over the same generated set emits `break:<uuid>|…` keys for exactly
  the generated Break-ref rows whose rule exists on the board — the
  positive assertion is **typed** (`wire.contains(&format!("break:{}|",
  break_rule.as_uuid()))` iff `with_break_row`), and the negative
  assertions hold: no `TaskId`/`AdhocAlarmId` uuid text, no
  `default:`/`alarm:` prefix anywhere in the reserved entry's wire.
- **Conjunction leg**: the full serialized dismissals section (card
  entries ∪ reserved entry) never carries a device-local family's
  identity — the property's title claim, now true per-family instead
  of per-element.

Stage 3's flip-proof procedure ("temporarily widen the carve-out and
confirm it fails, then restore — record that you did"): widen = let
the reserved-entry builder emit a `Default`-ref row (or drop the
break-family allowance) → the conjunction/negative assertions fail →
restore. The mutation is one match arm in the test, not in production
code.

The unit pins beside it: `parse_dismissal_wire_key` rejects
`"break:…|…"` (the line added to appdata.rs:3020-3022);
`parse_break_dismissal_wire_key` accepts the emitted key and rejects
bare-UUID/`default:`/`alarm:`/malformed keys; a round-trip
`break_dismissal_wire_key` → parse → same `(id, occurrence)`.

## 6. Test catalogue (TDD-first: each phase opens with its failing tests)

**Phase N.1 — domain: the id-family move, the `BreakRule.board` field,
the section grammar**

- `break_rule_id_is_documented_roaming` — no; doc positioning is not
  testable. The testable face of D-1 is the wire grammar below.
- `create_break_rule_requires_the_board` — `None` board → the typed
  rejection (structural `matches!`, no text).
- `create_break_rule_plans_the_board_flush` /
  `delete_break_rule_plans_the_board_flush` /
  `dismiss_break_occurrence_plans_the_board_flush` — the outbox op
  assertions (structural: `EnqueueOp(FlushBoardAppData(board))` in
  the planned batch; board = the payload's / the row's).
- Section grammar pins (appdata.rs tests): `modeled_break_rule_row_roundtrips`
  / `deleted_row_decodes_tombstone` / `missing_insistence_defaults_normal`
  / `unknown_insistence_demotes_the_row` /
  `zero_or_negative_threshold_demotes_the_row` /
  `non_object_row_demotes_verbatim`; emission pin
  `break_rules_for_scopes_to_the_board_and_emits_every_key`;
  tombstone-derivation pins (`derive_break_rule_tombstones`:
  base-present-live-missing-locally tombstones at the given stamp;
  tombstones pass through; verbatim never ordered; base-absent never
  tombstones — the truncated-view guard pin).
- Union pin: `break_rules_union_lww_laws` (the
  `dismissal_union_lww_laws` shape — newer clock wins, ties → overlay,
  idempotence) + the unknown-section relay leg
  (`break_rules_section_survives_an_old_peer_relay`: a document
  carrying `break-rules` unioned against an overlay that has no
  `break_rules` field keeps the section verbatim).
- Planner/board pins: `create_break_rule_stamps_the_board_on_the_row`
  (the minted row carries the payload board).
- Existing-suite extension: the persistence `plan_command_is_total`
  strategy gains `board: Some(_)` payload construction (mechanical).

**Phase N.2 — domain: the dismissals widening, the folds, the carve-out**

- Wire-key pins (§5's unit pins) + `break_dismissals_entry_for` pins:
  emits only Break-ref rows; orphan/foreign-board rules filtered
  (`rule.board == board`); non-empty gate.
- Fold pins: `fold_break_rules_adopts_stamps_and_lwws` (adopt with the
  folding board; newer-clock overwrite; older stands; tie → doc) /
  `fold_break_rules_tombstone_removes_iff_strictly_newer` /
  `fold_break_rules_verbatim_and_bad_keys_never_fold` /
  `fold_break_dismissals_imports_first_write_wins` /
  `fold_break_dismissals_ignores_unparseable_keys_without_dropping`
  (the verbatim relay) / `orphan_break_dismissals_fold_and_are_inert`
  (import without rule existence; the later-adopted rule is suppressed
  — the D-9 fold-order property, break flavor).
- Pipeline pins: `break_rules_section_folds_into_engine_state` +
  `reserved_break_dismissals_entry_folds_into_engine_state`
  (`apply_sync_report` with a fabricated document evidence; the merged
  view's `break_rules`/`dismissals` gain the rows; the batch carries
  the matching `UpsertBreakRule`/`RecordReminderDismissal` actions —
  batch and view same deltas); `card_fold_loop_never_reads_the_reserved_entry`
  (a document whose card entries and reserved entry overlap
  adversarially folds each under its own grammar).
- Prune pins (D-12): `reserved_break_entry_survives_the_prune` /
  `break_rules_section_is_invisible_to_the_prune`.
- **The carve-out property** (§5) — extended in place; the phase opens
  with it red (the reserved-entry builder does not exist yet).
- Evaluation-untouched sweep: the full 3.5 break/reminder suite re-runs
  green with **zero modifications** (the dispatch's "respite pins
  green unmodified" — asserted by review, enabled here by the
  no-semantic-delta design).

**Phase N.3 — storage: migration 0021 + codec**

- `pre_0021_database_upgrades_and_backfills_the_board` — a pre-0021
  database (the 0008-recipe fixture shape with
  `break_rules` rows lacking `board_id`) opens; rules load with the
  backfilled board; a zero-board fixture documents the recorded
  leniency (rows skipped at load — structural, typed).
- Codec roundtrips: break-rule row with board; the existing
  `break:`-ref suppression-table roundtrips stay green (unchanged).
- `apply_actions` upsert carries the column; one transaction unchanged.

**Phase N.4 — sync surfaces, app flag, journeys, gates**

- **Tier-1 wiremock pins** (actor_wiremock.rs, the
  `repo_with_*_flush_work` module shape): a flush with a board-scoped
  break rule uploads a document whose `sections.break-rules` carries
  the rule row (+ a derived tombstone when the base doc holds a
  deleted rule — the full ladder, the `repo_with_habit_flush_work`
  template); the uploaded `dismissals` section carries the reserved
  `"break"` entry with `break:`-keyed rows; a prune-across-flush pin
  (the reserved entry survives when present-cards exclude it).
  Fold-side wiremock: a pulled document carrying `break-rules` + the
  reserved entry lands in the engine state (the `mount_pull` +
  `eventually` shape).
- **Engine-snapshot roaming journey** (e2e_hermetic.rs, the `i*`
  shape): boot engine + sync actor over a wiremock serving a document
  with a `break-rules` row; `eventually` the published
  `AppState.reminders.break_rules` contains the rule **with the
  fold-stamped board**; a second journey pulls the reserved dismissals
  entry and asserts the break occurrence stops firing
  (`due_firings`/`active_notifications` drop it) and the withdrawal
  fires (the notifier channel's `Withdraw` message, the
  `withdrawals_for` ref-generic path observed end to end).
- **Cross-device dismissal journey** (domain-level, two-stage): device
  A's state (rule + fired + dismissed occurrence) → `break_dismissals_entry_for`
  + flush-shaped document → device B's `apply_sync_report` → B's
  break pass no longer lists the occurrence at the next tick
  (deterministic: fixed entries + crossing instant, no sleeps — the
  origin plan §8 fixture discipline). This is the acceptance line
  "a break dismissal on one device silences the break on another".
- CLI: `breaks add --board <sel>` → typed receipt; `--board` with an
  unknown id → the typed lookup refusal; ambiguous multi-board
  default → `AmbiguousBoard` (structural exits/outcomes, the
  `cli_habits.rs` shape, hermetic tempdir boots).
- `CHANGELOG.md` Unreleased entry (cross-device behavior change —
  stage 2).
- Gates: the full AGENTS §10 sequence **through
  `scripts/run-quality-gates.sh`** (the flock wrapper; an llvm-cov run
  was observed active at planning time — expect to queue). `cargo fmt
  --check`, clippy `-D warnings`, nextest, doc tests, `cargo deny`
  (untouched), the three-command coverage sequence with the
  quarantine step, trailing `rm -rf target/llvm-cov-target`.

## 7. Decision record

| ID | Question | Decision | Status |
|---|---|---|---|
| D-1 | Id-family home for `BreakRuleId` | **Own typed newtype in the roaming family, own wire-key grammar** (`break:` dismissal keys; bare-UUID rule-row keys). NOT the Rule family's grammar (the bare-UUID parse *is* the old-peer fail-closed mechanism; merging families would decode break dismissals as Rule refs on old peers and blur the typed filters). The move = ids.rs doc/position + device-local doc updates + wire participation + the proptest carve-out (families share one macro; membership is enforcement, not typing) | **decided (this plan's central call)** |
| D-2 | Row scoping | **Board-scoped rows, the habits device**: `BreakRule.board: BoardId`; no board on the wire (fold stamps it, D-H11); emission/fold/lazy-gate board-filtered. Board-less rows replicated to every document declined (hand-rolled variant; no trigger key; tombstone-everywhere resurrection window) | **decided** |
| D-3 | Payload shape | `CreateBreakRule { board: Option<BoardId>, … }` with `#[serde(default)]` (journal decode totality; redo never re-plans NotUndoable — `None` is a decode ghost); planner rejects `None` with a new typed `CommandError` variant; `DeleteBreakRule`/`DismissReminder` payloads unchanged | recorded |
| D-4 | Section shape | Board-level flat `BTreeMap<String, BreakRuleEntry>` (the habits/time-entries placement) over the **`RuleEntry` row grammar verbatim** (modeled/tombstone/verbatim; default-on-missing insistence; positivity-demotes for thresholds; decode never fails the document); serde name `"break-rules"` | recorded |
| D-5 | Write clock | Wire row carries `created` + `clock` (recipe shape, growth path additive); local row keeps `created_at` only — v1's only verbs are create/delete, so `clock == created` for every row this version writes; no `updated_at` field | recorded |
| D-6 | Union | Per-key embedded-clock LWW (`union_habits` shape): newer clock wins, ties → overlay, verbatim never ordered; unknown-section relay is free (D5 rule) | recorded |
| D-7 | Flush wiring | `FlushCtx.break_rules`; board-level overlay beside the habit builders; **hard-delete + base-driven tombstone derivation** (the `derive_rule_tombstones` shape + truncated-view guard), not the habit soft-delete model (no local tombstone state exists); lazy gate gains the break clause | recorded |
| D-8 | Surfaces | Create/Delete plan `board_flush(rule.board)` (the D-H9 op); `DismissReminder`'s Break arm plans it off the resolved row; settle leg stays Rule-only; CLI `breaks add --board` via `resolve_board` (single-board default, `AmbiguousBoard` refusal); receipts/lane unchanged | recorded |
| D-9 | Break-dismissal wire home | The `dismissals` section gains the **reserved board-level entry key `"break"`** (`BREAK_DISMISSALS_ENTRY_KEY`; never a card id), rows keyed `"break:<uuid>|<rfc3339>"`; old peers fail-closed-skip by construction (strict bare-UUID parse) and relay verbatim; their prune strips the entry (one-version tolerance, R-1) | **decided** |
| D-10 | Fold + emission | `fold_break_dismissals`: additive, first-write-wins, guard-immune, **no rule-existence check** (inert orphans; later-adopted rules suppressed — the D-9 fold-order property); emission `break_dismissals_entry_for` filters to existing same-board rules; withdraw direction free (ref-generic) | recorded |
| D-11 | Per-card filter + docs | `dismissals_entry_for`'s `Break ⇒ None` arm stays (contract); its comment and the five device-local doc claims updated — the D-15 prose dies with this chunk, the arm lives on with a new reason | recorded |
| D-12 | Prune | `prune_absent_cards`: dismissals `keep` preserves the reserved key; `break_rules` gains no arm (board-level invisibility) | recorded |
| D-13 | Fold position | §5b board-level arm after the habit folds (per-card rules → per-card dismissals → time-entries → habits/habit-log → break-rules → break-dismissals); `BreakRuleFoldChanges { upserts, deletes }` into the **existing** `Upsert/DeleteBreakRule` actions; copy-on-write slot; guard-immune | recorded |
| D-14 | Migration | **0021** (correction recorded: the dispatch's "0020" is landed by the At/in kind): `ALTER TABLE break_rules ADD COLUMN board_id TEXT NOT NULL DEFAULT ''` + single-board backfill `UPDATE`; zero-board rows skip at load (recorded leniency, trusted-written strictness); one migration-carrying chunk in flight (1.3 carries none — verified) | **decided, with the numbering correction recorded** |
| D-15 | Proptest carve-out | **Extend `no_local_ids_reach_the_dismissals_wire` in place**: card-entry leg unchanged; reserved-entry leg typed-positive for the Break family + typed-negative for `Default`/`Adhoc`; conjunction over the whole section. Family membership decides; no string allowlist; constraint commented at the property; stage-3 flip-proof = one test-local match arm | **decided** |
| D-16 | Fences held | No evaluation-semantics change (R-2); no entry-roaming change; no snooze/mute roaming (DR-1); no `UpdateBreakRule`; no settle-leg extension; no retention lane (config-sized section); no config knobs; no UI behavior change; no new deps | recorded |

## 8. Phasing & commit plan (one branch, N.1 → N.4 in order)

| Phase | Delivers | Commit shape |
|---|---|---|
| N.1 | Id-family docs, `BreakRule.board`, payload + planner + flush triggers, section grammar + union + emission + tombstone derivation | `feat(domain): break rules join the roaming id family` |
| N.2 | Reserved dismissals entry, wire keys, folds, pipeline arms, prune arm, the carve-out property | `feat(domain): the dismissals wire admits break keys` |
| N.3 | Migration 0021, codec, load, upgrade pin | `feat(storage): break rules gain their board column` |
| N.4 | FlushCtx/lazy-gate/overlay wiring, wiremock + e2e journeys, `--board` flag, CHANGELOG, gates | `feat(sync): break rules and their dismissals roam` (+ fixups, none with duplicate subjects — rebase-merge hygiene) |

TDD order inside each phase: pins first (red), then the production
change (green), then the property/roundtrip sweep. N.1 lands the
payload/planner changes and the section grammar together so the
compile-forced sweeps happen once; the wiremock/e2e pins land in N.4
against the finished flush wiring.

## 9. Risks & mitigations

- **R-1 — mixed-fleet reserved-entry stripping**: an old peer's flush
  prunes the `"break"` entry from the shared document (its
  `prune_absent_cards` knows only card keys). Bounded, self-healing
  (every round peer re-emits from local state; the union's additivity
  never erases a round peer's overlay), identical in kind to the
  stale-device re-emit tolerance every additive section already
  records (D-T13/R-3 of the origin plan). Worst case: a dismissal
  synced before an old peer's flush is re-synced after its next
  round-peer flush; suppression is idempotent.
- **R-2 — codec-file collision with chunk 1.3** (in flight, same
  batch: `refactor/storage-codec-text-form` collapses the storage
  codec to the delegation pattern; this chunk adds one column to the
  break-rules codec rows). Different families of lines (1.3: text-form
  delegation; 1.2: the break-rules board column), same files. The
  dispatch judged them parallel-safe; if both are in flight, the later
  branch rebases (the AGENTS §8 dance; no semantic merge of the two
  refactors). Stage 2 checks `origin/main` before writing.
- **R-3 — backfill arbitrariness**: a pre-0021 multi-board device's
  existing rules adopt `LIMIT 1`'s board. Accepted, recorded at the
  migration: the feature is one day old, the fleet is single-board by
  deployment, and the remedy is one `breaks add`. Never a silent
  runtime path — the query runs once, in the migration.
- **R-4 — insta/journal corpus churn**: `BreakRule` grows a field and
  `CreateBreakRule` grows a payload field; persisted dumps and fixture
  literals regenerate mechanically (reminders.rs/command.rs/persistence.rs/
  journal.rs/ui presentation.rs test fixtures; ~a dozen snapshot
  files). Never hand-waved as "expected diff" — updated in the phase
  that lands the field.
- **R-5 — double flush triggers** (a dismissal racing a delete, or a
  repeat command): the board op is content-free and coalesces
  push-side (the D-H9 doc); duplicate ops are harmless bookkeeping by
  construction — pinned once in N.1, not defended per-site.
- **R-6 — the `clock == created` v1 identity**: a hand-edited or
  future-version document whose `clock` diverges from `created` folds
  by `clock` but stores `created_at` locally; the local re-emission
  then loses the tie against the document's newer row — stable, no
  churn, no corruption (the doc row keeps winning until a genuine
  newer write). Recorded here so review classifies it as the designed
  tolerance, not a bug; the growth path (an `updated_at` field + no
  wire change) is one additive field when edit verbs arrive.

## 10. Size, handoff, coordination

- **Estimated diff**: ≈ 1.0–1.3k lines including tests — domain ~640
  (appdata ~300 incl. the extended property, merge ~140, pipeline ~50,
  command/messages/ids/undo ~120, reminders doc churn ~30), storage
  ~120 (0021.sql + codec + upgrade pin), sync ~200 (flush wiring ~60,
  wiremock/e2e pins ~140), app ~50 (`--board` flag + resolution),
  fixture/snapshot churn ~a dozen files. **M** — consistent with the
  dispatch table's sizing.
- **Dependencies**: none outside the workspace (deny untouched).
  Gates run under the flock wrapper.
- **File map** (stage 2: zero diffs outside it): `ids.rs`,
  `reminders.rs`, `messages.rs`, `command.rs`, `appdata.rs`,
  `merge.rs`, `pipeline.rs`, `lib.rs` (domain); `migrations/0021_break_rule_board.sql`,
  `repo.rs`, storage roundtrip/migration tests; `flush.rs` + its
  `FlushCtx` construction site(s) + `actor_wiremock.rs` +
  `e2e_hermetic.rs` (sync); `commands/breaks.rs` (app);
  `presentation.rs` (ui — fixture literal only); `CHANGELOG.md`;
  insta/journal fixture regeneration.
- **Handoff**: stage 2 implements N.1–N.4 on `feat/break-rule-roaming`
  in `../taskboard-WT-feat-break-rule-roaming`; stage 3 reviews (the
  dispatch's functional focus: recipe verbatim, fail-closed pins
  actually constrain, the flip-proof recorded, respite pins green
  unmodified), runs the gates, opens the PR `feat(sync): break rules
  and their dismissals roam` closing #588, and removes the worktree
  as its final act (doctor rules; `--force` per the lifecycle doc).
  **No push without the user's instruction during planning** — this
  commit stays local.

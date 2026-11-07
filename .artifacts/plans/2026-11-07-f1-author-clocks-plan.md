# [Plan] F1 — Author-Tagged Clocks (DR-9 chunk 1, issue #684)

- **Date**: 2026-11-07
- **Kind**: TDD implementation plan — chunk 1 of the boards-over-files
  dispatch. Branch `feat/f1-author-clocks` off `main` @ `fd72cadd`.
- **Origin**: the accepted DR-9 round
  `.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`
  Part A (D-BF1–D-BF2), ruling Q2 (binding), §5 test hooks, §6 risks;
  issue #684; the backbone report §7.3
  (`.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  — ties between devices are a convergence bug, not a fairness bug).
- **Counters re-verified at base** (2026-11-07, `fd72cadd`): next free
  ADR **0022** (`docs/adr/` tops out at 0021), next free migration
  **0023** (`migrations/` tops out at `0022_reflection_entries.sql`).
- **Status**: PLANNED — this plan; implementation follows TDD order §9.

## 0. Scope

Domain-wide author tagging of every roaming clock family, exactly as
D-BF1/D-BF2 fixed it: parallel author structs, the `(clock, author)`
duel order, additive clock text forms + migration 0023, the sidecar
`by` field, the convergence law proptested, ADR 0022.

**Non-goals** (from #684): boards/chunks/routing anything (chunks
3–5), the settings/proofs slice (chunk 2), Deck behavior changes
(authors ride neutrally; Deck-stamped data never gains an author),
per-device attribution features (#494 — the seam only), reflection/
habit storage migrations (device-local families keep their authorless
clocks; see §2.5).

## 1. Verified anchors (against `main` @ `fd72cadd`)

| Fact | Anchor |
|---|---|
| `TaskClocks` :385 (13 fields incl. `kind`'s serde default), `StackClocks` :426, `LabelClocks` :437, `BoardClocks` :525 — plain `DateTime<Utc>`, `Copy`, serde | domain `entities.rs` |
| Deck-native duel predicate `remote_wins(remote_ts, field_clock) = remote_ts > field_clock` (ties → local) :113 | domain `merge.rs` |
| Sidecar folds duel at `clock >=` / `>` per family: `fold_task_app_data` :607, `fold_reminder_rules` :771, `fold_reminder_dismissals` :864, `fold_break_rules` :925, `fold_break_dismissals` :1007, `fold_time_entries` :1068, `fold_habits` :1169, `fold_habit_log` :1281 (latter three via `updated_at`/`clock` comparisons) | domain `merge.rs` |
| The planner stamps via `all_task_clocks(now)` :3201 and ~40 per-field `clocks.<f> = now` sites; `plan_command` :846 signature `(app, outbox, command, ids, defaults, day, now)`; called from `undo.rs` :70/:571/:632 and `habit.rs` :788; export-level callers re-import through `taskboard-state` and `taskboard-ui-slint::router` | domain `command.rs` |
| `AuthorId` — strict-parse `Uuid` newtype, derives `Ord`, wire-strict serde, "the fold's Q2 slot … minted only by the future author-tagging wave" :72-77 | sync-files `ids.rs` |
| The fold order `(rank, clock, author, content)` with the `None < Some` author leg :17-19/:108-128 — already landed, consumes whatever the planner stamps | sync-files `fold.rs` |
| `DeviceIdentity { device_id: DeviceId, created_at }` — the `device.json` payload; path resolution app-side, file IO the transport crate's | sync-files `identity.rs`; app `bootstrap.rs` :171-191 |
| Storage clock columns `ck_*` TEXT, RFC 3339, decoded by the shared `decode::clock` helper (`DateTime::from_str` semantics) :284; rows assembled in `repo.rs` :443-531 (stack/label/board), :661-665 | storage `repo.rs` |
| Entity tables carry `ck_<field> TEXT NOT NULL DEFAULT '<epoch>'` per clock (the 0002 migration is the pattern) | storage `migrations/` |
| The sidecar is domain serde JSON sections; row types are decode-tolerant plain data | domain `appdata.rs` |
| The journal wire codec already carries the fold's author slot end to end (B-min) | sync-files `document.rs` |

## 2. Design

### 2.1 `AuthorId`'s canonical home: `taskboard-domain/src/ids.rs`

The domain is the shared-vocabulary crate and the dependency arrow
points `sync-files → domain`, so the type moves **down**: the struct
(newtype over `Uuid`, strict-parse mint, `Ord`, string serde, doc
comment) relocates verbatim to `taskboard-domain/src/ids.rs`;
`taskboard-sync-files` re-exports it
(`pub use taskboard_domain::ids::AuthorId;`) so every existing
`crate::ids::AuthorId` path — `fold.rs`, the wire row, the journal
codec — keeps compiling unchanged. `DeviceId` stays in sync-files
(it is substrate-path vocabulary, not domain vocabulary). Domain
gains no new dependency: `uuid` is already a workspace-pinned domain
dep. The sync-files `ids.rs` doc comments drop their "v1 never
constructs one" pin — chunk 1 is the minting wave; the *only* minting
sites are the app bootstrap handoff (§2.2) and tests.

### 2.2 The app-layer device-identity handoff

**Problem**: the planner must stamp `Some(author)` "whenever a device
identity exists" (D-BF1), but today `device.json` is resolved inside
the *files-sync gate* (`bootstrap.rs` :171) — a device with no
files-sync account has no identity, and the state actor has no access
to one either way.

**Decision — boot-time identity, independent of sync config**:

1. App bootstrap (early, before the state actor spawns) resolves the
   fixed home `<data>/taskboard/device.json` (D-SF4's path,
   unchanged): load-or-create via sync-files' identity module
   (create-once semantics preserved — a device that later configures
   files sync adopts the same identity; the gate's resolution is
   replaced by a lookup of the already-resolved identity, never a
   second mint). Cloned-disk detection stays where it is (the
   transport's own-path guard; unchanged).
2. The resolved `DeviceId` is handed to the state actor at spawn as a
   plain spawn parameter (an mpsc-commanded actor has no other input
   seam; config is the wrong lane — identity is minted state, not
   configuration). The actor stores it and threads it into every
   `plan_command`/planner call.
3. **The planner seam**: `plan_command` gains one parameter,
   `author: Option<AuthorId>`, after `now`. All internal stamping
   (`all_task_clocks`, per-field sites, `habit.rs`/`undo.rs` entry
   points) threads it. Headless tests pass `None`; the production
   app passes `Some(device_id)`. `Option` at the boundary keeps the
   "identity may not exist yet" state expressible (early-boot window,
   tests) without a second planner variant; the fold's `None < Some`
   leg makes an authorless stamped row honest rather than corrupt.

### 2.3 The four author structs (D-BF1, exact shapes)

Parallel siblings in `entities.rs`, one `Option<AuthorId>` per clock,
same field order, same serde posture (including `kind`'s decode
default — `#[serde(default)]` → `None`, mirroring `kind_clock_default`):

```rust
pub struct TaskAuthors  { pub title: Option<AuthorId>, pub description: Option<AuthorId>,
                          pub duedate: Option<AuthorId>, pub scheduled: Option<AuthorId>,
                          pub done: Option<AuthorId>,   pub position: Option<AuthorId>,
                          pub effort: Option<AuthorId>, pub importance: Option<AuthorId>,
                          pub urgency: Option<AuthorId>, pub recur: Option<AuthorId>,
                          #[serde(default)] pub kind: Option<AuthorId>,
                          pub archived: Option<AuthorId>, pub deleted: Option<AuthorId> }
pub struct StackAuthors { pub title: Option<AuthorId>, pub order: Option<AuthorId>,
                          pub deleted: Option<AuthorId> }
pub struct LabelAuthors { pub title: Option<AuthorId>, pub color: Option<AuthorId>,
                          pub deleted: Option<AuthorId> }
pub struct BoardAuthors { pub title: Option<AuthorId>, pub color: Option<AuthorId>,
                          pub archived: Option<AuthorId>, pub deleted: Option<AuthorId> }
```

`#[serde(default)]` on every field (not just `kind`) so old state
snapshots and journal pre-images decode authorless; the structs are
`Copy` and derive the same set as their clocks. `Task`/`Stack`/
`Label`/`Board` each gain an `authors: XAuthors` field — additive;
every existing exhaustive match on the clock structs keeps compiling
because the clock structs themselves do not change shape.

**The paired-write rule**: a clock and its author must move together.
Sites write both (`updated.clocks.title = now; updated.authors.title
= author;`), and §4's invariant test T-INV-1 fails the build's tests
if any production path advances a clock without advancing (or
deliberately clearing) its author slot. A terse domain helper per
family, `fn stamp(clock: &mut DateTime<Utc>, author_slot: &mut
Option<AuthorId>, now: DateTime<Utc>, author: Option<AuthorId>)`,
is the preferred spelling at multi-line sites; it exists so the pair
cannot drift, not as validation (no validate-and-forget — plain
assignment).

**Adoption stamping rules** (recorded here so the merge edits are
mechanical):

- *Local planner edits*: `author = Some(own device)` (§2.2).
- *Remote adoption from the Deck lane* (`adopt_remote_task`,
  `merge_*`, `adopt_board_if_newer`, tombstones stamped from remote
  observations): clock to the remote `lastModified`, author to
  **`None`** — the Deck lane has no author identity; `None` is the
  honest "authored off-device by an authorless lane" and keeps
  `None < Some` meaningful (a later equal-clock local edit wins).
- *Push-echo adoption* (`adopt_*_after_push`, `rebind_*`,
  `series_survivor_merge`): authors are **carried through
  unchanged** — the echo is our own write coming back, authorship
  does not change. Where an echo advances a clock (`updated_at`
  bumps), the author stays whatever it was.
- *The local document materialisation* (app's `PublishedJournal`
  state-derived doc) and *the engine's journal import arm*: author
  rides the fold's existing slot — no new logic, only the stamp
  source changes (§2.2).

### 2.4 The duel order change, site by site (D-BF2)

The order everywhere becomes **`(clock, author)` per field** with the
entity id as the never-reached residual. Concretely:

1. **Deck-native merges** (`merge_task` :422, `merge_stack` :1785,
   `merge_label` :1955, `merge_board` :2119 + the adopt family):
   incoming remote rows are authorless, so the generalized predicate
   `remote_beats(remote_ts, remote_author: Option<AuthorId>, local_clock,
   local_author)` reduces to today's `remote_ts > field_clock` for
   every input the Deck lane can produce — behavior is **pinned
   unchanged** (test T-DUEL-0), but the comparison sites are rewritten
   in the generalized form so chunk 5's files-lane reports (which fold
   through the *same* merge machinery per D-BF9) inherit the author
   leg for free. Authors flow through the merge outputs per §2.3's
   adoption rules.
2. **Sidecar document folds** (the seven/five `fold_*` families in
   §1's third anchor row) — the real convergence-bug sites: two
   devices' documents duel on clock alone today (`>=` ties→remote in
   some, `>` ties→local in others; the inconsistency is fine while
   ties are cross-device-fairness — it is not fine when the tie is a
   bug, per the backbone §7.3). Each fold's comparison becomes the
   tuple `(clock, author)`:
   - `Some(a) > None` at equal clocks (a B-full-authored row outranks
     an authorless one — the fold.rs precedent, now domain-wide);
   - `Some(a) vs Some(b)`, `a ≠ b`: total by `AuthorId`'s `Ord` —
     deterministic on every reader, which *is* the fix;
   - `None vs None` at equal clocks: each fold keeps its **current**
     tie direction (documented at the fold; the existing duel suite
     pins it). No fairness re-litigation inside this chunk.
3. **The engine's journal import arm** (state crate): already folds
   through the landed fold order for the wire, but the *state-side
   adoption* of a winning row into `AppState` must not lose the
   author — the import writes `authors` alongside every clock it
   adopts. Same paired-write rule, same invariant test.
4. **Membership duels** (`merge_membership` :166, `t >= r`): out of
   scope — memberships carry one clock, not a clock family; the
   `(t, author)` generalization lands with chunk 5's files-lane work
   where membership rows first gain a `by`. Recorded as out-of-scope
   here, not forgotten (§8).

### 2.5 Storage: the clock text-form extension + migration 0023

**Text-form grammar** (the stored `ck_*` TEXT columns; the shared
vocabulary every entity clock column speaks):

```
clock_text   := rfc3339 [ "#" author ]
rfc3339      := (unchanged RFC 3339 / chrono FromStr form)
author       := lowercase-hyphenated UUID (AuthorId::Display)
```

- **Encode**: `None` author → bare RFC 3339, **byte-identical to
  today** (every existing row and golden stays stable). `Some(a)` →
  `<rfc3339>#<a>`.
- **Decode** (new shared helper beside `decode::clock`, e.g.
  `decode::clock_with_author(text) -> (DateTime<Utc>, Option<AuthorId>)`):
  no `#` → `(clock, None)` — old rows decode authorless, the
  tolerant path; exactly one `#` → split, left half via the unchanged
  `decode::clock`, right half via `AuthorId::parse`; **any other
  shape** (two `#`, empty suffix, non-UUID suffix) →
  `RepositoryError::Corrupted` — strict after the split, fail-closed
  like the fold's row demotion rule (silently de-authoring would
  change fold rank). `decode::clock` itself stays for the
  non-family clock columns (journal stamps, habit logs, reflection)
  — those columns never carry the suffix, and their decoder refusing
  `#`-forms is the cheap tripwire that keeps the two vocabularies
  from blurring.
- **Migration 0023** (`0023_clock_author_columns.sql`): `ALTER TABLE
  … ADD COLUMN` pairs — one nullable `au_<field> TEXT` per clock
  column: `tasks` ×13 (`au_title`, `au_description`, `au_duedate`,
  `au_scheduled`, `au_done`, `au_position`, `au_effort`,
  `au_importance`, `au_urgency`, `au_recur`, `au_kind`, `au_archived`,
  `au_deleted`), `stacks` ×3, `labels` ×3, `boards` ×4 — 23 columns.
  NULL = "no author" (the backfill; no data rewrite — the clock
  columns themselves are *not* migrated, the suffix lives in the
  author column, not inside the clock text).

  **Schema-shape decision (recorded)**: D-BF2's "author suffix on the
  stored form" is realized as the `au_` *column* whose text value is
  the bare author UUID, not as `#` embedded in the `ck_` cell — the
  round's "clock text forms extend additively (an optional author
  suffix on the stored form)" names the *vocabulary* (one grammar for
  every serialized clock+author pair, so documents, journals, and
  debug dumps speak one form), while the columnar split is the
  storage realization of the same grammar (NULL ↔ absent suffix are
  the same state, as `RemoteTaskBaseline`'s NULL-column precedent).
  The `decode::clock_with_author` helper is the grammar's single
  parse point; the storage layer composes it as
  `(decode::clock(ck), au.map(parse).transpose())`.

- **Codec read/write**: row assembly sites (§1 :443-531, :661-665)
  read `(ck, au)` pairs into the new `authors` structs (NULL →
  `None`); writes emit the `au_` column from the struct. Old-DB → new
  codec roundtrip must be lossless in both directions (T-STORE-1/2).

**What deliberately does not migrate**: the reflection/habit/journal
clock columns (device-local families, ADR 0021 — no author concept),
the sidecar (JSON, serde-default tolerant, no migration), the journal
wire (already carries the author slot).

### 2.6 The sidecar `by` wire shape

Every sidecar row type that participates in a §2.4-2 fold gains an
optional, serde-`default` field `by: Option<AuthorId>` — JSON key
`"by"`, absent on every pre-F1 document (tolerant decode), written
only when `Some`. Row types (from `appdata.rs`, matching the fold
families): reminder-rule rows, reminder-dismissal rows, break-rule
rows, break-dismissal rows, time-entry rows, habit-fact rows, and the
task app-data rows that duel inside `fold_task_app_data`. The planner
fills `by` where it fills the row's clock. **Old-reader relay**: a
pre-F1 device decodes the row (unknown `by` ignored), re-publishes it
`by`-less at its own re-stamp — content and clock survive, the fold's
content leg still converges; the T-SIDE-2 test simulates this
verbatim relay.

### 2.7 ADR 0022 (outline, house style per 0021)

`docs/adr/0022-boards-over-files-contract-f1-author-clocks.org` —
title: *"ADR 0022: The Boards-over-Files Contract — Partition, Chunk
Lifecycle, the Author-Tagged Clock Order, the Widened Vocabulary"*.
Sections: header block (Date/Status: Accepted-lands-with-#684/
Depends on: ADR 0004, 0020 / Carriers: #684, #678, the
`nextcloud-files-sync` milestone); *Context* (the convergence bug:
ties between devices are a convergence bug not a fairness bug — the
backbone §7.3 argument; why author tagging precedes boards riding);
*Decision* (the four-part contract, quoted normatively: partition-only
routing (D-BF5), the board chunk/snapshot lifecycle (D-BF6/D-BF7), the
total order `(rank, clock, author, content)` / per-field `(clock,
author)` with `None < Some` (D-BF1/D-BF2), the connector-tagged
remote vocabulary direction (D-BF4, detailed in chunk 3's ADR
amendment)); *Consequences* (Deck lanes carry authors neutrally;
authorless rows keep today's ranks; the migration surface is one-time;
per-device attribution (#494) inherits the seam without new
machinery). Chunk 1 lands the ADR with the F1 parts normative and the
parts C–D sections marked as landed-by-later-chunks (the ADR is the
contract, not the implementation schedule).

## 3. The convergence law (the chunk's tripwire)

> **Equal-clocks-different-authors law**: for any two devices `a ≠
> b`, any entity field, any two distinct contents `x ≠ y` stamped at
> the *same* clock by `a` and `b` respectively, every reader that
> folds the two facts in either arrival order — and any superset of
> peers relaying them — adopts exactly one of `(x, a)` or `(y, b)`,
> the same one everywhere: `max_by((clock, author), …)` is total and
> permutation-invariant.

Implementation targets the law at three layers: the domain folds
(§2.4-2), the storage roundtrip (an author must survive the disk
intact or the law breaks across restarts), and the engine's journal
import (the end-to-end reader). T-PROP-1/2 below are its direct
encodings; the existing sync-files fold laws stay green untouched.

## 4. Test catalogue (TDD order §9)

**Domain — planner stamping**
- T-STAMP-1 (unit): `plan_command` with `Some(a)` stamps every clock
  it advances with `authors.<f> == Some(a)`; with `None`, authorless.
- T-STAMP-2 (unit): the create verbs (`CreateTask`, `CaptureTask`,
  create-stack/label/board) mint full authored clock sets.
- T-INV-1 (proptest, the paired-write invariant): for arbitrary
  command sequences, any field whose clock differs from its previous
  value has its author slot advanced-or-cleared in the same plan —
  no clock moves alone.

**Domain — duel order**
- T-DUEL-0 (pin): authorless Deck duels behave bit-identically to
  pre-F1 (`merge_task`/`merge_stack`/`merge_label`/`merge_board` tie
  directions unchanged); runs against the pre-existing duel suite.
- T-DUEL-1 (unit per family): equal clocks, incoming `Some(a)` vs
  local `None` → incoming wins; reverse → local wins.
- T-DUEL-2 (proptest): the §3 law over each sidecar fold family —
  two authors, equal clocks, distinct contents, both arrival orders
  → identical winners.
- T-DUEL-3 (unit): `None vs None` ties keep each fold's pinned
  direction (documents the deliberate no-fairness-change).
- T-ADOPT-1 (unit): remote adoption stamps author `None`; push-echo
  adoption preserves the existing author.

**Storage**
- T-STORE-1 (roundtrip): authored entity → store → load → authors
  identical; authorless entity roundtrips as all-`None`.
- T-STORE-2 (migration): a pre-0023 database (fixture with the 0022
  schema) migrates and loads; all authors decode `None`; clocks
  byte-stable.
- T-STORE-3 (decode grammar table): bare RFC 3339 → `(clock, None)`;
  `#<uuid>` → Some; `#garbage`, double-`#`, empty suffix →
  `Corrupted` (typed, no text assertions).
- T-STORE-4 (unit): non-family clock columns reject `#`-forms as
  `Corrupted` (the vocabulary tripwire).

**Sidecar**
- T-SIDE-1 (serde roundtrip): row with `by` serializes/decodes;
  `by`-absent decodes `None`; old-format golden files decode
  unchanged.
- T-SIDE-2 (old-reader relay simulation): decode-with-`by` as a
  pre-F1 reader (ignore unknown field), re-serialize, fold both
  documents → converged, no panic, content preserved.

**End-to-end**
- T-PROP-1 (the headline, per §5 of the round): two-engine journey —
  two engines, skewed-equal stamps, different edits to the same
  fields, exchanged through the journal wire in both orders →
  identical `AppState` on both (insta snapshot or structural eq),
  on every reader.
- T-JOURNEY-1: app-level — bootstrap mints `device.json` once (no
  files-sync configured), the state actor stamps `Some(device_id)`;
  a second boot reuses the identity.

**ADR/arc**: no test; the ADR lands in the same PR.

## 5. Risks & mitigations

- **The stamping surface is wide** (~40 per-field sites in
  `command.rs` + undo/habit re-entries): the paired-write helper +
  T-INV-1 make drift a test failure, not a review hope.
- **`plan_command` signature churn** reaches `taskboard-state`,
  `taskboard-ui-slint::router`, and domain callers (`undo`, `habit`):
  mechanical, single-PR; the parameter is trailing so call-site
  edits are uniform.
- **Authorless/`None < Some` rank surprises** when old and new
  devices mix: T-DUEL-0/T-DUEL-3 pin today's authorless behavior
  bit-for-bit; the `None < Some` forward choice was pinned by the
  landed fold and is *reused*, not invented here.
- **Migration surface** (23 columns × codec sites): fixture-based
  migration test T-STORE-2 plus the no-quality-ceiling rule (a
  roundtrip may not lose authors; garbage input may fail loudly).
- **Scope creep into membership/`remote_seen` duels**: fenced in
  §2.4-4; chunk 5's territory.
- **`kind`'s serde-default precedent**: `TaskAuthors::kind` mirrors
  it (`#[serde(default)]` → `None`); the default *clock* pairs with
  a default *absent* author — consistent, pinned by T-STORE-1's
  pre-kind-payload leg if the journal fixture covers it.

## 6. Estimated diff

| Area | Est. lines |
|---|---|
| domain `ids.rs` (move in) + `entities.rs` (4 structs + entity fields) | ~250 |
| domain `command.rs`/`undo.rs`/`habit.rs` (planner seam + stamp sites) | ~400 |
| domain `merge.rs` (generalized duel order + adoption author rules) | ~250 |
| domain `appdata.rs` (`by` fields) | ~80 |
| storage `repo.rs` codec + migration 0023 + fixture | ~250 |
| sync-files re-export + doc-comment unpinning | ~20 |
| app bootstrap identity lift + state actor threading | ~120 |
| tests (§4 catalogue) | ~900–1200 |
| ADR 0022 + plan/docs | ~150 |
| **Total** | **~2400–2700** |

## 7. Commit strategy

One coherent PR (`feat/f1-author-clocks`), commits in TDD order:
domain types + planner seam (+ T-STAMP/T-INV) → duel order (+ duels
suite) → storage (+ T-STORE) → sidecar (+ T-SIDE) → app/state
threading (+ T-JOURNEY) → ADR 0022. Every commit green per §10's
gates; `docs(plan)` for this plan first.

## 8. Out-of-scope discoveries → issues (standing authorization)

Filed during implementation if confirmed: membership-duel author
generalization (chunk 5 dependency, §2.4-4); `remote_seen` author
carriage; any fold whose `None vs None` tie direction proves
load-bearing for fairness documentation.

## 9. TDD implementation order

1. Move `AuthorId` into domain + the four `XAuthors` structs +
   entity fields; red→green T-STAMP/T-INV by threading the planner
   seam.
2. The duel order generalization in `merge.rs` (+ T-DUEL-*,
   T-ADOPT-1), sidecar `by` (+ T-SIDE-*).
3. Storage grammar helper + migration 0023 + codec (+ T-STORE-*).
4. App bootstrap identity lift + state actor threading + engine
   import author carriage (+ T-JOURNEY-1, T-PROP-1).
5. ADR 0022; full quality-gate run; PR.

## 10. References

- DR-9 round Part A + §5/§6; issue #684; the dispatch plan
  `.artifacts/plans/2026-11-07-boards-over-files-implementation-dispatch-plan.md`.
- Backbone report §5.5/§7.3 (`by` on every field; the convergence-bug
  framing); fold.rs's pinned `None < Some` doc comment.
- ADR 0004 (per-field LWW), ADR 0020 (substrate), ADR 0021 (house
  style + the device-local fence this chunk respects).

# [Plan] Reflection storage & journal CLI/export — DR-7 chunk 2 (#637) — stage-1 TDD implementation plan

- **Date**: 2026-11-06
- **Kind**: stage-1 implementation plan (dispatch chunk 2, branch
  `feat/reflection-storage-cli`) — no code in this commit.
- **Origins**: the DR-7 design round §3 Parts G–H (D-J11–D-J13) + §5
  (`.artifacts/plans/2026-11-05-journal-review-design-round-plan.md`,
  normative); issue #637; the landed chunk-1 plan
  `.artifacts/plans/2026-11-05-reflection-domain-plan.md` and **ADR
  0021** (both on `main` — the entity shapes below are read from the
  landed code, not the round doc's sketches); the dispatch plan §3
  (`.artifacts/plans/2026-11-05-v0.2-completion-dispatch-plan.md`).
- **Base**: `origin/main` @ `81c6f585` (chunk 1 landed; verified
  2026-11-06 — `origin/feat/reflection-domain` deleted, the reflection
  domain/ADR commits in `main`'s log).
- **Mission**: migration **0022** `reflection_entries` + the storage
  hydration (dead write arm → compile-checked macros, load codec), the
  `taskboard journal` verb family over the timers two-lane recipe, and
  the `export --what journal` leg. **This merge unblocks
  nextcloud-files-sync #631** — the PR body flags it.

## 1. Mission & binding constraints

- **Vocabulary (D-J1, ADR 0021 decision 1)**: the user-facing verb is
  `journal`; every code symbol is `Reflection*`. No new `Journal*`
  symbol anywhere (the domain crate's
  `tests/vocabulary_guards.rs` scans the domain; stage 3's diff grep
  is the second net for the app crate). The table is
  `reflection_entries`, never `journal_entries`.
- **The storage ruling (D-J13)**: exactly one action
  (`UpsertReflectionEntry`, already landed) — chunk 2 adds **no** new
  `PersistenceAction` variant; it *hydrates* the landed one (ADR 0021
  consequences: "chunk 1 lands the `UpsertReflectionEntry` action and
  a runtime-checked dead write arm … until chunk 2's migration 0022
  converts it to compile-checked macros and adds the load codec").
  Delete stays an upsert (tombstone-in-place) — there is no
  delete-from-storage code path to write.
- **Memory ≡ disk by construction**: every field the entity carries
  (tombstones included) round-trips through the table; the whole-state
  roundtrip proptest gains the reflection arm so the load/write codec
  cannot drift apart silently.
- **Export posture (D-T15 via D-J12)**: raw payload on stdout
  regardless of `--json`; the IPC relay carries the bytes verbatim;
  bare `export` (no `--what`) stays **byte-compatible** with today.
- **Two-lane recipe (D-J11)**: timers/track shape verbatim — one-shot
  wrappers via `with_core` + session `*_on` seams + one `collapse()`
  definition site; typed receipts in `output.rs`; **values read back
  from the published snapshot, never an echo of the input**.
- **Hermetic tests only** (AGENTS §5): no sleeps, typed outcomes, no
  text assertions (golden export fixtures are *data*, not labels),
  fixed stamps or injected calendars.
- **No `UpdateAppData`, no sync lane**: the family stays device-local
  (chunk 1 pinned it in the engine; nothing in this chunk may enqueue
  an outbox op).
- **Standalone commits** (rebase-merge hygiene): every commit builds
  green and carries a meaningful subject — the migration file, the
  macro conversion, and the refreshed `.sqlx/` cache are **one
  commit** (compile-time coupling, §2 fact 12).

## 2. Verified seam facts (do not re-derive; verified 2026-11-06 against `81c6f585`)

1. **Migrations**: `crates/taskboard-storage-sqlite/migrations/` holds
   `0001_init.sql` … `0021_break_rule_board.sql` — **0022 is the next
   free number**, exactly as D-J13 predicted (re-verified at base;
   ADR 0021's numbering note concurs).
2. **The landed entity** (`taskboard-domain/src/reflection.rs`):
   `ReflectionEntry { id: ReflectionId, cadence: ReviewCadence, day:
   NaiveDate, window_start: DateTime<Utc>, window_end:
   DateTime<Utc>, text: String, attributes: ReflectionAttributes,
   created_at, updated_at: DateTime<Utc>, deleted: bool }`;
   `ReflectionAttributes { mood/energy: Option<Rating>, flags:
   BTreeSet<HealthFlag>, extra_flags: BTreeSet<String> }`;
   `ReviewCadence::{Day,Week,Month}` with `parse_text_form`/
   `text_form` ("day"/"week"/"month"); `Rating` (1..=5 A1 mint,
   strict serde decode); `HealthFlag` (nine ruled kebab names,
   `try_new`/`parse_text_form`/`text_form`); `ReflectionState {
   entries: BTreeMap<ReflectionId, ReflectionEntry> }` with
   `live_entries()` the one live-read seam; `ReflectionEntry::is_live()`.
3. **The landed commands** (`messages.rs:516–586`):
   `RecordReflection { cadence, day, window: (DateTime<Utc>,
   DateTime<Utc>), text, mood: Option<u8>, energy: Option<u8>, flags:
   BTreeSet<HealthFlag> }`; `UpdateReflection { id, text:
   Option<String>, mood/energy: Option<Option<u8>> (double-Option
   serde), flags_add, flags_remove: BTreeSet<HealthFlag> }`;
   `DeleteReflection { id }`. **No window/cadence edit slots** — an
   entry's window is immutable after mint (chunk 1's payload ruling).
4. **The landed outcomes/errors** (`command.rs`):
   `CommandOutcome::{ReflectionRecorded{entry}, ReflectionUpdated{entry},
   ReflectionDeleted{id}}`; `CommandError::{UnknownReflection(id),
   InvalidReviewWindow, InvalidRating}` (all already mapped to kebab
   kinds in the app's `domain_error_kind`: `unknown-reflection`,
   `invalid-review-window`, `invalid-rating`). `NoOp` covers: empty
   record, all-unchanged edit, repeat delete, tombstoned-target edit.
5. **The landed dead write arm** (`storage-sqlite/src/repo.rs:1728–
   1782`): buffers `PersistenceAction::UpsertReflectionEntry` into
   `reflection_upserts` (partition at `repo.rs:1346`), writes via a
   runtime-checked `sqlx::query` against `reflection_entries` — the
   R-11 comment names #637 as the hydrating chunk. **Its bind list is
   the column mapping** (uuid text, cadence text form, day
   `to_string`, RFC-3339 stamps, mood/energy `Rating::get` as
   nullable ints, flags as sorted JSON array text, extra_flags as
   JSON array text, `deleted` bool).
6. **The landed load default** (`repo.rs:1183`): `load_inner` sets
   `reflection: ReflectionState::default()` with the chunk-2 comment.
7. **Storage patterns**: per-table `query_as!` row structs
   (`HabitRow` & co., `repo.rs:44–225`) + `decode_*` fns
   (`decode_habit`, `repo.rs:607`) returning
   `Result<(Id, Entity), RepositoryError>` with `Corrupted` on every
   malformed cell; codec text fns in `codec.rs`
   (`uuid_to_text`/`uuid_from_text`, enum `*_to_text`/`*_from_text`);
   upserts are compile-checked `sqlx::query!` macros inside the
   batch transaction.
8. **Migration tests** (`storage-sqlite/src/connect.rs`):
   `reopening_applies_no_new_migrations` pins `applied == 21`
   (**grows to 22**); `migration_0017_upgrades_a_pre_habit_database`
   is the upgrade-test recipe (raw-connect, run shipped migrations,
   DROP the table + delete the `_sqlx_migrations` row, reopen, assert
   the empty fold, then write + reopen-roundtrip).
9. **Storage test harnesses**: `tests/common/mod.rs::
   actions_from_state` builds the whole-state apply batch — **it has
   no reflection arm yet**; `tests/roundtrip.rs` proptests the
   whole state; `tests/journal_storage.rs` is the targeted-family
   file precedent for a new `reflection_storage.rs`.
10. **The two-lane recipe** (`commands/timers.rs`, 396 lines):
    one-shot `add/cancel/list` wrapping `tasks::with_core` around
    `add_on/cancel_on/list_on`; receipts projected from the outcome +
    the post-execute snapshot (`add_on` expects the minted row in the
    alarms map); `cancel_on` projects the **pre-execute** row;
    `resolve_alarm` = exact uuid → unique ≥8-char lowercased uuid
    prefix → label pass, typed `UnknownAdhocAlarm`/`AmbiguousAdhocAlarm`
    with candidate lists; `collapse(TimersCmd) -> Option<Invocation>`;
    `timers_command` (one-shot, `run_core` + `render_text_*`) and
    `timers_line` (session, `emit_result`). `track.rs` adds:
    `resolve_time_entry` (the no-label-pass shape — exact uuid →
    ≥8-char prefix → typed miss), `ExportQuery` (flag bag, no
    collapse), `export_on` filtering over the snapshot, `export_command`
    /`export_line` writing **raw bytes to `io.out`** and routing
    failures through `report_to` (D-TC10).
11. **Dispatch & routing** (`commands/mod.rs`): `route_for_ipc` pins
    `Timers/Track/Habits/Export/Find` → `IpcRoute::IpcOrLocal` (the
    D-TC1 "the daemon owns the rows" argument); the one-shot `run`
    match and the session `dispatch_line` match each carry one arm per
    family (`mod.rs:1239–1276`, `mod.rs:1665–1700`). The IPC server
    relays **raw argv words** (`ipc/protocol.rs`) and captures the
    dispatched output buffers — "the exact bytes the in-process
    dispatch would" produce (`ipc/server.rs:545`) — so a new verb
    family needs **no IPC protocol change**; `cli_ipc.rs` T16 is the
    byte-verbatim export relay pin.
12. **sqlx offline cache**: `.sqlx/` at the workspace root backs the
    compile-checked macros; `scripts/sqlx-prepare.sh` regenerates it
    and CI runs `cargo sqlx prepare --check` against the committed
    cache — **the new migration and the query!/query_as! conversion
    must land in one commit** with the refreshed `.sqlx/` entries or
    that commit does not compile.
13. **CLI surface** (`cli.rs`): `Command` is the clap enum; families
    carry `#[command(subcommand)] cmd: XxxCmd`; value-parser helpers
    (`parse_date_flag` strict `YYYY-MM-DD`, `parse_span_stamp`,
    `parse_anchor_edit`'s `none`-clear pattern) live in `cli.rs`;
    spec enums (`TaskKindSpec`, `ImportanceSpec`, …) bridge domain
    enums clap cannot derive on. `Export { format, since, until,
    task, board }` is the flag bag D-J12 widens.
14. **Output receipts** (`commands/output.rs`): serde `Serialize`
    structs (the `--json` envelope payload); **`JournalReceipt` is
    taken** by the undo substrate (the `tasks undo` durable receipt,
    `output.rs:472`) — reflection receipts must not collide with it.
    `emit_result(io, result, render)` does the envelope/`--json`
    split; `report_to` prints typed errors (`error_kind()` is the
    exhaustive kebab discriminator; exit 1 on failure, 0 on success).
15. **Completion**: `completion.rs::CompletionSpec::from_cli()` is
    derived from the clap tree; the test
    `the_spec_walk_matches_the_cli_catalogue` pins the offered-verb
    list ("boards … track, export, sync, status") — **`journal`
    joins it and the pin grows**. The REPL needs nothing: it
    dispatches through `dispatch_line` generically.
16. **Window derivation**: `AppConfig::resolved_day_calendar()`
    (`config.rs:1657`) builds `DayCalendar { day_start_time,
    offset }` from config + the OS offset; the pure
    `reflection::review_window(cadence, reference_date, calendar)`
    (`reflection.rs:538`) floors and returns the minted
    `ReviewWindow`; `DayCalendar::today(now)` exists (`days.rs:130`).
    The planner re-validates structurally (`ReviewWindow::try_new`).
17. **Vocabulary guards** (`domain/tests/vocabulary_guards.rs`): the
    `Journal*` identifier scan (identifier-boundary regex) allowlists
    only the undo substrate's homes; the `journal_entries` string is
    banned crate-wide. Both pass on `main`; chunk 2 adds no domain
    files, so the guards stay green — the app-side rule is stage 3's
    grep: **no `JournalXyz` identifier anywhere new; lowercase `journal`
    appears only as the clap name/string literals**.
18. **Engine/state crates**: nothing to touch — `StateCommand` routes
    generically through `plan_journaled_command`; the reflection
    section already grows the persisted insta dumps (chunk 1's
    shape-widening pass).
19. **CHANGELOG**: `## [Unreleased] ### Added` carries the chunk-1
    entry whose tail explicitly defers "Storage (migration + codec),
    the `journal` CLI verb family, …" to this chunk.

## 3. Design

### 3.1 The vocabulary ruling applied (D-J1) — the naming table

The collision risk here is *inverted* from chunk 1: the surface files
want the verb's name, the symbols want the family's. The binding:

| Thing | Name | Why |
|---|---|---|
| clap verb variant | `Command::Reflection` with `#[command(name = "journal")]` | a variant literally named `Journal` would be a new `Journal*`-adjacent code symbol and shadow the undo journal's vocabulary; clap's `name` decouples the parse from the symbol — code says `Reflection`, the user types `journal` |
| clap subcommand enum | `ReflectionCmd` | `JournalCmd` is banned outright |
| app module | `commands/reflection.rs` | symbol path, not surface |
| dispatch fns | `reflection_command` / `reflection_line` / `collapse` / `resolve_reflection_entry` | lowercase verb spelling only in *string literals* (the clap name, help text) — function identifiers say `reflection` |
| invocation type | `ReflectionInvocation` | the `TimersInvocation` shape |
| receipts/rows | `ReflectionRow`, `ReflectionAddReceipt`, `ReflectionEditReceipt`, `ReflectionDeleteReceipt`, `ReflectionsList` | `JournalReceipt` is **taken** by the undo substrate (`output.rs:472`) — collision, not style, rules this row |
| integration test file | `tests/cli_journal.rs` | DR-7 §5 prescribes the name ("`cli_journal.rs` in the `cli_timers.rs` shape"); test files are surface-named (`cli_track.rs`, `cli_timers.rs`) and file names are not code symbols |
| table | `reflection_entries` | D-J1 guard rule ii |

### 3.2 Migration 0022 (`0022_reflection_entries.sql`)

D-J13's DDL verbatim (the dead write arm's INSERT already matches it
column-for-column, fact 5):

```sql
CREATE TABLE reflection_entries (
    entry_id     TEXT PRIMARY KEY NOT NULL,  -- hyphenated UUID
    day          TEXT NOT NULL,              -- ISO date
    cadence      TEXT NOT NULL,              -- "day" | "week" | "month"
    window_start TEXT NOT NULL,              -- RFC 3339
    window_end   TEXT NOT NULL,              -- RFC 3339
    text         TEXT NOT NULL,
    mood         INTEGER,                    -- NULL = unset
    energy       INTEGER,                    -- NULL = unset
    flags        TEXT NOT NULL,              -- JSON array of known names
    extra_flags  TEXT NOT NULL,              -- JSON array of parked unknown names
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted      INTEGER NOT NULL            -- the in-place tombstone
);
```

- The 0008/0009/0015 recipe: hyphenated-UUID TEXT ids, RFC-3339
  stamps, **no FKs** (nothing references a reflection entry; entries
  reference no task), additive-only (ADR 0005).
- No index beyond the primary key: reads scan the whole map in memory
  after load; the table is write-through (the engine's batch apply),
  decade-scale single-digit MB (DR-7 §6 scale sanity).
- `deleted` is `INTEGER NOT NULL` (sqlite bool) — the row *is* the
  tombstone; there is no row-absence path in v0.2.
- **Build-order coupling** (fact 12): this file, the macro
  conversion (§3.3), the load codec, and the refreshed `.sqlx/` land
  in one commit — `query_as!` cannot typecheck against a table the
  embedded migrator has not shipped.

### 3.3 Storage: the write arm converts, the load codec lands

**Write** (`repo.rs`) — delete the R-11 dead-arm comment and the
runtime-checked `sqlx::query`; the buffered
`reflection_upserts` loop becomes the compile-checked `sqlx::query!`
upsert, the habit-write macro's shape verbatim (`INSERT … ON
CONFLICT(entry_id) DO UPDATE SET …`), binds byte-identical to fact 5's
mapping (mood/energy as `Option<i64>`, flags/extra_flags as JSON
array text via `serde_json::to_string`, stamps `to_rfc3339`,
`deleted` bool). The `codec.rs` additions stay minimal — the columns'
text forms already live on the domain types
(`ReviewCadence::text_form`, `HealthFlag::text_form`,
`Rating::get`); the only shared helpers worth extracting are
`reflection_flags_to_json(&BTreeSet<HealthFlag>) -> String` and
`reflection_extra_flags_to_json(&BTreeSet<String>) -> String` (each
one `serde_json::to_string` of a sorted-name vec; unit-tested
roundtrip incl. the empty set → `"[]"`).

**Load** (`repo.rs`):

- `struct ReflectionEntryRow (String, String, String, String, String,
  String, Option<i64>, Option<i64>, String, String, String, String,
  bool)` — the `query_as!` target (the `HabitRow` tuple shape).
- One `query_as!` over `SELECT entry_id, day, cadence, window_start,
  window_end, text, mood, energy, flags, extra_flags, created_at,
  updated_at, deleted FROM reflection_entries` (all rows — tombstones
  load too).
- `fn decode_reflection_entry(row: &ReflectionEntryRow) ->
  Result<(ReflectionId, ReflectionEntry), RepositoryError>` — the
  `decode_habit` shape:
  - `entry_id` → `codec::uuid_from_text` → `ReflectionId`; any parse
    miss → `RepositoryError::Corrupted`.
  - `cadence` → `ReviewCadence::parse_text_form` → `Corrupted` on
    `None`; `day` → `NaiveDate::parse_from_str("%Y-%m-%d")` →
    `Corrupted`; stamps → `decode::clock` (the shared RFC-3339
    decoder) → `Corrupted`.
  - `mood`/`energy` → `i64` → `u8::try_from` → `Rating::try_new` →
    **`Corrupted` on either miss** — the strict-decode posture (a
    stored 0/6+ is a data error, never a clamp; D-J3/P-3).
  - `flags`/`extra_flags` → `serde_json::from_str::<Vec<String>>` →
    `Corrupted` on malformed JSON; **known names mint into `flags`,
    unknown names park verbatim into `extra_flags`** — the decode
    seam is where tolerance lives (D-J13/D-H12); the parked set
    merges with the column's own parked rows (`BTreeSet` union).
  - `deleted` → bool as-is.
- `load_inner`'s default-comment block folds the rows into
  `state.reflection.entries` (insert per row — tombstones included;
  `ReflectionState`'s map is the storage of record and
  `live_entries()` does the filtering at read).

**No LWW guard in the SQL**: unlike `habit_log` (whose
`WHERE excluded.clock > clock` belt exists for the sync fold's
braces), reflection batches are engine-ordered upserts of whole rows;
the tombstone row *is* the delete, and the future import fold keys on
the entry UUID (ADR 0021) — adding a clock predicate now would be
speculative machinery with no caller (dropped per the no-dead-code
rule; the fold's design may add it at #631).

### 3.4 The `journal` verb family (D-J11)

**Grammar** (the round doc's sketch verbatim, adjusted only where the
landed payload forces it):

```
taskboard journal add [--cadence day|week|month] [--day <YYYY-MM-DD>]
            [--mood <1-5>] [--energy <1-5>] [--flag <name>]… [TEXT…]
taskboard journal list [--cadence <c>] [--since <date>] [--until <date>]
taskboard journal show <selector>
taskboard journal edit <selector> [--text <t>|--clear-text] [--mood <n>|--clear-mood]
             [--energy <n>|--clear-energy] [--flag <name>]… [--unflag <name>]…
taskboard journal delete <selector>
```

**cli.rs additions**:

- `Command::Reflection { cmd: ReflectionCmd }` — `#[command(name =
  "journal")]` (§3.1), doc comment in the surface's voice
  ("review-journal verbs over the in-process core; device-local —
  never synced", the `Timers` variant's shape).
- `enum ReflectionCmd { Add {…}, List {…}, Show {…}, Edit {…},
  Delete {…} }`:
  - `Add`: `#[arg(long, value_enum)] cadence: Option<CadenceSpec>`,
    `#[arg(long, value_parser = parse_date_flag)] day:
    Option<NaiveDate>`, `#[arg(long)] mood: Option<u8>`, `energy:
    Option<u8>`, `#[arg(long)] flag: Vec<String>` (value-parsed to
    `HealthFlag` at collapse), and trailing
    `text_words: Vec<String>` (`num_args(1..)` optional — the zero
    words + zero flags case is the planner's accepted no-op, not a
    clap refusal: capture-at-speed P1 must not reject an empty line,
    it just records nothing). The body is
    `text_words.join(" ")`.
  - `List`: `--cadence/--since/--until` (`parse_date_flag`).
  - `Show`: positional `selector: String`.
  - `Edit`: positional `selector`; `--text <t>` (conflicts with
    `--clear-text`), `--clear-text` (flag); `--mood <u8>` /
    `--clear-mood`; `--energy <u8>` / `--clear-energy` (each set/clear
    pair `conflicts_with`; the `parse_anchor_edit` precedent); 
    `--flag Vec<String>` / `--unflag Vec<String>`.
  - `Delete`: positional `selector`.
- `enum CadenceSpec { Day, Week, Month }` — the `TaskKindSpec`
  bridge: `#[value(name = "day")]` etc., mapping into
  `ReviewCadence` via `parse_text_form`/`text_form` at collapse.
  `parse_health_flag(&str) -> Result<HealthFlag, String>` wraps
  `HealthFlag::try_new` — **the mint's typed rejection is consumed at
  the clap boundary** (chunk-1 plan §3.3's declared consumer):
  an unknown flag name is a usage error (exit 2), never a planner
  rejection.
- `parse_rating_flag` is deliberately **not** added: `--mood 9`
  parses as `u8` and the planner's mint rejects it typed
  (`invalid-rating`, already mapped) — the D-TC8 posture (validity is
  the domain's law, not the parse's). A non-numeric value is a clap
  usage error regardless.

**collapse** (`commands/reflection.rs`): `collapse(ReflectionCmd) ->
Option<ReflectionInvocation>` — total except the vacuous
unreachable-branch cases (the `timers::collapse` comment). Arms:
`Add { cadence: ReviewCadence, day: Option<NaiveDate>, mood, energy,
flags: BTreeSet<HealthFlag>, text: String }` (dedup via the set),
`List { cadence: Option<ReviewCadence>, since, until }`, `Show {
selector }`, `Edit { selector, text: Option<String>, mood/energy:
Option<Option<u8>>, flags_add/flags_remove: BTreeSet<HealthFlag> }`,
`Delete { selector }`. `--text` → `Some(t)`; `--clear-text` →
`Some(String::new())` (the payload's legal empty-text clear);
`--mood n` → `Some(Some(n))`; `--clear-mood` → `Some(None)`.

**The one-shot lane** (`reflection_command`, the
`timers_command`/`track_command` shape): per-mode `run_core` +
renderer. **The session lane** (`reflection_line`): per-mode
`emit_result`. Both call the same `collapse` product (one definition
site).

**The `*_on` seams** (session core):

- `add_on(app, calendar, input) -> ReflectionAddReceipt` — derives
  the window app-side: `let window =
  review_window(input.cadence, input.day, calendar)` where `input.day`
  defaults to `calendar.today(Utc::now())` at the seam (the
  D-F6/D-J11 split: the app layer owns calendars, the planner stays
  blind); executes `RecordReflection { cadence, day: window.day(),
  window: window.bounds(), text, mood, energy, flags }`; on
  `ReflectionRecorded` it **reads the entry back from the published
  snapshot** (`state.reflection.entries[&entry.id]` — never echoes
  the input; the `timers::add_on` expect-discipline). Receipt:
  `ReflectionAddReceipt { created: true, entry: ReflectionRow }`.
  On `NoOp` (the empty record): `{ created: false, entry: None }` —
  nothing was stored, and that is success, not an error.
- `edit_on(app, selector, mask) -> ReflectionEditReceipt` — resolves,
  executes `UpdateReflection`, projects the row **post-execute** from
  the snapshot; `NoOp` (all-unchanged or tombstoned target — the
  latter unreachable through the CLI, see §3.5) → `{ changed: false,
  entry: <the resolved row> }` (the `track adjust` shape: the current
  row rides the no-op receipt).
- `delete_on(app, selector) -> ReflectionDeleteReceipt` — resolves,
  projects the **pre-execute** row (the `timers::cancel_on`
  discipline: post-execute the row is tombstoned), executes
  `DeleteReflection`; outcome `ReflectionDeleted` or `NoOp` (repeat —
  unreachable through the CLI, §3.5). Receipt carries the pre-state
  row.
- `list_on(app, filters) -> ReflectionsList` and `show_on(app,
  selector) -> ReflectionRow` — pure reads over the published
  snapshot's `live_entries()`; **no command executes** (the
  `timers::list_on` shape).

**Selector resolution** (`resolve_reflection_entry(state, selector)
-> Result<ReflectionId, CommandError>`): the
`resolve_time_entry` shape verbatim (D-J11: "like `resolve_alarm`"
minus the label pass — entries carry no label): exact uuid hit →
unique ≥8-char lowercased uuid prefix → typed miss. **Resolution
runs over `live_entries()` only**: a tombstoned entry is gone from
the family's own recovery surface, so its selector is a typed
unknown — an `edit`/`delete` on it never reaches the engine's
tombstoned-NoOp (that planner law stays engine-tested; the CLI's
contract is the sharper one). New app error variants
(`commands/mod.rs`): `UnknownReflectionEntry { selector }` → kind
`"unknown-reflection-entry"`, `AmbiguousReflectionEntry { candidates:
Vec<String> }` → kind `"ambiguous-reflection-entry"` (the
`UnknownTimeEntry` pair's shape; the *domain's*
`CommandError::UnknownReflection` stays engine-side — reachable only
by a direct `StateCommand` dispatch, e.g. the REPL's typed error
path).

**Rows and renderers**:

- `ReflectionRow` (`output.rs`): `{ entry_id: String, cadence: String
  (text form), day: NaiveDate, window_start/window_end: DateTime<Utc>,
  text: String, mood: Option<u8>, energy: Option<u8>, flags:
  Vec<String>, extra_flags: Vec<String>, created_at, updated_at }` —
  projected from the entity; **includes `extra_flags`**: the
  interactive surface shows the whole entry (parked names are data
  the user wrote from another device), while the *export* leg stays
  at D-J12's ruled columns (§3.6).
- Receipts wrap it (§3.1 table). Human renderers
  (`render_text_add/edit/delete/list/show`) follow the
  `render_text_*` voice — machine columns + the text body, no verdict
  language (P9): e.g. `journal add` renders the stamped row
  (`id`, window bounds, `mood 3/5 · 2 flags`-class summary, the text);
  `list` renders one line per row in canonical order; `edit` renders
  `changed: true|false` + the row.
- **Canonical order** (list and everything row-derived):
  `(window_start, entry_id)` ascending — the `track list`/export
  precedent generalized to the review window; deterministic across
  lanes and formats.

**Lane routing** (`commands/mod.rs`): `Command::Reflection` joins the
`IpcRoute::IpcOrLocal` arm (the D-TC1 argument verbatim: mutations
must land in the daemon's engine or a one-shot boot would write a
*different* database's rows; the reads stay on the same lane so
`journal list` answers the serving database's truth). The `run`
match grows the one-shot arm; `dispatch_line` grows the session arm —
both collapse through the single `collapse`. The REPL and the IPC
relay need no further work (facts 10/15).

**Completion** (`completion.rs`): `journal` joins the offered verbs;
the subcommand spec pins `add/list/show/edit/delete` and the flag
sets; the catalogue test's verb list grows (fact 15).

### 3.5 The export leg (D-J12)

- **cli.rs**: `Export` grows `#[arg(long, value_enum)] what:
  Option<ExportWhat>`; `enum ExportWhat { Tracking, Journal }`,
  `#[default] Tracking` — **bare `export` is byte-compatible** (the
  default arm compiles to today's code path untouched). `ExportQuery`
  grows `what: ExportWhat` (collapse via `unwrap_or_default()`).
- **`export_on`** branches on `what`:
  - `Tracking`: today's body verbatim (zero behavioral diff — the
    byte-compat pin is structural: default == explicit `--what
    tracking` over the same state).
  - `Journal`: over `live_entries()` (tombstones skipped — export is
    the live journal's data; tombstones are internal bookkeeping),
    filtered by `--since`/`--until` **inclusive on `window_start`**
    with the shared `day_start` UTC-day arithmetic (`window_start >=
    day_start(since)`, `window_start < day_start(until.succ_opt())`),
    ordered `(window_start, entry_id)` ascending.
  - **`--task`/`--board` under `--what journal` is a typed rejection**:
    the ruling names `--since`/`--until` as the journal leg's only
    filters, and silently ignoring a passed filter is the worst
    outcome for a data command. New variant
    `CommandError::ExportFiltersNotApplicable` → kind
    `"filters-not-applicable"`, exit 1. (Clap cannot express a
    value-dependent conflict, so this is a runtime typed check, not a
    usage error.)
- **Columns** (D-J12's ruled set, exactly eleven, in this order):
  `entry_id, day, cadence, window_start, window_end, mood, energy,
  flags, text, created_at, updated_at`.
  - CSV (`render_csv` machinery reused — **no second serializer**):
    header row first; `day` as `YYYY-MM-DD`; stamps RFC-3339;
    `mood`/`energy` empty cells when unset; `flags` as the JSON-array
    text (the storage encoding — `[]` empty, `["migraine",
    "poor-sleep"]` sorted); `text` through `csv_field` (quoted only
    when needed).
  - JSON: the same single-line, newline-terminated array shape as
    `render_json`; each row an object with the eleven keys —
    `mood`/`energy` `null` when unset, `flags` a **real array** (not
    a stringified one), `extra_flags` absent (ruled columns fixed).
  - **`extra_flags` and tombstones are deliberately not exportable in
    v0.2**: the column set is ruled data (a twelfth column breaks the
    contract; tombstones are bookkeeping). Both survive in
    storage/state — this is a recorded limitation, fenceable later,
    not an accident.
- **Posture**: `export_command`/`export_line` already write raw bytes
  to `io.out` and route failures through `report_to` — the journal
  leg inherits the whole D-T15/D-TC10 apparatus unchanged; `--json`
  never wraps the payload (fact 10; the relay is byte-verbatim by
  the server's captured-buffer mechanism).

### 3.6 What nothing else touches

- **`taskboard-state`**: no changes (generic routing; chunk 1's
  engine tier already publishes the rows).
- **`taskboard-domain`**: no changes — the entity, mints, windows,
  and payloads are landed; the vocabulary guards already pass.
- **Undo**: the three commands stay `NotUndoable(ReflectionSurface)`
  (chunk 1); `journal add/edit/delete` never surface undo hints.
- **Kiosk/UI-slint**: nothing (D-J14 fences the walk to chunk 3).
- **Insta snapshots**: the persisted dumps already carry the
  `reflection` key (chunk 1); hydration changes no dump. Storage
  `sqlite_smoke`-class suites gain no snapshot churn.

## 4. Decision summary (every open question, resolved)

| ID | Question | Decision | Reason |
|---|---|---|---|
| Q-1 | clap variant + subcommand-enum naming | `Command::Reflection` (clap `name = "journal"`), enum `ReflectionCmd`, module `commands/reflection.rs`; the integration test file alone is surface-named `cli_journal.rs` | `JournalCmd`/`Command::Journal` would be new `Journal*`-adjacent symbols — the exact drift the ruling and stage-3 trap (c) hunt; clap's `name` decouples surface from symbol. DR-7 §5 itself names the test file `cli_journal.rs`, and file names are not code symbols |
| Q-2 | Receipt naming | `ReflectionRow` / `ReflectionAddReceipt` / `ReflectionEditReceipt` / `ReflectionDeleteReceipt` / `ReflectionsList` | `JournalReceipt` is the undo substrate's durable receipt (`output.rs:472`) — a collision, not a style choice |
| Q-3 | Where `--day`'s today-default applies | At the `add_on` seam, via the lane's resolved `DayCalendar`; `collapse` carries `Option<NaiveDate>` verbatim | the one-shot lane reads `config.resolved_day_calendar()`, the session lane the session config's (the `track_line` D-TC5 shape); keeping the wall clock out of `collapse` keeps it testable without time injection |
| Q-4 | `--mood/--energy` parse strictness | clap parses `u8`; the 1..=5 law is the planner's typed rejection (`invalid-rating`) | the D-TC8 posture; the kind mapping already exists (fact 4), so a bad number fails typed end-to-end with zero new plumbing |
| Q-5 | Unknown `--flag` name | clap usage error (exit 2) via `parse_health_flag` → `HealthFlag::try_new` | chunk-1 plan §3.3 declares the mint's error "consumed by chunk 2's CLI `--flag` parse"; an unknown name can never reach the typed `BTreeSet<HealthFlag>` payload |
| Q-6 | Empty `journal add` (no text, no attrs, no flags) | clap accepts it; the planner's empty-record `NoOp` returns `{ created: false, entry: None }` as success | P1 capture-at-speed must not reject; the planner owns the no-op law (fact 4) |
| Q-7 | Selector resolution universe | `live_entries()` only — a tombstoned entry's selector is a typed `unknown-reflection-entry` | the family's own verbs are the recovery surface (ADR 0021 reason iii); a deleted row is user-gone. The engine's tombstoned-NoOp stays reachable engine-side only |
| Q-8 | Row canonical order (list/show/export) | `(window_start, entry_id)` ascending | the `track list`/export precedent; deterministic across lanes, formats, and devices |
| Q-9 | `--task/--board` under `--what journal` | typed `ExportFiltersNotApplicable` (`"filters-not-applicable"`, exit 1) | D-J12 rules since/until the journal leg's only filters; silent ignore is unacceptable for a data command; clap cannot express value-dependent conflicts |
| Q-10 | Export `flags` cell format | CSV cell = JSON-array text (the storage encoding); JSON payload = real arrays; unset mood/energy = empty CSV cells / JSON `null` | one encoding (storage's) across machine surfaces; `jq` gets structured data |
| Q-11 | `extra_flags` in exports | absent from the ruled eleven columns; present in `ReflectionRow` for `list`/`show` | the column set is ruled data; the interactive surface may show the whole entry — recorded limitation, fenceable, not an accident |
| Q-12 | Tombstones in export/list | skipped (both read `live_entries()`) | D-J12: "tombstoned rows are skipped"; export is the live journal's data |
| Q-13 | LWW guard in the storage upsert | none — plain whole-row upsert | engine-ordered batches + tombstone-in-place make a clock predicate caller-less today; the import fold (#631) may add it at its own design time |
| Q-14 | Whole-state roundtrip coverage | `actions_from_state` grows the reflection arm; the proptest generator emits reflection entries (open + tombstoned, parked unknown flags) | memory ≡ disk by construction (§1); without it the load codec's drift is invisible to the suite |
| Q-15 | Contract suite | `tests/contract.rs` untouched; the fake-repo parity for reflection lives in the new `reflection_storage.rs` targeted tests | the domain contract battery is task-centric; the targeted file follows the `journal_storage.rs` precedent without widening a shared harness |
| Q-16 | Migration + macro commit granularity | one commit: migration file + query!/query_as! conversion + load codec + refreshed `.sqlx/` | compile-time coupling (fact 12); CI's `prepare --check` fails on any split |
| Q-17 | `applied == 21` pin | grows to 22 in `reopening_applies_no_new_migrations` | fact 8; the count pin is the migration-census lockstep |

## 5. TDD test plan

Every item lands as a failing test first (red), then its production
arm (green), in the commit order of §6. No text assertions anywhere;
typed outcomes and structural facts only.

### 5.1 Storage tier (`crates/taskboard-storage-sqlite`)

1. **`connect.rs`**: `reopening_applies_no_new_migrations` — the
   census pin grows `21 → 22` (red the moment the migration file
   lands; same commit resolves it).
2. **`connect.rs`: `migration_0022_upgrades_a_pre_reflection_database`**
   — the 0017 recipe: raw-connect, run shipped migrations, `DROP
   TABLE reflection_entries` + strip the bookkeeping row, reopen —
   `state.reflection == ReflectionState::default()` (the empty fold);
   then apply one `UpsertReflectionEntry`, reopen, assert the row
   round-trips **including a tombstoned row and parked unknown
   flags**.
3. **`reflection_storage.rs`** (new, the `journal_storage.rs`
   shape):
   - codec unit roundtrips: NULL mood/energy; empty flag sets
     (`"[]"`); unknown flag names parked and re-encoded verbatim
     (decode → encode byte-stability through the storage codec); a
     tombstoned row survives apply + load unchanged; day/cadence
     text forms round-trip.
   - decode rejection facts: a corrupt uuid/date/RFC-3339/JSON cell
     yields `RepositoryError::Corrupted` (typed), never a panic —
     planted via a hand-written raw INSERT on a raw pool.
   - `actions_from_state`'s new reflection arm: a state with N live
     + M tombstoned entries applies and loads identically (the fake
     repo parity: `open_memory` vs the in-memory fake produce equal
     `reflection` maps).
4. **`roundtrip.rs`**: the whole-state proptest generator emits
   reflection entries (mix of cadences, ratings present/absent,
   flag sets incl. parked names, live + tombstoned); the
   write→load→write equality pin extends to `state.reflection`
   (Q-14).

### 5.2 App tier — the CLI journeys (`tests/cli_journal.rs`, hermetic tempdir boots)

Harness: the `cli_timers.rs` shape — `common::{standalone_config,
tempdir, forbidden_store}`, clap `try_parse_from` for grammar tests,
`collapse` + the one-shot fns for journeys, `dispatch_line` for the
session lane, `error_kind()` for refusals.

- **J1 `add` day defaults & persistence**: explicit `--day` +
  defaults (cadence day) — receipt `created: true`, the row read
  back from the snapshot equals the entity in state (id minted,
  clocks stamped inside the test's `[before, now]` bracket); a fresh
  boot's `list` sees it (durable).
- **J2 the default-`--day` leg, structural**: no `--day`, a 00:00
  `day_start_time` config — the created entry's window contains the
  boot-time `now` and its `day` label equals the calendar's
  `today(now)` reading (midnight-straddle-safe: no wall-clock
  equality, only containment + label-vs-instant consistency).
- **J3 week/month flooring**: `--cadence week --day` mid-week floors
  to that week (window bounds are the calendar's Monday-anchored
  168h; `day` = the floored Monday); `--cadence month` floors to the
  `[1st, next 1st)` span — the canonical-mint law read through the
  stored row (windows stored verbatim, never re-derived).
- **J4 attributes**: `--mood 3 --energy 4 --flag migraine --flag
  poor-sleep` — the row's ratings/flags round-trip; repeated
  `--flag x x` dedups (set semantics); `--flag not-a-flag` is a clap
  usage error (typed outcome: `try_parse_from` errs, exit-class 2 —
  no message text).
- **J5 the empty record**: bare `journal add` (no words, no flags) —
  `created: false`, `entry: None`; `list` stays empty; exit 0 (the
  accepted no-op is success).
- **J6 multi-word text**: positional words join with single spaces
  into `text` (pinned structurally on the stored row).
- **J7 `edit` change-mask**: text swap, `--mood` set, flag add+remove
  delta — `changed: true`, the post-row read from the snapshot
  carries exactly the delta (the untouched flag survives; clocks:
  `updated_at` advances, `created_at` never); a second identical
  edit — `changed: false`, row identical, `updated_at` unchanged
  (mask-no-churn).
- **J8 `edit` clears**: `--clear-mood` → mood `None`; `--clear-text`
  → empty text; `--clear-energy` likewise — the double-Option
  semantics read through the row.
- **J9 invalid payloads typed**: `--mood 9` → error_kind
  `invalid-rating`; `--cadence week --day` with a structurally
  impossible window is unconstructible through the CLI (the app-side
  derivation is total), so no test plants it — the planner guard is
  chunk-1-tested; a selector that misses → `unknown-reflection-entry`;
  a two-hit prefix → `ambiguous-reflection-entry` with both candidate
  uuids in the typed payload.
- **J10 selector grammar**: exact uuid; ≥8-char prefix; a 7-char
  prefix is a miss (not an ambiguity) — the `resolve_time_entry`
  law.
- **J11 `delete`**: receipt carries the pre-state row; `list`/`show`
  no longer resolve it (typed unknown); repeat `delete` is the same
  typed unknown (Q-7) — exit 1 both times.
- **J12 `list` filters & order**: `--cadence` filter; `--since`/
  `--until` inclusive on `window_start` (boundary entries land
  inside); canonical `(window_start, entry_id)` order pinned over a
  shuffled fixture.
- **J13 `show`**: the row equals the corresponding `list` row
  (projection parity, no second projection).
- **J14 `--json` envelopes**: `add`/`edit`/`delete`/`list`/`show`
  under `--json` emit serde-JSON whose typed fields equal the
  text-lane receipts (asserted as parsed values — the
  `cli_output.rs` discipline); **`export` under `--json` still
  writes the raw payload on stdout** (the envelope never wraps it).
- **J15 session-lane parity**: `dispatch_line` over a booted session
  core produces the same receipts/kinds as the one-shot lane for
  add/edit/delete/list/show (the two-lane equivalence pin, the
  `cli_timers.rs` T-series shape).

### 5.3 The export leg (`tests/cli_journal.rs` + the pins' new home)

- **J16 byte-compat pin**: one fixture state with tracking entries;
  `export` (no `--what`) and `export --what tracking` over the same
  state produce **byte-identical** stdout (CSV and JSON formats both)
  — the structural default==explicit law (D-J12's "bare export
  byte-compatible").
- **J17 journal CSV golden**: constructed entries with fixed stamps —
  one full row (ratings + two flags + text with a comma and a
  quote), one minimal row (no ratings, no flags, empty text), a
  tombstoned row (absent), rows filtered by `--since`/`--until`
  window_start edges (boundary-inclusive). The golden file is
  committed fixture **data** — asserted byte-for-byte, containing
  zero UI labels (AGENTS §5: goldens are data, not labels; the header
  row is the ruled column list).
- **J18 journal JSON golden**: the same fixture under `--format json
  --what journal` — single-line array, `null` ratings, real flag
  arrays, the eleven ruled keys exactly (key-set pin, not text).
- **J19 `filters-not-applicable`**: `--what journal --task <sel>` (and
  `--board`) → error_kind `filters-not-applicable`; `--what tracking
  --task` still works.

### 5.4 The IPC relay leg (`tests/cli_ipc.rs`)

- **J20 (extends the T16 shape)**: with the daemon serving the db —
  `journal add` over the socket lands in the **daemon's** database
  (a subsequent fallback `journal list` sees it — the D-TC1 lane
  argument, now reflection-verified); `export --what journal --format
  csv` over the socket is **byte-identical** to the daemon-down
  fallback run (the byte-verbatim relay pin, D-J12 + trap (a)).

### 5.5 Domain-side guard re-runs (no new tests)

- The vocabulary guards, the reflection proptests, and the engine
  tier are chunk-1-landed and untouched; they re-run in the gates.
  The stage-2 diff adds **zero** domain-crate lines.

### 5.6 Gates

Full AGENTS §10 suite through `scripts/run-quality-gates.sh`
(fmt, clippy `-D warnings`, nextest, doc tests, deny — **zero new
dependencies**, the round doc's §5 gate note — llvm-cov + quarantine).
`cargo sqlx prepare --check` passes against the committed `.sqlx/`.
CHANGELOG gains the Unreleased/Added entry naming #637 and the #631
unblock.

## 6. Stage-2 execution order (TDD pairs → standalone green commits)

1. **`feat(storage-sqlite): the reflection entries table`** — the
   0022 migration + §3.3's full conversion (macro write arm, load
   codec, `.sqlx/` refresh) + tests 5.1.1–5.1.3 + the census pin.
   One commit (Q-16); red-first: 5.1.1 + the upgrade test fail
   before the file lands.
2. **`test(storage-sqlite): the reflection roundtrip arm`** —
   `actions_from_state`'s arm + the proptest generator extension +
   5.1.4 (red first against the landed codec: the generator's states
   simply didn't round-trip reflection before).
3. **`feat(app): the journal verb family`** — cli.rs grammar +
   `commands/reflection.rs` (collapse, seams, resolution, lanes) +
   output.rs receipts + `CommandError` variants + routing arms +
   completion + tests J1–J15. (Large but one coherent deliverable;
   commits inside may split grammar/journeys if each stays
   standalone-green — stage 2's call, never a fixup-shaped split.)
4. **`feat(app): the export journal leg`** — `ExportWhat` + the
   `export_on` branch + the not-applicable filter check + J16–J19.
5. **`test(app): the journal relay leg`** — J20 in `cli_ipc.rs`.
6. **`docs(changelog): the reflection storage and journal entry`** —
   the CHANGELOG entry (may fold into commit 4 if stage 2 prefers;
   it must exist before handoff).

## 7. Files touched & estimated diff size

| File | Change | ≈ lines |
|---|---|---|
| `crates/taskboard-storage-sqlite/migrations/0022_reflection_entries.sql` | new | 15 |
| `crates/taskboard-storage-sqlite/src/repo.rs` | macro write arm + row struct + query + decode fn | +90 |
| `crates/taskboard-storage-sqlite/src/codec.rs` | the two flags-JSON helpers + unit tests | +40 |
| `.sqlx/` | refreshed query cache entries | ±2 files |
| `crates/taskboard-storage-sqlite/src/connect.rs` | census pin + upgrade test | +60 |
| `crates/taskboard-storage-sqlite/tests/common/mod.rs` | reflection arm | +8 |
| `crates/taskboard-storage-sqlite/tests/reflection_storage.rs` | new | +180 |
| `crates/taskboard-storage-sqlite/tests/roundtrip.rs` | generator + assertions | +70 |
| `crates/taskboard-app/src/cli.rs` | `Command::Reflection`, `ReflectionCmd`, `CadenceSpec`, `ExportWhat`, parse fns | +170 |
| `crates/taskboard-app/src/commands/reflection.rs` | new — collapse, seams, resolution, lanes, renderers | +430 |
| `crates/taskboard-app/src/commands/output.rs` | row + receipt types | +80 |
| `crates/taskboard-app/src/commands/mod.rs` | error variants + kinds + two dispatch arms + route arm | +45 |
| `crates/taskboard-app/src/completion.rs` | verb + subcommand specs | +25 |
| `crates/taskboard-app/tests/cli_journal.rs` | new — J1–J19 | +430 |
| `crates/taskboard-app/tests/cli_ipc.rs` | J20 | +55 |
| `CHANGELOG.md` | Added entry | +12 |

**Total ≈ 1,700 lines (±10%), roughly half tests** — size M as
dispatched. Zero domain/state/ui-slint lines; zero new dependencies.

## 8. Risks & mitigations

- **Vocabulary drift in the app crate** (the guard scans the domain
  only): mitigated by Q-1/Q-2's table — the surface/symbol split is
  decided *before* implementation, and stage 3's trap (c) greps the
  diff. The one sanctioned `journal`-spelled surface artifacts are:
  the clap `name = "journal"`, the doc/help prose, the test file
  name, and the CHANGELOG's user-facing verb.
- **Migration number collision** (DR-7 §6's standing risk): 0022
  re-verified free at base `81c6f585` (fact 1); the implementing
  stage re-verifies at its rebased base before the migration commit
  — a collision is a renumber (the sanctioned mechanical response).
- **Byte-compat regression on bare `export`**: the default arm must
  compile to the *same code path*, not a parallel one — Q-9's branch
  is `match what { Tracking => today's body, Journal => … }` with
  today's body moved verbatim; J16 pins it; stage-3 trap (a) re-pins
  the relay.
- **The `.sqlx/` cache going stale** (CI `prepare --check` red):
  Q-16's one-commit rule makes it impossible to forget — the macro
  cannot compile without the migration, and the cache refresh rides
  the same commit.
- **`JournalReceipt` name collision** (an easy autopilot mistake —
  "the journal family's receipt"): the ruling pre-empts it (Q-2);
  the compiler would actually catch a duplicate type name here, but
  the *worse* failure is a rename of the undo receipt — forbidden;
  the undo substrate is frozen.
- **Tombstone-selector UX surprise** (Q-7: `journal delete <tombstoned>`
  is a typed unknown, not a silent no-op): recorded as the designed
  contract — the family's verbs are the recovery surface *for live
  entries*; a deleted entry's selector is unknown by ruling. The
  test pins it (J11) so the contract is explicit, not accidental.
- **Export-column drift** (someone adds `extra_flags` "for
  completeness"): the eleven-column list is ruled data — J17/J18's
  key-set/header pins make any addition a red test, not a review
  debate.

## 9. Handoff notes

- **Worktree**: `../taskboard-WT-feat-reflection-storage-cli` on
  branch `feat/reflection-storage-cli`, created off `origin/main`
  @ `81c6f585` (worktree doctor passed: 183G free vs 25G floor).
  Stage 2 resumes it (recreate per AGENTS §8 if missing); stage 3
  rebases onto `origin/main` and tears the worktree down at handoff.
- **PR** (stage 3): title `feat(storage-sqlite,app): the reflection
  store and journal verbs`; body: "Closes #637" + "**This merge
  unblocks nextcloud-files-sync #631** (its precondition — entity +
  state home + storage — completes here)".
- **The pre-condition contract with #631**: tombstones are rows, the
  fold reads `deleted` directly; the entry UUID is the future union
  key; the load codec's tolerant parking is the wire-row decode
  rehearsal (ADR 0021). Nothing here adds sync machinery — that is
  #631's territory.
- **Fence reminders** (already tracked, do not design here):
  `$EDITOR` input (#642), stored templates (#639), the walk surfaces
  (#638/chunk 3), statistics (#623 slice B), roaming (#631).

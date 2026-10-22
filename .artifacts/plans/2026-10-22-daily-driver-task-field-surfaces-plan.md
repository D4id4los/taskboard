# [Plan] Daily-Driver Chunk 5 — Field Surfaces + Anchor Hiding on Lists + Sidecar Badge

- **Date**: 2026-10-22
- **Kind**: stage-1 implementation plan for daily-driver chunk 5
  (`feat/app-task-field-surfaces`; closes #209): `tasks edit` field
  flags, `TaskRow` growth + phrase rendering, REPL completion, UI
  decoration carriers + phrase helpers, the anchor exact-title filter
  over list surfaces, and the per-board sidecar-health badge.
- **Origin** (normative): implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-12 + DDF-5 (filter/badge clauses), §3 chunk 5, §4 gates,
  §6 bookkeeping; field-family design plan
  `2026-10-18-field-family-design-plan.md` §3.5 D-F5 + §5 ruling
  (ACCEPTED); sidecar design plan `2026-10-18-appdata-sidecar-design-plan.md`
  §7 (hiding + warning rulings); views-finding design plan
  `2026-10-19-views-finding-design-plan.md` §7 item 3 (the decorations
  slot) + item 1 (helper names are this chunk's output); dispatch plan
  `2026-10-19-daily-driver-implementation-dispatch-plan.md` §1/§6.
- **Status**: READY — MD-9 verified: chunk 4 (`feat/sync-appdata-fold`)
  is merged on `origin/main` @ `a721e3b` (fold primitives, flush,
  fences, anchor-title CLI guard, per-board health on `SyncStatus`,
  CLI `status` warning all present; remote branch deleted post-merge).
  This branch is cut from `origin/main` @ `a721e3b`.

## 0. Vocabulary relocations (MD-8 — symbols are the stable keys)

D-F5's line numbers are plan-day references; re-located on
`origin/main` @ `a721e3b`:

| Origin says | Lives at |
|---|---|
| `TasksCmd::Edit` (cli.rs :384–398) | `crates/taskboard-app/src/cli.rs:508` |
| `DueSpec` / `parse_due` (:267/:276) | `cli.rs:383` / `cli.rs:392` |
| `commands/tasks.rs::edit`/`edit_on` | `commands/tasks.rs:396` / `:410` (guard already inside `edit_on`) |
| `TaskRow` + `due_marker` (output.rs :89–108) | `commands/output.rs:89` (`DueMarker` :37, `DueContext` :48, `task_row` :716) |
| flag→changeset mapping | `commands/mod.rs:1203` (`task_changes` — four slots stubbed `None`) |
| `models.rs::push` + due wiring | `crates/taskboard-ui-slint/src/models.rs:106` (`task_card_model` :374 — `decorations` slot pushed empty) |
| `presentation.rs` `due_phrase`/`due_sets` | `presentation.rs:115` / `:48` (`duration_bucket` :95, `sync_badge_for_board` :162, `banner_rows` :205) |
| the SyncBadge seam | `SyncBadgeModel` at `ui/compositions/status-bar.slint:16`; read path `push_status_bar` (models.rs:314) |
| the chunk-4 health surface | domain `state.rs:79` (`SidecarHealth`) + `state.rs:66` (`SyncStatus.sidecar`); app projection `commands/output.rs:252` (`sidecar_view` → `SidecarHealthView`); CLI wording `commands/status.rs:350` |
| the anchor-title const | domain `appdata.rs:36` `ANCHOR_CARD_TITLE = "[taskboard] app-data"`, re-exported `lib.rs:59`; app guard arms `commands/tasks.rs:420` (edit) / `:481` (delete) |
| the decorations slot (doc 4 §7.3) | `ui/widgets/task-card.slint:21` `TaskDecoration {kind, label, tone}` + `:14` `DecorationKind {importance, urgency, habit, reminder}`; tertiary render loop `task-card.slint:124` |
| level/effort domain types | `taskboard-domain/src/entities.rs:50` (`Effort` newtype over u64), `:72`/`:86` (`Importance`/`Urgency`, five levels, serde `snake_case`, derived `Ord`) |
| flag→slot engine half (already landed) | domain `command.rs:333` (`UpdateTask` applies all four slots, empty changeset = typed `no_op`) |
| `tasks list` human render | `commands/tasks.rs:572` (`render_text_list`) |

Dependency fact this plan relies on: `taskboard-app` depends on
`taskboard-ui-slint`, and `taskboard-ui-slint` depends only on
`taskboard-domain` — the only crate both frontends see is **domain**.

## 1. Decisions (every open question settled; D-numbers are this plan's)

### D1 — The effort text grammar is a domain text form; `cli.rs::parse_effort` is the clap wrapper

The design requires **one formatter shared with the parser** (D-F5)
and the stage-2 hard rule restates it ("one effort grammar shared by
parser and formatter"). Three consumers need it: the CLI value parser
(`--effort`), the CLI row render (`TaskRow` effort as `30m`/`1h30m`),
and the UI `effort_phrase` — and the UI crate cannot see the app
crate. Therefore the grammar lives next to `Effort` in domain
(`entities.rs`), as the canonical text form of a domain value — the
existing `ReminderRef::text_form`/`parse_text_form` precedent
(`reminders.rs:83`/:94). This is not a deviation: D-F5 names no home
for the formatter, and doc 4 §7 item 1 delegates exact helper names
to this chunk's output.

- `Effort::parse_text_form(raw: &str) -> Option<Effort>` — grammar:
  - components `<u64 digits><unit>` concatenated, unit ∈ `s|m|h`
    exactly (the design names s/m/h; `d` is the reminders-offset
    grammar's unit and stays there). `1h30m`, `45s`, `2h` valid.
  - a string of bare digits with no unit anywhere = **minutes**
    (`30` → 1800s; the ruled human default).
  - a unit-less component after a unit-carrying one is invalid
    (`1h30` → `None`; unambiguous by construction).
  - no signs, no floats, no whitespace, no empty input (`-30m`,
    `1.5h`, `""` → `None`).
  - accumulate seconds in `u64` with checked arithmetic; overflow →
    `None` (never saturate — a wrapped estimate would be a lie). The
    design's "overflow past `u64` is a clap usage error" is the CLI
    wrapper mapping `None` to `Err`.
  - `none` is **not** in the domain grammar (a "clear" sentinel would
    import CLI intent into the domain); the wrapper owns it.
- `Effort::text_form(self) -> String` — canonical form: descending
  `h`/`m`/`s`, zero components omitted, all-zero → `0s` (the
  `ReminderRow` "canonical (`"10m"`, `"0s"`)" precedent). `5400` →
  `1h30m`, `3600` → `1h`, `59` → `59s`, `0` → `0s`.
- Canonicalization direction (test contract): `parse_text_form(
  text_form(e)) == Some(e)` for every `e`; the inverse holds only for
  canonical inputs (`parse("90")` = 90 min, `text_form` = `1h30m`).
- `cli.rs`: `fn parse_effort(raw: &str) -> Result<Option<Effort>,
  String>` — `"none"` → `Ok(None)`; else
  `Effort::parse_text_form(raw).map(Some).ok_or_else(|| <usage text
  naming the vocabulary>)`. Same `fn(&str) -> Result<_, String>`
  shape as `parse_due` → used as `value_parser`, so failures are clap
  usage errors (exit 2) exactly matching `parse_due`'s failure shape
  (the ruled trap). No `CommandError` arm, no new error type.

### D2 — Level text forms in domain; two parallel inherent impls, no trait

`--importance`/`--urgency` parse the hyphenated vocabulary
`very-low|low|normal|high|very-high` (D-F5: "the parser maps the
hyphenated CLI spelling to the serde variant name" — serde names are
`snake_case`, so the mapping is hand-rolled). The same level names are
needed by the `TaskRow` render, the JSON row, and the UI chip labels —
domain home, same argument as D1:

- `Importance::parse_text_form(&str) -> Option<Self>` /
  `Importance::text_form(self) -> &'static str`; identical pair on
  `Urgency`. Two concrete inherent impls — **no shared trait**: the
  two types are distinct on purpose (D-F1's ruled type separation) and
  no consumer needs genericity.
- `cli.rs`: `parse_importance`/`parse_urgency` wrappers with the
  `parse_effort` shape; `"none"` → `Ok(None)` (unset), anything else
  unmatched → usage error (D-F5: "Unknown levels are clap usage
  errors").

### D3 — `TaskRow` contract: four new fields; the human layout is polish

The fields on the row are the contract (D-F5's own sentence); exact
column layout is stage-2 polish. `TaskRow` (output.rs) grows:

- `scheduled: Option<DateTime<Utc>>` — `duedate`'s twin, ISO-8601 on
  the wire.
- `effort: Option<u64>` — whole seconds, documented on the field.
  (`Effort` is serde-transparent, so `Option<Effort>` would be
  wire-identical; the plain `u64` keeps the projection free of domain
  types per output.rs's "never the domain's serde shapes" rule.)
- `importance: Option<String>`, `urgency: Option<String>` — the
  canonical hyphenated level names (`"very-low"` …), i.e. D2's
  `text_form()`. One level vocabulary across CLI parse, CLI render,
  JSON row, and UI labels; a local mirror enum would be a second
  vocabulary with no reader.

`task_row()` fills them straight from the `Task` (levels via
`text_form()`). Human render (`render_text_list`, polish): after the
existing `(due …)`/marker segments, append — each omitted when the
field is `None` — `  (starts {scheduled})` (mirroring `(due …)`), the
`Effort::text_form()` string, and `{importance}/{urgency}` level
names. Stage 2 may tune spacing/ordering; nothing may re-render a
level in a different spelling.

### D4 — Phrase helpers (presentation.rs, slint-free), mirroring `due_phrase`

- `pub fn scheduled_phrase(scheduled: Option<DateTime<Utc>>, now:
  DateTime<Utc>) -> String` — `None` → empty (the absent-field
  convention, like `due_phrase`); else reuses `duration_bucket`:
  `<60s` → `starts now`; future → `starts in {q}`; past → `started
  {q} ago`. Self-describing words on purpose: unlike due, no tone
  carries the direction. Bucketing edges are already `duration_bucket`'s
  tested contract — this glue is tested for empty/set structure only.
- `pub fn effort_phrase(effort: Option<Effort>) -> String` — `None` →
  empty; else `effort.text_form()`. The UI wraps the domain
  formatter; it never re-implements the grammar (D1's whole point).
- Level labels for chips come from D2's `text_form()`; the
  level→`Tone` mapping is visual-only and lives in models.rs (the
  slint ferry — presentation.rs must stay slint-free). Proposed
  monotone mapping over existing tokens only: `VeryLow`→`neutral`,
  `Low`→`info`, `Normal`→`success`, `High`→`warning`,
  `VeryHigh`→`danger`. The contract is "existing tokens, monotone in
  severity"; the exact assignment is markup polish.
- No slint-side logic: the card's conditionals are empty-string /
  empty-array tests only; every semantic phrase is one of the pure
  functions above.

### D5 — UI carriers: two phrase strings + the existing decorations slot

Doc 4 §7 item 3: field decorations land in `TaskCardModel.decorations`;
no second decoration carrier.

- `TaskCardModel` (task-card.slint) grows `scheduled-phrase: string`
  and `effort-phrase: string` (empty = absent — the `due-phrase`
  pattern). The card renders each in the secondary band (a mono row
  mirroring the due chip, `Theme.text-secondary`, each `if != ""`,
  present at every breakpoint like the due chip — secondary class).
  No new tones for scheduled/effort (no ruled tone semantics).
- importance/urgency ride the **existing** `decorations:
  [TaskDecoration]` slot with the **existing**
  `DecorationKind.importance/urgency` kinds — `label` = the level's
  `text_form()`, `tone` = D4's mapping. `DecorationKind` grows no new
  kinds (no effort/scheduled chips — their phrases carry them), and
  chips stay tertiary (compact collapses them — existing behavior).
- models.rs grows a pure `field_decorations(task: &Task) ->
  Vec<TaskDecoration>` (unit-tested for kinds/tones; labels are never
  UI-text-asserted — their spelling is pinned at the domain layer's
  round-trip tests) and `task_card_model` fills the two phrase fields
  via D4's helpers.

### D6 — The anchor exact-title filter: display-only, applied at the row sources

Rule (DDF-5/doc 1 §7, ruling verbatim): hide a task iff
`task.title == taskboard_domain::ANCHOR_CARD_TITLE` — byte-exact, no
trim, no case-fold, no normalization (a trailing space is a visible
near-miss). The filter is **presentation-layer by ruling** — it never
enters the domain crate (the dispatched trap) and never touches
resolution: `resolve_task` keeps resolving the anchor by exact
title/uuid (the chunk-4 guard is what refuses its edit/delete; hiding
must not orphan that contract).

Application points — this chunk's list surfaces, filtered at the row
source so every derived count follows:

- app: `stack_groups` (commands/tasks.rs) filters before `task_row`
  → covers CLI `tasks list`, the REPL mirror (shared `list_on`), and
  the IPC line path (shared dispatcher).
- ui: `push_board` and `push_stack_detail` (models.rs) filter before
  the single card+table loop — cards and `IdTables` entries filter in
  the **same** pass; a tables/cards desync is a mis-targeting bug.
- ui: `banner_rows` (presentation.rs) filters — the banner is a list
  surface; `banner_overflow` (its derived count) then follows
  automatically.

Counts: no other count surface aggregates task rows today; the ruled
count applications (today sections, inbox chip, palette results) are
chunks 10/9's, whose file lists name the filter explicitly. Not
filtered here (deliberate scope): `tasks remind --list` rows, `labels
list` assigned-task counts, mutation receipts.

UI-side predicate: a one-line helper in presentation.rs
(`is_anchor_task(&Task)`); app-side: the same one-line comparison in
`stack_groups`. One comparison duplicated per presentation side is the
ruled shape (shared means domain, which is the trap).

### D7 — The sidecar badge reads the selected board's derived health

- Read path: `sync_badge_for_board`'s row (`SyncStatus.sidecar:
  SidecarHealth`) for `vs.board()` — the same selected-board rule as
  phase/pending (design §3.2 option (a)); never raw
  `AppDataEvidence`, never the aggregate fold (the dispatched trap:
  "the badge reading anything but the derived health").
- presentation `SyncBadge` grows `sidecar: SidecarHealth` (Copy,
  preserves the struct's `Copy`/`Eq`; default `Healthy` keeps every
  existing construction site honest). A healthy row renders nothing —
  absent evidence is the ruled silence.
- presentation grows `sidecar_caption(health: SidecarHealth) -> String`:
  `Healthy` → empty; `Unreadable(reason)` → the D-S2 ruled wording
  class (the "update this app" message; reason tag vocabulary
  `format_too_new`/`transport`/`decode`/`foreign_board`, mirroring
  chunk 4's `sidecar_view`). A small local match, not a domain
  addition — this chunk's file list keeps domain untouched, and the
  wording is presentation-edge text (status.rs:350 precedent).
- `SyncBadgeModel` (status-bar.slint) grows `sidecar-warning: string`
  (empty = none); status-bar.slint renders a warning-toned caption +
  `Badge` next to the sync indicator when non-empty — the exact shape
  of the existing `startup-findings` block. models.rs pushes
  `sidecar_caption(badge.sidecar)`.

### D8 — Completion grows three value tables; flag names come free

The clap-tree walk offers new long flags automatically. `flag_value_kind`
(completion.rs:259) grows:

- `"scheduled" => Kind::Literals(&["none"])` — the `"due"` mirror.
- `"importance" | "urgency" => Kind::Literals(&["very-low", "low",
  "normal", "high", "very-high", "none"])`.
- `"effort"` needs no entry (free text; `Kind::None` is the default).

### D9 — Flag plumbing: the parser's shape IS the changeset's shape

`task_changes` (commands/mod.rs:1203) grows the four flag params:

- `scheduled: Option<DueSpec>` → mapped exactly like `due`
  (`Set(dt) => Some(dt)`, `Clear => None`) — one parser, two flags
  (D-F5's sentence).
- `effort: Option<Option<Effort>>`, `importance`/
  `urgency: Option<Option<Importance/Urgency>>` — the wrappers'
  return types already equal `TaskChanges`'s double-option slots
  (`None` = untouched, `Some(None)` = clear-and-stamp,
  `Some(Some(_))` = set-and-stamp); the mapping is the identity.
- Zero new `CommandError` variants, zero engine/storage/domain-behavior
  changes — chunk 1's `UpdateTask` arms and chunk 4's lane routing
  already apply every slot.

`--scheduled`/`--effort`/`--importance`/`--urgency` join `Edit`'s flag
set without touching its (absent) arity group — see D10.

### D10 — Recorded observation, not fixed here: Edit's zero-flag doc gap

`tasks.rs:389`'s docstring claims "at least one flag required —
enforced by clap as a usage error", but `TasksCmd::Edit` carries no
required clap group and the engine types an empty changeset as
`no_op()` (command.rs:338) — a zero-flag edit is a silent no-op today.
Pre-existing, predating this chunk; changing Edit's arity enforcement
is a behavior change outside this chunk's ruled surface. This chunk
adds flags without altering that. The stage-3 reviewer should weigh
the docstring under the code-quality mandate (fix the wording or file
the issue — not silently).

## 2. The exact shapes (illustrative, not a diff)

```rust
// domain entities.rs, next to Effort
impl Effort {
    /// Parses the canonical text form: `<n><s|m|h>` components
    /// concatenated, or bare digits = minutes (`30`, `1h30m`, `45s`).
    /// Overflow and garbage are `None`.
    pub fn parse_text_form(raw: &str) -> Option<Self> { /* checked u64 */ }
    /// Canonical form: descending h/m/s, zero parts omitted, `0s` for zero.
    pub fn text_form(self) -> String { /* "1h30m" */ }
}
// Importance / Urgency: parse_text_form(&str) -> Option<Self> and
// text_form(self) -> &'static str over "very-low".."very-high".

// cli.rs
fn parse_effort(raw: &str) -> Result<Option<taskboard_domain::Effort>, String>;
fn parse_importance(raw: &str) -> Result<Option<taskboard_domain::Importance>, String>;
fn parse_urgency(raw: &str) -> Result<Option<taskboard_domain::Urgency>, String>;
// TasksCmd::Edit grows:
//   #[arg(long, value_parser = parse_due)]       scheduled: Option<DueSpec>,
//   #[arg(long, value_parser = parse_effort)]    effort: Option<Option<Effort>>,
//   #[arg(long, value_parser = parse_importance)] importance: Option<Option<Importance>>,
//   #[arg(long, value_parser = parse_urgency)]    urgency: Option<Option<Urgency>>,

// output.rs TaskRow grows:
//   pub scheduled: Option<DateTime<Utc>>,
//   pub effort: Option<u64>,            // whole seconds
//   pub importance: Option<String>,     // "very-low" vocabulary
//   pub urgency: Option<String>,

// presentation.rs
pub fn scheduled_phrase(scheduled: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String;
pub fn effort_phrase(effort: Option<Effort>) -> String;
pub fn sidecar_caption(health: SidecarHealth) -> String;
pub fn is_anchor_task(task: &Task) -> bool;

// models.rs
fn field_decorations(task: &Task) -> Vec<TaskDecoration>; // kinds/tones only
// SyncBadge grows `sidecar: SidecarHealth`; status bar pushes
// `sidecar-warning: string` = sidecar_caption(badge.sidecar).
```

Clap fallback (recorded): if the derive rejects
`Option<Option<T>>` with a custom value parser, the wrappers return a
tiny `EffortSpec`/`LevelSpec` enum mirroring `DueSpec` instead — same
mapping into `TaskChanges`, no wire change. Verified-unlikely; try the
double-option first.

## 3. TDD ordering (test first at every step; commits standalone)

1. **Domain text forms** — tests: effort grammar table (bare =
   minutes; `s`/`m`/`h` compose; `1h30` invalid; signs/floats/empty
   invalid; component/total overflow → `None`), canonical-form table
   (`5400 → "1h30m"`, `0 → "0s"`), round-trip proptest over a
   bounded-seconds strategy; level tables + all-values round-trip;
   domain parse rejects `none`. Then impl. Commit
   `feat(domain): the effort and level text forms`.
2. **CLI flags** — tests: wrapper units (`none`/vocabulary/garbage);
   `Cli::try_parse_from` lands each flag on the Edit arm and usage-
   errors on bad values (the house cli.rs test style); `task_changes`
   flag→slot mapping incl. clear-vs-untouched and a mixed `--due`
   changeset. Then impl. Commit
   `feat(app): the task-field edit flags`.
3. **TaskRow** — tests: o1-style payload shape (four fields' wire
   types/values present-or-null), `render_text_list` segments via
   row-level structure. Then impl. Commit
   `feat(app): the task row field projections`.
4. **Completion** — literal-table entries; fold into commit 2 or a
   one-line `feat(app)` follow-up only if separation aids review.
5. **UI presentation** — tests: `scheduled_phrase`/`effort_phrase`
   empty-vs-set structure, `sidecar_caption` per variant,
   `is_anchor_task` exact-vs-near-miss, `banner_rows` filtering. Then
   impl. Commit `feat(ui-slint): presentation phrases, the anchor
   filter, the sidecar caption`.
6. **UI models + slint** — tests: `field_decorations` kinds/tones per
   level; card model phrase wiring; badge push wiring. Then TaskCard/
   status-bar model+markup growth. Commit
   `feat(ui-slint): field decorations on the card, the sidecar badge`.
7. **Journeys + bookkeeping** — cli_editing legs (round-trip + clear
   per flag; anchor hidden from `tasks list` while still resolvable —
   an edit attempt on it yields `anchor-task-reserved`, proving both);
   REPL one-line mirror; UI journey legs (card counts exclude the
   anchor; the badge warns when the fake engine's row carries
   `Unreadable`); CHANGELOG Unreleased line. Commit
   `test(app): the field-surface journeys` +
   `test(ui-slint): the decorated-card and badge journeys` +
   `docs(changelog): the field surfaces, anchor hiding, sidecar badge`.

Snapshot/golden rule: no domain/registry/`AppState` shape changes →
**no** `insta` pass. Goldens change only if stage 2 adds decorated
card fixtures — then the regen is deliberate and recorded in the stage
report.

## 4. Typed-outcome test matrix (behaviour → layer)

| Behaviour | Layer / test |
|---|---|
| Effort grammar + canonical form + round-trip | domain `entities.rs` units + proptest |
| Level vocabulary round-trips, `none` rejected by domain | domain `entities.rs` units |
| `--effort 1h30m` → `TaskChanges.effort = Some(Some(5400s))`; `--effort none` → `Some(None)`; absent → `None` (×4 flags) | app `commands/mod.rs` mapping units + cli parse tests |
| Bad level/effort = clap usage error | app cli.rs `try_parse_from` `is_err` units |
| Row carries the four fields (types, absence) | app `cli_output.rs` payload test |
| CLI/REPL edit round-trips each field; `none` clears | app `cli_editing.rs` journeys (typed `TaskRow` fields) |
| Anchor absent from `tasks list` rows; near-miss visible; still resolvable (guard fires) | app `cli_editing.rs` journey |
| Phrases empty-when-absent, set-when-present; caption per health variant; `is_anchor_task` exact-only | ui `presentation.rs` units (+ existing proptest style) |
| Cards/tables exclude the anchor in the same pass; badge carries the selected board's health | ui `models.rs` units + `journeys.rs` legs |
| Decorations carry kind/tone per level; labels come from the domain text form (never UI-text-asserted) | ui `models.rs` units |
| Completion offers `none`/levels for the new flags | app `completion.rs` units (existing table style) |

The text-assertion boundary (AGENTS §5), made concrete for this
chunk: **assertable** — contract field values in the JSON payload
(the `format_too_new` precedent) and text-form codec outputs, whose
spelling *is* the contract (the `text_form_roundtrips` precedent);
**never assertable** — error messages, sync phase text, rendered UI
label/wording (badge caption, phrase text beyond empty/set, chip
labels).

## 5. Risks & mitigations

- **clap double-option parsing** (D9's fallback): `Option<Option<T>>`
  with a custom value parser is expected to work; if the derive
  refuses, switch to `DueSpec`-style spec enums (recorded, mechanical).
- **Tables/cards desync** if the anchor filter is applied to cards but
  not `IdTables` (or vice versa): filter once, before the loop, in the
  same pass (D6); the journey leg pins counts on both surfaces.
- **UI harness reach** for the badge: the fake engine must expose a
  per-board row with `SidecarHealth::Unreadable` — extending
  `tests/support/fake_engine.rs` is in-scope test support; if the
  mount-level assertion proves brittle, the caption mapping unit +
  models wiring unit carry the guarantee.
- **Grammar near-duplication** with reminders' `parse_before`
  (`s|m|h|d` offsets): deliberate non-sharing — different units, bare-
  number semantics, and failure homes; a shared "duration" abstraction
  would couple estimation to reminder offsets.
- **Worktree volatility**: this session's worktree was removed
  externally once and recreated per the AGENTS lifecycle; stage 2's
  prompt already says "recreate if missing" — run
  `scripts/worktree-doctor.sh` first as always.
- **Text-assertion creep** in a surfaces chunk: the matrix above
  draws the boundary; stage 3 polices it.

## 6. Estimated diff size

~750–850 LOC all-in: domain +~170 (text forms + tests), app +~230
(flags, mapping, TaskRow, completion + units), ui +~230 (phrases,
filter, badge, decorations + units), slint +~45 (two model growths +
two render sites), integration/journey tests +~120, CHANGELOG ~+6.
Consistent with the dispatch plan's **M** sizing.

## 7. Out of scope (chunk boundaries, restated)

No new verbs (field-specific verbs are a finding); no `tasks add`
flags (D-V6's ruled answer); no today/search/palette filtering
(chunks 10/9 own their surfaces); no task-detail-view field growth;
no slint-side semantics; no domain/engine/storage behavior changes
beyond the two text-form surfaces; no tracker edits (MD-4 — stage 3
posts the one #209 progress comment with the PR URL); no Edit arity
enforcement change (D10).

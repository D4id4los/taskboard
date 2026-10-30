# [Plan] Track CLI family & export verb (DR-5/3) — `taskboard track …`, `taskboard export`, typed receipts, raw-payload relay

- **Date**: 2026-10-29
- **Kind**: stage-1 implementation plan for chunk **DR-5/3** of the
  design-rounds batch dispatch
  (`.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §11.2; wave 8, issue **#488**). TDD-first; no implementation in this
  commit.
- **Origin (normative)**: the accepted design round
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md` —
  §3 Part E, **D-T14** (the `track` family: verb list, selector reuse,
  entry-id resolution, the tracked-today projection, duration
  rendering, the IpcOrLocal lane) and **D-T15** (the `export` verb:
  stdout, CSV default / JSON, inclusive start filters, the raw-payload
  envelope exception, the verbatim daemon relay) decision by decision;
  §1's CLI-recipe anchors (the timers two-lane recipe, the
  `route_for_ipc` exhaustiveness, the main.rs relay); §5's CLI/app
  test hooks; §7 chunk 3. The dispatch plan §3.4 fences bind: no
  `[tracking]` config, nothing ticks, #490–#496 stay fenced, DR-4
  owns merge semantics (this chunk reads state, never merges).
- **Branch / worktree**: `feat/track-cli-export` in
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-track-cli-export`
  (canonical sibling path; `/home/viktoria/src/taskboard` is a symlink
  view of the main checkout), branched off `origin/main` @ `f67c9bc0`.
- **Preconditions verified (2026-10-29, `git log origin/main`)**:
  - **DR-5/1 merged** — `2c41d92` (entity + TrackingState), `ab34153`
    (the five verbs with typed receipts), `a1c5440` (done/deleted
    close), `9d17d7d` (projections incl. `tracked_in_window`),
    `3e94f74` (engine tier), `548ecad` (ADR 0017).
  - **DR-5/2 merged** — `d48c640` (time-entries migration + load
    codec), `836ef6d` (sidecar section), `4d5263b` (overlay builder),
    `3175f6e` (document fold), `59f6b08` (flush carries the section).
  - **Migration-number deviation, recorded**: the dispatch §2 table
    assigned 0013 to DR-5/2; the actual landing took **0015** (next
    free at its rebase — the recorded-deviation rule). This chunk
    claims **no migration, no ADR, no resource**; the deviation
    touches nothing here.
- **Status**: **READY FOR STAGE 2** — every open question is resolved
  in §4 (D-TC1–D-TC16); no rulings outstanding.

---

## 1. Verified anchors (2026-10-29 against the base `f67c9bc0`; symbols are the stable keys, line numbers are THIS base's)

| Fact | Anchor | Evidence |
|---|---|---|
| The five verbs exist; their payloads are exactly what the CLI forwards | domain `messages.rs` | `StartTracking { task }` :313, `StopTracking` :319, `LogTime { task, start, end }` :325, `AdjustTimeEntry { id, start: Option<_>, end: Option<_> }` :340 (change mask, never a value — clearing an end is unexpressible), `DeleteTimeEntry { id }` :352 (unknown id = accepted no-op) |
| Typed outcomes the receipts project from | domain `command.rs` | `TrackingStarted { entry, closed }` :170, `TrackingStopped { entries }` :177, `TimeLogged { entry }` :182, `EntryAdjusted { entry }` :187, `EntryDeleted { id }` :192; `NoOp` the idempotent shape; planner maps `ClosedSpan::try_new` failures to `CommandError::InvalidSpan` (:2043, :2088) |
| The app error-kind table already speaks tracking | app `commands/mod.rs` | `domain_error_kind` :520: `D::UnknownTimeEntry → "unknown-time-entry"`, `D::InvalidSpan → "invalid-span"` — the envelope kinds exist today; the CLI-side selector errors are the only new kinds |
| The read side | domain `tracking.rs` | `TrackingState.entries: BTreeMap` :62; `open_entries()` :128 (derived, no `running` field); `newest_open()` :135 (start, id tie-break); `TimeEntry::duration(now)` :120 (clamped ≥ 0); `tracked_in_window(entries, window, now)` :148 — clips each span to the window and sums (the split-at-boundary projection; rows never physically split); `TimeEntry.task_title` the immutable snapshot |
| Day-window seam | domain `days.rs`, app `config.rs` | `day_window(now, NaiveTime, FixedOffset)` :41; the D-F6 consumer contract (consumers receive `(day_start_time, offset)` explicitly); `AppConfig::day_calendar()` :1245 — the validated-parse-`expect` + `*chrono::Local::now().offset()` precedent, "never on a validated config" |
| The two-lane recipe to mirror | app `commands/timers.rs` | one-shot wrappers over `tasks::with_core` (:25-60), `*_on` session seams (:76-205), `resolve_alarm` :154 (exact uuid → unique ≥8-char lowercase prefix → label pass), `TimersInvocation` + `collapse()` :306 (the ONE definition site; `None` only for the clap-unreachable vacuous group), `timers_command` :334 (one-shot dispatch over `run_core`), `timers_line` :374 (session lane over `emit_result`), renders read state truth back (:93-99: "the stamped value is state truth … not an input echo") |
| Task-selector reuse | app `commands/tasks.rs` | `resolve_task` :600 — exact uuid → unique ≥8-char prefix → exact **live**-title match; typed `UnknownTask`/`AmbiguousTask { candidates }`; `resolve_live_board` lives at `commands/mod.rs` :2008 (exact uuid \| exact title among live boards) |
| Routing is compiler-forced | app `commands/mod.rs` | `IpcRoute` :1857; `route_for_ipc` :1868 exhaustive — `Command::Timers { .. }` rides `IpcOrLocal` :1888 (the mutating-verb precedent); the routing-table test `ipc_routing_tests` :2042 pins lanes per command; `verb_timeout_secs` :1904 has a `_ => None` wildcard (no edit needed — the new verbs carry no `--timeout-secs`) |
| Both dispatch tables are exhaustive over `Command` | app `commands/mod.rs` | one-shot `dispatch` :649 (Timers arm :1076 collapses then routes to `timers_command`); session `dispatch_line` :1419 (Timers arm :1461 → `timers_line`) — two new `Command` variants force two arms each |
| The daemon relay is byte-verbatim | app `main.rs`, `ipc/server.rs` | one-shot IpcOrLocal arm :141: `client::try_connect` → `conn.exec(&raw_argv, budget)` → `relay(&outcome)` :250 writes `outcome.out`/`.err` verbatim and exits with the daemon's code; the daemon side `run_argv` :560 parses via `repl::resolve_words` (the daemon is the single parse authority) and captures `dispatch_line`'s `CommandIo` buffers — whatever a verb writes to `io.out` reaches the client's stdout byte-for-byte |
| Typed output contract | app `commands/output.rs`, `mod.rs` | projections are app-owned serde shapes (never domain serde); `emit_to` :1950 — one JSON line on `out` under `--json`, the human render otherwise; `report_to` :1967 — `{"error":{"kind","detail"}}` on `err`, exit 1; `NoOp` receipts reflect the current state (`tasks.rs` :265 precedent) |
| The test shape to copy | app `tests/cli_timers.rs` | hermetic standalone tempdir boots (`standalone_config` :26 + `common::tempdir` + `FakeCredentialStore`), one invocation = one boot/work/shutdown process, durability across boots, receipts asserted as typed values, refusals via `matches!`/`error_kind()`, the session lane via `dispatch_line` + `BasicSession` (T7 :264), clap rules without boots (T6 :237). The binary relay harness lives in `tests/cli_ipc.rs` (`spawn_daemon`, `run(args) -> (code, out, err)`, `wait_ready`) |
| Completion derives from the clap catalogue | app `completion.rs` | the spec walk `CompletionSpec::from_cli()` auto-offers new verbs (the catalogue test's verb assertions are membership-`any` :1119); positional id → kind (`"task" => Kind::Tasks` :281); **`timers cancel` is `Kind::None` — "no dynamic alarm completion (the D17 static-tokens ruling)"** :1241 |
| Parse-fn precedents | app `cli.rs` | `parse_due` :444 (RFC-3339 with explicit offset → UTC, bare `YYYY-MM-DD` → midnight UTC, `none` clears), `parse_before_flag` :543 (the `[reminders] default_before` grammar, reused by `timers --in`), `parse_at_flag` :559 (the host-local wake-up grammar — deliberately divergent, NOT reused for span stamps), `FindType` :163 (the clap `ValueEnum` precedent) |

---

## 2. Scope & non-goals

**In scope (app crate + tests + this plan's docs only):** the two new
top-level `Command` variants (`Track`, `Export`) with their clap
types; the new `commands/track.rs` module (wrappers, `*_on` seams,
`collapse`, `resolve_time_entry`, projections glue, renders, the CSV
emitter, `duration_phrase`); the `commands/mod.rs` dispatch /
`dispatch_line` / `route_for_ipc` arms and routing-test rows; the
`output.rs` receipt projections; the small `config.rs`
`resolved_day_calendar()` refactor; the completion rows; the new
`tests/cli_track.rs` suite; the `cli_ipc.rs` relay-leg journey.

**Non-goals:** any domain-crate diff (the verbs, projections, and
state home landed in DR-5/1); any storage diff (no migration, no
codec change — DR-5/2's table is read through the engine only); any
ui-slint diff (DR-5/4 owns the chip/badge/button surfaces — its
commits are already in main); a `[tracking]` config section (fenced);
anything that ticks (elapsed is derived at read; `status` computes
`now` at the command layer, the timers `list_on` precedent);
statistics/journal exports (fenced — they reuse this verb later per
D-T15); HH:MM span-boundary grammar (D-TC7); dynamic entry-id
completion (the D17 ruling, D-TC15); undo of tracking writes (undo-v2
fence); sync-lane behavior of any kind (fold/flush are chunk 2's,
already landed).

**Explicit fence honoured:** the export payload bypasses the `--json`
receipt envelope (D-T15's recorded exception) — this is the one
output-contract deviation in the chunk, pinned by its own test
(D-TC10).

---

## 3. The CLI surface (D-T14/D-T15, verb by verb)

### 3.1 `cli.rs` additions

```
Command::Track { cmd: TrackCmd }          // new top-level variant
Command::Export { …flags… }               // new top-level variant (no subcommands)

TrackCmd:
  Start { task: String }                                          // resolve_task selector
  Stop
  Status
  List  { task: Option<String>, since: Option<NaiveDate> }        // --task, --since
  Log   { task: String, for_: Option<TimeDelta>,
          from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>> } // exactly one mode
  Adjust { entry: String, start: Option<DateTime<Utc>>,
           end: Option<DateTime<Utc>> }                           // change mask
  Delete { entry: String }

Export { format: Option<ExportFormat>, since: Option<NaiveDate>,
         until: Option<NaiveDate>, task: Option<String>,
         board: Option<String> }          // ExportFormat: csv (default) | json, a clap ValueEnum
```

New parse fns (the `parse_due` shape, `cli.rs` test module each):

- `parse_span_stamp(raw) -> Result<DateTime<Utc>, String>` — RFC-3339
  with an explicit offset (normalized to UTC) or bare `YYYY-MM-DD`
  (that date's midnight UTC). **No `none`** (a span boundary is never
  "clear") and **no HH:MM** (D-TC7). Used by `log --from/--to` and
  `adjust --start/--end`.
- `parse_date_flag(raw) -> Result<chrono::NaiveDate, String>` — strict
  `YYYY-MM-DD`. Used by `list --since` and `export --since/--until`.

Clap mode rules on `Log` (the `timers add --in/--at` shape): `--for`
`conflicts_with_all = ["from", "to"]`; `--from` `requires = "to"`;
`--to` `requires = "from"`; a required group makes the vacuous line a
usage error. Malformed values are clap usage errors (exit 2) —
grammar at the parse edge, never a runtime string compare.

### 3.2 `commands/track.rs` (new module — the timers shape)

- One-shot wrappers `start/stop/status/list/log/adjust/delete(config,
  store, …) -> Result<XxxReceipt, CommandError>` over
  `tasks::with_core`; session seams `*_on(&crate::App, …)`; the
  collapsed-invocation mode switches `track_command(io, config_path,
  invocation)` (over `run_core`) and `track_line(app, day, io,
  invocation)` — the two lanes consume ONLY the collapsed enum.
- `collapse(cmd: TrackCmd) -> Option<TrackInvocation>` — the single
  definition site (D-TC2). `Log` collapses to
  `LogMode::For(TimeDelta) | LogMode::Span(from, to)`; `None` is
  returned only for the clap-unreachable vacuous log group (totality
  for direct calls, the `timers::collapse` comment).
- `resolve_time_entry(state, selector) -> Result<TimeEntryId,
  CommandError>` — exact uuid → unique ≥8-char lowercase uuid prefix
  → typed miss. No label pass (entries have no title). Unit-tested
  with hand-built uuids (D-TC4).
- `export` lane fns: `export(config, store, query) ->
  Result<ExportPayload, CommandError>` (one-shot) and
  `export_on(app, query)` (session seam); `ExportPayload` renders to
  bytes at the emit site (D-TC10) — the payload is data, so the
  module exposes `render_csv(rows) -> String` / `render_json(rows) ->
  String` as pure functions over `Vec<ExportRow>`.
- `duration_phrase(TimeDelta) -> String` — the human duration
  vocabulary: `<h>h <m>m` composition at minute granularity
  (`90m → "1h 30m"`, `45s → "0m"`, `0 → "0m"`; v1 is
  minute-granular by the round's own non-goals). Unit-tested.
- `csv_field(&str) -> String` — minimal RFC-4180 quoting: wrap and
  double-quote only when the field contains `,` `"` `\n` or `\r`.
  Hand-rolled, no new dependency (`cargo deny` untouched — §5's
  hook). Unit-tested.
- Renders `render_text_*` per verb, reading state truth back (never
  input echoes): `status` prints the open rows (title, elapsed
  phrase, entry id) and the tracked-today total in both phrase and
  whole-seconds forms.

### 3.3 Receipts (`output.rs`)

One shared entry projection — exactly the six machine columns the
whole family and export speak:

```rust
pub struct TrackEntryRow {            // end: null IS the open marker
    pub entry_id: String,             // local uuid
    pub task_id: String,              // local task uuid
    pub task_title: String,           // the immutable snapshot
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,   // None = open = the running timer
    pub duration_seconds: u64,        // derived at projection time (now passed in), clamped ≥ 0
}

pub struct TrackStartReceipt  { pub started: bool, pub entry: TrackEntryRow, pub closed: Vec<String> }
pub struct TrackStopReceipt   { pub stopped: bool, pub entries: Vec<TrackEntryRow> }
pub struct TrackLogReceipt    { pub entry: TrackEntryRow }
pub struct TrackAdjustReceipt { pub changed: bool, pub entry: TrackEntryRow }
pub struct TrackDeleteReceipt { pub entry_id: String, pub deleted: bool }
pub struct TrackStatusView    { pub open: Vec<TrackEntryRow>, pub tracked_today_seconds: u64 }
pub struct TrackListView      { pub entries: Vec<TrackEntryRow> }
```

`started/changed/deleted == false` are the `NoOp` projections — the
receipt reflects the current state read back from the published
snapshot (the `tasks.rs` :265 precedent), never an input echo.

Export's row type is the same six fields (`ExportRow` = `TrackEntryRow`
by value or type alias — stage 2 picks whichever reads better under
the SRP line; the JSON object shape is the contract, not the type
name).

---

## 4. Design — every open question, resolved

- **D-TC1 — Lanes: `Track` and `Export` are both `IpcOrLocal`.**
  Every tracking mutation must land in the process that owns the
  engine (a one-shot local boot would open an entry and orphan the
  intent at exit — the timers precedent routes identically); export
  reads the same engine state, and splitting the read off onto a
  local boot would report a *different* database's history when a
  daemon serves the db. Pin both rows in `ipc_routing_tests` (the
  compiler already forces the match arms; the test pins the choice).
  `verb_timeout_secs` needs no edit (wildcard `None` — no
  `--timeout-secs` on these verbs); `install_for`'s signal-watcher
  wildcard already covers them.
- **D-TC2 — One `collapse()` definition site** in `commands/track.rs`,
  consumed by both lanes (`track_command`, `track_line`); duplicated
  per-verb wrapper logic is the recipe's named hazard (dispatch
  §11.2's review focus). `Export` has no subcommands, so it has no
  collapse — its flag bag is the invocation.
- **D-TC3 — Mutating verbs project receipts from post-state** (the
  timers `add_on` pattern): execute the command, then read the entry
  back from `shared_state()` where the outcome names it
  (`TrackingStarted.entry`, `TimeLogged.entry`, `EntryAdjusted.entry`)
  — the `duration_seconds` field is derived at projection time, so a
  receipt is state truth, not a stale echo. `stop`/`start` NoOps
  (nothing open / already tracking this task) project the same
  receipts with the boolean false and the state's current rows.
- **D-TC4 — `resolve_time_entry` is `resolve_alarm` minus the label
  pass**: exact uuid → unique ≥8-char lowercase uuid prefix → typed
  miss. Entries carry no title of their own (`task_title` is the
  snapshot of *another* entity — a label-style pass would silently
  match across tasks). New app-side errors
  `CommandError::UnknownTimeEntry { selector }` (kind
  `unknown-time-entry`, detail = the selector) and
  `CommandError::AmbiguousTimeEntry { candidates: Vec<String> }`
  (kind `ambiguous-time-entry`, detail = the candidate uuids) —
  slotting into the existing `Unknown*/Ambiguous*` family and their
  `error_kind`/`error_detail` arms. Note: the *domain*
  `CommandError::UnknownTimeEntry(TimeEntryId)` already exists and
  keeps its kind via `domain_error_kind`; the app-side variant only
  serves selector resolution before a command is built.
- **D-TC5 — `track status` builds the day window from the validated
  config, passed as data.** Add
  `AppConfig::resolved_day_calendar() -> taskboard_domain::DayCalendar`
  (validated `day_start_time` parse-`expect` + `*chrono::Local::now().offset()`
  — the `day_calendar()` body, extracted; `day_calendar()` then
  builds `DayRollConfig` from it — a pure refactor, no behavior
  change). The one-shot wrapper and `dispatch_line`'s arm each call
  it once per invocation and hand the `DayCalendar` into
  `status_on(app, day, …)` (the D-F6 consumer contract). The total
  is `tracked_in_window(state.tracking.entries.values(),
  day_window(now, day.day_start_time, day.offset), now)` reported as
  whole seconds; the human render adds the phrase form. `open`
  lists **all** open entries, newest-first (`newest_open` ordering
  logic applied over the open set): D-T8 rules that `track status`
  lists all — the single-pick is the UI chip's concern, and the
  transient double-open state is shown honestly.
- **D-TC6 — `track list` order and filters.** Canonical
  `(start, id)` ascending (the `timers list` `(fire_at, id)`
  convention transposed to the entry's own axis). `--task` resolves
  through `resolve_task` and therefore only matches **live** tasks —
  recorded: history of soft-deleted tasks stays reachable via the
  unfiltered list and via export; a dead task's selector resolving
  nowhere is `resolve_task`'s existing contract, not a new one.
  `--since <date>` keeps entries with `start >=` that date's midnight
  UTC (D-T14 lists no `--until` on `list` — none is added).
- **D-TC7 — Span stamps are absolute, never wall-clock guesses.**
  `log --from/--to` and `adjust --start/--end` accept RFC-3339 with
  explicit offset or bare `YYYY-MM-DD` (UTC midnight) — the
  `parse_due` grammar minus `none`. `parse_at_flag`'s host-local
  HH:MM + tomorrow-rollover grammar is deliberately **not** reused:
  its rollover semantic is a wake-up promise, exactly wrong for
  retroactive history; a time-only span boundary is day-ambiguous.
  Fence note (not built): HH:MM-for-today is a usability follow-up
  if real use asks.
- **D-TC8 — Span *validity* is the planner's law, not clap's.**
  Cross-flag checks (e.g. `--from` after `--to`, a zero `--for`) do
  not live in `collapse()` — a validation failure there would have to
  return `None` and turn a user error into an `unreachable!()` panic,
  poisoning the total-collapse contract. The clap group enforces only
  *shape* (exactly one mode, paired flags); the domain's
  `ClosedSpan::try_new` rejects `end ≤ start` and the engine surfaces
  `CommandError::InvalidSpan` (kind `invalid-span`, exit 1, typed
  envelope). A zero/negative `--for` is therefore a typed
  `invalid-span`, consistent with `adjust` producing the same kind.
- **D-TC9 — `adjust` and `delete` take explicit ids, no
  confirmation.** Zero-flag `adjust` executes and reports
  `changed: false` (the tasks-edit zero-flag no-op precedent — it is
  a typed NoOp, not a usage error). `track delete` repeats as
  `deleted: false`: an unknown id is the accepted no-op by domain
  ruling (`DeleteTimeEntry`'s payload contract — a synced tombstone
  may have folded the row first), so unlike `timers cancel` this is
  NOT terminal-by-deletion. Deletes roam via chunk 2's tombstones —
  the plan's risk note about fat-fingered deletes is answered by the
  explicit-id rule (no wildcards) and stays out of scope here.
- **D-TC10 — Export bypasses the envelope; the relay still carries it
  byte-verbatim.** The export lane writes `render_csv`/`render_json`
  bytes to `io.out` **regardless of `io.json`** — `--json` changes
  nothing about the payload (it stays a transport flag; `--format
  json` selects the payload format; the two flags do not couple).
  Failures still ride `report_to`'s typed envelope on `io.err` with
  exit 1. This works unchanged under the daemon because `run_argv`
  captures the `CommandIo` buffers and `main.rs::relay` prints them
  verbatim — no relay change at all. Pinned twice: a hermetic test
  asserting identical stdout bytes under `json: true` and
  `json: false`, and the binary relay journey (§5 T17).
- **D-TC11 — Export's contract, column by column.** Default CSV with
  a header row `entry_id,task_id,task_title,start,end,duration_seconds`;
  timestamps RFC-3339 via `to_rfc3339()` (the app render convention);
  an open entry's `end` cell is **empty** and its
  `duration_seconds` accrues to the export instant (`duration(now)`,
  clamped); JSON is a single-line serde_json array of the same six
  fields (`"end": null` for open rows), newline-terminated — `jq` is
  the consumer. Order: canonical `(start, id)` ascending —
  deterministic goldens. Zero rows: header-only CSV / `[]` — success,
  exit 0.
- **D-TC12 — Export filters are UTC-day arithmetic on `entry.start`,
  inclusive of the named day.** `--since D`: `start >= D 00:00 UTC`;
  `--until D`: `start < (D+1) 00:00 UTC` — an entry starting any time
  on the `--until` date is included ("filters are inclusive on entry
  start", D-T15). Recorded: the filter convention is deliberately
  plain-UTC (deterministic for scripted pipelines), while the
  `[preferences] day_start_time` window stays a **display** concern
  used exactly where the design anchors it (`status`'s tracked-today
  total). `--task` reuses `resolve_task`; `--board` reuses
  `resolve_live_board` and keeps entries whose task's stack belongs
  to the board (soft-deleted tasks keep their stack ref, so
  attribution survives deletion). Rows whose task id is absent from
  `state.tasks` (e.g. after a board reset) export with their
  snapshot title and stored task id; `--board` filtering skips them
  (no attribution to resolve); `--task` filtering can never select
  them (the selector would not resolve).
- **D-TC13 — The `track`/`export` verbs are REPL-native with no
  interception.** `dispatch_line` gains the two arms (collapse →
  `track_line`; export → `export_line`); `repl.rs`'s intercept list
  is untouched. The session `status` arm receives the day calendar
  from `session.session_config()` via the same
  `resolved_day_calendar()` helper (D-TC5).
- **D-TC14 — No state shape moves.** The chunk adds no
  `StateCommand`, no `PersistenceAction`, no persisted field, no
  migration: **no insta regeneration** is expected, and any
  `.snap` diff in the gate run is a finding against the chunk. The
  diff is confined to `taskboard-app` (+ this plan); `cargo deny`
  is untouched (no new dependencies — the CSV writer and duration
  phrases are hand-rolled pure fns).
- **D-TC15 — Completion rides the catalogue conventions.**
  `CompletionSpec::from_cli()` auto-offers the new verbs; the
  catalogue test's pin lists gain `"track"`/`"export"`;
  `["track","start"]` and `["track","log"]` positional `task` →
  `Kind::Tasks` (parity with `resolve_task` — a title is offered iff
  the selector would accept it); `["track","adjust"]`/
  `["track","delete"]` entry positionals → `Kind::None` (the
  `timers cancel` D17 static-tokens ruling: no dynamic entry-id
  completion). Export/`track list` value flags ride the default
  (no rows) — matching the existing `--stack`-style coverage.
- **D-TC16 — Exit codes stay the house table.** Success 0 (including
  NoOp receipts and empty exports); typed runtime failures 1
  (selector misses/ambiguities, `invalid-span`, unknown selector
  kinds); clap usage errors 2 (mode-group violations, malformed
  stamps/dates/format values). No new exit codes.

---

## 5. TDD plan (stage-2 execution order; every phase lands green)

**Phase A — pure functions (no boot; unit modules in
`commands/track.rs` + `cli.rs`):**

1. `duration_phrase`: composition (`90m → "1h 30m"`), truncation
   (`45s → "0m"`), zero (`→ "0m"`), hours-only (`60m → "1h 0m"`).
2. `csv_field`: plain passes through; comma, double-quote, newline,
   CR each trigger wrapping; embedded quotes double (the nasty-title
   golden builds on this).
3. `parse_span_stamp`: RFC-3339 with offset normalizes to UTC;
   bare date → midnight UTC; garbage/`none`/`HH:MM` are usage errors.
4. `parse_date_flag`: `YYYY-MM-DD` parses; garbage is a usage error.
5. `resolve_time_entry`: exact-uuid hit; unique ≥8-char prefix hit;
   multi-hit → `AmbiguousTimeEntry` naming both candidates (hand-built
   `TrackingState` with `Uuid::from_u128` twins sharing an 8-char
   prefix — minted v7s cannot be forced to collide hermetically);
   miss → `UnknownTimeEntry`; a <8-char non-uuid selector is a miss,
   not an ambiguity.
6. `collapse`: the three log modes collapse (`--for`; `--from+--to`);
   the vacuous log group → `None` (the clap-unreachable branch);
   every other subcommand collapses to its variant.
7. Export row projection + `render_csv`/`render_json` goldens over a
   hand-built `Vec<ExportRow>` with **fixed timestamps** (exact bytes:
   header, RFC-3339 stamps, the empty-`end` open row, a
   comma+quote title, canonical order; JSON `null` end) — goldens are
   data, constructed once, asserted verbatim.

**Phase B — hermetic boots (`tests/cli_track.rs`, the T-series
shape):** standalone tempdir config + forbidden store; every test a
full boot/work/shutdown process; durability across boots; typed
receipts; refusals via `matches!`/`error_kind()`.

- **T1** `track start` stamps an open entry (state truth: the
  receipt's `start` sits in `[before, now]`), the entry survives a
  fresh boot (`status` sees it, `duration_seconds` grew) — the
  crash-resume leg through the CLI (ADR 0017's open-row-is-the-intent,
  now pinned at the surface).
- **T2** starting B closes A: the receipt's `closed` names A's entry
  id; exactly one open entry remains.
- **T3** the idempotent repeat: starting the already-tracked task
  again yields `started: false`, the same entry id, empty `closed`.
- **T4** `track stop` closes (receipt carries the stamped entries);
  the repeat yields `stopped: false` with no entries.
- **T5** `track log` both modes land closed entries (`--for 90m`;
  `--from/--to` fixed stamps — the receipt + list row carry the
  exact span); an inverted span (`--to` before `--from`) is the typed
  `invalid-span` failure (exit 1), never a boot-time refusal.
- **T6** `track adjust` on an open entry with `--end` closes it
  (`changed: true`, `end` set post-state); a zero-flag adjust is
  `changed: false` (the typed NoOp); `--start` moves a closed span.
- **T7** `track delete` removes (`deleted: true`); the repeat is
  `deleted: false` (D-TC9's honest no-op, contrasted with
  terminal-by-deletion on purpose).
- **T8** entry selectors through the boot lane: full uuid and ≥8-char
  prefix resolve to the same row; a miss is the typed
  `unknown-time-entry`.
- **T9** `track list`: canonical `(start, id)` order; `--task` filters
  (the other task's entries vanish); `--since` cuts by start.
- **T10** `track status` over a constructed day: entries logged around
  a **non-midnight** day start (explicit `DayCalendar` with a fixed
  `FixedOffset` — host-TZ-independent): a closed entry spanning the
  window boundary contributes only its in-window part (the
  `tracked_in_window` projection through the CLI — entries are never
  split), the open entry accrues to `now`, `tracked_today_seconds`
  is the exact sum.
- **T11** the session lane: `dispatch_line` routes every track verb +
  export over the `*_on` seams on one booted session (the timers-T7
  shape) — JSON payload lines on stdout, the failure line's typed
  envelope on stderr.
- **T12** clap rules without boots: the log mode group (both modes /
  neither / unpaired `--from` are usage errors), malformed
  stamps/dates/`--format` values are usage errors, `--format json`
  parses as the `ExportFormat` ValueEnum.

**Phase C — export end-to-end + the relay leg:**

- **T13** hermetic export journey: seed tasks + fixed-stamp
  `log --from/--to` entries + one open entry → `export` (default CSV)
  → assert the full byte golden for the closed rows (the open row's
  `end` cell empty, `duration_seconds` numeric) — the Phase-A golden
  composed through a real boot; `--format json` → the array with
  `"end": null`.
- **T14** filters through the boot: `--task`, `--board`, and the
  `--since`/`--until` inclusive boundary (an entry starting exactly
  at `--since` midnight is in; one starting the day after `--until`
  is out).
- **T15** the envelope exception: the export payload bytes are
  identical under `json: false` and `json: true`; a failing export
  (unresolvable `--task`) rides the stderr envelope with exit 1 in
  both modes.
- **T16** (`cli_ipc.rs`) the relay leg: daemon up (`wait_ready`) →
  seed via the socket (`track start`/`track log` relayed, receipts
  byte-shaped as the one-shot prints them) → `taskboard export
  --format csv` through the socket → stdout equals the in-process
  export's bytes, exit 0 → `track stop` through the socket closes
  (the daemon's engine owns the intent — the D-TC1 rationale,
  pinned end-to-end). One focused journey; the harness already
  exists.

**Gates:** the full AGENTS §10 suite through
`scripts/run-quality-gates.sh`; `cargo llvm-cov` shows the new
module covered; **no `.snap` changes** (D-TC14 — any insta diff is a
finding); `cargo deny` clean with no new deps.

---

## 6. Risks & mitigations

- **Time-dependent goldens.** Boot-stamped entries cannot be
  byte-golded (`now` moves). Mitigated by construction: byte goldens
  live in Phase A over hand-built rows with fixed timestamps and in
  Phase C over fixed-stamp `log` rows; the open row is asserted
  structurally (empty `end`, numeric duration). No wall-clock
  assertions anywhere (AGENTS §5).
- **CSV edge cases** (titles with commas/quotes/newlines from synced
  remote content): the quoting fn is unit-exhaustive and the golden
  fixture carries the nasty title. CRLF is quoted like any other
  embedded newline class.
- **Host-TZ dependence in the status test**: the day calendar is
  passed as data with an explicit `FixedOffset` (D-TC5/D-TC10 of the
  round's own D-F6 contract) — the test is host-independent by
  design.
- **Prefix-ambiguity unconstructible with minted v7 ids** in the
  hermetic tier: covered in Phase A over hand-built uuids (the
  `tracking.rs` test module's own `Uuid::from_u128` pattern); the
  boot tier pins the resolution kinds, not the collision.
- **The relay journey's weight** (a real daemon process per
  `cli_ipc.rs`): one journey, not per-verb; the harness's
  `wait_ready`/stop bookkeeping already exists.
- **Envelope-exception creep**: the raw-payload bypass is scoped to
  export only (D-TC10's test pins `track` receipts still riding the
  envelope under `--json`); a reviewer can diff the two behaviours
  by the test names.
- **Soft-lane conflict** (at most one `cli.rs`/`route_for_ipc` chunk
  in flight): DR-5/3 is wave 8's owner of that lane (DR-6/4 and
  DR-6/6 are other waves) — no action needed, recorded for the
  dispatch round's bookkeeping.

---

## 7. Estimated diff size

| File | Shape | ~Lines |
|---|---|---|
| `crates/taskboard-app/src/cli.rs` | `Track`/`Export` variants, `TrackCmd`, `ExportFormat`, 2 parse fns + grammar tests | +140 |
| `crates/taskboard-app/src/commands/track.rs` (new) | wrappers, seams, collapse, resolve, projection glue, renders, CSV, `duration_phrase` + Phase-A units | +650 |
| `crates/taskboard-app/src/commands/mod.rs` | 2 dispatch arms, 2 `dispatch_line` arms, 2 route rows + test rows, 2 error variants + kind/detail arms | +90 |
| `crates/taskboard-app/src/commands/output.rs` | 7 receipt/view projections | +100 |
| `crates/taskboard-app/src/config.rs` | `resolved_day_calendar()` extraction | +25 |
| `crates/taskboard-app/src/completion.rs` | catalogue pins + kind rows | +20 |
| `crates/taskboard-app/tests/cli_track.rs` (new) | the T-series | +550 |
| `crates/taskboard-app/tests/cli_ipc.rs` | the relay journey | +70 |

**Total ≈ 1.6k lines, all in `taskboard-app` + tests** — size M as
dispatched. Zero domain, storage, sync, or ui-slint diffs; no
migration; no new dependencies; no insta regeneration.

---

## 8. References

- Origin: `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  §3 Part E (D-T14/D-T15 — normative), §1 (CLI-recipe anchors),
  §5 (CLI/app test hooks), §6 (risks inherited: history-editing
  foot-guns → D-TC9's explicit ids), §7 (chunk 3).
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §11.2 (this chunk's covers/acceptance/prompts), §2 (resource
  ladder — the 0013→0015 recorded deviation), §1 (the soft lane this
  chunk owns).
- In-tree: ADR 0017 (the settled-open-entry model the CLI reads);
  ADR 0009 (the CLI output contract this chunk extends with one
  recorded exception); ADR 0012 (the lane table + byte-verbatim
  relay); the `timers` family (`commands/timers.rs`,
  `tests/cli_timers.rs`) — the recipe and its tests, mirrored.
- Sibling plans: `2026-10-28-tracking-ui-plan.md` (DR-5/4, the
  surface that consumes the same state read-only),
  `2026-10-26-tracking-domain-engine-plan.md` (DR-5/1, the verbs'
  semantics — not re-derived here).

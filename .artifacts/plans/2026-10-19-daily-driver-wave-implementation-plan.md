# [Plan] Daily-Driver Wave — Implementation (fields + sidecar + daily surfaces)

- **Date**: 2026-10-19
- **Kind**: implementation plan for the **accepted** daily-driver wave
  designs. The five design-round docs landed via PR #359 with their
  decision-session rulings baked in (all recommendations ACCEPTED;
  amendments recorded inside each doc):
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` (D-S1…D-S9),
  `2026-10-18-field-family-design-plan.md` (D-F1…D-F6),
  `2026-10-19-reminders-surfaces-design-plan.md` (D-R1…D-R13),
  `2026-10-19-views-finding-design-plan.md` (D-V1…D-V10),
  `2026-10-19-undo-architecture-design-plan.md` (D-U1…D-U5 — design
  only; its implementation is #176, a later wave). Chunked for one
  branch + PR per chunk; chunks are written so they can be implemented
  directly by an agent or promoted into a dispatch plan without
  restructuring. Each chunk names its normative origin sections by
  decision ID; **Deviation from an ACCEPTED ruling goes back to the
  user, never silently adapted.**
- **Status**: READY — **GATED**. The gate is the **multi-board
  implementation round completing** (milestone `wave-multi-board`:
  #125/#187/#179/#170 closed by its chunks; its dispatch plan is
  `.artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md`).
  As of 2026-10-19 that round is **1 of 8 chunks landed** (PR #358 —
  per-board SyncStates + migration 0006; #298 closed; #125/#187/#179/
  #170 open). The daily-driver chunks touch the same files multi-board
  chunks 2–7 rewrite (`entities.rs`, `command.rs`, `outbox.rs`,
  `merge.rs`, `pipeline.rs`, the sync actor, storage codec, CLI, UI
  view state) and every design doc states its shapes in post-multi-
  board vocabulary — stacking this wave's code on pre-multi-board
  `main` would force a mid-flight redesign. The wave's design round,
  by contrast, is **complete** (framing D-D5's parallel leg); nothing
  in this plan blocks on further design work.

## 1. Scope

**In** (framing §2, user-confirmed 2026-10-18; the issue set below is
the milestone `wave-daily-driver` roster as of 2026-10-19):

- **App-data sidecar** (#207): anchor ceremony, document schema,
  evidence/fold/flush/fences, per-board documents, reminder-rules
  section (#202 rules half — proofs stay with the D5 wave per the
  issue's scope comment).
- **Field family** (#208 scheduled split, #209 effort + importance/
  urgency + decorations, #244 lockstep seam) with the ruled
  native-`startdate` lane for `scheduled`.
- **Day-boundary preference** (#345).
- **Reminders surfaces**: N-rule verbs (#204), standalone timers &
  alarms (#211), toast sink (#344 — `notify-rust` 4.18.x stack
  approved 2026-10-19).
- **Views & finding**: today view (#346), inbox + quick capture
  (#347), fuzzy search (#348 — top-level `find` verb per the D-V9
  amendment).
- **Undo design round** (#349) — **complete**: the doc landed via
  PR #359 with rulings; the issue closes with the round. No undo
  implementation in this wave (#176 unscheduled; deferrals #353–#357).

**Out** (later waves / other homes): task-kind & respite flags and
recurrence parameters (Wave 2–3; the document schema **reserves** the
key space, nothing serializes); delivery-proofs + settings sync (D5,
Wave 4); insistence levels (#203, after this wave's sink); timers UI
(#352); GUI settings editor (#351); sidecar cross-board move repair
(#350); decision support, matrix view, organiser view, PDF export;
`tasks add` field-flag parity (doc 2 §7); the entire multi-board
implementation round's own scope.

## 2. Design facts every chunk relies on

File paths + symbols are the stable keys — line numbers appearing
anywhere (including the design docs' tables) are plan-day references;
every implementing agent re-locates symbols at its own start. Each
fact cites its ruling; per-chunk sections reference "DDF-N".

1. **DDF-1 — the field skeleton** (doc 2 D-F1/D-F3/D-F4, all
   ACCEPTED). `Task` grows `scheduled: Option<DateTime<Utc>>`,
   `effort: Option<Effort>`, `importance: Option<Importance>`,
   `urgency: Option<Urgency>`; `TaskClocks` grows one clock each
   (twelve slots total); `Effort` is a newtype over u64 seconds
   (`#[serde(transparent)]`, codec validates ≤ i64::MAX);
   `Importance`/`Urgency` are two distinct five-level enums
   (`VeryLow|Low|Normal|High|VeryHigh`, serde unit names
   `very_low`…, declaration order = severity order, derived
   `PartialOrd`/`Ord` is the ruled sorting contract); `TaskChanges`
   grows four double-option slots (`deserialize_double_option`
   reused); `Some(None)` clears **and stamps** the clock (clearing is
   a write). Home ruling: extend `Task`/`TaskClocks` — no parallel
   map; the domain shapes carry **no lane information** (the
   connector-independence ruling: the local data model is one
   construct; a future connector mapping a field differently is
   connector-side concern only). `PendingTaskIntent` grows one bool
   per app field + one `reminder_rules` flag (doc 1 D-S6), gaining
   `Serialize/Deserialize` derives for the manifest test. Migration
   **`0007_task_field_family.sql`**: 4 NULL value columns + 4
   epoch-defaulted `ck_` columns + `reminder_rules.updated_at`
   (epoch-defaulted, NOT NULL — the codec leaves it untouched until
   chunk 4 wires the domain field; INSERT-without-column inherits the
   default). Storage numbering: 0006 is multi-board's; this wave
   numbers 0007+.
2. **DDF-2 — `scheduled` rides Deck's native `startdate`** (doc 2
   D-F2, RULED branch A). `RemoteTask` gains `startdate` (duedate's
   twin); `merge_task` gains the per-field verdict
   (`remote_wins(ts_r, local.clocks.scheduled)`); `NewCardShape`/
   `CardShape` + `from_task` sites + the client's card write encoding
   carry it; contract fixtures pin the wire key. **The document's
   `tasks` section never gains a `scheduled` key** (manifest
   assertion 3 enforces). The write-path wire pin rides the 1.17.5
   contract-fixture gate (DDF-10). Consequences accepted: entity-stamp
   merge basis (identical loss window to `duedate`'s today); Deck-side
   date-granularity normalization on Deck-side edits (last writer
   wins, adopted).
3. **DDF-3 — the lockstep seam** (doc 2 D-F3, ACCEPTED; issue #244).
   One exported `TASK_FIELDS: &[(&str, &[&str], FieldFamily)]`
   manifest in the domain crate (clock / task fields / family;
   `FieldFamily::{Native, AppData}` records the **Deck connector's**
   lane assignment — 9 native rows incl. `scheduled`, 3 AppData rows
   `effort`/`importance`/`urgency`) + four assertions: (1) domain
   bijection (serde key sets of `TaskClocks`, `PendingTaskIntent`,
   `Task` ⊇ union, `TaskChanges` ⊆ clocks), (2) storage columns via
   `PRAGMA table_info(tasks)` against the manifest, (3) the
   `TaskEntry` codec's modeled key set = the AppData rows exactly
   (lands with the document codec chunk), (4) per-row engine op
   routing — `EditTask` slot → `UpdateTask` (Native) /
   `UpdateAppData` (AppData) (lands with the op-routing chunk).
   Assertions 1–2 land with the field chunk; 3–4 with the sidecar
   chunks (doc 2 §6's recorded split).
4. **DDF-4 — the document schema** (doc 1 D-S2, ACCEPTED + the
   loud-fence ruling). Versioned envelope `{format: 1, board,
   sections}`; sections `tasks` (keyed by decimal card id;
   `TaskEntry.fields` maps field key → `{v, clock}`; `null` = cleared;
   **no tombstones in tasks**) and `reminder-rules` (keyed by card id;
   `RulesEntry.rules` keyed by rule UUID; rule tombstones
   `{deleted, clock}`). Reserved, documented, never emitted:
   `proofs`, `settings`, and field keys `task-kind`/`respite`/
   `recurrence`. Unknown sections/keys/entries are preserved
   **verbatim** on re-upload (the D5 guarantee — a v1 device is a
   transparent relay). `format >` ours ⇒ fold nothing, flush nothing,
   ops strand **loudly** (typed evidence → per-board sidecar health →
   UI badge + CLI `status` "update this app" warning — the ruled
   addition). Foreign `board` ⇒ ignore on fold; flush may replace
   content. Flush GC prunes entries whose card id is absent from the
   cycle's complete snapshot. Types live in a new domain module
   `appdata.rs` (pure serde + pure functions).
5. **DDF-5 — the anchor ceremony** (doc 1 D-S1, ACCEPTED). Constants:
   card title **`[taskboard] app-data`** (exact match, no
   normalization), attachment filename **`taskboard-appdata.json`**,
   MIME `application/json` — compile-time, never config, never
   changed. **The title constant is exported from the domain crate**
   (doc 4 §7 item 5's reconciliation: the UI crate cannot depend on
   the sync crate; the sync crate consumes the domain const for card
   creation). Discovery: stateless per-cycle scan of the pulled
   snapshot (active + archived) for exact title matches, confirmed by
   attachment-with-reserved-filename on a card-detail read (the
   already-paid bound-card detail refresh captures
   `detail.attachments`); pick rule = newest attachment
   `lastModified`, tie-break lowest card id. Creation lazy
   (only when a flush has work; actor-side `create_card`, **not** an
   engine command; adopted by the next pull through R6). Exactly one
   unconfirmed title match + no carrier ⇒ adopt it (upload onto it);
   multiple ambiguous matches ⇒ create own. Loss taxonomy: card
   deleted/moved/renamed/archived ⇒ re-anchor + **rebuild flush**
   (all local entries); tombstoned attachment ⇒ `restore_attachment`
   then `update_attachment_content` (create-against-tombstone is
   unmeasured — never depended on); **never** unarchive/rename/delete
   anything user-made; residual duplicate carriers linger (cleanup
   declined, backlog). Anchor-task hiding is **presentation-layer**
   (lists, counts, today view, search) + an **app-layer CLI guard**
   (typed refusal of rename/delete on a task whose title equals the
   constant — app layer, never engine).
6. **DDF-6 — the fold and the guard mirror** (doc 1 D-S3/D-S6,
   ACCEPTED). `fold_task_app_data(merged, local, entry, intent)` (a
   `merge_task` sibling in domain `merge.rs`) + a rules fold: per
   field key, document clock ≥ local clock ⇒ take value, clock =
   `max(local, document)`; local strictly newer ⇒ local stands; the
   card stamp is **never read** (disjoint clock domains); absence ≠
   deletion; the fold contributes **no presence facts** (R3 stays
   snapshot-only; the fold queries `task_by_board_card`, never feeds
   it). Position: inside `apply_sync_report`, immediately after the
   task merge loop (i.e. after each task's `merge_task` +
   `keep_task_intent`). Guard mirror: `LocalOp::UpdateAppData(t)` with
   `queued_at > read_at` sets **all** the task's app-field flags +
   rules flag (coarse by design); the copy happens **inside the
   fold** — per protected field/rules, local value **and embedded
   clock** over the fold result (`queued_at == read_at` is pre-read,
   unprotected — the boundary convention).
7. **DDF-7 — cycle integration + fences** (doc 1 D-S4/D-S7,
   ACCEPTED). `SyncReport::Completed` grows `app_data:
   AppDataEvidence` — `Document(Box<AppDataDocument>) | Absent |
   Unreadable(AppDataUnreadable)` (`FormatTooNew{format} | Transport
   | Decode | ForeignBoard`); `Failed` never carries document
   evidence. Download during the pull phase (after `pull_snapshot`);
   anchor metadata from the card-detail read; change detection by
   attachment `(lastModified, filesize)` cached in `LoopState`
   **in-memory only** (no new persisted validator kind; one document
   GET per board per process start + one per foreign change; cold
   start re-GETs — idempotent fold makes it harmless). **Flush upload
   executes after the pull and before the report is sent** (the only
   position satisfying: after the cycle's card pushes, before the
   cycle ends, stamp bump lands in the *next* cycle's pull); content
   = union(cached/downloaded document, local entries for the group's
   tasks as of `read_at`) — read-modify-write, **never** blind
   overwrite; unknown content preserved; snapshot-absence GC applies;
   unresolved tasks' ops stay queued. Rebuild mode (re-anchor,
   restore, adoption, document-absent-with-local-data): local side =
   **all** local entries. Fences (D-S7 table): failed
   download/decode/format-too-new/foreign ⇒ fold nothing **and no
   upload** (ops queue); failed upload ⇒ no outcome, ops queue, cycle
   still `Completed`; permanent 4xx ⇒ counted `FailOp` dead-letters;
   anchor uncreatable (permission) ⇒ honest loud stranding;
   `SyncReport` send failure after upload ⇒ ops re-queue, idempotent
   re-upload. The engine derives a **per-board sidecar health** from
   the evidence variants (additive status; surfaces read it).
8. **DDF-8 — the outbox op + enqueue sites** (doc 1 D-S5, doc 2 §6,
   doc 3 D-R2). `LocalOp::UpdateAppData(TaskId)` — one op for fields
   + rules (deliberately coarse; board-scoped rejected: over-protects
   across tasks); codec tag `update_app_data` (additive; no legacy
   tag renamed). `plan_pushes` coalesces a board's pending ops into
   **one** document group executing **after `CreateTask` groups**
   (entries key on remote card ids). Board resolution via the entity
   graph (`op_targets_task` grows the arm; `pending_ops` accounting).
   Enqueue sites: `EditTask` app-field slots (alongside where
   `UpdateTask` enqueues) and the rule-mutation commands — until the
   N-rule verbs land, the **existing** `SetReminderRule`/
   `ClearReminderRules`; after, `AddReminderRule`/`RemoveReminderRule`/
   `ClearReminderRules` (create stamps, remove/clear emit one
   tombstone per removed rule). **No reminder data ever enters the
   native merge/push path** — the two `pipeline.rs` fence comments are
   amended, not deleted ("no reminder data enters the *native* sync/
   push path; rules ride the app-data document lane").
9. **DDF-9 — per-board documents + rules section** (doc 1 D-S8/D-S9,
   ACCEPTED). One anchor + one document **per board**, all work inside
   that board's own cycle; no cross-board ordering; lazy (zero sidecar
   cost without ops); read-only connectors fold-only; boards with no
   app-data anywhere get no anchor. Cross-board card moves lose
   app-fields — **accepted limitation**, repair = #350 (unscheduled).
   Rules: document key = the `ReminderRuleId` UUID itself (the
   document *is* the rule's remote identity; the task link rides the
   card key); `ReminderRule` grows one additive write clock
   (`updated_at` — 0007 pre-creates the column; chunk 4 lands the
   domain field + stamping on every mutation); deletes are tombstones;
   `deliveries`/`snoozes`/`muted` stay device-local (D5).
10. **DDF-10 — the live-tier gate** (doc 1 §4, RULED 2026-10-18: the
    dockerized **Deck 1.17.5** contract-fixture run — reproducible,
    in-repo, CI-able; never a probe against the live server). Gates
    the **first sidecar implementation chunk**: a second docker-tier
    variant pinned to Deck 1.17.5 running the attachment contract
    fixtures — upload part names, create-409, update-in-place with
    stable filename, soft delete + restore, no-validators-on-content —
    **plus the `startdate` read/write pins** (doc 2 D-F2's folded
    gate). A behavioral divergence on 1.17.5 is a **stop-and-report
    finding** (the design's expected-degraded stance stands until the
    user rules), never a silent adaptation.
11. **DDF-11 — the day window** (doc 2 D-F6, ACCEPTED). `[preferences]
    day_start_time` config section (default `"00:00"`; typed
    validation — must parse as a time-of-day, never silently
    defaulted; device-local, never synced until D5). Domain
    `days.rs::day_window(now, NaiveTime, FixedOffset) ->
    (DateTime<Utc>, DateTime<Utc>)` — pure, total, fixed-offset
    arithmetic (the DST ruling: ≤1h twice-yearly wobble accepted;
    no tz-database lookups in pure functions). Consumers contract:
    app bootstrap parses config once + resolves the device offset via
    `chrono::Local` at derivation time; consumers (UI, CLI, later
    engine features via bootstrap injection) receive `(NaiveTime,
    FixedOffset)` explicitly and call `day_window`; nothing reads
    figment mid-flight, nothing mutates `AppState` for it. Proptest:
    `start ≤ now < start+24h`; stepping `now` past `start` flips the
    window; +24h/day in fixed offset. **No `UserPreferences` domain
    entity** — the Phase-1 deferral stands (resurrection trigger
    recorded in the doc).
12. **DDF-12 — field surfaces** (doc 2 D-F5, ACCEPTED). No new
    top-level verbs for fields — `tasks edit` flags: `--scheduled`
    (the `parse_due` vocabulary, one parser two flags), `--effort`
    (hand-rolled `parse_effort`: bare number = **minutes**,
    `s`/`m`/`h` suffixes compose, `none` clears, overflow = clap
    usage error), `--importance`/`--urgency`
    (`very-low|low|normal|high|very-high|none`). REPL mirrors via the
    shared `*_on` handlers; completion grows. `TaskRow` + the UI row
    model grow `scheduled`/`effort` phrases + level carriers;
    presentation phrase helpers mirror `due_phrase`; decoration
    visuals via existing tokens; `TaskCardModel.decorations` is the
    carrier (doc 4 §7.3); **no slint-side logic** (semantic phrases
    are domain/presentation functions). `tasks add` takes no new
    flags (at-create scheduling ruled out, doc 4 D-V6).
13. **DDF-13 — reminder surfaces** (doc 3, all ACCEPTED / stack
    granted). **N-rule verbs** (D-R1/D-R3/D-R4): `SetReminderRule`
    **retired, not aliased**; `AddReminderRule {task, offset}` (dedup
    by offset — `(task, offset)` determines firing semantics; a dedup
    hit = `NoOp`-shaped receipt `ReminderRuleAdded {rule, added:
    false}`), `RemoveReminderRule {rule}` (typed unknown-id; selector
    at CLI = exact UUID | unique ≥8 prefix | the offset text form —
    unambiguous because of dedup), `ClearReminderRules` stays (now
    removes N). CLI group keeps one-mode-per-line; `--remove` grows;
    receipts `ReminderAddReceipt`/`ReminderRemoveReceipt`; UI = task
    editor rule-list composition (no new state; banner
    `snooze_target` unchanged). **Timers & alarms** (D-R5–D-R9):
    `ReminderRef::Adhoc(AdhocAlarmId)` declared **after `Default`**;
    `AdhocAlarm {id, label, fire_at, created_at}` in
    `ReminderState.alarms`; `ReminderFiring.task`/`Notification.task`
    become `anchor: ReminderAnchor {Task | Alarm}`; `due_firings`
    gains an alarms pass (map membership = liveness; mute does not
    apply); `CreateAdhocAlarm {label, when: AdhocWhen::{At, In}}`
    (planner stamps `In` = now + dur; `dur ≤ 0` typed rejection) /
    `CancelAdhocAlarm`; settlement prune in the post-batch pass
    **after** snooze compaction (delivered + settled ⇒ pruned, with
    dependent deliveries/snooze links); `ClearBoardContent` grows
    `Adhoc(_) => false` (device-global); migration **0008** creates
    `adhoc_alarms`; `timers add/list/cancel` CLI+REPL (label prefix
    selectors; `--at` parses host-local at the CLI edge); device-local
    (no op, no section); **no UI** (#352). **Toast sink** (D-R10
    granted / D-R11–D-R13): `notify-rust` 4.18.x pinned once in
    `[workspace.dependencies]` (default features, no image feature;
    `z-with-tokio` decided by measurement); `DesktopSink` implements
    the **unmoved** `NotificationSink` port behind which all DBus/
    platform quirks stay; passive toasts only (GNOME drops `actions`
    — interactive snooze stays in-app); `[reminders] sink =
    "auto"|"log"|"desktop"` (default `auto`: Desktop→DesktopSink,
    Daemon/Kiosk→LogSink); failure logs via the port's consumer —
    **no retry, no fallback-send** (double-delivery risk); boot-time
    choice, no live swap.
14. **DDF-14 — find** (doc 4 D-V8/D-V9/D-V10, ACCEPTED/amended).
    Domain `find.rs`: hand-rolled, dependency-free, pure, total,
    deterministic; case-folded subsequence classification into
    `MatchQuality {Exact, Prefix, WordStart, Substring, Subsequence}`
    (declaration order = rank); ranking key `(quality, title_len,
    canonical_id)` — **no numeric score**; scope = live boards/stacks
    (unarchived) + tasks `is_live()` (done included; title-only;
    anchor task NOT excluded here — hiding is presentation-side).
    Type limits caller-side. Proptest suite at the floor:
    determinism, case-fold invariance, empty-query floor, exact-title
    invariant, class monotonicity, growth monotonicity, type-filter
    partition, totality. **Crate-swap firewall**: the public surface
    (`find`, `SearchHit`, `SearchTarget`, `MatchQuality`) is the
    contract; internals + algorithm-shaped properties stay in-module
    so a fuzzy-matcher crate can replace them touching nothing
    outside `find.rs`. CLI: **top-level** `taskboard find <QUERY>
    [--type task|stack|board] [--limit N]` (the D-V9 amendment —
    `tasks find --type board` read self-contradictory; note the
    departure on #348); pure read, exit 0 on well-formed queries;
    `FindRow` projection; REPL `find <QUERY>`. GUI: flip reserved
    `PaletteSearch`; `Flow::Palette {selected}` + `ContextId::Palette`
    (chain-flip); `search-palette.slint` composition calling domain
    `find` per keystroke (no debounce at board scale); 8-row cap;
    `IdTables.palette` indices-back; jump-to retargets
    `select_board`/focus per hit kind; anchor filter on rows
    presentation-side; no kiosk surface; journey leg.
15. **DDF-15 — today view + inbox** (doc 4 D-V1–D-V7, ACCEPTED with
    the v2 re-open). **Today**: `days.rs::today_sections(tasks, now,
    window)` — four first-match sections (Overdue = `overdue_tasks`
    reused; Scheduled `∈ [start,end)` by `(scheduled, order, id)`;
    Due-today `duedate ∈ [now, end)`; Done `done ∈ [start,end)`),
    ids-only, one definition for all consumers; partition/first-match
    proptests. `ViewId::Today` subjectless in the cycle `Today →
    Board → StackDetail → TaskDetail`, **default desktop start
    view**; `view.today` global action (`UiEffect::FocusToday`);
    `ContextId::Today`; rows are `task-card` + decorations; empty day
    = `EmptyState`. Cross-board on desktop (board badge when >1 live
    board; selection-follows-focus retargets `ViewState.board` — Q8
    mechanics, so moves stay board-local); kiosk = focused-board
    subset, compact, passive, rotation-ready (the one flagged Q8
    departure — multi-board chunks must not "fix" it). Rollover rides
    the existing minute-boundary repaint (no new timer; `push`
    receives the window like `now`). `task.due-today` re-commit
    action (contexts `[Today]`; `DispatchCtx.day_window` grows;
    `UpdateTask {duedate: Some(Some(window_end))}`; in-wave,
    confirmation-free — ruled riders); done-count caption. **Inbox
    v2** (ruled): one **app-global Inbox as a designated real board**
    (exact title `Inbox`, created **unbound** on first capture with
    one stack `Inbox`, canonical-first resolution; a user board
    titled `Inbox` is adopted) + per-board **`Backlog`** stacks
    (default landing for board-directed adds); **`StateCommand::
    CaptureTask {title, board: Option<BoardId>}`** (None = global
    Inbox; Some = Backlog; never errors on the happy path);
    `view.inbox` jump action (quietly no-op with no Inbox board);
    toolbar inbox-count chip (desktop only); triage = `StackDetail` +
    existing verbs + the **flat cross-board move ring** (all live
    boards' stacks, `board › stack` hint — a deliberate behavior
    change to ordinary moves; native-lane throughout; fresh captures
    never touch the #350 gap); capture surfaces: flip reserved
    `CaptureQuick`, `Flow::Capture` + `capture-overlay.slint`
    (Enter commits-and-continues, Esc closes, empty-Enter closes;
    commits to the **global Inbox**), CLI/REPL `taskboard capture
    <TITLE> [--board <BOARD>]`; kiosk: no capture surface. Journey
    legs for today, triage, and the move ring.

## 3. Chunks

Sizes on the house scale. Re-chunk record vs framing §6's sketch: the
sketch's chunk 11 (undo design round) is **complete** (PR #359) and
drops out; the sketch's chunk 3/4 split follows doc 1 §7's own
mapping; the sketch's chunk 10 stays one chunk (doc 4 §6 confirms,
gated); `adhoc_alarms` takes migration **0008** (doc 3 D-R8's
"whichever lands first" resolved by ordering: 0007 = field family +
rule clock with chunk 1; 0008 = alarms with chunk 7); the anchor-task
hiding/fence-warning surfacing is distributed per surface (list rows
→ chunk 5; today/search → chunks 9/10; CLI guard + status warning →
chunk 4) per doc 1 §7's ruling text.

### Chunk 1 — Domain field family + lockstep manifest + 0007 + native `startdate` (L; branch `feat/domain-task-field-family`; closes #208, annotates #209/#244)

- **Origin**: doc 2 §3.1 D-F1, §3.2 D-F2 (branch A, ruled), §3.3
  D-F3 (assertions 1–2), §3.4 D-F4; doc 1 D-S6 (guard flags only);
  DDF-1/2/3.
- **Files** (symbols): domain `entities.rs` (`Task`, `TaskClocks`,
  `Effort`/`Importance`/`Urgency` types, `TASK_FIELDS` +
  `FieldFamily`), `messages.rs` (`TaskChanges` slots),
  `pipeline.rs` (`PendingTaskIntent` flags + serde derives),
  `merge.rs` (`scheduled` verdict in `merge_task`),
  `push.rs` (`NewCardShape`/`CardShape` + `from_task`),
  `test_support` (strategies); storage `migrations/
  0007_task_field_family.sql`, `repo.rs` (row struct, `decode_task`,
  upsert, whole-state batch), `codec.rs` (nothing — 0007's rule-clock
  column left to its default until chunk 4); sync `model.rs`
  (`RemoteTask.startdate`), `client.rs` (card write encoding),
  `tests/fixtures` + `tests/contract.rs` (startdate wire pins);
  assertions 1–2 (domain `entities.rs` tests; storage migration test).
- **Behaviours**: the twelve-clock skeleton complete; `EditTask`
  app slots stamp clocks + persist, **no outbox op yet** (the
  stated interim — see §5); native `startdate` round-trips create/
  update/read at tier 1 (wire-pinned) and merges per-field LWW;
  the manifest + assertions 1–2 green; the Task-widening insta pass
  (domain proptest corpora, engine snapshots, storage round-trips)
  is this chunk's one deliberate regeneration.
- **Non-goals**: no `UpdateAppData` arm (chunk 3), no enqueue (chunk
  4), no document keys (chunk 3), no `ReminderRule.updated_at`
  domain field (chunk 4 — 0007 pre-creates the column only), no
  CLI/UI (chunk 5), no preferences (chunk 2).

### Chunk 2 — Day-boundary preference (S; branch `feat/domain-day-boundary`; closes #345)

- **Origin**: doc 2 §3.6 D-F6; DDF-11.
- **Files**: app `config.rs` (`[preferences]` section + typed
  validation + the default), domain `days.rs` (new module:
  `day_window` + proptests). Nothing else — no storage, no state, no
  engine.
- **Behaviours**: the config round-trips with validation failures
  typed (never silently defaulted); `day_window` pure/total with the
  ruled proptest set; the consumers contract documented on the
  function (bootstrap resolves offset; consumers receive the pair).
- **Non-goals**: no consumer wiring (chunk 10 wires the today view;
  CLI markers none exist today), no `UserPreferences` entity, no
  sync.

### Chunk 3 — Sidecar transport: 1.17.5 gate + anchor ceremony + document codec + evidence (L; branch `feat/sync-appdata-transport`; annotates #207/#244)

- **Origin**: doc 1 §3.1 D-S1, §3.2 D-S2, §3.4 D-S4 (items 1–3:
  evidence, metadata, change detection), §3.5 D-S5 (the op arm +
  codec tag only), §4 (the ruled live-tier gate); doc 2 D-F2's folded
  wire pins; DDF-4/5/10.
- **Files**: the docker harness (`scripts/nextcloud_it_setup.sh` +
  sync `tests/common` + `it_docker.rs`): a **Deck 1.17.5 variant**
  parameterization running the attachment contract fixtures + the
  startdate read/write pins; domain `appdata.rs` (new module:
  envelope/sections/`TaskEntry`/`RulesEntry` types, codec, the union
  + GC **pure functions**, verbatim-preservation), `ids.rs` or
  entities (the exported anchor-title const), `outbox.rs`
  (`LocalOp::UpdateAppData` arm), `messages.rs` (`AppDataEvidence` +
  `AppDataUnreadable` on `SyncReport::Completed`); storage
  `codec.rs` (op tag `update_app_data`); sync `actor.rs` (anchor
  discovery/creation/adoption, detail-read metadata capture,
  `LoopState` `(lastModified, filesize)` + cached-document store,
  download-during-pull, evidence construction); manifest assertion 3
  (document key set = `{effort, importance, urgency}`).
- **Behaviours**: the 1.17.5 fixtures pass **before** the transport
  lands (a divergence = STOP-and-report to the user, DDF-10); anchor
  ceremony unit/tier-2 tested (creation, adoption, newest-stamp
  pick); a downloaded document reaches the engine as typed evidence
  (`Absent` stays silent); unknown content survives a codec
  round-trip verbatim; `SyncReport` fixtures regenerate (this
  chunk's insta pass).
- **Non-goals**: no fold, no health derivation, no flush upload, no
  enqueue sites (all chunk 4); no CLI/UI.

### Chunk 4 — Sidecar fold + flush + fences + rules section + op routing + surfacing (L; branch `feat/sync-appdata-fold`; closes #207 + #244, annotates #202)

- **Origin**: doc 1 §3.3 D-S3, §3.4 D-S4 (item 4: flush + rebuild),
  §3.5 D-S5 (planning/completion), §3.6 D-S6, §3.7 D-S7, §3.8 D-S8,
  §3.9 D-S9; doc 2 §6's enqueue contract; doc 3 D-R2; DDF-6/7/8/9.
- **Files** (symbols): domain `merge.rs` (`fold_task_app_data` +
  rules fold), `pipeline.rs` (fold position in `apply_sync_report`;
  `pending_task_intents` `UpdateAppData` arm; the two fence comments
  amended), `push.rs` (`plan_pushes` document group, after
  `CreateTask` groups; outcome handling), `reminders.rs`
  (`ReminderRule.updated_at` + stamping), `outbox.rs` (enqueue
  helpers as needed); state `actor.rs`/`core.rs` (per-board sidecar
  health from evidence; status surface); sync `actor.rs` (flush
  between pull and report; rebuild mode; restore-then-replace; the
  D-S7 fence table); storage `repo.rs`/`codec.rs` (rule-clock
  read/write); app `commands/*` (CLI `status` warning rendering +
  the **anchor-title CLI guard** — typed refusal of rename/delete on
  an anchor-titled task); manifest assertion 4 (per-row op routing).
- **Behaviours**: app-field edits and rule mutations enqueue
  `UpdateAppData` and converge through a tier-2 round-trip
  (edit → flush → second client's fold); the guard mirror holds
  (a post-`read_at` edit survives the same cycle's document
  adoption); fences exercised per the D-S7 table (failed download ⇒
  no upload; failed upload ⇒ ops queue + cycle `Completed`;
  permanent 4xx ⇒ counted dead-letters); format-too-new surfaces the
  loud per-board warning through CLI `status`; all four manifest
  assertions green.
- **Non-goals**: no N-rule verbs (chunk 6 — the existing verbs are
  the enqueue sites), no UI badge (chunk 5), no `adhoc` anything
  (chunk 7), no cross-board repair (#350).

### Chunk 5 — Field surfaces + anchor hiding on lists + sidecar badge (M; branch `feat/app-task-field-surfaces`; closes #209)

- **Origin**: doc 2 §3.5 D-F5; doc 1 §7 rulings (hiding + warning
  rendering); doc 4 §7 item 3; DDF-12 (+ the DDF-5 filter/badge).
- **Files**: app `cli.rs` (`--scheduled`/`--effort`/`--importance`/
  `--urgency` + `parse_effort`), `commands/tasks.rs` (both lanes),
  `commands/output.rs` (`TaskRow` + phrase rendering),
  `completion.rs`; ui `models.rs` (decorations carriers + phrases),
  `presentation.rs` (phrase helpers; the anchor exact-title filter
  over list rows/counts; the sidecar-health badge rendering).
- **Behaviours**: all four flags round-trip CLI/REPL with `none`
  clearing; decorations render from the presentation layer only
  (no slint logic); the anchor task disappears from list surfaces
  and counts; the format-fence warning renders on the badge when
  the health says so.
- **Non-goals**: no today view/search filtering (chunks 9/10 own
  their surfaces' filters), no `tasks add` flags, no field-specific
  verbs.

### Chunk 6 — N-reminder verbs + surfaces (M; branch `feat/state-reminder-rule-verbs`; closes #204)

- **Origin**: doc 3 Part A (D-R1–D-R4); DDF-13 verbs.
- **Files**: domain `command.rs`/`messages.rs` (retire
  `SetReminderRule`; `AddReminderRule`/`RemoveReminderRule` +
  outcomes; dedup-by-offset), app `commands/reminders.rs`
  (`RemindInvocation::Remove`, receipts), `cli.rs` (the group's new
  mode + selectors), `repl.rs`/`completion.rs`; ui task-editor
  rule-list composition (`compositions/` + actions wiring).
- **Behaviours**: first-remind flow identical; a second rule at a
  different offset **adds** (the semantic change, CHANGELOG-flagged);
  same-offset add = `added: false` naming the surviving rule;
  remove by UUID/prefix/offset; rules sync through the chunk-4
  enqueue/tombstone sites; engine snapshots + receipts/`--json`
  fixtures regenerate (this chunk's insta pass).
- **Non-goals**: banner/snooze target unchanged (D-R4); no alarm
  anything (chunk 7); no reminder center (#203).

### Chunk 7 — Standalone timers & alarms + 0008 (M; branch `feat/domain-adhoc-alarms`; closes #211)

- **Origin**: doc 3 Part B (D-R5–D-R9; the D-R6 ripple site table);
  DDF-13 timers.
- **Files**: domain `reminders.rs` (`AdhocAlarm`, `ReminderRef::
  Adhoc`, `text_form`/`parse_text_form` `"alarm:<uuid>"`,
  `base_occurrence`, `due_firings` alarms pass, `ReminderAnchor`
  payload widening, `notifications_for`), `command.rs`
  (`CreateAdhocAlarm`/`CancelAdhocAlarm` + `AdhocWhen`, `SnoozeReminder`
  arm, `ClearBoardContent` `Adhoc(_) => false`,
  `UnknownAdhocAlarm`), `persistence.rs` (`Upsert/DeleteAdhocAlarm` +
  the settlement prune after snooze compaction); storage
  `migrations/0008_adhoc_alarms.sql` + codec (deliveries/snoozes
  ride the existing text codec unchanged); app `commands/timers.rs`
  (new, two-lane), `cli.rs` `TimersCmd`, `repl.rs`; ui
  `presentation.rs` (`ReminderTarget` widening — banner task-only,
  mechanically unaffected).
- **Behaviours**: the D-R6 site table walked arm by arm; timers
  fire within one tick (documented bound); settlement prunes
  (property-pinned unobservability); device-local (no op/section);
  `ReminderState` widening = this chunk's insta pass.
- **Non-goals**: no UI (#352), no alarm sync (D5), no sub-second
  timers, no insistence.

### Chunk 8 — Reminder toast sink (M; branch `feat/app-desktop-notifications`; closes #344)

- **Origin**: doc 3 Part C (D-R10 granted / D-R11–D-R13); DDF-13
  sink.
- **Files**: root `Cargo.toml` (`notify-rust` 4.18.x pin, single
  workspace entry), app `notifications.rs` (`DesktopSink`),
  `config.rs` (`[reminders] sink`), `bootstrap.rs` (auto resolution
  by `AppMode`); payload-mapping units at the port boundary.
- **Behaviours**: desktop toasts day one with zero configuration
  (`auto`); daemon/kiosk keep `LogSink`; failure = logged
  `NotifyError`, never retried, never fallback-sent; `cargo deny`
  green over the new per-target tree; `z-with-tokio` decided by
  measurement (recorded either way).
- **Non-goals**: port signature unmoved; no actions/buttons, no
  sound, no escalation (#203); no live-swap seam.

### Chunk 9 — Fuzzy search: `find.rs` + top-level `find` + palette (M; branch `feat/domain-find`; closes #348)

- **Origin**: doc 4 Part C (D-V8–D-V10, the D-V9 amendment); DDF-14.
- **Files**: domain `find.rs` (new module + in-module proptest
  suite + the firewall); app `cli.rs` `CliCommand::Find`,
  `commands/find.rs` (new; `FindRow`), `repl.rs`; ui `view_state.rs`
  (`Flow::Palette`), `actions.rs` (flip `PaletteSearch`;
  `ContextId::Palette`; menu-cursor context widening),
  `router.rs`, `models.rs` (`IdTables.palette`), new
  `ui/compositions/search-palette.slint`, the anchor filter on
  result rows; `tests/journeys.rs` (the palette leg); registry/
  keymap snapshot regen (this chunk's insta pass).
- **Behaviours**: the DDF-14 property suite green; `find` exits 0
  on well-formed queries with empty tables for no hits; the palette
  jumps per hit kind; **stage 3 posts the amendment note on #348**
  (top-level verb, departed from the issue's `tasks find` wording,
  per D-V9).
- **Non-goals**: no description/label search, no debounce, no
  recent-jumps, no kiosk surface.

### Chunk 10 — Today view + inbox + capture + move ring (L; branch `feat/ui-today-inbox`; closes #346 + #347)

- **Origin**: doc 4 Parts A/B (D-V1–D-V7, the D-V5 v2 ruling);
  DDF-15.
- **Files**: domain `days.rs` (`today_sections` + proptests),
  `messages.rs`/`command.rs` (`CaptureTask` + the resolution rules),
  `persistence.rs` (as needed by the handler); ui `view_state.rs`
  (`ViewId::Today` + default, `Flow::Capture`, the move-ring
  generalization, `DispatchCtx.day_window` consumer), `actions.rs`
  (`view.today`/`task.due-today`/`view.inbox` + `ContextId::Today`
  context widening), `router.rs`, `models.rs`
  (`IdTables.today_tasks`, phrase wiring), `presentation.rs`
  (`today_sections` membership, `inbox_count`, board-badge helper,
  anchor filter on today rows + counts), new
  `ui/compositions/today-view.slint` + `capture-overlay.slint`;
  app `cli.rs` (`capture` verb; `tasks add` Backlog default),
  `commands/capture.rs` (new), `repl.rs`; `tests/journeys.rs`
  (today + triage legs); golden-visual + registry regen (this
  chunk's insta pass).
- **Behaviours**: the four sections partition (proptests); day
  rollover by stepped-`now` `push` calls (no sleeps); re-commit
  writes `duedate = window_end` confirmation-free; first capture
  creates the unbound Inbox board + stack + task; `Some(board)`
  lands in Backlog; an existing `Inbox`-titled board is adopted;
  the move ring crosses board edges with `board › stack` hints;
  the inbox chip counts to zero after a triage move (structural
  assertion).
- **Non-goals**: no time-ruler layout, no capture overlay fields,
  no PDF/`tasks today` verbs (backlog), no kiosk chrome, no
  cross-board app-field repair (#350).

### Chunk 11 — Docs + wave exit verification (S; branch `docs/daily-driver-wave`; the wave exit)

- **Origin**: the whole round; framing §6 chunk 12.
- **Files**: `docs/architecture.org` (the field family's two sync
  lanes + the sidecar subsystem + reminders surfaces + find/today/
  inbox + `[preferences]`), `README.org` (new verbs, flags, config
  keys, the anchor card's Deck-UX note), `CHANGELOG.md` (promotion
  from per-chunk Unreleased lines), the exit verification record at
  `.artifacts/reports/<date>-daily-driver-wave-exit-report.md`
  (evidence pointers per §5's table).
- **Bookkeeping**: the **#202 milestone-clear comment** (rules half
  landed via chunk 4; proofs ride the future D5 wave — the framing
  D-D2 record); milestone `wave-daily-driver` closure review.
- **Non-goals**: no code, no tracker edits beyond that comment.

## 4. Quality gates & conventions

Every chunk: the full AGENTS §10 gate set (`fmt --check`, `clippy
--workspace --all-targets -D warnings`, `nextest`, doc tests,
`cargo deny`, the CI-identical three-command llvm-cov sequence).
TDD per `docs/testing_strategy.org` with the house rules restated:
typed outcomes, never text assertions; no sleeps —
`tokio::time::pause()`/`advance()`, poll-a-predicate-with-deadline,
join handles; fakes over mocks; proptest for pure domain logic
(`day_window`, `today_sections`, `find`, the fold, the prune pass,
the manifest); one behaviour, one test, at the lowest layer that
owns it. New `.rs` files carry the per-crate SPDX header (AGENTS
§6); `#![forbid(unsafe_code)]` everywhere except
`taskboard-ui-slint`. **Snapshot rule** (DF-9's): a widening chunk
owns its one deliberate `insta` pass; no `.pending` snapshots ship.
**Tier notes**: chunk 3's 1.17.5 fixtures + chunk 4's tier-2
convergence ride the existing harness conventions; a runner without
docker records that honestly, never silently. **Branch/PR
conventions**: one branch + PR per chunk (AGENTS §8); no stacked
PRs by default; `--force-with-lease` only, never `--force`.

## 5. Sequencing & exit criteria

**Gate (round-level)**: chunk 1 dispatches only after the
multi-board implementation round completes (its milestone issues
closed; §Status). Each chunk's stage 1 verifies its predecessors'
preconditions on `origin/main` and STOPs on a stale base.

**Spine (default)**: `1 → 3 → 4 → {5 ∥ 6 → 7 → 8 ∥ 9 ∥ 10} → 11`,
with **2 landable any time before 10** (recommended immediately —
it is S and independent). Parallel groups never share worktrees;
concurrent chunks touch disjoint crates (5 app/ui rows vs 6/7
domain/state/app vs 8 app-config vs 9 domain-find/ui-palette vs 10
domain-days/ui-views — 9 and 10 both touch ui `view_state.rs`/
`actions.rs`/`models.rs` and app `cli.rs`/`repl.rs`: dispatch them
serially in either order, or accept the rebase dance).

**The wave's one deliberate interim** (sequencing fact, stated
loudly like multi-board's): between chunks 1 and 4, the four fields
exist, stamp clocks, and persist — but app-field edits enqueue no
sync op (`UpdateAppData` and its enqueue sites land in 3–4), so
effort/importance/urgency are **local-only** and rule edits ride
nothing. The interim is invisible to users (no surface exposes the
fields until chunk 5) and closes at chunk 4. The implementation
round does not "fix" it out of order.

**Exit criteria → verifying chunks**:

| Exit criterion | Verified by |
|---|---|
| All four fields sync day one: `scheduled` native (tier-2 round-trip), effort/importance/urgency + rules via the document (tier-2 convergence) | 1 (native), 4 (document) |
| Anchor ceremony: lazy creation, adoption, re-anchor + rebuild, restore-then-replace | 3 (unit + tier-2), 4 (rebuild flush) |
| The dockerized Deck 1.17.5 contract-fixture run green (attachment surface + `startdate`) | 3 |
| Manifest: all four lockstep assertions green | 1, 3, 4 |
| Format-fence loud warning end-to-end (evidence → health → CLI/UI) | 4 (status), 5 (badge) |
| `notify-rust` in the tree, `cargo deny` green per target, `auto` sink by mode | 8 |
| `find` property suite + top-level verb + palette journey | 9 |
| Today view journey, capture engine snapshots, inbox-count structural assertion, move-ring journey | 10 |
| CHANGELOG promoted, docs amended, exit report with evidence pointers | 11 |

## 6. Completion bookkeeping

Closing happens in the implementation round — issues close as
their chunks' PRs merge (PR-body `Closes #N`); annotations are one
progress comment per mapped issue per chunk. Per-chunk map:

| Chunk | Issue action |
|---|---|
| 1 | **closes #208**; annotates #209 (fields landed; decorations chunk 5), #244 (manifest + assertions 1–2; 3–4 ride chunks 3/4) |
| 2 | **closes #345** |
| 3 | annotates #207 (transport half: ceremony + codec + evidence; fold/flush chunk 4), #244 (assertion 3) |
| 4 | **closes #207** (sidecar complete), **closes #244** (assertion 4 — the seam complete); annotates #202 (rules half landed; proofs stay D5 per the issue's scope comment) |
| 5 | **closes #209** (fields + decorations complete) |
| 6 | **closes #204** |
| 7 | **closes #211** |
| 8 | **closes #344** |
| 9 | **closes #348** (+ the D-V9 amendment note: top-level `find` verb) |
| 10 | **closes #346**, **closes #347** |
| 11 | the **#202 milestone-clear comment** (proofs → D5 wave); the exit report |

**#349 (undo design round) closes with the design round itself** —
its deliverable landed via PR #359; closed 2026-10-19 with a pointer
(the implementation stays #176, unscheduled; deferrals #353–#357
already tracked). **#350/#351/#352** are the design rounds' ordered
tracker edits, deliberately unscheduled. The wave creates, labels,
or closes nothing beyond the milestone roster above.

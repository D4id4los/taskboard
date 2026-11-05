# [Plan] Journal & Review — Design Round (reflection entries, curated templates, the boundary-generic review walk, gentle surfacing)

- **Date**: 2026-11-05
- **Kind**: design-round doc — **DR-7**, chunk 1 of the v0.2 dispatch
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`
  §2), the reflection anchor of the v0.2 "Focus & Reflection"
  milestone. Design only — no production code in this commit.
- **Origin**: issue **#621** (normative: the round's ownership list,
  the rulings the round must take, the verified-seams inventory) +
  its three 2026-11-04 ruling comments; issue **#218** (the
  boundary-generic constraint); the v0.2 implementation plan
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`)
  §2 rulings R-1/R-2/R-5/R-7, §3 the cross-milestone contract
  (binding, quoted in §3-A), §4 VF-1…VF-9, §5.1 the settle-list; the
  ADHD report principles (`.artifacts/reports/2026-10-14-adhd-support-research-report.md`
  §5: P1/P2/P4/P7/P9/P12/P13); the roaming options report §4.C/§7/§9
  (`.artifacts/reports/2026-11-04-per-user-appdata-roaming-options-report.md`
  — the device-local ruling and the import-fold expectation).
- **Session rulings carried in (2026-11-04, user — restated normative,
  not re-derived)**: R-1 journal storage is **device-local in v0.2**
  (options report option C), roaming-ready by construction; R-2 the
  review flow is **boundary-generic from the start** (day/week/month
  are parameters of one review primitive, per-cadence display
  specialisation — not three features); R-5 health flags are a
  **curated enum v1**, additive later, not user-extensible; R-7 review
  surfacing is **gentle at the day boundary** (P12: inform, never
  nag) — surfaced, never forced.
- **In-session rulings (2026-11-05, user — amending this doc's
  proposals, dated notes at each changed site)**: **D-J3** flag set
  fixed — `low-medication-day` removed (unclear meaning),
  `minor-headache` and `medium-headache` added; **D-J4** prompt core
  fixed — achievements, gratitude, improvement ideas (the three the
  user's own journaling practice used). **Second pass, same day**:
  **D-J4** `What's still on my mind?` promoted into the ruled core
  (four prompts per cadence); **D-J9** C1–C8, C10, C11 approved
  verbatim (C2 amended three→four prompts to match). **Third pass,
  same day**: **D-J9** ruled **shape B** — the plan-tomorrow leg is
  a handoff transition, not a rendered stage (Carry forward flows
  directly into the plan-next-day surface; the planner's own exit
  is the skip); C9's strings retire and `Plan tomorrow` is recorded
  as the recommended planner-surface title; embedding the planner
  in the walk was assessed and declined (coupling arguments in
  §3-E's C9 history).
- **Branch / worktree**: `docs/journal-review-design-round` in
  `../taskboard-WT-docs-journal-review-design-round` (off `main` @
  `1dc5bf44`; `scripts/worktree-doctor.sh` passed — 219G free, floor
  25G — before creation).
- **Status**: **ACCEPTED** — the user reviewed and approved the
  round as designed (2026-11-05, in-session), resolving all three
  `pending user ruling` flags across three passes (§4's final
  status; no decision was left open). Stage 2 executed in the same
  session: the §7 chunk issues **#636** (domain & engine), **#637**
  (storage & CLI/export), **#638** (review walk UI & surfacing) and
  the fence issues **#639**–**#642** are open on GitHub under the
  v0.2 milestone, and #621/#218 carry the accepted-round annotation.
  Implementation is the three chunk issues (sequenced 1 → 2 → 3); a
  follow-up dispatch round chunks them (the DR-5/§14 precedent).

---

## 1. Context & verified anchors (do not re-derive; verified 2026-11-05 against the round's base `main` @ `1dc5bf44`)

Symbols are the stable keys; line numbers are the base's.

| Fact | Anchor | Evidence |
|---|---|---|
| **`journal` is taken**: the undo command journal owns the vocabulary (VF-3) | domain `journal.rs`, `ids.rs` | module doc "the undo journal substrate (ADR 0019)" (journal.rs :2-4); `JournalEntry` :47-73; `JournalState { entries, head }` :252-258; `JournalId` in the `local_id!` family (ids.rs); `UndoView`/`UndoLabel` :281-301; `command_label` :332-386; retention constants `JOURNAL_CAP`/`JOURNAL_AGE_FLOOR` :33-43. No CLI verb named `journal` exists (cli.rs top-level verbs :33-211: Connect/Login/Boards/Stacks/Tasks/Labels/Find/Capture/Timers/Breaks/Track/Habits/Export/Logout/Sync/Config/Status/Daemon/Gui) |
| Nested sub-states are the AppState pattern (VF-2) | domain `state.rs`, `persistence.rs` | `AppState` :25-55 (`reminders` :38, `tracking` :41, `habits` :44, `undo` :51 memory-only); `PersistedState` :82-110 mirrors every persisted map; `From<PersistedState> for AppState` :112-127 |
| Persistence is per-entity action batches; exhaustive match forces arms | domain `persistence.rs` | `PersistenceAction` :203+ (e.g. `UpsertTimeEntry` :275, `DeleteTimeEntry` :278, `UpsertHabit` :281, `RecordHabitLog` set-union :287); the storage crate's match is exhaustive, no wildcard |
| The entity precedent: LWW clocks + curated enum field + A1 mint | domain `tracking.rs` | `TimeEntry` :37-61 (`created_at`/`updated_at` = the LWW mutation clocks :52-56; `kind: TimeEntryKind` :57-60 with `#[serde(default)]` back-compat :59); `TimeEntryKind` curated enum with `parse_text_form`/`text_form` :69-103; `ClosedSpan::try_new` trust-boundary mint :118-155; `TrackingState` BTreeMap home :108-113; pure projections `tracked_in_window` :195-213, `work_in_window` :221-233 |
| The tombstone-in-place precedent | domain `habit.rs` | `Habit.deleted: bool` + `clocks.deleted` :211-213 (tombstone rides the row; "the log outlives the tracker — history is never purged"); `HabitClocks` :152-164 |
| Period vocabulary Day/Week/Month + canonical flooring exists | domain `habit.rs`, lib.rs | `PeriodKind` :63-71; `WindowStart::of_period` :123-129; `period_start` :310-321 (Day = date, Week = Monday-start, Month = day 1); re-exported from the domain root (lib.rs :104-105) |
| Day-window machinery is pure and calendar-fed (VF-4) | domain `days.rs` | `day_window` :55-70; `trailing_days_window` :80-95; `DayCalendar { day_start_time, offset }` :117-148 (`today`/`calendar_date`/`instant`); `today_sections` :199-242 (first-match, no derived bits); the consumer contract D-F6 (:41-49): `(day_start_time, offset)` parsed once at the app layer, passed as data |
| The undo classification is one site; new families join it (VF-7) | domain `undo.rs`, `journal.rs` | `classify` decides `PreImages` vs `NotUndoable` (undo.rs :89 doc; arms :126-228); `NotUndoableReason` :96-121 — `RestoreClass`, `BoardCeremony`, `ReminderSurface`, `TrackingSurface` :108-113, `HabitSurface` :114-120 — the enum a new family extends |
| The reminders pipeline exists but is reminder-shaped (surfacing alternative declined, D-J10) | domain `reminders.rs` | `ReminderRef { Rule, Default, Adhoc, Break }` :318-328 (a `Review` arm would force exhaustive-match fallout across surfaces/persistence for machinery the ruling does not want); `due_firings` :1303-1339 |
| The day-roll tick is the engine's boundary hook (considered for surfacing, declined for v1) | state `actor.rs`, `config.rs`; domain `command.rs` | `day_tick` (actor.rs :1250-1265, shaped like `reminder_tick`, watermark + durable-then-publish); `DayRollConfig { enabled, calendar: DayCalendar }` (config.rs :50-56); `plan_day_rolls(app, today, calendar, ids, now) -> Vec<PersistenceAction>` (command.rs :707-713) |
| Migrations: next free is **0022** | storage `migrations/` | 0001–0021 present, last `0021_break_rule_board.sql`; re-verified 2026-11-05 (VF-2's number confirmed; implementing chunks re-verify at their own base, single-flight per batch) |
| Ids: two families — roaming vs device-local | domain `ids.rs` | roaming `BoardId`/`StackId`/`TaskId`/`LabelId`/`ReminderRuleId`/`TimeEntryId`/`BreakRuleId` :40-76 (v7, minted via `IdGenerator`); device-local `AdhocAlarmId` :77-80 ("never synced"); v7 mint only in the planner |
| Commands are planned pure, `now` passed as data | domain `command.rs`, `clock.rs` | `plan_command(app, outbox, command, ids, defaults, now)`; typed `CommandError`; `CommandOutcome` receipt enum with `NoOp` idempotent-repeat precedent; crate rule "nothing calls `Utc::now()`" (clock.rs) |
| Quick capture is the Inbox-writing precedent (P1) | domain `messages.rs` | `CaptureTask { title, board: None = app-global Inbox }` :36-42 — never errors on the happy path, triage happens later |
| CLI verb families have a two-lane recipe | app `commands/timers.rs`, `track.rs` | one-shot wrappers via `with_core` + session `*_on` seams (timers.rs :25-60); one `collapse()` definition site :306-330; typed receipts in `commands/output.rs`; state read back from the published snapshot, never echoed (timers.rs :63-76 doc; track.rs `start_on` :131-176); selector resolution exact-uuid → ≥8-char prefix (timers.rs `resolve_alarm` :154-192) |
| The export verb is raw-payload stdout (VF-3) | app `cli.rs`, `commands/track.rs` | `Export` verb :146-152; `ExportFormat` :939; `export` :153-164 / `export_on` :436 / `export_command` :1111; DR-5 D-T15 recorded: "the journal/statistics exports of Wave 2 reuse this verb" |
| `ViewId::Review` is **taken** — the statistics home (VF-1) | ui-slint `view_state.rs` | `ViewId::Review` :48-53 ("the review/statistics surface #214 … subjectless, read-only … a cycle member like Today; KioskMode maps only Board/Today, so the appliance rotation can never select it"); `Settings` :54-56 is the non-cycle-member precedent |
| Subjectless flows are the overlay pattern (VF-8) | ui-slint `view_state.rs` | `Flow` :175+ — `Palette { selected }` :204, `Capture` :209 (subject-free, commit keeps it open), the focus overlay :215+ (XOR: flows replace each other; closing never stops the session) |
| UI effects are a pure router over `ActionId` (VF-8) | ui-slint `router.rs`, `actions.rs` | `UiEffect` :29+ (`FocusToday`, `BeginCapture`, `ToggleFocusMode` — each names its structural kiosk exclusion in its doc); `Mode` :18-25; the adapter applies effects, sends commands over mpsc |
| The notification surface is derived-at-read with a model-side kiosk gate | ui-slint `models.rs` | `push_notifications` :684-756 (kiosk empties the model :693-694 — "the model is emptied, never rendered"); rows projected by pure `presentation::notification_rows`; per-row buttons via the anchor-aware table |
| The review view skeleton discipline (VF-1) | ui-slint `ui/views/review-view.slint`, `presentation.rs` | read-only ledger doc :1-12 (D-8 two sections, D-9 collapse tiers); `ReviewModel` :50-57; `review_seconds_phrase` :1863, `review_window` :1909 — pure phrase derivations; `push_review` :1006 rides the one-way push pass (:888) |
| `[preferences] day_start_time` is the boundary config (VF-4) | app `config.rs` | `PreferencesSection { day_start_time }` :431-438; typed `ConfigError::InvalidDayStartTime` :957; device-local, never synced (:100-105) |
| ADR numbering: next free is **0020** | `docs/adr/` | 0001–0019 present, last `0019-undo-command-journal.org`; implementing chunk re-verifies at base |
| The cross-milestone scale sanity | roaming options report §7 | journal ≈ 0.5–2 KB/day → a year ≈ 0.2–0.7 MB/device — storage growth is a non-issue at v0.2 scale |

**Out-of-scope fences that stay binding**: journal roaming + the
files-substrate anything (milestone `nextcloud-files-sync`; §3-A's
contract is the only acknowledgment); reflection statistics/trends +
past-entries browsing (**#623 slice B's** owned territory — this round
designs no aggregation surface beyond the walk's own information
displays); weekly/monthly *interactive* sessions (#218, post-v0.2
parameterisations — the *primitive* is generic now, the extra surfaces
are not); plan-next-day internals (#622/chunk 3 — the walk consumes a
*reachable-by-command* seam, nothing more, §3-E); PDF export (#624);
per-user attribution (#494); capture templates (#216 — task-capture
templates, unrelated to reflection prompts); LLM-assisted prompts
(settle-list non-goal); kiosk editing of any review/journal surface.

## 2. Scope & non-goals

**In scope (design only):** the vocabulary ruling (the VF-3 collision,
§3-A); the `ReflectionEntry` entity family + attributes (§3-B); the
curated template representation (§3-C); the command family, planner
semantics, and undo arm (§3-D); the boundary-generic review primitive
and the day-cadence walk with its #622 handoff (§3-E); the gentle
day-boundary surfacing mechanism (§3-F); the CLI verb family + export
leg (§3-G); storage (§3-H); kiosk posture (§3-I); decision record §4;
test hooks §5; risks §6; chunk mapping §7.

**Non-goals:** any production code; roaming/sidecar/substrate anything
(§3-A's contract is quoted and honoured — no `UpdateAppData` is ever
enqueued); statistics consumption (#623 slice B); weekly/monthly
interactive walk surfaces (the CLI/entity are cadence-generic day one;
the GUI walk ships day-cadence only — §3-E); health-flag trend mining;
user-editable templates (fence §7); LLM prompts; a `[journal]`-family
config section (curated constants in code, P7 — knobs fenced);
retention/GC for the reflection store (scale §1 anchor says
non-issue at v0.2; named as a future concern in §6).

## 3. Design

### Part A — Vocabulary ruling + the cross-milestone contract (D-J1)

**The contract (impl-plan §3, binding — quoted):**

> 1. **Local-only home**: a domain entity + sqlite table + (per R-1)
>    **no appdata sidecar section, no board attachment, no new sync
>    lane**. Nothing journal-shaped enqueues `UpdateAppData`.
> 2. **Roaming-ready shape** …: `BTreeMap` home keyed by a
>    roaming-family UUID (v7); embedded per-entry mutation clock;
>    deletes are tombstone-shaped rows (or tombstone-derivable), never
>    silent absence; hand-written tolerant serde for any future wire
>    row; A1 newtypes at the trust boundary; unknown-field tolerance
>    in any serialized form.
> 3. **Vocabulary**: `journal` is **taken** —
>    `taskboard-domain/src/journal.rs` is the undo command journal
>    (`JournalEntry`, `JournalId`, `JournalState` are exported names).
>    The round rules a non-colliding vocabulary … nothing may shadow
>    or overload the undo names.
> 4. **The import-fold expectation**: per-entry clock LWW union across
>    devices' files; append-only entries union cleanly — … edits/deletes
>    [are] possible, so tombstone discipline is not optional even
>    device-local.

- **D-J1 — the vocabulary is `reflection`, split by layer.**
  **Domain/code symbols** (everything greppable in Rust): new module
  `taskboard-domain/src/reflection.rs`; types `ReflectionEntry`,
  `ReflectionId`, `ReflectionState`, `ReflectionTemplate`,
  `ReflectionAttributes`, `HealthFlag`, `ReviewCadence`, `ReviewWindow`.
  The contract's own example (`reflection.rs` / `ReflectionEntry`)
  is adopted verbatim. **User-facing surfaces** use `journal` (the
  CLI verb family is `taskboard journal …`, the export domain selector
  is `journal`) — #621's own wording, and the undo journal has no CLI
  verb, so no user-facing ambiguity exists. **Guard rules** recorded
  for every implementing chunk: (i) no code symbol outside
  `journal.rs`/`undo.rs` may use `Journal*` spelling; (ii) the sqlite
  table is `reflection_entries` (never `journal_entries`); (iii)
  "journal" in prose docs refers to the user-facing feature, "journal
  line/entry" to the undo substrate — the two never mix in one
  sentence without qualification. A second collision is also ruled
  here: **`Review` is taken** in the UI (`ViewId::Review` is the
  statistics home, VF-1) — the walk surface is named `ReviewWalk`
  everywhere (§3-E); the statistics view is never renamed.

### Part B — The entity and its state home (D-J2–D-J3, D-J5)

- **D-J2 — `ReflectionEntry` is a self-contained, editable,
  tombstone-in-place row.**

  ```rust
  pub struct ReflectionEntry {
      pub id: ReflectionId,            // local_id!, Uuid v7 — ROAMING family (contract §3.2)
      pub cadence: ReviewCadence,      // Day | Week | Month — which walk/verb produced it
      pub day: NaiveDate,              // the window-date label (DayCalendar::today of window_start)
      pub window_start: DateTime<Utc>, // the reviewed window's opening instant
      pub window_end: DateTime<Utc>,   // the window's closing instant (exclusive)
      pub text: String,                // the free-text body (may be empty)
      pub attributes: ReflectionAttributes, // mood/energy + health flags (D-J3)
      pub created_at: DateTime<Utc>,   // row creation stamp (the minting planner's now)
      pub updated_at: DateTime<Utc>,   // the LWW mutation clock (contract §3.2, embedded)
      pub deleted: bool,               // the in-place tombstone (contract §3.4, D-J6)
  }
  ```

  Roaming-readiness by construction: the row is keyed by a
  roaming-family UUID and carries its own mutation clock and its own
  window instants — the future files-sync import-fold reads rows
  exactly as stored (the #588 sidecar shape at entity scale, per the
  contract), no per-device derivation ever re-runs. Window instants
  are **stored, never derived at read**: the day-window preference is
  device-local and may differ across devices, so a stored entry means
  the same window everywhere. `day` is the human-facing label (the
  `DayCalendar::today` reading of `window_start` at mint time). The
  tombstone is **in place** (the `Habit.deleted` precedent, §1):
  delete flips `deleted` + bumps `updated_at`; the row survives.
  This reads the contract's "tombstone-shaped rows … never silent
  absence" literally — the row *is* the tombstone, so no
  tombstone-derivation machinery ever needs to exist for this family.

- **D-J3 — attributes: curated enums behind A1 mints; multiple
  entries per window are legal.**

  ```rust
  pub struct ReflectionAttributes {
      pub mood: Option<Rating>,          // 1..=5, A1 mint (`Rating::try_new`)
      pub energy: Option<Rating>,        // 1..=5, A1 mint
      pub flags: BTreeSet<HealthFlag>,   // curated enum v1 (R-5)
      pub extra_flags: BTreeSet<String>, // unknown names parked verbatim (tolerant decode)
  }
  ```

  - `Rating` is one hand-rolled A1 newtype (private `u8`, fallible
    mint rejecting 0 and ≥6) shared by both fields — the
    `NonZeroCount` precedent (habit.rs :23-59). Deliberate leniency
    documented at the type: decoded rows are trusted-written; the
    planner mints through `try_new`, so no validate-and-forget path
    exists on the write side.
  - `HealthFlag` is a curated serde enum (kebab-case unit variants;
    R-5's "additive extension later" rule = **append-only** variants,
    never renamed/renumbered — the serialized form is the name
    string). **Unknown names decode into `extra_flags` verbatim** and
    re-serialize byte-stably — the `AppDataSections.unknown`
    tolerance pattern applied to a set field, satisfying the
    contract's "unknown-field tolerance in any serialized form".
    Minting accepts only known names; an unknown name on a write
    path is a typed rejection (`UnknownHealthFlag`).
  - **The member set is RULED (2026-11-05, user)** — v1 carries, in
    enum order: `migraine`, `minor-headache`, `medium-headache`,
    `cold-or-flu`, `allergies`, `cramps`, `poor-sleep`,
    `overwhelmed`, plus `good-day` as the positive marker.
    (Amended 2026-11-05 from the proposed starter list: the user
    removed `low-medication-day` and added the two headache grades;
    `migraine` stays distinct as the severe case.)
  - **Multiple entries per window are legal** — no
    unique-per-window key (journaling twice in a day is normal; the
    surfacing check is "any live entry for the window", D-J10). The
    walk's entry and a quick `journal add` entry are the same shape;
    no origin marker exists (the honest-semantics precedent — a
    limitation recorded, not papered over, matters only for future
    trend reads which #623 slice B owns).

- **D-J5 — the state home: `ReflectionState`, nested like
  `TrackingState`.** `ReflectionState { entries:
  BTreeMap<ReflectionId, ReflectionEntry> }`; mounted as
  `AppState.reflection` and mirrored on `PersistedState` (the
  reminders/tracking/habits precedent, §1). `ReflectionId` joins the
  **roaming** id family (contract §3.2) — not the `AdhocAlarmId`
  device-local family — even though v0.2 never emits it anywhere:
  the id's *family* is the roaming-readiness. Tombstoned rows stay in
  the map (reads filter `!deleted` at projection sites; the
  `TrackingState::open_entries` derived-set precedent — no second
  stored set).

### Part C — Templates: curated data, not config (D-J4)

- **D-J4 — `ReflectionTemplate` is typed data with cadence-keyed
  identity; v1 ships a compiled-in curated set; no stored rows.**

  ```rust
  pub struct ReflectionTemplate {
      pub id: &'static str,        // stable slug — "daily-close", "weekly-close", …
      pub name: &'static str,      // display name
      pub cadence: ReviewCadence,
      pub prompts: Vec<&'static str>,
  }
  pub const DEFAULT_TEMPLATES: &[ReflectionTemplate] = &[ … ];
  ```

  The settle-list's "data, not config" ruling applied: no
  `[templates]` TOML section (P7 — configuration is a hazard; the
  tinkering surface for prompts is a fenced follow-up), and no
  sqlite rows in v1. The **roaming-ready seam** is the type shape:
  when user-editable templates land (the fence issue, §7), rows keyed
  by the same slug vocabulary become stored/roamed entities and the
  builtin set becomes seed data — nothing in v1's walk or entity
  precludes it. Builtin slugs are a reserved namespace (user-authored
  ids must not collide — the follow-up's mint rule, noted on the
  fence). Template selection in v1 is **implicit by cadence**: the
  walk's prompt stage uses the (exactly one) curated template for its
  cadence; the CLI `journal add` never prompts (free text + flags).
  **The prompt core is RULED (2026-11-05, user)**: the three
  categories their own journaling practice used — **achievements,
  gratitude, improvement ideas** (amended 2026-11-05: replaces the
  previously proposed went-well/drained/one-thing set). The v1
  curated set, concrete (the user edits any string verbatim at
  acceptance):

  | Template | `id` / `name` | Prompts |
  |---|---|---|
  | Day | `daily-close` / `Daily close` | `What did I get done today?` · `What am I grateful for today?` · `What would I do differently next time?` · `What's still on my mind?` |
  | Week | `weekly-close` / `Weekly close` | `What did I get done this week?` · `What am I grateful for this week?` · `What would I do differently next week?` · `What's still on my mind?` |
  | Month | `monthly-close` / `Monthly close` | `What did I get done this month?` · `What am I grateful for this month?` · `What would I do differently next month?` · `What's still on my mind?` |

  (Amended 2026-11-05, second in-session pass: the user promoted
  `What's still on my mind?` from the optional candidates into the
  ruled core — the open-loop capture prompt; its answers can be
  quick-captured into the Inbox after the walk closes, P1. The
  remaining optional candidates to pick 0–2 from at acceptance,
  cadence-scoped the same way: `What gave me energy …?` (narrates
  the D-J3 energy rating in the user's own words), `What do I want
  to remember about …?` (memory-keeping). Everything else about the
  mechanism is decided (D-J4's body above).)

### Part D — Commands, planner semantics, undo arm (D-J6–D-J7)

- **D-J6 — the verb set: three commands; windows are data; the
  planner stays calendar-blind.**

  | Command | Payload | Receipt (`CommandOutcome`) |
  |---|---|---|
  | `RecordReflection` | `{ cadence, day: NaiveDate, window: (DateTime<Utc>, DateTime<Utc>), text: String, mood: Option<Rating-mint-input>, energy: Option<_>, flags: BTreeSet<HealthFlag> }` | `ReflectionRecorded { entry: ReflectionEntry }`; empty record (no text ∧ no attributes) → `NoOp` |
  | `UpdateReflection` | `{ id, text: Option<String>, mood: Option<Option<u8>>, energy: Option<Option<u8>>, flags_add: BTreeSet<HealthFlag>, flags_remove: BTreeSet<HealthFlag> }` | `ReflectionUpdated { entry }`; all-unchanged → `NoOp` |
  | `DeleteReflection` | `{ id: ReflectionId }` | `ReflectionDeleted { id }`; repeat → `NoOp` |

  - **Window-as-data posture (the D-F6 consumer contract)**: the
    caller (CLI/GUI, which parsed `[preferences] day_start_time` once
    and holds the `DayCalendar`) derives `(day, window_start,
    window_end)` via the pure **`review_window(cadence, reference,
    calendar)`** (§3-E) and passes it in the payload; the planner
    never reads a calendar — it validates structurally through an
    **`ReviewWindow::try_new(cadence, day, start, end)`** A1 mint:
    `end > start`, plus per-cadence length sanity (Day = 24h; Week =
    168h; Month = end after start — month length varies, so only
    monotonicity is checkable calendar-free), and `day` consistency
    is taken on trust as mint input (the mint documents the
    leniency). Rejection: typed `InvalidReviewWindow`.
  - `UpdateReflection`'s double-`Option` slots are the change-mask
    (the `TaskChanges`/D-T7 precedent): outer `None` = unchanged;
    `Some(None)` = clear; `Some(Some(v))` = set (values minted
    through `Rating::try_new` at the planner — bad numbers are typed
    rejections, never clamps). Flags move by **delta**
    (`add`/`remove` sets), never whole-set replace — a fat-fingered
    edit cannot silently wipe the flag set. Text `Some("")` is the
    legal clear.
  - `DeleteReflection` tombstones in place (D-J2). Repeat deletes
    and updates of an already-deleted entry resolve to `NoOp`
    (idempotent-repeat precedent). Unknown id → typed
    `UnknownReflection`.
  - The walk records **one** entry per completed walk via
    `RecordReflection` with the walk's window and the prompt answers
    as text/attributes. It introduces no additional write path for
    task data (D-J9).

- **D-J7 — the family is not undoable: `NotUndoableReason::
  ReflectionSurface`, recorded with reasons.** The three commands
  classify `NotUndoable(ReflectionSurface)` (VF-7: every new family
  joins the classification with a recorded arm decision). Reasons:
  (i) the inverse of a record is a tombstone — a *different stored
  fact* than row-absence, exactly the lossiness that made `LogHabit`
  `HabitSurface` (the pre-log absent window argument); (ii) un-delete
  would resurrect with a minted-then-inverted clock — the tracking
  close/reopen asymmetry (ADR 0017) in mirror; (iii) the family's own
  verbs (`UpdateReflection`/`DeleteReflection`) are the recovery
  surface — cheaper than undo for self-authored text, and P9's
  "undo everywhere" is already served for the *task* operations the
  walk dispatches (those ride existing, classified commands).
  `command_label` grows the three arms (exhaustive match forces it).
  The task-side legs of the walk (triage moves, reschedules) are
  `MoveTask`/`UpdateTask` dispatches — already journaled per their
  landed classification; the walk adds nothing there.

### Part E — The review primitive + the day walk (D-J8–D-J9)

- **D-J8 — one parameterised primitive; the window vocabulary is
  borrowed, not duplicated; the surface is a flow, not a view.**
  - **`ReviewCadence { Day, Week, Month }`** — its own enum in
    `reflection.rs` (not `habit::PeriodKind`: separate concern,
    separate doc; the *arithmetic* is still single-implementation —
    below). The pure **`review_window(cadence, reference_date,
    calendar) -> ReviewWindow`** derives each cadence's window from
    the `days.rs` vocabulary: Day → `day_window(reference)`;
    Week → `[window_start(period_start(Week, ref)), window_start(
    period_after(Week, …)))`; Month → the calendar-month bounds the
    same way — composing the existing `habit::period_start`/
    `period_after` flooring (§1) with `days.rs::window_start`. One
    flooring implementation, two consumers (habits, review) — the
    VF-9 no-parallel-projection discipline. If the cross-module call
    reads poorly at implementation time, the *move* of
    `period_start`/`period_after` to `days.rs` (the calendar
    vocabulary home, habit re-exporting) is the sanctioned
    mechanical refactor — recorded here so the chunk does not
    invent a second floorer instead.
  - **The surface is `Flow::ReviewWalk { cadence, stage, cursor }`**
    — a subjectless XOR flow (the `Capture`/palette/focus-overlay
    precedent, §1), **not** a view: the walk is a closed ritual with
    a defined end that must never enter the view cycle or the kiosk
    rotation, and flows already carry the XOR + subject-free +
    reconcile-leaves-it-alone discipline. A view was rejected on
    three grounds: `ViewId::Review` is taken (D-J1's second
    collision), a cycle member would put the ritual on the kiosk
    rotation's candidate list, and an open-ended view has no
    "finished" semantic. Stages are a `WalkStage` enum (D-J9);
    `cursor` is the stage's row index (the palette-cursor-in-flow
    precedent); the Escape ladder is stage-back then close (the
    `TrackEntryDismiss` ladder shape); flows are XOR, so any other
    flow replaces the walk without committing it.
  - **Per-cadence specialisation is in the *information displays*,
    not the structure** (R-2's exact wording): one walk, four stages;
    the triage/reschedule/boxes stages read their task sets through
    cadence-parameterised windows. **The day walk reads
    `today_sections` verbatim** (VF-9: no parallel projection beside
    it); the week/month parameterisations read through two new pure
    window-projections in `reflection.rs` — `window_misses(tasks,
    now, window)` (live ∧ open ∧ the task's anchor instant —
    scheduled or due — fell inside the window and before `now`) and
    `window_done(tasks, window)` — both composed from the `days`
    vocabulary, no new classification of "today", no overlap with
    `today_sections`'s first-match semantics (which stay
    day-scoped). Aggregates (estimate-vs-actual drift) stay
    **#623's** territory even in the week walk's information stage —
    the walk links to the Review view's reading rather than
    re-deriving it (v0.2: the walk shows task lists, not statistics).
  - **v0.2 ships the day walk interactive; week/month stay CLI-level
    parameterisations.** The primitive (cadence enum, window
    derivation, entity cadence field, template cadence sets) is
    boundary-generic day one — #218's "cheapest to impose now" —
    but the interactive GUI walk mounts `Flow::ReviewWalk` with
    `cadence: Day` only; week/month walks are the post-v0.2 thin
    parameterisations (the #218 fence; the dispatch non-goal). The
    CLI writes week/month entries today (D-J11) — the data model
    never assumes day-only.

- **D-J9 — the walk is P13's sequence: triage → forgive → place →
  reflect.** Four stages, each a pure read + existing-or-new
  command dispatches:
  1. **Inbox triage (P1)**: the app-global Inbox stack's tasks (the
     `CaptureTask board: None` home), each row filing via
     `MoveTask` (existing verb, already classified/undoable) or
     keep-as-is. Neutral copy; no counts-of-shame.
  2. **Misses reschedule (P9)**: the window's misses (D-J8's
     projections) each offered neutral carry-forward actions —
     reschedule via `UpdateTask` (scheduled/effort slots) or leave.
     **No verdict language anywhere** (P9: "misses are data, not
     verdicts"); the concrete strings are the copy table below.
  3. **Plan tomorrow (P4)** — a **handoff transition, not a
     rendered stage** (amended 2026-11-05, user ruling "shape B"):
     advancing out of Carry forward dispatches the planner handoff
     immediately — no stage UI, no button, no click (the embedded-
     planner alternative was assessed and declined: hosting the
     planner's interaction state inside the walk flow merges two
     state machines and couples the chunks, and chunk 3 may ship
     the planner as a view, which a flow cannot embed). The **#622
     seam** stays exactly as contracted (impl-plan §5.2:
     "reachable-by-command, no shared state beyond AppState"): one
     `UiEffect` (chunk 3's naming — referenced, not owned here)
     opens the plan-next-day surface; flows are XOR so the planner
     replaces the walk; an **adapter-local return hint** (ui-
     internal, zero engine state) re-opens `Flow::ReviewWalk` at
     the Prompts stage when the planner closes — placed tasks
     appear to the prompts stage as plain state (they are
     `UpdateTask` writes, visible via AppState like everything
     else). The planner's own exit (close without placing) is the
     built-in skip — P12's permanent, unobscured exit; abandoning
     it resumes the walk at Prompts regardless (P9: nothing is
     lost, nothing is forced). If chunk 3's surface is not yet
     landed at a walk chunk's base, the leg is **skipped entirely**
     (Inbox → Carry forward → Prompts) — a simpler degrade than a
     fallback stage. **`Plan tomorrow` is recorded as the
     recommended title for the plan-next-day surface itself**
     (input to dispatch chunk 3's stage-1 naming decision, not
     binding there) — with no rendered stage, the surface's own
     title is what the user sees during this leg.
  4. **Prompted entry (P7 + P13)**: the cadence template's prompts
     rendered as fields over one `RecordReflection` commit
     (D-J6). Skipping every prompt records nothing (`NoOp`) and
     ends the walk gently.

  **What the user decides (D-J9)**: the walk and its surfacing card
  add user-visible copy in exactly the places tabled below — nothing
  else in this round carries taste-weighted text (the prompts are
  D-J4's, ruled above; stage mechanics, window derivations, and
  commands are all decided). The one constraint the strings must
  keep is the P9 rule (never verdict language — no "missed",
  "failed", "overdue"-slapping); everything else is taste. Empty
  stages render their `b` lines and auto-advance (structural,
  decided here — not part of the copy ruling).

  **Ruling status (2026-11-05, user, final)**: C1–C8, C10, C11
  **approved verbatim**. C2 amended `three prompts` → `four prompts`
  (the count follows D-J4's ruled four-prompt core). **C9 retired**
  — the third pass ruled the stage a handoff transition (shape B,
  stage list above), so no stage-3 header or button ever renders;
  `Plan tomorrow` migrates to the plan-next-day surface's own title
  as a recorded naming recommendation (D-J9).

  | ID | Surface | String | Register note |
  |---|---|---|---|
  | C1 | surfacing card title | `Day review` | the noun, not an instruction — **approved** |
  | C2 | surfacing card body | `A review for {date} is open — about two minutes, four prompts.` | effort-transparent (P13), no obligation language — **approved as amended** |
  | C3 | card action | `Review {date}` | the click affordance — **approved** |
  | C4 | walk header | `Review — {date}` | week/month phrase accordingly (`Review — week of {date}`) — **approved** |
  | C5 / C5b | stage 1 header / empty line | `Inbox` / `The inbox is empty.` | — **approved** |
  | C6 | stage 1 row actions | `File to…` / `Keep in inbox` | the only two moves — **approved** |
  | C7 / C7b | stage 2 header / empty line | `Carry forward` / `Nothing to carry forward.` | the Migration term (P9) — **approved** |
  | C8 | stage 2 row actions | `Move to tomorrow` / `Pick a date…` / `Leave as is` | the report's canonical neutral shape — **approved** |
  | C9 | stage 3 header + action | **retired** (shape B — no rendered stage) | was `Tomorrow's boxes` / `Open the planner`; superseded by the handoff transition |
  | C10 | stage 4 header | the template's `name` (`Daily close`) | copy = D-J4 data, not new text — **approved** |
  | C11 | stage 4 commit actions | `Record` / `Finish without recording` | equal-prominence exits (P12: no guilt path) — **approved** |

  **C9 history (closed 2026-11-05)**: originally the rendered third
  stage's header + button (`Tomorrow's boxes`, then opened for a
  better label); the user asked for placement/intent context,
  evaluated the embedding alternative, and ruled **shape B** — the
  stage becomes a handoff transition (stage list above), so no
  stage-3 copy exists and the C9 strings retire. The label work
  wasn't wasted: **`Plan tomorrow` is the recorded recommendation
  for the plan-next-day surface's own title** (dispatch chunk 3's
  stage-1 naming input), which is what the user sees during this
  leg. Embedding the planner's UI in the walk was assessed and
  declined (state-machine merge / duplicate-implementation
  coupling; not expressible if chunk 3 ships the planner as a
  view).

### Part F — Gentle surfacing: derived-at-read, one card, no machinery (D-J10)

- **D-J10 — the surfacing mechanism is a pure derivation, not a
  notification.** `review_due(reflection: &ReflectionState,
  calendar: DayCalendar, now) -> Option<ReviewDue>` (pure, in
  `reflection.rs`): take the **most recent day window that has
  already closed** (its `window_end <= now`), and return it iff **no
  live day-cadence entry carries that window**. The UI push pass
  turns `Some` into one **passive card on the Today view** (the
  default desktop surface, P10) — "the day is ready for review"-
  class inform copy plus the day's date phrase — activating
  `Flow::ReviewWalk { cadence: Day, … }` on click; a **global
  `ActionId` (`review.open`, palette/menu-reachable, no default
  keybinding v1 — P7: reachable, not promoted)** opens it from any
  view. Recording an entry for the window (by any lane — walk, CLI)
  clears the card at the next refresh; at most **one** card ever
  shows (the latest un-reviewed window — older gaps are visible via
  future statistics, never nagged).
  - **Rationale recorded**: R-7 says "surfaced, never forced"; P12
    says inform via presence, never assault. The derivation appears
    within a minute of the boundary on the wall-minute repaint
    (§1), needs **zero** new engine machinery — no tick arm, no
    notification plumbing, no occurrence keys, no dismissal state
    (a dismiss button that persists nothing would be a lie; a
    dismissal that persists state is reminder machinery — both
    declined). The wall-minute repaint cost is one pure set-scan of
    a small map — no kiosk-relevant budget moves.
  - **The VF-4 tick ride-in is considered-and-declined for v1**
    (recorded, per the settle-list's "riding VF-4's tick" lean):
    the day-roll tick's only surfacing contribution would be
    re-publishing state the repaint already re-derives; a
    *push-notification* leg at the boundary (riding the tick +
    notifier) is a legitimate future addition with a consent
    posture — fenced (§7), not designed here.
  - **Week/month never auto-surface in v0.2** — their walks are
    command-reachable parameterisations (D-J8); auto-surfacing them
    would be the nagging P12 forbids (three cards on Monday 00:05)
    and is part of the post-v0.2 parameterisation fence.
  - **Kiosk: nothing journal-shaped renders anywhere** (D-J14).

### Part G — CLI + export (D-J11–D-J12)

- **D-J11 — the CLI family: `taskboard journal …`** (the timers
  two-lane recipe verbatim: one-shot wrappers via `with_core` +
  session `*_on` seams + one `collapse()` definition site; typed
  receipts in `output.rs`; values read back from the published
  snapshot, never echoed):

  ```
  journal add [--cadence day|week|month] [--day <YYYY-MM-DD>]
              [--mood <1-5>] [--energy <1-5>] [--flag <name>]… [TEXT…]
  journal list [--cadence <c>] [--since <date>] [--until <date>]
  journal show <selector>
  journal edit <selector> [--text <t>|--clear-text] [--mood <n>|--clear-mood]
               [--energy <n>|--clear-energy] [--flag <name>]… [--unflag <name>]…
  journal delete <selector>
  ```

  - `--day` defaults to today (`DayCalendar::today`); it is the
    *reference date* the window derives from (a week entry with
    `--day` mid-week floors to that week — the canonical-mint law).
    The app layer derives the window from the parsed
    `day_start_time` + OS offset (the D-F6 seam); the planner
    re-validates structurally (D-J6).
  - Entry selectors resolve like `resolve_alarm` (exact uuid →
    unique ≥8-char prefix; typed `UnknownReflection`/
    ambiguous-candidates errors, never text-matched).
  - `add` is capture-at-speed (P1): positional text, zero required
    flags, never prompts. **No `$EDITOR` spawn in v1** (fenced, §7).
  - Lane: mutating subcommands → `IpcOrLocal` (the daemon owns the
    state, the timers/track precedent); `list`/`show` are pure
    reads over the published snapshot. Human renderers project
    phrases (`mood 3/5 · migraine · 2 flags`-class machine columns
    + human text form); receipts typed.
- **D-J12 — the export leg reuses the `export` verb additively.**
  New flag `--what <tracking|journal>`, **default `tracking`** —
  bare `taskboard export` is byte-compatible with today (DR-5
  D-T15's raw-payload contract untouched: CSV default, `--json`
  array, stdout-only, IPC relays bytes verbatim). `--what journal`
  columns: `entry_id, day, cadence, window_start, window_end, mood,
  energy, flags, text, created_at, updated_at`; filters `--since`/
  `--until` inclusive on `window_start`; **tombstoned rows are
  skipped** (export is the live journal's data; tombstones are
  internal bookkeeping). The shared CSV writer/JSON envelope
  machinery is `track.rs`'s, reused — no second serializer.

### Part H — Storage (D-J13)

- **D-J13 — migration 0022, one new `PersistenceAction`.**

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

  The 0008/0009/0015 recipe (hyphenated-UUID TEXT ids, RFC 3339
  stamps, no FKs — nothing references a reflection entry, and
  entries outlive nothing: they reference no task). **Exactly one
  new action: `UpsertReflectionEntry(ReflectionEntry)`** — the
  tombstone-in-place ruling (D-J2) makes delete an upsert, so the
  exhaustive-match forcing (VF-2) needs a single arm and memory ≡
  disk holds by construction (the D11 one-function-two-callers
  principle). Codec text fns per column + `decode_reflection_entry`;
  the flags columns roundtrip through the entity's own
  serde-compatible encoding (known names validated at decode into
  `HealthFlag`, unknown parked into `extra_flags` — the decode seam
  is where tolerance lives, the D-H12 posture).

### Part I — Kiosk posture (D-J14)

- **D-J14 — the walk is desktop-only; nothing journal-shaped renders
  on the kiosk in v0.2.** The focus-overlay structural exclusion is
  the template (VF-6: three-layer pinned): the `review.open` action
  dispatch no-ops on kiosk (the adapter's structural gate, D-F13
  shape), the flow cannot be entered, and the push pass pushes no
  journal/review rows onto any kiosk-visible model (`review_due`'s
  card is Today-view-only and Today-on-kiosk renders no card — the
  gate is in the push pass, model-side, the `push_notifications`
  pattern). The kiosk fence rulings (impl-plan §1: "kiosk editing of
  any review/journal surface — passive display rulings per feature")
  are honored at their strictest reading: v0.2 ships no passive
  journal display either (a future "reflection of the day" kiosk
  card is fenceable later, not designed now). Pins extend the
  existing kiosk gate test set.

---

## 4. Decision summary

| ID | Question | Decision | Status |
|---|---|---|---|
| D-J1 | The VF-3 vocabulary collision | **`reflection` family in all code symbols** (`reflection.rs`, `ReflectionEntry/Id/State/Template`, `ReviewCadence`, `ReviewWindow`, `HealthFlag`); **`journal` as the user-facing verb/selector** (CLI `taskboard journal …`, export `--what journal` — #621's own wording; the undo journal has no CLI verb); guard rules recorded (no `Journal*` code symbols outside journal.rs/undo.rs; table = `reflection_entries`; UI `Review` is taken by statistics → the walk is `ReviewWalk` everywhere) | **RULED (the contract charges this round)** |
| D-J2 | Entity shape | `ReflectionEntry` — roaming-family v7 id, cadence + day label + stored window instants (self-contained; no read-time derivation across devices), text, attributes, `created_at`/`updated_at` LWW clocks, `deleted` **tombstone-in-place** (the Habit precedent — the contract's "tombstone-shaped rows" read literally) | recorded |
| D-J3 | Attribute representation | `ReflectionAttributes { mood/energy: Option<Rating> (A1 1..=5), flags: BTreeSet<HealthFlag> (curated enum, append-only, R-5), extra_flags: BTreeSet<String> (unknown names parked verbatim — tolerant decode) }`; multiple entries per window legal. **Flag set RULED (2026-11-05, user)**: `migraine`, `minor-headache`, `medium-headache`, `cold-or-flu`, `allergies`, `cramps`, `poor-sleep`, `overwhelmed`, `good-day` | **RULED (2026-11-05, user)** |
| D-J4 | Template representation | Typed `ReflectionTemplate { id: slug, name, cadence, prompts }` data; v1 = compiled-in curated const set, implicit per-cadence selection, no stored rows, no config section (P7); the type shape is the roaming-ready seam for the user-editable fence. **Prompt core RULED (2026-11-05, user)**: achievements / gratitude / improvement ideas, plus the open-loop prompt `What's still on my mind?` (promoted same day, second pass) — concrete per-cadence texts in §3-C | **RULED (2026-11-05, user)**; 0–2 optional extras may still be picked at acceptance |
| D-J5 | State home | `ReflectionState { entries: BTreeMap<ReflectionId, ReflectionEntry> }` on `AppState.reflection` + `PersistedState` (the nested-state precedent); `ReflectionId` in the roaming id family; tombstones stay in-map, reads filter | recorded |
| D-J6 | Verb set + planner semantics | `RecordReflection` / `UpdateReflection` / `DeleteReflection` + receipts (`ReflectionRecorded/Updated/Deleted`, `NoOp` idempotence, typed `UnknownReflection`/`InvalidReviewWindow`/`UnknownHealthFlag`); **windows are data** (caller derives via `review_window` + `DayCalendar`, planner validates structurally through the `ReviewWindow::try_new` A1 mint — calendar-blind, the D-F6 contract); `UpdateReflection` double-Option change-mask + flag *delta* (never whole-set replace) | recorded |
| D-J7 | Undo arm | `NotUndoableReason::ReflectionSurface` for the family (reasons recorded: tombstone-inverse lossiness = the LogHabit argument; the family's own edit/delete verbs are the recovery; the walk's task legs ride classified commands); `command_label` grows three arms | recorded |
| D-J8 | Review primitive + cadence mechanics | `ReviewCadence { Day, Week, Month }` (own enum) + pure `review_window(cadence, reference, calendar)` composing `habit::period_start`/`period_after` flooring with `days.rs::window_start` (**one flooring implementation**; days.rs relocation sanctioned as mechanical refactor if the call reads poorly); the surface is **`Flow::ReviewWalk`** (subjectless XOR flow — not a view: `ViewId::Review` taken, ritual needs an end, rotation exclusion); per-cadence specialisation lives in the information displays (day reads `today_sections` verbatim — VF-9; week/month via new `window_misses`/`window_done` pure projections); v0.2 ships the **day walk interactive**, week/month stay CLI parameterisations (the #218 fence) | recorded |
| D-J9 | The walk stages + the #622 seam | Three rendered stages — inbox triage (P1, `MoveTask` dispatches) → misses reschedule (P9 neutral copy, `UpdateTask` scheduled/effort slots) → prompted entry (one `RecordReflection`) — joined by the **plan-tomorrow handoff transition** (**reachable-by-command**: advancing out of Carry forward dispatches one `UiEffect` that opens chunk 3's surface directly — no rendered stage, no click; XOR flow replacement; adapter-local return hint re-opens the walk at Prompts; the planner's own exit is the skip; zero shared state beyond AppState; planner-absent base skips the leg); no arithmetic (R-6 respected by construction). Embedding the planner in the walk assessed and declined (coupling). Copy **fully ruled**: C1–C8, C10, C11 verbatim (C2 amended to four prompts); C9 retired; `Plan tomorrow` recommended as the planner surface's title | **RULED (2026-11-05, user, shape B)** |
| D-J10 | Gentle surfacing | **Derived-at-read, zero machinery**: pure `review_due(reflection, calendar, now)` → at most one passive card on the Today view for the latest closed, un-journaled day window; global `review.open` action (no default keybinding v1); recording any entry clears it; **the VF-4 tick ride-in and the reminders-pipeline alternative both considered-and-declined** (reasons: P12 presence-not-push, no dismissal state, no occurrence machinery); a boundary push-notification leg is fenced; week/month never auto-surface | recorded |
| D-J11 | CLI verb set | `journal add/list/show/edit/delete` (two-lane recipe; `--cadence/--day/--mood/--energy/--flag` mint flags; change-mask edit flags with explicit `--clear-*`; set-delta `--flag/--unflag`); uuid/prefix selectors; mutating lane `IpcOrLocal`; no `$EDITOR` spawn v1 (fenced) | recorded |
| D-J12 | Export leg | `export --what <tracking\|journal>`, **default `tracking`** (bare export byte-compatible); journal CSV columns fixed; filters inclusive on `window_start`; tombstones skipped; the track.rs serializer machinery reused, no second serializer | recorded |
| D-J13 | Storage | Migration **0022** `reflection_entries` (re-verified 2026-11-05: 0001–0021 taken; chunks re-verify at base); **one** new `PersistenceAction::UpsertReflectionEntry` (tombstone-in-place makes delete an upsert); typed columns, flags as JSON-array TEXT + parked-extra column; no FKs | recorded |
| D-J14 | Kiosk posture | Walk desktop-only via the focus-overlay structural exclusion (three-layer pins extended); the push pass carries nothing journal-shaped onto kiosk-visible models; no passive journal display in v0.2 either (strictest reading of the per-feature passive rulings) | recorded |
| D-J15 | ADR | Chunk 1 writes **ADR 0020** ("the reflection family — device-local by ruling, roaming-ready by construction; the boundary-generic review walk; derived-at-read gentle surfacing"), recording D-J1/J2/J6/J7/J10 and the fence list (next number re-verified at base: 0019 is last today). **Amended 2026-11-05 (stage 3)**: the parallel DR-8 files-substrate round (`2026-11-05-filesync-substrate-implementation-dispatch-plan.md` §6) also reserves 0020 for its core-crate ADR — the re-verify-at-base rule governs: whichever chunk reaches its base second takes the next free number; renumbering is the sanctioned mechanical response, never a content change | recorded |
| — | Round scope | One primitive, day-interactive v0.2; roaming/statistics/templates/interactive-week-month/LLM/`$EDITOR` all fenced to §7 or owned issues; acceptance is the user's gate and was **granted in-session 2026-11-05** (the round never self-accepted — see the status header) | recorded |

**User rulings — final status after the 2026-11-05 in-session
passes (every mechanism and every string is now decided):**

1. **D-J3 — RULED (2026-11-05, user)**. The `HealthFlag` v1 set is
   fixed: `migraine`, `minor-headache`, `medium-headache`,
   `cold-or-flu`, `allergies`, `cramps`, `poor-sleep`,
   `overwhelmed`, `good-day` (`low-medication-day` removed at the
   user's direction; the two headache grades added).
2. **D-J4 — RULED (2026-11-05, user, two passes)**. The prompts are
   the user's three practiced categories — achievements, gratitude,
   improvement ideas — plus the open-loop prompt `What's still on
   my mind?` (promoted from the optional list in the second pass):
   four per cadence, concrete texts in §3-C. At acceptance the user
   may still append 0–2 of the remaining extras (energy narration,
   memory-keeping) — an option, not an open decision.
3. **D-J9 — RULED (2026-11-05, user, third pass, shape B)**. The
   plan-tomorrow leg is a handoff transition, not a rendered stage:
   Carry forward flows directly into the plan-next-day surface, the
   planner's own exit is the skip, Prompts follows. C1–C8, C10, C11
   approved verbatim (C2 amended to four prompts); C9 retired;
   `Plan tomorrow` recorded as the recommended planner-surface
   title.

**No decision is open.** The user's acceptance gate was passed
2026-11-05 (in-session; the status header records it), and stage 2
has since opened the §7 chunk issues (#636–#638) and fence issues
(#639–#642) and annotated #621/#218 — all as this section's tables
describe.

## 5. Test-strategy hooks (for the implementation chunks)

- **Domain properties (chunk 1)**: `review_window` floors canonically
  per cadence (proptest over reference dates × `DayCalendar`s: Day
  windows are exactly 24h and contain the reference; Week bounds are
  Monday-anchored 168h; Month bounds are `[1st, next 1st)`), equal
  inputs equal outputs; `Rating`/`ReviewWindow` mints reject typed
  (0, 6, inverted spans, cadence-length mismatches); serde
  roundtrips are byte-stable **including** `extra_flags` parking (an
  unknown flag name survives decode→encode verbatim) and tombstones;
  `window_misses`/`window_done` partition correctly against
  generated task sets (the `today_sections` property shape); the
  day-cadence `window_misses` result is consistent with
  `today_sections`'s overdue+scheduled-past reading where both
  apply (an equality pin over generated states — no second
  classification).
- **Planner (chunk 1)**: every verb on `FixedClock` `now` — receipts
  carry the stamped clocks; `RecordReflection` mints id + clocks and
  stores the caller's window verbatim; structural rejections
  (`InvalidReviewWindow`, `UnknownHealthFlag`, bad `Rating` input)
  are typed; `NoOp` for empty records, all-unchanged updates,
  repeat deletes, deleted-entry updates; `UpdateReflection`
  change-mask semantics (`Some(None)` clears; flag delta never
  replaces); undo classification refuses with `ReflectionSurface`
  (typed).
- **Engine (chunk 1)**: record → row visible in the published
  snapshot; delete → tombstone visible, reads filtered;
  apply-to-storage-then-interpret crash-resume (a fresh engine over
  the same repo lists the entry — deterministic, join/predicate
  waits only); insta snapshots grow the `reflection` section.
- **Storage (chunk 2)**: migration-upgrade test (empty table folds);
  codec roundtrips incl. NULL mood/energy, empty/unknown flags,
  tombstoned rows; the single upsert action in one transaction.
- **CLI/app (chunk 2)**: `cli_journal.rs` in the `cli_timers.rs`
  shape — hermetic tempdir boots, typed receipts, IPC relay leg,
  `--json` envelopes; window derivation from a fixed
  `day_start_time`/offset (fixed stamps — the app layer's calendar
  injection seam); export golden CSV/JSON against constructed
  fixtures (golden files are data, not labels); the **bare
  `export` byte-compat pin** (default `--what tracking` output
  unchanged); selector resolution typed errors.
- **UI (chunk 3)**: pure-fn units for `review_due` (fixed-stamp
  calendars: no entries → due; entry for the latest closed window →
  not due; older un-journaled windows never surface; week/month
  never due) and card/stage derivations; flow journeys assert the
  exact `StateCommand`s/`UiEffect`s dispatched (typed, no text) —
  including the handoff transition (Carry-forward advance
  dispatches the planner `UiEffect`; planner close resumes at
  Prompts; planner-absent base skips the leg); kiosk pins extended
  (action dispatch no-ops; no journal-shaped push reaches
  kiosk-visible models); keymap/preset snapshots regen as data.
- **Gates**: insta snapshots grow the section (persisted dumps only);
  `cargo deny` untouched (zero new dependencies — no chart/serde
  crates; everything is hand-rolled per the house stack); no log/UI
  text assertions anywhere (AGENTS §5); deterministic tests (no
  sleeps, fixed stamps, `tokio::time::pause`).

## 6. Risks & mitigations

- **Vocabulary confusion escaping the ruling** (the VF-3 risk the
  impl-plan itself names): mitigated by D-J1's guard rules + the
  layer split (code = `reflection`, surfaces = `journal`); the
  stage-3 review checklist (dispatch §2 trap (a)) greps for
  `Journal*` symbols outside the undo modules and
  `journal_entries`-spelled storage.
- **Two review surfaces confuse contributors** (`ViewId::Review`
  statistics vs `Flow::ReviewWalk`): names ruled distinct (D-J1);
  both docs (#214's and this round's) cross-reference; the walk
  never links INTO the statistics view in v0.2 (aggregates stay
  #623's, D-J8).
- **Planner-leg coupling creep** (the walk absorbing plan-next-day
  logic): the seam contract is reachable-by-command with an
  adapter-local return hint, zero shared state beyond AppState
  (D-J9); R-6's no-arithmetic fence holds because the walk ships no
  numbers at all; stage-3 trap (b/e of chunk 3's list) applies
  mutatis mutandis to the walk chunk.
- **The card drifts into nagging** (P12 violation): structurally
  bounded — at most one card, Today-view-only, no persistence, no
  push, no dismissal state to get wrong (D-J10); the notification
  leg that *could* nag is fenced behind a consent posture rather
  than shipped.
- **Cross-device window semantics** (day_start_time differs per
  device): windows are stored self-contained at mint (D-J2), so no
  reader ever re-derives; the label-vs-instant distinction is
  pinned by the `review_window` properties; the future import-fold
  union keys on entry UUID, not window — collisions are
  co-existing entries, never corruption (the options report's "the
  merge that cannot meaningfully corrupt").
- **Tombstone-in-place vs the files-sync fold**: the row *is* the
  tombstone (contract §3.4's literal reading) — the import fold
  reads `deleted` rows as tombstones directly, no derivation; the
  files-sync round validates this at its import design (the
  contract's named consumer); nothing in v0.2 blocks or precludes
  it.
- **Migration number collision** (0022 vs parallel chunks): the
  dispatch runs storage-bearing chunks single-flight per batch; the
  implementing chunk re-verifies the next free number at its base
  and records any collision (VF-2's standing rule).
- **Reflection store growth**: ~0.5–2 KB/day (options report §7
  scale sanity) — a decade is single-digit MB; tombstoned rows stay
  (contract: never silent absence); a retention/GC design is named
  as future work in the ADR's consequences section, not designed
  now.
- **Acceptance slips** → the #621 implementation chunks (§7's
  targets) slip with it; nothing else in the wave waits on this
  round (impl-plan §8's risk, unchanged).

## 7. Chunk mapping → the issues this round opens

Implementation chunks (**opened 2026-11-05 on acceptance** —
#636 / #637 / #638 in the table's order; each body = summary +
permalink here + the chunk's decision IDs; `backlog:` prefix, v0.2
milestone, labels per AGENTS §7):

| Chunk | Scope | Size | Issue title |
|---|---|---|---|
| 1 — domain & engine | D-J1–J7, D-J15: `reflection.rs` (entity, attributes, `ReviewCadence`, `review_window`, `window_misses`/`window_done`, curated templates const), `ReflectionState` home, the three commands + planner semantics, undo arm + `command_label`, properties, **ADR 0020** | M | `backlog: DR-7 chunk 1 — reflection domain & engine (entries, cadence windows, commands, undo arm)` (labels: `architecture`, `p2`) |
| 2 — storage & CLI/export | D-J11–J13: migration 0022 + `UpsertReflectionEntry` + codec, `journal` verb family (two-lane), `export --what journal` leg | M | `backlog: DR-7 chunk 2 — reflection storage & journal CLI/export (migration 0022, verbs, export leg)` (labels: `dx`, `p2`) |
| 3 — review walk UI | D-J8–J10, D-J14: `Flow::ReviewWalk` (day cadence) + stages + neutral copy, the #622 handoff seam + return hint, `review_due` card + `review.open` action + keymap snapshot regen, presentation fns, kiosk three-layer pins | M/L | `backlog: DR-7 chunk 3 — review walk UI & gentle surfacing (day walk, planner handoff, today-view card)` (labels: `ui`, `p2`) |

Sequencing: 1 → 2 → 3 (3 needs the verbs and reads the stored
entries; the walk's planner leg additionally needs chunk 3 of the
parent dispatch (`feat/plan-next-day`) landed — the leg is skipped
per D-J9's shape B otherwise). Accepted 2026-11-05: the round is
annotated onto #621 and #218; the chunk issues close with the
follow-up implementation round's last chunk (dispatch §10).

Fence issues (**opened 2026-11-05 on acceptance** — #639–#642;
out-of-scope by recorded rulings; each carries a permalink here):

| Fence | Issue title |
|---|---|
| User-editable reflection templates (the follow-up dimension; template rows become stored/roamed entities; slug namespace reservation) | `backlog: user-editable reflection templates (DR-7 follow-up dimension)` (`ui`, `p3`) |
| Interactive weekly/monthly walk sessions (the #218 thin parameterisations: GUI walks at Week/Month cadence + week-scoped planner leg) | `backlog: interactive weekly/monthly review sessions (#218 parameterisations over the DR-7 walk)` (`ui`, `p3`) |
| Boundary push-notification leg (riding the day-roll tick + notifier, consent posture per P12) | `backlog: review-boundary notification leg (DR-7 D-J10 follow-up, consent-gated)` (`ui`, `p3`) |
| CLI `$EDITOR`/multi-line input for `journal add` | `backlog: journal CLI editor-spawn input (DR-7 fence)` (`dx`, `p3`) |
| Reflection statistics/trends + past-entries browsing | **owned by #623 slice B** — referenced, not re-opened |
| Journal roaming | **owned by milestone `nextcloud-files-sync`** (the contract's first consumer) — referenced, not re-opened |

## 8. References

- Mandates: issue #621 (+ the three 2026-11-04 ruling comments —
  cadence, health-flag, surfacing, roaming-posture) and issue #218
  (the boundary-generic constraint, verbatim above); the companion
  dispatch plan §2 (this chunk's protocol, prompts, and acceptance
  gate).
- Normative plan:
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  — §2 rulings R-1/R-2/R-5/R-7, §3 the cross-milestone contract,
  §4 VF-1…VF-9, §5.1 this round's settle-list, §6 the chunk table.
- Origin analyses: `.artifacts/reports/2026-10-14-adhd-support-research-report.md`
  §5 (P1 capture, P2 externalised time, P4 boxes, P7
  constrain-by-default, P9 forgiveness, P12 gentle-by-default, P13
  temporal-landmark review); `.artifacts/reports/2026-11-04-per-user-appdata-roaming-options-report.md`
  §4.C/§7/§9 (device-local ruling, import-fold expectation, scale
  sanity).
- House-style precedent: DR-5
  (`.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md` —
  the entity/section/CLI/export recipe, the decision-table form,
  the chunk-issue convention this round's §7 extends).
- Adjacent rounds: DR-6
  (`.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  — the period vocabulary this round borrows); the effort-calibration
  plan (`2026-10-31-effort-calibration-surfaces-plan.md` — the Review
  home's D-8/D-9 discipline); the respite/break plan
  (`2026-10-31-respite-break-reminders-plan.md` — occurrence-key
  precedent cited by the declined surfacing alternative); the
  pomodoro plan (`2026-10-31-pomodoro-focus-plan.md` — the kiosk
  three-layer exclusion template).
- In-tree seams: `journal.rs` (the collision), `tracking.rs`/`habit.rs`
  (the entity precedents), `days.rs` (the window vocabulary),
  `undo.rs` (the classification site), `view_state.rs` `Flow`/
  `ViewId` (the surface precedents and the `Review` collision),
  `models.rs`/`presentation.rs` (the push/purity patterns),
  `commands/timers.rs`/`track.rs` (the CLI recipes),
  `docs/adr/0017`–`0019` (the ADR line chunk 1 extends with 0020).

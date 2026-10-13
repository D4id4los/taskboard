# [Plan] Reminders Chunk 3 — App Wiring, Config, CLI (TDD-first)

- **Date**: 2026-10-14
- **Kind**: implementation plan (dispatch chunk 3, stage 1) — no
  production code in this commit.
- **Origin**: design round
  `.artifacts/plans/2026-10-12-reminders-design-plan.md` (§5.4 config,
  §6 NotificationSink/LogSink, §11 CLI sketch, §13 D3/D5 — normative)
  and dispatch plan
  `.artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md`
  §1/§4 (chunk 3 scope + acceptance).
- **Branch / worktree**: `feat/reminders-app-wiring` in
  `../taskboard-WT-feat-reminders-app-wiring`, off `main` @ `3cd2103`
  — chunks 1 (domain model) and 2 (engine tick arm + storage) are
  already merged into `main` (`1207ef0`, `0da95e2`, `53ac27c`,
  `3cd2103`), so the serial default applies: **off `main`, not
  stacked** (dispatch §1).
- **Goal**: the app-layer half of reminders — bootstrap wires the
  notifier channel to a `NotificationSink` (v1 impl: `LogSink`), the
  `[reminders]` figment table reaches `ReminderEngineConfig`, the
  `tasks remind` verb family manages rules over the existing selector
  cascade, `tasks list` grows the overdue/due-soon marker column, and
  a hermetic E2E proves rule → tick → delivery → notification on a
  fake sink. The final commit marks the backlog origin entry ✅ DONE.
- **Status**: plan for stage-2 implementation; TDD-first, every commit
  standalone (rebase-merge hygiene, AGENTS §8).

---

## 1. Context & verified facts (anchored to `main` @ `3cd2103` — do not re-derive)

1. **The interim notifier wiring is marked for replacement.**
   `crates/taskboard-app/src/bootstrap.rs:561-564`: the notifier
   receiver is dropped (`_notifications`) and
   `taskboard_state::ReminderEngineConfig::disabled()` is passed to
   `spawn_state_engine`. Chunk 3 replaces both: the receiver feeds a
   sink consumer task, and the config comes from figment.
2. **The engine seam is complete and takes the sender by value.**
   `spawn_state_engine(..., notifier: mpsc::Sender<Notification>,
   reminder_cfg: ReminderEngineConfig)`
   (`crates/taskboard-state/src/actor.rs:234-242`). The tick arm's
   **first tick is immediate** (`next_tick = tokio::time::Instant::now()`,
   actor.rs:340-342): any boot with reminders enabled and an
   undelivered past-due firing notifies *during that boot*. The tick
   path persists delivery markers **before** sending notifications
   (actor.rs:783-805), and a closed notifier channel is a `debug!`
   no-op (`send_notifications`, actor.rs:830-834).
3. **`ReminderEngineConfig` deliberately carries no production
   policy** (`crates/taskboard-state/src/config.rs:19-33`): chunk 3
   encodes the defaults (30 s tick, 5 m before due) in figment.
4. **The domain has snooze + mute commands but no rule-management
   commands.** `StateCommand::SnoozeReminder` and
   `SetTaskRemindersMuted` exist (`crates/taskboard-domain/src/messages.rs:144-160`);
   `SetTaskRemindersMuted` plans to `CommandOutcome::Applied` with a
   `no_op()` repeat (command.rs:647-660). There is **no** command to
   set or clear a rule, and `CommandOutcome` (command.rs:96-111) has
   no reminder variants — the dispatch plan assigns "CommandOutcome
   additions" to chunk 3, so the rule verbs' domain half lands here.
5. **The persistence vocabulary is complete** (`persistence.rs:199-214`):
   `UpsertReminderRule`, `DeleteReminderRule`,
   `RecordReminderDelivery`, `RecordReminderSnooze`,
   `SetTaskRemindersMuted` — one semantics each in `apply_actions`.
   Task delete already cascade-deletes its rules (command.rs:200).
6. **`plan_command` generates ids at plan time**
   (`plan_command(app, outbox, command, ids: &dyn IdGenerator, now)`,
   command.rs:222-228; the `CreateTask` precedent stamps
   `CommandOutcome::CreatedTask(id)`). The engine **constructs** but
   never exhaustively **matches** `CommandOutcome` (actor.rs:518-520),
   so new outcome variants are additive everywhere.
7. **`CommandError::error_kind` already maps the reminder rejections**:
   `unknown-reminder-rule`, `invalid-snooze-postpone`
   (commands/mod.rs:299-300) — chunks 1–2 landed these.
8. **Config is a strict-table figment chain**
   (`crates/taskboard-app/src/config.rs:477-482`):
   `Serialized::defaults(AppConfig::default())` → `Toml::file(path)` →
   `Env::prefixed("TASKBOARD_").split("__")`. Every section is
   `#[serde(deny_unknown_fields)]`; `[sync]` uses plain `u64` seconds
   ("no string duration parsing", config.rs:149-151) converted at
   `sync_actor_config()` (config.rs:441-447) — the mapping-accessor
   precedent `reminder_engine_config()` will follow. Semantic checks
   surface as typed `ConfigError` variants from `validate()`;
   `ConfigError::InvalidDuration { field }` + `kind() ==
   "invalid-duration"` already exist (config.rs:237-241, 255-265).
   Figment's `Env` coerces `true/false`→bool, ints→numbers, and
   everything else (e.g. `5m`) to a string (figment 0.10.19
   `providers/env.rs` docs) — one string-typed field decodes file and
   env identically.
9. **Bootstrap is a two-parameter port-injecting constructor**
   (`bootstrap(config, credentials)`), doc-tested with an injected
   fake store (bootstrap.rs:15-54). `App::shutdown` destructures the
   whole struct, drops the parked `reports` sender **before** the
   engine join, joins the sync actors bounded, then the engine, then
   storage (bootstrap.rs:328-377); a storage-join failure is logged
   (`warn`) without changing `ShutdownOutcome` (bootstrap.rs:373-375)
   — the precedent the notifier join follows.
10. **The verb layer has a fixed two-lane shape**: one-shot wrappers
    (`tasks::add(config, store, …)` → `with_core`) and session seams
    (`tasks::add_on(app, …)`) consumed by `dispatch_line`
    (commands/mod.rs:828-1063, tasks.rs:34-50). Output projections
    are typed serde structs in `commands/output.rs`; errors flow
    through `CommandError` → `error_kind()`/`error_detail()` → the
    kebab-case `{"error":{"kind":…}}` envelope (mod.rs:1179-1194).
    `emit_result`/`run_core`/`run_mutation` are the dispatch helpers.
11. **`resolve_task` is the cascading selector** (exact uuid → unique
    ≥8-char prefix → exact live title; tasks.rs:480-522) — the remind
    verbs reuse it verbatim.
12. **`route_for_ipc` routes `Command::Tasks { .. }` wholesale**
    (mod.rs:1121-1126) — a new `TasksCmd` variant needs **no** routing
    change. The REPL completion catalogue enumerates task subverbs
    (`completion.rs:68-78, 256`) and must grow `remind`.
13. **Hermetic verb tests are external integration suites over real
    sqlite tempdirs** (`tests/cli_verbs.rs` + `tests/common/mod.rs`:
    `FakeCredentialStore`, `env_lock`/`EnvOverride`, `tempdir`), each
    invocation a full boot/work/shutdown; config tests
    (`tests/config.rs`, C-series) drive the real figment chain under
    the process env mutex.
14. **The backlog origin entry** is
    `.artifacts/taskboard-potential-improvements-backlog.md:422`
    ("[2026-10-08] Reminders/notifications subsystem") with a 2026-10-12
    status note ending "This entry stays open until those land." The
    four deferred-idea entries (reminder sync, insistence levels,
    multi-reminder UX, delivery pruning) stay untouched (dispatch §5).
15. **No `futures` crate exists in the workspace** (root `Cargo.toml`)
    — relevant to the port's boxed-future return type (§3 DA3).

## 2. Scope & non-goals

**In scope**: `NotificationSink` port + `NotifyError` + `LogSink` +
notifier consumer task (app crate); `bootstrap` signature/wiring and
`App::shutdown` join; `[reminders]` figment table + validation +
`reminder_engine_config()` mapping; domain additions
`StateCommand::SetReminderRule` / `ClearReminderRules` +
`CommandOutcome::{ReminderSet, ReminderCleared, ReminderMuted}` +
`SetTaskRemindersMuted` outcome change; the `tasks remind` verb family
(one-shot + session lanes, projections, text renderers, `--json`); the
`tasks list` marker column; REPL completion; the hermetic test suites
(C-series additions, new R-series, E2E); the backlog ✅ DONE commit.

**Non-goals**: `DesktopSink`/`notify-rust` and the interactive
snooze/dismiss surface (design plan 5); any CLI snooze verb (design
§11: snooze is the notification-interaction path, not a v1 verb); a
config knob for the due-soon horizon or the sink kind; reminder sync
(anything entering the outbox); `tasks list` filtering flags;
multi-rule management surfaces (N rules stay model-level); new
dependencies (zero — the port's boxed future is std-only).

## 3. Decisions (resolved; deviations from the design round carry reasons)

- **DA1 — base branch**: off `main` @ `3cd2103` (chunks 1+2 merged;
  dispatch §1 serial default). No stacking, no rebase dance expected.
- **DA2 — sink injection is a third `bootstrap` parameter**
  (`Arc<dyn NotificationSink>`), exactly the `CredentialStore`
  precedent (§1.9): `main` passes `Arc::new(LogSink)`; tests pass a
  recording fake. *Rejected*: config-driven sink selection (no such
  knob in the design; D3 says v1 is LogSink, full stop) and a
  `bootstrap_with_sink` twin (two entry points drift). The doc-test
  and every existing bootstrap caller update mechanically.
- **DA3 — port module and the boxed future**: new
  `crates/taskboard-app/src/notifications.rs` holding
  `NotificationSink`, `NotifyError`, `LogSink`, `spawn_notifier`, and
  `NOTIFIER_CAPACITY` (64, the interim literal at bootstrap.rs:564
  promoted to a documented const). **Deviation** from design §6's
  `BoxFuture<'_, …>` return: the workspace has no `futures`
  dependency, so the signature uses the std equivalent
  `Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + '_>>`
  (via `Box::pin`) — same contract, zero new deps; plan 5 can swap in
  `futures::future::BoxFuture` (a type alias) if it ever needs the
  crate. `NotifyError` carries one variant, `Delivery`, the abstract
  failure class — the port result must be fallible for plan 5's OS
  sink, and an uninhabited error type would make the signature a lie.
  `LogSink::deliver` emits one `tracing::info!` with typed fields
  (`reminder = %…text_form, task = %uuid, title, occurrence`) and
  returns `Ok(())` — it cannot fail.
- **DA4 — consumer lifecycle**: `bootstrap` spawns the consumer
  (`spawn_notifier(notifications, sink)`) immediately after creating
  the channel halves (bootstrap owns halves, ADR 0008), *before*
  `spawn_state_engine` (the engine's immediate first tick can notify
  during hydration-heavy boots; a consumer that exists late would race
  the capacity, not correctness — but ordering it first is free).
  Termination: the engine task owns the only sender clone; its exit
  closes the channel, `recv()` yields `None`, the consumer exits. In
  `App::shutdown` the consumer join runs **after** the engine join
  (`join_bounded(notifier_join, timeout, "notifier")`), logging a
  `warn` on join error and aborting on timeout — the storage-actor
  precedent (§1.9). `ShutdownOutcome` is unchanged: the field documents
  the *sync* actor, and a log-line flusher missing its budget is not
  process-outcome-relevant.
- **DA5 — `[reminders]` table shape and duration decoding.**
  ```rust
  /// `[reminders]` table.
  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct RemindersSection {
      pub enabled: bool,          // default true   (§5.4)
      pub tick_secs: u64,         // default 30     (§5.4)
      pub default_before: String, // default "5m"   (§5.4 / D5)
  }
  ```
  `default_before` is a **plain string parsed in `validate()`**, not a
  serde-newtype: a bad value must surface as
  `ConfigError::InvalidDuration { field: "reminders.default_before" }`
  (kind `invalid-duration`) — the design round's named vocabulary
  ("the `invalid-duration` config error kind already exists", §5.4) —
  whereas a `Deserialize` failure would collapse into the generic
  `decode` kind. `extract_raw` (the login path) skips `validate`, so
  an unparseable value there is inert until an accessor runs —
  accessors are only called post-`load`/post-`validate`
  (`server_url()` precedent). Validation: `default_before` must parse;
  `tick_secs != 0` (the `sync.poll_interval_secs` mirror); the string
  stays stored raw so `config print` echoes the user's own form.
  **Grammar** (one definition site shared with the CLI `--before`
  flag): a non-empty sequence of `<unsigned-int><unit>` components,
  unit ∈ `s | m | h | d`, at least one component, no sign, no spaces
  (e.g. `30s`, `5m`, `1h30m`, `2d`, `0s`); the total must fit `i64`
  seconds. Pure helper
  `parse_before(&str) -> Option<chrono::TimeDelta>` in `config.rs`
  (non-negative), unit-tested at the bottom of `config.rs` beside
  `parse_due`'s tests in `cli.rs`.
- **DA6 — mapping accessor**:
  `AppConfig::reminder_engine_config() -> taskboard_state::ReminderEngineConfig`
  — `enabled` passthrough, `tick: Duration::from_secs(tick_secs)`,
  `defaults: ReminderDefaults { default_before:
  ReminderOffset::before(-parsed) }` where `parsed` is the validated
  string (`"0s"` → `ReminderOffset::at_due()`; the config speaks
  "before", the domain speaks "added to duedate" — the negation is
  the whole mapping and is unit-pinned). The `sync_actor_config()`
  shape (§1.8) is the precedent.
- **DA7 — domain rule commands (the chunk-3 half of the verb surface).**
  ```rust
  /// Set (replace) the task's explicit reminder rules with one rule.
  SetReminderRule { task: TaskId, offset: ReminderOffset },
  /// Remove all of the task's explicit reminder rules.
  ClearReminderRules { task: TaskId },
  ```
  `plan_command` for `SetReminderRule`: `live_task` guard (the
  edit-style rejection, like `SnoozeReminder` — snoozing/reminding a
  dead task is a rejection, not a no-op); rule id from
  `ids.new_reminder_rule_id()` — the `IdGenerator` trait
  (`crates/taskboard-domain/src/idgen.rs:15-27`) grows this one
  method (`fn new_reminder_rule_id(&self) -> ReminderRuleId`), the
  `UuidV7Generator` impl stamps `Uuid::now_v7()`, and every fake
  generator (the domain's `CountingIds`, any state-crate fakes found
  by grep) grows the arm — the compiler enumerates them; **replace-all semantics**: actions =
  `DeleteReminderRule(id)` for each of the task's existing rules, then
  `UpsertReminderRule(new)`; outcome
  `CommandOutcome::ReminderSet { rule: ReminderRuleId }`. A rule on a
  task without a duedate is **legal** (occurrence is `None`; it arms
  when a duedate arrives — the due-anchored model makes this free, and
  refusing would force CLI users to order `edit --due` before
  `remind`). `ClearReminderRules`: `live_task` guard; zero existing
  rules → `no_op()` (idempotent repeat, the `SetTaskRemindersMuted`
  precedent); else `DeleteReminderRule` × N with outcome
  `ReminderCleared { removed: usize }` — the CLI receipt's `removed`
  count comes from the **pre-state** the handler already loaded for
  selector resolution, so a `NoOp` repeat still reports `removed: 0`.
  `SetTaskRemindersMuted`'s success outcome changes `Applied` →
  `CommandOutcome::ReminderMuted { task, muted }` — the design §11
  names this outcome; the boolean field carries the `--on` direction.
  The engine constructs outcomes but never matches them (§1.6), so
  this is a representation change only; the engine's reminder-tick
  outcome construction is untouched.
- **DA8 — CLI surface (flag form, exactly the design §11 sketch, plus
  `--on`)**:
  ```rust
  TasksCmd::Remind {
      task: Option<String>,        // uuid | unique ≥8 prefix | exact title
      #[arg(long, requires = "task")] before: Option<String>, // value_parser = parse_before
      #[arg(long, conflicts_with_all = ["task","clear","off","on"])] list: bool,
      #[arg(long, requires = "task", conflicts_with_all = ["before","off","on","list"])] clear: bool,
      #[arg(long, requires = "task", conflicts_with_all = ["before","clear","on","list"])] off: bool,
      #[arg(long, requires = "task", conflicts_with_all = ["before","clear","off","list"])] on: bool,
  }
  ```
  `tasks remind <sel> [--before 10m]` sets/replaces; `--list` lists;
  `--clear` removes; `--off`/`--on` mute/unmute. **Addition over the
  sketch**: `--on` (unmute). Reason: mute persists in the db; without
  an inverse the sketch makes `--off` a one-way door, and the domain
  already carries `SetTaskRemindersMuted { muted: false }` — the
  omission is an oversight, not a designed constraint. **Rejected**:
  sub-subcommand form (`tasks remind set …`) — the dispatch plan
  spells the flag forms verbatim. `--before`'s default is *not* a clap
  default: the handler resolves `before.unwrap_or(config default)`
  from the session/boot config so the config knob stays the single
  source (design §11: "default offset: config default_before").
  A bad `--before` value is a clap usage error (exit 2), the
  `parse_color`/`parse_due` precedent.
- **DA9 — `--list` is a pure read**: no `StateCommand`; the handler
  reads `shared_state()` and projects
  ```rust
  pub struct RemindersList { pub rules: Vec<ReminderRow> }
  pub struct ReminderRow {
      pub id: String,            // rule uuid
      pub task_id: String,
      pub task_title: String,
      pub before: String,        // canonical text ("10m", "0s")
      pub created_at: DateTime<Utc>,
      pub task_muted: bool,      // mutes are visible where rules are
  }
  ```
  Deterministic order: task canonical order, then rule id (the state's
  BTreeMap id order within a task). `before` renders from
  `ReminderOffset::as_duration()` through a canonical formatter
  (`<h>h<m>m<s>s`, zero components omitted, `"0s"` for zero) — one
  formatter shared by rows and receipts; the *config* string stays
  raw (DA5), the *projection* is canonical.
- **DA10 — mutation receipts** (typed, `--json`-scriptable, the
  `MutationReceipt` house shape):
  ```rust
  pub struct ReminderSetReceipt  { rule_id, task_id, task_title, before: String }
  pub struct ReminderClearReceipt { task_id, task_title, removed: usize }
  pub struct ReminderMuteReceipt  { task_id, task_title, muted: bool }
  ```
  Text renderers in the house one-line style
  (`reminder set for "title" 10m before due (rule <uuid>)`,
  `cleared 1 reminder rule from "title"`,
  `reminders muted for "title"` / `… unmuted …`).
- **DA11 — the marker column**: `TaskRow` grows
  `pub due_marker: Option<DueMarker>` with
  `#[serde(rename_all = "kebab-case")] enum DueMarker { Overdue, DueSoon }`
  — serialized as `null` / `"overdue"` / `"due-soon"` (payloads are
  snake_case like every `TaskRow` field; *error kinds* are the
  kebab-case contract, not payloads). Derived at row-build time via
  the domain's `overdue_tasks`/`due_soon` (one `Utc::now()` per list
  render, real clock — the marker is presentation, and the pure fns
  are already proptest-pinned P4); ignores rules, deliveries, snoozes,
  and mute by construction (banner truth). Horizon:
  `pub(crate) const DUE_SOON_HORIZON: chrono::TimeDelta = 24h` — a
  fixed named constant, not config: §5.4 defines no knob and the
  desktop strip (the feature's real consumer) will own its own value;
  a CLI constant is not worth a config surface. Text rendering
  appends ` !` for overdue and ` ~` for due-soon after the due date
  (two glyphs, documented in the verb's help text; no color codes —
  the existing list is plain).
- **DA12 — E2E shape** (details §10): standalone backend (no
  wiremock), real sqlite tempdir, `Arc<dyn NotificationSink>` fake,
  `#[tokio::test(start_paused = true)]`. **The immediate-first-tick
  trap** (§1.2) shapes the design: seeding the past duedate through a
  one-shot verb would fire + deliver *during that verb's own boot*
  (LogSink line), leaving nothing for the E2E boot to observe. So the
  E2E seeds board/stack/task/**rule** (no duedate) via one-shot verbs
  — nothing can fire without a duedate — then sets the duedate
  **inside** the observed boot via `app.engine.execute(UpdateTask …)`,
  and awaits the notification. This also proves the `remind` verb's
  persistence across boots for free.
- **DA13 — backlog final commit**: `docs(backlog): mark reminders
  subsystem done` (the `f4df211` house precedent), editing only the
  origin entry's status note (§12). The four deferred-idea entries
  stay byte-identical.
- **DA14 — headers & purity**: every new app-crate file carries
  `// SPDX-License-Identifier: AGPL-3.0-only`; the domain edits stay
  MIT OR Apache-2.0 and pure; `#![forbid(unsafe_code)]` untouched
  everywhere. **Zero new dependencies** (Cargo.toml untouched).

## 4. Config: `[reminders]` end to end

**Defaults** (figment `Serialized::defaults`, §5.4 verbatim):
`enabled = true`, `tick_secs = 30`, `default_before = "5m"` — as
`DEFAULT_REMINDER_TICK_SECS: u64 = 30` and
`DEFAULT_REMINDER_BEFORE: &str = "5m"` consts beside the sync ones
(config.rs:35-42). `AppConfig` gains `#[serde(default)] pub
reminders: RemindersSection` (+ `Default` impl, the `SyncSection`
pattern).

**Chain behavior to pin (C-series)**:
absent table → defaults; TOML `[reminders]` partial override (figment
merges per-key over `Serialized::defaults` — an absent key keeps the
default); `TASKBOARD_REMINDERS__ENABLED=false`,
`TASKBOARD_REMINDERS__TICK_SECS=5`,
`TASKBOARD_REMINDERS__DEFAULT_BEFORE=1h30m` env overrides (figment's
env coercion: bool/int/string, §1.8); strict table —
`[reminders]\ntoken = 'x'` → decode error (the secret-free
construction rule); `default_before = "5min"` → `validate` →
`InvalidDuration { field: "reminders.default_before" }`, kind
`invalid-duration`; `tick_secs = 0` → `InvalidDuration { field:
"reminders.tick_secs" }`; serde round-trip of defaults; the
`reminder_engine_config()` mapping (enabled passthrough, tick, and
**the sign flip**: `"5m"` → `ReminderOffset` whose
`as_duration().num_seconds() == -300`; `"0s"` → `at_due()`).
`parse_before` unit tests: accepts `0s/30s/5m/1h30m/2d`, rejects
`""`, `-5m`, `5min`, `5 m`, `m5`, `1.5h`, overflow (`999999999d`),
and trailing garbage (`5mX`).

## 5. Domain additions (taskboard-domain, pure)

New test module entries in `command.rs`'s tests (the
`with_rules`/`reminder_rule` helpers already exist there,
command.rs:1967-1985) + `messages.rs`/`command.rs` variants per DA7.
TDD: the plan-level tests first (each standalone-red against the
missing variants):

1. `set_rule_plans_a_new_rule_for_a_live_task` — actions contain
   exactly `[UpsertReminderRule]`, outcome `ReminderSet`, rule's task
   and offset carried, `created_at == now` (the plan stamp).
2. `set_rule_replaces_the_tasks_existing_rules` — pre-seed 2 rules
   (one foreign): actions = `DeleteReminderRule(own1)`,
   `DeleteReminderRule(own2)`, `UpsertReminderRule(new)`; the foreign
   rule untouched.
3. `set_rule_rejects_unknown_and_deleted_tasks` — `UnknownTask` /
   `TaskDeleted` rejections (typed).
4. `set_rule_on_a_task_without_a_duedate_is_legal` (DA7).
5. `clear_rules_plans_deletes_and_reports_the_count` — outcome
   `ReminderCleared { removed: 2 }`, foreign rules untouched.
6. `clear_rules_with_no_rules_is_a_no_op` — `no_op()` outcome, empty
   actions.
7. `clear_rules_rejects_dead_tasks` (edit-style guard).
8. `mute_outcome_carries_the_direction` — `SetTaskRemindersMuted{
   muted: true }` → `ReminderMuted { task, muted: true }`; the
   redundant repeat stays `NoOp` (behavior unchanged from chunk 2,
   only the success outcome's shape changes — assert both).
9. Serde/`TaskChanges`-style n/a (commands aren't serialized); no
   snapshot churn in the domain crate (planner outcomes aren't
   snapshotted; the state engine snapshot tests live in chunk 2's
   suite and are untouched by chunk 3 — `AppState`'s shape does not
   change here).

Engine layer: **no changes** (§1.6). Storage layer: **no changes**
(the persistence actions and repo support landed in chunks 1–2).

## 6. CLI verbs and dispatch

**Module**: new `crates/taskboard-app/src/commands/reminders.rs`
(SRP: the reminder verb family, the `labels.rs` shape), with:

- one-shot wrappers `set/clear/mute/list(config, store, …)` over
  `tasks::with_core` (reuse, not re-wrap), and `*_on(app, …)` session
  seams for `dispatch_line` — the two-lane rule (§1.10).
- `set_on`: load state → `resolve_task` (cascade reuse, §1.11) →
  offset = `--before` parsed value negated, else the **session/boot
  config's** `reminder_engine_config().defaults.default_before`
  (already negated by DA6 — the handler copies the configured
  `ReminderOffset`, so the negation exists in exactly one place) →
  `execute(SetReminderRule)` → reload → receipt (rule looked up by
  the outcome's id; canonical `before` text via the DA9 formatter).
- `clear_on`: resolve → count the task's rules in the pre-state →
  `execute(ClearReminderRules)` → `ReminderClearReceipt { removed:
  pre_count }` (a `NoOp` repeat reports 0, DA7).
- `mute_on(task, muted)`: resolve → `execute(SetTaskRemindersMuted)`
  → `ReminderMuteReceipt` (post-state membership check for the
  boolean — the outcome also carries it; read state, trust either,
  assert in tests they agree).
- `list_on`: pure read (DA9).
- Renderers `render_text_list/set/clear/mute` in the house style.

**Dispatch arms** (commands/mod.rs): `Command::Tasks { cmd:
TasksCmd::Remind { … } }` in `dispatch` (via `run_core` with the
right render fn per mode — one `remind_command` helper switches on
the mode flags, keeping `dispatch`'s match flat) **and** in
`tasks_line` for `dispatch_line` (the REPL/IPC lane; `route_for_ipc`
needs no change, §1.12). `--json` rides the existing
`emit_result`/`emit_to` path; failures flow `CommandError` →
kebab-case envelope (no new CLI error variants: unknown task →
`unknown-task`, dead task → `unknown-task`, engine storage failure →
`engine`; a bad `--before` never reaches a handler — clap exit 2).
**completion.rs**: add `remind` to the tasks subverb catalogue and
the flag-stem completions (`--before|--list|--clear|--off|--on`).

## 7. `tasks list` marker column

`task_row(state, task, now)` (output.rs:329) grows the `now`
parameter and sets `due_marker` by membership in the
domain-computed id sets (compute `overdue_tasks`/`due_soon` once per
`stack_groups` call — O(n), not per row). `render_text_list`
(tasks.rs:540) appends the glyphs (DA11). Callers thread one
`Utc::now()` through `list_on`. JSON: `due_marker` always present
(`null` when neither) — `TaskRow` never skips fields. Existing list
tests update mechanically (typed field, no text assertions on
unchanged rows: rows without duedates assert `None`).

## 8. Bootstrap wiring

```text
bootstrap(config, credentials, notifier_sink: Arc<dyn NotificationSink>)
  …
  let (notifier, notifications) = mpsc::channel::<Notification>(NOTIFIER_CAPACITY);
  let notifier_join = spawn_notifier(notifications, notifier_sink);   // before the engine
  spawn_state_engine(…, notifier, config.reminder_engine_config())    // was ::disabled()
```

`App` grows `notifier_join: JoinHandle<()>`; `shutdown` joins it
bounded after the engine join (DA4). `BootstrapError` unchanged. The
doc-test (bootstrap.rs:15-54) gains the fake-sink parameter; existing
callers (`main.rs`, `tests/bootstrap.rs`, verb helpers) pass
`Arc::new(LogSink)` except the E2E (fake). `main.rs`'s daemon arm
needs no new spawn — the consumer is inside `bootstrap`, so the
daemon, the one-shot verbs, and the REPL all notify identically (the
one-shot verbs' boots run the immediate first tick: a past-due
undelivered reminder logs one line per invocation — designed
behavior, §1.2, and the LogSink's whole v1 point).

## 9. REPL / IPC

`tasks_line`'s `TasksCmd::Remind` arm routes to the `*_on` seams with
the **session** config for the `--before` default (DA8). IPC frames:
the codec serializes `Command` — new variant rides along; the
`IpcVersion` constant does **not** bump (daemon and CLI ship the same
binary; a mixed-version pair already refuses via the version check).
Completion per §6.

## 10. Test catalogue (TDD order; hermetic only)

**Layer rule** (AGENTS §5): domain planner semantics → domain unit
tests (§5); pure config parsing/mapping → config.rs unit tests + C-series
(§4); verb routing/projection/receipts → R-series; the wiring → one
E2E family. No upper layers re-test planner semantics.

**R-series** (`tests/cli_reminders.rs`, standalone configs, no
wiremock — `common::valid_config` minus the server, or hand-built):
- R1 `remind` sets one rule (receipt fields; post-state `rules` has
  exactly one entry for the task; `--json` shape via the handler
  return value, as cli_verbs asserts receipts).
- R2 second `remind --before 1h` replaces (still one rule, new
  offset; `ReminderSet` new id).
- R3 `--before` default comes from config: config
  `default_before = "2m"` → rule offset −2 m (assert via
  `reminder_engine_config()`-shaped post-state read).
- R4 `--list` empty → `rules: []`; after R1 → one row (title,
  canonical `before`, `task_muted: false`).
- R5 `--clear` → `removed: 1`; repeat → `removed: 0`, exit 0.
- R6 `--off` → task in `state.reminders.muted`; `--on` → out. A
  repeat `--off` is still exit 0 (NoOp).
- R7 unknown selector → `error_kind() == "unknown-task"`; ambiguous
  title → `ambiguous-task` (cascade reuse proven by routing, not
  re-tested per branch).
- R8 clap level: `Cli::try_parse_from(["taskboard","tasks","remind",
  "--before","5min","t"])` errs (exit-2 class);
  `["…","remind","--list","t"]` errs (conflict); `["…","remind"]`
  errs (a mode is required — `task` or a flag).
- R9 marker column: overdue task (due = now − 1 h) → `Overdue`;
  due-soon (due = now + 2 h) → `DueSoon`; far future → `None`;
  no duedate → `None`; **muted + overdue still `Overdue`** (banner
  truth); done task → `None`.
- R10 REPL lane: `dispatch_line` with a `Remind` command over a
  booted session core returns 0 and mutates (routing-only, one case).

**E2E family** (`tests/cli_reminders.rs`, DA12):
- E1 **rule → tick → delivery → notification**: seed board/stack/
  task/rule via one-shot verbs (task has **no** duedate → nothing can
  fire during seeding boots); then
  `#[tokio::test(start_paused = true)]`: `bootstrap(config, fake
  store, Arc::new(RecordingSink))` (a `Vec<Notification>` behind a
  `Mutex`, the fakes-over-mocks rule; assert the *set*, not order);
  `app.engine.execute(UpdateTask { duedate: Some(now − 10 m) })`;
  await the predicate `sink.len() == 1` under
  `tokio::time::timeout(30 s, …)` polling with short `tokio` sleeps —
  under `start_paused` the auto-advance jumps logical time to the
  armed tick, so the wait is deterministic and instant (AGENTS §5:
  no wall-clock, no fixed sleeps — the deadline is the bounded
  escape, the poll is the mechanism). Assert the payload structurally
  (`ReminderRef::Rule(rule_id_from_receipt)`, task id, title,
  occurrence == duedate + offset). Assert terminal state:
  `shared_state().reminders.deliveries` contains the
  `(reminder, occurrence)` row. `app.shutdown().await`.
- E2 **at-most-once + restart suppression**: continue (or reboot a
  fresh `bootstrap` on the same db): advance several ticks → sink
  still holds exactly 1; the fresh boot's immediate first tick adds
  nothing (delivery persisted). Both assertions in one test to keep
  the "fresh boot" honest (shutdown, re-bootstrap, await quiet under
  a short timeout).
- E3 **disabled config**: `enabled = false` → no tick ever arms; the
  same setup yields an empty sink after the same bounded wait, and
  `shutdown` is clean (the config gate proven at app level — the
  engine-level auto-advance guard is chunk 2's, not re-tested).
- E4 **default reminder path** (cheap variant of E1): no explicit
  rule; the implicit `ReminderRef::Default(task)` fires at
  `duedate − default_before` (config `default_before = "0s"` for an
  exact occurrence assertion) — pins the DA6 mapping end to end.

**Bootstrap suite**: existing `tests/bootstrap.rs` cases pass
`Arc::new(LogSink)`; one new case pins the consumer's clean exit
(shutdown returns even with notifications in flight — the join
ordering, DA4).

## 11. Implementation & commit order (each standalone, gates green)

1. `feat(domain): set and clear reminder rule commands` — DA7
   variants + `IdGenerator::new_reminder_rule_id` + planner + outcome
   change + §5 tests.
2. `feat(app): reminders config table with engine mapping` —
   RemindersSection, consts, `parse_before`, validation,
   `reminder_engine_config()`, §4 unit tests (config compiles without
   the bootstrap change — the mapping is additive).
3. `feat(app): notification sink port with log sink consumer` —
   notifications.rs (DA3), `spawn_notifier`, `NOTIFIER_CAPACITY`;
   bootstrap signature + wiring + `App::shutdown` join + doc-test +
   caller updates (mechanical `Arc::new(LogSink)`); bootstrap tests
   green (engine still receives `ReminderEngineConfig::disabled()`?
   **No** — commit 3 switches to `config.reminder_engine_config()`
   together with the wiring; the interim `_notifications` drop and
   `disabled()` are removed in this commit so no commit ships a
   half-wired channel).
4. `feat(app): tasks remind verb family` — cli.rs variant + parse
   wiring, commands/reminders.rs, dispatch arms (both lanes),
   completion, output projections/receipts; R1–R8, R10.
5. `feat(app): tasks list due marker column` — `TaskRow.due_marker`,
   `task_row(…, now)`, render glyphs; R9.
6. `test(app): hermetic reminder notification e2e` — E1–E4 + the
   bootstrap consumer case (tests-only commit; the fake sink rides
   the DA2 seam).
7. `docs(backlog): mark reminders subsystem done` — §12.

Gates after every commit (AGENTS §10): fmt, clippy `-D warnings`,
nextest, doc-tests, deny, llvm-cov. No sqlx changes → no
`scripts/sqlx-prepare.sh` run needed.

## 12. The backlog ✅ DONE commit (exact edit)

In `.artifacts/taskboard-potential-improvements-backlog.md`, replace
the origin entry's status note (lines ~443-451, the blockquote) with:

```markdown
> **Status**: ✅ DONE (implemented 2026-10-14..2026-10-15, WI-I plans
> 1–4: `reminders.rs` domain model with proptests, engine tick arm +
> snooze/mute commands, sqlite migration 0004 + repo, app wiring with
> `[reminders]` config, `LogSink`, `tasks remind` verbs, and the
> overdue/due-soon marker — design
> `.artifacts/plans/2026-10-12-reminders-design-plan.md`, ADR 0013).
> Plan 5 (desktop toasts + interactive snooze) remains future work.
```

The four deferred-idea entries (reminder sync, insistence levels,
multi-reminder UX, delivery pruning) are untouched.

## 13. Risks & mitigations

- **The immediate-first-tick trap** (any boot with an undelivered
  past-due firing notifies *in that boot*, §1.2): mitigated by the
  E2E seeding design (DA12 — the duedate lands inside the observed
  boot) and by awareness in R-series setup (seed duedates future or
  absent). The behavior itself is designed (a CLI boot logging an
  overdue line is the LogSink's v1 contract).
- **Existing paused-time suites vs. an enabled arm**: app-level tests
  boot real configs with `enabled = true` by default now. The verb
  suites are plain `#[tokio::test]` (no pause → no auto-advance
  interaction); the E2E alone opts into `start_paused`, where tick
  wakes are the point. The engine crate's default-off helper policy
  is chunk 2's, untouched here. One watch item: any *other* app test
  that would start paused and boot a core — none exist today
  (verified: `start_paused` appears only in state/sync crates).
- **Bootstrap signature break** is intentionally wide (every caller):
  contained by `Arc::new(LogSink)` being a one-token fix, the
  compiler enumerating every site, and commit 3 doing the swap
  atomically.
- **Send-side backpressure**: a full notifier channel (capacity 64)
  blocks the engine loop until the consumer drains. `LogSink`
  delivers in microseconds — unreachable in v1; flagged for plan 5's
  real sink (spawn-per-delivery or a bounded-drop policy — its
  design round, not this chunk's).
- **Marker column churn in `--json` consumers**: `due_marker` is
  additive (`null` for the common no-due case); the payload contract
  is explicitly internal-but-scriptable (output.rs:7-10), and the
  house accepts additive fields (labels, standalone footer).
- **`ReminderMuted` outcome change** touches a chunk-2-landed
  behavior: covered by test 8 (§5) asserting both the new shape and
  the unchanged NoOp repeat; the engine never matches outcomes
  exhaustively (§1.6), so the blast radius is the domain crate's own
  tests plus the state-crate snapshot suite — reminder tick snapshots
  (if any assert mute outcomes) update in the same commit; grep
  `ReminderMuted\|SetTaskRemindersMuted` in state tests before
  committing.

## 14. Verification & size estimate

- **Diff estimate**: ~1,450–1,750 added / ~120 modified / 0 deleted
  lines across ~18 files — domain `messages.rs`/`command.rs`
  (+~260 incl. tests), app `config.rs` (+~230 incl. tests),
  `notifications.rs` (new, ~180), `bootstrap.rs` (~90 touched),
  `cli.rs` (+~110), `commands/reminders.rs` (new, ~330),
  `commands/mod.rs` (+~90), `commands/output.rs` (+~120),
  `commands/tasks.rs` (~60 touched), `completion.rs` (~30),
  `tests/cli_reminders.rs` (new, ~380), `tests/config.rs` (+~120),
  `tests/bootstrap.rs` + doc-test (~40), backlog (~12). Seven
  commits (§11). No Cargo.toml, no migration, no engine/storage
  source changes (test-only touch possible in state crate if mute
  outcomes are snapshot-asserted).
- **Self-check against the dispatch acceptance** (§4): hermetic E2E
  proves rule → tick → delivery → notification on the fake sink ✔
  (E1); config absent = defaults ✔ (C-series + §4);
  `TASKBOARD_REMINDERS_*` overrides work ✔ (C-series); gates green
  ✔ (per-commit gate runs); backlog entry ✅ DONE ✔ (§12).

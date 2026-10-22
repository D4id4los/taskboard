# [Plan] Daily-Driver Chunk 6 — N-Reminder Verbs + Surfaces

- **Date**: 2026-10-22.
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1/§7 (MD-1…MD-10, the code-quality mandate); wave implementation
  plan `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-13 (the verb family) + DDF-8 (the enqueue sites the verbs
  become), §3 chunk 6, §4–§6; reminders-surfaces design doc
  `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md`
  Part A §3.1 (D-R1), §3.2 (D-R2), §3.3 (D-R3), §3.4 (D-R4), §4
  (ACCEPTED record incl. the retirement rationale), §5 (test-strategy
  hooks), §7 (chunk mapping). All ACCEPTED rulings are binding (MD-7);
  every decision below stays inside them — the interpretation calls
  are flagged in §7.
- **Branch / worktree**: `feat/state-reminder-rule-verbs` in
  `../taskboard-WT-feat-state-reminder-rule-verbs`, off `origin/main`
  @ `e5ae7bf`. **MD-9 verified 2026-10-22**: chunk 4
  (`feat/sync-appdata-fold`) is merged — its implementation commits
  (`742c7e4`…`a721e3b`, plan `c5e98bb`) are ancestors of `origin/main`;
  chunk 5 has also merged (its impl commits `6143327`…`aeec656` plus
  changelog `e5ae7bf` = current `origin/main` HEAD), so this branch
  starts on the then-current `main` with no stacking (MD-2).
  `scripts/worktree-doctor.sh` exited 0 (91 G free, floor 25 G).
- **Goal**: the reminder-rule verb family grows from replace-with-one
  to N-rule add/remove: `SetReminderRule` retires in one motion,
  `AddReminderRule` (dedup by offset) and `RemoveReminderRule` (typed
  unknown-id) take its place, `ClearReminderRules` stays and becomes
  "remove N". The CLI grows the `--remove` selector mode, the UI task
  editor grows the rule-list composition, and the whole family keeps
  riding the chunk-4 sidecar lane unchanged. Closes #204.

## 1. Verified baseline (symbols re-located at this plan's start; lines are plan-day references)

Domain (`crates/taskboard-domain`):

- `messages.rs:214-219` — `StateCommand::SetReminderRule { task,
  offset }` (the replace-with-one variant; retires).
- `command.rs:1117-1141` — the `SetReminderRule` handler arm:
  `live_task` → `task_rule_deletes` (replace-all) → mint
  (`ids.new_reminder_rule_id()`, `created_at`/`updated_at` = `now`) →
  `UpsertReminderRule` → `enqueue(LocalOp::UpdateAppData(task))` →
  `CommandOutcome::ReminderSet { rule }`.
- `command.rs:1142-1157` — the `ClearReminderRules` handler:
  `live_task` → `task_rule_deletes` → NoOp-on-empty → one
  `UpdateAppData` enqueue → `ReminderCleared { removed }` (stays as
  is).
- `command.rs:264-270` — `task_rule_deletes` (the delete cascade
  helper, shared); `command.rs:1165` — `enqueue`; `command.rs:1283` —
  `no_op()`.
- `command.rs:57-107` — `CommandError::UnknownReminderRule(ReminderRuleId)`
  at `:102` (used by the `SnoozeReminder` arm at `:1075-1079`; reused
  by Remove).
- `command.rs:111-171` — `CommandOutcome::ReminderSet` at `:122`
  (retires), `ReminderCleared` at `:127` (stays).
- `command.rs:4236-4432` — the planner test block for
  set/clear (rewrites to the new verbs).
- `reminders.rs:109-125` — `ReminderRule { id, task, offset,
  created_at, updated_at }` (the D-S9 write clock already in place);
  `reminders.rs:29-66` — `ReminderOffset` newtype
  (`before()`/`at_due()`/`as_duration()`, ≤ 0 by construction);
  `reminders.rs:320-322` — `occurrence_of` (pure in
  `(offset, duedate)` — the dedup soundness fact); `reminders.rs:381-418`
  — `due_firings` with the derived rules-replace-default at
  `:392-402`; `reminders.rs:72-105` — `ReminderRef::text_form`/
  `parse_text_form` (the domain text-form precedent D5 mirrors).
- `persistence.rs:204-207` — `PersistenceAction::UpsertReminderRule`/
  `DeleteReminderRule`; `persistence.rs:295-305` — the
  `apply_actions` cascade (delete prunes the rule's deliveries and
  snooze links) — reused verbatim.
- `appdata.rs:621-643` — `rules_entry_for`; `appdata.rs:645-670` —
  `derive_rule_tombstones` (every base-present live rule missing
  locally tombstones at the removal clock): **the tombstone lane is
  already general — the new verbs need no flush code**.

State (`crates/taskboard-state`):

- `actor.rs:595-597` — the engine matches `CommandOutcome` only on
  `SyncRequested`/`NoOp` (the request-sync short-circuit): new
  outcome variants break nothing.
- `tests/snapshots.rs:742-835` — `s10_boards_reset` feeds
  `SetReminderRule` at `:773` (converts; see D14's no-diff proof).
- `tests/reminders.rs` (r1-r9) seeds `PersistedState` directly — no
  `SetReminderRule` references; untouched.

Sync (`crates/taskboard-sync-nextcloud`):

- `tests/it_docker.rs:762-764` — the tier-2 convergence site feeds
  `SetReminderRule` and matches `ReminderSet` (converts; D14).

App (`crates/taskboard-app`):

- `commands/reminders.rs` — the whole module: `format_before:27`
  (the canonical `<h>h<m>m<s>s` text form), `set:50`/`set_on:111`
  (the `SetReminderRule` mirror), `clear_on:151`, `mute_on:180`,
  `list_on:209`/`reminder_rows:218`, `live_title:246`,
  `render_text_*:257-305`, `RemindInvocation:310`
  (`List | Clear | Mute | Set`), `collapse:336` (the one-mode
  collapse, Set is the fallback arm), `remind_command:368` (the
  one-shot lane), `remind_line:416` (the session lane).
- `commands/output.rs:444-502` — `ReminderRow`/`RemindersList`
  (unchanged), `ReminderSetReceipt:471` (renames per D6),
  `ReminderClearReceipt:484`/`ReminderMuteReceipt:494` (stay);
  `ErrorEnvelope`/`ErrorBody` at `:696-710` (the ADR 0009 error
  shape — untouched).
- `commands/mod.rs` — `CommandError:115` (the app-level enum; grows
  the selector-failure variants, D7), `error_kind:365`,
  `error_detail:416`, `domain_error_kind:459` (already maps the
  engine's `UnknownReminderRule` → `"unknown-reminder-rule"` at
  `:468`), the dispatch arms at `:955` (one-shot) and `:1592` (the
  `dispatch_line` session lane the REPL routes through),
  `emit_result:1013`/`run_core:1030`.
- `commands/tasks.rs:512-554` — `resolve_task` (the selector-cascade
  precedent: exact uuid → unique ≥8 prefix → exact title, typed
  ambiguity carrying candidates, uuid-shaped misses fall through).
- `cli.rs:414-417` — `parse_before_flag` (the config grammar as a
  clap value parser); `cli.rs:547-573` — the `TasksCmd::Remind`
  flag-coherence group (`#[group(required = true, multiple = true)]`,
  one mode per line; `--before`/`--list`/`--clear`/`--off`/`--on`
  with mutual conflicts).
- `config.rs:539-568` — `parse_before` (the grammar's current one
  definition site: `<unsigned-int><unit>` components, unit ∈
  `s|m|h|d`, total must fit a `TimeDelta`).
- `repl.rs` — no remind-specific code: lines parse through clap and
  dispatch via `dispatch_line` (`:751`), landing in the shared
  `TasksCmd::Remind` arm (`mod.rs:1592`). **Finding recorded in D10:
  repl.rs needs no change.**
- `completion.rs` — the spec is walked from the clap tree at runtime
  (`flag_value_kind:259` maps flag names to value kinds; unknown
  flags default to `Kind::None`); the tests pin `tasks remind`
  positionals as `[Kind::Tasks]` and "then flags only"
  (`:1189-1191`). **Finding recorded in D10: completion.rs needs no
  change.**
- `tests/cli_reminders.rs` — the receipt/`--json` fixture suite:
  `r1_remind_sets_one_rule:74`, `r2_second_remind_replaces:109`
  (pins today's replace semantics — the semantic-change test),
  `r3_before_default_comes_from_config:141`, `r4_list:158`,
  `r5_clear:177`, `r6_mute:206`, `r7_selector_errors:231`,
  `r8_clap_mode_and_value_rules:251`, `r10_session_lane:290`, plus
  the e2e block (`e1_rule_fires_and_delivers_end_to_end:584`, a
  recording-sink delivery proof). No insta corpus in this crate —
  the "receipt fixtures" are typed assertions (D14).

UI (`crates/taskboard-ui-slint`):

- `actions.rs:148-181` — `ActionId` (32 variants; the design doc's
  "71" was plan-day), `id():186-221` (the dotted names), the
  `REGISTRY:296-622` const table (row = id/label/contexts/category/
  enabled/menu_rank/description/menu_group), the coverage tests at
  `:649-688` (the `ALL` list must grow with the enum).
- `router.rs:104-115` — `DispatchCtx { focused_task, focused_stack,
  reminder_target, board_target }` (the one-shot target precedent);
  `router.rs:124-199` — the exhaustive dispatch table (banner verbs
  at `:156-170` are the parameterized-command precedent);
  `DispatchOutcome::Command(StateCommand)` is the existing route to
  the engine.
- `adapter.rs:83-88` — the one-shot fields (`reminder_target`,
  `board_target`) consumed per route at `:214-219`; the banner seams
  `:1211-1254` (set target → `route(ActionId)`); the task seam
  `install_task_seam:1182-1195`.
- `models.rs:36-53` — `IdTables` (board/stack/banner index tables —
  the composition's rows get one); `push_task_detail:311-342`
  builds `TaskDetailModel` from the subject task.
- `presentation.rs:186-231` — `ReminderTarget`/`snooze_target` (the
  banner's first-rule-or-default; **untouched per D-R4**).
- `ui/views/task-view.slint` — the task-detail view (title/stack/
  due/state/description; the editor surface D-R4 extends, "not a new
  view family"); `ui/compositions/` — the component home for the
  rule list; `ui/widgets/icon-button.slint:14-22` — `IconKind`
  (`menu, snooze, mute, warn, plus, chevron-left`; grows a remove
  glyph).
- `tests/keymap_snapshots.rs:49` — the registry insta dump (+ the
  menu-group dumps); `tests/journeys.rs` — the SlintDriver harness
  (`assert_commands:154`); `tests/usability_components.rs` — the
  component harness. `tests/goldens/` covers board views only — the
  task view has no goldens (no golden regen).

IPC: `crates/taskboard-app/src/ipc/protocol.rs` carries no
`StateCommand` (external sessions dispatch text lines through
`dispatch_line`) — the verb retirement has **no wire-format
consequence**.

## 2. Shape of the chunk

After this chunk, one task's rule surface is:

- **Add** (`tasks remind <task> [--before dur]`, the REPL same, the
  UI add affordance): the task's rules grow by one — a fresh rule at
  the requested offset — unless a rule with that offset already
  exists, in which case nothing is planned and the receipt names the
  survivor with `added: false`.
- **Remove one** (`--remove <selector>` / the row affordance): the
  named rule deletes (cascade: deliveries + snooze links), one op
  triggers the flush, the flush derives the document tombstone.
- **Remove all** (`--clear <task>`, unchanged handler): N deletes,
  one enqueue, N tombstones at flush.
- **First-remind flow identical**: a rule-less task's first add
  mints exactly what today's set path mints (same actions, same
  state; only the receipt differs).
- **The banner and `snooze_target` are byte-identical** (D-R4);
  firings, deliveries, snoozes, mute, the evaluator — untouched.
- **Sync**: unchanged lane; the verbs are exactly the enqueue sites
  D-R2 enumerates (`AddReminderRule` stamps on create via the mint's
  `updated_at = now`; removes/clear emit their tombstones at flush
  through `derive_rule_tombstones`).

## 3. Decisions (every open question settled here)

### D1 — the retirement motion's exact removal set (one motion, compile-enforced)

`SetReminderRule` disappears in a single commit that touches every
construction site; nothing aliases it, nothing reinterprets it. The
complete removal set (workspace grep at plan time — stage 2 re-runs
it):

1. `messages.rs` — the `StateCommand::SetReminderRule` variant +
   its doc comment.
2. `command.rs` — the handler arm (`:1117-1141`);
   `CommandOutcome::ReminderSet` (`:122-125`); the planner test
   block's set-path tests (`:4236-4432` region, rewritten per §5).
3. `commands/reminders.rs` — `set_on`'s `execute(SetReminderRule)`
   + the `ReminderSet` outcome match (becomes `add_on`, D6).
4. `commands/output.rs` — `ReminderSetReceipt` (renames, D6).
5. `tests/cli_reminders.rs` — the r-suite sites (rewritten per §5).
6. `taskboard-state/tests/snapshots.rs:773` — s10's feed (→
   `AddReminderRule`, D14).
7. `taskboard-sync-nextcloud/tests/it_docker.rs:762-764` — the
   tier-2 convergence site (→ the new verb + outcome, D14).

The one-motion commit is deliberately larger than a typical commit
(~500 lines with the consumer switches): the ruling "retired, not
aliased — in one motion" means no intermediate state where both
verbs are live surface. The two additive commits before it (D5, D2)
keep every commit standalone-buildable.

Exclusions (checked, stay): `CommandError::UnknownReminderRule`
(still used by Snooze), `ReminderCleared`/`clear_on`/`render_text_clear`
/`ReminderClearReceipt` (Clear stays), the persistence arms (shared),
`derive_rule_tombstones` (shared).

### D2 — `AddReminderRule` planner mechanics

```rust
AddReminderRule { task: TaskId, offset: ReminderOffset }
```

(`messages.rs`, doc comment: adds a rule unless one at this offset
exists; legal without a duedate — occurrence arithmetic is
due-anchored, the retired variant's paragraph carried over.)

Planner order (design-mandated):

1. `live_task(app, *task)?` — the edit-style intent check, before
   dedup (reminding a dead task rejects even if the offset would
   "dedup").
2. Dedup: scan `app.reminders.rules.values()` for
   `rule.task == task && rule.offset == *offset` — `BTreeMap`
   iteration is id-ordered, so the hit is deterministic; `(task,
   offset)` fully determines firing semantics
   (`occurrence_of` is pure in `(offset, duedate)`), which is what
   makes both the dedup and the D9 offset selector sound.
   - **Hit**: `Ok(CommandPlan { actions: vec![], outcome:
     ReminderRuleAdded { rule: survivor.id, added: false } })` —
     zero actions, nothing persisted, the receipt names the
     survivor. Not the literal `CommandOutcome::NoOp` (that variant
     carries nothing); the *shape* is NoOp, the *receipt* is
     informative.
   - **Miss**: mint `ReminderRule { id:
     ids.new_reminder_rule_id(), task, offset: *offset, created_at:
     now, updated_at: now }`, actions = `[UpsertReminderRule(rule),
     enqueue(LocalOp::UpdateAppData(task))]`, outcome
     `ReminderRuleAdded { rule: rule.id, added: true }` — exactly
     today's set path minus the delete-all. On a rule-less task the
     action lists are identical (the deletes were empty): the
     first-remind-identical claim, test-pinned at three layers (§5).

`CommandOutcome` grows:

```rust
ReminderRuleAdded { rule: ReminderRuleId, added: bool }
ReminderRuleRemoved { rule: ReminderRuleId }
```

The outbox never influences the dedup (plan_command's standing
contract: rejections and no-ops are identical for any queue
contents). Two rules at one offset remain unrepresentable through
the verb surface; the document lane can still *carry* a duplicate
(foreign/legacy data) — the dedup plans by equality, so a second
add then hits the first-found survivor and names it; no
dedupe-the-map pass is added (no sanctioned write produces the
state; the evaluator handles duplicates observably as two firings —
the exact condition the verb now prevents going forward).

### D3 — `RemoveReminderRule` planner mechanics

```rust
RemoveReminderRule { rule: ReminderRuleId }
```

Planner: look the rule up in `app.reminders.rules` — miss ⇒
`Err(CommandError::UnknownReminderRule(rule))` (the existing typed
variant). Hit ⇒ actions = `[DeleteReminderRule(rule.id),
enqueue(LocalOp::UpdateAppData(rule.task))]` (one op for one
removal), outcome `ReminderRuleRemoved { rule: rule.id }`. The
`apply_actions` cascade (`persistence.rs:298-305`) prunes the
rule's deliveries and snooze links; the flush's
`derive_rule_tombstones` turns the local absence into the document
tombstone stamped with the op's removal instant. **No new flush,
outbox, or codec code exists in this chunk** — the verbs are the
enqueue sites, the machinery is chunk 4's.

Recorded interpretation: **no `live_task` check on Remove.** The
command targets the *rule*, not a task; `DeleteTask`'s cascade
(`task_rule_deletes`) makes a rule of a dead task unreachable, so a
live_task check would be dead code guarding an uninhabitable state.
Existence-only, like the Snooze arm's lookup. (If stage 2 finds a
reachable contrary state, STOP and report — that would be a
chunk-4 bug, not something to paper over here.)

### D4 — `ClearReminderRules` stays, verbatim

The handler already removes N (`task_rule_deletes` collects every
rule), NoOps on empty, emits one enqueue, and reports
`ReminderCleared { removed }`. The N-tombstone behavior is already
proven by chunk 4 (`derive_rule_tombstones` tombstones every
base-present rule missing locally). The only clear-path work in
this chunk is its test suite gaining an explicit N ≥ 2 tombstone
assertion at the planner layer (the dispatch plan's trap: "the
tombstone path missed for ClearReminderRules").

### D5 — the offset text form moves into the domain (interpretation call, recorded)

D-R3's selector cascade needs the offset *text form* parsed in two
new places: the CLI (`--remove <offset-text>`) and the UI's custom
add entry. The grammar (`<unsigned-int><unit>`, `s|m|h|d`) lives in
app `config.rs::parse_before` — unreachable from `taskboard-ui-slint`
(which depends on domain only), and duplicating it in presentation
would be exactly the second-grammar finding chunk 5's traps forbid.

Decision: domain `reminders.rs` grows the canonical text-form pair,
mirroring the `ReminderRef` precedent one module over:

- `ReminderOffset::text_form(self) -> String` — the canonical
  `<h>h<m>m<s>s` form (the current `format_before` body, moved;
  zero renders `"0s"`).
- `ReminderOffset::parse_text_form(raw: &str) -> Option<Self>` —
  the `parse_before` grammar, returning the offset (the
  "before-due" sign convention absorbed: `"10m"` → −600 s, `"0s"`
  → `at_due()`).

App delegation, one grammar:

- `config.rs::parse_before(raw)` keeps its
  `Option<TimeDelta>` signature and delegates:
  `ReminderOffset::parse_text_form(raw).map(|o| -o.as_duration())`
  (its config-validation and default-resolution call sites are
  unchanged).
- `cli.rs::parse_before_flag` unchanged (still the clap value
  parser over `config::parse_before`).
- `commands/reminders.rs::format_before` is **deleted**; its three
  call sites (`reminder_rows`, the add receipt, the remove receipt)
  use `.text_form()` directly.

Contract notes, documented on the methods: the text form is defined
on whole seconds (sub-second offsets — constructible only outside
the grammar/config/document surfaces — truncate; the roundtrip
property is pinned over grammar-reachable magnitudes). Canonical
forms are content, not input-echoes: `"2d"` round-trips as
`"48h"` (today's `format_before` already renders this way).

This adds `reminders.rs` to the chunk's file list beyond the
impl-plan's named two (`command.rs`/`messages.rs`). Recorded reason:
it implements D-R4's "custom `--before` grammar" affordance without
a second grammar, across the one dependency seam that allows it
(ui → domain). Not a deviation from any ACCEPTED ruling — D-R1's
vocabulary and D-R3's grammar are unchanged; only the definition
site moves.

### D6 — the app verb family: renames and receipt shapes

`commands/reminders.rs` (the two-lane shape is the point of the
`*_on` seams — everything extends mechanically):

- `set` → `add`, `set_on` → `add_on` (the lying name goes with the
  retired verb; the module doc header's "set/replace" wording goes
  too). `add_on` resolves the task, resolves the offset (explicit
  `--before` negated into `ReminderOffset::before`, else the booted
  config's `default_before`), executes `AddReminderRule`, and
  projects the receipt from post-state exactly as today — the
  outcome match becomes
  `CommandOutcome::ReminderRuleAdded { rule, added }`, and
  `added: false` still names the survivor, so the projection is
  identical code for both paths.
- New `remove` (one-shot wrapper) + `remove_on` (session seam):
  resolve the task (`resolve_task`), resolve the rule selector
  against the task's rules (D9), execute `RemoveReminderRule`,
  project the receipt from the **pre-execute** rule row (the rule
  is gone post-execute — the `TaskDeleteReceipt` pre-state
  precedent); the task title projects from post-state via
  `live_title`.
- `RemindInvocation::Set` → `Add`; grows
  `Remove { task: String, rule: String }`. `collapse`'s arm order:
  `list` → `clear` → `remove` → `off/on` → the `Add` fallback
  (bare `remind <task>` stays add-with-default; clap's conflict
  table makes the ordering unambiguous).

Receipts (`output.rs`, plain serde field names — the
`ReminderSetReceipt` precedent; the ADR 0009 kebab-case contract
governs the *error envelope*, which keeps its shape):

```rust
pub struct ReminderAddReceipt {      // renames ReminderSetReceipt
    pub rule_id: String,
    pub task_id: String,
    pub task_title: String,
    pub before: String,              // canonical text form
    pub added: bool,                 // false = dedup hit, names survivor
}
pub struct ReminderRemoveReceipt {
    pub rule_id: String,
    pub task_id: String,
    pub task_title: String,
    pub before: String,              // the removed rule's offset
}
```

Render texts: `render_text_set` → `render_text_add` (renders
"already set" from `added` without diffing state — the design's
wording); new `render_text_remove`. Clear/mute renders unchanged.
Both dispatch lanes (`remind_command`, `remind_line`) grow the
`Remove` arm; everything routes through the existing
`emit_result`/`run_core` machinery, so `--json` envelope behavior
is inherited, not re-implemented.

### D7 — the CLI selector-failure variants (typed, app-layer)

`commands/mod.rs::CommandError` grows the `UnknownTask` shape's
siblings:

```rust
UnknownReminderRule(String),                   // selector, engine sense untouched
AmbiguousReminderRule { candidates: Vec<String> },
```

- `error_kind()`: `"unknown-reminder-rule"` (the same kebab string
  the engine-path mapping already uses — same concept, honest
  convergence; a concurrent-removal race surfaces the engine's
  variant through `domain_error_kind` with the identical kind) and
  `"ambiguous-reminder-rule"`.
- `error_detail()`: the selector string / the candidates joined
  `", "` (the `AmbiguousTask` precedent).
- Candidates are **full rule uuids** in deterministic id order —
  for a prefix collision the full ids are the actionable answer
  (offsets would also work post-dedup, but ids are unambiguous even
  for foreign/legacy duplicate-offset state, D2).

The engine never does text matching: resolution is app-layer over
published state, like `resolve_task`.

### D8 — the CLI group grows `--remove` (one mode per line preserved)

`cli.rs::TasksCmd::Remind` gains:

```rust
/// Remove one of the task's rules (rule uuid | unique ≥8-char
/// uuid prefix | the offset text form, e.g. 30m).
#[arg(long, requires = "task",
      conflicts_with_all = ["before", "clear", "off", "on", "list"])]
remove: Option<String>,
```

The verb doc comment drops "set/replace" for "add"; the `--before`
help loses nothing (it *is* add). `collapse` maps `remove` ahead of
the `Add` fallback (D6). Flag-coherence tests extend `r8`
(add-only `--remove --before` refusal, `--remove` without task
refusal, the new mode alone parses).

### D9 — the rule selector cascade (resolve_rule) and its typed failures

`commands/reminders.rs` grows `resolve_rule(state, task_id,
selector) -> Result<ReminderRuleId, CommandError>`, scoped to the
target task's rules (D-R3: "resolved against the task's rules") —
a prefix hitting only *another* task's rule is a miss, not a
candidate:

1. **Exact uuid**: parse; if it names one of the task's rules →
   Ok. (First, so scripts keep working — the `resolve_task`
   precedent.)
2. **Unique ≥8-char prefix**: case-insensitive
   `starts_with` over the task's rule uuids — one hit → Ok, more →
   `AmbiguousReminderRule { candidates }` (immediately, like
   `resolve_task` — a growing prefix that stays ambiguous reports
   the candidates, it does not fall through).
3. **Offset text form**: `ReminderOffset::parse_text_form` → find
   the task's rule at that offset → Ok (uniqueness is the dedup
   invariant, D-R1) or fall through.
4. **Miss** → `UnknownReminderRule(selector)`.

A uuid-shaped selector that misses falls through to prefix/offset
rather than erroring early (the `resolve_task` uuid-v7-collision
ruling, carried). Unit tests pin each selector kind's success plus
the three failure shapes (unknown selector, ambiguous prefix,
offset-form miss), asserting `matches!(err, …)` — never text.

### D10 — REPL and completion: recorded no-change findings

- `repl.rs`: zero changes. REPL lines parse through the shared
  clap tree and dispatch through `dispatch_line`'s `TasksCmd::Remind`
  arm → `collapse` → `remind_line`; the new flag and mode flow
  through untouched code. The prompt named repl.rs as a seam; the
  finding is that the seam needs nothing — recorded here so stage 2
  doesn't invent churn.
- `completion.rs`: zero changes. Flag *names* complete from the
  clap walk (`--remove` appears automatically); flag *values* for
  `--remove` default to `Kind::None` (free text) — matching
  `--before` today. State-derived rule completion would need a new
  `Kind` (rule ids are live entities); that is a scope cut,
  consistent with the existing reminder flags and with the design
  docs (neither D-R3 nor §5 asks for it). Out-of-scope note for the
  stage report, not a backlog issue (MD-4: no tracker edits).

### D11 — UI: the action-layer growth (registry, ctx, routing)

Two new actions, the banner-verb pattern end to end:

- `ActionId::ReminderRuleAdd` — `"reminder.rule-add"`, label "Add
  reminder", contexts `GLOBAL` (menu-reachable everywhere, like
  snooze/mute), category `Task`, enabled, `menu_rank: 130`,
  `menu_group: Some(MenuGroup::Task)`, description naming the
  subject-task add.
- `ActionId::ReminderRuleRemove` — `"reminder.rule-remove"`, label
  "Remove reminder", same shape, `menu_rank: 140`.

`DispatchCtx` grows the parameter seams (the `board_target`
precedent):

```rust
pub rule_target: Option<ReminderRuleId>,      // remove
pub rule_add_offset: Option<ReminderOffset>,  // add
```

Routing (the banner arms' shape):

- `ReminderRuleAdd`: `(ctx.focused_task, ctx.rule_add_offset)` both
  present ⇒ `Command(AddReminderRule { task, offset })`; otherwise
  `NoTarget` (a menu invocation carries neither).
- `ReminderRuleRemove`: `ctx.rule_target` present ⇒
  `Command(RemoveReminderRule { rule })`; otherwise `NoTarget`.

The `tests::registry_covers_every_action_exactly_once` `ALL` list
grows both variants (compile-forced); `only_reserved_actions_stay_disabled`
passes unchanged (new rows are enabled). No keymap preset bindings —
the recorded banner-verb debt applies verbatim (menu and click
affordances only). The keymap/menu insta dumps regenerate (D14).

### D12 — UI: the rule-list composition (data + affordances)

New `ui/compositions/rule-list.slint`, mounted inside
`task-view.slint` after the metadata row (extends the editor, R1
discipline; no new view family):

- **Rows**: one per rule — the offset in the canonical text form
  (mono role, the due-phrase slot's styling), the created stamp
  (caption, secondary), and a remove affordance
  (`IconButton`, new `IconKind.remove` glyph following the file's
  existing glyph pattern). Rows are the composition's
  `[RuleRowModel] { before: string, created: string }` — **ids do
  not cross into slint**: index-resolution goes through a new
  `IdTables.task_rules: Vec<ReminderRuleId>` built in the same
  refresh pass (the banner-table discipline).
- **Add affordance**: a row of preset buttons plus a custom entry
  (the existing `TextField`). Presets are const data in
  `presentation.rs`: `pub const RULE_PRESETS: &[TimeDelta]` =
  `[5 min, 30 min, 1 h, 24 h]`, labels derived via
  `ReminderOffset::text_form` (`"5m"`, `"30m"`, `"1h"`, `"24h"`) —
  never hand-written strings. The custom entry parses through
  `ReminderOffset::parse_text_form` at the adapter; a parse miss
  dispatches nothing (absorbed — the UI owns no error-chrome
  surface; the CLI owns typed failures).
- **Hint**: when the task has ≥ 1 rule, the composition shows the
  derived-fact hint (no default fires while any rule does —
  `reminders.rs:392-402` is the derivation this presents). A
  presentation helper renders it; per AGENTS §5 no test asserts its
  text.
- **Callbacks** on `AppWindow`: `task-rule-remove-clicked(int)`
  (row index), `rule-add-preset-clicked(int)` (preset index),
  `rule-add-custom-accepted(string)`.
- **No cap** on rendered rows (the model is a map; the editor list
  scrolls — D-R4's no-cap stance). `TaskDetailModel` grows the rows
  array + the hint string; `push_task_detail` fills them from
  `state.reminders.rules` filtered by the subject task (id order —
  the table and the model agree by construction).

### D13 — UI: adapter seams and the banner precedent

`adapter.rs`: two one-shot fields (`rule_target`,
`rule_add_offset`) beside the existing pair, consumed-and-cleared
in `route()` (`:214-219`); `install_task_seam` grows the three
callback installs: resolve index → table id (remove) or preset/
parsed offset (add), set the one-shot, `route(ActionId::…)`. The
banner seams (`:1211-1254`) are the template; no new lanes, no
`UiRequest` growth (the rule verbs are commands, not view-state).

### D14 — snapshot & fixture regeneration scope (the one deliberate insta pass)

- **Expected to diff (deliberate)**: `keymap_snapshots.rs` — the
  registry dump gains two rows; the Task menu-group dump gains two
  entries. Reviewed as "the two rows, nothing else".
- **Expected NOT to diff (the identity proof)**: the state-engine
  S-series. s10's verb swap (`SetReminderRule` → `AddReminderRule`
  at `snapshots.rs:773`) must leave both `s10_*` snapshots
  byte-identical — on a rule-less task the two verbs plan identical
  action lists, which *is* the first-remind-identical acceptance
  criterion, snapshot-proven. **If s10 diffs, that is a finding**
  (the identity broke), not a snapshot to accept. No new S-scenario
  is added: the add/dedup/remove/clear planner behaviors are
  unit-tested at the domain layer (AGENTS §5: one behavior, one
  test, lowest layer; the engine thread adds routing only), and the
  sync-lane tombstone behavior is chunk 4's proven machinery
  exercised by the converted tier-2 site.
- **Receipt/--json fixtures**: `cli_reminders.rs` rewrites
  (`r2` becomes the add-semantics test: a second add at a different
  offset adds a second row; a repeat at the same offset returns
  `added: false` naming the first rule) and extends (remove by
  uuid/prefix/offset, the three typed failure shapes, session-lane
  `Remove` routing in `r10`'s style, clap coherence in `r8`).
  Typed assertions throughout — this suite has no insta corpus and
  gains none.
- **Tier-2** (`it_docker.rs:762`): the convergence site switches to
  `AddReminderRule` + `ReminderRuleAdded { added: true }`; the
  docker-harness honesty rules (MD-5) apply unchanged. A rule
  *removal* convergence (tombstone round-trip) already exists in
  chunk 4's suite via clear — this chunk adds no new tier-2
  scenario.
- **Untouched**: `golden_visuals` (board-only), web render
  snapshots, storage, the domain proptest corpus's existing
  members (P1/P2 alarm variants are chunk 7's).

### D15 — CHANGELOG: the semantic-change line

The Unreleased entry explicitly flags the behavior change of a live
verb: `tasks remind <task> --before` (and the REPL/UI equivalents)
**adds** a rule instead of replacing the task's rules; repeat adds
at the same offset are no-ops; removal is `--remove <selector>`;
`--clear` removes all N. First-use flow identical. (Design §6's
mitigation note, made user-visible.)

## 4. Implementation inventory (file-by-file, commit ladder)

Every commit standalone (builds + existing gates green); TDD — the
test of a behavior lands in its behavior's commit.

1. **`feat(domain): the reminder-offset text form`**
   - `taskboard-domain/src/reminders.rs`: `text_form`/
     `parse_text_form` + contract docs.
   - `taskboard-app/src/config.rs`: `parse_before` delegates.
   - `taskboard-app/src/commands/reminders.rs`: `format_before`
     deleted, call sites → `.text_form()`.
   - Tests: grammar pins move/extend (config's existing
     `parse_before` tests stay green); roundtrip property
     (grammar-reachable magnitudes) + garbage rejection; canonical
     form pins (`"0s"`, `"10m"`, `"1h30m"`, `"48h"` from `"2d"`).
2. **`feat(domain): the add and remove reminder-rule verbs`**
   - `messages.rs`: `AddReminderRule`/`RemoveReminderRule` (+docs).
   - `command.rs`: the two planner arms (D2/D3), the two outcome
     variants; `SetReminderRule` untouched (coexists for this one
     commit — it is still the only verb the app knows).
   - Tests: the §5 domain matrix incl. the dedup proptest.
3. **`feat(domain): retire SetReminderRule for the add/remove pair`**
   - The full D1 removal set in one motion: domain variant +
     handler + `ReminderSet` + planner tests; app `add_on` rename +
     switch + receipts (D6) + selector resolution (D9) + error
     variants (D7); `cli.rs --remove` (D8) + invocation/collapse +
     both lane arms; `cli_reminders.rs` rewrite/extend;
     `snapshots.rs` s10 verb swap; `it_docker.rs` conversion.
   - This commit's green gates are the compile-enforcement of the
     retirement (no construction site survives).
4. **`feat(ui-slint): the task-editor rule-list composition`**
   - `actions.rs` (D11), `router.rs` (ctx + arms + tests),
     `adapter.rs` (D13), `models.rs` (tables + projection),
     `task-view.slint`, `compositions/rule-list.slint` (D12),
     `icon-button.slint` (`IconKind.remove`), `presentation.rs`
     (presets + hint helper).
   - Tests: router arms (command/NoTarget), registry coverage list,
     journey leg (preset add → `AddReminderRule`; row remove →
     `RemoveReminderRule` via `assert_commands`), component test
     (rows render from state; hint presence).
   - The keymap/menu insta dumps regenerate here (D14's reviewed
     diff).
5. **`docs(changelog): the reminder rule verbs`** — D15's line
   (the house pattern: changelog rides its own docs commit, as
   chunks 4/5 did).

## 5. Test matrix summary (layer → subject)

Domain (`command.rs` tests + `reminders.rs` proptests):

- add on rule-less task: mint + upsert + one `UpdateAppData`
  enqueue, `added: true` — and the action list is *equal* to the
  retired path's on the same fixture (the identity pin).
- add at an existing offset: **zero actions**, `added: false`,
  receipt names the survivor; state unchanged.
- add at a second, different offset: a second rule exists; both
  fire (the semantic change).
- add rejections: `UnknownTask`/`TaskDeleted` before any dedup.
- remove unknown id: `UnknownReminderRule` typed.
- remove existing: `DeleteReminderRule` + one enqueue;
  `ReminderRuleRemoved`; the cascade empties the rule's
  deliveries/snooze links (apply_actions asserted through the
  action batch).
- clear N ≥ 2: N delete actions + one enqueue + `ReminderCleared
  { removed: N }`; clear empty: `NoOp`.
- proptest: dedup idempotence — for any strategy-built rule set,
  a second add at an existing offset plans no actions and names
  the same rule (the design §5 hook); text-form roundtrip + garbage
  rejection (D5).

App (`cli_reminders.rs` + unit):

- r-suite rewrite: second-add adds (different offsets → two list
  rows); same-offset repeat → `added: false`, no state change
  (list row count pinned); `r3` default-before unchanged; `r4`
  unchanged; `r5`/`r6` unchanged; `r7` gains the rule-selector
  failure kinds; `r8` gains the `--remove` coherence pins; `r10`
  routes `Remove` through the session lane.
- remove by all three selector kinds (uuid / unique prefix /
  offset text) each resolves to the same rule; unknown selector →
  `UnknownReminderRule`; colliding prefix → `AmbiguousReminderRule`
  carrying both ids; offset-form miss → `UnknownReminderRule`.
- receipts round-trip `--json` through the existing envelope
  machinery (structural field asserts, no text).

State: s10 verb swap with **no snapshot diff** (D14).

Sync: the `it_docker` convergence site on `AddReminderRule`
(tier-2 honesty rules apply).

UI: router arms; registry coverage; journey leg; composition
component test; keymap/menu dump diffs reviewed as the two rows.

## 6. Non-goals (dispatch plan §7 restated, plus this plan's)

- The banner path: `snooze_target`, `ReminderTarget`,
  `banner_rows` — byte-identical (D-R4). Per-firing snooze over N
  rules is the reminder center's (#203, deferred D-R12).
- No alarms anything (chunk 7 owns `ReminderRef::Adhoc`; the
  D-R6 ripple is not this chunk's).
- No reminder center, no rule *editing* (no edit verb exists or
  lands; removal + re-add is the flow), no per-rule mute, no cap
  on rules.
- No migration (0007 pre-created `updated_at`; no schema change
  in this chunk — the "shared migration slot" note in the design's
  §7 is chunk 7's 0008).
- No keymap preset bindings for the new actions (the recorded
  banner-verb debt).
- No state-derived rule completion (`Kind::None`, D10); no new
  engine S-scenario (D14); no reminder-center UI; no kiosk surface.
- No flush/outbox/codec changes — chunk 4's machinery, consumed
  as-is (any diff in `appdata.rs`/`pipeline.rs`/`outbox.rs`/
  `push.rs`/sync `actor.rs` non-test code is scope creep).

## 7. Risks, residual windows, and interpretation flags

- **Interpretation call (D5)**: the offset text form's definition
  site moves to the domain, adding `reminders.rs` to the file list.
  Reason recorded there; it implements D-R3/D-R4 as written across
  the ui dependency boundary. If the reviewer reads the impl-plan
  file list as exhaustive, this is the one item to rule on — the
  alternative (ui-side grammar duplication) is a house-finding.
- **Interpretation call (D3)**: no `live_task` on Remove — the
  cascade-unreachability argument. A reachable contrary state is a
  STOP-and-report, not a local guard.
- **s10 no-diff discipline**: the plan asserts a byte-identical
  snapshot outcome; a diff is a finding, never an accepted
  regeneration.
- **The one-motion commit is large** (~500 lines): deliberate; the
  ruling forbids a live interval where both vocabularies exist.
  Commits 1-2 keep every intermediate state buildable.
- **Dedup vs foreign documents**: a device can hold duplicate-
  offset rules folded from a foreign document; the verb surface
  then dedups against the id-first survivor. No GC pass is added —
  no sanctioned write produces the state, and the evaluator already
  treats duplicates observably (two firings). If the user wants a
  document-normalization pass, that is new design space (stage
  report note, not backlog edit — MD-4).
- **Receipt field churn**: `ReminderSetReceipt` renames — any
  out-of-tree script consuming `--json` keys breaks. Pre-1.0
  single-user product; the CHANGELOG line is the notice.
- **UI parse-miss silence** (D12): a typo'd custom offset
  dispatches nothing with no error surface — acceptable for an
  affordance whose presets cover the common path; the CLI owns
  typed failures. Flagged for stage 3's usability judgment.
- **Preset choice** (5m/30m/1h/24h) is unpinned by the design —
  presentation const data, trivially adjustable in review.

## 8. Acceptance checklist (stage 2/3 verify against this)

- [ ] `SetReminderRule` compiles nowhere; no alias, no reinterpretation
      (grep + the retirement commit's diff).
- [ ] First-remind flow behaviorally identical (domain identity pin
      + s10 byte-identical snapshots).
- [ ] A second rule at a different offset adds; both fire.
- [ ] Same-offset add plans zero actions; receipt `added: false`
      names the survivor.
- [ ] Remove by uuid, unique ≥8 prefix, and offset text form; all
      three failure shapes typed (`matches!`-asserted).
- [ ] Remove/clear emit their tombstones through the chunk-4 sites
      (one enqueue per command; N deletes; no flush-code diff).
- [ ] Rules converge through the document lane (tier-2 site green,
      honestly recorded if dockerless).
- [ ] Banner/snooze machinery untouched (diff-proof).
- [ ] CHANGELOG flags the semantic change explicitly.
- [ ] The insta diffs are exactly: keymap registry + Task menu
      group (two rows); nothing else; no `.pending` files ship.
- [ ] Full AGENTS §10 gate suite green.
- [ ] PR body carries `Closes #204`; no other tracker edits (MD-4).

## 9. References

- Dispatch plan §1/§7 (chunk 6, prompts 6.1-6.3, MD-1…MD-10).
- Wave implementation plan §2 DDF-8/DDF-13, §3 chunk 6, §4-§6.
- Reminders-surfaces design doc Part A (D-R1-D-R4), §4 rulings
  (retirement rationale), §5 test hooks, §6 risks, §7 chunk
  mapping.
- Sidecar design doc §3.9 (D-S9 — the rules section contract the
  verbs plug into), via D-R2's citation.
- Chunk 4 stage-1 plan `.artifacts/plans/2026-10-22-daily-driver-appdata-fold-plan.md`
  (D7's enqueue-site list, D8's tombstone derivation — the
  machinery this chunk consumes).
- ADR 0009 (the CLI surface + `--json` envelope contract).
- Issue #204 (the clobber complaint this chunk closes).

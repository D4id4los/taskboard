# [Plan] Chunk 1.7 — clap At-Least-One-Flag Groups on `tasks edit` + `labels edit`

- **Date**: 2026-10-31
- **Kind**: stage-1 implementation plan for backlog round 2, chunk 1.7
  (`fix/tasks-edit-flag-group`; closes #407): a zero-flag `tasks edit`
  or `labels edit` becomes a clap usage error (nonzero exit) instead
  of falling through to the engine's typed `no_op`.
- **Origin** (normative): issue #407 (recorded from chunk-5 field
  surfaces' stage-3 review per that plan's D10 — "fix the wording or
  file the issue — not silently"); dispatch plan
  `2026-10-31-backlog-round2-dispatch-plan.md` §1 chunk 1.7 + §3
  prompts 1.7.a–c; origin plan
  `2026-10-22-daily-driver-task-field-surfaces-plan.md` D10 (the
  ruling that ordered this entry).
- **Status**: READY — worktree
  `../taskboard-WT-fix-tasks-edit-flag-group` on branch
  `fix/tasks-edit-flag-group`, re-based onto the fetched `origin/main`
  tip (AGENTS #415's sanctioned re-branch; see §4 worktree risk for
  why the original `main`-cut base was replaced). Doctor pass: 235G
  free vs the 25G floor. Every open question settled below; the
  central mechanism decision is **empirically verified**, not assumed
  (§D1 evidence).

## 0. The bug, located

`TasksCmd::Edit` (`crates/taskboard-app/src/cli.rs:1259`, nine flag
fields: `title`, `description`, `due`, `scheduled`, `effort`,
`importance`, `urgency`, `archive`, `unarchive`) and `LabelsCmd::Edit`
(`cli.rs:1196`, flag fields `title`, `color`) carry docstrings
claiming an at-least-one-flag contract, but neither carries a clap
`ArgGroup`. A zero-flag invocation parses fine and dispatches an empty
changeset; the engine's existence-before-changeset guard then types it
as `no_op` — exit 0 (`taskboard-domain/src/command.rs:931` for
`UpdateTask`, `:1290` for `UpdateLabel`, both
`if changes == &TaskChanges::default() { return Ok(no_op()); }`).
The dispatch-plan acceptance: zero-flag edits rejected **at the clap
boundary**; at-least-one-flag edits unchanged; docstrings accurate;
CLI journeys green.

## 1. Decisions (every open question settled; D-numbers are this plan's)

### D1 — Mechanism: flattened `clap::Args` flag structs with a struct-level `#[group(required = true, multiple = true)]`; the variant-level `#[group]` attribute is REJECTED on evidence

The house already has the exact precedent: `RepeatModeFlags`
(`cli.rs:662-663`) — a `#[derive(clap::Args)]` struct carrying
`#[group(id = "repeat-mode", multiple = false, required = true)]`,
flattened into its subcommand variant via `#[command(flatten)]`.
This chunk copies the shape with `multiple = true`.

The tempting minimal alternative — `#[group(required = true,
multiple = true)]` directly on the `Edit` variants, as `Remind` does
(`cli.rs:1322`) — **does not work here and is ruled out**: clap_derive
puts *every* argument of the variant into the group, **including the
required positional** (`task`/`label`), and a present positional
satisfies the required group — the bug would survive untouched.
Verified empirically on clap 4.6.7 (the workspace's locked version)
with a scratch derive-only crate, 2026-10-31:

| invocation shape | variant-level `#[group(required, multiple)]` | flattened struct with struct-level group |
|---|---|---|
| positional only, zero flags | **PARSED** (positional satisfies the group — the bug) | `Err(MissingRequiredArgument)` |
| positional + one flag | PARSED | PARSED |
| positional + all flags together | — | PARSED (`multiple = true`) |
| `--archive --unarchive` | — | `Err(ArgumentConflict)` (member rule survives) |

So the ruled shape for both arms: move the flag fields into a
flattened flags struct; the positional stays in the variant, outside
the group. A variant-level group remains correct only where the
positional is *meant* to satisfy the group (`Remind`'s
add-with-default-before fallback — see §7 out-of-scope).

Exact attributes (per arm):

```rust
// cli.rs — immediately before LabelsCmd (its consumer; the
// RepeatModeFlags precedent keeps each flags struct adjacent to it):
/// The `labels edit` flags: at least one is required per line —
/// clap-enforced by the `label-edit-flags` arg group (`required =
/// true`, `multiple = true`). The positional label selector stays
/// outside the group: a variant-level group would let it satisfy the
/// requirement (clap_derive groups all variant arguments, positionals
/// included).
#[derive(Debug, Default, clap::Args)]
#[group(id = "label-edit-flags", required = true, multiple = true)]
pub struct LabelEditFlags {
    /// New title.
    #[arg(long)]
    pub title: Option<String>,
    /// New color (six hex digits).
    #[arg(long, value_parser = parse_color)]
    pub color: Option<String>,
}

/// The `tasks edit` field flags: at least one is required per line —
/// clap-enforced by the `task-edit-flags` arg group (`required =
/// true`, `multiple = true`). The positional task selector stays
/// outside the group (see [`LabelEditFlags`] for why).
#[derive(Debug, Default, clap::Args)]
#[group(id = "task-edit-flags", required = true, multiple = true)]
pub struct TaskEditFlags {
    /// New title.
    #[arg(long)]
    pub title: Option<String>,
    /// New description.
    #[arg(long)]
    pub description: Option<String>,
    /// RFC-3339 with an explicit offset (normalized to UTC),
    /// YYYY-MM-DD (midnight UTC), or `none` (clears). …   ← the
    /// existing doc comments move verbatim with their fields
    #[arg(long, value_parser = parse_due)]
    pub due: Option<DueSpec>,
    #[arg(long, value_parser = parse_due)]
    pub scheduled: Option<DueSpec>,
    #[arg(long, value_parser = parse_effort)]
    pub effort: Option<EffortSpec>,
    #[arg(long, value_parser = parse_importance)]
    pub importance: Option<ImportanceSpec>,
    #[arg(long, value_parser = parse_urgency)]
    pub urgency: Option<UrgencySpec>,
    /// Archive the task.
    #[arg(long, conflicts_with = "unarchive")]
    pub archive: bool,
    /// Unarchive the task.
    #[arg(long)]
    pub unarchive: bool,
}

// The variants shrink to selector + flatten:
Edit {
    /// The task (uuid | unique ≥8-char uuid prefix | exact title).
    task: String,
    #[command(flatten)]
    flags: TaskEditFlags,
},
// LabelsCmd::Edit likewise: label: String + #[command(flatten)]
// flags: LabelEditFlags.
```

Every field keeps its existing doc comment, `long`/`value_parser`,
and the `archive`↔`unarchive` `conflicts_with` pair verbatim — this
chunk moves fields, it does not redesign them.

### D2 — Names: `TaskEditFlags`/`LabelEditFlags`; group ids `task-edit-flags`/`label-edit-flags`

Struct names follow `RepeatModeFlags` (`<purpose>Flags`). Explicit
group ids follow the `repeat-mode` precedent (kebab-case, descriptive)
— never the derive's default (the struct ident), which would leak a
CamelCase id into help/error contexts. No collisions: clap ids are
scoped per subcommand; `tasks edit`'s arg ids are `task`, `title`,
`description`, `due`, `scheduled`, `effort`, `importance`, `urgency`,
`archive`, `unarchive` and `labels edit`'s are `label`, `title`,
`color` — neither proposed group id matches any of them, each other,
or the only other group id in the tree (`repeat-mode`, a sibling
subcommand). Struct derives: `Debug` (the parent enums derive `Debug`)
and `Default` (all fields `Option`/`bool`; keeps the five mechanical
construction sites in `dispatch_routing.rs`/`repl.rs` terse via
`..Default::default()`). No `Clone` — neither parent enum derives it.

### D3 — Semantics: `required = true` + `multiple = true` is exactly "at least one flag"

`required = true` makes the group demand ≥1 member; `multiple = true`
allows any number. (`required = true` alone, or `multiple = false`,
would wrongly cap an edit at exactly one flag.) Verified in the D1
experiment: eight flags together parse. The member-level
`--archive`/`--unarchive` conflict is orthogonal to group arity and
keeps firing (`ArgumentConflict`).

### D4 — The rejection is pinned at the parse boundary only, asserting the clap error kind — never text

House style is `Cli::try_parse_from` in `cli.rs`'s `mod tests`
(`cli.rs:1383+`); existing rejection pins are bare `is_err()` — this
chunk tightens the bar to the typed kind, the structural fact (AGENTS
§5 forbids message text). The prescribed assertions:

```rust
use clap::error::ErrorKind as ClapErrorKind;

let err = Cli::try_parse_from(["taskboard", "tasks", "edit", "draft"])
    .expect_err("a zero-flag edit is rejected at the parse boundary");
assert_eq!(err.kind(), ClapErrorKind::MissingRequiredArgument);
assert!(err.use_stderr(), "a usage error exits nonzero");
```

`use_stderr()` pins the nonzero-exit shape (usage errors exit 2
through `err.exit()`; the D1 scratch run printed
`exit_stderr=true`). No REPL or IPC-specific tests: `repl.rs:538` and
the daemon's line path parse through the *same* `Cli` parser, so the
rejection reaches every frontend at the one boundary — one behavior,
one test, lowest layer (AGENTS §5).

### D5 — The command arms stay semantically untouched; the engine keeps its `no_op` path

The diff touches zero statements in `commands/tasks.rs::edit`,
`commands/labels.rs::edit`, `*_on`, or the domain. Only the destructure
*shape* above the two dispatch sites changes (the mechanical churn
D6). The engine's empty-changeset → `no_op` stays: it remains
reachable from function-level callers (the `cli_editing.rs` journeys
call `tasks::edit`/`labels::edit` directly with
`TaskChanges::default()` — `cli_editing.rs:199-225` — and stay green,
untouched), and it is still the correct semantics for a repeated
identical edit (`NoOp`, exit 0). This chunk removes the CLI *entry*
to an empty changeset; it does not remove the engine's typed handling.

### D6 — Mechanical churn: the flatten restructures five construction/destructure sites and collapses `task_changes`'s arity

Complete site inventory (verified by grep 2026-10-31):

- `commands/mod.rs:1147` — line dispatch destructures
  `TasksCmd::Edit { task, title, …, unarchive }` → becomes
  `{ task, flags }` and passes `&flags` on.
- `commands/mod.rs:1517` — `task_changes` collapses from nine
  parameters to `fn task_changes(flags: &TaskEditFlags) ->
  taskboard_domain::TaskChanges` (internally destructures the struct;
  every mapping line unchanged); the
  `#[allow(clippy::too_many_arguments)]` above it is deleted.
- `commands/mod.rs:1956` — the session mirror (REPL + IPC shared
  `tasks_line`) → same `{ task, flags }` shape.
- `commands/mod.rs:1057` + `:1901` — the two `LabelsCmd::Edit` sites
  destructure through the struct in one step:
  `LabelsCmd::Edit { label, flags: LabelEditFlags { title, color } }`
  — the `labels::edit`/`labels::edit_on` calls keep their exact
  argument expressions.
- `tests/dispatch_routing.rs:220`, `:368`, `:609`, `:656` —
  construction sites → `flags: TaskEditFlags { title: Some(…),
  ..Default::default() }` / `LabelEditFlags { … }` (routing behavior
  assertions unchanged).
- `tests/repl.rs:1025`, `:1055` — construction sites → same shape.
- `cli.rs:1590`, `:1635` — the two existing `TasksCmd::Edit`
  destructures inside cli tests → nest through
  `flags: TaskEditFlags { scheduled, effort, importance, urgency, .. }`.

No other `TasksCmd::Edit`/`LabelsCmd::Edit` sites exist (checked
repo-wide, including `completion.rs` — the completion walk enumerates
args, not clap groups, so it is untouched; its "groups" vocabulary is
its own verb-grouping concept, unrelated).

### D7 — Docstrings: the two now-false claims get reworded in the behavior commit; the labels arm's claim becomes true and stays

The dispatch plan's acceptance line is "docstrings accurate"; the
stage-2 prompt's "docstrings stay as-is" was written against the
labels arm only. Current state, verified:

- `cli.rs:1259` — `Edit task fields (a zero-flag edit is a typed
  no-op).` → becomes FALSE after the fix. Reword to the enforced
  contract: `Edit task fields (at least one flag required).` (the
  labels arm's existing wording, now true for both).
- `commands/tasks.rs:477-479` — "A zero-flag edit is a typed no-op
  (`no_op`, exit 0) — no clap group enforces a flag minimum; a
  repeated identical edit is likewise an accepted no-op (`NoOp`, exit
  0)." → first half FALSE, second half TRUE. Reword the first
  sentence to: "The CLI refuses the zero-flag invocation at the parse
  boundary (the `task-edit-flags` clap group); an empty changeset
  arriving programmatically is still a typed no-op (`no_op`, exit 0),
  and a repeated identical edit is likewise an accepted no-op (`NoOp`,
  exit 0)." — the function-level contract (D5) stays documented.
- `cli.rs:1196` — `Edit a label (at least one flag required).` →
  becomes TRUE. Stays as-is.
- `commands/labels.rs:144-145` — "A changeset touching nothing that
  differs is an accepted no-op." → describes identical-value no-ops,
  orthogonal, still true. Stays as-is.

### D8 — Commits: one standalone green `fix(app)` commit + one `docs(changelog)` commit

TDD sequence (red → green), but the failing-red state is never
committed (AGENTS §8 commit hygiene: every commit builds). The
mechanical churn (D6) is required for compilation, so it cannot be
split from the group attributes — one commit carries tests +
implementation + docstring rewords:

1. `fix(app): tasks and labels edit demand at least one flag`
2. `docs(changelog): zero-flag edits rejected at the cli` — one
   `### Changed` entry under `[Unreleased]`, referencing #407, house
   style (stage-2 step 4).

PR title (stage-3 hands off): `fix(app): tasks and labels edit demand
at least one flag`.

## 2. Test catalogue (the full TDD-first list; all in `cli.rs` `mod tests`)

Written FIRST, in this order; the two zero-flag tests fail red
today (the parse succeeds), the rest pass before and after:

1. `zero_flag_tasks_edit_is_a_clap_usage_error` —
   `["taskboard", "tasks", "edit", "draft"]` → `Err`, kind
   `MissingRequiredArgument`, `use_stderr()`. The regression pin for
   #407's tasks half (the D10 bug, verbatim).
2. `zero_flag_labels_edit_is_a_clap_usage_error` —
   `["taskboard", "labels", "edit", "urgent"]` → same two asserts.
   The labels half is as much the subject as the tasks half (the
   issue names both; a tasks-only fix is the classic half-landing).
3. `a_single_flag_satisfies_the_edit_group_on_each_arm` —
   `tasks edit draft --title t` parses and lands on the arm (assert
   the field value through the flattened destructure);
   `tasks edit draft --archive` parses (a bare bool flag counts as a
   member — the group's arity logic must not demand values);
   `labels edit urgent --color 0f0f0f` parses. The three shapes cover
   the field kinds in the two groups (plain `Option<String>`,
   `bool`, `value_parser`-parsed) without enumerating all eleven.
4. `an_edit_naming_every_flag_together_still_parses` — tasks: all
   eight non-conflicting members at once (`title description due
   scheduled effort importance urgency archive` — `unarchive`
   necessarily omitted: it conflicts with `archive`, a member-level
   rule orthogonal to group arity, documented in the test comment);
   labels: `--title` + `--color` together. The `multiple = true` pin.
5. `the_edit_group_keeps_the_archive_unarchive_conflict` —
   `tasks edit draft --archive --unarchive` → `Err`, kind
   `ArgumentConflict`. Pre-existing behavior, previously unpinned;
   the restructure moves those attributes across the flatten
   boundary, so the pin protects exactly what this chunk could break
   (the D1 scratch run confirmed the mechanism, the in-repo test
   makes it a contract).

Adapted mechanically (not new behavior): the two existing
`TasksCmd::Edit` destructure tests (`cli.rs:1572`, `:1617` — field
flags landing, `none`-clears, garbage rejections) keep every
assertion and gain the `flags:` nesting; the five construction sites
(D6) keep their assertions.

Untouched and expected green: `cli_editing.rs` journeys (function
level), all completion tests, `cli_reminders.rs` (the `Remind` group
is not touched), `cli_repeat.rs` (the `repeat-mode` group is not
touched).

## 3. Typed-outcome test matrix (behaviour → layer)

| Behaviour | Layer / test |
|---|---|
| Zero-flag `tasks edit` rejected, typed kind, nonzero-exit shape | `cli.rs` parse test 1 |
| Zero-flag `labels edit` rejected, typed kind, nonzero-exit shape | `cli.rs` parse test 2 |
| Single flag suffices (value/bool/parser field kinds) | `cli.rs` parse test 3 |
| All flags together parse (`multiple = true`) | `cli.rs` parse test 4 |
| `--archive`/`--unarchive` conflict survives the group | `cli.rs` parse test 5 |
| Flag→changeset mapping unchanged | existing `task_changes`/destructure tests (adapted destructures only) |
| Routing/mirrors unchanged (exit codes, receipts) | `dispatch_routing.rs` + `repl.rs` (construction-shape adaptation only) |
| Function-level empty-changeset `no_op` stays (engine contract) | `cli_editing.rs` E2 (untouched, green) |
| REPL/IPC inherit the rejection | no new test — same `Cli` parse boundary (D4) |

Text-assertion boundary, concrete for this chunk: **assertable** —
the clap error kind (an enum), `use_stderr()`, parsed field values;
**never assertable** — clap's message/usage text, help output, error
wording.

## 4. Risks & mitigations

- **clap group semantics under the flatten** (the central assumption):
  retired by the D1 experiment on the locked clap 4.6.7; the new tests
  re-pin it in-tree, so a future clap bump that changes group behavior
  fails loudly at CI, not silently.
- **`requires`/`conflicts_with` references across the flatten
  boundary**: arg ids are unchanged (same field idents), so
  field-level references keep resolving — verified in the D1 run
  (`ArgumentConflict` still fired). Mitigated going forward by test 5.
- **Group-id collision** (stage-3's named trap): ruled out in D2 by
  enumeration; ids are subcommand-scoped.
- **Usage-string growth**: `tasks edit --help`/`labels edit --help`
  usage gains a `<--title <TITLE>|…>` group segment — accepted; no
  goldens or text assertions exist on help output (verified: the
  `insta` hits in the tree are unrelated).
- **Completion drift**: the walk enumerates subcommands/args, not
  groups (verified by reading `completion.rs`'s group vocabulary);
  existing completion tests must stay green in the gate run — any red
  there is a real finding, not noise.
- **Half-landing** (tasks fixed, labels missed): the catalogue pins
  both arms as first-class tests 1+2; stage-3's functional focus
  re-checks the diff for both.
- **Worktree volatility — observed this session, not hypothetical**:
  the first `git worktree add -b … off main` (@ `5bf90c63`, the then
  local main tip) was swept by a *concurrent chunk agent's*
  `worktree-doctor.sh` run minutes later: local `main` trails
  `origin/main` (@ `e8a919cd`), so a zero-commit worktree HEAD
  classified as "already merged upstream" — the doctor's disposable
  category — and the sweep hit inside the clean window before the
  plan commit. Recovery per AGENTS §8: the branch ref survived
  (worktree removal never deletes branches); the worktree was
  recreated on the existing branch, re-based onto `origin/main` (a
  tip-based HEAD is the doctor's KEEP category), and the plan was
  committed in the same breath. Standing rules for stages 2–3:
  doctor first; recreate with `git worktree add
  ../taskboard-WT-fix-tasks-edit-flag-group fix/tasks-edit-flag-group`
  (no `-b` — the branch exists); make the first commit (or verify
  HEAD is not contained in origin/main) before any pause; never
  assume an uncommitted fresh worktree survives parallel rounds.

## 5. Estimated diff size

~+190/−50 (~240 changed lines) all-in: `cli.rs` +~115/−30 (two
structs with doc comments, variant shrink, two adapted destructure
tests, five new tests ~+95), `commands/mod.rs` ±25 (four destructure
sites + `task_changes` collapse), `commands/tasks.rs` ±3 (docstring),
`tests/dispatch_routing.rs` ±20, `tests/repl.rs` ±20,
`CHANGELOG.md` +6. Consistent with the dispatch plan's **S** sizing.

## 6. Out of scope (chunk boundaries, restated)

- The engine's `no_op`/`NoOp` paths and `TaskChanges`/`LabelChanges`
  defaults — unchanged (D5); the unknown-target-empty-edit rejection
  (`commands/mod.rs` existence-before-changeset guard) is untouched.
- `tasks remind`'s variant-level group includes its positional, so
  bare `tasks remind <task>` parses as add-with-`default_before` —
  observed during D1, pre-existing, and the `--before` docstring
  names exactly that fallback: intended, not this issue, not changed.
- `track adjust` and `habits update` keep their ruled zero-flag
  no-ops (pinned by `cli_track.rs:354`, `cli_habits.rs:194`) —
  different verbs, different surfaces (#407 names only the two edit
  arms).
- No completion, IPC, daemon, storage, sync, or UI changes; no
  `labels`-family behavior beyond the parse boundary; no version bump
  (single global semver moves at release time).
- No push, no PR from stage 1; handoff at plan-landed-on-branch.

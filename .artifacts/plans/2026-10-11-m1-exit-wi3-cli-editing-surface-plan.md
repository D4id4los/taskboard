# [Plan] M1 Exit WI-3 — Full CLI Editing Surface (+ XDG Discovery, Storage-Failure Event, `logout`, `config print`)

- **Date**: 2026-10-11
- **Target**: `taskboard-app` (`cli.rs`, `commands/{mod,tasks,stacks,labels,logout,config}.rs`,
  `output.rs`, `config.rs`, `bootstrap.rs`, `main.rs` daemon arm),
  `taskboard-domain` (`messages.rs`, `outbox.rs`, `command.rs` one plan arm,
  `pipeline.rs` one intent arm, proptest strategies, `SystemEvent`),
  `taskboard-state` (`actor.rs` broadcast + spawn signature),
  `taskboard-storage-sqlite` (`codec.rs` one arm), `taskboard-sync-nextcloud`
  (`push_exec.rs` one executor). **No changes** to `taskboard-ui-slint`.
  **No new workspace dependencies** (`dirs`, `toml_edit`, `chrono` all present).
- **Goal**: complete the M1-exit report's work item 3 (report §4 B / §7 row 3):
  surface every existing domain verb in the CLI (`tasks edit/delete`,
  `stacks rename/delete/move`, `labels list/add/edit/delete`,
  `tasks label/unlabel`), upgrade the task selector from exact-uuid to the
  cascading resolver (exact uuid → unique ≥8-char prefix → exact title), and
  land the two pre-GUI riders: XDG config discovery and the storage-failure
  `SystemEvent` (the Phase 7 error banner's input), plus `logout` and
  `config print`. After WI-3 the CLI is the complete daily driver the WI-4
  REPL wraps.
- **Status**: PROPOSED — implementation may start immediately on
  `feat/app-cli-editing-surface` (steps §6).
- **Parent**: `.artifacts/reports/2026-10-11-m1-exit-pre-gui-analysis-report.md`
  §4 B, §7 row 3. Backlog entries consumed (2026-10-10 batch): "Task
  selectors beyond exact uuid", "`tasks edit/delete`, `stacks`/`labels`
  subcommands", "`logout`", "[2026-10-10] XDG config discovery", "[2026-10-10]
  Config doctoring / `taskboard config print`", "[2026-10-09] Storage-failure
  `SystemEvent`".
- **Predecessors**: WI-0 (backlog dedup, f61397d), WI-2 (hermetic stack/label
  push tests + D-series routing suite, 84fa33b), WI-1 / Phase 6b (stand-alone
  first, 2377771..13e1891) — all merged on `main`. Authored against `main` @
  `13e1891`.
- **Branch / worktree**: `feat/app-cli-editing-surface`, worktree
  `../taskboard-WT-feat-app-cli-editing-surface` (created from `main` @
  `13e1891`). One PR for the plan (Phase 6b precedent: plan revisions and
  implementation commits land on the same branch).

---

## 1. Context & Known Facts (do not re-derive; verified on `main` @ `13e1891`)

- **Every verb this plan surfaces already exists in the domain and is
  pinned by the domain/state suites.** `StateCommand` carries `UpdateTask`
  (command.rs:259), `DeleteTask` (:315), `RenameStack` (:368), `DeleteStack`
  (:384), `UpdateLabel` (:450), `DeleteLabel` (:473), `AssignLabel` (:499),
  `UnassignLabel` (:517); replies are `CommandOutcome` (command.rs:88-103),
  rejections are `CommandError::{UnknownTask, TaskDeleted, UnknownStack,
  StackDeleted, UnknownLabel, LabelDeleted, NoBoard, …}` (command.rs:55-84).
  The backlog entry states it outright: "the outbox/push machinery needs no
  changes for the missing verbs."
- **The push chain for all surfaced verbs is implemented and, since WI-2,
  hermetically tested.** `push_exec.rs` has executors + wiremock tests for
  `RenameStack` (test :1582), `DeleteStack` (:1650-1714), `CreateLabel`
  (:1746-1849), `UpdateLabel` (:1871-1941), `DeleteLabel` (:1971+); the
  client carries `update_stack` (client.rs:594), `delete_stack` (:616),
  `assign_label` (:792), `remove_label` (:808), `create_label` (:824),
  `update_label` (:847), `delete_label` (:869). WI-3 adds **one** new push
  executor (§1 next item) and otherwise never touches the sync crate's
  logic.
- **`stacks move` is the single verb that needs new domain surface** (the
  report names it; the domain has no `MoveStack`). Everything it needs is
  already in place: `StackClocks.order` exists (entities.rs:74); the
  pipeline's post-read intent protection already restores a stack's
  `order` for rename intents (pipeline.rs:184-194, `keep_stack_intent`);
  the Deck wire payload `StackChanges { title, order }` (client.rs:109-112)
  is a **full-send** — the `RenameStack` executor PUTs the entire row via
  `update_stack(…, &mapping::stack_changes_from(row))` (push_exec.rs:171-203),
  so a `MoveStack` executor is a literal mirror of it (identical wire call;
  only the intent arm differs). New: `StateCommand::MoveStack`,
  `LocalOp::MoveStack(StackId)` (outbox.rs grows one variant), one
  `pending_stack_intent` arm (pipeline.rs:163-181), one storage codec arm
  (`"move_stack"`, codec.rs:42-93 pattern).
- **The task selector is exact-uuid only today.** `parse_task_id`
  (tasks.rs:303-307) rejects anything else with `UnknownTask`; the backlog
  pins the upgrade: "exact id, then a unique id prefix (≥8 chars), then an
  exact-title match — mirroring `resolve_stack`". `resolve_stack` (the
  cascade's model) lives at tasks.rs:90-121 and pairs `UnknownStack` /
  `AmbiguousStack { candidates }`. `tasks done/move` are the existing
  consumers that switch over.
- **The handler pattern to copy is fixed by ADR 0009 and Phase 6b.**
  Core-booting verbs wrap an `AsyncFnOnce(&App)` body in `with_core`
  (tasks.rs:32-48; the Phase 6b handoff map says WI-3 grows exactly this
  seam); the dispatcher's `run_mutation` helper (mod.rs:430-445) types the
  body to `MutationReceipt` — new receipt shapes need it generalized;
  `CommandError::error_kind` is exhaustive so every new variant gets a
  compiler-forced kind (mod.rs:165-193), as is `domain_error_kind`
  (mod.rs:213-223). Receipts are serde projections in `output.rs`, never
  domain shapes.
- **Stand-alone-first (Phase 6b) makes every new verb testable without
  wiremock.** All editing verbs are backend-agnostic (they boot the core,
  mutate, print; under `[sync] backend = "standalone"` resolution the core
  performs no network I/O by construction). The ST-series harness
  (tests/cli_standalone.rs: `standalone_config` + `forbidden_store`)
  proves the pattern; the Phase 6b handoff map assigns WI-3 "the
  standalone test harness (no wiremock) for every new verb".
- **Config discovery is CWD-relative only.** `config_path` resolves
  `--config` > `TASKBOARD_CONFIG` > `None` (→ `./taskboard.toml`,
  `DEFAULT_CONFIG_PATH`, config.rs:27, :250-252; the figment chain merges
  an absent file as nothing, config.rs:408-413). The backlog wants
  `$XDG_CONFIG_HOME/taskboard/taskboard.toml` consulted between the env
  var and the CWD default. The `dirs` crate is already a dependency
  (`default_db_path`, config.rs:417-419), so `dirs::config_dir()` costs
  nothing. `login` writes config via a `toml_edit` overlay that preserves
  comments/unknown tables (login.rs:170-194) — `logout`'s strip is its
  mirror image.
- **`logout`'s substrate shipped in Phase 5 unused**: `CredentialStore::
  delete` (secrets.rs:98), `KeyringStore::delete` (:131-134, `NoEntry` →
  `CredentialError::NotFound`), the env store refuses `Unsupported`
  (:182-184), and the key is `user@host` (`credential_key`, :193-204).
  The open question "logout's interaction with `backend`" (backlog,
  2026-10-11 entry) is decided in §4.8.
- **The storage-failure event's plumbing points exist but the engine
  cannot broadcast yet.** `SystemEvent` is a `Copy` enum
  `NetworkLost|NetworkRestored|Shutdown` (messages.rs:172-180); bootstrap
  creates the channel and hands the engine the **receiver** only
  (bootstrap.rs:308-323); storage failures surface as the command reply
  `ExecuteError::Storage` at actor.rs:434 (memory never advanced; safe
  retry). The state tests already have a `FailingRepo` (fails the first N
  applies with `Unavailable`, engine.rs:32-70) to drive the failure. A
  broadcast needs `spawn_state_engine` to also take a
  `broadcast::Sender<SystemEvent>` (signature ripple: lib.rs:39-47 doc,
  bootstrap.rs, state tests `engine.rs`/`snapshots.rs`/`sqlite_smoke.rs`/
  `proptest.rs`, sync tests `e2e_hermetic.rs`/`common/sync_pair.rs` — all
  one-line call-site edits, compiler-driven).
- **A malformed label color is a *permanent* push failure** — the
  `CreateLabel` executor rejects `BadRequest` without sending when the
  stored color is not six hex digits (push_exec.rs:1791-1849), so a bad
  color poisons the outbox. Board colors tolerate this because `publish`
  maps defensively with a fallback (boards.rs:157-159) — **labels have no
  such fallback**, so the CLI must validate label colors strictly at the
  argument boundary (§4.7).
- **Suite inventory for the new tests**: D-series routing
  (`tests/dispatch_routing.rs`, one exit-code test per `Command` variant,
  WI-2), ST-series stand-alone journeys (`tests/cli_standalone.rs`),
  L-series login (`tests/cli_login.rs`), C-series config
  (`tests/config.rs`), J-series docker journeys J1-J4
  (`tests/it_cli.rs`, `tests/it_docker.rs`) — J5 is next.

## 2. Product Decisions (this plan; user-overridable at review)

1. **The task selector becomes human-usable everywhere** (backlog
   decision): cascade exact uuid → unique ≥8-char uuid prefix → exact
   live-title match; ambiguity and misses are typed errors. Exact-uuid
   stays first, so existing scripts keep working.
2. **`tasks edit` is flag-sparse** (`--title`, `--description`, `--due`,
   `--archive`/`--unarchive`); at least one change is required (clap
   usage error, exit 2, not a runtime error).
3. **`stacks move` is in scope** (the report names it); it is the one
   verb that grows the domain (§4.4) — everything else is app-layer.
4. **`labels list` joins the bundle** (report omits it): labels are
   otherwise invisible until assigned, which would make the label verbs
   untargetable. `stacks list` stays out (`tasks list` already shows
   stacks).
5. **No interactive confirmations on destructive verbs** (`tasks
   delete`, `stacks delete`, `labels delete`): the M1 CLI is
   scripting-first; the cascade semantics are documented in `--help`.
6. **`logout` deletes the stored secret and returns the install to
   stand-alone under the auto backend**; an explicit
   `backend = "nextcloud"` is never rewritten (ADR 0010 loudness rule —
   the decision the 2026-10-11 backlog entry deferred to WI-3, §4.8).
7. **`config print` is a decode-only diagnostic** that always prints
   (validity is reported *in the payload*, not via the exit code).
8. **XDG discovery changes reads only**; the write target policy
   (`login` creates `./taskboard.toml` on a fresh machine) is unchanged
   this WI (§7 notes the follow-up).
9. **`StorageFailed` is edge-triggered** (fires on the ok→fail
   transition, not per command — the backlog's debounce concern), and the
   daemon logs it; the Phase 7 UI banner is the real consumer.

## 3. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| Task selector | `resolve_task` cascade (uuid → ≥8-char unique prefix → exact title); `CommandError::AmbiguousTask`; wired into `done/move` + the new verbs |
| Tasks | `tasks edit` (sparse `TaskChanges` via flags), `tasks delete` (tombstone receipt), `tasks label`, `tasks unlabel` |
| Stacks | `stacks rename`, `stacks delete` (cascade-count receipt), `stacks move --order` (new domain `MoveStack` end-to-end, §4.4) |
| Labels | `labels list/add/edit/delete`; `resolve_label` cascade (uuid → title); strict six-hex color parser at the CLI boundary |
| Config | XDG read-path discovery (`--config` > `TASKBOARD_CONFIG` > XDG-if-present > `./taskboard.toml`); `ConfigError::kind()`; `config print` handler + `ConfigView` |
| Logout | `logout` handler: secret delete + config strip under auto backend; `LogoutReceipt` |
| System event | `SystemEvent::StorageFailed`; engine broadcast (edge-triggered) via a `spawn_state_engine` sender param; daemon-side event logger |
| Output contract | `MutationReceipt.labels` (additive); `TaskDeleteReceipt`, `StackDeleteReceipt`, `LabelReceipt`, `LabelRow`, `LogoutReceipt`, `ConfigView`; new error kinds (§5.4) |
| Tests | ST-series extensions (new `tests/cli_editing.rs`, hermetic sqlite, no wiremock); D-series arm per new variant; L-series logout; C-series XDG; domain/state/sync tests for `MoveStack`; state test for the storage event; J5 docker journey |
| Docs | ADR 0009 amendment; `architecture.org` (verb catalogue, config order, storage event); README quickstart; CHANGELOG; backlog status flips + appends (§7, §8) |

**Out of scope** (backlog or later WIs): connector-management verbs
(`connect add` — existing 2026-10-11 entry; composes with `logout`
afterwards); board unbind/`--forget-board` and any `DeleteBoard`; local
board-edit verbs (`RenameBoard`, existing entry); ordinal selectors and
`--title` search flags (cascade already covers prefix + title);
`tasks list` filtering/sorting options; XDG write-target flip (§7);
storage-failure events for the *report-apply* path (§4.9 note);
REPL/IPC (WI-4/5); any UI work.

### 3.1 CLI surface after this WI

```
# tasks (selector = uuid | ≥8-char unique prefix | exact title)
taskboard tasks list [--stack <sel>]                        # unchanged
taskboard tasks add <title> [--stack <sel>]                 # unchanged
taskboard tasks done <task> [--undo]                        # selector upgraded
taskboard tasks move <task> [--stack <sel>] [--order <n>]   # selector upgraded
taskboard tasks edit <task> (--title s | --description s | --due when | --archive | --unarchive)
taskboard tasks delete <task>
taskboard tasks label   <task> <label-sel>
taskboard tasks unlabel <task> <label-sel>

# stacks (selector = uuid | exact title, as today's --stack)
taskboard stacks add <title> [--order <n>]                  # unchanged (6b)
taskboard stacks rename <stack> <title>
taskboard stacks delete <stack>
taskboard stacks move <stack> --order <n>

# labels (selector = uuid | exact title)
taskboard labels list
taskboard labels add <title> [--color <6-hex>]
taskboard labels edit <label> (--title s | --color <6-hex>)
taskboard labels delete <label>

# connection & diagnostics
taskboard login […]                                         # unchanged
taskboard logout
taskboard config print
taskboard sync | daemon | boards {list,select,create,publish}  # unchanged
```

`--due <when>` parses RFC-3339 (`2026-10-24T09:00:00Z`), a bare date
(`2026-10-24` → midnight UTC), or the literal `none` (clear the field);
a malformed value is a clap usage error (exit 2). `--color` for labels
accepts exactly six hex digits (normalized to lowercase); anything else
is a usage error — this is load-bearing (§1: a bad label color is a
permanent push rejection, unlike lenient board colors which `publish`
maps defensively).

## 4. Design Decisions

1. **`resolve_task` mirrors `resolve_stack` with a prefix tier**
   (tasks.rs, next to `resolve_stack`). Cascade over live tasks only
   (`is_live`, the same population `tasks list` shows): (a) exact uuid
   parse that hits a live task wins; (b) else a ≥8-character prefix
   matched against live uuid strings — exactly one hit resolves, zero
   falls through, several is `AmbiguousTask`; (c) else exact title —
   one hit resolves, zero is `UnknownTask`, several is `AmbiguousTask`.
   `AmbiguousTask { candidates: Vec<String> }` carries the *titles* of
   the matched tasks (deterministic board order), mirroring
   `AmbiguousStack`. Known wrinkle, accepted: UUIDv7's timestamp prefix
   means tasks created within ~65 s of each other collide at 8 hex
   chars — the honest answer is the ambiguity error listing candidates;
   the user passes more characters or the full id. `parse_task_id` is
   deleted; `done`/`move` and every new task verb consume
   `resolve_task`. The **stack** and **label** selectors keep today's
   two-tier shape (uuid → exact title) — prefixes stay a task-only
   affordance until asked for (§7).

2. **Edit/delete receipts come from pre/post-execute snapshots, never
   re-derivation.** `tasks edit` projects the post-execute live task
   through `mutation_receipt` (the `add/done/move` precedent); `tasks
   delete` cannot (the task is tombstoned afterwards), so it snapshots
   `{id, title}` *before* executing and returns `TaskDeleteReceipt`.
   `MutationReceipt` gains `labels: Vec<String>` (additive JSON key —
   the output contract is internal-not-versioned, output.rs:8-10) so
   `tasks label/unlabel` can return the same receipt shape and the
   label state is visible in every mutation reply.

3. **Destructive receipts report the blast radius.** `stacks delete`
   counts the stack's live tasks pre-execute into
   `StackDeleteReceipt { id, title, cascaded_tasks }` (domain cascade
   tombstones them; pipeline.rs `stack_delete_cascade` tests pin it);
   `tasks delete`/`labels delete` return the plain delete receipts.
   Repeats are idempotent successes (`DeleteTask` on a tombstone is the
   domain's accepted no-op; `DeleteStack` likewise) — the receipt then
   reports the already-deleted entity's snapshot. No confirmation
   prompts (§2.5); `--help` documents the cascade.

4. **`MoveStack { id, order }` follows `MoveTask`'s shape, minus the
   stack half.** Domain: plan arm rejects `UnknownStack`/`StackDeleted`,
   treats an unchanged order as an accepted `NoOp` (nothing enqueued),
   else stamps `clocks.order`, mutates `order`, persists
   `UpsertStack`, enqueues `LocalOp::MoveStack(id)` (enqueue-on-change
   only, the `RenameStack` precedent — verify in the plan arm while
   implementing). Outbox: one `LocalOp` variant. Pipeline:
   `pending_stack_intent` gains `MoveStack(s) if s == stack =>
   rename = true` — a pending move protects the same fields a rename
   does (title+order+clocks, `keep_stack_intent` already restores
   exactly those), which is correct because the push is a full-send
   (next item). Push: `MaterializedOp::MoveStack` mirrors the
   `RenameStack` executor verbatim — same `update_stack(board, remote,
   stack_changes_from(row))` full-row PUT, same echo application
   (`map_stack_echo`), same unbound/missing-row synthetic-complete and
   `NotFound` classification (push_exec.rs:171-203, :1650-1714) — only
   the materialization maps the new op. Storage: `"move_stack"` codec
   arm + roundtrip test (stack-scoped `OpColumns`). Proptest:
   `state_command_strategy` grows the variant; the existing totality
   property covers it with no new harness code (Phase 6b precedent).

5. **`labels` verbs resolve labels by uuid or exact title**
   (`resolve_label`, the `resolve_stack` shape verbatim over the
   board's live labels), with `CommandError::UnknownLabel(String)` /
   `AmbiguousLabel { candidates }` at the CLI layer (the domain's typed
   `UnknownLabel/LabelDeleted` surface through the existing
   `domain_error_kind` arms). `labels add` defaults the color to a
   `DEFAULT_LABEL_COLOR` const (one value, chosen in review — Deck's
   UI default green `31cc7c` is the suggestion). `labels list`
   projects `LabelRow { id, title, color, assigned_tasks }` (count of
   live tasks carrying it) in board order — the discoverability floor
   that makes the other three verbs usable.

6. **`tasks label/unlabel` are selector composition, nothing more**:
   `resolve_task` + `resolve_label` → `AssignLabel`/`UnassignLabel`,
   receipt = `MutationReceipt` (with the new `labels` field reflecting
   the post-state set). `UnassignLabel` of an absent assignment is the
   domain's accepted no-op (idempotent, exit 0).

7. **Label colors are validated by a clap value parser** (`parse_color`
   in `cli.rs`: exactly six hex digits, normalized lowercase, usage
   error otherwise). Rationale §1: unlike board colors (lenient, with
   a publish-time fallback), an invalid label color is a permanent
   `BadRequest` push rejection that strands the outbox op — refuse it
   at the boundary. `boards create --color` stays lenient (its
   fallback exists; not this WI's business).

8. **`logout` = delete the secret, then de-configure iff the backend
   is auto-resolved.** Pre-core command (no lock, no db). Requires
   `nextcloud.server_url` + `username` to be present — else the typed
   `CommandError::NotConfigured` (kind `not-configured`; nothing to
   log out from). Then: (a) `store.delete(server, username)` —
   `CredentialError::NotFound` is an idempotent success (repeat
   logout, or logout-without-login); `Unsupported` (env store) is the
   honest typed failure (kind `credential`: unset
   `TASKBOARD_APP_PASSWORD` yourself); (b) if `[sync] backend` is
   **unset** (auto), strip `nextcloud.server_url`/`username` from the
   config file via a `toml_edit` overlay (the `login.rs:170-194`
   write's mirror — removes the two keys, preserving everything else),
   so the install cleanly resolves back to stand-alone; if
   `backend = "nextcloud"` is **explicit**, the file is left untouched
   (ADR 0010: the explicit backend is the operator's loud choice;
   deleting the secret under it must not silently flip the product
   shape — the next boot fails loudly at credential resolution, which
   is the correct "logged out of an explicitly-configured server"
   state). Receipt `LogoutReceipt { server_url, username,
   config_cleared: bool }`. The board binding and local db are
   untouched: a later `login` to the same server resumes sync where it
   left off (documented in `--help`; `--forget-board` stays backlog).
   Ordering rationale: secret first (the essential act), config second
   — a mid-way failure leaves a recoverable state either way (typed
   `ConfigWrite` error, exit 1; re-running logout is idempotent).
   Env-provided values (`TASKBOARD_NEXTCLOUD__*`) are operator
   territory: logout never edits the process environment, and
   `config_cleared` reflects the *file* (documented).

9. **`config print` decodes, validates, reports — never refuses.**
   Pre-core. Uses `extract_raw` (decode + db-path fill, no validation
   gate — the `login` precedent) and prints
   `ConfigView { config_path: Option<String>, backend: String,
   valid: bool, invalid_kind: Option<String>, config: AppConfig }`
   where `backend` is the resolved `sync_backend()` and
   `invalid_kind` is a new `ConfigError::kind()` discriminant mapping
   (`"file-not-found" | "decode" | "invalid-server-url" |
   "empty-username" | "invalid-duration" | "empty-db-path"` — typed
   variant names, no message text). Exit 0 even when `valid: false`
   (it is a *print*; the payload is the diagnosis); only `Decode`
   failures exit 1 (nothing printable). The config is secret-free by
   construction (`deny_unknown_fields` blocks stray secrets,
   config.rs:5-9), so the dump needs no redaction pass. Human output:
   a deterministic `key: value` rendering (path, backend, validity,
   then the four sections' fields) — plain formatting, no new toml
   serializer dependency. This deliberately does *not* change
   `CommandError::Config`'s kind (`"config"`) for other commands —
   `ConfigError::kind()` is a `config print`-scoped accessor.

10. **XDG discovery: presence-checked candidate between env and CWD.**
    New resolution in `AppConfig::config_path`:
    `--config` > `TASKBOARD_CONFIG` > XDG candidate *when the file
    exists* > absent (→ `./taskboard.toml` as today). The candidate is
    `dirs::config_dir()?.join("taskboard").join("taskboard.toml")`
    (`$XDG_CONFIG_HOME` on Linux, platform-appropriate elsewhere).
    Semantics preserved: `FileNotFound` still fires only for
    *explicitly requested* paths (`--config`/`TASKBOARD_CONFIG`);
    absent XDG and absent CWD files both mean "defaults only". The
    write target is unchanged: `login` overlays the *resolved* path —
    an existing XDG file (it wins reads, so it's the right target),
    else `./taskboard.toml` on fresh machines (§7 backlog note for the
    flip). Testability: the candidate computation is a pure function
    of an injected config dir (`fn xdg_candidate(config_dir:
    Option<&Path>) -> Option<PathBuf>`-shaped seam, unit-tested
    directly); no `XDG_CONFIG_HOME` env fiddling in tests. One rule,
    three consumers stay intact (`load`, dispatcher/login, `CommandCtx`
    in `main`). Help text of `--config` updated to name the full order.

11. **`SystemEvent::StorageFailed` is edge-triggered at the reply
    site.** Domain: one `Copy` unit variant (messages.rs) — it is a
    banner signal, not an error report (the command reply already
    carries the typed `RepositoryError`). Engine:
    `spawn_state_engine` gains a `broadcast::Sender<SystemEvent>`
    parameter (the actor already holds the receiver; both halves now
    come from bootstrap's one channel, bootstrap.rs:308). The actor
    loop keeps a `storage_failed: bool` edge: at the
    `ExecuteError::Storage` reply site (actor.rs:434) broadcast
    `StorageFailed` iff `!storage_failed`, then set it; any successful
    persistence resets it. This *is* the backlog's debounce — a dying
    disk fires once per failure epoch, not once per command. Recipient
    matches: the engine ignores its own `StorageFailed` (compiler-
    forced arm), as does the sync actor (same). Scope note: the
    command-reply path only — persistence failures while *applying
    sync reports* (no reply channel) keep today's log treatment;
    extending the event there is a Phase 7 follow-up (§7).
    Observability now: `bootstrap` grows
    `pub fn spawn_system_event_logger(system:
    &broadcast::Sender<SystemEvent>) -> JoinHandle<()>` — subscribe,
    log each received variant at warn (StorageFailed) / info, exit on
    `Lagged`/channel close; `main`'s daemon arm spawns it before the
    signal wait (log-only by design; no text assertions anywhere).

12. **Dispatch keeps one arm per verb, and `run_mutation` generalizes.**
    `run_mutation` (mod.rs:430) is retyped to
    `run_verb<F, Fut, T>(json, ctx, render: fn(&T) -> String, body)`
    so delete/label/config receipts flow through the same
    boot→work→receipt shell; the existing `MutationReceipt` arms pass
    `tasks::render_text_receipt`. Every new `Command`/sub-enum variant
    gets its dispatch arm (the D-series suite then covers it — the
    compiler + D-series together are the routing gate).

## 5. Normative Shapes (signatures binding; bodies indicative)

### 5.1 CLI (`cli.rs`)

```rust
pub enum Command {
    /* … */
    /// Delete the stored Nextcloud credentials (and, under the auto
    /// backend, the config's server/user keys).
    Logout,
    /// Effective-configuration diagnostics.
    Config { #[command(subcommand)] cmd: ConfigCmd },
}

pub enum ConfigCmd {
    /// Print the effective config (secret-free by construction).
    Print,
}

pub enum TasksCmd {
    /* List/Add/Done/Move unchanged; Add's `--stack` help notes the selector */
    /// Edit task fields (at least one flag required).
    Edit {
        /// The task (uuid | unique ≥8-char prefix | exact title).
        task: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// RFC-3339 or YYYY-MM-DD (midnight UTC); `none` clears.
        #[arg(long, value_parser = parse_due)]
        due: Option<DueSpec>,
        #[arg(long, conflicts_with = "unarchive")]
        archive: bool,
        #[arg(long)]
        unarchive: bool,
    },
    /// Delete (tombstone) a task; cascades its pending ops.
    Delete { task: String },
    /// Attach a label to a task.
    Label { task: String, label: String },
    /// Detach a label from a task (idempotent).
    Unlabel { task: String, label: String },
}

pub enum StacksCmd {
    /* Add unchanged */
    /// Rename a stack.
    Rename { stack: String, title: String },
    /// Delete a stack and its tasks (tombstone cascade).
    Delete { stack: String },
    /// Reorder a stack within the board.
    Move { stack: String, #[arg(long)] order: i64 },
}

pub enum LabelsCmd {
    /// List the board's labels.
    List,
    /// Create a label (color defaults when omitted).
    Add { title: String, #[arg(long, value_parser = parse_color)] color: Option<String> },
    /// Edit a label (at least one flag required).
    Edit {
        label: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_parser = parse_color)]
        color: Option<String>,
    },
    /// Delete a label (detaches it from all tasks).
    Delete { label: String },
}

/// `--due` grammar: set (RFC-3339 or bare date → midnight UTC) or clear.
pub enum DueSpec { Set(chrono::DateTime<chrono::Utc>), Clear }

/// Six lowercase hex digits or a clap usage error (labels only — §4.7).
fn parse_color(raw: &str) -> Result<String, String> { /* … */ }
fn parse_due(raw: &str) -> Result<DueSpec, String> { /* … */ }
```

### 5.2 Domain (`messages.rs`, `outbox.rs`, `command.rs`, `pipeline.rs`)

```rust
// messages.rs — StateCommand grows:
/// Reorder a stack on the bound board (no-op when the order is unchanged).
MoveStack {
    /// Stack to reorder.
    id: StackId,
    /// Requested sort order within the board.
    order: i64,
},

// messages.rs — SystemEvent grows (stays Copy; banner signal, no payload):
/// A persistence write failed and was rejected (memory unchanged);
/// first failure since the last success.
StorageFailed,

// outbox.rs — LocalOp grows:
/// Stack reorder (full-send on push, like RenameStack).
MoveStack(StackId),

// pipeline.rs — pending_stack_intent grows one arm:
LocalOp::MoveStack(s) | LocalOp::RenameStack(s) if s == stack => rename = true,
// (fold into the existing match; `keep_stack_intent` already restores
//  title+order+clocks — exactly the fields a pending move protects)
```

### 5.3 State engine (`actor.rs`, `lib.rs`)

```rust
/// The engine now also *publishes* system events (StorageFailed).
pub async fn spawn_state_engine(
    repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    sync_out: mpsc::Sender<SyncCommand>,
    reports: mpsc::Receiver<SyncReport>,
    system_out: broadcast::Sender<SystemEvent>,   // NEW
    system_in: broadcast::Receiver<SystemEvent>,
) -> Result<(EngineHandle, JoinHandle<()>), EngineStartupError>

// actor loop: `let mut storage_failing = false;`
// at the ExecuteError::Storage reply site:
if !storage_failing {
    let _ = system_out.send(SystemEvent::StorageFailed); // no receivers: fine
}
storage_failing = true;
// on any persistence success: storage_failing = false;
```

### 5.4 App errors & kinds (`commands/mod.rs`)

```rust
pub enum CommandError {
    /* … */
    /// The task selector matches several live tasks; carries their titles.
    #[error("task selector matches several tasks — pass one of {}", candidates.join(" | "))]
    AmbiguousTask { candidates: Vec<String> },
    /// No label matches the selector.
    #[error("no such label")]
    UnknownLabel(String),
    /// The label selector matches several live labels; carries titles.
    #[error("label selector matches several labels — pass one of {}", candidates.join(" | "))]
    AmbiguousLabel { candidates: Vec<String> },
    /// A command needs a configured connector that this install lacks
    /// (logout with no server/username).
    #[error("no Nextcloud connection is configured")]
    NotConfigured,
}
// error_kind(): "ambiguous-task", "unknown-label", "ambiguous-label",
//               "not-configured"
// error_detail(): AmbiguousTask/AmbiguousLabel → joined candidates;
//                 UnknownLabel → selector
```

### 5.5 Output (`commands/output.rs`)

```rust
pub struct MutationReceipt { /* … */ pub labels: Vec<String> }   // additive
pub struct TaskDeleteReceipt { pub id: String, pub title: String }
pub struct StackDeleteReceipt { pub id: String, pub title: String, pub cascaded_tasks: usize }
pub struct LabelReceipt { pub id: String, pub title: String, pub color: String }
pub struct LabelRow { pub id: String, pub title: String, pub color: String, pub assigned_tasks: usize }
pub struct LogoutReceipt { pub server_url: String, pub username: String, pub config_cleared: bool }
pub struct ConfigView {
    pub config_path: Option<String>,
    pub backend: String,             // "standalone" | "nextcloud" (resolved)
    pub valid: bool,
    pub invalid_kind: Option<String>, // ConfigError::kind() discriminant
    pub config: AppConfig,
}
```

### 5.6 Config (`config.rs`)

```rust
/// The XDG read-path candidate, pure over an injected base dir.
fn xdg_candidate(config_dir: Option<&Path>) -> Option<PathBuf> { /* … */ }

impl AppConfig {
    /// Resolution order: `--config` > `TASKBOARD_CONFIG` > XDG candidate
    /// (when it exists) > `None` (→ `./taskboard.toml`, absent tolerated).
    #[must_use]
    pub fn config_path(explicit_path: Option<PathBuf>) -> Option<PathBuf> { /* … */ }
}

impl ConfigError {
    /// Stable per-variant discriminant for `config print`'s payload
    /// (typed outcome, never message text).
    #[must_use]
    pub fn kind(&self) -> &'static str { /* … */ }
}
```

### 5.7 Handlers (`commands/`)

```rust
// tasks.rs — replaces parse_task_id; the shared selector seam:
fn resolve_task(state: &AppState, selector: &str) -> Result<TaskId, CommandError>;

// stacks.rs — grows rename/delete/move in the `add` shape.
// labels.rs — NEW module: list/add/edit/delete + resolve_label.
// logout.rs — NEW module: run(config_path, raw, store) -> LogoutReceipt
//             (§4.8 sequence: require fields → delete secret → strip file
//              iff auto backend → receipt).
// config.rs — NEW module (`crate::commands::config`, no clash with
//             `crate::config`): print(ctx) -> ConfigView (§4.9).
```

## 6. Worktree, Branch & TDD Implementation Order

Branch `feat/app-cli-editing-surface`, worktree
`../taskboard-WT-feat-app-cli-editing-surface` (already created from
`main` @ `13e1891`). One PR. TDD order — commit per step, every commit
passing the full gate (§9):

1. **Task selector cascade** (`test` first: `tests/cli_editing.rs`
   ST-series): `resolve_task` + `AmbiguousTask` + kind/detail; wire
   `done`/`move` off `parse_task_id` (delete it); tests pin
   exact-uuid, unique 8-prefix, prefix-collision ambiguity
   (`AmbiguousTask` lists titles), exact title, title ambiguity,
   unknown, and that a uuid parse that hits no live task falls through
   to prefix/title (not an error). D-series arms stay green (routing
   unchanged).
2. **`tasks edit`**: `Edit` clap arm + `parse_due` unit tests +
   `TaskChanges` mapping (including `DueSpec::Clear → Some(None)`),
   `MutationReceipt.labels` additive field (existing receipt tests
   extended), ST-series: title/description/due set+clear, archive
   toggle, no-op-repeat (`NoOp` exit 0), unknown/deleted task kinds,
   at-least-one-flag is exit 2 (clap). D-series arm.
3. **`tasks delete`**: handler + `TaskDeleteReceipt`; ST-series: happy,
   repeat-is-idempotent-success, unknown/deleted kinds, done-task
   delete, receipt printed under `--json`. D-series arm.
4. **`MoveStack` domain + engine**: `state_command_strategy` arm;
   command.rs unit tests (happy path stamps `clocks.order` + enqueues
   the op + persists; unchanged order → `NoOp` with nothing enqueued;
   `UnknownStack`/`StackDeleted`); state S/A-series: routed outcome
   `Applied`, outbox op recorded, `UpsertStack` persisted,
   `StateUpdated` published; totality proptest green.
5. **`MoveStack` sync + storage**: `"move_stack"` codec arm + roundtrip
   test; `pending_stack_intent` arm + a pipeline test (post-read move
   op protects the stack's order from the same-cycle pull);
   `MaterializedOp::MoveStack` executor mirroring `RenameStack` +
   hermetic wiremock tests (PUT carries the full row; echo applied;
   unbound → synthetic-complete; 404 → remote-missing classification;
   transport failure aborts).
6. **`stacks rename/delete/move` CLI**: handlers + receipts
   (`StackDeleteReceipt` with cascade count); `run_mutation` →
   `run_verb` generalization (mechanical; existing arms unchanged in
   behavior); ST-series: rename happy + unknown/ambiguous selectors,
   delete cascade receipt + idempotent repeat, move order change +
   no-op repeat + `--order` required (clap). D-series arms.
7. **`labels` group**: `commands/labels.rs` + `resolve_label` +
   `parse_color` (unit tests: six hex, case normalization, rejects
   `#fff`, 5/7 digits, non-hex) + `DEFAULT_LABEL_COLOR`; ST-series:
   list (empty + populated with assigned counts), add (default +
   explicit color), edit (title/color/no-op), delete (+ idempotent),
   selector ambiguity/unknown kinds, color rejection is exit 2.
   D-series arms.
8. **`tasks label/unlabel`**: ST-series: assign shows the label in the
   receipt's `labels`, unlabel removes it, unlabel-absent is exit 0,
   unknown task/label kinds, label selector ambiguity. D-series arms.
9. **XDG + `config print`**: `xdg_candidate` + new `config_path`
   order + `ConfigError::kind()` (unit tests, config.rs); C-series
   tests: XDG file wins over CWD when both exist, absent XDG falls
   through, explicit `--config`/env still win, `FileNotFound` only for
   explicit paths; `commands/config.rs` + `ConfigView` +
   `render_text`; tests: valid config (fields + resolved backend),
   invalid config (`valid: false` + `invalid_kind`, exit 0), env
   overlay visible in the dump, garbage TOML → exit 1 kind `config`.
   D-series arm. `--config` help text + `architecture.org` order note.
10. **`logout`** (`commands/logout.rs`): the §4.8 sequence with a
    `toml_edit` strip helper (mirror of `write_config_overlay`; unit:
    keys removed, comments/unknown tables preserved); L-series
    extension: logout deletes from a `FakeCredentialStore` and strips
    the file under auto backend; explicit `nextcloud` keeps the file
    (`config_cleared: false`); repeat logout idempotent
    (NotFound-success); env store → `credential` kind; unconfigured →
    `not-configured`; D-series arm.
11. **`StorageFailed`**: domain variant; `spawn_state_engine` sender
    param (ripple through the 8 call sites, compiler-driven); actor
    edge-trigger at the reply site; ignore-arms in engine + sync actor
    event matches; state test with `FailingRepo` (fail → one event,
    fail again → still one, recover → reset, fail → event again;
    extend the test-local `FailingRepo` with a re-arm, it is test
    code); bootstrap wiring + `spawn_system_event_logger` (unit:
    terminates when the channel closes) + `main` daemon-arm spawn.
12. **Docs, bookkeeping, gates, docker**: ADR 0009 amendment (§8);
    `architecture.org`; README quickstart edits; CHANGELOG; backlog
    status flips (six entries → ✅ DONE with plan ref) + §7 appends;
    full §9 gates; J5 docker journey
    (`it_nextcloud_docker_cli_editing_push` in `tests/it_cli.rs`):
    fresh board via `boards create` + `login` + `publish` (J4 shape),
    then `stacks rename/move`, `labels add/edit`, `tasks edit`, `tasks
    label`, `tasks delete`, `sync --json` → `Succeeded`, server-side
    truth via raw `DeckClient` (stack title/order, label exists+color,
    card title/duedate/label set, deleted card absent); local `tasks
    list --json` still serves survivors.

## 7. Out of Scope → Backlog (entries appended with this plan)

- **XDG write-target flip**: `login` on a machine with *no* config
  anywhere still creates `./taskboard.toml`; desktop nicety would be
  XDG-first writes. Cheap follow-up once reads ship.
- **`--forget-board` / board unbind** (already named by the 2026-10-10
  logout entry): no `UnbindBoard` domain command exists; own design
  question (what happens to remote-bound content on unbind).
- **Prefix selectors for stacks/labels**: task-only for now; lift the
  tier if users ask.
- **`StorageFailed` for the report-apply path**: persistence failures
  while applying sync reports (no reply channel) still only log;
  extend the edge-trigger there when Phase 7 needs it.
- **`CommandError::Config` kind refinement**: other commands could
  surface `ConfigError::kind()` instead of the flat `"config"`; defer
  to WI-5's wire-format pass so the JSON contract changes once.
- (Existing entries referenced but untouched: connector-management
  verbs 2026-10-11 — composes with logout afterwards; ordinal
  selectors; local board-edit verbs; multi-board.)

## 8. Docs & Bookkeeping (part of this WI's PR)

- **ADR 0009 amendment** (append a dated section, status stays
  Accepted): the editing verb catalogue and the two command classes'
  new members (`logout`/`config print` pre-core; editing verbs
  core-booting); the selector cascade contract (exact uuid → unique
  ≥8-char prefix → exact title; ambiguity is a typed error listing
  candidates); destructive verbs without confirmation
  (scripting-first, cascade documented in `--help`); label color
  boundary validation rationale (permanent push rejection vs the
  board-color fallback); `logout`'s backend interaction (secret
  always, config keys iff auto — ADR 0010 loudness preserved);
  `config print`'s decode-only contract (validity in the payload,
  exit 0); `StorageFailed` edge-trigger semantics and its Phase 7
  consumer.
- **`docs/architecture.org`**: CLI verb table gains the editing
  surface; the config section documents the full discovery order
  (`--config` > `TASKBOARD_CONFIG` > XDG-if-present >
  `./taskboard.toml`); the events section names
  `StorageFailed` and the daemon logger.
- **README.org**: quickstart gains a short "editing" block after the
  stand-alone flow (`stacks rename`, `tasks edit --due`, `labels add`
  + `tasks label`, `tasks delete`) and mentions `config print` /
  `logout` in the Nextcloud section.
- **CHANGELOG** `[Unreleased]` → Added: `tasks edit/delete/label/
  unlabel`, `stacks rename/delete/move`, `labels list/add/edit/
  delete`, cascading task selector, `logout`, `config print`, XDG
  config discovery, `StorageFailed` system event; Changed:
  `tasks done/move` accept prefixes and titles (exact uuid still
  first); `MutationReceipt` JSON gains `labels`.
- **Backlog**: flip the six consumed entries to ✅ DONE with this plan
  as reference; append §7's new entries.

## 9. Verification & Acceptance

- Full quality gate (AGENTS §10) on every step commit:
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo nextest run --workspace`, `cargo test --doc
  --workspace`, `cargo deny check`, `cargo llvm-cov --workspace`
  (`SQLX_OFFLINE=true`; the app crate must not drop below its current
  ~80.3% line floor — all new verbs arrive ST/D-tested, and the new
  dispatch arms are D-covered).
- Tier 2 (docker, once at the end): `eval "$(scripts/
  nextcloud_it_setup.sh up)" && cargo nextest run -p taskboard-app
  --run-ignored only -E 'test(it_nextcloud_docker)'` green including
  the new J5; teardown after.
- **Editing runbook (WI exit, user-run)**: on a stand-alone profile:
  (1) `boards create`/`stacks add`/`tasks add` fresh; (2) `tasks edit
  <title> --due 2026-10-20 --json` → receipt shows the due date and
  empty `labels`; (3) `labels add "urgent" --json`; `tasks label
  <title> urgent --json` → receipt's `labels` lists it; (4)
  `stacks rename Todo/Backlog`-style rename + `stacks move --order`;
  (5) `tasks list` reflects everything; (6) `tasks delete` of a
  helper task → exit 0, absent from `tasks list`; (7) `config print`
  names the resolved path, `backend: standalone`; (8) repeat each
  mutation → idempotent exit 0. On the Nextcloud profile: (9) edit +
  label + delete land server-side after `sync` (J5 proves this
  automatically); (10) `logout --json` → receipt `config_cleared:
  true` (auto backend), `taskboard sync` afterwards → kind
  `sync-disabled`; re-`login` resumes sync without data loss.
  Acceptance: user confirms.

## 10. Consumer Handoff Map

| Consumer | Takes from WI-3 |
|---|---|
| WI-4 (REPL) | The complete verb surface to wrap (its premise — "a REPL is only as useful as its verb surface" — is satisfied); `resolve_task/resolve_stack/resolve_label` as the interactive name-resolution layer; `run_verb` as the `&App`-extraction stepping stone |
| WI-5 (daemon IPC) | `ConfigError::kind()` + the error-kind table as wire-format inputs; receipts (`LogoutReceipt`, `ConfigView`) as payload precedents; `StorageFailed` as a broadcast-to-IPC forwarding candidate |
| Phase 7 UI | `StorageFailed` drives the error banner (the event's stated consumer); `config print`'s view parity for the settings screen; label colors arrive pre-validated |
| Future connectors | The label-color lesson recorded in ADR 0009 (boundary validation where the wire has no fallback); `MoveStack` completes the op vocabulary every connector must drain |

## 11. Risks & Mitigations

- **UUIDv7 prefix collisions** (same-millisecond/same-minute creations
  share prefixes): inherent to time-ordered ids; the cascade's
  ambiguity error lists candidate titles and longer prefixes resolve —
  never a silent mis-target. Pinned by an ST-series test.
- **`MutationReceipt` gains a field**: additive JSON key on an
  explicitly-not-versioned internal contract; CHANGELOG notes it; no
  known consumer parses strictly (the E2E reads fields by name).
- **`MoveStack` clobbers remote order**: impossible by construction to
  do *partially* — the push is the same full-row PUT `RenameStack`
  already makes, and the post-read intent arm protects a queued move
  against the same-cycle pull; hermetic wiremock + pipeline tests pin
  both halves.
- **`spawn_state_engine` signature ripple** (8 call sites): purely
  compiler-driven mechanical edits; no behavioral change anywhere
  except the new broadcast; the D/ST/L-series + state suites prove no
  drift.
- **Logout partial failure** (secret deleted, config strip fails):
  typed `ConfigWrite` error, exit 1; re-run is idempotent (NotFound →
  success path; strip retries). Env-derived config is documented as
  out of `logout`'s reach.
- **`config print` on an undecodable file**: exit 1 (nothing honest to
  print) — same class as every other command's `Decode` failure; the
  valid-but-invalid shape (decodes, fails validation) is exactly what
  the `valid`/`invalid_kind` fields exist for.
- **ST-series runtime growth**: every editing test boots a real
  (sqlite, hermetic) core; Phase 6b measured these as fast, and no
  wiremock keeps them off the flake watchlist; the per-file suite
  (`cli_editing.rs`) keeps nextest's parallelism doing the work.
- **Scope creep** (connect verbs, unbind, ordinals, board edits,
  report-apply events): each triaged to §7 with its owner named; the
  domain additions are exactly one command and one op.

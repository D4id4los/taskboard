# [Plan] `tracing` Pass for `taskboard-domain` (WI-F)

- **Date**: 2026-10-12
- **Kind**: stage-1 implementation plan (WI-F of the 2026-10-12 backlog
  batch) — two standalone code/doc commits plus the shared backlog
  bookkeeping commit. No tests are added (by policy, §5 / D-g).
- **Origin**: backlog entry "[2026-10-09] Logging statements for
  `taskboard-domain`" (backlog line 572); dispatch plan §4 WI-F.
- **Target**: `crates/taskboard-domain/Cargo.toml`,
  `crates/taskboard-domain/src/{merge.rs, pipeline.rs}`,
  `docs/architecture.org` (§ "Crate Roles & Dependencies", item 1),
  `.artifacts/taskboard-potential-improvements-backlog.md` (final commit).
- **Branch / worktree**: `feat/domain-tracing` in
  `../taskboard-WT-feat-domain-tracing`, off `main` @ `8e63c17`.
- **Goal**: a `trace`-level filter over `taskboard_domain` suffices to
  follow one entity's merge decision end-to-end (entry identities → every
  R1–R9 rule verdict → pipeline summary), with the domain dependency cap
  amended in the docs rather than silently relaxed, and every quality
  gate (fmt, clippy `-D warnings`, nextest, doc-tests, deny, llvm-cov)
  green.
- **Status**: PROPOSED 2026-10-12 — awaits user plan review; stage 2
  implements as specified on approval.

---

## 1. Context & Findings (verified 2026-10-12 against `main` @ `8e63c17`)

1. **Zero `tracing` in the domain crate today.**
   `crates/taskboard-domain/Cargo.toml:11-19` declares exactly
   `serde`, `chrono`, `uuid`, `thiserror` (+ optional `proptest`). No
   crate in `crates/taskboard-domain/src/` references `tracing`.
2. **The conflict-policy surface is the hole in the logging story.**
   The state engine logs around it — "sync report ingested" at debug
   (`taskboard-state/src/actor.rs:505`), command rejections with
   `?rejection` (`actor.rs:427`) — but *inside* the policy (which merge
   primitive ran, which rule verdicts fired) is invisible. Coding
   guidelines §4 ("Log enough to enable debugging", lines 57-60) makes
   this the crate to fix; M1 real-server bug reports need it.
3. **The instrumentable surface is exactly 20 public functions**: 18
   `pub` primitives in `merge.rs` (module is `pub`, so `merge_board` /
   `adopt_board_after_push` / `tombstone_board` are public even though
   not re-exported from `lib.rs:62-67`) plus `pipeline::apply_sync_report`
   (`pipeline.rs:268`) and `pipeline::apply_push_report`
   (`pipeline.rs:615`). Both pipeline entries share the private
   `apply_push_outcomes` (`pipeline.rs:690`) — the R9 decision table.
4. **House tracing conventions exist and are consistent.**
   `taskboard-state`/`taskboard-sync-nextcloud` use plain events with
   static message strings and typed field captures: `?err`, `?kind`,
   `?rejection`, `missed`, `board = target.get()`
   (`actor.rs:112-638`, sync `actor.rs:169-544`). Spans appear only at
   actor/repository boundaries (`#[tracing::instrument]` in
   `taskboard-storage-sqlite/src/repo.rs:335,519`) — never per-row.
5. **The workspace pin carries `features = ["log"]`**
   (`Cargo.toml:79`), and coding guidelines §4 line 54 *mandates* it:
   "Always include the `log` compatibility feature flag in `tracing`".
   The backlog entry's "workspace-pinned, no features" phrasing predates
   checking the pin and conflicts with the guideline (§2 D-a).
6. **The dependency cap is stated in `docs/architecture.org` item 1**
   (lines 143-144: "Dependencies: minimal (`serde`, `chrono`, `uuid`,
   `thiserror`).") — not in ADR 0004, which records policy, not deps.
   `coding_guidelines.org` states the purity *principle* ("zero network,
   zero UI, zero asynchronous runtime dependencies", lines 22-24) —
   `tracing` violates none of them (pure facade, no I/O, no runtime).
7. **The accepted-gap precedent for uncovered logging arms** is the
   deck-client entry "[2026-10-08] Deck client coverage pass: endpoint
   logging arms" (backlog line 418) + m1-exit report §2 item 5: logging
   arms are deliberately left uncovered because asserting them requires
   asserting log text, which AGENTS §5 forbids. No coverage ignore
   annotations exist anywhere (and `coverage(off)` is unstable) — the
   convention is documentation, not tooling.
8. **No hard coverage gate exists to break.** CI's codecov upload runs
   with `fail_ci_if_error: false` (`ci.yml:171`); SonarCloud is
   deliberately not a required check; the AGENTS §10 `cargo llvm-cov`
   run is informational. The binding gates are fmt/clippy/nextest/
   doc-tests/deny (+ the weekly mutants + Miri jobs, both CI-side).
9. **Mutation testing targets this crate weekly** (architecture.org:
   `cargo-mutants` "for pure & core crates (`taskboard-domain` and
   `taskboard-state`)", currently at 2 documented equivalent survivors —
   domain coverage report §6). `taskboard-state` already carries dozens
   of `tracing` event calls under that same gate without producing
   mutants: macro arguments are not mutation targets by default.
10. **Purity mechanics hold.** `tracing` events are lazily evaluated:
    when the subscriber's filter is above the event level, the macro
    performs one cached callsite interest check and skips field
    evaluation entirely. Field sigils `?x` (Debug) and `%x` (Display)
    borrow — no clones, no allocation at disabled levels (kiosk-safe,
    AGENTS §1).

## 2. Design Decisions

| # | Decision | Choice & rationale |
|---|---|---|
| D-a | **Feature inheritance** (the item's open question) | `tracing.workspace = true` — the workspace pin **verbatim, including `features = ["log"]`**. Three reasons: (1) the guideline *mandates* the `log` flag whenever `tracing` is used (finding 5) — the backlog's "no features" phrasing cannot win against `coding_guidelines.org` §4; (2) AGENTS §4's single-pin rule — a member-side feature divergence (`default-features = false`, no `log`) is a second configuration to keep in sync forever, and under feature unification it changes nothing in any workspace build (state/sync/app already enable `log`); (3) semantically, `log` only forwards events to the `log` facade **when no tracing subscriber is active** — in-app (subscriber installed at bootstrap) it is inert; for a hypothetical external library consumer it is exactly the bug-report localisation we want. |
| D-b | **Cap amendment home + wording** | `docs/architecture.org` § "Crate Roles & Dependencies" item 1 — where the cap text lives (finding 6). ADR 0004 untouched: it is the sync conflict *policy* ADR; dependency lists are not its subject. Exact wording in §4.2 below. |
| D-c | **Span policy** | One `debug_span` per pipeline entry point (`apply_sync_report`, `apply_push_report`) — one span per sync cycle, matching the storage crate's per-batch (never per-row) precedent (finding 4). **No spans in merge primitives**: they are called per entity per cycle; span churn there buys nothing the events + enclosing pipeline span don't already give. |
| D-d | **Level contract** | `trace!` = entry identities: local id, remote ref, the compared timestamps, and *user content* (the title, for entity recognition in a bug report). `debug!` = rule verdicts as typed facts: `rule = "R5"`, discriminant `outcome = "resurrected"`, booleans, ids, timestamps — never prose. User content never appears above `trace`. **Silent by design**: the R4 unchanged-remote fast paths (steady-state cycles must not emit O(board) no-op events). |
| D-e | **Field-capture conventions** | Static message strings only (greppable, no formatting); `?x`/`%x` sigils (borrowed, finding 10); rule tags via module-level `const RULE_R1: &str = "R1"; …` single definitions (typo-proofing where tests are forbidden from asserting text, D-g); ids and enums as `?id` (no tracing `Value` impls needed); `DateTime<Utc>` as `?ts` (chrono has no `Value` impl). |
| D-f | **Scope: conflict-policy surface only** | The 18 merge primitives + 2 pipeline entries (incl. the shared private `apply_push_outcomes` and the `keep_*_intent` self-clobber guards reached from them). `plan_command` and `plan_pushes` stay out: their invocation and outcomes are already logged at their actor boundaries (state `actor.rs:427/505`, sync `actor.rs:325/422`); domain-level duplicates would be noise. The backlog's own wording scopes this pass to "each merge primitive and `apply_sync_report`". |
| D-g | **TDD waived for the logging arms, by rule** | AGENTS §5 forbids asserting log text; driving the events for coverage alone is the "metric-chasing" the deck-client entry (finding 7) already rejected. The regression net is the existing suite: **every existing domain/state/e2e test must pass byte-for-byte unchanged** — instrumentation is behavior-neutral by construction (events have no side effects, restructuring limited to binding `if` conditions to named locals). |
| D-h | **Unconditional instrumentation** | No `cfg` gates, no feature flag for the logging: bug reports come from release builds on kiosks; a compile-out path would silently recreate the hole this item exists to close. Cost at disabled levels is one interest check per site (finding 10). |

## 3. Instrumentation inventory (78 sites: 2 spans, 20 trace events, 56 debug events)

### 3.1 Conventions (all sites)

- Message: short static string (`"task merged"`, `"R5 resurrect"`, …).
- Fields: `rule` (const tag), entity ids (`?task_id`, `?card`, `?stack_id`,
  `?label_id`, `?board_id`, `?op_id`), timestamps (`?remote_ts`,
  `?field_clock`, `?observed_at`, `?read_at`, `?now`), booleans bare.
- Trace-entry events add `%title` (borrowed Display; the *local* title for
  `merge_*`/`tombstone_*`, the *remote* title for `adopt_*`).
- Module docs of `merge.rs` / `pipeline.rs` gain one short paragraph
  documenting this level contract (part of commit 2).

### 3.2 `merge.rs` — 18 primitives (18 trace entries + 21 debug sites)

| Primitive (rule) | Entry `trace!` fields | `debug!` decision sites |
|---|---|---|
| `adopt_remote_task` (R1) | `?task_id`, `?card`, `?stack` (remote ref), `?remote_ts`, `%remote.title` | 2: adopted (`rule=R1`, resolved `?stack_id`); **deferred_unmapped_stack** (`outcome = "deferred_unmapped_stack"`) — the None arm, the single unmapped-stack strategy |
| `merge_task` (R4/R5/R7/R8) | `?task_id`, `?card`, `?remote_ts`, `?remote_seen`, `%local.title` | 2: one R4 verdict event (booleans `title/description/duedate/done/archived_remote_wins` + `labels_remote_wins` (R7) + `position_remote_wins` & `position_stack_resolved` (R8)); one conditional R5 event when `local.deleted` (`outcome = "resurrected"/"tombstone_stands"`, `?remote_ts`, `?deleted_clock`) |
| `tombstone_task` (R3) | `?task_id`, `?observed_at` | 1: tombstoned (new clock via max) |
| `adopt_task_after_push` (R9a/b) | `?task_id`, `?card` (echo ref), `?echo_ts` | 1: echo_adopted (+ `stack_fallback` bool: unmapped echo stack kept local position) |
| `finalize_pushed_task_delete` (R9c) | `?task_id` | 1: delete_finalized (binding cleared) |
| `merge_stack` (R6 + R5 defense) | `?stack_id`, `?stack` (ref), `?remote_ts`, `?remote_seen`, `%local.title` | 1: verdict event (booleans `title_remote_wins`, `order_remote_wins`, `remote_delete_wins`, `resurrected`) |
| `adopt_remote_stack` (R1) | `?stack_id`, `?stack`, `?board_id`, `?remote_ts`, `%remote.title` | 1: adopted (+ `adopted_tombstoned` = `deleted_at.is_some()`) |
| `adopt_stack_after_push` (R9a) | `?stack_id`, `?stack` (echo ref), `?echo_ts` | 1: echo_adopted |
| `merge_label` (R6 + R5 defense) | `?label_id`, `?label` (ref), `?remote_ts`, `?remote_seen`, `%local.title` | 1: verdict event (`title_remote_wins`, `color_remote_wins`, `remote_delete_wins`, `resurrected`) |
| `adopt_remote_label` (R1) | `?label_id`, `?label` (ref), `?board_id`, `?remote_ts`, `%remote.title` | 1: adopted |
| `adopt_label_after_push` (R9a) | `?label_id`, `?label` (echo ref), `?echo_ts` | 1: echo_adopted |
| `adopt_board_if_newer` (R6) | `?board_id`, `?remote_board`, `?remote_ts`, `?remote_seen`, `%local.title` | 2: adopted; kept_not_newer (the early-return arm) |
| `merge_board` (R6, title-wise) | `?board_id`, `?remote_board`, `?remote_ts`, `%local.title` | 1: title verdict (`title_remote_wins` bool — the local-rename defense) |
| `adopt_board_after_push` (R9a) | `?board_id`, `?remote_board` (echo), `?echo_ts` | 1: echo_adopted |
| `adopt_remote_board` (R1) | `?board_id`, `?remote_board`, `?remote_ts`, `%remote.title` | 1: adopted |
| `tombstone_board` (R3/R6) | `?board_id` | 1: tombstoned |
| `tombstone_stack` (R3/R6) | `?stack_id`, `?observed_at` | 1: tombstoned |
| `tombstone_label` (R3/R6) | `?label_id`, `?observed_at` | 1: tombstoned |

Implementation shape: `if` conditions bind to named locals first
(`let title_wins = remote_wins(ts_r, local.clocks.title); if title_wins { … }`)
so each verdict event states the booleans it reports — behavior-identical
restructuring, no logic moves. `adopt_remote_task` binds
`resolve_local_stack(...)` to a local before constructing the `Task` so
the adopted/deferred verdict logs once.

### 3.3 `pipeline.rs` — 2 entries (2 spans + 2 trace entries + 35 debug sites)

| Site | Instrumentation |
|---|---|
| `apply_sync_report` entry | `debug_span!("apply_sync_report")` with `outbox_len`, `pushes`, `snapshot_stacks`, `snapshot_tasks`, `snapshot_labels`; one `trace!` with `?board` (snapshot board ref), `?read_at`, `?now` |
| `apply_push_report` entry | same span shape (`"apply_push_report"`); same trace fields |
| §1 `apply_push_outcomes` — `Applied` + delete ops (R9c) | 3 sites (DeleteTask/DeleteStack/DeleteLabel arms): push_delete_finalized (`?op_id`, entity id) |
| §1 — echo adoptions (R9a/b) | 4 sites (Task/Stack/Board/Label echo arms): echo_adopted (`?op_id`, entity id, `?echo` ref) |
| §1 — `Applied` no echo (R9d reorder) | 1 site: completed_no_echo (`?op_id`) |
| §1 — `RemoteMissing` (R9e) | 5 sites (task / stack / board / label / assign-unassign arms): remote_missing_finalized (`?op_id`, entity id; task arm adds `cancelled_ops` count) |
| §1 — `Rejected` (R9) | 2 sites: dead_lettered (`?op_id`, `?kind` — BadRequest) / stays_queued (`?op_id`, `?kind`) |
| §2 board | 3 sites: merged (R6) / adopted (R1, `?fresh_id`) / cascade fired (board deleted) |
| §3 stacks loop | 2 sites: merged (R6; `newly_tombstoned` bool) / adopted (R1). **Fast path silent** (D-d) |
| §4 labels loop | 2 sites: merged (R6) / adopted (R1). Fast path silent |
| §5 tasks loop | 3 sites: merged (R4/R5; `resurrected` bool) / resurrect delete-op drop (`?op_id`, R5) / adopted (R1, `?fresh_id`). Fast path silent; deferral is logged inside `adopt_remote_task` (§3.2) |
| §6 presence reconciliation (R3) | 3 sites (tasks/stacks/labels): absence_tombstoned (entity id, remote ref; task site adds `cancelled_ops` count) |
| §6b stack cascade (R6) | 1 site (inside the loop): cascaded_task (`?task_id`, `?stack` remote num) |
| §7 closing summary | 1 site per entry (2 total): report_applied — `actions`, `ops_completed`, `ops_failed`, `remaining_ops`, `?phase` discriminant |
| Self-clobber guards (`keep_task_intent`, `keep_stack_intent`, `keep_board_title`, `keep_label_intent`) | 4 sites, emitted only when at least one flag is set: self_clobber_guard (`?task_id`/…, the protected-field booleans). These are the highest-value debug events in the pass — "why did my local edit survive/lose against the pull" is exactly the M1 bug-report question. Private helpers gain the entity-id parameter (private signatures only; no public API change) |

### 3.4 Silent by design

- R4 unchanged-remote fast paths (§3/§4/§5 pipeline guards): a steady-state
  cycle emits the two span events + closing summary only.
- `soft_delete_verdict` returning `None` (no tombstone change): covered by
  the verdict event's booleans; no separate emit.
- `tracing` internal disabled paths: not ours to instrument.

### 3.5 Count

`merge.rs`: 18 T + 21 D. `pipeline.rs`: 2 spans + 2 T + 35 D.
**Total: 78 sites.** Expected diff ≈ 300–340 insertions across 5 files.

## 4. Dependency & docs amendment (commit 1)

### 4.1 `crates/taskboard-domain/Cargo.toml`

```toml
[dependencies]
serde.workspace = true
chrono.workspace = true
uuid.workspace = true
thiserror.workspace = true
# Pure-macro observability of the conflict policy (R1–R9); no I/O, no
# subscriber, no runtime — the amended cap lives in architecture.org.
tracing.workspace = true
```

### 4.2 `docs/architecture.org` item 1 (the one-line cap amendment)

Current (lines 143-144):

> Dependencies: minimal (=serde=, =chrono=, =uuid=, =thiserror=). Zero
> UI, network, database, or async-runtime dependencies; …

Amended to:

> Dependencies: minimal (=serde=, =chrono=, =uuid=, =thiserror=,
> =tracing= for pure-macro event instrumentation of the conflict policy
> — no I/O, no subscriber, no runtime). Zero UI, network, database, or
> async-runtime dependencies; …

### 4.3 Gate analysis for the dependency

- `cargo deny check`: `tracing`/`tracing-core`/`tracing-attributes` are
  MIT; `log` is MIT OR Apache-2.0 — all inside `deny.toml`'s allow list;
  `multiple-versions = "warn"` at worst (once_cell already in tree).
  Stage 2 runs deny as part of commit 1's verification.
- MSRV 1.96: unaffected (tracing 0.1's MSRV is far below).
- `#![forbid(unsafe_code)]`: unaffected.
- No public API change anywhere in this item (private `keep_*` helper
  signatures only).

## 5. Coverage-gate interaction

- **No tests are added or changed.** The logging arms join the deck-client
  precedent (finding 7) as deliberately uncovered: asserting them would
  require asserting log text (AGENTS §5 forbids; D-g). The accepted gap
  is recorded here and echoed in commit 2's body; no coverage-ignore
  tooling exists to mirror (and `coverage(off)` is unstable — not used).
- **llvm-cov**: the macro expansions add regions whose enabled-branch
  never executes under the test harness (no subscriber at `trace` in
  tests), so region % dips marginally. No hard gate exists (finding 8:
  codecov `fail_ci_if_error: false`, SonarCloud non-required). The
  99.74% line figure from the domain coverage report is expected to hold
  approximately — the events sit on already-executed lines.
- **Weekly mutants job**: macro arguments are not mutation targets by
  default; `taskboard-state`'s existing tracing call sites under the same
  gate produce no mutants (finding 9). Stage 2 spot-checks with a scoped
  local `cargo mutants -p taskboard-domain --in-place` only if runtime
  permits; the weekly CI run is the authoritative gate — a new survivor
  there is a follow-up finding, not a blocker for this branch.
- **Miri weekly job (WI-C's scope)**: `tracing`'s disabled-path code runs
  under every existing domain test; no new unsafe or FFI; no interaction
  expected beyond the job's existing tuning.

## 6. Worked example — one entity end-to-end at a `trace` filter (acceptance demonstration)

Scenario: bound local task "buy milk", tombstoned locally at 10:05; a
remote title edit lands at 10:06; the next completed pull ingests it.
With the app's subscriber filtered to `RUST_LOG=taskboard_domain=trace`
(trace shows debug too), the sequence a bug report would carry:

```text
DEBUG apply_sync_report{outbox_len=1 pushes=0 snapshot_stacks=1 snapshot_tasks=1}: sync report entered board=<ref> read_at=… now=10:07
TRACE merge_task: task merged entry task_id=<uuid> card=<board,stack,card> remote_ts=10:06 remote_seen=10:00 title="buy milk"
DEBUG merge_task: task field verdicts rule=R4 task_id=<uuid> remote_ts=10:06 title_remote_wins=true description_remote_wins=false duedate_remote_wins=false done_remote_wins=false archived_remote_wins=false labels_remote_wins=false position_remote_wins=true position_stack_resolved=true
DEBUG merge_task: R5 resurrect rule=R5 task_id=<uuid> outcome=resurrected remote_ts=10:06 deleted_clock=10:05
DEBUG apply_sync_report: resurrected task delete op dropped rule=R5 op_id=<uuid>
DEBUG apply_sync_report: sync report applied actions=1 ops_completed=0 ops_failed=1 remaining_ops=0 phase=Failed(BadRequest)
```

…plus, where a post-read local edit was protected, the
`self_clobber_guard` event naming the restored fields. Every line is
reachable from the entity's id; nothing above `trace` carries the title.
This walkthrough is the plan-level demonstration the dispatch asks for —
**not** asserted by any test (D-g). (`phase=Failed(BadRequest)` there is
existing behavior: the dropped delete op counts as dead-lettered.)

## 7. Implementation phases (stage 2) — three standalone commits

1. **`chore(domain): adopt the workspace tracing pin and amend the documented dependency cap`**
   — Cargo.toml + architecture.org wording (§4). Verification: `cargo
   check -p taskboard-domain`, `cargo deny check`, fmt/clippy. Stands
   alone: a dependency with no users yet, docs-consistent.
2. **`feat(domain): instrument the sync conflict policy (R1-R9) with tracing`**
   — merge.rs + pipeline.rs per §3 (inventory, conventions, consts,
   module-doc paragraphs, `keep_*` id params). Verification: full gate
   suite; **all existing tests pass unchanged** (the behavior-neutrality
   net, D-g); the §6 walkthrough sanity-checked once via a scratch
   binary/example run at `trace` (manual, not committed — or a
   `cargo run -p taskboard` REPL session with `RUST_LOG` set; stage 2
   picks whichever is cheapest and records it in the report).
3. **`docs(backlog): mark the domain tracing pass done`**
   — the "[2026-10-09] Logging statements for taskboard-domain" entry
   ✅ DONE with a one-line note (dep added with `log` per guidelines §4;
   78 sites; accepted uncovered logging arms per deck-client precedent).

Gates before handoff (AGENTS §10): `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo nextest run --workspace`, `cargo test --doc --workspace`,
`cargo deny check`, `cargo llvm-cov --workspace`.

## 8. Risks

1. **Coverage optics** (accepted): a marginal region dip from
   never-enabled macro branches; no hard gate; documented here and in the
   commit body (§5).
2. **Mutants watch item**: if a future cargo-mutants release starts
   mutating macro arguments, the boolean field expressions could sprout
   trivial survivors; mitigation is the weekly job's watch, not this
   branch.
3. **Debug volume on large boards**: bounded to changed entities + push
   outcomes per cycle (fast paths silent, D-d); trace volume is unbounded
   by design but only on request.
4. **Rule-tag typos are invisible to tests** (no text assertions, D-g);
   mitigated by the single-definition consts (D-e) and review.
5. **Backlog-vs-guideline conflict resolved against the backlog** (D-a):
   recorded here so the deviation from the entry's "no features" phrasing
   is a documented decision, not a silent one.
6. **`log`-facade double-emit edge**: with no subscriber active *and* a
   `log` logger installed, events forward to the facade once — single
   emission either way; in-app the subscriber makes the fallback inert.

## 9. Acceptance-criteria mapping (dispatch §4 WI-F)

| Criterion | Where |
|---|---|
| `trace` filter follows one entity's merge end-to-end | §6 worked example |
| Dep-cap amendment lands | §4.2 wording, commit 1 |
| Workspace-pinned tracing, feature decision documented | §2 D-a, §4.1 |
| No log-text assertions; uncovered arms per precedent | §2 D-g, §5 |
| `cargo deny` green with tracing's license tree | §4.3 |
| Gates green; backlog entry ✅ DONE | §7 phases + gate list |

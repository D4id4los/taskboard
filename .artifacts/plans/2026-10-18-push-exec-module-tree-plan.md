# [Plan] `push_exec` Module-Tree Decomposition

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 10 of the remediation-wave-1
  dispatch (dispatch §1/§11, prompt 10.1). Planning only — no
  implementation in this commit.
- **Origin**: issue **#247** (normative; "Proposed Approach:
  `push_exec/{overlay,execute,outcomes}` module tree — mechanical, no
  API change"), findings report row **F6-10** (P2, axis E) + agenda
  row **E-2** (module tree, M — promoted to now by the session).
  Owning report:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md`
  (§secondary factoring — "`push_exec.rs` weight"; F6-10 row).
- **Branch / worktree**: `refactor/push-exec-module-tree` in
  `../taskboard-WT-refactor-push-exec-module-tree`, cut off `main` @
  `53b533c` (= `origin/main`).
- **Anchor base**: every `file:line` anchor below is against
  `53b533c` — the post-chunks-2/5/6 shape (3 376 lines; the audit's
  2 678 grew by chunk 2's pinning tests and chunk 5's reresolve tests
  + fix). Chunk 9 (`refactor/sync-client-helpers`) is **not yet
  implemented or merged** at planning time — see §ordering.
- **Size**: M (per the dispatch's grade).

---

## §shape — what this chunk does (and only this)

`crates/taskboard-sync-nextcloud/src/push_exec.rs` becomes the module
directory `src/push_exec/` with the issue-normative three production
modules plus the relocated test suite:

```
src/push_exec/
├── mod.rs       module docs + `mod` decls + re-exports (the tree anchor)
├── overlay.rs   BindingOverlay — binding math (stratum 1)
├── execute.rs   PushExecutor — the decision table + task arms (stratum 2)
├── outcomes.rs  GroupOutcome + outcome assembly + classification (stratum 3)
└── tests.rs     the in-file #[cfg(test)] suite, moved wholesale
```

The audit identifies the three production strata as: `BindingOverlay`
(binding math), `execute`'s per-`MaterializedOp` decision table (+ the
per-kind task arms it delegates to), and the outcome-assembly helpers
(`primary_outcome`/`all_applied`/`classify` et al.). The split is
mechanical; the crate's `pub use` surface does not change.

## §inventory — per-module symbol inventory (anchors @ 53b533c)

### `push_exec/mod.rs` — after the split

| Content | Source (old `push_exec.rs`) |
|---|---|
| module doc header `//!` (the executor overview incl. the #258 note) | :1-13, verbatim (one intra-doc link adjusted, §mechanics) |
| `mod overlay;` `mod execute;` `mod outcomes;` (all **private**) | new |
| `pub use overlay::BindingOverlay;` `pub use execute::PushExecutor;` `pub use outcomes::GroupOutcome;` | new (keeps `crate::push_exec::*` paths stable) |
| `#[cfg(test)] mod tests;` | new (points at `tests.rs`) |

Everything else moves out. `mod.rs` ends at ~30 lines.

### `push_exec/overlay.rs` — binding math (old :29-79)

| Symbol | Kind | Visibility old → new |
|---|---|---|
| `BindingOverlay` | `pub struct` (fields `board`, `stack_by_local`, `task_by_local`, `label_by_local`) | `pub` unchanged; fields private → **`pub(super)`** (§visibility) |
| `BindingOverlay::from_index` | `pub fn` (:43) | `pub` unchanged |
| `BindingOverlay::board` | `pub const fn` (:66) | `pub` unchanged |
| `BindingOverlay::card_of` | private `fn` (:70) | → **`pub(super)`** |

Own imports: `std::collections::HashMap`,
`taskboard_domain::{RemoteBoardId, RemoteIndex, RemoteCardRef}` (the
field/param types written `taskboard_domain::`-qualified stay
qualified verbatim; `RemoteStackId`/`RemoteLabelId` appear only in
those qualified paths).

### `push_exec/execute.rs` — the executor (old :98-889 minus helpers)

| Symbol | Kind | Visibility old → new |
|---|---|---|
| `PushExecutor<'a>` | `pub struct` (:100) | `pub` unchanged |
| `PushExecutor::new` | `pub fn` (:108) | `pub` unchanged |
| `PushExecutor::overlay` | `pub const fn` (:114) | `pub` unchanged |
| `PushExecutor::execute` | `pub async fn` (:123) — the `MaterializedOp` decision table: `Noop`, `CreateTask`, `UpdateTask`, `MoveTask`, `DeleteTask` arms + the inline stack/board/label arms (`CreateStack` :148, `RenameBoard` :178, `RenameStack`\|`MoveStack` :212, `DeleteStack` :244, `CreateLabel` :269, `UpdateLabel` :307, `DeleteLabel` :343) | `pub` unchanged |
| `PushExecutor::create_task` | private `async fn` (:376) | private (now module-private to `execute.rs` — same reach, all callers in-file) |
| `PushExecutor::fetch_card_for_write` | private `async fn` (:534) | same |
| `PushExecutor::update_task` | private `async fn` (:603) | same |
| `PushExecutor::move_task` | private `async fn` (:810) | same |
| `PushExecutor::delete_card` | private `async fn` (:861) | same |

The `#[allow(clippy::too_many_lines)]` attributes on `execute`
(:122), `create_task` (:375) and `update_task` (:602) travel with
their functions (dropping one fails the clippy gate).

Own imports: the old file's `taskboard_domain::{CardShape,
EntityTables, LocalOp, MaterializedOp, PendingOp, PushGroup,
PushOutcome, PushResult, RemoteBoardId, RemoteCardRef, RemoteEcho,
RemoteStackId}` slice actually referenced, `crate::client::DeckClient`,
`crate::color::DeckColor`, `crate::error::DeckError`,
`crate::mapping::{self, PushCtx}`, `crate::model::{Card, StackFilter}`,
`use super::overlay::BindingOverlay;`, `use
super::outcomes::{all_applied, cannot_ride_a_task_group, classify,
is_transport, primary_outcome, reject_kind, reject_primary};`.

### `push_exec/outcomes.rs` — outcomes & classification (old :81-96, :891-985)

| Symbol | Kind | Visibility old → new |
|---|---|---|
| `GroupOutcome` | `pub enum` (:84; `Outcomes`, `Aborted { landed }`) | `pub` unchanged |
| `cannot_ride_a_task_group` | associated `fn` (:896) | → free `fn`, **`pub(super)`** |
| `primary_outcome` | associated `fn` (:914) | → free `fn`, `pub(super)` |
| `all_applied` | associated `fn` (:928) | → free `fn`, `pub(super)` |
| `reject_primary` | associated `fn` (:940) | → free `fn`, `pub(super)` |
| `reject_kind` | associated `fn` (:949) | → free `fn`, `pub(super)` |
| `classify` | associated `fn` (:962) | → free `fn`, `pub(super)` |
| `is_transport` | `pub(crate) fn` (:980) | → **`pub(super)`** (grep-verified zero users outside `push_exec.rs` @ 53b533c) |

**Decision — free functions, not a split `impl PushExecutor`** (the
one structural choice the split forces): all six helpers are
`self`-less pure functions over `(group, result | err, ctx)` — they
are associated only by file-history accident. Keeping them associated
would mean an `impl PushExecutor` block inside `outcomes.rs`, i.e. the
executor's definition scattered across two modules — invisible
accretion, the exact disease F6-10 diagnoses, and an SRP finding under
the dispatch's code-quality mandate ("module boundaries = the SRP
lens"). Free functions make the dependency direction explicit:
`execute → {overlay, outcomes}`; `overlay → domain`;
`outcomes → {domain, error, mapping}`. No module cycle exists (and
Rust permits sibling references regardless — the tree is a clean DAG
by construction, not by compiler force).

Cost, enumerated (§mechanics C3): dropping the `Self::` prefix at
**45 call sites** — `Self::classify` ×15, `Self::primary_outcome`
×11 (one inside `classify` itself, old :965), `Self::all_applied` ×9,
`Self::reject_primary` ×6, `Self::cannot_ride_a_task_group` ×2,
`Self::reject_kind` ×2. No other symbol-level edit exists in the
chunk.

Own imports: `taskboard_domain::{PushGroup, PushOutcome, PushResult}`,
`crate::error::DeckError`, `crate::mapping::{self, PushCtx}`.

### `push_exec/tests.rs` — the suite (old :987-3376, moved wholesale)

The `#[cfg(test)] mod tests { … }` block becomes `tests.rs` (body
verbatim; the `mod` wrapper line and its closing brace are replaced by
the `#[cfg(test)] mod tests;` declaration in `mod.rs`). It contains
the shared fixture helpers (`uid`…`overlay_with`, `refused_client`,
`serve_one_create`) and all 50 tests — including chunk 2's pinning
tests (defer-only-labels :1624, subsumed-label resolvability :1675,
move fast-path pair :1766/:1828) and chunk 5's reresolve suite
(:1957-2324).

**Decision — one test module, not per-module test splits**: the
fixtures (`overlay_with`, `tables_with*`, wiremock setup) are shared
across every test category; splitting by production module would
force a fixture-support module — a new abstraction, explicitly out of
scope (§non-goals). One behavior-suite module is one responsibility.

**Import fix (the only test-side edit)**: `use super::*;` today
silently relies on the parent module's *private* `use` aliases
(`HashMap`, the `taskboard_domain` names, `DeckClient`,
`DeckColor`, …). After the split `super` is the bare `mod.rs`, so the
module's own use-block (:990-994) must grow the names the suite
actually references (grep-census @ 53b533c):
`taskboard_domain::{CardShape, EntityTables, LocalOp, MaterializedOp,
PendingOp, PushGroup, PushResult, RemoteBoardId, RemoteCardRef,
RemoteEcho, RemoteIndex, RemoteLabelId, RemoteStackId}` — 13 names.
NOT imported (zero uses in the suite — importing them fails
`-D warnings`): `HashMap`, `DeckColor`, `DeckError`, `mapping`,
`PushCtx`, `Card`, `StackFilter`, `PushOutcome`. The three production
types keep arriving via `use super::*` through the `mod.rs`
re-exports.

## §visibility — the policy

1. **Crate surface unchanged**: `lib.rs:38` (`pub use
   push_exec::{BindingOverlay, GroupOutcome, PushExecutor};`) and
   `lib.rs:23` (`pub mod push_exec;`) are untouched; `actor.rs:36`
   (`use crate::push_exec::{…}`) resolves through the `mod.rs`
   re-exports. No file outside `src/push_exec*` changes.
2. **Module-private by default**: `mod overlay; mod execute; mod
   outcomes;` are private module declarations; their names are not
   part of any path outside the tree.
3. **The split forces exactly four visibility edits** (recorded, the
   chunk's only modifier changes): `BindingOverlay`'s four fields and
   `card_of` widen private → `pub(super)` (the executor's arms and
   `fetch_card_for_write`/`create_task` mutate/read
   `stack_by_local`/`task_by_local`/`label_by_local` directly — old
   :158, :287, :408, :580 — and call `card_of`, old :144/:610/:816);
   the six outcome helpers take `pub(super)` as free fns;
   `is_transport` tightens `pub(crate)` → `pub(super)` (no external
   users). No insert/extend methods are added to `BindingOverlay` —
   that would be a new abstraction (§non-goals); the direct field
   writes move as-is under the widened visibility.

## §mechanics — move mechanics & commit series

**Doctrine**: pure moves first, minimal forced fixes riding the same
commit. Prompt 10.1 phrases this as "pure move commits + one
import-fix commit per move", but a move commit whose import fixes lag
in a follow-up does not compile — and AGENTS §8 requires every commit
to stand alone (rebase-merge hygiene). **Recorded deviation**: one
commit per move, carrying its forced fixes (visibility markers,
imports, `Self::`-prefix drops), each enumerated in the commit body so
review can diff-follow: per-commit, the new file's content must match
a contiguous slice of the old file byte-for-byte except for the SPDX
line, a one-line module doc, the use-block, the `pub(super)`
markers, and (execute.rs only) the dropped `Self::` prefixes.

Every commit compiles and passes `cargo fmt --check` + `cargo clippy
-D warnings`; the full AGENTS §10 suite runs green at the branch tip
(stage 2), and per-commit `cargo nextest run -p
taskboard-sync-nextcloud` stays green throughout (the moved tests are
the proof of behavior-neutrality at every step).

| # | Commit | Contents |
|---|---|---|
| C1 | `refactor(sync): move push_exec.rs to a module directory` | `git mv src/push_exec.rs src/push_exec/mod.rs` — path-only rename, 100% similarity, trivially green. |
| C2 | `refactor(sync): extract push_exec::overlay` | Move old :29-79 → `overlay.rs` (SPDX + one-line doc + own imports); 4 fields + `card_of` → `pub(super)`; `mod.rs` gains `mod overlay;` + `pub use overlay::BindingOverlay;` and loses the moved block. Green: the executor impl (still in `mod.rs`) sees `pub(super)` items — note a *parent* module cannot see a child's privates, which is exactly why the widening must ride this commit. |
| C3 | `refactor(sync): extract push_exec::outcomes` | Move old :81-96 + :891-985 → `outcomes.rs` (helpers become free `pub(super)` fns; `is_transport` tightened `pub(super)`); `mod.rs` gains `mod outcomes;` + `pub use outcomes::GroupOutcome;` + a temporary `use self::outcomes::{…}` and drops the `Self::` prefixes at the 44 in-impl call sites (the 45th, old :965, moves with `classify`). Green: same-compilation proof as C2. |
| C4 | `refactor(sync): extract push_exec::execute` | Move old :98-889 (struct + impl minus helpers) → `execute.rs`; the C3 `use` line relocates there; `mod.rs` gains `mod execute;` + `pub use execute::PushExecutor;` and is now the bare anchor. |
| C5 | `refactor(sync): move the push_exec test suite to tests.rs` | Move old :987-3376 body → `tests.rs`; `mod.rs` gains `#[cfg(test)] mod tests;`; tests.rs use-block grows the 13 domain names (§inventory). Suite green — this commit is itself the chunk-2-floor proof. |

New-file preamble (the only added prose in the chunk): the crate's
SPDX header (`// SPDX-License-Identifier: MIT OR Apache-2.0`, AGENTS
§6 — mandatory for every new `.rs` file) plus one `//!` line naming
the module's stratum. `tests.rs` likewise. One intra-doc-link edit:
the `mod.rs` header's `[`PushGroup`]` link (old :2) loses its bracket
form (`` `PushGroup` ``) because `PushGroup` is no longer imported in
`mod.rs` (a broken rustdoc link is a warning even where un-gated;
plain-code-span is the minimal fix). `overlay.rs`'s `[`RemoteIndex`]`
link keeps resolving (imported there).

## §neutrality — the behavior-neutral proof obligations

The exhaustive list of allowed non-verbatim edit classes (everything
outside these in the whole-branch diff is a stage-3 finding):

1. new-file preambles (SPDX + one-line doc) and per-file use-blocks;
2. `mod`/`pub use` declarations in `mod.rs` (incl. the lib-unchanged
   re-export trio);
3. the four visibility markers of §visibility.3;
4. the 45 dropped `Self::` prefixes (C3);
5. the tests.rs import-list addition (C5);
6. the one intra-doc-link fix in `mod.rs`.

No statement, arm ordering, attribute, comment body, or test body
changes. `cargo fmt --all -- --check` must be a no-op on the moved
text (it was formatted before). Stage-3 verification:
`git diff main...HEAD --find-renames --find-copies` + per-commit
review of the non-rename remainders against this list; grep proofs —
`pub use push_exec::{BindingOverlay, GroupOutcome, PushExecutor};`
still the sole crate-surface line (plus the pre-existing lib.rs
lines, byte-identical); `git grep -n "Self::classify\|Self::primary_outcome\|Self::all_applied\|Self::reject_"` returns nothing.

## §floor — the tests that must stay green, unmodified

Chunk 2's six pinning tests (three in `push_exec`'s suite — defer,
subsumed labels, the move fast-path pair; three wiremock pull arms in
`tests/actor_wiremock.rs`), chunk 5's reresolve suite, and the crate's
wiremock/contract/e2e tiers. The in-file ones move byte-identical
(their bodies are the neutrality proof); the `tests/`-dir suites never
reference `push_exec` items directly (grep-verified) and cannot
notice the split.

## §ordering — chunk 9 interaction (rebase expectation)

The dispatch chain orders 2 → 6 → 9 → 10 in the sync crate. Chunks
2/5/6 are merged into `main` (PRs #315/#320/#321); **chunk 9 is at
stage-1 only** (branch exists, plan committed, no implementation, no
PR). This chunk is therefore cut off `main` per the prompt, and
**expects a rebase** once chunk 9 lands: 9's plan touches exactly two
regions inside this tree — `is_transport` (old :980-985 + callers
:493/:509 → deleted in favor of `DeckError::is_transport()`; lands in
`outcomes.rs`/`execute.rs` here) and the `update_task` label-delta
failure arms (old :703-756 → `execute.rs` here). Both are small
textual conflicts with a mechanical resolution: keep the module
layout, apply 9's edit at the new location. If 9's PR is open before
stage 2 starts, rebase this branch onto it first (`git fetch origin
--prune; git rebase --onto origin/main <tip-of-9> refactor/push-exec-module-tree`
after 9 merges, per AGENTS §8's stacked-rules analog; no PR of ours
is stacked behind 9's — both target `main` independently, so the
ordinary rebase-onto-main after its merge suffices).

## §non-goals

- **No logic edits** — zero behavior change; a logic-looking diff is a
  self-finding (prompt 10.2): revert it, file an issue instead.
- **No renames beyond what the split forces** — every type/fn/test
  name survives verbatim; the only name-shape change is the dropped
  `Self::` prefix.
- **No new abstractions** — no overlay insert-methods, no traits, no
  helper restructuring, no test-fixture module, no re-exports beyond
  the stable trio.
- **No test-suite restructuring** — one `tests.rs`, fixtures shared
  as today.
- No reformatting, no comment rewriting, no touching other files
  (`lib.rs`, `actor.rs`, `mapping.rs`, `docs/` all byte-identical —
  `docs/` carries no `push_exec` references, grep-verified).

## §risks — traps and their answers

1. **Cyclic-visibility / parent-child privacy** (the named trap): a
   parent cannot see a child module's private items, and siblings
   cannot see each other's — extracting `overlay`/`outcomes` while
   the executor impl still sits in `mod.rs` breaks compilation unless
   the `pub(super)` widenings ride the *same* commit (hence the C2/C3
   commit shapes). Within `pub(super)`, visibility is confined to the
   `push_exec` subtree — nothing leaks to the crate surface.
2. **The tests' hidden glob dependency**: `use super::*` currently
   imports the parent's *private use-aliases*; the fix is the
   enumerated 13-name import list — under-importing fails compilation
   (loud), **over-importing fails `-D warnings`** (the census in
   §inventory is exactly the guard).
3. **Lost lint attributes**: the three `#[allow(clippy::too_many_lines)]`
   markers must travel with their fns or the clippy gate fails.
4. **Chunk 9's rebase** (§ordering) — expected, mechanical, planned.
5. **`#[allow]`/doc-comment drift masquerading as moves** — stage 3
   diffs every non-verbatim remainder against §neutrality's six
   classes; anything else is a finding.
6. **Coverage/gate mechanics**: identical code + identical tests ⇒
   identical coverage; the §10 sequence (incl. the quarantine step)
   runs unchanged. `cargo nextest` handles module moves without
   snapshot or fixture impact (no `insta` snapshots exist in this
   crate's suite).

## §acceptance — mapped to dispatch §11

- "plan names the split" — §shape/§inventory.
- "`pub use` surface unchanged (no downstream edits)" — §visibility.1;
  stage 3 greps the surface line byte-identical.
- "behavior neutral — chunk 2's six tests + wiremock suites green
  unmodified" — §floor; C5 is the in-file proof point.
- "file-size delta visible in the PR body" — stage 3 reports the
  before/after counts; planning-time prediction @ 53b533c:
  `push_exec.rs` 3 376 → `mod.rs` ~30, `overlay.rs` ~60, `execute.rs`
  ~830, `outcomes.rs` ~130, `tests.rs` ~2 395.

# [Plan] App/Storage Hygiene Chunk (C6: #865 + #876 + #877)

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan — C6 of the backlog-reduction round
  ("hygiene r2"); normative sources:
  `.artifacts/plans/2026-11-17-backlog-reduction-round-plan.md` §3-C6
  (the option-2 and shared-helper decisions bind) and the dispatch
  plan §1/§7; issues #865, #876, #877.
- **Branch**: `test/app-storage-hygiene` (worktree
  `taskboard-WT-test-app-storage-hygiene`, cut off `main` @ 540d2776).
- **Stage-2 sequencing note**: the dispatcher orders C6 **after the
  wave**, and the wave branches (and this branch's eventual rebase
  target) contain the #708 sidecar-author lineage
  (`e7bde8c9`..`44c6f32b`), which `main` @ 540d2776 does **not**.
  Stage 2 therefore **rebases onto the then-current `origin/main`
  first** and executes on that tree. All line numbers below were
  read on 540d2776; the post-sidecar tree shifts them (verified on
  `ci/cargo-mutants-capped`, which contains the sidecar commits):
  the two `author` params sit at pipeline.rs:461/1785 with
  `#[allow(unused_variables)]`, the fixtures carry extra "strips
  0026" edits (`b06426c6`, `9512e408`, `44c6f32b`).

## 1. Scope

Three independent work items, one PR:

1. **#865** — the ~12 ambient-sensitive `AppConfig::load` unit tests
   reroute to the file-layer decode (`extract_file_only` /
   `extract_raw`); exactly one integration twin keeps the composed
   (file + env) load covered, under the existing env lock. Round-plan
   decision: **option (2)**, binding.
2. **#876** — drop the dead `author` parameter from
   `apply_sync_report` / `apply_files_board_snapshot`, their
   `#[allow(unused_variables)]` attrs, and every call site's trailing
   argument. Mechanical, compiler-driven.
3. **#877** — shared migration-rewind fixture helper
   (`rewind_to(pool, N)`) with a **declarative strip table generated
   from the migration ledger** (runtime, via `MIGRATOR.iter()`);
   cross-check test is the recorded fallback only. All rewind
   fixtures migrate onto it (more than the four named — see §4);
   a throwaway migration demonstrates the zero-edit property; the
   evidence is recorded on #877.

## 2. #865 — composed-load coverage moves to the integration layer

### 2.1 Sweep result (the ambient-sensitive inventory)

All in-crate unit call sites of `AppConfig::load(Some(..))` that
compose figment's **env provider** over the file (edition-2024
`forbid(unsafe_code)` forbids env mutation here — the recorded house
pattern, `ui_requests.rs` "the real env scan is pinned in the
integration suite"):

| File | Tests | Line (540d2776) |
|---|---|---|
| `crates/taskboard-app/src/connections.rs` | `upsert_appends…`, `upsert_replaces_the_named_entry…`, `upsert_replaces_in_an_inline_table_array…`, `upsert_creates_the_subtree_in_a_sync_less_file…`, `upsert_into_an_absent_file…`, `upsert_creates_the_subtree_beside_a_credential_store_husk…`, `ui_overlay_writers_write_only_their_key_and_round_trip`, `upsert_accepts_a_credential_store_husk_beside_the_array` (8 `load` round-trip assertions) | 423, 443, 469, 526, 552, 601, 617, 626, 659, 683 |
| `crates/taskboard-app/src/commands/config.rs` | `us10_s1_config_set_writes_the_toml_layer_and_rereads` (the `AppConfig::load(..).is_ok()` leg; the primary assertion already reads `extract_file_only`) | 469 |
| `crates/taskboard-app/src/migration.rs` | `migrate_config_file…` scalar-world pin (asserts `sync.connectors.is_none()` on a loaded config) | 667 |

= **12 unit tests** (issue #865 named 3; the sweep found the rest in
`connections.rs` and `migration.rs`). #865's originally-named
`commands::connect::tests::the_overlay_rewrites_entry_0_of_an_*`
already read `extract_raw` on this tree (`connect.rs:714`, `:936`,
`:976` are the remaining `load` users in connect.rs — none assert
env-composable *file-layer* facts beyond validity; they reroute with
the rest). Production `load` call sites are untouched — they are the
composed-load path by design.

### 2.2 Decisions

- **D-865-a (reroute)**: every test above swaps
  `AppConfig::load(Some(p))` → `AppConfig::extract_file_only(Some(p))`
  (or keeps `extract_raw` where the test already used it and only its
  *validity* leg used `load`). `extract_file_only` is the right oracle:
  these tests assert what the *file* now contains; env composition is
  not their behavior. Typed outcomes preserved (`Result`/field values);
  no text assertions introduced.
- **D-865-b (twin home & scenario)**: the twin lands in
  `crates/taskboard-app/tests/config.rs` (the C-series composed-load
  suite, which already owns `taskboard_env_lock`). Scenario
  `composed_load_env_overrides_the_file_layer`: write a temp toml with
  `nextcloud.server_url = "https://file.example"`, under
  `taskboard_env_lock()` set `TASKBOARD_NEXTCLOUD__SERVER_URL=https://env.example`,
  assert `AppConfig::load(Some(p))` yields the env value while
  `extract_file_only(Some(p))` yields the file value — one test pins
  both that composition happens and that the file layer alone does not
  see env. **Exactly-once law (round law c)**: this is the suite's only
  composed-load-via-file test; the existing C-series tests (C2
  precedence etc.) construct configs without a file or cover failure
  classes — the twin fills the round-trip gap the unit tests vacate;
  no coverage is deleted, only re-homed.
- **D-865-c (rejected, round-recorded)**: no env-scratching test crate
  (reintroduces env mutation to escape `unsafe`), no per-test
  accept-and-document comments (that was the interim the issue exists
  to close).

- **D-865-d (recorded residual, #897)**: `extract_raw` **composes
  env** (defaults → toml → `TASKBOARD_*`, `config.rs:1292`) — the
  existing `extract_raw` *validity-only* unit legs (connect.rs
  "still decodes" / "round-trips and validates") stay exposed to
  *undecodable* env garbage in principle. The issue's hostile fixture
  (a decodable `SERVER_URL`) leaves them green, so C6 does not touch
  them; the residual is filed as #897.

**Acceptance**: the hostile-env full-workspace run
(`TASKBOARD_NEXTCLOUD__SERVER_URL=https://hostile.invalid cargo
nextest run --workspace`) is green at the unit layer; the twin is
green under the same export (it sets its own env under the lock).

## 3. #876 — dead `author` parameter drop

### 3.1 Call-site inventory (post-sidecar tree; verified on a branch containing e7bde8c9)

- Signatures: `crates/taskboard-domain/src/pipeline.rs`
  - `apply_sync_report(..., author: Option<AuthorId>)` — last param,
    `#[allow(unused_variables)]` on it (:461); the debug span does not
    record author.
  - `apply_files_board_snapshot(..., author: Option<AuthorId>)` — last
    param, same attr (:1785).
- Production call sites (whole workspace): exactly **two**, both in
  `crates/taskboard-state/src/actor.rs` — the Deck leg (:1072, passes
  `author`) and the Files leg (:1135, passes `author`). No app/sync/
  storage call sites (grep-verified on the post-sidecar tree).
- Test call sites: ~85 in `pipeline.rs`'s test modules (positional
  trailing `author,` / `Some(author),` / `None,` args) plus the
  `author_journey_tests` local `fold_report` shim (:8751..8775) —
  mechanical trailing-arg removal, compiler-driven.
- Out of scope (recorded): `fold_report`'s own `author` param
  (:2802) is **live** (the author-journey convergence law consumes
  it); the files-lane `fold_*` helpers' author params are live on the
  same tree. Only the two entry-point params are dead — the issue's
  exact scope. If a later round finds the folds' params dead too,
  that is new evidence, not this chunk.

### 3.2 Decision

- **D-876-a**: drop the two params, both `#[allow(unused_variables)]`
  attrs, and every trailing argument; the
  `#[allow(clippy::too_many_arguments)]` on `apply_files_board_snapshot`
  may loosen if the count drops below the lint threshold (compiler
  decides; keep the attr only if still needed). `apply_sync_report`'s
  attr stays (the injected-seams shape still exceeds the threshold).
- **D-876-b**: no behavior assertions change; the existing
  author-journey tests pin the stored-slot semantics already. The
  chunk adds no new test for a deletion the compiler verifies
  (AGENTS §5: don't test what the type system proves).

**Acceptance**: `cargo clippy --workspace --all-targets -- -D warnings`
green with the allow attrs gone; full suite green (the actor call
sites prove no consumer remained).

## 4. #877 — the shared rewind helper

### 4.1 Fixture sweep (wider than the issue's four)

Rewind-style migration fixtures on this tree:

| Fixture | Location | Strips |
|---|---|---|
| pre-0002 (`migration_0002…`) | storage `src/connect.rs` | v2 bookkeeping (:444) |
| pre-0005 | storage `src/connect.rs` | ck_* columns, legacy tag restore, v5 row |
| pre-0006 | storage `src/connect.rs` + **`taskboard-app/tests/common/mod.rs` `write_v01x_db`** | connector column, sync_status rebuild, v6 row |
| pre-0015 | storage `src/connect.rs` | time_entries, break_rules (0018/0021 cascade), v15/18/21 rows |
| pre-0016 | storage `src/connect.rs` | command_journal tables, v22? (its comment: additive) |
| pre-0017 | storage `src/connect.rs` | habits tables, v17 row |
| pre-0021 ×2 legs | storage `tests/roundtrip.rs` (:2348, :2394) | break_rules.board_id column, v21 row |
| pre-0023 | storage `tests/roundtrip.rs` (post-`44c6f32b`) | clock/author columns, v23 row + later-strip cascade |

The #708-round edits (`b06426c6`/`9512e408`/`44c6f32b`, arriving via
the rebase) touch exactly these fixtures' strip lists — **symbol-level
overlap with the sibling branches is total for the four named
fixtures and zero elsewhere**: C2's landed stage-2 files are
`ci.yml`, `ui journeys.rs`, `journeys/support/fixture.rs`;
C3's are `cli_ipc.rs`, `ipc/client.rs`, `cli_tty.rs`; neither set
intersects C6's planned diff (config unit tests, connections.rs,
migration.rs, tests/config.rs, pipeline.rs, actor.rs, storage
connect.rs/rewind.rs, roundtrip.rs, app `tests/common/mod.rs`). The
rebase may show textual conflicts *inside the four fixtures only*
(one-line strip additions vs. the helper rewrite); resolution always
takes the helper form.

### 4.2 Design decisions

- **D-877-a (home)**: new module
  `crates/taskboard-storage-sqlite/src/rewind.rs`, exported as
  `#[doc(hidden)] pub mod rewind;`. The storage crate is
  workspace-internal; `#[doc(hidden)]` keeps it out of the rendered
  API while letting both the crate's own unit tests, its
  integration tests (`tests/roundtrip.rs`), and the app crate's
  `tests/common/mod.rs` (dev dependency path) use it. No new feature
  flag. SPDX MIT OR Apache-2.0 header; `#![forbid(unsafe_code)]`
  inherited.
- **D-877-b (API)**:
  ```rust
  pub async fn rewind_to(
      pool: &SqlitePool,
      target: i64,
      extra: &[ExtraStrip],
  ) -> Result<(), RewindError>
  pub struct ExtraStrip { pub sql: &'static str, pub why: &'static str }
  #[derive(Error)] pub enum RewindError { #[error("..")] Sql(#[from] sqlx::Error), .. }
  ```
  `rewind_to` does three things in order: (1) derives the mechanical
  strips for every version > target from the **strip table**;
  (2) appends the caller's `extra` steps (the non-derivable effects —
  0005's legacy-tag restore, 0006's singleton rebuild-and-reseed);
  (3) deletes `_sqlx_migrations` rows with `version > target`.
- **D-877-c (the declarative table, generated from the ledger)**: the
  table is **derived at runtime from `MIGRATOR.iter()`** — each
  migration's embedded SQL is parsed with a conservative grammar
  recognizing exactly:
  - `CREATE TABLE [IF NOT EXISTS] <name>` → `DropTable(name)`
  - `ALTER TABLE <t> ADD [COLUMN] <c> …` → `DropColumn { table, column }`
  Anything else in a migration (indices, triggers, `UPDATE`
  backfills, rebuilds) is **not** mechanically strippable and is
  surfaced by the cross-check test (below) as "version V carries
  unstrippable statements — a fixture rewinding past V needs an
  `ExtraStrip` or a grammar extension". This *is* "generated from the
  ledger where feasible"; the fallback (a hand-maintained `const`
  table + cross-check test against the ledger) is taken only if
  stage 2 records a concrete parser-infeasibility finding (e.g. a
  migration whose strippability depends on execution order the
  grammar cannot see) — the finding goes in the plan comment and on
  #877.
  **Corollary law (documented at the parser)**: new migrations must
  keep their schema-touching statements in the recognized shape, or
  teach the grammar in the same commit — the cross-check test makes
  any violation loud at `nextest` time, which is precisely the
  #877 failure mode (found only by a full-suite run) closed.
- **D-877-d (the cross-check test, always on)**:
  `rewind_table_covers_the_ledger` — walks `MIGRATOR.iter()`, asserts
  every migration parses into the table or is explicitly listed in a
  `MANUAL: &[i64]` (today: none — 0006's rebuild is fixture-side
  `extra`, not a table gap), and asserts the derived strips of e.g.
  `rewind_to(·, 14)` against the known 0015/0018/0021 cascade. This
  test is the permanent canary; the throwaway migration (D-877-e) is
  the one-time human-visible evidence.
- **D-877-e (throwaway-migration evidence, kept)**: stage 2 adds
  `0027_throwaway_canary.sql` (an `ALTER TABLE boards ADD COLUMN
  canary TEXT`), runs the migrated fixtures + the cross-check test
  **unmodified** (they now rewind past 0027 with zero edits), records
  the run on #877, then **removes the file in a separate commit**
  before hand-off ("the evidence run, then the canary removed; the
  cross-check test is the standing canary" — stage-1's call per the
  round plan's "or kept as a canary per stage-1's call": the test
  already guarantees the property; a second, SQL-file canary would be
  dead weight in the migrations dir forever).
- **D-877-f (fixture migration)**: all eight fixtures in §4.1 move
  onto `rewind_to`, their `sqlx::query("DROP/ALTER/DELETE …")`
  blocks replaced by the call + `extra` lists. The assertions
  (column shapes, epoch defaults, backfill semantics) do not move or
  change — round law (c): the helper changes fixture *construction*,
  never fixture *verification*. `write_v01x_db` in app
  `tests/common/mod.rs` keeps its seeded rows and its
  `#[allow(clippy::too_many_lines)]`; only its strip block is
  replaced.

**Acceptance**: the four named fixtures (plus the wider sweep) all
route through `rewind_to`; the throwaway migration needs zero fixture
edits (evidence on #877); the cross-check test pins table↔ledger
agreement; full suite green.

## 5. TDD order (stage 2)

1. #877 red: write the cross-check test against a `rewind` stub →
   implement parser + `rewind_to` to green → migrate fixtures one
   commit per file (`refactor(storage): …`), suite green after each.
2. #865 red: add the twin in `tests/config.rs` (green immediately —
   new coverage), then reroute the 12 unit tests
   (`test(app): route the config round-trips through the file layer`),
   verify the hostile-env run.
3. #876 last (pure deletion): compiler-driven sweep, clippy + suite.
4. Evidence run (throwaway migration) → record on #877 → remove.
5. Quality gates via `scripts/run-quality-gates.sh` (flock law);
   commit each standalone step (rebase-merge hygiene).

## 6. Risks

- **R1 — rebase timing**: the #876 premise holds only on the
  post-sidecar tree; if the wave has not landed when stage 2 starts,
  the stage rebases onto the then-current `origin/main` and must find
  the allow-attrs there — if they are absent, STOP and report (the
  dispatcher's sequencing slipped, not a plan failure).
- **R2 — parser brittleness**: a migration with unusual formatting
  (multi-statement ALTER, quoted identifiers) may parse wrong. The
  cross-check test fails loudly rather than under-stripping silently;
  the `MANUAL` list is the escape hatch. Under-stripping in a fixture
  surfaces at that fixture's own assert, same as today.
- **R3 — `extract_file_only` vs `load` divergence**: rerouting may
  surface configs that only validate with defaults from env (none
  known — the 12 tests write complete files). If one fails, the test
  found a real ambient dependency: fix by completing the fixture
  file, never by reverting to `load`.
- **R4 — fixture textual conflicts on rebase** (§4.1): confined to
  the four storage fixtures; resolution takes the helper form
  unconditionally.

## 7. Estimated diff

~550–650 lines across ~9 files: new `rewind.rs` ~220 (parser, table,
helper, cross-check + parser tests); 8 fixture migrations net ~−80;
#865 reroute ~+15/−12 across 3 files + twin ~+45; #876 ~−90
(2 signatures, 2 actor sites, ~85 test call sites of one trailing
token each).

## 8. Open questions → resolved decisions index

| Question | Decision |
|---|---|
| Twin home / scenario | §2.2 D-865-b (`tests/config.rs`, env-over-file round-trip) |
| Helper API / table shape / generation | §4.2 D-877-b/c (runtime ledger-derived table; static-table fallback only with recorded finding) |
| Canary vs throwaway | §4.2 D-877-e (throwaway + standing cross-check test) |
| Which fixtures migrate | §4.1 (all eight, wider than the issue's four) |
| #876 scope boundary | §3.2 D-876-a/b (two entry params only; folds stay) |

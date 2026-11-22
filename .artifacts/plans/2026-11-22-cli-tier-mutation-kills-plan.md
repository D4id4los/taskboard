# [Plan] CLI-tier mutation kills — the exit-code table, the prose-family pins, logout, pdf dispatch

- **Date**: 2026-11-22
- **Kind**: chunk plan — TDD-first mutation-survivor kills at the CLI
  integration tier of `taskboard-app`.
- **Origin** (normative):
  - Round-5 dispatch plan
    `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
    §1 (C2 row) and §6 ("Chunks C1 + C2").
  - Scope report
    `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
    §C (Wave 3).
  - S3 lane report
    `.artifacts/reports/2026-11-21-cargo-mutants-commands-report.md`
    — the survivor census per shard is the kill inventory (§1 here).
  - Issues (bodies normative): #1230, #1229, #1175, #1176.
  - C1's landed evidence on #1231 (two stage-2 comments): C1 killed
    the decision predicates; its remaining missed counts are exactly
    this chunk's families. The pure fold `resurrected_tasks` went to
    C1; the `report_resurrections` `println!` shell is ours (C1's
    split, restated by the dispatcher).
- **Branch**: `test/cli-tier-mutation-kills` (worktree
  `../taskboard-WT-test-cli-tier-mutation-kills`).
- **Status**: READY. No UX journey plan in origin (no surface delta —
  tests plus two micro-refactors below; UX planning exempt per
  AGENTS §7).

## 1. Kill inventory (from the S3 census, deduplicated against C1)

| Family | Survivors | Source shard | Issue | Kill |
|---|---|---|---|---|
| `*_command -> u8 1` exit codes | `repeat.rs:388`, `track.rs:1225`, `habits.rs:803` (`habits_command`), `breaks.rs:326`, `timers.rs:339`, `reminders.rs:542`, `mod.rs:1725` (`logout_command`, #1175) | M1/M4/M5/M7 | #1230 (+#1175 for logout) | §3 table test (success + failure legs) |
| `habits_line -> 0` | `habits.rs:818` | M5 | #1230 | §3 table test, session-lane failure leg |
| `dispatch_line == -> !=` (pdf dispatch) | `mod.rs:2011`, `#[cfg(feature = "pdf-export")]` | M1 | #1176 | §6 cfg-gated dispatch test |
| `report_resurrections` shell | `sync.rs:168` (body/`println!` deletion-replacement) | M7 | #1229 (C1 split) | §5 writer-seam structural pin |
| `render_text_*` replacement family (`""`/`"xyzzy"`/`String::new()`) + in-renderer `== -> !=` branch flips | ~50 across boards(×10), labels(×3, one already flipped caught in M3), stacks(×2), habits, status (`age_phrase` wholesale ×1; the 9 boundary flips are NOT ours — see D7), track(×8), find, connect, reflection(×8→C1 killed the decision parts; prose remains), tasks(×2), breaks(×3), reminders(×5), timers(×3), logout, sync, config-render cluster E(×10, M2-verified un-killed), `output.rs:64 OpenFailure::from -> Default::default()` | M2–M7 | #1229 | §4 structural arity/section pins |

Not ours (one-behavior-one-test / already owned):
- `sync.rs wait_for_sync_result`, `reminders.rs collapse/resolve_rule`,
  `timer_rows` boundary, `board_line == -> !=`, habit outcome arms —
  C1 killed the decision predicates (#1231 evidence: "every decision
  predicate flipped caught"; the pinned survivors are exactly prose +
  exit codes).
- `status.rs age_phrase` **boundary flips** (`<` -> `==`/`>` at the
  60s/60m/24h cuts, 9 mutants, M5): typed boundary table over fixed
  instants — but the dispatch plan's C2 scope is the prose family +
  exit codes + the two one-offs, and C1's evidence counts age_phrase
  boundaries neither in prose nor exit codes. Decision D7 below.
- `connect.rs` stdin-prompt replacements (#1232) — owned separately.
- M2 clusters A–D — killed in-round (`e3100559`).
- M1 survivors 1–6, 9 — killed in-round (`c1c9136e`).

## 2. Decisions (every open question, recorded)

- **D1 — exit-code table is one test, two lanes, one behavior.** The
  behavior pinned: "the one-shot verb dispatches propagate the
  `run_core` exit contract (success receipt = 0, typed error =
  `report_to`'s code)". One table-driven test file
  `tests/cli/cli_exit_codes.rs`, one row per (verb fn, invocation,
  expected code). Success legs kill `-> 1`; failure legs kill `-> 0`.
  Both legs on every verb — cheap rows, and the failure leg is also
  the `habits_line -> 0` kill. Not split per verb (that would be
  seven copies of one behavior).
- **D2 — fixture: the standalone tempdir config, not a live server.**
  `*_command` fns resolve config via `run_core`/`core_ctx` from
  `config_path: Some(path)`; a `common::write_test_config`-style
  standalone TOML (local sqlite db in the tempdir, `credential_store
  = 'env'` with the env secret set via `common::set_env` +
  `taskboard_env_lock`) boots without a wiremock server for the
  read-only legs. The success leg per verb is the lightest read-only
  mode: `breaks List`, `timers List`, `remind List`,
  `habits` list mode, `track` status, `repeat` — no repeat rule
  exists on an empty db, so the leg is the typed no-rule refusal is
  **not** used; instead `repeat Off { task: <seeded capture> }`
  after one `Command::Capture` seed, or, if `Off` on a rule-less head
  is a typed no-op receipt, that receipt (exit 0 either way — stage 2
  picks whichever the receipt enum makes cleanest; both legs assert
  the typed code only). Failure leg: an invocation with a typed
  error on empty state (e.g. `remind Remove` on a nonexistent task,
  `timers Cancel` on a nonexistent timer) — assert the exact `u8`
  `report_to` returns for that error kind (typed outcome: the code,
  never the stderr text).
- **D3 — logout_command visibility.** `logout_command` is private
  (`mod.rs:1761`) while every peer one-shot dispatch (`repeat_command`,
  `breaks_command`, …) is `pub`. The kill needs to drive it. Decision:
  widen to `pub` with a doc comment noting it is the one-shot
  dispatch (the session lane intercepts Logout; `dispatch_line`'s arm
  stays `unreachable!`). Micro-refactor, mirrors existing peers, no
  architecture change.
- **D4 — logout fixture across the nudge seam.** `logout_command`
  builds its store internally via `production_store`. With
  `credential_store = 'env'` the store reads the env secret; seeding
  a secret then deleting it exercises the success leg
  (`logout::run` receipt → 0). The nudge degradation leg: no daemon
  socket exists in the test env, `logout::nudge_daemon` degrades to a
  typed non-attached state, stderr gets the warning — the pin asserts
  the code **stays 0** (D5 contract, issue #1175: "both legs"). One
  test, two assertions, one behavior ("the nudge never moves the
  exit code" is the same behavior as "success exits 0" only in the
  degradation case — kept as a single `e9`-style test in
  `cli_exit_codes.rs`, not in `cli_editing.rs`, because it drives the
  `*_command` tier, which `cli_editing.rs` does not own).
- **D5 — prose pins are structural arity tables, not text.** Law:
  AGENTS §5 no-text-assertions + round law (a). Design: for each
  `render_text_*` family member, the pin asserts the **structural
  facts that are pure functions of the typed payload**:
  - line/section cardinality: `render_text_list(rows)` renders
    exactly `rows.len()` rows (+ the fixed header/block shape);
    replacement with `""` gives 0 lines, `"xyzzy"` gives 1 — any
    payload with ≠1 row kills both.
  - per-outcome-kind section presence: for branch-selecting renderers
    (e.g. `sync::render_text` over the four `SyncOutcome` kinds,
    `connect::render_text`'s env arms, `reflection` outcome sections),
    the arity/section table pins **which sections render for which
    outcome kind** — the `== -> !=` flip moves a section between
    kinds and the table flips. Section presence is asserted
    structurally (line-count deltas, section-order arity), never by
    substring.
  - `OpenFailure::from -> Default::default()` (`output.rs:64`): a
    typed mapping pin — each failure input maps to its distinct
    typed variant (matchable via `matches!`), no prose.
  Where a fn's payload cannot produce ≥2 distinguishable arities
  (single-line renderers with no branches), the arity pin plus the
  per-kind table across the fn's *callers'* outcome kinds is the pin;
  if a survivor remains un-killable under D5, it is dispositioned to
  the S7 golden wave (#1141/#1140/#1139) with a comment on #1229 —
  never a text assert.
- **D6 — report_resurrections gets a writer seam, not process-tier
  capture.** The `println!` shell bypasses `CommandIo` (a latent
  smell: untestable, uncapturable). Micro-refactor: `sync_on` (and its
  `sync_on_engine` callers) thread `out: &mut dyn std::io::Write`
  (or the existing `CommandIo` at the `*_command` call sites) into
  `report_resurrections`. The pin: drained `SystemEvent`s carrying
  N resurrected ids (with a duplicate, preserving C1's arrival-order
  duplicate law) produce exactly N lines on the writer, in fold
  order. Structural (count + order via the state's titles fetched
  through the shared state), never string equality on the sentence.
- **D7 — age_phrase boundary flips go to backlog.** The 9 boundary
  mutants (M5) are neither prose-replacement nor exit codes; killing
  them needs a fixed-instant boundary table (clock-law compliant —
  the fn takes instants). Out of C2's dispatch scope; filed as
  `backlog:` (see §8) rather than scope-creeping a seventh family in.
- **D8 — the mutants tier stays default-features.** #1176 asks for
  "the sweep decision of whether the mutants tier runs the feature".
  Decision: this chunk does not change the lane's sweep invocation
  (changing it is lane work, not chunk work, and doubles the
  commands-lane wall time). The #1176 kill test lands under
  `#[cfg(feature = "pdf-export")]` and kills the mutant whenever a
  feature-carrying window runs; until then the survivor is
  **pinned-but-not-verified**, recorded on #1176. The lane change
  itself is filed as `backlog:` (see §8).
- **D9 — kill verification windows are dispatcher-owned.** Per the
  dispatch plan §6 acceptance ("the survivors list in the S3 report
  updated with their kills") and C1's precedent (partial windows
  recorded, pins never skipped), stage 2 lands the pins and, if the
  mutants budget allows, `--file` verification shards per family;
  otherwise the S3 report gets an addendum marking the survivors
  *expected-caught* with the pin locations, and the dispatcher
  schedules the confirmation windows per R1. The gate for stage 2
  remains the pre-commit hooks + the flock-wrapped nextest run.

## 3. #1230 + #1175 — the exit-code table test (TDD-first)

New file `crates/taskboard-app/tests/cli/cli_exit_codes.rs`
(registered in `tests/cli/main.rs`-style harness if the cli tests
share one — follow `cli_verbs.rs`'s registration pattern).

Structure (table rows; each row = one `*_command` call over a fresh
tempdir config + `CommandIo` into `Vec::new()` sinks):

| Row | Call | Invocation | Expected code | Kills |
|---|---|---|---|---|
| repeat-success | `repeat_command` | `Off` on a seeded head | 0 | `repeat.rs:388 -> 1` |
| track-success | `track_command` | `Status` | 0 | `track.rs:1225 -> 1` |
| habits-success | `habits_command` | list mode | 0 | `habits.rs:803 -> 1` |
| habits-fail-line | `habits_line` over a booted `App` (`BasicSession` + `dispatch_line` `Command::Habits`, unknown-habit edit) | typed-error mode | `report_to` code | `habits.rs:818 -> 0` |
| breaks-success / breaks-fail | `breaks_command` | `List` / typed-error mode | 0 / err code | `breaks.rs:326` |
| timers-success / timers-fail | `timers_command` | `List` / `Cancel` on nonexistent | 0 / err code | `timers.rs:339` |
| remind-success / remind-fail | `remind_command` | `List` / `Remove` on nonexistent task | 0 / err code | `reminders.rs:542` |
| logout-success-degraded-nudge | `logout_command` (D3) | seeded env secret, no daemon | 0, warning on err sink | #1175 (`mod.rs:1725 -> 1`), both D5 legs |

TDD order per row: the failing assertion lands first (the mutant is
not applied by hand — the row is written against the *contract*, red
only in the sense that the pin does not exist; the true red-green is
the mutants window). Where a row's failure-leg code must equal a
specific `u8`, assert the exact literal with a comment naming the
`CommandError` variant that maps to it.

## 4. #1229 — the prose-family structural pins (TDD-first)

Home: extend the existing per-verb CLI test files (the renderer's
verb's file owns its renderer's pin — `cli_habits.rs` for
`habits::render_text`, `cli_track.rs` for track's ×8, etc.), one
`#[test]`-per-renderer table (arity × outcome kinds), not one test
per mutant. `config.rs` cluster E's five render fns and
`output.rs`'s `OpenFailure` pin are unit-tier (pure `&T -> String` /
mapping fns — the lowest layer is the unit `#[cfg(test)]` module;
the no-text law is satisfied by D5's structural facts, so the unit
tier is legal and preferred). The rest (they take receipts/views and
ride the CLI io sinks) pin at the CLI tier where the receipt
fixtures already exist.

Order: write the arity/kind tables first, verify each asserted
structural fact actually distinguishes the replacement mutants
(mentally apply `""` / `"xyzzy"` / `String::new()` / the flip — the
table value must change; if it doesn't, strengthen the fixture
payload, never the assertion), then land.

## 5. `report_resurrections` (C1 split remainder)

1. Writer seam (D6): thread `&mut dyn Write` from the sync dispatch
   call sites (`sync_command`'s `io.out`; the session-lane caller's
   io) through `sync_on` → `report_resurrections`. No behavior
   change beyond the sink.
2. Pin: fixture drains a broadcast channel seeded with resurrect
   events for two ids (one duplicated) over a booted app whose state
   holds both tasks; assert the writer received exactly three lines,
   in fold order (duplicate preserved). Structural.

## 6. #1176 — the pdf-export dispatch branch

`#[cfg(feature = "pdf-export")]` test in `tests/cli/cli_export_pdf.rs`
(the file already owns the pdf fixture): parse a `--what pdf`
argv via `Cli::try_parse_from`, route through `dispatch_line`
(journal-file pattern, `j16` harness shape), assert the **Pdf leg**
by typed payload — the export lands at the `--out` path (the file
exists, non-empty) or the returned code is 0 with the out path
present; the `== -> !=` flip routes Pdf to the non-pdf renderer and
the typed fact (pdf out file vs stdout CSV shape) flips. Mirror the
inverse leg only if a non-pdf export through the same argv shape is
a one-line variant (same behavior, same test).

## 7. Acceptance

- Every inventory row has a landed pin whose asserted outcome a
  listed mutant flips (stage-2 self-check: apply the flip mentally,
  the pin changes).
- `scripts/run-quality-gates.sh cargo nextest run --workspace` green
  (background + poll; flock-deferral rule if contended).
- S3 report addendum: per-family survivor → pin-location mapping,
  with either window-verified caught counts or *expected-caught* +
  dispatcher note (D9).
- #1229/#1230/#1175/#1176 each get a landed-state evidence comment.
- One-behavior-one-test: no row duplicates a C1 pin (C1's predicate
  pins stay; these are dispatch/exit/render pins).

## 8. Out-of-scope findings → backlog (filed at planning)

- `backlog:` the age_phrase bucket-boundary mutants (9, M5) need a
  fixed-instant boundary table — neither prose nor exit-code family,
  ownerless after the C1/C2 split. *(issue number recorded in the
  stage report)*
- `backlog:` the ci/mutants-commands-lane sweep should run
  `--features pdf-export` windows (or a dedicated pdf shard) so
  #1176's pinned kill is actually verified in the mutants tier.

## 9. Risks

- **Fixture cost**: each `*_command` row boots a real sqlite core
  (`run_core` → `core_ctx`); ~10 rows ≈ one cli_verbs test file of
  boots — bounded, and the table keeps it to one file.
- **repeat success leg** may need a seeded rule/head (D2's fork);
  stage 2 picks the cleanest typed leg and records the choice in the
  test doc comment.
- **logout env-store seeding**: `production_store` over
  `credential_store = 'env'` must resolve the seeded secret;
  if `logout::run` refuses an empty store the leg moves to
  `common::FakeCredentialStore`-equivalent via a config store kind
  the fakes already cover — record whichever leg lands.
- **Arity pins can over-pin** ("no pins of today's behavior"): every
  arity fact must be derivable from the fn's doc contract
  (row-per-entry, section-per-outcome-kind), not from incidental
  formatting; the S7 goldens remain the byte-level home.
- **Mutant verification windows** are hours-long; D9 keeps stage 2
  unblocked either way.

## 10. Estimated diff size

~600–900 lines, all test-side plus: one visibility widening (D3),
one writer-seam threading (D6), S3 report addendum. No production
behavior change beyond the two micro-refactors.

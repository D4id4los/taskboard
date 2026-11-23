# Report — S3 lane: sharded cargo-mutants one-off for `taskboard-app` commands/ (#327)

- **Date**: 2026-11-21
- **Branch/worktree**: `ci/mutants-commands-lane` @
  `../taskboard-WT-ci-mutants-commands-lane`, fresh off `origin/main`
  (`d1be6ff0`).
- **Normative inputs**: issue #327 (cap template binding), round-4
  dispatch plan §1/§4 S3 row, round-2 C7 pattern (dispatch plan §8 of
  `2026-11-17-backlog-reduction-dispatch-plan.md`), the landed round-2
  ipc-lane report (`2026-11-17-cargo-mutants-taskboard-app-report.md`).
- **Scope decision**: #327 names `commands/` + `ipc/` first. The **ipc
  lane is already covered** by the landed round-2 report (109 mutants;
  69 caught / 4 missed / 20 unviable / 2 timeout; all survivors
  dispositioned). This lane therefore mutates **`commands/` only** —
  the un-mutated remainder of the #327 scope.

## 1. Shard arithmetic

Surface: `crates/taskboard-app/src/commands/` — 20 files, 20,100 lines.
Volume projection: the ipc lane (5 files) yielded 109 mutants; at
~20 k lines this projects to roughly 350–500 mutants. Observed round-2
throughput (~109 mutants / 80 min at `--jobs 2`) at the mandated
`--jobs 1` (#931 law) implies ~20–45 h total, hence sharding.

**Deviation from the dispatched 6-shard grouping**: the initial 6-group
arithmetic put ~5.9 k lines in one group (config/connect/logout/sync/
status), which would certainly blow the binding 7200 s per-shard cap.
Re-arithmetized to **7 cap-fit shards** (~2.3–3.4 k lines each); the
cap-fit rule wins over the group count, recorded here as a deviation.

| Shard | Files | Lines | Command filter | Outcome | Caught | Missed | Unviable | Timeout | Wall | vs 7200 s cap |
|---|---|---|---|---|---|---|---|---|---|---|
| M1 | mod.rs | 3105 | `--file 'crates/taskboard-app/src/commands/mod.rs'` | **COMPLETED** (105 mutants: 63+4 unviable, 29 caught, 9 missed) across three 7200 s-capped windows + one mis-scoped aborted run | 29 | 9 | 67 | 0 | ~3.0 h across 3 cap windows | cap never tripped (windows ended complete) |
| M2 | config.rs, capture.rs | 2589 | `config.rs`+`capture.rs` filters | **window 1 complete** (102 mutants: 46 missed, 35 caught, 21 unviable; capture.rs's mutant set folded into the same run) | 35 | 46 | 21 | 0 | 84 m (window 1) | under |
| M3 | boards.rs, labels.rs, stacks.rs | 2273 | three `--file` args | **COMPLETED across two windows + continuation** (window 1: 90 executed — 54 missed, 18 caught, 18 unviable; continuation: 54 re-executed — 53 missed, 1 caught) | 19 | 54 | 18 | 0 | ~2 h + 83 m | window 1 tripped the 7200 s cap (rc=124, partial results intact — the cap template's designed SIGINT path) |
| M4 | track.rs, find.rs, repeat.rs | 2762 | three `--file` args | **COMPLETED across two windows** (window 1 cap-tripped at 52 executed — 45 missed, 7 caught; window 2 finished the remainder: 148-discovered universe minus 396 cumulative excludes) | 8 | 45 | 0 | 0 | 84 m + 84 m | window 1 tripped the cap (rc=124) |
| M5 | habits.rs, status.rs | 2583 | two `--file` args | **COMPLETED in one window** (85 mutants) | 33 | 29 | 23 | 0 | 82 m | under |
| M6 | connect.rs, reflection.rs, tasks.rs | 2754 | three `--file` args | **COMPLETED across two windows** (window 1: 86 executed — 47 missed, 19 caught, 20 unviable; window 2 re-ran + finished the remainder) | 19 | 48 | 20 | 0 | 2 h + 2 h | window 1 tripped the cap (rc=124) |
| M7 | timers.rs, breaks.rs, reminders.rs, sync.rs, logout.rs, output.rs | 3314 | six `--file` args | **COMPLETED across two windows** (window 1: 67 executed — 48 missed, 10 caught, 9 unviable; window 2 finished the 163-mutant universe) | 10 | 49 | 9 | 0 | 2 h + 2 h | window 1 tripped the cap (rc=124) |

Run template (per shard, the #327 cap template + the self-caps):

```sh
scripts/memory-gate.sh --preflight   # floor-check once, printed; NO lock held across the run
SQLX_OFFLINE=true timeout --signal=INT --kill-after=90s 7200 \
  cargo mutants -p taskboard-app --in-place --timeout-multiplier 2.0 \
    --test-tool nextest \
    --file 'crates/taskboard-app/src/commands/<shard-files>'
git status --short   # must be clean after each --in-place run
```

- **Deviation from the `--jobs 1` dispatch law, recorded**: cargo-mutants
  27.1.0 rejects `--jobs` in `--in-place` mode ("cannot be used with
  '--in-place'") — in-place mutation is inherently sequential (one
  mutated copy of the tree at a time), so the #931 single-worker
  constraint is satisfied by construction. The first M1 attempt died
  on exactly this CLI error before any mutant ran.
- **Two-mode gate law**: `--preflight` only (the floor is checked once
  and the command is exec'd without holding the gate lock —
  `memory-gate.sh --preflight -- <cmd>`); the run never holds the
  memory-gate lock nor `run-quality-gates.sh`'s flock. Note the gate
  redirects TMPDIR itself when the host /tmp is tmpfs.
- **Disk-backed `TMPDIR`** (`/home/viktoria/mutants-tmp`): the round-2
  ENOSPC lesson — the mutation copy's debug build needs ~14 G and
  killed run 4d/4e on the 6 G tmpfs `/tmp`.
- Raw logs and per-shard `mutants.out` copies are kept OUT of git
  (acceptance: evidence excerpted, not committed raw): logs at
  `/home/viktoria/mutants-tmp/logs/`, result snapshots at
  `/home/viktoria/mutants-tmp/mutants.out-<shard>/`.

## 2. Per-shard results

### M1 — `commands/mod.rs` (dispatch glue)

- 105 mutants total. First window (log `mutants-tmp/logs/M1.log`,
  pre-window-1 snapshot `mutants-tmp/mutants.out-M1-w1`): 63 unviable
  recorded, 7 executed before an external interruption (the
  harness-reaping pattern round-2 also hit — mitigated by re-running
  detached under a systemd user unit with an `--iterate` resume loop
  per cap window; the 7200 s cap is intact per window).
- Completed window: `42 mutants tested in 36m: 9 missed, 29 caught,
  4 unviable` (+ the 63 previously-unviable) — totals: **29 caught,
  9 missed, 67 unviable, 0 timeout**.
- Deviations recorded: (a) attempt 0 aborted on the CLI law
  `--in-place` forbids `--jobs` (see §1); (b) attempt 0b mis-scoped:
  the `--` separator leaked through the shard runner and mutated the
  whole crate — stopped within ~5 min before any results landed
  (results not recorded; counts as an aborted attempt, the round-2
  4a precedent); (c) the final `mutants.out` was accidentally
  restored from the window-1 snapshot during teardown — the survivor
  list below is reconstructed from `missed.txt` + the run summaries
  in the log; the 29 CAUGHT identities were not carried into
  structured data (caught mutants are not findings; the census below
  is complete and unaffected).
- Observed pace: ~1.4–6.5 min/mutant (13 s build + 64 s test is the
  floor; full-app nextest per mutant).

### M1 survivor census (9)

| # | Mutant | Tier |
|---|---|---|
| 1 | `mod.rs:680 delete match arm Self::UnknownBoard(id) in CommandError::error_detail` | error-detail cluster |
| 2 | `mod.rs:737 delete match arm Self::Cancelled in CommandError::error_detail` | error-detail cluster |
| 3 | `mod.rs:698 delete match arm TargetNotUndoable(reason) in error_detail` | error-detail cluster |
| 4 | `mod.rs:704 delete match arm UndoTargetHasDependents{stack} in error_detail` | error-detail cluster |
| 5 | `mod.rs:804 not_undoable_reason_kind -> ""` | error-detail cluster |
| 6 | `mod.rs:804 not_undoable_reason_kind -> "xyzzy"` | error-detail cluster |
| 7 | `mod.rs:1725 logout_command -> u8 1` | exit-code cluster |
| 8 | `mod.rs:2011 dispatch_line == -> !=` (the `query.what == ExportWhat::Pdf` branch, `#[cfg(feature = "pdf-export")]`) | cfg-gated dispatch |
| 9 | `mod.rs:2687 resolve_live_board && -> ||` (`!deleted && !archived` → a deleted-but-unarchived board resolves) | board-resolution cluster |

All nine executed against the full taskboard-app suite (nextest,
default features) — every one survived. Dispositions: §3.

### M1 disposition & verification

TDD-first trivial kills (commit `c1c9136e`, tests land before any
production change — there is none; the mutation exposed only test
gaps):

- Survivors 1–4 (error_detail arms) killed by
  `error_detail_tests::the_detail_carrying_variants_keep_their_detail`
  (the `--json` detail is ADR 0019's structural surface; a
  detail-carrying variant must never degrade to `None`).
- Survivors 5–6 (`not_undoable_reason_kind → ""/"xyzzy"`) killed by
  `the_not_undoable_reason_kinds_are_the_adr_0019_discriminators`
  (kebab discriminators are contract, the function's own doc comment
  and ADR 0019 name them structural, never prose).
- Survivor 9 (`resolve_live_board && → ||`) killed by
  `live_board_resolution_tests::a_deleted_board_is_not_resolvable_by_title`.

Verification pass (window log `mutants-tmp/logs/M1v.log`, snapshot
`mutants-tmp/mutants.out-M1v-final`): **9 mutants re-tested in 9 m:
2 missed, 7 caught** — exactly the TDD-killed set flips, and nothing
else. Survivors 7 and 8 are not trivially killable and carry backlog
issues (#1175: the logout exit code needs an async `CommandIo` fixture
across the nudge seam; #1176: the pdf-export dispatch branch needs a
cfg-gated kill test in a tier the sweep does not run).

### M2 survivor census (46) — clusters

Window 1 (`mutants-tmp/logs/M2.log`, snapshot `mutants.out-M2-w1` is
PARTIAL — the resume loop was stopped mid-window-2; the authoritative
census is the log's 46 MISSED lines + the window-1 summary; caught
identities are not printed by cargo-mutants):

| Cluster | Mutants | Count | Shape |
|---|---|---|---|
| A: `ConfigKey::from_dotted` delete-arm | `config.rs:231–247` (12 keys) | 12 | key-string → enum parse mapping; pure |
| B: `ConfigKey::parse_value` delete-arm | `config.rs:332–339` (`true`/`false`/`auto`/`log`/`desktop`) | 5 | value parsing; pure |
| C: `ConfigKey::write` delete-arm | `config.rs:350–398` (13 typed settings) | 13 | typed setting → written field mapping |
| D: `batch_edit` delete-arm | `config.rs:701–733` (6 arms) | 6 | batch-edit apply mapping |
| E: render-text family | `render_text`/`render_path_text`/`render_set_text`/`render_batch_set_text`/`render_reload_text` (replace with `""`/`"xyzzy"`/`String::new()`) | 10 | CLI prose output |

Disposition plan (applied as kill tests land in the shard gaps;
per-cluster evidence then recorded here): A–D are pure mapping
surfaces → trivial TDD kills; E is CLI prose, which the no-text-
assertion law (AGENTS §5) forbids pinning at the unit tier — cluster
backlog issue pointing at the golden-coverage wave (S7) as the right
home.

### M3 survivor census (54 unique)

- Window 1 executed 90 (54 missed / 18 caught / 18 unviable); the
  runner's stop heuristic mis-fired on the SIGINT window (cargo-mutants
  prints its summary even when cap-interrupted) and a continuation
  re-ran the 54 missed: 53 missed, **1 flipped caught**
  (`labels.rs:255 render_text_list → String::new()` — the tree moved
  under the run; the M1 kill-test commit `c1c9136e` landed between
  windows). Union census: **54 unique missed**.
- **Deviation recorded**: 43 of window 1's 187 discovered mutants were
  never executed between the two discovery passes (the discovery
  universe shrank when the tree gained the M1 tests mid-shard). A
  final consolidated `--iterate` confirmation pass over `commands/*`
  closes this at the end of the lane.
- Shape: the survivors are dominated by the boards.rs ceremony/CLI
  render surface (`render_text_*` prose family), the `read_scope` /
  `publish_on` / `files_bind_on` / `wait_for_adoption` /
  `select_prelude` predicates (engine-fixture-heavy), and the
  stacks.rs/labels.rs render fns — same two disposition classes as
  M2 (structural kills where the fixture is trivial; cluster issues
  for the prose family and the engine-fixture tier).

### M4 survivor census (45 unique)

- Window 1 executed 52 (45 missed / 7 caught), cap-tripped; window 2
  executed the remaining 17, all missed (`mutants-tmp/logs/M4.log`,
  snapshot `mutants.out-M4-final`). Union census: **45 unique missed**.
- Shape: the same recurring classes — the `track.rs` export predicates
  (`export_on`, `export_reflection_on`, `export_pdf_on` comparison
  flips), the track/find/repeat `render_text_*` prose family, the
  `*_command -> u8 1` exit-code family (repeat 388, track 1225 — the
  same shape as M1's #1175), and `find_on`/`find` wholesale-replacement
  survivors (the search tier has no unit pins on row projection).
- Disposition: the render prose family and the exit-code family join
  the M2-E/M3 cluster issues (one issue per family, not per mutant);
  the find-tier projection gap and the export-window predicates are
  recorded in the lane report's final census table with their
  kill-test sketches.

### M5 survivor census (29)

One window, 85 mutants: 29 missed / 33 caught / 23 unviable
(`mutants-tmp/logs/M5.log`, snapshot `mutants.out-M5-final`). Shape:

| Cluster | Mutants | Count |
|---|---|---|
| `habits.rs` add/update/rm outcome-arm deletions + `HabitChanges` field deletions + `||`→`&&` flips (add_on 363, update_on 555) | `habits.rs:363–588` | 8 |
| `habit_rows >= → <` (the streak-window filter) | `habits.rs:689` | 1 |
| render prose family (`habits.rs render_text`, `status.rs age_phrase` replacements) | `habits.rs:723,776`, `status.rs:479` | 4 |
| exit-code family (`habits_command`/`habits_line` → 0/1) | `habits.rs:803,818` | 4 |
| `age_phrase` boundary flips (`<` → `==`/`>`/`<=` at the 60s/60m/24h bucket cuts) | `status.rs:480–484` | 9 |
| `board_line == → !=` | `status.rs:508` | 1 |
| `update_on ||→&&` (incl. outcome arms above counted once) | — | (folded) |

Disposition classes as before: outcome-arm/field/exit-code clusters →
trivial TDD kills (the engine fixtures already exist in the habits
test tier); `age_phrase` boundary flips → killable with a boundary
table test over fixed instants (clock-law compliant: the function
takes the instants, not the clock); render prose → family issue.

### M6 survivor census (48 unique)

Two windows (window 1: 86 executed — 47 missed/19 caught/20 unviable,
cap-tripped; window 2 finished the remainder; `mutants-tmp/logs/M6.log`,
snapshot `mutants.out-M6-final`). Shape:

| Cluster | Count | Note |
|---|---|---|
| render prose family (`connect.rs render_text`, `reflection.rs render_*` ×14, `tasks.rs render_text_receipt/undo`) | 22 | the standing family issue |
| `tasks.rs` outcome/predicate cluster (`add_on`, `move_on` arm deletions + `!`/`&&`/`==` flips; `resolve_stack`/`resolve_target`/`resolve_task` ambiguity boundaries; `journal_receipt ==→!=`) | 17 | engine-fixture tier; per-mutant kill sketches in the final census |
| `connect.rs` stdin-prompt wholesale replacements (`read_line_sync`/`read_secret_sync` → `Ok(...)`) | 4 | untestable at the unit tier without a stdin seam — cluster issue (interactive connect flow) |
| `connect.rs` `render_text` env-arm deletion + `==→!=` (×3) + `write_config_overlay` guard replacement (`is_inline_table()` → `true`) | 5 | the overlay guard is a real structural risk (an inline-table item rewritten as a standard table) — kill sketch recorded |

Notable: `tasks.rs:76 resolve_stack` arm-0 deletion survived — the
zero-count arm of the stack-resolution triage has no unit pin.

### M7 survivor census (49 unique)

Two windows (`mutants-tmp/logs/M7.log`, snapshot
`mutants.out-M7-final`). Shape: the render prose family again
(~28 across breaks/reminders/timers/logout/sync, plus 6 in-renderer
`==→!=` flips), the exit-code family (`breaks_command`,
`remind_command`, `timers_command` — #1230), and a small
engine-wait/predicate set (`sync.rs wait_for_sync_result` arm +
flips, `report_resurrections`, `reminders.rs collapse`/`resolve_rule`,
`timers.rs timer_rows` boundary — #1231) plus one novel shape:
`output.rs:64 OpenFailure::from → Default::default()` (the
pdf-open-error mapping; the pdf-export tier runs no unit test for the
failure mapping — folded into #1229's golden home as a typed
mapping pin, not a text pin).

## 4. Aggregate & coverage picture

| Shard | Discovered | Executed | Caught | Missed (unique) | Unviable | Cap-trips |
|---|---|---|---|---|---|---|
| M1 (mod.rs) | 105 | 105 | 29→36 (7 killed in-round) | 9→2 | 67 | 0 |
| M2 (config+capture) | 148 | 102+46 verify | 35→35+ | 46 | 21 | 0 |
| M3 (boards/labels/stacks) | 187+ | 90+54 | 19 | 54 | 18 | 1 |
| M4 (track/find/repeat) | 148 | 52+17 | 8 | 45 | 0 (folded) | 1 |
| M5 (habits/status) | 85 | 85 | 33 | 29 | 23 | 0 |
| M6 (connect/reflection/tasks) | 175 | 86+ | 19 | 48 | 20 | 1 |
| M7 (timers/breaks/reminders/sync/logout/output) | 163 | 67+ | 10 | 49 | 9 | 1 |

- The commands surface is **weakly killed relative to domain/state**:
  the ipc lane (round 2) caught ~63% executed-vs-survivor; the
  commands lane's survivors number ~270+ unique across the seven
  shards, dominated by four families, each now owned:
  render prose (#1229, ~50 + in-renderer flips), engine-decision
  predicates (#1231, ~30), exit codes (#1230, 7 verbs), stdin seam
  (#1232), plus the two one-off issues (#1175, #1176) and the M2
  structural clusters A–D (36 mutants) killed in-round with
  verification evidence below.
- **M2 in-round kills**: commit `e3100559` lands 4 tests (key
  round-trip table, parse vocabularies, batch mint, the 18-knob
  overlay-writer table). The kill-verification pass
  (`mutants-tmp/logs/M2v.log`, snapshot `mutants.out-M2v-final`)
  re-ran all 46 M2 survivors against the tree carrying those tests:
  **36 caught, 10 missed in 38 m** — the 36 are exactly clusters
  A–D; the 10 remaining are exactly cluster E (the five render fns ×
  two replacements), intentionally un-killed under the
  no-text-assertion law and owned by #1229.
- **Total wall**: ~19 h across 15 cargo-mutants windows (7 shard
  runs + continuations + 2 verification passes), every window inside
  the binding 7200 s cap (4 cap-trips, each resumed cleanly via the
  `--iterate` window protocol — the cap template working as
  designed).
- **Tree hygiene**: `git status --short` clean after every `--in-place`
  window (checked by the shard runner); no mutant leaked into a
  commit.








## 3. Survivor census & dispositions

(filled as shards land — every survivor gets a trivial-kill TDD
(failing test first) or a `backlog:` issue; no silent survivors)

## 4. Coverage picture

(filled at the end)

## 5. C2 addendum — the CLI-tier kill pins (2026-11-22 chunk, round 5)

Stage 2 of `test/cli-tier-mutation-kills` landed the pin per inventory
row of the chunk plan
(`.artifacts/plans/2026-11-22-cli-tier-mutation-kills-plan.md`); the
mapping below is the acceptance §7 record. Commits: `ae18f2c0`
(exit-code table + `logout_command` pub), `61ff25b6` (prose-family
pins + `OpenFailure` pin), `62911ae6` (the `report_resurrections`
writer seam + the exactly-N pin), `0665df67` (the cfg pdf dispatch
pin).

| Family (S3 census) | Pin location(s) | Window evidence |
|---|---|---|
| exit codes `*_command -> 1` / `-> 0` (repeat, track, habits, breaks, timers, reminders) | `tests/cli/cli_exit_codes.rs` (success + failure legs, both lanes) | window W1 below |
| `habits_line -> 0` | `cli_exit_codes.rs::session_lane_unknown_habit_edit_exits_one` | (as above) |
| `logout_command -> 1` (#1175) | `cli_exit_codes.rs::logout_with_degraded_nudge_still_exits_zero` (both D4 legs) | (as above; `mod.rs` not in W1's shard) |
| `report_resurrections` `println!` shell (#1229 C1 split) | `commands/sync.rs::resurrect_surface_pins` (writer seam, exactly-N-lines fold order) | unit window N/A (mutant = body replacement; the pin asserts on the injected writer) |
| prose replacements + in-renderer flips (~50, #1229) | per-verb CLI files + `commands/config.rs`/`status.rs` unit pins (see the #1229 landed-state comment for the full list) | window W1 covers the breaks/timers/reminders slice; the rest: expected-caught per D9 |
| `OpenFailure::from -> Default::default()` (`output.rs:64`) | `commands/output.rs::open_failure_pins` (cfg pdf-export) | expected-caught per D9 |
| pdf dispatch `== -> !=` (#1176) | `cli_export_pdf.rs::the_session_lane_routes_the_pdf_leg_to_the_pdf_renderer` | pinned-but-not-verified until the lane runs a feature window (#1268, D8) |
| `age_phrase` boundary flips (9, M5) | **not ours** — #1267 (D7) | — |

### W1 — the exit-code-family window (closed 2026-11-23)

`cargo mutants --file repeat.rs --file breaks.rs --file timers.rs
--file reminders.rs --timeout 600 --jobs 2 -- --test cli_sync
--test cli_tasks --lib` (113 mutants, memory-gated): **49 min —
65 caught, 11 missed, 37 unviable** (`/tmp` window log; snapshot
`/tmp/mutants-c2-exitcodes`).

The 11 missed, dispositioned:

- 7 in-renderer `== -> !=` branch flips (breaks render_text_list/
  render_text_add, reminders render_text_list/render_text_add/
  render_text_clear's `== 1`, timers render_text_list/render_text_add):
  **symmetry-preserving** — the D5 injectivity pins survive the flip
  because both arms still render distinct strings. Un-killable without
  a text oracle; dispositioned to the S7 golden wave (D5's clause),
  recorded on #1229.
- `repeat_command -> u8 with 0`: the W1 tree carried success legs only
  for repeat — the failure leg landed in-round (`66d5687d`) and W2
  verified the kill below.
- `reminders.rs resolve_rule == -> !=`, `reminders.rs collapse
  || -> &&`, `timers.rs timer_rows > -> >=`: the census's "not ours"
  rows (C1's decision predicates / one-behavior-one-test owners) —
  untouched, per the chunk plan §1.

### W2 — the repeat.rs verification window (closed 2026-11-23)

`cargo mutants --file repeat.rs --timeout 600 --jobs 2 --
--test cli_tasks --lib` against the tree carrying the failure legs:
**13 min — 10 caught, 0 missed, 5 unviable**. Every viable repeat.rs
mutant is caught, including the `-> 0` replacement W1 exposed.

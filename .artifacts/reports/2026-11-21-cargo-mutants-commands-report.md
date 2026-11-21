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
| M4 | track.rs, find.rs, repeat.rs | 2762 | `--file 'crates/taskboard-app/src/commands/track.rs' --file 'crates/taskboard-app/src/commands/find.rs' --file 'crates/taskboard-app/src/commands/repeat.rs'` | pending | — | — | — | — | — | — |
| M5 | habits.rs, status.rs | 2583 | `--file 'crates/taskboard-app/src/commands/habits.rs' --file 'crates/taskboard-app/src/commands/status.rs'` | pending | — | — | — | — | — | — |
| M6 | connect.rs, reflection.rs, tasks.rs | 2754 | `--file 'crates/taskboard-app/src/commands/connect.rs' --file 'crates/taskboard-app/src/commands/reflection.rs' --file 'crates/taskboard-app/src/commands/tasks.rs'` | pending | — | — | — | — | — | — |
| M7 | timers.rs, breaks.rs, reminders.rs, sync.rs, logout.rs, output.rs | 3314 | six `--file` args | pending | — | — | — | — | — | — |

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




## 3. Survivor census & dispositions

(filled as shards land — every survivor gets a trivial-kill TDD
(failing test first) or a `backlog:` issue; no silent survivors)

## 4. Coverage picture

(filled at the end)

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
| M1 | mod.rs | 3105 | `--file 'crates/taskboard-app/src/commands/mod.rs'` | pending | — | — | — | — | — | — |
| M2 | config.rs, capture.rs | 2589 | `--file 'crates/taskboard-app/src/commands/config.rs' --file 'crates/taskboard-app/src/commands/capture.rs'` | pending | — | — | — | — | — | — |
| M3 | boards.rs, labels.rs, stacks.rs | 2273 | `--file 'crates/taskboard-app/src/commands/boards.rs' --file 'crates/taskboard-app/src/commands/labels.rs' --file 'crates/taskboard-app/src/commands/stacks.rs'` | pending | — | — | — | — | — | — |
| M4 | track.rs, find.rs, repeat.rs | 2762 | `--file 'crates/taskboard-app/src/commands/track.rs' --file 'crates/taskboard-app/src/commands/find.rs' --file 'crates/taskboard-app/src/commands/repeat.rs'` | pending | — | — | — | — | — | — |
| M5 | habits.rs, status.rs | 2583 | `--file 'crates/taskboard-app/src/commands/habits.rs' --file 'crates/taskboard-app/src/commands/status.rs'` | pending | — | — | — | — | — | — |
| M6 | connect.rs, reflection.rs, tasks.rs | 2754 | `--file 'crates/taskboard-app/src/commands/connect.rs' --file 'crates/taskboard-app/src/commands/reflection.rs' --file 'crates/taskboard-app/src/commands/tasks.rs'` | pending | — | — | — | — | — | — |
| M7 | timers.rs, breaks.rs, reminders.rs, sync.rs, logout.rs, output.rs | 3314 | six `--file` args | pending | — | — | — | — | — | — |

Run template (per shard, the #327 cap template + the self-caps):

```sh
scripts/memory-gate.sh --preflight   # floor-check once, printed; NO lock held across the run
TMPDIR=/home/viktoria/mutants-tmp SQLX_OFFLINE=true \
  timeout --signal=INT --kill-after=90s 7200 \
  cargo mutants -p taskboard-app --in-place --timeout-multiplier 2.0 \
    --jobs 1 --test-tool nextest \
    --file 'crates/taskboard-app/src/commands/<shard-files>'
git status --short   # must be clean after each --in-place run
```

- **Two-mode gate law**: `--preflight` only; the run never holds the
  memory-gate lock nor `run-quality-gates.sh`'s flock — the `--jobs 1`
  + `timeout` self-caps are the protection.
- **Disk-backed `TMPDIR`** (`/home/viktoria/mutants-tmp`): the round-2
  ENOSPC lesson — the mutation copy's debug build needs ~14 G and
  killed run 4d/4e on the 6 G tmpfs `/tmp`.
- Raw logs and per-shard `mutants.out` copies are kept OUT of git
  (acceptance: evidence excerpted, not committed raw): logs at
  `/home/viktoria/mutants-tmp/logs/`, result snapshots at
  `/home/viktoria/mutants-tmp/mutants.out-<shard>/`.

## 2. Per-shard results

(filled as shards land)

## 3. Survivor census & dispositions

(filled as shards land — every survivor gets a trivial-kill TDD
(failing test first) or a `backlog:` issue; no silent survivors)

## 4. Coverage picture

(filled at the end)

# App audit p2 batch — `session_ceremonies` rename + test-scaffolding hoist (chunk 11)

- **Date**: 2026-10-25 (stage-1 plan written 2026-10-26)
- **Chunk**: 11 of the backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §12)
- **Branch**: `refactor/app-audit-p2` (worktree
  `../taskboard-WT-refactor-app-audit-p2`, created off `main` @ `cf3a117`)
- **Covers**: #253 (F8-03, rename/move `session_ceremonies`), #254 (F8-08,
  hoist the app test scaffolding)
- **Stage**: 1 of 3 (planning). No implementation, no push. Stage 2
  implements; stage 3 reviews, opens the PR, and closes both issues.
- **Fact freshness**: every line number below was re-verified on 2026-10-26
  against `main` @ `cf3a117` (the app crate has not drifted since the
  2026-10-16 audit, but its line numbers have — e.g. the IPC caller moved
  `:428` → `:455`). Re-grep at implementation time before editing.

Normative inputs: AGENTS.md (§2 boundaries, §5 testing rules, §8 git
rules, §10 gates), issues #253/#254, the audit report
`.artifacts/reports/2026-10-16-v0.1.0-audit-crate-app-report.md`
(F8-03 §G, F8-08 §H/§sonar), dispatch plan §1/§12.

Non-goals (dispatch §12, restated): zero production behavior changes;
no flaky-test fixes (chunk 12 owns #384/#408/#417); no budget averaging —
the 10 s vs 30 s divergence gets the one recorded decision in §4 below.

---

## 1. Decision D1 — rename the module; do not move `attach_from_persisted`

**Decision**: `git mv
crates/taskboard-app/src/session_ceremonies.rs
crates/taskboard-app/src/connector_ceremonies.rs` and update the module
doc header. The daemon-side body stays in the module; nothing moves
beside the IPC caller.

**Why rename (and why the issue's alternative loses)**:

1. **The shared helpers have one home and it is this module.**
   `attach_from_persisted` (`session_ceremonies.rs:239-353`) is built
   from `deck_client_from_spec` (`:219-229`, doc: "shared with the
   daemon-side attach — one behavior, one home") and mirrors
   `connect_session` (`:77-133`) step for step (detach-first swap, the
   `wait_for_boot_pull` settle at `:128-129` vs `:350-351`). Moving the
   daemon body beside its only caller (`ipc/server.rs:443-468`) would
   either drag those helpers into `ipc/` (a frontend module owning
   frontend-agnostic ceremony bodies) or leave `ipc/server.rs` importing
   ceremony internals from the old module — the coupling only changes
   direction. (Grep census, §3: `default_spec`/`deck_client_from_spec`
   have no consumers outside the module today, but they are the
   in-module seams that keep connect and attach one behavior.)
2. **The module's own design rule argues against the move.** Its doc
   (`:8-11`) records ADR 0014's rule: "verbs that mutate the session
   itself live in the session layer; the verb bodies stay
   frontend-agnostic functions." `attach_from_persisted` *is* such a
   frontend-agnostic body (no `ReplSession`, typed wire outcomes in,
   typed wire outcomes out); `ipc/server.rs` is a frontend. The IPC
   handler's own comment (`server.rs:449-454`) says exactly this: the
   body "lives … not on the wire". Moving the body into the frontend
   would contradict the rule the module exists to hold.
3. **The name was the only wrong thing.** The module hosts three
   connector ceremonies — session `connect`, session `logout`, daemon
   `attach` — across two frontends. `connector_ceremonies` names all
   three; "session" named one frontend's line-wrappers. The audit's own
   framing (F8-03): "the function is frontend-agnostic, the *name* is
   not."
4. **`connector_ceremonies` sits naturally in the existing module
   graph**: `crate::connectors` (specs, `configured_connectors`) already
   exists; the ceremonies module beside it reads as "the connectors
   domain's choreography". The issue's suggested name is adopted
   verbatim.
5. **Mechanical cost is minimal and history stays clean**: `git mv` +
   four import/comment sites + the doc header (§3). No test references
   the module path (grep census, §3).

**Doc-header rewrite (part of the rename, F8-03's actual axis)**: the
new header opens "The connector ceremonies (ADR 0014, design
§4.4/§4.6)" and states the frontend split explicitly: `connect_line` /
`logout_line` are the session (REPL) frontend's arms (they take
`&mut ReplSession`); `attach_from_persisted` is the daemon (IPC)
frontend's body; the choreographies and helpers are frontend-agnostic
functions per the ADR's rule. The existing ADR/design references and
the D10 no-nudge note carry over unchanged.

**Stale-doc rider (same commit, same axis)**: `default_spec`'s doc
(`:204-206`) still says "the session's `build_client` and the
daemon-side attach share it" — `build_client` no longer exists anywhere
in the workspace (grep-verified 2026-10-26; the only hit is that doc
line). Reword to name the real sharers: `connect_session` (the session
ceremony) and `attach_from_persisted` (the daemon ceremony).

## 2. Decision D2 — the scaffolding hoist shape

Target home: `crates/taskboard-app/tests/common/mod.rs` — the
established shared fixture module (`#![allow(dead_code)]` at module
level for per-binary subsets; already holds `binary_command`,
`write_test_config`, the wiremock fixtures). 22 of 25 test binaries
already `mod common;`.

Hoisted items (all `pub`, bodies verbatim from `cli_ipc.rs` unless
noted):

| Item | Shape | Notes |
|---|---|---|
| `wait_until` | `pub async fn wait_until(pred: impl FnMut() -> bool, what: &str)` | One budget: **30 s** (D3 below). 50 ms poll, `tokio::time::sleep` (yields to the runtime). Doc comment merged from both copies (cli_lock's copy carries the "real-time deadline — the hermetic suites' tokio-clock rules do not reach subprocesses" rationale; keep it). |
| `Child` | `pub struct Child(pub std::process::Child);` + kill-on-drop `Drop` | Verbatim tuple struct with a **public field** — both files' call sites use `daemon.0.id()` / `daemon.0.try_wait()`, so a pub field keeps every call site unchanged. The "kills the child on drop so a failing assertion cannot leak a lock-holding daemon" doc carries over. |
| `spawn_daemon` | `pub fn spawn_daemon(config: &Path, password: Option<&str>) -> Child` | The `Install::spawn_daemon_inner` body (`cli_ipc.rs:86-97`) parameterized by config path: `binary_command()` (TASKBOARD_* env hygiene) + `["--config", …, "daemon"]` + stdio null ×2 + explicit password set-or-absent. The env-scrub doc comment (`:82-85`) carries over. |

Adoptions:

- **`cli_ipc.rs`**: delete its local `wait_until` (`:19-25`) and `Child`
  (`:29-36`); `Install::spawn_daemon` / `spawn_daemon_with_password`
  stay as the thin named wrappers (`:71-80`, doc comments intact) and
  `spawn_daemon_inner` (`:86-97`) collapses to a delegation to
  `common::spawn_daemon(Path::new(&self.config()), password)`.
  `wait_ready` (`:117-131`) and `Install`/`run` stay in the file —
  single-binary users don't move into `common` (the module is for
  cross-binary duplication, matching how `binary_command` got hoisted
  historically).
- **`cli_lock.rs`**: delete its local `wait_until` (`:21-27`) and `Child`
  (`:31-38`); the k3 daemon spawn (`:53-60`) — same shape as
  `spawn_daemon_inner` (stdio null ×2, `TASKBOARD_APP_PASSWORD` set,
  env hygiene via `binary_command`) — adopts
  `common::spawn_daemon(&config, Some("pw"))`. The lock poll (`:61`)
  calls `common::wait_until`.

Deliberately untouched (recorded so stage 3 doesn't read them as
missed spots):

- **`cli_tty.rs` daemon spawn** (`:696-705`) and explicit kill
  (`:760-761`): different stdio contract — `stdin(null)` *and stderr
  piped to a file* so an early-exit daemon's stderr can be surfaced in
  the panic (`:709-712`). Not the duplicated shape; adopting the shared
  helper would change its boot-failure diagnostics.
- **`mcp_smoke.rs`**: bare-invocation child (not a daemon), `nix`
  SIGTERM + bounded exit poll + explicit kill (`:203-221`), synchronous
  context. Different contract; the audit already ruled these explicit
  kills acceptable.
- **The inline exit-poll loops** — `cli_lock.rs:96-106` (SIGTERM exit,
  10 s) and `cli_ipc.rs:178-188` + `:275-282` (stop/successor exit,
  30 s) — are *not* `wait_until` and stay as-is. Folding them would
  change `cli_lock`'s effective exit deadline 10 s → 30 s, an
  outcome-zone change (a daemon exiting in 10–30 s flips fail → pass)
  outside this chunk's zero-behavior-change mandate. They are the same
  divergence axis, so they are recorded here as known debt; if any of
  them lands on the flaky watchlist, chunk 12 owns the decision.
- The two `read_to_string` helpers (`cli_lock.rs:40-42`,
  `cli_ipc.rs:38-40`) are three-line trivialities duplicated per file;
  not named by F8-08, left alone (hoisting them adds a common import
  for less code than it removes).

## 3. Decision D3 — one budget: 30 s, `FnMut` (per-call-site justification)

Definitions today:

| Definition | Budget | Bound | Poll |
|---|---|---|---|
| `cli_lock.rs:21-27` | 10 s | `Fn` | 50 ms |
| `cli_ipc.rs:19-25` | 30 s | `FnMut` | 50 ms |

Every caller, audited:

| Caller | Site | Current | What it synchronizes on | Cost/iteration | Relies on tightness? |
|---|---|---|---|---|---|
| k3 lock poll | `cli_lock.rs:61` | 10 s | offline daemon boot reaches lock acquisition (`taskboard.lock` appears) | cheap `Path::is_file` stat | **No** — nothing asserts the lock is acquired *fast*; it is pure process synchronization |
| `wait_ready` | `cli_ipc.rs:118-126`, called from e1`:144`, e4`:249`, e4`:272`, e5`:292`, e8`:545`, e9`:603` | 30 s | a spawned daemon serves `status --json` (`"daemon":true`) | **a full one-shot process per poll** (~0.2–0.5 s warm; multiplicatively slower under nextest parallelism / llvm-cov instrumentation) | No — synchronization only; the 30 s headroom is the load-tested value |
| e4 death poll | `cli_ipc.rs:260-264` | 30 s | daemon reaped after SIGKILL | cheap `try_wait` | No (SIGKILL is immediate; only reap latency) |
| e6 ready poll | `cli_ipc.rs:373-384` | 30 s | daemon serves status under a relative `--config` | full one-shot process per poll, plus `current_dir` | No |
| e7 ready poll | `cli_ipc.rs:442-457` | 30 s | nextcloud-backend daemon (wiremock) serves status | full one-shot process per poll + backend boot | No |

**Decision: one budget, 30 s, `FnMut` semantics, hoisted once into
`common`.** Justification, per the dispatch's "per call-site, not
averaged" rule:

- No caller's *test semantics* rely on the tight budget. All five
  caller groups are real-clock subprocess synchronization points (AGENTS
  §5: the hermetic suites' tokio-clock rules cannot reach subprocesses —
  the doc comment both copies share). None asserts speed; the budget is
  failure headroom only.
- Widening the one 10 s site (k3's lock poll) to 30 s **cannot slow any
  passing run** — the loop exits at the first true predicate — and
  cannot turn a pass into a fail. Its only effect is rescuing slow
  passes under load, which is precisely the failure mode chunk 12's
  watchlist entries describe; the dispatch fences that territory off,
  but the safe direction for the unified budget is unambiguous.
- The tight 10 s is the drifted copy: the same file's own SIGTERM exit
  loop uses the same 10 s while `cli_ipc`'s two structurally identical
  exit loops use 30 s (§2). One drifted constant should not drag four
  proven call sites down to it.
- `FnMut` is the strictly more general bound: cli_lock's `||
  lock_sidecar.is_file()` is `Fn`, and every `Fn` closure satisfies
  `FnMut`, while cli_ipc's `|| daemon.0.try_wait(…)` closures *require*
  `FnMut`. One bound compiles all existing callers unchanged.
- Cost ceiling: an already-failing wait pays at most +20 s before its
  panic (`"{what} did not happen in time"`); healthy runs pay nothing.

**Rejected**: a budget *parameter* with per-call values (re-opens the
divergence this chunk exists to close — #254 says "one budget");
averaging (dispatch-forbidden); keeping 10 s (shrinks the proven 30 s
callers' load headroom — the opposite of the safe direction).

**Effective-budget delta ledger (for stage 3's "preserve every caller's
effective semantics" check)**: exactly one call site changes effective
budget — k3's lock poll, 10 s → 30 s, widening only, reason above. All
other `wait_until` callers: unchanged (30 s → 30 s). The inline exit
loops: untouched (§2). No predicate, poll cadence, or assertion text
changes.

## 4. Hoisting order (serial, one branch, one PR)

1. **Commit 1 — the #253 rename** (`refactor(app): rename
   session_ceremonies to connector_ceremonies` — subject type is
   `refactor`, module `app`):
   - `git mv crates/taskboard-app/src/session_ceremonies.rs
     crates/taskboard-app/src/connector_ceremonies.rs`
   - Rewrite the module doc header (§1): connector ceremonies across
     the two frontends; keep ADR/design refs and D10.
   - Fix the stale `build_client` doc rider (§1).
   - `crates/taskboard-app/src/lib.rs:46`: `mod connector_ceremonies;`
     — let `cargo fmt` re-sort it to its alphabetical slot (between
     `connections` and `connectors`; rustfmt's `reorder_modules` is on).
   - `crates/taskboard-app/src/repl.rs:48`:
     `use crate::connector_ceremonies::{connect_line, logout_line};`
     (rustfmt re-sorts the import block).
   - `crates/taskboard-app/src/ipc/server.rs`: the call path at `:455`
     and the comment naming the module at `:453`.
   - `docs/adr/0014-connector-attach-session-login.org:169`: the
     `=session_ceremonies.rs=` filename reference →
     `=connector_ceremonies.rs=` (it references a living file; the
     prose around it stays).
   - Tests follow automatically (they never referenced the module —
     grep census below). Gates: `cargo fmt --all -- --check`,
     `cargo clippy --workspace --all-targets -- -D warnings`,
     `cargo nextest run --workspace`.
2. **Commit 2 — the #254 hoist** (`test(app): hoist wait_until, Child,
   and the daemon-spawn helper into tests/common`):
   - `tests/common/mod.rs`: add `wait_until` (30 s/`FnMut`, §3),
     `Child`, `spawn_daemon` (§2 table, doc comments carried over).
   - `tests/cli_ipc.rs` + `tests/cli_lock.rs`: delete the local copies,
     route through `common::` (§2 adoptions).
   - Nothing else moves (§2 "deliberately untouched").
   - Full AGENTS §10 gate suite (the app crate's suite is the subject;
     run the workspace gates in full).

No push, no PR from stage 2; stage 3 re-bases, reviews the full diff,
runs the gates, ships per dispatch §1 step 3, closes #253 and #254, and
tears the worktree down.

## 5. Grep-audit checklist (stale-reference sweep)

After commit 1 (each must hold; any hit outside the allowed set is a
finding):

- [ ] `grep -rn "session_ceremonies" crates/` → **zero hits**.
- [ ] `grep -rn "session_ceremonies" docs/` → **zero hits** (ADR 0014
      updated per §4).
- [ ] `grep -rn "session_ceremonies" . --include="*.rs" --include="*.org"
      --include="*.md" --include="*.toml"` → only `.artifacts/`
      historical reports and this plan (records of the past, not
      references to living code — dispatch §12: "`.artifacts` plan
      references only as notes").
- [ ] `grep -rn "build_client" crates/` → zero hits (rider §1).
- [ ] `grep -c "connector_ceremonies" crates/taskboard-app/src/lib.rs
      crates/taskboard-app/src/repl.rs
      crates/taskboard-app/src/ipc/server.rs` → ≥1 each.

After commit 2:

- [ ] `grep -rn "fn wait_until" crates/taskboard-app/tests/` → exactly
      one hit: `tests/common/mod.rs`.
- [ ] `grep -rn "struct Child" crates/taskboard-app/tests/` → exactly
      one hit: `tests/common/mod.rs`.
- [ ] `grep -rln "impl Drop for Child" crates/taskboard-app/tests/` →
      exactly one hit: `tests/common/mod.rs` (kill-on-drop preserved).
- [ ] `grep -n "from_secs(10)" crates/taskboard-app/tests/cli_lock.rs` →
      only the inline SIGTERM exit loop (`:96`, intentionally kept);
      the hoisted `wait_until`'s 30 s literal exists exactly once, in
      `common/mod.rs`.
- [ ] `grep -rn "wait_until(" crates/taskboard-app/tests/*.rs` → every
      call site is in `cli_lock.rs` / `cli_ipc.rs` and lexically
      preceded by `common::` or a `use common::…` import.
- [ ] `grep -rn "spawn_daemon" crates/taskboard-app/tests/` → one
      definition (`common/mod.rs`), delegating wrappers + call sites in
      `cli_ipc.rs` / `cli_lock.rs`.

Full gates (AGENTS §10) close the sweep both times.

## 6. Acceptance mapping (dispatch §12)

| Acceptance clause | Met by |
|---|---|
| Sonar duplication clusters gone | commit 2: `wait_until` ×1, `Child` ×1, spawn helper ×1 (§2, §5 checklist) |
| Rename complete, no stale paths, grep-audited | commit 1 + §5 checklist part 1 |
| Every former call site compiles against the single helper with the budget decision recorded | §2 adoptions; §3 table + delta ledger |
| Full app suite green, unchanged semantics | zero production diff in commit 2; commit 1 is a mechanical rename; one intentional budget widening (k3, §3) |
| Gate suite green | §4 gate steps, AGENTS §10 in full |

## 7. Risks & handoff notes

- **Line-number drift**: all cites are pinned to `main` @ `cf3a117`
  (2026-10-26). Re-grep before editing (§5 checklist doubles as the
  discovery pass).
- **k3's effective budget widens 10 s → 30 s** — the only semantic
  delta in the whole diff, intentional and recorded (§3). If stage 3
  finds any other effective-budget delta, it is a finding, not an
  adaptation.
- **Chunk 12 dependency**: the flaky pass triages "against the hoisted
  helpers" — keep the public shapes exactly `common::wait_until`,
  `common::Child`, `common::spawn_daemon` so chunk 12's prompts resolve.
  None of the flaky trio (#384 cli_login, #408 ipc/ip13, #417
  bootstrap) is a `cli_lock`/`cli_ipc` test, so the hoist does not
  touch them.
- **Sonar cluster collapse** is a CI-side effect, not locally gated;
  stage 3 notes it in the PR body rather than waiting on Sonar.
- **Worktree**: `../taskboard-WT-refactor-app-audit-p2` exists on
  `refactor/app-audit-p2` (created via
  `git worktree add -b … off main` after a green doctor run: 89 G free,
  floor 25 G). Stage 2 works there per dispatch §1; teardown per §1
  step 3 after PR-green.
- **Not in scope**, recorded for future passes: the three inline
  exit-poll loops (§2), `read_to_string` ×2 (§2), F8-10's handler-side
  UX write at `session_ceremonies.rs:176-181` (a separate P3 finding
  with its own sketch — it rides no chunk yet).

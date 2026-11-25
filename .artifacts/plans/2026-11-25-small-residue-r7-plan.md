# The Small Residue R7 Chunk Plan (round 7, chunk B6)

- **Date**: 2026-11-25
- **Kind**: chunk plan — stage-1 output for dispatch-plan §10 (chunk B6),
  branch `fix/small-residue-r7`.
- **Covers (normative per the dispatch plan)**: #1356, #1348, #1284,
  #1362, #1333, #1280 (read-only audit), #1283, #1281, #1345,
  #1343/#1335/#1314 — plus the **verify-and-close inventory**
  (#1016/#1010/#1008/#1004/#1290/#1172/#1273/#1097-class,
  #907/#892/#776/#1103/#867/#1160) executed in this stage-1 per the
  dispatch plan §13.
- **Origin**: `.artifacts/reports/2026-11-25-backlog-reduction-round7-scope-report.md`
  §B6; `.artifacts/plans/2026-11-25-backlog-reduction-round7-dispatch-plan.md`
  §1/§10/§13. Issue bodies normative.
- **Laws binding this chunk**: per-item fix/pin/audit-report; #1280's
  audit is read-only (deliverable =
  `.artifacts/reports/2026-11-25-keybinding-coverage-audit.md`, every
  gap dispositioned; gaps become `backlog:` issues, never in-round
  assignments); #1362's registry generation must not break the probe
  discovery order the goldens rely on; out-of-scope findings become
  `backlog:` issues immediately. No UX plan (per dispatch §1) — the
  recorded deviation on #1314 is in §5 below.

## 1. Per-item diagnoses & fix shapes

### 1.1 #1356 — CLI absolute-order verbs cannot cross a dense adjacent pair (bug, p3)

- **Defect (verified in tree)**: the CLI move verbs keep absolute-order
  payloads — `tasks.rs:391` dispatches `StateCommand::MoveTask { id,
  stack, order }` with `order = --order | append_order(...)` (and the
  habit twin at `commands/habits.rs:701`, `StateCommand::MoveHabit {
  habit, order }`). On a densely ordered stack (creation mints
  `last + 1`), moving a row over its adjacent neighbour can only name
  an occupied order; the `(order, id)` canonical sort then resolves the
  collision by UUIDv7 mint order — the same luck the UI retired in
  round-6 B3 (#1247's gap-index + dense-renumber `TaskReorder` /
  landed `HabitReorder` grammar).
- **Fix shape**: CLI-only translation. Add `--before <selector>` /
  `--after <selector>` spellings to `task move` (and `habit move`),
  resolving the neighbour row through the existing selector grammar and
  dispatching the already-landed gap-index commands (`TaskReorder` /
  `HabitReorder`) — no new engine verb. The absolute `--order` form is
  retained; its dense-pair limitation is documented in the CLI help
  next to the new flags. No engine or UI change.
- **Pin**: `it_cli` test — on a densely ordered stack, `task move
  --after <adjacent>` places the row immediately after the neighbour
  deterministically (adjacency asserted, not UUID-luck), and the
  habit twin likewise. Help-text change verified structurally (the
  flags exist and route), never by text assertion.
- **File sites**: `crates/taskboard-app/src/cli.rs` (arg grammar),
  `crates/taskboard-app/src/commands/tasks.rs` (~:380), `.../habits.rs`
  (~:690), `tests/it_cli.rs` / `tests/cli/cli_tasks.rs` pins.

### 1.2 #1348 — the in-session unlock arm renders Debug, not the terminal fold (dx, p3)

- **Defect (verified)**: `commands/mod.rs:2112` emits the in-session
  unlock remediation outcome via `format!("{outcome:?}")`, while the
  top-level `taskboard unlock` front (main.rs `remediation_workspace` +
  `remediation_exit`) uses the human-readable fold + the JSON spelling.
  One semantics, two fronts, forked rendering; the workspace shape is
  duplicated across the bin/lib boundary.
- **Fix shape**: hoist the workspace builder and the outcome fold into
  the lib (`crates/taskboard-app/src/commands/`), have both fronts
  (`main.rs` and the REPL `dispatch_line` unlock arm) call the shared
  builder + fold. The session arm renders the fold's terminal form, not
  `{outcome:?}`.
- **Pin**: snapshot (`insta`) test of the shared fold over a fixed
  `StatusRemediationOutcome`, plus a structural pin that the REPL arm
  and the top-level front consume the same fold function (both call
  sites resolve to the one hoisted symbol — a compile-level fact, not a
  text assertion at test time). No `Display`-text assertions.

### 1.3 #1284 — v7-uuid ≥8-char selector ambiguity over journal ids (dx, p3)

- **Defect (verified)**: the restore/track selector grammar resolves
  exact uuid → unique ≥8-char lowercased prefix (`commands/track.rs`
  ~:535/:561/:857). uuid v7 ids minted in the same millisecond share
  long leading hex, so journal-id families hit the typed ambiguity far
  more often than the v4-shaped entity ids the grammar was designed
  against.
- **Decision (recorded ruling)**: keep the ≥8-char minimum (it is the
  documented public convention; raising it unilaterally would break
  muscle memory), but make the ambiguity *honest and loud*: the typed
  ambiguity error already exists (`AmbiguousTimeEntry`-class); extend
  the journal-addressed verbs' ambiguity arm to **list the adjacent
  candidate ids** in the refusal so the operator can lengthen the
  prefix — and document the v7-prefix hazard in the CLI help of every
  journal-addressed verb. The D1ii deep-history picker stays the
  structural cure and remains tracked by this issue's disposition note.
- **Pin**: unit pin — two same-millisecond journal ids sharing a ≥12-char
  prefix resolve to `Ambiguous*` with both candidates present (typed
  `matches!`, plus a structural candidates-count assertion), and a
  unique longer prefix resolves.

### 1.4 #1362 — build.rs hand-maintains the 30-probe registry (ci/dx, p3)

- **Defect (verified)**: `crates/taskboard-ui-slint/build.rs:32-63`
  lists all 31 probe stems by name; a new probe file without a build.rs
  edit fails only via the *absence* of its test binary.
- **Fix shape**: **option 1** — derive the list at build-script run
  time: `read_dir(tests)` filtered on the `*.slint` probe convention,
  **sorted by stem** so the compile order is deterministic (the trap:
  the goldens rely on the probes' discovery order; a name-sorted
  derivation preserves a stable, reproducible order — stage-2 verifies
  the golden set is byte-identical by re-running the affected probe
  targets before/after). The icon probe's extra
  `rerun-if-changed` special case is keyed on the discovered icon
  probe stem. No explicit array remains.
- **Pin**: structural — the acceptance criterion is behavioral: stage-2
  drops a scratch probe file, rebuilds the package, and observes the
  probe binary exists (local verification recorded in the stage report;
  the scratch probe is not committed). All 31 existing probes compile
  with element debug info; nextest probe targets unchanged (gate run).

### 1.5 #1333 — holder-liveness probe false-positives on a live coverage suite (dx, p3)

- **Defect (verified)**: `scripts/gate-lock-liveness.sh` (sourced by
  `scripts/run-quality-gates.sh:71`, used at :105 and `liveness_tick`
  :157-172) classifies `likely-hung` from the **flock-holding bash pid's
  CPU delta alone**; during a coverage trio the bash parent sits in
  `wait` at zero CPU while `cargo-nextest`/`rustc` children work —
  minutes of false "likely-hung (no CPU time…)" reports.
- **Fix shape**: probe the holder's process tree: when the holder's own
  CPU delta is zero, enumerate live descendants (`ps --ppid` walk,
  BFS to depth ~4) and sum their CPU deltas / check for any live
  `cargo`/`cargo-nextest`/`rustc` descendant; classify alive if any
  descendant exists with nonzero delta or is a recognized heavy
  child. The true hung signal stays: zero holder delta AND no live
  heavy descendants. Report-only posture unchanged (#1110).
- **Pin**: shellcheck-clean; a constructed fixture (a holder bash that
  spawns a sleeping-then-working child) exercised manually and recorded
  in the stage report — script triage law: the round-5 A6 waivers are
  updated if the new code trips a check.

### 1.6 #1280 — app-wide keybinding-coverage audit (p2, ui) — READ-ONLY

- **Scope**: enumerate every `ActionId` (`crates/taskboard-ui-slint/src/actions.rs`)
  and every presented verb (menus, context menus, cards, overlays) across
  all surfaces; diff against the chord tables (`src/keymap/presets.rs`,
  all three presets); classify each gap: common verb → chord gap
  (becomes a `backlog:` issue; the O-8 case law governs any future
  spelling), rare verb → recorded focus-navigation justification per
  docs/ux_planning.org §2.7's exception clause; include #1257-class
  posture-only departures.
- **Deliverable**: `.artifacts/reports/2026-11-25-keybinding-coverage-audit.md`
  — the coverage matrix (verb → binding / served-by-modal-grammar /
  exception-with-justification), every gap dispositioned. **No code
  changes; no in-round chord assignments.** The habit surface's §1a
  sweep (HabitLog Ctrl+Shift+l, HabitEdit Return) is the seeded first
  rows.
- **Pin**: none (audit); the dispositioned-gap→issue mapping is the
  acceptance evidence, checked at stage-3 walkthrough.

### 1.7 #1283 — MenuEntry's &'static str labels cannot name engine-derived rows (p3, ui)

- **Defect (verified)**: `crates/taskboard-ui-slint/src/menu.rs:25`
  pins `pub label: &'static str`, so a context-menu row cannot carry
  its engine-derived subject (D1i's restore row ships the static
  "Restore habit" label).
- **Fix shape**: widen to `Cow<'static, str>` (constructor helpers keep
  `impl Into<Cow<'static, str>>` so every existing `&'static` call
  site is source-compatible); update the registry projection and the
  menu-shell model bridge; one consumer exercises the engine-derived
  path (the habit-restore row names its subject through the roster —
  the rendering half already exists; this is the naming-type half of
  the D1i Manual item).
- **Pin**: presentation/journey pin — a menu row built with a
  `Cow::Owned` label projects the owned string through the shell model
  (structural equality on the projected label value, not text-in-DOM).

### 1.8 #1281 — the status panel's open dispatch reaches the lane twice (p3, ui)

- **Defect (verified)**: two fronts converge on
  `adapter.dispatch_status_query` per mount — the route arm
  (`UiEffect::OpenStatusPanel`, adapter.rs:5704/:7355) and the
  composition's init seam (`install_status_panel_seam` →
  `on_status_panel_open`, adapter.rs:13803-13808) — a doubled
  IPC/actor round-trip contradicting OP3 D-3 ("one query per mount").
- **Fix shape**: keep exactly one front. Decision: the **route arm is
  the dispatcher** (it is the adapter-owned open path); the seam
  becomes arm-free (the `on_status_panel_open` seam retires, or is
  retained only if the composition needs a non-route open path —
  stage-2 confirms no other caller; if retained it must not dispatch).
  The refresh seam (`on_status_panel_refresh`) is untouched.
- **Pin**: the D2 command journeys' `drain_open_queries` helper now
  drains **exactly one** query per mount (an exact-count assertion on
  the lane, replacing the two-drain accommodation).

### 1.9 #1345 — bulk restore's one-fact-per-row, no single-step bulk undo (p3, ui)

- **Defect**: the restore-all rows (labels overlay + deleted-habits
  section, PR #1347) dispatch one `RestoreLabel`/`RestoreHabit` per row
  behind one armed confirm; each is its own undoable journal fact
  (#1187 law per row), so undoing a bulk restore walks back one step
  at a time — unlike `ClearReminderRules`' UF1 pre-image bundle.
- **Fix shape**: a bulk-restore engine verb that records a **pre-image
  bundle** (the tombstones the confirm resurrects) as one journal fact,
  so one `Undo` restores the whole roster the bulk confirm created.
  Journey delta: the existing undo-family journeys plan's R-6
  filed-forward home (§2, `.artifacts/plans/2026-11-22-undo-family-journeys-restore-clear-surfaces.md`,
  home #1245) — the bulk ceremony rides the established armed-confirm
  grammar; no new verb visible to the keyboard law.
- **Pin**: state-engine snapshot test (`insta`) — bulk confirm emits
  one fact; one `Undo` yields the pre-bulk `AppState` tree exactly.
- **Risk note**: engine surface change inside a "smalls" chunk; scoped
  by reusing the UF1 pre-image machinery (`ClearReminderRules` shape).

### 1.10 #1343 — aggregated Today break-rule roster shows archived-board rules (p3, ui)

- **Defect (verified)**: `presentation.rs:2271` — `break_rule_rows`
  filters `board.is_none_or(|b| rule.board == b)`; with the
  board-free `None` there is no board-liveness filter, so a rule whose
  home board is archived still renders (focusable/dispatchable) on the
  Today roster. The habits fold rides `live_boards` (pinned by the A2
  tests); the break-rule fold diverges.
- **Fix shape**: fold the board-free roster over `live_boards(state)`
  exactly as `today_habit_rows` does (archived/deleted boards
  contribute nothing); same law, no recorded reason for divergence.
- **Pin**: presentation test — archive a board holding a break rule →
  the board-free roster excludes it (the habits-fold A2 pin mirrored).

### 1.11 #1335 — break-rule create from Today offers no target-board choice (ui)

- **Defect (verified)**: the break-rule create form still mints its
  roaming board from `view_state.board()` (adapter.rs ~:17539-17589
  routes `BreakRuleCreate` over the one-shot draft), a leftover of the
  retired board scoping on the board-free Today view.
- **Fix shape**: the create draft targets the **first live board in
  canonical order** — the D-4 law `first_live_board` already implements
  for habit create (`presentation.rs:2764-2768`, its "only remaining
  role" comment anticipates this). This is the issue's own
  "or first-live-board default" arm → **closes #1335**.
- **Pin**: presentation test — on the board-free Today view the
  break-rule create draft mints into `first_live_board(state)`.

### 1.12 #1314 — habit create from Today offers no target-board choice (ui) — DEFERRED (recorded decision)

- **Decision**: the D-4 interim law (first live board) already landed
  and is honest; the target-board **picker** is a new user-visible verb
  (a new flow), which under AGENTS §7's mandatory UX-journey law
  requires an accepted ux-journey plan — and this chunk is dispatched
  "no UX plan". Implementing a picker here would violate both laws.
  **#1314 stays open** with a dated comment recording the deferral and
  its reason; the dispatcher's at-merge list for B6 drops #1314
  (recorded here as the governing decision; stage-3 re-states it).
  The issue remains the round-8 candidate for the picker round.

## 2. The #1356/#1351 pairing (sequencing decision, recorded)

B6 takes **#1356** (the dense-pair CLI fix) on `fix/small-residue-r7`;
**B3** (`fix/move-orderkey-parity`, #1351 — the UI order-key mint
re-homed/parity-pinned to the domain's `floor_midpoint`) follows
**after** B6's PR lands: B3 branches off the fetched `origin/main`
containing B6's merge, so the parity pin covers the post-#1356 tree
and the two move-grammar changes never race the same minting sites.
Rationale: #1356 changes *which command* the CLI dispatches (into the
already-landed gap-index grammar); #1351 pins *the UI's mint* against
the domain function — the pin is only total once the CLI no longer
mint absolute orders into the dense regime. If B6's PR is still open
when B3 dispatches, B3 re-bases on B6's branch tip (stacked-PR rules,
rebase dance per AGENTS §8) rather than duplicating the grammar.

## 3. Verify-and-close inventory (executed 2026-11-25, stage-1)

| Issue | State before | Verdict | Evidence |
|---|---|---|---|
| #1004 | CLOSED | verified closed | closure comment on the tracker: PR #1109 (r3 BUi bundle) |
| #1290 | CLOSED | verified closed | PR #1347 rebase-merged; `tombstoned_habits.first()` arm retired; roster enumerates deleted habits as a section |
| #1172 | CLOSED | verified closed | PR #1272: the three preambles onto the B4a harness; residual forked to #1273 |
| #1273 | CLOSED | verified closed | PR #1339: zero raw mounts remain in usability_components.rs |
| #1097-class (#1097) | CLOSED | verified closed | closed at verified merge of PR #1173 (r4 S5) |
| #907 | CLOSED | verified closed | PR #1262: shellcheck triage (5 fixed / 4 waived / 3 clean) + self-skipping pre-commit hook step |
| #892 | CLOSED | verified closed | PR #1262: the byte-identical constraint lifted (HF4 touched merge.rs); cross-reference refreshed |
| #776 | CLOSED | verified closed | PR #1262: premise refuted — `boards use` is live in the REPL (D6-6 intercept) |
| #1160 | CLOSED | verified closed | PR #1262: idle-box discard rule in docs/agent_dispatch.org §6 |
| #1016 | OPEN | **dropped (no landing)** | journeys binary now lives in `taskboard-ui-slint/tests/journeys`; `DriverOptions` uses a per-process `OnceLock` with a typed mismatch panic (`support/driver.rs:497-509`) — the failure mode is now loud, but "plain `cargo test --test journeys` passes" was not demonstrated; no r4–r6 landing closes it. Commented; round-8 candidate |
| #1010 | OPEN | **dropped (no landing)** | premise unchanged — the CLI `--due` parser and the GUI `due_edit` module remain separate grammars; no r4–r6 landing. Commented; round-8 candidate |
| #1008 | OPEN | **dropped (no landing)** | premise unchanged — armed-flow menus still rebuild rows via the re-projection path (adapter.rs:553-554, :3100, :3467); the named seam is not landed. Commented; round-8 candidate |
| #1103 | OPEN | **dropped (operator-owned)** | still valid (the doctor's 2026-11-25 sweep lists the unregistered `taskboard-WT-feat-*` dirs); scope report §13 explicitly reserves #1103 for the operator — not dispatched. Commented |
| #867 | OPEN | **dropped (operator-owned)** | hosted-account curl-matrix audit is operator work by construction; scope report §13 reserves it. Commented |

Net: **9 verified-closed (evidence on the tracker), 5 dropped with
recorded reasons** (3 round-8 candidates, 2 operator-owned). Closure
comments posted for the dropped five (dated verify-pass note each).
The nine already-closed issues carry their closure evidence on the
tracker; re-closing is a no-op — this table is the verification
record.

## 4. Stage-2 execution order

1. #1362 (build.rs derivation — verify goldens byte-identical first,
   it gates everything probe-based)
2. #1333 (script fix — unblocks shared-gate runs early)
3. #1348, #1284, #1283, #1281, #1356 (independent lib/app/ui fixes)
4. #1343, #1335 (Today presentation pair, one commit group)
5. #1345 (engine verb + snapshot pins)
6. #1280 audit last (needs the landed tree as its reading base), report
   + dispositioned-gap backlog issues.
   Per-item commits: `fix(<module>): ...` / `test(...)` / `docs(audit)`,
   each standalone per AGENTS §8 commit hygiene.

## 5. Open questions → recorded decisions

- **#1284 window**: keep ≥8 + loud typed ambiguity listing candidates
  + documented v7 hazard (§1.3). Deep-history picker (D1ii) remains the
  structural cure, tracked by the issue's disposition, not this chunk.
- **#1362 determinism**: option 1 (read_dir, name-sorted) accepted;
  the golden order trap is discharged by a byte-identical golden
  re-run check before/after at stage-2 (§1.4).
- **#1281 which front dies**: the route arm dispatches; the mount seam
  retires (§1.8).
- **#1314**: deferred, needs a UX journey plan (§1.12) — deviation from
  the dispatch plan's at-merge list, recorded here as governing.
- **#1016-class survivors**: no close without a demonstrated landing;
  the three open premises are commented and re-queued for round-8
  triage (§3).

## 6. Risks

- **#1362** is the only item that can silently move the golden/probe
  surface — mitigated by the byte-identical check ordering (§4 step 1).
- **#1345** is an engine-surface change inside a smalls bundle; scoped
  by the UF1 reuse, pinned by the snapshot one-Undo test.
- **#1280's audit** can surface many gaps; by law they become issues,
  never in-round work — the risk is report sprawl, capped by the
  matrix format.
- Gate contention: everything through `scripts/memory-gate.sh`; the
  full suite via `scripts/run-quality-gates.sh`; deferral per the
  flock rule.

## 7. Estimated diff size

~700–1000 lines including tests, across ~20 files (build.rs, gate
scripts, app cli/commands, ui-slint menu/adapter/presentation, engine
bulk verb, it_cli/presentation/engine pins, the audit report). No
domain-crate change expected (the UF1 reuse rides existing machinery).

## 8. Backlog issues opened by this stage

- None new: every out-of-scope gap found already has its issue
  (#1314 stays as the picker tracker; #1016/#1010/#1008 are the
  round-8 candidates; #1103/#867 operator-owned). Stage-2 may open
  issues from the #1280 audit gaps — to be reported in the stage-2
  handoff.

# The kind-aware `StartTracking` no-op — plan

- **Chunk**: backlog round 3, batch 2, chunk 2.1 (dispatch plan
  `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md` §3)
- **Normative**: GitHub issue #599 (fenced from #496 / PR #598); the
  pomodoro plan's §6 D-F2 and §9 R-2
  (`.artifacts/plans/2026-10-31-pomodoro-focus-plan.md`); the respite
  plan's D-3
  (`.artifacts/plans/2026-10-31-respite-break-reminders-plan.md`)
- **Branch**: `feat/start-tracking-kind-aware-noop` (off
  `origin/main` @ `ebc8fc43`, which carries chunk 1.2's merge
  `048728b4`)
- **Size class**: S — the production diff is one predicate; the rest
  is pins and the CHANGELOG entry.
- **PR title (stage 3)**: `feat(domain): the tracking no-op knows its kind`

## 1. The verified landing state

The `StartTracking` planner arm
(`crates/taskboard-domain/src/command.rs:2207-2274`) resolves liveness
(`live_task`, unknown/dead tasks reject typed before anything else),
then applies the idempotent-repeat guard (R-6):

```rust
let open: Vec<&TimeEntry> = …filter(|entry| entry.end.is_none())…;
if open.len() == 1 && open[0].task == *task {
    return Ok(no_op());
}
```

**The defect**: the guard is task-keyed and kind-blind. `TimeEntryKind`
landed with chunk 3.5 (#493) as a payload field, but the repeat check
never compares it. Every other shape — zero open, a foreign open
entry, a transient double-open — falls through to the close-all →
open → dismiss → flush path (`close_all_open`,
`command.rs:487-504`), which is already kind-correct: the minted row
carries `kind: *kind` (`command.rs:2236`).

Two observed consequences (issue #599, both verified in tree):

1. The focus runtime's focus→break hand-off
   (`crates/taskboard-app/src/focus.rs:184-219`) must be the
   **two-verb** sequence `StopTracking` → `StartTracking { kind:
   Respite }`, awaited to each receipt — a single kind-switching
   dispatch would be silently swallowed (D-F2's "kind-blind no-op
   land-mine").
2. The CLI's `track start <task> --kind respite` while that task's
   Work entry runs is a silent no-op (`TrackStartReceipt { started:
   false }`, `crates/taskboard-app/src/commands/track.rs:206-220`) —
   against the respite plan's documented D-3 intent ("opens the
   *running break*").

## 2. Decisions

| # | Decision | Status |
|---|---|---|
| D-1 | The no-op guard compares task **and** kind; every other shape falls through to the unchanged close-all+open path | **decided** |
| D-2 | The switch is **one verb application** — a single atomic `CommandPlan`, not an internal two-phase sequence | **decided** |
| D-3 | Journal/undo classification unchanged: `NotUndoable(TrackingSurface)`; the switch *newly journals* a node where the old no-op journaled nothing | **decided** |
| D-4 | Pin catalogue: the four open-shape combinations, pinned TDD-first in the domain planner tests | **decided** |
| D-5 | The respite-while-working journey pins at the app layer (`cli_track.rs`) | **decided** |
| D-6 | The focus runtime is **untouched** — the two-verb workaround stays, now belt-and-braces; D-F2's switcher limitation dissolves by design | **decided** |
| D-7 | CHANGELOG: a `### Fixed` entry under Unreleased (wording recorded verbatim, §4) | **decided** |
| D-8 | The guard's law comment is reworded kind-aware (exact replacement recorded) | **decided** |

### D-1 — the guard's new shape

The fix is one predicate extension at `command.rs:2220`:

```rust
if open.len() == 1 && open[0].task == *task && open[0].kind == *kind {
    return Ok(no_op());
}
```

`TimeEntryKind` is `Copy + PartialEq` (`tracking.rs:69-79`) — no new
types, no A1 machinery, the comparison is total. Everything below the
guard is untouched: the fall-through path already closes all opens,
mints the fresh row with the payload kind, fires the D-T5 dismissal
side effect for the target task's active occurrences, flushes the
ops (the target's op last and always), and returns
`TrackingStarted { entry, closed }`.

The guard order stays: `live_task` **before** the guard — a repeat
against a since-deleted task still rejects typed
(`start_tracking_rejects_unknown_and_deleted_tasks` pins it), never
no-ops.

`StopTracking`'s nothing-open no-op is kind-independent by nature and
stays as landed.

### D-2 — one verb application, not an internal sequence

The switch rides the **existing** single-application close-all+open
path. Rejected alternative: an internal `StopTracking`-shaped
pre-phase (synthetic sub-plan or two outcomes from one call) — that
is new planning machinery for a path the arm already implements; the
double-open and foreign-open shapes have closed-and-reopened inside
one `StartTracking` plan since ADR 0017. Concretely, one call yields
**one** receipt:

- `CommandOutcome::TrackingStarted { entry, closed: [old_id] }` — the
  old entry stamped `end = now`, the fresh entry carrying the payload
  kind with `start = now` (zero-gap; the plan's single `now`).

The app-layer projection needs no change: `start_on`'s
`TrackingStarted` branch reads the minted row back from the published
snapshot (`track.rs:189-202`); its `NoOp` branch's `expect` stays
sound because `NoOp` still implies this task's open entry exists —
now additionally of the requested kind, which the projection matches
on task only (unchanged, still correct).

### D-3 — the journal/undo consequences, spelled out

The close+open path classifies exactly as the landed tracking arm —
**`UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)`
(`undo.rs:215-220`), no `undo.rs` edit, no new classification row.**
The ADR 0017 reason stands verbatim: the inverse would have to
re-open closed entries — no command expresses that — plus the D-T5
dismissal side effects. ADR 0019's §3 table is untouched.

What *does* change is journal traffic, mechanically, via the
empty-batch rule (`undo.rs:69-74`: an accepted no-op journals
nothing):

- **Before**: a kind-switching dispatch returned `NoOp` with zero
  actions → journaled **nothing**; the journal line was untouched.
- **After**: the switch is an accepted mutating command → it appends
  **one** journal node carrying `NotUndoable(TrackingSurface)` —
  precisely what the foreign-open and double-open shapes already do
  every time.
- `Undo` issued while that node is head refuses typed
  `TargetNotUndoable(TrackingSurface)` — `plan_undo`
  (`undo.rs:496-512`) reads the head's facts and never skips a
  non-undoable node. Before the fix, the swallowed no-op left the
  head on the *previous* command, so undo would have undone that
  instead. This is the landed semantics for every effectful
  `StartTracking`; the switch merely reaches it on a new input
  shape. Recorded, accepted (risk R-2).
- Retroactive re-marking of *history* stays `AdjustTimeEntry`'s
  change-mask verb (respite D-3) — the switch is about the *running*
  entry; the two paths do not compete.

Pin: one undo-side test that the switch journals the
`TrackingSurface` node and undo refuses typed (§3, P-6).

### D-4 — the domain pin catalogue (TDD-first)

Pinned in `command.rs`'s test module beside the existing pins, in
this red-first order (see §5). The four combinations of (open
entry?, same task?, same kind?):

- **P-1 — none open → opens.** Already pinned:
  `start_tracking_opens_with_title_snapshot_and_flush_op` (Work) and
  `start_tracking_opens_a_running_break_with_the_kind_payload`
  (Respite payload, fresh open row). No new test; both must stay
  green unchanged.
- **P-2 — same task + same kind → `NoOp`.** The existing
  `start_tracking_on_the_only_open_entry_is_a_no_op` covers the Work
  repeat. **New**: the Respite repeat — a `StartTracking { kind:
  Respite }` over an open Respite entry of the same task is also a
  `NoOp` with zero actions. Discrimination: under a wrong
  implementation that made *everything* switch, this pin fails — it
  guards the NoOp half of the law. Name:
  `start_tracking_the_respite_repeat_is_also_a_no_op`.
- **P-3 — same task + different kind → closes and reopens (the
  fix).** Two directions, because the guard is symmetric:
  - `start_tracking_a_respite_over_a_running_work_entry_switches`:
    open Work entry on the task, dispatch `StartTracking { task,
    kind: Respite }` → `TrackingStarted`; the old entry stamped
    `end = now`; the fresh entry open with `kind == Respite`;
    `closed == [old_id]`; the batch shape matches the foreign-open
    path (two entry upserts + dismissal/flush ops per the D-T5
    machinery already pinned there).
  - `start_tracking_work_over_a_running_break_entry_switches_back`:
    open Respite entry, dispatch `kind: Work` → same shapes with
    `kind == Work` on the fresh row.
  - Discrimination: **both fail under the landed kind-blind arm**
    (it returned `NoOp`) — they are the fix's proof pins. The
    direction split is two tests, one fact each (house style).
- **P-4 — different task open → closes foreign and opens.** Already
  pinned: `start_tracking_closes_the_foreign_open_entry_first` and
  the double-open `start_tracking_over_a_double_open_closes_and_reopens`.
  No new test; both stay green (their payloads are Work-over-Work,
  which the new guard still routes to the close-all+open fall-through
  because the open entry's task differs / the shape is double-open).

**P-5 — the switch journals its node (undo side).** In
`undo/tests.rs` beside the existing tracking-surface classification
test (:1480): seed an open Work entry, dispatch the switching
`StartTracking { kind: Respite }`, assert the journal head moved and
its facts are `NotUndoable(TrackingSurface)` (structural assertion on
the journal projection), then assert a following `Undo` refuses
`CommandError::TargetNotUndoable(TrackingSurface)` — `matches!` on
the typed variant, no text. Discrimination: under the old arm the
command journaled nothing and undo reached the previous node.

### D-5 — the respite-while-working journey (app layer)

One integration test in `crates/taskboard-app/tests/cli_track.rs`
beside t1–t4 — the CLI-visible proof of respite D-3's "opens the
*running break*":

- Harness: `run_start` currently fixes the kind at the invocation
  (default Work); give the helper a kind-taking form (parameter or a
  thin `run_start_kind` sibling — implementer's choice, keep the
  existing call sites untouched).
- `t_starting_the_running_break_while_work_runs_switches`: seed one
  task → start Work → start again with `kind: Respite` →
  `receipt.started` is true, `receipt.closed` names the Work entry's
  id, `receipt.entry` carries the respite kind and is open; a
  `run_status` follow-up shows exactly one open row, the respite
  one. Structural/typed assertions only (receipt fields; no message
  text).
- Discrimination: under the landed arm this receipt was
  `started: false` with the Work row echoed — the test fails there.

Routing-only at this layer: the behavior's lowest-layer pin is P-3;
this journey pins the receipt projection and the selector path
(`resolve_task` → `start_on`) end to end.

### D-6 — the focus hand-off unchanged

**Zero edits to `crates/taskboard-app/src/focus.rs` and its tests.**
Verified against the landed runtime:

- The two-verb hand-off (`focus.rs:184-219`) stays *correct*:
  `StopTracking` closes the Work entry (its nothing-open no-op is
  the accepted degrade), then `StartTracking { kind: Respite }`
  observes zero opens — P-1's shape — and opens the break. After the
  fix the sequence is redundant for the switch case but not wrong;
  it remains the runtime's documented shape (D-F2 decided the verb
  sequence; this chunk does not re-litigate it). Collapsing it to a
  single dispatch is a possible future simplification, out of scope
  here — if wanted, it is a new issue, not this branch.
- The existing focus journey tests (work→break anchoring on
  `entry.start`, break→idle, re-anchor-on-replacement) are the
  regression net and must pass untouched. Per the dispatch plan:
  a broken focus journey is a deviation to report, never a
  workaround to delete.
- What dissolves by design (recorded as intended): D-F2's v1
  limitation "a user pressing Start on the switcher for the task
  whose break entry is open gets a `NoOp` (the next focus can't be
  started on the same task until the break ends)". After the fix,
  pressing Start (Work) on that task **ends the break early** —
  closes the Respite entry, opens Work. This is user-initiated,
  matches "starting work ends the break", and is coherent with the
  focus runtime's re-anchor property (the session IS the open entry;
  a replacement entry re-anchors the phase for free).
  The break-reminder trigger consumes the data (an open Work row
  resets the streak) — no evaluator edit.
- Kiosk's absorbed-dispatch layer and the ui-slint flow-table pins
  assert the *dispatched command* shapes — payloads are unchanged,
  so those pins are untouched by this chunk.

### D-8 — the guard's law comment

The comment above the guard (`command.rs:2211-2213`, the R-6
"Idempotent repeat" paragraph) is reworded to the kind-aware law —
the replacement prose, verbatim:

```rust
// Idempotent repeat (R-6): exactly one open entry and it is this
// task's *and* this kind's (#599) — a different kind is a switch,
// not a repeat. Any other open shape (a foreign open entry, a
// transient double-open) closes all and reopens.
```

The module-header NoOp classes list and the `TrackStartReceipt` doc
(`output.rs:948-950`) already read kind-neutral; only the receipt
doc's parenthetical "(this task's open entry already exists)" gains
"of the requested kind" if the implementer finds it misleading —
otherwise untouched.

## 4. CHANGELOG entry (D-7)

Added verbatim to the topmost `### Fixed` group under
`## [Unreleased]` (the respite-family group that already carries the
tracked-total fix), as the first entry of that group:

```markdown
- Starting a tracked block in the other kind no longer silently does
  nothing (#599): while a task's Work entry runs, `track start <task>
  --kind respite` now closes the running work entry and opens the
  running break (the respite marking's documented intent), and
  starting work on a task whose break entry runs closes the break and
  starts the work block. The idempotent no-op now compares task and
  kind — repeating the exact same task and kind still does nothing.
  The focus runtime's stop-then-start break hand-off is unaffected,
  and undo still reports the tracking surface as not undoable.
```

## 5. Implementation order (TDD-first, per dispatch §2.1.b)

1. Pins red: P-2's Respite-repeat pin, P-3's two switch pins, P-5's
   journal pin, D-5's CLI journey (the last needs the harness helper
   first). Confirm they fail **for the right reason** — the switch
   pins and the journey fail with `NoOp`/`started: false` observed;
   the Respite-repeat pin passes under the old code by design (it is
   a regression guard, green before and after — only P-3/P-5/D-5 go
   red-first).
2. The fix: the D-1 predicate + the D-8 comment. Everything green.
3. Full gate suite via `scripts/run-quality-gates.sh` (AGENTS §10),
   from the worktree; then the CHANGELOG entry; handoff at
   PR-green per the dispatch plan (stage 3 opens the PR and closes
   #599).

## 6. Risks

- **R-1 — surfaces that "benefited" from the swallow**: any caller
  firing `StartTracking` while the *other* kind runs now causes a
  close+open instead of nothing. Audit result: the dispatch sites are
  the focus runtime (two-verb, unaffected), the CLI `track start`
  (the fix's subject), and the ui-slint router/overlay Start routes
  (payload `Work`; previously NoOp over a Respite entry, now the
  intended break-end). All three are covered above; no other
  `StartTracking` emitters exist in the tree.
- **R-2 — the undo line moves on a switch** (new journal node where
  the no-op journaled nothing): `Undo` right after a switch refuses
  typed instead of undoing an earlier command. Landed semantics for
  every effectful `StartTracking`; accepted (D-3).
- **R-3 — the focus runtime**: verified nothing breaks (D-6); its
  journey tests are the net. If one fails, that is a finding, not a
  fence to relax.
- **R-4 — wire/sync/storage**: no payload, receipt-schema, sidecar,
  or sqlite changes — `TimeEntry` rows already carry `kind`; the
  switch produces ordinary stamped + minted rows through the landed
  fold paths.
- **R-5 — proptests**: the tracking convergence proptests use Work
  payloads and pin open-entry convergence, which the fall-through
  already guarantees; they stay green and unchanged (their invariant
  is kind-blind by nature).

## 7. Fences (what this chunk does NOT do)

- No `focus.rs` change, no new `StateCommand`/`CommandOutcome`
  variant, no receipt-schema change, no IPC/parse change.
- No undo classification change (ADR 0017/0019 stand verbatim).
- No runtime collapse of the two-verb hand-off (recorded as a
  possible future issue, not this branch).
- No retroactive-history marking work (that is `track adjust
  --kind`, landed).

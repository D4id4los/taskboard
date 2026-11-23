# Backend Residue — Chunk Plan (Round 5, Chunk E3)

- **Chunk**: E3 of backlog-reduction round 5 (dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md` §9)
- **Branch**: `test/backend-residue`
- **Scope report**: `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md` §E
- **Covers**: #1017, #1185, #1232, #1216, #1163, #1159
- **Round laws applied**: typed outcomes (no text assertions); issue bodies
  normative; behavior-identical where the corpus is the oracle; evidence-carried
  diagnosis for #1159 (the R2 method: loaded runs, counts recorded);
  out-of-scope findings become `backlog:` issues immediately.

## 0. Planning-stage diagnosis (worktree `../taskboard-WT-test-backend-residue`, off `origin/main` d7646762)

Every seam below was read at the landed tip; decisions are recorded per item.
No open questions remain.

### #1017 — habit rows rebuild decorations ModelRc every pass — **FIX**

`models.rs::habit_card` (line ~846) builds
`decorations: ModelRc::new(VecModel::from(decorations))` fresh on every call,
and the habits pass (~line 943, `..habit_card(row)`) re-pushes whole rows.
`TaskCardModel::PartialEq` compares `decorations` by handle identity, so every
habit row compares unequal every refresh pass and the habits model re-pushes.

The board cards already solve this: `PushModels::card_decorations`
(`BTreeMap<TaskId, MirroredRows<TaskDecoration>>`, models.rs ~287) with the
documented handle-identity rule (tag-color plan D-9). The habit pass skips it.

**Decision**: extend the same rule to the habits section. Add
`habit_decorations: BTreeMap<HabitId, MirroredRows<TaskDecoration>>` to
`PushModels` (and the label-chips twin only if habit rows ever grow chips —
today `label_chips` is a static empty model, keep it as-is); the habits apply
path derives the complete desired decoration rows, mirrors them onto the
persistent handle via `MirroredRows::push` (unchanged rows notify nothing), and
installs that handle into the card. `habit_card` becomes the *derive* half
(a pure function returning the desired `Vec<TaskDecoration>` plus the scalar
fields) so the fold/property tests keep their oracle; the *install* half moves
to the apply path. Map eviction rides the same rule as `card_decorations`
(never grows past the live rows). Typed pin: a models-level test asserting
that a second identical refresh pass does not replace the decoration handle
(handle identity preserved) — the #592 identity law at the habits surface.

Diff: `crates/taskboard-ui-slint/src/models.rs` (+~60/−20), one models test.
Corpus is the oracle — no snapshot/journey output change expected; if a golden
moves, the corpus is re-recorded and the reason recorded on #1017.

### #1185 — the doctor sweeps a fresh worktree branched off a stale local main — **FIX (doctor + law text)**

Current landed state of `scripts/worktree-doctor.sh`: it *does* fetch first
(`fetch origin --prune --quiet`, warn-and-continue on failure), and it *does*
have a tip guard (`KEEP (HEAD at origin/main tip)` when HEAD == fetched tip).
The gap #1185 names is the *strictly-below-tip* fresh worktree: created with
`git worktree add -b <branch> <path>` with **no start point** → branched off
the **local** `main` ref; if local `main` is behind the fetched `origin/main`
(mid-round PRs landed), HEAD is an ancestor of `origin/main` but not equal to
it → the `merge-base --is-ancestor` containment check passes →
`REMOVE (HEAD merged)` on a zero-commit, clean-tree, in-flight worktree. The
script's own "Known limitation" comment documents exactly this hole.

**Decision** (both legs of the issue's proposal; they compose):

1. **Doctor grace window**: before the containment removal, skip a worktree
   whose branch has **zero commits ahead of its merge-base with
   `origin/main`** AND whose branch birth (the first reflog entry of
   `refs/heads/<branch>`, `git reflog show <branch> --format=%gD`) is younger
   than `WORKTREE_DOCTOR_GRACE_HOURS` (default 2). Output row
   `KEEP   (fresh — grace window):` with the same column shape. A landed-PR
   leftover whose branch has unique commits is unaffected; a *truly* abandoned
   zero-commit scratch is pruned by hand per the script's never-pushed stance
   (that is the documented trade, unchanged). Composes with E1's a1069da6:
   that commit touched `scripts/ux-coverage.sh` only — no overlap, no rebase
   coupling; this branch carries both if E1 hasn't merged yet.
2. **Law text**: the dispatch mechanics in
   `docs/agent_dispatch.org` (worktree-creation step) gain the explicit start
   point: `git worktree add -b <branch> <path> origin/main` — branch off the
   fetched tip, never the bare `-b <branch> <path>` spelling. AGENTS.md §8's
   create line gets the same start point. (AGENTS.md is updated, not
   rewritten — the §8 block already says "never without `-b`, never on
   `main`"; this adds the start point.)

**Test**: a scripts fixture test per the #1186 pattern (the fixture harness
under `tests/scripts/`): a temp repo, a fresh worktree with zero commits on a
branch whose merge-base is strictly below a fake fetched tip → grace KEEP;
same worktree with the reflog birth artificially aged (refutable via the env
override: set grace to 0) → the pre-existing REMOVE decision. No wall-clock
assertions — the grace check reads the reflog timestamp, the test controls it
through the env override, and both outcomes are typed exit/decision rows.

Diff: `scripts/worktree-doctor.sh` (+~20), one fixture test, two docs edits.

### #1232 — the interactive connect prompts' stdin test seam — **FIX**

Diagnosis: the seam *half-exists*. `connect.rs` already defines
`trait PromptReader` (line ~58: `read_line`/`read_secret`/`read_confirm`) and
`pub async fn run<P: PromptReader>` is fully generic — the mutants survive
because **no test drives a scripted reader through `run`'s prompt legs**; the
only tests use `StdinPrompts` itself (line ~740, non-tty confirm) and pure
helpers (`parse_confirm`, `attach_nudge_mapping`). `read_line_sync` and
`read_secret_sync` (lines 124/140) are the mutation survivors.

**Decision**: a handwritten `ScriptedPrompts` fake (AGENTS §5 fakes-over-mocks
rule) implementing `PromptReader` from a `VecDeque` of canned answers,
recording the prompts it was asked. Two in-module tests drive `gather_identity`
/ `run`'s prompt legs and assert **typed outcomes**:

- an empty secret answer → the identity ceremony refuses (the typed
  `CommandError` variant / the retry-or-refuse outcome the ceremony defines —
  read at implementation, asserted with `matches!`, never by message text);
- a non-empty answer routes verbatim into the overlay/backend-switch decision
  (the recorded prompt sequence proves the answer text reached the decision).

If `run`'s full path needs IO beyond the prompts (config writes, IPC), scope
the tests to the prompt-leg functions (`gather_identity`, `decide_backend_switch`,
`decide_repoint` — all generic over `P`) so the fake drives exactly the seam
the mutants live on; that is the lowest layer owning the behavior (§5
one-behavior-one-test). The two recorded mutants die; the mutants-lane record
is updated in the stage report (the sweep record itself is dispatcher-owned).

Diff: `crates/taskboard-app/src/commands/connect.rs` tests module (+~80),
zero production-code change expected (the trait is the seam). If the legs need
`pub(crate)` widening for a `tests/`-side integration test, prefer in-module
`#[cfg(test)]` instead.

### #1216 — headless backend drops raw key events when the modal grab doesn't survive — **DECISION: documentation leg (option b), not driver change**

Diagnosis: `driver.rs::press_stroke` (line ~220) delivers through
`Window::dispatch_event` — the same channel as the real window — but Slint's
focus-root key routing needs a focus owner; when the recur editor's
`panel-scope` focus grab does not survive the headless pass
(`text_focus_count == 0`), the stroke lands nowhere. The landed journeys'
workaround is `handle.dispatch_action(ActionId::…)` (the typed action the
binding resolves to), e.g.
`small_gated_journeys.rs::us2_s3_escape_cancels_staged_draft`.

**Decision**: option (b) — document the "route the typed action" rule in
`docs/testing_strategy.org`'s journey-driver section (a short subsection: when
a raw `press_stroke` after a modal mount closes nothing, the grab did not
survive the pass; route the binding's typed action via `dispatch_action`, and
assert the typed outcome). Option (a) (window-level fallback in the headless
key dispatch) is **rejected for this chunk**: it changes which events existing
journeys deliver (previously-dropped strokes would start routing), which is a
behavior change in the test backend whose blast radius spans every key-driven
journey — not behavior-identical, and the corpus does not oracle it. If a
future journey genuinely needs raw-stroke-after-modal, the finding goes back
to #1216 as a comment with the journey name (recorded as a follow-up note in
the stage report, not a new issue — the issue stays open as the tracker for
option (a)).

Diff: `docs/testing_strategy.org` (+~25), one source comment on `press_stroke`
pointing at the doc rule.

### #1163 — insta snapshot filenames embed the binary name — **FIX**

Diagnosis: `crates/taskboard-state/tests/snapshots.rs` uses plain
`insta::assert_yaml_snapshot!` calls (no `Settings`); the 55 files under
`tests/snapshots/` carry the `snapshots__` prefix — insta's default
module-path prefix, which for a test binary *is* the binary's module name.
Any consolidation that renames the test binary silently renames every
`.snap`. Same latent constraint for ui-slint's insta fixtures.

**Decision**: pin the filenames to the declared snapshot names, decoupled from
the module path. A `snapshot_settings()` helper in the test crate: a
`insta::Settings` with the module-name prefix disabled and `snapshot_path`
pinned to the existing directory, bound via `bind_to_thread()` in a fixture or
applied per capture. Implementation note (verify at stage 2 against the pinned
insta version): the knob is
`Settings::set_prepend_module_name_to_snapshot(false)` (filenames then derive
from the declared name only); the fallback if the knob's shape differs is
`Settings::set_snapshot_path` + explicit per-call snapshot names — either way
the contract is "filename is a function of the declared snapshot name alone",
documented in `docs/testing_strategy.org`'s snapshot section so future
insta-bearing suites follow it.

The one-time rename is 55 `git mv`s (`snapshots__s1_persisted_state.snap` →
`s1_persisted_state.snap`); the corpus content is untouched and the suite is
the oracle — behavior-identical by construction. The S2b constraint lifts:
`snapshots.rs` may merge into another binary in a later consolidation without
a corpus move. Comment on #1163 with the convention; ui-slint adoption stays
out (no insta fixtures there yet — noted on the issue, not implemented).

Diff: `crates/taskboard-state/tests/snapshots.rs` (+~15), 55 file renames,
one docs edit.

### #1159 — ip16 pins the whole-suite wall floor at 60 s — **ARMED DIAGNOSIS, evidence-carried, with a conditional one-word fix**

Landed state per the R2 amendments: R2b (#1122) killed the verb-boot 10s
quantizer (sub-second residuals); R2c (#1138, option a) inverted the teardown —
bounded joins dropped 30 s → 6.6 ms. Neither explains ip16's flat 60.4 s.

**Diagnosis hypothesis (from the landed code)**: the 60 s is not the drain
(`DRAIN_BUDGET` is 5 s, `common::LIVE_DEADLINE` bounds the whole test) and not
the daemon join — it is the **relay-ed `sync` verb's cycle wait budget**:
`cli.rs` line ~213, `sync --timeout-secs` **default 60**. ip16's
`exec_once(&db, &argv("--json sync"))` relays a cycle for an identity pointing
at `https://cloud.example.com` — an unroutable name; the test comment already
accepts "a timeout under loaded runners" as an outcome, i.e. the pinned
absence (`sync-disabled` / `sync-unavailable` refusals) is asserted on
whatever the cycle returns, and the wall cost is the 60 s wait budget
expiring. Measured flat across nextest thread counts because the budget is
per-test, not scheduler-dependent.

**Deliverable (stage 2, the R2 method — loaded runs, counts recorded)**:

1. Measure: run ip16 alone with per-leg timestamps (tracing or `Instant`
   prints behind a temporary patch, never landed) — record the leg wall
   table: boot / status / attach / status / sync-exec / boards create / exec
   refusals / join. Accept/reject the hypothesis by the numbers.
2. If confirmed: change the fixture's argv to
   `argv("--json sync --timeout-secs 2")` — a one-line test change. The typed
   outcome assertions are untouched (any cycle outcome stays fine; the two
   refusal-class `assert_ne!`s are the pin), so the contract survives; the
   suite floor drops to the next-longest test (~43 s per the S2a report's
   numbers). No quality ceiling is introduced: the test asserts classes of
   outcome, never a speed.
3. If rejected (the 60 s lives elsewhere): the measured leg table is the
   deliverable; post it on #1159 as the evidence-carried diagnosis and leave
   the fix armed with the corrected target.

Measurement law: the loaded-run caveat applies (the #1160 idle-box rule is
E6's, landing in parallel — if not yet landed, note the box-load context in
the numbers posted to #1159). No wall-clock assertions in any landed test.

Diff: 0–1 line of test code + the issue comment carrying the leg table.

## 1. Sequencing (stage-2 shape)

1. #1185 doctor + law text (smallest blast radius, unblocks parallel
   worktrees — do first).
2. #1163 insta decoupling (the 55-rename commit is mechanical and isolated).
3. #1017 habit decoration handles (the only real refactor; corpus oracle).
4. #1232 ScriptedPrompts fake + the two mutant-killer tests.
5. #1216 docs leg.
6. #1159 measurement last (needs a quiet box for honest numbers; the gate
   flock keeps the suite exclusive anyway).

Commit per item, `<type>(<module>)` subjects, rebase-clean standalone commits.

## 2. Acceptance

- Per-item: fix landed or evidence-carried diagnosis posted (#1159).
- Typed outcomes everywhere; no text assertions; no sleeps; corpus oracle
  for #1017/#1163.
- Pre-commit hooks green; `scripts/run-quality-gates.sh cargo nextest run
  --workspace` green (flock-deferral rule: if the lock is held and no work
  remains, commit, report "gate deferred: lock held by <holder>", end).
- Diffs confined to: `scripts/worktree-doctor.sh`,
  `docs/agent_dispatch.org`, `AGENTS.md` (§8 one line),
  `docs/testing_strategy.org`, `crates/taskboard-ui-slint/src/models.rs`,
  `crates/taskboard-app/src/commands/connect.rs`,
  `crates/taskboard-state/tests/snapshots.rs` + its `tests/snapshots/` corpus,
  and at most one line in `crates/taskboard-app/tests/services/ipc.rs`.

## 3. Risks

- **#1017**: the habits pass currently re-pushes whole rows; introducing
  stable handles could surface a latent identity bug elsewhere (the corpus
  and the models-level handle-identity test are the oracle; a golden move is
  recorded, not hidden).
- **#1163**: the insta knob's exact name/behavior varies by version — verify
  against the pinned insta before the rename; the fallback (explicit names)
  is planned and equally decoupling.
- **#1185**: the reflog-birth read can fail on a branch with no reflog —
  treat "unreadable birth" as not-fresh (fall through to the existing
  predicates; KEEP-biased is the doctor's documented posture).
- **#1159**: the argv change must not tighten the sync-cycle outcome space —
  the refusal-class pins and the "any cycle outcome is fine" clause are
  re-read at implementation; if a 2 s budget makes the outcome
  nondeterministic in a *class* the test pins, keep the diagnosis-only
  deliverable (the plan's step 3).

## 4. Estimated diff

~250–350 lines of code/docs across seven files, plus 55 mechanical
`.snap` renames and ≤1 line in `ipc.rs`.

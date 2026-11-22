# Chunk E1 — Gate/Journey Token Residue — Implementation Plan

- **Date**: 2026-11-22
- **Round**: backlog-reduction round 5, chunk E1 (dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §9; scope report
  `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  §E — normative)
- **Branch**: `test/gate-journey-token-residue` (off `origin/main`
  008750ab)
- **Issues**: #1181, #1186, #1164, #1217, #1236, #1237, #1172, #1183
  (+ #1250 adjacency — verdict in §9)
- **Size estimate**: M (~400 changed lines across `scripts/`, three
  committed plan artefacts, `adapter.rs`, three journeys members, one
  new journey leg, `docs/ux_planning.org`)
- **Gate**: pre-commit hooks + `scripts/run-quality-gates.sh cargo
  nextest run --workspace` (flock-deferral rule binding: if the lock is
  held and no useful work remains, commit, report
  "gate deferred: lock held by <holder>", end the stage).

## 0. Recorded decisions (stage-1 resolutions)

1. **#1186 is already fixed on `main`** — commit `a1069da6`
   ("ux-coverage's fixture prune survives a symlinked --root") reordered
   the resolution so `EVID_ROOT=$root` is assigned *after*
   `root=$(cd "$root" && pwd -P)` (scripts/ux-coverage.sh:670–682),
   which canonicalizes a relative `--root .` spelling too. E1 does NOT
   re-fix; it adds the covering self-test (§2) and closes the issue
   against the landed commit.
2. **Backtick handling = strip inside `cellget`, keep plans as
   authored** (#1181 "Ask" option a). No Kind-detection special case
   needed: with the spans stripped, `` `ActionId::BoardArchive` ``
   parses exactly like a bare token.
3. **Absent-cell grammar stays strict**: `is_absent` keeps matching
   only empty / `—` / `-` (scripts/ux-coverage.sh:374). Prose-bearing
   dash cells ("`— (binding rows)`") are NOT made absent by the gate —
   the plans are amended to true absence or real tokens instead
   (§4–§6). The gate-side relaxation is filed as a backlog issue (see
   §11) — one grammar change per gate family per chunk keeps the
   self-test matrix reviewable.
4. **W-1's Commands cell becomes true absence (`—`)**: the walk is pure
   view-flow machinery — `ActionId::ReviewWalkPlanner` routes to
   `UiEffect::ReviewWalk` (router.rs:2254); no `UiRequest` or
   `StateCommand` rides the row. Prose → `—`, erratum note per the
   supersession pattern.
5. **#1183's seam test is written now, not deferred**: the issue's ask
   is conditional ("if tombstoned boards ever become selectable"), but
   the precondition *can* be hand-built in the journey harness (journey
   tests construct `AppState` directly). Writing the leg now honors
   AC-3 literally and costs one test member; the "tombstone selectable"
   future (#152) then extends, not creates.
6. **#1250 is subsumed via the convention route** (§9): the per-round
   decade-band sampling law is recorded in this plan and cross-referenced
   from `docs/ux_planning.org`; plan-scoped token matching is explicitly
   NOT built.
7. **Helper adoption (§7) and seed-echo extraction (§8) are
   behavior-identical**: verified by the existing journey suites; no
   snapshot churn expected (state trees unchanged).

## 1. #1181 — backtick-aware token parsing in `cellget`

**Current**: `cellget` (scripts/ux-coverage.sh:104–109) trims
whitespace only; `` `ActionId::X` `` keeps its backticks, so the
`StateCommand::` / `UiRequest::` / `ActionId::` prefix arms
(scripts/ux-coverage.sh:983–990) and the `delta_kind` lookup never fire.

**Design**: in the awk `cellget` function, after the whitespace
`gsub`, add `` gsub(/`/, "", cell) `` — inline code spans are stripped
wholesale (single-level spans only; the plan grammar never nests
backticks). This preserves authored plan format and fixes every
downstream consumer at once (`header_matches`, `cellcount` consumers,
the wiring loop at :936+, the cap/step parsers — all go through
`cellget`).

**Self-tests (the N-A/B5 precedent — each new rule gets a fixture
plan)**:
- `plan-backtickprefix.md`: §6 wiring row whose Commands / ActionId
  cells wrap tokens in backticks (`` `ActionId::BoardArchive` ``-style,
  pointed at the existing fixture tree's `actions.rs` /
  `requests.rs`); expect FOUND (not MISSING-from-backtick-literal).
- `plan-backtickstep.md`: §4 journey table with backticked step tokens
  (`us1_s1`) and backticked `bound-test:` cell; expect the same
  resolution as the bare equivalent (`plan.md` fixture outcomes).
- Self-test runner extended with the two cases (PASS/exit-code +
  row-label greps, structural only per the no-text-assertion law's
  spirit — the gate's own output labels are its structural interface,
  matching the existing self-test style).

**Risk**: backticks inside legit prose cells (e.g. erratum notes in
plan prose) are now stripped before matching — acceptable; prose never
matched correctly anyway, and errata live outside §4/§6 tables.

## 2. #1186 — canonicalize-before-scan: covering self-test + closure

The fix landed (`a1069da6`; see decision 1). Add **self-test 14**:
invoke the script with a *relative* `--root` (cd into the repo root,
`--root .`), against the fixture-pin fixture, and assert the fixture
tree is pruned (the fixture's miniature `seam_inventory.rs` never wins
`find_evidence` — same structural probe as self-test 13's
symlink-spelled variant). Comment on #1186 with the landed SHA + the
new self-test; close at merge (dispatcher per the runbook; stage 3 may
close after PR-open if the dispatcher convention allows — default:
report, dispatcher closes).

## 3. #1164 — the day-review plan's W-1 Commands cell

`.artifacts/plans/2026-11-18-day-review-ux-plan.md:145` — replace the
Commands cell `UiRequest variants` with `—` (decision 4: the row has no
command delta; `ReviewWalkPlanner` → `UiEffect::ReviewWalk` is view
flow). Add a dated erratum note at the plan's erratum/back-matter
section: "*2026-11-22 (E1, #1164): W-1's Commands cell carried prose
('UiRequest variants'); the walk is flow-only — cell corrected to `—`
(the gate scans declared tokens only).*" Verify with
`scripts/ux-coverage.sh <plan> --gate` → W-1 leaves MISSING, no new
 MISSING introduced beyond the plan's honest open set.

## 4. #1217 — the habit-family plan's W-11/W-12 rows

`.artifacts/plans/2026-11-21-habit-family-ux-plan.md:505–506`:

- **W-11**: callback + command cells are prose ("`— (binding rows)`",
  "— (bindings only, verbs route via W-1..W-8)") → both become bare
  `—` (the row binds existing routing; no new seams, no new commands).
  The ActionId cell lists seven comma-separated backticked tokens; the
  gate resolves one `ActionId::`-prefixed token per cell. Amendment:
  keep ONE canonical token in the ActionId cell
  (`ActionId::HabitSkip` — the row's representative) and move the
  full seven-verb enumeration into the row's prose column (the
  "Today context rows" description cell) or the row's note line, so
  the gate greps a single well-defined token. Erratum note dated
  2026-11-22 (E1, #1217) explaining the one-token-per-cell grammar.
- **W-12**: strip the parenthetical annotations after the `—` /
  tokens ("(pre-existing …)", "(classification changed per R-7)") —
  the annotations move to the erratum note verbatim so no
  information is lost; cells become `—`, `ActionId::Undo`,
  `StateCommand::UndoLast` (already resolvable forms).
- Post-edit gate run must exit 0 for the plan's found set (the HF3
  rows already resolve FOUND since 9fa57c75; W-11/W-12 must not
  regress them — they are shared rows).

## 5. #1236 — the small-gated plan's W-5/W-6/W-7 Test cells

`.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md:565–567`:
rewrite the three Test cells from the short
`journeys/small_gated_journeys.rs::us4_s1…` form to the crate-prefixed
`crates/taskboard-ui-slint/tests/journeys/small_gated_journeys.rs::…`
form (the exact edit the PR2 erratum made for W-14..W-16). Dated
erratum note (2026-11-22, E1, #1236). Acceptance from the issue:
`scripts/ux-coverage.sh <plan> --gate` exits 0 on the branch with all
tests landed.

## 6. #1237 — the plan-place seed echo: one helper, three sites

`crates/taskboard-ui-slint/src/adapter.rs`: the mint + route + seed
echo tail is written three times — `activate_plan_cursor`
(:5951–5967), and the `on_plan_box_activated` / `on_plan_pool_activated`
seam installs (:13199–13235, which re-state the tail inline on the
adapter-side).

**Design**: private `fn begin_plan_place(&mut self, window: &AppWindow,
task: TaskId)` owning: `self.plan_target = Some(task)`;
`self.route(window, ActionId::PlanBeginPlace)`; then the seed echo —
`set_plan_time_text(SharedString::from(placed_row_time_text))` for the
placed row vs `set_plan_time_text(SharedString::new())` for the pool
row. The placed-vs-pool difference is one parameter
(`seed: Option<SharedString>`-shaped — exact signature decided by the
call sites' data at implementation time; keep it a plain enum-free
`Option` unless the A1 lens says a newtype is warranted, which it does
not for an internal two-caller helper). All three call sites delegate.
Behavior-identical; the planner journeys pin the seed semantics
(§testing gate covers them).

## 7. #1172 — three journeys members onto the B4a harness

Migrate the private mount preambles onto
`crates/taskboard-ui-slint/tests/support/harness.rs`:

- `journeys/keymap_case_seam.rs` — private `fn mount(` (raw
  ArcSwap/FakeEngine channel wiring) → `Harness::mount` /
  `mount_with_settings`.
- `journeys/focus_coverage.rs:244` — `fn mount(...) -> Mounted` → the
  harness equivalent; the local `Mounted` struct dissolves into the
  harness's return type (assertion helpers move to method calls).
- `journeys/usability_components.rs:25` — `fn mount_populated(` → the
  harness; if the harness lacks the "populated" fixture preset, extend
  the harness with a constructor that mounts the populated state
  (harness extension is behavior-additive, not a weakening).

Delete the three private preambles. No assertion changes; the suites
must pass untouched. This retires the SonarCloud duplication exposure
(#1062 class) and puts the members under future harness fixes.

## 8. #1183 — the tombstone arm/confirm seam journey leg

Add to `journeys/board_ceremonies_journeys.rs`: a UI-level leg that
hand-builds an `AppState` containing a tombstoned board (decision 5),
mounts through the B4a harness, arms the ceremony (keyboard path —
`BoardArchive`, the seeded-refusal posture), drives confirm, and
asserts **typed outcomes**: the engine's tombstone refusal surfaces
(the typed error state in the snapshot/effect), and nothing changed
(board set identical, no `StateCommand::` delta emitted beyond the
refusal). One verb (`BoardArchive`) suffices to honor AC-3's
seam-test claim — `BoardReset`/`BoardReconcile` ride the same
liveness-map posture and stay engine-side pinned (recorded in the
test's doc comment). Structural assertions only; no text matching.

## 9. #1250 — subsumption verdict

**Subsumed via the convention route; no gate-matcher work.** The
collision (#1250) is generic low-band story numbers (`us1_s1`-style)
matching earlier plans' tests tree-wide. E1 already touches the gate
grammar (§1); adding plan-scoped matching would be a second,
independent matcher redesign — out of chunk shape. Instead:

- This plan records the **per-round decade-band law**: before
  numbering stories, the writing J-agent samples the next unused
  `US-<decade>` band (grep the journeys tree for the
  `us<N>_s` prefix family; pick a band with zero existing `fn us<N>_`
  members). Round 5's used bands are 1–7 and 21–26; round 6 starts
  sampling from the first fully-unused decade.
- A cross-reference line lands in `docs/ux_planning.org`
  (§ journey-spec / step-token law area, near the #920/#965 notes)
  pointing at this section as the convention's home.
- Comment on #1250 with the plan permalink; close at merge. The
  plan-scoped-matching alternative is NOT filed separately — the
  convention retires the practical failure mode; if it recurs, that
  recurrence is the new evidence for the matcher work.

## 10. Verification plan

1. `scripts/ux-coverage.sh --self-test` — existing 13 self-tests plus
   the new §1 (two) and §2 (one) cases, all PASS.
2. Gate runs against the three amended plans: day-review (§3),
   habit-family (§4), small-gated (§5) — expected honest outcomes
   recorded per plan; the small-gated run must exit 0.
3. `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe; background + poll) — adapter helper (§6), harness
   migration (§7), tombstone leg (§8) covered by the journey suites.
4. `cargo fmt --all -- --check`, `cargo clippy --workspace
   --all-targets -- -D warnings` via the pre-commit hooks.
5. No `tokio::time::sleep`, no text assertions, no quality ceilings —
   the tombstone leg asserts the typed refusal variant and the
   unchanged board set.

## 11. Backlog issues filed by stage 1

- **#1266**: `backlog:` ux-coverage dash-cell grammar —
  annotation-bearing dash cells ("`— (binding rows)`") are not
  `is_absent`; consider treating a leading em-dash as absence. Filed
  because §0 decision 3 keeps the gate strict and amends plans
  instead; the relaxation idea needed a home. (labels: `testing`,
  `p3`)

## 12. Commit sequence (stage 2)

1. `fix(scripts): strip inline code spans in ux-coverage cellget (#1181)`
   + self-tests.
2. `test(scripts): the relative --root spelling still prunes the fixture (#1186)`
   — self-test 14.
3. `docs(plans): token-normalize the day-review, habit-family and small-gated wiring rows (#1164 #1217 #1236)`
   — plan edits + dated errata + the ux_planning.org cross-ref (#1250).
4. `refactor(ui-slint): extract begin_plan_place from the three seed-echo sites (#1237)`
5. `test(ui-slint): migrate the three remaining private mount preambles onto the B4a harness (#1172)`
6. `test(ui-slint): the tombstone arm/confirm typed-refusal journey leg (#1183)`

Squash/amend freely before PR-open (commit hygiene law §8).

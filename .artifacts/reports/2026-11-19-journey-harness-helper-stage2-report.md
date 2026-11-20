# Chunk B4a stage-2 report — journey-harness helper + headless-mix split

- Round: backlog-reduction round 3, chunk B4a, branch
  `refactor/journey-harness-helper`, worktree
  `taskboard-WT-refactor-journey-harness-helper`.
- Normative inputs: scope report §B4, dispatch plan §1/§10, plan
  `.artifacts/plans/2026-11-19-journey-harness-helper-plan.md`
  (D-1016, D-1062, commit slicing §5, measurement method §4).

## Landed commits (stage 2)

1. `5230a566` test(ui-slint): the journey harness helper module —
   `tests/support/harness.rs`: the `Harness` moves verbatim out of
   `journeys.rs` (struct, all methods, the two dummy broadcast
   receivers), gains the `run_journey` preamble family
   (`run_journey`, `_with_settings`, `_with_files_sync`, `_full`,
   `_no_escape*`), all reading `crate::HEADLESS_RENDERER`.
2. `49eb78ae` test(ui-slint): the journeys binary speaks one platform —
   the 9 `headless_renderer: false` preambles convert onto
   `run_journey`/`run_journey_with_settings` (D-1016). No fallback
   invoked.
3. `2d1ea8ba` fix(ui-slint): the stay-on-today env seam becomes a
   per-mount argument — see "Finding F1" below.
4. `6b23ff5e` test(ui-slint): the journey preambles run through the
   helper — the mechanical `run_journey` adoption across the remaining
   `journeys.rs` sites; non-Harness `driver::run` sites (direct-driver
   tests, 3 multi-mount closures) keep `driver::run` with
   `headless_renderer: crate::HEADLESS_RENDERER` instead of a literal.

## Acceptance evidence

- Plain cargo test (the dispatch law (c) oracle), default parallel
  threads, one process:
  - `cargo test -p taskboard-ui-slint --test journeys`
    → **408 passed; 0 failed** (wholesale).
  - `cargo test -p taskboard-ui-slint --test journey_suites`
    → **109 passed; 0 failed; 8 ignored** (the golden-regen guards).
- Law (c): no converted test's outcome changed; **the pre-approved
  `rendered_journeys.rs` fallback was NOT invoked**. All 9 false-mode
  conversions pass under the embedded rasterizer, exactly as the
  plan's §2 snapshot-tier check predicted (no golden-tier use).
- No test name, journey token, or assertion body changed (byte-level;
  the ux-coverage paths keep resolving).
- Workspace gates: `scripts/run-quality-gates.sh cargo nextest run
  --workspace` — result recorded in the stage report handoff section /
  dispatcher-owned §10 suite.

## Duplication delta (plan §4's method)

5-line shingle duplication (whitespace-normalized, comment/blank
lines excluded) over `tests/journeys.rs` + `tests/journeys/*.rs`:

| metric | before (613ce54f) | after (6b23ff5e) |
|---|---|---|
| duplicated shingle lines | 21 155 | 11 605 (−45%) |
| total 5-line shingles | 25 102 | 22 716 |
| file lines (`journeys.rs` alone) | 25 616 | 23 136 |

Secondary metric (plan §4): `journeys.rs` per-test `DriverOptions`
mode literals **404 → 0** (395 `true` + 9 `false` before; after: 0
literals — 358 `run_journey*` call sites plus 46 `driver::run` sites
carrying `headless_renderer: crate::HEADLESS_RENDERER`, the binary
const). No `Harness::mount` site carries its own options anywhere.
Tertiary (SonarCloud new-code %) lands on the PR analysis — PR body.

## Findings

**F1 (fixed in-chunk, #1016 residue):** the first honest plain
`cargo test --test journeys` run exposed the real reason the binary
failed wholesale: not only the OnceLock mode mix. 
`the_desktop_mount_never_rotates` set the process-global
`TASKBOARD_JOURNEYS_STAY_ON_TODAY` and never unset it, poisoning every
later journey's mount (no board escape → cascading failures; with the
mix aborted, one early `OPTIONS` panic had masked everything behind
`SendError`/`RecvError`). The env seam (one `set_var`, five
`remove_var` defensive dances) is replaced by an explicit per-mount
`Harness::mount_no_escape` / `run_journey_no_escape`; the three today
legs use it; assertions untouched. This is a fixture-hygiene fix, not
a behavior change; law (c) unaffected (all assertions identical).

**Mechanical-rewrite residue (recorded, in-scope):** 42 `driver::run`
call sites did not match the single-mount preamble shape (4
direct-driver tests needing `&mut SlintDriver`, 3 multi-mount
closures — the kiosk-vs-desktop matrix legs, plus odd-arg sites).
Their options now read `crate::HEADLESS_RENDERER`; #1016's mix is
fully gone (0 literals) while the remaining setups stay bespoke.

**Journey_suites members (recorded, per plan D-1062):** none adopted
the helper. Every member mounts through file-local bare-mount helpers
(`mount_populated`, `capture_golden`, pinned-clock mounts) that do not
have the `Harness::mount` shape; `habits_journey` carries its own
minimal `Harness` (mounts without guard/escape), `focus_law_journeys`
and `board_move_journeys` use the `JourneyHarness` Preset-keymap seed
— the plan explicitly excludes a forced rewrite for these. The helper
exists for B4b's seven pins to compose with.

**#1062 comment (per plan):** `focus_journey.rs`'s `JourneyHarness`
(Preset-keymap shape + board-escape ritual) was deliberately NOT
migrated — B4b-adjacent follow-up; the comment thread on #1062 carries
it (stage-3/dispatcher action per plan §3).

## Deviations from plan §5 slicing

- Commit 2 grew the env-seam fix commit (3) — the acceptance gate
  ("plain cargo test wholesale green") was unreachable without it;
  the fix is confined to the journey files + `tests/support/`.
- `run_journey` ships as free functions (`support::harness::run_journey*`)
  rather than `Harness::run_journey` associated fns — the plan's §3
  sketch showed a free `pub fn run_journey`; kept that shape.
- `journeys.rs` shrank 2 480 lines net; the harness gained ~150 lines
  (the preamble family + the no-escape variant).

## Stage-3 review addendum (2026-11-20)

- Rebase onto the moved `origin/main` (R4's consolidation + the Bui
  residue fixes) needed a hand resolution in four `journeys.rs` tests
  (`every_task_view_cell_and_chip_carries_its_info_tip`,
  `the_track_form_quick_lists_open_per_field_and_dismiss`,
  `us1_s1_picker_close_restores_the_invoking_field`,
  `us1_s3_commit_exit_restores_the_row_anchor`): main's W4/W-2
  semantic edits ported onto the converted preamble shape. The delta
  versus the pre-rebase branch is exactly main's recorded changes; law
  (c) holds.
- Code-quality fix (folded into the env-seam commit): the
  `run_journey_no_escape`/`run_journey_no_escape_with_settings`
  preamble pair had zero call sites (the three today legs mount
  through `Harness::mount_no_escape` inside their recorded
  `driver::run` residues) — removed as speculative generality;
  `run_journey_full` demoted to private (one internal caller);
  `EscapeBoard` un-pubed. The §"Landed commits" wording "the preamble
  family (`…_no_escape*`)" therefore describes the pre-review shape;
  the surviving today-leg seam is the `Harness::mount_no_escape`
  method pair.
- Post-fix evidence: plain `cargo test --test journeys` → 408 passed;
  `--test journey_suites` → 115 passed, 8 ignored (up from stage 2's
  109 — the rebase brought main's new tests); ux-coverage gate on the
  committed keyboard-focus plan still resolves MISSING: 0.

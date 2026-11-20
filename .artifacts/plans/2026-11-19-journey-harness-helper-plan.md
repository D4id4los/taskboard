# Chunk B4a — journey-harness helper + headless-mix split — implementation plan

- **Round**: backlog-reduction round 3, chunk B4a (scope report
  `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md`
  §B4; dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  §10 — normative).
- **Branch**: `refactor/journey-harness-helper` (this worktree, branched
  off the fetched `origin/main` at c1e07796).
- **Covers**: #1062 (shared journey-harness helper — the SonarCloud
  new-code duplication driver) and #1016 (the `journeys` binary mixes
  `DriverOptions { headless_renderer: true/false }`; plain
  `cargo test --test journeys` fails wholesale on the `driver::run`
  OPTIONS `OnceLock` assert).
- **Acceptance** (dispatch plan §10): plain `cargo test --test journeys`
  passes wholesale — the OnceLock mix is gone; the helper absorbs the
  duplicated setup (measured — §4's method); the journey corpus green
  under nextest AND plain cargo test; diffs confined to the journey
  files + `tests/support/` + the docs that name them (no Cargo.toml
  change under the recorded decision — §2).
- **Round laws**: (a) typed outcomes (no new text assertions anywhere);
  (c) the corpus is behavior-identical — any journey test whose outcome
  changes under the split/convert is a finding; the drop-option's
  snapshot-tier check is recorded in §2 and it is negative;
  (d) out-of-scope findings become `backlog:` issues — §6.
- **Journey gate**: none — test-support internals only (no
  user-visible capability; `docs/ux_planning.org` exemption applies).

## 0. Landed shape this chunk anchors to (read first)

R4's test-binary consolidation has merged. The ui-slint tests tree:

- `tests/journeys.rs` — **unchanged** as its own root binary; still
  declares `mod support;` directly and its own
  `pub(crate) const HEADLESS_RENDERER: bool = true;`. This is the only
  remaining **mixed** binary: 9 `headless_renderer: false` sites among
  ~500 `true` sites (#1016's open scope lives exactly here).
- `tests/journey_suites.rs` — the merged root for the seven members
  under `tests/journeys/` (`focus_law_journeys`, `board_move_journeys`,
  `habits_journey`, `golden_visuals`, `keymap_case_seam`,
  `usability_components`, `focus_coverage`), one shared
  `#[path = "support/mod.rs"] mod support;`, members use
  `crate::support`. One-platform (`HEADLESS_RENDERER: true`), plain
  cargo test green.
- `tests/probes/` — members of `probe_composition.rs`
  (`headless: false`) and `probe_pixel.rs` (`headless: true`); a
  `false` platform is legitimate *in its own process* — the disease is
  the mix inside one binary, not the mode.
- `tests/support/` — `driver.rs` (`DriverOptions`, `SlintDriver`,
  `JourneyDriver`, the `run` OnceLock pair SENDER/OPTIONS, and the
  self-tests that read `crate::HEADLESS_RENDERER`), `fake_engine.rs`
  (`FakeEngine::channel` + `settle`), `fixture.rs` (`golden_state` &
  co), `focus_journey.rs` (`JourneyHarness::mount` — the r9 directional
  dedup, the *seed* of this chunk's helper), `guard.rs`, `health.rs`,
  `golden_compare.rs`, `pixel_probe.rs`.
- No `[[test]]` sections in `crates/taskboard-ui-slint/Cargo.toml` —
  targets are auto-discovered roots (subdirectories are not
  auto-discovered, which is what makes R4's layout work).

A **new root binary still plugs into this layout** (a top-level
`tests/<name>.rs` with `#[path = "support/mod.rs"] mod support;` and a
root `HEADLESS_RENDERER` const) — that is the recorded fallback in §2.

## 1. The duplication inventory (#1062's evidence)

Three strata, all measured on this worktree:

1. **The harness-class duplicate**: `tests/journeys.rs` lines ~41–120
   define a private `struct Harness { driver, handle, engine, guard,
   files_tx, requests_rx }` with `mount`, `mount_with_settings`,
   `mount_with_files_sync`, `mount_full` — the same fixture-continuity
   preamble `tests/support/focus_journey.rs` already generalized for
   the directional journeys (`JourneyHarness::mount`: one `golden_state`
   fixture, one driver, one mount, bind, escape). Two hand-rolled
   copies of the same shape; every new journey file re-rolls a third
   (the SonarCloud finding's mechanism: PR #1055 3.5% vs 3%, PR #1061
   6.3%).
2. **The per-test preamble**: ~500 tests repeat the
   `let (fixture, state) = fixture::golden_state();` +
   `support::driver::run(DriverOptions { headless_renderer: <b> },
   |_| { let mut harness = Harness::mount(SlintDriver::default(),
   state); ... })` block — 5–8 lines of identical scaffolding per test
   across `journeys.rs` and the `journey_suites` members.
3. **The option-literal**: every test carries the
   `headless_renderer:` literal even though the binary-level const
   (`HEADLESS_RENDERER`) already records the one-platform decision —
   the literal is what made the #1016 mix possible.

## 2. Decision D-1016: convert, do not split, do not drop

**Recorded snapshot-tier check (the drop-option's evidence)**: exact
per-test slicing of the 9 `headless_renderer: false` tests
(`a_palette_activation_shows_the_task`, `the_switcher_drives_and_tracks_the_view`,
`the_palette_input_row_stays_one_line`,
`emacs_double_c_g_cancels_from_the_today_chain`,
`the_control_walk_reaches_fields_buttons_and_the_picker`,
`scroll_actions_page_the_focused_area`, `the_menu_curates_itself`,
`a_right_press_outside_dismisses_the_date_picker`,
`us2_s1_the_setter_opens_below_the_anchor_never_covering_it`) shows
**zero** calls to `snapshot()`, `take_snapshot`, `golden_compare`, or
`pixel_probe` — no test in the false mode touches the golden tier. The
mode's only purpose in this binary is historical: element queries,
stroke/click injection, and geometry (`absolute_position`, `size`,
`focused_rect`) all work under the embedded software rasterizer
(`headless_renderer: true`) — the `journey_suites` members and ~500
true-mode tests in `journeys.rs` itself use those same verbs.

**Decision**: convert all 9 sites to `headless_renderer: true` (each
site is also rewritten onto the §3 helper, so the option literal
disappears). One binary, one platform; plain `cargo test --test
journeys` becomes honest; no new Cargo.toml target; no coverage
dropped; the ux-coverage evidence paths that name the journeys binary
keep resolving unchanged (the #1070 lesson — no rename).

**Fallback (only if a converted test's outcome changes — corpus law
(c))**: the change is a finding. Diagnose first; if the false mode is
genuinely load-bearing for a test's semantics, move the affected tests
verbatim to a new root `tests/rendered_journeys.rs`
(`#[path = "support/mod.rs"] mod support;`,
`pub(crate) const HEADLESS_RENDERER: bool = false;`) — the recorded
R4-plug-in shape, plus the tooling-path sweep (grep scripts/docs for
`--test journeys`-style references and extend to the new name). This
fallback is a recorded contingency, not a parallel workstream.

**Accepted residue**: after conversion, `driver::run`'s false branch
is exercised only by `probe_composition.rs` and (contingently) the
fallback binary. That is test-support coverage, not product coverage;
recorded here so nobody "fixes" it with a contrived test.

## 3. Decision D-1062: the helper API

New module `tests/support/harness.rs`, consumed via `crate::support`
by both roots. It generalizes `focus_journey.rs`'s seed **and**
absorbs `journeys.rs`'s private `Harness` (the superset: guard,
files-sync watch, request-lane drain). Public surface:

```rust
// tests/support/harness.rs
pub struct Harness {
    pub driver: SlintDriver,
    pub handle: UiHandle,
    pub engine: FakeEngine,
    pub guard: Guard,
    pub files_tx: Option<tokio::sync::watch::Sender<FilesSyncChip>>,
    /* request-lane receiver half, as today */
}

impl Harness {
    pub fn mount(state: AppState) -> Self;                       // SlintDriver::default() inside
    pub fn mount_with_settings(state: AppState, settings: UiSettings) -> Self;
    pub fn mount_with_files_sync(...) -> Self;                   // signatures moved verbatim from journeys.rs
    pub fn mount_full(...) -> Self;
}

/// The per-test preamble: one fixture, one run, one mount.
/// Options come from `crate::HEADLESS_RENDERER` — no per-test literal.
pub fn run_journey<R>(
    state: AppState,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R;
```

Design rules:

- `run_journey` takes the built `AppState` (the fixture stays at the
  call site — tests pick `fixture::golden_state()` or a bespoke
  builder deliberately; folding the fixture in would hide which one a
  test speaks). The `driver::run` SENDER/OPTIONS OnceLock mechanics are
  untouched.
- `journeys.rs` keeps the name `Harness` (mechanical
  `use support::harness::Harness;`); test names, journey tokens, and
  assertion bodies are byte-identical — the ux-coverage gate matches
  names/tokens and the corpus is behavior-identical (law (c)).
- `focus_journey.rs` (`JourneyHarness`) is **not** migrated in this
  chunk — its mount has a `Preset`-keymap shape and a board-escape
  ritual the two directional files rely on; migrating it is B4b-adjacent
  follow-up (comment left on #1062, not a new issue — the scope report
  already carries "#1062's non-harness remainder").
- `journey_suites` members adopt `run_journey`/`Harness` for their
  per-test preambles where the shape matches (habits/focus_law/
  board_move mount differently — no forced rewrite); the Sonar finding
  was about *new* files re-rolling setup, and the helper now exists for
  B4b's seven pins to compose with (dispatch plan §11: hand-rolled
  setup becomes a finding there).
- A `HEADLESS_RENDERER: bool = true` root const already exists in
  `journeys.rs` — kept; `run_journey` reads `crate::HEADLESS_RENDERER`
  exactly as `driver.rs`'s self-tests already do, so a future second
  binary inherits correctness by construction.

## 4. Duplication measurement method

- **Primary (the recorded equivalent, local)**: a 5-line-shingle
  duplication count over `tests/journeys.rs` +
  `tests/journeys/*.rs` (python script, run in the stage-2 report;
  before/after numbers land in the stage-2 report — this plan records
  the method, not the result).
- **Secondary**: count of `Harness::mount(` sites carrying their own
  `DriverOptions` literal (before: 9 false + ~491 true in journeys.rs;
  after: 0 — `run_journey` owns the options).
- **Tertiary (post-PR)**: SonarCloud new-code duplicated-lines % for
  the journey files on the PR's analysis — the original gate metric
  from #1062 (PR #1055: 3.5%; PR #1061: 6.3%). Record in the PR body.

## 5. Implementation order (stage 2)

1. **Commit 1** — `test(ui-slint): the journey harness helper module`:
   add `tests/support/harness.rs` (the generalized `Harness` + `run_journey`),
   move `journeys.rs`'s private `Harness` onto it (verbatim bodies),
   `journeys.rs` imports via `mod support` + `use support::harness::...`.
   Corpus green under nextest; plain `cargo test --test journeys` still
   red at this point (the 9 false sites remain) — expected.
2. **Commit 2** — `test(ui-slint): the journeys binary speaks one platform`:
   rewrite the 9 false-site preambles onto `run_journey` (which reads
   `crate::HEADLESS_RENDERER` = true). Acceptance evidence: plain
   `cargo test --test journeys` wholesale green, plus the full
   `cargo test --test journey_suites` for the neighbor.
3. **Commit 3** — `test(ui-slint): the journey preambles run through the helper`:
   mechanical `run_journey` adoption across the remaining ~491 sites in
   `journeys.rs` and the matching-shape `journey_suites` members
   (sed-assisted; every test name/token untouched; diff reviewed by
   shingle count + spot greps, corpus law (c) enforced by the suite
   being green).
4. **Commit 4** — `docs(plan): record the duplication measurement`
   (stage-2 report numbers appended to the plan/report artefact).

Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
run --workspace` PLUS `cargo test --test journeys` (plain, wholesale)
and `cargo fmt --all -- --check` / clippy as usual. Long commands
background + poll; commit before any wait; defer per the flock rule.

## 6. Out-of-scope findings → backlog issues

Filed during planning (numbers in the stage report):

1. `taskboard-WT-feat-reflection-domain` sits next to the project root
   but is not a registered git worktree (worktree-doctor WARN,
   2026-11-20) — housekeeping/dx.
2. If stage 2 observes any converted-test behavior delta, that is a
   *finding* (law (c)), handled per §2's fallback, not an issue.

## 7. Risks

- **R1 — false-mode semantics**: a subtle rendered-vs-headless
  behavior difference (coordinate rounding, paint-dependent property
  fires) flips a converted test. Mitigation: law (c) treats it as a
  finding; §2 fallback is pre-approved.
- **R2 — mechanical rewrite size**: ~500 sites in a 25.6k-line file.
  Mitigation: commits 1–2 are surgical and reviewable; commit 3 is
  sed-mechanical with a zero-semantic-edit review checklist (stage 3's
  trap (a)).
- **R3 — ux-coverage token paths**: any accidental test/token rename
  breaks the gate. Mitigation: names byte-identical; stage 3 greps the
  scripts.
- **R4 — plain-cargo-test flakes from other binaries** (the
  `app_services` cross-member interference noted on #1016 is app-side,
  out of this chunk's diff): this chunk's acceptance is scoped to
  `--test journeys` (and `--test journey_suites` as the neighbor);
  app-side residuals stay on #1016's comment thread for their own
  chunk.

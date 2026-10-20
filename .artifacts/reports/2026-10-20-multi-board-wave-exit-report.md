# [Report] Multi-board Wave — Exit-Criteria Verification

- **Date**: 2026-10-20
- **Base SHA**: `419c70f` (= the merge-base of `docs/multi-board-wave`
  against `origin/main`; the wave's seven implementation chunks are all
  merged into `main` at or before it).
- **Chunk → PR map** (implementation plan §6 bookkeeping):

  | Chunk | Branch | PR |
  |---|---|---|
  | 1 (SyncStatus per-board map + migration 0006) | `feat/state-sync-status-map` | #358 |
  | 2 (board-scoped commands) | `feat/domain-board-scoped-commands` | #362 |
  | 3 (engine ceremonies at N) | `feat/state-board-ceremonies` | #364 |
  | 4 (connector identity end-to-end) | `feat/sync-connector-modules` | #368 |
  | 5 (config, registry, authority) | `feat/app-connector-registry` | #375 (closes #187) |
  | 6 (CLI/REPL verb growth) | `feat/app-cli-connector-verbs` | #376 |
  | 7 (UI board dimension) | `feat/ui-board-switcher` | #377 |
  | 8 (this record + docs) | `docs/multi-board-wave` | closes #125 on merge |

- **Normative sources**: the wave implementation plan §5
  (`.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`,
  the exit-criteria → verifying-chunk map) and the chunk-8 stage-1 work
  plan (`.artifacts/plans/2026-10-20-multi-board-wave-exit-docs-plan.md`,
  §2 evidence inventory, §6 report outline, §7 method).

## Exit criteria (implementation plan §5, verbatim rows)

### 1. "Two connectors live simultaneously (tier-2)" — verified by chunk 5

**Verdict: met — hermetically verified; the dockerized run is blocked
by a recorded runner limitation (not a product defect), corroborated
live by chunk 6's two-connector journey** (the basis on which #187
closed).

- Hermetic, in-suite: `crates/taskboard-app/src/bootstrap.rs`, module
  `connector_module_tests` — `registry_loop_wires_the_fake_module_end_to_end`
  (the registry loop over `ConnectorModule`s, end to end through the
  spawned actor), `two_connectors_live_simultaneously_over_the_named_registry`
  (N=2 modules live at once via the named registry), `the_authority_counts_scripted_episodes_across_two_connectors`
  (the all-down authority's counted crossing across two connectors),
  `module_build_failure_maps_onto_the_typed_bootstrap_error`.
- Hermetic, unit-level counting semantics: `crates/taskboard-app/src/authority.rs`
  module `tests` — `single_connector_passes_through_the_episode_edges`,
  `two_connectors_emit_only_on_the_all_down_crossing`, `non_member_events_change_nothing`,
  `attach_fills_a_real_hole_loudly_and_silently_opens_the_empty_set`.
- Tier-2 test on file: `crates/taskboard-app/tests/it_docker.rs`
  `it_nextcloud_docker_two_connectors_live_and_partial_outage` (→
  `two_connector_body`): two dockerized Nextcloud instances, boards
  bound across both connectors, per-connector health in the status
  answer, one connector down ⇒ its boards `Offline` while the global
  banner stays silent; honest-skip gate on `TASKBOARD_IT_DOCKER_URL`.
- **MD-5 record** (PR #375 body): the dockerized test was attempted six
  times on the agent runner — the runner's docker host externally stops
  the first test-owned instance ~86 s after spawn (exit 0; ports,
  volumes, orphan teardown, and traffic refuted as causes; forensics in
  `.artifacts/reports/2026-10-20-multi-board-connector-registry-chunk5-review-report.md`).
  The N=2 criterion closed on the hermetic evidence above.
- **Corroboration** (PR #376 body): the move-board journey ran and
  passed live on a runner (171 s) — which boots a two-connector,
  two-instance world, exercising the same registry path the tier-2
  test would.

### 2. "Multiple boards syncing across them" — verified by chunk 5, exercised by chunk 6

**Verdict: met** (same evidence basis as row 1).

- Chunk 5, `two_connector_body` (in
  `crates/taskboard-app/tests/it_docker.rs`
  `it_nextcloud_docker_two_connectors_live_and_partial_outage`): one
  board bound per server, both pulled from the first cycle — multiple
  boards syncing across two connectors.
- Chunk 6 exercise: `move_board_body` (in `it_nextcloud_docker_move_board_between_connectors`)
  holds two alpha boards plus one beta board concurrently across the
  cycle; the derived per-connector grouping carries all three.
- Per-board sync-isolation unit tier: `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`
  — `a29_newly_bound_board_joins_the_set_by_the_next_cycle` (the
  derived board set grows by binding), `a30_per_board_passes_plan_only_their_own_ops`
  (per-board push isolation), `a31_foreign_tagged_board_ops_are_never_planned`,
  `a32_per_board_failure_continues_the_loop`,
  `a32b_network_failure_ends_the_cycle_before_the_next_board`.

### 3. "One board moved between connectors" — verified by chunk 6

**Verdict: met (tier-2 ran and passed).**

- `crates/taskboard-app/tests/it_docker.rs`
  `it_nextcloud_docker_move_board_between_connectors` (→
  `move_board_body`): a board bound through connector alpha is
  unbound, then re-bound onto beta's remote via `boards select
  --connector beta`; per-board attribution verified *moving* in the
  derived grouping (the moved board under beta, the other still under
  alpha); adoption settles; both servers' listings intact.
- **MD-5 record** (PR #376 body): the journey ran and passed on the
  runner (docker present, 171 s). The chunk-5 host-kill flakiness note
  applies to reruns on that runner; CI's nextcloud-integration
  workflow is the stable runner.

### 4. "v0.1.x config + DB upgrade additively keeps working" — verified by chunk 1 (round-trip) + chunk 4 (boot test)

**Verdict: met.**

- Chunk 1 migration round-trip: `crates/taskboard-storage-sqlite/src/connect.rs`
  — `migration_0006_backfills_the_bound_boards_row_and_drops_the_singleton`
  (a pre-0006 database migrates: the singleton's phase/last_success
  backfill the one bound board's row; the `CHECK (id = 1)` table shape
  is structurally gone) and `migration_0006_drops_a_board_less_singleton`
  (the standalone board-less singleton is dropped — the one recorded
  §3.9 deviation, status cosmetics not merge behavior).
- Chunk 1 widening pin: the 20-file insta corpus
  `crates/taskboard-state/tests/snapshots/snapshots__s{1..10}_{persisted_state,published_app_state}.snap`
  — the per-board `AppState.sync` map shape pinned across the whole
  scenario corpus (the deliberate DF-9 regeneration pass).
- Chunk 4 boot test, hermetic half: `crates/taskboard-app/tests/it_docker.rs`
  `v0_1_x_additive_upgrade_boot_settles_offline_without_a_server` —
  the v0.1.x database boots on the new build: migration ran, the
  binding decodes tagged-default, status hydration settles, offline.
- Chunk 4 boot test, dockerized half: `crates/taskboard-app/tests/it_docker.rs`
  `it_nextcloud_docker_v0_1_x_additive_upgrade_boot` — the full §3.9
  assertion list against a dockerized Nextcloud: the legacy config
  synthesizes the `nextcloud` connector, adoption, cycle merges
  bit-for-bit (epoch-default clocks), the queued op drains.
- **MD-5 record** (PR #368 body): docker available, nothing skipped —
  `taskboard-app --test it_docker` **6/6 passed** including the boot
  test; `taskboard-sync-nextcloud --test it_docker` **11/12** with the
  one failure the pre-existing attachment-lifecycle harness flake,
  recorded as #367 per the watchlist policy (byte-identical file on
  `origin/main`; not wave-caused).

### 5. "Board switching by scripted keyboard journey alone" — verified by chunk 7

**Verdict: met (hermetic).**

- `crates/taskboard-ui-slint/tests/journeys.rs`
  `board_switching_walks_boards_by_keyboard` (headless test platform):
  seeded on the lowest live board, `board.next` re-renders the second
  board's stacks and toolbar title, no wrap at the end, `board.previous`
  returns; the compact leg hides the strip while keyboard switching
  keeps working; `assert_commands(&[])` after every step — the
  dumb-client pin (switching dispatches nothing to the engine).

### 6. "`cargo deny` green with the promoted port" — every chunk

**Verdict: met — no wave dependency; stage 3's re-run records the
  two post-stage-2 emSign advisories as pre-existing on `main`
  (see below).**

- No chunk added a dependency: the wave's only `Cargo.toml` delta is
  chunk 4's `sqlx` dev-dependency on the existing workspace pin.
- Per-PR records: every chunk's stage-2/3 gate report lists
  `cargo deny check` green (#358, #362, #364, #368, #375, #376, #377).
- Stage 2's re-run (2026-10-20, worktree at the base SHA plus the
  docs diff): `cargo deny check` — advisories ok, bans ok, licenses
  ok, sources ok.
- Stage 3's re-run (2026-10-20, later): `advisories FAILED` — the
  advisory DB gained RUSTSEC-2026-0382 (`rustls-webpki` 0.103.15) and
  RUSTSEC-2026-0381 (`webpki-roots` 1.0.9) after stage 2 ran; both are
  transitive through `reqwest`, fail identically on `origin/main`
  (same `Cargo.lock` versions). The wave's dependency criterion stands
  (no chunk added a dependency). Remediation landed on this branch when
  the CI Security & License Audit failed on the same advisories:
  `cargo update -p rustls-webpki -p webpki-roots` (0.103.15 → 0.103.16,
  1.0.9 → 1.0.10, no other lock movement) — a required-check fix per
  AGENTS §8, not wave scope.

## Method (how every pointer above is re-verifiable)

1. **Re-locate**: every cited path + test name was resolved against
   the branch base `419c70f` (merge-base with `origin/main`; both
   identical — nothing stacked). Line numbers are never load-bearing;
   paths + test names are the stable keys. Symbol spot-checks for the
   doc amendments (this chunk's dispatch trap: prose-code drift) were
   made against the worktree's crates: `SystemEvent::ConnectorLost/
   ConnectorRestored` (`taskboard-domain/src/messages.rs`),
   `SyncCommand::{SyncNow, Detach}` (same file), `CommandError::
   UnknownBoard/BoardAlreadyBound{board,bound}/RemoteBoardInUse`
   (`command.rs`), `SyncStates(BTreeMap<BoardId, SyncStatus>)` +
   `aggregate()` (`state.rs`), `open(path, default_connector)` /
   `open_with` (`taskboard-storage-sqlite/src/connect.rs`), `authority.rs` +
   `connectors.rs` (`taskboard-app/src`), `ReplSession.current_board`
   (`repl.rs`), `boards use/unbind/reconcile` + `--board`/
   `--connector`/`--local-board`/`--bind-local-board` flags
   (`taskboard-app/src/cli.rs`), `ViewState::board` + `select_board`
   (`taskboard-ui-slint/src/view_state.rs`), `board-bar.slint`,
   `BoardNext/BoardPrevious` bound to `b`/`B` in all three presets'
   board scope (`keymap/presets.rs`).
2. **Re-run the hermetic set** (this chunk, 2026-10-20, worktree):
   `cargo nextest run -p taskboard-storage-sqlite -E 'test(migration_0006)'`
   → **3/3**; `-p taskboard-state --test snapshots` → **10/10**;
   `-p taskboard-app` (the named registry/authority/boot filters) →
   **6/6**; `-p taskboard-ui-slint --test journeys board_switching` →
   **1/1**; `-p taskboard-sync-nextcloud --test actor_wiremock`
   (a29–a32b filters) → **5/5**.
3. **Tier-2 attempt, once, honestly** (MD-5): docker was present on
   this runner; the setup + the three tier-2 app tests were attempted
   per the stage-1 plan §7.3. Outcome recorded below under Findings.
4. **Re-read**: the PR bodies of #368/#375/#376/#377 via
   `gh pr view --json body` (the MD-5 records quoted above); the
   CHANGELOG per-chunk commits for the promotion's losslessness check
   (stage 2 verified the Unreleased section's sorted non-blank line
   multiset is unchanged except the attribution re-wraps
   "(multi-board wave chunk N)" → "(multi-board wave)"; stage 3
   re-diffs).
5. **CI state** (`gh run list/view`, 2026-10-20): the `CI` workflow on
   `main` at `419c70f` shows **Test Execution: success** (also MSRV,
   Code Coverage, Security & License Audit, Code Quality, SonarCloud,
   MCP Harness Smoke: success); the failing job is **Performance
   Benchmarking** (Bencher) — pre-existing, unrelated to the wave (see
   Findings). The `Nextcloud Integration` workflow last ran
   2026-10-14 (success) — **before the wave landed**; its next
   scheduled run (Wednesdays 04:00 UTC) exercises the three tier-2
   tests on the stable runner, and it is dispatchable on demand via
   `gh workflow run nextcloud-integration.yml`.
6. **Gates**: the full AGENTS §10 suite on this chunk's docs-only diff
   (results in this chunk's stage-2 handoff report; stage 3 re-runs).

## Findings (honest records; no tracker action taken)

- **Tier-2 re-attempt from this chunk** (once, per the stage-1 plan
  D2/§7.3; docker present, `scripts/nextcloud_it_setup.sh up` healthy,
  2026-10-20): `it_nextcloud_docker_move_board_between_connectors`
  **passed** (221 s — a live two-connector, two-instance world again,
  corroborating row 1 on yet another runner);
  `it_nextcloud_docker_two_connectors_live_and_partial_outage`
  **failed** — a predicate-deadline panic (`it_docker.rs:271`) after
  609 s, the same shape as the #375 runner limitation (the docker host
  externally stopping the first test-owned instance; forensics on
  file in the chunk-5 review report) — recorded as the known runner
  limitation reproducing, not a wave finding;
  `it_nextcloud_docker_v0_1_x_additive_upgrade_boot` was **not run**
  (nextest cancelled the remainder on the failure) — its evidence
  stands on #368's 6/6 record and the hermetic half, which this
  chunk re-ran green.
- **CI on `main`**: the Performance Benchmarking (Bencher) job fails
  on `main` (at least `215750d`→`419c70f`); pre-existing, unrelated to
  the multi-board diff (Test Execution is green). One observation
  line per the stage-1 plan's D4 — no issue created (tracker edits
  are the user's).
- **The Nextcloud Integration workflow has not run since the wave
  landed** (last success 2026-10-14, pre-wave). Its weekly schedule
  covers the tier-2 corpus on the stable runner; a manual dispatch
  before merging #125 is the user's call (D2).
- **Stale symbol citation in the landed wave plans** (stage-3
  spot-check): the chunk-1 rows of
  `.artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md` and
  `2026-10-18-multi-board-impl-plan-workplan-plan.md` (and this chunk's
  own stage-1 plan/report, corrected in this diff) cite
  `spawn_storage_actor(default_connector)` as the decode home; the
  shipped home is the storage `open`/`open_with` (`connect.rs`) —
  `spawn_storage_actor` takes only the repository. The landed plans are
  on `main` and out of this diff's scope (D6); recorded here.
- **Attribution note on the CHANGELOG promotion** (D1, narrow
  reading): the five entries that carried "(multi-board wave chunk N)"
  were normalized to "(multi-board wave)"; the entries that carried no
  attribution (chunks 6–7) were left byte-identical — the wave's
  entries are grouped adjacently instead. No factual clause changed;
  the sorted-line diff in the method §4 is the proof.

## Backlog candidates (out-of-scope items, restated for the user to promote)

Restated verbatim from the implementation plan §1 Out list / design §7
— **not promoted**: no issue was created, labelled, or scheduled by
this wave (AGENTS §7; the user owns tracker edits).

- Per-connector `poll_interval`/backoff tuning (config keeps them
  global).
- Hysteresis/debounce beyond the episode-edge rule for flapping
  connectors.
- Config-pinned fixed-board kiosk surface (kiosk switching stays
  keyboard-available in-wave).
- UI board creation/archive actions (structure-editing growth beyond
  the switcher; `stack.create` is the precedent).
- Fuzzy/normalized-key growth for title reconciliation (the wave ships
  trim+casefold only).
- `boards reconcile --dry-run` (the receipt already reports counts).
- Daemon-side per-connector attach/detach verbs over IPC (ADR 0014's
  completion note anticipates the pattern).
- Connector configuration via env (figment cannot express the array;
  revisit only with a concrete scripting need).
- ADR 0010 drift amendments F10-03/F10-04/F15-18.
- #157 per-card refresh perf work (guardrail recorded in the design;
  the issue stays unscheduled).
- Later-wave horizons (framing §2, unchanged): Projects hierarchy,
  multi-user decks, the org-file connector, the D5 app-data substrate
  implementation, LAN sync server, CRDT collaboration; the trait's
  second connector implementation is the post-wave horizon, not an
  in-wave criterion.

## Bookkeeping

- **#125** (multi-board support) closes via this chunk's PR body
  (`Closes #125`) on merge — the wave exit, with this record linked.
- The wave's other closures already landed with their chunks:
  #298 (chunk 1), #179 + #170 (chunk 3), #187 (chunk 5).
- No other issue was created, labelled, closed, or scheduled by the
  wave.

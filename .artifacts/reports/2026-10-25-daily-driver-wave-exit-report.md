# Daily-Driver Wave — Exit Verification Report

- **Date**: 2026-10-25 (stage-2 execution day; base `c0465fa` — the
  merge-base of `docs/daily-driver-wave` against `origin/main`, =
  `origin/main`'s tip at verification time; re-fetched and confirmed
  2026-10-25).
- **Kind**: the wave exit verification record for the daily-driver
  implementation round (implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §5's exit-criteria table; chunk plan
  `.artifacts/plans/2026-10-25-daily-driver-wave-exit-docs-plan.md`).
  One section per exit-criterion row: the criterion verbatim, the
  verifying chunks, the named evidence (paths + test names only —
  D9; line numbers below are plan-day references, never
  load-bearing), the MD-5 tier record quoted honestly, the verdict.
- **Verdict vocabulary**: *met* / *met — hermetic with recorded
  live-tier limitation* / *not met* (chunk plan D4).
- **The round in one line**: eleven chunks — ten implementation PRs
  merged (#396 → #433), this chunk's docs + this report — delivering
  the task field family, the per-board app-data sidecar, N-rule
  reminders, standalone alarms, the desktop toast sink, fuzzy find,
  and the today/inbox/capture surfaces.

## The landed round — chunk → PR map

| Chunk | Branch | PR (merged UTC) | Closes |
|---|---|---|---|
| 1 field family + 0007 + native startdate | `feat/domain-task-field-family` | #396 (10-21 21:40) | #208; annotated #209/#244 |
| 2 day-boundary | `feat/domain-day-boundary` | #398 (10-21 22:40) | #345 |
| 3 sidecar transport + capability seam | `feat/sync-appdata-transport` | #402 (10-22 12:56) | — (annotated #207/#244) |
| 4 sidecar fold/flush/fences/rules | `feat/sync-appdata-fold` | #405 (10-22 18:15) | #207 + #244; annotated #202 |
| 5 field surfaces + anchor hiding + badge | `feat/app-task-field-surfaces` | #406 (10-22 20:41) | #209 |
| 6 N-reminder verbs | `feat/state-reminder-rule-verbs` | #410 (10-22 22:25) | #204 |
| 7 timers & alarms + 0008 | `feat/domain-adhoc-alarms` | #419 (10-23 14:14) | #211 |
| 8 toast sink | `feat/app-desktop-notifications` | #420 (10-23 16:05) | #344 |
| 9 find + palette | `feat/domain-find` | #422 (10-24 13:18) | #348 |
| 10 today/inbox/capture/move ring | `feat/ui-today-inbox` | #433 (10-24 23:48) | #346 + #347 |
| 11 this chunk | `docs/daily-driver-wave` | — | — (the #202 comment is stage 3's ordered exception) |

Stage-2 reports on file: chunk 3
(`.artifacts/reports/2026-10-22-daily-driver-appdata-transport-stage2-report.md`
+ its STOP report), chunk 4
(`.artifacts/reports/2026-10-22-daily-driver-appdata-fold-stage2-report.md`).
Chunks 1, 2, 5, 6, 7, 8, 9, 10 have no separate stage-report files —
their PR bodies are the stage records (re-read via `gh pr view`
during this verification).

CI on `main` at the base: the `CI` workflow's latest runs are green
(2026-10-24, on the chunk-10 merges).

---

## Exit-criterion rows

### 1. "All four fields sync day one: `scheduled` native (tier-2 round-trip), effort/importance/urgency + rules via the document (tier-2 convergence)"

**Verdict: met — hermetic with recorded live-tier limitation** (the
native lane is live-tier-proven on both tiers; the document lane is
tier-1-proven end to end, and this session's tier-2 convergence
attempt could not complete — the fixtures failed at their
first-ever execution, on a scaffolding race, recorded honestly
below and in Findings).

*Native lane* (chunks 1, 3):

- Manifest/clock pins:
  `crates/taskboard-domain/src/pipeline.rs` ::
  `task_fields_manifest_pins_the_domain_structures` +
  `task_key_set_covers_the_manifest_task_fields` (proptest);
  `crates/taskboard-storage-sqlite/tests/schema.rs` ::
  `tasks_schema_matches_the_task_fields_manifest` +
  `insert_without_the_family_columns_inherits_epoch_defaults` —
  all green in this session's hermetic re-run (82/82 pipeline,
  5/5 schema).
- Wire pins (value/null/absent decode, create/update encode):
  `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`, the
  a-series startdate tests — green (58/58 this session).
- **Live tier-2, this session (2026-10-25)**:
  `crates/taskboard-sync-nextcloud/tests/it_docker.rs` ::
  `it_nextcloud_docker_startdate_round_trip` ran **green on the
  live default tier** (Nextcloud + Deck, refreshed image — Deck
  reports 1.19.1), and
  `it_nextcloud_docker_deck1175_startdate_unsupported` ran **green
  on the live gate tier** (Deck 1.17.5, brought up via
  `scripts/nextcloud_it_setup.sh` with `DECK_VERSION=v1.17.5`).
  The fallback tier's measured behavior stays pinned by the same
  test on the wiremock side as well.

*Document lane* (chunks 3, 4):

- Tier-1 (hermetic, all green this session): the fold/guard/rules
  matrix in `crates/taskboard-domain/src/merge.rs` tests; the
  engine pins in `crates/taskboard-state/tests/engine.rs` (incl.
  `completed_report_derives_sidecar_health_and_features_on_the_row`);
  the f-series in
  `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`
  (58/58).
- **Tier-2 convergence (C-series): never executed green.** The
  C1–C4 fixtures (`it_nextcloud_docker_sync_app_field_convergence`,
  `it_nextcloud_docker_sync_rule_convergence`,
  `it_nextcloud_docker_sync_echo_idempotence`,
  `it_nextcloud_docker_deck1175_scheduled_fallback_convergence`)
  were `#[ignore]`-gated skips with no record of any execution
  through chunk 4 (the chunk-4 stage-2 report records "recorded,
  not run"; no docker in that session; no later record exists).
  **This session made the round's first real attempt** — docker
  was present, the default tier came up, the fixtures ran for the
  first time — and **all four failed at the same scaffolding
  race**: `boot_convergence`'s immediate
  `stacks.keys().next().unwrap()` (`it_docker.rs:603`, plan-day
  reference) races the first pull because
  `SyncPair::set_board`'s `SyncNow` nudge is fire-and-forget
  (`tests/common/sync_pair.rs`). This is a test-scaffolding bug,
  not an established production failure: the same cycle machinery
  the fixtures would exercise is covered green at tier 1 (the
  f-series flush/fold tests) and the native-lane live fixtures on
  the same harness ran green in the same session. Per the
  docs-only scope of this chunk the fixtures were not touched;
  Findings carries the fix proposal.

### 2. "Anchor ceremony: lazy creation, adoption, re-anchor + rebuild, restore-then-replace"

**Verdict: met** (chunks 3, 4).

- Unit: `crates/taskboard-sync-nextcloud/src/anchor.rs` `mod tests`
  — `scan_matches_exact_titles_only`,
  `scan_confirms_on_the_reserved_filename_live_or_tombstoned`,
  `pick_prefers_the_newest_stamp_then_the_lowest_card_id`,
  `anchor_action_reads_the_state_machine`,
  `creation_target_picks_the_first_live_stack_at_last_position` —
  green this session.
- Wiremock f-series:
  `tests/actor_wiremock.rs` — `f02_flush_creates_the_anchor_when_none_exists`
  (lazy creation), `f06_single_title_match_adopts` (adoption),
  `f05_tombstoned_carrier_restores_then_replaces` (restore-then-
  replace), `f14_foreign_board_replaces_with_rebuild` (rebuild),
  `f19_rebuild_trigger_widens_the_scope_past_the_group` — green
  (58/58).
- **Live tier-2, this session**:
  `it_nextcloud_docker_attachment_content_lifecycle` — green on the
  default tier, and its `…_deck1175` variant — green on the gate
  tier (both live, 2026-10-25).

### 3. "The dockerized Deck 1.17.5 contract-fixture run green (attachment surface + `startdate`)"

**Verdict: met** (chunk 3; re-proven live at final HEAD this
session).

- Chunk-3 record (the stage-2 report §2 + PR #402 body): default
  tier **14/14**, gate tier **3/3**, zero divergences, re-run at
  the round's final HEAD; the ruling chain STOP report →
  characterization report
  (`.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`)
  → the capabilities addendum
  (`.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`).
- **This session's live re-run on the gate tier** (Deck 1.17.5
  brought up fresh): `it_nextcloud_docker_deck1175_startdate_unsupported`,
  `it_nextcloud_docker_deck1175_capabilities`,
  `it_nextcloud_docker_deck1175_attachment_content_lifecycle` —
  **3/3 green, live**.
- Honesty note: CI does not wire the gate tier — the
  `deck1175` fixtures are env-guarded (unset ⇒ skip, never fail),
  so the green record is local-runs-only by construction.

### 4. "Manifest: all four lockstep assertions green"

**Verdict: met** (chunks 1, 3, 4) — all green in this session's
hermetic re-run:

- Assertion 1 (domain structures pinned):
  `crates/taskboard-domain/src/pipeline.rs` ::
  `task_fields_manifest_pins_the_domain_structures`.
- Assertion 2 (storage schema matches the manifest):
  `crates/taskboard-storage-sqlite/tests/schema.rs` ::
  `tasks_schema_matches_the_task_fields_manifest`.
- Assertion 3 (the modeled key set):
  `crates/taskboard-domain/src/appdata.rs` `mod tests` ::
  `modeled_keys_are_the_appdata_rows_plus_scheduled` (the reworded
  four-modeled-keys form, `scheduled` capability-gated).
- Assertion 4 (engine op routing):
  `crates/taskboard-state/tests/manifest_routing.rs` ::
  `every_manifest_row_routes_through_its_declared_lane` +
  `every_taskchanges_slot_is_manifest_covered`.

### 5. "Format-fence loud warning end-to-end (evidence → health → CLI/UI)"

**Verdict: met** (chunks 4, 5) — hermetic:

- `tests/actor_wiremock.rs` `f13_format_too_new_fence_no_upload` +
  the whole fence set (f11/f12/f14) and
  `f07_scheduled_edit_flushes_through_the_document_while_not_native`
  — green (58/58).
- Health derivation:
  `crates/taskboard-domain/src/state.rs` :: `derive_sidecar_health`
  onto `SyncStatus.sidecar`; engine-level pin
  `crates/taskboard-state/tests/engine.rs` ::
  `completed_report_derives_sidecar_health_and_features_on_the_row`.
- CLI loud warning:
  `crates/taskboard-app/src/commands/status.rs` ::
  `sidecar_warning_projects_typed_through_the_view_and_serde`
  (structural/typed assertions — the rendered-string form was
  dropped per AGENTS §5 in PR #405's review).
- UI badge: `crates/taskboard-ui-slint/src/presentation.rs` ::
  `sidecar_caption` / the `SyncBadge` tests.

### 6. "`notify-rust` in the tree, `cargo deny` green per target, `auto` sink by mode"

**Verdict: met** (chunk 8):

- `crates/taskboard-app/src/notifications.rs` :: `resolve_sink_matrix`
  (the full 3×3 mode × sink resolution) + the payload-mapping units
  at the port boundary — green this session (taskboard-app 552/552,
  14 skipped = the docker-guarded integration set).
- The workspace pin: `notify-rust = "4.18.2"`, default features
  off, `z-with-tokio` adopted by measurement
  (root `Cargo.toml` `[workspace.dependencies]`, with the
  measurement record in the pin's comment).
- `cargo deny check` — green in PR #420 and re-run green this
  session ("advisories ok, bans ok, licenses ok, sources ok").

### 7. "`find` property suite + top-level verb + palette journey"

**Verdict: met** (chunk 9) — hermetic:

- The in-module suite in `crates/taskboard-domain/src/find.rs`
  `mod tests` (8 proptests, contract-vs-algorithm tagged — the
  crate-swap firewall), part of the domain-lib run: 463/463 green.
- CLI `find` tests (typed `--type`/`--limit`, exit 0 on no hits)
  in `crates/taskboard-app/tests/` — green (in the app run).
- Palette journey: `crates/taskboard-ui-slint/tests/journeys.rs` ::
  `the_palette_searches_and_jumps_across_boards` — green (83/83).

### 8. "Today view journey, capture engine snapshots, inbox-count structural assertion, move-ring journey"

**Verdict: met** (chunk 10) — hermetic:

- `crates/taskboard-ui-slint/tests/journeys.rs` ::
  `the_today_view_walks_completes_and_recommits` +
  `the_inbox_triage_moves_across_the_board_edge` (the chip-reads-
  zero structural assertion rides it) — green (83/83).
- The capture engine snapshots (the chunk-10 `insta` pass over the
  s-series corpus) — green, no `.pending` snapshots in the tree.
- `crates/taskboard-domain/src/days.rs` :: the `today_sections`
  P1–P6 proptests — green (in the domain-lib run).
- The today golden matrix
  (`crates/taskboard-ui-slint/tests/golden_visuals.rs` +
  `goldens/`) — green in the workspace run.

### 9. "CHANGELOG promoted, docs amended, exit report with evidence pointers"

**Verdict: met** — by this chunk's PR itself: the promoted
`CHANGELOG.md` (eleven wave entries, attributions normalized to
"daily-driver wave", adjacency restored, losslessness diff-proven:
40 deletions = 28 relocated r2-entry lines + the orphan marker + 11
attribution rewrites, 39 insertions = the same lines back), the
amended `docs/architecture.org` and `README.org`, and this report.
No `Closes` line — this chunk closes nothing.

---

## Method (what stage 2 actually ran, 2026-10-25)

1. **Re-locate**: `git fetch origin --prune`; base `c0465fa`
   confirmed = `origin/main` tip. Every §2.3 path + test name
   resolved by grep before running.
2. **Hermetic re-run** (worktree `../taskboard-WT-docs-daily-driver-wave`):
   `cargo nextest run -p taskboard-domain pipeline::` (82/82),
   `-p taskboard-storage-sqlite --test schema` (5/5),
   `-p taskboard-state --test manifest_routing` (3/3),
   `-p taskboard-domain --lib` (463/463),
   `-p taskboard-sync-nextcloud --test actor_wiremock` (58/58),
   `-p taskboard-app` (552/552, 14 skipped — the docker-guarded
   set), `-p taskboard-ui-slint --test journeys` (83/83),
   `cargo deny check` (ok: advisories/bans/licenses/sources).
3. **Tier-2 attempt** (docker present; `scripts/worktree-doctor.sh`
   exit 0, 97 G free): default tier up
   (`scripts/nextcloud_it_setup.sh up`); first `--run-ignored only`
   pass turned out to be a false pass (env-guarded skips return
   early — 0.006 s "passes"); re-run with the setup's exported
   credentials: **21 run, 17 passed live, 4 failed**
   (the three C-series + `it_nextcloud_docker_capabilities`;
   diagnosis under Findings F1/F2). Gate tier up
   (`TASKBOARD_IT_SUFFIX=-deck1175 DECK_VERSION=v1.17.5
   TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache`, port 8371): the
   three `deck1175` gate fixtures **3/3 green live**; the C4
   convergence fixture failed at the same scaffolding race. The
   deck1175 tiers were left up (the runner's docker; torn down by
   the teardown pass or `nextcloud_it_setup.sh down`).
4. **Re-read**: `gh pr view` over #396/#398/#402/#405/#406/#410/
   #419/#420/#422/#433 (the merged-PR record re-confirmed via
   `gh pr list --state merged`); the chunk-3/4 stage-2 reports on
   file.
5. **Semantic-change fidelity**: the N-rule entry re-checked
   against `crates/taskboard-domain/src/command.rs`'s planner
   (`AddReminderRule` arm) and the receipt types
   (`ReminderAddReceipt` + `added` in
   `crates/taskboard-app/src/commands/output.rs`) — accurate; the
   move-ring / `tasks add` Backlog / Today-default statements
   re-checked against `crates/taskboard-ui-slint/src/view_state.rs`
   (`UiSettings.kiosk` gating, "desktop mount starts on `Today`")
   and the chunk-10 CLI surface — accurate. No fixes needed (D11's
   expectation held).
6. **Doc-symbol spot-checks**: every new README claim checked
   against `crates/taskboard-app/src/cli.rs` / `config.rs` /
   `commands/mod.rs` and the UI sources (this caught one plan-day
   error — Finding F6).

## Findings

- **F1 — the C-series convergence fixtures fail at their
  first-ever execution** (a test-scaffolding race, not an
  established production bug): `boot_convergence`
  (`crates/taskboard-sync-nextcloud/tests/it_docker.rs:603`,
  plan-day reference) unwraps A's stacks map immediately after
  `set_board`, whose `SyncNow` nudge is fire-and-forget
  (`crates/taskboard-sync-nextcloud/tests/common/sync_pair.rs`) —
  the first pull's completion is not awaited. Affects C1, C2, C3,
  C4 identically. Proposed fix prompt: await a predicate (the
  existing `eventually` helper) for A's stack/board contents in
  `boot_convergence` before returning, then re-run the C-series on
  both tiers; the chunk-4 stage-2 report's recommendation of a
  tier-2 convergence pass thereby stays open until that lands.
- **F2 — the capabilities fixture's version pin is stale against
  the refreshed image**:
  `it_nextcloud_docker_capabilities` pins the decoded
  `deck.version` as the literal `"1.19.0"`; the current docker
  image ships Deck **1.19.1** and the probe decoded it correctly —
  the pin, not the production code, is what broke. Proposed fix:
  assert a minimum version / parse-and-compare instead of the
  literal.
- **F3 — the CI weekly `Nextcloud Integration` lane has never run
  wave code**: last run 2026-10-21 (a failure, pre-wave — before
  chunk 1 merged), last success 2026-10-14. It is dispatchable on
  demand (`gh workflow run nextcloud-integration.yml`) — the
  user's call (chunk plan D5: no autonomous CI dispatch).
- **F4 — the orphaned `=======` marker** inside `[Unreleased]`
  Added (line 43 at `c0465fa`, a mangled historical conflict
  resolution) was removed as part of the promotion.
- **F5 — non-wave README staleness left untouched** (chunk plan
  D7): the Download section's "Until the first release is cut,
  build from source" (v0.1.0 shipped 2026-10-16) is stale; fixing
  it was out of this chunk's scope.
- **F6 — the chunk plan's README example for `--scheduled` was
  wrong**: `--scheduled "tomorrow 09:00"` does not parse — the
  flag reuses `parse_due`'s grammar (RFC-3339 | `YYYY-MM-DD` |
  `none`; `crates/taskboard-app/src/cli.rs` `parse_due`). The
  shipped README example uses `--scheduled 2026-10-26` and states
  the grammar.
- **F7 — the refreshed default-tier image reports Deck 1.19.1**
  (the plan-day facts said 1.19.0); no wave behavior depends on
  the patch version (the seam is `CARD_STARTDATE` presence), and
  the live `startdate_round_trip` ran green on it.

## Backlog candidates (restated, **not promoted** — no issues created)

From the implementation plan §1 Out list and the design docs'
backlog sections: the reserved document key space
(`task-kind` / `respite` / `recurrence`) and the `proofs` /
`settings` sections (the future D5 wave); #203's insistence
semantics; #352 (timers UI surface); #351 (the settings editor);
#350 (cross-board card moves lose app-fields — the sidecar's
accepted limitation); `tasks add` flag parity with capture; the
decision-support ideas. Plus this verification's own candidates:
**F1's C-series boot-race fix** (the round's one open tier-2 gap)
and **F2's version-pin loosening**.

## Bookkeeping

- Milestone `wave-daily-driver`: every roster issue CLOSED through
  chunks 1–10's PRs (#208, #209, #204, #211, #344, #348, #346,
  #345, #347, #207, #244) — except **#202, open by design**: its
  reminder-rule half (the rules sync) landed via chunk 4 (PR #405,
  `fold_reminder_rules` + the document's reminder-rules section);
  delivery proofs and settings sync stay with the future D5 wave
  per the issue's scope comment.
- The one ordered tracker comment on #202 (the milestone-clear
  framing above + links) is **stage 3's** single exception to the
  no-tracker-edits rule. The milestone's actual closure is the
  user's act. Stage 2 made no tracker edits, created no issues,
  and edited no labels or milestones.

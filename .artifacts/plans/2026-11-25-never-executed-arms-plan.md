# The Never-Executed Arms — chunk plan (round 7, chunk B5-p2)

- **Chunk**: B5-p2 of `2026-11-25-backlog-reduction-round7-dispatch-plan.md` §7
- **Branch**: `test/never-executed-arms` (worktree
  `../taskboard-WT-test-never-executed-arms`, off fetched `origin/main`)
- **Issues**: #1372, #1370, #1371, #1369, #1368 (p2, testing)
- **Stage**: 1 plan (no implementation in this stage)
- **Laws**: AGENTS §5 (fakes, no sleeps, typed outcomes, no text
  assertions, one-behavior-one-test), §6.1–6.2 clock discipline,
  dispatch-plan §7 acceptance (each arm executes in a test, kill
  evidence per arm, no integration duplicates of unit pins)

---

## 0. Ground rules for the whole chunk

- **Kill evidence**: every new test names the mutation(s) it kills in a
  doc comment ("kills: …"), and stage 2 records one manual
  mutation spot-check per item group (invert an arm / swap a route /
  break an arithmetic → the named test fails) in the stage-2 report.
- **Hermeticity**: no DBus, no real toasts, no wire beyond the crate's
  own fake-transport model; wiremock only where the wire is the
  contract — decision below: nowhere in this chunk needs it.
- **Clock**: any fixture whose asserted output depends on the running
  day anchors the fixture relative to the same clock the code reads
  (`Utc::now()` captured once, windows derived from it) — the relative
  oracle rule of testing_strategy §6.1. No `tokio::time::sleep`.
- **Gate**: pre-commit hooks + `scripts/run-quality-gates.sh cargo
  nextest run --workspace`; every cargo call through
  `scripts/memory-gate.sh`.

---

## 1. Item #1372 — files-sync error projection + cycle error arms

### Arm inventory (file:line on `origin/main`)

| Arm | Where | Status today |
|---|---|---|
| `From<&TransportError> for LaneFailure` — all 12 match arms | `crates/taskboard-sync-files-nextcloud/src/status.rs:109–127` | never executed (status.rs 42.86%) |
| `LaneOutcome::Failed((&err).into())` lane-decision wiring | `src/board_cycle.rs:229` | only the `Server(503)` instantiation ever fires (existing `Effect::Status(503)` tests) |
| Cycle error paths: `Forbidden`/`NotFound`/`BadRequest`/`RateLimited`/`Server(n)` driving `CycleOutcome::Failed` | `src/cycle.rs` (`run_cycle` → `publish_status`, 509+) | success-only matrix |
| Peer-document garbage → `ParkKind::Quarantined` / decode-failure arms | `src/cycle.rs:916` (`ingest_peer_document`) + serde arms | success-only |
| `adopt_own_file` adoption path (`cycle.rs:658`, called from 628/645/1230/1448) | 404-on-own-files arm | never scripted |

### Decisions

- **[D1.1] Test home: in-crate** — `status.rs` gains a `#[cfg(test)]
  mod tests`; the cycle/lane error scripts live in the existing
  `board_cycle.rs` `mod tests` (line 1735) and, for `run_cycle`-level
  verdicts, alongside the `actor.rs` run_cycle harness (lines
  2239–2361 already spawn `run_cycle` over the fake).
- **[D1.2] Fake, not wiremock.** `FakeFilesTransport` (`test_support.rs`)
  already models `Effect::Status(u16)` / `Effect::Body(Vec<u8>)`
  one-shots; the contract under test is the *projection*, not the wire
  encoding. Tier-1 wiremock stays reserved for wire-contract suites
  (the `retained_etag_wiremock.rs` precedent) — none here.
- **[D1.3] Tests (approx. 5):**
  1. `lane_failure_projection_table_covers_every_transport_error` —
     exhaustive unit table: one `TransportError` per variant →
     `assert_eq!(LaneFailure::from(&e), expected)` for all 12 arms,
     including `InvalidBaseUrl → BadRequest` and the
     `Json|PropfindListing → Payload` pair. Kills: any swapped/renamed
     projection arm.
  2. `scripted_wire_errors_project_into_typed_lane_failures` — one
     board lane, one cycle per row: stage `Effect::Status(403/404/400/
     429/500)` on the lane's first request; assert
     `status.lanes[remote] == LaneOutcome::Failed(LaneFailure::…)` and
     `CycleOutcome::Failed`. Kills: the `Failed((&err).into())` →
     `Ok` replacement and per-variant swaps.
  3. `a_garbage_peer_document_parks_as_quarantined` — `Effect::Body`
     garbage on the peer GET; assert `parked[path] ==
     ParkKind::Quarantined` (typed). Kills the decode-failure arm.
  4. `a_missing_own_appdata_file_is_adopted` — stage `Status(404)` on
     the own-appdata GET so `adopt_own_file` executes; assert the
     adoption finding / re-attach PUT (terminal model state: the file
     exists again with our revision). Kills the 404-adoption arm.
  5. `rate_limited_carries_the_retry_after_projection` — `Status(429)`
     + the retry-after parsing path (`TransportError::RateLimited {..}`
     → `LaneFailure::RateLimited`), asserting the typed variant only.
- **[D1.4] What NOT to duplicate**: the existing 503/fork/quarantine
  success-cycle matrix; the wiremock client suites (`suite/`); any
  actor backoff timing (untouched — decisions assert the status watch,
  not the schedule).
- **Determinism**: direct `run_cycle`/`run_board_lanes` calls, no
  sleeps; `tokio::time::pause()` where an actor harness is reused.

---

## 2. Item #1370 — the dispatch-table verb-to-handler closures

### Arm inventory (`crates/taskboard-app/src/commands/mod.rs`, the
### one-shot `dispatch` at line 1058; file:line on `origin/main`)

| # | Arm | Line |
|---|---|---|
| 1 | `BoardsCmd::Archive` → `boards::archive(config, store, board, true)` | 1216 |
| 2 | `BoardsCmd::Unarchive` → `archive(..., false)` ("restore") | 1224 |
| 3 | `BoardsCmd::Delete { confirm }` → `boards::delete` | 1234 |
| 4 | `BoardsCmd::Reset { confirm }` → `boards::reset` | 1243 |
| 5 | `BoardsCmd::Unbind { confirm }` → `boards::unbind` | 1322 |
| 6 | `LabelsCmd::Restore` → `labels::restore` | 1448 |
| 7 | `TasksCmd::Undo { mode: None }` → `tasks::undo` | ~1585 |
| 8 | `TasksCmd::Undo { To { id } }` → `tasks::undo_to` | ~1592 |
| 9 | `TasksCmd::Redo { mode: None }` → `tasks::redo` | ~1600 |
| 10 | `TasksCmd::Redo { To { id } }` → `tasks::redo_to` | ~1608 |
| 11 | `TasksCmd::History { json }` → `tasks::history` (+ `io.json \|= json`) | ~1620 |
| 12 | `Command::Find` run_core arm → `find::find` + `render_text_list` | ~1636 |

### Decisions

- **[D2.1] Test home: `crates/taskboard-app/tests/core/
  dispatch_routing.rs`** — the D-series already owns this file, its
  `ctx()` harness builds a real `CommandCtx`, and its
  `one_shot_core_verbs_route_success_end_to_end` (line 544) is the
  success-routing precedent. Add a `dispatch_routes_success_<family>`
  trio of table tests (boards / labels+find / tasks) rather than
  twelve standalone tests.
- **[D2.2] Seeding**: the `cli_undo.rs` `seeded_config` pattern —
  `common::standalone_config` + `boards::create` (+ stack/task/label
  as each verb needs), `forbidden_store()` for the credential lane.
  Each row parses the real argv through `Cli::try_parse_from` (the
  clap surface is part of the kill surface) and drives `dispatch`.
- **[D2.3] Assertions — typed terminal state, not receipt text**:
  archive/unarchive → a follow-up `boards::list` shows the archived
  flag flipped; delete/reset/unbind → the board/stack/label counts and
  the typed confirm-refusal first (`ErrorKind` via `matches!` on the
  confirm-missing leg where the receipt enum exposes it; the confirm
  flag riding through is killed by the refusal firing *only* when
  `confirm == false` and the success leg only when true); tasks
  undo/redo family → the typed `JournalReceipt` variant returned via
  the io sink's JSON mode (`io.json = true`, structural
  deserialization, as the U-series does); history → the history
  receipt is non-empty with the seeded entry's typed fields; find →
  the typed row set (title match), exit 0.
- **[D2.4] Kill evidence**: swap `archive(board, true)`→`false`
  (row 1 fails), reroute `Redo{To}`→`redo` (row 10 fails), drop the
  `json` fold in row 11 (the JSON-mode sink assertion fails), swap the
  Find render closure for another renderer (structural render pin —
  arity/marker — catches it without text-matching prose).
- **[D2.5] What NOT to duplicate**: `dispatch_line` session-lane pins
  (cli_undo U-8, cli_exit_codes), the handlers' direct-call suites
  (cli_boards/labels/tasks), the D-series failure-routing rows
  (credential-load exit 1), and the #1230 exit-code table
  (cli_exit_codes.rs) — this chunk adds only the *missing* success
  routing rows.
- **[D2.6] `outcome_mismatch` arms** (`labels.rs:354`, `track.rs` ×7):
  **decision — not drivable in this chunk.** They fire only when the
  real engine returns a `CommandOutcome` outside the pinned family —
  unreachable without a fake-engine seam at the command tier, which is
  #1231's closed scope. New information → comment on #1231 (done in
  stage 1; see the backlog section), no new issue.

---

## 3. Item #1371 — the export dialog's Run path

### Ground-truth correction (recorded decision **[D3.0]**)

The coverage report's "journeys never complete a Run" evidence is
**stale on this point for the tracking leg**: `export_journeys.rs`
`us2_s2_run_dispatches_export_run_spec` (line 242) and
`us3_s2_run_dispatches_the_pdf_spec_with_the_override` (line 496)
already drive `invoke_export_run` and assert the minted `ExportSpec`
fields, and `us2_s3` folds a `Refused(BadTaskFilter)` through the
outcome area (exercising `refusal_index` via the adapter's fold). Stage
2 therefore **starts with a targeted fresh measurement**
(`scripts/memory-gate.sh cargo llvm-cov nextest -p taskboard-ui-slint
--all-features --no-report …` scoped to the journey binary +
export_dialog) and implements only what the measurement shows dark.
The pure-fn pins below are written regardless — they are unit-layer
pins the journeys cannot provide (one-behavior-one-test: the journey
proves routing; the unit pin proves the mapping table).

### Arm inventory

| Arm | Where | Plan |
|---|---|---|
| `ExportVm::parse_date` (empty→None / `%Y-%m-%d` / malformed→None) | `export_dialog.rs:182` | in-crate unit pin (new `mod tests`) |
| `ExportVm::mint_spec` default-VM product (all filters None, `out` None, `layout` None off-pdf, `open_after` posture) | `export_dialog.rs:210` | in-crate unit pin; journeys own the field-echo legs |
| `ExportVm::refusal_index` / `requests::refusal_index` exhaustive over `ExportRefusal` | `export_dialog.rs:227`, `requests.rs:524` | exhaustive unit table (one row per kind; also asserts distinctness of indices — the mapping is an address space) |
| `ExportVm::default` / `seeded` defaults | `export_dialog.rs:108` | unit pin over the seeded buffer's initial flags |
| adapter mint/refusal call sites (`adapter.rs:9034`, `9417`, `9442`, fold 9129) | adapter.rs | only if the fresh measurement shows them dark (e.g. the journey binary's coverage gap); if dark, extend `us2_s2` with the destination-empty leg + a refusal-fold leg rather than new journeys |

- **[D3.1] Kill evidence**: swap one `refusal_index` arm (table
  fails), change the `%Y-%m-%d` format string (parse_date pin fails),
  make `mint_spec` mint a non-empty `out` for an empty destination
  (default-VM pin fails).
- **[D3.2] What NOT to duplicate**: the field-validity/Run-gate
  journeys (us2_s1, us3_s1), the kiosk absence gate, the escape-during-
  run law.

---

## 4. Item #1369 — DesktopSink delivery + focus-boundary surfaces

### Arm inventory (`crates/taskboard-app/src/notifications.rs`)

| Arm | Where |
|---|---|
| `DesktopSink::deliver` (payload→toast fields, critical urgency, handle tracking) | 197–247 |
| `DesktopSink::withdraw` (remove tracked handle; unknown key → Ok) | 247+ |
| `TRACK_CAP` eviction (≥256 live handles → set cleared) | 225 |
| backend error → `NotifyError` | deliver's `Err` arm |
| `boundary_phrases` (4 typed pairs) | 586 |
| `resolve_boundary_surface` matrix | 530 |
| `mount_focus_boundaries` (3 surfaces) | 631 — partially tested (`mount_focus_boundaries_resolves_the_posture_matrix` at 717) |
| `DesktopBoundaries::boundary` (toast + chime, failures logged-and-dropped) | the `FocusBoundarySink` impl |

### Decisions

- **[D4.1] Small production seam refactor (approved scope)**:
  `DesktopSink::deliver` and `DesktopBoundaries::boundary` call
  `notify_rust::Notification::new() … .show_async()` **directly —
  there is no backend seam today** (the issue's "the sink's backend
  seam" does not exist yet). Extract a private
  `trait ToastBackend: Send + Sync + Debug` with
  `fn show(&self, payload: &ToastPayload) -> Result<ToastHandle,
  NotifyError>` and `fn close(&self, handle: &ToastHandle)`; the prod
  impl wraps notify-rust (the only code that touches the OS);
  `DesktopSink` and `DesktopBoundaries` hold an `Arc<dyn ToastBackend>`
  with `Default` = the real backend (bootstrap call sites unchanged).
  `#![forbid(unsafe_code)]` and the AGPL header hold; ~60 production
  lines, behavior-preserving.
- **[D4.2] Fake: handwritten `RecordingBackend`** (fakes-over-mocks)
  in the same `mod tests`: records `(payload fields…, result)` tuples
  and hands out counted handles; a toggleable failure mode for the
  `NotifyError` path. The existing fake `ChimeSink` (focus tests'
  recording chime) is reused for the boundary chime assertion.
- **[D4.3] Tests (approx. 6):**
  1. `deliver_maps_insistence_to_the_critical_posture_and_tracks_the_handle`
     — Normal → not critical, Alarm → critical (typed flag recorded by
     the fake); handle tracked under `(reminder, occurrence)`. Kills:
     the `payload.critical` inversion, the tracking-insert drop.
  2. `withdraw_removes_the_tracked_handle_and_unknown_keys_are_ok` —
     deliver→withdraw→`close` observed; withdraw of a never-delivered
     key → `Ok(())` and no `close`. Kills: the removal and the
     best-effort contract.
  3. `deliver_past_the_track_cap_clears_the_tracked_set` — 257
     distinct occurrences; the 257th deliver clears, so a later
     withdraw of handle #1 observes no close. Kills: the
     `len() >= TRACK_CAP` arithmetic (`>` vs `>=`, off-by-one).
  4. `a_failing_backend_surfaces_notifyerror_and_does_not_track` —
     failing fake → `Err(NotifyError)` (typed `matches!`), tracked set
     empty. Kills: swallowing the typed error.
  5. `boundary_phrases_map_each_focus_boundary_variant` — pure table
     over the four `FocusBoundary` variants (asserts the pair
     identity per variant; the strings are the artifact under test, as
     with the existing `toast_payload` pins — not error/UI-label
     assertions).
  6. `desktop_boundaries_toasts_once_and_chimes_through_the_shared_sink`
     — `DesktopBoundaries` over `RecordingBackend` + recording chime:
     one toast, one chime, backend failure does not panic nor skip the
     chime (logged-and-dropped contract, D-R13). Kills: dropping the
     chime call, propagating a toast failure.
  7. `resolve_boundary_surface_matrix` — the 2×3 matrix as a unit pin
     (extend the existing posture-matrix test only if it doesn't
     already cover all six cells — check, don't duplicate).
- **[D4.4] What NOT to duplicate**: `LogSink`, `toast_payload`/
  `body_kind` inline pins, the queue/actor machinery suites, and any
  real DBus/notify-rust behavior (the prod backend wrapper is
  behavior-preserving glue; its wire behavior is the OS's).

---

## 5. Item #1368 — the five habit subcommands

### Arm inventory

| Arm | Where |
|---|---|
| `HabitsCmd::Skip/Pause/Resume/History/Stats` → `collapse` arms | `commands/habits.rs:247–252` |
| `skip_on` / `pause_on` / `resume_on` / `history_on` / `stats_on` | `habits.rs:307 / 339 / 366 / 1233 / 1275` |
| `run_on` routing arms for the five invocations | `habits.rs:439–450` |
| `render_history` / `render_stats` | `habits.rs:952 / 979` |
| `window_state_view` (Paused/Met/Skipped/PreSchedule/Missed/Pending arms) | `habits.rs:1190` |

### Decisions

- **[D5.1] Test home: `tests/cli/cli_habits.rs`**, H-series extension
  (h06–h09) using the existing `parse`/`seeded_config`/`run_one`
  harness (`habits_command` over a seeded standalone tempdir — the
  dispatch + typed outcome tier; the GUI is never involved). The
  renderer pins join the existing
  `habits_render_arity_and_distinguishment` structural test's
  conventions (arity/ordering/distinguishment, no prose matching).
- **[D5.2] Tests (approx. 5):**
  1. h06 skip: `habits skip <selector>` → typed
     `HabitOutcome::Skipped` receipt + terminal state (`skip` flag on
     the current window's log entry read off a fresh boot); unknown
     selector → `matches!(err, CommandError::UnknownHabit{..})`.
     Kills: Skip→Restore reroute in `run_on`.
  2. h07 pause/resume round trip: `pause --until <anchored date>` →
     `Paused` receipt + `paused_until` fact; `resume` → cleared; the
     pause-covers arm surfaces as `HabitWindowStateView::Paused` in a
     follow-up history. Kills: Pause/Resume swaps, the
     `pause_covers_window` composition order.
  3. h08 history: seed log rows relative to the running window
     (anchor captured once via `Utc::now()` at fixture time, windows
     derived — §6.1 relative-oracle rule; cf. flaky #606's midnight
     lesson: never hard-code a calendar date). Assert the typed rows:
     chronological window order, counts, and one state per seeded arm
     (Met / Skipped / Missed / Pending / PreSchedule / Paused across
     two habits' seeds as needed). Kills: each `window_state_view`
     arm's inversion + `render_history`'s ordering.
  4. h09 stats: typed `HabitStatsView` projection on the same seeded
     series (adherence ratio, streak fields — the domain's typed
     values, structural assertions). Kills: Stats→History reroute,
     `render_stats` distinguishishment.
  5. h10 restore-of-skipped rides the existing H-series restore only
     if the coverage check shows `Restore` dark (it is *not* in the
     issue's list — default: don't add).
- **[D5.3] Clock discipline**: history/stats read `Utc::now()`
  internally (no seam). The fixture anchors into the running window
  (compute the current period from a captured `now`), keeping the test
  deterministic-by-construction at any wall-clock moment *within* a
  window; the residual midnight-crossing hazard is inherent to the
  un-seamed production read and is recorded as new information on the
  deferred clock-drift issue #680 (comment in stage 1), not a new
  issue.
- **[D5.4] What NOT to duplicate**: the engine semantics (state/domain
  at 98%+), H-series add/log/adjust/update/delete, the domain
  `statistics_horizon`/`adherence` pins (assert the CLI projection's
  typed view, not the math).

---

## 6. Execution order (stage 2)

1. Worktree bootstrap + branch (done at stage 1).
2. #1369 seam refactor first (the only production-code change; land it
   as its own commit so test commits are pure additions).
3. #1372, #1370, #1371, #1368 test waves — one commit per item,
   `<type>(<module>)` subjects (`test(sync-files)`, `test(app)`,
   `test(ui)`, `test(app)`).
4. Gate: `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (background + polls per the long-command discipline; flock-deferral
   rule if contended).
5. Hand off at commits-landed; the dispatcher owns the full suite,
   stage 3, PR.

## 7. Risks

- **#1371 ground truth**: the cited dark arms may already be covered
  by the journeys; the fresh targeted measurement (D3.0) gates what is
  written — if the journey binary is excluded from coverage on the
  runner, the *measurement* method itself is the finding and only the
  unit pins land (plus a note on #1371).
- **#1369 seam refactor** touches notification delivery glue; the
  behavior-preserving claim is guarded by the existing
  spawn_notifier/actor suites staying green.
- **#1370 success rows boot a full core per row** (~12 boots); nextest
  parallelism absorbs it, but the trio-of-tables shape (D2.1) keeps it
  to ~12 short-lived boots, not suites.
- **#1368 midnight-crossing**: mitigated by relative anchoring (D5.3);
  the residual hazard is documented on #680, not papered over with a
  pinned date.
- **files-sync 412/fork interplay**: the adoption test must not
  collide with `four12_once` staging; stage 404 (not 412) on the
  initial listing per D1.3-4.

## 8. Estimated diff

~700–1000 lines across 8–10 files: ~60 production lines (the
notifications.rs `ToastBackend` seam), the rest tests + harness
reuse; no new crates, no new dependencies.

## 9. Backlog / issue actions (stage 1, standing authorization)

- Comment on **#1231** (closed): the `outcome_mismatch` arms are the
  surviving concrete instance of its fake-engine seam rationale.
- Comment on **#680**: `habits` history/stats read `Utc::now()` with
  no clock seam — a new deferred clock-drift site (testability debt
  noted in D5.3).
- No new issues required (searched first; #1230/#1231 cover the
  routing/render survivor classes).

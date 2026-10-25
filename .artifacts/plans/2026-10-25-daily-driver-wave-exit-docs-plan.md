# [Plan] Daily-Driver Chunk 11 — Docs Amendments + Wave Exit Verification

- **Date**: 2026-10-25
- **Kind**: stage-1 work plan (dispatch plan §12, prompt 11.1) for
  chunk 11 of the daily-driver implementation round — docs + the exit
  verification record. No code, no tracker edits (the one ordered
  #202 comment is stage 3's, §11 here).
- **Branch / worktree**: `docs/daily-driver-wave` at
  `../taskboard-WT-docs-daily-driver-wave`, created off `origin/main`
  = `c0465fa` (chunk 10's merge commit — the current tip);
  `scripts/worktree-doctor.sh` exit 0 before creation (101 G free,
  floor 25 G; one unrelated in-flight worktree kept).
- **Precondition MD-9 verified (2026-10-25)**: all ten implementation
  chunks are merged on `origin/main`. The remote branch refs were
  deleted after each rebase-merge, so containment was verified
  through the authoritative records instead of `git merge-base
  --is-ancestor`: `gh pr list --state merged` shows all ten PRs with
  merge timestamps in dependency order (#396 10-21 → #398 10-21 →
  #402 10-22 → #405 10-22 → #406 10-22 → #410 10-22 → #419 10-23 →
  #420 10-23 → #422 10-24 → #433 10-24), every issue the PRs carry
  `Closes` lines for is CLOSED (#208, #345, #207, #244, #209, #204,
  #211, #344, #348, #346, #347), and chunk artefacts spot-checked
  present on `origin/main` (`domain/src/{find,days,appdata,
  capabilities}.rs`, `migrations/0008_adhoc_alarms.sql`,
  `app/src/commands/{find,capture,timers}.rs`,
  `ui-slint/ui/views/today-view.slint` +
  `ui/compositions/{search-palette,capture-overlay}.slint`).
  Milestone `wave-daily-driver`: every roster issue CLOSED except
  **#202 (open by design — the milestone-clear comment frames it)**.
- **Normative sources** (MD-7 rank): the wave implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  (§3 chunk 11, §5 the exit-criteria table, §6 bookkeeping, §1 Out
  list); the five ACCEPTED design docs' decision records —
  `2026-10-18-appdata-sidecar-design-plan.md` (D-S1…D-S9 + the
  2026-10-22 amendment note), `2026-10-18-field-family-design-plan.md`
  (D-F1…D-F6, D-F2 branch A as amended by the capability addendum),
  `2026-10-19-reminders-surfaces-design-plan.md` (D-R1…D-R13),
  `2026-10-19-views-finding-design-plan.md` (D-V1…D-V10, D-V5 v2,
  D-V9 amended), `2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5 — design only, no chunk; #176 unscheduled); the version
  -capabilities addendum `2026-10-22-deck-version-capabilities-design
  -plan.md` (D-V1/D-V2/D-V3 of that doc) + the characterization
  report; the dispatch plan §1/§12/§13. **No deviation from any
  ACCEPTED ruling is needed or permitted here** — this chunk writes
  documentation about what landed.
- **Line numbers below are plan-day references** (MD-8, taken at
  `c0465fa`, 2026-10-25); paths, symbols, and test names are the
  stable keys. The exit report cites **paths + test names only** (D9).
- **Precedent followed**: the multi-board wave's exit chunk
  (`.artifacts/plans/2026-10-20-multi-board-wave-exit-docs-plan.md` →
  `.artifacts/reports/2026-10-20-multi-board-wave-exit-report.md`) is
  the house shape for this chunk; where this plan is silent on
  method, that precedent's settled decisions apply (its D1/D2/D4/D6
  analogues are re-recorded here as D1/D5/D7/D9).

## 1. Scope

**In** (dispatch §12 "Covers", whole):

1. `docs/architecture.org` — the wave's subsystems made true in the
   crate roles + a new app-data sidecar subsection (§3 below).
2. `README.org` — the new verbs/flags/config keys + the anchor
   card's Deck-UX note (§4 below).
3. `CHANGELOG.md` — promotion of the wave's per-chunk Unreleased
   lines into the wave's release-note shape, losslessly (§5).
4. The exit verification record at
   `.artifacts/reports/<stage2-date>-daily-driver-wave-exit-report.md`
   — one section per implementation-plan §5 row with named tests and
   honest tier records (§6).
5. Bookkeeping review inputs: the milestone roster state and the
   #202 comment framing are documented in the report's Bookkeeping
   section (§6); the comment itself is stage 3's only tracker edit.

**Out** (dispatch §12 + implementation plan §3 chunk 11 / §1):

- No code — any `.rs`/`.toml`/`.sql` diff is a finding.
- No release cut: entries stay under `[Unreleased]`;
  `bump_version.sh` is the user's flow (AGENTS §9).
- No ADR: the wave landed none (16 ADRs, 0001–0016, all pre-wave);
  if stage 2 believes one is owed, that is a Findings line, not a
  diff.
- No tracker edits beyond stage 3's single #202 comment; no label or
  milestone edits; no new issues; the milestone
  `wave-daily-driver`'s actual closure is the user's act (MD-4).
- Non-wave doc drift observed while working (e.g. README's "Until
  the first release is cut" — v0.1.0 shipped 2026-10-16) is recorded
  in the report's Findings, never fixed in this diff (D7) — the PR
  diff is exactly this chunk's artefacts.
- No backlog promotion: out-of-scope ideas are restated in the
  report's backlog-candidates section for the user.

## 2. Re-verified evidence inventory (2026-10-25, base `c0465fa`)

### 2.1 The landed round — chunk → PR map

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
| 9 find + palette | `feat/domain-find` | #422 (10-24 13:18) | #348 (+ the D-V9 amendment note posted) |
| 10 today/inbox/capture/move ring | `feat/ui-today-inbox` | #433 (10-24 23:48) | #346 + #347 |
| 11 this chunk | `docs/daily-driver-wave` | — | — (the #202 comment is the ordered exception) |

Stage-2 reports on file: chunk 3
(`2026-10-22-daily-driver-appdata-transport-stage2-report.md` + the
STOP report), chunk 4
(`2026-10-22-daily-driver-appdata-fold-stage2-report.md`). Chunks 1,
2, 5, 6, 7, 8, 9, 10 have no separate stage-report files — their
PR bodies are the stage records (quoted via `gh pr view`, §7.4).

### 2.2 Current doc state (what stage 2 amends)

- `docs/architecture.org` (487 lines): post-multi-board vocabulary
  throughout. Wave gaps: crate role 1 lists no `appdata.rs` /
  `capabilities.rs` / `days.rs` / `find.rs`, no field family, no
  `TASK_FIELDS`, no `CaptureTask`/reminder-verb vocabulary, no
  `SyncReport.app_data`; role 2 has no fold/guard/health/tick-alarms
  story; role 3 has no capability seam, anchor ceremony, flush, or
  fences; role 4's schema list stops at 0006 + the 0004 reminder
  tables (no 0007/0008); role 5 has no today view / capture / palette
  / rule-list / move-ring; role 6 documents `[reminders]` as
  =enabled/tick_secs/default_before= only, "desktop toasts are a
  follow-up behind the same port" (now shipped — chunk 8), and the
  `commands` verb list lacks `find`/`capture`/`timers`/the
  `tasks edit` field flags/`remind --remove`; the tech-stack table
  lacks `notify-rust`; no `[preferences]` anywhere; the "Board
  Selection is Caller-Owned" subsection predates the today view.
- `README.org` (534 lines): no `find`/`capture`/`timers`, no
  `tasks edit --scheduled/--effort/--importance/--urgency`, no
  `[preferences]`, no `[reminders] sink`, no anchor-card Deck-UX
  note. **Wave-caused staleness (in scope to fix)**: the legacy-path
  block's "a bare add requires exactly one live stack" note — chunk
  10 changed bare `tasks add` to land in the single live board's
  Backlog (zero boards refuse typed; several boards =
  `ambiguous-board`; `tasks move` gained `--board`). **Non-wave
  staleness (Findings only, D7)**: "Until the first release is cut,
  build from source" (v0.1.0 shipped 2026-10-16).
- `CHANGELOG.md`: `[Unreleased]` (lines 10–561 at `c0465fa`) holds
  the wave's eleven entries interleaved with the GUI r2/kiosk/
  multi-board/perf entries, **plus one orphaned `=======` line at
  :43** (a mangled historical conflict resolution — no `<<<<<<<` /
  `>>>>>>>` companions in the file; markdown-harmless, but it sits
  inside the section this chunk promotes and is removed in the
  pass, D1). The wave's entries and their current attributions:

| # | Section | Entry opens | Attribution today |
|---|---|---|---|
| 1 | Added | "The today view, global Inbox, quick capture…" | (#346, #347, chunk 10) |
| 2 | Added | "Fuzzy search…" | (#348, chunk 9) |
| 3 | Added | "Desktop reminder toasts via `[reminders] sink`…" | (#344, chunk 8) |
| 4 | Added | "Standalone timers & alarms…" | (#211, chunk 7) |
| 5 | Added | "The task-field surfaces + anchor hiding + the sidecar badge…" | (#209, chunk 5) |
| 6 | Added | "The app-data sidecar fold, flush, and rules section…" | (#207 + #244, chunk 4) |
| 7 | Added | "The app-data sidecar transport + the Deck capability seam…" | (#207, chunk 3) |
| 8 | Added | "The day-boundary preference…" | (#345) — none |
| 9 | Added | "The task field family…" | (#208, #209) — none |
| 10 | Added | "The `TASK_FIELDS` lockstep manifest…" | (#244) — none |
| 11 | Changed | "**Reminder rules are N-rule additive**…" | (#204, chunk 6) |

  The three semantic-change statements the dispatch traps name are
  present and accurate as written: the N-rule entry states the
  add-not-replace change + the `ReminderSetReceipt` →
  `ReminderAddReceipt` rename; the chunk-10 entry states all three
  deliberate behavior changes (ring crosses board edges; `tasks add`
  Backlog default; Today as the desktop launch view); stage 2
  re-verifies each against the code before promoting (§7.5), stage 3
  re-checks (dispatch §12 prompt 11.3).

### 2.3 Named tests (the exit report's pointers; all re-verified to exist at `c0465fa`)

| Exit row | Verifying chunk | Named evidence (path + test) |
|---|---|---|
| Fields sync day one | 1 | `crates/taskboard-domain/src/pipeline.rs` `task_fields_manifest_pins_the_domain_structures` + the `task_key_set_covers_the_manifest_task_fields` proptest (assertion 1); `crates/taskboard-storage-sqlite/tests/schema.rs` `tasks_schema_matches_the_task_fields_manifest` + `insert_without_the_family_columns_inherits_epoch_defaults` (assertion 2 + epoch defaults); the chunk-1 PR's tier-1 startdate wire pins (value/null/absent decode, create/update encode — `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`, re-locate the a-series startdate names) |
| | 3 (gate) | `it_nextcloud_docker_startdate_round_trip` — **ran green on the default tier** (NC 35.0.1 / Deck 1.19.0, three times incl. final HEAD); `…_deck1175_startdate_unsupported` pins the fallback tier's measured behavior |
| | 4 (document lane) | the fold/guard/rules matrix in `crates/taskboard-domain/src/merge.rs` tests + `crates/taskboard-state/tests/` engine pins; **C1–C4 tier-2 convergence fixtures are compiled `#[ignore]`-gated skips, never executed** (no docker in the chunk-4 session — the stage-2 report's "Tier-2 convergence outcomes (MD-5 tier honesty — recorded, not run)"; PR #405 body repeats it). Chunk 6 extended the C2 site (`…_sync_rule_convergence`); no record of any C-series execution exists |
| Anchor ceremony | 3, 4 | `crates/taskboard-sync-nextcloud/src/anchor.rs` `mod tests` (`scan_matches_exact_titles_only`, `scan_confirms_on_the_reserved_filename_live_or_tombstoned`, `pick_prefers_the_newest_stamp_then_the_lowest_card_id`, `anchor_action_reads_the_state_machine`, `creation_target_picks_the_first_live_stack_at_last_position`); `tests/actor_wiremock.rs` f-series (`f02_flush_creates_the_anchor_when_none_exists` — lazy creation, `f06_single_title_match_adopts` — adoption, `f05_tombstoned_carrier_restores_then_replaces`, `f14_foreign_board_replaces_with_rebuild`, `f19_rebuild_trigger_widens_the_scope_past_the_group`); tier-2: `it_nextcloud_docker_attachment_content_lifecycle` green on both tiers incl. soft delete/restore |
| Deck 1.17.5 gate | 3 | the stage-2 report §2 record: default tier **14/14**, gate tier **3/3** (`it_nextcloud_docker_deck1175_{startdate_unsupported, capabilities, attachment_content_lifecycle}`), zero divergences, re-run at final HEAD; the ruling chain: STOP report → characterization report → the capabilities addendum; CI runs the deck1175 suite as env-guarded skips (honest note, PR #402) |
| Manifest assertions | 1, 3, 4 | assertion 1 (row 1's pipeline.rs tests); assertion 2 (row 1's schema.rs); assertion 3 — the reworded modeled-key-set test in `crates/taskboard-domain/src/appdata.rs` `mod tests` (re-locate exact name); assertion 4 — `crates/taskboard-state/tests/manifest_routing.rs` `every_manifest_row_routes_through_its_declared_lane` + `every_taskchanges_slot_is_manifest_covered` (incl. the flag-routed `scheduled` row in all three arms) |
| Format fence loud warning | 4, 5 | `tests/actor_wiremock.rs` `f13_format_too_new_fence_no_upload` (+ f11/f12/f14 — the whole fence set) + `f07_scheduled_edit_flushes_through_the_document_while_not_native`; the per-board health derivation + `SidecarHealthView` in `taskboard status` (chunk-4 tests, structural — the rendered-string assertions were dropped per AGENTS §5, PR #405 review notes); chunk 5's badge — `sidecar_caption`/`SyncBadge.sidecar` tests in `crates/taskboard-ui-slint/src/presentation.rs` |
| notify-rust / deny / auto | 8 | `crates/taskboard-app/src/notifications.rs` `resolve_sink_matrix` (the full 3×3) + the payload-mapping units at the port boundary; the workspace pin (4.18.2, `z-with-tokio` by measurement — commit `a5c746b`'s record); `cargo deny` green in PR #420 + this chunk's re-run |
| find | 9 | the in-module suite in `crates/taskboard-domain/src/find.rs` `mod tests` (8 proptests, contract vs algorithm-shaped tagged — the firewall); CLI `find` tests (typed `--type`/`--limit`, exit 0 on no hits) in `crates/taskboard-app/tests/`; `crates/taskboard-ui-slint/tests/journeys.rs` `the_palette_searches_and_jumps_across_boards` |
| today/inbox/capture/ring | 10 | `journeys.rs` `the_today_view_walks_completes_and_recommits` + `the_inbox_triage_moves_across_the_board_edge` (the chip-reads-zero structural assertion rides it); the capture engine snapshots (the chunk-10 insta pass over the s-series corpus); `today_sections`' P1–P6 proptests in `crates/taskboard-domain/src/days.rs`; the today golden matrix (`crates/taskboard-ui-slint/tests/golden_visuals.rs` + `goldens/`) |
| Row 9 (this chunk) | 11 | the PR itself + this report |

### 2.4 Tier-record facts the report must quote (MD-5)

- Chunk 3's gate ran for real on both tiers at final HEAD (the
  stage-2 report §2 + PR #402 body) — the strongest live-tier
  evidence of the round.
- Chunk 4's C-series never executed; tier 1 (wiremock) covers the
  same observables (the stage-2 report says so explicitly and
  recommended a stage-3 pass that no record shows happening).
- The CI `Nextcloud Integration` workflow (the weekly tier-2 lane)
  last ran **2026-10-21 — a failure of the "Nextcloud Integration
  (docker)" job, before chunk 1 merged** (pre-wave; last success
  2026-10-14). No wave code has ever run on that lane; it is
  dispatchable on demand (`gh workflow run nextcloud-integration.yml`)
  — the user's call (D5).
- CI on `main` at `c0465fa`: the `CI` workflow's latest runs are
  green (2026-10-24, on the two chunk-10 merges).

## 3. `docs/architecture.org` amendment list (section-by-section)

Spine: the wave's one architectural story is the **two-lane field
family** (native `startdate` where the connector's `CARD_STARTDATE`
holds; the per-board app-data sidecar document otherwise) plus the
**sidecar subsystem** that carries the AppData lane; everything else
is per-crate-role trueness. Every amendment must read true against
the shipped code — stage 2 names the symbol each claim cites and
spot-checks it in the worktree (the dispatch's prose-code-drift
trap).

1. **Crate role 1 (`taskboard-domain`)** — grow the entity/message
   inventory: the field family (`Task.scheduled/effort/importance/
   urgency` + four `TaskClocks` slots; `Effort` the u64-seconds
   newtype; `Importance`/`Urgency` two distinct five-level enums,
   declaration order = severity), the exported `TASK_FIELDS`
   manifest (`FieldFamily::{Native, AppData}`), `TaskChanges`
   double-option slots (`Some(None)` clears and stamps); the new
   modules: `days.rs` (`day_window`, `today_sections` — pure
   fixed-offset day arithmetic, the consumers contract),
   `find.rs` (the ranking core, crate-swap firewall),
   `appdata.rs` (the versioned document codec, embedded-clock
   union + snapshot-absence GC, verbatim unknown-content
   preservation, `scheduled` the fourth modeled key),
   `capabilities.rs` (`DeckFeatures` + `flags_for` — the codebase's
   only version switch); `merge.rs`'s `fold_task_app_data` +
   `fold_reminder_rules`; `reminders.rs`'s `AdhocAlarm` /
   `ReminderRef::Adhoc` / `ReminderAnchor{Task|Alarm}` / the alarms
   pass + settlement prune; `CaptureTask` with the Inbox/Backlog
   resolution rules; the reminder verbs (`AddReminderRule`/
   `RemoveReminderRule`/`ClearReminderRules` — `SetReminderRule`
   retired); `SyncReport::Completed.app_data: AppDataEvidence`
   (`Document|Absent|Unreadable{FormatTooNew|Transport|Decode|
   ForeignBoard}`). Keep the crate's purity sentence true — the
   sidecar module is pure serde + pure functions (`serde_json` for
   verbatim preservation is its only new dependency).
2. **Crate role 2 (`taskboard-state`)** — the engine's sidecar half:
   the fold runs inside `apply_sync_report` after the task merge
   loop; the guard mirror (`UpdateAppData` intent sets all app-field
   flags + rules; the fold copies values **and embedded clocks**);
   per-board sidecar health derived onto `SyncStatus.sidecar`
   (memory-only); op routing driven from the manifest (assertion 4);
   the tick's alarms pass + the settlement prune after snooze
   compaction; the `CaptureTask` handler (unbound global Inbox board
   created/adopted canonical-first; per-board Backlog).
3. **Crate role 3 (`taskboard-sync-nextcloud`)** — the connector's
   sidecar half: the capability probe (`GET …/cloud/capabilities`,
   `deck.version` → `DeckFeatures`, once per actor lifetime,
   conservative fallback); capability-gated card conversion
   (`scheduled` → wire `startdate` only when native); the
   unconditional additive attachment-create `data` field + the
   `fileName` → `info.filename` echo fallback (the 1.17.5 parity
   wire changes); the anchor ceremony (exact-title `[taskboard]
   app-data` scan of active+archived, detail-read confirmation,
   newest-stamp/lowest-id pick, exactly-one-unconfirmed adoption,
   lazy actor-side creation); download during the pull; the flush
   between pull and report as a read-modify-write union (lane-GC +
   snapshot-absence GC; rebuild mode = all local entries;
   restore-then-replace for tombstoned carriers); the D-S7 fences;
   `LoopState`'s in-memory caches (documents + features — no
   persisted validator kind); the document push group in
   `plan_pushes` (one per board, after `CreateTask` groups).
4. **Crate role 4 (`taskboard-storage-sqlite`)** — the schema list
   gains migrations **0007** (the four value columns + four
   epoch-defaulted clock columns + `reminder_rules.updated_at`)
   and **0008** (`adhoc_alarms`, 0004 conventions); the rule write
   clock is read/written by the codec since chunk 4.
5. **Crate role 5 (`taskboard-ui-slint`)** — the view machine's
   growth: `ViewId::Today` (subjectless, cycle head, the **desktop
   default start view** — kiosk keeps Board via `UiSettings.kiosk`),
   `ContextId::{Today, Palette}`, `Flow::{Capture, Palette}`,
   the compositions `today-view.slint` / `capture-overlay.slint` /
   `search-palette.slint` / `rule-list.slint`, the flat cross-board
   move ring (`board › stack` hint), the toolbar inbox chip,
   `IdTables.{today_tasks, palette}`, the field-decoration carriers
   (`TaskCardModel.decorations` + the presentation phrase helpers),
   the sidecar badge. No slint-side semantics — presentation only.
6. **Crate role 6 (`taskboard-app`)** — config gains the
   `[preferences] day_start_time` section (typed validation,
   device-local) and `[reminders] sink = auto|log|desktop`
   (`auto` resolves by `AppMode` at boot via
   `notifications::resolve_sink` / `bootstrap::resolve_notifier_sink`);
   `notifications` grows `DesktopSink` (notify-rust 4.18.2) behind
   the unmoved `NotificationSink` port — replace "desktop toasts are
   a follow-up" with the shipped story (passive toasts, no
   retry/fallback-send); `commands` gains `find` / `capture` /
   `timers`, the `tasks edit` field flags (`parse_effort`'s one
   grammar), `remind --remove` selectors, and the anchor-title CLI
   guard (`CommandError::AnchorTaskReserved`, app layer).
7. **New subsection "The app-data sidecar"** (under "Architecture
   Overview & Core Patterns", after the board-selection subsection):
   the wave's architectural narrative in compact form — the two
   lanes and the `FieldFamily` assignment (9 native rows incl.
   `scheduled`-when-native, 3 AppData rows); the per-board document
   (versioned envelope, tasks + reminder-rules sections, reserved
   key space `proofs`/`settings`/`task-kind`/`respite`/`recurrence`,
   verbatim preservation — the v1-device-as-transparent-relay
   guarantee); evidence → per-board health → the loud format fence
   (CLI `status` warning + UI badge); device-local things that never
   sync (alarms, deliveries/snoozes/muted); the accepted limitation
   (cross-board card moves lose app-fields — #350). A short
   "vocabulary after the daily-driver wave" table may carry the
   command/verb deltas if prose grows unwieldy — mirror the
   multi-board subsection's compactness, do not pad.
8. **Communication-protocol table** — no new channels landed (the
   sidecar rides the existing sync-actor cycle; the sink rides the
   existing notifier port); verify the table still reads true and
   touch only the *State Engine → Notifier* row if it names LogSink
   as the only sink (it should name the port with both sinks).
9. **Tech-stack table** — add `notify-rust` (4.18.2,
   `[workspace.dependencies]` pin, default-features off with
   `z-with-tokio` by measurement; desktop toast delivery).
10. **Board-selection subsection** — add the today view row: the
    today surface is cross-board on desktop (selection-follows-focus
    retargets `ViewState.board`) and the desktop launch view; the
    caller-owned rule stands.

## 4. `README.org` amendment list

1. **New "daily driving" usage** (the quickstart neighborhood): the
   four field flags with one example line
   (`taskboard tasks edit <task> --scheduled "tomorrow 09:00"
   --effort 1h30m --importance high --urgency low`; `none` clears);
   `taskboard find <QUERY> [--type task|stack|board] [--limit N]`
   (exit 0, empty table on no hits); `taskboard capture <TITLE>
   [--board <BOARD>]` (global Inbox vs Backlog; the GUI Ctrl+K
   overlay); `taskboard timers add/list/cancel` (`--in`/`--at`,
   host-local at the CLI edge, device-local — never synced).
2. **Configuration section**: add `[preferences]` with
   `day_start_time` (default `"00:00"`; an unparseable value is the
   typed `invalid-day-start-time` error, never a silent default;
   device-local, never synced); add `[reminders] sink =
   "auto"|"log"|"desktop"` (default `auto`: Desktop → toasts,
   Daemon/Kiosk → the journald log path, byte-identical to before).
3. **The sync section — the anchor card's Deck-UX note** (the
   dispatch-named item): once a board syncs app-only data (an
   effort/importance/urgency edit or a reminder rule), taskboard
   creates one card titled `[taskboard] app-data` in that Deck
   board carrying a `taskboard-appdata.json` attachment — it is
   taskboard's document store, not a task; leave it alone (the CLI
   refuses rename/delete on its local mirror; deleting it Deck-side
   triggers a rebuild, not data loss). Also state the two-lane
   `scheduled` behavior: Deck's native start-date field where the
   server supports it (Deck ≥ 1.19), the sidecar document where it
   does not (e.g. Deck 1.17.5) — detected automatically per server.
4. **Desktop UI keys** (where the switcher keys are documented):
   Today is the desktop launch view (kiosk keeps the board);
   Ctrl+K opens quick capture (Enter commits-and-continues into the
   global Inbox, Esc closes); the search palette (Ctrl+F / Ctrl+s /
   `/`) jumps to boards/stacks/tasks; the toolbar inbox chip routes
   to the Inbox; **the move ring crosses board edges** (ordinary
   task moves cycle every live board's stacks — the deliberate
   change; `tasks move --board` pre-filters by board).
5. **Fix the wave-caused stale note** (§2.2): "a bare add requires
   exactly one live stack" → bare `tasks add` lands in the single
   live board's Backlog stack (created on demand); zero boards
   refuses typed as before; several live boards is a typed
   `ambiguous-board`; use `--stack` or `--board` to aim elsewhere.
6. **Kiosk**: verify the existing rows read true (kiosk launches on
   the board view; no capture surface; the log sink) — expected
   no-op, amend only if a sentence contradicts the landed code.
7. **Accuracy sweep** of every new verb/flag/key spelling against
   `crates/taskboard-app/src/cli.rs` on the branch (the precedent's
   §4.6 rule — e.g. `--in`/`--at` not `--duration`; the selector
   vocabulary per verb).

## 5. `CHANGELOG.md` promotion plan

Inventory: §2.2's table (11 wave entries; 10 Added + 1 Changed).
Rules (mirroring the precedent's D1, adapted):

- **No release cut** — everything stays under `[Unreleased]`; the
  user runs `bump_version.sh` when they cut a release.
- **Attribution normalization**: the wave's entries name the wave,
  not internal chunk numbers — rewrite each entry's "(…, chunk N)"
  attribution to "(…, daily-driver wave)" (e.g. "(#346, #347,
  chunk 10)" → "(#346, #347, daily-driver wave)"; "(#204, chunk 6)"
  → "(#204, daily-driver wave)"), and give the three bare entries
  (day-boundary, field family, manifest) their wave attribution
  ("(#345, daily-driver wave)" etc.). Keep every issue number and
  every factual clause byte-equivalent.
- **The orphan marker**: delete the stray `=======` line at :43
  (it sits inside the promoted section; its removal is part of the
  promotion's hygiene, recorded in the report's Findings with this
  line number as plan-day reference).
- **Adjacency**: the wave's Added entries must read as one set. The
  GUI-r2 "action registry round-up & object context menus" entry
  currently sits inside the run (between the fuzzy-search and
  toast-sink entries) — relocate it, content untouched, to the
  start of the non-wave Added content (directly after the wave's
  run). Position-only move; the r2 entries' other positions are
  untouched. (Precedent D1's "re-ordering only where needed" clause,
  applied once.)
- **Nothing invented**: no umbrella/summary entry; each landed
  entry already states its user-visible change.
- **Semantic-change fidelity** (the dispatch trap): stage 2
  re-reads the three trap-named statements against the code —
  the N-rule add-semantics entry against
  `crates/taskboard-domain/src/command.rs`'s planner + the receipt
  types; the move-ring/deliberate-changes list against chunk 10's
  landed behavior; the Today-default statement against
  `UiSettings.kiosk`'s gating — and fixes only misstatements (none
  found in the survey; the survey expects none).
- **Losslessness proof**: stage 2 diffs its result against the
  inventory entry-by-entry (the precedent's sorted-line-multiset
  check, allowing exactly: the attribution rewrites, the orphan-line
  deletion, the one relocation); stage 3 re-diffs.

## 6. Exit report outline (`.artifacts/reports/<stage2-date>-daily-driver-wave-exit-report.md`)

Header: date; base SHA (`c0465fa`, the merge-base of the PR against
`origin/main`); the chunk → PR map (§2.1); links to the
implementation plan §5 and this plan. Then **one section per §5
exit-criterion row** (criterion verbatim), each with: the verifying
chunk(s) + named tests (paths + test names from §2.3 — no vague
pointers); the MD-5 tier record where applicable (§2.4, quoted
honestly — including anything that could not run); the verdict.
Verdict vocabulary (D4): **met** / **met — hermetic with recorded
live-tier limitation** / **not met**. Expected verdicts (stage 2
confirms against its own runs):

1. **All four fields sync day one** — expected: "met — the native
   lane tier-2-proven (chunk 3's gate ran `startdate_round_trip`
   green on Deck 1.19.0; the 1.17.5 fallback tier pins the measured
   unsupported behavior), the document lane tier-1-proven with the
   C1–C4 tier-2 convergence fixtures recorded as never-executed
   skips (no docker in the chunk-4 session; the CI weekly lane has
   not run wave code)" — the honest hermetic phrasing of D7 in the
   precedent, per MD-5. If stage 2's own tier-2 attempt passes
   C-series fixtures, upgrade the row and say so.
2. **Anchor ceremony** — expected "met": unit + wiremock f-series +
   both-tier attachment lifecycle (creation/adoption/pick/restore-
   then-replace/rebuild).
3. **Deck 1.17.5 gate green** — "met" with the records quoted
   (14/14 + 3/3, zero divergences, final-HEAD re-run; the STOP →
   ruling → addendum chain; CI's env-guarded skip noted).
4. **Manifest assertions** — "met" (all four homes, §2.3).
5. **Format-fence loud warning end-to-end** — "met" (f13 + the
   health derivation + `status` + the badge; note the structural
   assertion style per AGENTS §5).
6. **notify-rust / deny / auto** — "met" (the 3×3 matrix, the
   measured `z-with-tokio` record, per-PR deny + this chunk's re-run).
7. **find suite + verb + palette journey** — "met".
8. **Today/inbox/capture/ring** — "met".
9. **CHANGELOG promoted, docs amended, exit report** — "met" by
   this PR (self-referential; cite the PR).

Then: **Method** (§7 restated in report prose so every pointer is
re-verifiable); **Findings** (one line each, honest — the known
candidates: the C-series tier-2 gap; the CI weekly lane's pre-wave
failure 2026-10-21; README's "Until the first release is cut"
staleness; the orphan `=======` marker's removal; anything stage 2
encounters); **Backlog candidates** (restated verbatim from the
implementation plan §1 Out list + the design docs' backlog sections
— task-kind/respite/recurrence reserved keys, proofs/settings
sections D5, #203 insistence, #352 timers UI, #351 settings editor,
#350 cross-board repair, `tasks add` flag parity, decision support;
**not promoted**, no issues created); **Bookkeeping** (the milestone
roster: every issue closed through chunks 1–10 except #202 — open
pending the D5 wave; the #202 milestone-clear comment is stage 3's
ordered exception with the framing below; the milestone's closure
itself is the user's act; no other tracker edits).

## 7. Verification method (stage 2 executes; the report restates it)

1. **Re-locate**: resolve every §2/§3/§4 path/symbol against the
   branch base (`c0465fa`; re-run `git fetch origin --prune` first).
2. **Re-run the hermetic set** (worktree, named filters; record
   counts): `cargo nextest run -p taskboard-domain pipeline::` +
   `-p taskboard-storage-sqlite --test schema` (assertions 1–2);
   `-p taskboard-domain --lib appdata` + `find` + `days` +
   `capabilities` (the in-module suites); `-p taskboard-state
   --test manifest_routing` (assertion 4); `-p taskboard-sync-nextcloud
   --test actor_wiremock` (the f-series + the S-series evidence
   tests); `-p taskboard-app notifications` (the sink matrix) +
   the CLI `find`/`timers`/`capture` integration filters;
   `-p taskboard-ui-slint --test journeys` (the three wave legs);
   `cargo deny check`.
3. **Tier-2 attempt, once, honestly** (MD-5; the precedent's D2):
   if docker is present, bring the default tier up
   (`scripts/nextcloud_it_setup.sh up`) and run the C-series +
   the deck fixtures via `cargo nextest run -p taskboard-sync-nextcloud
   --run-ignored only` (the default-tier set), recording pass/fail
   either way; attempting the pinned Deck 1.17.5 variant
   (`TASKBOARD_IT_SUFFIX=-deck1175 … DECK_VERSION=v1.17.5`, the
   invocation recorded in the chunk-3 STOP report §1) is recommended
   — if it cannot be brought up, record that, never skip silently.
   **No autonomous CI dispatch**: do not `gh workflow run
   nextcloud-integration.yml`; record instead that the lane is
   dispatchable and has not run wave code (last run 2026-10-21,
   pre-wave failure; last success 2026-10-14).
4. **Re-read**: `gh pr view` for #396/#398/#402/#405/#406/#410/
   #419/#420/#422/#433 (the bodies are the chunks' stage records —
   quote the tier records from them); the stage-2 reports for chunks
   3/4 on file.
5. **Semantic-change fidelity pass** (§5's trap list) and the
   README/architecture symbol spot-checks (§3/§4's "names the
   symbol each claim cites").
6. **Gates**: the full AGENTS §10 suite on the docs-only diff
   (§9).

## 8. Settled decisions (open questions resolved)

- **D1 — CHANGELOG promotion semantics**: no release cut; entries
  stay under `[Unreleased]`; promotion = attribution normalization
  to "(…, daily-driver wave)" + adjacency of the wave's entries +
  the orphan-marker deletion + the one r2-entry relocation; every
  factual clause preserved; nothing invented; non-wave entries
  otherwise untouched (§5).
- **D2 — architecture.org spine**: a new "The app-data sidecar"
  subsection carries the two-lane + subsystem narrative; the crate
  roles are amended for trueness, not rewritten; every claim names
  its symbol (§3).
- **D3 — README**: the anchor-card Deck-UX note lives in the sync
  section; the wave-caused `tasks add` staleness is fixed in this
  diff; non-wave staleness goes to Findings (§4, D7).
- **D4 — verdict vocabulary**: met / met-hermetic-with-recorded-
  live-tier-limitation / not met (the precedent's grades); the
  fields-sync row is expected to close on the second grade unless
  stage 2's own tier-2 attempt upgrades it.
- **D5 — tier-2 policy**: one honest attempt if docker is present,
  default tier first, deck1175 recommended; no autonomous CI
  dispatch (the lane's state is recorded, dispatching is the
  user's call) (§7.3).
- **D6 — report filename date**: `<stage2-date>` = the day stage 2
  writes it (2026-10-25 if it runs today); this plan refers to it
  symbolically.
- **D7 — non-wave drift**: wave-drift in the two docs is fixed;
  everything else stale lands in the report's Findings — the PR
  diff is exactly the two doc amendments + CHANGELOG + exit report
  + this plan.
- **D8 — tracker discipline**: stage 2 makes no tracker edits; the
  #202 milestone-clear comment is stage 3's single ordered
  exception (framing in §11); the milestone is closed by the user,
  never by an agent (MD-4).
- **D9 — pointer stability**: the report cites paths + test names
  only; line numbers never load-bearing.
- **D10 — commit ladder** (stage 2; each commit standalone,
  rebase-merge hygiene): `docs(architecture): the daily-driver wave
  subsystems` → `docs(readme): daily-driver verbs, fields, and
  config keys` → `docs(changelog): promote the daily-driver wave
  entries` → `docs(report): daily-driver wave exit verification`;
  this plan commit precedes them (`docs(plan): daily-driver wave
  exit docs`).
- **D11 — the three CHANGELOG semantic-change statements**: present
  and accurate in the survey; stage 2 re-verifies against code
  (command.rs planner, chunk-10 behavior, `UiSettings.kiosk`
  gating) and fixes misstatements only — the survey expects none.

## 9. Quality gates (AGENTS §10, verbatim sequence)

Docs-only diff, but the full suite runs regardless: `cargo fmt
--all -- --check`; `cargo clippy --workspace --all-targets -- -D
warnings`; `cargo nextest run --workspace`; `cargo test --doc
--workspace`; `cargo deny check`; the CI-identical three-command
llvm-cov sequence (+ the profraw quarantine; + the trailing
`rm -rf target/llvm-cov-target` local hygiene).

## 10. Risks

- **Prose-code drift**: architecture.org/README claims that don't
  match the shipped code are the dispatch's named trap — mitigated
  by §3/§4's symbol-per-claim rule and stage 3's spot-check
  mandate.
- **CHANGELOG losslessness**: eleven entries interleaved with four
  other work streams' entries, plus the orphan marker and the
  relocation — mitigated by §5's inventory + the entry-by-entry
  diff check at both stages.
- **Tier-2 honesty**: the round's one soft spot is the never-run
  C-series; the report must neither overstate (a "met" without the
  recorded limitation would be a finding) nor understate (the
  tier-1 + default-tier gate evidence is genuinely strong).
- **Evidence pointers must resolve**: stage 3 verifies every cited
  test exists and is the cited behavior's (prompt 11.3) — the §2.3
  names were verified at plan time but are plan-day references.
- **Full gate-suite cost on a docs-only diff** (~build + coverage
  tree): expected; the worktree is removed at stage-3 teardown per
  AGENTS §8.

## 11. Stage-3 handoff (dispatch §12 prompt 11.3 specifics)

Rebase onto `origin/main` first (drop nothing — nothing stacked;
push `--force-with-lease` only if history was rewritten); review
the diff against the post-rebase base under the **evidence &
artefact mandate** in full: every exit row's pointers resolve (run
the cited tests); tier records quoted honestly; the docs match the
landed code (spot-check each new verb/flag/config key against the
source); the promotion states the three semantic changes
accurately; the diff is exactly the docs + report. Then ship: push,
open the PR (body: the wave-exit summary + the exit-report link;
**no `Closes` line** — chunk 11 closes nothing; #202 remains open
by design), `gh pr checks --watch` to green, `mergeStateStatus:
CLEAN`. Post the one ordered tracker comment on **#202**: the
`wave-daily-driver` milestone is otherwise clear (every roster
issue closed through chunks 1–10); reminder-rule sync — the rules
half — landed via chunk 4 (PR #405, `fold_reminder_rules` + the
document's reminder-rules section); delivery proofs and settings
sync stay with the future D5 wave per the issue's scope comment;
link the exit report + PR. **No other tracker edits; no milestone
edits.** Teardown: `git -C /home/viktoria/src/taskboard worktree
remove --force ../taskboard-WT-docs-daily-driver-wave`, confirmed
via `git worktree list`.

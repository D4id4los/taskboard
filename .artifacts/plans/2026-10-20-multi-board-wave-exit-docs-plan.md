# [Plan] Multi-board Chunk 8 — Docs Amendments + Wave Exit Verification

- **Date**: 2026-10-20
- **Kind**: stage-1 work plan (dispatch plan §9, prompt 8.1) for chunk 8
  of the multi-board implementation round — docs-only, no code, no
  tracker edits.
- **Branch / worktree**: `docs/multi-board-wave` at
  `../taskboard-WT-docs-multi-board-wave` (created off `origin/main` =
  `419c70f`, chunk-7 stage-2 report tip; chunks 5–7 verified merged —
  PRs #375, #376, #377).
- **Normative sources** (MD-7 rank): implementation plan §3 chunk 8 /
  §5 (the exit-criteria → verifying-chunk map) / §6 (bookkeeping) / §1
  (Out list); design §4 (coherence map — the four-vocabulary table is
  the documentation spine), §3.9 (compat summary), §7 (backlog
  candidates); dispatch plan §1 (execution model), §9 (chunk 8 covers,
  deliverable, acceptance, traps), §10 (bookkeeping). Deviations are
  recorded in §8; nothing here deviates from a design §5 ACCEPTED
  decision.
- **Line numbers below are plan-day references** (MD-8, re-verified
  2026-10-20 against `419c70f`); symbols, paths, and finding IDs are
  the stable keys. Stage 2 re-locates every cited symbol at its own
  start.

## 1. Scope

**In** (dispatch §9 "Covers", whole): amend `docs/architecture.org`
(five wave topics: connector registry + `ConnectorModule`, per-board
status map + derived attribution, counted all-down authority,
`SetBoard` retirement, caller-owned selection — §3 below); amend
`README.org` (config shape + synthesis rule + typed rejections;
multi-board usage `--board`/`boards use`/switcher keys — §4); promote
the CHANGELOG's per-chunk Unreleased lines into the wave's
user-visible entries, losslessly (§5); write the wave exit-criteria
verification record at
`.artifacts/reports/<stage2-date>-multi-board-wave-exit-report.md`
(§6), one section per implementation-plan §5 exit-criterion row with
named tests and honest tier-2 records (MD-5), plus the method section
and the backlog-candidates restatement.

**Out** (non-goals, dispatch §9 + implementation plan §1): no ADR 0010
text (landed verbatim in chunk 4 — duplication is a stage-3 finding);
no new drift amendments (F10-03/F10-04/F15-18 stay backlog); no code
(any `.rs` diff is a finding); no tracker edits (issues ride the PR
body: `Closes #125` — the wave exit; no autonomous issue creation,
labelling, or backlog promotion); no release cut (bump_version.sh is
the user's flow); non-wave doc drift observed during work is recorded
as report findings only, not fixed in this diff.

## 2. Re-verified evidence inventory (2026-10-20, base `419c70f`)

The wave's seven chunks and their PRs (all merged; bookkeeping map
per implementation plan §6):

| Chunk | Branch | PR | CHANGELOG lines landed via |
|---|---|---|---|
| 1 | `feat/state-sync-status-map` | #358 | `dbd01da` (rode the feat commit) |
| 2 | `feat/domain-board-scoped-commands` | #362 | `9bd4d10` |
| 3 | `feat/state-board-ceremonies` | #364 | `6014a53` (rode the feat commit) |
| 4 | `feat/sync-connector-modules` | #368 | `2c27be6` |
| 5 | `feat/app-connector-registry` | #375 | `bdfe7b9` (rode the test commit) |
| 6 | `feat/app-cli-connector-verbs` | #376 | `4c23ccd` |
| 7 | `feat/ui-board-switcher` | #377 | `9a147b6` |

Current doc state (the files stage 2 amends):

- `docs/architecture.org` (398 lines): the communication-protocol
  table still documents `SetBoard` ("State Engine → Sync Actor" row)
  and "this actor is the network authority" emitting the global
  events ("Sync Actor → Everyone" row); crate roles 3/4 still say
  "pulls the bound board" (singular), `SetBoard` sets the pull
  target, `sync_status` singleton; crate role 6 documents the
  standalone/backend sync spawn, not the registry; no mention of
  `ConnectorModule`, the authority, the per-board map, or board
  selection anywhere.
- `README.org` (422 lines): the Configuration section shows only the
  legacy scalar shape; "Legacy path" notes still say "*Single board:*
  the MVP binds exactly one board; `boards select` refuses a rebind"
  — stale since chunk 2/3; no `--board`/`--connector`/`boards use`/
  switcher-keys usage anywhere.
- `CHANGELOG.md`: `[Unreleased]` carries the wave entries interleaved
  with kiosk-wave/perf entries; chunks 1–5 entries carry inline
  "(multi-board wave chunk N)" attributions, chunks 6–7 carry none.

Named tests on `origin/main` (the exit report's pointers; every path
re-verified to exist at `419c70f`):

| Verifying chunk | Named test (path `crates/…`) | Asserts |
|---|---|---|
| 1 | `taskboard-storage-sqlite/src/connect.rs` `migration_0006_backfills_the_bound_boards_row_and_drops_the_singleton` (inline, ~:387) | a pre-0006 db migrates: the singleton's phase/last_success backfill the one bound board's row; the `CHECK(id=1)` shape is structurally gone (C10-1/C10-2) |
| 1 | same file `migration_0006_drops_a_board_less_singleton` (~:458) | the standalone board-less singleton is dropped (C10-3, the recorded §3.9 deviation) |
| 1 | `taskboard-state/tests/snapshots/` — 20-file corpus `snapshots__s{1..10}_{persisted_state,published_app_state}.snap` (`--test snapshots`) | the `AppState.sync` per-board-map widening pinned across the whole scenario corpus (DF-9 snapshot pass 1) |
| 4 | `taskboard-app/tests/it_docker.rs` `v0_1_x_additive_upgrade_boot_settles_offline_without_a_server` (~:772, hermetic — runs everywhere) | the v0.1.x db boots: migration ran, tagged-default decode, status hydration — the boot-time half of the §3.9 contract |
| 4 | `taskboard-app/tests/it_docker.rs` `it_nextcloud_docker_v0_1_x_additive_upgrade_boot` (~:839, tier-2) → `v0_1_x_additive_upgrade` body | the full §3.9 list against a dockerized Nextcloud: synthesized connector, adoption, cycle merges bit-for-bit, queued op drains |
| 4 | `taskboard-sync-nextcloud/tests/actor_wiremock.rs` `a29_newly_bound_board_joins_the_set_by_the_next_cycle`, `a30_per_board_passes_plan_only_their_own_ops`, `a31_foreign_tagged_board_ops_are_never_planned`, `a32_per_board_failure_continues_the_loop`, `a32b_network_failure_ends_the_cycle_before_the_next_board` | the derived board set, per-board push isolation, foreign-tag stranding, continue-vs-end-early with the once-only episode edge |
| 5 | `taskboard-app/src/bootstrap.rs` `mod connector_module_tests`: `registry_loop_wires_the_fake_module_end_to_end`, `two_connectors_live_simultaneously_over_the_named_registry`, `the_authority_counts_scripted_episodes_across_two_connectors`, `module_build_failure_maps_onto_the_typed_bootstrap_error` (inline, ~:1003+) | the registry loop over `ConnectorModule`s, N=2 live via the named registry, the authority's counted crossing — the hermetic N=2 evidence |
| 5 | `taskboard-app/src/authority.rs` `mod tests`: `single_connector_passes_through_the_episode_edges`, `two_connectors_emit_only_on_the_all_down_crossing`, `non_member_events_change_nothing`, `attach_fills_a_real_hole_loudly_and_silently_opens_the_empty_set` (inline) | the all-down counting semantics unit-level |
| 5 | `taskboard-app/tests/it_docker.rs` `it_nextcloud_docker_two_connectors_live_and_partial_outage` (~:954, tier-2) → `two_connector_body` | two dockerized instances, boards bound across both, per-connector health in status, one connector down ⇒ its boards `Offline` + the global banner silent; honest-skip gate on `TASKBOARD_IT_DOCKER_URL` |
| 6 | `taskboard-app/tests/it_docker.rs` `it_nextcloud_docker_move_board_between_connectors` (~:1250, tier-2) → `move_board_body` | a board bound through alpha, unbound, re-bound onto beta's remote via `--connector beta`; per-board attribution verified moving in the derived grouping; both servers' listings intact |
| 7 | `taskboard-ui-slint/tests/journeys.rs` `board_switching_walks_boards_by_keyboard` (~:474, hermetic headless platform) | seed = lowest live board; `board.next` re-renders the second board's stacks + toolbar title; no wrap at the end; `board.previous` back; the compact leg hides the strip while keyboard switching works; `assert_commands(&[])` after every step — the dumb-client pin |

MD-5 tier-2 records in the PR bodies (cited, never papered over):

- **#368 (chunk 4)**: docker available; app `--test it_docker` 6/6
  passed incl. the v0.1.x boot test; sync `--test it_docker` 11/12
  with the pre-existing harness flake recorded as #367
  (watchlist policy; not wave-caused).
- **#375 (chunk 5)**: the dockerized two-connector test was attempted
  six times on the agent runner — the runner's docker host
  externally stops the first test-owned instance ~86 s after spawn
  (exit 0; ports/volumes/orphan-teardown/traffic refuted; forensics
  in
  `.artifacts/reports/2026-10-20-multi-board-connector-registry-chunk5-review-report.md`).
  The N=2 criterion was verified hermetically in-suite
  (`bootstrap::connector_module_tests`); #187 closed on that basis.
- **#376 (chunk 6)**: the move-board journey ran and passed on its
  runner (docker present, 171 s) — which also boots a two-connector,
  two-instance world live (corroborating the N=2 row); the chunk-5
  host-kill note carried forward; "CI's nextcloud-integration
  workflow is the stable runner".
- **#377 (chunk 7)**: hermetic by construction (the headless test
  platform); full gates green, 1275 nextest tests.

CI state: the `CI` workflow's **Test Execution** job is green on
`main` at `419c70f`; the failing `main`-run job is **Performance
Benchmarking** (Bencher) — pre-existing, unrelated to the wave. The
`Nextcloud Integration` workflow (weekly Wednesdays 04:00 UTC +
manual dispatch) last ran 2026-10-14, before the wave landed; its next
scheduled run exercises the three tier-2 tests on the stable runner.

## 3. `docs/architecture.org` amendment list (section-by-section)

Spine: design §4's four-vocabulary table, restated compactly, then
made true per crate role. Every amendment must read true against the
shipped code (dispatch §9 trap: prose-code drift is a finding; stage 2
names the symbol each claim cites and spot-checks it in the worktree).

1. **"Inter-Component Communication Protocol" table** —
   *State Engine → Sync Actor* row: `SyncCommand` nudges are
   `{SyncNow, Detach}`; delete "`SetBoard` (pull-target selection,
   Phase 6's CLI path)" and state the replacement: the actor derives
   its per-cycle board set from the persisted connector-tagged
   bindings (no targeting message; the broadcast `SyncNow` is the only
   nudge, deduped in-flight). — *Sync Actor → Everyone* row: actors
   emit `SystemEvent::ConnectorLost/ConnectorRestored(ConnectorId)`
   (episode-edge: exactly once per offline episode, report-then-restore
   order); the global `NetworkLost/NetworkRestored` names move to the
   authority. — *new row, App authority → Everyone*: the app-layer
   counting task (`taskboard-app/src/authority.rs`, seeded from the
   configured module set, updated by attach/detach hooks) subscribes
   the system broadcast and emits the global events on the all-down
   crossing (last up flips down ⇒ `NetworkLost`; first up ⇒
   `NetworkRestored`); single-connector installs are byte-for-byte
   the old semantics. — *Session ⇄ core (connector ceremonies)* row:
   attach/detach are per-name (`ConnectorNameInUse`), matching the
   registry.
2. **Crate role 1 (`taskboard-domain`)**: payloads gain `ConnectorId`
   (config-name newtype), the connector-tagged `RemoteBoardBinding`
   on `Board.remote`, and `AppState.sync` as the per-board
   `SyncStates(BTreeMap<BoardId, SyncStatus>)` map with the
   `aggregate()` fold; attribution stays *derived from bindings,
   never stored* (design §4's one phrasing).
3. **Crate role 2 (`taskboard-state`)**: `SyncReport` ingestion
   re-keys the per-board map by the report's board; the
   `ConnectorLost(c)` arm scopes `go_offline` to *c's boards* (derived
   from the engine's own binding tags); `ConnectorRestored(c)`
   forwards the nudge.
4. **Crate role 3 (`taskboard-sync-nextcloud`)**: replace
   "pulls the bound board" and "`SetBoard` sets the pull target" with
   the derived owned-board set recomputed each cycle from persisted
   bindings (tombstoned boards drain; foreign tags excluded;
   per-board pull-cache keys, no cache-clear-on-rebind; unchanged
   boards cheap via per-board validators — the #157 guardrail);
   the push ownership filter (boards owned by this connector);
   the offline FSM emits the per-connector episode-edge events.
5. **Crate role 4 (`taskboard-storage-sqlite`)**: `sync_status` is a
   per-board table (migration 0006 rebuilt the singleton; the
   migration also added the `boards.connector` tag column — NULL
   decodes as the spawn-time default connector
   (`open(path, default_connector)` / `open_with` in
   `connect.rs`), upserts write the tag;
   legacy NULLs are read-only relics).
6. **Crate role 5 (`taskboard-ui-slint`)**: the view machine gains the
   board dimension — `ViewState.board` (seeded by
   `presentation::live_board` when absent), `select_board`
   mirroring `select_stack`, `board.next/previous/select` actions,
   the `board-bar.slint` strip (one segment per live board; compact
   hides it, keyboard switching remains); `IdTables` not re-keyed
   (the `push` builder derives its board argument from the
   selection); switching is pure view state (dumb-client rule).
7. **Crate role 6 (`taskboard-app`)**: bootstrap owns the connector
   registry loop over `[[sync.connectors]]`-configured
   `ConnectorModule`s (`connectors.rs`, ADR 0010's amended split:
   domain vocabulary + app module trait; `dyn` dispatch, one
   implementation today) and spawns the all-down authority;
   `commands` grows `boards unbind`/`boards reconcile`/REPL
   `boards use`, the `--board`/`--connector` flags, and the additive
   per-connector `status` arrays; `repl` carries
   `ReplSession.current_board`.
8. **New subsection "Board selection is caller-owned"** (under
   "Architecture Overview & Core Patterns", after the thin-client
   subsection): three homes, one per frontend — CLI `--board <uuid>`
   with the single-live-board default resolved at invocation (zero or
   ≥2 live boards ⇒ typed `ambiguous-board` refusal), REPL
   `ReplSession.current_board` set by `boards use`, UI
   `ViewState.board` (the switcher is its only mutator); nothing of
   selection enters the domain crate (DF-2/design §4).
9. **New compact "Vocabulary after the multi-board wave" table**
   (near the domain role or the communication section; design §4
   verbatim rows): Commands (nine board-scoped arms carry
   `board: BoardId`; `CreateBoard` always mints; `UnbindBoard` /
   `ReconcileByTitle` added) · Outbox ops (unchanged — already
   board-addressed; push filters by connector-owned boards) ·
   `CommandError` (−`NoBoard` −`BoardExists`; +`UnknownBoard` +
   `BoardDeleted`; `BoardAlreadyBound { board, bound }`;
   +`RemoteBoardInUse { remote, board }`) · Status payloads
   (`AppState.sync` per-board map; `SyncCommand` = `{SyncNow,
   Detach}`; `SyncReport` unchanged).

## 4. `README.org` amendment list

1. **Configuration section**: after the legacy scalar example, add the
   plural shape — `[[sync.connectors]]` array of named entries
   (`name` kebab-case + unique, `backend`, `server_url`, `username`;
   first entry = the default connector) with a two-connector example;
   state the synthesis rule (array absent ⇒ the legacy world verbatim:
   the `backend` auto rule stands, resolved `nextcloud` + `[nextcloud]`
   synthesize one connector named `nextcloud`, resolved `standalone`
   ⇒ none) and the typed mix rejections (scalar + array;
   `[nextcloud]` + array; `standalone` + array; duplicate or
   non-kebab name; `nextcloud` resolved with an empty array — each
   naming its offending key); note env composition: the array cannot
   be expressed via env, and `TASKBOARD_SYNC__BACKEND` over an array
   file is now a loud typed error.
2. **"Attaching a server" / a new "Multiple boards and connectors"
   subsection**: `--board <uuid>` on the board-scoped verbs (omit ⇒
   the single live board; zero or several ⇒ typed `ambiguous-board`
   listing candidates); `--connector <name>` on `boards list` /
   `boards select` / `boards publish` / `sync` (unknown name ⇒ typed
   `unknown-connector`); the per-connector/per-board `status` arrays;
   REPL `boards use <uuid>` (session-scoped; one-shot and
   daemon-socket lines refuse `session-only`).
3. **Move-board ceremony** (one short block, chunk 3+6 story):
   `boards unbind [--board] [--confirm]` (content and ops stay,
   stranding honestly until a re-bind), `boards reconcile` after
   re-bind, then `boards select <remote-id> --local-board <uuid>
   --connector <name>` rebinds onto the second connector — the
   documented move-board flow.
4. **Rewrite the stale "Single board" note** ("Legacy path: sync-only
   install" notes list): the one-board invariant is gone — multiple
   boards bind independently (each board's own binding; rebind
   semantics per board: same-remote rebind is an accepted no-op,
   `board-already-bound` names the board and its existing binding,
   a remote already adopted elsewhere refuses `remote-board-in-use`).
5. **UI switcher keys**: where the desktop UI operation is introduced,
   add the board dimension — the board-bar strip (one segment per
   live board; compact/kiosk hides it), `b`/`B`
   (`board.next`/`board.previous`, no wrap) in every preset's board
   scope, the per-board sync badge following the selected board;
   switching is pure view state.
6. **Accuracy sweep of every example against the shipped CLI**
   (stage 2): flag/verb spellings re-checked against
   `crates/taskboard-app/src/cli.rs` on the branch (e.g. both
   `--local-board <uuid>` and `--bind-local-board` exist on
   `boards select`; keep examples minimal and true).

## 5. `CHANGELOG.md` promotion plan

Inventory (the lossless checklist — every entry must survive stage 2's
edit; content verbatim, attribution normalized):

- **chunk 1** (`dbd01da`, Changed): "Sync status is tracked per board
  (multi-board wave chunk 1)…" — migration 0006, per-board table,
  `sync.aggregate()` fold, existing surfaces unchanged.
- **chunk 2** (`9bd4d10`, Changed): "Board-scoped commands carry
  their target (multi-board wave chunk 2)…" — nine verbs, `--board`,
  `ambiguous-board`, `no-board`/`board-exists` JSON kinds gone
  (breaking note), `boards create` always mints.
- **chunk 3** (`6014a53`, Added): "Board ceremonies at N (multi-board
  wave chunk 3)…" — `boards unbind`/`boards reconcile`, scope-stating
  refusals, `--local-board`, nudge-driven bind + auto-reconcile +
  wipe-count receipt.
- **chunk 4** (`2c27be6`, Added + Changed): "Connector identity
  end-to-end (multi-board wave chunk 4)…" (tagged bindings, derived
  per-cycle board sets, per-board caches, `ConnectorModule` seam) and
  "`boards select` and the sync actor no longer exchange a targeted
  `SetBoard` command…" (Changed).
- **chunk 5** (`bdfe7b9`, Added): "Multiple active connectors
  (multi-board wave chunk 5)…" + "Named connector registry…" +
  "Per-connector network events with an all-down authority…" (incl.
  the `require_connector` ⇒ `sync-disabled` change and the
  `TASKBOARD_SYNC__BACKEND`-over-array loud error).
- **chunk 6** (`4c23ccd`, Added): the additive `status` answer; the
  `--connector` flags entry; REPL `boards use`; the completion entry.
- **chunk 7** (`9a147b6`, Added): the UI board dimension entry
  (strip, `b`/`B`, per-board badge, pure-view switching).

Rules (decision D1, §8): **no release cut** — the entries stay under
`[Unreleased]` until the user runs `bump_version.sh`; the promotion
is the wave's *grouping*: normalize the inline attributions to the
single phrase "(multi-board wave)" (chunk numbers are internal
jargon), keep every factual clause byte-equivalent, touch no
non-wave entry, invent no umbrella/summary entry (each landed entry
already states its user-visible change — a new aggregate entry is the
"invented entry" trap). Order within Added/Changed stays as-is (the
wave's entries already cluster chronologically; re-ordering only
where needed so the wave's entries read as one set — e.g. the chunk-6
connector-verb entries sit with the chunk-5 connector entries).
Stage 2 verifies losslessness by diffing its result against this
inventory entry-by-entry; stage 3 re-diffs.

## 6. Exit report outline (`.artifacts/reports/<stage2-date>-multi-board-wave-exit-report.md`)

Header: date, base SHA (the merge-base of the PR against `main`),
the chunk → PR map (§2's table), links to the implementation plan
§5 and this plan. Then **one section per exit-criterion row**
(implementation plan §5, verbatim criterion text), each containing:
the criterion; the verifying chunk(s) + **named test** (path + what
it asserts, from §2's inventory — no vague "chunk 5's tests"
pointers); the MD-5 tier-2 run/absence record where applicable; the
verdict (met / met-hermetically-with-recorded-runner-limitation /
not met). Rows:

1. **Two connectors live simultaneously (tier-2)** — chunk 5:
   `it_nextcloud_docker_two_connectors_live_and_partial_outage` +
   `two_connector_body` (tier-2) and the hermetic set
   (`bootstrap::connector_module_tests::*`, `authority.rs` tests);
   record: #375's six-attempt docker-host stop with forensics link,
   the hermetic close, and #376's corroborating live two-connector
   run; the verdict names the hermetic evidence as the closing basis
   (as #187's closure did).
2. **Multiple boards syncing across them** — chunk 5
   (`two_connector_body`: one board per server, both pulled from the
   first cycle), exercised by chunk 6 (`move_board_body`: two alpha
   boards + one beta board live concurrently); same records.
3. **One board moved between connectors** — chunk 6:
   `it_nextcloud_docker_move_board_between_connectors` +
   `move_board_body`; record: ran and passed on the runner (171 s,
   #376).
4. **v0.1.x config + DB upgrade additively keeps working** — chunk 1
   (round-trip: both `migration_0006_*` tests + the 20-file snapshot
   corpus as the widening pin) + chunk 4 (boot test: hermetic half +
   `it_nextcloud_docker_v0_1_x_additive_upgrade_boot`); record: #368's
   6/6 app it_docker incl. the boot test (11/12 sync with the #367
   pre-existing flake noted, not wave-caused).
5. **Board switching by scripted keyboard journey alone** — chunk 7:
   `board_switching_walks_boards_by_keyboard` (hermetic; includes the
   dumb-client `assert_commands(&[])` pin and the compact leg).
6. **`cargo deny` green with the promoted port** — every chunk: no
   new dependency anywhere in the wave (chunk 4's only Cargo.toml
   delta is a `sqlx` dev-dep on the existing workspace pin); cite the
   per-PR deny results + this chunk's own re-run (§7).

Then: **Method** (§7 below, in report prose so every pointer is
re-verifiable by a reader); **Findings** (non-wave doc drift observed,
CI-on-main perf-job failure observation, anything stage 2 could not
re-run — honest, one line each); **Backlog candidates** (design §7 /
implementation plan §1 Out list restated verbatim for the user to
promote — per-connector `poll_interval`/backoff, hysteresis,
config-pinned fixed-board kiosk, UI board create/archive, fuzzy
reconciliation growth incl. `--dry-run`, daemon-side per-connector
attach/detach IPC verbs, connector config via env, ADR 0010 drift
amendments F10-03/F10-04/F15-18, #157 perf work; explicitly *not*
promoted — no issues created); **Bookkeeping** (#125 closes via the
PR body on merge; the wave's other closures already landed with
their chunks: #298/#187/#170/#179).

## 7. Verification method (stage 2 executes; the report restates it)

1. **Re-locate**: resolve every §2 path/symbol against the branch's
   post-rebase base (`git fetch origin --prune`; merge-base with
   `origin/main`); line numbers are this plan's — the report cites
   paths + test names only (stable keys).
2. **Re-run the hermetic set** (worktree, named filters):
   `cargo nextest run -p taskboard-storage-sqlite migration_0006`;
   `cargo nextest run -p taskboard-state --test snapshots`;
   `cargo nextest run -p taskboard-app -E 'test(two_connectors_live_simultaneously) | test(the_authority_counts) | test(registry_loop_wires) | test(two_connectors_emit_only) | test(single_connector_passes) | test(v0_1_x_additive_upgrade_boot_settles)'`;
   `cargo nextest run -p taskboard-ui-slint --test journeys board_switching`;
   `cargo nextest run -p taskboard-sync-nextcloud --test actor_wiremock a29 a30 a31 a32`;
   `cargo deny check`. Record counts.
3. **Tier-2 attempt, once, honestly** (MD-5): if docker is present,
   `eval "$(scripts/nextcloud_it_setup.sh up)"` then
   `cargo nextest run -p taskboard-app --run-ignored only -E 'test(it_nextcloud_docker_two_connectors_live) | test(it_nextcloud_docker_move_board) | test(it_nextcloud_docker_v0_1_x)' --no-tests=fail`;
   record pass/fail either way; if the #375 host-kill reproduces,
   record it as the known runner limitation (forensics already on
   file) — a reproduction is a record, not a finding against the
   wave. If docker is absent, record the absence.
4. **Re-read**: `gh pr view <N> --json body` for #358/#362/#364/#368/
   #375/#376/#377 (the MD-5 records §2 quotes); the CHANGELOG
   per-chunk commits (`git show <sha> -- CHANGELOG.md`) for the
   lossless diff.
5. **Gates**: the full AGENTS §10 suite on the docs-only diff
   (pre-commit hooks and CI run regardless).

## 8. Settled decisions (open questions resolved)

- **D1 — CHANGELOG promotion semantics**: no release cut; entries
  stay under `[Unreleased]`; grouping = attribution normalization to
  "(multi-board wave)" + adjacency of the wave's entries, every
  factual clause preserved, nothing invented, non-wave entries
  untouched (§5).
- **D2 — tier-2 verification by this chunk**: re-run attempts are
  in scope (§7.3, once, honest record); **autonomous CI dispatch is
  not** — stage 2 does not `gh workflow run nextcloud-integration`
  (the dispatch prompt's method is re-run/re-read, and a ~30 min
  externally-visible CI run is the user's call). The report's method
  section instead records that the workflow is dispatchable
  (`gh workflow run nextcloud-integration.yml`) and that its next
  weekly run (Wednesdays 04:00 UTC; last ran 2026-10-14, pre-wave)
  exercises the three tier-2 tests on the stable runner.
- **D3 — exit report filename date**: `<stage2-date>` = the day
  stage 2 writes it (this plan's outline refers to it symbolically;
  2026-10-20 if stage 2 runs today).
- **D4 — CI-on-main failure**: the report cites the **Test
  Execution** job (green at `419c70f`) as CI evidence; the
  Performance Benchmarking job's `main` failures are a pre-existing
  environment observation — one Findings line, no tracker action
  (creating an issue is tracker work outside this chunk's bookkeeping
  map).
- **D5 — snapshot corpus placement**: cited under the upgrade row as
  chunk 1's widening pin (the impl plan's map names "round-trip" for
  chunk 1; the corpus is the same chunk's deliberate insta pass 1,
  DF-9 — evidence of the shape the upgrade preserves).
- **D6 — non-wave drift**: wave-drift in the two docs is fixed;
  anything else stale spotted in them stays out of the diff and lands
  in the report's Findings (the PR diff is exactly the two doc
  amendments + CHANGELOG + exit report + this plan, dispatch §1
  hygiene).
- **D7 — N=2 verdict wording**: the row's verdict reads
  "met — hermetically verified; the dockerized run is blocked by a
  recorded runner limitation (not a product defect), corroborated
  live by chunk 6's two-connector journey" — mirroring the basis on
  which #187 closed, never overstating tier-2 evidence.
- **D8 — line-number policy**: this plan's line references are
  plan-day; the deliverables cite paths + test names only.

## 9. Quality gates (AGENTS §10, verbatim sequence)

Docs-only diff, but the full suite runs regardless: `cargo fmt --all
-- --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo nextest run --workspace`; `cargo test --doc --workspace`;
`cargo deny check`; the CI-identical three-command llvm-cov sequence
(+ quarantine, + `rm -rf target/llvm-cov-target` local hygiene).
Stage 3 then re-runs the suite, pushes, opens the PR (body carries
`Closes #125` and links the exit report), watches checks to green +
`mergeStateStatus: CLEAN`, and removes the worktree.

## 10. Risks

- **Tier-2 non-determinism on this runner** (#375's docker-host stop,
  six attempts): mitigated by the hermetic closes (§2) and D7's
  wording; stage 2 records any reproduction as a record, not a wave
  finding.
- **Prose-code drift**: the architecture.org communication table and
  crate roles carry pre-wave prose interleaved with stale bits;
  mitigated by the section-by-section list (§3) naming the symbol
  each claim must read true against, and stage 3's spot-check
  mandate.
- **CHANGELOG losslessness**: the wave's entries interleave with
  kiosk/perf entries in the same Added/Changed sections; mitigated by
  the per-chunk inventory (§5) and the entry-by-entry diff check at
  stages 2 and 3.
- **README accuracy**: flag/verb spellings drift easily (e.g.
  `--local-board` vs `--bind-local-board`); mitigated by §4.6's
  sweep against `cli.rs`.
- **Full gate suite cost on a docs-only diff** (~build + coverage
  tree): expected; the worktree is removed at stage-3 teardown per
  AGENTS §8 lifecycle.

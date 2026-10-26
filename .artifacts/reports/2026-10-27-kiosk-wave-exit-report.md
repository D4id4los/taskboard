# Kiosk Wave — Exit Verification Report

- **Date**: 2026-10-27 (stage-2 execution day; base `b14d030` — the
  merge-base of `docs/kiosk-wave` against `origin/main`, =
  `origin/main`'s tip at verification time, re-fetched and confirmed
  2026-10-27).
- **Kind**: the wave exit verification record for the kiosk
  implementation round (implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md` §5's
  exit-criteria table; chunk plan
  `.artifacts/plans/2026-10-26-kiosk-wave-exit-docs-plan.md`).
  One section per exit-criterion row: the criterion verbatim, the
  verifying chunks, the named evidence (paths + test names only —
  D12; line numbers are never load-bearing), the verdict.
- **Verdict vocabulary** (chunk plan D8): *met* / *met — with
  recorded evidence limitation* / *not met*.
- **The wave in one line**: fourteen chunks of plan → twelve landed
  code/docs rounds + the user spike (phase B) + this exit — the
  appliance, the instrumentation, the deployment profile, the web
  dashboard, the file credential store, the sync/engine metrics, the
  ambient view; the on-device budget verdicts recorded; the milestone
  empty (0 open / 13 closed, verified live 2026-10-27, §Bookkeeping).

## The landed wave — item → evidence → tracker map

PR numbers re-derived 2026-10-27 via `gh pr list --state merged` (the
commit hashes are the implementation plan's plan-day anchors, kept
for traceability):

| Wave item | Evidence landed | PR (merged UTC) | Tracker |
|---|---|---|---|
| K1 `taskboard gui` verb | `crates/taskboard-app/src/cli.rs` `Command::Gui` | #365 (10-19) | #237 closed |
| K2 kiosk window profile flag | `UiSettings.kiosk` (`crates/taskboard-ui-slint/src/lib.rs`) | #366 (10-19) | #237 closed |
| K3 appliance lifecycle + ADR 0016 | `crates/taskboard-app/src/ui.rs` `kiosk_start`; `docs/adr/0016-kiosk-appliance-mode.org` | #369 (10-19) | #237 closed |
| K4 `.desktop` + launch docs | `contrib/taskboard.desktop`; README "Launching" | #370 (10-19) | #237 closed |
| K5 process smoke harnesses | `scripts/process_smoke.py` (S1 RSS / S3 cold-start modes) | #371 (10-19) | #299 #262 closed |
| K6 refresh+draw bench | `crates/taskboard-ui-slint/benches/refresh_draw.rs` | #372 (10-20) | #300 closed |
| K7 mapping bench | `crates/taskboard-sync-nextcloud/benches/` (the DTO↔domain criterion bench) | #374 (10-20) | #263 closed |
| K8 sync counters + engine metrics | `crates/taskboard-domain/src/state.rs` `SyncStatus.counters` + `aggregate()`; `crates/taskboard-app/src/commands/output.rs` counters + the `engine` object; `crates/taskboard-state/src/metrics.rs` `EngineMetrics` | #379 (10-20) | #156 #145 closed |
| K9 deployment profile, unit, runbook | `crates/taskboard-app/src/config.rs` `KioskSection` + `resolve_fixed_board`; `contrib/taskboard-kiosk.service`; `docs/kiosk_bringup_runbook.org` | #380 (10-21) | #233 closed |
| K10 dashboard design round + ruling | hyper + capability token + loopback default ruled in (root `Cargo.toml` pin comments) | #381 (10-21) | #234 closed |
| K11 web dashboard implementation | `crates/taskboard-app/src/web/{mod,routes,render,token}.rs`; `crates/taskboard-app/tests/web_dashboard.rs` | #383 + #385 (10-21) | #234 closed |
| K12 encrypted-file credential store | `crates/taskboard-app/src/secrets.rs` (the third `CredentialStore` impl); the chacha20poly1305 + getrandom pins | #382 (10-21) | #159 #126 closed |
| Phase-B spike, D250 leg | the netbook report (`.artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md`, PR #387): the four §7 slots + the i686 lane | #387 (10-21) | #233 closed with the verdict table |
| Spike-exit rulings | ADR 0016's 2026-10-21 amendment; runbook §1/§5.5/§8; the #160 decision comment | #388 (10-21) | #160 closed 2026-10-21 |
| K13 ambient view + rotation | `crates/taskboard-ui-slint/src/view_state.rs` `KioskMode`/`KioskRotation`; `[kiosk.rotation]` in `config.rs`; the journey legs + goldens | #516 (10-26 22:52) | #363 closed |
| K14 this chunk | `docs/kiosk-wave` | — | — (closes nothing) |

Chunks 1–13 have no separate stage-report files — their PR bodies
are the stage records (re-read via `gh pr view` during this
verification; #516's body carries the rotation design record incl.
the no-engine-command assertion mechanism).

---

## The D250 verdict table

The four runbook-§7 slots, verbatim from the netbook report's
"Measurement results" table (recorded 2026-10-21 on `aspire-net-srv`,
the D250 — quoted, never re-measured, per D9):

| # | Verdict | Budget | Measured | Result |
|---+---------+--------|----------+--------|
| 1 | RSS steady/peak | ≤ 200 MB | 49 712 kB steady / 50 160 kB peak (headless daemon, N=10 000 fixture, 13 samples `"steady": true`) | **within (~25 %)** |
| 2 | Full/partial repaint | ≤ 50 / ≤ 20 ms | K6 bench run **on-device** (i686 cross-built criterion binary, software renderer, 1366×768 frame): full 31.1 / 46.4 / 202.8 ms at N = 100 / 1 000 / 10 000; partial 110.8 / 659.0 / 6 141 ms | **full within at board-scale N ≤ 1 000 (marginal at 1 000); partial — see the two-stage row below** |
| 3 | Cold start | ≤ 5 s | 2 106.7 ms spawn → first-frame, X11, N=10 000 fixture | **within (~42 %)** |
| 4 | Sync-cycle CPU share | recorded | on-device `last_cycle_ms` = **685 ms**/cycle (Wi-Fi-LAN http to the dev-side Nextcloud; 30 s poll; cycles clean: pulls 5/cycle, failures 0) | recorded |

**The partial-repaint row — the two-stage story** (chunk plan D2;
neither the stale "pending #390" nor an unqualified pass):

1. **The spike-day record (2026-10-21)**: partial failed as measured
   at every scale — but the measurement was dominated by the D6 dirt
   (the partial region computed ≈ 97 % of the frame:
   whole-`VecModel`-replacement dirt, so "partial" was a full redraw
   plus dirty-region overhead). The recorded verdict was "bounded
   above, true partial unmeasured"; the spike-day numbers bound it
   from above.
2. **The post-#390 record**: #390 (the incremental model push /
   dirt-tracking follow-up) **closed 2026-10-26** — the
   incremental-push fix direction landed via the #373 ListView
   virtualisation round, and the re-measure reports record the
   dev-machine true-partial legs:
   `draw-partial/today` 4.81 / 4.87 / 5.75 ms and
   `draw-partial/board` 2.99 / 4.78 / 1.76 ms at N = 100 / 1 000 /
   10 000 (`.artifacts/reports/2026-10-26-listview-virtualisation-stage2-report.md`
   §4, and `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`)
   — within the ≤ 20 ms budget and N-independent (the viewport-scale
   scaling class), with `draw-full/board/10 000` now 2.78 ms where
   the eager board crashed (#462).

**Verdict row for partial**: *dev-machine true-partial within budget
after the #390/#373 incremental-push change (N-independent,
viewport-scale); the on-device partial verdict remains
bounded-by-the-spike-day measurement until the recorded on-device
re-measure runs* — the #373 on-device re-measure leg is the
outstanding user-executable follow-up (runbook §7's method).

Weighing the extremes exactly as the 2026-10-21 assessment
prescribes: the wall shows one board, so 10 000 tasks/board is the
extreme fixture — the full-frame 202.8 ms on-device (and the 60.5 ms
dev-machine reference) is the extreme's number, not the board-scale
budget's; at realistic board scale (N ≤ ~1 000) the ≤ 50 ms full
budget held on the D250. After #373 the extreme collapsed (2.78 ms
dev-machine full-board draw at N = 10 000), making the old extreme
row historical.

## The ruled-decisions ledger

One row per decision, each with its landing place:

| Decision | Ruling | Landing place |
|---|---|---|
| Renderer posture | The measured permission-based software-GL fallback is sanctioned: the appliance user stays out of `video`/`render`, libGL falls back to llvmpipe — the configuration the budget verdicts were measured under. No `LIBGL_ALWAYS_SOFTWARE` pin, no build flip. | ADR 0016's 2026-10-21 amendment; runbook §1/§8 |
| i686 release target | A documented on-demand lane, no release-matrix leg. | Runbook §5.5 (PR #388) |
| sd_notify / #160 | Not needed under `Type=simple` + `Restart=always`, with the D250 evidence recorded. | #160's decision comment; issue closed 2026-10-21 |
| Dashboard stack/auth/bind | hyper 1.x server + capability-token URL auth + loopback default; a non-loopback bind requires a resolvable token. | The K10 design round (PR #381); shipped as `crates/taskboard-app/src/web/` + `WebSection` (PR #383) |
| File store key management | D-K8 option (a): the generated 0600 key file + ChaCha20-Poly1305 AEAD envelope. | The framing plan §4 record; shipped as `secrets.rs` (PR #382), ADR 0008 amendment |
| D-K1…D-K10 (the wave framing) | All ACCEPTED 2026-10-19. | The framing plan's §4 record |
| D-V3 kiosk rulings | Board-pinned, compact, passive, no capture — landed as the ambient view's actual gates (the pin matrix, the passive journeys, no capture affordance). | The views-finding design; shipped in PR #516 |

## The Q6 input (recorded, not decided)

**Q6 — the post-wave default `[app] mode` — is the user's pending
ruling; the wave decides nothing.** The recorded input, for that
ruling:

- The appliance is config-file-driven: `mode = "kiosk"` lives in the
  unit's config file (`/etc/taskboard/taskboard.toml`), never an
  env-var launch — the D250 ran exactly this way (ADR 0015's #236
  rule: the unit imports no mode authority; `contrib/taskboard-kiosk.service`
  states the config file as the only mode authority).
- Display-aware bare routing already covers the double-click pain the
  2026-10-16 deferral named: a `daemon`-mode launch in a graphical
  session boots the GUI (#236).
- `ssh -X` sessions carry `DISPLAY` — the concrete argument for
  keeping the daemon default: a maintenance session's relayed launch
  would otherwise route by display presence.
- The D250 ran as a dedicated systemd-driven appliance where the
  default mode never decides anything (the config file does).
- The three options remain as the decision briefing §3 framed them
  (keep `daemon` + current routing / default `desktop` / default
  `kiosk`), with the briefing's stated expectation being "keep daemon
  + current routing".

A ruling here is a small follow-up chunk (a config default + doc
line), not milestone work; nothing in this wave lands it.

## The follow-up ledger (by reference — no issue edited)

Each entry's state as read live on the tracker 2026-10-27:

- **#390** (true-partial repaint verdict / incremental push) —
  **closed** 2026-10-26 10:54 UTC (closed by `9f05d67`, the
  view-pinned re-measure bench commit; the fix landed via the #373
  ListView virtualisation round). Residuals live elsewhere: the
  on-device re-measure (below) and the column re-solve region
  bimodality finding recorded in the ListView stage-2 report.
- **#373's on-device re-measure leg** — outstanding; user-executable
  per runbook §7's method (the virtualised views have never been
  measured on the D250).
- **#462** — **open**: the software renderer's i16 pixel-ceiling
  panic (the eager board's shape; the ListView renders past it by
  construction — the renderer-arithmetic fix remains).
- **#463 / #464** — **open**: the detect-side refresh/diff
  follow-ups.
- **#386** (the `connect` 0600-rewrite bug) — **closed** 2026-10-25
  (fixed by `7fe6f0e`; the dispatch plan had listed it open).
- **The optional NF310 measurement leg** — user-executable (runbook
  §7, binary leg A); the milestone's exit criteria are satisfied by
  the recorded D250 verdicts without it.

---

## Exit-criterion rows

### 1. "`mode = "kiosk"` boots the appliance: display-gated window, loud headless fallback, close-exits, `daemon stop` over ssh, `status` serves"

**Verdict: met** (K3 + K1/K2) — hermetic:

- `crates/taskboard-app/src/ui.rs` :: `kiosk_start` (the display
  gate before any slint call; `KioskStart::{Appliance, Headless}`)
  and the unit `kiosk_start_splits_the_display_gate`.
- The appliance watcher + the shared tail:
  `crates/taskboard-app/src/main.rs` :: `serve_and_shutdown` /
  `drain_and_shutdown` (close, signals, and `daemon stop` converge
  on one graceful shutdown).
- ADR 0016 (the routing table + the Option-B fence).
- The routing tests in `crates/taskboard-app/tests/` (the
  bare-invocation table's cells); green in this session's
  taskboard-app run.

### 2. "`taskboard gui` + `.desktop` + launch-docs table complete the double-click story"

**Verdict: met** (K1, K4):

- `crates/taskboard-app/src/cli.rs` :: `Command::Gui` (+ the
  relay-refusal tests in the same file's test module).
- `contrib/taskboard.desktop`; `scripts/package_release.py` staging.
- README "Launching" — the bare-invocation table with the `gui` row
  and the `.desktop` paragraph.

### 3. "RSS, cold-start, refresh+draw, sync-CPU instrumentation exist with recorded dev-machine numbers (or the S2 evidence outcome)"

**Verdict: met — with recorded evidence limitation** (K5, K6, K7;
the limitation is the honest tier record: dev-machine numbers are
references, the on-device verdicts are the spike's, recorded above):

- `scripts/process_smoke.py` (the rss / cold-start modes).
- `crates/taskboard-ui-slint/benches/refresh_draw.rs` (the
  refresh/draw lanes; the headless software draw leg).
- The sync-crate mapping bench (`crates/taskboard-sync-nextcloud/benches/`).
- The netbook report's dev-reference tables (§"Dev-machine criterion
  references") and the on-device slot table (quoted above).

### 4. "Sync/engine metrics readable lock-free; `status` renders counters structurally"

**Verdict: met** (K8) — hermetic:

- `crates/taskboard-domain/src/state.rs` :: `SyncStatus.counters` +
  `aggregate()` (+ the `aggregate_rows` fold/property tests).
- `crates/taskboard-app/src/commands/output.rs` :: `cycles` /
  `offline_episodes` / `last_cycle_ms` + the `engine` object.
- Handle-side: `crates/taskboard-state/src/metrics.rs` ::
  `EngineMetrics` (atomics off the engine handle, never in
  `AppState`).
- The state-crate insta/collection tests and the app
  status-rendering units — green in this session's workspace run.

### 5. "Kiosk deployment: `[kiosk] fixed_board` typed, system unit, runbook"

**Verdict: met** (K9) — hermetic (109/109 in the `-p taskboard-app
config` filter this session):

- `crates/taskboard-app/src/config.rs` :: `KioskSection`,
  `resolve_fixed_board` (the four-cell pin matrix doc), and the
  typed-error tests (`resolve_fixed_board_refuses_an_unpinned_multi_board_boot`,
  `resolve_fixed_board_returns_none_without_a_pin`,
  `resolve_fixed_board_resolves_a_live_board`).
- `contrib/taskboard-kiosk.service`; `docs/kiosk_bringup_runbook.org`.

### 6. "On-device budget verdicts (RSS ≤ 200 MB; repaint ≤ 50/20 ms; cold start ≤ 5 s; sync CPU share) + renderer/backend decision + #160 ruling + Q6 input"

**Verdict: met — with recorded evidence limitation** (phase B user
spike, recorded here in K14): the four slots are quoted above; the
renderer decision and the #160 ruling sit in the ruled-decisions
ledger; the Q6 input is recorded-not-decided. The limitation is the
partial-repaint leg's two-stage story (spike-day bounded-above
measurement; the on-device re-measure of the virtualised views
outstanding).

### 7. "Dashboard serves boring HTML under the ruled auth/bind model"

**Verdict: met** (K10 + K11) — hermetic (5/5 this session):

- `crates/taskboard-app/src/web/routes.rs` / `render.rs` /
  `token.rs`; `config.rs` :: `WebSection`.
- `crates/taskboard-app/tests/web_dashboard.rs` ::
  `w1_the_dashboard_journey_over_the_real_server` +
  `w2_a_generated_token_never_secures_a_lan_bind` (the LAN-bind
  refusal typed).

### 8. "Headless credential story complete: file store, typed failures, no silent fallback"

**Verdict: met** (K12) — hermetic (9/9 in the `-p taskboard-app
secrets` filter this session):

- `crates/taskboard-app/src/secrets.rs` (the third `CredentialStore`
  impl; the corruption-shape fence tests in its `mod tests`).
- `config.rs` :: `credential_store = "file"`; the ADR 0008
  amendment; the D250 ran its live sync cycle on exactly this store
  (the netbook report's slot-4 setup).

### 9. "Ambient view rotates board-pinned, passive, timer-driven"

**Verdict: met** (K13) — hermetic this session (journeys 3/3,
goldens 2/2, view_state 68/68):

- `crates/taskboard-ui-slint/src/view_state.rs` :: `KioskMode` /
  `KioskRotation` (`tick`, the declared-order wrap, the typed mint
  errors `KioskRotationError::{TooFewModes, DuplicateModes}`).
- The adapter-owned dwell timer (`crates/taskboard-ui-slint/src/lib.rs`).
- `crates/taskboard-app/src/config.rs` :: `RotationSection` /
  `KioskRotationMode` / `KIOSK_ROTATION_MIN_DWELL_SECS` +
  `KioskPinRequired` (the pin matrix doc on `resolve_fixed_board`).
- The journey legs: `crates/taskboard-ui-slint/tests/journeys.rs` ::
  `the_kiosk_mount_still_lands_on_the_board` /
  `the_kiosk_rotation_flips_modes_on_the_dwell_timer` /
  `the_kiosk_today_mode_renders_only_the_pinned_board` — mock-clock
  (no sleeps); the passive gates pinned (inbox chip reads zero, the
  notification surface stays empty, the banner-only surface model);
  the banner + sync-phase carry-over asserted across flips.
- The goldens: `crates/taskboard-ui-slint/tests/golden_visuals.rs` ::
  `the_kiosk_board_golden_holds` / `the_kiosk_today_golden_holds`
  (`tests/goldens/kiosk-{board,today}-0.5-1000.png`).
- The no-engine-command rule (KDF-11) holds two ways, per PR #516's
  stage record: a unit sweep over all `Flow` variants × modes
  asserting no `FlowOutcome::Command`, and the rotation journey's
  `harness.engine.drain()` asserted empty after multiple dwells.
- The desktop paths are byte-identical when the rotation table is
  absent (the `kiosk_rotation: None` hand-over arms nothing — a
  type-level guarantee, unit-pinned in `lib.rs`'s default-settings
  test).

### 10. "Docs/CHANGELOG promoted; exit report; milestone closure review"

**Verdict: met** — by this chunk's PR itself: the amended
`docs/architecture.org` (the kiosk appliance subsection + the
per-crate-role/tech-stack amendments), the amended `README.org`
(rotation keys, the web dashboard section), the promoted
`CHANGELOG.md` (the wave's entries as one run, attributions
normalized, the missing `[web]` entry added; losslessness diff-proven:
13 deletions = 10 relocated rotation-entry lines + 3 attribution
rewrites, 25 insertions = the same lines back + the new entry), and
this report. No `Closes` line — this chunk closes nothing.

---

## Method (what stage 2 actually ran, 2026-10-27)

1. **Re-locate**: `git fetch origin --prune`; base `b14d030`
   confirmed = `origin/main` tip. Every §2.3 path + symbol re-located
   by grep before writing (the plan's names were plan-day
   references); the per-item PR numbers re-derived via
   `gh pr list --state merged`.
2. **Hermetic re-run** (worktree `../taskboard-WT-docs-kiosk-wave`):
   `cargo nextest run -p taskboard-app config` (109/109, 490 skipped
   — the docker-guarded set),
   `-p taskboard-ui-slint --test journeys the_kiosk` (3/3),
   `-p taskboard-ui-slint --test golden_visuals kiosk` (2/2),
   `-p taskboard-ui-slint --lib view_state` (68/68),
   `-p taskboard-app secrets` (9/9),
   `-p taskboard-app --test web_dashboard` (5/5) — all green.
3. **No bench re-runs, no on-device legs** (D9): the recorded
   spike-day and post-#373 numbers are the evidence; this report
   cites, never re-measures.
4. **Re-read**: `gh pr view 516` (chunk 1's stage record + the
   no-engine-command assertion's form), #388, and the closed issues
   #363/#160/#233/#390/#386 (state + closing record only); `gh api`
   the milestone (§Bookkeeping).
5. **Semantic-fidelity pass**: the rotation CHANGELOG entry re-read
   against `config.rs::resolve_fixed_board` + its matrix doc
   (accurate: the landed rule is the typed `kiosk-pin-required`
   refusal); every new README/architecture claim checked against the
   named symbol (`WebSection`'s real field names, `DEFAULT_WEB_BIND`
   = `127.0.0.1:8787`, `WEB_REFRESH_MIN_SECS` = 5, the token
   resolution order in `web/token.rs`, the pin-comment rationales in
   the root `Cargo.toml`).
6. **Gates**: the full AGENTS §10 suite on the docs-only diff (run
   2026-10-27, green; see the PR's checks).

## Findings

- **F1 — the stale "verdict pending #390" premise**: the dispatch
  plan §0 and the 2026-10-21 report both record the partial budget
  as pending #390; #390 closed 2026-10-26 and the incremental push
  landed via #373's round. The verdict table tells the two-stage
  story (chunk plan D2) — neither stale-pending nor an unqualified
  pass. Flagged here so no future reader re-derives it.
- **F2 — the GUI-r2 CHANGELOG entries cite `#390` for interaction
  repairs** (the `#390` in the round-2 entries for the settings
  surface, the interaction repairs, etc.): #390 is the
  dirt-tracking/partial-verdict issue, not the GUI-review-round-2
  issue — the citations point at the wrong issue number. Non-wave
  lines, untouched in this diff (D14); worth a one-line correction
  in a future docs pass.
- **F3 — the daily-driver exit report's F5 README staleness
  ("Until the first release is cut") is since fixed**: no hit at
  this base (the 2026-10-26 audit-drift pass, PR #452, is the likely
  fixer). Recorded so the precedent's finding reads resolved.
- **F4 — `RotationSection`'s doc comment says "exactly two distinct
  entries" while the validation rule is "≥ 2 distinct"**: with the
  enum at exactly two variants the two phrasings coincide today; if
  a third `KioskRotationMode` ever lands, the doc comment, not the
  validator, is what needs the update. Cosmetic; untouched (no code
  changes in this chunk).

## Backlog candidates (restated, **not promoted** — no issues created)

- The multi-board rotation ring (D-V3's explicit backlog — the pin
  is one board by ruled design).
- Brightness/DPMS control for the ambient display.
- The F16-28 attachable-GUI fence standing (ADR 0016's Option B,
  unfired by design).
- The #462 renderer-arithmetic fix (the i16 pixel ceiling).
- The column re-solve region bimodality finding (the ListView
  stage-2 report's residual).
- The Q6 ruling chunk (a config default + a doc line, after the user
  rules).
- The optional NF310 measurement leg (user, runbook §7).
- #373's on-device re-measure leg (user, runbook §7).

## Bookkeeping — the milestone closure review

Verification commands with their observed outputs (run live
2026-10-27):

```
$ gh api repos/D4id4los/taskboard/milestones --jq \
    '.[] | select(.title=="wave-kiosk") | {open_issues, closed_issues}'
{"closed_issues":13,"open_issues":0}

$ gh issue list --milestone "wave-kiosk" --state open
(empty)

$ gh issue view 363 --json state,closedAt
{"closedAt":"2026-10-26T13:10:14Z","state":"CLOSED"}

$ gh pr view 516 --json state,mergedAt
{"mergedAt":"2026-10-26T22:52:42Z","state":"MERGED"}
```

Every roster issue is closed (#237, #299, #300, #262, #263, #156,
#145, #233, #160, #234, #159, #126, #363); the milestone reads
**0 open / 13 closed** — the emptiness is mechanical fact, achieved
by chunk 1's merge (#516 → `Closes #363`, the last open issue).
Per AGENTS §7, closing is the archive: nothing deleted, nothing
reopened. The milestone's formal closure is the user's act. This
chunk made no tracker edits of any kind — no comments, no closes,
no label or milestone edits, no new issues, and its PR body carries
no `Closes` line.

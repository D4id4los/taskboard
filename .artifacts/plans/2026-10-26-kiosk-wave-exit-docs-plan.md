# [Plan] Kiosk Chunk K14 — Docs Promotion + Wave Exit Verification

- **Date**: 2026-10-26
- **Kind**: stage-1 work plan (dispatch plan §3, prompt 2.1) for
  chunk 2 of the kiosk closure round — the `wave-kiosk` milestone's
  docs promotion + the exit verification record. No code, no tracker
  edits (MD-2: this chunk posts no comments, closes nothing, its PR
  body carries no `Closes` line).
- **Branch / worktree**: `docs/kiosk-wave` at
  `../taskboard-WT-docs-kiosk-wave`, created off `main` =
  `origin/main` = `b14d030` (chunk K13's changelog commit — the
  current tip); `scripts/worktree-doctor.sh` exit 0 before creation
  (73 G free, floor 25 G; one unrelated in-flight worktree kept,
  none removed).
- **Precondition MD-1 verified (2026-10-26)**: chunk 1
  (`feat/ui-kiosk-ambient-view`) is merged on `origin/main`. The
  remote branch ref was deleted after the rebase-merge, so
  containment was verified through the authoritative records (the
  daily-driver exit plan's MD-9 method): `gh pr list --state merged`
  shows **PR #516** "feat(ui): kiosk ambient view — display-mode
  rotation (K13)" MERGED 2026-10-26 22:52 UTC; **#363** CLOSED
  (state `CLOSED`); chunk-1 artefacts spot-checked present on
  `origin/main` (`config.rs` `RotationSection`/`KioskRotationMode`/
  `KioskPinRequired`, `view_state.rs` `KioskMode`/`KioskRotation`,
  `tests/goldens/kiosk-{board,today}-0.5-1000.png`, the K13 journey
  legs in `tests/journeys.rs`). Milestone `wave-kiosk`: **0 open /
  13 closed** — the milestone is already empty; K14 records the
  verification, it does not perform it.
- **Normative sources** (dispatch MD-4 rank): the wave implementation
  plan `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 KDF-1…KDF-12 as amended by landed reality, §3 K14, §5 the
  exit-criteria table, §6 bookkeeping); the views-finding design's
  D-V1/D-V3 kiosk rulings; the framing's §4 ACCEPTED record
  (D-K1…D-K10, 2026-10-19); the 2026-10-21 assessment
  `.artifacts/reports/2026-10-21-kiosk-milestone-remaining-steps-report.md`
  (steps 1–5, the spike verdicts, the follow-up set); the spike-exit
  ruling records (PR #388 lineage: the ADR 0016 amendment of
  2026-10-21, runbook §1/§5.5/§8, the #160 decision comment); the
  decision briefing §3 (the five spike-ruled items); the netbook
  report `.artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md`
  (the four §7 measurement slots); this dispatch plan §0/§3. **No
  deviation from any ACCEPTED ruling is needed or permitted here** —
  this chunk writes documentation about what landed. One
  **landed-reality amendment** is recorded (§2.4/D2) in the dispatch
  plan's own MD-4 style: the follow-up-ledger and partial-verdict
  facts moved after the dispatch plan was written; this plan
  re-anchors them on today's tree and tracker.
- **Line numbers below are plan-day references** (taken at
  `b14d030`, 2026-10-26); paths, symbols, and test names are the
  stable keys. The exit report cites **paths + test names only**
  (the precedent's D9).
- **Precedent followed**: the daily-driver wave's exit chunk
  (`.artifacts/plans/2026-10-25-daily-driver-wave-exit-docs-plan.md`
  → `.artifacts/reports/2026-10-25-daily-driver-wave-exit-report.md`)
  is the house shape for this chunk; where this plan is silent on
  method, that precedent's settled decisions apply (its
  D1/D4/D7/D9/D10/D11 analogues are re-recorded here as
  D7/D8/D14/D12/D13/D2's neighbours).

## 1. Scope

**In** (dispatch §3 "Covers", whole):

1. `docs/architecture.org` — the shipped kiosk subsystem set made
   true: the Executive Summary's "the passive ambient view remains
   the kiosk wave's later ambition" becomes shipped-reality; a new
   kiosk-subsystem subsection; the per-crate-role amendments
   (§3 below).
2. `README.org` — the `[kiosk]` rotation keys + the ambient-rotation
   behaviour in the deploying-a-kiosk section, the `[web]` dashboard
   config + usage section (both verified absent today, §2.2), the
   accuracy sweep (§4).
3. `CHANGELOG.md` — promotion of the kiosk wave's per-chunk
   Unreleased lines into the wave's release-note shape, losslessly,
   **plus the one verified missing entry**: the `[web]` dashboard has
   no CHANGELOG line anywhere (§2.2/§5 — added, evidence-verified
   against the tree).
4. The exit verification record at
   `.artifacts/reports/<stage2-date>-kiosk-wave-exit-report.md` —
   the D250 verdict table, the ruled-decisions ledger, the Q6 input
   recorded-not-decided, the follow-up ledger by reference, the
   exit-criteria walk with evidence pointers (§6).
5. The milestone closure review — the verification commands and
   their observed outputs, recorded in the report's Bookkeeping
   section (§6.10).

**Out** (dispatch §3 + implementation plan §3 K14 / §6, MD-2):

- No code — any `.rs`/`.toml`/`.slint`/`.sql` diff is a finding.
- No release cut: entries stay under `[Unreleased]`;
  `bump_version.sh` (+ `changelog.py promote`) is the user's flow
  (AGENTS §9). "Promotion" here is the precedent's sense: per-chunk
  lines consolidated into wave-shaped entries with normalized
  attributions, adjacency restored, nothing invented.
- No tracker edits of any kind: no comments, no closes, no label or
  milestone edits, no new issues, no `Closes` lines in the PR body.
  The milestone's actual closure is the user's act; the emptiness is
  verified and recorded (MD-2).
- No Q6 implementation and no Q6 recommendation-as-decision: the
  input is recorded, the ruling is the user's (D10).
- No NF310 execution, no on-device re-measure: user legs, recorded
  in the follow-up ledger by reference (D3/D9).
- No bench re-runs for "fresh" numbers: the recorded spike-day
  numbers are the evidence; a re-run would produce new dev-machine
  numbers that are not the spike's verdicts (D9).
- Non-wave doc drift observed while working (e.g. the CHANGELOG's
  GUI-r2 entries citing `#390` for interaction repairs — that issue
  is the dirt-tracking issue; the README's "Until the first release
  is cut" line if still present) is recorded in the report's
  Findings, never fixed in this diff (D14) — the PR diff is exactly
  this chunk's artefacts.
- No backlog promotion: out-of-scope ideas are restated in the
  report's backlog-candidates section for the user.

## 2. Re-verified evidence inventory (2026-10-26, base `b14d030`)

### 2.1 The landed wave — item → evidence → tracker map

Re-anchored from the 2026-10-21 assessment's table (its commits and
closures re-verified live where the tracker can speak) plus the
post-assessment landings:

| Wave item | Evidence landed | Tracker |
|---|---|---|
| K1–K4 appliance spine (gui verb, window profile, lifecycle + ADR 0016, `.desktop` + launch docs) | merged (phase-A round) | #237 closed |
| K5/K7 harnesses + benches (RSS smoke, cold-start stopwatch, mapping bench) | `scripts/process_smoke.py`, the sync-crate bench; dev-machine numbers recorded | #299 #262 #263 closed |
| K6 refresh+draw bench | the ui-slint bench; headless draw works; the D6 dirt finding recorded | #300 closed |
| K8 sync counters + engine metrics | `25122c2`, `84dce32`; `output.rs` counters + the `engine` object | #156 #145 closed |
| K9 deployment profile, kiosk unit, bring-up runbook | `config.rs` `KioskSection`, `contrib/taskboard-kiosk.service`, `docs/kiosk_bringup_runbook.org` (`b8bb610`, `6a7a5ca`, `e2bc3f7`) | #233 closed (on the spike evidence) |
| K10 dashboard design round + ruling | hyper + capability token + reconnect script ruled in (`703ca61`, `46f5a43`, `47398b3`) | #234 closed |
| K11 web dashboard implementation | `src/web/{mod,routes,render,token}.rs`, `WebSection`, `tests/web_dashboard.rs` (`88e99bb`…`9eebbb7`) | #234 closed |
| K12 encrypted-file credential store | chacha20poly1305 + getrandom pins, the file store, ADR 0008 amendment (`9de114a`…`ed56cc9`) | #159 #126 closed |
| Phase-B spike, D250 leg | environment + i686 lane + the four §7 slots (`241402a`, PR #387) | #233 closed with the verdict table |
| Spike-exit rulings | ADR 0016 amendment + runbook §1/§5.5/§8 (PR #388); #160 decision comment | #160 closed 2026-10-21 |
| K13 ambient view + rotation | PR #516 merged 2026-10-26 (`a126f3d`…`b14d030`): `[kiosk.rotation]`, `KioskMode`/`KioskRotation`, the dwell timer, the journey legs, the goldens | #363 closed |
| K14 this chunk | `docs/kiosk-wave` | — (closes nothing) |

Stage 2 re-derives the per-item PR numbers via
`gh pr list --state merged --search kiosk` (the PR bodies are the
chunks' stage records where no stage-report file exists; the
commit-hash columns above are the plan-day anchors).

### 2.2 Current doc state (what stage 2 amends)

- `docs/architecture.org` (626 lines): post-daily-driver throughout.
  Wave gaps: the Executive Summary's mode paragraph ends "the
  passive ambient view remains the kiosk wave's later ambition"
  (:23-24 — the one "kiosk planned" sentence, now false); crate
  role 5 knows `ViewId::Today` + "kiosk keeps Board" but nothing of
  the kiosk display-mode state machine (`KioskMode`/`KioskRotation`,
  the adapter-owned dwell timer, the passive gates, the kiosk
  goldens); crate role 6's config inventory lists `[reminders]` /
  `[preferences]` / `[ui]` but neither `[kiosk]` (fixed_board +
  rotation) nor `[web]`; its `secrets` sentence says "the keyring
  and env stores" (the file store is the shipped third impl); its
  `status` sentence lacks the sync counters + the `engine` object;
  the module map lacks the `web` server; the tech-stack table lacks
  hyper/hyper-util and chacha20poly1305/getrandom; no kiosk
  subsystem narrative exists (the sidecar subsection is the
  precedent's shape).
- `README.org` (634 lines): already carries `credential_store =
  "file"` (:138-141, :539-544), the launching table's kiosk row
  (:321-330), and a "Deploying a kiosk" section (:366-381: the unit,
  the runbook, `[kiosk] fixed_board`). Wave gaps: no
  `[kiosk.rotation]` keys, no ambient-rotation behaviour sentence
  (passive display, the two modes, the dwell, no input, no capture
  affordance, `kiosk-pin-required`); no `[web]` config table and no
  dashboard usage section (what it shows, loopback default, the
  capability-token ceremony for a wider bind, meta-refresh).
- `CHANGELOG.md` (1194 lines; `[Unreleased]` = :10-685): the kiosk
  wave's landed entries verified present — the credential store,
  `[kiosk] fixed_board`, the kiosk unit, the runbook, the per-board
  sync counters, the engine counters (:238-273), the S2 bench
  (:356), the process smoke harnesses (:367), the `.desktop`
  launcher (:382), the README launching table (:387), the kiosk
  appliance (:391), the window-profile flag (:397), `taskboard gui`
  (:402), and K13's ambient-rotation entry (:25, `b14d030` — modes
  + `dwell_secs` floor 10 s, typed errors, `kiosk-pin-required`).
  **Verified missing: the `[web]` dashboard has no entry anywhere**
  (grep over the whole file for `[web]`/dashboard/http-server
  terms: zero hits outside unrelated lines) — K11 landed without its
  changelog line; this chunk adds it, written from the tree
  (`WebSection`'s enabled/bind/refresh surface, the token ceremony,
  the `w1` journey). Attributions on the kiosk lines carry internal
  chunk numbers ("kiosk wave K12", "kiosk wave K9") — normalized
  per the precedent (D7). The GUI-r2 entries citing `#390` for
  interaction repairs (:58, :219, :476, :501, :628) are **non-wave
  lines — untouched**; recorded as a Findings line (D14).

### 2.3 Named evidence (the exit report's pointers; all verified to exist at `b14d030`)

| Exit row (implementation plan §5) | Verifying chunk(s) | Named evidence (path + symbol/test) |
|---|---|---|
| `mode = "kiosk"` boots the appliance: display-gated window, loud headless fallback, close-exits, `daemon stop` over ssh, `status` serves | K3 (+ K1/K2) | `crates/taskboard-app/src/ui.rs` `kiosk_start` / the `KioskStart` decision + the appliance watcher; `main.rs` `serve_and_shutdown`/`drain_and_shutdown` (the shared tail); ADR 0016; the routing tests in `crates/taskboard-app/tests/dispatch_routing.rs` (stage 2 re-locates the exact kiosk-cell test names) |
| `taskboard gui` + `.desktop` + launch-docs table | K1, K4 | `cli.rs` `Command::Gui`; `contrib/taskboard.desktop`; `scripts/package_release.py` staging; README "Launching" (:321-330) |
| RSS, cold-start, refresh+draw, sync-CPU instrumentation with recorded dev-machine numbers | K5, K6, K7 | `scripts/process_smoke.py` (rss/cold-start modes); `crates/taskboard-ui-slint/benches/` (the refresh/draw bench — stage 2 re-locates the exact file name); the sync-crate mapping bench; the netbook report's dev-reference tables |
| Sync/engine metrics readable lock-free; `status` renders counters structurally | K8 | `taskboard-domain/src/state.rs` `SyncStatus.counters` + `aggregate()`; `output.rs` `cycles`/`offline_episodes`/`last_cycle_ms` (:154-169) + the `engine` object; the handle-side `EngineMetrics`; the state-crate insta/collection tests (stage 2 re-locates names) |
| Kiosk deployment: `[kiosk] fixed_board` typed, system unit, runbook | K9 | `config.rs` `KioskSection` (:449) + `resolve_fixed_board` (:996) + the pin-matrix doc comment (:1080); `contrib/taskboard-kiosk.service`; `docs/kiosk_bringup_runbook.org` |
| On-device budget verdicts + renderer/backend decision + #160 ruling + Q6 input | phase B (user), recorded in K14 | the netbook report's "Measurement results" table (the four slots); runbook §1/§5.5/§8; ADR 0016's 2026-10-21 amendment; the Q6 input section of the exit report (§6.4 here) |
| Dashboard serves boring HTML under the ruled auth/bind model | K10 + K11 | `src/web/routes.rs`/`render.rs`/`token.rs`; `config.rs` `WebSection` (:503); `crates/taskboard-app/tests/web_dashboard.rs` `w1_the_dashboard_journey_over_the_real_server` |
| Headless credential story: file store, typed failures, no silent fallback | K12 | `secrets.rs` (the third `CredentialStore` impl); `config.rs` `credential_store = "file"` (:153); the ADR 0008 amendment; the secrets round-trip tests (stage 2 re-locates names) |
| Ambient view rotates board-pinned, passive, timer-driven | K13 | `view_state.rs` `KioskMode`/`KioskRotation` (`tick`, the dwell, `KioskRotationError::{TooFewModes,DuplicateModes}`); `lib.rs` the adapter-owned dwell timer (:188, :232); `config.rs` `RotationSection`/`KioskRotationMode`/`KIOSK_ROTATION_MIN_DWELL_SECS` + `KioskPinRequired` (:744, the matrix doc at :1080); `tests/journeys.rs` `the_kiosk_mount_still_lands_on_the_board` / `the_kiosk_rotation_flips_modes_on_the_dwell_timer` / `the_kiosk_today_mode_renders_only_the_pinned_board` (the passive gates: inbox chip reads zero, banner-only surface model); `tests/golden_visuals.rs` `the_kiosk_board_golden_holds` / `the_kiosk_today_golden_holds`; the no-engine-command property rides the journey set (rotation is pure view state — stage 2 records the exact assertion form the K13 PR carried) |
| Docs/CHANGELOG promoted; exit report; milestone closure review | K14 | this chunk's PR + the report's §6.9/§6.10 |

### 2.4 Verdict facts the report must quote — including the landed-reality amendment

- **The D250 spike verdicts, as recorded 2026-10-21** (the netbook
  report's table — the exit report quotes, never re-measures):
  RSS 49 712 kB steady / 50 160 kB peak ≤ 200 MB (within, ~25 %);
  cold start 2 106.7 ms ≤ 5 s (within, ~42 %); full-frame repaint
  31.1 / 46.4 / 202.8 ms at N = 100 / 1 000 / 10 000 (within at
  board scale ≤ ~1 000, the 10 k extreme over); partial repaint
  failed as measured at every scale — but the measurement was
  dominated by the D6 dirt (partial region ≈ 97 % of frame), so the
  recorded verdict was "bounded above, true partial unmeasured";
  sync cycle 685 ms/cycle recorded (clean cycles, 0 failures).
- **The landed-reality amendment (D2) — the partial verdict moved.**
  The dispatch plan §0 and the 2026-10-21 report both say the
  partial budget "stays 'bounded above, verdict pending'" on issue
  #390. On today's tree that is stale in its issue-pending half:
  **#390 is CLOSED COMPLETED** (2026-10-26 10:54 UTC, closed by
  `9f05d67`, the view-pinned-re-measure bench commit) — the
  incremental-push fix direction it tracked landed via the #373
  ListView virtualisation round, and the re-measure reports record
  the post-change numbers: `draw-partial/today` 4.81 / 4.87 /
  5.75 ms and `draw-partial/board` 2.99 / 4.78 / 1.76 ms at
  N = 100 / 1 000 / 10 000 (dev machine, the ListView stage-2
  report §4) — within the ≤ 20 ms budget and N-independent
  (viewport-scale scaling class), with `draw-full/board/10 000` now
  2.78 ms where the eager board crashed (#462). **The honest verdict
  row is therefore neither "pending #390" nor a pass**: the
  dirt-tracking change landed and the dev-machine true-partial legs
  are within budget post-change, while **the on-device (D250)
  re-measure of the virtualised views has not been run** — it is the
  recorded follow-up (the dispatch plan's own "#373's on-device
  re-measure" ledger entry). The exit report's verdict-table row for
  partial: "dev-machine true-partial within budget after the
  #390/#373 incremental-push change (N-independent, viewport-scale);
  the on-device partial verdict remains bounded-by-the-spike-day
  measurement until the recorded on-device re-measure runs". This
  amendment is an evidence re-anchor in the dispatch plan's own
  MD-4 style, not a deviation from a ruling — no ruling covers the
  verdict's status. Stage 2 quotes both stages of the story (the
  spike-day record AND the post-change record) with pointers.
- **The ruled decisions, all recorded** (the exit report's ledger
  quotes each with its landing place): renderer posture — the
  measured permission-based software-GL fallback is sanctioned, no
  env pin, no build flip (ADR 0016's 2026-10-21 amendment; runbook
  §1/§8); i686 — a documented on-demand lane, no release-matrix leg
  (runbook §5.5); sd_notify — not needed under `Type=simple` +
  `Restart=always`, the D250 evidence recorded, #160 closed
  2026-10-21; dashboard — hyper 1.x server + capability token +
  loopback default (the K10 ruling, `Cargo.toml` :68-74); the file
  store's key management — D-K8 option (a), the generated 0600 key
  file + AEAD envelope (framing §4); the framing's D-K1…D-K10 all
  ACCEPTED 2026-10-19; D-V3's kiosk rulings (board-pinned, compact,
  passive, no capture) landed as the ambient view's actual gates.
- **Q6 (default `[app] mode`) is recorded-not-decided** (D10): the
  exit report carries the on-device input — the appliance is
  config-file-driven (`mode = "kiosk"` in the unit's config, never
  an env-var launch, ADR 0015's #236 rule); display-aware bare
  routing already covers the double-click pain the 2026-10-16
  deferral named; `ssh -X` sessions carry `DISPLAY` (the concrete
  argument for keeping the daemon default); the D250 ran as a
  dedicated systemd-driven appliance where the default mode never
  decides anything — plus the three options and the briefing's
  expectation ("keep daemon + current routing"), all explicitly
  marked as the user's pending ruling. The wave decides nothing.
- **The follow-up ledger, re-anchored (D3)** — by reference, no
  issue edits: **#390** closed 2026-10-26 (the incremental push
  landed via #373's round; residuals: the on-device re-measure and
  the stage-2-recorded column re-solve region bimodality finding);
  **#373's on-device re-measure leg** outstanding (user-executable
  per runbook §7's method); **#462** open (the software renderer's
  i16 pixel-ceiling panic — the eager board's shape; the ListView
  renders past it by construction, the renderer-arithmetic fix
  remains); **#463/#464** open (the detect-side refresh/diff
  follow-ups); **#386** closed (the `connect` 0600-rewrite bug,
  fixed by `7fe6f0e` — the dispatch plan listed it as open);
  the **optional NF310 measurement leg** (user, runbook §7).

### 2.5 Tracker state (verified live 2026-10-26)

- `wave-kiosk`: **0 open / 13 closed** (the `gh api …/milestones`
  reading). Every roster issue closed; the emptiness is already
  mechanical fact at plan time.
- #363 CLOSED (chunk 1's PR carried `Closes #363`; the tracker shows
  the closure timestamp 2026-10-26). #160 closed 2026-10-21 with the
  decision comment. #233 closed on the spike evidence.

## 3. `docs/architecture.org` amendment list (section-by-section)

Spine: the wave's one architectural story is **the appliance and its
surfaces** — one process = the daemon core + a fullscreen
presentation that degenerates to a passive ambient display; every
kiosk subsystem (deployment profile, metrics, credential store, web
dashboard, rotation) hangs off that shape without a new channel.
Every amendment must read true against the shipped code — stage 2
names the symbol each claim cites and spot-checks it in the worktree
(the dispatch's prose-code-drift trap).

1. **Executive Summary** (:21-24): replace "the passive ambient view
   remains the kiosk wave's later ambition" with the shipped
   sentence: the ambient view + rotation shipped (the
   `KioskMode`/`KioskRotation` display-mode state machine over the
   pinned board, UI-side dwell timer, zero engine commands), and the
   on-device budget verdicts are recorded (pointer to the exit
   report). Keep the two-bullet product vision; its "vision" framing
   for passive kiosk mode may now read "shipped" — stage 2 rewords
   the minimum that makes the paragraph true, no rewrite.
2. **New subsection "The kiosk appliance and its surfaces"** (after
   "The app-data sidecar", before "Inter-Component Communication
   Protocol"; the sidecar subsection's compactness — no padding):
   - the appliance shape (ADR 0016): one process = the
     `daemon_lifecycle` head + the ADR 0015 hand-over with
     `UiSettings.kiosk`; the display gate before any slint call
     (`kiosk_start`, the verified no-display panic evidence), the
     loud headless fallback that keeps serving IPC; close-exits
     through the shared serve-drain-shutdown tail; no kiosk IPC verb
     (the F16-28 fence unfired);
   - the deployment profile: `[kiosk]` typed under
     `deny_unknown_fields` — `fixed_board`'s resolution matrix
     (absent pin + 1 live board = that board; absent pin + ≥ 2 =
     `KioskPinRequired`, the K13 ruling that replaced "rotation
     scope" — quote the :1080 doc-comment matrix; unknown/dead pin =
     typed error) and `[kiosk.rotation]` (`modes` ≥ 2 distinct
     Board|Today, `dwell_secs` floor 10 s refused-never-clamped,
     every invalid value a typed error); the system unit
     (`Restart=always`, config-file mode, display-env import) and
     the runbook;
   - the ruled renderer posture (one sentence + the ADR 0016
     amendment pointer): the shipped winit+femtovg stack renders
     through Mesa software GL on the target class; the
     permission-based fallback is the sanctioned contract;
   - the metrics surfaces: state-borne per-board counters riding the
     existing `SyncReport` ingestion + the handle-side
     `EngineMetrics` (never in `AppState`), read lock-free by the
     CLI `status` and the dashboard — no new channel, no new wake
     source;
   - the credential file store: the third `CredentialStore` impl
     behind the unmoved port, `credential_store = "file"`, the
     generated 0600 key + chacha20poly1305 AEAD envelope, no silent
     fallback (ADR 0008 amendment);
   - the web dashboard: an app-layer hyper server on the daemon
     lifecycle, opt-in `[web]`, loopback default with the capability
     token gating a wider bind, meta-refresh polling — the second
     dumb presentation client over the same `ArcSwap` read (the
     framework-swap seam's second exercise; ADR 0015's rule holds
     over HTTP);
   - the ambient view: `KioskMode::{Board,Today}` rotation driven by
     the adapter-owned dwell timer in the UI crate — pure view
     state, no capture affordance reachable, the overdue banner +
     offline tone carried across flips, the desktop paths
     byte-identical when the table is absent.
3. **Crate role 5** (:405-448): after the daily-driver widening
   sentence, add the kiosk widening: `KioskMode`/`KioskRotation`
   (`view_state.rs` — the modes, the dwell, `tick`, the typed
   mint errors), the adapter-owned dwell timer (`lib.rs`), the
   passive gates (no capture affordance; the banner-only surface
   model in the today mode), the kiosk goldens
   (`tests/goldens/kiosk-*.png`). Presentation only — rotation never
   dispatches.
4. **Crate role 6** (:450-564): the config inventory gains `[kiosk]`
   (fixed_board + rotation, with the typed-error rule) and `[web]`
   (enabled/bind/refresh); the `secrets` sentence gains the file
   store; the `status` sentence gains the cumulative per-board
   counters and the `engine` object; the module map gains `web`
   (the dashboard server: routes/render/token). The `ui` sentence
   already reads true (bare kiosk → appliance) — touch nothing
   there.
5. **Tech-stack table** (:566-590): add hyper + hyper-util (the
   dashboard's HTTP server; pinned in `[workspace.dependencies]`
   with the pin-comment rationale) and chacha20poly1305 + getrandom
   (the file credential store's AEAD + key generation; the i686
   no-AES-NI software-curves rationale from the pin comments).
6. **Communication-protocol table** (:183-200): verify trueness only
   — no new channel landed (the dashboard reads `ArcSwap` like the
   UI; rotation is UI-internal view state; metrics ride the existing
   ingestion publishes). Touch nothing unless a row reads false.
7. **"Board Selection is Caller-Owned"** (:123-144): add one
   sentence — the kiosk appliance pins the board through
   `[kiosk] fixed_board` (resolved typed at boot; the caller-owned
   rule stands, the pin is the deployment's caller decision).

## 4. `README.org` amendment list

1. **"Deploying a kiosk"** (:366-381): keep the existing rows (unit,
   runbook, fixed_board — verified true); add the rotation keys and
   the ambient behaviour: `[kiosk.rotation]` with `modes =
   ["board", "today"]` and `dwell_secs` (floor 10 s, typed errors —
   an invalid value refuses the boot, never a silent default); the
   appliance's window is a passive ambient display (the board's
   kanban reading and the today view's upcoming reading alternate on
   the dwell timer; no keyboard/mouse input expected; no
   capture/edit affordance reachable); the pin-less multi-board
   boot refuses typed (`kiosk-pin-required`) — one board or an
   explicit pin.
2. **New "Web dashboard" usage section** (near the sync/CLI usage
   sections): opt-in via a `[web]` table (`enabled` defaults true
   inside a present table — writing `[web]` is the opt-in); loopback
   bind default (the trusted arm); a non-loopback bind requires the
   capability token (where the token lives, the boot check); what it
   shows (the read-only board view, sync health from the counters,
   offline state); meta-refresh polling with the self-DoS floor;
   the tablet path it exists for.
3. **Configuration section**: add `[kiosk]` and `[web]` rows to
   whatever table lists the config sections (the section that
   documents `[preferences]`/`[reminders]` — stage 2 re-locates its
   exact shape), with the same typed-error language.
4. **Credential store**: the existing `file` rows (:138-141,
   :539-544) verified true — expected no-op; amend only if a
   sentence contradicts the landed code.
5. **Accuracy sweep** of every new key/spelling against
   `crates/taskboard-app/src/config.rs` on the branch (the
   precedent's §4.7 rule — e.g. `dwell_secs` not `dwell`,
   `modes` lowercase, `refresh_secs` per `WebSection`'s real field
   names).

## 5. `CHANGELOG.md` promotion plan

Inventory: §2.2's kiosk entry list (14 landed entries: 12 verified
present + the K13 rotation entry + the missing `[web]` entry to
write). Rules (mirroring the precedent's D1, adapted):

- **No release cut** — everything stays under `[Unreleased]`;
  `bump_version.sh` (+ `changelog.py promote`) is the user's flow.
- **The missing `[web]` entry**: written from the tree (the
  `WebSection` surface + the token ceremony + what the page shows),
  one entry in Added, house style (issue #234, kiosk wave
  attribution). Nothing invented beyond what the code does.
- **Attribution normalization**: the wave's entries name the wave,
  not internal chunk numbers — "(kiosk wave K12, #126/#159; …)"
  → "(kiosk wave, #126/#159; …)" (same for K9 and any other
  numbered form; K13's entry already reads "(#363, K13)" → becomes
  "(#363, kiosk wave)"). Keep every issue number and every factual
  clause byte-equivalent otherwise.
- **Adjacency**: the wave's Added entries should read as one set
  where achievable with position-only moves (the precedent's
  "re-ordering only where needed" clause, applied sparingly — the
  kiosk entries are already scattered across the GUI-r2,
  daily-driver, and multi-board entries; stage 2 groups the kiosk
  run and touches no other entry's position more than the move
  requires).
- **Semantic-change fidelity** (the dispatch trap list, applied to
  the one K13 semantic change): the rotation entry's
  `kiosk-pin-required` statement re-verified against
  `config.rs::resolve_fixed_board` + its matrix doc comment — the
  pre-K13 plan-day world said "absent pin + N boards = the rotation
  scope"; the landed rule is the typed refusal. The entry already
  states the landed rule (verified at :25-34); stage 2 re-reads it
  against the code and fixes misstatements only.
- **Nothing invented**: no umbrella/summary entry; the added `[web]`
  entry states its own user-visible change.
- **Losslessness proof**: stage 2 diffs its result against the
  inventory entry-by-entry (the precedent's sorted-line-multiset
  check, allowing exactly: the attribution rewrites, the added
  `[web]` entry, the kiosk-run adjacency moves); stage 3 re-diffs.
- **The GUI-r2 `#390` citations are non-wave lines — untouched**
  (Findings, D14).

## 6. Exit report outline (`.artifacts/reports/<stage2-date>-kiosk-wave-exit-report.md`)

Header: date; base SHA (the merge-base of the PR against
`origin/main`); kind; links to the implementation plan §5 and this
plan; **the wave in one line** (fourteen chunks of plan → twelve
landed code/docs rounds + the user spike + this exit — the
appliance, the instrumentation, the deployment profile, the
dashboard, the file store, the metrics, the ambient view; verdicts
recorded; the milestone empty). Then:

1. **§ The landed wave — item → evidence → tracker map** (§2.1's
   table, PR numbers re-derived via `gh`).
2. **§ The D250 verdict table** — the four §7 slots verbatim from
   the netbook report (budget / measured / result columns), each
   row with its evidence pointer; **the partial-repaint row carries
   the two-stage honesty** (§2.4/D2): the spike-day record
   (dirt-dominated, bounded above) AND the post-#390 record
   (dev-machine true-partial within budget, N-independent; the
   on-device re-measure the outstanding follow-up). Never "pending
   #390" (stale), never an unqualified "pass" (the on-device leg
   did not re-run). The 10 k full-frame extreme and the dev-machine
   60.5 ms reference weighed exactly as the 2026-10-21 report
   prescribes (the wall shows one board; 10 k/board is the extreme
   fixture).
3. **§ The ruled-decisions ledger** — one row per decision with its
   landing place: renderer posture, i686 lane, sd_notify/#160,
   dashboard stack/auth/bind, D-K8 option (a), D-K1…D-K10 (the
   framing §4 ACCEPTED record), D-V3's kiosk rulings as landed
   gates. Every row cites its record (ADR/runbook/PR/issue).
4. **§ The Q6 input (recorded, not decided)** — the on-device facts,
   the three options, the briefing's expectation; the explicit
   sentence: this is the user's ruling; the wave decides nothing; a
   ruling there is a small follow-up chunk, not milestone work.
5. **§ The follow-up ledger (by reference)** — §2.4's re-anchored
   set (#390 closed with residuals; #373's on-device re-measure;
   #462 open; #463/#464 open; #386 closed; the optional NF310 leg),
   each with its issue number and current state as read on the
   tracker this day; **no issue edited**.
6. **§ Exit-criteria walk** — one subsection per implementation-plan
   §5 row (criterion verbatim), each with: the verifying chunk(s),
   the named evidence (§2.3's table — paths + test names only),
   the verdict. **Verdict vocabulary** (D8): *met* / *met — with
   recorded evidence limitation* / *not met*. Expected: every row
   met; the on-device row met-with-limitation (the partial leg's
   two-stage story; the re-measure outstanding); row 9 (ambient
   view) met on the K13 journeys/goldens/config evidence; row 10
   met by this PR itself (self-referential, cite the PR).
7. **§ Method** — what stage 2 actually ran (§7 restated in report
   prose so every pointer is re-verifiable).
8. **§ Findings** — one line each, honest: the stale
   "verdict pending #390" premise this plan amends; the GUI-r2
   CHANGELOG `#390` mis-citations (untouched, non-wave); README
   non-wave staleness if still present (the "Until the first
   release is cut" line — stage 2 checks whether the daily-driver
   round's F5 was since fixed); anything else stage 2 encounters.
9. **§ Backlog candidates (restated, not promoted — no issues
   created)**: the multi-board rotation ring (D-V3's explicit
   backlog); brightness/DPMS; the F16-28 attachable-GUI fence
   standing; the #462 renderer-arithmetic fix; the column re-solve
   bimodality; the Q6 ruling chunk; the optional NF310 leg.
10. **§ Bookkeeping — the milestone closure review**: the
    verification commands with their observed outputs:
    `gh api repos/D4id4los/taskboard/milestones --jq '.[] |
    select(.title=="wave-kiosk") | {open_issues, closed_issues}'`
    (→ 0 / 13); `gh issue view 363 --json state,_closedAt` (→
    closed); `gh pr view 516 --json state` (→ MERGED); `gh issue
    list --milestone "wave-kiosk" --state open` (→ empty). The
    sentence per AGENTS §7: closing is the archive — nothing
    deleted, nothing reopened; the milestone's formal closure is
    the user's act; this chunk made no tracker edits.

## 7. Verification method (stage 2 executes; the report restates it)

1. **Re-locate**: `git fetch origin --prune`; rebase-check; resolve
   every §2/§3/§4 path/symbol against the branch base (re-run the
   greps; the §2.3 names were verified at plan time but are plan-day
   references).
2. **Hermetic re-run of the kiosk-relevant suites** (worktree, named
   filters; record counts): `cargo nextest run -p taskboard-app
   config` (the `[kiosk]`/`[web]`/credential decode + typed-error
   tests — stage 2 narrows the filter to the section's real test
   names); `-p taskboard-ui-slint --test journeys
   the_kiosk` (the three K13 legs) + `--test golden_visuals
   kiosk` (the two goldens); the rotation state-machine units
   (`-p taskboard-ui-slint view_state` or the file's filter); the
   file-store secrets tests (`-p taskboard-app secrets`); the
   dashboard journey (`-p taskboard-app --test web_dashboard`);
   `cargo deny check`.
3. **No bench re-runs, no on-device legs** (D9): the recorded
   numbers are the evidence; the report cites, never re-measures.
4. **Re-read**: `gh pr view 516` (chunk 1's stage record + the
   structural no-engine-command assertion's form), #388 (the
   rulings PR), the closed issues #363/#160/#233/#390/#386 (state
   + closing record only — no edits); `gh api` the milestone.
5. **Semantic-fidelity pass**: the rotation CHANGELOG entry vs
   `resolve_fixed_board`; every new README/architecture claim vs
   the named symbol (the §3/§4 symbol-per-claim rule).
6. **Gates**: the full AGENTS §10 suite on the docs-only diff (§9).

## 8. Settled decisions (open questions resolved)

- **D1 — MD-1 verification method**: the remote branch was deleted
  post-rebase-merge, so containment is proven via the merged-PR
  record + the closed issue + artefact spot-checks on
  `origin/main` (§ header). Verified; no gate remains — chunk 2
  has no #373 gate, only its K13 predecessor.
- **D2 — the partial-repaint landed-reality amendment**: the
  dispatch plan's "verdict pending #390" is stale in its
  issue-pending half (#390 closed 2026-10-26; the incremental push
  landed via #373's round; dev true-partial within budget,
  N-independent). The exit report tells the two-stage story and
  keeps the on-device re-measure as the open follow-up (§2.4).
  Recorded here so stage 2 neither writes the stale "pending" line
  nor swings to an unqualified pass. Not a ruling deviation — no
  ruling covers the verdict status; the amendment follows the
  dispatch plan's own MD-4 landed-reality mechanism. Flagged to the
  user in this plan's handoff report regardless.
- **D3 — the follow-up ledger re-anchor**: #390 → closed with
  residuals; #386 → closed (fixed `7fe6f0e`); **#462 added** (open,
  kiosk-board-relevant — the dispatch plan's ledger omitted it);
  #463/#464 open; #373's re-measure + NF310 user legs outstanding.
  By reference; no edits (§2.4).
- **D4 — architecture.org spine**: a new "The kiosk appliance and
  its surfaces" subsection carries the wave's narrative; the crate
  roles are amended for trueness, not rewritten; every claim names
  its symbol (§3).
- **D5 — the Executive Summary**: the "later ambition" clause is
  replaced with the shipped sentence; the minimum true rewrite, no
  vision-section rewrite (§3.1).
- **D6 — README**: the `[web]` dashboard gets a usage section (its
  absence is the wave's only README-feature gap); the rotation keys
  join "Deploying a kiosk"; the accuracy sweep is config.rs-anchored
  (§4).
- **D7 — CHANGELOG promotion semantics**: no release cut; the
  missing `[web]` entry is written from the tree; attribution
  normalization drops internal chunk numbers ("kiosk wave K9" →
  "kiosk wave"); adjacency via position-only moves; losslessness
  diff at both stages; every factual clause otherwise preserved
  (§5).
- **D8 — verdict vocabulary**: met / met — with recorded evidence
  limitation / not met (the precedent's grades; the on-device row
  closes on the second grade) (§6.6).
- **D9 — measurement discipline**: no bench re-runs, no on-device
  legs; the report cites the recorded evidence (§7.3).
- **D10 — Q6 discipline**: the input is recorded with the options
  and the briefing's expectation, explicitly the user's pending
  ruling; nothing lands, nothing is recommended-as-decided (§6.4).
- **D11 — tracker discipline**: no edits of any kind; no `Closes`
  in the PR body; the closure review is verification-only, its
  commands + outputs recorded in the report (§6.10).
- **D12 — pointer stability**: the report cites paths + test names
  only; line numbers never load-bearing.
- **D13 — commit ladder** (stage 2; each commit standalone,
  rebase-merge hygiene): `docs(architecture): the kiosk wave
  subsystems` → `docs(readme): kiosk rotation keys and the web
  dashboard` → `docs(changelog): promote the kiosk wave entries` →
  `docs(report): the kiosk wave exit verification`; this plan
  commit precedes them (`docs(plan): kiosk wave exit docs`).
- **D14 — non-wave drift**: untouched in the diff; Findings lines
  (the GUI-r2 `#390` CHANGELOG citations; README staleness if
  present) (§6.8).
- **D15 — report filename date**: `<stage2-date>` = the day stage 2
  writes it; this plan refers to it symbolically.

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
- **The verdict-table honesty line**: the partial-repaint row is
  the chunk's hardest sentence — the stale "pending #390" and the
  unqualified "pass" are both findings; §2.4/D2 fixes the exact
  two-stage phrasing and stage 3 checks the row against the
  recorded reports.
- **CHANGELOG losslessness**: fourteen-plus entries interleaved
  with four other work streams' entries, plus one invented entry
  (the `[web]` line — the plan's single "write new prose" act,
  bounded by the tree evidence) — mitigated by §5's inventory +
  the entry-by-entry diff check at both stages.
- **Evidence pointers must resolve**: stage 3 verifies every cited
  test exists and is the cited behavior's — the §2.3 names were
  verified at plan time but are plan-day references.
- **Q6 framing drift**: a recommendation phrased as a decision is a
  dispatch trap; §6.4's explicit recorded-not-decided sentence is
  the guard.
- **Full gate-suite cost on a docs-only diff** (~build + coverage
  tree): expected; the worktree is removed at stage-3 teardown per
  AGENTS §8.

## 11. Stage-3 handoff (dispatch §3 prompt 2.3 specifics)

Rebase onto `origin/main` first (drop nothing — nothing stacked;
push `--force-with-lease` only if history was rewritten); review
the diff against the post-rebase base under the **evidence &
artefact mandate** in full: every exit row's pointers resolve (run
the cited tests); the verdict table's partial row tells the
two-stage story (D2); Q6 reads recorded-not-decided (D10); the
docs match the landed code (spot-check each new key/claim against
the source); the promotion is lossless modulo the recorded
allowances (D7); the follow-up ledger is complete per D3 and
edits nothing; the diff is exactly the two doc amendments +
CHANGELOG + exit report + this plan. Then ship: push, open the PR
to `main` with `gh` (body: the exit-report path + the closure
statement; **no `Closes` lines**), `gh pr checks --watch` to
green, `mergeStateStatus: CLEAN` (rebase + `--force-with-lease`
if BEHIND/DIRTY). No comment posting (MD-2). Never merge; never
touch `main`. Teardown: `git -C /home/viktoria/src/taskboard
worktree remove --force ../taskboard-WT-docs-kiosk-wave`,
confirmed via `git worktree list`.

# [Plan] Kiosk Wave — Design Round & Scope (framing)

- **Date**: 2026-10-19
- **Kind**: design-round framing plan for the `wave-kiosk` milestone. It
  audits the milestone roster against everything the repository and
  tracker know about kiosk work (§2/§9), records the wave-level
  decisions the user's session must rule (§4, recommendations stated,
  **nothing applied**), states the Stage-0 design questions the two
  open design rounds must settle (§5), and sketches the implementation
  chunking with effort that the wave implementation plan
  (`.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`,
  same PR) carries in full. It follows the
  `2026-10-18-daily-driver-wave-design-plan.md` shape.
- **Origin**: user request 2026-10-19 (plan the wave-kiosk milestone
  the way wave-multi-board and wave-daily-driver were planned; assess
  the "daily-driver ∥ kiosk in parallel, no major conflicts" advice
  against the detailed daily-driver plan; audit the kiosk issue
  roster). Inputs consumed:
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  (the accepted design-of-record, Option A + §6 rulings Q1–Q7),
  `.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md` (the
  accepted wave-1 chunking — re-anchored by this round, see §3),
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (§3.7 kiosk rows: the display-mode rotation item and the
  desktop/tablet overview as "kiosk cousin"; §6 wave sequencing),
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md`
  (the `§budgets` envelope + sketches S1–S4 the four harness issues
  consume), `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`
  (§9/F16-26…F16-29 kiosk seam map), `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  (its kiosk deferrals at :1066 and :1341: compact degeneration +
  the config-pinned fixed-board surface), `.artifacts/plans/2026-10-19-views-finding-design-plan.md`
  (D-V3: the kiosk surface is this wave's to own; today view is
  rotation-ready), `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  + its dispatch plan (the parallelism counterparty), and the ten
  milestone issues' bodies.
- **Status**: framing **PROPOSED** — §4 decisions await the user's
  session; the tracker bookkeeping (§9) applies only after that
  ruling. The wave implementation plan's phase-A chunks are
  READY-unblocked (their designs are accepted); phase-C chunks are
  gated on §5's design rounds and on daily-driver chunk 10. Code
  anchors verified on `main` @ `5480869` (post daily-driver planning
  PR #360; multi-board implementation round 1 of 8 landed via PR
  #358).

---

## 1. Mission

Make `mode = "kiosk"` real on the target hardware: the Option-A
appliance (daemon-style core + IPC server + fullscreen on-seat window
in one process, `daemon stop`/ssh control, close-exits-appliance),
the instrumentation and deployment profile the gen3-class netbooks
need (software renderer, 1 GB RAM, systemd supervision, credential
store without a Secret Service), the on-device bring-up spike that
turns the audit's budget envelope into verdicts, and — as the wave's
second surface — the tablet-path web dashboard and the passive
ambient view with display rotation. The wave ends with the default-
`[app] mode` question (Q6, deferred 2026-10-16) answerable from real
on-device experience.

## 2. Scope

### 2.1 In — the milestone roster as it stands (10 issues, audited)

| Issue | What it contributes | Verdict |
|---|---|---|
| #237 kiosk mode (Option A, accepted 2026-10-16) | the appliance itself: `taskboard gui` verb, kiosk lifecycle, window profile, packaging/launch docs | **fits** — the wave's spine; design accepted, chunk plan re-anchored by this round |
| #233 hardware bring-up (NF310 + D250) | the on-device spike: renderer/backend decision + budget verdicts | **fits** — two-headed: agent-side prep (harnesses, profile, runbook) + user-executed on-device measurement; the milestone's completion gate is user-gated by design |
| #299 RSS smoke harness (F13-01/S1) | dev-machine RSS instrumentation feeding the spike | **fits** — audit sketch is the design |
| #300 refresh+draw bench (F13-03/S2) | dev-machine repaint instrumentation via the `test_platform` seam | **fits** — carries a known API-blocker risk (slint may not draw headlessly); outcome recorded as evidence either way |
| #262 cold-start stopwatch (F13-04/S3) | spawn-to-first-frame instrumentation | **fits** — pairs with #299 in one process-harness chunk |
| #263 DTO↔domain mapping bench (F13-05/S4) | sync CPU-leg bench | **fits** — pure criterion work, `hot_path` conventions |
| #156 sync-actor metrics | dashboard content + `status` observability | **fits** — small design choice (§5.3); sequencing-sensitive vs the daily-driver round (see the parallelism report) |
| #234 web kiosk dashboard (tablet path) | opt-in daemon HTTP surface, boring HTML, auth-gated | **fits** — but needs its design round first (§5.1); backlog ruling already prefers the browser path over native Android |
| #160 systemd/journald remainder | `sd_notify` readiness only (template shipped, ADR 0012) | **fits, conditional** — keep in-milestone, decide at the spike (§4 D-K5); `Restart=always` likely suffices |
| #126 keyring fallback remainder | the encrypted-file credential store (= #159's scope) | **fits** — needs the key-management mini-design (§5.2) + stack approval if passphrase-derived |

### 2.2 In — proposed additions (the §4 rulings apply these)

- **New issue: kiosk ambient view + display-mode rotation** — the
  future-directions §3.7 row ("kiosk kanban ⇄ upcoming-list
  switching: a display-mode state machine + rotation policy", S–M)
  plus the multi-board design's deferred "config-pinned fixed-board
  kiosk" and the views-finding D-V3 statement that the kiosk surface
  is this wave's to own. **No issue tracks this today** — the only
  substantive kiosk work item with no tracker home (§9).
- **#159 encrypted-file credential store** — the concrete remainder
  of #126; assign alongside it (or #126 closes citing #159).
- **#145 engine metrics counters** — the dashboard's second content
  source; same seam as #156, bundled dispatch.

### 2.3 Out (later waves / other homes — restated so nobody re-derives)

Option B, the attachable GUI process over IPC streams (Q7 ruled it
after the dashboard; its contract cost stays recorded at the F16-28
extraction fence per F16-27 — no kiosk-management IPC verb lands in
this wave, so the fence does not fire); native Android/Slint port
(#234's option a, re-enter only if tablet interactivity becomes
real); aarch64 release legs (#115 — the kiosk hardware is x86_64);
brightness/dim/DPMS power management; multi-kiosk fleet management;
watchdog machinery beyond systemd supervision (Option A accepted
process-level restart; F16-29's "no watchdog" bound stands);
`#194` IPC disable knob and the E-12 security hardening family
(revisit with the dashboard's auth model, not silently);
second-priority OS targets for the kiosk path; undo, recurrence, and
every other daily-driver/Wave-2–3 item.

## 3. Baseline — what exists on `main` @ `5480869` (re-anchoring the 2026-10-16 facts)

The 2026-10-16 wave-1 plan's §2 design facts 1–2 are **stale in
exactly two places**, both changed by the #236 display-aware bare
launch fix (`3ad16bb`, PR #332, 2026-10-18):

1. **`display_available` now exists** — a pure decision fn over the
   two env seams (`crates/taskboard-app/src/ui.rs:82-84`), with the
   env-reading wrapper beside it; its doc comment explicitly invites
   the kiosk wave to consume it. The wave-1 plan's "new pure decision
   fn" work item is done; the gate wiring remains.
2. **The bare-routing table has a new cell**: `daemon` + no-TTY +
   display now boots the **desktop UI** (`ui.rs:112-121`;
   `desktop`→DesktopUi, `daemon`+TTY→Repl, `daemon`+display→DesktopUi,
   else daemon lifecycle). `kiosk` still maps to `DaemonLifecycle` in
   **every** cell, and the daemon mode gate still refuses
   `Desktop|Kiosk` typed (`main.rs:256-261`) — #237's contract is
   untouched, and the commit message states "the gui verb is the
   kiosk wave's scope": **no `gui` subcommand exists** (cli.rs's
   catalogue: connect/login/boards/stacks/tasks/labels/logout/sync/
   config/status/daemon).

Everything else the accepted design relies on verified unchanged:
the desktop GUI boots its own core under the instance lock
(`run_desktop`, `ui.rs:191-244`); the daemon tail shape
(bootstrap → system-event logger → `ipc::server::spawn_server` →
select(signal, `server.stopped()`) → bounded drain → shutdown,
`main.rs:248-340` — note the 2026-10-18 engine-exit-reachability
round reshaped the exit path; stage 1 re-locates); IPC protocol v1
carries Exec/Status/Shutdown/Detach/Attach only; the UI crate has
three views, no fullscreen/borderless handling, `REPAINT_CADENCE_MS
= 100` compile-time (`adapter.rs:35`), `[ui]` is scale + keymap
under `deny_unknown_fields`; one criterion bench exists
(storage `hot_path`), Bencher-tracked in CI; no metrics counters
anywhere (sync "metrics" are the state-borne per-board `SyncStates`
+ `aggregate()` fold, multi-board chunk 1); `CredentialStore` ports
keyring + env with a typed `Backend` error naming the env
alternative (`secrets.rs`); `contrib/taskboard.service` is the user
daemon unit (`Restart=on-failure`, explicit `daemon` verb); release
packaging stages binary + licenses + README + CHANGELOG — **no
`.desktop` file**; the overdue-reminder banner already renders on
the board surface (`presentation.rs` `banner_rows`) — a passive-
display primitive the ambient view builds on; slint ships default
features (winit + femtovg with software fallback) —
`renderer-software`/`backend-fb` are not compiled in (ADR 0015
deferral; the spike decides).

## 4. Wave-level decision record (user session — PROPOSED, one recommendation each)

| ID | Question | Recommendation |
|---|---|---|
| D-K1 | Milestone shape: three phases — (A) appliance + instrumentation + deployment profile, all dispatchable now; (B) the user-executed on-device spike fed by A; (C) dashboard + file store + ambient view, gated on design rounds and daily-driver chunk 10 | **Accept** — matches the accepted sequencing (wave 1 → spike → wave 2 surfaces) without splitting the milestone |
| D-K2 | Create the missing **ambient view + rotation** issue and assign it | **Yes** (§9 gives the body) — it is the only untracked kiosk work item; rotation is future-directions §3.7 and the D250 wall display's reason to exist |
| D-K3 | Add **#159** and **#145** to the milestone | **Yes** — #159 is #126's concrete remainder; #145 is the dashboard's second feed, same seam as #156 |
| D-K4 | **#123** (daemon IPC remote control): implemented by WI-5, follow-ups separately tracked, still open | **Close as completed** with a pointer (closing is the archive, AGENTS §7); it is not kiosk-wave work |
| D-K5 | **#160** `sd_notify` readiness | **Keep in-milestone, decide at the spike**: `Restart=always` + Type=simple likely suffices; implement only if start-up ordering demands it (the issue's own condition) |
| D-K6 | Metrics shape for #156 (+#145) | **State-borne per-board counters on `SyncStatus`** (ride existing `SyncReport` ingestion + `ArcSwap` publishes; dashboards/UI read lock-free; no new channel) and a **handle-side atomic `EngineMetrics` struct** for #145 (no snapshot churn); alternatives recorded in §5.3 |
| D-K7 | Dashboard HTTP dependency | **Not decided here** — stack approval belongs to the §5.1 design round (candidates + one recommendation there); no HTTP crate enters the tree before that ruling |
| D-K8 | File-store key management | **Generated 0600 key file** default (unattended kiosks; no new dependency) **+ optional passphrase-derived** variant (argon2; stack approval at stage 1); final ruling can ride the chunk's stage-1 plan unless the user wants it in this session |
| D-K9 | Default `[app] mode` (Q6, deferred 2026-10-16) | **Re-surface after the spike** with on-device evidence; the wave's exit chunk records the input, nothing lands before |
| D-K10 | Parallel dispatch posture vs the in-flight multi-board round and the gated daily-driver round | **Accept the "no major conflicts" advice with the three named mitigations** of the parallelism report (K-anchor sequencing for sync-actor metrics and the cli.rs adjacency; ambient view strictly after daily-driver chunk 10) — full analysis: `.artifacts/reports/2026-10-19-kiosk-daily-driver-parallelism-analysis-report.md` |

## 5. Stage-0 design questions (the round's remaining work)

The appliance itself needs **no** new design — Option A + Q1–Q7 are
accepted (2026-10-16) and the audit located every seam (F16-26:
three typed mode-gate flip sites, the inline daemon tail extraction
first, the display gate now existing; F16-28's IPC fence does not
fire — no kiosk-management verb this wave; F16-29's bounds stand).
Two design rounds and three small seams remain:

1. **Web kiosk dashboard** (#234; docs-only round, dispatchable
   immediately in parallel — the D-D5 precedent): HTTP stack choice
   (tiny_http-scale vs axum-scale vs hand-rolled hyper; one
   recommendation + stack approval); bind + auth model (default
   loopback; any wider bind gated on an auth ruling — token vs
   none-with-warning vs reverse-proxy assumption); content scope
   (read-only board view + sync health from the metrics + offline
   state; explicitly boring CSS for the 2014-era Android browser,
   hands-on test flag); refresh strategy (meta-refresh polling —
   no JS, or minimal JS with a no-JS fallback); caching; where the
   server lives (app-layer task on the daemon lifecycle, opt-in via
   config); interaction with `standalone` mode.
2. **Encrypted-file credential store** (#126/#159; can fold into its
   chunk's stage-1 given D-K8's default): key-management variant(s),
   file format (age-style vs simple AEAD envelope — new-dependency
   audit either way), `[nextcloud] credential_store = "file"` config
   surface, unlock UX for the passphrase variant, failure fences.
3. **Small seams settled inside the implementation plan** (§2 design
   facts, no round needed): the `[kiosk]` config section shape
   (fixed-board pin lands with the deployment-profile chunk; rotation
   keys land with the ambient view); the metrics shapes (D-K6); the
   kiosk systemd unit template (system-level, `Restart=always`,
   display-env import, `TASKBOARD_APP__MODE=kiosk`-style config not
   env); the S2 bench's headless-draw fallback evidence rule.

## 6. Implementation chunk sketch (effort; the implementation plan details)

Sizes on the house scale (S ≤ ~3 days; M ~1–3 weeks; L ~3–8 weeks).
Full chunk sections — files, behaviours, non-goals, branch names —
are the implementation plan's §3; this table is the effort answer.

| # | Chunk | Issues | Size | Phase |
|---|---|---|---|---|
| K1 | `taskboard gui` verb | #237 | S | A |
| K2 | Kiosk window profile flag | #237 | S | A |
| K3 | Kiosk appliance lifecycle + ADR 0016 | #237 | M | A |
| K4 | Packaging & launch docs (`.desktop`, README) | #237 | S | A |
| K5 | Process smoke harnesses: RSS + cold-start | #299, #262 | S–M | A |
| K6 | Refresh+draw bench (test_platform seam) | #300 | M | A |
| K7 | DTO↔domain mapping bench | #263 | S | A |
| K8 | Sync metrics on `SyncStatus` (+ engine counters if D-K3) | #156 (+#145) | S–M | A* |
| K9 | Kiosk deployment profile + systemd unit + runbook | #233 prep, #160 | S–M | A |
| K10 | Web dashboard design round (docs-only) | #234 | S–M | A∥ (design) |
| — | On-device bring-up spike (user-executed) | #233 | user days | B |
| K11 | Dashboard implementation | #234 | M | C |
| K12 | Encrypted-file credential store | #126, #159 | M | C |
| K13 | Kiosk ambient view + display rotation | new (D-K2) | M | C |
| K14 | Docs + wave exit verification | — | S | C |

\* K8's dispatch window is sequencing-sensitive (parallelism report
§3.1): before the multi-board round's sync-actor chunk or in the
multi-board→daily-driver gap — not concurrent with either.

**Effort totals**: phase A ≈ 3–3.5 M-equivalents (~3 weeks of agent
chunks, compressible by parallel dispatch — K1∥K2, K5∥K6∥K7 are
disjoint); phase B is user hardware time plus a small agent support
burden; phase C ≈ 2.5 M-equivalents. **Wave total: L (~5–8 weeks
calendar)**, of which the design-complete, dispatch-today subset
(K1–K9) is roughly half. For calibration: wave-multi-board dispatched
8 chunks (2 L, 5 M, 1 S), daily-driver 11 (3 L, 6 M, 2 S) — kiosk's
14 (1 M impl-spine + harnesses + surfaces) sits between them.

## 7. Sequencing

1. **Now**: this framing + the implementation plan + the parallelism
   report land as one PR; the user's session rules §4.
2. **Phase A** (immediately after, in parallel with the in-flight
   multi-board round per D-K10): the appliance chunks
   `K1 ∥ K2 → K3 → K4`; the instrumentation chunks `K5 ∥ K6 ∥ K7`
   (disjoint files) and `K8` in its sequencing window; `K9` after
   K3 (the profile configures the appliance it lands with) and after
   the multi-board config chunk's `[[sync.connectors]]` settles
   (same file, rebase-scale); the K10 design round anytime (docs-
   only, D-D5 precedent).
3. **Phase B** (after A): the user runs the spike on the NF310 and
   D250 per the K9 runbook; verdicts land as a bring-up report; the
   renderer/backend decision, the #160 ruling, and the Q6 input are
   its outputs.
4. **Phase C** (gated): K11 after the K10 design is accepted (+ K8
   for content); K12 after D-K8 stands (any time, actually —
   `secrets.rs` is untouched by both other rounds; treat as
   phase-A-dispatchable if the user prefers); K13 strictly after
   daily-driver chunk 10 (today view) — a dependency, not a
   conflict; K14 last, serialised with the other waves' docs chunks.
5. **Tracker**: §9 applies at the user's ruling; milestone closure
   review rides K14.

## 8. Risks

- **S2 headless-draw API blocker** (#300): slint 1.18.1 may not draw
  without a display server — the chunk records the outcome as
  evidence and falls back to refresh-leg benching + spike-side draw
  measurement; never a silent scope cut.
- **Dashboard = a network surface on the daemon**: the auth model is
  a hard gate before any non-loopback bind; conservative default
  (loopback, opt-in LAN) is the design round's starting position,
  not a post-hope.
- **1 GB devices are unforgiving**: budgets exist (RSS ≤ 200 MB,
  repaint ≤ 50 ms full / 20 ms partial, cold start ≤ 5 s, 30 s sync
  cycle); the harnesses exist so the spike measures instead of
  guessing; llvmpipe is absent by design (pure software renderer).
- **Cross-wave dependency discipline**: the ambient view depends on
  daily-driver chunk 10; dispatching it early would collide with
  DD-9/10's exact files (`view_state.rs`/`actions.rs`/`models.rs`,
  `cli.rs`/`repl.rs`) — the parallelism report names this as the one
  hard ordering rule.
- **Three rounds in flight**: multi-board (7 chunks remaining),
  daily-driver (gated, 11), kiosk (14) — worktree disk floors and
  agent-session capacity are real constraints; cap concurrency per
  host and honour `scripts/worktree-doctor.sh` stops.
- **Scope creep**: Option B, brightness/fleet management, and the
  matrix/organiser views keep knocking; §2.3 is the fence.

## 9. Tracker bookkeeping (PROPOSED — applies at the §4 ruling, not before)

- **Create + assign**: `backlog: Kiosk ambient view + display-mode
  rotation (board-pinned per D-V3; kanban ⇄ today/upcoming)` —
  labels `ui` + `performance`, body cites future-directions §3.7
  :173, the multi-board detailed design's config-pinned-fixed-board
  deferral, and D-V3's "the kiosk surface is the kiosk wave's to
  own"; permalink to this plan.
- **Assign to `wave-kiosk`**: #159, #145 (scope note on #145: the
  handle-side counter shape per D-K6).
- **Scope comment on #126**: remainder = #159's scope; both close
  together in chunk K12.
- **Close as completed**: #123 (pointer to WI-5/ADR 0012; follow-ups
  #193/#194/#195/#196 stay open, unscheduled).
- **Milestone description** for `wave-kiosk` (currently empty):
  one-liner mirroring `wave-daily-driver`'s style — "Option-A kiosk
  appliance (gui verb, appliance lifecycle, window profile,
  packaging), audit perf instrumentation (RSS, repaint, cold start,
  sync CPU), kiosk deployment profile + on-device bring-up spike,
  sync/engine metrics, tablet-path web dashboard, encrypted-file
  credential store, and the passive ambient view with display
  rotation."
- No other tracker edits; no issues closed beyond #123; nothing
  unassigned from the milestone.

## 10. References

- Accepted kiosk design: `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  (Option A + §6 Q1–Q7 rulings); wave-1 chunking:
  `.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md` (superseded
  as implementation-of-record by this round's implementation plan;
  remains design-of-record input).
- Display-aware launch (the re-anchoring commit): `3ad16bb`, PR #332;
  ADR 0015's #236 amendment.
- Audit seams: perf budgets report (`§budgets` rows 1–5, S1–S4
  sketches, C1/C2 notes, F13-01…F13-09); arch-fit report §9
  (F16-26…F16-29, the file map: app `ui.rs`/`main.rs`/`config.rs`/
  `ipc/`/`bootstrap.rs`, ui-slint `lib.rs`/`adapter.rs`, "the wave
  touches no domain contract"); findings report rows A-5, P1-5/P1-6/
  P1-7, I-1.
- Parallel counterparties: `2026-10-18-multi-board-impl-dispatch-plan.md`
  (chunk file sets), `2026-10-19-daily-driver-implementation-dispatch-plan.md`
  (chunk file sets + MD-1 round gate).
- Kiosk-surface dependencies: multi-board detailed design (compact
  degeneration, config-pinned fixed board); views-finding D-V3 +
  D-V1 rotation-readiness.
- Pattern exemplars: `2026-10-18-daily-driver-wave-design-plan.md`
  (this doc's shape), `2026-10-18-multi-board-wave-design-plan.md`.
- Code anchors @ `5480869`: `ui.rs:82-84` (`display_available`),
  `ui.rs:112-121` (`bare_invocation_route`), `main.rs:256-261` (mode
  gate), `main.rs:248-340` (`daemon_lifecycle`), `ui.rs:191-244`
  (`run_desktop`), `adapter.rs:35` (`REPAINT_CADENCE_MS`),
  `test_platform.rs:18-28` (NullPlatform/NullWindowAdapter),
  `secrets.rs` (CredentialStore), `contrib/taskboard.service`,
  `benches/hot_path.rs`.

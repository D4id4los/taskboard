# Backlog Readiness Triage — 2026-10-31

- **Date**: 2026-10-31
- **Kind**: report — a full re-triage of the GitHub issue tracker after
  the 2026-10-25 → 10-30 implementation burst, classifying every open
  issue into **ready now** vs. **should wait**, with dependencies and
  a suggested dispatch shape. Commissioned for the user's review
  ahead of an implementation plan; no code, no tracker changes.
- **Amendment (2026-11-01)**: two of the commissioned picks — #352
  (timers surface) and #490/#491 (tracked-time surfaces) — were
  absorbed into GUI review round 4 (chunks 9/10) per that round's
  confirmed §2 decision 4; the backlog round-2 dispatch plan's
  amendment records the details.
- **Method**: open-issue set diffed against the 2026-10-25 triage
  (124 → 110 open; 42 closed, 26 new); bodies of all 26 new issues
  read in full; the 335 commits on `main` since `90c947c` reviewed for
  premise changes; targeted code spot-checks (#231's preset bindings,
  #448's lane failure in run `40669687511`); milestone and PR state
  pulled. Old-issue bodies are assumed known from the 2026-10-25
  survey; only their *status* is re-classified here.

## 1. What changed since the 2026-10-25 triage

The week executed essentially the entire 2026-10-25 dispatch plan plus
every queued design round, two additional design rounds (time
tracking, recurring/habits), GUI review round 3, the kiosk wave's
remaining chunks, and the ListView virtualisation round:

- **All 12 dispatch-plan chunks landed and closed their issues**
  (#386, #395, #434/#435, the docs pass, #425, #426, #427, #390, #283,
  #245, #253/#254, the flaky pass). PR #441 (the plan) merged 10-25.
- **The four queued design rounds ran and shipped**: DR-1 reminder-rule
  sync (#202 closed), DR-2 signal handling (#174/#192/#196/#200/#162
  closed — REPL/pty Ctrl-C handling landed), DR-3 reminders surface
  pair (#203 closed — insistence flavour + escalation driver + alarm
  window landed; **#352's timers composition remains open**), DR-4
  merge policy (#128/#129/#130/#150 closed — OR-set labels, resurrect
  carve-out, crash-window reconcile all landed; chunk S = #480
  remains).
- **Two bonus design rounds ran**: recurring/habits (#212 closed —
  habits domain, sidecar roaming, CLI + today-view surfaces landed;
  follow-ups #504–#506) and time tracking (#210 core landed — track
  CLI, entries, storage 0015, UI affordances; fences #490–#496).
- **Undo v1 landed** (journal, ADR 0019, verbs + view); the v2 family
  stays open by ruling, plus new #555.
- **GUI review round 3 landed** (navigation/palette/menu, board
  surface, taskview dates, reminder delivery, settings polish, flow
  state machine, legacy-config migration). **The ui-slint freeze is
  lifted** — no crate is under active claim.
- **Kiosk wave closed** (ambient view + rotation; `wave-kiosk` and
  `wave-daily-driver` milestones both show 0 open issues) — and the
  kiosk is evidently in real use: it immediately produced #462.
- **26 new issues**, four of them p1/p2 bugs found by the newly-live
  tier-2 docker lane and the view-pinned bench: #447/#448/#449 (anchor
  scan on Deck 1.19.1 / the 1.17.5 C4 flush), #462 (kiosk i16 render
  panic), plus a fresh perf cluster (#463/#464), the Today-view
  virtualisation fence (#461), and three sibling pty flakes
  (#521/#524/#549).
- **The weekly Nextcloud lane is red on main** (2026-10-28 run: C1
  `it_nextcloud_docker_sync_app_field_convergence` hangs 30 s at
  anchor confirmation — exactly #448's mechanism). #436's triage is
  therefore live, not prospective.

## 2. Verdict summary

| Bucket | Count | Issues |
|---|---|---|
| Ready now — tier 1 (production-broken cluster) | 4 | #448, #447, #449, #436 |
| Ready now — tier 2 (kiosk p1 + render/perf wave) | 4 | #462, #463, #464, #461 |
| Ready now — tier 3 (small, well-specified) | 8 | #480*, #450, #458, #407, #282, #401, #201, #536 |
| Ready now — tier 4 (dedicated passes) | 14 | #521, #524, #549, #287, #336, #384, #270, #271, #272, #273, #275, #276, #279, #280 |
| Ready on product-priority call (unblocked, user picks) | 18 | #352, #490, #491, #492, #493, #495, #496, #504, #505, #506, #555, #351, #411, #412, #214, #350, #116, #194 |
| Wait — external/calendar/deferred by ruling | 12 | #440, #428, #429, #444, #494, #219, #114, #115, #117, #113, #121, #122 |
| Wait — trigger-gated (measurement/consumer/demand) | 27 | #131, #133, #135, #137, #138, #139, #140, #141, #142, #146, #148, #152, #153, #154, #155, #157, #167, #181, #127, #180, #191, #193, #195, #198, #220, #221, #222 |
| Wait — needs design round / usage evidence | 12 | #213, #215, #216, #217, #218, #184, #185, #327, #353, #354, #355, #356, #357 |
| Tracker hygiene (verify, then close) | 3 | #166, #231, #210 (+ milestone states) |

\* #480 is ready the moment the user confirms-or-flips fork F-133 (the
issue says the dispatch waits for that ruling).

Rough shape: **~30 issues are cleanly dispatchable right now** (tiers
1–4), **18 more are one product decision away**, and the remaining
~60 are wait-class for reasons that were deliberate when they were set
— they should not be re-derived.

## 3. Ready now

### Tier 1 — the anchor cluster (production-broken today)

The only place where the product is currently misbehaving against the
live backend. One fix chunk, one branch:

- **#448 (p1 bug)** — `normalize_file_name` folds Deck 1.19.1's
  `info.filename` (basename *without* extension) over the `data`
  payload, so the anchor scan never confirms and re-creates the
  appdata anchor on every cycle against Deck ≥ 1.19.1. Characterized
  with wire evidence; fix direction given (rank
  `fileName → info.basename → info.filename`).
- **#447 (p2, testing)** — the same fix's test half: pin the measured
  1.19.1 listing shape without regressing the 1.17.5 echo shape.
  Same file, same change as #448 — not separable.
- **#449 (p2 bug)** — distinct failure on the 1.17.5 gate tier (C4):
  the fallback flush never *creates* the anchor (DB-verified zero
  attachments). Less characterized; the issue says to triage it in
  the same chunk with actor-level tracing.
- **#436 (ci)** — the weekly lane is red on main (2026-10-28, C1 hang
  confirmed in run `40669687511`). After the fix chunk merges:
  re-run the lane, triage the 1.17.5 leg against #449's outcome,
  close.

Dependency: strictly serial inside the chunk (#447+#448 → #449 triage
→ #436 verification). #282 (below) can ride the same tier-2 session.

### Tier 2 — the render/perf wave (fresh bench evidence, serialize inside)

The #460 view-pinned re-measure produced three well-specified wins and
one p1. All touch the refresh pass / view files — run them as one
wave with internal ordering, never two agents in `models.rs` at once:

- **#462 (p1, ui)** — the kiosk board panics (i16 software-renderer
  overflow, `overflow-checks = true`) once a column exceeds ~2.5k
  tasks. The structural lever is the **already-landed #373 ListView
  mechanism**; a `clip: true` quick fix exists but is a visual-design
  decision (goldens). Stage 1 picks the lever and owns the golden
  question. Pairs with **#536** (below) on a single upstream slint
  report — the i16 facts are already written down in the issue.
- **#463 (p2, performance)** — O(1) `Arc::ptr_eq` unchanged-guard at
  the top of the refresh pass. Small, isolated, biggest detect-side
  win (20 ms/tick at N=10 000 → ~0). Dispatch first.
- **#464 (p2, performance)** — Rust-side diff mirror replacing the
  clone-per-row `row_data` comparison. After #463 (same pass; #463's
  guard bounds #464's payoff).
- **#461 (p2, performance, ui)** — virtualise the Today view's section
  lists with the same #373 mechanism, plus the anchor-on-removal
  semantics the board doesn't need. After #462 (shared mechanism and
  view files; #462 is the p1).

### Tier 3 — small, well-specified, mutually independent

- **#480 (p2, testing)** — DR-4 chunk S: the skew interleaving pins
  (`…_currently_pins_…` debt tests + a Clock-seam hermetic demo).
  **Blocked only on the user's F-133 ruling** (test-pack now vs. build
  the Δ-band guard) — the issue explicitly asks for confirm-or-flip
  before dispatch. Also owns the stale-#133 bookkeeping comment.
- **#450 (chore, dx)** — worktree-doctor blind spot: a fresh
  never-pushed worktree at `origin/main`'s tip reads as disposable and
  got swept mid-session once already (lost plan work). Small; the
  issue sketches the false-positive check too.
- **#458 (p3, testing)** — the one real survivor of the #427 UI
  mutants sweep (`scheduled_phrase` direction flip). Recommend
  resolution option 2 from the issue (type the direction
  Future/Past/Now × bucket, A1-style) — kills the mutant with zero
  text assertions and matches the house policy.
- **#407 (p3, ui/dx)** — the clap `ArgGroup(required = true)` on
  `tasks edit` / `labels edit`. Tiny, spec'd, still open from the
  last round.
- **#282 (p3, reliability)** — dead-board drain liveness; its
  precondition "verify Deck's actual write-answer on the tier-2 rig"
  is now cheap — the rig comes up weekly. Natural rider on the tier-1
  chunk's session.
- **#401 (p3, architecture)** — additive `AppDataUnreadable`
  4xx/5xx split. Small, self-contained.
- **#201 (p3, architecture)** — age-based pruning of delivery proofs;
  audit-verified mechanical (F16-20; cascade precedent exists).
- **#536 (testing/dx)** — slint testing-backend empty element queries
  with a second conditional chrome control. In-repo action is only
  documentation + an upstream report — do it as #462's sibling, not
  standalone.

### Tier 4 — dedicated passes

- **Flaky pass**: **#521 + #524 + #549** are one item, not three —
  same file, same mechanism (60 s outer pty timeout under full-suite
  load), three observations in three days. The fix shape is the outer
  timeout/budget seam, per each issue's reading. The same pass should
  re-disposition **#287, #336, #384** (open since the last pass's
  capture policies; close or re-arm per the watchlist policy).
- **Audit cleanup wave**: **#270, #271, #272, #273, #275, #276**
  (per-crate small cleanups), **#279** (shutdown/lifecycle hardening
  bundle), **#280** (pin the two convention-only contracts). One
  caveat dominates: these findings date to **2026-10-17**, and ~335
  commits have landed since — every finding must be re-verified
  against the current tree before fixing (the chunk-5 re-verification
  rule); several may already be fixed or made stale by the week's
  churn. Parallelizable per crate.

## 4. Ready on product-priority call (unblocked; the user picks)

All gates below have landed; what's missing is only appetite:

- **#352** — timers & alarms UI composition. DR-3 designed the pair;
  the escalation/alarm-window half landed, the timers list/ack
  surface remains. Stage 1 confirms remaining scope against the DR-3
  plan.
- **Time-tracking fences** (DR-5 deps all landed): **#490** (today
  totals), **#491** (task-detail entry history), **#492** (tracker
  task type), **#493** (respite flag), **#496** (pomodoro overlay —
  the DR-3 escalation driver it wanted also landed). Internal
  dependency: **#495** (break reminders) needs **#493** first.
- **DR-6 follow-ups**: **#505** (Kani recurrence harnesses — the #426
  pilot's conventions exist), **#506** (habit UX trio: convert verb,
  series pause, streak repair), **#504** (tracker reminders — the
  most design-flavored of the three).
- **#555** — undo v2's cheapest widening (the tracking/habit arms the
  v1 chunk classified as mechanically invertible). The classification
  table is done; this is ready wherever undo appetite sits.
- **UI features**: **#351** (GUI settings editor), **#411** (board
  selector dropdown for many boards), **#412** (window-geometry
  persistence) — no gate left; r2 + r3 landed.
- **#214** — effort-estimate calibration loop. Its "after Wave 2
  lands both fields" gate is now satisfied (effort field + time
  tracking); the first slice is pure projection surfaces.
- **#350** — app-data sidecar repair for cross-board card moves.
  Sized M, approach sketched in the issue (sibling-document lookup at
  fold time); it is the main remaining *reliability* gap for
  multi-board users.
- **#116** — Nextcloud major-version matrix for the docker tier. Its
  trigger ("once the client surface grows beyond the board slice")
  is now clearly met — and this week proved the lane's value. Natural
  bundle with the #436 lane work.
- **#194** — IPC disable knob; the issue's "when asked for" is the
  only gate; listed here so it isn't forgotten in the wait pile.

## 5. Wait — and why

**External / calendar / deferred by ruling** (do not schedule):
#440 (zbus 6 — gated on slint's atspi stack), #428 (Prusti ~2027 Q2),
#429 (quarterly churn re-measure), #444 (native toast actions —
explicitly not a v1 commitment, per-platform case-by-case), #494
(per-user attribution — Wave 4, no identity substrate), #219 (MCP
driver tooling — post-1.0 park), #114 (cargo-dist ≥ 1.0), #115
(aarch64 targets), #117 (self-hosted LAN runner), #113 (Snyk
remainder), #121 (Deck comments/admin endpoints), #122 (OAuth2).

**Trigger-gated — the trigger is written in the issue; none has
fired**: #131, #135, #137, #138, #139, #141, #142, #146, #148 (the
state/storage/perf family — all "only after measurement shows need";
the audit verified the current shapes as designed), #152 (needs a
Deck listing endpoint that doesn't exist), #153/#154/#155/#157 (sync
scaling — note #157's O(bound-cards) trigger is the *closest to
firing* now that kiosk boards grow into the thousands; revisit at the
next kiosk session), #167 + #198 (first external `--json` consumer),
#181 (report-apply `StorageFailed` — when a consumer needs the
broadcast seam), #127 (endpoint logging arms — next coverage gate or
client change), #180/#191/#224/#225 (REPL/uuid DX — demand-gated),
#193/#195 (IPC protocol v2 / multi-user — design when a consumer
appears), #220/#221/#222 (Retry-After refinements — additive; the
weekly lane is now the observation point; act if it ever logs a 429),
#133 (clock-skew — stays open with the documented trigger; #480's
pins will hold the semantics; **its F-133 fork needs the user's
ruling via #480**), #140 (storage naming — migration-locked, bundle
with the next schema migration).

**Needs a design round or usage evidence**: #213 (subtask hierarchy —
Wave 3 gate), #215 (workflow presets — waits for saved views/review
templates), #216 (description templates — rides the markup/parser
story), #217 (default next-task fill — its own design round; the
Wave-1 blockers cleared but the round hasn't run), #218 (weekly/
monthly review — rides the boundary-generic review-machinery design),
#184/#185 (the org-mode epic — behind the ADR 0010 vocabulary fork),
#327 (the capped app mutants one-off — explicitly chunk-coupled to a
future app-test-hardening chunk, not standalone), and the undo v2
family **#353, #354, #355, #356, #357** (each carries an explicit
"needs further consideration / real-usage evidence" ruling; #555
above is the ready exception).

## 6. Tracker hygiene candidates (flag; user action)

- **#166** — the body's own status block has said "DONE" since
  2026-10-11 (`taskboard status` over IPC shipped). Close it.
- **#231** — the snooze/mute preset bindings it asked for **landed**
  in the navigation/palette/menu round (commit `4c8d2657`;
  `keymap/presets.rs` binds `ReminderSnooze`/`ReminderMute` in
  standard (~:81), emacs (~:289), and vi (:474)). Verify the preset
  tests/snapshot went with it, then close.
- **#210** — the time-tracking core landed (track CLI, entries,
  storage, UI affordances) with all follow-ups carved into
  #490–#496. Verify nothing of #210's core scope remains, then close
  it with fences cross-referenced.
- **Milestone states**: all four milestones (`v0.1.0`,
  `remediation-wave-1`, `wave-kiosk`, `wave-daily-driver`) show 0
  open issues; the three delivered ones can be closed as milestone
  objects.
- **#436** closes via the tier-1 chunk's lane re-run (not hygiene —
  sequenced work).

## 7. Suggested dispatch shape (for the commissioning decision)

1. **Chunk A (P1, one branch, serial inside)**: #447 + #448 + #449,
   with #282 riding the tier-2 session; on green, re-run the weekly
   lane and close #436.
2. **Chunk B (P1)**: #462 — stage 1 picks virtualisation vs. clip
   (goldens question), and drafts the shared upstream slint report
   (#536's facts ride along).
3. **Chunks C/D (P2 perf, serial in the refresh pass)**: #463, then
   #464; #461 follows after B lands (shared mechanism).
4. **Chunk E (one-liner gate)**: #480 — needs only the F-133
   confirm-or-flip from the user.
5. **Chunk F**: the pty flake pass (#521/#524/#549 + old-watchlist
   dispositions).
6. **Filler lane** (any order, all independent): #450, #458, #407,
   #401, #201, #279, #280, the audit cleanup wave (re-verified).
7. **Product lane**: pick from §4 by appetite — the time-tracking
   fences and #352 are the most customer-visible.
8. **Hygiene**: the §6 closures, a two-minute no-branch task.

## 8. Commissioning record (2026-10-31, user rulings)

The user reviewed this report on 2026-10-31 and ruled:

- **F-133 / #480: Option A confirmed** — test-pack the skew exposure
  now; the Δ-band guard stays symptom-triggered. Rationale: *"I want
  to know if this is an actual problem first."* Recorded on #480;
  #480 is dispatchable as specified; #133 stays open with its
  documented trigger.
- **§4 product-lane picks**: **include** #352; **include the
  time-tracking fences** (#490, #491, #492, #493, #495, #496);
  **leave out the DR-6 follow-ups** (#504, #505, #506 — stay
  unscheduled); **include #555**; UI features: **include #412 only**
  (skip #351, #411); **include #214**, **#350**, **#116**, **#194**.
- **Otherwise**: the report's ready-now tiers 1–4 stand as
  recommended (tier 4's flaky pass and audit cleanup wave included).
- **Tracker hygiene executed 2026-10-31**: #166, #231, #210 closed
  with evidence comments; milestones `remediation-wave-1`,
  `wave-kiosk`, `wave-daily-driver` closed as objects (`v0.1.0` left
  open as the release marker). #436 closes via the tier-1 chunk's
  lane re-run, not hygiene.
- The commissioned set is dispatched by
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`.

Note on a §4 slip: #412 (window-geometry persistence) was listed in
§4 but absent from §7's product-lane examples; the user's inclusion
ruling above is the operative one.

Everything else stays put; its issue text already owns the trigger.

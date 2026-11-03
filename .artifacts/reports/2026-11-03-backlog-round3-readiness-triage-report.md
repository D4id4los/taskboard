# Backlog Round 3 — Readiness Triage

- **Date**: 2026-11-03
- **Kind**: report — a full re-triage of the GitHub issue tracker after
  backlog round 2 landed in full and GUI review round 4 reached chunk 9,
  classifying every open issue into **ready now** vs. **decision point**
  vs. **stays open with a recorded reason**, with a suggested dispatch
  shape. Commissioned for the user's review ahead of the round-3
  dispatch plan; no code, no tracker changes.
- **Goal (user-stated, differs from round 2)**: after round 3 (plus the
  in-flight round-4 tail) there should be **no currently tackleable
  backlog issue left open** — every remaining open issue must carry a
  documented reason it is not tackleable now (external gate, unfired
  trigger, design gate, or watchlist policy) before the next wave
  milestone planning starts.
- **Method**: the 82 open issues diffed against the 2026-10-31 triage's
  110-issue set (42 closed since — the round-2 chunks and hygiene; ten
  new: #560, #570, #572, #580, #588, #590, #592, #595, #599, #600);
  bodies of all new and re-classified issues read in full; disposition
  comments on the round-2 leftovers (#282, #287, #336, #384, #524)
  read; `origin/main` since 2026-11-01 reviewed (round-2 batch 6 +
  round-4 chunks 1–8 landed; no open PRs — chunk 9 in progress);
  targeted premise checks on the current tree (`cli_habits.rs:125`
  UTC oracle still present; migrations at 0019; `cascade_board`
  untouched). Old-issue classifications are re-derived only where the
  week's landings changed the premise.

## 1. What changed since the 2026-10-31 triage

- **Backlog round 2 landed in full** (all 25 non-absorbed chunks,
  batches 1–6): the anchor cluster (#447/#448/#449, #436), the render
  lane (#463, #464, #462+#536, #461), the small tier (#480, #450,
  #458, #407, #401, #201), the pty flake pass (#521/#524/#549 fixed
  with root cause; #287/#336/#384 re-triaged, no recurrence), the
  audit cleanup wave (#270–#273, #275, #276, #279, #280), and the
  product lane (#194, #116, #555, #412, #493+#495, #492, #350, #496,
  #214).
- **GUI review round 4 is at chunk 9 of 11** — chunks 1–8 merged
  (board surface, column resize, icons, hover, taskview layout+focus,
  datetime entry, task reminders + migration 0018). Remaining: chunk 9
  (timer surfaces, **#352**), chunk 10 (tracking surfaces,
  **#490/#491**), chunk 11 (settings connections, **#411** adoption).
  These four issues are **owned by round 4** and are out of round 3's
  scope.
- **The round-2 leftovers have recorded dispositions, not debt**: #282
  was wire-verified on the rig (403 ⇒ retryable strand confirmed — the
  fix is a real production change, deliberately fenced); #287/#336/
  #384 stay open by watchlist policy (no recurrence ⇒ no speculative
  fix); #524 was root-cause-fixed but has **two fresh post-fix
  observations** (2026-11-02 PR #586 coverage run, 2026-11-03 chunk-7
  gate run) whose signature plausibly matches **#572**'s production
  race in the sibling test.
- **Ten new issues**: one p1 product bug (#572, found by the flake
  pass's progress-deadline seam), three watchlist observations (#560
  deterministic local-vs-UTC window, #570 relay export race,
  #524-recurrences above), and six follow-up fences carved out of
  landed round-2 chunks (#580 storage codec, #588 break-rule roaming
  p2, #590 taskview field tooltips, #592 ListView anchoring polish,
  #595 install_seam! macro, #599 StartTracking kind-awareness, #600
  focus-mode follow-ups).
- **Round-2's §4 product picks all landed**; of that bucket only #351
  (user-skipped then) and the DR-6 follow-ups #504/#505/#506
  (user-left-out then) remain open — round 3's drain goal re-surfaces
  them as decision points. **#426 (Kani pilot) is closed** (2026-10-26),
  so #505's stated gate is satisfied.

## 2. Verdict summary

| Bucket | Count | Issues |
|---|---|---|
| Owned by round 4 (in flight) | 4 | #352, #490, #491, #411 |
| Ready now — tier 1 (p1 product bug) | 1 | #572 |
| Ready now — tier 2 (p2) | 1 | #588 |
| Ready now — tier 3 (small independents) | 5 | #282, #599, #560, #570, #327 |
| Ready now — tier 4 (M-sized) | 4 | #580, #590, #595, #351 |
| Ready now — re-confirm under drain goal (user skipped in r2) | 2 | #505, #506 |
| Fixed, pending re-triage via #572 | 1 | #524 |
| Decision point — include-with-shape or defer | 3 | #504 (design-first), #592 (measure-first), #600 (recommend defer) |
| Watchlist — not tackleable until recurrence (policy) | 3 | #287, #336, #384 |
| Registers — verify-then-keep hygiene | 5 | #274, #277, #284, #285, #286 |
| Wait — external/calendar/deferred by ruling | 12 | #440, #428, #429, #444, #494, #219, #114, #115, #117, #113, #121, #122 |
| Wait — trigger-gated (trigger written in the issue, unfired) | 29 | #131, #133, #135, #137, #138, #139, #140, #141, #142, #146, #148, #152, #153, #154, #155, #157, #167, #180, #181, #191, #193, #195, #198, #220, #221, #222, #224, #225, #127 |
| Wait — design round / usage evidence | 12 | #213, #215, #216, #217, #218, #184, #185, #353, #354, #355, #356, #357 |

Total: 82. Notes on shape: #590/#595/#351 are ui-territory chunks
that must serialize behind round-4 chunks 10/11 (task-detail and
settings surfaces are still churning `adapter.rs`/`models.rs`);
#570 is triage-shaped (first observation, no repro) and #327 is
tackleable only as the opening move of a dedicated app-test-hardening
chunk (the issue's own coupling) — both included in tier 3 as such.

Rough shape: **13 issues are cleanly dispatchable now** (tiers 1–4
plus the two re-confirms), **3 need one user call each**, and the
remaining 62 stay open (or close via round 4 / #572's rider) for
reasons that were deliberate when set — each now carries an explicit
reason line so the drain goal's test ("nothing tackleable left") is
auditable at wave-planning time.

## 3. Ready now

### Tier 1 — the p1 product bug

- **#572 (p1, product)** — rare REPL wedge: after a ^C during the
  masked read, the select-cancel race leaves the session without
  re-entering `readline()` — the editor never re-arms, zero pty bytes
  forever. Found by the flake pass's progress-deadline seam (a typed,
  evidenced failure, not a harness artifact); reproduction recipe and
  suspect area are written in the issue. **Rider**: re-triage #524's
  two post-fix observations (2026-11-02/03, both "no pty output for
  30 s" under load) against the fix — the same-mechanism hypothesis is
  plausible (the sibling test shares the masked-read ^C scene); if
  confirmed, #524's recurrences are attributed and it closes; if not,
  they stay captured per policy. This chunk is round 3's first
  dispatch.

### Tier 2 — p2 architecture

- **#588 (p2, architecture)** — coherent roaming of break-rule config
  and break dismissals, fenced from chunk 3.5 with the recipe written
  down: a board-level `break-rules` sidecar section (the
  reminder-rules recipe), widening the dismissals wire grammar to
  `break:<uuid>` keys, and the roaming-id carve-out in the
  `no_local_ids_reach_the_dismissals_wire` proptest. Section-recipe
  sized; touches domain/state + the sync sidecar grammar (sync-crate
  lane).

### Tier 3 — small, well-specified

- **#282 (p3, reliability)** — dead-board drain liveness. The tier-2
  rig verified the premise (writes on a confirmed-dead board answer
  403 ⇒ retryable strand; only `BadRequest` dead-letters), so the fix
  direction is confirmed: symmetric `cascade_board` cancellation or a
  write-side dead-board carve-out in the executor classification.
  Domain pipeline + sync executor.
- **#599 (refactoring)** — make `StartTracking`'s idempotent no-op
  kind-aware (same task + different kind ⇒ close-and-open, per chunk
  3.5's D-3 intent). Small domain/state change with a documented
  proposal; a landed-behavior fix in the respite family's scope.
- **#560 (flaky, deterministic)** — the habits daily-window oracle
  asserts the UTC date while production resolves the local calendar;
  fails deterministically whenever the local date leads UTC (00:00–
  01:00 CET). Verified still present at `cli_habits.rs:125`. The
  chunk rules the contract (local vs UTC — the week's due-grammar
  work points local, but the ruling is the chunk's to record) and
  fixes oracle or production accordingly; no weakening.
- **#570 (flaky, triage)** — first observation of the relay export
  instance-locked race; bounded triage only (repro attempt, capture
  policy, disposition) — pairs naturally with #560 as a small
  watchlist pass; the recurrence scans over #287/#336/#384 ride the
  same session (they are otherwise untouched this round).
- **#327 (p2, ci/testing)** — the capped cargo-mutants one-off for
  `taskboard-app` (`commands/` + `ipc/` first), explicitly coupled to
  "the future app-test-hardening chunk": run the sweep fresh, triage
  survivors, fix or fence each (the #427/#458 precedent). Tackleable
  as that chunk's opening move; M-sized including survivor triage.

### Tier 4 — M-sized refactors/features

- **#580 (p3, refactoring)** — storage codec text-form unification
  (F5-10, re-verified 2026-11-02): collapse the hand-written TEXT
  forms to the `ReminderRef` delegation pattern and fold the 18×
  clock-parse helper. Storage crate; no migration expected.
- **#590 (p3, ui)** — hover tooltips for the task view's text-field
  sites (r4 D21 remainder); the planned TextField tip arm proved
  infeasible on the pinned runtime, the likely mechanism is a
  settings-`LabeledField`-shaped wrapper. **After round-4 chunk 10**
  (task-detail territory).
- **#595 (p3, refactoring/dx)** — `install_seam!` macro over the 110
  adapter seam installs (F7-08's trigger fired ~9× over). Mechanical
  M/L over the crate's most-churned file; stage 1 re-derives the
  call-shape catalogue and re-adjudicates the standing
  generics-vs-macro rationale. **After round-4 chunk 11 merges** —
  the file must be quiet.
- **#351 (ui/dx)** — GUI settings editor. Unblocked and S–M on the
  house scale (typed config-write port generalizing the logout write
  path; figment precedence honored honestly; secrets read-only).
  User-skipped in round 2's product picks; round 3's drain goal
  re-surfaces it — include unless re-declined. **Territory note**:
  the settings surface overlaps round-4 chunk 11 — schedule behind it.

### Re-confirms (user left these out of round 2; drain goal says pick or record a reason)

- **#505 (p3, testing)** — Kani harnesses for the recurrence
  arithmetic. Its gate (#426 pilot) **landed 2026-10-26**; the
  pilot's conventions exist. Small, self-contained.
- **#506 (p3, ui/ux)** — habit UX trio (convert verb, series pause,
  streak repair). Three small well-specified items over already-
  landed seams; bundle or split at dispatch.
- (#351 above is the third of these.)

## 4. Decision points (the user calls; round 3 waits on none of them, but the drain goal wants a recorded answer)

- **#504 (p3, architecture)** — tracker reminders (`ReminderRef::
  Tracker`, occurrence semantics, roam ruling, ladder interplay). The
  genuinely design-flavored DR-6 follow-up: four design rulings are
  named in the issue and are the user's to confirm. Either (a)
  include in round 3 as a design-first chunk (stage 1 surfaces the
  rulings, user confirms, then implement), or (b) defer to the wave
  milestone planning where it composes with the reminder/roam theme
  (#588's dismissals-wire widening is adjacent). Recommendation:
  **defer to wave planning** — it is product shape, not debt, and
  round 3 already carries the reminder-roaming seam work via #588.
- **#592 (p3, performance/ui)** — optional row-anchored scroll
  anchoring for above-viewport ListView changes. Stage 3 of #461
  **accepted content-anchoring as the contract**; this issue is an
  optional polish with an explicit measure-first gate (the
  compensation may fight the runtime's own re-anchoring). Either (a)
  include as a measure-first chunk that ends in implement-or-close-
  with-evidence, or (b) record it as optional polish deferred to the
  kiosk/UX theme. Recommendation: **(b) defer** — the contract is
  accepted and pinned; a measurement pass with no user pain behind
  it is exactly what wave themes are for.
- **#600 (enhancement)** — focus-mode follow-ups (auto-chain/long-
  break cadence; phase-boundary notifications). One half needs a
  per-session-memory design ruling, the other is explicitly gated on
  notification-template machinery that does not exist yet. Fresh v1
  fence from today. Recommendation: **defer to wave planning** (it is
  one coherent design question about the focus runtime's future, and
  the pomodoro v1 just landed unsoaked).

## 5. Stays open — with the reason on record

- **Watchlist policy (not tackleable until recurrence)**: #287,
  #336, #384 — re-triaged 2026-11-01 with negative repro protocols
  and capture policies; the house rule is no speculative fix without
  a repro. They stay open as the archive; the round-3 watchlist pass
  (#560/#570) re-scans for recurrences. #524 likewise stays open
  until #572's fix re-triages its two post-fix observations.
- **Registers (verify-then-keep)**: #274 (sync), #277 (app), #284
  (security), #285 (performance), #286 (domain) — these are living
  accepted-risk records with named triggers, not work items.
  Hygiene action for round 3's bookkeeping: a bounded verification
  pass over each register's triggers, because events have moved
  since 2026-10-17 — the kiosk wave landed (relevant to #285's soak
  census and #277's Kiosk-mode verdict), round-2/r4 churned the app
  crate (relevant to #277's split triggers), the sync sweep-scope
  F1-03 fix landed (#274's ignore-list rider). Update verdicts that
  fired; close a register only if every recorded trigger is consumed
  or migrated to fresher homes. No branch required.
- **External / calendar / deferred by ruling (do not schedule)**:
  #440 (zbus 6 — gated on slint's atspi stack), #428 (Prusti
  ~2027 Q2), #429 (quarterly churn re-measure), #444 (native toast
  actions — not a v1 commitment), #494 (per-user attribution —
  Wave 4), #219 (MCP driver tooling — post-1.0 park), #114
  (cargo-dist ≥ 1.0), #115 (aarch64 targets), #117 (self-hosted LAN
  runner), #113 (Snyk remainder), #121 (Deck comments/admin
  endpoints), #122 (OAuth2).
- **Trigger-gated (the trigger is written in the issue; none has
  fired)**: the state/storage/perf family #131, #135, #137, #138,
  #139, #141, #142, #146, #148 (measurement-gated; the audit
  verified current shapes as designed — round 2's render lane was
  the measured part of this family and it is done); #152 (needs a
  Deck listing endpoint that doesn't exist), #153/#154/#155 (sync
  scaling), **#157** (O(bound-cards) pull refresh — still the
  closest to firing now that kiosk boards grow into the thousands;
  explicitly re-flagged for the wave planning's kiosk theme), #167
  + #198 (first external `--json` consumer), #181 (report-apply
  `StorageFailed` — consumer-gated), #127 (endpoint logging arms —
  next coverage gate or client change), #180/#191/#193/#195/#224/
  #225 (REPL/IPC DX — demand-gated; #193/#195 design when a consumer
  appears), #220/#221/#222 (Retry-After refinements — additive; act
  if the weekly lane ever logs a 429; none recorded), #133
  (clock-skew — documented trigger; #480's pins hold the semantics),
  #140 (storage naming — migration-locked; the bundle target is now
  **0020**, since 0018 (respite) and 0019 (tracker kind) both landed
  without it).
- **Needs a design round or usage evidence**: #213 (subtask
  hierarchy — Wave 3 gate), #215 (workflow presets — waits for saved
  views/review templates; note #214's review view + phrases just
  landed, lowering this barrier), #216 (description templates —
  rides the markup/parser story), #217 (default next-task fill —
  its own design round), #218 (weekly/monthly review — rides the
  boundary-generic review machinery that now has a first instance),
  #184/#185 (org-mode epic — behind the ADR 0010 vocabulary fork),
  and the undo v2 family #353, #354, #355, #356, #357 (each carries
  an explicit real-usage-evidence ruling; #555 was the ready
  exception and it landed).

## 6. Suggested dispatch shape (for the commissioning decision)

1. **Chunk A (P1, first)**: #572 — root-cause and fix the post-^C
   editor re-arm race; rider: re-triage #524's two post-fix
   observations against the fix (attribute or keep captured).
   App-crate cli/tty path — no conflict with round-4's ui work.
2. **Chunk B (P2)**: #588 — break-rule roaming (domain/state +
   sync sidecar grammar; sync-crate lane).
3. **Small independents (any order)**: #599 (domain/state —
   serialize against B on the domain source), #282 (domain pipeline
   + sync executor — sync lane), the **watchlist pass** (#560 fix +
   #570 triage + recurrence scans over #287/#336/#384), #505 (Kani
   harnesses, domain tests only), #506 (habit UX trio).
4. **M lane**: #580 (storage), #327 (app mutants one-off → app-test
   hardening), #351 (settings editor — **after round-4 chunk 11**),
   #590 (taskview tooltips — **after round-4 chunk 10**), #595
   (install_seam! — **after round-4 chunk 11**; the file must be
   quiet).
5. **Hygiene (no branch)**: the register verification pass over
   #274/#277/#284/#285/#286.
6. **Explicit defer rulings to record** (if accepted): #504, #592,
   #600 → wave milestone planning, each with the reason line from §4.

After round 3 and the round-4 tail, the open-issue set is exactly:
4 closed-by-round-4, 3 watchlist-by-policy, ≤5 registers, 12
external, 29 trigger-gated, 12–13 design/usage-gated — every entry
carrying its reason. That satisfies the drain goal's test.

## 7. Commissioning record

The user reviewed this report on 2026-11-03 and ruled:

- **§4 deferrals approved**: #504, #592, #600 all deferred to the next
  wave milestone, **`v0.2` "focus & reflection"** (milestone #6,
  created for the purpose). All three are assigned to it on the
  tracker and carry a ruling comment citing this report.
- **The ready set stands as recommended** (tiers 1–4 plus the two
  re-confirms #505/#506): 12 chunked issues + the #524 rider + the
  register-hygiene bookkeeping, dispatched by
  `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md`.
- After round 3 and the round-4 tail, every remaining open issue
  carries its §5 reason line — the drain goal's test is satisfied by
  construction and auditable at v0.2 planning time.

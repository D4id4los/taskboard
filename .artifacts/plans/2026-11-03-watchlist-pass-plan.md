# [Plan] The November Watchlist Pass (backlog round 3, chunk 1.6)

Chunk 1.6 of the backlog round 3 dispatch
(`.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md` §2, chunk
1.6): rule the #560 habits daily-window contract, triage the #570
relay-export race within bounds, and run the standing recurrence scans
over #287/#336/#384. Branch `test/watchlist-pass-2026-11`, worktree
`../taskboard-WT-test-watchlist-pass-2026-11`.

**Scope note written after a world-change on the plan day**: the h02
oracle fix this chunk was expected to produce **already landed on
`origin/main`** — commit `396bd6e3` ("test(app): the habit window pins
the local day calendar, bracketed", 2026-11-04, riding PR #609 via the
duplicate-observation issue #606, now closed). #560 itself is still
open and unruled. This chunk therefore *verifies and records* rather
than *fixes*; §1.6 spells out what remains and why that satisfies the
dispatch acceptance criterion ("the fix lands … with a pin that fails
under the wrong calendar" — it landed, on main, before the branch
opened; this chunk proves it load-bearing and closes #560 with the
ruling).

## 0. Gate verification (plan day, all open)

- `scripts/worktree-doctor.sh` → exit 0, free 216 G (floor 25 G). One
  KEEP-listed worktree (`feat-ui-r4-tracking-surfaces`, never pushed) —
  untouched.
- Worktree created per AGENTS §8:
  `git worktree add -b test/watchlist-pass-2026-11
  ../taskboard-WT-test-watchlist-pass-2026-11 origin/main` → branch at
  `396bd6e3` (the landed h02 pin, see §1.6).
- Issue states at plan time: #560 **open, 4 comments, no ruling**;
  #570 **open, 0 comments** (first observation only); #606 closed
  (fixed by #609); #287/#336/#384 open with the 2026-11-01 re-triage
  comments as their latest records.

## 1. The #560 contract ruling — the natural daily window's calendar

### 1.1 The question

`log_window` (taskboard-domain/src/habit.rs:418) attributes a habit +1
to the "natural" window: `WindowStart::of_period(kind, day.today(now))`
over an injected `DayCalendar { day_start_time, offset }`. Which
calendar does `today` mean for a habit user: the **device-local**
calendar (offset from the OS timezone, resolved per derivation) or the
**UTC** calendar (offset fixed at zero)?

### 1.2 The local-calendar reading

The day vocabulary answers this at the type's own documentation layer:

- `days.rs` D-F6 consumer contract: *"App bootstrap parses
  `[preferences] day_start_time` once … and resolves **the device's
  fixed offset** from the OS timezone at derivation time."*
  `AppConfig::resolved_day_calendar` (config.rs:1431) implements it
  verbatim: `offset = *chrono::Local::now().offset()`.
- `DayCalendar.offset` doc: *"The device's fixed UTC offset, resolved
  once at boot."*
- The night-owl rule only exists because the user's *lived* day is the
  subject: `day_start_time` shifts the boundary so 03:00 local can stay
  inside yesterday's window (days.rs D7b). On a UTC calendar every
  `day_start_time` knob decouples from the user's day; a +02:00 user's
  00:30 habit log would be filed under the wrong date every single
  night.
- The whole feature vocabulary — 24 h grace, streak "morning of an
  unmet today still shows yesterday's run", the D-C8 ADHD-forgiveness
  make-up window — is phrased in days the user actually lives.

### 1.3 The UTC alternative (recorded, rejected)

A UTC window would make receipts deterministic against `Utc::now()`
oracles and identical to CI runners — **convenience for the test
suite**, not semantics for the user. It contradicts D-F6's written
contract, breaks the night-owl knob for every non-UTC user, and would
ripple: streak/grace arithmetic, the day-roll engine config
(`day_calendar()`), and the UI's own derivation all read the same
calendar. Rejected on semantics, per the dispatch mandate ("the
user-visible semantics argument, not convenience").

### 1.4 The house's local-time direction (evidence)

- `c228159f` (r4 D28): the due grammar speaks local time — parse, echo,
  picker, validity probes all resolve through an injected local
  TimeZone seam; *"the display finally tells the truth about when a
  task is due"*. The habit window is the same class of user-facing
  "when" question.
- `3aa7cb96`: the golden harness pins its *process* to UTC so local
  contracts stay machine-independent — i.e. the house pattern is "the
  app speaks local, the test pins the zone it needs".
- `log_window`'s doc: attribution *"is the window choice of the +1 …
  the night-owl rule"* — local wording throughout.

### 1.5 The ruling

**The natural daily window is the device-local calendar's today (D-F6:
midnight rollover on the device's fixed UTC offset). Production is
correct; the pre-`396bd6e3` test oracle was the wrong-calendar side.**
This was the implicit reading of the landed fix (see below); this plan
makes it the recorded contract ruling that #560 asked for.

### 1.6 State of the world — the fix already landed; what this chunk does

PR #609's test-only commit `396bd6e3` (now on `origin/main`, riding the
#572 chunk per its stage-3 review instruction) rewrote h02's oracle:

- expected window derived through the same vocabulary production uses —
  `DayCalendar { day_start_time: NaiveTime::MIN, offset:
  *chrono::Local::now().offset() }`, `day.today(Utc::now())`;
- the log call is **bracketed** (`before`/`after` reads, assertion
  `window ∈ {before, after}`), so a day-roll between the oracle's read
  and the command's read cannot false-fail;
- the `adjust` step reuses the receipt's own window instead of a second
  clock read.

Verified green by its author during the exact 00:00–01:00 local window
that failed deterministically before, and by CI on #609.

**This chunk's remaining #560 work** (decisions D1–D3):

1. **Record the ruling** — this section is the record; stage 3 closes
   #560 with a dated ruling comment (draft in §3.4) linking the plan,
   `396bd6e3`, PR #609, and #606 (the closed duplicate-observation
   issue, which stays closed as the archive).
2. **Verify the landed pin is calendar-load-bearing** — the dispatch
   flip-proof mandate, executed against the landed code (protocol in
   §1.8; the transcript recorded in the stage-2 report and referenced
   from the #560 closing comment).
3. **No re-fix, no weakening** — stage 2 does not touch h02 unless the
   flip-proof exposes a gap (§1.9's notes tell stage 3 what is
   deliberate, so it doesn't flag them as findings).

### 1.7 The counterfactual fix shape (for the record; NOT taken)

Had the ruling been UTC, the fix would have been **production**: swap
`resolved_day_calendar`'s offset source (`chrono::Local::now()` → a
UTC-resolved offset), audit the UI's own derivation and the day-roll
config for the same seam, pin the boundary shape (the 00:00–01:00
straddle) with a zone-seam test, and CHANGELOG the behavior change —
and per the dispatch's lane note, that fix **serializes behind chunk
1.2's merge** (`feat/break-rule-roaming`, domain/state lane). No
production change is taken under the local ruling, so **the 1.2 gate is
not engaged** for this branch (D2). This paragraph is the explicit
serialization record the dispatch asked stage 1 to make.

### 1.8 The flip-proof protocol (binding for stage 2)

Mandate: "the pin must fail under the wrong calendar: prove it by
temporarily flipping the oracle's calendar and observing the failure,
then restore." Against the landed bracket oracle, the deterministic
procedure (no waiting for the 00:00–01:00 window):

1. Temporarily edit h02's oracle `DayCalendar` to the **wrong
   calendar** — the original #560 shape: `offset:
   chrono::FixedOffset::east_opt(0).unwrap()` (UTC) — leaving
   production untouched. Commit nothing; this edit is transient.
2. Pick a hostile process zone **guaranteed to straddle at the current
   instant**: with `h = date -u +%H`, if `h ≥ 12` use `TZ=UTC-14`
   (POSIX sign inversion: local = UTC+14, local date leads for UTC
   ≥ 10:00), else `TZ=UTC+11` (local = UTC−11, local date lags for
   UTC < 11:00). Either choice covers every instant of the day.
3. Verify the straddle *before* the run and record both dates:
   `date -u +%F` and `TZ=<picked> date +%F` must differ; assert that
   fact in the proof record.
4. `TZ=<picked> cargo nextest run -p taskboard-app --test cli_habits
   h02_log_receipts_name_window_and_count_and_adjust_corrects` — expect
   the typed failure *"the natural daily window is the local-calendar
   today"* with receipt = local date ≠ oracle = UTC date, exactly the
   #560 signature. Save the verbatim output into the stage-2 report.
   This simultaneously proves the bracket does **not** mask a wrong
   calendar (it only absorbs time-passage) and that the assertion is
   load-bearing.
5. Restore the oracle (`git checkout -- crates/taskboard-app/tests/
   cli_habits.rs`), re-run h02 plain → green.

Do not approximate the hostile zone with a real IANA name whose offset
depends on season; the fixed POSIX offset makes the straddle provable
from the recorded dates alone.

### 1.9 No-weakening ledger for the landed pin (stage-3 review aid)

- **The bracket tolerates two adjacent *todays*, never two
  *calendars*.** The calendar is fixed by the oracle's `DayCalendar`
  construction (same offset source as production); `{before, after}`
  absorbs only the day rolling between reads. The flip-proof (§1.8)
  demonstrates the calendar is load-bearing. AGENTS §5's
  "no weakening to tolerate both calendars" is honored.
- **The oracle hardcodes `day_start_time: NaiveTime::MIN`** while its
  comment claims "the same calendar `resolved_day_calendar` builds".
  That holds because the h-series exercises the standalone *default*
  config, whose `DEFAULT_DAY_START_TIME` is midnight (config.rs:61).
  Kept as landed (D8): parsing the config's string in the test would
  couple the harness to config internals for zero behavioral gain; a
  default change is a coupled change that must revisit this test
  anyway.
- **No TZ pin is needed post-`396bd6e3`** (D9): the oracle reads the
  same `Local` offset production reads, and the bracket absorbs the
  only remaining race. This supersedes the golden-harness-style
  `set_var` pin this plan originally sketched — strictly better: no
  `unsafe` in an app test crate, no dependence on nextest env-injection
  behavior.
- **Residual**: production reads its own clock inside the bracket;
  `window` must be one of the two reads. No construction exists where
  a wrong calendar passes. If h02 ever fails again post-`396bd6e3`,
  that is new evidence → back to the watchlist, never tuned away.

## 2. #570 — the relay-export instance-locked race (bounded triage)

### 2.1 The seam (plan-day code read, mechanism unconfirmed)

`t16_track_and_export_over_the_relay` (cli_ipc.rs:688) spawns a daemon,
runs the `track` family over the socket, then `export --format csv`.
The 2026-11-01 observation (PR #569, docs-only, CI run 41328813021):
the final export failed typed `instance-locked` → core bootstrap
failed, exit 1 — i.e. the one-shot verb **fell through to the
in-process boot** while the daemon was up and holding the instance
lock.

The transport auto-routing lives in `try_connect`
(ipc/client.rs:122): on `connect()` error kind `NotFound` /
`ConnectionRefused` / `PermissionDenied` it returns `None` *immediately*
("no artifact / stale artifact / not ours — fall through now"); only
other error kinds retry within `CONNECT_BUDGET`. The unconfirmed
hypothesis: under full-suite load a live daemon's listener can
transiently refuse (accept-queue pressure — the same environmental
family as the #287 loopback hypothesis), `try_connect` classifies the
refusal as "no daemon", and the one-shot fallback boots a core into the
held lock. A competing shape (listener not yet bound when the export
connects) is unlikely here: the export runs well after
`wait_ready()` and several successful relayed commands — the failure
came after the relay demonstrably served traffic. Capture must
distinguish these; neither is confirmed.

### 2.2 Repro protocol (bounded; binding for stage 2)

Load shape — the #572 recipe (issue #572, "Reproduction recipe"),
adapted to this test:

1. From the worktree, start bounded spinners holding sampled load ≈ 2×
   nproc: `n=$(nproc); for i in $(seq $((2*n))); do yes >/dev/null
   2>&1 & done` — record the pids for teardown. Plus **one** parallel
   `cargo build --workspace` in a scratch checkout (not this worktree)
   left running for the first ~10 iterations.
2. Loop, one iteration at a time (no parallel test processes — the
   daemon-under-test must be the only contention victim):

   `cargo nextest run -p taskboard-app --test cli_ipc
   t16_track_and_export_over_the_relay`

   The nextest filter runs t16 alone, so sibling pty flakes cannot
   contaminate the record.
3. **Bound**: 30 iterations or 90 minutes wall, whichever first
   (D5). Record which stopping rule fired. ~1/25 rarity (#572's
   measured shape) makes 30 a reasonable single-pass sample; a negative
   result is a *bounded negative*, never "does not exist".

### 2.3 Capture policy (before ANY re-run)

On failure, record before touching anything else (the house policy,
#287's thread):

1. the full nextest failure section verbatim — the exit-code assert
   line and both `{out}`/`{err}` payloads (the `instance-locked`
   envelope is the signature, but the *preceding* stderr lines are the
   evidence: a transport/tracing line from `try_connect` vs. silence
   distinguishes the §2.1 hypotheses);
2. the install scratch dir path from the output (the socket sidecar and
   daemon log if present) — preserved until the issue comment is
   posted, then cleaned;
3. lane (loaded-loop), load shape (spinner count, sampled `uptime`
   load, parallel build y/n for that iteration), commit.

### 2.4 Disposition (D5)

- **Reproduced** → post the evidence on #570 with the §2.3 record and
  the §2.1 hypothesis it confirms/refutes. Root-cause + fix is its own
  future chunk (client resilience to a transient local refusal is a
  design question — whether the fall-through should retry-refused
  briefly, or bootstrap should yield to a live daemon — not a test
  patch). **No speculative fix in this chunk** (dispatch hard rule;
  the #384 precedent: no fix without repro — and even a repro only
  earns the evidence, the design call stays a separate decision).
- **Bounded negative (30 iterations green)** → post the bounded-negative
  record (exact iteration count, load shape, commit) + the §2.3 capture
  policy as #570's standing policy (mirroring #336's annotation shape).
  Stays open. One clean pass is not recurrence-free evidence for a
  never-captured, once-observed race.
- Either way: **close only if root-caused and fixed** (dispatch
  acceptance). Realistically this chunk leaves #570 open with evidence.

## 3. Standing recurrence scans — #287 / #336 / #384

### 3.1 Protocol (binding for stage 2)

1. **Thread read first** (the thread is the primary record):
   `gh issue view 287|336|384 --comments` — anything after the
   2026-11-01 re-triage comments is a new observation; triage it per
   that issue's own capture policy before scanning runs.
2. **Bounded run scan** since the last scan's window:
   `gh run list --created ">=2026-11-01" --limit 100` (the 2026-11-01
   pass scanned 2026-10-31 → 2026-11-01 with `--limit 30`; three days
   of CI at this repo's cadence fits comfortably in 100). Failures →
   `gh run view <id> --log-failed`, grep for the family signatures:
   #287 → `actor_wiremock` (a10/a12/`create_stack_transport_failure`),
   #336 → `delete_404_re_resolves_and_deletes_at_the_fresh_binding`,
   #384 → `l13_alias_happy_path_writes_stderr_connect_keeps_it_empty`.
   Local gate runs are out of scope for `gh run list` — a local
   observation arrives as an issue comment and is handled in step 1.
3. **Comment format** (one dated comment per issue; reuse each issue's
   standing capture policy by reference, never re-stated so policies
   cannot drift apart):

   > **Re-triage 2026-11-04 (watchlist pass, backlog round 3 chunk
   > 1.6). <one-line recurrence verdict over the 2026-11-01 → 2026-11-04
   > window, with run ids for any hit — or "no recurrence; the
   > 2026-10-26 negative repro protocol stands and has not been
   > re-spent">. Capture policy: unchanged, per the <date> comment
   > above. Ruling: no code change — <no repro / stays open>.*

4. **Rulings absent new evidence** (D6): all three stay open, no code
   change — the 2026-11-01 reasoning stands (two quiet weeks is not
   recurrence-free evidence for #287/#336; #384's next occurrence is
   high-signal since the assert now carries the buffer bytes).
   Scanning never deletes or rewrites an existing capture policy
   (stage-3 check).

### 3.2 #560/#570 comment drafts (stage 3 posts after gates are green)

- **#560 (closing comment)**: the §1.5 ruling verbatim + links: this
  plan, `396bd6e3` / PR #609 ("test(app): the habit window pins the
  local day calendar, bracketed"), #606 (closed duplicate,
  stays archived), the §1.8 flip-proof transcript (dates + failure
  output), and the note that pre-fix observations on this thread were
  the same mechanism as #606's.
- **#570**: per §2.4 — evidence or bounded-negative record + capture
  policy; stays open either way unless root-caused and fixed in-chunk
  (not expected).

## 4. Decision record (every open question, resolved)

- **D1 — #560 ruling**: the natural daily window is the **device-local
  calendar's today** (D-F6). Production correct; the old test oracle
  was wrong-calendar. Semantic grounds (§1.2–1.4), not convenience.
- **D2 — 1.2-serialization**: not engaged — the local ruling needs no
  production change; the UTC counterfactual's serialization record is
  §1.7.
- **D3 — chunk deliverable for #560**: ruling record (this plan + the
  closing comment) + landed-pin verification (flip-proof §1.8). No
  h02 edits unless the flip-proof fails.
- **D4 — flip-proof mechanics**: hour-picked fixed POSIX TZ
  (`UTC-14` / `UTC+11`), dates verified pre-run, oracle flipped to UTC
  offset, failure observed, restore. §1.8 is binding.
- **D5 — #570 protocol**: 30 loaded iterations (or 90 min), #572 load
  shape, capture-before-rerun (§2.3), disposition per §2.4, no
  speculative fix.
- **D6 — scans**: thread-first, then `gh run list --created
  ">=2026-11-01" --limit 100`; dated comment per §3.1(3); all three
  stay open, policies untouched.
- **D7 — issue closures at stage 3**: #560 closes (ruling + evidence +
  PR links). #570 closes only if root-caused and fixed (not expected).
  #606 stays closed. #287/#336/#384 get scan comments only.
- **D8 — the landed oracle's `NaiveTime::MIN` hardcode**: kept (§1.9).
- **D9 — no TZ pin / no `set_var` in app tests**: the bracket design
  supersedes it (§1.9).
- **D10 — CHANGELOG**: none — no production change lands on this
  branch (the only production-adjacent change, `396bd6e3`, already
  landed via #609 with its own changelog handling).

## 5. Stage-2 work order (precise)

1. Read AGENTS.md, the five issues (+ #606, #572), the dispatch §1/§2
   chunk 1.6, and this plan in the worktree; recreate the worktree if
   missing.
2. Verify the branch contains the landed pin (rebase onto origin/main
   per dispatch §1 step 3 if behind; `396bd6e3` or later).
3. Execute §1.8 (flip-proof) and record the transcript.
4. Execute §2.2–§2.4 (#570) and record the outcome.
5. Execute §3.1 (scans) and draft the per-issue comments (stage 3
   posts).
6. Full AGENTS §10 gate suite via `scripts/run-quality-gates.sh`.
7. No push, no PR (stage 3 ships), CHANGELOG untouched (D10).

## 6. Acceptance checklist (maps to dispatch chunk 1.6)

- [ ] #560 ruling recorded with its semantic reasoning (§1.5) — not a
      convenience argument.
- [ ] The landed fix verified: flip-proof transcript shows the pin
      failing under the wrong calendar, then green restored.
- [ ] #560 closed with ruling + evidence + PR links (stage 3).
- [ ] #570 carries a triage disposition stating exactly what ran
      (iterations, load shape, outcome) — no overclaiming; closed only
      if root-caused and fixed.
- [ ] #287/#336/#384 carry dated scan comments; no capture policy
      deleted or weakened anywhere.
- [ ] No test weakened (§1.9 ledger reviewed by stage 3).
- [ ] Full gate suite green on the branch.

## 7. Risks

- **#570 rarity**: one observation ever; 30 iterations may end
  negative. Mitigated by honest bounded-negative recording (§2.4) —
  the issue stays open with a standing capture policy, which is the
  correct outcome for a once-seen race.
- **Loaded-loop side effects on the shared box**: spinners + a parallel
  build can push the machine into heavy load for up to 90 minutes;
  other agents' work may slow (the gate flock serializes suites, so no
  correctness risk — only latency). Teardown pids recorded; the bound
  caps exposure.
- **POSIX TZ subtleties**: the sign inversion (`UTC-14` = east) is
  counterintuitive; the pre-run `date` verification (§1.8 step 3)
  makes the proof self-checking — if the dates don't differ, the run
  is not evidence.
- **Branch carries near-zero code**: the honest diff is the plan +
  records; if stage 2 expects to "fix" something it will find the fix
  already on main. This plan is the guard against a redundant or
  conflicting re-fix (D3).
- **A post-`396bd6e3` h02 observation** during this chunk's gate runs
  would be new evidence on a "fixed" test: stop, capture, triage on
  the watchlist (#560 reopened or a new issue) — never tuned away
  (§1.9 residual).

# The clock-seam audit — wall-clock dependence across the workspace's tests and the passes they exercise

- **Date**: 2026-11-06
- **Branch**: `docs/clock-seam-audit` (analysis only; this report is the sole
  artefact — nothing opened, nothing fixed, no issues closed).
- **Normative base**: `origin/main` @ `81c6f585` (includes both prior
  wall-clock fixes: `1072f064` the #658 anchor, `81c6f585` the #660
  `UiSettings::clock` seam — both read as templates below).
- **Normative bar**: `AGENTS.md` §5 ("deterministic by construction", "no
  pins of today's behavior", "never use `tokio::time::sleep`"),
  `docs/testing_strategy.org` §6 (mock time) and §10 ("fixture builders
  must not read the clock" — the Miri-isolation rule).
- **Method**: full grep sweep of every `.rs` file for
  `Utc::now()/Local::now()/SystemTime::now/now_()/today()`, each hit
  classified production-vs-inline-test by `#[cfg(test)]` module extents;
  then a per-site read of what the read feeds, which assertion observes
  it, and which calendar boundary flips the outcome. Three deep-dive
  passes covered the ui-slint suite, the app-crate suite, and the
  domain/state/storage/sync suites; load-bearing claims (the #627 clip,
  the #610 window math, the golden tolerance, the seam wiring) were
  re-verified by direct read.

## 1. The verdict

The wall-clock flake class is **real, mechanistically single, and almost
entirely concentrated in two surfaces**: the ui-slint render/journey
suite and the app-crate CLI handler suite. Every actor crate below those
(state engine, Deck sync, files sync, storage) is already fully seamed —
**zero live wall-clock dependence** was found in domain/state/storage or
the hermetic sync tiers. The one prior exception (the #660 golden) is
fixed; what remains is:

| Severity | Definition | Count |
|---|---|---|
| **A** — deterministic-in-window | fails *every* run inside a daily wall-clock window; CI-visible | **2 live** (#627, #610) |
| **B** — probabilistic boundary race | fails only when a boundary is crossed mid-test (second/minute/midnight precision) | **6 clusters, ≈20 tests** |
| **C** — latent drift | no failure observed; the shape is time-derived and will bite when a consumer changes | **6 clusters, ≈30 sites** |

The failure-window **law** behind every A and most B findings:

> A fixture stamped `now − Δ` changes bucket/date class exactly while
> the wall clock sits in **[day-window start, day-window start + Δ)**.
> The window start is the *calendar's* boundary (UTC midnight under the
> UI default `day_start = (00:00, UTC)` — `lib.rs:164-167`, the window
> math at `taskboard-domain/src/days.rs:55-70`), *not* a fixed 03:00/06:00
> hour. The 03:00 and 06:00 "class boundaries" recorded in #660 are
> midnight + the fixture offsets (3 h, and 30 h mod 24 h = 6 h).

Two corollaries: (i) a multi-hour `Δ` seed produces a multi-hour daily
failure window — these are *not* rare races but guaranteed failure bands
the CI schedule happens to miss; (ii) any seed whose *end* matters (a
"counted 1 h" expectation) extends the band by the span length, because
`work_in_window` clips through `now` (`presentation.rs:1374-1397`).

**CI schedule exposure**: PR CI has historically run outside the band
(#627's green run: 21:30 UTC; #660's green main run: 23:29 UTC), but the
*scheduled* lanes sit inside it — the weekly Miri cron fires **Monday
03:00 UTC** and the benchmark lane **Thursday 04:00 UTC**
(`docs/testing_strategy.org` §10/§13), both inside the classic
[00:00, 06:00) UTC failure band. The watchlist observations themselves
came from gate runs at 00:06–03:10 local time. Any nightly/weekly lane
that ever grows a ui-slint or cli_track leg inherits the band.

## 2. The seam inventory today — and where each one stops

Six tools exist; each has a precise jurisdiction and a precise edge.

1. **Pure now-as-data** (the domain rule). Every domain/presentation
   function takes `now: DateTime<Utc>` (and the calendar) as parameters;
   nothing in `taskboard-domain` calls `Utc::now()` outside `SystemClock`
   (`clock.rs:6-9` module doc, enforced by grep). *Stops at*: the CLI
   handler layer, which captures `Utc::now()` at the command boundary —
   legal by design, but the capture is what the CLI tests race (§4 A2).
2. **The injected `Arc<dyn Clock>`** (the actor rule). State engine
   (`taskboard-state/src/actor.rs:260`, held `:323`/`:638`, wired
   `SystemClock` at `taskboard-app/src/bootstrap.rs:1248`), Deck sync
   (`sync-nextcloud/src/actor.rs:61`, read_at `:416`/`:441`, skew-pinned
   by `actor_wiremock.rs:4444-4484`), files sync
   (`sync-files-nextcloud/src/actor.rs:38` → `cycle.rs:83`, wired at
   `app/config.rs:1733`), escalation (`app/escalation.rs:491`), focus
   (`app/focus.rs:232`). The day-roll planner takes `today` as pure data
   (`state/config.rs:50-76`, `domain/command.rs:733-738`) — no
   `Local::today` exists anywhere in domain/state. *Stops at*: nothing —
   this layer is clean; the Miri realtime gates correlate *exactly* with
   the only two non-injectable reads (`SystemClock` itself, `clock.rs:40`;
   `UuidV7Generator`, `idgen.rs:48-104`, gated `:111`).
3. **`RetrySleep`** — the backoff seam (both sync crates:
   `sync-nextcloud/src/client.rs:39-47`, injection `:419`;
   files twin `client.rs:45-53`, injection `:203`). Complete; contract
   tests assert exact backoff sequences through `RecordingSleeper`
   (`tests/contract.rs:80-81`).
4. **`UiSettings::clock` → `Adapter::frozen_now` → `day_ctx()`** — the
   #660 render-pass content seam (`ui-slint/src/lib.rs:97-104`, threaded
   `:375`, held `adapter.rs:158-162`, consumed `:966-970`). Freezes the
   derive pass's `now` for everything `day_ctx` feeds: today sections,
   review ledger/bars, chips, due/scheduled phrases, tracked totals (the
   model pushes at `adapter.rs:1174, 2076, 2081, 2128`). Production
   hand-overs pass `None` (`app/ui.rs` — the seam's own doc). *Stops at*:
   **eighteen direct reads in the adapter** the seam does not cover —
   the seconds gate (`adapter.rs:1171`), due-editor validity
   (`:1200/1212/1217`), timer setter compose (`:1302`, `:1383`),
   track-form parse/seed/commit (`:1433`, `:1475`, `:1570`, `:1618` —
   these read `Local::now().date_naive()` directly), notice expiry
   (`:3438`, `:3475`), due echo/parse/sync/settle (`:6905`, `:6910`,
   `:6929`, `:7116`, `:7258`, `:7294`), picker opens (`:7208`, `:7763`);
   plus `lib.rs:648-649` (the cadence tick — deliberately unfrozen: the
   seam freezes *content*, never cadence) and **`lib.rs:775`
   `run_alarm_pass`** — a *content* derivation (`alarm_window_rows`,
   elapsed phrases) that bypasses the seam. This partial threading is
   the audit's central "next seam" finding: see §6 batch 2 and §7 R2.
5. **Zone pinning** (`pin_utc_zone`, `tests/golden_visuals.rs:68-70`)
   — not a clock tool but a localization tool: pins `TZ=UTC` so
   device-local derivations render machine-independently (rationale
   `:62-67`). Composes with 4; does nothing for time-of-day drift.
6. **Fixture anchoring** (the #658 fix, `models.rs:3894-3916`): the test
   stamps `start = max(now − 3 h, UTC-midnight)` — the seed is clamped
   into the running bucket's interior, and the *relative* assertion
   (running bar > 0) holds at every instant after midnight. *Stops at*:
   exactness oracles — an anchor still varies with `now` (the counted
   amount is clipped), so it can never stabilize an absolute-date
   column or a byte-exact golden (the #660 lesson).

Plus the tokio layer: every actor test runs `start_paused` with
`FixedClock` + `advance_both` lockstep (`state/tests/reminders.rs:138-194`)
— the reminder/alarm/occurrence and kiosk day-roll suites
(`state/tests/engine.rs:2962-3223`, `DAY0 = 2026-06-15` at `:2845`) are
fully deterministic by construction, and all 78 insta snapshots carry
only fixed-stamp content (histogram verified: epochs, T0-derived, DAY0
series, and hardcoded s39 literals `snapshots.rs:1378-1384`).

## 3. The decision framework — anchor vs seam vs injected clock

The two prior fixes look inconsistent (#658 anchored a fixture; #660
added a production seam) but are **consistent under a rule that was
never written down**. Writing it down:

**Rule 0 — match the tool to the oracle, not to the test's mood.** Ask
what the assertion compares against, then take the *cheapest* tool that
makes the oracle's inputs fully determined:

| The assertion's oracle | The tool | Why | Precedent |
|---|---|---|---|
| **Relative / invariant** — "positive", "non-empty", "count == seeds", membership; expectation computed from a captured `now` | **Anchor the fixture** (clamp seeds into the window interior: `max(now − Δ, window_start)`, or derive spans from the same calendar) | The production read is *correct by design* (a live pass computing now-derived content); only the seed's bucket placement was wrong. Test-only diff, zero production surface. | #658 (`models.rs:3894-3916`); the cli_repeat/cli_habits flank-bracket style (`cli_repeat.rs:521-539`, `cli_habits.rs:114-138`) |
| **Exact artifact** — committed golden PNG / snapshot whose pixels or bytes embed clock-derived *text* (absolute dates, elapsed phrases, bar distributions) | **Freeze through a content seam** (pin `UiSettings::clock`, seed relative to the pinned instant, regenerate once) | No seeding can stabilize content that rotates with the calendar (the ledger's date column rotates at every midnight *regardless of seeds*). Anchoring is provably insufficient here. | #660 (`golden_visuals.rs:833-885` — the template) |
| **Exact instants** committed through a live form (LogTime writes, parse round-trips) but *no committed artifact* | **Single-read capture** — one `let now` shared by seed and expectation (bracket/flank), or pin through the seam if the path is seam-covered | The race is the *dual read* (test reads now, production reads now again, midnight falls between). One shared read collapses it; a seam also works when the content flows through `day_ctx`. | journeys `:7564` (bracket), `:13912-13915` (self-recorded race) |
| **An actor that stamps time as data** (engine outbox, sync read_at, heartbeats) | **Injected `Arc<dyn Clock>`** — production change, the only tool that is one | The stamp is state, not presentation; tests must own it across a long actor life. | `state/actor.rs:260`, `sync-nextcloud/actor.rs:61` |
| **A pure function's behavior at a boundary** | **now-as-data** (pass any instant you like) | Already universal below the handler layer; nothing to add. | `presentation.rs` test module — all 21 `let now = Utc::now()` sites pass it straight into the pure function and derive expectations from the same value: **already fine** |
| **Machine-dependence of local-wall render** (any zone) | **Zone pin** (`pin_utc_zone`) | Orthogonal to time-of-day; composes with every tool above. | `golden_visuals.rs:68-70` |

**The tie-breakers**, for when two tools both work:

- **Anchor over seam** whenever the oracle allows it: a test-only clamp
  is smaller than any production parameter, and AGENTS §7's flaky policy
  wants minimal dedicated-pass diffs.
- **Seam over anchor** when the artifact renders *absolute* time text —
  this is not preference but impossibility (the date column rotates).
- **Extend an existing seam** rather than adding a new one: the render
  pass already has its freeze point (`day_ctx`); the marginal cost of
  pinning one more journey through it is a settings literal plus a seed
  rewrite. A *new* seam needs the stronger case (see R1: the CLI
  handler's `now` parameter is the completion of an existing
  data-threading contract, not a new seam).
- **Never freeze cadence**: the seam's own doc rule (`lib.rs:102-104`)
  — the repaint tick and the slint mock clock keep driving; only the
  derive pass's `now` freezes. Tests that need time to pass still use
  tokio pause/advance.

**The residual inconsistency to retire**: the *scope* of a freeze must
match the *content paths* of the artifact. The #660 seam froze `day_ctx`
because that is what the ledger renders. A future golden over the alarm
window, the notice line, or editor validity will *not* be frozen by it
(the bypass list in §2.4) — the rule is: extend the seam to a pass only
when an exact-artifact test actually renders that pass's content
(need-driven), and anchor/bracket otherwise. Freezing all eighteen
bypass reads prophylactically would grow production surface for no test.

## 4. The inventory

Severity: **A** deterministic-in-window · **B** probabilistic boundary
race · **C** latent. "Seam?" = would the existing `UiSettings::clock`
freeze fix it (i.e. is the flip in `day_ctx`-derived content)?

### 4.1 Class A — deterministic-in-window (fix first)

| # | Site | Mechanism | Failure window | Seam? | Fix direction | Effort |
|---|---|---|---|---|---|---|
| A1 | `ui-slint/src/flow_table_tests.rs:3556-3557` (`closed_entry_on` helper `:2806-2827`; #627) | Entries stamped at *Local-today* fixed wall hours (10:00/12:00, clamp 22) while `tracked_today_rows` clips through `now` (`presentation.rs:1374-1397`) and drops zero-work rows; the day-total phrase (`models.rs:2378`, `:2535-2536`) folds the *clipped* totals | Rows assertion fails while `now ≤ 12:00` local; the "1 h" phrase additionally fails until the entry is fully past (`13:00` local). Issue records ~11:00 (older call shape). Zone-dependent (stamps are local-wall, window is the UTC day — in extreme offsets the stamps can land on another UTC date entirely) | **Yes** — rows and total derive from `day_ctx`; the test builds `UiSettings` already | **Pin the clock** (the seam template: `clock: Some(fixed)`, stamp entries relative to the fixed instant). A pure anchor also works for the rows leg but *not* for the exact "1 h" phrase near the window start — the phrase needs the full hour counted, which clipping denies until `ws + 1 h` | S |
| A2 | `app/tests/cli_track.rs:1320-1348` (#610) | Work span `[now − 2000 s, now]` vs the test's fixed-offset `day_calendar()` (05:00 @ +02:00 → window opens **03:00 UTC**, `days.rs:55-70`); handler `status_on` captures its *own* `Utc::now()` (`commands/track.rs:69-72`) and correctly clips (`status_view` → `work_in_window`); the expectation hardcodes `2_000` | **[03:00:00, 03:33:20) UTC daily** (= window start + 2000 s — the law of §1). Observed 621 s at 03:10:21 UTC | n/a (CLI) | Either (a) **anchor**: `start = max(now − 2000 s, window_start)` and keep `2_000`; or (b) **assert-relative**: expect `now − max(span_start, window_start)` against the same calendar; or (c) **thread `now`** through `status_on` (R1) — `status_view` already takes it (`track.rs:763`) | S |

### 4.2 Class B — probabilistic boundary races

| # | Cluster | Sites | Mechanism / window | Seam? | Fix |
|---|---|---|---|---|---|
| B1 | **UTC-midnight straddle, seam-covered** | `journeys.rs:13768` (reorder renders two boxes via `next_day_window(now)+9h/11h`), `:13916-13917` (placement expected-instant — the race is self-recorded at `:13912-13915`), `flow_table_tests.rs:3614` (placed-box reopen seed), `models.rs:3896→:4110` (the post-#658 residual: helper's `now` vs the pass's `day_ctx` now) | The test reads `Utc::now()`, the adapter's `day_ctx` reads it again; UTC midnight falls between → "tomorrow" jumps +24 h → the seeded instant lands outside the plan window → assertion fails | **Yes** | Pin the journey's clock through the seam and derive both the seed and the window from the pinned instant |
| B2 | **Local-midnight straddle, seam-BYPASSED** | `flow_table_tests.rs:2808/2939`, `:2881`, `:3298` (exact-`LogTime` commits; seeds and expectations each re-derive `Local::today`), `journeys.rs:10998` (timer-setter grammar: adapter composed via `adapter.rs:1302`, expectation via a second `Local::now()`) | Local midnight falls between the adapter's `Local::now().date_naive()` (`adapter.rs:1570/1618/1302`) and the test's second read → the composed "tomorrow at 09:30" and the expected instant disagree by 24 h | **No** — the editor/setter paths bypass `day_ctx` | **Single-read capture**: one `Local::today` shared by seed and expectation (test-side; kills the dual read). Do *not* extend the seam here (R2) |
| B3 | **Golden chip minute-drift** | `tests/support/fixture.rs:348-353` (`golden_state` open entry at `now−12m`) and `:466-493` (`today_overdue_state`) → 9 golden journeys: `golden_visuals.rs:148` (board matrix), `:190`, `:272` (today matrix), `:380`, `:400` (kiosk), `:429` (notification — plus the −30 m alarm row), `:503` (task view), `:587` (focus overlay), `:667` (track entries) | The toolbar tracking chip renders `· 12m` from the *capture-time* elapsed (`models.rs:2126-2134` via `day.now`); the minute bucket 12m→13m crosses between fixture build and snapshot. Tolerance math: one changed digit ≈ 6.5/255 against the 32×24 per-cell budget 8/255 (`golden_visuals.rs:29-32`, `support/golden_compare`) — **borderline absorbed**; a width-changing rollover (e.g. `9m→10m`) would exceed it | **Yes** (the chip derives from `day_ctx`) | The review-ledger template, applied to the remaining nine: `clock: Some(fixed)`, seeds relative to the pinned instant, one regen (batch 2). Review-empty `:785` and review-ledger `:833` are already clean/fixed |
| B4 | **CLI 24 h window flip** | `cli_track.rs:590-631` (t10) | Window anchored at a captured `now`; the handler's second read can step a *day* past it exactly at 03:00:00.000 UTC (the strict-bound assert `:617-620` fires first) | n/a | Same treatment as A2 (all three shapes close it); the `now` thread (R1) closes it exactly |
| B5 | **Whole-second parse race** | `cli_timers.rs:94-97` (`--at 09:30` leg) | `parse_at_flag` reads `Local::now()` twice (`cli.rs:833-838`); parse and the test's `fire_at > Utc::now()` assert can land astride 09:30:00.000 local → the roll-to-tomorrow branch fires mid-assert | n/a | Bracket (read now before parse) or assert `fire_at >= Utc::now()` / the `%H:%M` shape |
| B6 | **Same-second echo stamp** | `app/tests/common/mod.rs:286` (`card_json` floors `lastModified` to whole seconds), consumed by `cli_verbs.rs:288` (`pending_ops == 0`) | Local edit stamped at N.5 s, echo fixture built at N.7 s floors back to N → echo reads *older* than the local edit → the outbox never drains (the #130 carve-out's input) | n/a | Stamp the echo strictly future (`now + 1 s`) — the fixture is test-owned |

### 4.3 Class C — latent (no observed failure; will bite on consumer change)

| # | Cluster | Sites | Why latent; what unlocks it |
|---|---|---|---|
| C1 | **Minute-margin alarm/notice seeds** (seam-bypassed passes) | `journeys.rs:10360/10366`, `:10714/10719`, `:11213`, `:11250`, `:11317`, `:11368`, `:12223`; `flow_table_tests.rs:1655/1687`; `lib.rs:1216` | Seeds 1–5 min from now against `run_alarm_pass` (`lib.rs:775`) and the notice (`adapter.rs:3438`) — passes that read the clock directly. Fails only under a multi-minute stall; becomes A-class the day a *golden* renders the alarm window or notice line |
| C2 | **Unpinned bar position** | `journeys.rs:12263-12277` (review surface journey) | Asserts counts/order/emptiness (relative) — but the `now−3h` span's *bar position* varies in [00:00, 03:00) UTC; pinning is one settings literal if this ever becomes exact |
| C3 | **Unrendered fixture stamps** | `tests/support/fixture.rs:251` (rule `created_at`), `support/fake_engine.rs:141` (done=now — today-walk membership has a UTC-midnight-ms edge), `:155/:164/:181` | Nothing renders them today; a rule-date golden or a done-today boundary journey unlocks |
| C4 | **Vacuous-pass risk** | `tests/virtualisation.rs:818` | `done = Utc::now()` inserts a Done-today row; if the stamp straddles the window the insertion silently doesn't happen and the "row did not move" assertion passes vacuously (weaker coverage, not failure) |
| C5 | **CLI latent shapes** | `cli_track.rs:436-452` (t6 stamps a *future* end — latent only if a future-end guard ever lands), `cli_repeat.rs:858-868` (p7 UTC-date vs window-date anchor — every direction keeps `next: None`), `:988-1002` (p8 pause bound dominates), `cli_habits.rs:114-138` (DST: the test captures the offset once, the handler re-resolves it twice a year) | Today's assertion styles (flanks, verbatim echoes, structural checks) neutralize them; recorded so a rewrite doesn't silently drop the bracket |
| C6 | **Fixture hygiene in the stamped-actor crates** (§10 policy: "fixture builders must not read the clock") | `state/tests/engine.rs:1504` (**ungated realtime read in a Miri-scoped binary** — the one §10 violation with a scheduled lane behind it), `sync-nextcloud/src/push_exec/tests.rs:46/103/122/156`, `sync-files/src/identity.rs:96`, `sync-files-nextcloud/src/device.rs:139/156/175/191` (`FixedClock(Utc::now())` — test-only, value never consulted), `sync-files-nextcloud/src/actor.rs:205-214` (`StepClock` reads `Utc::now()+30s` per call while its doc claims a logical clock — the sibling `SteppingClock` at `sync-nextcloud/tests/common/mod.rs:43-68` is the correct shape) | No assertion observes any of these values today; they are §10 violations and Miri hazards, not flakes |

### 4.4 Audited clean (negative results, recorded so the pass doesn't re-sweep)

- **State engine, reminders, occurrences, kiosk day-roll, habits,
  tracking, snapshots, undo, reflection** — `FixedClock` + `start_paused`
  + `advance_both` lockstep (`state/tests/reminders.rs:138-194`); the
  day-roll suite runs on `DAY0` fixed data (`engine.rs:2845`, tests
  `:2962-3223`); all 78 snapshots carry fixed-stamp content only.
- **taskboard-domain** — zero ungated wall reads; the stale predicate
  takes `today` as data (`command.rs:733-738`); the two realtime gates
  (`clock.rs:40`, `idgen.rs:111`) are the inherent seam arms.
- **taskboard-storage-sqlite** — the `ts()` fixed-stamp blocks
  everywhere (roundtrip `:28-30`, journal `:20-22`, benches `:61-63`).
- **sync hermetic tiers** — `FixedClock` pinned after every fixture
  stamp (`actor_wiremock.rs:46-49`), the skew pin `:4444-4484`,
  `SteppingClock`, `InstantSleeper`/`RecordingSleeper` through
  `RetrySleep`. Live/docker tier wall reads are deliberate and
  tier-gated (`tests/common/mod.rs:266`, `:554-569`; `sync_pair.rs:26-27`
  — "real timing is the subject").
- **REPL/TTY** — `repl.rs` and `cli_tty.rs` carry **zero** chrono reads;
  the prompt is static (`src/repl.rs:51`). The #654/#521 tty flakes are
  load-class, not clock-class.
- **web_dashboard, cli_output, ui_settings, it_cli, icon probe, all
  other probe suites, keymap/pin/seam inventories** — fixed stamps or no
  time-derived content (`cli_output.rs` uses fixed epoch stamps; the
  icon-probe golden is static tints).

## 5. Reconciliation with the watchlist issues

| Issue | Status vs. this audit |
|---|---|
| **#627** (kiosk tracked section, A1) | Audit **matches** the issue's diagnosis (wall-hour stamps vs clip-through-now) and **refines the fix direction**: the issue proposes anchoring; the audit shows the *phrase* assertion (`tracked_day_total == "1h"`) cannot be satisfied by a pure anchor inside `[ws, ws+1h)` — the clip denies the full hour — so the minimal deterministic fix is the `UiSettings::clock` pin the test's own `mount` already accepts. Anchoring remains valid if the phrase expectation is made relative. Either disposition is a dedicated-pass item; the five `closed_entry_on` call sites (`:2939`, `:3149`, `:3420`, `:3556-3557`) each need their own (B2 covers the exact-instant three). |
| **#610** (cli_track respite, A2) | **Matches**: the issue's suggested shapes (bracket/anchor vs. calendar-relative expectation) are exactly audit options (a)/(b); the audit adds option (c) — thread `now` through `status_on` (R1) — and notes the respite leg is unaffected (work-only total). |
| **#658** (tracking sections mirror) | **Fixed on main** (`1072f064`) — the anchor is the audit's Rule-0 row 1 and the fix is correctly shaped. Residual: the helper-vs-pass dual read is B1 (pin or single-read closes the ms race). |
| **#659** (review ledger, "now−3h" window) | **Fixed on main** (`81c6f585`) together with #660. The issue offered anchor-or-inject; the landed inject is the *only* sufficient tool here (the ledger's absolute dates rotate at every midnight regardless of seeds) — the audit's framework **explains why** the anchor half of the suggestion would have failed. |
| **#660** (review ledger golden, capture class) | **Fixed on main** (`81c6f585`); the audit adopts it as the template and corrects one detail in the issue's mechanism text: the class boundaries are not fixed 03:00/06:00 UTC hours but midnight + the fixture offsets (§1's law). |
| #650, #570, #384, #336, #287 | **Not clock-class** (dismissals-wire CI single observation / IPC instance-lock / stderr prompt leak / wiremock expect-counts / actor suite). Unchanged by this audit; out of its scope. |

## 6. Remediation order (batched by mechanism, not by test)

**Batch 1 — "seed inside the window"** (kills both A rows + B4; test-only
except the optional R1 thread). Apply the #658 clamp (`max(now − Δ,
window_start)`) or the calendar-relative expectation to: the
`closed_entry_on` family (A1 — via the clock pin, see the refinement
above), `cli_track.rs:1320` (A2), `cli_track.rs:590` (B4). Effort: S per
site; one focused PR per lane (ui-slint / app) per AGENTS §7.

**Batch 2 — "freeze the goldens through the existing seam"** (kills B3;
no production change). Apply the review-ledger template — `clock:
Some(fixed instant)`, seeds relative to it, one reviewed regen — to the
nine clock-exposed golden journeys (`golden_visuals.rs:148/190/272/380/
400/429/503/587/667`). This is mechanical once the pinned instant is
chosen (a mid-day UTC instant, mirroring `:844-847`). Effort: M (nine
journeys + PNG regen review).

**Batch 3 — "fixture hygiene below the UI"** (kills C6; §10 compliance).
Fixed stamps for the nine ungated wall reads + replace `StepClock` with
a stepping clock. The `state/tests/engine.rs:1504` read is the urgent
one (Miri lane exposure). Effort: S.

**Deferred (ruled or low-yield)**: B1 (pin four journeys — fold into
batch 2's PR since it is the same template), B2 (single-read capture —
test-only, separate ui-slint pass), B5/B6 (two S fixes, app crate), C1
(leave until a golden renders the alarm window — R2), C2–C5 (record;
fix on consumer change).

## 7. Rulings needed from the user

- **R1 — #610's fix shape**: pure test-side anchor (zero production
  diff) vs. threading `now: DateTime<Utc>` through `status_on`
  (`commands/track.rs:69`), completing the D-F6 data-threading contract
  the function's own doc already names (`day` travels as data; `now` is
  the un-threaded half; `status_view` at `:763` already takes it). The
  thread erases A2 *and* B4 exactly and matches the house pattern for
  the other view handlers (they all capture at the boundary); the anchor
  is smaller. **Recommendation: the thread**, but it is a production
  signature change on a watchlist fix — the user's call.
- **R2 — seam scope growth**: extend `UiSettings::clock` to the bypass
  passes (`run_alarm_pass`, notice expiry, editor validity — §2.4's
  list) now, or need-driven (only when an exact-artifact test renders
  that content)? **Recommendation: need-driven** — prophylactic freezing
  grows production surface for no current test; anchor/bracket covers
  the interim (C1's margins are minutes).
- **R3 — B2's editor-path races**: single-read capture in the tests
  (recommended; test-only) vs. extending the freeze seam into the
  editor passes (production change; the paths read `Local::today`
  directly, so the seam would be a *new* field, not `day_ctx`).
- **R4 — batch 3's `StepClock`**: replacing it is test-only but touches
  a shared test double whose doc comment is currently false; trivial
  yes/no.

## 8. Method note and limits

Line numbers cite `origin/main` @ `81c6f585`; inline-test boundaries
were computed mechanically (`#[cfg(test)]` module extents) with two
known corrections applied (whole-file test modules `flow_table_tests.rs`
and `push_exec/tests.rs`). The tolerance arithmetic in B3 is an estimate
from glyph-area over the 32×24 cell grid, not a measured regen diff —
batch 2 makes it moot. The audit did not execute any test; windows are
derived from the code and cross-checked against the observed capture
records on the issues.

## 9. Addendum (2026-11-06, ~13:30 UTC) — the audit missed one live instance, caught by CI hours after publication

PR #663 (this branch, docs-only) failed its required Test Execution
check on `the_review_ledger_golden_holds` — the test §4.2's B3 row and
§5's #660 reconciliation had classified *fixed*. The failure is the
audit's own B1 mechanism (dual read) hiding inside the #660 fix: the
journey pins the clock and its *added* seeds, but the state comes from
`fixture::golden_state()`, whose open entry is stamped at the real
`Utc::now() − 12 m` (`fixture.rs:348-353`). Whenever the wall clock
sits **after** the pinned instant (12:00 UTC), that entry lies in the
frozen day-window's future, the frozen pass clips it away, and the
render loses ~4 h of counted work (luma 83–100). Failure window:
**[12:00, 24:00) UTC daily** — main's green 08:30 UTC run and the
13:06 UTC PR failure bracket it exactly; reproduced locally at
13:17 UTC.

The lesson generalizes the §6.2 rule by one clause: **pinning a
journey's clock freezes its oracle — every fixture the journey
inherits must then be audited against the *frozen* instant, not just
the seeds the fix adds**. A seed that was harmless under the real
clock (self-consistent: elapsed ≈ 12 m at any wall time, which is why
the unseamed board goldens pass) becomes a boundary bomb the moment
the oracle freezes.

Fix (landed on this branch, `fix(ui-slint)` commit): the review
journey re-stamps the inherited open entry to `PINNED_NOW − 12 m`;
golden regenerated once and the PNG diff reviewed region-by-region —
every changed pixel is the open entry's counted-work expression
(7/30-day Work totals, the yesterday bar rescaled by the new window
max, the footer chip `task-1 · 4h → · 12m`), verified by a visual
judge; the full binary passes under `TZ=Pacific/Kiritimati` and
`TZ=Etc/GMT+12`. Chunk 2's systematic `golden_state_at(PINNED_NOW)`
remains the durable shape; until it lands, any *new* journey that
pins the clock while consuming `golden_state()` unmodified inherits
this exact bomb — the dispatch plan's chunk-2 prompts now say so.

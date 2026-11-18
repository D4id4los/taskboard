# Journey Fixture Seams — Chunk Plan (post-round-9 N-D1)

- **Date**: 2026-11-18
- **Kind**: chunk plan (stage-1 output of the dispatch plan
  `.artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md` §5)
- **Branch**: `test/journey-fixture-seams`
- **Covers**: #884, #897, #921, #914, #905
- **Scope law** (normative, from the dispatch plan): diffs confined to
  test helpers/fixtures; no production code change beyond a recorded
  seam need (this plan records exactly one: #921's clock-seam routing,
  §D3). No push, no PR from stage 1.
- **Origin**: recommendation report
  `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
  §Tier-1 N-D1. Early placement is deliberate: the fixture-seam fix is
  machinery r9's journey-heavy chunks extend; landing it first is
  cheaper than rebasing r9's tail around it.

## 0. Round laws restated (binding on stages 2–3)

(a) Typed outcomes only — no error-message/status/label-text
assertions anywhere in this chunk. (b) Single-read capture or
injected clock for every wall-clock-oracle test — never sleeps. (c)
Never weaken a correct test: the #884 vocabulary fix must not
re-anchor any expectation without a recorded diagnosis — the shift
inventory in §D1 is the ledger stage 3 audits against. (d) Backlog
issues immediately, reported in the stage report.

## 1. Seams as read (2026-11-18, worktree off `origin/main` @ `7aaa3452`)

- **#884** — `crates/taskboard-ui-slint/tests/support/fixture.rs`
  hand-builds `DayCalendar { day_start_time: MIN, offset: UTC }` at
  four sites (`journey_state_at` :369, `golden_state_at` :461,
  `today_overdue_state_at` :605, plus `add_reflection` :169 with a
  *different* `4:00` start), and one raw read bypassing any calendar
  (`today_overdue_state_at`'s `now.date_naive() − 1 day` overdue
  stamp). The adapter's day derivations hold
  `(settings.day_start_time, boot_offset)` and re-derive the calendar
  per apply (`Adapter::calendar`, `adapter.rs` :2106; mount at
  :9041–9042). On a UTC-offset-zero box the two coincide; elsewhere
  "which day is yesterday" differs between fixture and pass by the
  offset.
- **#897** — after C6's option-2 reroute, the load round-trips moved
  to `extract_file_only` (`commands/connect.rs` :936, :977 already
  use it). The issue's named validity-only legs: `connect.rs` :714
  (`extract_raw(Some(&config)).expect("the overlay still decodes")`)
  remains ambient-exposed — `extract_raw` is defaults → toml →
  `TASKBOARD_*` env (`config.rs` :1292), so an undecodable hostile
  env value flips the leg. A full-workspace audit (stage-1, grep
  `extract_raw` under `crates/taskboard-app`) shows further
  test-side callers (`ui_requests.rs` :3542/:3609 `expect("decodes")`,
  `it_docker.rs` :351/:470, `cli_connect.rs` :63, `cli_editing.rs`
  :552/:578/:590/:613) — see §D2 for the disposition.
- **#921** — `journeys.rs::us2_s2_quick_options_open_on_the_current_slot`
  (:23230) reads `chrono::Local::now().time()` *after* the adapter
  captured the same wall clock lazily into `editor.time_open_seed`
  (`adapter.rs` :2375). A quarter-slot boundary crossing between the
  two reads flips `nearest_slot` on one side → spurious failure. The
  capture bypasses the existing clock seam
  (`UiSettings::clock`, `lib.rs` :109, freezing `day_ctx`'s `now`,
  `adapter.rs` :318).
- **#914** — `commands/config.rs::set_tests::daemon_fixture` (:662)
  re-implements the `RefusingStore` + standalone-bootstrap shape of
  `ui_requests.rs::test_workspace_with_app` (:1986), and leaks its
  tempdir guard via `std::mem::forget(dir)` (:681). The sibling
  fixture `ui_requests.rs::port_workspace` (:3648) already returns
  the guard properly.
- **#905** — the flaky observation is a *read-window* race, not a
  save/restore bug: in `tests/config.rs::taskboard_env_lock_restores_ambient_values`
  (:708), the two restore assertions read `std::env::var` **after**
  `drop(lock)` — i.e. without the env mutex held. Under nextest each
  test is its own process (masked); under threaded `cargo test` a
  sibling's `EnvOverride::set` (e.g. `c16_env_override_names_both_answers`
  planting `/tmp/.tmpXXXX/env-named.toml` — exactly the observed
  leaked value) can interleave between our restore and our read.
  The helper's save/restore path itself is correct: save-then-clear-all
  under the mutex, restore in `Drop` while the guard field still
  holds the mutex (drop order: restore, then release).

## 2. Decisions

### D1 — #884: shared harness calendar constructor, pinned into the mount settings

**Decision**: single-source the vocabulary; do *not* parameterize
every fixture on offset.

1. Add to `tests/support/fixture.rs` one constructor —
   `pub fn harness_calendar() -> taskboard_domain::DayCalendar` —
   returning `DayCalendar { day_start_time: MIN, offset: UTC+0 }`,
   documented as *the* harness day vocabulary and the constant the
   mounted adapter must agree with.
2. Replace the three MIN+UTC hand-builds (`journey_state_at`,
   `golden_state_at`, `today_overdue_state_at`) with
   `harness_calendar()`.
3. Route `today_overdue_state_at`'s overdue stamp through
   `calendar.today(now).pred_opt()` (the same shape
   `journey_state_at` already uses), deleting the raw
   `now.date_naive() − 1` read. The issue names exactly this class
   of raw read as the divergence.
4. Pin the divergence shut at the mount: the journey harness's mount
   settings state `day_start_time` and (where the settings surface
   exposes it) `boot_offset` **from the same constructor's values**
   (stage 2 reads the mount settings struct; if `boot_offset` is
   device-derived at boot rather than settable, the zone pin
   (`pin_utc_zone`, testing_strategy §6.1 tool 5) makes the device
   offset UTC for the harness processes, and the plan's non-UTC
   residual reduces to the settings' `day_start_time` — which the
   mount now takes from the shared constant). The goal per the issue:
   fixture and adapter read *one* calendar by construction, not two
   hand-maintained tables.
5. `add_reflection`'s `4:00` calendar is **kept, deliberately** —
   diagnosis recorded: its windows are entry *content* whose
   boundaries the existing green suite already pins (changing the
   start would shift window boundaries and re-anchor walk
   expectations — exactly what law (c) forbids without a diagnosis,
   and none is needed: the divergence #884 names is about the
   *day-vocabulary* derivations, not this helper's explicit
   4:00 review windows). A comment cross-referencing #884 and this
   plan is added so the next reader knows the difference is
   deliberate.

**Shift inventory** (law (c); every entry is a keep-or-diagnose call
stage 2 executes exactly as written and stage 3 audits):

| # | Test class / fixture | Anchoring under the fix | Call |
|---|---|---|---|
| 1 | `journey_state(_at)` overdue stamp | `calendar.today(now)` via shared calendar; values identical (MIN+UTC unchanged) | **keep** — no expectation changes |
| 2 | `golden_state(_at)` review-card pin window | same constructor; values identical | **keep** |
| 3 | `today_overdue_state(_at)` reflection pin window | same constructor; values identical | **keep** |
| 4 | `today_overdue_state(_at)` overdue stamp | routed through `calendar.today()`; on MIN+UTC the result is byte-identical to the raw read (`Utc` date naive − 1 == calendar yesterday at offset 0, day_start MIN) | **keep** — identical by construction; stage 2 proves green without touching assertions |
| 5 | `add_reflection` 4:00 windows | unchanged | **keep** — deliberate difference, documented (D1.5) |
| 6 | every journey test consuming these fixtures | no assertion touched | **keep** — acceptance: the full journeys binary passes unmodified; any red test is a bug or a diagnosis, never a snapshot refresh |

**If any test turns red under D1** (it must not, per row 4's
identical-by-construction argument): stop, diagnose, record — do not
refresh goldens. That outcome would falsify row 4's argument and is
worth its own issue before anything else.

### D2 — #897: reroute the remaining named leg through `extract_file_only`

**Decision**: apply the recorded #865 option-2 pattern's remainder —
reroute file-layer validity-only assertions to `extract_file_only`
(zero-loss: the assertions there are file-layer facts; none intends
env composition).

1. `commands/connect.rs` :714 → `extract_file_only` (the assertion
   checks the written overlay decodes — a file-layer fact). The
   :936/:977 legs named by the issue already use `extract_file_only`
   (post-C6); nothing to do there — noted so stage 3 doesn't hunt.
2. Stage 2 audits the other test-side `extract_raw` callers found in
   the §1 grep (`ui_requests.rs` :3542/:3609, `it_docker.rs`
   :351/:470, `cli_connect.rs` :63, `cli_editing.rs` :552/:578/:590/:613):
   any that (i) run without an env lock *and* (ii) assert
   file-layer-only facts get the same reroute in this chunk
   (same zero-loss argument). Any that compose env deliberately stay
   on `extract_raw` but must hold `taskboard_env_lock()` — if one
   composes env *without* the lock, that is a finding → fix (add the
   lock) + note in the stage report; if it is integration-tier
   (it_docker) where the harness already sandboxes env, keep and
   record. Production callers are untouched, obviously.
3. Hostile-env evidence: stage 2 proves the rerouted legs with the
   binary's tests run under a hostile
   `TASKBOARD_SYNC__POLL_INTERVAL_SECS=abc` ambient (the #897
   mechanism) — set via `taskboard_env_lock()` + `set_env` in a
   scratch verification run, not committed as a test (the composed
   twin `composed_load_env_overrides_the_file_layer` already owns
   the composed-load behavior; no new test, no duplication per
   AGENTS §5's one-behavior-one-test).

### D3 — #921: single-read via the existing clock seam (the one recorded production seam need)

**Decision**: extend `UiSettings::clock`'s jurisdiction to the
open-seed capture; do not add a new seam or a test-only accessor.

1. Production change (recorded, minimal): at the lazy capture
   (`adapter.rs` :2375), when the mount's clock freeze is active the
   seed derives from the frozen instant —
   `clock.map(|now| now.with_timezone(&Local).time())
   .unwrap_or_else(|| Local::now().time())`. Production behavior is
   unchanged (every production hand-over passes `None` — the
   existing contract at `lib.rs` :109's seam); the freeze gains the
   seed capture as a content path per §6.2's need-driven extension
   rule ("extend the freeze to a new pass only when an
   exact-artifact test actually renders that pass's content" —
   `date_time_nearest_row` is exactly that content, and the
   tie-breaker "extend an existing seam rather than adding a new
   one" picks this over a bespoke seed accessor).
2. Test change: `us2_s2_quick_options_open_on_the_current_slot`
   mounts with the harness's pinned clock (`fixture::PINNED_NOW` —
   already the mount freeze of every clock-exposed test, #660) and
   asserts `date_time_nearest_row ==
   nearest_slot(quarter_slots(), PINNED_NOW local time)`. The
   expectation now derives from the *pinned* instant: the
   minute-boundary window is gone by construction (single-read —
   the adapter reads only the frozen clock; the test reads only the
   constant). No wall-clock read remains on either side.
3. Shift inventory entry: this re-anchors us2_s2's expectation from
   "device now at open" to "the pinned instant" — **that is the
   fix, not a weakening**: the old expectation was a second,
   racing wall-clock read; the new one is strictly more
   deterministic and asserts the same production contract under the
   freeze. Recorded here so stage 3 treats it as the plan's own
   call (law (c) satisfied — the diagnosis is the issue itself,
   #921).

### D4 — #914: shared standalone-boot fixture, guard returned, leak killed

**Decision**: one shared `pub(crate)` `#[cfg(test)]` helper; no
`mem::forget` anywhere on the new path.

1. Extract a crate-internal test-support helper (new
   `crates/taskboard-app/src/test_support.rs`, `#[cfg(test)]
   pub(crate)`, SPDX AGPL header per crate tier) holding: the
   `RefusingStore` type, and
   `pub(crate) async fn standalone_boot_fixture(tag: &str) ->
   StandaloneFixture` where `StandaloneFixture` is a struct owning
   the `tempfile::TempDir` guard + `config_path: PathBuf` + `App`
   (accessors `fn config_path(&self)`, `fn app(&mut self)` — or
   plain `pub` fields; stage 2 picks the idiomatic minimal shape).
   The guard drops when the struct drops: tempdir lives exactly as
   long as the assertion window, leak gone. `Deref` is *not*
   implemented (no cleverness; explicit fields).
2. `set_tests::daemon_fixture` is deleted; its six call sites
   (:889 :914 :942 :973 :1011 :1034) bind the struct and read
   `config_path` off it. The tag-carried config filename
   (`taskboard-{tag}.toml`) moves into the shared helper.
3. `ui_requests.rs::test_workspace_with_app` / `port_workspace`
   rebase onto the same helper where their shape matches (standalone
   boot + `RefusingStore`); if their workspace extras (cwd guard,
   ports) make the fold forced, leave them and record why — the
   #877 shared-helper-first law is satisfied by the daemon lane
   consuming the shared core, not by maximal unification.
4. Acceptance: `grep -rn "mem::forget" crates/taskboard-app/src`
   returns no test-fixture hits; the daemon-lane tests pass
   unchanged (no assertion edits).

### D5 — #905: diagnosis recorded + read-window fix (re-acquire the mutex)

**Decision**: the save/restore path is correct; the flake's window
is the test's own post-drop reads. Fix the test, not the helper.

1. Mechanism (recorded here as the diagnosis): the restore
   assertions in `taskboard_env_lock_restores_ambient_values`
   execute after `drop(lock)` returns the env mutex; under threaded
   `cargo test` a sibling's `EnvOverride::set`/restore cycle (the
   observed value — a sibling's `/tmp/.tmpXXXX/env-named.toml` —
   matches `c16_env_override_names_both_answers`'s plant) can
   interleave between our restore and our read. nextest (one process
   per test) masks this, which is why it reproduces only in
   whole-binary `cargo test` runs. The helper's
   save-then-clear-all + restore-under-mutex semantics are correct
   as landed (#729's design).
2. Fix: re-acquire `env_lock()` around the post-drop restore
   assertions. Soundness: env mutations happen only under the
   mutex, and every mutex user restores its own prior while still
   holding it — so by the time our re-acquire succeeds, the env
   deterministically shows our restored values; the only remaining
   way the read fails is a *genuinely leaking* sibling, which is
   precisely the behavior the test exists to catch. Deterministic
   by construction, no sleep, no deadline-poll needed.
3. A comment at the re-acquire names the window and points at #905
   (and this plan's §D5) so the next reader does not "simplify" the
   guard back off.
4. No change to `taskboard_env_lock`/`TaskboardEnvLock` in
   `tests/common/mod.rs` — the helper is correct; changing it would
   be speculative. Watchlist disposition: with the read-window
   closed, #905's flake class is eliminated; the issue closes with
   this PR's evidence (dispatcher at merge), watchlist label
   dropped only after a green observation window per the standing
   flaky rule.

## 3. Implementation order & commits (stage 2)

Per-issue commits, each standalone (rebase-merge hygiene):

1. `test(ui): single-source the journey day vocabulary (#884)` —
   D1 + shift inventory execution.
2. `fix(app): route the overlay validity legs through extract_file_only (#897)` — D2.
3. `test(ui): pin the us2_s2 open-seed oracle to the mount clock (#921)` — D3 (carries the production seam line + the test).
4. `test(app): shared standalone boot fixture without the tempdir leak (#914)` — D4.
5. `test(app): close the env-lock restore assertion window (#905)` — D5.

Order is chosen so the fixture change (#884) lands while the
journeys suite is otherwise untouched — any red there is
unambiguous evidence.

## 4. Gate & acceptance

- Stage gate: pre-commit hooks +
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (flock-safe; background + poll; defer per the rule if contended).
  Full §10 suite is the dispatcher's.
- Acceptance (from the dispatch plan, made concrete):
  - journeys binary green with **zero assertion edits** under D1;
  - the hostile-env scratch run of D2 green;
  - us2_s2 green with the pinned clock and no `Local::now()` in the
    test body;
  - no `mem::forget` in `crates/taskboard-app/src` test fixtures;
  - D5's re-acquired reads green, mechanism comment in place;
  - diff confined to test helpers/fixtures + the one recorded
    adapter seam line (D3.1).

## 5. Risks

| Risk | Mitigation |
|---|---|
| D1's row-4 identity argument fails on some platform (naive-UTC vs calendar divergence at day_start MIN is exact, but a red test would mean a hidden second oracle) | stop-and-diagnose clause in §D1; no snapshot refresh without a recorded diagnosis |
| D3's production line touches `adapter.rs` — r9 collision surface | the line is one expression inside the existing capture block, not a structure r9's chunks own (widgets/overlays/habits/keyboard/menus); stage 2 keeps the diff to that expression and stage 3 re-checks the collision framing against r9's landed files |
| `boot_offset` may not be settable via mount settings (D1.4) | fallback recorded in-place: zone pin makes device offset UTC, leaving `day_start_time` — which the mount takes from the shared constant |
| The D2 audit may pull more callers than the issue named | bounded by the zero-loss test in §D2.2; anything that composes env deliberately is out of scope and only lock-audited |
| The env-lock re-acquire (D5) could deadlock if a future helper nests it | the mutex is documented non-reentrant; D5 uses `env_lock()` (not `taskboard_env_lock()`) for the read scope — no snapshot side effects |
| Gate lock contention with r9 running | flock-deferral rule: commit, report "gate deferred: lock held by <holder>", end stage |

## 6. Estimated diff size

~250–350 lines across:
`tests/support/fixture.rs` (~40), `tests/journeys.rs` (~20),
`src/adapter.rs` (~6, the recorded seam), `src/lib.rs` doc line if
the seam contract comment needs updating (~3),
`commands/connect.rs` (~4), `commands/config.rs` (~−60/+10),
`src/test_support.rs` (new, ~90), `ui_requests.rs` (~−40/+10 if the
fold in D4.3 proceeds), `tests/common/mod.rs` (~25),
`tests/config.rs` (~15). Documentation-of-record: this plan; issue
closures are the dispatcher's at merge.

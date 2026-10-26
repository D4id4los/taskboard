# Disposition comment drafts — flaky watchlist pass (backlog-triage chunk 12)

- **Date**: 2026-10-26 · branch `test/flaky-watchlist-pass` · base `6d5507e`
- **Kind**: stage-2 bookkeeping — the standalone disposition comment for
  each of the six watchlist issues, drafted per dispatch §13 / plan §6.
  Stage 3 posts these verbatim (adjusting the PR link) and applies the
  recorded close/keep-open action. Machine: 16 cores / 30 G RAM.

---

## #431 — golden_visuals driver self-tests (DriverOptions OnceLock) — CLOSE as completed

> **Fixed** — deterministic harness bug, not timing, exactly as the
> issue read. `support::driver::run` pins the first `DriverOptions` it
> sees in a process-global `OnceLock` and asserts every later call
> agrees ("a process has one platform"); the two self-tests compiled
> into the `golden_visuals` binary requested *different* options
> (`true` vs `false`), so under plain `cargo test` whichever ran
> second panicked. CI never saw it because nextest runs each test in
> its own process.
>
> Fix (commit `802a566`): the geometry self-test now requests
> `headless_renderer: true` — the value every journey call site in the
> tree uses (surveyed: all 20 sites), and the only arm `false` skips is
> the "software" renderer-name label, which the geometry reads do not
> depend on. Covering that arm is deliberately a separate-binary job,
> out of scope here (non-goal, not a follow-up issue).
>
> Verification: `cargo test -p taskboard-ui-slint --test golden_visuals
> -- --test-threads=1 support::` fails on base (`6d5507e`) with the
> OnceLock mismatch panic and passes 13/13 after the flip; nextest lane
> green throughout (1913/1913 workspace, full §10 gate suite green).
> Rejected alternative, as the issue's second suggestion: moving the
> self-tests into their own binary — heavier, and solves nothing once
> the options agree.

## #408 — ipc ip13 detach TimedOut under load — CLOSE as completed

> **Root-caused and fixed.** Mechanism (code read, plan §1.1): ip13's
> `served_nextcloud` fixture attaches a connector pointed at
> `https://cloud.example.com` — a dead identity host. The sync actor
> still runs its startup capabilities probe and first cycle against
> it, and processes `Detach` only between cycles, so detach latency =
> DNS + TCP-failure time on the loaded machine, unbounded by
> construction. `detach_connector_named` waits with
> `stop_actor(config.app.shutdown_timeout_secs)` (default 5 s); under
> full-suite parallel load (16 nextest processes, DNS storms) that
> once expired → actor aborted → `DetachOutcome::TimedOut`. Not a
> wiring regression — same family as #417 below, one fix.
>
> Fix (commit `2a8ef68`): the fixture injects
> `config.app.shutdown_timeout_secs = 60` through the existing
> production config seam — the same seam `commands/sync.rs:466`
> already injects through. Injected, not enlarged: `stop_actor`,
> `join_bounded`, and the 5 s default are untouched; no assertion
> changed (`Detached` and the rest of the journey pin byte-identical).
> 60 s ≈ 12× the failed default (real margin over pathological local
> DNS walks) while staying 5× under the ip13 journey's 300 s
> `LIVE_DEADLINE`; a genuine wedge now fails legibly at 60 s instead
> of flaking at 5 s. Trade-off recorded in the plan (§8).
>
> Stress evidence (post-fix, this machine): 10× `cargo nextest run -p
> taskboard-app` + 3× `cargo nextest run --workspace` + 1× instrumented
> llvm-cov lane — all green, zero retries needed.

## #417 — bootstrap control-handle detach TimedOut under load — CLOSE as completed

> **Root-caused and fixed** — same family as #408, same commit
> (`2a8ef68`). The control-handle test attaches via `any_client()` =
> `https://nc.example` (dead identity host); identical mechanism:
> dead-host wind-down (startup probe + first cycle's connect failure)
> racing the 5 s default `stop_actor` join budget under full-suite
> load, one expiry → `TimedOut` where the test pins `Detached`.
>
> Fix: the test sets `config.app.shutdown_timeout_secs = 60` beside
> its existing config pins — through the production config seam,
> nothing production-side touched, assertions byte-identical
> (`Detached` / `NoConnector` / fail-closed `NoOp`). Unlike ip13 this
> test has no journey wrapper, so a genuine wedge costs a minute of
> CI; nextest slow-test marking is the backstop — judged better than a
> 5 s load flake (plan §8).
>
> Stress evidence (post-fix): 10× `-p taskboard-app` + 3× full
> workspace + 1× instrumented lane, all green.

## #384 — cli_login l13 stderr assert (prompt leak suspicion) — STAY OPEN, capture policy

> **Annotated: mechanism correction + capture policy. No fix beyond
> evidence capture** (dispatch ruling: no repro → no speculative fix).
>
> Mechanism correction to this issue's reading: the duplicated
> `App password: App password: ` in the captured output is **routine,
> not the leak**. The connect ceremony always prompts for the app
> password (no password flag exists), writing the label newline-less
> to *process stderr* — not to the `CommandIo.err` buffer the test
> asserts on. Under nextest, stdin is /dev/null → empty password →
> `verify` against a method+path mock → success. So every run of l13
> puts exactly that two-prompt string on the nextest-captured process
> stderr; its presence in the 2026-10-21 failure output is expected
> and tells us nothing about the failure.
>
> What actually failed: the panic at the `connect_err.is_empty()`
> assert proves the **buffer** was non-empty — but the assert message
> was a static string, so the content was lost. No production path in
> this flow writes prompt text into `CommandIo.err` (audited writers:
> the login deprecation warning, the attach nudge warning, and the
> JSON envelopes). Honest candidates: a transient loopback failure of
> the in-process wiremock `verify`, a config-write error under /tmp
> pressure, or something else — undeterminable retroactively.
>
> Capture policy (landed in commit `b7c2625`): the assert now carries
> the captured bytes in its message —
> `assert!(connect_err.is_empty(), "connect keeps stderr clean: {:?}",
> ...)`. The asserted fact is unchanged (structural `is_empty`; this
> is diagnostics, not a text assertion). **On the next occurrence**:
> record the full panic message verbatim (it now names the buffer
> content), plus lane, load shape, and commit — that content is the
> root-cause evidence. Repro attempt this pass: the full protocol (3×
> full workspace, 10× `-p taskboard-app`, 1× instrumented lane) ran
> green — no recurrence. Issue stays open on the watchlist.

## #287 — actor_wiremock / push_exec one-off failures — STAY OPEN, capture policy + recurrence watch

> **Annotated: capture policy + recurrence watch; repro protocol
> negative this pass.** One unexplained observation per symptom
> family; the tests' assertions are correct — they lack evidence, not
> strength.
>
> Repro record (2026-10-26, branch `test/flaky-watchlist-pass` @
> `b7c2625`, 16-core machine): 3× full-workspace `cargo nextest run`,
> 10× `cargo nextest run -p taskboard-sync-nextcloud`, 1× instrumented
> `cargo llvm-cov nextest --workspace` — **all green**, including the
> two named tests (`a10_304_with_a_cold_cache_refetches_unconditionally`,
> `create_stack_transport_failure_aborts`). Negative result recorded
> per the dedicated-pass protocol.
>
> Working hypothesis (unconfirmed, plan §1.4): transient loopback
> connect failure under machine load (accept-queue pressure from
> dozens of concurrent wiremock servers across nextest processes) →
> the actor/executor sees a transport error → abort or Failed-report →
> the structural assert fires. Consistent with all three observed
> symptoms (0.398 s a10 failure, 21 ms delete_404 failure before the
> mock ever saw traffic, coverage-lane create_stack failure). The
> harness's check-then-advance loop rules out a second-cycle race for
> a10.
>
> Capture policy — on any recurrence, record before re-running:
> (1) the full nextest failure section (`--nocapture` output; the
> `refused_client` diagnostics arms and the exact panicking assert ride
> it), (2) lane + load shape + commit. A transport-arm diagnostic or a
> Failed-report panic confirms the hypothesis; a confirmed transient
> loopback root cause is environmental and earns a follow-up issue
> (client resilience to transient connect failures — design question,
> not a test patch). Closure waits for either a root cause or a fixed
> observation window with zero recurrence. No client-level retry /
> connect-timeout change in this pass — kiosk resilience question, out
> of the flaky pass's scope.

## #336 — push_exec delete_404 one-off — STAY OPEN, capture policy + recurrence watch

> **Annotated: capture policy + recurrence watch; repro protocol
> negative this pass.** Same family and same comment substance as
> #287 above (posted there; not duplicated): 2026-10-26 protocol over
> `test/flaky-watchlist-pass` @ `b7c2625` — 3× full workspace, 10×
> `-p taskboard-sync-nextcloud`, 1× instrumented lane — all green,
> including
> `delete_404_re_resolves_and_deletes_at_the_fresh_binding`.
>
> The 21 ms failure timing (2026-10-18, full-workspace load) fits
> server boot + mounts, then a panic before/without the request
> traffic — i.e. `verify()` panicking because a transient transport
> connect failure aborted the push before the 404 mock ever saw a
> request. The `.expect(1)` counts and `server.verify()` are correct
> as-is; the gap is evidence, not assertion strength.
>
> Capture policy — on recurrence, record the full nextest failure
> section (the exact panicking assert and any transport-arm
> diagnostics), lane, load shape, and commit, before re-running. Then
> root-cause with the #287 family. Stays open unless reproduced and
> root-caused.

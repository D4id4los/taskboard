# Post-Round-8 Implementation Recommendation (2026-11-16)

Question: which open issues make sense to implement/fix **now**, given
that `2026-11-16-gui-review-round8-plan.md` (ACCEPTED, 8 chunks per its
§7) is being implemented?

Excluded upfront — round 8 owns them (its §2/§4 coverage table): #811,
#823, #741, #743, #745, #748, #749, #750, #746, #756, #668, #669, #670,
#724, #753, #759, #706, #832, #834, #836, #839, #840, #772, #763, #712,
#838, plus the folded machinery/framework debt (#715, #716, #762, #782,
#768, #760, #775). Also excluded: the C-6 standing deferrals
(#807/#809/#812/#813/#819/#820 — re-affirmed open, no work), #113/#114/#115
(user intent gate), audit registers and dated timers.

The remaining space is exactly round 8's §5 hand-off list plus the
non-UI backlog. Organizing principle: **crate/surface disjointness** —
round 8 lives in `taskboard-ui-slint` + the UI adapter + keymap tests
(+ the domain day-end module in its chunk 3); the sets below live in
`taskboard-sync-nextcloud`, `taskboard-state`, the recur engine
(different domain module), and app/CI test infrastructure, so they can
run in parallel worktrees without collision.

## R-A. First: give #731 a verdict (S — gate hygiene)

#731 (p2, security: cargo-deny advisories / bundled SQLite CVE under
sqlx) still claims CI-red on `main`, yet today's `main` runs are green.
Either the state self-resolved (advisory data or dependency changed) or
it is intermittent/suppressed. One small chunk: reproduce
`cargo deny check` on current `main`; if green because the bundled
version was fixed upstream, close with evidence; if still failing, fix
on a branch (`cargo update -p libsqlite3-sys` / pinned upgrade, or a
dated accepted-risk entry in register #284). A red-or-ambiguous
required gate must not stay ambiguous while every later PR depends on
that check.

## R-B. Sync correctness round (`taskboard-sync-nextcloud` + transport)

One coherent round, fully disjoint from round 8:

- **#695 (p1)** — hosted Nextcloud keeps the ETag on a same-second,
  same-size rewrite (reproduced with curl, body has the matrix). The
  F4/F5 "ETag churns on every PUT" characterization is no longer
  unconditional; validators/compare logic must handle the retained
  ETag without false-clean or false-stale outcomes.
- **#133 (p2)** — clock-skew mitigation for sync comparisons. Same
  seam as #695 (second-granularity comparisons); fix designs should be
  done together so the skew model and the validator model agree.
- **#220/#221/#222 (p3 ×3)** — the Retry-After family: honor it in
  client retry waits, HTTP-date form + 503 coverage, push-abort floor.
  Same transport surface; natural third chunk of the round.

**#708 (p1**, F1 sidecar folds' local-author leg breaking the
convergence law for third readers) belongs in this family but is a
design lift, not a fix: schedule a **design round** for it (it is
architecture + sync + reliability), then dispatch. Do not bolt it onto
R-B's implementation chunks.

Journey plans: none needed — engine/sync internals are exempt.

## R-C. Domain bug: #713 (S/M, high value-per-effort)

#713 (p2): proptest found a real recur attribution bug — the Done
stamp lands outside its member's window (Weekly multi-weekday,
Until-bound anchor). Pure `taskboard-domain` + proptest work; the
failing property likely already exists or falls out of the repro seed
recorded on the issue. One sharp fix chunk. Caveat: round 8's chunk 3
also touches domain (day-end due calculation, N-4) — different module
(recurrence vs day boundary), but stage-1 should confirm symbol-level
file disjointness before running the chunks concurrently.

## R-D. Flaky-test dedicated pass (§7 mandate; round 8 §5 hands this off)

The watchlist holds 8 open issues. Per AGENTS §7 these are triaged in
a dedicated pass over the `flaky-test` label query — never
opportunistically — and round 8 explicitly does not do them:

- **#697 / #521 / #846** — the `cli_tty` Ctrl-C family (three distinct
  tests, one raw-mode-under-load seam; cross-linked by the cleanup
  round). Highest signal: fix the seam once, close three.
- **#287** (actor_wiremock), **#336** (push_exec 404 re-resolve),
  **#384** (l13_alias happy path), **#570** (cli_ipc relay
  instance-locked), **#650** (no_local_ids, CI-only), **#682**
  (proptest merge tie) — individual triage: stabilize, quarantine with
  a dated watchlist note, or close as reproduced-and-fixed.

## R-E. Test-infrastructure hygiene chunk (S, app crate)

- **#728** — config tests restore CWD manually; a mid-test panic leaks
  the tempdir cwd into siblings → guard-type fix.
- **#729** — config tests inherit ambient `TASKBOARD_*` env →
  env-lock helper (serialized env fixture).
- **#730** — XDG test sandbox is Linux-only (`dirs::config_dir`
  ignores `XDG_CONFIG_HOME` on macOS/Windows) → portable sandbox.
- **#607** — load-reproduction scripts leak CPU-spinner subshells
  (kill %N cleanup no-op) → scripts-only fix.
- Optional rider: **#833** — audit the lib/journey test helpers for
  guarded-refresh staleness (read-only audit, fits the same pass).

One coherent chunk, `taskboard-app` tests + `scripts/`; exempt from
journey plans; zero overlap with round 8 surfaces.

## R-F. Worth scheduling soon, but second-tier (decision needed)

- **#327 (p2, testing/ci)** — capped cargo-mutants one-off for
  taskboard-app (commands/ + ipc/ first). Bounded, CI-only, disjoint.
- **#356 → #355 (undo v2 state-side foundation)** — `taskboard-state`
  work is disjoint from round 8, but it is a family start, not a quick
  win. The UI legs (#354/#353) must wait until round 8 lands: they are
  user-visible, so they need journey plans under the now-mandatory
  framework, and they would churn the same UI surfaces round 8 is
  rebuilding.
- **#845 (dx)** — `config set` writes only the file layer; concurrent
  knobs diverge until restart. A real behavioral gap, but it changes
  user-visible CLI semantics → journey plan required (framework is
  mandatory since 2026-11-16).
- **#764 (dx)** — `boards publish --to-files` CLI verb: new verb →
  journey plan required. Second-tier for the same reason.
- **#843 (testing/dx)** — investigation: journey_state overdue task
  not surfacing in Today under the default mount. Read-only
  diagnosis can run anytime, but any resulting fix lands in Today-view
  code adjacent to round 8's focus-restoration chunk — diagnose now,
  fix after round 8.

## Explicitly deferred (fine to keep waiting)

- **Performance/sync-architecture seven** (#131/#137/#138/#141/#146/
  #148/#152/#154/#155/#157 + #139/#140/#142): premises were
  re-verified 2026-11-16 by the cleanup's liveness pass, so they are
  live — but each is a perf project, and correctness (R-B) buys more
  than speed right now. Revisit after round 8 + R-B land.
- **Feature-scale items**: #122 (OAuth2 device/PKCE), #218 + #639–#642
  (review family, v0.2, UI → journey plans), #700/#701/#718/#723
  (account/files architecture), #185/#184 (org backends), #213
  (subtask design round), #215/#216/#217, #494 (per-user attribution,
  Wave 4), #444 (native notification buttons — UI), #121 (Deck
  comments endpoints), #117 (LAN live-tier runner), #193/#195 (IPC
  evolution), #440 (zbus refresh — gated on slint's atspi stack),
  #167/#180/#191/#198/#224/#225 (CLI/dx surface polish).

## Recommended dispatch order

1. **R-A** (#731 verdict) — unblocks trust in every required check.
2. **R-C** (#713) and **R-E** (config/test hygiene) — small, sharp,
   immediately parallelizable in disjoint crates.
3. **R-D** (flaky dedicated pass) — independent; can start anytime.
4. **R-B** (sync correctness round, #695+#133 then Retry-After; #708
   design round in parallel) — the round's heavyweight.
5. R-F items per operator decision; nothing else this week.

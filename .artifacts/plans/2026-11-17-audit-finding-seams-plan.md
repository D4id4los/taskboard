# [Plan] Chunk C4 — audit-finding seams (#860 + #868)

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan (dispatch plan §5, prompt C4.1).
- **Branch**: `fix/audit-finding-seams` (created off `origin/main` @
  44c6f32b, worktree `../taskboard-WT-fix-audit-finding-seams`).
- **Normative inputs**: round plan
  `.artifacts/plans/2026-11-17-backlog-reduction-round-plan.md` §3-C4;
  dispatch plan `2026-11-17-backlog-reduction-dispatch-plan.md` §1/§5;
  issues #860 (fix prompt) and #868 (row-5 verdict + hardening);
  `.artifacts/plans/2026-11-16-sync-etag-skew-plan.md` §4/§8.
- **Status**: READY for stage 2 (implementation).
- **Verified against**: `origin/main` @ 44c6f32b. Symbol anchors
  re-verified at stage-1 time (2026-11-17); re-anchor by symbol if
  main moved.

## 1. r8 carve-out verdict (round plan §3-C4)

**Proceed with both halves; do NOT fold #860 into r8.** Evidence:

In-flight r8 branches (fetched 2026-11-17, `git branch -r` + local
worktrees): `feat/ui-r8-board-interaction`,
`fix/ui-r8-files-sync-status`, `fix/ui-r8-task-detail-editor` (the
round-8 friction branch and remaining seeds are not yet pushed;
these three are the live set per `git worktree list`).

Symbol-level overlap check against C4's planned diff sites:

| C4 site | r8 branches touching it |
|---|---|
| `taskboard-ui-slint/src/lib.rs` `observe_connector_episode` (~:979) | none (board-interaction touches `lib.rs` only at `view_component_tests` ~:3839; files-sync at `mount_tests` ~:1184; task-detail at `view_component_tests` ~:3096) |
| `taskboard-ui-slint/src/adapter.rs` tick push (~:671) / `refresh` early-return (~:1531–1539) | none (no r8 hunk in adapter.rs lines 600–1700) |
| `taskboard-sync-nextcloud/src/actor.rs` | none (only files-sync touches a sync crate — `taskboard-sync-files-nextcloud/board_cycle.rs`, a different crate) |

Disjoint. Default per the round plan (this round) applies.

## 2. Decision D1 — #860 fix shape

**Decision: the seam (`observe_connector_episode`) gains an
unconditional `push_notice(&window)` between `record_system_event`
and `refresh` — the early-return in `refresh` is NOT touched.**

Rationale:

- The tick loop's order is exactly this: unconditional
  `push_notice` first (`lib.rs:671`, "the push is cheap enough to
  run unconditionally"), then the guarded `refresh`. The seam should
  replay the tick-tail shape it claims to replay ("the same
  record-then-refresh path the system drain's pre-refresh hook
  runs") — and today it replays only the refresh half of it.
- Putting the push inside `refresh`'s unchanged early-return instead
  would fire on every guarded caller (tick included, which then
  pushes twice per unchanged tick) and would widen the #463
  unchanged-refresh guard's contract for one seam's benefit —
  against the audit-table classification the doc comment cites
  (`2026-10-31-unchanged-refresh-guard-plan.md` §5: the notice is a
  `push_notice`-surface input, not a derive input).
- The "decouple notices from refresh entirely" alternative (round
  plan's escape hatch) is not needed: with the seam-level push the
  seam's contract holds for every `SystemEvent` variant, and no
  `refresh` semantics change.

Diff: `lib.rs` — one call + doc-comment sentence; the seam's doc
comment is updated to state the notice half explicitly.

### The pinning test (D1 acceptance)

New test in `taskboard-ui-slint` `mount_tests` (the tier that owns
`UiHandle` seam behavior — "one behavior, one test, lowest layer"):
drive a `UiHandle` with a state snapshot containing a task, call
`observe_connector_episode(&SystemEvent::TaskResurrected(id))`, then
assert the **typed/structural outcome** through the window getters:
`notice_visible == true` (never the text — no text assertions,
AGENTS §5). Contrast assertion optional at stage 2's discretion but
not required: one behavior, one test.

Clock discipline: the assertion is a single-read capture (the
notice's `until` is `now + RESURRECT_NOTICE_HORIZON`, read in the
same instant as the assert) — no fixture anchoring or clock freeze
needed; record this classification in the test's doc comment
(`testing_strategy.org` §6.1).

`flow_table_tests.rs:2493`'s `derive_passes == 1` pin must stay
green — the seam-level push adds no derive pass (it predates
`refresh`), which is exactly why D1 chose the seam over the
early-return. Stage 2 keeps that pin and treats a red as a bug, not
as a pin to weaken.

## 3. Decision D2 — #868 seam design

**Verdict recap (in-place, issue #868 / skew plan §3 row 5):** the
row-5 skip at `taskboard-sync-nextcloud/src/actor.rs` (~:1045–1077,
`cached.last_modified == candidate.last_modified && cached.filesize
== candidate.filesize`) is correct as-is; C4 lands the optional
hardening's prerequisite seams **plus** the guard, per round plan
§3-C4 ("lands the seams **and** the guard behind them").

### D2a — fetch-time seam on `CachedAppData`

- `CachedAppData` (`actor.rs:212`) gains `fetched_at: i64` (unix
  seconds), stamped from the injected clock at the single insert
  site (`actor.rs:~1092`, the `download_attachment` Ok arm). Only
  success caching exists today, so one stamp site.
- The stamp flows from the actor's **existing** clock seam:
  `SyncActorConfig.clock: Arc<dyn Clock>` (`actor.rs:58`) —
  **decision: no new clock seam is introduced**; the issue's "plus a
  clock in the Deck actor" prerequisite is already satisfied by the
  #133-era injection. The clock reaches `sidecar_evidence` as a new
  `clock: &dyn Clock` parameter (precedent: the ceremony path takes
  `clock: &dyn Clock` at `actor.rs:1139`).
- Construction-site updates: `actor.rs:795` (the initial
  `CachedAppData` literal) stamps `clock.now()` too.

### D2b — the guard and its band constant

**Constant: `pub(crate) const SKEW_BAND_SECS: i64 = 2;` — the #133
named tolerance, twinned into `taskboard-sync-nextcloud` per the
skew plan §4's module-scope, twinned-per-crate convention (the files
crate already carries its own twin; this is the Deck crate's twin,
not a second constant).**

**Guard shape (recorded decision, resolving the issue's prose):**
the equal-validator skip is trusted only while the cache entry is
fresh; the skip fires (silently returns the cached document) when

```rust
clock.now() - cached.fetched_at < SKEW_BAND_SECS
```

and once the entry's age reaches the band, the **next scheduled
cycle re-fetches** the content (falls through to
`download_attachment`, restamping `fetched_at`) — exactly the
issue's "re-fetched on the next scheduled cycle rather than trusted
indefinitely". Band semantics for the test oracle: age **inside**
the band ⇒ guard silent (skip holds); age crossing **outside** the
band ⇒ guard fires (re-fetch). The test asserts both sides.

Documented at the constant: the band is deliberately aggressive in
production (a Deck sidecar older than 2 s re-fetches once per
scheduled cycle) — bounded cost (one attachment GET per board
cycle, the same request the un-equal path already makes), and it is
the only shape expressible with a single named constant, which the
round law fixes. A cadence-style band (re-fetch near a revalidation
due point) would need a second constant and is recorded as backlog
debt (see §5).

Behavior outside the guard: zero. The metadata-unequal path, the
tombstone/absent paths, and the cache-fill stamping semantics are
untouched; the comment block at `actor.rs:1052–1067` is rewritten to
state the verdict + the guard instead of "recorded as backlog debt".

### D2b test (the band guard pin)

A wiremock test in `taskboard-sync-nextcloud` (the crate's existing
wiremock harness) with the **injected** `Arc<dyn Clock>`:

1. First cycle: serve a board snapshot + attachment download; assert
   the document arrives and the download endpoint was hit once.
2. Advance the injected clock by `< SKEW_BAND_SECS`, run a second
   cycle with identical metadata; assert the download endpoint was
   **not** hit again (skip silent inside the band).
3. Advance past `SKEW_BAND_SECS`, run a third cycle, metadata still
   identical; assert the download endpoint **was** hit (guard fires).

Request-count oracles on the wiremock journal — structural facts,
never text. Clock control via the injected seam (pause/advance not
applicable to a real-actor wiremock test; the injected clock IS the
deterministic control — round law [L](b) satisfied).

## 4. Open questions — all resolved

| Question | Decision |
|---|---|
| Fold #860 into r8? | No — recorded disjointness (§1); default "this round" stands. |
| #860 fix site: seam vs `refresh` early-return vs decoupling | Seam-level `push_notice` (D1); early-return untouched (would double-push on the tick path and widen the #463 guard); decoupling unneeded. |
| Which layer owns the #860 test | `mount_tests` — `UiHandle` seam behavior's lowest tier; no flow-table duplicate. |
| #868 actor clock: new seam? | No — reuse `SyncActorConfig.clock` (D2a); pass `&dyn Clock` into `sidecar_evidence`. |
| Band constant | The #133 twin `SKEW_BAND_SECS: i64 = 2`, twinned into this crate (D2b); no second constant. |
| Guard semantics | Freshness trust window: age < band ⇒ skip silent; age ≥ band ⇒ next-cycle re-fetch (D2b). |
| Production cost of the 2 s band | Accepted and documented at the constant (one extra GET per board-cycle past 2 s); cadence-band alternative is backlog debt (§5). |
| UX journey plan | Not required — no user-visible capability changes (dispatch plan §1: "no UX plan in origin"). |

## 5. Backlog issues opened this stage

- Recorded in the final stage report; entered before stage 2
  dispatch (standing authorization, AGENTS §7).

## 6. Risks

- **R1**: the 2 s freshness band makes the Deck sidecar cache
  near-no-op at production cadence (re-fetch ~every cycle). Accepted
  by D2b as the single-constant shape; if the cost matters on kiosk
  hardware, the backlog issue proposes the cadence constant. The
  correctness direction (never trust indefinitely) is the right one;
  the *economy* direction is the open debt.
- **R2**: `sidecar_evidence` gains a `clock` parameter — call-site
  churn inside `actor.rs` only; the function is crate-private.
- **R3**: the #860 seam push uses `chrono::Utc::now()` inside
  `push_notice` (existing wall-clock read, unchanged) — the test's
  single-read capture absorbs it; stage 3 audits no *new* wall-clock
  read is added (none is planned).
- **R4**: r8 branches may land before C4's PR opens — rebase conflict
  risk is low (recorded disjointness) but the carve-out verdict
  should be re-checked at the dispatcher's gate review if the
  friction chunk lands first.

## 7. Estimated diff

`lib.rs` (+~6 incl. doc), `mount_tests` test (+~40),
`actor.rs` (struct field + 2 stamp sites + param + guard + comment
rewrite, +~30), wiremock band test (+~130), harness touch-ups
(+~20). **~220 lines, test-heavy**, confined to the named seams.

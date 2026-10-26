# [Plan] Sync Skew Interleaving Pins (DR-4/S)

- **Date**: 2026-10-26
- **Kind**: stage-1 TDD plan for chunk DR-4/S of the 2026-10-26
  six-rounds dispatch (issue #480, branch `test/sync-skew-pins`).
  The chunk is the DR-4 design plan §3 **D-4b** test-pack plus the
  **D-4a** bookkeeping comment — a tests-only chunk that pins the
  clock-skew behavior ADR 0004 documents, so any future change to it
  fails CI and reads as debt (AGENTS §5).
- **Origin** (normative):
  `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md` §3
  D-4a/D-4b (the deliverable, item by item), §2 facts 10–11, §4 fork
  F-133, §6 chunk S; the dispatch plan §1/§4.2; the audit's §skew
  comparison-site map C1–C9 and interleavings (i)–(iii)
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-time-dst-report.md`
  §skew) — the pins are the audit's interleavings cast as tests.
- **Branch / worktree**: `test/sync-skew-pins` in
  `../taskboard-WT-test-sync-skew-pins`, off `origin/main` @
  `27343ad`. Doctor passed (86G free, floor 25G).
- **Status**: PLANNED — stage 2 implements; stage 3 posts the #133
  comment (§6) and ships.

---

## 1. Mission & the diff-shape rule

**Zero production code.** The branch diff is test code and nothing
else. As a review-checkable rule (dispatch §4.2 acceptance: "diff is
tests + the issue comment only"):

- **Allowed**: new `#[test]`/`proptest` functions inside
  `crates/taskboard-domain/src/merge.rs`'s existing `#[cfg(test)] mod
  tests`; one new A-series test (+ its local mount helper) in
  `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`.
- **Forbidden**: any hunk to `merge.rs`'s non-test code, `entities.rs`,
  `command.rs`, `pipeline.rs`, the sync actor, `merge_testutil.rs`
  (compiled unconditionally — `lib.rs:62`, so it counts as src), any
  other crate, any Cargo.toml, any ADR or doc file. No new
  dependencies; no insta snapshot changes (whole-state shapes are
  untouched).

The one known doc blemish is deliberately **not** fixed here:
`merge.rs:22-24`'s module doc says "Tests use logical times and are
skew-free by construction", which the new pins falsify. Editing that
sentence would make this chunk's diff a src hunk against a
blocking review gate; the correction rides chunk DR-4/G (wave 3),
which edits `merge.rs`'s docs anyway. Recorded so the reviewer reads
it as decided, not missed.

## 2. Verified baseline (seam facts at `27343ad`)

- `merge.rs:71` `remote_wins(remote_ts, field_clock) := remote_ts >
  field_clock` — strict; ties keep local (ADR 0004's tie-break).
- `merge.rs:178-285` `merge_task`: one gate per field
  (`:195-247`); **R5 resurrect** at `:249-254` —
  `local.deleted && remote_wins(ts_r, local.clocks.deleted)` ⇒
  `deleted = false`, `clocks.deleted = ts_r`; `remote_seen := ts_r`
  always (`:283`).
- `merge.rs:550-564` `tombstone_task` — tombstone clock =
  `max(old, observed_at)` (audit C3: the observation stamp).
- `merge.rs:573-637` `adopt_task_after_push` (audit C4) — echo
  values adopted unconditionally; every clock advances to
  `max(local, echo.last_modified)`.
- `merge_testutil.rs`: `BASE = 36_000` (10:00:00Z), `ts(secs)`,
  `bound_task(card)` (title `local {n}`, all twelve clocks at BASE,
  `remote_seen = Some(ts(BASE))`), `remote_task(stack, card, lm)`
  (title `card {n}`), `card_ref(1, 1)`.
- `entities.rs:278-312` `TaskClocks` (twelve per-field write clocks);
  `entities.rs:429-432` `Task.remote_seen: Option<DateTime<Utc>>`.
- Local commands stamp `clocks.<field> = now` unconditionally (e.g.
  `command.rs:488`) — **no max with the existing clock**: an edit
  after an echo-inflated clock carries the honest (lower) stamp. This
  is what stages interleaving (i).
- Actor seam (fact 10, F11-01 landed as `29b1acb`):
  `SyncActorConfig.clock: Arc<dyn Clock>` (`actor.rs:49-61`); the
  cycle's `read_at = cfg.clock.now()` (`actor.rs:398`, rides
  `SyncReport::Completed`/`Failed` verbatim, `messages.rs:491/:521`);
  the listed-but-unreadable dead-board synthesis stamps
  `deleted_at = Some(clock.now())` and
  `last_modified = max(listed_lm, clock.now())` (`actor.rs:994-997`,
  audit C5's laundering site); the dead-board stacks carve-out turns
  404 listings into empty ones (`actor.rs:1165-1174`).
- Wiremock A-series harness: `cfg()` already injects
  `FixedClock(ts(T + 1000))` (`actor_wiremock.rs:39-51`);
  `spawn_with_cfg` (`:238-264`), `nudge_and_report` (`:268-275`),
  `next_report`'s wall-deadline + paused-time stepping (`:73-86`);
  `T = 1_700_000_000` (`common/hermetic.rs:25`); the absent-board
  tombstone precedent a11 (`:740-772`). wiremock's unstubbed default
  answer is 404; the capabilities probe exploits this (CONSERVATIVE).
- Issue #133: OPEN, `architecture` + `p2`; already carries the DR-4
  design-round comment noting F11-01 done (`29b1acb`). Chunk S's
  comment (§6) adds the pins-landing note and the operational
  trigger — it must not duplicate the F11-01 prose.

## 3. The merge-tier pins (three tests in `merge.rs` `mod tests`)

Placed under a `// ---- DR-4/S: the skew interleaving pins (D-4b,
#133) ----` separator, in the fixture vocabulary of the existing
module. Every scenario's stamps are exact values (the prompt's rule:
values, not "ahead by a bit"); each test's doc comment states the
real-time facts so the skew lie is visible to a reader, not just the
numbers. `ctx` is `RemoteIndex::default()` and `native_scheduled` is
`true` throughout (no scenario touches `scheduled`).

### S1 — interleaving (i)-residual: the echo shadow

```
remote_stamp_beats_newer_local_edit_currently_pins_skew_echo_shadow_residual
```

Staged through the echo (D3) so the pin is the named interleaving,
not generic LWW:

1. **First edit** — title `"edited once"`, `clocks.title =
   ts(BASE + 300)` (real 10:05:00).
2. **The push echo, server 30 s fast** — `echo = remote_task(1, 1,
   BASE + 330)`; `after_echo = adopt_task_after_push(&edited, &echo,
   &ctx, true)` ⇒ `clocks.title == ts(BASE + 330)`: the skew horizon
   sits 30 s in the local future (C4's `max`). Assert the inflation.
3. **Second local edit at real 10:05:10** — title `"edited twice"`,
   `clocks.title = ts(BASE + 310)` (the engine's honest `now`, below
   the horizon the echo left — `command.rs` stamps, no max).
4. **The skewed remote write** — a web edit that really happened at
   10:05:05, i.e. *before* the second edit; the 30 s-fast server
   stamps it `remote_task(1, 1, BASE + 335)` with
   `remote.title = "edited on the web"` (distinct from the echo's
   title so the winner's provenance is unambiguous).
5. `merge_task(&edited_twice, &remote, &ctx, true)`.

Assert: `merged.title == "edited on the web"` (the real-time-earlier
write won), `merged.clocks.title == ts(BASE + 335)`.

**Why `currently_pins_`**: real-time order (remote 10:05:05 < local
10:05:10) is invisible to the algebra — only stamp order decides
(C1/C2). Accepted-by-design loss class (audit F11-02, P2); note in
the doc comment that D-4c's Δ-band guard does *not* cover this
interleaving (it guards only the resurrect comparisons), so this is
residue that survives even a triggered guard — exactly why it reads
as permanent debt. Self-healing bound: the next local title edit
re-wins (audit §skew (b)).

### S2 — interleaving (ii): the resurrect flip, both directions

```
resurrect_flip_under_skewed_stamps_currently_pins_skew_resurrect_flip
```

One test, two acts (D4; the design's single name covers both
directions; house precedent
`scheduled_merges_per_field_lww_both_directions`):

- **Act A — server fast ⇒ wrongly resurrects.** Local tombstone at
  real 10:06:00, honest engine stamp (C3): `bound_task(1)` with
  `deleted = true`, `clocks.deleted = ts(BASE + 360)`. A remote write
  that really happened at 10:05:50 (before the delete), server
  +30 s ⇒ `remote_task(1, 1, BASE + 380)`. `merge_task` ⇒ assert
  `!merged.deleted`, `merged.clocks.deleted == ts(BASE + 380)`,
  `merged.remote == Some(card_ref(1, 1))`. Doc-comment fact worth
  writing down: the resurrect *stamps the tombstone clock with the
  lying value* (`:253`), compounding the skew for the next
  comparison. Pipeline-side consequence (out of scope here): the
  pending `DeleteTask` re-push is dropped (fact 6).
- **Act B — local fast ⇒ stale tombstone shields itself.** Local
  clock 60 s fast: the real 10:06:00 delete is stamped
  `clocks.deleted = ts(BASE + 420)` (10:07:00). A genuinely fresher
  remote edit at real 10:06:30, honest server stamp ⇒
  `remote_task(1, 1, BASE + 390)`. `merge_task` ⇒ assert
  `merged.deleted` stands, `merged.clocks.deleted == ts(BASE + 420)`
  unchanged, and `merged.remote_seen == Some(ts(BASE + 390))` — the
  fresh change was observed but can never land until server stamps
  pass 10:07:00 (bounded self-healing; the audit's "tombstone could
  never be adopted" hazard, the mirror C5 protects boards against).

**Why `currently_pins_`**: R5's comparison is correct given honest
clocks — the existing pins `newer_remote_edit_resurrects_local_
tombstone` / `older_remote_edit_keeps_tombstone` already pin exactly
that. S2 pins the same comparison under lying stamps; this is the
interleaving D-4c's guard targets (act A only — `ts_r >
clocks.deleted + N`; act B would need remote-age bias, rejected), so
the pin doubles as the acceptance oracle a triggered guard is
measured against.

### S3 — seconds-tie (iii): contract, plain name

```
same_second_remote_write_loses_to_sub_second_local_edit
```

- **Local edit** — title `"edited locally"`, `clocks.title =
  ts(BASE + 330) + chrono::TimeDelta::try_milliseconds(400).unwrap()`
  (10:05:30.400 — the engine's sub-second `Utc::now()`; built inline
  because `merge_testutil::ts` is whole-second and must not grow a
  helper, D12).
- **Remote write genuinely later in real time** (10:05:30.900),
  truncated to Deck's whole-second wire stamp ⇒
  `remote_task(1, 1, BASE + 330)` (10:05:30.000).
- `merge_task` ⇒ assert `merged.title == "edited locally"` (local
  stands — `remote_wins(10:05:30.000, 10:05:30.400)` is false),
  `merged.clocks.title` still the sub-second stamp (an unapplied
  observation advances nothing).

**Why contract, no `currently_pins_`**: this is ADR 0004's explicit
tie-break ruling (strict `>`, ties keep local) interacting with
Deck's documented seconds resolution (audit C9 / §skew (iii); the
push wire truncates to seconds). The direction is consistently
local-favoring — safe-leaning by design. If it ever changes it is a
deliberate ADR amendment, not tolerated debt; the doc comment cites
the ADR so the distinction survives.

## 4. The Clock-seam demo (actor tier, hermetic wiremock)

```
a18_sync_actor_stamps_come_from_the_injected_clock_when_skewed
```

in `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`
(`#[tokio::test(start_paused = true)]`, one fn, two acts — D7/D8;
the `a5_a6_…` combined-test precedent). Behavior under test is one:
every stamp the actor mints flows from the injected seam, even when
the seam is skewed — the audit appendix's "hermetic suites run on
the real clock" no longer constrains skew tests because F11-01
landed the seam (design D-4b's explicit supersession).

**Mount set per act** (cannot reuse `mount_standard_pull` — its
board detail answers 200): boards listing 200
(`boards_json(false, …)` — board 42 listed, live flags); board
detail 404; both stacks listings 404. The 404 detail hits the
listed-but-unreadable dead-board synthesis (`actor.rs:974-998`) and
the stacks carve-out empties the listings (`:1165-1174`) — the cycle
completes with the dead board in the snapshot (a11 proves the
sibling absent-board path completes). Capabilities probe unstubbed ⇒
404 ⇒ CONSERVATIVE, as everywhere in the suite. Repo seed:
`repo_with(&[(2, STACK)], &[])` — bound board + stack, no drafts.

- **Act 1 (ahead, δ = +1000 s)** — `FixedClock(ts(T + 1000))`, fresh
  server. `spawn_with_cfg`, nudge, take `SyncReport::Completed`.
  Assert exact equalities:
  - `read_at == ts(T + 1000)` — provenance proof: a wall clock
    returns 2026-era instants, never this value;
  - `snapshot.board.deleted_at == Some(ts(T + 1000))` — the seam
    stamp laundered into a remote-view field (audit C5's hazard,
    deterministic);
  - `snapshot.board.last_modified == ts(T + 1000)` — the ahead seam
    wins the synthesis `max`.
- **Act 2 (behind, δ = −3600 s)** — fresh server,
  `FixedClock(ts(T - 3600))`. Same mount, same assertions except:
  - `read_at == ts(T - 3600)`;
  - `snapshot.board.deleted_at == Some(ts(T - 3600))`;
  - `snapshot.board.last_modified == ts(T)` — the `max` floor: a
    behind local stamp cannot lower the listed stamp (the documented
    reason the synthesis maxes, `actor.rs:988-996`).

All assertions are exact equalities on typed report fields — no
actor internals (the harness doc's assertion-surface rule), no text
assertions, no wall clock consulted anywhere; the only time movement
is the harness's virtual stepping (`next_report`, `start_server`)
under real-loopback deadlines.

## 5. Explicit non-goal

D-4c (the Δ-band guard: `ts_r > clocks.deleted + N`,
`[sync] skew_band_secs` default 2) is **specified, not built** — its
complete implementation spec lives in the DR-4 plan §3 D-4c and is
built only when the §6 trigger fires (fork F-133's ruling). No
config surface, no guard code, no guard tests in this chunk.

## 6. The #133 comment (D-4a — posted by stage 3, issue stays OPEN)

Stage 3 posts this verbatim after checks go green (dispatch prompt
S.3). It adds to the existing DR-4 comment instead of repeating it.

> **Chunk S (DR-4/S, `test/sync-skew-pins`) — the interleavings are
> now pinned by tests.**
>
> The F11-01 half was already verified above (`29b1acb`). This chunk
> lands the other half of D-4: the audit's skew interleavings are
> pinned so any change to the documented behavior fails CI and the
> debt stays visible instead of silent:
>
> - `…_currently_pins_skew_echo_shadow_residual` (merge.rs) —
>   interleaving (i)-residual: a second local edit inside the
>   echo-inflated skew window loses to a remote write that precedes
>   it in real time. Note: not covered by the Δ-band guard even if
>   it is ever built (that guard only arbitrates resurrect
>   decisions) — this residue is permanent, by design.
> - `…_currently_pins_skew_resurrect_flip` (merge.rs) —
>   interleaving (ii), both directions: a server-fast write
>   resurrects a card deleted after that write in real time; a
>   local-fast tombstone shields itself from genuinely-fresher
>   remote edits.
> - `same_second_remote_write_loses_to_sub_second_local_edit`
>   (merge.rs) — interleaving (iii) pinned as **contract** (ADR
>   0004's tie-break over Deck's seconds resolution), not debt.
> - A18 (sync-nextcloud wiremock tier) — proves the actor's stamps
>   (`read_at`, synthesized dead-board tombstones) flow from the
>   injected `Clock` seam under ±skew, so any future guard is
>   testable at both tiers.
>
> **The trigger that would ever reopen the guard** (operational form
> of the body's conditional): real-server use showing
> revert/resurrect-on-sync symptoms matching interleaving
> (i)-residual or (ii) — concretely, a card visibly returning after
> deletion, or a local edit reverting after a sync, on this device
> against the user's server. Record the observation as a comment
> here with the sync logs; the response is building D-4c exactly as
> specified (DR-4 plan §3 D-4c: `[sync] skew_band_secs`, default 2,
> at the two resurrect-decision comparisons; it fixes the
> server-fast direction only). Absent such an observation this
> issue stays open as the debt marker and nothing is built.

## 7. Decision record (every open question, closed)

- **D1 — placement**: the three merge-tier pins live in
  `merge.rs`'s existing `mod tests` (same crate, fixture vocabulary,
  mutants/proptest tooling untouched); the demo is A-series test
  `a18` in the existing wiremock suite. No new files.
- **D2 — values are the spec**: every stamp is an exact
  BASE-relative second (or sub-second composition), never "ahead by
  a bit"; each test's doc comment spells the real-time vs stamp
  order so the skew lie is auditable.
- **D3 — (i) staged through the echo**: the echo-inflation step
  (`adopt_task_after_push`, asserted) is part of the scenario —
  without it the pin would be indistinguishable from generic LWW and
  would not be the audit's named interleaving.
- **D4 — (ii) is one test, two acts**: the design's single
  `…_skew_resurrect_flip` name covers both directions; house
  precedent supports the combined form.
- **D5 — (iii) is contract**: plain name, ADR citation in the doc
  comment, no `currently_pins_`.
- **D6 — no src/ doc-comment fix**: the stale "skew-free by
  construction" module doc (`merge.rs:22-24`) is corrected by chunk
  G, not here (§1). Reviewer-visible decision, not an oversight.
- **D7/D8 — demo shape**: A18, one fn, two acts (ahead +1000 s /
  behind −3600 s), explicit mount set (boards 200; detail, stacks,
  archived-stacks 404); exact-equality assertions make wall-clock
  provenance impossible; report fields only, never actor internals.
- **D9 — `merge_testutil` untouched**: it is compiled unconditionally
  (`lib.rs:62`), so adding a sub-second `ts` variant would be a src
  hunk; S3 builds its stamp inline with chrono.
- **D10 — bookkeeping ownership**: stage 2 does not touch #133;
  stage 3 posts the §6 comment post-green; the issue stays OPEN
  (closing it is the archive act for when the guard question is
  actually resolved — D-4a).
- **D11 — no ADR/docs amendment**: D-4 rules "no amendment — no
  decision changes"; the chunk's only docs output is this plan and
  the §6 comment.

## 8. Risks

- **Later chunks vs the pins**: S pins the semantics that G/L/D
  deliberately keep — G's changed-field gate preserves today's
  behavior bit-for-bit for baseline-less tasks (D-1b's fallback
  rule), and every pin scenario is baseline-less by construction, so
  the pins survive G unchanged; L touches labels (untouched here);
  D touches the R3 `tombstone_task` path, not the R5 comparison. If
  a later chunk does move a pinned outcome, the pin fails by design
  and forces the conscious re-name/amendment conversation — that is
  the pack working, but stages 2/3 of those chunks must treat pin
  failures as findings, never as snapshot churn to regenerate.
- **Demo coupling**: A18's secondary assertions encode the dead-board
  synthesis's `max` semantics (production behavior, untouched by the
  batch). If those change, the primary `read_at` assertion still
  isolates seam provenance.
- **Timing**: no new timing surface — the harness's
  wall-deadline + paused-stepping invariants carry over; nothing
  sleeps, nothing polls the wall clock.
- **Review-gate strictness**: the tests-only rule is enforced
  byte-level (D6/D9 exist to keep src hunks at zero); the plan is
  the reviewer's reference for the two deliberate non-fixes.

## 9. Diff estimate & acceptance

- `crates/taskboard-domain/src/merge.rs` (tests module only):
  ~150–190 added lines (3 tests + separator + doc comments).
- `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`:
  ~90–120 added lines (A18 + its mount helper).
- Total ≈ **240–310 added lines, 0 deletions, 0 production hunks**.

Acceptance (dispatch §4.2, verbatim-checked): diff is tests and
nothing else; each `currently_pins_` name matches the interleaving
it pins and would fail under changed behavior; the seconds-tie test
is a plain contract test; the Clock-seam demo proves stamp
provenance under pause/advance with no wall clock; existing merge
properties and proptests pass unchanged; the full AGENTS §10 gate
suite is green; #133 commented per §6, still OPEN.

## 10. References

- DR-4 design plan §3 D-4, §2 facts 10–11, §4 F-133, §6 chunk S.
- Dispatch plan §1 (stage protocol, lanes), §4.2 (this chunk).
- Audit §skew: C1–C9 map, interleavings (i)–(iii), the P2 verdict
  and standing trigger.
- Issue #133 (+ its existing DR-4 comment), issue #480.
- ADR 0004 (the contract pinned, unamended).

# [Plan] Sync Correctness + Clock Seam — Stale-Binding Re-resolve & `Arc<dyn Clock>` Injection

- **Date**: 2026-10-17
- **Kind**: stage-1 plan for chunk 5 of the remediation-wave-1 dispatch
  (dispatch §1/§6, prompt 5.1). Planning only — no implementation in
  this commit.
- **Origin**: issues #258 (F10-02 — the audit's one live sync bug;
  agenda row M-1, **schedule-now**) and #259 (F11-01/F11-02 — the sync
  actor's clock seam; agenda row L-1, **schedule-now**, prerequisite
  for any skew work), both normative. Owning reports:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-sync-persistence-report.md`
  (F10-02 · §C07-11 interleaving) and
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-time-dst-report.md`
  (F11-01 · §census rows 127–134, §skew). The ADR note lands in
  `docs/adr/0007-sync-actor-push-first-cycle.org`.
- **Branch / worktree**: `fix/sync-stale-binding-clock` in
  `../taskboard-WT-fix-sync-stale-binding-clock`, cut off `main` @
  `2884a6a` (= `origin/main`).
- **Anchor base**: all `file:line` anchors below are against `2884a6a`
  (post chunk-1 rename, post chunk-2 test pinning — chunk 2's PR has
  landed; its tests are the floor, §floor).
- **Size**: M (per the dispatch's grade).

---

## §bug — what exactly is broken (F10-02, issue #258)

The push executor's binding overlay keys a task's remote ref as
`(board, stack, card)` (`push_exec.rs:29-75`; the stack rides in
`task_by_local`). Deck card ids are unique per board, and a remote
cross-stack move changes only the ref's stack component — the pull
side already matches identity by `(board, card)` and deliberately
ignores the stack for exactly this reason
(`taskboard-domain/src/pipeline.rs:499-513`). The push side kept the
stack in the ref and has no re-resolve.

The interleaving that loses a queued edit:

1. Another client moves card X from stack A to stack B after our last
   pull (the overlay's ref still says A).
2. The local user edits X → `UpdateTask` op queued (durable), the
   task table carries the edited values.
3. Next cycle: the overlay resolves X at (A, X); the pre-write fetch
   `GET boards/<b>/stacks/A/cards/X` answers **404**. The fetch-error
   arm classifies with `PushCtx { existing: true, delete: false }`
   (`push_exec.rs:539-548`), and `mapping.rs:155-161` maps
   `NotFound + existing` to `PushResult::RemoteMissing`. Same shape in
   `move_task`'s fetch arm (`push_exec.rs:764-775`).
4. The engine's R9e arm tombstones X and **cancels the queued edit**
   (`pipeline.rs:1046-1073`); `tombstone_task` clears the binding
   (`merge.rs:268-282`).
5. The same cycle's pull lists X under stack B; with the binding
   cleared the card re-adopts as a **new** task with the remote field
   values (`pipeline.rs:388-398` builds the match map from the
   post-echo task map; `:558-569` adopts unmatched cards).

Net: the raced local edit is silently lost and a tombstone twin
remains. ADR 0007's clause "stale-binding 404s are never misread as
`RemoteMissing`" (`0007.org:84-86`) fails for the move case — the
audit's C07-11, verdict *partially-implemented*.

## §decision-1 — re-resolve, not defer (the issue's two remedies weighed)

The issue offers two shapes; **this chunk implements the re-resolve**.

**Chosen — re-resolve once against a fresh listing before
classifying.** On a `DeckError::NotFound` from the *pre-write fetch*
in `update_task` / `move_task`:

1. Re-resolve: `client.stacks(board, StackFilter::Active)`
   (`client.rs:511-526`); if the card id is not among the listed
   cards (`model.rs:95` — stacks carry their cards), query
   `StackFilter::Archived` next (a moved card may land in an archived
   stack; the pull merges both listings for its presence truth,
   `actor.rs:656-680`, so the re-resolve must consult the same two
   sources).
2. **Found** at stack S → refresh the overlay
   (`task_by_local[task] = (S, card)`) and **refetch once** at the
   fresh location (`client.card(board, S, card)`):
   - `Ok(card)` → the group continues its normal flow with the
     fetched card (update PUT / move reorder, unchanged).
   - `Err(NotFound)` → `RemoteMissing` (the detail read outranks the
     lagging listing — tier-2 verified that listings lag Deck's
     cache).
   - `Err(other)` → classify through the normative table with the
     same `PushCtx { existing: true, delete: false }`.
3. **Absent** from both successful listings → `RemoteMissing` — now
   truthful: absence was confirmed against the same listing pair the
   pull trusts.
4. **Listing error** → the listing's error *replaces* the original
   404 through `mapping::classify_push` with
   `PushCtx { existing: true, delete: false }`: transport class →
   `GroupOutcome::Aborted` (decision 8 — no outcome, cycle stops);
   listing `NotFound` (board gone) → `RemoteMissing`; `Forbidden` /
   auth / server → their rejections (retryable; the pull's dead-board
   machinery owns the actual dead-board cascade). No new outcome
   kinds; the re-resolve never widens the classification table.
5. **No loop**: at most one re-resolve + one refetch per group
   execution.

**Why re-resolve over the defer alternative** (`Rejected { LocalData }`
on pre-write-fetch 404s, letting the pull re-bind first):

- *Defer leaves the edit's survival to LWW.* A deferred op stays
  queued (`pipeline.rs:1103-1111` — only `BadRequest` dead-letters),
  but its payload is resolved from the **current task table** at the
  next plan (`LocalOp::UpdateTask(_)` carries just the id;
  `plan_pushes` materializes from current tables). The self-clobber
  guard protects only ops queued *after* `read_at` — the raced edit
  was queued before it — so in the very same cycle the pull's merge
  compares the edit's field clocks against the move's `lastModified`
  by plain LWW, and whenever the remote move stamps newer than the
  local edit (move-after-edit, a real interleaving) the edited values
  are overwritten in the table *before* the next cycle can push them.
  The edit is lost with the exact silence F10-02 is about. Re-resolve
  lands the edit in **this** cycle from the fetched card, independent
  of clock ordering.
- *Re-resolve keeps `RemoteMissing` truthful* (classified only after
  a fresh listing confirmed absence); defer would make it permanently
  unreachable from the pre-write fetch — a weaker signal for
  diagnosing genuine deletions.
- It restores the module's own documented promise (`push_exec.rs:8-11`)
  and mirrors the pull side's identity rule (board, card).
- The genuine-deletion outcome converges to today's terminal state:
  fetch 404 → listing confirms absence → `RemoteMissing` → R9e
  tombstone + cancel. No behavior change where the 404 was honest.

**Write-path 404 arms stay unchanged** (the PUT arm
`push_exec.rs:602-611`, the reorder arm `:800-808`, the label
sub-endpoint arms `:636-657`/`:674-689`): those fire only *after* a
successful fetch, so the binding was fresh at fetch time — a 404 there
is a genuine mid-flight disappearance and `RemoteMissing` is the
honest verdict. (The PUT is not stack-addressed — the body's stack
comes from the fetched card; the reorder path carries the *fetched*
stack.) This is the "no other classification changes" non-goal made
precise.

## §decision-2 — no config knob

The re-resolve is always on. It costs at most three extra requests
(active + archived listing + refetch) and **only on a pre-write-fetch
404** — the rare path where today's code already loses data. A knob
would be dead config surface with no user story: this is the
classification policy, not an option (ADR amendment says so, §adr).
`SyncActorConfig` gains no behavioral switch for it.

## §decision-3 — the Clock seam: reuse the domain `clock` module

**Trait**: `taskboard_domain::clock::Clock` as-is
(`clock.rs:17-30` — `Send + Sync + Debug`, `now(&self) ->
DateTime<Utc>`; `SystemClock` reads `Utc::now()`). A sync-local trait
is rejected: the engine already runs on this exact seam, the sync
crate already depends on `taskboard-domain`, and a second trait would
duplicate the one abstraction the skew backlog (F11-02, later round)
needs to manipulate both actors through. Recorded per the dispatch
prompt's "prefer the domain `clock` module if it fits" — it fits with
zero changes.

**Injection point**: a new field on `SyncActorConfig`
(`actor.rs:39-49`), per the normative issue:

```rust
pub struct SyncActorConfig {
    pub poll_interval: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    /// Wall-clock seam for every LWW-participating stamp the actor
    /// mints (read_at, dead-board tombstones, archive-mismatch
    /// corrections). Production injects SystemClock; tests a fake.
    pub clock: Arc<dyn taskboard_domain::Clock>,
}
```

The `Debug`/`Clone` derives hold (`Clock: Debug`, `Arc: Clone`).
`cycle()`'s `_cfg` parameter (`actor.rs:299`) becomes used; the
clock reaches `pull_snapshot` (`actor.rs:529-739`) as a parameter
threaded from `cfg`.

**The seven wall reads and their conversions** (verified exhaustive:
`grep Utc::now()` in the crate's `src/` finds exactly these seven
production sites; the four in `push_exec.rs` are inside `mod tests`):

| Site | Stamp | Becomes |
|---|---|---|
| `actor.rs:310` | `read_at` (self-clobber guard input + report envelope) | `cfg.clock.now()` |
| `actor.rs:342` | `NoBoard` failure report's `read_at` | `cfg.clock.now()` |
| `actor.rs:591/:592` | dead-board tombstone `deleted_at` / `last_modified` floor | `clock.now()` (threaded) |
| `actor.rs:617/:618` | synthesized tombstone (board gone 404/403) | `clock.now()` (threaded) |
| `actor.rs:717` | archive-mismatch correction `last_modified` floor | `clock.now()` (threaded) |

The same-clock-by-convention comment at `actor.rs:308-309` is
replaced by a note that the stamps flow through the injected seam.

**App wiring**: `AppConfig::sync_actor_config()`
(`taskboard-app/src/config.rs:638-645`) gains
`clock: Arc::new(taskboard_domain::SystemClock)`. *Recorded
mechanics deviation from the issue's "pass a clone (of the
bootstrap's engine clock, bootstrap.rs:585)"*: `SystemClock` is a
stateless zero-sized struct — a fresh `Arc` reads the identical wall
source — and the attach path (`bootstrap.rs:293`) builds its config
from `AppConfig` where the boot function's engine-clock `Arc` is not
in scope; threading it through `App` would add plumbing for zero
observable difference. Both spawn sites keep calling
`self.config.sync_actor_config()` unchanged.

**Compile ripples** (all mechanical, compiler-driven): the doctest in
`spawn_sync_actor`'s docs (`actor.rs:96-100`); `const CFG:
SyncActorConfig` literals in `tests/actor_wiremock.rs` (~:55),
`tests/e2e_hermetic.rs` (~:44), `tests/common/sync_pair.rs` (~:19) —
`Arc<dyn …>` is not const-constructible, so each becomes a
`fn cfg() -> SyncActorConfig`; `tests/actor_detach.rs` (~:38) and any
other construction site the compiler names. **Floor guarantee**: the
*test bodies* of chunk 2's suite stay byte-identical — only the
shared `CFG` initializer changes shape (§floor).

## §decision-4 — fake-clock test conversions

**The fake**: `FixedClock { at: DateTime<Utc> }` in the sync crate's
`tests/common/mod.rs` (house pattern: per-crate test fakes —
`taskboard-state`'s `tests/common/mod.rs:38` `FixedClock` is the
precedent, not importable across crates; domain `test_support` is for
domain-level fakes). Instant pinned at `ts(T + 1000)` — strictly after
every fixture stamp (base `T = 1_700_000_000`): both hermetic suites
seed `queued_at: ts(T)` (`actor_wiremock.rs:152`,
`e2e_hermetic.rs:784`) and fixtures stamp remote entities at `T`, so
today's real-now-dominates orderings (ops seen by the cycle:
`queued_at <= read_at`; local write clocks beat remote fixture
stamps in LWW) are preserved deterministically. If a stepwise clock
ever becomes necessary, add it then — no speculative generality now.

**Conversion scope**:

- `tests/actor_wiremock.rs` (A-series, hermetic): `cfg()` injects the
  fixed clock. Existing assertions don't pin wall values (dead-board
  tests assert `deleted_at.is_some()`), so no floor test changes.
- `tests/e2e_hermetic.rs` (I-series, full engine+actor pair): engine
  **and** actor share **one** fake `Arc` (replacing the engine's
  `SystemClock` at `:65` and the actor's config clock). Splitting the
  pair (real engine clock, fake actor clock) would distort the
  self-clobber boundary `queued_at <= read_at` — never do that.
- `tests/actor_detach.rs`: the fixed clock (uniformity; it observes
  no stamps).
- `tests/common/sync_pair.rs` (D-series, docker-tier) and the live
  suites (`it_docker.rs`, `it_live.rs`): keep `SystemClock` injected
  explicitly through the seam. These tiers run against real servers
  where real timing is the subject; the seam makes the choice
  explicit, which is all #259 asks of them.
- In-crate `mod tests` in `push_exec.rs` keep their `Utc::now()`
  *test-data* stamps (op `queued_at` in the fixture helpers): they are
  inputs the executor never compares, not actor wall reads; the seam
  mandate covers the actor's stamps.

**Contingency**: if an I-series assertion flips under the pinned fake
(it would have been pinning real-now dependence — a current-behavior
pin), stage 2 stops and reports it rather than weakening the test
(AGENTS §5); the fallback is keeping that suite on explicit
`SystemClock` with the deviation recorded in the stage-2 report.

## §regression-tests — the catalogue (TDD; one behavior, one test, lowest layer)

**Executor layer** — in-crate `mod tests` of `push_exec.rs`
(wiremock-backed `DeckClient`, the established pattern; these sit
beside chunk 2's floor tests and reuse its fixture helpers). The two
marked REGRESSION must fail on current code (stage 2 proves it
before the fix lands, then the test+fix land as one standalone
commit):

| # | Test (name sketch) | Fixture shape | Typed assertion |
|---|---|---|---|
| E1 | `update_preflight_404_reresolves_the_card_from_a_fresh_listing` — **REGRESSION** | overlay (A, X); `GET A/X`→404; active listing shows X in stack B; `GET B/X`→card; PUT→200 echo | outcomes = primary `PushResult::Applied { echo: Some(RemoteEcho::Task(ref)) }` with the echo ref's **stack == B**; `executor.overlay()` resolves the task to B afterwards |
| E2 | `move_preflight_404_reresolves_then_reorders_from_the_fresh_location` — **REGRESSION** | overlay (A, X), move to (S2, 5); `GET A/X`→404; listing→X at B; `GET B/X`→card (order 0); reorder request | outcome `Applied { echo: None }`; the reorder request's path carries the **fetched** stack B (wiremock path matcher) and the destination S2/5 (body matcher) — the stale A appears in no request after the re-resolve |
| E3 | `preflight_404_with_the_card_absent_from_both_listings_classifies_remote_missing` | 404 at A; both listings without X | primary `RemoteMissing` (pins that the fix is not a blanket defer; passes today, must keep passing) |
| E4 | `preflight_404_with_a_lying_listing_maps_the_refetch_404_to_remote_missing` | 404 at A; listing claims X at B; `GET B/X`→404 | primary `RemoteMissing` (detail outranks the lagging listing) |
| E5 | `preflight_404_with_a_transport_failing_listing_aborts_the_cycle` | 404 at A; listing → transport error | `GroupOutcome::Aborted { landed: [] }` (today: misclassified `RemoteMissing` — this test also fails first) |
| E6 | `preflight_404_with_a_forbidden_listing_rejects_forbidden` | 404 at A; listing → 403 | primary `Rejected { kind: Forbidden }` (`matches!`, no text) |

E5 note: it fails on current code too — it is part of the bug proof,
not a floor break.

**Actor layer** — `tests/actor_wiremock.rs` (A-series numbering
continues after `a17`), one scenario test that reproduces the issue's
interleaving end-to-end at routing level only:

- `a18_remote_moved_card_survives_its_queued_edit` — **REGRESSION**,
  the issue's exact scenario: repo seeded with a bound board, task X
  bound at `(BOARD, STACK_A, X)` carrying the edited title, one
  `UpdateTask` op queued at `ts(T)`; mounts: the conditional pull
  pair (`mount_conditional_pull` with the listing showing X under
  `STACK_B`), `GET …/stacks/A/cards/X`→404, `GET …/stacks/B/cards/X`→
  card detail, `PUT …/cards/X`→200 echo. Nudge → await the
  `Completed` report. Assert (typed): the report's `pushes` contain
  the op's outcome as `Applied` with an echo whose stack is B (and no
  `RemoteMissing` among the pushes — structural, not text);
  `snapshot.tasks` contains X with `id.stack == B`. The engine's
  response to `RemoteMissing` (tombstone + cancel + re-adopt twin) is
  domain-tested already — this test proves the actor never sends that
  verdict for a stale binding, which is the whole bug.

**Clock-seam tests** — `tests/actor_wiremock.rs`, one per stamped
site family (deterministic stamps are the seam's observable):

- `a19_dead_board_tombstone_stamps_come_from_the_injected_clock` —
  the listed-but-unreadable board path (`:591/:592`): fake at
  `T+1000`; assert the report's snapshot board `deleted_at ==
  Some(ts(T+1000))` and `last_modified >= ts(T+1000)`.
- `a20_report_read_at_comes_from_the_injected_clock` — commanded
  cycle with no board bound (`:342`, and `:310` rides the same
  seam): assert the `Failed { NoBoard, read_at }` report's `read_at
  == ts(T+1000)`.
- `a21_archive_mismatch_stamp_uses_the_injected_clock` — a bound card
  whose detail disagrees with the local archived flag (`:717`):
  fixture stamp `T` < fake `T+1000`; assert the snapshot task's
  `last_modified == ts(T+1000)` (the max floors at the fake instant).

## §adr — ADR 0007 amendment wording (two sections, after the existing 2026-10-17 amendment)

1. *Amendment (2026-10-17): stale-binding pre-write-fetch 404s
   re-resolve once before classification (#258)* —

   > The "stale-binding 404s are never misread as `RemoteMissing`"
   > clause now holds for the remote-move case. The push-side ref
   > carries the stack component, so a card another client moved
   > 404s its pre-write fetch at the stale location. On a
   > pre-write-fetch 404 in update/move, the executor re-resolves
   > the card once against the fresh active+archived stack listings
   > — the same presence truth the pull trusts; card identity is
   > (board, card), the stack component ignored, mirroring the
   > pull-side match. Found → the overlay binding is refreshed and
   > the fetch retried once at the fresh location; absent from both
   > listings → `RemoteMissing`, now confirmed-truthful; the
   > listing's own failure classifies through the normative write
   > table (transport aborts the cycle; a board-gone listing 404
   > maps to `RemoteMissing`). Write-path 404s *after* a successful
   > fetch — a genuine mid-flight disappearance — remain
   > `RemoteMissing`. There is no configuration knob: this is the
   > classification policy.

2. *Amendment (2026-10-17): the cycle's wall stamps flow through the
   injected `Clock` seam (#259)* —

   > The actor's wall reads — `read_at`, the `NoBoard` report stamp,
   > the dead-board tombstone stamps, the archive-mismatch
   > correction stamp — now read `SyncActorConfig.clock: Arc<dyn
   > taskboard_domain::Clock>`, the same seam the engine uses,
   > replacing the former same-clock-by-convention note. Production
   > injects `SystemClock` (stateless: engine and actor read one
   > wall source); the hermetic suites inject a fixed clock, making
   > every LWW-participating stamp deterministic. Skew mitigation
   > itself remains unimplemented — the backlog entry's conditional
   > trigger stands; this seam is its prerequisite.

Style follows the existing amendment section (`0007.org:159-164`):
dated heading, issue cross-ref, original wording kept verbatim above,
decision-scope statement only (no rewrite of the ADR's decision).

## §non-goals

- **No skew-mitigation logic** — F11-02's remedy (merge-level
  tie/keep-local over raw age bias) is a later round; this chunk only
  opens the seam. No `SyncActorConfig` knob, no skew detection, no
  stamp-adjustment code.
- **No other classification changes** — the write-path 404 arms, the
  delete arms, and the label sub-endpoint arms keep today's
  classification (§decision-1 records why each is already honest).
- **No pull-side, pipeline, or domain production changes** — the fix
  is sync-crate-internal; the domain's `(board, card)` identity, R3,
  and R9e arms are untouched.
- **No engine clock changes** — the engine's seam already exists.
- **No `push_exec` module split** (chunk 10), **no client helper
  dedupe** (chunk 9) — those chunks own those shapes.
- **Noticed, out of scope → follow-up issue** (wave bookkeeping):
  `delete_card` (`push_exec.rs:811-839`) has no pre-write fetch and
  deletes at the overlay's stale stack; a remote move before a local
  `DeleteTask` therefore 404s → `RemoteMissing` → tombstone + the
  same-cycle pull re-adopts a twin — the same silent-loss shape for
  *delete* intent. The issue's remedy scopes this chunk to
  `update_task`/`move_task`; stage 2/3 files this as a new GitHub
  issue (AGENTS §7 conventions) rather than expanding here.

## §floor — chunk 2's tests (landed on main, must pass unmodified)

`main` @ `2884a6a` already carries chunk 2 (commits `a834263`,
`4a74d60`, plus review follow-ups `a0b1999`, `36047ca`, `9f1d720`,
`8dab663`, `299f4cf`). The floor:

- `push_exec.rs` in-crate tests: `update_defers_when_only_unresolvable_labels_differ`
  (:1574), `update_completes_subsumed_label_ops_by_resolvability`
  (:1625), `move_same_stack_different_order_sends_the_reorder`
  (:1716) — and the pre-existing
  `update_preflights_the_card_then_puts_the_full_body`,
  `move_sends_reorder_only_when_the_position_differs`.
- `actor_wiremock.rs`: `a11_absent_board_is_confirmed_then_tombstoned`
  (:661), `a15_detail_500_fails_the_cycle_without_a_tombstone`
  (:966), `a16_forbidden_detail_while_listed_synthesizes_the_tombstone`
  (:1012), `a17_stacks_404_on_a_live_board_fails_the_cycle` (:1066).

These must pass byte-identical after both changes. The only permitted
touch is the shared `CFG` initializer's compile-forced reshape
(const literal → function; §decision-3). The re-resolve must not
alter any arm these tests pin: none of them exercises a pre-write
fetch 404 with a live card elsewhere (a17 is a *stacks listing* 404
on a live board — the pull path, untouched).

## §execution — stage-2 commit series (each standalone, rebase-merge hygiene)

1. `fix(sync): re-resolve stale-binding pre-write fetch 404s before classifying`
   — E1–E6 + `a18` + the executor change (fetch-arm re-resolve helper
   + `BindingOverlay` rebind) as one commit. TDD order inside the
   commit: write the regression tests, run them against unmodified
   production (E1/E2/E5 and `a18` fail — the recorded bug proof),
   then implement, then green. A failing test is never committed
   alone.
2. `refactor(sync): inject the Clock seam through SyncActorConfig`
   — config field, seven site conversions, `pull_snapshot` threading,
   app config wiring, doctest, `FixedClock`, const→fn ripples,
   A/I-series conversions, `a19`–`a21`. Behavior-neutral by
   construction (stamps still read the same wall source in
   production).
3. `docs(adr): ADR 0007 amendments — stale-binding re-resolve + clock seam`
   — the two amendment sections (§adr), plus a grep for any other doc
   that quotes the 404 clause or the wall-read note
   (`docs/architecture.org`, crate docs) and a pointer-style update
   where they name it.

Gates: full AGENTS §10 suite, plus the chunk-specific check: `grep -rn
"Utc::now()" crates/taskboard-sync-nextcloud/src/actor.rs` returns
zero production hits (test modules excepted) after commit 2.

## §risks

- **Listing-lag residual window**: a card moved so recently that both
  fresh listings omit it still misclassifies as `RemoteMissing` —
  the original shape. Accepted residual: the pull rests on the same
  listings (ADR 0007's snapshot-completeness contract and its
  "server that lies" accepted-risk note); the re-resolve closes the
  stale-*binding* window, which is the observed bug. Recorded in the
  ADR amendment by implication (absence confirmed against the listing
  pair); the stage-3 report states it explicitly.
- **a18 mount complexity**: the scenario needs the conditional-pull
  mounts *plus* card-level mounts (404 at A, detail at B, PUT). If
  `mount_conditional_pull` can't express a card under a second stack,
  extend the hermetic fixture helpers in `tests/common/` (shared
  mechanics only, per that module's charter).
- **I-series conversion flips an assertion** → §decision-4's
  contingency (stop, report, fall back to explicit `SystemClock` for
  that suite with a recorded deviation). The pinned `T+1000` instant
  is chosen precisely to preserve today's orderings, so this is
  expected not to fire.
- **Same-file chain pressure**: chunks 6/9/10 follow in the sync
  chain; this chunk must land (PR open) before them per dispatch
  order. If chunk 6's PR opens first, rebase onto it — the files
  overlap (`actor.rs`, `client.rs`) but the hunks shouldn't.
- **`FixedClock` naming collision**: none — the sync crate's
  `tests/common/mod.rs` has no clock today; the name repeats
  `taskboard-state`'s private helper by design (house pattern).
- **Coverage gate**: new arms are exercised by wiremock-backed tests
  (instrumented binaries like the rest of the crate's suites) — no
  change to the quarantine dance expected.

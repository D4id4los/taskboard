# [Plan] Sync Push/Pull Test Pinning — the Six-Behaviour Catalogue (P1 Trio)

- **Date**: 2026-10-17
- **Kind**: stage-1 plan for chunk 2 of the remediation-wave-1 dispatch
  (dispatch §1/§3, prompt 2.1). Planning only — no implementation in
  this commit.
- **Origin**: issues #292 / #293 / #294 (normative; findings F6-01 /
  F6-02 / F6-03, decision-session rows P1-2/3/4, all **schedule-now**)
  out of the v0.1.0 audit findings report §findings (axis B, test
  coverage/strength). Test-only chunk: **production code must not
  change**.
- **Branch / worktree**: `test/sync-push-pull-pinning` in
  `../taskboard-WT-test-sync-push-pull-pinning`, cut off `main` @
  `7449c1a` (= `origin/main`; chunk 1's rename has landed and does not
  touch the sync crate).
- **Anchor base**: all `file:line` anchors below are against `7449c1a`.

---

## §context — the three seams and what already covers them

### S1 — `update_task` defer/delta chain (`push_exec.rs:519-739`, issue #292)

The preflight GET (:533-553) is followed by the defer decision
(:574-586): when the group's only delta is *unresolvable* labels
(`has_unresolved && !fields_differ && desired == fetched_labels`), the
whole group defers — `GroupOutcome::Outcomes(Vec::new())` (:585), every
member op stays queued. Otherwise the field PUT (:601-616) and the
label-set delta through `assignLabel`/`removeLabel` (:618-695) run, and
the success tail assembles outcomes (:697-738): the primary's `Applied`
plus **one outcome per subsumed label op**, `Applied { echo: None }`
when the op's label is resolvable in the overlay, `Rejected {
LocalData }` when not (:727-736).

Existing suite coverage: the PUT path (`update_preflights_the_card_then_puts_the_full_body`),
the assign/remove sub-endpoint rides, and the preflight-404
classification. **Not covered: the defer conjunction (:574-586) and the
subsumed-label outcome assembly (:706-737)** — the audit's top
mutant-survivor cluster (14 survivors).

### S2 — `move_task` fast path (`push_exec.rs:741-809`, issue #293)

After the preflight GET, `fetched.stack_id == target_stack && fetched.order == to.1`
(:776) short-circuits "already there" to a synthetic `Applied` with
**no** reorder request. The existing
`move_sends_reorder_only_when_the_position_differs` covers the
cross-stack cell ((7,3)→(8,1)) and the same-position cell ((7,3)→(7,3)).
**Not covered: same stack / different order** — the cell where a mutant
on the order conjunct (:776) silently drops the position change and
returns a false "already there".

### S3 — pull dead-board determination (`actor.rs:529-772`, issue #294)

`pull_snapshot` decides liveness from the boards listing (:551-553) and
the board detail (:558-626):

| Arm | Anchor | Behavior |
|---|---|---|
| listed, detail ok but `!is_live` | :565-568 | dead from the detail payload |
| listed, detail `NotFound`/`Forbidden` | :570-594 | **dead — tombstone synthesized from the *listing* entry, stamped now** |
| listed, detail other error (e.g. 500) | :595 | `Err` propagates — cycle **Failed**, no snapshot |
| unlisted, detail ok | :598-603 | liveness from the detail |
| unlisted, detail `NotFound`/`Forbidden` | :604-623 | tombstone synthesized from the local board |
| stacks read `NotFound`/`Forbidden` **and** `dead_board` | :752-761 | carved to an empty listing (tombstone stays valid) |
| stacks read `NotFound`/`Forbidden` on a **live** board | :762 | `Err` propagates — cycle **Failed** |

A11 pins the unlisted+404-confirm cell. **Not covered: the 403-while-listed
tombstone (:571-594), the detail-500 failure fall-through (:595), and the
live-board stacks-404 failure (:762)** — the tombstone-cascade risk: a
transport/server error misread as board death lets the engine's merge
tombstone a live local board (the most destructive failure mode in the
P1 set, per #294's "Why").

### Classification facts the tests rely on (verified @ `7449c1a`)

- `client.rs:301-311`: 403 → `DeckError::Forbidden`, 404 → `NotFound`,
  other 5xx → `Server(code)`; only `RateLimited | Unavailable |
  Transport` retry (`client.rs:1360-1362`) — **a 500 and a 404 are
  single-attempt**, so request censuses are exact (no `.expect(3)`
  needed, unlike the 503 in A4).
- `mapping.rs:190-208` `classify_read`: `Server(_)` and `NotFound` →
  `SyncErrorKind::Server`; `Forbidden` → `SyncErrorKind::Forbidden`.
  (On reads a 404 is a *Server* failure — `RemoteMissing` is a
  push-side concept.)
- `SyncReport::Failed` carries **no snapshot** — the report variant
  itself is the "no tombstone" proof at the actor layer.

## §decisions

### D1 — Layer assignment (one behavior, one test, lowest layer)

- The three push behaviors (T1-T3) are **suite tests**: the in-crate
  `#[cfg(test)] mod tests` at `push_exec.rs:918` — the established
  lowest layer for `PushExecutor` behavior (its own fixture vocabulary:
  `overlay_with`, `op`, `group`, `card_json`, `empty_tables`).
- The three pull behaviors (T4-T6) are **wiremock actor tests** in
  `tests/actor_wiremock.rs` (A-series; they pin `pull_snapshot` arms
  behind the actor's report, which no lower layer exposes — the
  function is crate-private and drives three endpoints).
- Nothing goes through the engine thread or the I-series; no test
  duplicates another's behavior.

### D2 — "Zero requests" means zero *mutating* requests, asserted by census

`update_task`'s preflight GET (:533) precedes the defer decision by
design (fetch-before-write) — "defer → zero requests" (#292) cannot and
should not mean "no request at all". The hermetic pinning technique
(reused from `create_with_an_unresolved_stack_rejects_local_data_without_sending`
and `delete_of_an_unbound_entity_completes_synthetic_without_sending`):

1. Mount **only** the mocks the behavior permits (T1: the GET alone).
2. Assert the terminal `GroupOutcome` (empty outcomes for the defer).
3. Assert the exact census: `server.received_requests().await.unwrap()`
   — length and, where discriminating, method/path of each entry.

Any mutated-away defer fires the PUT: it hits no mounted mock, wiremock
answers 404, `classify_push` returns `RemoteMissing` (existing=true) —
so *both* the outcome-emptiness and the census fail. No `.expect()`
needed on mocks that must stay unrouted; the census is the assert.

### D3 — T2 pins the *desired* subsumed semantics; suspected live bug recorded

The `:697` comment states the contract: "One outcome per queued label
op (the `create_task` precedent): … a resolvable op is Applied and an
unresolvable one rejects." T2 asserts exactly that: `1 + subsumed.len()`
outcomes, the label ops decided by resolvability. **This is desired
semantics, not today's behavior** (wave rule; AGENTS §5's no-pins
litmus).

Suspected live bug (planning-stage evidence, stage 2 confirms): the
success tail starts from `primary_outcome` (:700), which already emits a
synthetic `Applied { echo: None }` for **every** subsumed op
(`push_exec.rs:845-855`, "decision 6"), and the `:706` loop then
*appends* a second outcome per label op. For a group with subsumed
label ops, each label op id appears **twice**; the first (synthetic
`Applied`) wins on the engine side (`apply_push_report` completes the
op on first sight — `pipeline.rs:814-833`), so an **unresolvable label
op completes as `Applied` and its `Rejected { LocalData }` dead-letter
is silently shadowed** — the op leaves the outbox as if applied, while
the server never received the label (no binding exists to send). The
create path does not have this shape (it starts from a primary-only
vec, `push_exec.rs:413-418`), and `plan_task_groups`
(`push.rs:563-586`) does put label ops in an update group's `subsumed`
(edits bucket) — the double-emit is reachable in production.

Expected T2 failure shape against unmodified production, for subsumed
`[101 resolvable, 102 unresolvable]`: `outcomes.len() == 5` (not 3) and
`outcomes[2] == (102, Applied { echo: None })` (not `Rejected`).
**Stage-2 protocol on this failure** (dispatch §3 acceptance + prompt
2.2): stop, do not fix production, do not weaken the test — write the
test body as prescribed, keep it out of the landed commits (see §procedure
C3), report the failure verbatim, and open a GitHub issue (AGENTS §7
`backlog:` conventions) carrying the evidence chain and the proposed
fix prompt from §risks R1. The other five tests land normally.

### D4 — Tombstone-vs-failure discrimination is the report variant + attribution hygiene

- **The discriminator** between "tombstone cascade" and "plain failure"
  is the typed report: `SyncReport::Completed` whose snapshot carries
  `board.deleted_at == Some(_)` (tombstone adopted) versus
  `SyncReport::Failed { kind, .. }` (no snapshot exists at all). No
  log text, no error strings (AGENTS §5).
- **Attribution hygiene in T5** (403-while-listed): the stacks endpoints
  are mounted **200-empty**, not 404 — so the *only* dead-board
  determinant in the scenario is the 403 detail arm (:571-594). If the
  tombstone were to arrive via the stacks path instead (the :755
  carve-out), the stacks reads would have had to fail — they don't.
  Leaving stacks unrouted (wiremock's default 404) would muddy the
  attribution and rely on implicit behavior; mounting 200s keeps the
  scenario self-explaining.
- **Censuses distinguish early-return from cascade**: T4 asserts the
  cycle stopped after 2 reads (a cascade would have read stacks); T6
  asserts 3 reads (the archived listing is never reached after the
  active listing fails).

### D5 — Failed reports pin the typed kind, and no `NetworkLost`

T4/T6 assert `kind: SyncErrorKind::Server` (not just `Failed`):
misclassifying a hard 500/404 as transport would retry ×3 (breaking the
census) and broadcast `NetworkEvent::NetworkLost` (`actor.rs:492-495`)
— a user-visible cascade of its own. Both tests therefore also assert
`actor.system_rx.try_recv().is_err()` (the A8 precedent): a dead-server
condition must not declare the network lost.

### D6 — Naming and placement

- Suite tests join `push_exec.rs`'s `mod tests` in house behavior-name
  style; wiremock tests continue the A-numbering as **A15/A16/A17**
  with the doc-comment precedent (one `///` block each, stating the
  pinned arm and anchor).
- No new fixture infrastructure: T1-T3 reuse the suite helpers; T4-T6
  reuse `start_server`, `repo_with`, `spawn`, `set_board_and_report`,
  `next_report`, and the `hermetic.rs` JSON builders
  (`boards_json`/`board_json`/`stacks_json`). The only fixture work is
  inline mock mounts per test (the A-series pattern for non-standard
  pulls — A9/A11 mount their own too).

## §catalogue — six behaviors, one test each

### T1 (suite) `update_defers_when_only_unresolvable_labels_differ` — #292

- **Pins**: `push_exec.rs:574-586` (the defer conjunction and its
  empty-outcomes early return).
- **Fixture**: `MockServer` with exactly one mock — `GET
  /index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55` → 200,
  body `card_json(55, 7, 3)` (title "old", `labels: []`, order 3; the
  `Card` model serde-defaults `description` to `""`, `duedate`/`done`
  to `None` — `model.rs:103-134` — so the shape below is field-equal).
  Executor `overlay_with(42, &[], &[(10, 7, 55)], &[])` — card bound,
  **no label bindings**. Group: primary 100,
  `MaterializedOp::UpdateTask { task: task_id(10), card: CardShape {
  title: "old", description: "", duedate: None, done: None, order: 3,
  archived: false, labels: {label_id(9)} } }`, subsumed `[]`, outbox
  `[]`, `empty_tables()`.
- **Asserts (typed, terminal)**: the outcome is
  `GroupOutcome::Outcomes(outcomes)` with `outcomes.is_empty()`; census
  `received_requests().len() == 1` and `requests[0].method.as_str() ==
  "GET"` — zero mutating requests (D2).
- **Mutant kill**: defer arm removed/inverted → the PUT fires (fields
  unchanged), unmatched → 404 → `RemoteMissing` outcome: emptiness and
  census both fail. A conjunct-weakened condition (e.g. dropping
  `has_unresolved`) changes behavior only in scenarios this test's
  fixture makes unreachable — that residual is accepted; the
  canonical defer scenario is what #292 prescribes.

### T2 (suite) `update_completes_subsumed_label_ops_by_resolvability` — #292 (:707)

- **Pins**: `push_exec.rs:706-737` — both arms of the resolvability
  ternary (`Applied { echo: None }` / `Rejected { LocalData }`), one
  outcome per subsumed label op.
- **Fixture**: mocks — `GET …/stacks/7/cards/55` → 200 `card_json(55,
  7, 3)`; `PUT …/stacks/7/cards/55` → 200 `card_json(55, 7, 3)`
  `.expect(1)`; `PUT …/stacks/7/cards/55/assignLabel` → 200
  `card_json(55, 7, 3)` `.expect(1)`; `server.verify().await`.
  Executor `overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)])` — label 6
  → remote 3 bound; label 9 unbound. Outbox: `op(101,
  AssignLabel(task_id(10), label_id(6)))`, `op(102,
  AssignLabel(task_id(10), label_id(9)))`. Group: primary 100
  `UpdateTask` with the T1 shape but `labels: {label_id(6),
  label_id(9)}`, subsumed `[101, 102]`. Wire consequence: `desired =
  [3]`, `fetched_labels = {}` → exactly one `assignLabel(3)`; the
  defer arm does not fire (`desired ≠ fetched_labels`).
- **Asserts (desired semantics, D3)**: `outcomes.len() == 3`;
  `outcomes[0] == (op 100, Applied { echo: Some(_) })`;
  `outcomes[1] == (op 101, Applied { echo: None })`;
  `outcomes[2] == (op 102, Rejected { kind: LocalData })` — all via
  `matches!`/field equality on the typed enums, `op` ids via
  `OpId(uid(…))` equality (house style).
- **Expected to fail against current production** (len 5, 102 shadowed
  — see D3): stage-2 protocol applies. It is *not* weakened, `#[ignore]`d,
  or reworded to tolerate duplicates.

### T3 (suite) `move_same_stack_different_order_sends_the_reorder` — #293

- **Pins**: `push_exec.rs:776-782` — the fast path must **not** fire
  when only the order differs (same stack); plus the reorder wire
  shape (:785-799).
- **Fixture**: mocks — `GET …/stacks/7/cards/55` → 200
  `card_json(55, 7, 3)` `.expect(1)`; `PUT
  …/stacks/7/cards/55/reorder` → 200 body `"[]"` `.expect(1)`;
  `server.verify().await`. Executor `overlay_with(42, &[(5, 7)],
  &[(10, 7, 55)], &[])` — target stack local 5 → remote 7 (the same
  stack the card sits on). Group: primary 100, `MaterializedOp::
  MoveTask { task: task_id(10), to: (stack_id(5), 9) }`, subsumed `[]`.
  This is exactly the issue's `(7,3) → (7,9)` scenario.
- **Asserts**: `Outcomes` len 1, `(op 100, Applied { echo: None })`;
  census len 2 with `requests[1].method == "PUT"`; reorder body (parse
  `requests[1].body` as JSON) `== {"order": 9, "stackId": 7}` —
  "exactly one reorder with `order: 9`" (#293), destination stack in
  the body (tier-2 shape, same assert style as the existing move
  test).
- **Mutant kill**: the `:776` order conjunct dropped/negated → fast
  path fires → no PUT → `verify()` fails and the census never reaches
  2.

### T4 (wiremock A15) `a15_detail_500_fails_the_cycle_without_a_tombstone` — #294

- **Pins**: `actor.rs:595` — a listed board whose detail answers a
  non-403/404 error propagates; the cycle fails; no snapshot, no
  tombstone.
- **Fixture** (`tests/actor_wiremock.rs`, `#[tokio::test(start_paused =
  true)]`): mounts — `GET /…/boards` → 200 `boards_json(false,
  &json!([]))` (board **listed**, live); `GET /…/boards/42` → **500**;
  nothing else (stacks must never be reached). Repo
  `repo_with(&[(2, STACK)], &[])` (bound board, empty outbox → no push
  phase). Drive via `set_board_and_report`.
- **Asserts**: `matches!(report, SyncReport::Failed { kind:
  SyncErrorKind::Server, .. })` (500 → `DeckError::Server(500)` →
  `classify_read` → `Server`; single attempt — §context facts); census
  `received_requests().len() == 2` (the cycle stopped at the detail
  failure — a tombstone cascade would have read on);
  `actor.system_rx.try_recv().is_err()` (no `NetworkLost` broadcast —
  D5).

### T5 (wiremock A16) `a16_forbidden_detail_while_listed_synthesizes_the_tombstone` — #294

- **Pins**: `actor.rs:571-594` — a board present in the listing whose
  detail answers 403 is dead; the tombstone is synthesized from the
  *listing* entry, stamped now.
- **Fixture**: mounts — `GET /…/boards` → 200 `boards_json(false,
  &json!([]))` (the lagging listing still claims the board live);
  `GET /…/boards/42` → **403**; `GET /…/boards/42/stacks` → 200
  `stacks_json(&json!([]))`; `GET /…/boards/42/stacks/archived` → 200
  `"[]"` (D4 attribution hygiene: stacks healthy — the 403 detail is
  the only death signal). Repo `repo_with(&[(2, STACK)], &[])`. Drive
  via `set_board_and_report`.
- **Asserts**: the report is `SyncReport::Completed` (the tombstone
  snapshot is *valid*, not a failure); `snapshot.board.deleted_at
  .is_some()`; `snapshot.board.id == RemoteBoardId(BOARD)`;
  `snapshot.stacks.is_empty() && snapshot.tasks.is_empty()` (the A11
  tombstone-shape mirror); census len 4 (listing, detail, both stacks
  reads — the pull ran to completion through the dead-board path). No
  timestamp assertions (the tombstone is stamped `Utc::now()` — wall
  clock, deliberately unpinned).

### T6 (wiremock A17) `a17_stacks_404_on_a_live_board_fails_the_cycle` — #294

- **Pins**: `actor.rs:752-762` — the dead-board carve-out's
  `if dead_board` conjunct: on a **live** board a 404 stacks listing
  must propagate, not carve to "empty board".
- **Fixture**: mounts — `GET /…/boards` → 200 `boards_json(false,
  &json!([]))`; `GET /…/boards/42` → 200 `board_json(false,
  &json!([]))` (detail answers, board live → `dead_board == false`);
  `GET /…/boards/42/stacks` → **404**; nothing else. Repo
  `repo_with(&[(2, STACK)], &[])`. Drive via `set_board_and_report`.
- **Asserts**: `matches!(report, SyncReport::Failed { kind:
  SyncErrorKind::Server, .. })` (read-side 404 → `Server`); census len
  3 (listing, detail, active stacks — the archived read is never
  reached); `system_rx.try_recv().is_err()`.
- **Mutant kill / why it matters**: dropping the `if dead_board` guard
  carves the 404 to an empty listing and the cycle *completes* with an
  empty snapshot — the engine would then merge-tombstone every local
  task on a live board. `Failed` ≠ `Completed` is the pin.

## §procedure — binding for stage 2

1. **T1 + T3 first** (suite, no interdependence): implement, run
   `cargo nextest run -p taskboard-sync-nextcloud` — both must pass
   against untouched production; a failure is a suspected bug (stop,
   report — do not fix production here).
2. **T2** exactly per catalogue. If it passes: continue. If it fails
   with the D3 shape (len 5 / shadowed `Rejected`): do not weaken, do
   not commit it — hold the test body for the report, proceed to step 3,
   and follow §procedure C3 for the issue.
3. **T4-T6** (wiremock): implement per catalogue with the house
   harness; all three must pass against untouched production.
4. **C — commit shape** (every commit standalone, gates green):
   - C1 `test(sync): pin update_task defer and same-stack reorder arms`
     — T1 + T3 (and T2 when it passes; otherwise T2 joins the bug
     issue, not the branch).
   - C2 `test(sync): pin pull dead-board determination arms` — T4-T6.
   - C3 *(only on the D3 failure)*: open the GitHub issue per AGENTS §7
     (`backlog:` title, `testing`+`p1`… labels judged at filing;
     body: the D3 evidence chain, the failing assertion output, the
     held T2 test body, and the proposed fix prompt from §risks R1).
     No production edit, no failing test committed.
5. **Gates**: full AGENTS §10 suite green in the worktree
   (`fmt`, `clippy -D warnings`, `nextest`, `test --doc`, `deny`,
   `llvm-cov`). Note for review: `llvm-cov`'s percentage is not a gate
   criterion beyond running clean.
6. No push, no PR (stage 3 ships). Issues close via the PR body
   (`Closes #292` / `#293` / `#294` — or, if D3 fired, #292 stays open
   with a comment pointing at the new bug issue; stage 3 decides with
   the user's dispatch rules).

## §non-goals (explicit)

- **No production changes** — not even comment edits in `src/`
  (dispatch §3: diff = tests only; any production edit is a stage-3
  finding). The D3 bug is *reported*, not fixed.
- **No new fixture infrastructure** — no new helpers in
  `tests/common/`, no new suite constructors; inline mocks only (D6).
- No engine-thread / I-series / contract-suite additions; no snapshots
  (insta) touched; no coverage-percentage targets.
- No drift into the other dead-board arms (:553/:568/:602/:705) beyond
  the incidental exercise the three prescribed tests give them — the
  issue prescribes three tests, not arm-by-arm exhaustion.

## §risks

- **R1 — T2 exposes the double-emit bug (high likelihood, high
  impact, fully prepared)**: `update_task`'s success tail starts from
  `primary_outcome` (synthetic `Applied` for every subsumed op) and the
  `:706` loop appends a second outcome per label op — reachable via
  `plan_task_groups`' edits bucket; first-outcome-wins on the engine
  side shadows `Rejected { LocalData }` for unresolvable labels (op
  leaves the outbox as applied though nothing was sent). If stage 2
  confirms: five tests land, T2's body moves to the new issue with
  this proposed fix prompt — *"In
  `crates/taskboard-sync-nextcloud/src/push_exec.rs` `update_task`,
  build the success-tail outcomes from a primary-only vec (the
  `create_task` precedent at :413-418) so the :706 loop is the sole
  outcome source for subsumed label ops (resolvable → Applied,
  unresolvable → Rejected{LocalData}); update/other subsumed ops keep
  their synthetic Applied; add the held T2 test; re-run chunk 5's
  dependencies."* — filed under AGENTS §7, referenced from #292.
- **R2 — census fragility**: T4/T5/T6 request counts assume the
  current read set (unconditional detail; conditional-but-cold
  listings that return data → no refetch). A test accidentally seeding
  persisted validators (the A10 pattern) would shift counts — the
  catalogued fixtures seed none. If a census fails while the report
  assert passes, suspect the fixture first, then a production drift
  (which is a finding to report, not to absorb).
- **R3 — wiremock unmatched-default**: leaving an endpoint unmounted
  makes it answer 404 — *indistinguishable from a deliberate 404
  mock*. T4/T6 intentionally exploit this for must-not-be-reached
  endpoints (their censuses prove non-reach); T5 mounts everything it
  expects to be read. Stage 3 must not "helpfully" add mocks — a
  census shift is the signal something changed.
- **R4 — D3's engine-side account** (`apply_push_report` first-wins)
  is read from `pipeline.rs:788-833`; if stage 2's observed engine
  behavior differs (e.g. last-wins), the *executor-layer* double
  outcome still fails T2's len assert — the bug report stands either
  way, with the engine-side nuance corrected in the issue text.
- **R5 — no fixture gaps remain**: all six tests build purely on
  existing helpers (verified helper-by-helper against
  `push_exec.rs:918-1129` and `tests/common/hermetic.rs`); no arm
  needs support that does not exist.

## §acceptance (from dispatch §3)

- The six behaviors pinned exactly as #292/#293/#294 prescribe —
  defer → empty outcomes + zero mutating requests; subsumed-label
  outcomes by resolvability at :707; (7,3)→(7,9) ⇒ exactly one
  reorder `order: 9`; detail 500 → `Failed { Server }` no tombstone;
  403 while listed → tombstone; live-board stacks 404 → `Failed {
  Server }`.
- All landed tests pass against unmodified production; a failing new
  test is a live bug → stop, report, never weaken (D3 protocol).
- Gates green; branch diff = tests only (+ this plan); production
  diff empty.

# Chunk D3 Plan — move marker sidecar concurrency guard (#1385)

Round 8, chunk D3 · branch `fix/move-marker-concurrency` ·
dispatch plan: `2026-11-25-backlog-reduction-round8-dispatch-plan.md`
§1/§8 · scope report:
`2026-11-25-backlog-reduction-round8-scope-report.md` §D-D3 ·
issue: #1385. Status: planned 2026-11-26.

## 1. Diagnosis — the unguarded read-modify-write

`crates/taskboard-sync-nextcloud/src/move_marker.rs::write_move_marker`
mutates a shared document with no precondition between read and write:

1. `client.attachments(...)` — locate the anchor sidecar attachment;
2. `client.download_attachment(...)` — read the current document bytes;
3. merge in memory: `document.unknown.insert(MOVED_TO_FILES_KEY, ...)`
   (foreign `unknown` keys and `AppDataSections` are preserved because
   the merge starts from the downloaded document);
4. `client.update_attachment_content(...)` / `upload_attachment(...)`
   — PUT the merged document wholesale.

The PUT carries no precondition. The Deck attachment content route
serves no `ETag`/`Last-Modified` and offers no conditional-write
plumbing at all (`client.rs` ~line 1520, D1 spike §3.2: "serves no
`ETag`/`Last-Modified` … no validator plumbing is offered"). So the
last PUT wins wholesale: any writer that read the document before a
concurrent PUT lands will overwrite that writer's keys.

Racing writers per the issue (#1385) and the B5-p1 plan §5 R2/§8:

- **W-A** = `write_move_marker` (this device, board-move ceremony
  `board_move_ceremony.rs::mark_deck`), inserting
  `moved_to_files` under `MOVED_TO_FILES_KEY`;
- **W-B** = the live files-sync lane's sidecar fold flush on another
  device, writing its own `unknown` keys / sections into the same
  attachment.

Interleaving `A.read → B.read → A.write → B.write` loses A's marker
key entirely (lost marker: devices with no files tier never learn the
board moved — the D-BF11 contract breaks). The reverse order loses
B's live-lane keys (lost update: the #1366/D-BF11 mixed-fleet merge
contract, pinned by the B5-p1 S4 scenario, holds only single-writer).

## 2. Guard decision — retry-with-recheck (bounded), not conditional PUT

**Decision**: a bounded read→merge→write→recheck loop inside
`write_move_marker` (retry-with-recheck; the issue's second recorded
option), with a typed exhaustion error.

**Rationale**:

- **Conditional PUT is impossible on the real API.** The recorded
  option presupposes an `If-Match`/ETag precondition on attachment
  updates; `DeckClient`'s own docs record that the attachment content
  route serves no validators (D1 spike §3.2, quoted in §1). Adding
  `If-Match` plumbing would be dead code against production Deck and
  would only be exercisable by wiremock fictions. Rejected.
- **Retry-with-recheck matches the actual conflict model.** The merge
  is a single-key insert into a JSON map; re-merging the marker into a
  *freshly re-read* document is convergent — each retry incorporates
  the concurrent writer's keys plus the marker, so once both writers
  have run, the next recheck observes a document that already equals
  its own merge output and terminates. The loop is bounded (fixed
  attempt constant, e.g. 3), so a pathological peer cannot spin the
  writer forever.
- **The recheck oracle is structural, not textual**: after the write,
  re-download the attachment and compare the parsed document against
  the merge of that same re-downloaded base with the marker
  (`expected = merge(server_doc, marker)`; success iff
  `server_doc == expected`). Equivalently: marker key present with the
  intended value AND every key/section read before the write is still
  present. On mismatch (a concurrent write landed between our read
  and write), retry with the fresh document as the new base.
- **Exhaustion is a typed outcome** (round law [L](a)): a new
  `MoveMarkerError::Contended` variant (name at stage 2's discretion;
  `#[error("the sidecar kept changing under the marker write")]`-class
  doc, no text assertions anywhere).

Scope guard: the fix lives in `move_marker.rs` plus, if needed, a
narrow `DeckClient` addition only if the recheck needs an existing
method it lacks — `download_attachment` already exists, so **no
client.rs change is expected**. The upload path (fresh document, no
existing attachment) is not retried: a create-vs-create race on a
board with no anchor card is out of #1385's stated scenarios (both
uploads are additive attachments, not RMW); note it as known-limit,
not silent.

## 3. Lost-update pin design (TDD stage 2; fails pre-fix)

House law: deterministic by construction, wire-level, typed
outcomes, exact request counts (`expect(n)`), anchored clock
fixtures. Located in the existing
`crates/taskboard-sync-nextcloud/tests/suite/move_marker.rs`.

**P1 — `concurrent_sidecar_write_is_not_lost` (the lost-update pin).**
Scripted wiremock sequence on the S2 shape (anchor card exists):

1. `attachments` → the sidecar attachment (as S2);
2. first `download_attachment` → document D1 carrying a foreign
   live-lane key `K_b` and no marker;
3. `update_attachment_content` request 1 is accepted (200);
4. **recheck** `download_attachment` (second download) → document D2
   = a concurrent writer's output: `K_b`' evolved value, a *new* live
   key `K_c`, **no marker** — i.e. the concurrent PUT landed after
   ours and clobbered it;
5. `update_attachment_content` request 2 accepted; final recheck
   download → D3 = merge(D2, marker);
6. loop's structural oracle satisfied → `Ok(())`.

Wire pins: exactly 2 downloads before each of exactly 2 updates (1
initial read + 1 recheck per attempt), no `create_card`; the final
captured multipart body parses to a document with
`moved_to_files()` == the input marker AND both `K_b` (evolved) and
`K_c` present. **This test fails pre-fix** (only one update is
issued; the fix adds the recheck + second attempt) — stage 2 lands
the pin first, observes the failure, then the fix.

**P2 — `contention_exhaustion_is_a_typed_error`.** Every recheck
download returns a marker-less document (the peer keeps winning).
Pin: bounded attempts (exactly N updates, exactly N+1 downloads),
then `matches!(err, MoveMarkerError::Contended)`-class typed
outcome. No infinite loop, no text assertions.

**P3 — `stable_sidecar_write_is_a_single_update` (regression
guard).** Recheck download returns exactly what we wrote (merge(D1,
marker)); pin: one initial read, one update, one recheck download,
`Ok(())`. Ensures the happy path costs one extra GET, not a retry
cycle. (S2/S3 of the B5-p1 suite keep their request-count semantics;
stage 2 adjusts those counts only if they now include the recheck —
documented in the PR, never weakened.)

## 4. Verify-pass outcomes (per dispatch plan §8)

| Item | State | Verdict |
|---|---|---|
| #1268 (pdf-export mutants window) | CLOSED (COMPLETED) | closed-with-evidence: stage-2 decision + scope proof on commit `29152b35` (round-6 D2, `ci/gate-ci-residue`); window shipped as a third `mutation-testing.yml` job; the deliberate no-full-app-lane ruling is recorded on the issue. No fold needed. |
| #1277 (doctor comm locale warnings) | CLOSED (COMPLETED) | closed-with-evidence: fixing commit `c7bb2c43` "fix(scripts): the doctor's comm inputs sort in C locale (#1277)" referenced in the issue timeline and in `main`'s history. No fold needed. |
| #1237 (seed-echo triplication) | CLOSED (COMPLETED) | closed-with-evidence: PR #1272 — `Adapter::begin_plan_place` extracted; all three sites delegate behavior-identically. No fold needed. |
| #1172 (three private mount preambles) | CLOSED (COMPLETED) | closed-with-evidence: PR #1272 + commit `c9d10e78` (round-5 E1) migrated keymap_case_seam/focus_coverage/usability_components onto the B4a harness; residual inline mounts tracked as #1273 (still open, correctly). No fold needed. |
| #1097 (stale ci.yml bench_fixtures comment) | CLOSED (COMPLETED) | closed-with-evidence: PR #1173 verified merge at `937e89ab` → `4423a859` (round-4 S5). No fold needed. |

All five verified closed-with-evidence against the tracker; nothing
folds into this chunk.

## 5. Stage-2 work list

1. Land the two pins P1/P2 (+P3) in
   `tests/suite/move_marker.rs` first; observe P1/P2 fail pre-fix
   (P1 fails on request counts; P2 has no typed variant pre-fix —
   land it with the fix's variant in the same push, asserting
   `matches!(err, _)` exhaustively so it compiles only post-fix, or
   stage the variant addition first; either is acceptable — record
   which).
2. Implement the guard in `move_marker.rs`: extract the merge step
   (`fn merge_marker(document, marker) -> AppDataDocument` or
   equivalent), add the bounded attempt loop with the structural
   recheck oracle, add the typed exhaustion variant to
   `MoveMarkerError`. Keep `write_move_marker`'s signature and
   no-op arms (`NotFound`/`Forbidden`, empty stacks) untouched.
3. Update the B5-p1 suite's exact-count pins if the recheck download
   changes their expectations (S2/S3 update-path counts) — PR note,
   no weakening.
4. Known-limit doc comment on the upload path (create-vs-create
   race out of scope).
5. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo
   nextest run --workspace` (background + poll; defer per the
   #879/#894 rule if contended). Every `cargo` call routes through
   `scripts/memory-gate.sh`.

## 6. Stage-3 traps

- **Do not touch `client.rs`** unless the recheck genuinely needs a
  missing method (`download_attachment` exists; it does not).
- **Do not add `If-Match` plumbing** — rejected in §2; the D1-spike
  finding is normative context, cite it in the PR body.
- **Exact-count pins**: `expect(n)` on every mock; the retry loop
  changes counts deterministically (fixed constant), not variably —
  if a count is nondeterministic the design is wrong, re-examine.
- **No text assertions** on the new error variant; typed
  `matches!` only. The variant's `#[error]` string is narration.
- **Retries spine interference**: `DeckClient::call_multipart`
  retries transient statuses; the mocks return deterministic 200s so
  the loop's counts stay exact.
- **PR body references #1385**; closure is the dispatcher's at merge
  (§14). The B5-p1 finding provenance (§5 R2/§8 of
  `2026-11-25-write-move-marker-tier1-plan.md`) belongs in the body.

## 7. Backlog issues opened by stage 1

None: the diagnosis produced no out-of-scope findings beyond the
known-limit note (§2/§5.4), which is recorded in the plan and PR, not
a tracker item. Verify pass: all five items closed-with-evidence
(§4) — no folds, no new issues.

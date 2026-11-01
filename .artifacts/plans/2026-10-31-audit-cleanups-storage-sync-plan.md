# [Plan] Chunk 2.3 — Audit cleanups: storage + sync (#272 + #273)

- **Date**: 2026-10-31 (dispatch-scheduled name; written 2026-11-02)
- **Chunk**: 2.3 of backlog round 2
  (dispatch plan: `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §4 "Chunk 2.3 — audit cleanups: storage + sync")
- **Branch**: `chore/audit-cleanups-storage-sync`, cut off the fetched
  `origin/main` at `a3987d11` (local `main` was 2 UI-only commits behind;
  `git diff 4d8be313..a3987d11` over the audited paths is empty, so the
  re-verification below is valid at the branch point). Worktree:
  `../taskboard-WT-chore-audit-cleanups-storage-sync` (real dir beside the
  main checkout at `/home/viktoria/nextcloud/viktoria/programming/`,
  `src/` symlink; created after `scripts/worktree-doctor.sh` passed —
  232 G free, floor 25 G).
- **Covers** (normative): #272 → F5-04/F5-05/F5-06/F5-07 (fix) +
  F5-10 (explicitly deferred — not here) + F5-11/F5-08 (recorded
  verdicts, no action); #273 → F6-12/F6-13/F6-14/F6-15 + F12-07.
  Originating reports:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md`
  (findings §, axis H/G/E/I.4),
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md`
  (axis G),
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-security-report.md`
  (F12-07, axis N drift cross-ref).
- **Gate**: chunk 1.1 merged — the dispatch named PR **#448**, stale;
  the work landed as **PR #562** (`fix/anchor-filename-fallback`,
  MERGED 2026-11-01T00:40Z, rebase-merge). Verified by merge-commit
  ancestry: `d443c114` ∈ `origin/main` (the pruned branch ref is gone,
  so branch containment is checked via the PR's merge commit). Gate
  satisfied. Also already merged: the sibling chunk 2.2 (PR #576,
  `chore/audit-cleanups-domain-state` — its plan is the structural
  precedent for this one).
- **Status**: planning complete; every open question resolved in §3
  (D1–D9). Implementation not started.
- **Hard rule** (dispatch acceptance): **zero behavior changes beyond
  the decode-mapper arity hardening.** Comment/doc edits change no
  bytes of behavior; the rename changes no behavior (same variant, new
  name — one deliberate user-facing class-label string change, listed
  in D8); the mapper fix changes behavior only on the
  malformed-arity path (previously a panic or a silently-ignored
  column → now `Corrupted`); well-formed data decodes identically
  (the roundtrip proptest + T7 corruption-class pins are the proof
  and must stay green unmodified).

---

## 1. Method

- Re-verification ran finding by finding against the current tree
  (`origin/main` @ `a3987d11` = this worktree), with the audited
  v0.1.0 coordinate (`925f50e3`) quoted alongside. Every site was
  read at its current location; nothing was taken on faith from the
  issue text or the audit's line numbers.
- Churn context confirmed: since 2026-10-17 the storage crate doubled
  (repo.rs 1 092 → 2 124 lines — habits, time entries, the undo
  journal, command journal, reminder dismissals/adhoc alarms), and the
  sync crate grew the anchor ceremony (#562: `anchor.rs`), the
  sidecar flush (`flush.rs`), the `push_exec/` module split (F6-10's
  fix, `5ede7fc4`), and `is_transport()` (F6-19's fix). Kani
  harnesses (`verify.rs`) landed in both crates. None of this
  repaired an in-scope finding: **"fixed-already" among the in-scope
  findings: none** — stage 2 has no ✅-comment targets *inside* the
  covered findings (but see §8 bookkeeping: F5-03 *is* fixed-already
  context for #272's closing comment, and F5-11/F5-08 get
  re-verification verdict comments per the issue's "recorded
  verdicts" clause).

## 2. Re-verification table (the core)

| Finding | Verdict | Audited @ v0.1.0 (`925f50e3`) | Current @ `a3987d11` — evidence |
|---|---|---|---|
| **F5-04** (P3) duplicated "not an integrity verdict" rationale pair in `error.rs` | **still drifted** (content unchanged) | `error.rs:35–38`: two inline pairs duplicating the fn doc @ `:21–23` | `error.rs:35–38` byte-equivalent: pair 1 ("Other database errors (syntax, RAISE-triggered aborts, …) are not integrity verdicts about the stored data.") + pair 2 ("Non-exhaustive upstream enum: everything else (syntax, RAISE-triggered aborts, …) is not an integrity verdict."), the fn doc now `:17–23`. Unrepaired since the audit |
| **F5-05** (P3) stale "future Phase 5 bootstrap" doc in the storage actor | **still drifted** | `actor.rs:81–82`: "(the future Phase 5 bootstrap awaits it …)" | `actor.rs:80–82` verbatim: "the **future** Phase 5 bootstrap awaits it for a clean shutdown once all handles are dropped". The bootstrap reality it should name: `taskboard-app`'s shutdown struct carries `storage_join`/`engine_join`/`escalation_join` (`bootstrap.rs:372–374`) and the crate doc states the cleanup "drops the storage handle and joins the [actor]" (`bootstrap.rs:10`) |
| **F5-06** (P3) `load_inner` allow-comment says "eight SELECTs" | **drifted further** | comment @ `repo.rs:406`; 12 SELECTs @ audit time (the audit itself corrected "eight" → 12) | comment @ `repo.rs:681` — still "// eight SELECTs; the length is the schema's". `load_inner` (`repo.rs:682–1123`) now runs **19** SELECTs (boards, stacks, labels, tasks, task_labels, outbox, sync_metadata, sync_status, reminder_rules, reminder_deliveries, reminder_snoozes, reminder_dismissals, adhoc_alarms, reminder_muted, command_journal, command_journal_head, time_entries, habits, habit_log). A numeric count has now drifted twice by construction (8 → 12 → 19); the fix must be count-free |
| **F5-07** (P3) decode mappers index `nums[0..2]` arity-blind — an arity change panics inside `load` instead of yielding `Corrupted` | **still drifted, same shape** | `repo.rs:179, 197–202, 237–243, 301–306` indexing the `Vec` from `remote_columns_present_or_none` | Four sites, shifted: `decode_board` `repo.rs:273–281` (`nums[0]`), `decode_stack` `:302–307` (`nums[0..2]`), `decode_task` `:341–348` (`nums[0..3]`), `decode_label` `:443–448` (`nums[0..2]`); all fed by `decode::remote_opt` (`repo.rs:227–229`) → `codec::remote_columns_present_or_none` (`codec.rs:442–461`). The codec returns a `Vec` whose length **equals the caller's slice length** — arity safety rests entirely on the call-site literal (all-or-none and negativity *are* checked codec-side; only arity is not). `decode::remote_opt` has exactly these four callers. Under-arity panics (`nums[1]` on a 1-vec); over-arity silently ignores the extra column — both become `Corrupted` in D4 |
| **F5-10** (P3) serde-wire ↔ codec-text mirror; domain-owned `text_form`/`parse_text_form` would collapse codec.rs to delegation; folds the clock-parse helper | **still drifted (same design, larger surface) — explicitly deferred, not here (D5)** | mirrors @ `codec.rs:41–58, 123–159, 163–215, 219–246`; ReminderRef delegates @ `:251–262`; 14× clock-parse | Mirrors live at `codec.rs:147` (`op_kind_to_text`), `:309` (`sync_phase_to_text`), `:320` (`sync_error_kind_to_text`), `:365/:378` (`validator_key_to/from_text`); `ReminderRef` still delegates (`:397–408` → domain `text_form`/`parse_text_form`). New text-form families joined codec.rs since the audit (importance/urgency/insistence — documented "one-representation discipline", `codec.rs:18–96`; recur/baseline as serde JSON, `:98–122`) — same unification candidate. The `.parse().map_err(\|_\| Corrupted)` clock-parse boilerplate grew **14× → 18×** (stack ×3 `repo.rs:319–328`, task ×12 `:384–429`, label ×3 `:457–466`) |
| **F5-11** (P3) reminder-block split evaluated and **declined** — revisit when it exceeds ~⅓ of repo.rs | **re-affirmed: trigger not fired** | reminder block `repo.rs:972–1089` of 1 092 lines (~11%) | repo.rs is 2 124 lines; the reminder family (row structs `:157–206`, decode `:208–216`/`:498–520`, load SELECTs + hydration `:787–850`/`:1040–1100`, write partitions `:1704–2020`) ≈ 546 lines ≈ **26%** — below the ~⅓ trigger. The growth since the audit is *not* reminders: habits, time entries, and the undo journal are new co-resident families. Recorded verdict stands; see D6 for the nuance this adds to any future revisit |
| **F5-08** (P3) three documented lossy conversions, accepted | **unchanged — accepted (out of scope, verdict re-recorded)** | `pending_ops` saturation, `remote_col` saturation, `offset_seconds` truncation | Not re-located in detail (no action per the issue); the closing comment on #272 records the verdict as re-verified-unchanged at `a3987d11` |
| **F6-12** (P3) crate description says "WebDAV/REST" — zero WebDAV code | **still drifted** | `Cargo.toml:9`; grep `webdav\|dav` over src/ → zero hits | `Cargo.toml:10`: `description = "Nextcloud WebDAV/REST synchronization client actor for taskboard"`. Grep over `src/`: only two comment hits, `client.rs:376`/`:1734` — both describing *rejected* "WebDAV-style" userinfo URLs (anti-feature docs, not WebDAV support; they stay). AGENTS.md:36 still "Nextcloud REST/WebDAV API actor" |
| **F6-13** (P3) architecture.org crate row hides the Deck-client role | **still drifted, row grew around the gap** | `architecture.org:182–193`, "the injected =DeckClient=" the only client mention | Row now `architecture.org:414–454` — grew the anchor ceremony, sidecar flush, connector FSM — yet still never names the client role; the only mention remains "the injected =DeckClient=" (`:453–454`). Current share for the sentence: client half (`client/model/color/ocs/backoff/error.rs`) = 3 133 of 8 183 production src lines ≈ **38%** |
| **F6-14** (P3) `DeckError::Envelope` also captures request-side serde failures | **still drifted** | `error.rs:23–25` + `send_json` mapping | `error.rs:24–26`: doc "The response body could not be decoded as the expected OCS JSON.", Display "unexpected response payload", `#[from] serde_json::Error`. `client.rs:1469` still maps the request body's `serde_json::to_value` failure through `DeckError::from` → the same variant: a client-side serialization bug diagnoses as "unexpected response **payload**". Variant count still 13; `is_transport()` landed beside it (F6-19 fixed, context) |
| **F6-15** (P3) `GroupOutcome` vs domain `PushOutcome` — one letter apart, no cross-links | **still drifted, both moved** | `push_exec.rs:80–93` vs domain `remote.rs` | `GroupOutcome` now `push_exec/outcomes.rs:12–30` (module split `5ede7fc4`): doc describes per-group defer/abort semantics, no mention of `PushOutcome`. Domain `PushOutcome` now `remote.rs:207–214` ("A pushed operation's outcome, as reported by the sync actor."): no mention of `GroupOutcome`. The confusable pair is exactly as the audit left it |
| **F12-07** (P3) N-axis half of the WebDAV description evidence | **still drifted** | `Cargo.toml:9` (the only `webdav` match in the tree at audit time) | Same line (`Cargo.toml:10`); the anti-feature comments in `client.rs` are now the only other matches in the crate. Fixed by the same edit as F6-12 (D7) |
| *Context, not in scope:* **F5-03** (P2) `execute_batch` dispatched via 13 `if let` chains, not compiler-checked | **fixed-already** (cite for #272's closing comment) | `repo.rs:687–1089`, 13 `if let` chains | `execute_batch` is an exhaustive `match` with an explicit no-wildcard comment ("a new `PersistenceAction` variant must fail this", `repo.rs:1160`) — landed in `1112fb2d` "refactor(storage): make execute_batch dispatch compiler-exhaustive". Not a #272/#273 finding; recorded here so nobody re-finds it |

## 3. Decisions (every open question resolved)

### D1 — F5-04: delete the *second* inline pair; keep the arm's first pair

- **Edit**: `crates/taskboard-storage-sqlite/src/error.rs:37–38`
  deleted (the "Non-exhaustive upstream enum: …" pair — a verbatim
  duplicate of the fn doc's closing sentence at `:21–23`). The first
  pair (`:35–36`) stays as the arm-local comment: it names what *this
  arm's* `_` catches, which a reader at the match arm should not have
  to scroll for; the fn doc keeps the full rationale (shape-match
  discipline, orphan rule, non-exhaustiveness).
- **Rejected**: deleting both pairs — the audit's proposed action is
  "delete the duplicated inline pair" (singular); a bare `_ =>`
  with no local comment invites the next reader to re-derive whether
  RAISE-triggered aborts are integrity verdicts.

### D2 — F5-05: present tense, naming the awaiter without a cross-crate dependency claim

- **Edit**: `crates/taskboard-storage-sqlite/src/actor.rs:80–82`, the
  `spawn_storage_actor` doc's parenthetical becomes present tense and
  names the actual mechanism:
  "(the bootstrap drops the last handle and awaits it for a clean
  shutdown — `taskboard-app`'s bootstrap, `bootstrap.rs`)".
- Prose mention of the consumer is fine (dependency direction is a
  *code* rule; the doc states a fact about who awaits). **Rejected**:
  an intra-doc link to `taskboard-app` — the path does not resolve
  from this crate and would break `cargo doc` warning-free.

### D3 — F5-06: count-free reword, permanently

- **Edit**: `crates/taskboard-storage-sqlite/src/repo.rs:681`
  `// eight SELECTs; the length is the schema's` →
  `// one SELECT per table; the length is the schema's`.
- Rationale: the count has drifted twice (8 → 12 → 19) and drifts by
  construction — every migration adds a SELECT. The rationale itself
  (length is schema-driven, the transaction narrative justifies the
  allowance) still holds. The neighbouring comment at `:683–687`
  ("One read transaction around all SELECTs") is already count-free
  and untouched.

### D4 — F5-07: typed per-ref decode helpers; malformed arity → `Corrupted`, TDD-pinned

The audit's sketch ("destructure with slice patterns at the sites")
refines naturally into **four named helpers in the existing `decode`
module** (`repo.rs:220–261`), because all four sites share the same
all-or-none + arity contract and `decode::remote_opt`'s only callers
are these four sites — the helper shape removes the four
closure-`transpose` dances instead of adding four inline `match`
boilerplates.

- **Shape** (one of four; `X` = board/stack/card/label):

```rust
/// The optional remote binding from the entity's remote-id columns:
/// all-NULL → `None`; any arity other than the ref's exact width —
/// or a mixed NULL/NOT-NULL row, or a negative id — is corruption.
pub(crate) fn remote_stack(cols: &[Option<i64>]) -> Result<Option<RemoteStackRef>, RepositoryError> {
    match remote_opt(cols)?.as_deref() {
        None => Ok(None),
        Some([b, s]) => Ok(Some(RemoteStackRef {
            board: RemoteBoardId(*b),
            stack: RemoteStackId(*s),
        })),
        Some(_) => Err(RepositoryError::Corrupted),
    }
}
```

  `remote_board` → `RemoteBoardBinding` (width 1), `remote_card` →
  `RemoteCardRef` (width 3), `remote_label` → `RemoteLabelRef`
  (width 2). `remote_opt` stays as the private all-or-none step
  inside the module (it is the codec mapping); the four call sites
  become one-liners:
  `let remote = decode::remote_card(&[row.remote_board_id, row.remote_stack_id, row.remote_card_id])?;`
  — each shorter than today, and the closure/`transpose` shape
  disappears.

- **Semantics pinned** (the dispatch acceptance's "malformed row
  yields `Corrupted`, never a panic — pinned"):
  - under-arity (`remote_stack(&[Some(1)])`) → `Err(Corrupted)`;
    **today this panics at `nums[1]`** — the TDD failing test;
  - over-arity (`remote_board(&[Some(1), Some(2)])`,
    `remote_card(&[Some(1), Some(2)])`) → `Err(Corrupted)`;
    **today the extra column is silently ignored** — the subtler
    drift direction, also a TDD failing test;
  - all-NULL (`remote_board(&[None])`, …) → `Ok(None)` — the unset
    state passes through (new plumbing through the helpers, pinned
    once);
  - mixed-NULL and negative-id rejections are
    `remote_columns_present_or_none`'s contract, already pinned
    codec-side (`codec.rs:833–839`) — **not** re-pinned at the
    helper layer (one behavior, one test, lowest layer).

- **Test home**: repo.rs has **no** `#[cfg(test)]` module today (the
  audit's tier map confirms: repo semantics live in
  `tests/roundtrip.rs`). The helper arity contract is a unit-level
  fact about private fns → repo.rs's **first** in-file
  `#[cfg(test)]` module, mirroring codec.rs's in-file style. No new
  dev-dependencies.

- **Behavior-change budget**: well-formed data decodes identically
  (the slice patterns accept exactly the widths the literal call
  sites pass). The only behavioral delta is the malformed path:
  panic/silent-ignore → `Corrupted` — the finding's requested
  hardening, and the one place the dispatch's "zero behavior
  changes" clause is lifted.

### D5 — F5-10: explicitly deferred — recorded as not-here, spun into its own issue

The dispatch defers the M-sized text-form unification. Recorded
facts so the deferral is actionable later (see §2 F5-10 row): four
mirrors + the new importance/urgency/insistence and recur/baseline
families + the 18× clock-parse helper, against the `ReminderRef`
delegation precedent (`codec.rs:397–408`).

- **Bookkeeping**: stage 3 closes #272 (every other finding
  addressed or verdict-recorded) and files a new issue —
  title `backlog: storage codec text-form unification (F5-10)`,
  labels `p3` + `refactoring` + `audit-v0.1.0`, body = short
  summary + permalinks to the storage audit report's Axis H section
  and this plan's §2 F5-10 row (deep context stays in the
  artefacts; AGENTS §7 conventions). Draft text in §8.
- **Rejected**: pulling even the "cheap companion" (the `decode::clock`
  helper) into this chunk — it edits 18 call sites in a file this
  chunk otherwise only comments; that is F5-10's blast radius, and a
  partial F5-10 defeats the deferral's purpose.

### D6 — F5-11: decline re-affirmed; the revisit trigger's framing updated

No action in this chunk (per the issue). The re-verification adds one
fact for whoever revisits: repo.rs's growth since the audit came from
*new families* (habits, time entries, undo/command journal), not from
reminders — if the file ever splits, the natural seam is by family
(reminders / habits / tracking / journal), not by moving the reminder
block out alone. Stage 2 posts the re-verification verdict (≈26%,
trigger not fired, family-split framing) as an issue comment; no diff
hunk.

### D7 — F6-12 + F12-07: the description says Deck REST, in both places

- **Edit 1**: `crates/taskboard-sync-nextcloud/Cargo.toml:10`
  → `description = "Nextcloud Deck REST synchronization client actor for taskboard"`.
- **Edit 2**: `AGENTS.md:36`
  → `- \`taskboard-sync-nextcloud\`: Nextcloud Deck REST API actor.`
  (same minimal one-line style as the sibling rows; the client+actor
  duality belongs to architecture.org, not the boundary table).
- **Kept**: the two `client.rs` comment mentions of "WebDAV-style"
  URLs (`:376`, `:1734`) — they document a *rejected* URL shape
  (userinfo-as-auth), which is precisely not WebDAV support.
- **Grep audit** (stage 2): after the edit,
  `grep -rni webdav AGENTS.md docs/architecture.org crates/*/Cargo.toml`
  → zero hits; `grep -rni webdav crates/taskboard-sync-nextcloud/src/`
  → exactly the two kept comments. (Other docs/ADRs may still carry
  WebDAV wording — chunk 15's, not this chunk's.)

### D8 — F6-14: **rename `Envelope` → `Json`** (re-doc alone rejected)

The issue allows rename *or* re-doc; the dispatch asks for a decision
weighing churn against the week's sync changes. **Decision: rename.**
Rationale, against the re-doc alternative:

1. *The re-doc that actually fixes the misdiagnosis must touch the
   Display string anyway.* A request-side serialization bug surfaces
   in logs and bug reports as "unexpected response **payload**" — the
   misleading half is the Display line, not just the doc comment.
   Doc + Display + a name that still says the opposite is half a
   fix at ~the same edit surface minus the mechanical part.
2. *The rename is compiler-checked; the re-doc is not.* 15 sites,
   every one a compile error if missed — vs. a stale doc-comment
   mention that survives until the next reader trips on it.
3. *The churn risk the dispatch warns about has already resolved.*
   The week's sync changes are merged (anchor #562, `push_exec/`
   split, `is_transport`); no live branch carries `Envelope` work.
   The workspace ships one binary and is the API's only consumer —
   AGENTS §2's agility clause applies ("API/blame churn is not a
   decision variable while the API is internal").
4. *Bonus de-confusion*: `DeckError::Envelope` vs `OcsEnvelope`
   (`ocs.rs`'s OCS response wrapper). Today the variant name suggests
   the envelope type; after the rename `OcsEnvelope` keeps its
   meaning uncontested and `Json` names the failure class plainly.

- **New definition** (`error.rs:24–26`):
  doc: "A JSON failure in either direction: a request body could not
  be serialized, or a response body could not be decoded as the
  expected OCS JSON."; Display: `#[error("json encode/decode failure")]`;
  variant: `Json(#[from] serde_json::Error)`.
- **Complete site list** (the stage-2 grep audit; 15 variant/doc
  sites — all must be gone):
  `error.rs:25` (definition);
  `client.rs:1241` (reorder swallow — F6-18's accepted seam, name
  only), `:1787` (unit test);
  `tests/contract.rs:322`, `:339` (contract asserts);
  `mapping.rs:149`, `:188` (doc-comment mentions — reworded to
  `Json`), `:174`, `:199` (classify arms), `:557`, `:623` (unit-test
  constructors);
  `push_exec/execute.rs:701` (label-delta arm);
  `taskboard-app/src/connections.rs:84` — **the user-facing
  class-label string `DeckError::Envelope(_) => "Envelope"` becomes
  `"Json"`** (deliberate, listed: the connection-error class name
  mirrors the variant; leaving it "Envelope" would recreate this
  finding one layer up).
- **Not touched** (name collisions that are *not* this variant):
  `OcsEnvelope` (`ocs.rs`), `ErrorEnvelope`
  (`taskboard-app/src/commands/output.rs:842`, an unrelated JSON
  error-response type) and its use at `commands/mod.rs:2169`.

### D9 — F6-15: doc cross-links at both definitions, link-direction aware

- **Edit 1** (`push_exec/outcomes.rs:11–14`, the `GroupOutcome` doc
  gains a disambiguation line): "One letter apart from the domain's
  per-op [`PushOutcome`]: that type is one operation's result inside
  the [`Outcomes`] vec; `GroupOutcome` is the per-group bookkeeping
  around the whole group (deferred, or aborted with the landed
  evidence)." — real intra-doc links resolve here (the crate depends
  on `taskboard-domain`).
- **Edit 2** (`remote.rs:206–208`, the `PushOutcome` doc gains the
  mirror line): "Grouped per push group by the sync actor's
  `GroupOutcome` (`taskboard-sync-nextcloud`) — per-group
  abort/landed bookkeeping around these per-op results." — **plain
  code-format prose, deliberately not an intra-doc link**:
  `taskboard-domain` does not depend on the sync crate; an
  unresolvable link breaks `cargo doc` warning-free (the sibling
  chunk's gate).
- **Rejected**: renaming either type (the issue asks only for
  cross-links; `GroupOutcome` is crate-internal, `PushOutcome` is
  domain API — churn without a finding behind it).

## 4. Implementation order & commit breakdown

One logical commit per finding-cluster, each standalone and
build-green (rebase-merge hygiene, AGENTS §8). Pure-doc commits
first (zero-risk), then the code-adjacent ones:

| # | Commit | Findings | Content |
|---|---|---|---|
| 1 | `docs(storage): the audit comment cleanups` | F5-04, F5-05, F5-06 | the three comment rewords (D1–D3) — one stale-comment cluster, one crate |
| 2 | `refactor(storage): remote-column decode arity becomes a typed rejection` | F5-07 | the four decode helpers + the four one-liner call sites + repo.rs's first `#[cfg(test)]` module with the TDD arity pins (D4) |
| 3 | `refactor(sync): the DeckError Envelope variant renames to Json` | F6-14 | the 15-site grep-audited rename incl. the connections.rs class label (D8) |
| 4 | `docs: the sync crate is a Deck REST client, not WebDAV` | F6-12, F12-07, F6-13 | Cargo.toml description + AGENTS §2 row + the architecture.org row sentence (D7 + D10 below) |
| 5 | `docs(sync): the outcome types cross-link their one-letter-apart pair` | F6-15 | the two doc lines (D9) |

### D10 — F6-13: the architecture.org sentence (rides commit 4)

One sentence (the issue's budget), inserted into the crate row right
after its opening ("The sync actor (ADR 0007):"), in the row's org-mode
prose style (=…= for code):

#+begin_example
about 38% of the crate (3 100 of 8 200 production src lines) is a
general Deck wire client (=client.rs= + =model.rs=/=color.rs=/=ocs.rs=/
=backoff.rs=/=error.rs=: boards/stacks/cards/labels/attachments CRUD
with its own hermetic contract tier), which the actor half consumes
through that surface;
#+end_example

Stage 2 compresses to one flowing sentence matching the row's voice;
the numbers may round ("roughly two-fifths") but the file inventory
must stay exact. **Rejected**: folding the sentence into the O1
crate-split fix — O1 was recommended by the audit but never accepted;
the sentence cannot wait on a split that may not happen.

## 5. Verification (stage 2 exit criteria)

- Full §10 gate block through `scripts/run-quality-gates.sh` (this
  box runs parallel agents): fmt, clippy `-D warnings`, nextest, doc
  tests, `cargo deny`, the llvm-cov triple with quarantine + trailing
  rm.
- **TDD evidence**: the malformed-arity tests (D4) must fail against
  the pre-fix code (panic for under-arity, silent pass for over-arity
  — assert `Err(Corrupted)`), then pass. The roundtrip proptest and
  the T7 corruption-class pins stay green **unmodified**.
- **Grep audits**:
  - `grep -rn 'DeckError::Envelope\|Envelope(' crates/ --include='*.rs'`
    → only `OcsEnvelope`/`ErrorEnvelope` hits survive (D8's list
    fully renamed);
  - `grep -rni webdav AGENTS.md docs/architecture.org crates/*/Cargo.toml`
    → zero; the two kept `client.rs` comments are the crate's only
    remaining matches;
  - `grep -rn 'eight SELECTs\|future Phase 5' crates/` → zero;
  - `grep -c 'parse()\s*$'`-style: not needed — no F5-10 work.
- `cargo doc -p taskboard-storage-sqlite -p taskboard-sync-nextcloud
  -p taskboard-domain --no-deps` builds warning-free (D9's
  link-direction choice is the proof: sync-side links resolve,
  domain-side mentions are plain prose).

## 6. Risks

1. **`connections.rs:84` changes a user-facing string** ("Envelope" →
   "Json" in the connection-error class label). Deliberate and
   listed in D8; it is the class name mirroring the variant, and the
   alternative (keeping "Envelope") recreates the finding one layer
   up. Low.
2. **repo.rs's first `#[cfg(test)]` module**: new test home in a file
   that sent everything to `tests/roundtrip.rs` before. Kept minimal
   (arity pins only, no new deps); the tier-map rationale is recorded
   in D4 so the next audit does not re-litigate it. Low.
3. **Slice-pattern refactor touches all four decode mappers**: a
   transposed field in a helper would be caught by the roundtrip
   proptest (256 cases, whole-`PersistedState`) before any snapshot
   could. Compile-time arity + runtime proptest. Low.
4. **`origin/main` moves while this chunk implements** (it moved
   twice during planning). The findings were pinned at `a3987d11`;
   stage 2 re-verifies the touched sites before editing (the
   sibling's practice), and the dispatch's stale-PR-number lesson
   (#448 → #562) applies to any gate re-check: verify by branch name
   + merge-commit ancestry, never by number alone. Medium
   probability, low cost.
5. **Kani harnesses (`verify.rs`) reference the renamed variant**:
   the D8 grep list covers `src/` including `verify.rs`; no hit
   found at planning time, but the grep audit runs over it too
   (it compiles only under Kani, so a stale mention would not fail
   the gates). Low.

## 7. Out of scope (recorded, not done)

- **F5-10** (D5 — its own future M chunk; spin-off issue per §8).
- **F5-08** — accepted-risk verdicts, no action (issue's own text).
- **F5-03** — already fixed (`1112fb2d`), context only.
- **F6-18** (`reorder_card`'s Envelope swallow) — accepted risk; the
  rename touches its match arm's *name* only, not its semantics.
- **The O1 crate split** (`taskboard-deck-client`) — recommended by
  the sync audit, never accepted; D10's sentence is written to be
  true with or without it.
- **ADR/docs WebDAV wording outside AGENTS §2 + the crate row** —
  chunk 15's, per both audit reports' cross-refs.

## 8. Stage-2/3 bookkeeping (issue mechanics)

- **#272 closing comment** (stage 3, at merge): per-finding
  re-verification verdicts — F5-04/05/06/07 fixed (commit refs);
  F5-03 fixed-already (`1112fb2d`, context); F5-11 decline
  re-affirmed (≈26% < ~⅓, family-split framing, D6); F5-08 unchanged
  accepted; F5-10 deferred → spun into the new issue below.
- **#273 closing comment** (stage 3): F6-12/13/14/15 + F12-07 fixed
  (commit refs), including the rename decision's one-line rationale
  and the connections.rs label consequence.
- **F5-10 spin-off issue** (stage 3 files it before closing #272):

  > **Title**: `backlog: storage codec text-form unification (F5-10)`
  > **Labels**: `p3`, `refactoring`, `audit-v0.1.0`
  > **Body**: The storage codec hand-writes TEXT forms mirroring
  > domain serde knowledge for op tags, sync phases/error kinds,
  > validator keys — plus (new since the audit) importance/urgency/
  > insistence and recur/baseline — while `ReminderRef` already uses
  > the delegation pattern (domain-owned `text_form`/`parse_text_form`,
  > `codec.rs:397`). Unifying collapses codec.rs to delegation and
  > folds the 18× clock-parse helper (`decode::clock`). Effort M.
  > Origin:
  > `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md`
  > (Axis H, F5-10); re-verified 2026-11-02 at `a3987d11`
  > (`.artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md`
  > §2, D5).

- **No CHANGELOG** (dispatch); no push, no PR from stage 1/2 — hand
  off at PR-open is stage 3's with the user merging.

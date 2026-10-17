# [Plan] Sync Client Helpers — `is_transport`, `label_delta_failure`, `call<T>`

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 9 of the remediation-wave-1 dispatch
  (dispatch §1/§10, prompt 9.1). Planning only — no implementation in
  this commit.
- **Origin**: issues #248 (F6-19, agenda H-1 **schedule-now**), #250
  (F6-21, agenda H-1), #249 (F6-20, agenda H-2 **accept-with-trigger** —
  the trigger fired: this wave's sync chunks touch the client surface),
  all normative. Owning report:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md`
  (axis H rows F6-19/20/21).
- **Branch / worktree**: `refactor/sync-client-helpers` in
  `../taskboard-WT-refactor-sync-client-helpers`, cut off `main` @
  `53b533c` (= `origin/main`).
- **Anchor base**: all `file:line` anchors below are against `53b533c`
  — post chunks 1/2/3/4/5/6/7 merges (PRs #306/#315/#317/#318/#320/#321/
  #322), so the audit's v0.1.0 anchors have shifted; the census below
  re-anchors every site.
- **Size**: M (per the dispatch's grade; #248 and #250 are S each, #249
  is the M).
- **Floor**: chunk 2's pinning tests — `push_exec.rs` `#[cfg(test)]`
  module (defer + same-stack reorder arms, subsumed-label resolvability)
  and `tests/actor_wiremock.rs` (dead-board determination arms) — plus
  the crate's contract/e2e/wiremock suites, all green unmodified at
  `53b533c`. They stay byte-identical through this chunk.

---

## §shape — three independent helpers, three standalone commit-series

The chunk lands three mechanically independent refactors on one branch,
each its own commit (rebase-merge hygiene: every commit builds
standalone and passes the gates):

1. `refactor(sync): deck error owns its transport-triple membership` —
   #248, touches `error.rs`, `client.rs`, `push_exec.rs` (`mapping.rs`
   unchanged — see the recorded implementation decision in
   §decision-1).
2. `refactor(sync): collapse the label-delta failure arms` — #250,
   touches `push_exec.rs` only.
3. `refactor(sync): shared request choreography in the deck client` —
   #249, touches `client.rs` only.

Series 2 and 3 touch disjoint files; series 1 and 2 both touch
`push_exec.rs` in disjoint regions (the `is_transport` definition/
callers at `:493`/`:509`/`:980` vs the label arms at `:703`–`:756`).
Order 1 → 2 → 3 (smallest ripple first; #249's diff is the one worth
reviewing with nothing else in flight).

## §decision-1 — `DeckError::is_transport` (#248, F6-19)

**Fact discovered during planning**: a free function already exists —

```rust
// push_exec.rs:980-985
pub(crate) fn is_transport(err: &DeckError) -> bool {
    matches!(
        err,
        DeckError::Transport(_) | DeckError::RateLimited { .. } | DeckError::Unavailable
    )
}
```

with two callers (`push_exec.rs:493`, `:509` — the post-create
archive/done-stamp abort checks). So the triple is encoded in **four**
places today, exactly as the audit counted: the retry loop
(`client.rs:1367-1371`), `classify_push`'s `None` arm
(`mapping.rs:173`), `classify_read`'s `Network` arm
(`mapping.rs:192-194`), and this free fn.

**Helper** — the free fn moves onto the type it answers about, in
`error.rs`:

```rust
impl DeckError {
    /// Whether this is a network-level retryable failure — the
    /// `Transport` / `RateLimited` / `Unavailable` triple. The retry
    /// loop, both classifiers, and the push executor's abort check all
    /// key off this membership; a fifth retryable class edits exactly
    /// here.
    pub(crate) fn is_transport(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::RateLimited { .. } | Self::Unavailable
        )
    }
}
```

The name is the issue's (and audit's) chosen name; the doc comment
carries the semantic that "transport" here *means* the network-level
retryable triple (a `RateLimited` is not a transport failure in the
narrow sense — the name survived the audit's wording review, changing
it now would churn four call sites for nothing).

**Conversion census (4 sites):**

> **Recorded implementation decision (stage 2, review-confirmed)**: the
> two `mapping.rs` rows below were **superseded** — both classifiers
> ship untouched, keeping their named triple arms. The planned
> early-return shape cannot compile: both classifier matches are
> catch-all-free and exhaustive, and a guard arm
> (`err if err.is_transport()`) does not count toward exhaustiveness,
> so deleting the triple arm or guarding it both break the build. The
> early-return sketch would have traded the compile-time safety net
> (a new `DeckError` variant must be classified in both classifiers)
> away for the dedupe; the shipped choice keeps the net and dedupes
> only the procedural sites (retry loop, executor abort checks,
> former free fn) — a fifth retryable class edits `is_transport`
> *and* is forced into both classifier arms by the build. The
> helper's doc comment states this scope (procedural sites only).
> Recorded in the series-1 commit message.

| Site | Today | After |
|---|---|---|
| `client.rs:1367-1371` retry loop | `retryable @ (DeckError::RateLimited { .. } \| DeckError::Unavailable \| DeckError::Transport(_))` pattern | `Err(retryable) if retryable.is_transport() =>` — guard form; the catch-all `Err(other)` arm after it keeps the match exhaustive (the `Result` scrutinee is fully covered by `Ok(..)`/`Err(..)`) |
| `mapping.rs:173` `classify_push` | triple or-pattern arm `=> None` | **kept** — exhaustive match retains its named arm (recorded decision above) |
| `mapping.rs:192-194` `classify_read` | triple or-pattern arm `=> SyncErrorKind::Network` | **kept** — exhaustive match retains its named arm (recorded decision above) |
| `push_exec.rs:980-985` free fn + callers `:493`, `:509` | free `is_transport(&err)` | fn deleted; callers become `err.is_transport()` |

**Why the classifiers keep their named arms (supersedes the planned
early-return rationale)**: a guard arm (`err if err.is_transport()`)
does not count toward match exhaustiveness, and these matches have no
catch-all — so neither a guard nor a deleted triple arm compiles; the
only shapes that do are the named triple arm (shipped) or an
early-return plus a catch-all, and the catch-all destroys the
compiler-forced classification of future variants. The named arms
keep the taxonomy's compile-time safety net: every new `DeckError`
variant must be answered by both classifiers, and a fifth
transport-class variant is added to the or-pattern the compiler
points at. The retry loop, by contrast, matches on a `Result`
scrutinee with a catch-all `Err(other)` arm, so the guard conversion
is safe there (and shipped).

**Consequences stay distinct** (the audit's explicit requirement): the
retry loop still retries-then-returns; `classify_push` still answers
`None` (abort, no outcome); `classify_read` still answers
`SyncErrorKind::Network`; the executor still aborts the cycle. The
helper owns *membership* only.

## §decision-2 — `label_delta_failure` (#250, F6-21)

The two classify-or-abort label-delta failure arms in `update_task`
are re-anchored at `53b533c` to:

- **assign arm**: `push_exec.rs:703-723` (inside the
  `for label_num in &desired` loop),
- **remove arm**: `push_exec.rs:741-756` (inside the
  `for label_num in current` loop).

Both build `PushCtx { existing: true, delete: false }`, call
`mapping::classify_push`, and on `Some(..)` return
`primary_outcome(group, Applied { echo: map_echo(&updated, board) })`
— the fields' PUT landed; on `None` warn and return
`Aborted { landed: Vec::new() }`. The only textual difference today is
that the assign arm carries two explanation comments the remove arm
lacks; the *code* is identical. The audit's 41-line Sonar cluster.

**Helper** — an associated fn of `PushExecutor` (it uses
`Self::primary_outcome`), placed next to `classify`
(`push_exec.rs:962`):

```rust
/// Classify-or-abort tail shared by the label-delta sub-endpoints
/// (assign/remove): a *classified* failure reports the fields' PUT as
/// landed — its echo is authoritative for the primary — and the label
/// ops retry next cycle; a transport-class failure aborts the cycle
/// with nothing reported: the PUT was a full-send of fields that stay
/// unchanged across the retry, so re-sending the whole group is safe.
fn label_delta_failure(
    group: &PushGroup,
    updated: &Card,
    board: RemoteBoardId,
    err: DeckError,
) -> GroupOutcome {
    if mapping::classify_push(&err, PushCtx { existing: true, delete: false }).is_some() {
        return GroupOutcome::Outcomes(Self::primary_outcome(
            group,
            PushResult::Applied { echo: Some(mapping::map_echo(updated, board)) },
        ));
    }
    tracing::warn!(error = %err, "push aborted: transport failure");
    GroupOutcome::Aborted { landed: Vec::new() }
}
```

Types: `updated` is the `Card` the fields' PUT returned (or the
fetched clone on the tolerated `Envelope` case, `push_exec.rs:665-679`);
`board` is `card_ref.board` (`RemoteBoardId`, `Copy` — both sites
already pass it by value to `map_echo`). Both arms collapse to:

```rust
Err(err) => return Self::label_delta_failure(group, &updated, card_ref.board, err),
```

**Consciously NOT collapsed** — the create-path label arm
(`push_exec.rs:453-468`): same classify-or-abort *shape*, different
*consequence* — its abort carries `GroupOutcome::Aborted { landed:
outcomes }` because the card create DID land and its evidence must
travel (no idempotency keys; decision 3). Different semantics, no
shared body. Recorded so the reviewer does not read it as a missed
third site.

The `#[allow(clippy::too_many_lines)]` on `update_task`
(`push_exec.rs:602`) is re-checked after the collapse; if the fn has
dropped under clippy's threshold the allow goes, otherwise it stays
(the fn legitimately reads in phases — its own comment says so).

## §decision-3 — the client `call` choreography helper (#249, F6-20)

### What is actually repeated

Every JSON request method in `client.rs` repeats this scaffold:

```rust
tracing::debug!(<bespoke request fields>, "<bespoke event>");   // stays (see below)
let resource = format!(<bespoke path>);
let result: Result<T, DeckError> =
    self.send_json(Method::M, &resource, <body>).await;
match &result {
    Ok(x)   => tracing::info!(<bespoke ok fields>, "<ok event>"),
    Err(err) => tracing::warn!(<bespoke err fields>, error = %err, "<err event>"),
}
result
```

The scaffold (send → bind → match → log → return) is the duplication;
the angle-bracketed parts are **per-site specifications**: request
fields (`title`, `color`, `?filter`, ids…), ok fields (`count`/
response-id/`deleted_at`/`bytes`), err fields (request ids — only
*some* sites carry them today), and event texts. No `debug!` line
logs the resource string; several ok/err arms carry data that exists
only in the response (`count`, created-entity ids, `deleted_at`,
content length).

### The design decision (deviation from the issue's sketch, recorded)

Issue #249 sketches `call(&self, method, resource, body, ok_event:
&str, err_event: &str)` — uniform, fieldless event strings, "methods
reduce to resource + body". That sketch silently changes the log
output of every converted site (drops `count`, created ids,
`deleted_at`, byte counts, request ids) — exactly the class of subtle
behavior difference chunk prompt 9.1 forbids unifying silently, and
`&str` event parameters cannot even occupy the message-literal
position of a `tracing` macro. The crate's own established shared
helpers (`card_flag`, `card_label` — the audit's proof of shape) keep
field-rich logging; a fieldless helper goes against the crate's grain.

**Chosen design**: one helper wrapping send + outcome-logging, with
the per-site logging passed as a closure over the typed result —
every log event, field, and text stays byte-identical, and the
duplicated scaffold is deleted, not wrapped:

```rust
/// Sends one JSON request (with retries) and logs the typed outcome
/// through `log`. The scaffold every request method shares; the
/// per-site event fields and texts live in each method's `log`
/// closure.
async fn call<T, L>(
    &self,
    method: Method,
    resource: &str,
    body: Option<impl Serialize>,
    log: L,
) -> Result<T, DeckError>
where
    T: serde::de::DeserializeOwned,
    L: FnOnce(&Result<T, DeckError>),
{
    let result = self.send_json(method, resource, body).await;
    log(&result);
    result
}
```

A converted method reads:

```rust
pub async fn board(&self, id: u64) -> Result<Board, DeckError> {
    tracing::debug!(board_id = id, "fetching deck board");
    self.call(Method::GET, &format!("boards/{id}"), None::<serde_json::Value>, |result| match result {
        Ok(_) => tracing::info!(board_id = id, "deck board fetched"),
        Err(err) => tracing::warn!(board_id = id, error = %err, "fetching deck board failed"),
    })
    .await
}
```

Mechanics: `T` infers from the method's return type (every converted
method's tail is the `call(...).await` expression); `None` bodies keep
their existing `None::<serde_json::Value>` annotation exactly as
today; single-use `let resource` bindings are inlined into the `call`
argument (each is used exactly once — the retry loop inside
`send_with_retries` logs the `Call`'s own copy); the per-site
`debug!` lines stay untouched (they carry request parameters the
helper must not uniformize).

The multipart pair duplicates the same tail around a hand-built
`Call` (`CallBody::Multipart`), with a verbatim-repeated
`map_status`+`decode_envelope` handler — they get a sibling, no wider
than its two call sites:

```rust
/// Multipart sibling of `call` for the two attachment-content routes;
/// `scope`/`for_update` carry the app-level update route's shape
/// (D1 spike §3.2).
async fn call_multipart<T, L>(
    &self,
    method: Method,
    resource: &str,
    scope: Scope,
    upload: &AttachmentUpload,
    for_update: bool,
    log: L,
) -> Result<T, DeckError>
where
    T: serde::de::DeserializeOwned,
    L: FnOnce(&Result<T, DeckError>),
```

(body: build the `Call`, `send_with_retries` with
`map_status`+`decode_envelope`, `log(&result)`, return.)

Both helpers are private (`send_json`'s visibility — no callers
outside `client.rs` exist). Bounds are exactly as wide as the call
sites need (review mandate): `DeserializeOwned` + `FnOnce`.

### Per-site census (29 choreography sites at `53b533c`)

Ok/err "fields" below = what today's `info!`/`warn!` arms carry beyond
the event text; `resp` = response-derived.

| # | Method | client.rs | Path | Ok fields | Err fields | Conversion |
|---|---|---|---|---|---|---|
| 1 | `boards` | :375 | `send_json` | `count` (resp) | — | `call` |
| 2 | `board` | :447 | `send_json` | `board_id` | `board_id` | `call` |
| 3 | `create_board` | :468 | `send_json` | `board.id` (resp) | — | `call` |
| 4 | `delete_board` | :494 | `send_json` | `board_id`, `deleted_at` (resp) | `board_id` | `call` |
| 5 | `stacks` | :519 | `send_json` | `count` (resp) | — | `call` |
| 6 | `stack` | :541 | `send_json` | `stack_id` | — | `call` |
| 7 | `card` | :559 | `send_json` | `card_id` | — | `call` |
| 8 | `labels` | :582 | `send_json` | `count` (resp) | — | `call` |
| 9 | `attachments` | :603 | `send_json` | `count` (resp) | — | `call` |
| 10 | `upload_attachment` | :635 | multipart `Call` | `attachment.id` (resp) | — | `call_multipart` |
| 11 | `download_attachment` | :685 | `send_bytes` | `attachment_id`, `bytes` (resp) | — | **left bespoke** (see inventory) |
| 12 | `update_attachment_content` | :733 | multipart `Call`, `Scope::App` | `attachment.id` (resp) | — | `call_multipart` |
| 13 | `delete_attachment` | :782 | `send_json` | `attachment_id`, `deleted_at` (resp) | — | `call` |
| 14 | `restore_attachment` | :819 | `send_json` | `attachment_id`, `deleted_at` (resp) | — | `call` |
| 15 | `update_board` | :855 | `send_json` | `board_id` | `board_id` | `call` |
| 16 | `restore_board` | :872 | `send_json` | `board_id` | `board_id` | `call` |
| 17 | `clone_board` | :893 | `send_json` | `board.id` (resp) | `board_id` | `call` |
| 18 | `create_stack` | :910 | `send_json` | `stack.id` (resp) | — | `call` |
| 19 | `update_stack` | :933 | `send_json` | `stack_id` | `stack_id` | `call` |
| 20 | `delete_stack` | :955 | `send_json` | `stack_id` | `stack_id` | `call` |
| 21 | `create_card` | :973 | `send_json` | `card.id` (resp) | — | `call` |
| 22 | `update_card` | :1002 | `send_json` | `card_id` | `card_id` | `call` |
| 23 | `delete_card` | :1024 | `send_json` | `card_id` | `card_id` | `call` |
| 24 | `reorder_card` | :1083 | `send_json` | `card_id` | `card_id` | **left bespoke** (see inventory) |
| 25 | `create_label` | :1163 | `send_json` | `label.id` (resp) | — | `call` |
| 26 | `update_label` | :1186 | `send_json` | `label_id` | `label_id` | `call` |
| 27 | `delete_label` | :1208 | `send_json` | `label_id` | `label_id` | `call` |
| 28 | `card_flag` (private) | :1222 | `send_json` | `card_id`, `flag` | `card_id`, `flag` | `call` |
| 29 | `card_label` (private) | :1242 | `send_json` | `card_id`, `label_id`, `action` | `card_id`, `label_id`, `action` | `call` |

Not choreography sites, untouched: `fetch_boards`/`fetch_stacks`/
`fetch_card` (:396/:409/:425 — debug + `fetch_conditional`, no
info/warn tail: poll-frequency reads would log noise on every 304);
`archive_card`/`unarchive_card`/`assign_label`/`remove_label`
(:1047/:1061/:1131/:1147 — thin delegators); `new`/`with_sleeper`;
plumbing `send_json`/`send_bytes`/`fetch_conditional`/
`send_with_retries`/`attempt`.

Net: 25 sites → `call`, 2 → `call_multipart`, 2 left bespoke for
recorded reasons. Expected diff ≈ −100 to −140 lines of `client.rs`
(the audit's 250–300 estimate counted the bespoke `debug!`/field
lines this design deliberately preserves; the reduction is smaller
than the sketch's ceiling because the log *content* is kept
byte-identical).

## §inventory — behavior-difference inventory (enumerated, none silently unified)

1. **`reorder_card` tolerates `Envelope` after send**
   (`client.rs:1115-1118`): a 2xx body that fails to decode is
   rewritten to `Ok(())` *after* `send_json`, and today's warn logs
   the post-processed outcome. A naive conversion would log the
   pre-rewrite error. **Preserved**: the method stays on `send_json`
   with its bespoke tail; not converted.
2. **`download_attachment` uses `send_bytes`** (no envelope decode,
   no content-type gate — the route answers `text/plain`, D1 §3.2):
   different decode path, single call site. **Preserved**: no helper
   exists for a population of one; not converted.
3. **`update_attachment_content` routes `Scope::App`** while
   `upload_attachment` routes `Scope::OcsApi`: **preserved
   structurally** — `call_multipart` takes `scope` and `for_update`
   parameters.
4. **`fetch_*` conditional variants log no outcome**: deliberate
   (poll-frequency 304s). **Preserved**: excluded from conversion.
5. **Per-site ok/err log fields vary** (census columns above): 13
   sites log response-derived data, 15 carry request ids in the err
   arm, 14 carry none — today's inconsistency included. **Preserved
   byte-for-byte** by the closure design; unifying them is
   consciously *not* done (the issue sketch would; recorded here as
   the reason the signature deviates).
6. **`update_card`'s err arm carries braces** (:1012-1014) —
   formatting artifact; normalizes to the common shape, no behavior.
7. **`mapping.rs:192` lists the triple `Transport`-first where
   `:173` lists `RateLimited`-first** — or-pattern order is
   semantics-free; the helper's member set is identical.
   **Consequence-neutral by construction.**
8. **`classify_push`/`classify_read` are untouched as shipped** (the
   planned early-return evaluation-order change was dropped — see the
   recorded decision in §decision-1); the only membership-test shape
   that changed is the retry loop's guard, whose `Err` arms are
   disjoint so ordering is semantics-free there.
9. **The create-path label arm** (`push_exec.rs:453-468`) shares the
   label-delta *shape* but aborts with `landed: outcomes` (the create
   landed; evidence travels). **Preserved as-is**: not collapsed into
   `label_delta_failure`.
10. **`actor.rs:480` `advertised_floor`** matches `RateLimited` only
    (Retry-After hint extraction) — adjacent to the triple but a
    different concern. **Untouched.**
11. **`client.rs` retry loop binds the error** (`retryable @ (…)`) to
    return it after the budget is exhausted — the guard conversion
    (`Err(retryable) if retryable.is_transport()`) keeps the binding
    and the returned error value identical.
12. **The two label-delta arms' comments differ** (assign explains,
    remove is silent): merged into one authoritative doc comment on
    the helper stating the constraint (fields-PUT-landed semantics);
    the code bodies were already identical.

## §tests — what pins the refactor

No new tests. The seams are pinned by existing suites, all of which
must pass **unmodified**:

- #248: `mapping.rs` tests `classify_push_maps_every_table_row`
  (:391) and `classify_read_maps_transport_and_server_classes`
  (:464) already assert the triple's `None`/`Network` outcomes; the
  retry loop is pinned by the crate's contract/retry tests.
- #250: chunk 2's pins — the `update_task` defer/subsumed-label
  suites in `push_exec.rs`'s test module — exercise exactly these
  arms' classify-vs-abort split.
- #249: logging is asserted nowhere (AGENTS §5 forbids text
  assertions); the wiremock/contract/e2e suites pin request shapes,
  which the internal scaffold extraction cannot alter. A unit test
  for `is_transport` itself would re-implement the `matches!`
  (§5: don't test inline reimplementations) — the four converted
  sites' existing coverage is the pin.

## §non-goals

- No retry-policy changes (budgets, classes, backoff) — #248 moves
  *membership*, not behavior.
- No new endpoints, no public-API changes (all three helpers are
  `pub(crate)`-or-private).
- No log-content changes: every event's fields and texts stay
  byte-identical (inventory #5 is the recorded refusal).
- No `push_exec.rs` decomposition (chunk 10 owns it), no module moves.
- No `fetch_conditional`/`send_bytes`/`attempt` plumbing rework.
- No `send_json` signature changes.
- Out-of-scope observations become GitHub issues, not drive-bys.

## §risks

1. **Reviewer reads the closure as "boilerplate wrapped, not gone"**
   (dispatch 9.3's dedupe proof): mitigated by this plan's framing —
   the scaffold is deleted; what remains per site is the site's
   logging *specification* (fields/texts differ per the census). The
   recorded deviation from the issue's fieldless sketch is inventory
   #5; stage 3 judges against this plan.
2. **Classifier exhaustiveness**: shipped shape resolved this — the
   classifiers keep their named triple arms (§decision-1 recorded
   decision), so the compile-time classification net is intact; the
   retry loop's guard is safe because that match has a catch-all
   `Err(other)` arm.
3. **Type-inference friction** at converted sites (`None` body
   annotations, closure param types): today's code already annotates
   `None::<serde_json::Value>`; the closure parameter is fully
   inferred from `call`'s signature. `reorder_card` — the one genuine
   inference/logic trap — is excluded (inventory #1).
4. **Line-count expectation mismatch** vs the audit's 250-300: the
   PR body must state the actual delta and the preserved-log reason
   so the number does not read as under-delivery.
5. **Chunk-10 ordering**: this branch must merge before chunk 10
   (module tree) starts or chunk 10 rebases — the dispatch chain
   already encodes this; no action inside the chunk.

# [Plan] Chunk 3.2 — Unreadable 4xx/5xx split (#401)

- **Date**: 2026-10-31 (dispatch-scheduled name; written 2026-11-02)
- **Chunk**: 3.2 of backlog round 2
  (dispatch plan: `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §"Chunk 3.2 — Unreadable 4xx/5xx split (#401)"; size S; same-crate gate
  1.1 → 2.3 → **3.2** → 4.3)
- **Branch**: `feat/appdata-unreadable-split`, cut off the fetched
  `origin/main` at `67c0b531` (local `main` was 38 commits behind; the
  same precedent as chunk 2.3's plan — every seam below was read at the
  branch tip, not at local `main`). Worktree:
  `../taskboard-WT-feat-appdata-unreadable-split` (real dir beside the
  main checkout at `/home/viktoria/nextcloud/viktoria/programming/`,
  `src/` symlink; created after `scripts/worktree-doctor.sh` passed —
  230 G free, floor 25 G; upstream tracking unset).
- **Covers** (normative): GitHub issue **#401** ("split
  `Unreadable(Transport)` into 4xx/5xx download-failure evidence"),
  which names the two additive shapes (new variants **or** a
  `SyncErrorKind` payload) and the single mapping site
  (`sidecar_evidence`). Design context: sidecar design plan
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` — D-S4
  (the typed evidence enum) and D-S7's failure-fence table, whose
  "Document download fails (transport)" row is the only download row
  the design named (the gap this chunk closes).
- **Gate**: chunk 2.3 merged. The dispatch's numbers #272/#273 are the
  backlog *issues*; the implementing PR is **#578**
  (`chore/audit-cleanups-storage-sync`), MERGED 2026-11-02T01:12Z.
  Verified by merge-commit ancestry: `d3b9654e` ∈ `origin/main`
  (rebase-merge rewrote the commit SHAs — the pruned branch ref is not
  an ancestor; the rewritten commits are present, e.g. `4e865dbf`
  "the DeckError Envelope variant renames to Json", `ffcf4464`
  "remote-column decode arity becomes a typed rejection"). Gate
  satisfied.
- **Status**: planning complete; every open question resolved in §3
  (D1–D9). Implementation not started.
- **Acceptance** (dispatch): a 4xx download failure and a 5xx map to
  **distinct typed evidence**, pinned at the mapping boundary;
  wire/snapshot additions additive (old persisted rows still decode);
  suite green.
- **Hard rule**: **widening only, no semantic drift.** Every behavior
  the current pins hold for today's evidence forms — the cycle
  completes, folds nothing, never uploads, the cache stays untouched,
  no `NetworkLost` event — must hold for every existing form *and*
  every new form after this chunk. The only intended behavior change
  is the *label* on two download-failure classes that `Transport`
  conflates today.

---

## 1. Method

Every seam was read at the branch tip (`67c0b531`). The chunk is S and
additive, but the enum sits under **six** match sites (two of them
*not* compile-forced), one persisted serde wire, two presentation
surfaces, and one docs line — so the plan's job is to enumerate every
edit, pin the fence semantics that are *not* compiler-enforced, and
mark the one existing pin that deliberately moves.

## 2. Seam map (what touches `AppDataUnreadable` today)

| # | Site | Kind of coupling | Compile-forced on new variants? |
|---|---|---|---|
| S1 | `crates/taskboard-domain/src/messages.rs:627-642` — the enum (`FormatTooNew { format }` / `Transport` / `Decode` / `ForeignBoard`; `Copy + Eq + serde`) | the definition | — |
| S2 | `crates/taskboard-sync-nextcloud/src/actor.rs:917-930` — `sidecar_evidence`'s download `Err` arm: **every** `download_attachment` error → `Transport` | **the mapping this chunk splits** | no (single construction site) |
| S3 | `crates/taskboard-sync-nextcloud/src/actor.rs:656-663` — cycle flush gate: `attempt = !matches!(evidence, Unreadable(Transport \| Decode \| FormatTooNew))` — a **deny-list** or-pattern | semantics | **no — new variants silently fall through to `attempt = true`** (contained: `run_flush`'s residual arm returns `Idle`, §S6) |
| S4 | `crates/taskboard-sync-nextcloud/src/flush.rs:182-186` — `has_flush_work`'s `rebuild_trigger = matches!(evidence, Absent \| Unreadable(ForeignBoard))` | semantics (allow-list) | no — new variants → no rebuild trigger (safe default) |
| S5 | `crates/taskboard-sync-nextcloud/src/flush.rs:214-227` — `run_flush`'s base match: `Document` → read base; `Absent \| Unreadable(ForeignBoard)` → rebuild base; residual **`Unreadable(_) => FlushVerdict::Idle`** | the structural no-upload fence | no — residual arm already denies every future variant |
| S6 | `crates/taskboard-domain/src/state.rs:88-109` — `SidecarHealth::Unreadable(AppDataUnreadable)` on the persisted `SyncStatus` (`:76`); `derive_sidecar_health` is a **total pass-through** (`Unreadable(reason) => SidecarHealth::Unreadable(*reason)`) | persistence + engine | no — widening-transparent, zero engine edits |
| S7 | `crates/taskboard-app/src/commands/output.rs:257-283` — CLI `sidecar_view`: exhaustive 4-arm match → `SidecarHealthView { reason: String, format }` (snake strings: `"format_too_new"`, `"transport"`, `"decode"`, `"foreign_board"`) | CLI wire | **yes** |
| S8 | `crates/taskboard-ui-slint/src/presentation.rs:313-329` — `sidecar_caption`: inner exhaustive match → kebab tags (`"transport"`, `"decode"`, `"foreign-board"`; `FormatTooNew` outer-matched, `unreachable!()` in the inner arm) | UI badge | **yes** |
| S9 | `docs/architecture.org:364-365` — the enum spelled verbatim: `Unreadable{FormatTooNew|Transport|Decode|ForeignBoard}` | docs | no |
| S10 | `crates/taskboard-sync-nextcloud/src/client.rs:1489-1502` (`send_bytes` → `map_status` `:338-353`) | **the download error domain**: status-typed `DeckError`s, raw bytes (no JSON decode → `Json(_)` impossible on this route); retryable triple (`Transport(_)` / 429 `RateLimited` / 503 `Unavailable`) surfaces only after backoff exhaustion; 500-family `Server(u16)` is immediate | — |

**The download error domain** (what `sidecar_evidence`'s `Err` arm can
actually receive, per S10): `Transport(_)`, `RateLimited { .. }`
(429, exhausted), `Unavailable` (503, exhausted), `Unauthorized` (401),
`Forbidden` (403), `NotFound` (404), `BadRequest` (400), `Conflict`
(409), `PreconditionFailed` (412), `Server(u16)` (immediate 5xx).
`Json(_)`, `InvalidBaseUrl`, `InvalidUpload` are unreachable on this
route (no body decode; client built once at boot; upload-only local
validation) but the classifier must still arm them (D4).

**Existing pins** (what holds the mapping today):

| Pin | Asserts |
|---|---|
| `a36_unreadable_variants_travel_typed` (`tests/actor_wiremock.rs:2931-2977`) — the variant table | each decode-path form (`Decode`/`FormatTooNew`/`ForeignBoard`, body-mounted at 200) travels typed, the cycle completes, and **`download_hits == 1`** — the cache-untouched invariant, per form |
| `a37_download_failure_folds_nothing_and_still_completes` (`:2984-3013`) | a **500** download → `Unreadable(Transport)` today, cycle completes, **no `SystemEvent`** (not a lost network) — *this pin moves to `ServerFault`, D9* |
| `f11_transport_fence_no_upload` (`:3664-3700`) | 500 download → no flush PUT, flush ops stay queued — the no-upload fence end-to-end |
| `f12`/`f13`/`f14` (`:3702-3760+`) | the decode / format / foreign-board fences (`f14`: ForeignBoard *does* flush, rebuild) |
| `unreadable_evidence_folds_nothing_and_warns_on_the_row` (`taskboard-domain/src/pipeline.rs` tests, `:2675-2721`) | `Unreadable(FormatTooNew)` folds nothing + the row carries the reason; `Absent` is healthy |
| anchor.rs tests `:485-511` | `build_evidence` maps corrupt body / too-new format / foreign board — **untouched by this chunk** (decode path, not download path) |
| `S-5` metadata-cache test | success-only caching — untouched |
| state tests (`engine.rs:1827`, `snapshots.rs` `Absent`-only, `undo.rs`, `proptest.rs`) | all use `Absent`/`FormatTooNew` — **zero existing snapshot churn** |
| `presentation.rs:2368-2375`, `journeys.rs:2410` | UI caption tags + the render-only-when-unreadable journey |

## 3. Decisions (every open question resolved)

### D1 — Shape: distinct unit variants, **not** a `SyncErrorKind` payload

The issue shows both are additive; the payload variant
(`Rejected(SyncErrorKind)`) is rejected, for four recorded reasons:

1. **Domain width.** `SyncErrorKind` (`state.rs:170-188`) is the
   *cycle-failure* vocabulary — `NoBoard`, `LocalData`, `Network` are
   unreachable on an attachment download. A payload carrying a wider
   enum than its value domain is exactly the leniency the house
   type-driven policy (AGENTS §2.1) says to avoid while
   enums-with-data suffice; policing it (`Rejected(Network)` must be
   unconstructible) would need a minting-constructor apparatus this
   chunk doesn't earn.
2. **Two spellings of one fact.** With the payload, the retryable
   triple could surface as both `Transport` and
   `Rejected(SyncErrorKind::Network)`; the mapping would need an
   `is_transport()` fork to keep them apart. Distinct variants make
   the conflation structurally impossible.
3. **Consumers act on three operator actions, not seven kinds.**
   Self-heals-by-retrying (transport) vs fix-credentials/sharing
   (4xx) vs runbook/manual-restore (5xx) is the entire actionable
   distinction the issue asks for (`Auth` vs `Forbidden` have no
   distinct operator action; `NotFound` self-heals — D6). Finer
   payload grain would put unused vocabulary on the persisted state
   wire (S6) and both presentation surfaces (S7/S8).
4. **Classifier discipline preserved.** The mapping stays a
   compiler-exhaustive match over `DeckError` (D4) — a new
   `DeckError` variant forces the evidence classifier to decide, the
   same mechanism `mapping.rs` documents for `classify_read`. A
   payload would add a second compiler-exhaustive surface
   (`classify_read`) between the error and the evidence for no
   consumer gain.

### D2 — Variant names: `AccessDenied` and `ServerFault`

```rust
pub enum AppDataUnreadable {
    /// The document's `format` is newer than this build's ... (unchanged)
    FormatTooNew { format: u32 },
    /// The content download failed with a transport-class error
    /// (connection, DNS/TLS, timeout) or a rate-limit/service-unavailable
    /// budget exhausted after retries — self-heals by retrying. Also the
    /// conservative fallback for download outcomes nothing else classifies.
    Transport,
    /// The server refused the download: 401 (credential rejected) or
    /// 403 (the account cannot read the attachment). Retrying cannot
    /// self-heal until credentials or the ACL change; the lane stays
    /// fold-only.
    AccessDenied,
    /// The server answered the download with a persistent fault: an
    /// immediate 5xx, or a 4xx rejection that is neither auth nor
    /// access (400/409/412). The runbook's manual-restore territory.
    ServerFault,
    /// The body is not decodable ... (unchanged)
    Decode,
    /// The decoded `board` is not the cycling board ... (unchanged)
    ForeignBoard,
}
```

Names avoid collision with the existing vocabularies
(`DeckError::Forbidden`, `SyncErrorKind::Forbidden`/`Server`) while
staying operator-legible; ordering keeps the three download classes
adjacent. Derives unchanged (`Debug, Clone, Copy, PartialEq, Eq,
serde::Serialize, serde::Deserialize`) — `Copy` survives (unit
variants), so S6's pass-through and every existing generic bound hold.

### D3 — The mapping table

New classifier `classify_download_evidence(err: &DeckError) ->
AppDataUnreadable`, applied in `sidecar_evidence`'s download `Err`
arm (S2). Conservative default: **anything not explicitly
re-classified stays `Transport`** — today's behavior, per the issue.

| `DeckError` (download route) | HTTP | Today | New | Rationale |
|---|---|---|---|---|
| `Transport(_)` | — (conn/DNS/TLS/timeout) | `Transport` | `Transport` | unchanged — the self-healing class |
| `RateLimited { .. }` | 429, after retries | `Transport` | `Transport` | unchanged — exhausted backoff, self-heals by retrying |
| `Unavailable` | 503, after retries | `Transport` | `Transport` | unchanged — same class |
| `Unauthorized` | 401 | `Transport` | **`AccessDenied`** | credential rejected — "fix credentials", not "retry" |
| `Forbidden` | 403 | `Transport` | **`AccessDenied`** | the issue's motivating case — the read-only connector's permission wall; fold-only lane until the ACL changes |
| `Server(u16)` | 500/502/504…, immediate | `Transport` | **`ServerFault`** | the issue's other half — persistent server fault, manual-restore territory |
| `BadRequest` | 400 | `Transport` | **`ServerFault`** | a GET with no parameters rejected is a server-side quirk; retrying cannot fix it (`classify_read` maps 400 → `Server` — same verdict family) |
| `Conflict` | 409 | `Transport` | **`ServerFault`** | not constructible by a plain GET; classified, never panicked |
| `PreconditionFailed` | 412 | `Transport` | **`ServerFault`** | same — classified for exhaustiveness |
| `NotFound` | 404 | `Transport` | **`Transport`** | **D6** — the conservative default, deliberately |
| `Json(_)` | — | unreachable | **`Decode`** | documented even though `send_bytes` never decodes: *if* the route ever gains envelope parsing, a 2xx-undecodable body is the `Decode` fence's exact subject — not a server fault |
| `InvalidBaseUrl` / `InvalidUpload` | — | unreachable | **`Transport`** | conservative fallback with a doc comment (client built once at boot; upload-only validation) — mirrors `classify_read`'s conservative-comment precedent |

### D4 — Classifier home and structure

`mapping.rs` beside `classify_push`/`classify_read` — the crate's
normative-classifier home. Structure: **one compiler-exhaustive match
over `DeckError`, grouped exactly per the D3 table, no wildcard arm**
(the `mapping.rs` discipline: a future `DeckError` variant must be
forced to decide here). Doc comment names the conservative-default
rule and cross-links D-S7's fence table and the issue. `#must_use`,
pure — unit-testable at the lowest layer (T1).

### D5 — Flush gates: flip the actor deny-list to a default-deny allow-list

S3's `attempt = !matches!(.. Transport | Decode | FormatTooNew)` is a
deny-list that new variants silently escape (they'd reach
`run_flush` — contained by S5's residual `Idle`, but the ceremony
call is wrong). Restructure to the positive form:

```rust
let attempt = matches!(
    read_evidence,
    AppDataEvidence::Document(_)
        | AppDataEvidence::Absent
        | AppDataEvidence::Unreadable(AppDataUnreadable::ForeignBoard)
);
```

— the read-succeeded allow-list. A future variant lands in
`attempt = false` by construction: **the write side defaults to
deny**. S4 (`has_flush_work`) and S5 (`run_flush` residual arm) need
**no edits** — both already default-deny new variants; a comment at S5
gains the new variant names so the doc doesn't under-describe ("the
unreadable evidence fence" comment lists Transport/Decode/FormatTooNew
today). End-to-end fence behavior is pinned per form by T3/T4
regardless of the gate shape.

### D6 — 404 on the download stays `Transport` (recorded, not debt)

A 404 mid-cycle is the tombstone race (attachment deleted between the
detail scan and the content GET). Considered and rejected:
`Absent` would wake the flush write arms (S4/S5 group `Absent` with
`ForeignBoard` as *may replace content*) — a blind overwrite against a
vanishing carrier, forbidden by D-S7's read-modify-write fence.
`AccessDenied`/`ServerFault` would both misdirect the operator (the
former says "fix sharing", the latter "manual restore"; neither
applies — **next cycle's metadata scan sees the tombstone and the
evidence becomes `Absent` on its own**). `Transport` is honest
("retry next cycle") and is today's behavior — the conservative
default the issue blesses. The D3 row documents this; no
`..._currently_pins_...` naming applies because this is the desired
semantics, not tolerated debt.

### D7 — What deliberately does **not** change

- `build_evidence` (anchor.rs) — the decode path
  (`Decode`/`FormatTooNew`/`ForeignBoard`) is untouched; its pins
  stay green unmodified.
- `derive_sidecar_health` (S6) — total pass-through; zero engine
  edits; the engine's fold consumes only `Document` evidence by
  construction, so widening cannot change ingest semantics (the
  issue's own invariant).
- The success-only document cache (`sidecar_evidence` inserts on
  `Document` only) — variant-agnostic, additive-safe; pinned per new
  form by T3.
- `SyncErrorKind`, `DeckError`, `classify_read`, `classify_push`,
  backoff, `APPDATA_FORMAT`, the sqlite schema (dispatch rule 4: no
  migration) — untouched.
- The tracing warn in the download `Err` arm (S2) keeps logging the
  typed `?err`; only the evidence value it wraps changes.

### D8 — Presentation surfaces (additive strings, per-surface style)

- CLI (S7): two new arms → `reason: "access_denied"` / `"server_fault"`
  (snake, matching the existing CLI strings), `format: None`.
  `architecture.org:178` already documents `sidecar.reason` as
  additive on the CLI wire.
- UI (S8): two new inner arms → `"access-denied"` / `"server-fault"`
  (kebab, matching the existing tags). The inner match is
  compile-forced; the `FormatTooNew { .. } => unreachable!()` arm
  stays valid (FormatTooNew remains outer-matched).
- Both surfaces' existing unit enumerations extend by two entries
  (T6).

### D9 — The one pin that moves: `a37`'s expected variant

`a37` mounts a 500 and asserts `Unreadable(Transport)`. After the
split a 500 is `ServerFault`. The update keeps every invariant the
test guards (cycle completes, typed evidence, no `NetworkLost`
event) and changes only the expected variant — the mapping under
test *deliberately changed*; this is not weakening (AGENTS §5: the
test found no bug; the contract moved by design). The
transport-class rows (conn/429/503 → `Transport`) get their own
deterministic pin at the mapping boundary (T1) rather than a
retry-exhausting wiremock mount — the backoff-exhaustion mechanics
are the client layer's already-pinned behavior, not this chunk's.

## 4. TDD steps (ordered; red before green where the step adds behavior)

**T1 — mapping boundary (lowest layer, pure).**
`mapping.rs` unit tests for `classify_download_evidence`: one row per
D3 table entry (401→`AccessDenied`, 403→`AccessDenied`,
`Server(502)`→`ServerFault`, `BadRequest`→`ServerFault`,
`Conflict`→`ServerFault`, `PreconditionFailed`→`ServerFault`,
`Transport`→`Transport`, `RateLimited`→`Transport`,
`Unavailable`→`Transport`, `NotFound`→`Transport`,
`Json`→`Decode`, `InvalidBaseUrl`→`Transport`,
`InvalidUpload`→`Transport`). Asserts are `matches!`/equality on the
enum value — structural, no text. This is the dispatch acceptance's
"4xx vs 5xx distinct **at the mapping boundary**" pin. Written
first; red until D4 lands.

**T2 — domain widening + compile-forced consumers.**
Add the two variants (D2) to `messages.rs`. The build then forces S7
and S8; extend their unit enumerations in the same commit (D8) so
every commit stands alone. Doc comments per D2.

**T3 — the mapping, live, per form (wiremock variant table).**
Extend `a36`'s table to carry the mount status (a small triple
`(body, status, expected)` instead of `(body, expected)`), adding:
`("", 403, AccessDenied)` and `("", 500, ServerFault)` — each row
asserts the typed variant, cycle completion, and
**`download_hits == 1`** (cache untouched, per form — the existing
per-form invariant continues over the new forms).

**T4 — the fence, end-to-end, per new form (wiremock).**
Two f-series siblings of `f11`: mount download 403 / 500 with flush
work seeded (`flush_seed::repo_with_flush_work`) and a content-PUT
mock; assert `AccessDenied` / `ServerFault` evidence, **no PUT hits**,
flush ops stay queued, cycle completes. These pin D5's restructured
gate and S5's residual arm against the new forms regardless of gate
shape. Additionally one two-cycle pin: with a 403 download, run two
nudges and assert `download_hits == 2` — a failure form never
populates the cache (the success-cache pin `S-5` covers the other
side).

**T5 — engine-level fold-nothing + row carry for a new form.**
Extend the pipeline test family
(`unreadable_evidence_folds_nothing_and_warns_on_the_row`) with one
`Unreadable(AccessDenied)` (or `ServerFault`) run: local app values
survive the fold, the `SyncStatus.sidecar` row carries the typed
reason. One representative form suffices — `derive_sidecar_health`
and the fold are variant-agnostic by construction (D7); the test
guards the *wiring*, not each variant.

**T6 — presentation enumerations (with T2).**
`output.rs`/`presentation.rs` unit tests that enumerate the reason
strings/tags gain the two new entries each (house style: matched by
value — these strings *are* the typed CLI wire contract).

**T7 — old-rows-still-decode pin (the issue's wire requirement).**
A codec-level test proving a persisted-state JSON whose
`SyncStatus.sidecar` row says `Unreadable("Transport")` (old shape)
still decodes on the new build, plus a roundtrip of the two new
variant names. Anchor at implementation time beside the existing
state-decode tests (`codec.rs` decode helpers / `repo.rs` load tests
— follow whatever serde-roundtrip pin pattern exists there; if none
exists, a domain-level serde roundtrip test for
`SidecarHealth::Unreadable` is the fallback home).

**T8 — docs.**
`docs/architecture.org:364-365`: the enum spelling gains
`AccessDenied|ServerFault`. One sentence in the same block noting the
three download classes (self-heal / fix-access / manual-restore) so
the runbook framing survives into the docs.

**Commit plan** (every commit standalone, rebase-merge hygiene):
`feat(domain): unreadable download evidence gains access-denied and
server-fault forms` (T1+T2+T6+T7) →
`feat(sync): the sidecar download maps 4xx/5xx failures distinctly`
(T3+T4+T5) → `docs: the unreadable evidence table names the new
forms` (T8). Order may collapse to two commits if T2 and T3/T4 land
together without an intermediate non-building state — the enum
widening alone breaks S7/S8, so T1/T2/T6 must share a commit.

## 5. Wire / snapshot impact (all additive)

- **Persisted state** (S6): `SyncStatus.sidecar` serializes the
  enum by variant name. Old rows (`Healthy`,
  `Unreadable(Transport|Decode|ForeignBoard|FormatTooNew)`) decode
  unchanged on the new build — the issue's "old reports still
  decode", pinned by T7. New rows carry new names; **a downgrade**
  to an older build reading a new-name row fails serde decode →
  `RepositoryError::Corrupted` (the codec's typed corruption
  verdict, `codec.rs:111`'s `.ok()` path) — accepted additive-enum
  forward incompatibility, minor-version release-note material.
- **insta snapshots**: zero churn — every existing snapshot uses
  `Absent` evidence (`snapshots.rs`, `undo.rs`, `proptest.rs`,
  `bootstrap.rs` fixtures); new tests add new snapshots only.
- **CLI JSON**: additive `reason` values (`"access_denied"`,
  `"server_fault"`); consumers matching reason strings see new values
  only when the new condition occurs. Documented additive
  (`architecture.org:178`).
- **UI**: additive caption tags; the warning-render journey is
  variant-agnostic and untouched.
- **No IPC/message change**: `SyncReport`/`AppDataEvidence` are
  in-process mpsc only; the sqlite schema gains nothing (no
  migration — dispatch rule 4 holds).

## 6. Risks

| Risk | Mitigation |
|---|---|
| S3's flush gate is *not* compile-forced — the one silent-drift site | D5 flips it to a default-deny allow-list (future variants fail safe); T4 pins the fence end-to-end per new form; S5's residual `Idle` arm is the structural backstop either way |
| `a37` pin move could be mistaken for test weakening | D9 records why the invariant set is preserved; the review check is: cycle-completes + typed-evidence + no-`NetworkLost` all still asserted |
| 404 semantics debated (it is *gone*, not *transport*) | D6 records the three alternatives and why each is worse; next cycle self-heals to `Absent` — observable, not stranded |
| Downgrade decode failure (`Corrupted`) | accepted (additive serde enum); release notes; T7 pins the forward direction the issue demands |
| Transport-class rows (429/503/conn) only pinned at the mapping boundary, not via a retry-exhausting wiremock mount | the backoff-exhaustion path is the client layer's pinned behavior (error.rs doc + backoff tests); re-testing it per evidence form would duplicate a lower layer's test (AGENTS §5, one behavior one test lowest layer) |
| `unreachable!()` in the UI caption's `FormatTooNew` arm | unaffected: FormatTooNew keeps its outer arm; the inner match stays compile-forced for the new arms (S8) |

## 7. Diff estimate

≈ **260–330 lines including tests**, no schema migration:

- `messages.rs` +16 (two variants + doc comments)
- `mapping.rs` +30 (classifier + doc) and +45 unit tests (T1)
- `actor.rs` ±10 (Err arm call + gate restructure, D5)
- `output.rs` +12 / `presentation.rs` +10 (arms + tag strings)
- `pipeline.rs` tests +~55 (T5); codec/serde pin +~25 (T7)
- `actor_wiremock.rs` +~120 (a36 extension, two f-siblings, two-cycle
  pin, a37 update)
- `docs/architecture.org` +3 lines

## 8. Stage-2 handoff

- Work in the existing worktree
  `../taskboard-WT-feat-appdata-unreadable-split` (branch
  `feat/appdata-unreadable-split` @ `67c0b531` + the plan commit; if
  resuming later, recreate it per AGENTS §8 after
  `scripts/worktree-doctor.sh`).
- Quality gates through `scripts/run-quality-gates.sh` (the shared
  flock wrapper), never pasted raw.
- Hand off at "PR open + `gh pr checks` green"; one PR for the
  chunk (single coherent deliverable); remove the worktree at
  PR-green. Do not push without the user's instruction; the user
  merges on GitHub.
- Expected CI hazards: none known — no migration, no snapshot churn,
  no workspace dependency changes.

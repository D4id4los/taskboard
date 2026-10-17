# [Plan] EngineCommand → EngineEnvelope Rename — Scope & Mechanical Procedure

- **Date**: 2026-10-17
- **Kind**: stage-1 plan for chunk 1 of the remediation-wave-1 dispatch
  (dispatch §1/§2, prompt 1.1). Planning only — no implementation in
  this commit.
- **Origin**: issue #302 (normative; standing decision (c) of the
  v0.1.0 audit session, findings report §agenda row (c) +
  §agenda-outcomes row (c)): the payload/envelope split between
  `StateCommand` and `EngineCommand` is mechanism-accurate and
  documented, but the session judged the teachability cost real —
  rename so the payload→envelope relation is self-evident.
- **Branch / worktree**: `refactor/enginecommand-envelope-rename` in
  `../taskboard-WT-refactor-enginecommand-envelope-rename`, cut off
  `origin/main` @ `7b31c0b` (local `main` @ `070241e` was behind; the
  dispatch plan itself landed on `origin/main` via PR #108's rebase
  merge, so `origin/main` is the dispatch base).
- **Census base**: `7b31c0b` (all file:line anchors below are against
  this tree; the planning checkout's `crates/` tree is byte-identical,
  verified by `git diff --stat` = empty).

---

## §context — the two enums and the relation being named

- `taskboard-domain/src/messages.rs:20` — `StateCommand` (UI/CLI →
  State Engine mpsc **payload**): pure data, `Serialize` +
  `Deserialize`, 24 variants, zero channel types. The module doc
  (`messages.rs:4-7`) already fixes the vocabulary: "Every type here is
  pure data. The channel-bearing *envelopes* … are declared by the
  actor crates that own the channels."
- `taskboard-state/src/actor.rs:43` — `EngineCommand` (the engine
  inbox **envelope**): `#[derive(Debug)]` only, four channel-bearing
  variants —
  - `Execute { command: StateCommand, reply: Option<oneshot::Sender<…>> }` (:46)
  - `Flush { reply: oneshot::Sender<()> }` (:56)
  - `ReadState { reply: oneshot::Sender<…> }` (:65)
  - `AttachConnector { sync_out: mpsc::Sender<SyncCommand>, reply: oneshot::Sender<AttachReceipt> }` (:75)

  Its own doc (:40-41) says "The engine inbox: channel-bearing
  envelopes owned by this crate, with the domain payloads inside
  (payload/envelope split)" — the type is the only word in the
  sentence that disagrees with the docs' terminology.

The rename's entire purpose is typographic: make the type name say
what every prose description already says.

## §decisions

### D1 — New name: `EngineEnvelope` (candidate confirmed)

Evaluated against the payload/envelope wording the docs already use:

| Candidate | Verdict |
|---|---|
| **`EngineEnvelope`** | **Chosen.** (a) The codebase's own prose already uses exactly this word for exactly this type: the enum doc calls its variants "channel-bearing envelopes", the `SyncStateReader` impl doc (`actor.rs:203`) says "read port over the engine envelope", the `ReadState` variant doc (:60) says "read envelope", `architecture.org:129` and ADR 0007:66 both write "=EngineCommand::ReadState= envelope". (b) Joins the existing `Engine*` family in the state crate (`EngineHandle`, `EngineCore`, `EngineParts`, `EngineStartupError`) and pairs naturally with domain's `EngineSignal` without colliding. (c) Zero collisions: `grep -rn EngineEnvelope` over the tree hits only the dispatch plan's own candidate-name mentions (`.artifacts/`). |
| `EngineInbox` | Rejected: names the queue, not the message — "an inbox called `Execute`" reads wrong, and no doc uses "inbox" for the variants (only for the channel as a whole). |
| `StateCommandEnvelope` (prefixed scheme) | Rejected: verbose, and misleading — `Flush`/`ReadState`/`AttachConnector` carry no `StateCommand`; the envelope carries more than commands. Also invites the twin question of renaming `StateCommand` itself, which this chunk explicitly avoids (see §serialization). |

Homonym note (recorded, not blocking): sync's `DeckError::Envelope`
(`error.rs:23-25`, the F6-14 finding) is an unrelated HTTP-body parse
error in another crate; F6-14's own remedy may rename it to `Json`
later. No conflict at any surface.

### D2 — Variant and field names: all unchanged

- Variants `Execute` / `Flush` / `ReadState` / `AttachConnector` stay:
  each names the verb the envelope asks the engine to perform, and
  each reads correctly under the new head noun
  (`EngineEnvelope::Execute`, `…::Flush`, `…::ReadState`,
  `…::AttachConnector`).
- `Execute`'s field `command: StateCommand` stays. The relation is
  carried by the type name (`EngineEnvelope` vs `StateCommand`); the
  field name describes its role in the verb ("execute *this
  command*"). Renaming it to `payload` would trade the natural
  verb-object reading for a redundant restatement of the type-level
  relation — churn without clarity.
- **No other identifiers rename.** The private `EngineHandle` field
  `commands` (`actor.rs:108`), `COMMAND_BUFFER` (:36), the
  `command_rx` locals (:259, :307), and `run_loop`'s `commands`
  parameter name the *role* (the engine's command inbox); the type
  names the *shape* (each item is an envelope). Minimal diff = safest
  zero-behavior-change review.

### D3 — Historical-docs treatment

- `docs/architecture.org` (:125, :129, :175): inline rename — it is a
  living doc and the dispatch acceptance names it explicitly. Wording
  otherwise untouched (chunk 12 `docs/arch-org-release` owns the
  rewrites around F15-01/02/03).
- ADR 0007 (:66) and ADR 0014 (:33): **one dated amendment bullet
  each** (house style: `* Amendment (2026-10-17): …`, precedent ADR
  0003:51, 0004:132, 0009:167, 0011:96, 0012:52), noting the rename
  and pointing at issue #302. Original decision prose stays verbatim —
  ADRs are decision records of their time, and the amendment keeps
  both spellings greppable.
- `CHANGELOG.md`: the two `EngineCommand` mentions (:246, :263) are
  inside the released `[0.1.0]` section and stay verbatim (Keep-a-
  Changelog: historical sections record what shipped). The rename gets
  its own one-line `### Changed` entry under the existing empty
  `## [Unreleased]` heading — the re-export is public API of a
  library-licensed crate.
- `.artifacts/**`: untouched. Dated plans/reports are the audit trail
  (AGENTS §7); the ~90 `EngineCommand` mentions there are historical
  by design. The stage-3 grep gate excludes them explicitly (§procedure
  V4) so they are not flagged as half-renames.

### D4 — Commit shape (per dispatch prompt 1.1)

Two commits, each standalone:

1. **`refactor(state): rename EngineCommand to EngineEnvelope`** —
   every code identifier + the rustdoc/intra-doc-link mentions inside
   the two touched `.rs` files. Compiles; full gate suite green; the
   workspace contains zero live `EngineCommand` code references.
2. **`docs: sweep EngineCommand prose to EngineEnvelope`** — the
   living-docs sweep of D3 (architecture.org, ADR amendments,
   CHANGELOG Unreleased entry). Docs-only; gates stay green.

No third commit; nothing else moves.

## §census — the full ripple surface (@ 7b31c0b)

Workspace-wide `grep -rn EngineCommand` (excluding `target/`,
`.artifacts/`, `.git/`): **21 hits in 6 files** — and the code ripple
is far narrower than the dispatch prompt anticipated: **no app, UI,
sync, storage, or test-module references exist**. The enum is consumed
only inside its own crate; `taskboard-app` (bootstrap, CLI commands,
IPC) and `taskboard-ui-slint` (forwarder) reach the engine exclusively
through `EngineHandle`'s methods (`execute`/`dispatch`/`flush`/
`attach_connector`) and the `SyncStateReader` port, which construct
the envelopes internally. The audit's "wide but mechanical ripple"
(agenda row c) is therefore entirely a *prose* ripple, not a code one.

| File | Sites | What |
|---|---|---|
| `crates/taskboard-state/src/actor.rs` | 12 | enum def :43; `EngineHandle.commands` field type :108; constructors :126, :139, :161, :184, :215; `run_loop` receiver param :307; match arms :358, :379, :386, :389 |
| `crates/taskboard-state/src/lib.rs` | 2 | crate doc `[`EngineCommand`]s` :5; `pub use` re-export :81 |
| `docs/architecture.org` | 3 | channel-table rows :125, :129; engine bullet :175 |
| `docs/adr/0007-…org` | 1 | `ReadState` envelope mention :66 (amendment, D3) |
| `docs/adr/0014-…org` | 1 | `AttachConnector` mention :33 (amendment, D3) |
| `CHANGELOG.md` | 2 | historical 0.1.0 lines :246, :263 (verbatim, D3) |

Also verified clean: no `.snap` (insta) file, no Slint file, no test
directory, no `Cargo.toml`, no workflow file mentions the type; the
planning checkout's `crates/` tree is byte-identical to the census
base. Expected test-file changes from the rename: **zero**.

## §serialization — proof for the non-goal

The non-goal: **no behavior, signature-shape, or serialization
changes.** Proof that the rename cannot touch any persisted/on-wire
format:

1. `EngineCommand` (the renamed type) derives **only `Debug`**
   (`actor.rs:42`) and holds `oneshot::Sender`/`mpsc::Sender` fields —
   it is unserializable by construction and appears in no serde call,
   no codec, no snapshot, no fixture anywhere in the tree.
2. `StateCommand` (untouched by this chunk) derives
   `Serialize`/`Deserialize` (`messages.rs:19`), but the census shows
   it reaches no persisted or on-wire format: the daemon IPC protocol
   (`taskboard-app/src/ipc/protocol.rs:19`) carries raw **argv words**
   (`Exec { v, argv: Vec<String> }`) and rendered responses — the
   server re-parses argv via `Cli::try_parse_from` (ADR 0012 §4.2
   "the request carries raw argv words"); storage persists
   `PersistenceAction`s (commands are interpreted into actions before
   the repo sees anything); the sync crate has zero `StateCommand`
   hits; UI's `router.rs` constructs values, never serializes them.
   The serde derives are exercised only by domain-internal roundtrip
   tests.
3. Since no `StateCommand` serialization site exists outside domain
   test code, and the chunk renames neither `StateCommand` nor any
   variant/field of either enum, wire and disk formats are provably
   invariant under this diff. (Serde's wire shape keys on variant
   names, none of which change.)

## §procedure — exact edits (binding for stage 2)

### Commit 1 — `refactor(state): rename EngineCommand to EngineEnvelope`

1. `crates/taskboard-state/src/actor.rs`: single-token replace
   `EngineCommand` → `EngineEnvelope` at the 12 census sites. No other
   token changes; the enum's doc comment (:40-41) already describes
   envelopes and needs no edit (the type name now agrees with it).
2. `crates/taskboard-state/src/lib.rs`: doc line :5
   `[`EngineCommand`]s` → `[`EngineEnvelope`]s`; re-export line :81.
3. Verify:
   - V1 `grep -rn "EngineCommand" crates/` → zero hits.
   - V2 `cargo fmt --all -- --check`, `cargo clippy --workspace
     --all-targets -- -D warnings`, `cargo nextest run --workspace`,
     `cargo test --doc --workspace`, `cargo deny check` all green
     (full AGENTS §10 suite; `cargo llvm-cov --workspace` per §10 —
     rename-neutral, coverage deltas can only be line-mapping noise).
   - V3 `cargo doc --no-deps -p taskboard-state` reports no broken
     intra-doc link (the renamed `[`EngineEnvelope`]` link at lib.rs:5
     must resolve).

### Commit 2 — `docs: sweep EngineCommand prose to EngineEnvelope`

1. `docs/architecture.org`: :125 `=EngineCommand::Execute=` →
   `=EngineEnvelope::Execute=` (and the `=EngineCommand::Flush=` on
   the same row); :129 `=EngineCommand::ReadState=` →
   `=EngineEnvelope::ReadState=`; :175
   `=EngineCommand::{Execute … Flush…}` → `=EngineEnvelope::{…}`.
   Surrounding wording untouched.
2. ADR 0007: append one amendment bullet (dated 2026-10-17) noting
   `EngineCommand` was renamed `EngineEnvelope` (issue #302); leave
   :66 verbatim. ADR 0014: same for its :33 mention.
3. `CHANGELOG.md`: under `## [Unreleased]` add `### Changed` with one
   line: `taskboard-state`'s public `EngineCommand` re-export renamed
   to `EngineEnvelope` — mechanical, no behavior change (issue #302).
4. Verify:
   - V4 `grep -rn "EngineCommand" . --exclude-dir=target
     --exclude-dir=.artifacts --exclude-dir=.git` → exactly the four
     sanctioned survivors: CHANGELOG :246/:263 (historical 0.1.0) and
     ADR 0007:66 / ADR 0014:33 (original prose beside their amendment
     notes). Everything else zero.
   - V5 gate suite still green (docs-only diff; pre-commit hooks).

Stage 3 additionally re-checks V1–V5 and reviews that the diff
contains no logic edits (dispatch prompt 1.3's zero-behavior-change
proof: `git diff main...HEAD` shows only the rename + prose).

## §non-goals (explicit)

- **No behavior change** — no logic, no arm reordering, no logging
  text change, no channel/buffer size change.
- **No signature-shape change** — variant set, field names, field
  types, and trait impls of the enum are byte-identical modulo the
  type name; `EngineHandle`'s public methods unchanged.
- **No serialization change** — proven impossible (§serialization).
- **No `StateCommand` rename** — the payload keeps its name; the
  relation becomes explicit via the envelope's name alone.
- **No drive-by fixes**: F4-04 stale state-config doc comments belong
  to chunk 13 (#246); `DeckError::Envelope` renaming belongs to
  F6-14's lane; anything else noticed becomes a new GitHub issue, per
  the dispatch's wave-wide bookkeeping rule.
- **No `.artifacts/` edits** and no CHANGELOG `[0.1.0]` edits (D3).

## §risks

- **Chunk-12 overlap**: `docs/arch-org-release` rewrites
  architecture.org sections adjacent to our three edited rows
  (F15-02's channel-ownership sentence sits at :137-141, same table).
  Dispatch order already serializes this (chunk 1 lands first; chunk
  12 rebases). Trivial text conflicts at worst.
- **Teachability not fully realized by the rename alone**: the goal is
  served only if docs keep pointing the relation; the amendment notes
  and the Unreleased entry are what keep future greps landing right.
- **Review trap — phantom half-renames**: `.artifacts/` and the
  historical CHANGELOG lines keep the old name *by design*; V4's
  exclusion list is the contract, so stage 3 must not "fix" them.
- **Name-similarity**: `EngineSignal` (domain broadcast payload) vs
  `EngineEnvelope` (state mpsc envelope) — both already coexist in
  prose with these exact roles; `EngineCommand`→`EngineEnvelope`
  strictly reduces the confusion surface (`Command` was the overloaded
  word: `StateCommand`, `SyncCommand`, `EngineCommand` all carried it).
- None of the risks touch runtime behavior; the code diff is 14
  identifier sites in 2 files.

## §acceptance (from dispatch §2, plus this plan's additions)

- Single mechanical rename commit-series exactly as §procedure; every
  commit standalone (builds, gates green, meaningful subject).
- Zero behavior change; zero test-content changes (expected count 0).
- All call sites and doc comments updated per the census; V1/V4 grep
  contracts hold with exactly the four sanctioned survivors.
- architecture.org + crate docs name the new type; domain crate gains
  no dependency (trivially: only `taskboard-state` files change in
  commit 1).
- Full AGENTS §10 gate suite green on both commits.

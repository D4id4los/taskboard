# [Plan] Multi-board Chunk 6 — CLI/REPL Connector Verbs and Per-connector Status

- **Date**: 2026-10-20
- **Kind**: stage-1 TDD-first work plan for chunk 6 of the multi-board
  implementation round (dispatch plan §7, prompt 6.1). No code in this
  commit; this plan is stage 2's contract.
- **Origin** (normative rank, dispatch MD-7): implementation plan
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`
  §2 DF-2/DF-6/DF-9 (attribution), §3 chunk 6, §4, §5, §6; design doc
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  §3.2 (the wire-compat consequence, D-W6's additive door), §3.3
  (caller-side burden, CLI consequences), §3.6 (the
  `require_connector` pluralisation + `--connector` paragraphs), §5
  ACCEPTED record; dispatch plan
  `.artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md`
  §1/§7.
- **Status**: READY for stage 2. Precondition verified 2026-10-20:
  chunk 5 (PR #375, `feat/app-connector-registry`) is merged; base is
  `origin/main` @ `215750d`. Worktree
  `../taskboard-WT-feat-app-cli-connector-verbs`, branch
  `feat/app-cli-connector-verbs`. `scripts/worktree-doctor.sh` green
  (144G free, floor 25G; it also flagged chunk 5's already-gone
  worktree registration — pruned, nothing on disk). Chunk 7 may be in
  flight concurrently: app crate vs ui crate, no shared files, no
  shared worktree (dispatch §1).
- **Exit criterion owned here**: "one board moved between connectors"
  (impl plan §5) — unbind + rebind onto the second connector's remote,
  verified by **per-board status attribution moving** in the additive
  `status` answer. Tier-2 two-instance journey (MD-5 honest record).
  PR carries no closure keyword; annotation comment on #125 (MD-4).

## 1. Scope

**In** (impl plan §3 chunk 6, normative): the additive `status` answer
(`connectors: […]` + `boards: […]`; the `sync` field byte-shape
unchanged — D-W6's only door) in `commands/status.rs` +
`commands/output.rs` + the IPC `Status` answer's shared view; human
rendering of the arrays (zero-bound connector ⇒ configured-and-idle);
`--connector <name>` flags on the verbs where targeting matters
(`cli.rs` + preludes); `ReplSession.current_board` + the
`boards use <uuid>` line **replacing chunk 2's interim fill**;
completion growth (board ids, connector names, the new verb/flags);
the move-board tier-2 journey; this chunk's CHANGELOG Unreleased line.

**In, by recorded deviation (D6-1/D6-2)**: `StateCommand::BindBoard`
gains `connector: ConnectorId`, and its equality/refusal comparisons
become binding-scoped. The impl plan's chunk-6 non-goal says "no
engine/domain changes beyond the wire shape", but chunk 5's committed
stage-1 plan D5-18 explicitly recorded that "`BindBoard` tags the
engine's `default_connector`, and that is **chunk 6's move-board
surface**", and the wave exit criterion assigned to this chunk
("rebind onto the second connector's remote") is unimplementable
without a way to mint a non-default tag. This is a gap-fill the round
already owns, not a new design: see D6-1 for the fidelity argument
(it touches no ACCEPTED decision-record row).

**Out** (non-goals, restated): no UI work (chunk 7 — the
`INTERIM(multi-board-2 → chunk-7)` markers in
`crates/taskboard-ui-slint/src/view_state.rs` are chunk 7's, untouched
here); no per-connector poll/backoff knobs; no daemon-side
attach/detach *verb* growth beyond what chunk 5 landed (the wire's
`AttachConnector.name` already exists); no `--connector` on
`connect`/`login`/`logout` (D6-11); no fuzzy reconciliation growth; no
JSON schema versioning; no engine changes beyond the D6-1/D6-2 motion;
no tracker edits beyond the #125 annotation comment (stage 3).

## 2. Re-verified seam table (2026-10-20, base 215750d)

Symbols are the stable keys (impl plan §2); line numbers are this
plan's day references. Every symbol was re-located in the worktree at
base `215750d`.

| Seam | Where | Shape today |
|---|---|---|
| `StatusView` | `crates/taskboard-app/src/commands/status.rs:37` | `{daemon, version?, backend?, pid?, sync?, holder_pid?, connector_attached?}` — all optional fields `skip_serializing_if`; `server(app)` reads `app.engine.shared_state().load_full().sync.aggregate()`; `no_daemon(db)`, `in_session(flag)` |
| `render_text` / `emit` | `status.rs:128/166` | one daemon line with the sync-phase clause + detached suffix; `emit` is the single emission path (JSON on `out` under `--json`, text otherwise) |
| `SyncStatusView` | `crates/taskboard-app/src/commands/output.rs:136` | `{phase, last_success, pending_ops, error_kind}` — the `sync` field's shape (the byte pin) |
| IPC `Response::Status` | `crates/taskboard-app/src/ipc/protocol.rs:95` | `{v, status: StatusView}` — the daemon answer envelope; round-trip + key-set pins in `protocol.rs` tests (`absent_optional_fields_are_skipped` pins the all-`None` view to `{"daemon": false}` exactly) |
| binary `status` routing | `crates/taskboard-app/src/main.rs:234` | daemon present ⇒ IPC `conn.status()` ⇒ `StatusView::server`; else `no_daemon`; REPL `status` line ⇒ `dispatch_line`'s `in_session` arm (`commands/mod.rs:1204`) |
| `require_connector` | `commands/mod.rs:46` | pluralised (D5-8): refuses `SyncDisabled` iff `configured_connectors(config).is_empty()` |
| `configured_connectors` / `ConnectorSpec` | `crates/taskboard-app/src/connectors.rs:148/169` | `Vec<ConnectorSpec{name, backend, server_url, username}>`; array world = exactly the array in file order (first = default); legacy scalar synthesizes `nextcloud` or none |
| `client_for` | `crates/taskboard-app/src/commands/boards.rs:30` | builds `DeckClient` from `config.server_url()/username()` — the **legacy scalar accessors**; shared by `boards list`, `select_prelude`, `publish_prelude` |
| `boards list/select/publish` | `boards.rs:47/748/599` | `require_connector` → `client_for` → remote IO; `select_prelude` pre-validates the remote id against a fresh listing; `select_on`/`publish_on` dispatch `StateCommand::BindBoard { board, remote }` (`boards.rs:684/886+`) |
| `resolve_board` / `resolve_board_or_none_for_bind` | `commands/mod.rs:456/501` | DF-2's CLI default: flag ⇒ typed lookup; else single-live default, zero/≥2 ⇒ `AmbiguousBoard { candidates }` |
| `SyncCommand::BindBoard` arm | `crates/taskboard-domain/src/command.rs:876` | mints `RemoteBoardBinding { connector: default_connector (engine spawn param), board: remote }`; same-remote no-op + `BoardAlreadyBound` + `RemoteBoardInUse` compare **board id only** (the in-code "at N=1 the connector tag is constant" interim note, command.rs:879); bind-wipe compares `r.board != *remote` |
| `default_connector` (engine) | `crates/taskboard-state/src/actor.rs:314`, `bootstrap.rs:671` | `modules.first()`'s id, fixed at spawn; also the sync-report adopt-new mint (stays) |
| `RemoteBoardBinding` | `crates/taskboard-domain/src/ids.rs:132` | `{connector: ConnectorId, board: RemoteBoardId}`; descendant refs (`RemoteStackRef` etc., ids.rs:141–167) carry `board` only — **no connector tag** (accepted design) |
| `AppState.sync` | `crates/taskboard-domain/src/state.rs:41/101` | `SyncStates(BTreeMap<BoardId, SyncStatus>)` with `get(board)` + `aggregate()` (phase dominance `syncing>failed>offline>idle`, max `last_success`, saturating sum) |
| `sync` verb | `commands/sync.rs:57/73/107` | `SyncOutcome{Succeeded,Failed,TimedOut,NoWait}(SyncStatusView)`; the wait decision table polls `sync.aggregate()`; `sync_on` refuses `SyncUnavailable` on a `NoOp` nudge |
| `ReplSession` | `crates/taskboard-app/src/repl.rs:348` | `{config, store, json_default, config_path, prompts}` — **no `current_board`** (chunk 2's interim: the shared single-live default inside `resolve_board` is the session's only board resolution) |
| the marked interim | `repl.rs:1065` (test comment) | "the single-live default is the INTERIM multi-board-6 resolve" — the only `interim` marker in the app crate (the other three are ui/chunk-7) |
| REPL line routing | `repl.rs:454/478/521` | `resolve_line`/`resolve_words` (clap parse + the five routing refusals) → loop intercepts `connect`/`login`/`logout` as ceremonies (repl.rs:579–615), everything else → `dispatch_line(&session, …)` (immutable session) |
| session dispatch | `commands/mod.rs:1182` | `dispatch_line` over `impl CoreSession`; `boards_line`/`stacks_line`/`labels_line`/`tasks_line` call the `*_on` seams with the line's `Option<BoardId>` |
| completion | `crates/taskboard-app/src/completion.rs:73/253/264/616/754` | `Kind{Verbs,SubVerbs,Flags,Stacks,Tasks,Labels,Literals,None}`; `flag_value_kind`/`positional_kind` map by long name / clap id; `CompletionSpec::from_cli()` walks the real clap catalogue (spec-walk test pins it); `ReplHelper::new(state)` — state only, no config |
| CLI catalogue | `crates/taskboard-app/src/cli.rs:29` | `--board` on the board-scoped verbs (chunk 2); `Sync{no_wait,timeout_secs}`; `BoardsCmd` incl. chunk 3's `Unbind`/`Reconcile`; **no** `--connector`, no `boards use` |
| tier-2 harness | `crates/taskboard-app/tests/it_docker.rs:951`, `tests/common/mod.rs` | chunk 5's `it_nextcloud_docker_two_connectors_live_and_partial_outage`: `spawn_instance_sync(suffix, port)` + `OwnedInstance` + `Cleanup` drop, `create_board_and_stack`, `retry`, `eventually`, `seed_tagged_board_binding`, `KeyedFixedStore`, real `[[sync.connectors]]` config, poll 1s |
| runner caveat | `.artifacts/reports/2026-10-20-multi-board-connector-registry-chunk5-review-report.md` | this runner's docker host kills the first test-owned instance ~86 s after spawn (six identical failures, cause isolated to the host, not the branch); stage 2 ran it green once; CI's nextcloud-integration workflow is the stable runner |

## 3. Settled decisions

Every open question the dispatch prompt named is settled here with a
recorded decision (D6-N). Deviations from the normative documents are
marked with their reason; none touches an ACCEPTED design-record row
(design §5) — the one textual deviation (D6-1) is argued against that
bar explicitly.

### 3.1 D6-1 — `BindBoard` gains the connector (the deferred move-board surface)

**Decision.** `StateCommand::BindBoard` widens to
`BindBoard { board: BoardId, remote: RemoteBoardId, connector:
ConnectorId }` (domain `command.rs` + the `Execute` wire mirror in
`messages.rs`), and the engine mints the binding tag from the
**command**, not from `EngineParts.default_connector`. The engine
stays config-blind: it does not validate the name against any
configured set (topology rule — the engine is connector-count
agnostic, DF-10); the **app layer resolves and guards** the name (D6-5:
`resolve_connector` refuses `UnknownConnector(name)` against
`configured_connectors(config)` before any dispatch). A mistagged
binding minted by a hypothetical future caller would strand honestly
(no actor owns it; `pending_ops` climbs visibly) — the same honesty
class as unbound ops.

**Why this is in-scope (the recorded deviation).** The impl plan §3
chunk 6 lists only app files and says "no engine/domain changes
beyond the wire shape". But chunk 5's committed stage-1 plan (D5-18,
`.artifacts/plans/2026-10-19-multi-board-connector-registry-plan.md`,
also quoted in its tier-2 test at `it_docker.rs:1069–1071`) records:
"chunk 5 has no ceremony verb that mints a non-default tag —
`BindBoard` tags the engine's `default_connector`, and **that is
chunk 6's move-board surface**". The wave exit criterion "one board
moved between connectors … verified by chunk 6" (impl plan §5) is
`unbind + rebind onto the second connector's remote`; with a
fixed-default mint, a rebind onto connector beta's remote would mint
an alpha-tagged binding that alpha's actor cannot sync (remote id
absent on alpha's server) and beta's actor never picks up (its
derived board set filters by tag, sync `actor.rs:301`). The criterion
is unimplementable without this motion, so the round already owns it
— deferring again would fail the chunk's own acceptance.

**Fidelity argument (MD-7).** Not a deviation from any ACCEPTED row:
design §5's record accepts Q1 ("explicit `board: BoardId` on all
board-scoped commands; caller-owned selection") and Q3 ("app-layer
named routing over **tagged bindings**; domain `ConnectorId` from
config names"). Neither pins the binding tag's *mint source*; design
§3.5's ceremony subsections sketch `BindBoard { board, remote }`
without ever answering "how does a bind onto connector 2 name
connector 2" — the gap chunk 4's implementation papered over with the
spawn default and the in-code interim note ("at N=1 the connector tag
is constant", command.rs:879), and chunk 5's D5-18 explicitly deferred
here. The field addition is additive vocabulary on an mpsc-internal
enum (`StateCommand` is never persisted and never crosses IPC — the
same non-compat argument the design used for retiring `SetBoard`).
`EngineParts.default_connector` **stays**: the sync-report adopt-new
mint still uses it.

**Construction sites updated**: `boards::select_on` /
`boards::publish_on` (pass the resolved `ConnectorId`), the domain and
state test corpora (pass explicit tags; existing corpus tests pass the
same tag they today pass as the spawn default — see D6-12), and no
others (grep-verified: `boards.rs`, `snapshots.rs`, `engine.rs`,
domain `command.rs` tests).

### 3.2 D6-2 — binding-scoped equality and refusals

The plural world makes board-id-only comparisons wrong (remote id
namespaces are per-connector); the same in-code interim note owns
this. With the connector in the command:

- **Same-binding rebind** = accepted no-op iff
  `board.remote == Some(RemoteBoardBinding { connector, remote })`
  (full equality — same remote through the same connector).
- **`BoardAlreadyBound { board, bound }`** fires when the board holds
  any *different* binding (including the same remote id on a
  different connector — a genuinely different remote board).
- **`RemoteBoardInUse { remote, board }`** scopes the cross-board
  guard to the same connector: another *live* board holding exactly
  `(connector, remote)` refuses; a different connector's remote with
  the same numeric id does not collide (it is a different board).
- **Bind-wipe**: the comparison stays `descendant.remote.board !=
  remote` (board-id-scoped), unchanged. Recorded edge: a rebind from
  `(alpha, 7)` onto `(beta, 7)` — same numeric id, different
  connector — keeps refs whose `board == 7`; descendant refs carry no
  connector tag (the accepted design's stance, ids.rs:141–167), so a
  tag-scoped wipe would require widening every ref type. Pathological
  (a cross-server board-id collision on the one board being moved),
  out of scope, recorded here so stage 3 sees it was considered.

Tests: domain `command.rs` — the mint-source pin (the command's tag
lands in the binding), same-binding no-op, same-id-different-connector
⇒ `BoardAlreadyBound`, cross-connector `RemoteBoardInUse` no-collision,
and the wipe-count pin (unchanged rule, re-pinned against the widened
arm). The two existing default-tag mint tests
(`bind_board_mints_the_binding_with_the_default_connector_tag`,
`…_custom_default_connector_tag`) are **updated, not weakened**: they
pin that the command's tag is the mint (passing the tag the old test
passed as spawn default keeps their snapshot output identical).

### 3.3 D6-3 — the additive `status` answer: shape, serde mechanics, IPC growth

**New view types** (`commands/output.rs`):

```rust
/// One connector's status row: the derived health aggregate over its
/// boards' sync rows, and those boards.
pub struct ConnectorStatusView {
    pub name: String,                    // the config-minted id
    pub health: SyncStatusView,          // D6-4's derivation
    pub boards: Vec<BoardStatusView>,    // its bound boards, board-id order
}

/// One board's status row in the arrays.
pub struct BoardStatusView {
    pub board: String,                   // local board uuid
    pub title: String,
    pub remote: Option<u64>,             // bound remote board id; None = local-only
    pub connector: Option<String>,       // derived attribution; None = local-only
    pub sync: SyncStatusView,            // the map row, or the default when none exists
}
```

`StatusView` grows two trailing fields (declaration order is the JSON
key order — pinned):

```rust
    /// Per-connector status (present in core-holding projections:
    /// every configured connector, zero-bound ones included).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectors: Option<Vec<ConnectorStatusView>>,
    /// The flat per-board view (same population rule).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boards: Option<Vec<BoardStatusView>>,
```

**Serde mechanics**: `Option<Vec<_>>` + `default` +
`skip_serializing_if` gives additivity in **both** directions — an old
binary decoding a new answer ignores the unknown keys (no
`deny_unknown_fields` on `StatusView`); a new binary decoding an old
answer gets `None` (ADR 0012's additive precedent, no
`PROTOCOL_VERSION` bump — the `AttachConnector.name` pattern).

**Byte-shape pins** (the D-W6 door):

1. The `sync` field's *value* shape is `SyncStatusView` verbatim —
   untouched, still the `SyncStates::aggregate()` fold over **all**
   boards. Design §3.2's parenthetical ("over the default connector's
   boards") and impl plan chunk 1 ("the `aggregate()` fold") agree in
   the single-board v0.1.x world they priced; chunk 1's landed
   all-boards fold is the normative later reading and stays (the
   per-connector detail now lives in `connectors[]`; changing the
   `sync` semantics now would be the field's quiet redefinition).
2. `no_daemon` answers stay **byte-identical** to today: both new
   fields stay `None` there — the existing
   `absent_optional_fields_are_skipped` full-equality pin
   (`{"daemon": false}`) must pass **unmodified**.
3. `in_session` answers (D6-10) grow the arrays but keep
   `sync: None` (its documented presence rule is "present when
   `daemon`" — unchanged).
4. The daemon-side answer gains exactly the two keys, appended after
   `connector_attached` — a new pin asserts the server answer's key
   set = the old set ∪ {`connectors`, `boards`}, and the `sync`
   object's key set unchanged.

**IPC growth**: none in `protocol.rs` itself — `Response::Status`
already carries `StatusView`; the arrays ride inside it. `protocol.rs`
diff is pins only (the round-trip corpus gains populated arrays; the
key-set pin above). The client (`ipc/client.rs`) and the binary's IPC
relay need no change (they forward the typed view).

### 3.4 D6-4 — attribution derivation and configured-and-idle

`StatusView::server(app)` derives, in one pass over
`app.engine.shared_state().load_full()` + `configured_connectors
(&app.config)`:

1. Group every **live** (`!deleted`; archived included — archived
   boards stay commandable and sync) board's map row under its
   binding's connector tag. Attribution is **derived from the
   binding, never stored** (DF-9; the stage-3 trap list).
2. Per configured connector (config order — the default first):
   `ConnectorStatusView { name, health: aggregate_rows(its rows),
   boards: […] }`. Zero bound boards ⇒ `health` is the default status
   (`idle`, `0` pending) and `boards: []` — rendered
   **configured-and-idle**: honest, never hidden.
3. `boards[]`: every live board in board-id order — bound ones with
   `connector`/`remote` set and their row (default when no row exists
   yet, e.g. a just-bound board before its first cycle), local-only
   ones with `connector: null`/`remote: null` (the map has no row
   after unbind; the default renders). Hiding local-only boards would
   make the flat view useless in exactly the N-board world it exists
   for. Recorded caveat: an unbound board's stranded ops are **not**
   counted in its default row — that count is the unbind receipt's
   and `read_scope`'s job (app-side outbox attribution in status
   would be new engine-adjacent semantics, not the wire shape).

**The fold helper**: `SyncStates::aggregate()`'s fold logic
(dominance/max/sum, `state.rs:129`) must not be duplicated app-side.
Domain `state.rs` gains the pure free function
`pub fn aggregate_rows(rows: impl Iterator<Item = &SyncStatus>) -> SyncStatus`
and `SyncStates::aggregate()` delegates to it. App surfaces
(`status.rs` derivation, D6-6's scoped `sync --connector`) call
`aggregate_rows` over their grouped rows. Additive, pure, one test at
the domain layer (the existing `aggregate()` corpus keeps passing —
pin that delegation with one equality assertion).

**Health is not the authority's up/down set**: per-connector health
is the map-derived fold (per-board `Offline` is set by the engine when
a connector's boards go dark, DF-7) — the authority stays the only
network-truth source and stays private (`authority.rs` exposes no
query; none is added).

### 3.5 D6-5 — which verbs gain `--connector`; resolution; composition

**The targeting-matters rule applied** (design §3.6: "verbs grow
`--connector <name>` where targeting matters" — a verb gains the flag
iff its behavior differs across connectors):

| Verb | `--connector` | Why |
|---|---|---|
| `boards list` | **yes** | which server's boards are listed; prelude client |
| `boards select <remote-id>` | **yes** | which connector's listing pre-validates the id **and which tag the bind mints** (D6-1) |
| `boards publish` | **yes** | which server the remote board is created on + the bind tag |
| `sync` | **yes** | scopes the wait decision table and the reported status to that connector's boards (the nudge stays broadcast — cycling every connector is the cheap `SyncNow`; D6-6) |
| `status` | **no** | its point is the *whole* answer; nothing is targeted |
| `boards create/rename/color/archive/unarchive/delete/reset/unbind/reconcile`, `stacks`, `labels`, `tasks`, reminders | **no** | they target boards/entities (`--board`, selectors); the connector follows the board's binding (derived) — a flag here would be a second way to say what the binding already says |
| `connect`/`login`/`logout` | **no** (D6-11) | the shape-aware writer rewrites the default (first) entry — DF-6's pinned rule; `logout` is identity-keyed (server, username), not name-keyed |
| `daemon`, `config`, `gui` | **no** | not connector verbs |

**Resolution** (`commands/mod.rs`, `require_connector`'s sibling):

```rust
/// Resolves the connector a targeting verb addresses: the named
/// entry, else the default (first). Call after `require_connector`.
pub(crate) fn resolve_connector(
    config: &AppConfig,
    flag: Option<&str>,
) -> Result<ConnectorSpec, CommandError>
```

`None` ⇒ first entry; `Some(name)` ⇒ the matching `ConnectorSpec`;
no match ⇒ new `CommandError::UnknownConnector(String)` (kind
`unknown-connector`, carrying the offending name — the
`UnknownBoard`/`AmbiguousBoard` pattern). No clap-level value parser:
the configured set is config state, so the guard lives handler-side
where config lives.

**Prelude threading**: `client_for(config, store)` (legacy scalar
accessors) gains a spec-based sibling — `client_for_spec(&ConnectorSpec,
store)` (loads the credential by the spec's `(server_url, username)` —
the store is already keyed per identity, DF-6); `boards::list`,
`select_prelude`, `publish_prelude` take the resolved spec (keeping
`require_connector` first). `select_on`/`publish_on` take the
`ConnectorId` for the `BindBoard` dispatch.

**`--board`/`--connector` composition**: orthogonal axes — `--board`
(DF-2 rules, unchanged) names the *local* target; `--connector` +
positional/derived remote name the *remote* side. The move journey is
exactly their composition:
`boards select <beta-remote-id> --connector beta --bind-local-board
[--board <uuid>]` after `boards unbind --board <uuid> --confirm`.

### 3.6 D6-6 — `ReplSession.current_board`, `boards use`, the scoped `sync`

**`boards use`** is a real clap arm — `BoardsCmd::Use { board:
Option<BoardId> }` (`value_parser = parse_board_id`) — so completion,
help, and uuid validation come from the catalogue, not hand-rolled
parsing:

- **REPL**: the session loop intercepts it before `dispatch_line`
  (the connect/logout ceremony precedent, repl.rs:579–615 — it must
  mutate the session, and `dispatch_line` takes `&session`). With a
  uuid: resolve against the live boards (typed `UnknownBoard`/
  `BoardDeleted` pass-through, the `resolve_board(Some(id))` rule),
  set `session.current_board`, emit the receipt. Without: the query —
  emit the current selection. Receipt type `BoardUseView { board:
  Option<String>, title: Option<String> }` (`skip_serializing_if` on
  `title`; `board: None` serializes as `null` — the honest "no board
  selected" answer).
- **One-shot `dispatch`**: refuses typed — new `CommandError::SessionOnly`
  (kind `session-only`; UX text points at the interactive session).
- **Daemon IPC exec**: refused routing-level before dispatch, the
  terminal-bound vocabulary the server already uses for
  `connect`/`login`/`logout` (server.rs:527–567) — the daemon holds no
  interactive session.

**`current_board` mechanics**: `pub current_board: Option<BoardId>` on
`ReplSession` (None at boot — no seeding: DF-2's REPL home is the
`boards use` line only; the *UI* seed rule is chunk 7's
`ViewState` business). **Pre-fill**: a pure function
`prefill_boards(cmd: Command, current: Option<BoardId>) -> Command` in
`repl.rs` that fills every `None` `--board` (and `select`'s
`--local-board`) from `current`; the REPL loop applies it in the
`Execute` arm before `dispatch_line`. Exhaustively unit-tested arm by
arm. `None` current ⇒ no prefill ⇒ the verbs fall to today's
single-live default inside `resolve_board` (DF-2's CLI rules verbatim
— the REPL's zero-board/≥2-board ambiguity refusal keeps its shape).
**Stale selection** (the board dies mid-session): prefill fills the
dead id; the verb's `resolve_board` refuses typed `BoardDeleted`/
`UnknownBoard` naming it — honest and actionable via `boards use
<other>`; **no auto-clear** (no handler silently mutates session
state). **No unset verb**: `boards use <other-uuid>` re-points and the
session is ephemeral; a `--clear` has no named reader (recorded).
The interim marker at repl.rs:1065 goes; its assertion (chunk-3 verbs
routing with flags intact) survives with the comment rewritten.

**Scoped `sync --connector`**: `sync_on`/`wait_for_sync_result` take
the resolved scope; per poll the watched aggregate is
`aggregate_rows(rows of the connector's currently-bound boards)` —
bindings are re-derived each poll (a mid-wait bind/unbind changes the
set honestly). The decision table (Succeeded/Failed/TimedOut) applies
to the sub-aggregate unchanged; `no_wait` reports the scoped view;
`SyncOutcome`'s shape is untouched (the JSON payload byte-shape pin).
Wiring: CLI `Sync` arm + `sync_line` grow the flag; `sync::run`
resolves the spec after `require_connector`.

**The interim-fill removal inventory** (dispatch acceptance: no
marked REPL interim survives): (1) repl.rs:1065–1066 — the test
comment (rewritten); (2) the structural gap itself — the session's
board resolution was the shared single-live default; `current_board` +
pre-fill is the replacement (chunk 2's plan, "REPL dispatch sites …
each marked interim (chunk 6 … replace them)" — the REPL never grew
inline first-live-board code; its interim *was* the absence of session
state, so the inventory closes with (1)+(2)). The ui crate's three
`INTERIM(multi-board-2 → chunk-7)` markers are **not** ours.

### 3.7 D6-7 — REPL `status` grows the arrays

`dispatch_line`'s `Status` arm keeps `StatusView::in_session` (the
client-side discrimination, `daemon: false`) but derives and fills
`connectors`/`boards` from the session's own live core (`app` — the
same derivation as `server`, factored into a shared helper
`derive_connector_views(state, configured) -> (Vec<_>, Vec<_>)` used
by both projections). Reason: the session *is* the core holder;
a REPL `status` that hides per-connector health while one-shot
`status` against the same database shows it would be a regression the
plural world creates on day one. `sync` stays `None` there (D6-3 pin
3). `no_daemon` stays array-less (nothing is booted — there is nothing
to derive).

### 3.8 D6-8 — completion growth

- `Kind::Boards` — live (`!deleted`) boards through the existing
  `live_entries` pattern (title-matched, uuid replacement,
  collision-disambiguated rows): sources `--board`, `--local-board`
  (`flag_value_kind`), and `boards use`'s positional.
- `Kind::Connectors` — the configured names, prefix-filtered (plain
  quoted literals): source `--connector` (`flag_value_kind`).
  **Plumbing**: `candidates()`/`complete()` gain the names alongside
  the state snapshot (a `&[String]` parameter); `ReplHelper::new`
  gains them (from the session's `configured_connectors` at editor
  spawn — names are boot-fixed for a session; the connect ceremony
  rewrites the default entry's identity, never the name set).
- **The select-positional collision**: `boards select`'s positional
  (a remote `u64`) and `boards use`'s positional (a local uuid) would
  share clap id `board`; `positional_kind` maps by id. Fix: set the
  select positional's clap id to `remote_board` (`#[arg(id =
  "remote_board")]` — internal id only, the displayed syntax is
  unchanged), then `positional_kind`: `"board" → Kind::Boards`,
  `"remote_board" → Kind::None` (remote ids are server state — not
  completable locally; `boards list` is their source, unchanged).
- The spec-walk pin test (`the_spec_walk_matches_the_cli_catalogue`)
  grows `use` and the `--connector` rows — the walk itself is
  automatic (it reads the clap catalogue).
- Context correctness (the stage-3 trap): board ids offered only at
  `--board`/`--local-board`/`boards use` positions; connector names
  only at `--connector` positions; nothing new offered at task/stack/
  label selector positions.

### 3.9 D6-9 — the move-board journey (the exit criterion)

**Tier-2** (`it_docker.rs`, beside chunk 5's two-connector test,
same harness conventions): `it_nextcloud_docker_move_board_between_
connectors` (`#[ignore]`, the `it_nextcloud_docker` filter CI already
runs) — own two instances (`-c6a`/`-c6b`, ports 8392/8393;
`spawn_instance_sync` sequential + `Cleanup` drops), `KeyedFixedStore`
two identities, real `[[sync.connectors]]` (alpha = A first, beta =
B), poll 1s, `LIVE_DEADLINE`-class own budget.

The world is built **entirely through the ceremony verbs — no db
fixture** (the point of the surface chunk 5 lacked):

1. `boards select <board-a-id> --connector alpha` (fresh install ⇒
   mint) → adoption settles; `boards create` a second local board,
   `boards select <board-a2-id> --connector alpha --local-board <l2>
   --bind-local-board` → settles. Two boards attributed under alpha
   in `StatusView`'s derivation (assert structurally via the app-side
   view helper, not by re-deriving in the test).
2. `boards unbind --board <l1> --confirm` (strand honestly; the
   receipt's counts are asserted by chunk 3's tests — here the
   binding is gone).
3. `boards select <board-b-id> --connector beta --bind-local-board`
   (l1 is the single live unbound board) → the **rebind onto beta's
   remote**: assert the binding mints tag beta (D6-1's surface),
   adoption settles through beta's actor, and **the per-board status
   attribution moved**: l1 now sits under beta in the derived
   connector grouping, l2 still under alpha — asserted per board, the
   criterion's wording, not mere command success. Server-side: beta's
   board content adopted (l1's stacks/cards present), alpha's boards
   intact (both servers' listings re-checked via their `DeckClient`s).

**Hermetic half** (runs everywhere): the derivation matrix — a
booted standalone core + raw-sqlx-seeded bindings (the chunk-5
`seed_tagged_board_binding` helper) over two tags exercises
`StatusView`'s grouping/health/configured-and-idle/flat-boards
derivation without any server; plus the D6-1/D6-2 domain tests and
the D6-5/D6-6/D6-8 unit suites (§4). **MD-5**: no docker on the
runner (or the chunk-5-documented host-kill flakiness) ⇒ the honest
skip note + report record; CI's workflow is the stable runner (the
chunk-5 precedent — six attempts, forensics, recorded).

### 3.10 D6-10 — `connect`/`logout` stay name-less (recorded closure)

`connect`/`login` rewrite the default (first) entry's identity keys
(DF-6's shape-aware writer — chunk 5's D5-5, pinned by tests); adding
`--connector` there would be *config editing semantics* ("create/
repoint connector beta"), a different ceremony the design never
scoped. `logout` deletes per-identity credentials (keyed
`(server, username)`, never the name). Recorded so stage 3 does not
read the omission as an oversight.

### 3.11 D6-11 — snapshot corpus: no insta pass expected (verified reasoning)

The state-crate snapshot corpus (`tests/snapshots.rs`, 20 snap files)
snapshots `AppState` trees, not commands. The `BindBoard` widening
changes *call sites*, not the persisted/published shapes: corpus
tests that today pass the spawn default as their command's tag keep
byte-identical snapshots when they pass the same tag explicitly (the
minted `RemoteBoardBinding` is the same value). New tests use
distinct tags to pin the command-source mint (D6-1). **Rule for stage
2**: if any `.snap` drifts, stop and check it is exactly the mint-
source change and nothing else — then this chunk owns that one
deliberate pass per DF-9's snapshot rule, reviewed as such; `.pending`
snapshots never ship.

### 3.12 D6-12 — CHANGELOG

`CHANGELOG.md` Unreleased (Added): the additive per-connector/
per-board `status` answer (daemon + REPL; `sync` field unchanged);
`--connector` on `boards list/select/publish` and `sync`; REPL
`boards use` + session board pre-fill; completion for boards,
connectors, and the new surface; the cross-connector bind (the
move-board surface). Chunk 8 promotes.

## 4. TDD plan (red → green per phase; AGENTS §5 rules)

Typed outcomes only — variants/structural payloads asserted, never
message text (rendering pins assert *data survival* — uuids, names,
ids present/absent — the `render_text_names_the_facts` precedent, and
journeys never assert text); no sleeps (`tokio::time::pause()`/
`advance()` where the runtime is in play; `eventually` predicates with
deadlines in tier-2); fakes over mocks (the standalone-core hermetic
boots; `KeyedFixedStore`; raw-sqlx seeds); one behavior, one test, at
the lowest layer that owns it — rendering at the output/status layer,
routing only above it.

### Phase 1 — domain (crates/taskboard-domain)

1. `command.rs`: `BindBoard { board, remote, connector }` mints the
   command's tag (replaces the two default-tag mint tests — same
   snapshot output, command-sourced tag).
2. Same-binding rebind (full equality) ⇒ no-op; same remote id,
   different connector ⇒ `BoardAlreadyBound`.
3. `RemoteBoardInUse` scoped per connector: alpha-42 vs beta-42 do
   not collide; same binding held by another board still refuses.
4. Bind-wipe count unchanged under the widened arm (foreign-board-id
   refs wipe; same-board-id refs stay).
5. `messages.rs`: the widened `Execute` arm round-trips (the wire
   mirror corpus).
6. `state.rs`: `aggregate_rows` fold — dominance/max/saturating-sum
   over an arbitrary subset; `aggregate()` delegates (one equality
   pin over the existing corpus inputs).

### Phase 2 — state engine (crates/taskboard-state)

7. `snapshots.rs`/`engine.rs` `BindBoard` construction sites widen
   (same tags ⇒ snapshots identical — D6-11; any drift ⇒ the
   deliberate-pass rule). No behavioral engine test beyond the domain
   planner tests (the mint is planned, not executed, engine-side).

### Phase 3 — app status + output (the rendering layer)

8. `output.rs`: `ConnectorStatusView`/`BoardStatusView` serde — key
   sets, key order, round-trips; `BoardStatusView.remote/connector`
   `null` vs absent semantics (always-present keys inside the arrays;
   `Option` serializes `null`, not skipped — pinned).
9. `status.rs`: `derive_connector_views` — grouping by derived
   attribution (bindings tagged alpha/beta over a seeded state);
   health per connector; zero-bound ⇒ configured-and-idle (the
   default `health` + empty `boards`); flat `boards[]` includes
   local-only boards (`connector: null`); live-only filter (a
   tombstoned board's binding does not surface); board-id ordering.
10. `StatusView::server` fills `Some` arrays + the unchanged `sync`
    aggregate; `no_daemon` keeps both `None` — the
    `{"daemon": false}` full-equality pin passes **unmodified**;
    the server answer's key-set pin (old keys ∪ the two new,
    appended); `sync`'s object key set unchanged (the byte pin).
11. `in_session` grows arrays (D6-7), `sync` stays `None` — pin.
12. `render_text` (human layout, D6-3b): one line per connector
    (`connector alpha: idle, 2 pending`), one indented line per bound
    board (uuid, title, remote id, phase, pending), the zero-bound
    connector's configured-and-idle line, trailing local-only board
    lines — data-survival assertions (the uuid, each name, the remote
    id present; the flat-boards array is machine-only, not rendered
    twice). The existing daemon-line pins pass unmodified (the arrays
    render after the current clauses).

### Phase 4 — CLI surface + preludes (routing)

13. `cli.rs` parse pins: `--connector` parses on `boards list/
    select/publish` and `sync` (value lands in the arm); `boards use`
    with/without a uuid parses; select's positional id rename is
    invisible to the syntax (parse pin).
14. `resolve_connector`: `None` ⇒ first spec; named hit ⇒ the spec;
    miss ⇒ `UnknownConnector(name)` — unit, three cases.
15. `boards list --connector gama` on a two-connector config refuses
    `UnknownConnector` **before any client construction** (the
    routing pin: resolution precedes IO — no network in the test).
16. `client_for_spec`: builds from the spec's identity (fixed store
    fake); the legacy `client_for` callers all migrate (grep pin in
    review).
17. `sync` scoping (hermetic): seeded bindings + rows; the scoped
    aggregate helper returns the sub-fold (`no_wait` path asserts the
    scoped view without a cycle); the unscoped verb byte-identical
    (existing sync tests unmodified).

### Phase 5 — REPL session (repl.rs, session state)

18. `boards use <uuid>`: sets `current_board`, emits the receipt;
    unknown/deleted ids refuse typed (engine kinds through
    `resolve_board`); `boards use` query answers current/`null`.
19. The pre-fill walk: every board-scoped arm fills from
    `current_board` (including `select`'s `--local-board`); `boards
    create`/`list`/tasks verbs untouched; `None` current ⇒ no-op —
    exhaustive per-arm unit table.
20. Unset behavior: with no `current_board`, verbs route through the
    single-live default (the existing ambiguity refusal journey stays
    green unmodified — the REPL R-series corpus is the pin).
21. One-shot `boards use` ⇒ `SessionOnly` (kind pin); IPC exec of
    `boards use` ⇒ the server's routing refusal (the terminal-bound
    vocabulary — server test).
22. The repl.rs:1065 interim comment is gone (review checklist, not
    a test).

### Phase 6 — completion (completion.rs)

23. `Kind::Boards` via `live_entries` parity (title→uuid, collision
    disambiguation — mirror the Stacks test); `Kind::Connectors`
    prefix-filters the configured names.
24. `flag_value_kind`: `--board`/`--local-board` ⇒ Boards,
    `--connector` ⇒ Connectors; `positional_kind`: `board` ⇒ Boards,
    `remote_board` ⇒ None (select's remote positional does not
    complete local uuids).
25. The spec-walk catalogue pin grows `use` + the new flags;
    context pins: connector names offered only at `--connector`
    positions, board ids only at theirs.

### Phase 7 — IPC + E2E routing (above the output layer: routing only)

26. `protocol.rs`: `Response::Status` round-trips populated arrays;
    an old-shape answer (arrays absent) decodes with `None`s (the
    both-ways additivity pin); existing wire pins unmodified.
27. `cli_ipc.rs`/`ipc.rs`: one routing journey — daemon `status
    --json` answers the arrays (structural); the existing status
    journeys unmodified.
28. `it_docker.rs`: D6-9's move-board journey + the honest MD-5 skip
    note; the chunk-5 two-connector journey unmodified.

### Phase 8 — bookkeeping

29. CHANGELOG Unreleased lines (D6-12); `cargo fmt`/`clippy -D
    warnings`/`nextest`/doc-tests/`cargo deny`/the CI-identical
    three-command coverage sequence green; worktree handoff per
    dispatch (stage 2 does not push).

## 5. File-by-file diff map + size estimate

| File | Motion | Est. lines |
|---|---|---|
| `domain/src/command.rs` | `BindBoard` arm + equality/refusal scoping + tests (D6-1/2) | ~140 |
| `domain/src/messages.rs` | wire mirror + round-trip test | ~20 |
| `domain/src/state.rs` | `aggregate_rows` + delegation + test | ~50 |
| `state/tests/{snapshots,engine}.rs` | construction sites widen (same tags) | ~30 |
| `app/src/commands/output.rs` | the two view types + serde tests | ~160 |
| `app/src/commands/status.rs` | derivation helper, `server`/`in_session` growth, render, pins | ~260 |
| `app/src/commands/mod.rs` | `resolve_connector`, `UnknownConnector`/`SessionOnly` kinds, dispatch/`boards_line`/`sync_line` threading | ~150 |
| `app/src/commands/boards.rs` | `client_for_spec`, prelude/spec threading, `BindBoard` dispatch sites | ~120 |
| `app/src/commands/sync.rs` | scoped aggregate + threading | ~80 |
| `app/src/cli.rs` | `--connector` ×4, `Use` arm, positional id rename, parse pins | ~110 |
| `app/src/repl.rs` | `current_board`, `boards use` interception, pre-fill walk, tests, interim comment | ~260 |
| `app/src/completion.rs` | two `Kind`s, plumbing, spec pins | ~180 |
| `app/src/ipc/protocol.rs` | pins only | ~60 |
| `app/tests/{cli_output,repl,ipc,cli_ipc,cli_verbs}.rs` | §4's catalogue | ~380 |
| `app/tests/it_docker.rs` (+ `common/` reuse) | the move journey | ~220 |
| `CHANGELOG.md` | Unreleased lines | ~10 |

**Estimate**: ~2 200 lines added/changed incl. tests (~950
production, rest tests), 16 files, 4 crates touched (domain/state
lightly, app heavily, sync-nextcloud **zero**). House scale **M**,
matching the impl plan's chunk-6 rating.

## 6. Risks

1. **The D6-1 engine motion in an "app-side" chunk** — the one
   deliberate scope deviation. Mitigations: it is the smallest motion
   that satisfies D5-18's deferral (one field + comparison scoping);
   domain-layer tests own the semantics; the app layer guards names;
   the corpus reasoning (D6-11) keeps snapshot churn at zero unless
   review says otherwise. If stage 2 finds the equality scoping
   cascading further than D6-2 lists (e.g. push-side assumptions on
   board-id equality), **stop and record** rather than widening —
   that would be new design, back to the user (MD-7).
2. **Tier-2 runner flakiness** — chunk 5's review documented this
   host killing test-owned containers (~86 s). The journey carries
   its own budget, Cleanup drops, and the MD-5 honest record; CI is
   the stable runner. Never weaken the journey to green a local run.
3. **Answer growth vs existing pins** — every existing status/wire
   pin that must stay unmodified is named in §4 (the
   `{"daemon": false}` equality, the `sync` key set, the CLI/
   docker journeys). Any other pin that drifts is a finding to
   diagnose, not accept.
4. **Completion plumbing churn** — `ReplHelper::new` and
   `candidates`/`complete` signatures grow the connector names; the
   editor bootstrap call sites are few (repl.rs + tests) but the
   change is wide-ish for what it does. Alternative (names through
   `AppState`) is wrong — config is not engine state.
5. **The `sync --connector` mid-wait re-derivation** — bindings
   changing under a waiting `sync` is rare; re-deriving per poll is
   honest but makes the wait's subject time-varying. Recorded as the
   chosen semantics (the honest reading of "the connector's boards");
   a snapshotted-at-start alternative was rejected (a mid-wait move
   would report stale attribution).

## 7. Handoff

Stage 2 implements exactly this plan in
`../taskboard-WT-feat-app-cli-connector-verbs` (recreating the
worktree on `feat/app-cli-connector-verbs` if missing), TDD, every
commit standalone (`<type>(<module>): …`), the full AGENTS §10 gate
suite green, the CHANGELOG line, no push, no PR, no tracker edits.
Deviations from this plan are recorded with reasons in the stage-2
report; a deviation touching an ACCEPTED design-record row goes back
to the user instead (MD-7).

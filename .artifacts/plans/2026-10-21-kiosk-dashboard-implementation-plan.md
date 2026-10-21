# [Plan] Web Kiosk Dashboard — Implementation (K11 stage 1)

- **Date**: 2026-10-21
- **Kind**: stage-1 implementation plan for chunk K11 of the kiosk
  implementation round (docs-only; branch `docs/kiosk-dashboard-impl-plan`).
  The implementing and reviewing agents work from this document
  (three-stage protocol: planning → implementation → review).
- **Origin**: the accepted design of record
  `.artifacts/plans/2026-10-21-kiosk-dashboard-design-plan.md`
  (K10, merged to `main` as 47398b3/46f5a43/703ca61 — every item
  ruled, §5 decision table, plus the §6 tablet addendum incl. the
  ruled-in §6.4 reconnect script and the §7 forms-readiness
  constraints); implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 KDF-12, §3 K11-lane); dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  (§12 "deliberately not dispatched here" — K11 dispatches from the
  accepted K10 design); #234 (the tablet's constraints).
- **Status**: awaiting the user's acceptance; nothing here is
  implemented yet. Evidence anchors re-verified 2026-10-21 at
  `origin/main` tip `47398b3` (the design doc's `e2bc3f7` references
  were spot-checked and still hold; line numbers below are against
  `47398b3`).

## 1. Scope

Implement the web kiosk dashboard exactly as the accepted design
rules it: an **opt-in, read-only HTTP surface** on the shared
headless boot (daemon + kiosk appliance lifecycles), serving an
index page (sync health + engine counters) and one page per board,
styled for a 2014-era Android browser, refreshed by meta-refresh
with a single ES5 reconnect script, secured by loopback-by-default
bind + capability-token URL for LAN binds.

**Implementation branch**: `feat/kiosk-web-dashboard` (one branch,
one PR — the plan is one coherent deliverable).

**In scope** (each step below is a verifiable unit):

1. Workspace pins for the hyper server stack (§3.1).
2. The `[web]` config section with typed decode rules (§3.2).
3. The token resolution + generation ceremony (§3.3).
4. The dashboard server module: hyper accept loop, GET-only route
   table, pure read handlers, cache headers (§3.4).
5. The pages + stylesheet + reconnect script under the Q7
   2014-browser checklist (§3.5).
6. Lifecycle integration in `boot_and_serve` /
   `drain_and_shutdown` (§3.6).
7. Standalone-mode behaviour: serves identically, honest sync
   section (§3.7).

**Out of scope** (design backlog candidates + rulings; do not
build): any POST/write surface (the §7 constraints only *shape* the
code, they never add arms), TLS/reverse-proxy machinery, SSE or any
push, Basic-auth variant, gzip, fleet overview, dark/ambient theme,
JSON API, fullscreen scripting, token rotation tooling (delete file
+ restart is the documented ceremony), per-board RSS. Domain work
for time tracking: none (§7.3 — verified, no domain surface needed
or pre-built).

## 2. Verified anchors (re-verified at `47398b3`)

| # | Fact | Evidence |
|---|---|---|
| A1 | the shared headless boot is `boot_and_serve` (`main.rs:311`), entered by `daemon_lifecycle` (`main.rs:280`) and `run_kiosk` (`main.rs:421`); it returns `(Arc<App>, IpcServerHandle)` after `ipc::server::spawn_server` (`main.rs:295`) | grep + read of `main.rs:280-460` |
| A2 | the handle shape to mirror: `IpcServerHandle` with `stopped_receiver()` (`ipc/server.rs:88`) and bounded `join(drain_budget)` (`ipc/server.rs:118`); `drain_and_shutdown` (`main.rs:384`) is the single shutdown choreography | grep of `ipc/server.rs` |
| A3 | the read path: `ActorHandle::shared_state() -> Arc<ArcSwap<AppState>>` (`crates/taskboard-state/src/actor.rs:202`); `EngineMetricsSnapshot` via `ActorHandle::metrics()` (`actor.rs:206`, `metrics.rs:19-42`) | grep of state crate |
| A4 | the content sources: `SyncStatus { phase, last_success, pending_ops, counters }` + `SyncCounters` (`crates/taskboard-domain/src/state.rs:49-88`); `SyncPhase::Offline` renders offline from the snapshot | read of `state.rs` |
| A5 | the config precedent: `KioskSection` (`config.rs:370`) — `deny_unknown_fields`, absent table = default behaviour, `skip_serializing_if` for unchanged serialization | read of `config.rs` |
| A6 | workspace pin discipline: only `reqwest` pinned among HTTP crates today (`Cargo.toml:57`); `uuid` with v4 already pinned (`Cargo.toml:66`) — token generation needs no new dependency | grep of root `Cargo.toml` |
| A7 | the hyper server stack is approved (design Q1, gate closed): pin `hyper`/`hyper-util`/`http-body-util` once in `[workspace.dependencies]`; B1–B6 of the design: zero new release-binary crates, pure feature activation on hyper 1.12.0 / hyper-util 0.1.21 / http-body-util 0.1.5 (already client-side via reqwest; server features already compiled in dev/CI lanes via wiremock) | design §2 B1–B6, §5 item 1 |
| A8 | desktop/REPL boots never pass through `boot_and_serve` (design B12; `ui.rs` routing) — placing the spawn there fences them out by construction | design B12 |

## 3. Implementation steps

Each step lands as its own commit (clean subject, stands alone);
tests accompany the code they verify (TDD where a behaviour is
being added).

### 3.1 Workspace pins (the approved stack)

Root `Cargo.toml` `[workspace.dependencies]`, mirroring the
reqwest pin's discipline:

```toml
hyper = { version = "1", default-features = false, features = ["http1", "server", "client"] }
hyper-util = { version = "0.1", default-features = false, features = ["http1", "server", "service", "tokio"] }
http-body-util = "0.1"
```

`taskboard-app` gains the three as normal deps (workspace-inherited
versions). `client` stays on hyper so the shared feature set
matches what reqwest activates (B1/B2 — the union is what compiles;
keeping it explicit documents the overlap). `cargo deny check` must
stay green over the changed tree (unchanged crate set, feature
deltas only); the existing suite is the guard that reqwest's client
path is undisturbed (design Q10 risk 2).

### 3.2 `[web]` config section

New `WebSection` in `crates/taskboard-app/src/config.rs` following
the `KioskSection` precedent (A5): `deny_unknown_fields`, the
absent table = today's behaviour (no server, no port, no task),
`skip_serializing_if` so unset configs serialize unchanged.

Fields (design Q9, verbatim):

```toml
[web]
enabled = true                  # required-true form: the table's presence + enabled spawns
bind = "127.0.0.1:8787"         # default when the section is present
refresh_secs = 30               # default 30, typed floor 5 (Q4)
# token = "…"                   # explicit override; unset = generated (§3.3)
```

Typed decode rules (part of the accepted ruling, each with a unit
test):

- `refresh_secs < 5` → typed config error (self-DoS floor).
- non-loopback `bind` (any address that is not a loopback IP after
  parse) **with no resolvable token** (no `token` field and §3.3
  resolution finds nothing) → typed boot error. Never
  warn-then-serve; the error names the remedy (set a token or bind
  loopback) without asserting on message text in tests (assert the
  typed variant).
- `enabled = false` or absent table → the section decodes and the
  server never spawns (fence test: no listener, no task).
- `enabled = true` with a loopback bind needs no token (loopback is
  the trusted default arm; Q2).

Tests: figment decode matrix (absent / minimal / full / each typed
refusal) — lowest layer only; no duplication through boot.

### 3.3 Token ceremony

Resolution order: explicit `[web] token` (operator override) → a
generated `uuid::v4` persisted in a **0600 file beside the
database** (`<db-path>.web-token`, the K12 key-file ceremony shape;
no new dependency, A6). On generation: create with 0600 (reject a
pre-existing file with looser permissions — typed error, never a
silent chmod), write once, log the full serving URL once at boot
(`tracing::info!`, the URL includes the token by design — accepted
leakage, design Q2).

- Token resolution/generation is a pure-ish unit over a tempdir:
  generate → reload → same token; override wins over file; loose
  permission on an existing file → typed variant; file unwritable →
  typed variant.
- No runtime rotation; the runbook lane documents delete + restart.

### 3.4 Dashboard server module

New `crates/taskboard-app/src/web/` (module tree, not one file):

- `mod.rs` — `spawn_dashboard(app: &App, web: WebSection, token,
  engine: ActorHandle)` → `DashboardServerHandle` mirroring
  `IpcServerHandle` (A2): a `stopped` watch + bounded
  `join(drain_budget)`. Hyper accept loop on one task; each
  connection served by `hyper_util::server::conn::auto` with a
  bounded-threads-if-any posture (at 1–2 pollers, plain
  per-connection spawn under tokio is fine — no new executor).
  Bind failure is a typed boot error (address in use etc.).
- `routes.rs` — **the route table keyed by `(method, path)`**
  (design §7 rule 1, binding): v1 registers GET arms only. Paths:
  `/d/<token>/` (index), `/d/<token>/board/<board-uuid>` (board
  page), `/d/<token>/style-v< N>.css` (stylesheet). Any other path,
  any other method → 404 (uniformly — a wrong token is
  indistinguishable from a nonexistent path; that is the
  capability discipline). A non-GET on a valid path → 405, keeping
  the (method, path) key honest for the future forms lane without
  serving anything.
- `render.rs` — **pure handlers**: `(request metadata, AppState
  snapshot, EngineMetricsSnapshot, render params) → response`
  (design §7 rule 2). No `Arc` escapes into rendering; each request
  does one lock-free `shared_state().load()` (A3) plus one
  `metrics()` read. The server never subscribes to anything — no
  broadcast receiver, no watcher, no wake source (KDF-8/D-K6;
  design Q4 facts).
- `cache.rs` or inline — headers per Q8: dynamic HTML
  `Cache-Control: no-store`; stylesheet
  `Cache-Control: max-age=31536000` with the version in the URL
  (`/style-v1.css`; a redesign ships `/style-v2.css`). No
  ETag/Last-Modified machinery ever.

Unit tests: routing table (path × method → handler/404/405,
including wrong-token-is-404), header attachment, handler purity
(same snapshot in → byte-identical response out; `insta` snapshots
of the rendered HTML per page shape — see §4).

### 3.5 Pages, stylesheet, reconnect script (Q7 checklist binding)

Two HTML generators in `render.rs`, both taking the snapshot:

- **Index**: per-board sync health (phase, `SyncCounters`
  (`cycles/pushes/pulls/failures/offline_episodes/last_cycle`),
  `last_success`, `pending_ops`), engine counters, app version +
  mode, and the sync-backend section typed from the backend
  resolution — standalone shows "standalone" (typed from
  `SyncBackend`, never inferred from empty counters; Q6). Links to
  each board page.
- **Board page**: stacks as columns (CSS `display: table` layout),
  cards as one component: title + due date + done state; no
  descriptions (Q3 ruling: minimal in v1; §7 rule 6 — the card is a
  single render component so widening is additive). Offline phase
  renders from the same snapshot (A4).

The Q7 checklist, each item verifiable in review:

1. HTML5 doctype, `<meta charset="utf-8">`, viewport meta.
2. CSS tables + `inline-block` only; **no CSS Grid**; no flex
   reliance.
3. System font stack; no web/icon fonts; no images.
4. **No JS required** — pages render + meta-refresh with scripting
   disabled; the **single** allowed `<script>` is the §6.4
   reconnect enhancement (design ruling): ES5-only, in-page XHR
   re-fetch of the current URL on the refresh cadence with a
   "reconnecting…" state + retry while the server is unreachable;
   meta-refresh remains the baseline (when scripting is on, the
   script may suppress the meta navigation it replaces — the page
   must still work with the script absent).
5. meta-refresh polling, default 30 s from `[web] refresh_secs`
   (floor 5).
6. Modest CSS: colours, borders, padding, `border-radius`,
   `box-shadow`, media queries (tablet panel + phone fallback);
   no `vh/vw`/`calc` where a percentage suffices.
7. UTF-8 text; no emoji decor; no transforms/animations.
8. Page-weight budget: each board page ≤ ~50 KB at board scale —
   asserted by a test over a constructed large board (a ceiling on
   garbage is not a quality ceiling; this is a binding design
   budget).

The ES5 script is static text in the binary (`include_str!`); it is
not generated, templated, or configurable in v1.

### 3.6 Lifecycle integration

In `boot_and_serve` (A1), immediately after `spawn_server`: if the
`[web]` section is present and enabled → resolve token (§3.3),
decode rules already enforced at config load → `spawn_dashboard`;
the `DashboardServerHandle` threads out beside the
`IpcServerHandle` (return type widens to carry both, or a small
struct — implementer's choice, keep `drain_and_shutdown`'s
signature churn minimal) and its `join` rides the existing bounded
drain with the same shutdown budget. Desktop/REPL are unreachable
by construction (A8). The daemon `status` view **may** surface the
serving URL (design §6.2 item 3 — read-only, no new verb;
optional, drop if it fights the status shape).

Integration tests (hermetic, loopback, ephemeral port — `127.0.0.1:0`):
spawn the real server over a constructed core, then with a plain
hyper/reqwest client: index journey, board journey, wrong-token
404, non-GET 405, stylesheet cached, shutdown drains within budget
(assert terminal state via the handle, never sleeps — AGENTS §5).

### 3.7 Standalone

No gate: a standalone daemon serves identically (Q6). The index's
sync section reads the typed backend (§3.5). One test constructs a
standalone-flavoured snapshot and asserts the section renders from
the backend type (structural assertion, no text matching).

## 4. Testing strategy (mapped to AGENTS §5)

- **Pure render handlers + `insta`**: snapshot the HTML for index
  and board page across constructed snapshots (empty board, board
  with done/overdue cards, offline phase, standalone) — the HTML
  *is* the UI state here, so snapshotting it is the engine-snapshot
  discipline applied to the web lane; no snapshot asserts on
  wall-clock time (inject/freeze the timestamp used for rendering).
- **Routing/auth units**: the `(method, path)` table incl.
  wrong-token-404 uniformity; config decode matrix; token
  ceremony tempdir units.
- **One integration journey**: real loopback server, ephemeral
  port, plain client; hermetic — no external network, no tablet
  emulation in CI.
- **Determinism**: no `sleep`; poll predicates with deadlines or
  join handles; the refresh cadence enters rendering as data, never
  as a timer the tests wait on.
- **The reconnect script**: untestable by Rust CI beyond structure
  — test that it is the only `<script>` element in each page and
  that the pages contain the meta-refresh baseline regardless (the
  no-JS-required guarantee). The hands-on tablet test (Q7 item 9)
  is explicitly the user's; this plan flags it in the PR body and
  the #234 close note.
- **No text assertions**: typed outcomes for all refusals/errors;
  snapshot tests cover presentation.

## 5. Quality gates

The AGENTS §10 sequence in full before handoff
(`fmt`/`clippy -D warnings`/`nextest`/doc tests/`deny`/the
three-command coverage lane + quarantine + `rm`). Specific gates
this chunk adds:

- `cargo deny check` green over the changed tree (feature-only
  delta, A7).
- The existing reqwest-dependent suites green (client path
  undisturbed — the Q10 blast-radius gate).
- Page-weight budget test (§3.5 item 8).
- New `.rs` files in `taskboard-app` carry
  `// SPDX-License-Identifier: AGPL-3.0-only`;
  `#![forbid(unsafe_code)]` untouched (crate-wide already).

## 6. Review focus (stage-3 checklist)

1. Any POST arm, any write path, any state subscription (broadcast
   receiver / watcher) — each is a finding (read-only lane, no
   wake sources).
2. Route table keyed by `(method, path)`; handlers pure
   (§7 rules 1–2 verifiable in the diff).
3. Non-loopback without token actually refuses (typed, tested);
   wrong token indistinguishable from wrong path.
4. Q7 checklist item-by-item against the emitted HTML (grep the
   snapshot output; no grid/flex/JS-beyond-one-script/emoji).
5. Token file 0600 ceremony: no silent chmod, no fallback to
   serving tokenless on LAN.
6. Config: absent table changes nothing (fence test exists);
   `refresh_secs` floor typed.
7. Shutdown: the handle rides the bounded drain; no unbounded
   awaits, no `std::thread` spawns outside the tokio runtime.

## 7. Sequencing & exit criteria

Steps 3.1 → 3.7 in order (each depends on the previous); total
estimate M–L (one branch, one PR). Exit: all gates green, the
hermetic integration journey passes, the PR annotates #234 and
**closes** it (design §8: the issue closes with K11; the hands-on
tablet test is named in the close note as the user's remaining
half), worktree removed per the lifecycle rule.

Handoff (per AGENTS §8): push `feat/kiosk-web-dashboard`, open the
PR, `gh pr checks` green — the user merges. If this docs PR
(`docs/kiosk-dashboard-impl-plan`) is still open when the feature
branch starts, base the feature branch on `main` anyway and note
the doc path — no stacking needed (the doc references resolve once
both merge).

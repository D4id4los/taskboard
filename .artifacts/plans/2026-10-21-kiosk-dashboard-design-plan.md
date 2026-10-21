# [Plan] Web Kiosk Dashboard — Design Round Proposals (for the user's decision session)

- **Date**: 2026-10-21
- **Kind**: design-proposal record for the web kiosk dashboard (#234),
  kiosk implementation round chunk K10 (docs-only; branch
  `design/kiosk-dashboard`). Every question below carries options,
  trade-offs, and **exactly one** recommendation; **nothing here is
  ruled** — this document is the agenda + proposal record for the
  user's decision session. The HTTP-stack recommendation additionally
  carries an explicit **user-approval gate** (§3.1; D-K7, MD-10): no
  HTTP crate or feature enters `Cargo.toml` in this round.
- **Origin**: implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 KDF-12, §3 K10, §4 — normative); framing
  `.artifacts/plans/2026-10-19-kiosk-wave-design-plan.md` (§5.1 the
  question list; §4 D-K7 — stack approval belongs to this round);
  decision briefing
  `.artifacts/plans/2026-10-19-kiosk-wave-decision-briefing-plan.md`
  (§1.7 dependency facts — re-verified below, not trusted; §2 the
  Tier-2 questions); accepted kiosk design
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  (§3 Option C — the dashboard as the tablet-path surface, sequenced
  before Option B by §6 Q7); #234 (the tablet's constraints);
  dispatch plan §1/§11 (MD-10, the evidence & artefact mandate).
- **Status**: **ACCEPTED 2026-10-21** — the user's session ruled
  every question (§5; the Q1 stack-approval gate is **closed** —
  hyper approved). Two addenda landed with the rulings: §6 (the
  tablet deployment path — the session's connectivity question,
  answered) and §7 (designing for a future write surface — the
  session's forms condition). §6 carries exactly one **open
  sub-decision** (the reconnect script) that awaits the user's nod;
  everything else is ruled. K11 (implementation) dispatches from
  this document. Evidence base: the worktree at `origin/main` tip
  `e2bc3f7` (2026-10-21; the kiosk round's chunks 1–9 are merged
  there — the metrics surface of K8 is **code on main**, not a plan).

## 1. Background

#234 wants a Lenovo Yoga Tablet 2 1050F (Atom Z3745, 2 GB RAM,
Android 5.0) as a dedicated desk display. The issue's own assessment
chose the browser path over a native Slint Android port, with two
binding constraints: the Android 5.0 browser is **2014-era**, so the
page must stay deliberately boring CSS, and it needs a **hands-on
test** on the actual tablet (an agent cannot press that screen).
The 2026-10-16 design round (§3 Option C, §6 Q7) sequenced this
dashboard before the attachable-GUI Option B; KDF-12 fixes its lane:
an **opt-in HTTP surface on the daemon lifecycle**, loopback by
default, content read-only (board view + sync health + offline
state), meta-refresh polling with a no-JS baseline, and — the
architectural keystone — the server is *another dumb presentation
client* reading `ArcSwap` state exactly like the UI does (ADR 0015's
rule, held over HTTP). Interactivity stays CLI/ssh by Option A's
design; management is never the dashboard's job.

What changed since the briefing was written: the kiosk round landed.
The appliance lifecycle (`run_kiosk`, ADR 0016), the `[kiosk]`
config section, and the K8 metrics (per-board `SyncCounters` on
`SyncStatus` + the handle-side `EngineMetrics`) are all on `main`
now — so this round's "verified anchors" are shipped code, and the
dashboard's content sources exist today.

## 2. Verified baseline (re-verified 2026-10-21, worktree @ `e2bc3f7`)

The briefing's §1.7 dependency facts were re-derived from scratch
(`cargo tree`, `Cargo.lock`, `Cargo.toml`); every row below names
its command + observed outcome. Where the re-run sharpened the
briefing's claim, the sharpening is stated.

| # | Fact | Evidence (command → observed) |
|---|---|---|
| B1 | hyper **1.12.0** is in the release binary's normal-dep graph, client-side, via reqwest | `cargo tree -i hyper -e normal --workspace` → `hyper ← hyper-rustls 0.27.10 ← reqwest 0.12.28 ← taskboard-sync-nextcloud ← taskboard-app` (plus `hyper-util 0.1.21` the same way) |
| B2 | hyper's release-activated features are `client`, `default`, `http1` — **not** `server` | `cargo tree -e normal,features --workspace -i hyper` → feature set {client, default, http1} |
| B3 | hyper-util's release features lack `server`/`service`; both **are already compiled in the dev/CI graph** via wiremock (as is hyper's `server`) | `cargo tree -e normal,features -i hyper-util` → {client, client-legacy, client-proxy, default, http1, tokio}; `cargo tree -e features -i hyper` (all edges) → one `server` activation, via `wiremock 0.6.5` (dev-dep of taskboard-app + taskboard-sync-nextcloud) |
| B4 | `http-body-util 0.1.5` is already a release-graph crate (hyper's body helpers) | `cargo tree -i http-body-util -e normal --workspace` → via `reqwest 0.12.28` |
| B5 | `httparse 1.10.1` is in the release graph (hyper's parser) | `cargo tree -i httparse -e normal --workspace` → via `hyper`/`hyper-util` |
| B6 | tokio's release features already include everything a server task needs | `cargo tree -e normal,features -i tokio` → {io-util, macros, net, rt-multi-thread, time} ⊆ activated set |
| B7 | **axum 0.8.9 is in `Cargo.lock` but resolves to no package in the workspace graph** — sharper than the briefing: its only lock parent chain is `taskboard-app`'s **off-by-default `tokio-console` feature** (`console-subscriber → console-api → tonic → axum`) | `cargo tree -i axum --workspace` → error "package ID specification `axum` did not match any packages" (same with `--target all`); `awk` over `Cargo.lock` → `axum`'s only dependent is `tonic`, whose feeders (`console-api`, `console-subscriber`, `tonic-prost`) all root at `taskboard-app/Cargo.toml:24,53`'s optional `tokio-console` feature. Note: the AGENTS §10 coverage lane runs `--all-features`, so axum *does* compile there — but never in default or release builds |
| B8 | **tiny_http is absent** from the tree; resolves to 0.12.0 with plain-HTTP defaults | `grep '^name = "tiny_http"' Cargo.lock` → no match; `cargo add tiny_http --dry-run --package taskboard-app` → "Adding tiny_http v0.12.0", features list shows SSL off by default (not needed). crates.io API `dependencies` for 0.12.0: required deps `ascii`, `chunked_transfer`, `httpdate`, `log` — none of `ascii`/`chunked_transfer` are in the lock today (`httpdate` is, as a target-specific hyper dep not compiled on this host; `log` already ships via `tracing`'s `log` compat, `Cargo.toml:97`) |
| B9 | the read path the dashboard would use exists and is exercised on `main` | `crates/taskboard-state/src/actor.rs:212` `pub fn shared_state() -> Arc<ArcSwap<AppState>>`; `run_kiosk` demonstrates the exact pattern (`main.rs:434-441`: `app.engine.shared_state().load().boards…`); the UI holds the same `Arc` (`taskboard-ui-slint/src/adapter.rs:53,406`) |
| B10 | the K8 metrics surface is **merged code** (commit `84dce32`), not in-flight | `crates/taskboard-domain/src/state.rs:49-88`: `SyncStatus { phase, last_success, pending_ops, counters: SyncCounters }`; `SyncCounters { cycles, pushes, pulls, failures, offline_episodes, last_cycle }` with a saturating `fold_into`; `SyncPhase::Offline` renders offline state from the same snapshot; `crates/taskboard-state/src/metrics.rs:19-42`: handle-side `EngineMetrics` atomics + `EngineMetricsSnapshot`, read via `ActorHandle::metrics()` (`actor.rs:206`), already surfaced by `status` (`commands/status.rs:91-108`) |
| B11 | the lifecycle insertion point: `boot_and_serve` (`main.rs:311`) is the one shared headless boot — both `daemon_lifecycle` (`main.rs:280`) and `run_kiosk` (`main.rs:421`) enter it; it already spawns a server-shaped task (`ipc::server::spawn_server` → `IpcServerHandle` with `stopped_receiver()` + bounded `join(drain_budget)`) whose handle rides `drain_and_shutdown`'s bounded drain (`main.rs:384-410`) | read of `main.rs:280-460` + `ipc/server.rs:60,88,118,156` |
| B12 | desktop and REPL boots never pass through `boot_and_serve` | `bare_invocation_route` (`ui.rs:169-178`): Desktop→DesktopUi, Daemon+TTY→Repl, Daemon+display→DesktopUi, Kiosk→KioskAppliance, Daemon→DaemonLifecycle; `run_desktop` (`ui.rs:276`) boots its own core under the instance lock |
| B13 | "standalone" is not a mode — it is the no-connector sync backend (ADR 0010) | `config.rs:167,636-658`: `SyncBackend::Standalone` when no `nextcloud.server_url`; `[app] mode` stays Daemon/Desktop/Kiosk (`config.rs:90-98`) |
| B14 | the opt-in config-section precedent the dashboard follows | `[kiosk] KioskSection { fixed_board: Option<Uuid> }` (`config.rs:365-378`): absent table = default behaviour, `deny_unknown_fields`, `skip_serializing_if` for unchanged serialization |
| B15 | token generation needs **no new dependency**: `uuid` v4 is approved stack, already pinned | root `Cargo.toml:66`: `uuid = { version = "1", features = ["v4", "v7", "serde"] }` |
| B16 | workspace HTTP pins today: only `reqwest` (`Cargo.toml:57`); no hyper/tiny_http/axum direct pin exists — whichever stack wins is pinned once in `[workspace.dependencies]` (AGENTS §4) | grep of root `Cargo.toml` |
| B17 | 2014-browser layout facts: CSS Grid unsupported at Chrome 37 (flag-only; full support from 57); modern flexbox IS supported since Chrome 29 | caniuse.com/css-grid and caniuse.com/flexbox support tables (accessed 2026-10-21) |

Baseline deltas vs the briefing (§1.7): every number re-verified
correct; B7 sharpens "in `Cargo.lock`, no inverse path" into the
exact mechanism (the off-by-default `tokio-console` feature), and
B3/B4 add that the hyper server code path and body helpers are
already exercised by the workspace's own dev/CI lanes.

## 3. Question record (one recommendation each)

### Q1 — HTTP stack (recommendation; **user approval required**)

**What's being chosen**: the crate that parses HTTP and serves the
dashboard page inside the daemon/appliance process.

**Options**:

1. **`tiny_http` 0.12** — the smallest-API candidate. Sync,
   thread-per-connection; ~a screenful of app code for a one-page
   server; fine for 1–2 pollers. Cost (B8): three genuinely new
   crates in the release binary (`tiny_http`, `ascii`,
   `chunked_transfer`; `httpdate` lock-present but host-uncompiled),
   and an upstream with sporadic maintenance cadence for a
   network-facing parser.
2. **hyper server** (promote to direct workspace pins: `hyper`
   `server`+`http1`, `hyper-util` `server`+`service`, plus
   `http-body-util`) — B1–B6: **zero new external crates in the
   release binary**; pure feature activation on hyper 1.12.0 /
   hyper-util 0.1.21 / http-body-util 0.1.5, which already ship
   client-side via reqwest; tokio needs no feature change; the
   server code is already compiled (and battle-exercised) in the
   dev/CI lanes via wiremock; one HTTP implementation in the binary
   instead of two. Cost: hyper 1.x's server API is lower-level than
   a framework — the chunk owns a small connection-accept loop and a
   service fn (bounded extra code, estimate ~100–150 lines + tests);
   enabling the features grows the release binary by server modules
   of a crate already shipping in it, at the same version.
3. **axum 0.8** — routing/extractor ergonomics. Cost (B7): a real
   release-tree addition (`axum`, `axum-core`, `tower`,
   `tower-layer`, `tower-service`, `matchit`, `sync_wrapper`) — the
   framework tax for a page that is explicitly never growing an API
   surface (KDF-12; the briefing's own reasoning). Its lock presence
   is an artefact of the off-by-default `tokio-console` feature, not
   latent availability.
4. **hand-rolled over `TcpStream` + `httparse`** — near-zero tree
   cost in principle… but B5 shows hyper already carries `httparse`
   in the release binary, so versus option 2 this saves **nothing**
   on the tree axis while making us own request-line/header parsing,
   size limits, malformed-input handling, and keep-alive on a
   network surface — dominated.

**Recommendation: option 2 — the hyper server**, pinned once in
`[workspace.dependencies]` (`hyper = "1"`, `hyper-util = "0.1"`,
`http-body-util = "0.1"`, `default-features = false` with explicit
feature lists, matching the reqwest pin's discipline). It is the
only option with a zero-crate delta, it keeps a single HTTP
implementation in the binary, and the code-volume premium over
`tiny_http` is one bounded module. Option 1 remains the fallback if
the user weighs API simplicity above tree discipline.

> **User approval required (D-K7 / MD-10)**: this recommendation is
> *not* a decision. No HTTP crate or feature enters `Cargo.toml` in
> this round. K11 pins the approved stack after the user's explicit
> approval in the decision session — the two steps (recommendation
> here, approval there) are deliberately separate.
>
> **APPROVED 2026-10-21** (§5, item 1): the session approved the
> hyper-server recommendation — D-K7's gate is closed. K11 pins
> `hyper`/`hyper-util`/`http-body-util` once in
> `[workspace.dependencies]`; still nothing enters the tree in
> *this* round.

### Q2 — Bind + auth model (the security decision)

**What's being chosen**: what the socket binds to, and what gates a
non-loopback reader.

**Facts**: the daemon is a long-lived process on LAN kiosks; #234's
content is board titles/tasks (personal, not secret-secret);
framing §8 names this the hard gate before any non-loopback bind;
TLS is ruled out of scope for the daemon itself (briefing §2.1).

**Options**:

1. Loopback-only, always — safest, but the tablet (#234's whole
   point) could not reach it; non-viable alone.
2. Loopback default; LAN bind with **no auth + a loud startup
   warning** — simplest; honest for a trusted home LAN; anyone on
   the LAN reads board titles in cleartext; a misconfigured bind on
   another network is silently exposed.
3. Loopback default; LAN bind requires a **token** — the URL
   becomes a capability: `/d/<token>/…`. Sub-variants for the
   credential check: (a) HTTP Basic auth (implemented by every
   browser of this vintage, but a human-chosen password carries a
   fraction of a generated token's entropy and the auth dialog is
   first-load friction on a wall tablet); (b) **unguessable path segment**
   (capability URL) — 122 bits of `uuid::v4` entropy (B15), zero
   session machinery, zero JS, survives meta-refresh because the
   token rides the URL, one line of config.
4. Assume a reverse proxy provides TLS + auth — zero code; the ops
   burden (and the failure mode) lands on the user's LAN; reasonable
   for enthusiasts, wrong as the product default.

**Recommendation**: loopback bind by default (`127.0.0.1:8787`;
port arbitrary and configurable — chosen only to avoid the clichéd
8080); any non-loopback bind is a **typed boot error unless a token
is resolvable** (never a warning-then-serve); the token is variant
3(b), the unguessable path segment, generated as a `uuid` v4 and
persisted in a 0600 file beside the database (the D-K8/K12
generated-key-file ceremony precedent), with an optional explicit
config override for operators who want a chosen value. The boot log
names the serving URL once. Token-in-URL leakage is bounded and
accepted: it can reach our own logs and the tablet's browser
history — both on the user's own hardware. TLS stays out of scope
for the daemon; variant 4 is documented as the enthusiast path for
anyone who wants it.

### Q3 — Content scope

**What's being chosen**: what the pages show.

**Facts**: all content is one `ArcSwap` load away (B9, B10) —
boards/stacks/cards, per-board `SyncStatus` + counters, offline
phase, engine counters; the metrics doc comments themselves name
"the dashboard" as the reader. KDF-6/Option A: management stays
CLI/ssh; KDF-12: read-only is the lane.

**Options**: (a) one index page only (health summary, board list);
(b) index + one page per board (the kanban as CSS table columns);
(c) any management surface (write/edit) — ruled out by the accepted
design; (d) richer board detail (descriptions, labels, activity) —
page weight and 2014-browser risk grow for little passive-display
value.

**Recommendation**: **(b)** — an index page (per-board sync health:
phase, counters, `last_success`, `pending_ops`, offline episodes;
engine counters; app version + mode) and one page per board (stacks
as columns, cards as title + due date + done state; no
descriptions). Read-only, no forms, no POST handler anywhere — the
server answers GET only, which is also its security posture
(one-directional).

### Q4 — Refresh strategy

**Options**: (a) meta-refresh polling, no JS at all; (b) minimal JS
timer/fetch with a no-JS fallback; (c) SSE/WebSocket push.

**Facts**: (c) is rejected three ways — the 2014 browser, the
complexity, and KDF-8/D-K6's no-new-wake-source ruling (the
dashboard must not become a state subscriber that wakes the
engine; a poll reads the `ArcSwap` on demand, lock-free, and never
wakes anything — B9). (b) buys smoother updates at the cost of a
script lane to keep honest on a browser whose JS engine vintage is
exactly the unknown (§1).

**Recommendation**: **(a)** — `<meta http-equiv="refresh">`
polling, default 30 s, `[web] refresh_secs` configurable with a
typed floor (5 s) against self-DoS; zero `<script>` elements in v1
pages. The refresh interval is honest about being a display
cadence, not a protocol.

### Q5 — Server placement & lifecycle

**Options**: (a) spawn inside `boot_and_serve` beside the IPC
server (B11); (b) a separate `taskboard web` verb process; (c) on
every frontend boot including desktop/REPL.

**Facts**: (a) covers exactly the two headless lifecycles (daemon +
kiosk appliance, B11) with one spawn site; the handle mirrors
`IpcServerHandle`'s shape (a `stopped` signal + bounded `join`)
and rides the same `drain_and_shutdown` drain — shutdown
choreography stays single-copy, the K3 extraction's whole point.
(b) is impossible without redesign: a second core per database is
`instance-locked` (KDF-4); serving state means owning the core.
(c) is wrong twice: the desktop already has a first-class surface
(the window), and `run_desktop`/REPL never enter `boot_and_serve`
(B12) — placing the spawn there makes (c) unreachable by
construction, which is the desired fence.

**Recommendation**: **(a)** — a `DashboardServerHandle` spawned in
`boot_and_serve` immediately after `spawn_server`, only when the
`[web]` section is present and enabled (Q9); its join rides the
existing bounded drain with the same shutdown budget.

### Q6 — Standalone-mode interaction

**Facts**: standalone is the no-connector backend (B13, ADR 0010),
not a mode; a standalone daemon still owns the full core and the
`ArcSwap` — every dashboard content source except sync counters is
connector-independent. KDF-9: unattended standalone kiosks never
touch a credential store — the dashboard must not change that.

**Options**: serve identically with an honest "standalone — no
connector" sync section (recommended) vs refusing to serve when
standalone (adds a gate nobody asked for) vs special-casing
standalone content (a second page shape to maintain).

**Recommendation**: the dashboard **works standalone, identically**
— the index renders boards + engine counters and the sync section
states "standalone" (typed from the backend resolution, not
inferred from empty counters). Corollary worth recording: a
machine running desktop mode cannot serve the dashboard (the
desktop holds the core under the instance lock, B12) — that is
correct; the always-on deployment shapes are precisely the daemon
and the appliance.

### Q7 — 2014-browser compatibility checklist (binding on K11)

The checklist K11 implements against and the hands-on test verifies
(sources: B17 caniuse rows; #234's "deliberately boring CSS"
constraint; the tablet's Android 5.0 vintage):

1. HTML5 doctype, `<meta charset="utf-8">`, viewport meta tag.
2. Layout via CSS tables (`display: table/table-row/table-cell`)
   and `inline-block`; **no CSS Grid** (B17: unsupported at Chrome
   37); flexbox not *relied* upon (B17: supported since Chrome 29,
   but the conservative stack is the house line — columns come from
   tables, not flex).
3. System font stack; no web fonts, no icon fonts; no images beyond
   optional CSS-drawn accents.
4. **Zero JavaScript** (Q4) — the pages render with scripting
   fully disabled; no `<noscript>` content needed because nothing
   degrades.
5. meta-refresh polling only; no `fetch`/XHR/SSE/WebSocket.
6. Modest CSS only: colours, borders, padding, `border-radius`,
   `box-shadow`, media queries for the tablet's panel and a phone
   fallback width; avoid `vh/vw` and `calc` where a plain
   percentage does.
7. UTF-8 text; no emoji decor (2014 Android font coverage is
   spotty); no CSS transforms/animations.
8. Page-weight budget: each board page ≤ ~50 KB HTML at board
   scale (inline or one cached stylesheet — Q8).
9. **Hands-on test on the Yoga Tablet 2 1050F — flagged to the
   user** (the agent cannot press the screen): load index + one
   board page, leave running 24 h, verify refresh continuity, and
   set the device's screen-sleep policy (a system setting, not a
   page's to make).

### Q8 — Caching

**Options**: (a) `Cache-Control: no-store` on dynamic HTML + a
long-cached versioned stylesheet; (b) ETag/Last-Modified 304
machinery on pages; (c) short `max-age` on pages.

**Facts**: pages are small, clients are 1–2, polls are 30 s — 304
negotiation saves nothing measurable and costs validator
bookkeeping; a stale `max-age` on a passive display is exactly the
wrong failure (the wall shows old state after a change). Every
HTTP/1.1 browser implements `Cache-Control`; the risk direction on
an old tablet browser is over-caching when not told, which
`no-store` addresses directly.

**Recommendation**: **(a)** — dynamic pages: `Cache-Control:
no-store` (correctness over LAN chattiness); the single stylesheet:
`Cache-Control: max-age` long, its URL carrying a content version
(`/style-v1.css`), so a redesign ships as `/style-v2.css` and no
validation machinery ever exists.

### Q9 — Config-section sketch (K11's starting shape)

Follows the `KioskSection` precedent (B14): typed,
`deny_unknown_fields`, absent table = today's behaviour (no server,
no port, no task):

```toml
[web]
# Serving is opt-in: the table's presence + enabled = true spawns
# the dashboard server on the daemon/appliance lifecycles.
enabled = true
bind = "127.0.0.1:8787"        # non-loopback REQUIRES a token (Q2)
refresh_secs = 30              # typed floor 5 s (Q4)
# token override; unset = generated uuid v4 persisted in a 0600
# file beside the database (the K12 key-file ceremony shape).
# token = "…"
```

Decoding rules that are part of the recommendation: a non-loopback
`bind` without a resolvable token is a **typed boot error** (never
served, never warned-then-served); `refresh_secs` below the floor
is a typed config error; the section never appears in serialized
configs unless set (B14's `skip_serializing_if` convention).

### Q10 — Risks

- **A network surface on the daemon** (framing §8's named risk):
  contained by loopback default, the token gate, GET-only handlers,
  and the stack decision (Q1) — whichever crate parses HTTP, we do
  not. Tests stay hermetic: handler units over constructed state +
  one loopback-ephemeral-port integration journey; no external
  network in CI.
- **Feature-activation blast radius** (Q1 option 2): enabling
  `server` features must not disturb reqwest's client path — the
  existing suite plus `cargo deny` over the unchanged tree are the
  K11 gate; the dev lanes already compile these features (B3), so
  the coverage lane has exercised them since wiremock landed.
- **2014-browser unknowns**: the checklist (Q7) is conservative by
  construction; the hands-on tablet test is the authoritative
  check and is explicitly the user's.
- **Token exposure**: bounded to our logs + the tablet's history
  (Q2); documented, accepted; rotation = delete the file + reboot
  (documented in the runbook lane, not built).
- **Refresh stampede**: 1–2 clients by design; each poll is one
   lock-free `ArcSwap` load (B9) — the no-new-wake-source ruling
   holds by construction; a pathological client count is a config
   problem (don't share the URL), not a code one.
- **Content correctness**: the page reads a snapshot per request;
   a mid-cycle read shows the last published state — the same
   consistency the UI badge has, no stronger guarantee is claimed.

### Backlog candidates (out of scope; no tracker action here)

Per the evidence & artefact mandate these are recorded for the
user to promote, not filed: SSE/streaming variant (if a browser
surface ever needs push); a JSON read API for scripts (would
reopen the axum question under a real API need — none exists);
multi-kiosk fleet overview page; dark/ambient styling for OLED
wall displays; HTTP Basic-auth variant; gzip; per-board RSS/Atom
feed; the E-12 IPC-hardening family revisit alongside the auth
model (framing §2.3); Option B attachable GUI (post-dashboard by
the 2026-10-16 Q7 ruling, its own design round).

## 4. What K11 inherits from acceptance

The accepted rulings of §3 (plus the stack approval) are K11's
normative input: the crate pins (approved §5.1), the
`DashboardServerHandle` spawn contract in `boot_and_serve`, the
GET-only page set (index + per-board), the token/bind decode
rules, the no-JS + checklist constraints, the caching headers, and
the hands-on-test flag to the user — **plus the §6/§7 addenda**
(the tablet deployment story, incl. the open reconnect
sub-decision if ruled in, and the forms-readiness constraints).
K11's stage-1 plan re-locates every symbol at its own start
(MD-8) — the line numbers here are 2026-10-21 references against
`e2bc3f7`.

## 5. Decision record (user session 2026-10-21)

| Item | Ruling |
|---|---|
| Q1 HTTP stack | **Accepted** — the hyper server is approved; the D-K7/MD-10 stack gate is closed. K11 pins `hyper`/`hyper-util`/`http-body-util` once in `[workspace.dependencies]` |
| Q2 bind + auth | **Accepted in shape**; the session asked how the tablet topology works in practice — the §6 addendum is the answer (deployment, connection, and permission management); the loopback default, the token capability-URL, and the typed non-loopback gate stand as designed |
| Q3 content scope | **Accepted as the first implementation, with a binding design constraint**: a future write surface (forms — start/stop time tracking on tasks, marking tasks done, "or some such") must remain possible without rework — the §7 addendum records the constraints that keep that door open |
| Q4 refresh | **Accepted** (meta-refresh, no JS). §6.4 derives one scoped amendment candidate (the reconnect script) from the tablet-path analysis — an **open sub-decision**, not folded in |
| Q5 placement | Accepted |
| Q6 standalone | Accepted |
| Q7 browser checklist | Accepted |
| Q8 caching | Accepted |
| Q9 config sketch | Accepted |
| card detail | **Minimal in v1 by ruling** — the user widens detail/interactivity only after on-tablet practice; the per-board page keeps card rendering shaped so widening is additive (§7 rule 6) |

## 6. Addendum — the tablet as kiosk display: connection & permission (2026-10-21)

The session's question: the webserver runs on a *different device*
than the tablet — how do connection and permission work? The
design already assumes exactly that split; this section makes the
operational story explicit.

**6.1 Topology.** The daemon is the server: whichever always-on LAN
device runs `taskboard` as the daemon (or the kiosk appliance —
both lifecycles serve, Q5) hosts the dashboard. The tablet is a
*pure browser client* and runs nothing of ours — no app, no
account, no agent. The tablet is precisely #234's reason for the
LAN arm of Q2.

**6.2 Connection management.**

- `[web] bind` points at the LAN (the non-loopback arm; the token
  gate applies — Q2/Q9). The serving URL is
  `http://<server>:8787/d/<token>/`.
- **Address stability**: a DHCP reservation (static lease) for the
  server device, or a name the LAN's router resolves. mDNS
  (`.local`) resolution in Android 5's stock browser is not
  reliably available — the URL should carry the reserved IP or the
  router-level name; the hands-on test verifies what the tablet's
  browser actually resolves.
- Host firewall: port 8787 open for the LAN scope, if the server
  device runs one.
- **Setup ceremony** (once per install): (1) enable `[web]` with
  the LAN bind; (2) on first boot the token file is generated and
  the full URL logged; (3) the operator reads the URL — boot log
  over ssh/journal, and K11 may surface the serving URL in the
  daemon's existing `status` view (read-only, no new verb); (4)
  enter the URL once on the tablet and save it as a bookmark /
  home-screen shortcut. From there the page is self-sustaining
  (meta-refresh) and restarts of the *server* don't change the
  URL (the token persists in its file).

**6.3 Permission management.**

- **The URL is the credential** — 122 bits of token (Q2). No
  tablet-side permission, no login, no secret stored on the
  tablet beyond the bookmark itself. Anyone on the LAN holding
  the URL can read, in cleartext — the accepted Q2 posture; the
  reverse-proxy variant (Q2 option 4) remains the documented path
  for anyone who wants TLS.
- **Revoke/rotate**: delete the token file and restart — the old
  URL dies, the new one is logged, re-entered on the tablet once.
  Rotation is an operator action, never automatic.
- The tablet's own exposure is a bookmark; a wiped or shared
  tablet leaks nothing but a dead-or-live URL.

**6.4 Outage behavior — and the one open sub-decision.** With pure
meta-refresh (Q4 as accepted), a refresh navigation that *fails*
(server mid-restart, Wi-Fi blip) replaces the document with the
browser's error page — and an error page carries no refresh
instruction, so an unattended tablet stays on it until a human
reloads. Server-side outages are short by design (`Restart=always`
supervision, sub-second restarts), but the failure window is
real. Options: (a) accept for v1 exactly as accepted and learn the
real-world frequency during the on-tablet tests; (b) a ~15-line
ES5 progressive-enhancement script that, when scripting is
available (Chrome 37 supports everything needed: XHR, ES5 DOM),
replaces navigation-refresh with in-page re-fetching plus a
"reconnecting…" state and automatic retry — the no-JS meta-refresh
baseline remains for scripting-disabled browsers, so Q7's
no-JS-required guarantee is unchanged, but Q4's "zero `<script>`
elements" line would soften to "no JS *required*". This amends an
accepted item, so it is **not** folded in: **recommendation (b),
folded into K11's scope — awaiting the user's explicit ruling.**

Also recorded honestly: Android 5 offers no permanent page
fullscreen without scripting; the tablet display will show the
browser's URL bar. The hands-on test evaluates whether that
matters on a desk tablet (the §7 write-surface work, if it ever
lands, would be the natural moment to revisit).

## 7. Addendum — designing for a future write surface (2026-10-21)

The session accepted the read-only scope *as the first
implementation* on the explicit condition that forms remain a
future possibility (start/stop time tracking, marking tasks
done). The constraints that keep that door open — binding on K11,
each verifiable in review:

1. **The route table is keyed by (method, path)**; v1 registers
   GET arms only. POST arms are additive later — no handler
   re-architecture, no routing rewrite.
2. **Read handlers stay pure** — (request, `AppState` snapshot) →
   response. The future write handlers translate form fields into
   the **existing typed `StateCommand` vocabulary**
   (`crates/taskboard-domain/src/messages.rs:20`) and send through
   the same engine command channel the CLI and the desktop UI
   use; typed refusals render as page states. No parallel
   mutation path is ever built — ADR 0015's dumb-client rule is
   extended to an input-capable client, not amended away.
3. **What already exists**: `SetTaskDone` ("complete or reopen"),
   `MoveTask`, `UpdateTask`, and the stack/label arms — the
   "marking tasks done" example needs **zero** domain work. Time
   tracking (the session's other example) has **no domain surface
   today** (verified 2026-10-21: no timer/tracking vocabulary in
   `taskboard-domain`) — it would be a domain feature first and a
   dashboard form second; this addendum constrains the *web
   layer* only, it does not pre-build domain arms.
4. **Write-surface security, recorded now so it is not
   rediscovered**: plain HTML forms can cross-origin POST without
   CORS, so CSRF is live for any write arm — the capability token
   doubles as the CSRF secret (a cross-site attacker does not
   hold the URL's token). That is adequate for the LAN threat
   model as accepted, but a write surface deliberately **reopens
   Q2's threat model** with its own small design pass before any
   POST arm is implemented.
5. **Forms do not conflict with the 2014-browser stance**: plain
   HTML forms predate CSS and need no scripting; the
   POST-redirect-get pattern keeps meta-refresh semantics intact.
   The no-JS guarantee (Q7) survives a write surface.
6. **Card detail stays minimal in v1** (ruled) with widening held
   until on-tablet practice says what detail/interactivity level
   suits the endpoint — the per-board page renders cards as one
   component so widening (labels, due, description) is additive.

## 8. Tracker action on landing

Per the dispatch plan's bookkeeping: landing this design
**annotates #234** (stage 3, MD-4; one comment, no Closes line —
"design round landed and accepted 2026-10-21; K11 dispatches from
it, one open sub-decision recorded in §6.4"). The issue closes
with K11.

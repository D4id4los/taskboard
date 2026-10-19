# [Plan] Kiosk Wave — Decision-Session Briefing (every open question)

- **Date**: 2026-10-19
- **Kind**: decision-support briefing for the kiosk-wave framing's
  ruling session. It expands every open question the wave carries —
  the framing's §4 record (D-K1…D-K10), the §5 design-round
  questions, the spike-ruled items, and the stage-1 decisions
  (listed for visibility only) — into background, options with
  trade-offs, one recommendation each, and what each alternative
  costs. It decides nothing; §4 of the framing plan remains the
  record rulings land in.
- **Origin**: user request 2026-10-19 ("detailed breakdown of all
  open questions with enough information to make an informed
  decision"); the framing plan, the implementation plan, and the
  parallelism report (same PR); dependency-graph facts verified
  2026-10-19 via `Cargo.toml`/`Cargo.lock`/`cargo tree` (§1.7/§1.8).
- **Status**: complete. Tier 1 is the session's agenda; the
  **minimal ruling set** is §4.

## 1. Tier 1 — wave-level rulings (framing §4, D-K1…D-K10)

### 1.1 D-K1 — Milestone shape: three phases in one milestone

- **What you're choosing**: accept the phase structure — (A)
  appliance + instrumentation + deployment profile + dashboard
  design round, dispatchable immediately; (B) your on-device spike;
  (C) dashboard impl + file store + ambient view, gated — as gates
  inside the one `wave-kiosk` milestone, or restructure.
- **Facts**: the backlog already speaks of "kiosk wave 1/wave 2"
  (Q7's ruling sequenced the dashboard before the attachable GUI);
  the milestone was created at the audit's tracker reorg with all
  ten issues; the daily-driver precedent runs phase-like gates
  inside one milestone (its MD-1 round gate) rather than splitting
  milestones; effort: A ≈ 3–3.5 M-equivalents, C ≈ 2.5, total L
  (~5–8 weeks calendar).
- **Options**: (a) one milestone, phase gates as the implementation
  plan §5 states (recommended — one closure review, one exit
  report, gates do the sequencing work); (b) split into
  `wave-kiosk-1`/`wave-kiosk-2` milestones (finer "when" signaling
  on the tracker; costs a second milestone to maintain and blurs
  that the spike feeds both halves); (c) no phases, dispatch as
  dependencies allow (risks dispatching the ambient view into the
  daily-driver UI-file collision the parallelism report §2.2
  names).
- **Embedded sub-choice**: K12 (file store) sits in phase C only
  because its D-K8 ruling is open; `secrets.rs` is untouched by
  both other rounds, so once D-K8 stands it may dispatch in phase A
  (recommended if you want it sooner).

### 1.2 D-K2 — Create the ambient-view + rotation issue

- **What you're choosing**: whether the milestone's passive-display
  deliverable gets a tracker home.
- **Facts**: it is the **only** substantive kiosk work item with no
  issue: future-directions §3.7 sizes it S–M ("display-mode state
  machine + rotation policy on the kiosk surface"); the
  views-finding design rules the kiosk surface is this wave's to
  own (D-V3) and builds today-view rotation-ready (D-V1);
  multi-board's design deferred "config-pinned fixed-board kiosk"
  here; the D250 exists as a **pure wall display** — without this
  item the appliance would show the interactive desktop UI
  fullscreen, which is not the passive product the hardware is
  for. Scope fence in the drafted body (framing §9): board-pinned
  kanban ⇄ today/upcoming rotation; **no** multi-board rotation
  ring (backlog); no capture/edit surface on kiosk.
- **Options**: create + assign now (recommended — chunks K13 and
  the K9 config keys reference it); create after the spike (loses
  the issue-number anchor in two chunks' bookkeeping); don't create
  (the milestone then has no passive-surface exit criterion and the
  D250's purpose is unmet by this wave).

### 1.3 D-K3 — Assign #159 (file store) and #145 (engine counters)

- **Facts**: #126's own status note says its remainder **is** #159's
  scope — #126 can never close without it; #145 is the dashboard's
  second content feed (#234's body says "reusing the already-
  tracked sync-actor/engine metrics"), and D-K6 gives it a
  churn-free shape; both are small.
- **Options**: assign both (recommended — closes the loop on #126
  and gives the dashboard its full content); assign only #159
  (dashboard shows sync-side metrics only — acceptable, thinner);
  assign neither (#126 stays permanently open; dashboard rides
  #156 alone).

### 1.4 D-K4 — Close #123 (daemon IPC for remote control)

- **Facts**: its body's status note reads "IMPLEMENTED" (WI-5,
  ADR 0012 — socket, argv relay, `status`/`daemon stop`, systemd
  template); the follow-ups it names are separately tracked
  (#193 keep-alive/event streams, #194 disable knob, #195
  multi-user, #196 read deadlines); house rule: closing is the
  archive — never delete, and the kiosk wave adds no IPC verb
  (KDF-6), so nothing will ever "finish" it.
- **Options**: close as completed with a pointer comment
  (recommended); leave open as an umbrella (a permanently-open
  implemented issue duplicates its children's tracking).

### 1.5 D-K5 — #160 `sd_notify` readiness

- **Facts**: the template shipped (ADR 0012, `Restart=on-failure`
  user unit); the open remainder is `Type=notify` readiness only,
  gated by the issue's own condition — "only if start-up ordering
  actually needs it"; the kiosk unit (K9) is `Type=simple` +
  `Restart=always`; nothing in the wave needs start-up ordering
  today (no dependent units exist). Implementation cost is tiny
  either way — std-only `NOTIFY_SOCKET` datagram (~20 lines) or the
  `sd-notify` crate — so the question is need, not cost.
- **Options**: keep open, decide at the spike with ordering
  evidence (recommended — matches the issue's condition; the
  decision lands as an issue comment then); close now as
  wontfix-needed (risks reopening if the deployment grows a
  dependent unit); implement preemptively (spends a chunk on
  unneeded readiness plumbing).

### 1.6 D-K6 — Metrics shape (#156 sync, #145 engine)

- **What you're choosing**: where the numbers live so every
  consumer (UI badge, CLI `status`, the coming dashboard) reads
  them without new machinery.
- **Facts**: sync status has been state-borne since multi-board
  chunk 1 — per-board `SyncStates` + an `aggregate()` fold keeps
  all existing surfaces shape-stable; `SyncReport` ingestion
  already republishes `AppState` each cycle; broadcasts are lossy
  for late/reconnecting subscribers; a `metrics`-crate exporter has
  zero consumers today.
- **Options for #156**: (a) **state-borne counters on the per-board
  status** (recommended) — lock-free reads by every existing
  consumer, rides existing publish points, no new wake source;
  costs one deliberate insta pass (K8 owns it) and
  cumulative-since-boot semantics (documented, restart-safe from
  zero); (b) `SystemEvent`-borne metrics — no state growth, but
  lossy: a dashboard must hold a live subscriber, reconnects leave
  gaps, restart wipes everything; (c) a side channel beside the
  actor — second plumbing path for the same facts, no reader
  exists; (d) metrics-crate + exporter — new dependency and
  surface for nobody yet.
- **Options for #145**: handle-side atomic `EngineMetrics` struct
  (recommended — counters change per command, so `AppState` would
  churn snapshots and the ArcSwap publish per command; a handle
  read costs nothing) vs state-borne (snapshot churn) vs event
  (lossy at command frequency).

### 1.7 D-K7 — Dashboard HTTP stack (pre-rulable; default: the K10 round rules)

- **Facts** (dependency graph verified 2026-10-19): **hyper 1.12 is
  already in the release binary's graph** (via reqwest/rustls —
  client side; enabling its server features on a direct dep adds
  no new external crate); **axum 0.8.9 is in `Cargo.lock` but has
  no inverse path in the workspace build graph** — it is not in
  the release binary today, so adopting it is a real tree
  addition (tower + server machinery); **tiny_http is absent**
  (one small new crate); `httparse` is present transitively. The
  page is deliberately boring, read-only, 1–2 LAN clients,
  meta-refresh polling; interactivity belongs to CLI/ssh by
  Option A's design.
- **Options**: (i) `tiny_http` — smallest tree and simplest code,
  sync/thread model, fine for a handful of pollers; (ii) **hyper
  server** (promote to direct, enable server features) — zero new
  external crates, more hand-written service code; (iii) axum —
  ergonomic routing, but the biggest release-tree addition for a
  one-page server, justified only if the dashboard is expected to
  grow an API surface (it is explicitly not); (iv) hand-rolled
  over `TcpStream`+`httparse` — near-zero tree cost, but we own
  HTTP parsing (a security surface) to save one crate.
- **Recommendation**: let K10 carry the candidates through a
  working-tree audit and one recommendation (the house pattern —
  the notify-rust approval came exactly that way); if you want to
  pre-rule, the boring-page analysis favors (i) or (ii). Whatever
  lands: pinned once in `[workspace.dependencies]`, `cargo deny`
  clean.

### 1.8 D-K8 — File-store key management (pre-rulable; default: stage-1 rules within the variant)

- **Facts**: no crypto crate exists anywhere in the tree today
  (verified); `getrandom`/`rand` are present transitively; the
  `CredentialStore` port + config selector exist; the kiosk
  hardware boots unattended (no passphrase typing at 05:00 on a
  wall display); #159's complaint about the env store is
  `ps -E`/same-user visibility; the keyring remains the strongest
  store where a Secret Service exists.
- **Threat-model honesty**: a 0600 key file beside the ciphertext
  on the same disk protects against other local users, accidental
  world-readable copies, and env-var visibility — **not** against
  root or full-disk theft. It is strictly better than plaintext or
  env-in-unit-file, strictly available where the keyring isn't,
  and weaker than the keyring where it is.
- **Options**: (a) **generated 0600 key file + AEAD envelope**
  (recommended default) — boots unattended; needs one RustCrypto
  AEAD crate (`aes-gcm` or `chacha20poly1305`: pure Rust, no
  system deps, deny-friendly) + a direct `getrandom`; (b)
  passphrase + argon2 KDF — interactive unlock, which **fails the
  unattended-kiosk requirement** unless the key is cached
  (reintroducing (a)); right shape for a headful desktop user who
  refuses both keyring and env; needs argon2 **and** an AEAD; (c)
  (a) now, (b) as an additive port impl later if the desktop need
  appears (recommended sequencing); (d) `age` envelope —
  well-audited and CLI-interopable, but a larger tree (x25519/
  bech32/HKDF) for a symmetric local secret.
- **The zero-code alternative, named honestly**: do nothing and
  carry `TASKBOARD_APP_PASSWORD` in the kiosk systemd unit —
  zero implementation, and exactly the same-user visibility #159
  rejects; viable if you accept that exposure on your own LAN
  kiosks. If so, K12 defers and #126/#159 stay open (or close
  wontfix) — a legitimate scope decision, yours to make.

### 1.9 D-K9 — Default `[app] mode` (Q6, deferred 2026-10-16)

- **Facts**: deferred then because "it needs the kiosk on-device
  experience first"; since then the #236 display-aware launch
  fixed the actual pain (double-click → silent headless daemon):
  today a display-present no-TTY launch opens the GUI under the
  daemon default, shrinking the case for any change.
- **Eventual options** (re-surfaced at spike exit with evidence):
  keep `daemon` default (status quo; systemd/REPL/ssh semantics
  untouched — and `ssh -X` sessions carry `DISPLAY`, where a
  desktop default would try to open a window through the
  forwarded X: a concrete argument for daemon); desktop default
  for bare interactive launches; or the current world — daemon
  default + display-aware routing + the explicit `gui` verb.
- **Recommendation**: keep the deferral; expect "keep daemon +
  current routing" to be the ruling.

### 1.10 D-K10 — Parallel dispatch posture

- **Facts**: the parallelism report's verdict — the original
  "minimal overlap" advice holds for the appliance (file-disjoint
  from every daily-driver chunk); two qualifications (ambient view
  is post-DD-10 **by dependency**; sync metrics needs a dispatch
  window around `sync/actor.rs`), two rebase-scale multi-board
  adjacencies (`cli.rs`, `config.rs`), docs-exit chunks must
  serialize, and worktree disk floors argue for ≤2 concurrent
  implementation chunks per host.
- **Options**: accept the posture + rules (recommended — phase A
  starts now, overlapping the multi-board round); serialize the
  waves (weeks slower, buys nothing the rules don't already buy);
  parallel without the rules (one predictable mid-file conflict on
  the programme's most-contested file plus disk-floor risk).

## 2. Tier 2 — design-round questions (K10 dashboard; K12 stage-1)

Ruled at the round's acceptance (or pre-ruled now if you already
know your answer):

1. **Bind + auth — the security decision** (K10's centre):
   default loopback-only (safe); any LAN bind opt-in. For the LAN
   arm: (i) no auth + a loud startup warning and documented
   firewall stance — simplest, honest for a home/office LAN with
   boring content, but anyone on the LAN reads your board titles
   in cleartext; (ii) an unguessable path segment
   (`/d/<generated-token>/…`) from a 0600 config/file — small step
   up, no session machinery, leaks only into our own logs;
   (iii) assume a reverse proxy provides TLS+auth — zero code,
   ops burden lands on you. Recommendation: loopback default +
   (ii) for LAN, refined by the round; TLS is out of scope for the
   daemon itself.
2. **Content scope**: read-only board view + sync health (K8) +
   offline state (recommended) vs any management surface (write)
   — management stays CLI/ssh by design.
3. **Refresh strategy**: meta-refresh polling every 30–60 s
   (configurable) with a no-JS baseline (recommended) vs
   SSE/WebSocket (rejected: 2014 browser + complexity + polling
   suffices for a passive display).
4. **Placement/lifecycle**: app-layer task on the daemon and
   appliance lifecycles behind an opt-in config section
   (recommended); not on desktop/REPL boots.
5. **Standalone mode**: the dashboard works standalone (it reads
   local state) — recommended yes; it is the standalone kiosk's
   own monitoring surface.
6. **Browser compat**: Android 5 / Chrome 37-era constraints —
   CSS-tables/inline-block layout, no flex/grid reliance, no JS
   required; a hands-on tablet test is flagged to you (the agent
   cannot press that tablet's screen).
7. **HTTP stack**: §1.7.
8. **K12 stage-1**: envelope format (versioned AEAD blob vs
   `age`), file locations (key + ciphertext beside the db, 0600),
   config key naming, no key rotation in scope.

## 3. Tier 3 — spike-ruled (recorded with evidence, not pre-decided)

1. **Renderer/backend path** (ADR 0015's deferral): the candidates
   — X11 + femtovg-on-llvmpipe (zero build changes; llvmpipe
   performance unknown on in-order cores), slint
   `renderer-software` feature (build-feature flip + K9's config/
   env), or `backend-fb`/`backend-linuxkms` on a no-X minimal
   image (biggest deployment change, smallest footprint). K9
   ships the config/env lane; the spike measures and picks.
2. **`sd_notify`** (D-K5's evidence): does any dependent unit need
   start-up ordering on the real deployment?
3. **Q6 input** (D-K9): the on-device launch experience.
4. **i686 fallback**: only if 64-bit BIOS boot fails on either
   unit (both CPUs are Intel-64 capable; verify during bring-up).
5. **Budget verdicts**: RSS ≤ 200 MB, repaint ≤ 50/20 ms, cold
   start ≤ 5 s, sync-CPU share — measured on the NF310/D250
   against the audit envelope; a failed budget is a **conscious
   decision point** (remediate, re-budget with evidence, or accept
   degraded), never a silent pass.

## 4. Minimal ruling set

Strictly required to dispatch everything dispatchable: **D-K2**
(the K13/K9 bookkeeping anchors on the new issue), **D-K3** (the
#145 half sizes K8), and **D-K10** (the posture). Every other item
carries a recorded default that binds only at its chunk's
dispatch and can be overturned until then: D-K1 (structure as
planned), D-K4 (close #123 at bookkeeping time), D-K5 (keep open,
decide at spike), D-K6 (state-borne + handle-side), D-K7 (K10
rules; pre-rule optional), D-K8 (key-file default; zero-code
deferral is the honest alternative), D-K9 (defer to spike exit).

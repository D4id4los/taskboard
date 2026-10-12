# [Plan] Connector Attach & Session-Level Login — Design Round (WI-J)

- **Date**: 2026-10-12
- **Kind**: design round (stage 1 of WI-J) — **no production code in this
  commit**. Produces the design + draft ADR the user reviews before any
  implementation is planned. Stages 2–3 of WI-J (formalize ADR 0014 +
  docs amendments, then review) are unblocked by the approved direction.
- **Origin**: backlog entries "[2026-10-11] CLI commands to add a
  Nextcloud connection" and "[2026-10-11] `login` inside a running
  session (REPL / GUI)"; dispatch plan §4 WI-J
  (`.artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md`).
- **Target**: this document only. Future implementation targets (§11):
  `taskboard-state`, `taskboard-app`; Phase 7 UI consumes the surface
  defined in §6.
- **Branch / worktree**: `docs/connect-flow-design` in
  `../taskboard-WT-docs-connect-flow-design`, off `main` @ `1b9a65c`.
- **Goal**: one decision-ready design covering (1) connector-management
  CLI verbs that respect ADR 0010's loudness rule, (2) the session-level
  attach flow onto a booted core — the "attachable `sync:
  Option<SyncActor>`" and "sync-channel rewiring" candidates compared,
  (3) the REPL masked-secret prompt interaction, and (4) the Phase 7
  GUI's reuse of the same flow; plus a draft ADR (§9) and explicit open
  decisions (§10) for the user to pick at plan review.
- **Status**: DIRECTION APPROVED 2026-10-12 (user review, round 2);
  round 3 (same day) folds in **WI-H's merge to `main`** (PR #48,
  `bdd9d97` — `logout` detaches a running daemon's connector): §1's
  fact base updated to the merged reality, the attach design (§4.2)
  recomposed on top of WI-H's primitives instead of the pre-merge
  assumptions, D5 rewritten (the IPC-verb pattern now exists; only
  daemon-side *attach* remains future work), and — per WI-H plan
  decision D7's explicit handoff ("session-level login **and logout**
  … belongs to WI-J's design round") — the matching **in-session
  logout** ceremony added (§4.6; both ADR 0011 session refusals, not
  just `login`, now die). Round-2 mandate unchanged and still
  structural: **multi-connector readiness** — v1 builds exactly one
  connector, but the shape chosen here (§4.2, §4.5) must not need a
  second major change when the multi-connector round arrives.
- **Based on**: `main` @ `bdd9d97` (post-WI-H; rebased).

---

## 1. Context & verified facts (anchored, do not re-derive)

All line numbers verified on `main` @ `bdd9d97` (round 3 re-verified
post-WI-H; round-1/2 anchors that WI-H did not touch are unchanged).

1. **`login` already works on a fresh *auto*-rule install.** The
   backlog's framing ("a stand-alone install has no CLI path … without
   hand-editing `taskboard.toml`") is only true for *explicit*
   standalone. `login_command`
   (`crates/taskboard-app/src/commands/mod.rs:654-682`) refuses only
   when `raw.sync.backend == Some(SyncBackend::Standalone)`; under the
   auto rule (no `backend` key, no `server_url`) it prompts for the
   server, verifies, writes `server_url`/`username` — after which auto
   resolves `nextcloud`. The **real** one-shot gaps are: (a)
   explicit-`standalone` installs have no verb that flips the backend;
   (b) the operation has no connector-shaped name; (c) nothing works
   against a *running* core.
2. **The login ceremony is already a frontend-agnostic seam.**
   `login::run(config_path, raw, server, user, &mut dyn PromptReader,
   &store)` (`crates/taskboard-app/src/commands/login.rs:92-149`):
   gather identity → verify via `DeckClient::boards()` → persist config
   overlay (`toml_edit`, atomic rename; `write_config_overlay`,
   login.rs:170-194) → store the secret. Nothing persists unless the
   server verified the credentials. `PromptReader` (login.rs:23-39) is
   the injected input seam; `StdinPrompts` (login.rs:53-81) is the
   terminal impl — masked `rpassword` with `*` feedback on a TTY, a
   plain line on pipes.
3. **`App`'s connector slot is interior-mutable since WI-H.** What
   round 1 pinned as `sync: Option<SyncActor>` (private, committed at
   construction) is now `sync: std::sync::Mutex<Option<SyncActor>>`
   (`crates/taskboard-app/src/bootstrap.rs`, PR #48) — lifecycle
   bookkeeping inside `App`, the mutex never held across `.await`
   (AGENTS §3 note, WI-H plan D-"AGENTS §3"). Under `standalone`
   bootstrap still *drops* the sync-command receiver and the
   sync-report sender immediately (ADR 0010) — "the engine sees the
   closed-channel world it already handles". The engine received its
   halves — the command *sender* clone and the report *receiver* — as
   spawn arguments; WI-H changed nothing there.
4. **The engine's nudge semantics key on channel liveness.**
   `forward_sync_now` (`crates/taskboard-state/src/actor.rs:480-496`)
   `try_send`s `SyncNow`: `Ok` **and `Full`** count as forwarded →
   `mark_syncing()` (the memory-only `Syncing` transient); only
   `Closed` means "no sync actor" → no transient. **Consequence: a
   merely *parked* (alive, undrained) command receiver makes every
   standalone mutation mark `Syncing` forever** — no report ever
   settles it, and the queue fills (32) after which `Full` *still*
   counts as forwarded. This single fact decides §4's option
   comparison.
5. **A freshly spawned sync actor self-configures its pull target.**
   The cycle resolves its target as "the commanded one, else the
   persisted board binding" via `SyncStateReader` (`crates/
   taskboard-sync-nextcloud/src/actor.rs:298-324`), and the first cycle
   fires immediately (`deadline = now`, actor.rs:179-180). An unbound
   db (standalone install, local-only board) skips silently (decision
   10). **Attach therefore needs no `SetBoard` ceremony when a binding
   is persisted — spawn is attach.**
6. **The engine loop can swap its sync halves in-loop.** The select
   loop holds `sync_out` and the report receiver as loop-locals
   re-borrowed each iteration (taskboard-state/src/actor.rs:279-345),
   with a `reports_open` flag maintained from `Closed` arms. Rebinding
   both in a new `EngineCommand` arm (and resetting `reports_open`) is
   mechanically direct; `EngineCommand` is the engine crate's
   channel-bearing envelope type (ADR 0004 payload/envelope split;
   actor.rs:41-67).
7. **The engine handle already wraps request-response over the
   command inbox.** `Execute`/`Flush`/`ReadState` carry oneshot
   replies (fact 6); an attach ack rides the same pattern.
8. **REPL refusals are parse/route-level, and the session owns `App`
   by value.** `resolve_words` refuses `login`/`logout`/`config`/
   `daemon` + per-line `--config` (`crates/taskboard-app/src/repl.rs:
   269-292`, `LineRefusal`); `repl_loop(app: crate::App, …)` owns the
   core and hands `&App` to `dispatch_line` (repl.rs:306-357). The
   editor thread owns rustyline and **adds every non-empty line to
   history** (repl.rs:129-131) — a secret elicited mid-ceremony must
   never take that path. The `CoreSession` trait
   (`{session_config, session_store}`, commands/mod.rs:765-773) is the
   shared seam `dispatch_line` routes over; `ReplSession`
   (repl.rs:162-170) is a plain struct the loop owns and can mutate
   *between* lines.
9. **Connector verbs gate on the session config, not live state.**
   `sync_line`/`boards` preludes call `require_connector(
   session.session_config())` (commands/mod.rs:45-50, 807+); after an
   in-session connect the session's config view must be refreshed or
   every connector verb keeps refusing `sync-disabled`.
10. **IPC lanes are a pure exhaustive table.** `route_for_ipc`
    (commands/mod.rs:1091-1106): `login`/`logout`/`config`/`boards
    list` are `Local`; core verbs `IpcOrLocal`; `status`/`daemon`
    `IpcOnly`. Every future verb must pick a lane (ADR 0012).
11. **`logout` is the teardown mirror.** `logout::run`
    (crates/taskboard-app/src/commands/logout.rs:36-77): requires the
    configured identity (`NotConfigured`), deletes the secret
    (`NotFound` idempotent), strips the config keys **iff** the
    backend auto-resolved — an explicit `backend = "nextcloud"` is
    never rewritten (the loud choice stays loud).
12. **WI-H merged (PR #48, `bdd9d97`): the detach half of the
    connector-liveness story is now production code.** Option (b)
    landed: `SyncCommand::Detach` (a domain inbox command — the actor
    finishes any in-flight cycle, then returns, dropping its client
    and reports sender; never the `Shutdown` broadcast),
    `App::detach_connector(&self) -> DetachOutcome::{Detached,
    NoConnector, TimedOut}` (takes the actor out of the mutex, sends
    `Detach`, joins bounded by `shutdown_timeout_secs`; idempotent),
    `App::connector_attached(&self) -> bool`, the typed
    `CommandError::SyncUnavailable` ("sync-unavailable") refusal when
    the engine's `RequestSync` answers `NoOp` on a dead channel, the
    one-shot `logout` best-effort daemon nudge, and the IPC
    `DetachConnector` verb (`Request{v, server_url, username}` /
    `Response{v, outcome}`; exact-match identity guard
    `identity-mismatch`; `PROTOCOL_VERSION` stays 1 — additive
    variants never bump, recorded policy in the ADR 0012 amendment).
    **WI-H plan D7 hands this round the matching session-level half**:
    "session-level login **and logout** (WI-J's attach flow plus the
    matching detach semantics, REPL and the Phase 7 GUI alike) is
    WI-J's design round — WI-H's `App::detach_connector` is the seam
    WI-J's designer evaluates for reuse."
13. **`App::shutdown` already tolerates connector-count changes.**
    WI-H reshaped the join to take the lifecycle mutex once and match
    on the actor's presence (`Some → bounded join, None → skip`);
    `sync_commands()` answers a fail-closed sender when absent. An
    `App` that gains or loses a connector mid-session shuts down
    correctly with zero further changes to `shutdown`'s *join* logic
    — but see fact 14.
14. **The engine's linger-drain gates on the reports channel
    closing.** The loop returns from drain only when `draining &&
    !reports_open` (taskboard-state/src/actor.rs), and
    `reports_open` flips to `false` only on `recv()` → `Closed` —
    i.e. when *every* reports sender has dropped. Today that is
    guaranteed by construction: the only sender lives in the sync
    actor's task and dies with it *before* the engine join
    (shutdown's actor-join-then-engine-join ordering). Any design
    that parks a reports sender inside `App` must drop it **before
    awaiting the engine join**, or shutdown deadlocks to the
    bounded-abort path. This is the ordering rule §4.2's fan-in
    inbox has to obey. WI-H's detach honors it naturally: the
    detached actor drops its sender inside its own task before the
    bounded join returns.
15. **`Detach` is graceful by construction** (WI-H plan D4): the
    actor's select arm and its post-cycle nudge absorber both answer
    `Detach` by returning *after* any in-flight cycle — a `SyncNow`
    queued before the `Detach` completes and reports first (pinned
    by WI-H's D-series actor journey). The engine is untouched by
    detach: its single sender's next `try_send` meets `Closed` and
    its "continuing without sync" arm takes over — the same
    closed-channel world as standalone boots (fact 4, the mirror
    image).
16. **The daemon-IPC verb pattern for connector-liveness mutations
    is established** (ADR 0012 amendment): additive request/response
    variants carrying the identity they act on, exact-match guard,
    typed kebab-case outcomes, `PROTOCOL_VERSION` unchanged
    ("additive = minor; shape change = bump"). A future daemon-side
    *attach* verb rides this pattern without an envelope redesign —
    and needs **no secret on the wire**: the daemon can re-read its
    config and load the secret from the credential store itself
    (exactly what bootstrap does at boot, fact 2's store seam).
17. **`status` already reports `connector_attached:
    Option<bool>`** (WI-H): present only when a daemon answers;
    a detached daemon answers `Some(false)`. The session-level
    connect/logout ceremonies below keep this field honest without
    further wire changes (§4.6 refreshes it through the same
    `App::connector_attached`).

## 2. The three gaps (problem statement)

- **G1 — introduction.** An explicit-`standalone` install cannot become
  a syncing install from the CLI; the only path is hand-editing
  `taskboard.toml`. The loudness rule (ADR 0010: no *silent* backend
  flips) must be respected — the fix is an explicit verb, not a quieter
  validation.
- **G2 — liveness.** A booted core cannot *gain* a connector without
  a process restart — no frontend can attach one to a live session
  (REPL, GUI) or daemon. Losing one is half-solved since WI-H's
  merge: a daemon's connector detaches on a one-shot `logout` nudge
  (fact 12), but a *session* still cannot (ADR 0011's refusals), and
  the detach primitive has no attach mirror. ADR 0011 pinned the v1
  honesty ("exit, `taskboard login`, re-enter") and explicitly
  backloged the session-level flow because the Phase 7 GUI cannot
  ask users to quit-and-relaunch to connect.
- **G3 — interaction.** The REPL cannot elicit a masked secret: rustyline
  owns the terminal on the editor thread, histories everything, and the
  async loop must never block on stdin (ADR 0011's core lesson).

Everything below is one design because the *same ceremony* (gather →
verify → persist → store) terminates into three different consumers:
no core (one-shot), the caller's core (session attach), and — later —
a daemon's core (deferred, §10 D5).

A fourth, round-2 driver sits above all three: **multi-connector
readiness** (user mandate, 2026-10-12). ADR 0010 already names future
`taskboard-sync-<name>` crates; the user wants the *option of several
simultaneously active connectors* kept open, implemented later but
without another major architectural change. Concretely: every seam
this design touches must be count-agnostic — no `Option<TheActor>`,
no "the sync channel", no single-connector assumption baked into the
engine or `App` — while v1 enforces "at most one" at the app/config
layer only (§4.5 owns the now-vs-later split). This mandate is what
shapes §4.2's fan-out/fan-in topology.

## 3. Design A — connector-management CLI verbs

### 3.1 The verb: bare `taskboard connect` (D1, resolved)

```
taskboard connect [--server URL] [--user NAME] [--switch-backend] [--force]
taskboard login  [--server URL] [--user NAME]     # deprecated alias
```

- Under the **auto** rule `connect` runs today's `login` ceremony
  verbatim (§7) — the same operation wearing the product's vocabulary
  ("connect a Nextcloud"), fixing discoverability (G1-b).
- **`login` is a deprecated alias of `connect`** (D1): same ceremony,
  same receipt, one-line deprecation notice on stderr (stdout's
  payload contract untouched; notice text is UX, never asserted). The
  alias deliberately does *not* grow `--switch-backend`/`--force` —
  under explicit `standalone` it keeps refusing (kind
  `backend-explicit-standalone`) with guidance naming
  `connect --switch-backend`, and in the re-point case it refuses
  (kind `connection-exists`) naming `connect`. This keeps the
  deprecated surface strictly narrower than its replacement. Removal
  timeline: revisit at v0.1.0 release prep (pre-v0.1.0 churn is
  sanctioned; a cycle of deprecation is cheap either way).
- **Explicit `standalone` needs a loud flip — flag *and* confirm
  (D2).** The config's explicit word must not be silently
  contradicted (ADR 0010). `connect` rewrites
  `sync.backend = "nextcloud"` in the same atomic `toml_edit` overlay
  that writes `server_url`/`username` when the user says so *once*:
  - `--switch-backend` passed → proceed, no prompt (the scripted
    path; the flag is the loud act recorded in argv).
  - flag absent, stdin is a TTY (or a REPL session, §4.4) → one
    y/N confirm ("the config pins backend = standalone; switch it to
    nextcloud?"); declining is the typed
    `backend-explicit-standalone` refusal, exit 1 — the loud rule
    honored interactively with the better REPL user story.
  - flag absent, no TTY (pipes, `--json` scripts, cron) → the typed
    refusal, guidance names the flag. Interactivity follows the same
    TTY detection `StdinPrompts` already uses (login.rs:64) — the
    confirm rides `PromptReader`, so scripted tests fake it like any
    other prompt and hermetic suites never see a TTY.
- **Re-pointing an existing connection: interactive by default,
  `--force` for scripts (D3).** When a *different* server/user is
  already configured, `connect` refuses with kind
  `connection-exists` — unless the user answers the y/N confirm
  ("re-point from A to B?") or passes `--force` (the familiar unix
  escape hatch: same act, scripted). On the confirmed/forced path the
  ceremony, after the new credentials verify, deletes the *old*
  identity's secret in the same sequence that stores the new one —
  overwrite without orphans; declining keeps everything untouched.
  Re-`connect` to the *same* server/user remains an accepted
  idempotent re-verify (secret refresh, no prompt).
- **Multi-connector forward-compatibility:** today `connect` targets
  "the" connector — the single `nextcloud` backend word. When §4.5's
  future arrives, the verb grows an explicit target (`connect
  nextcloud …`, `connect caldav …`) with today's form meaning "the
  default/first connector" — the bare-verb shape chosen now is the
  shape that survives that addition.
- Rejected alternatives:
  - *`connect add` / `connection set <server> <user>`*: implies a
    multi-connector management surface (list/remove) M1 does not have
    — a single `backend` config word and one credential tuple. A bare
    verb with flags is honest about the single-connector present while
    leaving the namespace free for a subcommand family when a second
    connector crate exists (ADR 0010 keeps that door open).
  - *Keeping `login` as the primary name*: the name says
    credentials, the operation says connection (and will say
    *connections*, plural). The alias preserves every existing
    script; the primary name moves to the product's vocabulary.
  - *Flag-only flips (no confirm)*: scripted loudness was never in
    doubt; the confirm covers the *human at a terminal* case, where a
    refusal that says "pass --switch-backend" is worse UX than one
    question. Both modes stay loud; neither can flip silently.
  - *`sync enable` / config verbs*: config-surgery verbs under `sync`
    would split the connection concept across namespaces and tempt a
    generic "edit the config" verb (explicitly not a goal).

### 3.2 Wire lanes (ADR 0012 table extension)

- One-shot `connect` → **`Local`** (pre-core: config + keyring + one
  verify HTTP call; boots nothing, takes no lock) — and `login`, as
  the deprecated alias, keeps its existing `Local` row.
- The daemon-liveness caveat mirrors WI-H's documented behavior: a
  one-shot `connect` while a daemon serves the db persists the
  connection; the daemon's core keeps running connector-less until
  restart (or until the §10 D5 verb exists). v1 documents this exactly
  as ADR 0012 documented `logout`-while-daemon; a stderr heads-up line
  is a settings-round UX decision, not this round's contract.

## 4. Design B — the session-level attach flow

The backlog names two mechanisms ("bootstrap's `sync:
Option<SyncActor>` becomes attachable, or a dedicated re-wiring of the
sync channel halves"). The verified facts force a sharper statement:
**making the field settable is necessary under every variant — the real
question is where the channel halves the engine holds come from.**
Three candidates:

### 4.1 Option A — vacant slot with parked halves (rejected)

Mechanism: under `standalone`, bootstrap parks `sync_rx`/`reports` in an
`App`-owned vacant connector slot instead of dropping them;
`App::attach(client)` spawns the sync actor over the parked halves and
flips the slot to occupied. Engine untouched.

Why it fails (fact 4): a parked-but-alive command receiver makes
`try_send` return `Ok` → every standalone mutation "forwards" its
nudge → `mark_syncing()` → a `Syncing` transient no report ever
settles (the reports sender is parked too). Standalone installs would
show perpetual syncing; `wait_for_boot_pull`-style consumers degrade;
the queue eventually fills where `Full` *still* counts as forwarded.
ADR 0010's invariant — "the engine sees the closed-channel world it
already handles" — would become "a third world: open-but-drainless".
Rescuing A needs an engine-side connector-presence flag, i.e. an
engine change *plus* a second mechanism for one fact; strictly worse
than B.

### 4.2 Option B — additive engine rewire over a fan-out/fan-in topology (the approved design)

Round 1 drafted this as a *swap* of the engine's two channel halves.
The round-2 multi-connector mandate kills the swap shape: swapping
halves on the second attach would orphan the first connector. The
refined mechanism is additive — the engine becomes connector-*count*
agnostic, with v1 enforcing count ≤ 1 at the app layer:

**Command side (engine → connectors): additive fan-out.**

1. The engine's nudge seam becomes a registry, not a single sender:
   the loop holds `sync_outs: Vec<mpsc::Sender<SyncCommand>>`
   (initialized exactly as today — one entry under a `nextcloud`
   boot; under `standalone`, one *dead* entry: bootstrap keeps
   creating the boot pair and dropping the receiver, so the boot path
   is byte-identical, fact 3). `forward_sync_now` fans out: forwarded
   iff **any** send returns `Ok`/`Full`; `Closed` entries are pruned
   lazily right there — which now covers **both** dead-entry paths:
   the standalone boot's dropped receiver *and* a detached actor's
   dropped receiver (WI-H, fact 15). With ≤ 1 live connector this is
   observably identical to today's single-`try_send` (facts 4, 6);
   with N it is the correct "someone will sync this" semantics.
2. New channel-bearing envelope variant (engine crate, per ADR 0004):
   `EngineCommand::AttachConnector { sync_out:
   mpsc::Sender<SyncCommand>, reply: oneshot::Sender<AttachReceipt>
   }` — **push**, never replace. The reply settles after the push
   (FIFO ordering — same durability story as `Flush`, fact 7). The
   v1 guard is the app layer's: `App::attach_connector` refuses with
   typed `AlreadyAttached` when its registry is non-empty (D4) — the
   engine itself stays count-blind, which is precisely what the
   multi-connector round needs it to be. Note the guard composes
   with WI-H's detach: `detach_connector` *takes* the actor out, so
   the empty-registry check permits **re-attach after detach** while
   refusing double-attach — exactly the lifecycle a
   connect→logout→connect session walk produces.

**Report side (connectors → engine): one permanent fan-in inbox.**

3. Bootstrap creates the reports pair **once, at boot, forever** and
   hands the engine the receiver as today; the twin sender is *parked
   in `App`* under every backend. Every attached actor's spawn gets a
   `clone()` of that one sender — N actors, one inbox, and the
   engine's select loop keeps its single `report_rx` arm with **zero
   structural change and no `reports_open` juggling**. (Parking a
   *sender* is semantically inert — `recv()` simply never fires while
   no actor exists; only the receiver side carried the Option-A
   poison, fact 4.)
4. The drain-ordering rule (fact 14): because the parked sender lives
   in `App`, `App::shutdown` must **drop the connector channel halves
   (parked sender + per-actor senders) before awaiting the engine
   join** — otherwise the linger-drain waits for a `Closed` that only
   `App`'s own drop can produce. One explicit, ordered drop step in
   `shutdown`; pinned by a test (standalone boot → shutdown →
   `actor_aborted == false`). Note the current sync-actor join
   already runs before the engine join (bootstrap.rs:186-190), so the
   step slots in naturally; fact 13's "zero changes" claim is
   amended to "zero changes to the *join* logic, one added drop
   step".

**App side.**

5. WI-H already made the slot interior-mutable
   (`sync: Mutex<Option<SyncActor>>`, fact 3); this design widens
   the same bookkeeping to `connectors: Mutex<Vec<SyncActor>>` —
   v1 invariant: length ≤ 1, enforced by `attach_connector`'s
   `AlreadyAttached` guard and by bootstrap (one-or-zero at boot).
   The mutex discipline carries over verbatim (lifecycle state
   only, never held across `.await`, AGENTS §3 note). 
   `sync_commands()` clones every live sender (empty → the same
   fail-closed sender as today); `shutdown` takes the lock once and
   joins all present actors (fact 13). The Option→Vec widening is
   the "no second major change" insurance: the multi-connector
   round lifts the app-layer guard and nothing else in `App` moves.
6. `App::attach_connector(&self, client: DeckClient) ->
   Result<(), AttachError>` — `&self`, mirroring WI-H's
   `detach_connector(&self)`: the IPC dispatcher and the REPL loop
   both hold `&App`/`&mut`-free views, and a future daemon-side
   attach verb (fact 16) calls this exact method. Body: create one
   fresh command pair, send `AttachConnector { sync_out:
   sender.clone(), … }` and await the ack, then `spawn_sync_actor(
   client, engine_reader, fresh_rx_cmd,
   parked_reports_sender.clone(), system.clone(),
   config.sync_actor_config())`, push the `SyncActor` into the
   registry. The actor self-targets from the persisted binding and
   fires its first cycle immediately (fact 5) — the caller may then
   await settling with the *existing* `wait_for_boot_pull` helper
   (bounded, degrade-to-local — offline-first; the receipt does not
   depend on it).

Why it wins: the invariant "channel liveness **is** connector
presence" survives unchanged (no flags, no third world); the boot
path is untouched; the engine's only structural concession is
`Vec` + a fan-out loop; and it *composes with WI-H's merged detach
rather than replacing it* — `detach_connector` stays exactly as
merged (`SyncCommand::Detach`, graceful in-flight-cycle completion,
bounded join, fact 15); under the registry the engine-side
consequence of a detach is simply a dead entry the next fan-out
prunes. It generalizes *by construction*: attach #2..N is the same
command again with the app-layer guard lifted. Cost: one engine
arm + the fan-out seam + one handle method + the shutdown drop
step — the only `taskboard-state` diff in this design.

### 4.3 Option C — teardown and re-bootstrap in-process (rejected)

Shut the core down, re-run `bootstrap` with the new config, hand the
frontends the new `App`. No engine change — but: (a) **lock-epoch
hole**: between `shutdown` (releases the instance lock) and the new
  bootstrap's acquire, any concurrent process can steal the db; the
  ceremony would fail `instance-locked` mid-flight; (b) every
  frontend-held handle is invalidated — the REPL's completion helper
  holds the old `ArcSwap<AppState>` (fact 8), the GUI would hold
  views onto a dead core; (c) hydration + boot-pull re-run for a
  purely local state change. Rejected; recorded because it is the
  zero-engine-diff siren.

### 4.4 The session choreography (what `connect` does in-session)

Order matters; every step is typed-failable and leaves a recoverable
state on failure:

1. **Ceremony** (§7) with the session's `PromptReader` (§5): verify →
   persist config overlay (+ optional `--switch-backend` rewrite) →
   store secret. Until this completes, *nothing* about the running
   core has changed — a failed verify is just a failed line (ADR
   0011: lines never abort the session).
2. **Refresh the session config view**: re-read the config from the
   file the ceremony just wrote (validate; on the degenerate
   read-back failure, refuse the attach and tell the user to restart —
   the file is verified content, so this is belt-and-braces). The
   REPL loop owns `ReplSession` and mutates it between lines (fact
   8); connector preludes then see `nextcloud` (fact 9).
3. **Attach**: build `DeckClient` from the refreshed config + the
   session store, call `App::attach_connector` (§4.2), optionally
   await the first settling cycle (`wait_for_boot_pull`-shaped,
   bounded, degrade-to-local on timeout — offline-first).
4. Emit the receipt (`LoginReceipt`-shaped plus an `attached: true`
   field; JSON contract addition, pre-v0.1.0-sanctioned like WI-D2's
   kind surfacing).

**Routing consequence:** `connect` is not a `dispatch_line` arm — it
mutates the session itself (`&mut App` + `&mut ReplSession`), which
`dispatch_line(&session, &app, …)` cannot express. It joins
`exit`/refusals as a **session-layer action**: the REPL intercepts
`Command::Connect` at the session loop (a `login` line routes to the
same ceremony under its deprecated name, §5). Rule for ADR 0014:
*verbs that mutate the session itself live in the session layer; the
verb bodies stay frontend-agnostic functions.* The GUI's "Connect…"
menu item and the future daemon-side verb call the same §7/§4.2
functions with their own prompt sources.

### 4.5 Multi-connector readiness — what v1 fixes now vs. what the future round owns

The round-2 mandate is "architecture ready, implementation later".
This section is the contract: the items in **Fixed now** are part of
this design's implementation items (§12); the items in **The future
round owns** are known, additive, and *do not require re-touching
anything in Fixed now*.

**Fixed now (count-agnostic seams v1 builds anyway):**

- Engine nudge registry + fan-out + lazy pruning (§4.2.1) — the loop
  never says "the connector".
- One permanent fan-in reports inbox with per-attach sender clones
  (§4.2.3) — the report path is already N-wide.
- `EngineCommand::AttachConnector` is additive-push (§4.2.2) — attach
  #2 is the same message again, guard lifted.
- `App.connectors: Vec<SyncActor>` with an app-layer ≤ 1 invariant
  (§4.2.5) — lifting the guard is an app-layer diff.
- `CredentialStore` keying is already per-(server, username) — no
  singleton assumption in secrets.
- The `connect` verb shape (bare verb, optional explicit target
  later, §3.1) and the daemon-verb placement decision (§6) keep the
  *frontend* surface count-agnostic.

**The future round owns (deferred, each additive, none re-opens this
design's seams):**

- *Config shape*: `[sync] backend` stays a scalar word in v1. The
  multi round introduces `[[sync.connectors]]`-style array-of-tables
  (plus a resolution rule against the legacy scalar; the scalar
  remains "the default/first connector" so `--switch-backend`'s
  writes stay valid). Not designed further here — figment strictness
  and validation loudness get their own pass.
- *`SyncCommand` routing*: `SetBoard { id }` is untargeted today
  (fine at N=1). At N>1 it grows a connector routing key (or an
  enveloped per-connector command) — an additive message change in
  the domain vocabulary, per ADR 0010's "port stays vocabulary"
  stance; the engine's fan-out topology does not change again.
- *Per-connector status projection*: `AppState.sync` is a singleton
  (`SyncPhase`, `last_success`, …). The multi round makes it
  per-connector (map keyed by connector id) — an additive projection
  change in domain+state; v1's `status`/banner surfaces keep reading
  the singleton, which remains well-defined as "the (one) connector's"
  status.
- *Per-connector board ownership*: `Board.remote` is a bare
  `RemoteBoardId` today; multi needs a source/connector tag on the
  binding so pushes and pulls route to the owning actor (ADR 0010's
  identity-based attach is per-connector by then). Additive domain
  field + push-filter change.
- *Network authority*: the sync actor is "the network authority" for
  `NetworkLost/Restored` (ADR 0007/0012). At N>1 that authority
  merges (any-connector-down vs all-down) — a `SystemEvent` semantics
  round, not a topology one.
- *Connector trait promotion*: ADR 0010 keeps
  `SyncCommand`/`SyncReport`/`SyncStateReader` as vocabulary, not a
  trait; a second concrete connector may promote them. Orthogonal to
  attach topology.
- *Verb surface growth*: `connect <name>`, `connect list/remove`,
  per-connector `status` fields — CLI round, rides the config-shape
  round.

**The invariant that makes this readiness real** (ADR 0014 decision
wording): *the engine and `App` are connector-count-agnostic; v1
enforces "at most one active connector" exclusively at the app/config
layer (`attach_connector`'s guard + the scalar backend word), and
every deferred item above is an additive change that does not alter
channel topology.* If a future proposal needs to change §4.2's
topology to reach N connectors, that proposal has misdesigned
something else.

### 4.6 The mirror: in-session `logout` (round 3, per WI-H plan D7)

WI-H wired the one-shot `logout` → daemon detach nudge and
deliberately left the REPL refusing `logout` — its plan decision D7
hands the session-level half to this round: "session-level login
**and logout** … REPL and the Phase 7 GUI alike". The design is the
connect choreography (§4.4) run backwards, reusing WI-H's merged
primitives verbatim:

1. **Local ceremony first** (`logout::run` unchanged): require the
   configured identity, delete the secret, strip the auto config
   keys — an in-session `logout` is exactly the one-shot body minus
   the process shell, the same extraction shape §4.4 applies to
   `connect`.
2. **Detach the session's own connector**: `App::detach_connector()`
   — already merged, `&self`, graceful (an in-flight cycle completes
   first), bounded join, idempotent `NoConnector` when the session
   booted standalone or already detached. No new machinery; the
   honesty riders WI-H shipped (`sync` → typed `sync-unavailable`,
   `status` → `connector_attached: false`) apply to the session core
   automatically.
3. **Refresh the session config view** (the §4.4 step-2 mirror): the
   stripped config re-resolves to standalone, so connector preludes
   go back to refusing `sync-disabled` — the session is honestly
   local-only until a later `connect` line re-attaches (which the
   round-3 §4.2.2 guard explicitly permits: re-attach after detach).
4. **Receipt**: `LogoutReceipt` + `daemon_connector`-shaped field
   naming the detach outcome (`detached`/`no-connector`/`timed-out`)
   — the same vocabulary WI-H's IPC response established.

Consequences: `LineRefusal::Logout` dies alongside
`LineRefusal::Login` (§5.4); ADR 0011's in-session refusal list
amendment now covers both; the GUI's "Sign out" menu item is this
same sequence, exactly as its "Connect…" is §4.4. The one-shot
`logout`'s daemon nudge (WI-H) is unchanged — a session is its own
lock holder, so there is no daemon to nudge from inside it.

## 5. Design C — the REPL masked-secret prompt

### 5.1 The constraint set

The editor thread owns rustyline and blocks in `readline()` between
lines (fact 8); the async loop must never touch stdin (ADR 0011);
secrets must never enter history or the completion machinery; and a
secret is elicited *mid-handler*, not as part of the submitted line —
so the loop must be able to ask the editor thread for a follow-up
read while no `readline()` call is in flight. A naive "prompt-request
channel the editor polls between lines" races: after delivering
`Line("connect")` the editor re-arms `readline()` immediately and the
request would sit unanswered behind the user's next keystrokes.

### 5.2 The disposition protocol (recommended)

Extend the editor thread from a one-way `LineEvent` stream to a
per-line request/response handshake:

- After sending `Line(word)s`, the editor thread **waits for a
  `Disposition`** before printing the next prompt / re-entering
  `readline()`:
  - `Disposition::Next` — line fully handled; resume normal editing.
  - `Disposition::Prompt { label, kind }` (`kind` ∈ `Line` |
    `Masked` | `Confirm`) — the running handler needs one more
    input: perform it on the editor thread (`Masked` → `rpassword`
    masked read with `*` feedback — the same config `StdinPrompts`
    uses, login.rs:47-51; `Line` → a cooked-mode line read; `Confirm`
    → a y/N line reduced to `bool` there), send the answer back, and
    await the next disposition (handlers may prompt repeatedly:
    server, user, password, the D2/D3 confirms).
- Ctrl-C during a prompted read is `Interrupted`: the pending prompt
  answers with a cancellation and the ceremony aborts with the
  existing typed `CommandError::Prompt` — the session survives (a
  failed line never aborts it).
- The editor thread performs *reads only* — no handler logic; it is
  the terminal's single owner, and the masked path bypasses history
  (`add_history_entry` is never called) and the completion helper by
  construction.

Backpressure/seriality are unchanged: the loop processes one line at
a time, so at most one disposition cycle is live; the existing bounded
channel keeps its role for `Line`/`Eof`/`Interrupted` events. Tests
inject a scripted disposition peer exactly as they inject `LineEvent`
channels today — no TTY in hermetic suites.

### 5.3 Rejected alternatives

- **`--password` flag / env-only secrets in-session**: secrets in
  argv/Shell history/process listings — a security regression
  (`login` refuses to take the password as a flag for the same
  reason).
- **`rpassword` from the async loop**: blocks a runtime worker —
  ADR 0011's founding lesson (fatal on a 1-core kiosk).
- **rustyline-native masking**: rustyline 17 has no echo-off
  readline API; `rpassword` is already the house tool for masked
  reads.
- **Handler logic on the editor thread**: turns the reader thread
  into a dispatcher; violates the frontend seam (§4.4's rule).

### 5.4 The `PromptReader` reuse

`ReplPrompts` (session-owned) implements the ceremony's existing
`PromptReader` trait by shipping each prompt to the disposition
channel and awaiting the answer. The trait grows one method for D2/D3
— `read_confirm(&mut self, prompt) -> bool` (y/N; `StdinPrompts`
reads a line, the REPL maps it to the `Confirm` disposition kind, a
GUI maps it to a yes/no dialog) — so every interactive decision the
ceremony makes rides the same injected seam. The ceremony code (§7)
cannot tell the REPL from the terminal from — later — a GUI dialog
source; the three-ish labeled inputs map 1:1 onto a GUI form.

**In-session `login` and `logout` (D1 + round 3):** a `login` line
no longer hits `LineRefusal::Login` — it routes to the same session
ceremony as `connect`, preceded by the one-line deprecation notice
(stderr; stdout/JSON contract untouched). A `logout` line no longer
hits `LineRefusal::Logout` either — it routes to the §4.6 in-session
ceremony (local teardown + `detach_connector` + config refresh),
per WI-H plan D7's handoff. The remaining refusals (`Daemon`,
`ConfigCommand`, per-line `--config`) stand.

## 6. Design D — Phase 7 GUI reuse (and the daemon, deferred)

The GUI consumes exactly three pieces, all frontend-agnostic by
construction:

1. **The ceremony function** (§7) with a dialog-backed `PromptReader`:
   three fields (server, user, app password), one "Connect" button,
   the same verify-then-persist order. Nothing in the ceremony prints
   or reads directly.
2. **`App::attach_connector`** (§4.2): the menu action calls it after
   the ceremony; no CLI/REPL types appear in its signature. The GUI's
   "connecting/online/offline" rendering is the same
   `sync.phase`/`SystemEvent` surface the boot path produces — the
   attach's first cycle makes `Syncing` → settled observable exactly
   like a boot (fact 5).
3. **The config-refresh step** (§4.4 step 2): the GUI's long-lived
   config view is refreshed from the ceremony's write, exactly like
   the REPL's `ReplSession`.

What the GUI deliberately does **not** need: the editor-thread
disposition protocol (dialogs replace it), `dispatch_line` changes,
or any LineRefusal lifting. What the GUI gets for free by design: the
offline banner, `NetworkLost/Restored`, and shutdown sequencing are
already connector-presence-keyed. A future GUI *account switcher*
over N connectors rides §4.5's future round and changes none of the
three pieces above — it calls `attach_connector` more than once once
the app-layer guard lifts.

**Daemon (fourth consumer — the detach half already exists on
`main`):** WI-H's merge (fact 12) landed the server-executed
*detach* verb (`DetachConnector` over IPC: additive variants, no
version bump, identity-matched, typed outcomes) and the one-shot
`logout` nudge that drives it. The *attach* sibling — a daemon-side
`connect` verb — remains future work that now rides an established
pattern rather than needing an envelope decision: additive
`Request::AttachConnector`-shape variants, no bump, and **no secret
on the wire** (fact 16: the daemon re-reads its config and loads the
credential store itself, exactly as bootstrap does). This round's
contract is unchanged and now half-verified by WI-H's code: the
machinery lives in the app lib (`bootstrap.rs`, `&self` methods —
§4.2.6) so the future IPC round wires a verb, not logic. Interim
honesty gap, mirroring WI-H's documented pre-fix state: a one-shot
`connect` while a daemon serves persists the connection, but the
daemon's core stays connector-less until restart or that future
verb — v1 documents it exactly as ADR 0012 documented
logout-while-daemon, and the one-shot may print the mirror of
WI-H's logout warning (a UX call for the implementation plan).

## 7. The shared ceremony — exact seams

One function family, three call sites (one-shot, session, future
daemon/GUI):

```rust
// commands/connect.rs (new) — the ceremony, frontend-agnostic:
// gather (flags | PromptReader) → guards (D2 backend confirm-or-flag,
//   D3 re-point confirm-or-force) → verify → persist config overlay
//   (+ backend rewrite iff switched) → store secret (+ delete the
//   re-pointed old secret iff forced/confirmed) .
pub async fn run(
    config_path: &Path,
    raw: AppConfig,
    args: ConnectArgs,          // server/user/switch_backend/force
    prompts: &mut dyn PromptReader,
    store: &Arc<dyn CredentialStore>,
) -> Result<ConnectReceipt, CommandError>

// bootstrap.rs — the attach primitive (§4.2), the merged
// detach_connector's mirror (WI-H): &self, same registry mutex.
impl App {
    pub async fn attach_connector(&self, client: DeckClient)
        -> Result<(), AttachError>;   // AlreadyAttached guard = the
                                     // v1 app-layer count-≤-1
                                     // invariant; permits re-attach
                                     // after detach_connector
    // connector_attached(&self) -> bool — already merged (WI-H);
    // detach_connector(&self) -> DetachOutcome — already merged.
}
```

- One ceremony, one name, one test home: `login`'s CLI arm calls
  `connect::run` with `switch_backend = false, force = false` (the
  alias is parse/dispatch-level, not a second body — one behavior,
  one test home per AGENTS §5). `login.rs`'s existing
  verify→persist→store tail moves into `connect.rs` behavior-
  preserving; its scripted-prompt test harness carries over verbatim.
- `ConnectReceipt` extends `LoginReceipt` with the facts the receipt
  owes the operator: `backend_switched`, `repointed`, later
  `attached`.
- The REPL's `Command::Connect` interception, the one-shot dispatch
  arms (`connect` + the `login` alias), and `route_for_ipc`'s new
  `Local` row are the only frontend-specific code in the whole
  design.

## 8. Testing strategy (for the future implementation plan)

- **Engine (taskboard-state)**: attach is additive — nudges fail
  closed *before*, forward *after*; a report sent on a clone of the
  permanent inbox sender is ingested; dead registry entries are
  pruned on the next fan-out; `AttachConnector`'s ack settles after
  the push; the parked-sender drain rule (standalone `App` shutdown
  drops connector halves before the engine join → `actor_aborted ==
  false`; a deliberately-kept sender would hang — pinned as the
  ordering contract, fact 14). All deterministic — no time
  manipulation needed; fakes per AGENTS §5.
- **Ceremony (taskboard-app)**: scripted `PromptReader` fakes pin
  ordering (verify-before-persist), the explicit-standalone guard's
  three modes (flag → proceed; TTY → confirm-yes proceeds /
  confirm-no refuses typed; no TTY no flag → typed refusal), the
  re-point guard's three modes (confirm/`--force` → proceed **and**
  the old secret is deleted after the new verify; decline → typed
  `connection-exists`), idempotent same-identity re-verify (no
  prompt), backend rewrite in the overlay, and the `login` alias
  routing to the same body with both flags false (structural
  assertions on the written TOML, no text asserts).
- **Session choreography**: hermetic session tests inject line events
  + a scripted disposition peer; assert the session config refresh,
  the attach call sequence, and that a failed ceremony leaves the
  core untouched (`connector_attached() == false`, phase unchanged).
  The §4.6 mirror gets the same tier: an in-session `logout` on a
  connected session detaches (outcome `Detached`), the refreshed
  session config resolves standalone, a following `sync` line
  refuses typed `sync-unavailable` (WI-H's rider, now in-session),
  and a subsequent `connect` line re-attaches (guard permits
  re-attach after detach).
- **Completion/lane tables**: `route_for_ipc(Connect) == Local`;
  completion offers `connect` and its flags; the masked path never
  enters history (structurally: the disposition answer path has no
  history call — pinned by the protocol's unit tests).
- **E2E (binary tier)**: the existing pipe-stdin harness drives the
  one-shot `connect` (piped plain-line secret path) against a
  scripted server, exactly as the L-series drove `login`.

## 9. Draft ADR 0014 (for stage 2 to formalize after approval)

```org
#+title: ADR 0014: Connector Attach & Session-Level Login
- *Date:* 2026-10-XX
- *Status:* Draft (WI-J design round, pending user direction)
- *Amends:* ADR 0010 (decision 9's refusal surface), ADR 0011
  (the in-session login/logout refusals)

* Context
ADR 0010 made the connector optional but fixed at boot; ADR 0011 made
login honest by refusing it mid-session; the Phase 7 GUI cannot ask
users to relaunch to connect. One design covers introducing a
connection (CLI), activating it on a booted core (REPL/GUI/daemon),
and eliciting the secret (REPL terminal).

* Decision
1. *`taskboard connect` introduces a connection; `login` is its
   deprecated alias.* One ceremony (guards → verify → persist →
   store). Under explicit =standalone= the flip is loud twice over:
   =--switch-backend= for scripts, a y/N confirm on a TTY. Re-pointing
   a configured install likewise: y/N confirm by default, =--force=
   for scripts; the confirmed/forced path deletes the replaced
   identity's secret after the new verify (no keyring orphans).
2. *Attach is an additive engine rewire over a fan-out/fan-in
   topology, composing with WI-H's detach.* Commands: the engine
   keeps a =Vec= of connector senders and fans nudges out (forwarded
   iff any send lands; =Closed= entries pruned lazily — covering
   both standalone boots and detached actors);
   =EngineCommand::AttachConnector= (fresh command sender + oneshot
   ack) *pushes*, never replaces. Reports: one permanent fan-in
   inbox created at bootstrap; every attached actor clones its
   sender. =App::attach_connector(&self)= mirrors the merged
   =detach_connector(&self)= (WI-H) over the same lifecycle mutex;
   detach machinery is reused verbatim, not replaced. Channel
   liveness remains the sole presence signal; parked command halves
   were rejected (they flip standalone nudge semantics = perpetual
   =Syncing=); re-bootstrap was rejected (lock-epoch hole + handle
   invalidation).
3. *The engine and `App` are connector-count-agnostic; "at most one
   active connector" is an app/config-layer invariant only*
   (=App.connectors: Vec=, =attach_connector='s =AlreadyAttached=
   guard, the scalar =[sync] backend= word). Several simultaneously
   active connectors are a stated future direction: v1 builds none
   of it, but every seam above is already N-wide, and the deferred
   list (per-connector =SyncCommand= routing, status projections,
   board-ownership tags, config array-of-tables, merged network
   authority) is additive and does not re-open channel topology.
   =App::shutdown= drops connector channel halves before the engine
   join (the linger-drain gates on the reports channel closing).
4. *Attach self-configures.* The actor resumes its pull target from
   the persisted binding and fires immediately; the caller may await
   settling via the boot-pull helper (bounded, offline-first).
5. *Session verbs that mutate the session live in the session
   layer.* =connect= is intercepted by the REPL loop (like =exit=),
   not a =dispatch_line= arm: it needs the session mutation (attach +
   config refresh). Verb bodies stay frontend-agnostic functions. A
   =login= line routes to the same ceremony under its deprecated
   name. *In-session =logout=* is the mirror (WI-H plan D7's
   handoff): local teardown ceremony + =detach_connector= + config
   refresh; both ADR 0011 session refusals die.
6. *The REPL secret prompt is a disposition handshake.* The editor
   thread awaits a per-line disposition; =Prompt{kind: Masked}=
   performs a masked =rpassword= read on the editor thread; answers
   bypass history and completion; =Confirm= rides the same
   handshake; Ctrl-C cancels the ceremony typed. The async loop
   never touches stdin.
7. *The GUI contract is the ceremony fn + =attach_connector=/
   =detach_connector= + config refresh* — no REPL/CLI types
   involved; "Sign out" is the §4.6 mirror of "Connect…".
   Daemon-side *attach* over IPC remains future work that rides
   WI-H's merged pattern (additive variants, no version bump,
   identity on the request) and needs no secret on the wire — the
   daemon re-reads its config and credential store itself.

* Consequences
- taskboard-state gains one command arm + the fan-out seam;
  taskboard-app gains the connect verb + login alias, the two
  session ceremonies (connect, logout), the disposition protocol,
  =Mutex<Vec= SyncActor==-shaped connector state, and the shutdown
  drop step; ADRs 0010/0011 get amendment notes (0011's: both
  in-session refusals are replaced by ceremonies).
- The detach half of connector liveness is WI-H's, merged and
  reused as-is; this round adds only its mirror.
- The multi-connector round lifts the app-layer guard, extends the
  deferred list in §4.5, and touches nothing designed here.
```

## 10. Decisions (resolved at plan review 2026-10-12, round 2)

- **D1 — RESOLVED: bare `taskboard connect`; `login` becomes a
  deprecated alias** (same ceremony under the old name, stderr
  deprecation notice, narrower flag surface — no
  `--switch-backend`/`--force`; its refusals name `connect`). Removal
  revisited at v0.1.0 release prep. Details §3.1.
- **D2 — RESOLVED: `--switch-backend` *and* an interactive confirm.**
  The flag serves scripts; a y/N confirm (via `PromptReader::\
  read_confirm`) serves the terminal/REPL story; flag-absent +
  no-TTY stays the typed refusal. Details §3.1, §5.4.
- **D3 — RESOLVED: re-point handled interactively by default,
  `--force` for scripting** (the unix-utility style). Confirmed or
  forced re-points overwrite the config keys and delete the replaced
  identity's secret after the new verify — no keyring orphans.
  Declining/absent-both is the typed `connection-exists` refusal.
  Details §3.1.
- **D4 — RESOLVED as recommended: typed `AlreadyAttached` refusal via
  oneshot ack** — which under the round-2 topology doubles as *the*
  app-layer enforcement of "at most one active connector" until the
  multi-connector round lifts it (§4.5). Details §4.2.
- **D5 — RESOLVED BY EVENTS (round 3): WI-H merged (PR #48, §1
  fact 12) and answered the envelope question this deferral was
  waiting on.** The detach half of connector liveness is production
  code (`SyncCommand::Detach`, `App::detach_connector`, the
  `DetachConnector` IPC verb with the additive-no-bump policy).
  Daemon-side *attach* over IPC stays future work — but it is no
  longer "deferred to a design round": it rides WI-H's merged verb
  pattern (identity on the request, typed outcomes, no version bump)
  and calls this design's `App::attach_connector(&self)`; no secret
  travels the wire (§6, fact 16). WI-H plan D7 also handed this
  round the session-level logout that originally looked like WI-H
  territory — folded in as §4.6. Scope note for stage 2: §4.6 is a
  round-3 addition to WI-J's dispatch scope (the original §4 WI-J
  text named only verbs/attach/prompt/GUI); it is small, reuses
  merged machinery, and completes ADR 0011's amendment — flag it at
  stage-2 dispatch so the scope growth is a visible yes.

## 11. Code the implementation would touch (inventory for stage 2)

| Crate / file | Change |
|---|---|
| `taskboard-state/src/actor.rs` | `EngineCommand::AttachConnector` variant, additive loop arm (push to registry), nudge fan-out + lazy pruning seam, `EngineHandle::attach_connector`, receipt types |
| `taskboard-app/src/bootstrap.rs` | `App::attach_connector(&self)` (mirror of merged `detach_connector`); `sync: Mutex<Option<SyncActor>>` (WI-H) → `connectors: Mutex<Vec<SyncActor>>`; the permanent reports inbox + parked sender; the shutdown drop-halves-before-engine-join step; extract the client-construction half of the `SyncBackend::Nextcloud` boot arm for reuse |
| `taskboard-app/src/cli.rs` | `Command::Connect { server, user, switch_backend, force }`; `Login` marked deprecated (alias semantics) |
| `taskboard-app/src/commands/mod.rs` | one-shot dispatch arms (`connect` + alias), `route_for_ipc` rows, ceremony extraction points |
| `taskboard-app/src/commands/connect.rs` | NEW: ceremony (§7), D2/D3 guards + confirms, receipt + renders |
| `taskboard-app/src/commands/login.rs` | body moves to `connect.rs` behavior-preserving; `PromptReader::read_confirm` added; `StdinPrompts` keeps serving both names |
| `taskboard-app/src/commands/logout.rs` | session-capable extraction for §4.6 (the `logout::run` body minus the process shell; one-shot wrapper + daemon nudge unchanged — WI-H's) |
| `taskboard-app/src/repl.rs` | `Command::Connect` and `Command::Logout` session interception (+ `login` alias routing with the deprecation notice; `LineRefusal::Login` **and** `Logout` die), session config refresh, `ReplPrompts` |
| `taskboard-app/src/repl.rs` (editor thread) | disposition channel + protocol (§5.2: `Line`/`Masked`/`Confirm`); masked read path |
| `taskboard-app/src/completion.rs` | `connect` verb/flag candidates (login stays offered until removed) |
| `taskboard-app/src/main.rs` | exit-code glue only |
| `docs/adr/0014-*.org` + `docs/architecture.org` | stage 2: formalize ADR + comms-table rows + verb catalogue |
| backlog | both origin entries ✅ (with the WI-H cross-reference note) |

Untouched by this design: `taskboard-domain` (the connector-liveness
message it needed — `SyncCommand::Detach` — already merged via WI-H;
attach adds no further domain change, the rewire is envelope-level),
`taskboard-sync-nextcloud` (`spawn_sync_actor` already has the exact
signature attach needs — it receives its reports sender clone; the
`Detach` arm is WI-H's, reused as-is), `taskboard-storage-sqlite`,
`taskboard-ui-slint`.

## 12. Implementation slicing (future work-item plans, not this round)

1. **WI-J.1** engine additive rewire (registry + fan-out + ack) and
   the app-side attach primitive (`Mutex<Vec>` widening, permanent
   reports inbox, shutdown drop step) — the whole
   multi-connector-ready topology, with the count-≤1 guard and the
   re-attach-after-detach path (state crate + app bootstrap; engine
   + shutdown-ordering tests).
2. **WI-J.2** `connect` verb + `login` alias + ceremony extraction +
   lanes + D2/D3 guards (one-shot CLI; hermetic + E2E).
3. **WI-J.3** REPL: session interception of `connect`/`login`/
   `logout` (§4.4 + §4.6) + config refresh + disposition protocol
   (the TTY-hermetic test seam).
4. (future, pattern ready — §6) daemon-side attach verb riding
   WI-H's additive-IPC pattern; no envelope decision left to make.
5. (future, own round) multi-connector lift: app-layer guard → N,
   plus the §4.5 deferred list.

Each lands as its own plan/PR per AGENTS §8 granularity; WI-J.1 →
WI-J.2 → WI-J.3 is the natural order (2 and 3 are independent after
1).

## 13. Backlog interactions

- Both origin entries carry the design-round note, updated to
  direction-**approved** and round-3-revised for WI-H's merge (this
  commit); ✅ DONE happens in the implementation items, not stage 1.
- **New backlog entry added (round 2): "Multiple active sync
  connectors"** — the mandate, recorded so the future round has a
  home: lifts the app-layer count-≤1 guard and owns §4.5's deferred
  list (per-connector routing/status/ownership, config
  array-of-tables, merged network authority, verb surface growth).
- WI-H's own entry ("`logout` while a daemon runs") is ✅ DONE on
  `main` since PR #48 — no interaction beyond the D7 handoff §4.6
  implements.
- The "REPL history persistence" entry is unaffected (the disposition
  path adds no history semantics).
- Remaining backlog candidates: daemon-side attach verb (pattern
  ready, §6), `connect` while daemon runs (the documented liveness
  gap + optional warning), `login`-alias removal (v0.1.0 release
  prep).

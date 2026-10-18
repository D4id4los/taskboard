# [Plan] Multi-board Design Round — Stage-1 Work Plan (dispatch chunk 1)

- **Date**: 2026-10-18
- **Kind**: stage-1 work plan for dispatch chunk 1
  (`docs/multi-board-detailed-design`) of the multi-board design-round
  dispatch. It fixes **how** the stage-2 research agent investigates
  framing §5 Q1–Q9 and writes the detailed-design draft: the seams to
  re-verify (with plan-day line numbers; finding IDs are the stable
  keys), the option space each question must consider, the evidence
  each option needs, the deliverable's outline, and the risk list.
  Every method-level open question is resolved below as a recorded
  decision (MD-1…MD-10). **This plan does not answer any design
  question** — option enumeration here delimits the search space the
  framing plan itself named; choosing among options is stage 2's work,
  within the binding constraints.
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-18-multi-board-design-round-dispatch-plan.md`
  §1 (shared execution model, stage 1) + §2 (chunk 1 covers/acceptance);
  framing plan
  `.artifacts/plans/2026-10-18-multi-board-wave-design-plan.md`
  (normative except §6, advisory input only).
- **Status**: DONE (stage 1). Stage 2 executes this plan in
  `../taskboard-WT-docs-multi-board-detailed-design` on branch
  `docs/multi-board-detailed-design`.
- **Verified baseline (plan day)**: `main` @ `62dd839`
  (= `origin/main`, post-PR #333 framing plan + PR #334 board/stack
  structure editing; precondition check passed — the framing plan's
  commits `d5ee1db`/`16b4b3a` are on `main`). All anchors in §3 were
  located by grep on this tree; drift versus the framing plan's
  `878bb12` anchors is recorded per finding ID.

---

## 1. Mission (what stage 2 produces)

The deliverable artefact
`.artifacts/plans/<today>-multi-board-detailed-design-plan.md`
(status PROPOSED) resolves framing §5 Q1–Q9 as **one coherent design**:
per question — re-verified anchors, options, trade-offs, exactly one
recommendation, consequences for later work; plus the draft ADR 0010
amendment text (Q4), the F16-22 migration design against 0001/0005
conventions (Q2/Q9), a decision record left PROPOSED for the user
(D-W9's SyncStatus keying is **posed with a recommendation, never
self-ruled**), and a backlog-candidates section. Binding constraints
(dispatch §2): D-W1…D-W8 may not be contradicted (deviations need a
recorded reason and go back to the user); the connect-flow §4.5/ADR
0014 topology invariant is the design's acceptance test; AGENTS §2
domain purity for the trait port; zero production code, zero tracker
edits.

## 2. Method decisions (recorded; binding for stage 2)

- **MD-1 Anchor discipline.** Stage 2 re-verifies **every** anchor it
  will cite against the worktree's code at execution start, records the
  worktree base SHA in the design doc's baseline section, and reports
  drift **by finding ID** (IDs are the stable keys; line numbers are
  not). §3 below is the plan-day snapshot @ `62dd839`; stage 2 may find
  remediation-wave-1 tail chunks have shifted numbers — that is
  expected (framing §7 item 2), not a stop condition.
- **MD-2 Verification commands.** Each seam group in §3 carries the
  locating grep; stage 2 re-runs them and the design doc's baseline
  table names the command class used, so the review agent can reproduce
  the verification (evidence & artefact mandate).
- **MD-3 Deliverable organization.** One section per framing §5
  question, numbered Q1–Q9 to match the framing plan exactly; each
  contains: anchors (re-verified), option space, per-option evidence
  and trade-offs, exactly one recommendation, consequences, and a
  topology-invariant conformance note ("additive on which fan-out/
  fan-in seam, or why the engine is untouched"). Cross-cutting sections
  per §5's outline below.
- **MD-4 Option-space policy.** The option spaces in §4 are the
  framing plan's own named alternatives, minimally elaborated so each
  is evaluatable. Stage 2 evaluates all listed options (a dismissed
  option still gets its dismissal evidence); a genuinely new option
  discovered mid-investigation may be added, recorded as a deviation
  with its origin — never silently.
- **MD-5 Coherence pass (mandatory, last).** After drafting all nine
  sections, stage 2 runs the cross-question consistency check in §6 and
  fixes contradictions before the gates. A per-question answer that
  contradicts another is a defect (dispatch §2 covers).
- **MD-6 Evidence standard.** Every option's evaluation cites (a)
  re-verified code seams (finding IDs), (b) the binding constraint(s)
  it must respect (D-W*, WI-J §4.5 invariant, AGENTS §2/§5), and
  (c) concrete consequences for the implementation chunking (framing
  §6 is advisory — the round may re-chunk in its consequences, not
  re-scope). Recommendations are decision-grade: options, trade-offs,
  one recommendation, consequences.
- **MD-7 No-code discipline.** Zero production code, zero tracker
  edits, no scratch files in the repo. Rust-shaped vocabulary sketches
  (enum variants, struct fields, trait signatures) are allowed **only
  inside the artefact** as design content; they are illustrations, not
  diffs. Out-of-scope ideas go to the backlog-candidates section for
  the user to promote (no autonomous issue creation).
- **MD-8 D-W9 handling.** Q2's keying options are presented
  decision-grade with exactly one recommendation, and the decision
  record row for D-W9 is explicitly marked **open — user rules**.
- **MD-9 ADR 0010 amendment.** The draft amendment is written as full
  replacement text for ADR 0010's "connector port stays vocabulary, not
  a trait" decision (plus its consequences mentioning that stance),
  marked *draft — lands with the trait-promotion implementation chunk*
  (dispatch §4). The remaining ADR 0010 drift items (F10-03/F10-04/
  F15-18 amendments) are noted as adjacent backlog candidates only —
  not this round's edits.
- **MD-10 Snapshot & migration method (Q9).** The migration design is
  written as a SQL-level sketch against 0001's conventions plus the two
  reusable 0005 precedents (epoch-default columns keeping pre-migration
  rows' merge behavior bit-for-bit; in-migration re-tag with decode
  rejecting the legacy tag), with the `insta` regeneration planned as
  one deliberate pass inside the wave's first implementation chunk —
  never as drifting bitrot (framing §8). D-W6 policy applied: smooth
  non-breaking preferred; any breaking step argued on concrete benefit.

## 3. Anchor re-verification table (plan day @ `62dd839`)

Stage 2 re-locates each row (MD-1) and cites **its own** numbers.
"Drift" = difference vs the framing plan's `878bb12` anchors / the
audit's `v0.1.0` anchors; the finding ID is the stable key.

| Finding / source | Seam (crate path abbreviated: domain=`crates/taskboard-domain/src`, state, sync, app, ui likewise) | Plan-day lines | Drift |
|---|---|---|---|
| F16-25 | domain/state.rs entity maps `boards/stacks/tasks/labels/reminders` | 27–37 | ≈ framing 24–43, holds |
| F3-04/F16-22 | domain/state.rs `sync: SyncStatus` / `last_updated` / `SyncStatus` struct | 39 / 42 / 47 | exact |
| F16-22 | state/core.rs `sync_status()` getter / `mark_syncing` / setter cluster | 72 / 113 / re-verify cluster | exact (cluster re-check) |
| F16-22 | storage migrations/0001_init.sql `sync_status` table `CHECK (id = 1)` + seed | 102–109 | exact |
| F3-04/F16-23 | domain/outbox.rs op enum; `UpdateBoard(BoardId)` / `DeleteBoard` | enum 22–62; 51 / 53 | framing cited file only |
| F16-06 | domain/command.rs `live_board()` resolver + **nine** call sites | 134; 364/465/575/603/620/637/656/754/824 | exact, all nine |
| F8-06 | domain/command.rs `CommandError` single-board variants `NoBoard`/`BoardExists`/`BoardAlreadyBound(RemoteBoardId)` | 77 / 80 / 84 | framing said `:56` — drift +21/+24/+28 |
| F8-06 | app/commands/mod.rs mirrored variants `BoardAlreadyBound`/`NoBoard`/`LocalBoardExists` | 108 / 149 / 179 | audit said 108/149/178 — +1 on the last |
| #334 | domain/messages.rs `SetBoardColor`/`SetBoardArchived`/`DeleteBoard`/`ResetBoard` | 142 / 149 / 158 / 164 | exact |
| #334 | domain/entities.rs `BoardClocks` | 95 | exact |
| F16-25 (trait precedent) | domain/persistence.rs `trait SyncStateReader` (BoxFuture port pattern) | 68 | exact |
| F16-05 | app/bootstrap.rs `connectors: Mutex<Vec<SyncActor>>` / `.first()` anonymous pick / `ensure_vacant` call / `AttachError::AlreadyAttached` | 141 / 181 / 266 / 431 | framing said 266/**443** — drift −12 on the variant |
| F16-05/F6-17 | sync/actor.rs resume pull-target block (persisted binding → first bound board) | ~328–334 + boards iteration ~357–360 | framing said 327–333/362 — re-pin precisely |
| F16-24 | app/commands/boards.rs `destruct_on` ceremony + receipts | 233–299, receipts 152–156/272/283 | exact (audit's 274–308/410–425 @ v0.1.0 = select/refuse/adopt sites — re-pin) |
| F7-05 | ui/presentation.rs `live_board()` helper (first live non-archived) | 248–251 | framing said 250–254 — small drift |
| F7-05 | ui/view_state.rs consumer | 198 | exact |
| F7-05 | ui/models.rs `IdTables` struct / builder | 35 / 108 | exact |
| Q8 seams | ui/actions.rs `ActionId` / `ActionEntry` / `StackCreate` action (registry precedent); ui/keymap/{mod,presets,stroke}.rs; ui/ui/compositions/board-layout.slint | 92 / 177 / ~107, 296–307 | new anchors (this round's) |
| Q3/Q4 seams | domain/entities.rs `Board.remote: Option<RemoteBoardId>` / `remote_seen`; domain/ids.rs `RemoteBoardId` | 124 / 136 | new anchors |
| Q6 seams | app/config.rs `[sync] backend` scalar + `sync_backend()` resolution; app/session_ceremonies.rs `--switch-backend` rewrite | re-verify (audit F16-15: config.rs:159-177/526-535 @ v0.1.0) | to re-pin |
| Q7 seams | sync/actor.rs `SystemEvent::NetworkLost/Restored` emission; app/bootstrap.rs system broadcast wiring; ui banner consumer | re-verify by grep | to re-pin |

Also re-pin at execution: storage repo.rs status persistence sites
(audit @ v0.1.0: repo.rs:496/953), sync/actor.rs status-report region
(audit F16-05: actor.rs:585-604 @ v0.1.0; today's `SyncReport`
sends sit near 344/455), CLI `status`/`sync` verbs' singleton reads
(app/commands/sync.rs), and ADR 0010's vocabulary-stance lines
(ADR 0010 "The connector port stays vocabulary, not a trait" — near
lines 140–145 of the .org).

## 4. Per-question investigation method

### Q1 — Board parameter shape (F16-06 + F16-23, one motion)

- **Re-verify**: all anchors of rows F16-06, F8-06 (both crates),
  F3-04/F16-23 (outbox), plus `messages.rs` `SyncCommand`/`SyncReport`
  payloads (framing §10 cites messages.rs:272/292) and the CLI/REPL
  board-verb call paths in app/commands/boards.rs.
- **Option space** (framing Q1's own fork, elaborated for evaluation):
  1. *Selected-board state + explicit override*: an AppState (or
     session) "current board" the engine resolves when a command
     omits the parameter; explicit per-command override field.
  2. *Fully explicit scoping*: every board-scoped `StateCommand`
     carries `board: BoardId`; callers (CLI/REPL/UI) resolve any
     selection themselves; no engine-side implicit resolution.
  3. *Hybrid*: explicit command parameter + a UI/CLI-layer
     "selected board" concept that never enters the domain.
  Sub-decisions the chosen shape must also fix: what replaces
  `CreateBoard`'s single-live-board refusal (`BoardExists` at
  command.rs:575–576); what replaces first-live-board implicit
  selection at the two non-engine consumers (sync resume actor.rs
  ~328–334; UI presentation.rs:248–251/view_state.rs:198).
- **Evidence each option needs**: a per-option mapping of **all nine**
  `live_board` sites to their post-change shape (the half-run trap —
  dispatch acceptance (b): the chosen shape shown applied to all four
  vocabularies: commands, outbox ops, `CommandError` variants, status
  payloads); compiler-forced propagation analysis (which F8-06 variants
  widen mechanically, which new refusal variants appear — e.g. the
  multi-binding seed); caller-side burden for CLI/REPL (flags/verbs)
  and UI (Q8 interplay); snapshot-churn scale; migration impact if any
  (outbox ops are already board-addressed — verify none needed);
  topology note (commands are engine-internal; no channel change).

### Q2 — SyncStatus keying + the F16-22 three-layer widening (D-W9: user rules)

- **Re-verify**: state.rs:39/42/47 rows; core.rs setter cluster;
  0001_init.sql:102–109; repo.rs persistence sites; the three consumer
  surfaces (UI banner, CLI status verb, sync-actor report writes).
- **Option space**: (a) per-board status with connector attribution;
  (b) per-connector status; (c) (connector, board) pair keying. Each
  option states where the global `last_updated` lands (F3-04 names
  both).
- **Evidence each option needs**: the read/write diff at each of the
  three layers (domain struct, engine mutators, storage table) — an
  option whose engine-mutator diff explodes the status setter cluster
  loses points; the table-rebuild migration shape it implies (feeds
  Q9); how D-W7's partial-connectivity semantics project onto each
  keying (feeds Q7); actor report shapes (`SyncReport::Completed` etc.
  — what attribution they must carry); UI banner/CLI status rendering
  under N; snapshot regeneration scale. **MD-8**: recommendation only;
  decision record row stays open for the user.

### Q3 — Connector addressing home (F16-05)

- **Re-verify**: bootstrap.rs:141/181/266/431 row; sync/actor.rs resume
  block; the engine-side fan-out registry (`sync_outs: Vec<…>`, state
  crate actor.rs); `SyncCommand::SetBoard`'s untargeted shape
  (connect-flow §4.5 names it); `Board.remote`/`RemoteBoardId` rows.
- **Option space**: (a) routing tag on every `SyncCommand`; (b) app-layer
  registry map + per-connector command channels (routing table
  board→connector, engine fan-out unchanged, app routes); (c) enveloped
  per-connector commands (connector id on an envelope the app
  unwraps). Plus: where connector ids are **minted** (config key vs
  minter) and **typed** (domain newtype vs app-local String).
- **Evidence each option needs**: how `App.connectors.first()`
  (bootstrap.rs:181) and the resume-path pick become named under it;
  how a board's owning connector is derived (`Board.remote` tag — the
  connect-flow "board-ownership tags" deferred item); strict
  additivity proof against §4.2's topology (an option that changes the
  fan-out/fan-in shape is wrong, full stop); interaction with the Q4
  trait's identity model; CLI verb implications (Q6); message-diff
  size across the four vocabularies (ties to Q1's one-motion rule).

### Q4 — Trait promotion shape (D-W4, F16-08)

- **Re-verify**: persistence.rs:68 (the BoxFuture port pattern
  precedent); the full `SyncCommand`/`SyncReport`/`SyncStateReader`
  vocabulary surface (messages.rs, persistence.rs, actor.rs imports);
  `spawn_sync_actor`'s signature and call sites (bootstrap.rs);
  ADR 0010's vocabulary-stance paragraph (MD-9's rewrite target);
  AGENTS §2 crate-boundary rules; `cargo deny` boundary configs
  (existing deny tomls) — the exit criterion "boundary gates stay
  green".
- **Option space**: trait surface — (a) one port trait owning
  commands+reports+state-reader+spawn contract; (b) split ports
  (vocabulary trait vs spawn/connector-factory trait) mirroring how
  `SyncStateReader` already stands alone; home — (c) domain crate via
  the BoxFuture pattern vs (d) a new tiny `taskboard-connector-port`
  crate; dispatch model — object-safe `dyn` vs generic; what
  taskboard-sync-nextcloud implements vs what the app spawns against
  (today's `spawn_sync_actor` free function vs a factory on the port).
- **Evidence each option needs**: AGENTS §2 purity compliance (zero
  network/UI/async-runtime deps — the BoxFuture pattern's
  crate-freedom must be re-verified, not assumed); narrow-seam
  discipline (no discovery, no dynamic loading, no plugin registry —
  the framing §8 scope-creep guardrail); the concrete N=2 exit
  criterion path (how a second connector would implement/spawn under
  it); the draft ADR 0010 amendment text (MD-9); test-fake story
  (handwritten fakes over the port, AGENTS §5).

### Q5 — Ceremony redesign (F16-24)

- **Re-verify**: commands/boards.rs select/refuse/adopt sites
  (re-pinned from audit's 274–308/410–425) + destruct_on row; the
  `BoardAlreadyBound(RemoteBoardId)` seed (command.rs:84, app mod.rs:108);
  the stale-binding re-resolve landed by remediation chunk 5 (locate
  via pipeline.rs re-resolve sites — the M-1 fix); outbox `DeleteBoard`
  tombstone-finalizing push; #179/#170 issue text (read-only, `gh issue
  view` — no tracker edits).
- **Option space**: multi-binding refusal vocabulary (widen
  `BoardAlreadyBound` vs new dedicated variants list); unbind (#179)
  ceremony shape (engine command + outbox consequence + CLI verb);
  title-match attach (#170) vs keeping ADR 0010's identity-based
  attach with the union caveat documented; reset/delete of one board
  among N — generalising `destruct_on`'s typed refusal + pre-count
  receipt to a named target; adopt/resume at N boards (which actor
  resumes which board — ties to Q3 routing).
- **Evidence each option needs**: typed-outcome discipline (AGENTS §5
  — refusal outcomes as variants, never text); loud-flag consistency
  with `connect --switch-backend`/`--force` precedent; tombstone/push
  interplay at N connectors (which connector finalizes a tombstone);
  composition with the stale-binding re-resolve; ceremony UX receipts
  (pre-count semantics at N).

### Q6 — Config shape

- **Re-verify**: app/config.rs `[sync]` scalar resolution
  (`sync_backend()`), validation loudness patterns; session_ceremonies'
  `--switch-backend` config rewrite; figment's array-of-tables +
  strictness behavior (verified against the pinned figment version's
  docs/behavior — a probe sketch in the artefact, no repo code).
- **Option space**: `[[sync.connectors]]` array-of-tables with named
  entries; the legacy-scalar resolution rule variants (scalar aliases
  the default/first connector vs scalar synthesizes a named entry at
  load vs scalar rejected once any array exists); validation loudness
  (which mixes are rejected with typed errors vs tolerated).
- **Evidence each option needs**: `connect --switch-backend`'s write
  path stays valid under each rule (round-trip story); D-W6 additive
  compat (a v0.1.x single-board config keeps working — exit
  criterion); Q3 interplay (the connector name/id minting source);
  figment strictness facts (cited, reproducible); validation-error
  typing (app error taxonomy).

### Q7 — Network authority mechanics (D-W7 decided; mechanics open)

- **Re-verify**: `SystemEvent::NetworkLost/Restored` emission sites in
  sync/actor.rs; the system broadcast wiring in bootstrap.rs; the UI
  banner consumer (presentation.rs/status-bar.slint); how the actor
  currently decides down/restored (transport classification).
- **Option space**: (a) each actor emits its own per-connector state
  and an app-layer aggregator computes all-down and emits the system
  event; (b) actors keep emitting NetworkLost/Restored and a single
  authority reconciles/suppresses (subscription-side aggregation);
  (c) the engine aggregates from report attribution (rides Q2's
  keying). UI banner semantics: per-connector detail vs single
  all-down banner vs both.
- **Evidence each option needs**: D-W7 conformance (all-down ⇒
  NetworkLost; any responding ⇒ healthy — an option that emits
  NetworkLost while one connector responds is wrong); no new channels
  (broadcast reuse; topology invariant); Q2 keying interplay; kiosk
  banner behavior under partial connectivity (R9 degradation); race
  behavior on flapping connectors (hysteresis needs, if any —
  evidence from the existing backoff/classification code).

### Q8 — UI board dimension (F7-05; R4/R1/R9)

- **Re-verify**: the F7-05 rows (presentation.rs, view_state.rs:198,
  models.rs IdTables); actions.rs registry shape (`ActionId`/`ActionEntry`,
  the `StackCreate` entry as the structure-editing precedent); keymap
  module structure; board-layout.slint + status-bar/toolbar
  compositions; the GUI requirements R1/R4/R9 texts (§2 of the GUI
  plan); how the stack switcher currently switches (the seam to
  mirror).
- **Option space**: `ViewState.board: BoardId` + board switcher as an
  action-layer action (mirroring the stack switcher) vs a view-tree
  navigation; where selection lives (pure UI state vs engine-visible
  selected-board — **must** reconcile with Q1's chosen shape; a Q1
  "selected-board in AppState" makes this trivial, a Q1
  "fully-explicit" makes it UI-local — the design must state which and
  why coherently); IdTables re-keying per board vs per-view-instance;
  board-bar composition under R1 component discipline; kiosk-surface
  degradation under R9 (does the kiosk board view gain switching at
  all, or is it fixed-board by config).
- **Evidence each option needs**: **concrete** actions-registry/keymap
  integration points (dispatch acceptance (e): named action ids,
  context ids, keymap entries — e.g. where `board.next`/`board.select`
  would register); the keyboard-journey design (R4's acceptance bar
  names "navigate boards" a core verb — the scripted journey's board
  leg); dumb-client discipline (AGENTS §1: no business logic in UI —
  switching must not mutate engine state unless via commands); R1
  component reuse (board-bar from existing compositions); snapshot/
  harness implications.

### Q9 — Migration & compat plan (D-W6)

- **Re-verify**: 0001_init.sql conventions (style, naming, seed
  pattern — the whole file skimmed, not just :102–109); 0005's two
  precedents in full (epoch-default columns; in-migration re-tag +
  decode rejecting the legacy tag — find the decode-side code);
  0003's backfill pattern; the insta snapshot corpus (state crate
  tests/snapshots) and what widening `AppState` rewrites.
- **Option space**: per-Q2-option migration variants (each keying
  choice implies its table shape — Q9 presents the rebuild for each,
  so the user's D-W9 ruling has its migration priced); the config
  additive path (per Q6); snapshot regeneration strategy (one
  deliberate regeneration pass in wave chunk 1 — standing plan — vs
  staged); any breaking change candidates and their concrete-benefit
  argument (D-W6's only door for breaking).
- **Evidence each option needs**: conformance checklist against
  0001/0005 conventions (column naming, defaults, seed handling,
  transactional shape); pre-migration row behavior (bit-for-bit merge
  behavior where the 0005 precedent applies — status rows' semantics
  under rebuild); decode-rejection reuse for status/op re-shaping; the
  additive v0.1.x upgrade test design (exit criterion); rollback/abort
  story if any.

## 5. Deliverable outline (for stage 2)

```markdown
# [Plan] Multi-board Wave — Detailed Design (Stage-0 round)
- header: Date / Kind / Origin (framing §5, dispatch §2) / Status PROPOSED
1. Mission & binding constraints (recap; D-W1..D-W8 table pointer; the
   topology-invariant acceptance test stated once, prominently)
2. Verified baseline (anchor table @ the worktree's base SHA, drift by
   finding ID, locating-command classes — MD-1/MD-2)
3. Q1 .. Q9 (one section each, per MD-3's internal structure)
4. One-design coherence map (cross-question vocabulary table: the four
   vocabularies × the recommended shapes; the selected-board/identity
   story across Q1/Q3/Q8; the status flow across Q2/Q7)
5. Decision record (PROPOSED; one row per Q + D-W9 marked open-for-user)
6. Draft ADR 0010 amendment (full text, marked draft per MD-9)
7. Backlog candidates (out-of-scope ideas, user promotes)
8. References (inputs list with permalinks)
```

Estimated size: ~800–1000 lines (house scale **L**, matching the
chunk's dispatch size).

## 6. Coherence pass checklist (MD-5; stage 2 runs last)

1. Q1's shape appears in all four vocabularies (commands, outbox ops,
   `CommandError` variants, status payloads) — no half-run.
2. Q1's selection story × Q8's switcher: one selected-board home, not
   two competing ones.
3. Q3's connector identity × Q4's trait identity model × Q6's config
   name minting: one identity story end-to-end.
4. Q2's keying × Q7's aggregation × Q8's banner: status flows named
   consistently (no "per-connector" in one section and "pair-keyed" in
   another for the same read).
5. Q5's ceremonies use Q1's error vocabulary and Q3's routing; the
   `destruct_on` generalisation keeps the loud-flag pattern.
6. Q9's migration prices every Q2 option the user will choose among.
7. Every section's topology note is additive-on-the-seams or
   engine-untouched; anything touching connect-flow §4.2's shape is a
   redesign of something else (the invariant's own wording).
8. D-W1…D-W8 respected verbatim; D-W9 posed, not ruled (MD-8).

## 7. Risks

- **Anchor drift in flight** (remediation-wave-1 tail landing while
  this round runs): mitigated by MD-1 (re-verify at execution start,
  drift by finding ID). Plan-day drift is already nonzero (§3).
- **Half-run Q1**: the four-vocabulary trap; mitigated by §6 item 1
  and the coherence map section.
- **D-W9 self-ruling**: mitigated by MD-8 + §6 item 8; review's trap
  (b) backs it.
- **Trait scope creep** (plugin-flavoured surface): mitigated by Q4's
  evidence list (narrow-seam discipline as an explicit check) and
  framing §8.
- **Topology re-opening**: mitigated by MD-3's per-question
  conformance note + §6 item 7.
- **Option-space bias**: §4's spaces are the framing plan's own
  forks; MD-4 allows additions only as recorded deviations.
- **Artefact bloat / design-as-implementation**: mitigated by MD-7
  and the ~800–1000-line target with the §5 outline fixed in advance.
- **#157 amplification unpriced** (framing §8): Q2/Q3/Q7 evaluations
  must each state the per-connector cycle-iteration consequence or the
  recorded guardrail — added to MD-6's consequence requirement.

## 8. Deviations from the dispatch prompts

None. The precondition passed (framing plan on `main` via PR #333's
commits `d5ee1db`/`16b4b3a`); the worktree and branch were created as
prompted; no design content, no push.

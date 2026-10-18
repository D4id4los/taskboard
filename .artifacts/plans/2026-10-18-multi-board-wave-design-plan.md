# [Plan] Multi-board Wave — Design Round & Scope (framing)

- **Date**: 2026-10-18
- **Kind**: design-round framing plan for the `wave-multi-board`
  milestone. It records the wave scope and the wave-level decisions
  accepted at the user review (2026-10-18, §5), states the Stage-0
  design questions the round must settle (§6), and sketches the
  implementation chunking that the round's output feeds (§7). It is
  not itself the detailed design; the round's design doc supersedes
  it question by question, then the wave implementation plan and
  dispatch plan follow the kiosk-wave pattern.
- **Origin**: user request 2026-10-18 (prepare the wave-multi-board
  milestone while remediation-wave-1's last two chunks are in flight),
  plus: the v0.1.0 audit findings report
  `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`
  (fold map: multi-board ← F3-04, F7-05, F16-06, F16-22, F16-23,
  F16-24, +P3 F16-07/25; multi-connector ← F16-05, F16-08), the
  milestone issues #125/#187/#298, the future-directions analysis
  report §3.5/§4-D2/§6 (Wave 4), the GUI design requirements plan
  (R4/R9/§3), the connect-flow design plan §4.5 (the deferred
  multi-connector contract), and growth-roadmap Phase 6.
- **Status**: PROPOSED — scope and wave-level decisions **accepted**
  (§5); Stage-0 design questions (§6) open. All code anchors verified
  on `main` @ `47b2cba` (post remediation chunks 1/11; chunk 15 is in
  flight and §4 notes what it changes).

---

## 1. Mission

Lift the two standing structural invariants — one live board, one
active connector — so N boards sync across N simultaneously active
connectors with per-board routing, **without re-opening the WI-J
channel topology** (its §4.5 invariant is this wave's acceptance
test: every change lands additively on the fan-out/fan-in seams; a
proposal that needs to change §4.2's topology has misdesigned
something else).

## 2. Scope (user-confirmed 2026-10-18)

**In** — future-directions D2 stages 1–2 only:

- Multi-board engine semantics (board parameter on the command
  surface, single-live-board refusal lifted, ceremonies at N).
- Connector registry in bootstrap + per-board routing table.
- **Fully implemented multi-connector** (D-W3): two connectors live
  at once is an exit criterion, not a seam verified on paper.
- **Connector trait promotion** (D-W4): the ADR 0010 vocabulary
  becomes a real port; ADR 0010 amended.
- UI board dimension incl. the desktop board switcher (D-W5).
- Config `[[sync.connectors]]` shape + CLI/REPL verb growth.
- The F16-22 SyncStatus three-layer widening — first chunk (standing
  decision, 2026-10-17 P1-1).

**Out** (later waves, do not pre-build): Projects hierarchy above
boards; multi-user team decks + time attribution; the org-file
connector; the D5 app-data substrate *implementation* (reminder-state
sync, settings sync) — only its shared decision point F16-08 touches
this wave and it is now decided (trait promotion, D-W4); LAN sync
server; CRDT collaboration.

## 3. Why the data plane is ready and the control plane is not

Verified on `main` @ `47b2cba` (audit chunk 16 W2/W8, re-checked):

- **Ready by design** (F16-25, F16-07): `AppState` entity maps
  (`state.rs:25-33`), the outbox `board_id` column (migration 0003),
  per-board validator keys, engine fan-out registry + permanent
  fan-in reports inbox (connect-flow §4.2, ADR 0014),
  `App.connectors: Mutex<Vec<SyncActor>>` with the ≤1 guard purely
  app-layer (`bootstrap.rs:141`, `ensure_vacant` →
  `AttachError::AlreadyAttached` at `bootstrap.rs:254-266`),
  `CredentialStore` keyed per-(server, username). Hierarchy and
  user-identity shapes are absent everywhere — clean slate (F16-07).
- **The singletons to widen** (F3-04, F16-22): `AppState.sync:
  SyncStatus` (`state.rs:47-56`) and the global `last_updated`
  (`state.rs:52-54`); engine mutators (`core.rs:72/113` and the
  status setter cluster); storage `sync_status` table with
  `CHECK (id = 1)` (`0001_init.sql:103`) — the wave's **only
  non-additive schema migration** (table rebuild; the additive lane
  does not cover it).
- **The implicit "the bound board"** (F16-06): `live_board` resolves
  it silently at five command sites (`command.rs:134`, used at
  `:364/:465/:575/:598/:615`); the sync actor's resume path picks the
  first bound board (`actor.rs:327-333`); the UI takes the first
  live non-archived board (`presentation.rs:248-252`,
  `view_state.rs:177-193`).
- **Compiler-forced extension points** (F8-06): `CommandError`'s
  single-board variants (`command.rs:56`, board-refusing arms) widen
  mechanically once the vocabulary decision is made.
- **Trait-promotion precedent**: `SyncStateReader` is already a
  domain trait (`persistence.rs:68`, `BoxFuture` port pattern) —
  promoting the connector vocabulary follows an established shape
  and respects AGENTS §2 domain purity.
- **In flight**: remediation-wave-1 chunk 15
  (`feat/board-stack-structure-editing`, #132/#171/#177/#304) lands
  `Board` clocks, board-edit verbs, and the reset/delete ceremony —
  exactly the board-lifecycle machinery this wave generalises to N.
  **The design round must read that branch's shapes before
  finalising** (user will signal when it reaches review).

## 4. Wave-level decision record (user review, 2026-10-18)

| ID | Decision |
|---|---|
| D-W1 | Milestone scope = D2 stages 1–2 (§2). |
| D-W2 | Tracker: #244's milestone **cleared** (the per-field lockstep seam belongs to the future field-family wave); **#179** (board unbind) and **#170** (title-match attach) **pulled into** `wave-multi-board`. Applied tracker-side with this plan. |
| D-W3 | Multi-connector is **fully implemented in-wave** — exit criteria include two connectors live simultaneously. |
| D-W4 | **Connector trait promoted now** (F16-08 resolved): the `SyncCommand`/`SyncReport`/`SyncStateReader` vocabulary becomes a real port; recorded as an ADR 0010 amendment. |
| D-W5 | **UI is in-wave**: the desktop board switcher and the `ViewState` board dimension ship in this wave. |
| D-W6 | Migration policy: **prefer a smooth non-breaking migration** (v0.1.x single-board config + DB keep working additively); breaking changes are acceptable **only with a concrete, clear benefit** shown in the design round — the app has no deployments yet. |
| D-W7 | Network authority at N connectors: **all connectors down ⇒ `NetworkLost`; any responding ⇒ healthy**. One offline server while others respond is normal operation, not an outage signal. |
| D-W8 | The Stage-0 design round **starts now**, in parallel with remediation-wave-1's tail (docs-only, no code conflicts; precedent: the parallel-during-gui dispatch round). Implementation dispatch waits for remediation-wave-1 to close and chunk 15 to merge. |
| D-W9 | SyncStatus keying (per-board vs per-connector vs pair): **open** — the user awaits the design round's detailed proposal before ruling. |

## 5. Stage-0 design questions (the round's work)

Each question carries its evidence anchors and any binding
constraints from §4/§5. The round resolves each with one recorded
decision and a recommendation; D-W9's answer goes back to the user.

1. **Board parameter shape** (F16-06 + F16-23, one motion): how the
   five `live_board` sites become an explicit board parameter —
   selected-board state + explicit override vs fully explicit
   scoping; what replaces `CreateBoard`'s single-live-board refusal
   and the first-live-board implicit selection; the chosen shape
   applied to **all four vocabularies** (commands, outbox ops,
   `CommandError` variants, status payloads).
2. **SyncStatus keying** (F16-22, the P1): per-board status with
   connector attribution vs per-connector vs (connector, board)
   pair; decides the one table-rebuild migration; the global
   `last_updated` rides the same decision (F3-04 names both).
3. **Connector addressing home** (F16-05): routing tag on commands
   vs registry map; where connector ids are minted and typed; how
   the resume path (`actor.rs:327-333`) and `App.connectors.first()`
   (`bootstrap.rs:181`) stop being "the anonymous one". Interacts
   with the trait's identity model (D-W4).
4. **Trait promotion shape** (D-W4, F16-08): the port's surface
   (commands/reports/state-reader + spawn contract), its home
   (domain purity per AGENTS §2 — the `BoxFuture` port pattern),
   what `taskboard-sync-nextcloud` implements vs what the app
   spawns against; the ADR 0010 amendment text. Keep it the narrow
   vocabulary seam ADR 0010 anticipated, not a plugin system.
5. **Ceremony redesign** (F16-24): select/refuse/adopt/resume at N
   boards and multi-binding; unbind (#179); title-match attach
   (#170); reset/delete of one board among N (generalising chunk
   15's ceremony); interplay with the stale-binding re-resolve that
   remediation chunk 5 landed.
6. **Config shape**: `[[sync.connectors]]` array-of-tables + the
   legacy `[sync] backend` scalar resolution rule (scalar = the
   default/first connector, so `connect --switch-backend`'s writes
   stay valid); figment strictness and validation loudness.
7. **Network authority mechanics** (D-W7): who aggregates all-down
   across N actors and emits `NetworkLost`/`Restored`; UI banner
   semantics under partial connectivity.
8. **UI board dimension** (F7-05): `ViewState.board: BoardId` +
   board-switcher seam mirroring the stack switcher; the switcher as
   an action-layer action with a keyboard journey (R4's acceptance
   bar names "navigate boards" a core verb); board-bar composition
   under R1 component discipline; IdTables keyed per board
   (`models.rs:35-45/109-128`); kiosk-surface degradation (R9).
9. **Migration & compat plan** (D-W6 applied): the DB rebuild
   written against 0001's conventions; the config additive path; the
   `insta` snapshot regeneration strategy for the widened
   `AppState` (deliberate regeneration, never bitrot); any breaking
   change argued on concrete benefit.

## 6. Implementation chunk sketch (post-acceptance input)

Sizes use the house scale. This sketch feeds the wave implementation
plan + dispatch plan written after the design round; the round may
re-chunk, not re-scope.

| # | Chunk | Covers (issues · findings) | Size |
|---|---|---|---|
| 1 | SyncStatus three-layer widening + table-rebuild migration | #298 · F16-22/F3-04 — **first, alone** (standing decision P1-1) | M |
| 2 | Domain vocabulary: board-scoped commands/outbox/errors/status payloads + the connector trait port | F16-23/F16-06/F8-06 + D-W4 | L |
| 3 | Engine: single-live-board lift + N-board ceremonies (unbind #179, title-match #170) | F16-24 | M |
| 4 | Sync crate: implement the trait, per-connector routing keys, explicit resume target, `Board.remote` connector tag | F16-05 + connect-flow §4.5 routing/ownership | M |
| 5 | App/config/bootstrap: `[[sync.connectors]]`, guard lift, N-actor spawn, board→connector routing table, all-down authority | §4.5 config/authority + D-W7 | L |
| 6 | CLI/REPL verb growth (`connect <name>/list/remove`, multi-board `boards` verbs, per-connector `status`) | §4.5 verb growth | M |
| 7 | UI board dimension + keyboard-first board switcher | F7-05 + R4/R1/R9 | M |
| 8 | Docs: ADR 0010 amendment, architecture.org, README, CHANGELOG | wave bookkeeping | S |

Dependency spine: 1 alone → 2 → 3 → 4 → 5 → 6/7 (parallel after
their deps) → 8. Everything rebases onto chunk 1.

**Exit criteria (draft for the round to refine)**: two connectors
configured and live simultaneously against two dockerized Nextcloud
instances (tier-2); multiple boards across them syncing; one board
moved between connectors; a v0.1.x single-board config + DB migrates
additively and keeps working; board switching operable by a scripted
keyboard journey alone; `cargo deny` boundary gates stay green with
the new trait port.

## 7. Sequencing

1. **Now**: this framing plan's PR (docs-only, parallel-safe with
   the remediation tail).
2. **Stage-0 design round**: next round of work — dispatched or
   session-run per the user's call; must incorporate the landed
   shapes of `feat/board-stack-structure-editing` (user signals when
   it reaches review) and re-verify anchors (remediation chunks
   landing meanwhile may shift line numbers; the findings IDs are
   the stable keys).
3. **Then**: wave implementation plan (kiosk-exemplar style) →
   dispatch plan (template) → chunks, after remediation-wave-1
   closes.

## 8. Risks

- **#157 amplification**: per-card pull refresh is already
  O(bound cards) per cycle; N boards per connector multiplies it.
  The round must size per-connector cycle iteration and either
  keeps it sane or records the guardrail (the issue itself stays
  unscheduled perf work).
- **Snapshot churn**: widening `AppState` rewrites `insta` snapshots
  wholesale — plan it as one deliberate regeneration inside chunk 1,
  never as drifting bitrot across chunks.
- **Trait scope creep**: D-W4 promotes the narrow vocabulary seam;
  anything plugin-flavoured (discovery, dynamic loading) is out.
- **Mid-wave schema migration**: the F16-22 rebuild is the one
  non-additive step — first chunk by standing decision so nothing
  stacks on the singleton it removes.
- **Config ambiguity**: the legacy-scalar resolution rule needs
  loudness (reject unknown mixes, never silently pick).

## 9. Tracker bookkeeping (applied 2026-10-18)

- #244 milestone **cleared** (stays open, unscheduled; the
  field-family wave owns it).
- #179, #170 **added** to `wave-multi-board`.
- Milestone now: #125, #170, #179, #187, #298. No issues closed.

## 10. References

- Audit fold map + evidence: findings report §fold-map,
  `2026-10-17-v0.1.0-audit-findings-report.md` (F16-05/06/07/08/22/
  23/24/25, F3-04, F7-05, F8-06; agenda P1-1).
- Multi-connector contract: connect-flow design plan §4.5
  (`2026-10-12-connect-flow-design-plan.md`), ADR 0014.
- Wave framing: future-directions analysis report §3.5/§4-D2/§6
  (`2026-10-14-future-directions-analysis-report.md`).
- UI constraints: GUI design requirements plan R1/R4/R9/§3
  (`2026-10-14-gui-design-requirements-plan.md`).
- Pattern exemplars: `2026-10-16-kiosk-daemon-gui-control-plan.md`
  (design round + decision record), `2026-10-16-kiosk-mode-wave1-plan.md`
  (wave implementation plan), `.artifacts/templates/dispatch-plan.md`.
- Code anchors @ `47b2cba`: `state.rs:25-56`, `core.rs:72/113`,
  `0001_init.sql:103`, `command.rs:56/134/364/465/575/598/615`,
  `bootstrap.rs:141/181/254-266`, `actor.rs:327-333`,
  `presentation.rs:248-252`, `view_state.rs:177-193`,
  `messages.rs:247/267`, `persistence.rs:68`.

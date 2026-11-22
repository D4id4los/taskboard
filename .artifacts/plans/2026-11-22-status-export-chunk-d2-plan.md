# Chunk D2 — status/export polish — implementation plan

- **Round**: backlog-reduction round 5, chunk D2 (scope report
  `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  §D, Status/export polish group — normative; dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §1/§8, chunk D2 with traps (a)–(c)).
- **Branch**: `feat/status-export-polish` (worktree
  `../taskboard-WT-feat-status-export-polish` per AGENTS §8).
- **Journey plans** (the Origin this chunk implements — stage 2 opens
  only on "GATE CLEARED: the operator accepted the journeys"):
  - `.artifacts/plans/2026-11-22-status-export-journeys-command-ux-plan.md`
    (#1147 command lane + #1148 live refresh)
  - `.artifacts/plans/2026-11-22-status-export-journeys-preview-ux-plan.md`
    (#1149 export preview + clipboard)
- **Covers**: #1147, #1148, #1149.
- **Acceptance**:
  1. Every P1 story of both journey plans completes end-to-end
     including escapes (the P1-slice rule): US-1/US-2/US-3 of the
     command plan, US-1/US-2 of the preview plan. US-4 of the command
     plan (live refresh) is P2 and governed by D-6's fallback law —
     see Risk R-1 below.
  2. `scripts/ux-coverage.sh <plan> --gate` exits 0 on BOTH plans.
  3. Diffs confined to: `crates/taskboard-ui-slint/`
     (`ui/compositions/status-panel.slint`,
     `ui/compositions/export-dialog.slint`, `src/adapter.rs`,
     `src/requests.rs`, `src/actions.rs`,
     `tests/journeys/status_export_journeys_command_journeys.rs`,
     `tests/journeys/status_export_journeys_preview_journeys.rs`,
     the structural/seam inventories), `crates/taskboard-app/`
     (the remediation + preview/clipboard executors and their
     `it_*` tests), `crates/taskboard-domain/src/undo/tests.rs`
     (the serde pins only).
  4. Round laws: (a) typed outcomes only — never text assertions;
     (d) out-of-scope findings become `backlog:` issues immediately.

## 1. Per-story decisions (deriving the journeys)

### 1.1 The command lane (#1147) — command plan US-1..US-3

- **One request, three actions** (plan D-1): `UiRequest::StatusRemediate`
  with `StatusRemediationAction { KillHolder, ForceUnlock, RestartDaemon }`
  and a typed `StatusRemediationOutcome`. The request/outcome types
  live in `crates/taskboard-ui-slint/src/requests.rs` beside
  `ExportRunOutcome` (the gate's UiRequest token home); the executor
  lives app-side.
- **Staged confirm** (plan D-2): the armed-confirm strip is the
  task-delete precedent's grammar (W-4 is a presentation-only row —
  no dispatch until confirm).
- **Verb semantics** (plan D-3/D-4/D-5): KillHolder = SIGTERM to the
  displayed `holder_pid` (refuse `HolderNotAlive` when stale,
  `SelfPid` when it is this process); ForceUnlock = stale-lock-sidecar
  removal, offered executable only in the stale posture but
  *authorized* by an executor liveness re-check (`HolderAlive`
  refusal — the enable flag is presentation, not authorization);
  RestartDaemon = graceful `daemon stop` over IPC when serving, then
  a detached `taskboard daemon run` spawn, the outcome carrying the
  new pid (`no-daemon` is not a refusal on the stop leg).
- **Kiosk** (plan D-9, dispatch trap (a)): remediation rows render
  disabled with honest captions on kiosk; the structural inventory
  asserts no remediation dispatch is reachable there.
- **Post-fact refresh** (plan D-3): every executed remediation is
  followed by one `StatusQuery` re-dispatch — the outcome posture is
  never stale.
- **Serde pins** (plan D-8, the #993 pattern): hand-written-JSON
  round-trip tests for the action and outcome enums land with the
  types.

### 1.2 Live refresh (#1148) — command plan US-4

- **Watch lane** (plan D-6): one `UiRequest::StatusWatchSubscribe`
  riding the panel's open query; the fold happens OUTSIDE the adapter
  borrow (the OP2 erratum's re-entrancy law). No poll timer, ever.
- **Fallback (binding, the D2 dispatch's sequencing note)**: if E2
  (`fix/state-storage-small-residue` — the #1137 seam generalization)
  has not landed by D2 dispatch, US-4 is NOT implemented: the W-5 row
  and the `StatusWatchSubscribe` delta are removed from the plan
  under a dated supersession note, the round-4 snapshot+explicit-Refresh
  posture stands unchanged, and the deferral is recorded on #1148.
  The row ships removed, never left MISSING at the gate.
- Note: the #1137 watch channel itself is already landed (PR #1173 —
  the storage diagnostics hop). What E2 adds is the generalized
  status seam US-4 consumes; the chunk must verify that seam's
  presence (a `StatusWatch*` receiver exported from the engine side)
  before wiring W-5, not assume it.

### 1.3 Export preview + clipboard (#1149) — preview plan US-1/US-2

- **One generator, two sinks** (plan D-1, dispatch trap (c)): preview
  and clipboard copy drive the same app-side export engine over the
  same `ExportSpec` the Run button sends; the seam test asserts
  payload identity (preview payload == engine export payload on the
  same spec, byte-for-byte).
- **Read-only pane + presentation cap** (plan D-2): the cap is a
  fold-flag fact (`truncated`); file and clipboard payloads are never
  truncated. No size-ceiling assertions (the round law's no-quality-
  ceilings litmus: a better generator keeps every test green).
- **Copy regenerates app-side** (plan D-3): never copies the pane's
  possibly-truncated text; typed `Copied { bytes }` /
  `ClipboardUnavailable`.
- **Family gating** (plan D-5): the controls render on the csv/json
  family pick only — a fold flag, not hidden lanes; no pdf leg
  (round-4 R-5 stands).
- **Serde pins** (plan D-6): `ExportPreviewOutcome` /
  `ExportClipboardOutcome` round-trip pins land with the types.

## 2. Order of work (stage 2)

1. Command-contract first: the four UiRequests
   (`StatusRemediate`, `StatusWatchSubscribe` — conditional on R-1,
   `ExportPreview`, `ExportClipboardCopy`), the outcome enums, and
   the serde pins.
2. The app-side executors (remediation; preview/clipboard) with their
   `it_*` tests (TDD — the executor tests are the red layer).
3. The UI wiring rows W-1..W-6 (command plan) and W-1..W-4 (preview
   plan) + the journey tests, in story order US-1→US-3, then US-4
   (if the seam is present), then the preview stories.
4. The kiosk structural inventory updates (D-9's no-dispatch
   assertion) and the seam inventory (`seam_inventory.rs`) entries
   for every new callback — an unlisted callback is INVENTORY-MISS at
   the gate.
5. Gate runs: `scripts/ux-coverage.sh <both plans> --gate` must exit 0.

## 3. Risks

- **R-1 — the E2 sequencing risk (normative)**: the live-refresh
  story depends on a seam that may not exist at dispatch time. The
  fallback (§1.2) is decided, not open: degrade to the snapshot lane,
  amend the plan, record the deferral on #1148. Never implement a
  private engine-side watch inside this chunk — that is E2's
  territory and a cross-chunk boundary violation.
- **R-2 — process control in tests**: the remediation executor's
  tests must not spawn/kill real processes nondeterministically.
  The executor takes its signal/remove/spawn effects as seams
  (function-point or trait seams) so the typed decision logic is
  tested pure and the effect layer stays thin. Never
  `tokio::time::sleep` in tests (AGENTS §5).
- **R-3 — menu-row budget**: the remediation rows live INSIDE the
  status panel, not in the action menu — no new root quick rows
  (the #1184 saturation and the OP3 erratum's G6 geometry law are
  respected by construction).
- **R-4 — clipboard availability**: headless test environments may
  have no clipboard; the executor's seam law (R-2) covers this — the
  journey tests assert the typed outcome contract, not system
  clipboard side effects.

## 4. Estimated diff size

M (≈600–900 lines including tests): four request types + two
executors + seven/six wiring rows + two journey-test files + serde
pins + inventory updates. No domain-type changes beyond test-module
additions (R-3 of the command plan: the lane never touches domain
state).

## 5. Backlog issues filed during planning

- Kiosk supervisor-managed remediation lane (the command plan's
  non-goal #1) — filed with this round's J2 artefacts.
- CLI remediation verbs (`unlock` / `daemon restart`) symmetry ask
  (the command plan's non-goal #2) — filed with this round's J2
  artefacts.

Filed during planning: #1248 (the kiosk supervisor lane), #1249
(the CLI remediation verbs).

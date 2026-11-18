# [Plan] Overlay Migration — round 9 chunk 5 (`refactor/ui-overlay-migration`)

- **Date**: 2026-11-18
- **Kind**: implementation plan — TDD-first, per-mount migration of the
  ad-hoc overlays/dialogs onto the chunk-3 base widgets
  (`ModalDialogHost`, `AnchoredOverlayHost`), the two anchor-mirror
  race fixes, and the task-details polish (description stretch +
  Effort Estimate always-on row). Stage-1 plan for dispatch-plan §6.
- **Origin**: J2 UX journey plan
  `.artifacts/plans/2026-11-18-widget-framework-ux-plan.md`
  (R-4..R-9 + US-1/US-3 normative); dispatch plan
  `.artifacts/plans/2026-11-18-gui-review-round9-dispatch-plan.md`
  §1/§6 (incl. the 2026-11-18 backlog-reduction riders #881/#880);
  research `.artifacts/reports/2026-11-18-round9-widget-framework-research.md`
  §4–§5 and `.artifacts/reports/2026-11-18-round9-dayreview-details-research.md`
  §2 (root causes with line refs).
- **Status**: READY for stage 2. Chunk-3 precondition verified:
  `feat/ui-widget-framework-base` merged (PR #1005; origin/main
  d52c9b22 carries `ui/widgets/modal-dialog-host.slint`,
  `ui/widgets/anchored-overlay.slint`, `Theme.scrim`, the
  dialog/menu sizing tokens, and the timers.slint click-time
  anchor-mirror proof). Worktree:
  `../taskboard-WT-refactor-ui-overlay-migration`, branch off
  origin/main (`worktree-doctor.sh` exit 0).
- **Line refs caveat**: task-view.slint line numbers below are from
  the branch worktree at d52c9b22; the research reports' refs are
  pre-chunk-3 and may drift by a few lines — the seams are named
  element ids, which are stable.

## 1. Scope — the mount inventory

| # | Mount | Current seam (branch refs) | Target base | Ruling |
|---|---|---|---|---|
| M1 | Due/scheduled editor (the in-flow `DueEditor` swap) | task-view.slint `due-cell`/`if (due-open): DueEditor` (~:650–706, :761–775 both columns) | true overlay on `AnchoredOverlayHost` anchored at the due/scheduled column; **the swap dies** | R-7, R-8 |
| M2 | Date picker (calendar+time panel) | app-window.slint ~:795–840, hand clamp/flip + `DatePickerFacts` mirrors (task-view.slint ~:315–333) | `AnchoredOverlayHost` | R-8, US-2/S1-S2 |
| M3 | Time-field quick-list flyouts ×4 | app-window.slint ~:1235–1315 (`TimeFieldFacts.open && site == N` twins), time-field.slint ~:85–126 mirror loop | one `AnchoredOverlayHost` mount, `site` discriminating the panel contents | US-2/S3, R-8 race 2 |
| M4 | Track-entries embedded date picker | track-entries.slint ~:213 (`if (form-grid-open): DatePicker` inside the form's layout) | window-level `AnchoredOverlayHost` anchored at the form's date field | same-law sibling (research §2.3) |
| M5 | Recur editor | app-window.slint ~:1475–1510, hand scrim/centering | `ModalDialogHost` | US-1/S1, R-4 |
| M6 | Labels overlays (both mounts: task-detail ~:954–1005, board roster ~:1006–1050) | hand backdrop + centered panel | `ModalDialogHost` (both sites share one host wiring; `shows-assignment` stays the site discriminator) | US-1/S1, R-4 |
| M7 | Search palette | app-window.slint ~:1429–1525 (backdrop + centered panel) | `ModalDialogHost`, no buttons, focus-trapped (walk-stops = row count + query field) | R-5 |
| M8 | Description editor vertical-stretch fix | task-view.slint edit mount ~:1297–1315 (`HorizontalLayout` with `vertical-stretch: 1` but the `MultiLineEditor` carries none) | in-place fix (no base widget — it is the inline-editor family, J2 §2 law) | US-3/S1 |
| M9 | Effort Estimate always-on one-line row | task-view.slint ~:891–936 (caption + conditional echo/edit swap) | caption "Effort Estimate" + always-mounted `TextField` on one line; commit on focus loss (R-9); the swap machinery (`effort-open`, `effort-edit-opened`, `effort-cancelled`, `effort-focus-lost`) deleted | US-3/S2, R-9 |

**Non-goals** (dispatch §6): day-review (chunk 7), habit editor
(chunk 10), LabeledValueRow generalization (chunk 14 — see §4
compatibility law), dropdown/action-menu/color-strip migration
(chunk 15/17 — M2/M3 cover only the picker family this chunk owns;
the remaining flyouts stay until their chunk), review-walk
(chunks 7/17).

## 2. Recorded decisions

### D-1 Migration order (picker repro first)

1. **M1 + M2 together** — the operator's pinned repro (R-8): the
   stale-anchor race (task-view.slint ~:315–333) exists *because of*
   the M1 swap; fixing M2's geometry without removing the swap leaves
   the repro half-fixed. First commit pair: failing geometry test
   (`om_us1_s1`, §5) → M1 overlay migration → M2 re-mount → green.
2. **M3** — the quick-list re-mirror (race 2), same seam family.
3. **M4** — the track-entries sibling, rides M3's host idiom.
4. **M5** recur, **M6** labels, **M7** palette — the modal family,
   independent of each other; one commit per mount so each stands
   alone (rebase-merge hygiene).
5. **M8 + M9** — details polish, then the swap-machinery deletion
   sweep and the dead-flag census update.

### D-2 The anchor-mirror races fix at the geometry seam (chunk-3 idiom)

Both races die by adopting the chunk-3 pattern proven in
`compositions/timers.slint` (~:168–175, commented on #954): the
opener pushes its realized geometry into the facts **at click time**
(deterministically realized), and derived-alias `changed` mirrors
keep the facts live across host scrolls. Concretely:

- M1/M2: the due/scheduled cell's `clicked`/`activated` handlers push
  `DatePickerFacts.anchor-*` from the *column element's* realized
  geometry in the same handler that opens; the `changed date-anchor-*`
  re-mirrors stay as keep-alives. With the M1 swap gone, the column
  height no longer changes in the picker's open pass, which removes
  the pre-swap-height flip-misbranch (research §2.4 race 1) — the
  test (`om_us2_s1`) opens the picker in the same layout pass as the
  editor-arm to pin it.
- M3: `time-field.slint`'s open-flip mirror becomes the click-time
  push (the timers idiom verbatim); the `changed anchor-abs-*`
  keep-alives stay. The panel-move race (research §2.4 race 2) is
  pinned by `om_us2_s3` asserting the placed panel's rect never
  intersects the field's rect even when the host panel's `y`
  re-resolves after mount.

**Not by clamping**: the last-resort clamp pulling a panel over its
anchor disappears with the host's law-1/2/3 placement; no mount keeps
a hand clamp expression. Review focus (dispatch 5.3): any surviving
hand-copied dismissal/clamp expression at a migrated mount is a
finding.

### D-3 Per-mount dismissal roles (J2 R-4)

| Mount | `DialogKind` / family | Outside click resolves to | Seam |
|---|---|---|---|
| M1 due/scheduled editor overlay | edit (field editing) | **save** — the same commit seam as Return (`date-accepted`) | `date-editor-focus-lost` collapses into it; Escape keeps the existing cancel path via the action layer |
| M2 date picker, M3 flyouts, M4 picker | light-dismiss (no scrim family) | dismiss (unchanged semantics) | existing `dismiss-requested` seams |
| M5 recur editor | **edit** — R-4 flips today's die-the-draft | **save**: routes the `recur-commit-clicked` seam; the draft's validity gate applies (an invalid draft does not commit — it stays open, the validity echo shows) | new `accepted()` wiring; `dismissed()` is then dead for this site — not wired |
| M6 labels panel | **flip by inner state**: the add/edit form open ⇒ edit (outside = form commit, guarded by the same colour/title validity); form closed ⇒ confirm (outside = dismiss) | per flip | `labels-overlay-form-commit-requested` / `labels-overlay-dismiss-requested` |
| M7 palette | confirm (no draft; R-5 trap only) | dismiss | `palette-dismiss-requested` |

Recorded consequence for M5: this *changes behavior* (outside click
commits a valid recur draft instead of discarding). That is exactly
R-4's operator ruling ("modern autosave expectation"); the census row
for `recur.editor` gets the supersession note in this chunk. Capture
overlay's `kind: confirm` flip is explicitly *not* here (chunk 3 left
it to W2 deliberately — stays captured in the census).

### D-4 Migrated rows stay chunk-14-compatible

The M1 closed cell, M9 effort row, and the metadata rows keep their
**caption-above + single-line-content `VerticalLayout` shape** — no
LabeledValueRow adoption, no new edit grammars, no re-addressing of
walk stop ids (5/6/11 keep their numbers; the effort row's stop
inventory note is updated where the round-8 census recorded
`task.effort_edit`). Chunk 14's absorption must remain a widget swap
over the same shapes. M9's row is one line: caption "Effort Estimate"
relabelled, `TextField` always mounted (`initial-text` seeded from
`task.effort-echo`), clear affordance retained as the field's
existing clear path + empty commit = `Some(None)` (unchanged walk
posture).

### D-5 Effort commit-on-focus-loss must not double-dispatch

R-9: no explicit save step. Commit fires on focus loss **and** on
Return; the guard is a last-committed-text comparison in the adapter
seam (a commit of an unchanged value dispatches nothing — typed
outcome, no text assertions). Closing the task-detail view while the
field is dirty commits once (focus-loss fires) and the view teardown
must not fire a second commit: the field's `focus-lost` handler is
the only dispatch site; `edited` only mirrors text. `om_us3_s2`
asserts exactly one `UpdateTask` for a type-then-click-away journey
and zero for a focus-without-edit journey.

### D-6 Rider: FakeEngine `StateCommand::DeleteLabel` settle arm (#881)

`tests/support/fake_engine.rs`'s `settle` gains the
`StateCommand::DeleteLabel { .. }` arm (mirroring the
`DeleteBreakRule` arm ~:203), so the migrated labels journeys
(`om_us1_s1_labels`) read like the other lifecycle journeys. Settle
stays **fixture continuity, not an oracle** — the file's contract
comment (~:9) is the law; the arm drains the channel so subsequent
assertions see the refreshed model, it asserts nothing itself.
Closes #881 at merge.

### D-7 #880 evidence note

The always-mounted Effort textbox (M9) dissolves #880's Escape-rung
framing: with no open/close swap there is no transient editor state
for Escape to race — Escape's task-detail binding behaves as on any
other field (the action layer's text-entry guards). When this chunk
lands, #880 closes **with this evidence** cited in a closing comment
(closes-at-merge, dispatcher-owned like #881).

## 3. Geometry & journey test catalogue (TDD)

New probe: `tests/overlay_migration_probe.slint` +
`tests/overlay_migration_probe_tests.rs` (the chunk-3
`widget_framework_probe` pattern; all rig tests `on_slint_thread`).
Token rows cite J2's journey steps; every test name embeds its token.

| Test | Token | Mount(s) | Assertions (typed/structural only) |
|---|---|---|---|
| sibling rects stable across due-editor open/close | us1_s1 | M1 | every metadata-row sibling's rect equal before/after open and after close (the R-7 screenshot pair, codified); zero layout shift |
| picker placed in the same layout pass as the arm | us2_s1 | M1+M2 | open the picker in the editor-arm pass: panel rect ∩ anchor rect = ∅; `flipped` verdict matches the below-fits law for the fixture geometry |
| flip-above near the floor | us2_s1 | M2 | anchor near `viewport-floor`: panel above, still ∅ intersection |
| time flyout never over its field | us2_s3 | M3 | for each site: panel rect ∩ field rect = ∅; after a forced host-panel y re-resolution, the facts mirror tracks and the placement law still holds |
| track-entries picker opens without form growth | us1_s1 | M4 | form's row rects unchanged across `form-grid-open` toggle; panel ∩ field = ∅ |
| recur modal anatomy | us1_s1, us1_s2, us1_s4 | M5 | panel centered, width within the R-6 bounds law; scrim paint present; Tab walk clamped to the declared stops; outside click (edit kind) routes the commit seam with a valid draft and commits nothing with an invalid one |
| labels modal anatomy + lifecycle journey | us1_s1, us1_s2 | M6 | both mounts (task-detail + board) carry scrim/mask; outside click with form open routes the form-commit seam (validity-gated), with form closed routes dismiss; assign→delete→settle journey passes with the D-6 arm |
| palette containment | us1_s2, us1_s3, us1_s4 | M7 | Tab cycles within the palette's stops (mask armed by the host); inside click never closes; outside/Escape dismiss |
| description edit-mount height | us3_s1 | M8 | edit-mount editor viewport height == display-mount height (geometry assertion via the `editor_viewport_probe` arm pattern) |
| effort commit-on-focus-loss | us3_s2 | M9 | type+blur ⇒ exactly one `UpdateTask`; focus+blur without edit ⇒ zero; Return ⇒ exactly one; close-view-while-dirty ⇒ exactly one |
| swap machinery dead | — | M1, M9 | fold test: the removed flags/props absent from the generated window (structural: the probe compiles without them; the seam inventory test updated) |

Existing suites that must stay green unchanged: the round-3 date
journeys (command dispatch identical — M1–M4 re-wire geometry and
mount, never callbacks), `focus_seam_probe_tests`,
`widget_framework_probe_tests`. Where a golden pins a migrated
surface's pixels, it is re-baselined in the same commit (no
weakening of other goldens).

## 4. Compatibility law (chunk 14 / chunk 15 / chunk 17)

- Rows keep caption+content `VerticalLayout` shapes (D-4); no
  LabeledValueRow, no TextButton adoption (chunk 13 owns the dialog
  button row's refinement — the host's `DialogButtonModel` is
  consumed as-is here).
- M2/M3 mount via `AnchoredOverlayHost` but keep their facts globals
  (`DatePickerFacts`, `TimeFieldFacts`) — AnchoredOverlayHost's
  absorption of the mirroring (chunk 17 / #954) will move the *call
  sites*, not the mounts' contract.
- Walk stop ids unchanged (5, 6, 11 and the effort row's); the
  FocusFacts restore anchors unchanged.

## 5. Risks

1. **M1 is the largest behavioral surface**: making DueEditor a
   window-level overlay moves the typed-entry field out of the view's
   layout — the field's focus-on-mount, the validity echo placement,
   and the walk stops that target the in-flow editor must be re-homed
   to the overlay panel (mask walk-stops = the editor's tab stops).
   Mitigation: the round-3 date journeys run unchanged as the dispatch
   parity gate; `om_us1_s1` is written *first* and fails on `main`.
2. **R-4 flips recur's outside-click semantics** (D-3) — the one
   deliberate behavior change in this chunk; called out in the PR body
   and the census supersession.
3. **Overlay exclusivity**: due-editor overlay, date picker, and the
   quick-list flyout can now be open simultaneously through distinct
   flags. The existing flow-exclusivity gates in the adapter
   (`date-picker-open` vs editor-open) carry over; the plan forbids
   new parallel-open states — arming the editor closes the picker and
   vice versa, pinned by a fold row in `om_us2_s1`'s test.
4. **Click-time mirroring under the testing backend**: the timers
   proof shows the pattern works, but M1's anchor is a *layout child*
   (the column) not the button itself — the handler reads
   `column.absolute-position` in the click pass; if the testing
   backend realizes it stale in any rig, the `changed` keep-alives
   cover it and the geometry test would catch it.
5. **Diff size**: M1 alone touches task-view.slint, app-window.slint,
   adapter push seams, and the probe — the whole chunk estimates
   **~+1200/−1000 lines** (net ≈ neutral), with the swap machinery
   deletions (M1 swap, M9 swap, hand clamps at M2–M4, hand
   scrims/centering at M5–M7) making the *machinery* diff strictly
   net-negative per the chunk acceptance.

## 6. Execution order for stage 2 (per mount, TDD)

Each step: failing test → migration → green → commit (standalone,
`refactor(ui): ...`/`fix(ui): ...`/`test(ui): ...` per house type).

1. probe skeleton + `om_us1_s1` failing on `main` (M1 repro).
2. M1+M2: swap → overlay, click-time mirrors, race 1 pinned. Fix pair.
3. M3: re-mirror + single host mount; race 2 test (`om_us2_s3`).
4. M4: track-entries window-level picker.
5. M5: recur onto ModalDialogHost (edit kind per D-3) + census note.
6. M6: labels both mounts + D-6 settle arm (#881 closes at merge).
7. M7: palette modal + containment journey (R-5).
8. M8: description stretch fix + `om_us3_s1`.
9. M9: effort row + commit guard + `om_us3_s2`; #880 evidence comment
   drafted for the dispatcher's at-merge close.
10. Deletion sweep: dead flags/props/callbacks (`effort-open`,
    `due-open`-adjacent swap bits, hand clamps/scrims), seam
    inventory + census updates, `scripts/ux-coverage.sh
    .artifacts/plans/2026-11-18-widget-framework-ux-plan.md --gate`.

Stage 2's gate: pre-commit hooks +
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(long-command discipline per dispatch §1 — the coverage trio never
runs inside the subagent).

## 7. Stage-2 outcome notes (2026-11-18, implementation)

### 7.1 PR callout note — the recur dismissal-role flip (D-3/R-4)

To copy verbatim into the PR body:

> **Behavior change (deliberate, J2 R-4):** the recurrence editor's
> outside click now *saves* a valid draft (the modern autosave
> expectation) instead of discarding it. An invalid draft still
> dispatches nothing — the editor stays open with the validity echo.
> Escape keeps the old clean close. Pinned by
> `journeys.rs::om_us1_s1_recur_scrim_click_commits_a_valid_draft_and_nothing_else`;
> the census (`docs/ux-surface-census.org`, recur editor row) carries
> the supersession note.

### 7.2 The #880 evidence comment (for the dispatcher's at-merge close)

To copy verbatim into the closing comment:

> Closed by the overlay migration (round 9 chunk 5,
> `refactor/ui-overlay-migration`): the Effort Estimate row is the
> always-on one-line field (commit on Return and focus loss, the D-5
> last-committed guard) — the open/close swap whose transient editor
> state Escape used to race no longer exists. Escape's task-detail
> binding behaves as on any other field (the action layer's
> text-entry guards). Evidence: `journeys.rs::om_us3_s2`
> (type+blur ⇒ exactly one `UpdateTask`; focus+blur without edit ⇒
> zero; Return+blur never double-dispatches).

### 7.3 Deviations from the stage-1 plan

- om_us2_s3 lives in `journeys.rs` (real-app mount), not the probe:
  the testing backend's slint-side `absolute-position` reads are
  unreliable in a freshly composed probe window (the recorded #954
  issue; the setter journey replays facts the same way). The real-app
  quick-time journeys carry the widget's click-time push.
- om_us3_s1 asserts the description SECTION's realized height
  equality across the display/edit mounts (typed seam) instead of the
  editor element's geometry: a conditional remount's element-query
  geometry is stale in the testing backend. The fill law is
  structural (`height: 100%` in the edit mount).
- M4 needed two adjacent fixes the migration surfaced: the model's
  `track_entries_open` read the top flow (the picker child rendered
  the panel closed under its own child — latent top-only-read bug,
  now chain-aware), and the grid's dismissal arm forces the refresh
  (the drop is pure presentation; no outcome carries it).
- The M4 grid mount carries no panel-mirror facts seam: a conditional
  mount's `changed` mirrors do not deliver in the testing backend;
  non-intersection is the host's base law (G-2, probe-pinned both
  directions), and the journey pins the anchor push + routing.

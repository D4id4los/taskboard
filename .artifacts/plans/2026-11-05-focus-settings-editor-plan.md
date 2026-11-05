# [Plan] Focus settings editor — the `[focus]` section in the settings view (#625) — stage-1 implementation plan

- **Date**: 2026-11-05.
- **Kind**: stage-1 TDD implementation plan for chunk **6** of the
  v0.2 focus-&-reflection dispatch. Zero production code in this
  commit; this file is the chunk's entire planning artefact.
  Implementation is stage 2's, review + shipping stage 3's.
- **Origin**: issue **#625** ("backlog: settings-view editor for the
  `[focus]` timings", ruling 2026-11-04: the editor enters v0.2); the
  implementation plan §5.6 + VF-8
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`);
  the dispatch plan §7
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`,
  branch `feat/focus-settings-editor`, size S); the pomodoro plan §3
  **D-F5** (`.artifacts/plans/2026-10-31-pomodoro-focus-plan.md`) — the
  config shape and typed error this editor is a *view over*; the
  landed follow-ups knobs (chunk 5, PR #645 merged 2026-11-05).
- **Status**: **READY** — every question the seams raised is settled
  with a recorded decision (§6, D-FS1…D-FS10). The one interpretation
  this planning pass contributes beyond the fence issue is the
  **diff-shape reading** (§5): the chunk grows the *write port* files
  (`settings_overlay.rs`, `ui_requests.rs`, `ui.rs`) exactly as the
  GUI settings editor chunk did, while `config.rs` (semantics) and
  `focus.rs` (runtime) stay untouched — the dispatch trap "any
  config.rs/runtime hunk is a finding" is read against that line.

## 1. Mission & binding constraints

The settings view gains a **Focus** section: one editable row per
`[focus]` knob, riding the landed settings-editor machinery — no new
config semantics, no runtime change, no new widgets. Binding
constraints (AGENTS §2/§5, impl-plan §5.6, dispatch §7):

- **Dumb client**: the UI stages drafts and mints typed intents only;
  validation authority stays `AppConfig::validate`, reached through
  the prove gate (`settings_overlay::prove_and_write`). The ui-slint
  mirror predicates exist so the seam can refuse before the lane and
  the field can flag early — parity pinned by fixtures, never authority.
- **Typed rejection, never a silent clamp**: every rejection the
  ladder can produce surfaces as field validity / typed outcome. No
  clamping anywhere in the editor.
- **No text assertions** (AGENTS §5): journeys assert typed model
  properties (`FieldValidity`, `SettingsOutcome`, echo strings that
  ARE the data) and the typed `UiRequest`s that reached the lane —
  never status/error copy.
- **Kiosk untouched**: settings is a desktop surface; no kiosk layer
  changes (the three focus kiosk pins from the pomodoro chunk stay).
- **Branch precondition verified 2026-11-05**: chunk 5 landed (PR
  #645 → `origin/main` `6a3bc161`), so the editor grows rows for all
  seven landed knobs. Verified at base `origin/main` = `947b9f76`.

**Verified anchors** (symbols at base, spot-checked):

| Anchor | Where | Role here |
|---|---|---|
| `FocusSection` — 7 knobs | `taskboard-app/src/config.rs:147-192` | the row set's truth |
| `FOCUS_MINS_MAX=240`, `FOCUS_EVERY_MAX=24`, defaults 25/5/15/4 | `config.rs:118-122` (+ every-max const) | the mirrors' bounds |
| validate ladder: minutes `1..=240`, `long_break_every` `1..=24`, always (even disabled) | `config.rs:1482-1495` | the authority the gate reaches |
| `ConfigError::InvalidFocusTiming { field }`, kind `"invalid-focus-timing"`, `field()` attribution | `config.rs:1071-1074,1096-1111,1140` | the typed rejection the gate answers with |
| `UiRequest::Set*` + `SettingsSnapshot` + `Field`/`InvalidKind` lattice | `taskboard-ui-slint/src/requests.rs:120-351` | the lane this chunk grows |
| `settings_general_committed` / `GeneralField` (indices 5–11) | `taskboard-ui-slint/src/adapter.rs:3188-3277,4486-4595` | the commit seam extended |
| `push_general_settings` (echo/validity/env-lock/on-restart push) | `adapter.rs:3928-4034` | the push extended |
| `run_editor_write` → `fresh_settings_snapshot` reply choreography | `taskboard-app/src/ui_requests.rs:429-448` | the persist path ridden, unchanged |
| `map_overlay_error` / `editor_field_key` / `editor_invalid_kind` | `ui_requests.rs:465-514` | the attribution rule extended |
| `settings_overlay::prove_and_write` + `set_*` writers | `taskboard-app/src/settings_overlay.rs:54-123` | the writer recipe |
| `settings_snapshot_from_config` (facts + applies) | `taskboard-app/src/ui.rs:315-470` | the snapshot builder extended |
| `settings_form` mirrors + `FieldStatus` triad | `taskboard-ui-slint/src/settings_form.rs` | the mirror pattern reused |
| `invalid_text` / `status_text` / `HELP_*` | `taskboard-ui-slint/src/settings_status.rs` | the copy lattice extended |
| Scale commit-on-release posture | r3 D20a/D20b/D20c, r4 D38; `slider.slint`, `scale-released` seam | cited precedent (not reused as a widget) |
| Seam inventory (two-sided manifest, 174 installs) | `tests/seam_inventory.rs` | grows by +3 entries as data |
| Settings journeys J-1…J-7 + `general_snapshot()` fixture | `tests/journeys.rs:12300+` | the catalogue extended J-F* |

## 2. The row set (D-FS1)

Seven knobs at base → seven rows, in the landed config order. A new
**"Focus"** section header between **General** and **Reminders**
(grouping: general preferences, then the feature sections).

| # | Config key | Type | Widget / grammar | Bounds | Default |
|---|---|---|---|---|---|
| 1 | `focus.enabled` | bool | `SegmentedControl` ["On","Off"], commit on pick | — | off |
| 2 | `focus.work_mins` | u64 | `GeneralField` text, **field-index 12** | `1..=240` | 25 |
| 3 | `focus.break_mins` | u64 | `GeneralField` text, **field-index 13** | `1..=240` | 5 |
| 4 | `focus.chain` | bool | `SegmentedControl` ["On","Off"], commit on pick | — | off |
| 5 | `focus.long_break_mins` | u64 | `GeneralField` text, **field-index 14** | `1..=240` | 15 |
| 6 | `focus.long_break_every` | u64 | `GeneralField` text, **field-index 15** | `1..=24` | 4 |
| 7 | `focus.notify` | bool | `SegmentedControl` ["On","Off"], commit on pick | — | on |

Widget choice per type (the plan's settle-question):

- **Bool → the segmented On/Off control** (`reminders-enabled`
  precedent): the house's landed boolean grammar — selection renders
  from the snapshot (`index 0 = On`), the pick dispatches the typed
  intent, the reply re-renders the index. No checkbox/switch widget
  exists; inventing one is out of scope.
- **Numeric → the general text-field grammar, NOT a slider/stepper.**
  The scale slider (r4 D38's release-commit posture) exists because
  scale **live-applies** — the drag gesture previews a visual change.
  Every `[focus]` key is **OnRestart** (D-FS4): a drag would preview
  nothing, and the slider's per-row scroll-anchor machinery (r3 D20c)
  buys nothing. The general fields (tick, poll, backoff, due-soon —
  all bounded numbers) landed as text fields; focus minutes are the
  same class. **The dispatch acceptance "commit-on-release pinned (no
  live re-lay under drag)" is satisfied structurally and pinned
  explicitly** (J-F9): these rows mount no gesture widget at all, and
  the journey pins that per-keystroke edits dispatch zero requests —
  only the Return seam routes, one intent per commit (the D9 grammar
  the general fields already pin). Stage 3 reads the acceptance
  against this ruling, not against a slider row that must not exist.
- The text rows continue the **`field-edited` index contract**
  (connector form owns 0–3, scale 4, general 5–11): focus minutes take
  **12–15**; the contract comment in `settings-view.slint` (the
  callbacks' doc block) and `GeneralField::from_index` update together.

**Facts promotion** (D-FS2): `focus.enabled`, `focus.work_mins`,
`focus.break_mins` currently render as read-only **Deployment facts**
(`ui.rs` facts list, pomodoro chunk). They become editable rows and
**leave the facts list** — a key renders exactly once, never both
editable and read-only. The four follow-ups knobs were never facts.
`settings_snapshot_from_config` drops those three `fact(...)` rows;
the snapshot projection test's fact pins don't reference them
(verified: it pins `ui.stack_widths`, `ui.window_geometry`,
`app.mode`, `web.token` only).

## 3. Validity surfacing (D-FS3)

Two mirror predicates in `settings_form.rs`, the landed per-field
pattern (parity with the config ladder pinned by truth-table tests;
parse failure maps to the reserved `NotKebab` arm, bounds to
`OutOfRange`, trimmed-empty is Neutral):

```rust
fn focus_count_invalid_kind(text: &str, max: u64) -> Option<InvalidKind>
pub fn focus_mins_invalid_kind(text: &str)  -> Option<InvalidKind> // 1..=240
pub fn focus_every_invalid_kind(text: &str) -> Option<InvalidKind> // 1..=24
```

- The bounds are hardcoded with comments citing `FOCUS_MINS_MAX` /
  `FOCUS_EVERY_MAX` (the `due_soon_invalid_kind` 3652 precedent —
  ui-slint cannot depend on taskboard-app; drift breaks a test on
  either side, and the journeys pin the round-trip through the real
  gate).
- No new `InvalidKind` arms: parse → `NotKebab`, `0`/over-max →
  `OutOfRange` (the `secs_invalid_kind` shape verbatim).
- The push reuses the generic `general_status` mapping; the flavour
  rides the existing `FieldValidity` triad (r3 D19) — border + error
  line, Neutral when the field is empty. Error copy lands as new
  `invalid_text` per-field arms (presentation copy, never asserted).
- **Gate attribution** (the acceptance's "every rejection the config
  ladder can produce surfaces as field validity"): the new `Field`
  arms enter `editor_field_key` (`focus.work_mins` …) and
  `editor_invalid_kind` (`OutOfRange` for the four numerics; the three
  bool arms exist for the same-section blame comparison only, the
  landed rule). An own-blame gate refusal → `Invalid { field,
  OutOfRange }` on that row; any other blame → `PersistRejected`
  (coarse, the file refused) — the landed `map_overlay_error` rule,
  zero new machinery.
- Defense in depth, not dead code: the mirror refuses `0` before the
  UI mints, so a garbage `SetFocusWorkMins` can only reach the app
  from a non-UI lane client — the writer test still pins the gate's
  typed refusal of it (never a silent clamp at any layer).

## 4. Commit / revert semantics (D-FS4…D-FS6)

- **Text rows** — the general-field grammar verbatim (r3 D20a's
  writer-point discipline): keystrokes stage the draft (echo =
  draft-when-present); Return commits (mirror refusal keeps the draft
  under its flavour; empty Return refuses typed `Empty`; then the
  typed intent dispatches, the draft clears, Busy shows); focus loss
  discards the draft (echo re-syncs to the committed value).
- **Bool rows** — commit on pick (the keymap-preset/reminders
  grammar): the pick dispatches immediately; the selection renders
  from the snapshot, so a failed pick re-renders nothing (the old
  value stays selected).
- **Revert on failed persist** (the acceptance pin): on an `Err`
  reply the VM's snapshot is *not* replaced (`PendingReply::Snapshot`
  landed rule), so every echo and every selection re-derives from the
  last-known-good file values — the failed value never sticks — and
  the status line shows the typed outcome. Pinned by J-F8.
- **Apply semantics**: all seven keys are **OnRestart** in the
  `applies` map — the focus runtime spawns at boot from
  `config.focus_config()` (`bootstrap.rs:1314`), config is read once;
  no pretend-live application (plan D5's honesty rule). The rows
  render the "takes effect at the next restart" caption through the
  existing `on-restart` push. The `applies` map grows 9 → 16 entries;
  the snapshot projection test's `len == 9` grows to 16 (extension of
  the map's own contract, not a weakened pin).
- **Env locks ride free**: `env_overridden_keys` diffs file-only vs
  effective config generically — `TASKBOARD_FOCUS__WORK_MINS` et al.
  land in the hand-over set, and the landed rendering responds
  (segments render disabled; text rows render read-only with the env
  caption; commits absorbed at the defence arm). Zero new machinery.
- **The one-op-at-a-time pending gate** covers every new seam
  (in-flight focus op gates the others) — the J-7 machine, extended.

## 5. The lane & write-port growth (D-FS7) — the diff-shape reading

New typed intents, one per editable field (the closed-enum secrets
boundary rule in `requests.rs` — no variant could carry a credential):

- `Field` grows: `FocusEnabled`, `FocusWorkMins`, `FocusBreakMins`,
  `FocusChain`, `FocusLongBreakMins`, `FocusLongBreakEvery`,
  `FocusNotify`.
- `UiRequest` grows the seven `SetFocus*` variants, each
  `reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>`
  (the snapshot-reply rule: rows re-render from the fresh file echo).
- `SettingsSnapshot` grows the seven typed values.
- App side, the **existing config-persist path extended** (the exact
  shape PR #628 landed for the general fields — `8ddf2e5d..f6af9851`
  is the file-shape precedent):
  - `settings_overlay.rs`: seven `set_focus_*` writers —
    `set_secs(path, "focus", "work_mins", …)` one-liners / bool
    `prove_and_write` calls; the whole-document prove gate refuses
    typed (`InvalidFocusTiming` carries the dotted `field`).
  - `ui_requests.rs`: seven `handle_settings` arms riding the
    unchanged `run_editor_write` choreography; `editor_field_key` /
    `editor_invalid_kind` arms.
  - `ui.rs`: the seven snapshot fields, the seven `applies` entries
    (OnRestart), the three facts rows removed (D-FS2).

**The dispatch trap interpretation, recorded for stage 3**: §7's
review prompt reads "only ui-slint settings files + tests + CHANGELOG
— any config.rs/runtime hunk is a finding". The write-port files
above are not `config.rs` and not the runtime: they ARE "the existing
config-persist path" the chunk origin (§7 covers text, issue #625)
names as the write route. Findings are hunks in `config.rs`
(`FocusSection`, the validate ladder, `FocusTiming` minting) and the
runtime files (`focus.rs`, `escalation.rs`, `notifications.rs`) —
those must be *absent* from the diff. Everything else in this chunk's
file list is the settings editor's own landed territory.

**Explicit non-goals** (dispatch §7): no config semantics change; no
focus-runtime change; no editor rows outside `[focus]`; no kiosk
surface; no CLI/config-print change (`config print` already echoes
the raw section, D-F5).

## 6. Decision record

| ID | Decision | Why / precedent |
|---|---|---|
| D-FS1 | Seven rows, config order, "Focus" section between General and Reminders; bool → segmented On/Off (commit on pick), numeric → general text field (Return commit, focus-loss discard), indices 12–15 | the landed grammars; no new widget; §2 |
| D-FS2 | The three pomodoro-era focus facts leave the Deployment facts list (editable rows render once) | §2; no double render |
| D-FS3 | Mirrors `focus_mins_invalid_kind` / `focus_every_invalid_kind` over a shared bound-checked core; no new `InvalidKind`; bounds hardcoded with parity comments | `secs_invalid_kind`/`due_soon_invalid_kind` shape; §3 |
| D-FS4 | All seven keys apply OnRestart in `applies` | runtime reads config at boot only (`bootstrap.rs:1314`); §4 |
| D-FS5 | Text rows: draft staged on edit, Return commits, focus loss discards; bool rows: commit on pick | r3 D20a writer points; reminders/sink picks; §4 |
| D-FS6 | Revert on failed persist = snapshot-not-replaced (echoes re-derive from last-known-good); typed outcome on the status line | `PendingReply::Snapshot` landed rule; §4 |
| D-FS7 | Seven `Field`/`SetFocus*`/writer/`handle_settings` arms; snapshot +7 typed values; applies +7 | the PR #628 write-port recipe; §5 |
| D-FS8 | No slider rows: OnRestart knobs have nothing live to preview; commit-on-release pinned structurally (J-F9: keystrokes dispatch nothing, one intent per Return) | §2, r4 D38's posture honored in the text grammar |
| D-FS9 | Three new adapter seams: `settings_focus_enabled_picked` / `settings_focus_chain_picked` / `settings_focus_notify_picked`; text rows reuse `settings_field_edited`/`settings_general_committed`/`settings_general_focus_lost` | one callback per control (`reminders-picked`/`sink-picked` precedent); the seam inventory grows +3 as data |
| D-FS10 | Help facts: `HELP_FOCUS_*` consts in `settings_status.rs`, pushed per row (the `HELP_DAY_START` pattern) | one source per help fact, the r2 D31 rule |

## 7. Test catalogue (TDD-first: each phase opens with its failing tests)

**Mirror units** (`settings_form.rs` tests):

- `the_focus_mins_truth_table_matches_the_config_bounds` — 1 and 240
  pass; 0, 241, "abc", "-1", "2.5" refuse (`OutOfRange` / `NotKebab`
  as ruled); "" and "   " → `None` (Neutral).
- `the_focus_every_truth_table_matches_the_config_bounds` — 1 and 24
  pass; 0, 25 refuse; non-numbers → the reserved parse arm.
- `focus_status_maps_the_three_flavours` — the `general_status`
  composition over both predicates (empty → Neutral, failing →
  Invalid(kind), passing → Valid).

**App write-port units**:

- `settings_overlay`: `focus_minute_writes_and_the_gate_refuses_garbage`
  (write 50 → file carries it; write 0 → `OverlayError::Rejected`,
  file byte-identical); `focus_every_bound_refuses_at_the_gate` (25 →
  rejected typed `invalid-focus-timing` blaming `focus.long_break_every`);
  `focus_bool_writes_round_trip` (enabled/chain/notify true/false).
- `ui_requests`: `focus_commits_persist_and_reply_the_fresh_snapshot`
  (each variant updates the file, the reply echoes the new typed
  value); `focus_gate_refusal_attributs_own_blame` (a 0-minute write
  answers `Invalid { FocusWorkMins, OutOfRange }`); the snapshot
  projection test grows: seven fields projected, `applies.len() == 16`
  with the seven focus keys OnRestart, and the three promoted keys
  **absent from the facts** (J-F1's app-side twin).

**Journeys** (`tests/journeys.rs`, the J-F catalogue over
`general_snapshot()` grown with the focus values — the fixture is the
hand-over's shape):

- **J-F1** `the_focus_rows_render_the_snapshot` — seven typed echoes /
  three segmented indices render; the on-restart captions' facts are
  pushed (`focus_work_on_restart` etc.); the facts model carries no
  `focus.*` row; nothing dispatches at mount.
- **J-F2** `a_focus_edit_round_trips_and_echoes_the_reply` — stage
  index 12, commit; exactly one `SetFocusWorkMins { secs: 45 }`
  dispatches; the fresh-snapshot reply re-renders the echo; Ok lands.
  A pick dispatches `SetFocusEnabled { enabled: false }`; the reply
  re-renders the index.
- **J-F3** `invalid_focus_commits_refuse_typed_without_routing` —
  "0", "241" (index 12) and "0", "25" (index 15) refuse at the seam:
  the validity flavour renders Invalid, the status shows the refusal
  outcome, **zero requests dispatch**, the draft stays visible under
  its flavour.
- **J-F4** `env_locked_focus_rows_render_locked_and_refuse_the_seam` —
  a focus key in the hand-over env set: the row locks, the commit/pick
  absorbs, nothing dispatches.
- **J-F5** `an_empty_focus_return_refuses_typed_without_routing` (the
  J-5 arm at the new indices).
- **J-F6** `a_focus_return_less_edit_never_survives_the_surface` (the
  J-6 arm: focus loss re-syncs the echo to the committed value).
- **J-F7** `a_focus_op_in_flight_gates_the_other_settings_ops` (the
  J-7 arm: pending gate covers the new seams).
- **J-F8** `a_failed_focus_persist_reverts_the_echo_and_shows_the_outcome`
  — `Err(SettingsError::PersistRejected)` reply: the echo/selection
  re-derive from the unchanged snapshot (the pre-commit value), the
  outcome renders typed; no retry loop, no stuck Busy.
- **J-F9** `focus_keystrokes_never_dispatch_until_the_commit_seam` —
  the commit-on-release structural pin: a sequence of
  `settings_field_edited` calls over index 12 drains zero requests;
  the single Return seam dispatches exactly one intent (D-FS8's
  acceptance reading, made test).

**Structural / data pins**:

- `seam_inventory.rs`: the three new installs enter `SEAM_INVENTORY`
  (both sides force honesty — the count grows 174 → 177; the doc
  comment's "as of the chunk-4.2 base" tally updates with it, the
  manifest's declared habit).
- The `SettingsSnapshot` construction sites (`ui.rs`, the journeys'
  `general_snapshot()`) are exhaustive over the new fields by build.

**Snapshot/golden regens**: keymap snapshots untouched (no keymap
change); no settings golden PNG exists and none is added (the chunk's
visual surface is plain rows in an already-shipping view); the
"data snapshots" that change are exactly the three above (seam
manifest, applies map, journey fixture) — all data-only.

## 8. Phasing & commit plan (one branch, FS.1 → FS.4 in order)

1. **FS.1 — the lane** (`requests.rs`, `settings_form.rs`,
   `settings_status.rs`): the mirror predicates + truth tables (red
   first), `Field`/`UiRequest`/`SettingsSnapshot` growth, the
   `invalid_text` arms + `HELP_FOCUS_*`. Standalone commit:
   `feat(ui-slint): the focus lane types and mirrors`.
2. **FS.2 — the write port** (`settings_overlay.rs`, `ui_requests.rs`,
   `ui.rs`): the seven writers + dispatch arms + snapshot/applies/
   facts changes + the app tests (red first). Commit:
   `feat(app): the focus settings writers`.
3. **FS.3 — the surface** (`settings-view.slint` global properties +
   Focus section rows, `adapter.rs` GeneralField arms + push arms +
   three pick seams, `seam_inventory.rs` +3): the render +
   round-trip + refusal journeys (red first). Commit:
   `feat(ui-slint): the focus section in the settings view`.
4. **FS.4 — the acceptance journeys + bookkeeping**: J-F7…J-F9, the
   CHANGELOG Unreleased entry for #625. Commit:
   `test(ui-slint): the focus settings journeys and the #625 changelog entry`.

Every commit standalone (rebase-merge hygiene). Full gate suite
through `scripts/run-quality-gates.sh` before handoff; fmt/clippy
clean (no warnings offered).

## 9. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Stage 3 reads the write-port growth against trap (a)'s letter and flags legitimate hunks | §5 records the interpretation; the PR body restates it; the diff keeps `config.rs`/`focus.rs`/`escalation.rs`/`notifications.rs` at zero hunks — checkable with `git diff main...HEAD --stat` |
| Index-contract drift (view indices vs `GeneralField::from_index`) | one contract, two sites updated in the same commit; J-F2 pins 12 and 15 end-to-end (an index the adapter doesn't resolve silently routes nothing — the journey fails) |
| The facts promotion surprises a consumer | `grep`-verified: the facts list has no other consumer than the snapshot builder + the two render sites (view facts loop, journey fixtures); the projection test pins the absence |
| Seven near-identical arms invite copy-paste drift | every arm rides the shared choreography (`GeneralField` machinery, `run_editor_write`, `general_status`) — the arms carry only the per-field data (key, field, predicate, echo) |
| Slint property-name sprawl in the global | the landed per-field naming (`<key-abbrev>-echo`, `validity-<…>`, `error-<…>`, `<…>-env-locked`, `<…>-on-restart`) extended mechanically; no new property *kinds* |
| `long_break_every`'s 24 bound silently tracking `FOCUS_EVERY_MAX` by hand | both mirrors' truth tables pin both bounds' neighbours (0/1 and max/max+1); the config-side bounds tests already pin the ladder — drift breaks one side loudly |

## 10. References

- Issue #625 (the fence + the ruling); #600 (the knob origin).
- `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md` §5.6, §4 VF-8.
- `.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md` §1 (shared execution model), §7 (this chunk).
- `.artifacts/plans/2026-10-31-pomodoro-focus-plan.md` §3 D-F5, §6 (the decision record's config rows).
- `.artifacts/plans/2026-10-26-ui-r3-settings-polish-plan.md` — **D19** (the validity flavour API: the FieldValidity triad, border tokens, error line), **D20a** (the echo property + the writer-point draft discipline: commit-on-Return, focus-loss discard, draft-when-present echo), **D20b/c** (the slider gesture + scroll anchor — cited as the reason the slider is *not* reused).
- `.artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md` — **D38** (preview dies; release-commit pins: one commit per gesture, no theme write mid-drag — the posture the text grammar honors structurally), **D39** (the dropdown/selection echo grammar the segmented picks already ride).
- `.artifacts/plans/2026-11-03-gui-settings-editor-plan.md` — the write-port decisions this chunk reuses verbatim (the prove-gate attribution D14, the snapshot-reply rows-rerender rule, the closed-enum intents rule).
- `.artifacts/plans/2026-11-05-focus-followups-plan.md` — the knobs' landed shapes (D-FF9) this editor exposes.
- AGENTS.md §2 (crate boundaries), §5 (testing directives), §7 (artefact layout), §8 (git workflow), §10 (gates).

# [Plan] GUI Review Round 3 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-26
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **7** chunks of the GUI review
  round 3 fix round (UI flow state machine, board surface, task view
  & dates, settings polish, legacy config migration, navigation &
  menu, reminder OS-delivery). No code in this commit.
- **Origin**:
  `.artifacts/plans/2026-10-26-gui-review-round3-plan.md` — findings
  R3-F1…F41 (§1), fold-in decision §2, designs §3 (W-SM, D1–D32),
  testing §4, inventory §5, slicing §6, risks §7, bookkeeping §8 —
  normative. Round-2 plans remain background
  (`2026-10-23-gui-review-round2-plan.md` and its dispatch plan for
  the shared execution model style). Plus the user's fences: DR-2/3/4/5/6
  and undo-v1 are separate agents' rounds.
- **Status**: READY — pending the user's acceptance of the design
  round (same branch). Chunk 1 dispatchable on merge; 2, 3 gated on
  1's merge; 4 on 3; 6 on 3; **5 and 7 independent** (may be
  dispatched any time after merge, parallel lanes permitted — they
  share no files with 1–4).

## 1. Chunks & ordering

| Chunk | Design | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §3.1 W-SM | `fix/ui-r3-flow-state-machine` | flow audit report, rules R1–R5, total interpret table, title re-edit | L |
| 2 | §3.2 | `feat/ui-r3-board-surface` | ghost size, glyphs, tooltips, empty-space context menu, empty board + default-kanban presets, card checkbox, slint deprecations | L |
| 3 | §3.3 | `feat/ui-r3-taskview-dates` | due/scheduled editors, time grammar, picker overlay + keyboard, reminder row times, due-soon horizon, description (gated) | L |
| 4 | §3.4 | `feat/ui-r3-settings-polish` | eye toggle, InfoTip width, status placement, inline validation, scale sync/drag/scroll | L |
| 5 | §3.5 | `feat/ui-r3-legacy-config-migration` | automatic legacy config migration | M |
| 6 | §3.6 | `feat/ui-r3-navigation-menu` | view switcher, ShowTask, palette layout, control-nav, scroll actions, menu curation, C-g C-g, #231 | L |
| 7 | §3.7 | `fix/ui-r3-reminder-delivery` | DesktopSink probe + Wayland/KDE fix, delivery seams | M |

**Dependency graph**: serial 1 → 2 → 3 → 4; 6 after 3; 5 and 7
independent (no file overlap with the others — 5 is app-side config,
7 is app-side notifications). Each chunk's stage 1 has the
**precondition gate**: its predecessors' PRs merged into `main`
(chunk N verifies with `git fetch origin --prune && git log
origin/main --oneline -5`); if not merged, STOP and report.

**Shared execution model** — identical to the round-2 dispatch plan
§1 (`.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`):
stage 1 plans TDD-first in the worktree `../taskboard-WT-<branch>`
(`scripts/worktree-doctor.sh` first; commit `docs(plan): …`, no
push); stage 2 implements (standalone commits, CHANGELOG Unreleased,
full AGENTS §10 gate suite green, no push/PR); stage 3 rebases onto
`origin/main` (rebase dance), reviews vs acceptance + the
code-quality mandate below, fixes on the branch, ships (PR, checks
green, `mergeStateStatus: CLEAN`, then `git worktree remove --force`
from the project root). Never merges; never touches `main`.

**Code-quality mandate** (stage 3, every chunk — copy the verbatim
text from the round-2 dispatch plan §1 "Code-quality mandate",
including the no-text-assertions clause).

## 2. Chunk 1 — Flow state machine (`fix/ui-r3-flow-state-machine`)

- **Covers**: design §3.1 (W-SM-a…d) + facts it cites. Deliverable
  includes the audit report
  `.artifacts/reports/2026-10-26-ui-flow-state-audit-report.md`
  (research artefact — the evidence & artefact mandate applies to
  it; implementation parts get the code-quality mandate). Non-goals:
  no board-surface visuals (chunk 2), no due-picker changes (chunk
  3), no typestate introduction (W-SM-c constraint).
- **Acceptance**: (a) audit report exists with the six mechanisms
  anchored and every flow's affordance named; (b) interpret table
  total by construction — no wildcard default arm (compiler-enforced);
  (c) R1: pointer click-outside commits non-empty text flows,
  discards empty ones (journeys); (d) R2: drop-miss/stale-column
  cancels `DragMove` cleanly (journey); (e) R3: view switches,
  settings open, palette open settle-then-run under any flow
  (journeys incl. EditDue→Settings for R3-F18); (f) R4: `Begin*`
  under a foreign flow yields-or-settles with visible feedback, never
  silent (journeys for R3-F9); (g) R5: every flow has a pinned
  visibility affordance; (h) title: task-detail entry does not
  auto-arm edit; title clickable-to-edit, always re-armable, full
  row width (journeys + R3-F26); (i) empty-board begin_create_stack
  opens a *visible* flow (minimal fix here; full empty-board page is
  chunk 2); (j) existing flow tests pass or carry recorded decisions.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of GUI review round 3 ("flow state machine", branch fix/ui-r3-flow-state-machine) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md (incl. §2.1 type-driven policy), then the design round .artifacts/plans/2026-10-26-gui-review-round3-plan.md §3.1 (W-SM, normative) + §1 findings 7/8/9/10/18/26 + §4/§7-R1, and the dispatch plan §1/§2.
2. Read the current seams: src/view_state.rs (Flow 58-101, begin/commit/discard 964-1072, settle reconcile 1529-1550), src/adapter.rs (interpret table 589-783 — note the (_, _) default arm at 770, the EditDue swallow 677-683, OpenSettings 756, settle_flows_on_leaving 532-584, drag finish 2398-2432, BeginCreate arms 686-692), ui/widgets/text-field.slint (accepted/edited only, 87), src/shell.rs (chain 62-82), src/keymap/presets.rs (ModeInsert vi-only 339-340), ui/views/task-view.slint (title 88-118), src/adapter.rs one-shot buffers 107-139. Tests: view_state.rs 1847-2793 flow pins, journeys 261/927/1194/1752/1981+.
3. Run scripts/worktree-doctor.sh, then create ../taskboard-WT-fix-ui-r3-flow-state-machine on a new branch off main (precondition: round-3 artefacts merged).
4. Write the TDD-first plan at .artifacts/plans/2026-10-26-ui-r3-flow-state-machine-plan.md settling: the audit report outline (deliverable); the exact settle rules per flow×pointer-outside (commit vs discard vs cancel per flow kind); the yield/settle classification for every non-conflicting effect under every flow (the table that replaces the default arm — enumerate, do not hand-wave); the drag settle fix (which early-outs die); the visible-flow rule per flow; the title-edit redesign (no auto-arm on entry; click/hover grammar; re-arm seams incl. a bound action in all presets — name the registry rows); the TextField focus-loss seam mechanism (where focus loss is observed on pinned Slint — probe and record); buffer-lifetime invariants for due_buffer/width_drag/scale_drag; the test catalogue (transition-table test shape, journey list); Tier-4 items (click-out feel, stuck-state repro scripts for the user's five scenarios).
5. Commit: docs(plan): ui flow state machine rework. Do not implement; do not push.

Report: plan path, decisions (esp. the yield/settle table summary and the focus-loss probe result), risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of GUI review round 3 ("flow state machine").

1. Read AGENTS.md, design round §3.1 + §4, dispatch plan §1/§2, and the stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-flow-state-machine-plan.md) in ../taskboard-WT-fix-ui-r3-flow-state-machine (recreate if missing).
2. Implement per the approved plan: the audit report (as a docs commit), then the rules R1–R5, total table, title re-edit, focus-loss seam. TDD, standalone commits. The transition-table test must fail before the default arm is removed. Any existing test that pinned a broken behaviour gets the recorded-decision comment naming the plan section.
3. Full gate suite (AGENTS §10) green. CHANGELOG Unreleased updated (stuck states fixed, title editing rework).
4. No push, no PR. Never touch main.
Report: per-gate results, the Tier-4 checklist (the five user scenarios as repro scripts), deviations.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of GUI review round 3 (branch fix/ui-r3-flow-state-machine).

1. Read AGENTS.md, design round §3.1/§4/§7-R1, dispatch plan §1/§2 (both mandates apply: evidence & artefact for the report, code-quality for the code), the stage-1 plan; review the full post-rebase branch diff.
2. Functional focus: (a) the default arm is gone and the table is total (compile-proof, not convention); (b) no settle path double-commits (click-out then Enter must not create twice); (c) drag settle cannot regress the round-2 gesture (chunk-2-r2 interleaved-refresh tests still green); (d) the audit report's claims match the code (spot-check every file:line it cites); (e) the empty diff over taskboard-domain/taskboard-app; (f) no text assertions.
3-7. Mandate, gates, fixes, ship (PR + checks + CLEAN), teardown as §1 states. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items restated.
```

## 3. Chunk 2 — Board surface (`feat/ui-r3-board-surface`)

- **Covers**: design §3.2 (D1–D6) + slint deprecation sweep (fold-in).
  Non-goals: no flow logic (chunk 1 owns), no palette/menu.
- **Acceptance**: (a) ghost TaskCard binds captured card size
  (geometry pin + Tier-4); (b) plus glyphs at IconButton visual
  weight everywhere; (c) Add Task ghosts tooltip-wired (structural);
  (d) column-body right-click opens the stack context menu
  (journey); (e) empty board: board bar/tabs visible, centered
  labelled New Stack with working field, "Create Default Kanban
  Setup" creates the four stacks via existing CreateStack commands
  (journeys; preset table extensible — a second test preset proves
  it); (f) card checkbox toggles done via SetTaskDone (click ≡ key
  journey; done visual consistent); (g) Slint build emits zero
  deprecation warnings (gate log is the evidence); (h) deprecation
  sweep covers the whole ui crate, not just touched files.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of GUI review round 3 ("board surface", branch feat/ui-r3-board-surface).

1. Read AGENTS.md, design round .artifacts/plans/2026-10-26-gui-review-round3-plan.md §3.2 (D1–D6, normative) + §1 findings 1/2/4/5/6/9/10/11/12/41 + §2 fold-ins + §7, dispatch plan §1/§3. Precondition: chunk 1 merged (git fetch origin --prune; verify), STOP if not.
2. Read the seams: ui/app-window.slint (ghost 482-491), ui/drag-state.slint, src/adapter.rs (drag arm 2336), ui/compositions/board-layout.slint (ghost source sizing 421-437, PlusGlyph uses 528/481/695, ghost tooltips 470-472/684-686 vs New Stack 505-515, empty branch 146-158, board bar gating 160-181, header context 264-277, column body 238/407, task cards), ui/widgets/plus-glyph.slint, ui/widgets/task-card.slint, board preset-adjacent code (stack create flows). Also run a build capturing ALL slint deprecation warnings (cargo build -p taskboard-ui-slint 2>&1 | grep -i deprecat) and list them in the plan.
3. scripts/worktree-doctor.sh, then ../taskboard-WT-feat-ui-r3-board-surface on a new branch.
4. Plan at .artifacts/plans/2026-10-26-ui-r3-board-surface-plan.md settling: ghost size capture (what exactly is measured at arm time, where stored); glyph decision (shrink PlusGlyph vs adopt IconButton — one call, record it); tooltip texts as presentation constants; the column-body right-click seam signature; the empty-board composition (bar visible, centered CTA, working field — reuse chunk-1's visible-flow rule); the board-preset table shape (module home, data shape, the dispatch loop over CreateStack, second-preset test); the checkbox anatomy (left-edge, target/announce semantics, done styling reuse); the deprecation fix list (each warning → its fix); test catalogue + Tier-4 (ghost size feel, checkbox feel, empty-board flow).
5. Commit: docs(plan): board surface fixes & empty-board presets. No implement, no push.
Report: plan path, decisions, the deprecation warning inventory, risks, diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of GUI review round 3 ("board surface").
1. Read AGENTS.md, design §3.2, dispatch §1/§3, stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-board-surface-plan.md) in ../taskboard-WT-feat-ui-r3-board-surface (recreate if missing).
2. Implement per plan, TDD, standalone commits; presentation constants for all new user-visible strings; no text assertions; the deprecation sweep must end with a warning-free Slint build (paste the proof in the report).
3. Full gate suite green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, deprecation proof, Tier-4 items, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of GUI review round 3 (branch feat/ui-r3-board-surface).
1. Read AGENTS.md, design §3.2/§7, dispatch §1/§3 (code-quality mandate applies), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) ghost binds real captured size — no window-sized Rectangle residue; (b) the preset dispatch uses the EXISTING CreateStack commands (no domain diff); a second test preset exists proving extensibility; (c) the checkbox dispatches SetTaskDone both directions and cannot desync from the model (no local-only state); (d) empty-board changes do not disturb chunk-1 flow rules (flows journeys green); (e) zero slint deprecation warnings in the build log; (f) no text assertions; empty diff over domain/state/storage/sync.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 4. Chunk 3 — Task view & dates (`feat/ui-r3-taskview-dates`)

- **Covers**: design §3.3 (D7–D15) + fold-ins #230/#228. Non-goals:
  no settings fields (chunk 4), no picker preset bindings (chunk 6 —
  this chunk keeps the existing arrow/Enter/Escape keys working and
  syncs month-follow).
- **Acceptance**: (a) due field shows interactive grammar + hover +
  click-opens; (b) scheduled-date editor parity with due (D8);
  (c) time grammar: `YYYY-MM-DD HH:MM` parses to the right instant,
  date-only keeps 00:00, echo shows time when set (proptest);
  (d) picker opens by default on entering due-edit, mounts as
  overlay anchored to the field, pushes nothing (geometry pin);
  (e) editor open seeds the buffer with the set value; the x-clear
  button clears; Enter with untouched value re-commits it unchanged
  (no accidental clears — journey); (f) month follows the cursor;
  month buttons keyboard-activatable (Space/Enter via the control
  walk or direct keys — chunk 6 refines bindings); (g) reminder rows
  humanized (structure pins); (h) due-soon horizon config knob
  consumed by presentation (config round-trip test); (i) #228: only
  if `TaskChanges` supports description — else the chunk ships
  without it and files the deferral note (stage-1 gate).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of GUI review round 3 ("task view & dates", branch feat/ui-r3-taskview-dates).
1. Read AGENTS.md, design round §3.3 (D7–D15, normative) + §1 findings 13/14/15/16/17/28/29 + §7-R2, dispatch §1/§4. Precondition: chunk 2 merged; STOP if not.
2. Read the seams: ui/views/task-view.slint (due field 146-160, title 88-118), ui/compositions/due-editor.slint (79-181), src/due_edit.rs (parse 42-45, DuePickerState 100-103, move_days 129-138), src/adapter.rs (due seams 3129-3275), ui/widgets/text-field.slint (trailing affordance 30-34/90-110), src/models.rs (rule rows 923-926), src/presentation.rs (due helpers, no humanizer), domain READ-ONLY: entities.rs 412-416 (duedate/scheduled), messages.rs 320-326 (TaskChanges.scheduled; verify description support), ui/views/today-view.slint (due-soon consumption), config [ui] section (UiSection). Tests: due_edit/unit, journeys due sections.
3. scripts/worktree-doctor.sh; then create ../taskboard-WT-feat-ui-r3-taskview-dates on a new branch.
4. Plan at .artifacts/plans/2026-10-26-ui-r3-taskview-dates-plan.md settling: the extended grammar (exact accepted forms incl. time; validation); scheduled editor (widget reuse, seams, model wiring); overlay picker (mount point, anchor geometry source, dismiss layer, default-open on edit entry — interplay with chunk-1 flow rules); buffer seeding + x-clear (the trailing-affordance variant API); month-follow fix; month-button keyboard activation (direct keys now, registry rows later — record the boundary with chunk 6); reminder-row presentation (exact formats); due_soon_days config plumbing; description verify verdict (if supported: widget, seams; else STOP-defer note); test catalogue + Tier-4 (overlay feel, time-entry feel).
5. Commit: docs(plan): task view dates & description. No implement, no push.
Report: plan path, decisions (esp. description verify verdict, overlay anchor mechanism), risks, diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of GUI review round 3 ("task view & dates").
1. Read AGENTS.md, design §3.3, dispatch §1/§4, stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-taskview-dates-plan.md) in ../taskboard-WT-feat-ui-r3-taskview-dates (recreate if missing).
2. Implement per plan, TDD, standalone commits; no domain edits (scheduled/description ride existing TaskChanges — a missing field is a STOP per plan); no text assertions; picker overlay must respect min-viewport (clamp like the main menu).
3. Full gate suite green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items, deviations (explicitly: did #228 ship?).
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of GUI review round 3 (branch feat/ui-r3-taskview-dates).
1. Read AGENTS.md, design §3.3/§7-R2, dispatch §1/§4 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) ONE commit seam for due AND scheduled (no parallel dispatch paths); (b) the accidental-clear regression is pinned (open editor → Enter unchanged value must not clear); (c) overlay picker closes on editor close/view switch/Escape and never traps pointer events outside (dismiss layer); (d) time grammar cannot produce a date-shifted instant (UTC/local handling pinned by tests); (e) month-follow cannot desync via set_cursor/move_months paths; (f) no domain diff; (g) no text assertions (reminder-row formats asserted structurally/length-bounded, not by string).
3-7. Mandate, gates, fixes, ship, teardown per §1. Report verdict, findings, fixes, PR URL, confirmations, Tier-4 items, and whether #228 shipped (if yes: post its ✅ DONE comment after green).
```

## 5. Chunk 4 — Settings polish (`feat/ui-r3-settings-polish`)

- **Covers**: design §3.4 (D16–D20). Non-goals: no connection logic,
  no migration (chunk 5), no status-text changes.
- **Acceptance**: (a) eye toggle switches input-type, never
  persists/logs the revealed value beyond the field (structural);
  (b) InfoTip frames size from help text (geometry pin); (c) status
  line outside the Flickable, visible at min viewport + scale 3.0
  (journey); (d) inline validation: invalid border + error line per
  keystroke, valid border when it passes, neutral when empty
  (journeys per field; validator unit tests); (e) field↔slider sync
  both directions (journeys); (f) slider drag completes across
  preview re-layouts (journey + probe recorded; fallback
  preview-on-release if the probe fails — recorded decision);
  (g) scale commit anchors scroll on the scale row (journey incl.
  downward change).

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of GUI review round 3 ("settings polish", branch feat/ui-r3-settings-polish).
1. Read AGENTS.md, design round §3.4 (D16–D20, normative) + §1 findings 3/19/20/21/23/24/25 + §7-R2/R3, dispatch §1/§5. Precondition: chunk 3 merged; STOP if not.
2. Read the seams: ui/widgets/text-field.slint (secret 84, border 38-41, trailing 90-110), ui/widgets/info-tip.slint (57-65), ui/widgets/tooltip.slint (95-105), ui/widgets/slider.slint (98-115), ui/views/settings-view.slint (scale field 551-561, status 569-585, flick 263, content 284), src/adapter.rs (settings_field_edited 1620-1638, scale preview/commit 1931-2014, push diff 1504), src/settings_form.rs (ladder), src/settings_status.rs. Tests: journeys settings 1981+, ui_settings.rs (unchanged).
3. scripts/worktree-doctor.sh; ../taskboard-WT-feat-ui-r3-settings-polish on a new branch.
4. Plan at .artifacts/plans/2026-10-26-ui-r3-settings-polish-plan.md settling: the validity flavour API (states, border tokens, error-line slot, per-field validator wiring from settings_form, when validation runs); eye toggle anatomy; InfoTip/tooltip width mode (which term carries the help text); status placement (layout shape); scale: the echo property + writer points; the slider-grab probe design (mechanism hypothesis: Flickable steal — how to verify on pinned Slint; fix candidates: scroll-disable-while-pressed / row outside flick / grab-priority; fallback preview-on-release); scroll anchoring (how the scale row's viewport offset is captured/restored across a base-scale change); test catalogue + Tier-4 (drag feel, validation feel).
5. Commit: docs(plan): settings polish. No implement, no push.
Report: plan path, decisions, risks, diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of GUI review round 3 ("settings polish").
1. Read AGENTS.md, design §3.4, dispatch §1/§5, stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-settings-polish-plan.md) in ../taskboard-WT-feat-ui-r3-settings-polish (recreate if missing).
2. Implement per plan, TDD, standalone commits; validators live beside settings_form (pure, unit-tested); error copy = presentation constants; no text assertions; secrets discipline untouched.
3. Full gate suite green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, the slider probe verdict (preview kept or fallback), Tier-4 items, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of GUI review round 3 (branch feat/ui-r3-settings-polish).
1. Read AGENTS.md, design §3.4/§7-R2/R3, dispatch §1/§5 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) validation cannot fight the Save gate (two sources of truth reconciled — validators derive from the same ladder); (b) the eye toggle never lands the password on any model/Debug/log (redaction pins untouched); (c) the slider fix is probe-backed, and if the fallback shipped, preview-on-release is pinned as the recorded behaviour; (d) scroll anchoring works for both directions and does not fight the user's manual scrolling afterwards; (e) status line placement survives scale 3.0 + min window (journey); (f) no text assertions; app-crate diff empty.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 6. Chunk 5 — Legacy config migration (`feat/ui-r3-legacy-config-migration`)

- **Covers**: design §3.5 (D21) + §7-R5. Independent — no
  precondition beyond the artefacts merge.
- **Acceptance**: (a) legacy config loads, migrates, persists via
  overlay (content test: connectors array correct, `[nextcloud]`
  gone, unrelated tables/comments preserved); (b) backup file
  written best-effort; (c) persist-failure falls back to in-memory
  migration with a warn + later overlay completion; (d) secrets
  untouched (store keyed server+username — pin); (e) the
  `LegacyConfig` typed rejection + UI status text + read-only row
  gating become dead and are removed with their tests; (f) REPL
  connect ceremonies on a legacy config also migrate (same path);
  (g) existing config tests green or recorded-decision.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of GUI review round 3 ("legacy config migration", branch feat/ui-r3-legacy-config-migration).
1. Read AGENTS.md, design round §3.5 (D21, normative) + §1 finding 22 + §7-R5, dispatch §1/§6.
2. Read the seams: taskboard-app/src/config.rs (figment 1129-1134, load 751, extract_raw 808, validate 856-868 incl. the mixed-shape rejection, atomic write 1221), connectors.rs (legacy synthesis 29), connections.rs (overlay_upsert_connector 182), ui_requests.rs (LegacyConfig gates 317-320/392/446, read_raw_config 670), taskboard-ui-slint/src/settings_status.rs (50), settings-view read-only gating (44-46/370/382/390). Tests: config tests, connections overlay tests, ui_settings legacy refusal tests (283).
3. scripts/worktree-doctor.sh; ../taskboard-WT-feat-ui-r3-legacy-config-migration on a new branch.
4. Plan at .artifacts/plans/2026-10-26-ui-r3-legacy-config-migration-plan.md settling: migration function home + signature (pure core + IO shell), exact file transformation (table strip, entry synthesis incl. name choice), when it runs (load? first overlay write? both), backup naming + failure policy, the in-memory fallback path, removal inventory (rejection gates, status variant, UI text, read-only gating, tests), REPL-path verification, test catalogue (round-trip, idempotence — second load must not duplicate, mixed-shape refusal stays for hand-made files).
5. Commit: docs(plan): legacy config auto-migration. No implement, no push.
Report: plan path, decisions, risks, diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of GUI review round 3 ("legacy config migration").
1. Read AGENTS.md, design §3.5, dispatch §1/§6, stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-legacy-config-migration-plan.md) in ../taskboard-WT-feat-ui-r3-legacy-config-migration (recreate if missing).
2. Implement per plan, TDD, standalone commits; the migration core is pure and unit-tested (property: migrating twice is identity); info log carries names/hosts only.
3. Full gate suite green (removed tests replaced by migration tests); CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of GUI review round 3 (branch feat/ui-r3-legacy-config-migration).
1. Read AGENTS.md, design §3.5/§7-R5, dispatch §1/§6 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) idempotence pinned (no duplicate connector on reload); (b) comments/unrelated tables preserved byte-wise where toml_edit guarantees; (c) no secret ever lands in the toml or logs; (d) the rejection removal is complete (grep for LegacyConfig residue — a dead variant left behind is a finding); (e) REPL + settings + save ceremonies all exercised over the migrated shape (wiremock journeys).
3-7. Mandate, gates, fixes, ship, teardown per §1. Report verdict, findings, fixes, PR URL, confirmations.
```

## 7. Chunk 6 — Navigation, palette, menu (`feat/ui-r3-navigation-menu`)

- **Covers**: design §3.6 (D22–D29) + fold-in #231. Precondition:
  chunk 3 merged (picker keys), chunk 1 merged (flow walk).
- **Acceptance**: (a) toolbar switcher (option A) drives
  ViewToday/NavFocusBoard with selected-state sync from view_state
  (journeys; if the user redirected at plan review, that option's
  mechanics ship instead); (b) `nav.show-task` navigates
  board+stack+focus+scroll from ANY state incl. compact (journeys;
  palette Enter uses it; wide-layout + view-switch halves verified);
  (c) palette: input row fixed height, results below (geometry pin);
  (d) control-nav rows bound in all presets (contracts green,
  snapshots regen) walking fields/buttons/picker; date picker drops
  hardcoded arrow grabs and joins the walk; (e) scroll actions bind
  emacs C-v/M-v, standard PgDn/PgUp, vi Ctrl+f/Ctrl+b, scrolling the
  focused area (journeys); (f) menu: submenu re-rank, preset-irrelevant
  rows filtered, dynamic greying for state-dependent rows (structural
  pins), hamburger chip hover-only; (g) emacs double-C-g aborts
  pending prefixes (keymap test + snapshot); (h) #231 bindings land
  (snapshots).

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of GUI review round 3 ("navigation, palette, menu", branch feat/ui-r3-navigation-menu).
1. Read AGENTS.md, design round §3.6 (D22–D29, normative) + §1 findings 27/32/33/34/35/36/37/38/39/40 + §7-R3, dispatch §1/§7. Precondition: chunks 1 and 3 merged; STOP if not.
2. Read the seams: ui/compositions/toolbar.slint (38-76), ui/widgets/segmented-control.slint, src/view_state.rs (palette_jump 1173-1212, select_board 484-493, focus walk, FocusScrollHint plumbing src/adapter.rs 789/822), ui/app-window.slint (palette 464-475, overlays), ui/compositions/search-palette.slint, src/menu.rs (root_page 69-98, group_page 103-111, object_entries 163-165, MenuEntry 29), src/actions.rs (menu_rank/group 305/315, TaskMoveCancel 717-724, ModeNormal 830-837), src/keymap (prefix machinery, C-g), ui/compositions/due-editor.slint (84-115 hardcoded keys), ui/widgets/icon-button.slint (57-66), src/keymap/presets.rs + snapshots, registry rows for reminder snooze/mute.
3. scripts/worktree-doctor.sh; ../taskboard-WT-feat-ui-r3-navigation-menu on a new branch.
4. Plan at .artifacts/plans/2026-10-26-ui-r3-navigation-menu-plan.md settling: switcher wiring (model ↔ view_state sync both ways incl. settings return); show_task semantics (board select, stack select, focus, scroll request; name + registry row; palette integration; what happens when the task is on another board or done); palette layout numbers; control-nav walk definition (what counts as a control per view; interaction with text-editing chains — outside Insert); per-preset binding table for ALL new rows (conflict-contract checked, C-f/C-b board-context preservation); scroll action targets (which container per view; FocusScrollHint vs container API); menu changes (re-rank table, filter rule, dynamic enabled plumbing shape); C-g C-g keymap mechanism; snapshot regen list; test catalogue + Tier-4 (switcher feel, walk feel).
5. Commit: docs(plan): navigation, palette & menu curation. No implement, no push.
Report: plan path, decisions (binding table), risks, diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of GUI review round 3 ("navigation, palette, menu").
1. Read AGENTS.md, design §3.6, dispatch §1/§7, stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-navigation-menu-plan.md) in ../taskboard-WT-feat-ui-r3-navigation-menu (recreate if missing).
2. Implement per plan, TDD, standalone commits; snapshots regenerate in the same commits as their rows; new labels/descriptions = presentation constants; no text assertions.
3. Full gate suite green (conflict/shadow contracts unweakened); CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, snapshot diffs, Tier-4 items, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of GUI review round 3 (branch feat/ui-r3-navigation-menu).
1. Read AGENTS.md, design §3.6/§7-R3, dispatch §1/§7 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) show_task works from every starting view and for cross-board targets (the palette bug's exact repro pinned); (b) control-nav never hijacks keys inside active text flows (chunk-1 rules hold); (c) all-preset coverage for every new row + snapshots exact; (d) menu greying is adapter-driven state, not per-frame re-derivation of the registry; preset filtering happens at page build; (e) the picker's arrows now come from the walk/registry — no Key.* literals left in due-editor.slint (grep); (f) C-g C-g cannot break normal C-g cancel (both pinned); (g) no text assertions; domain diff empty.
3-7. Mandate, gates, fixes, ship, teardown per §1. Then post #231's ✅ DONE comment. Report verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 8. Chunk 7 — Reminder OS-delivery (`fix/ui-r3-reminder-delivery`)

- **Covers**: design §3.7 (D30–D32) + the DR-3 fence. Independent of
  chunks 1–6.
- **Acceptance**: (a) a recorded live probe on this machine (session
  bus, bare notify-rust toast, portal question) with the verdict in
  the plan; (b) the fix per verdict ships (portal backend or
  transport/env repair) with the toast landing in KDE (Tier-4 manual
  item on this machine); (c) delivery attempts/outcomes flow through
  a LogSink-testable seam (CI pins logic without a daemon); failures
  surface to the user (status/today surface) and are never
  warn-and-swallow-only; (d) the DR-3 fence recorded in code comments
  + the chunk issue notes (settlement stays DR-3 chunk 1); (e) no
  domain/state diffs.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of GUI review round 3 ("reminder OS delivery", branch fix/ui-r3-reminder-delivery).
1. Read AGENTS.md, design round §3.7 (D30–D32, normative), the DR-3 plan's fence lines (.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md §1 end + §3.1 fence), dispatch §1/§8.
2. Read the seams: taskboard-app/src/notifications.rs (DesktopSink 159-199, resolve_sink 283-290, spawn_notifier 297-332), bootstrap wiring 782-786/857-871, config reminders section (timer_armed actor.rs:373, enabled default config.rs:293-323), the in-app surface feed (models.rs:421, presentation.rs:378, domain reminders.rs:768/803-815 — read-only; the prune cascade is DR-3's). Then run the LIVE PROBE on this machine: (a) echo $XDG_CURRENT_DESKTOP/$XDG_SESSION_TYPE/$DBUS_SESSION_BUS_ADDRESS from a login-like shell; (b) a tiny cargo script or existing binary path sending a bare notify-rust toast — did it land in KDE? (c) check for xdg-desktop-portal availability + the org.freedesktop.portal.Notifications path; (d) capture the app's own log line on a forced delivery attempt.
3. scripts/worktree-doctor.sh; ../taskboard-WT-fix-ui-r3-reminder-delivery on a new branch.
4. Plan at .artifacts/plans/2026-10-26-ui-r3-reminder-delivery-plan.md recording: the probe method + results (evidence per the artefact mandate), the root-cause verdict, the chosen fix (portal backend vs transport/env) with the rejected alternative recorded, the delivery-seam shape (LogSink-testable attempt/outcome events), the user-visible failure surfacing point, the DR-3 fence comments' locations, test catalogue (seam pins; the real-toast check listed as Tier-4), risks.
5. Commit: docs(plan): reminder OS delivery plumbing. No implement, no push.
Report: plan path, probe verdict, chosen fix, risks, diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of GUI review round 3 ("reminder OS delivery").
1. Read AGENTS.md, design §3.7, dispatch §1/§8, stage-1 plan (.artifacts/plans/2026-10-26-ui-r3-reminder-delivery-plan.md) in ../taskboard-WT-fix-ui-r3-reminder-delivery (recreate if missing).
2. Implement per plan: the delivery seam (typed attempt/outcome, LogSink-testable), the fix, failure surfacing, fence comments. TDD where logic exists; no sleeps (mock clock); no domain/state diffs.
3. Full gate suite green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items (the on-this-machine KDE toast checklist), deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of GUI review round 3 (branch fix/ui-r3-reminder-delivery).
1. Read AGENTS.md, design §3.7, the DR-3 fence, dispatch §1/§8 (evidence & artefact mandate applies to the probe; code-quality to the code), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) probe evidence is complete and reproducible (commands + observed outcomes in the plan); (b) failures can no longer vanish (every delivery attempt ends in a typed outcome that reaches a user-visible surface or the log at warn+); (c) the seam is daemon-independent (CI green without KDE); (d) the DR-3 fence is commented at the touched seams and neither round's fix is duplicated; (e) no domain/state diffs; no text assertions.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 9. Completion bookkeeping

- **Closes on merge** (stage 3 posts ✅ DONE + PR link): #228 (chunk
  3, gated on the description verify), #230 (chunk 3), #231 (chunk
  6).
- **Annotates**: #351 (migration half landed via chunk 5; free-form
  editor stays), #474 (chunk 7 notes the delivery half landed;
  settlement stays DR-3 chunk 1), #210 (chunk 2 notes the checkbox
  half shipped; play/pause rides #489/DR-5).
- **Stays open, untouched**: #411, #412, #444, #463, #464, #352-#357
  (DR-3/undo territories), all DR-5/DR-6 issues.
- **New issues**: none planned; stage discoveries go to `backlog:`
  per AGENTS §7.
- **What opens next**: GUI review round 4 over this round's merges +
  the DR rounds' surfaces.

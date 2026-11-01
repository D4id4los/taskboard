# [Plan] GUI Review Round 4 — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-01
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **11** chunks of the GUI review
  round 4 fix round (board surface, column resizing, icon assets,
  hover machinery, task-view layout & focus, date/time entry, task
  reminders, timer surfaces, tracking surfaces, settings
  connections). No code in this commit.
- **Origin**:
  `.artifacts/plans/2026-11-01-gui-review-round4-plan.md` — findings
  R4-F1…F29 (§1), recorded decisions §2 (all six confirmed — 1–4 by
  the user, 2026-11-01), designs
  §3 (D1–D40), testing §4, inventory §5, slicing §6, risks §7,
  bookkeeping §8 — normative. Round-3 plans remain background
  (`2026-10-26-gui-review-round3-plan.md` + its dispatch plan for the
  shared execution model style).
- **Status**: READY — the design round and all four §2 decisions are
  accepted by the user (2026-11-01: Lucide SVGs; at/in task
  reminders; hybrid time combobox; backlog absorption). The backlog
  round-2 plan amendment (dropping its chunks 2.5/3.4) landed in the
  same docs branch. All 11 chunks are dispatchable in the order
  below; the per-chunk DECISION GATE lines in the prompts pass by
  construction and remain as tripwires.

## 1. Chunks & ordering

| Chunk | Design | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §3.1 (D1–D4) | `fix/ui-r4-board-surface` | card-title truncation, right-click closes menu + all overlay dismissal layers, column-background left-click selects stack, single-click add-task | M |
| 2 | §3.2 (D5–D8) | `feat/ui-r4-column-resize` | divider redistribution model, content minimums, slack distribution, keyboard sibling | L |
| 3 | §3.3 (D9–D12) | `feat/ui-r4-icon-assets` | Lucide SVG assets + `Icon`/colorize component, IconKind migration, distinct notification icons, D19 amendment | M |
| 4 | §3.4 (D13–D15) | `fix/ui-r4-hover-fixes` | tooltip empty-line, ghost tooltips on view transitions (total dismiss seam), hints-at-start investigation | M |
| 5 | §3.5 (D16–D17) | `fix/ui-r4-taskview-layout` | one header row (back + checkbox + title + tracking), description stretch, tracked-time summary + per-second tick seam | L |
| 6 | §3.6 (D18–D21) | `fix/ui-r4-taskview-focus` | full tab-stop inventory + focus visuals, emacs four-key walk, focus-driven insert engagement (systemic Enter fix), hover sweep | L |
| 7 | §3.7 (D22–D28) | `feat/ui-r4-datetime-entry` | picker flip, steppers-from-empty, click-away commit, two-phase emacs picker + live sync, time combobox, offset validation, zone unification | L |
| 8 | §3.8 (D29–D31) | `feat/ui-r4-task-reminders` | at/in reminder kind + migration 0018, armed-state rows + countdown/ISO, un-mute + muted indicator | L |
| 9 | §3.9 (D32–D34) | `feat/ui-r4-timer-surfaces` | timer countdown + ISO display, mouse setter popover, #352 remainder | M |
| 10 | §3.10 (D35–D37) | `feat/ui-r4-tracking-surfaces` | entries list + add/edit/delete over LogTime/Adjust/Delete, #490 today totals | L |
| 11 | §3.11 (D38–D40) | `feat/ui-r4-settings-connections` | scrap scale instant preview, connections dropdown + indicator, #411 board-switcher adoption | M |

**Dependency graph**: serial default 1 → 11. Hard gates: 2 on 1
(board files); 6 and 7 on 5 (task-view files); 8 on 6 (insert
engagement); 9 on 7 + 3; 10 on 7 + 5 + 3; 11 on 3 (icons). Each
stage 1 runs the **precondition gate**: `git fetch origin --prune`
and verify its gate chunks' PRs merged into `origin/main`; if not,
STOP and report. Parallel lanes permitted on the user's direction
(worktree budget via `scripts/worktree-doctor.sh`): 3 ∥ 4; 11 ∥ 5–8;
9 ∥ 10 after their gates. **Decision gates** (Status above) bind
chunks 3, 7, 8, 9, 10 — a stage-1 agent finding its §2 decision
unconfirmed in the origin doc STOPs and reports.

**Backlog queue coordination** (binding, origin §2 decision 4,
confirmed): backlog round-2 chunks 2.5 (#352) and 3.4 (#490/#491)
are absorbed by chunks 9/10 here and must not be dispatched from the
backlog plan (its 2026-11-01 amendment — same docs branch — records
the absorption); all other backlog round-2 chunks stand; `models.rs` rebase friction with the
render lane (2.1/4.1) is expected — standard rebase dance.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch>` (slashes as
dashes) next to the project root (AGENTS §8), branched off the
then-current `main`, no PR ever stacked. **Branch names are never
reused after their PR lands** (AGENTS §8, issue #415): a resume or
second round works on `<branch>-v2` or re-branches off the fetched
`origin/main`. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never without `-b`, never on `main`).
Disk hygiene: any agent creating or recreating a worktree runs
`scripts/worktree-doctor.sh` from the main checkout first — nonzero
exit stops the chunk; stage 3 removes the worktree as its final
handoff task.

**Shared execution model** — identical to the round-3 dispatch plan
§1 (`.artifacts/plans/2026-10-26-gui-review-round3-implementation-dispatch-plan.md`),
which itself defers to the round-2 dispatch plan §1: stage 1 plans
TDD-first in the worktree (`docs(plan): …`, no push); stage 2
implements (TDD, standalone commits, CHANGELOG Unreleased, full
AGENTS §10 gate suite green via `scripts/run-quality-gates.sh`, no
push/PR); stage 3 begins with the rebase onto `origin/main` (rebase
dance, `--force-with-lease`), reviews the full post-rebase diff
against acceptance + the code-quality mandate, fixes on the branch,
ships (push, PR, `gh pr checks --watch` to green,
`mergeStateStatus: CLEAN`), then tears the worktree down (absolute
anchor, `git worktree list` confirmation). Never merges; never
touches `main`.

**Code-quality mandate** (stage 3, every chunk): copy the verbatim
text from the round-2 dispatch plan §1 "Code-quality mandate"
(`.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`),
including the no-text-assertions clause.

**Blanket acceptance rule** (every chunk that adds or reworks an
interactive element — 5, 6, 7, 8, 9, 10, 11): every new interactive
element ships (a) a hover tip (label + description + hint keys where
bound), (b) tab-walk coverage, (c) dispatch through the action layer
(no inline side effects), per origin D21.

## 2. Chunk 1 — Board surface repairs (`fix/ui-r4-board-surface`)

- **Covers**: origin §3.1 (D1–D4), findings F2/F6/F7/F8. Non-goals:
  no column-width model change (chunk 2), no icon changes (chunk 3),
  no tooltip machinery (chunk 4).
- **Acceptance**: (a) board + today cards render the title across the
  full card width, eliding only at the card's right edge (goldens at
  the fixture widths regenerate with full titles; a
  narrow-viewport golden still elides at the edge); (b) right-click
  outside an open context menu closes it, and the same arm exists on
  the date-picker, palette, and capture dismissal layers (journeys);
  (c) left-click on a column's empty body selects the stack (same
  state as clicking the header — journey pins selected-stack + first
  task focus); (d) one click on any column's Add-task ghost opens
  the create field *in that column* and selects that column, under
  `Flow::None`, under a foreign flow (settle-then-run, no
  settle-nothing skip), and with focus elsewhere (journeys; the
  flow-table test updated for the targeted `BeginCreate` shape);
  (e) Ctrl+O behaviour unchanged (journey).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of GUI review round 4 ("board surface repairs", branch fix/ui-r4-board-surface) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-11-01-gui-review-round4-plan.md §3.1 (D1–D4, normative) + §1 findings 2/6/7/8 + §4/§7, and the dispatch plan §1/§2.
2. Read the seams: ui/widgets/task-card.slint (title 159-171, checkbox 128-130), ui/theme.slint (RoleText 155-164, space tokens), ui/compositions/board-layout.slint (delegate width 531-554, drop-zone 290-294, column body 238/407, header click 363-364, body right-click 308-315, add-task ghost 583-620/828-862), ui/app-window.slint (dismissal layers 441-446/476-481/570-581), src/adapter.rs (select_column 4035-4048, settle_pointer_conflict 4009-4012, add-task ghost path 3953-3961, interpret BeginCreate arm 994-1000, conflict matrix 1484-1518/1518, width press/drag 3239-3316), src/view_state.rs (select_stack 952-956, create_target_stack 1072-1088, begin_create 1182-1186), src/router.rs (TaskCreate 488), src/models.rs (create_open 926). Tests: flow/interpret table tests, board journeys, goldens (tests/goldens/board-*.png regenerate).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then git worktree add -b fix/ui-r4-board-surface ../taskboard-WT-fix-ui-r4-board-surface off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-01-ui-r4-board-surface-plan.md settling: the title-fill mechanism (stretch + min-width token — name the token and its derivation; how the board delegate's width:100% interacts with the column min after chunk 2 lands — note but do not implement); the dismissal-layer right-click arm (exact pointer-event condition, all four layers); the column-body left-click seam (touch hierarchy vs cards and drag zones — how the body area avoids stealing card clicks; interaction with settle_pointer_conflict); the targeted BeginCreate effect shape (Option<StackId> field vs new effect variant — one call, with the flow-table totality impact enumerated) + dropping the settle-nothing skip for pointer-originated BeginCreate + select-on-click; the test catalogue (journeys above + regenerated goldens) + Tier-4 (resize-gesture feel untouched, click feel).
5. Commit: docs(plan): board surface repairs. Do not implement; do not push.
Report: plan path, decisions (esp. the BeginCreate shape), risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of GUI review round 4 ("board surface repairs").
1. Read AGENTS.md, design §3.1, dispatch §1/§2, and the stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-board-surface-plan.md) in ../taskboard-WT-fix-ui-r4-board-surface (recreate on the branch if missing; -v2 if the branch's PR landed).
2. Implement per the plan, TDD, every commit standalone. New user-visible strings are presentation constants; no text assertions; time-independent tests only.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG Unreleased updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items, deviations.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of GUI review round 4 (branch fix/ui-r4-board-surface).
1. Read AGENTS.md, design §3.1/§4/§7, dispatch §1/§2 (code-quality mandate per §1), the stage-1 plan; review the full post-rebase branch diff.
2. Functional focus: (a) the title fix cannot reintroduce the ellipsis-minimum squeeze (the golden with a long title at min viewport still elides at the card edge, never mid-card); (b) the body left-click cannot swallow card clicks or break drag-to-move (journeys green); (c) the targeted BeginCreate cannot regress Ctrl+O (focused→selected→first resolution for key origin unchanged); (d) the dismiss-layer right-click arm fires on down, not click (no menu-flash), and the menu does not reopen from the same event; (e) flow-table totality kept (no new wildcard arm); (f) no text assertions; empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship (PR + checks green + mergeStateStatus CLEAN), teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 3. Chunk 2 — Column-resize divider model (`feat/ui-r4-column-resize`)

- **Covers**: origin §3.2 (D5–D8), finding F3 + §2 decision 6.
  Non-goals: no horizontal scrolling (the row clips at minimums —
  recorded fallback), no persistence-lane changes beyond semantics.
- **Acceptance**: (a) a divider between two columns drags to
  redistribute space between exactly those two; other columns' edges
  do not move (journey with ≥3 columns, property test: sum
  preserved); (b) minimum widths derive from content (header +
  chrome + readability floor), demonstrably smaller than today's
  160 px (unit test with a short header); (c) window resize
  distributes delta proportionally; stack add/remove rebalances;
  Σmins > viewport clamps to minimums (property tests); (d) keyboard
  ±32 adjusts the focused column against its neighbour (journey);
  (e) the width snapshot persistence round-trip still holds
  (existing tests adapted, not weakened); (f) the resize gesture
  feels anchored: the dragged boundary tracks the pointer, nothing
  else moves (Tier-4).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of GUI review round 4 ("column-resize divider model", branch feat/ui-r4-column-resize) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify chunk 1's PR merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, design round §3.2 (D5–D8, normative) + §2 decision 6 + §1 finding 3 + §7-R1, dispatch §1/§3.
2. Read the seams: ui/compositions/board-layout.slint (columns row 264-294, resize-handle 396-415, callback chain 129-131, header), ui/views/board-view.slint (118-120), ui/app-window.slint (271-273), src/widths.rs (entire pure module: floor 11-15, step 22, ceiling 56-59, heal 30-33), src/adapter.rs (width_drag 202, press/drag/release 3239-3316, effective-minimums 695-740, settle kills buffer 758-759), src/view_state.rs (stack_widths 272/1099-1131, keyboard resize 1142-1179), src/models.rs (width-px push 855-872/932), taskboard-app/src/ui_requests.rs (debounce 91-114). Tests: widths property tests, width persistence journeys.
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-column-resize ../taskboard-WT-feat-ui-r4-column-resize off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-column-resize-plan.md settling: the explicit-width row model (how the HorizontalLayout renders fixed widths summing to the row; what happens to horizontal-stretch/max-width on the drop-zone); the divider element (one handle per boundary — header-only or full-height hit area; hover/cursor affordance; which callback carries (left-index, delta)); the redistribution math in widths.rs (pure functions: redistribute(total, widths, i, delta, mins) — clamps, slack distribution on resize/add/remove, proportional rebalance; enumerate the cases); the content-minimum formula (header preferred width measurement source + chrome + readability floor constant — name it); the auto-sentinel's fate (widths.rs/adapter.rs −1.0 auto columns — keep, redefine, or kill; one call, recorded); the keyboard sibling semantics incl. the last-column edge; the migration of existing persisted widths (sub-160 old floors heal today via widths.rs:30-33 — decide the new heal); the probe: gesture feel on the reference machine if available, else Tier-4; the test catalogue (property suite + journeys + persistence round-trip).
5. Commit: docs(plan): divider column resizing. No implement, no push.
Report: plan path, decisions (esp. the widths.rs API shape and the auto-sentinel call), risks, diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of GUI review round 4 ("column-resize divider model").
1. Read AGENTS.md, design §3.2, dispatch §1/§3, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-column-resize-plan.md) in ../taskboard-WT-feat-ui-r4-column-resize (recreate if missing).
2. Implement per the plan, TDD, standalone commits; widths.rs stays pure with the property suite; persistence-lane changes are semantic only (no new requests); no text assertions.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of GUI review round 4 (branch feat/ui-r4-column-resize).
1. Read AGENTS.md, design §3.2/§7-R1, dispatch §1/§3 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) redistribution is sum-preserving by property, not by convention; (b) no stretch residue on the columns row (grep horizontal-stretch in board-layout column code); (c) the 160 px constant is gone or re-purposed with a recorded reason; (d) persisted-width round-trip incl. legacy sub-floor values heals, never panics (property test); (e) the gesture cannot move a non-adjacent column (journey with 4+ columns); (f) persistence still snapshots once per gesture (no request storm — existing debounce tests green); (g) no text assertions; empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items (drag feel, keyboard feel).
```

## 4. Chunk 3 — Icon assets (`feat/ui-r4-icon-assets`)

- **Covers**: origin §3.3 (D9–D12), finding F1 + §2 decision 1
  (**decision gate**: Lucide confirmed). Non-goals: no new icons
  beyond the enumerated set (dismiss, timer/alarm, link + link-off,
  chevron-down, pencil — plus re-mapping the existing 13), no
  non-IconKind icon usage anywhere.
- **Acceptance**: (a) the probe lands first and is referenced by the
  plan (one asset, one IconButton, light/dark tokens, both CI
  renderers); (b) `icon-button.slint` renders via the `Icon`
  component (`@image-url` + `colorize`) — no geometry blocks remain
  (grep); (c) IconKind→asset mapping is total (structural test); the
  13 existing kinds map 1:1 (visual parity spot-checked in goldens);
  (d) "Dismiss notification" and "Mark task done" render distinct
  icons (golden/journey); (e) `assets/icons/` carries the vendored
  SVGs + the ISC license file; binary size delta recorded in the
  report; (f) the D19 amendment lands in icon-button.slint's header
  comment and docs/architecture.org where D19 lives.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of GUI review round 4 ("icon assets", branch feat/ui-r4-icon-assets) in /home/viktoria/src/taskboard.

DECISION GATE: the design round §2 decision 1 must read CONFIRMED (or the user must have redirected to an alternative set) before you plan; if it still says PROPOSED with no user verdict recorded in the doc or the dispatching session, STOP and report.

1. Read AGENTS.md, design round §3.3 (D9–D12, normative) + §2 decision 1 + §1 finding 1 + §7-R2, dispatch §1/§4.
2. Read the seams: ui/widgets/icon-button.slint (entire: vocabulary 14-36, tone map 89-102, geometry blocks 104-309, focus surface 63-84, tooltip arm 317-323), ui/compositions/notification-surface.slint (55-110: both check uses), every IconKind call site (grep — due-editor 55, timers 87, settings-view 319, date-picker 103-140, habits 76-88, alarm-window 53-67, status-bar 155-163/320, rule-list 66, task-view 159/324/330, toolbar 94, board-layout 744), ui/theme.slint (icon metrics, tone tokens), the build (crates/taskboard-ui-slint/build.rs — how ui/ is compiled; confirm @image-url embedding path), docs/architecture.org (the D19 decision text).
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-icon-assets ../taskboard-WT-feat-ui-r4-icon-assets off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-icon-assets-plan.md settling: the vendored set list (existing 13 re-mapped + the new kinds — exact Lucide names, one row each); the Icon component shape (inherits Image, width/height from theme icon metrics, colorize from the tone map — pure function IconKind→image-url); the probe design (render one asset in a test composition across theme tokens; what evidence gates the migration); the LICENSE file + any attribution the ISC requires; the D19 amendment wording (both homes); the golden strategy (which goldens regenerate — notification surface, alarm window, status bar); the acceptance grep (no `if (root.icon ==` geometry blocks). Vendor the SVGs in stage 2, not stage 1 — but list every file in the plan.
5. Commit: docs(plan): svg icon assets. No implement, no push.
Report: plan path, the icon table, risks, diff size (incl. expected binary delta).
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of GUI review round 4 ("icon assets").
1. Read AGENTS.md, design §3.3, dispatch §1/§4, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-icon-assets-plan.md) in ../taskboard-WT-feat-ui-r4-icon-assets (recreate if missing).
2. Implement per the plan, TDD where logic exists (the mapping totality test), standalone commits: probe commit first, then the vendored assets + license, then the component swap, then the new kinds + notification distinction, then the D19 amendment. Regenerate affected goldens in the same commit as the visual change.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated; record the binary size delta (before/after target size) in the report.
4. No push, no PR. Never touch main.
Report: per-gate results, the probe evidence, binary delta, Tier-4 items (icon crispness at scale 0.5/3.0, light/dark), deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of GUI review round 4 (branch feat/ui-r4-icon-assets).
1. Read AGENTS.md, design §3.3/§7-R2, dispatch §1/§4 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) SVG assets are monochrome single-path files (a color SVG would flatten wrong under colorize — check each vendored file); (b) the mapping is total and every call site still compiles against IconKind (no stringly asset references leaking into views); (c) both notification buttons now differ (golden evidence); (d) the license file covers the vendored set and the D19 amendment is consistent in both homes; (e) no dead geometry code left; (f) kiosk memory: no runtime-loaded assets (everything @image-url embedded — grep for Image elements with runtime sources).
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 5. Chunk 4 — Hover machinery fixes (`fix/ui-r4-hover-fixes`)

- **Covers**: origin §3.4 (D13–D15), findings F4/F5/F14. Non-goals:
  no task-view hover coverage (chunk 6), no tooltip content/wording
  changes beyond the layout fix.
- **Acceptance**: (a) an empty description renders no empty line —
  probe first, then the conditional-element fix (probe + layout pin);
  (b) no view transition leaves a tooltip mounted: the enumerate-once
  structural test walks every view-swapping effect arm and asserts
  the dismiss routing; a board→task-view journey pins the user's
  exact repro; (c) the hints-at-start investigation is recorded in
  the chunk plan with method + verdict + evidence, and the fix (or
  the recorded by-design decision with its presentation change)
  lands; (d) tooltip delay/show behaviour otherwise unchanged
  (existing tooltip tests green).

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of GUI review round 4 ("hover machinery fixes", branch fix/ui-r4-hover-fixes) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, design round §3.4 (D13–D15, normative) + §1 findings 4/5/14 + §4, dispatch §1/§5.
2. Read the seams: ui/widgets/tooltip.slint (state 18-75, panel width 98-117, rows 135-169), ui/widgets/info-tip.slint, src/tooltip.rs (placement 34-58), src/lib.rs (delay timer 351-394, TOOLTIP_DELAY_MS 431), src/adapter.rs (dismiss_tooltip call sites 781-784/877-892/1066-1072/1547, apply_effect arms — enumerate every view-swapping effect), src/models.rs (hint projections 782-851, the documented-empty emacs stack-create hint 833-835), src/keymap/mod.rs (first_binding_display 119-138, visible_chain 72-81), src/shell.rs (chain masking), taskboard-app/src/ui.rs (keymap build timing 386-392), src/adapter.rs (settings-reply keymap swap ~2053-2065). Tests: tooltip journeys, keymap snapshot tests.
3. scripts/worktree-doctor.sh; then git worktree add -b fix/ui-r4-hover-fixes ../taskboard-WT-fix-ui-r4-hover-fixes off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-hover-fixes-plan.md settling: the empty-line probe (does an always-mounted visible:false row reserve space on pinned 1.18.1 — how measured: a composition probe or golden) + the conditional-element fix; the total dismiss seam (the enumerate-once list of view-swapping effects — open/close task, view switches, settings, board select — where the dismiss call lives so no future arm can forget it; the structural test shape that enforces totality); the hints-at-start investigation protocol: fresh-boot repro script (emacs preset, which elements, before/after first interaction), the discriminating experiments for the four hypotheses (chain masking at mount; preset-unbound actions; keymap swapped by the settings reply after mount; hint-row visibility), and the fix per verdict — if the verdict is by-design emptiness, the presentation decision (render the action's menu-path hint vs nothing) is recorded; test catalogue.
5. Commit: docs(plan): hover machinery fixes. No implement, no push.
Report: plan path, the investigation protocol, decisions, risks, diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of GUI review round 4 ("hover machinery fixes").
1. Read AGENTS.md, design §3.4, dispatch §1/§5, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-hover-fixes-plan.md) in ../taskboard-WT-fix-ui-r4-hover-fixes (recreate if missing).
2. Implement per the plan, TDD, standalone commits: the probe verdict recorded in the plan (append or amend with a second docs commit if it differed), then the fixes. The investigation runs LIVE: boot the app per the repro script (Tier-4 if headless — then the verdict is seam-analysis with recorded evidence). No text assertions; tooltip copy untouched.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, the hints-at-start verdict + evidence, Tier-4 items, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of GUI review round 4 (branch fix/ui-r4-hover-fixes).
1. Read AGENTS.md, design §3.4/§4, dispatch §1/§5 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) the totality test actually enumerates the effect enum (a new view-swapping effect without a dismiss must fail the test, not slip through a default arm); (b) the empty-line fix cannot collapse the hint row when keys exist (layout pin both ways: description-only, keys-only, both, neither); (c) the hints verdict's evidence is reproducible from the plan's method section; (d) tooltip delay/placement behaviour untouched (existing tests green); (e) no text assertions; empty diff outside ui-slint.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, the hints verdict restated for the user.
```

## 6. Chunk 5 — Task-view layout (`fix/ui-r4-taskview-layout`)

- **Covers**: origin §3.5 (D16–D17), findings F9/F10/F11/F15/F16 +
  §2 decision 5. Non-goals: no focus/navigation changes (chunk 6),
  no entries list (chunk 10), no reminder rows (chunk 8).
- **Acceptance**: (a) one header row: back button inline with the
  title, done/undone checkbox left of the title dispatching
  `SetTaskDone` both directions (click ≡ key journey; done visual
  consistent with cards), Start/Stop tracking at the row's right
  (journeys for both verbs); (b) the title row is content-height —
  no phantom padding (golden); the trailing spacer is gone and the
  description editor absorbs leftover height (golden at two window
  heights); (c) the tracked-time summary row renders the total at
  second precision, live-ticking only while an entry is open
  (journeys: static when closed, ticks on the paused clock when
  open); (d) the per-second repaint condition is mounted-surface
  gated and kiosk behaviour is pinned unchanged (test); (e) the
  blanket rule: the new checkbox/tracking/summary elements ship
  hover tips + walk stops (structural).

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of GUI review round 4 ("task-view layout", branch fix/ui-r4-taskview-layout) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, design round §3.5 (D16–D17, normative) + §2 decision 5 + §1 findings 9/10/11/15/16 + §7-R7, dispatch §1/§6.
2. Read the seams: ui/views/task-view.slint (entire: back row 155-165, title row 166-205, metadata row 208-337, state column + tracking 306-336, rule list mount 342-349, description 357-396, spacer 398), ui/widgets/text-field.slint (multiline flavour 103-110, focus behaviour 130-143), ui/widgets/icon-button.slint, src/presentation.rs (open_elapsed_phrase 551-558, detail_tracking_verb 646-661), src/models.rs (task detail push incl. can-track 1117/1218-1219, timers-visible gating precedent 482), src/lib.rs (repaint driver 470-559, dirty conditions 534-540), src/adapter.rs (REPAINT_CADENCE_MS 48), domain READ-ONLY: taskboard-domain/src/tracking.rs (TimeEntry 36-66, duration 110-124, newest_open 126-138) — the summary reads engine state, no domain change. Tests: task-view journeys, goldens.
3. scripts/worktree-doctor.sh; then git worktree add -b fix/ui-r4-taskview-layout ../taskboard-WT-fix-ui-r4-taskview-layout off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-taskview-layout-plan.md settling: the header-row composition (element order, title stretch, what the closed title element becomes so chunk 6 can make it a focus stop without rework — coordinate the seam); the checkbox anatomy (reuse of the card checkbox pattern, SetTaskDone dispatch, done visual parity); the tracking button's move (its hover/route seams follow it); the description editor's multiline growth (vertical-stretch, min/max lines, commit seams unchanged); the summary row's placement + the pure total helper in presentation.rs (sum incl. open elapsed; second-precision formatter — structural tests only); the tick seam design (the dirty condition: which mounted surfaces count, how "second boundary" is detected, kiosk exclusion — pin the kiosk path); test catalogue + Tier-4 (row heights at scale 0.5/1.5/3.0, tick smoothness).
5. Commit: docs(plan): task view layout rework. No implement, no push.
Report: plan path, decisions, risks, diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of GUI review round 4 ("task-view layout").
1. Read AGENTS.md, design §3.5, dispatch §1/§6, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-taskview-layout-plan.md) in ../taskboard-WT-fix-ui-r4-taskview-layout (recreate if missing).
2. Implement per the plan, TDD, standalone commits; the total helper is pure with unit tests; time-dependent tests use the paused clock; new strings are presentation constants; no text assertions.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of GUI review round 4 (branch fix/ui-r4-taskview-layout).
1. Read AGENTS.md, design §3.5/§7-R7, dispatch §1/§6 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) the checkbox cannot desync from the model (no local-only state; both directions pinned); (b) the tracking move cannot break the status-bar chip or notification-surface start button (their routes are untouched — journeys green); (c) the tick seam cannot fire on kiosk or with no seconds surface mounted (pins); the repaint cost claim is evidenced (the report carries the measured or reasoned delta); (d) the description editor keeps its commit/discard seams (round-3 rules) while stretching; (e) the summary's seconds precision matches D17 (structural formatter tests, no string pins); (f) empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 7. Chunk 6 — Task-view focus & insert engagement (`fix/ui-r4-taskview-focus`)

- **Covers**: origin §3.6 (D18–D21), findings F12/F24/F26.
  Non-goals: no date-picker phase machinery (chunk 7 — but D20's
  masking rule lands here and chunk 7 builds on it), no reminder-row
  content (chunk 8).
- **Acceptance**: (a) every interactive element in the task view is a
  tab stop with a focus visual — closed title, due/scheduled cells,
  description, preset chips, checkbox, tracking, back (structural
  stop inventory + the title-focus journey: tabbing shows the active
  element at every position); (b) emacs C-n/C-p join C-f/C-b as
  control-walk bindings in TaskDetail (conflict contracts green,
  snapshots regenerated; Board bindings untouched); (c) D20 systemic
  rule: a focused text field masks the view context in every view —
  the journey set (field focus → preset Return never hijacks) runs
  across all three presets and at least the reminder field, title
  field, description, and the today timers form; the palette
  exception is enumerated and pinned; (d) the reminder-field Enter
  bug is dead (Enter commits the offset; the checkmark ≡ Enter);
  (e) the hover sweep covers every task-view interactive element
  (structural inventory); (f) no existing field journey weakened.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of GUI review round 4 ("task-view focus & insert engagement", branch fix/ui-r4-taskview-focus) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify chunk 5's PR merged. If not: STOP and report.

1. Read AGENTS.md, design round §3.6 (D18–D21, normative) + §1 findings 12/24/26 + §7-R3, dispatch §1/§7.
2. Read the seams: ui/views/task-view.slint (closed cells 181-205/235-256/275-294/363-384), ui/compositions/rule-list.slint (chips 81-105, custom field 106-118 — the missing edited seam), ui/compositions/timers.slint (the form-edited engage precedent 33-37/105-121), ui/widgets/text-field.slint (inner input 130-143 — where has-focus publishes), ui/app-window.slint (key-root capture 230-244), src/adapter.rs (intercept 389-443, walk re-injection 1120-1127/1254-1282, walk_reentry 3063-3073), src/shell.rs (chain build 66-99, masking), src/actions.rs (CONTROL_NAV rows 268-269/384-394/565-584), src/keymap/presets.rs (emacs TaskDetail rows 328/333-334/395-415, standard 184-205, vi 533-554), tests/walk_probe.slint + walk_probe_tests.rs, the search palette's own chain handling (the named exception). Tests: keymap conflict/shadow contracts, snapshot files, field journeys.
3. scripts/worktree-doctor.sh; then git worktree add -b fix/ui-r4-taskview-focus ../taskboard-WT-fix-ui-r4-taskview-focus off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-taskview-focus-plan.md settling: the focus-stop inventory per view (which elements, which FocusScope shape, the focus-ring visual reuse); the closed-cell activation seams (Enter/Space/click → the existing open-edit effects — no new effects); the emacs binding rows for C-n/C-p (conflict-contract analysis per context; Board untouched); the D20 mechanism: the global editing-facts singleton (declaration home, the text-field.slint publish, how shell.rs consults it, what happens to the per-mount engage seams — which die, which stay for non-TextField inputs; the palette exception and any others enumerated); the interaction with chunk 7's two-phase picker (record the seam contract: the DatePicker chain head stays non-masking and the field-phase masking is exactly D20); the hover sweep inventory (elements + label/description/keys content per element — presentation constants); the test catalogue (stop inventory, walk journeys per preset, the Return-never-hijacks journey matrix, snapshot regen list).
5. Commit: docs(plan): task view focus and insert engagement. No implement, no push.
Report: plan path, decisions (esp. the D20 seam contract), risks, diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of GUI review round 4 ("task-view focus & insert engagement").
1. Read AGENTS.md, design §3.6, dispatch §1/§7, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-taskview-focus-plan.md) in ../taskboard-WT-fix-ui-r4-taskview-focus (recreate if missing).
2. Implement per the plan, TDD, standalone commits; snapshots regenerate in the same commits as their rows; hover copy is presentation constants; no text assertions; conflict/shadow contracts must stay green — a contract failure is a finding to fix, never a test to weaken.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, snapshot diffs summary, Tier-4 items (walk feel, tab order sanity), deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of GUI review round 4 (branch fix/ui-r4-taskview-focus).
1. Read AGENTS.md, design §3.6/§7-R3, dispatch §1/§7 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) D20 is genuinely systemic (grep: no per-mount edited-seam residue for TextField engagement outside the enumerated exceptions; the widget publishes, views cannot forget); (b) the Return-never-hijacks matrix covers all three presets and the reminder-field repro verbatim; (c) the walk reaches every stop in the inventory (the structural test and the manual Tier-4 agree); (d) vi/standard presets unchanged in behavior (snapshots diff only the intended rows); (e) the palette still works (its exception pinned); (f) no text assertions; empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 8. Chunk 7 — Date/time entry (`feat/ui-r4-datetime-entry`)

- **Covers**: origin §3.7 (D22–D28), findings F18/F19/F20/F21/F22/F27
  + §2 decision 3 (**decision gate**: the hybrid combobox confirmed).
  Non-goals: no reminder grammar/kind changes (chunk 8), no timer or
  tracking consumers of the time combobox beyond the date picker
  (chunks 9/10 mount their own).
- **Acceptance**: (a) the picker's height is constant across month
  paging (six-row reserve, or the recorded freeze-placement fallback)
  — no flip mid-interaction (geometry pin across months incl. leap
  February); (b) steppers work from empty (seed decision recorded;
  the existing `date_only_picker_stays_midnight…` pin reconciled);
  typed HH:MM seeds the picker state live; (c) focus-loss/click-away
  on the date fields commits parseable non-empty text, discards and
  restores on empty/non-conforming (journeys; junk text cannot
  linger); (d) the two-phase emacs flow works end to end — the
  user's exact sequence (walk to due → Enter → C-n/p/f/b day pick →
  field focus → type time → Enter commits) is a journey; both phases
  keep field text and picker state in sync (no-oscillation property
  test); (e) the time combobox lands as `ui/widgets/time-field.slint`
  with typed input (lenient parse), steppers, quick-list dropdown,
  full keyboard operation (walk stops, Enter/Esc), and replaces the
  picker's stepper row; (f) offset syntax validation wires
  `parse_text_form` (and the due relative grammar) into the D19
  tri-state validity on the custom-reminder field, the due editor's
  relative form, and the today timers form (validity-state tests);
  (g) the due grammar parses and echoes local time (conversion at
  the seam; storage stays UTC) — zone journeys incl. a DST edge via
  the clock seam; (h) blanket rule (hover + walk stops on every new
  element).

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of GUI review round 4 ("date/time entry", branch feat/ui-r4-datetime-entry) in /home/viktoria/src/taskboard.

DECISION GATE: design round §2 decision 3 (hybrid time combobox) must be confirmed; if still PROPOSED with no user verdict, STOP and report.

GATE: git fetch origin --prune; verify chunks 5 and 6 merged. If not: STOP and report.

1. Read AGENTS.md, design round §3.7 (D22–D28, normative) + §2 decision 3 + §1 findings 18/19/20/21/22/27 + §7-R4, dispatch §1/§8.
2. Read the seams: ui/app-window.slint (picker overlay + dismissal 441-468, the y-flip math 454-460), ui/compositions/date-picker.slint (panel height 63, grid, stepper row 124-144, focus-on-mount 36-45), ui/compositions/due-editor.slint (43-60), src/due_edit.rs (grammar 12-30/92-138, DuePickerState 177+, step_time 247-263, seeding 197-206, grid weeks 312-326, the midnight pin 611-620), src/adapter.rs (due seams 4385-4614: edited/accepted/open/commit funnel, picker key application 4523-4549, default-open 4511-4515), src/shell.rs (DatePicker chain head 54/76-98), src/keymap/presets.rs (picker rows 229-245/433-449/561-581), ui/widgets/text-field.slint (validity 21-25/95-102, trailing affordances), src/settings_form.rs (the validator wiring pattern 73-124/209-235), taskboard-domain/src/reminders.rs READ-ONLY (parse_text_form 108-135), ui/views/today-view.slint + ui/compositions/timers.slint (the form fields that gain validity), ui/views/task-view.slint (anchor geometry 102-120). Tests: due_edit unit + journeys, date-picker goldens.
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-datetime-entry ../taskboard-WT-feat-ui-r4-datetime-entry off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-datetime-entry-plan.md settling: the flip fix (six-row reserve vs freeze-at-open — probe/record; leap-February and 5-week months in the pin); the stepper seed decision (00:00 recommended, now recorded as alternative) + the reconciled midnight pin; the focus-loss commit/discard rule for date fields (parseable non-empty commits; else restore — exact state transitions on Flow::EditDate); the two-phase state machine (phase flags, the Enter/Escape ladder, how D20's masking interacts with text phase; the sync helper in due_edit.rs — cursor→buffer and buffer→picker writes, the no-oscillation property); the time combobox widget (anatomy, the lenient parse table (9/930/9:45/09:45), stepper semantics ±15 rollover, quick-list generation + scroll-to-nearest + filter, keyboard operation incl. walk stops; how the date picker mounts it); the validity wiring (field list, the validators' home beside due_edit/presentation, error-line copy as constants); the zone unification (parse local → store UTC, echo local; the DST edge cases to pin; NO stored-instant migration — recorded rationale per §7-R4); test catalogue + Tier-4 (picker feel at min viewport, combobox feel).
5. Commit: docs(plan): date and time entry rework. No implement, no push.
Report: plan path, decisions (esp. flip fix, phase machine, zone plan), risks, diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of GUI review round 4 ("date/time entry").
1. Read AGENTS.md, design §3.7, dispatch §1/§8, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-datetime-entry-plan.md) in ../taskboard-WT-feat-ui-r4-datetime-entry (recreate if missing).
2. Implement per the plan, TDD, standalone commits: the combobox widget first (pure parse + unit tests), then the picker integration, then the phase machine + sync, then validity, then the zone change last (largest journey surface) with goldens regenerated in the same commit. No domain diffs; no text assertions.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, the flip-fix verdict that shipped, Tier-4 items, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of GUI review round 4 (branch feat/ui-r4-datetime-entry).
1. Read AGENTS.md, design §3.7/§7-R4, dispatch §1/§8 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) the zone change cannot shift existing stored dues (no data migration; journeys pin parse→store→echo round trips through a fixed local offset and across a DST boundary); (b) the phase machine cannot trap (Escape from either phase exits; Enter in picker phase never commits a half-edited time unintentionally); (c) the sync helper cannot oscillate (property test green; manual loop-guard audit); (d) the combobox is the ONLY time entry path in the date picker (no orphaned stepper row); its parse is total over the documented forms and rejects garbage with validity, never silently; (e) the click-away rule holds for BOTH date fields and cannot fight chunk 6's insert engagement (journeys green); (f) validity wiring reuses parse_text_form/the due grammar — no second parser (grep); (g) empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 9. Chunk 8 — Task reminders that fire (`feat/ui-r4-task-reminders`)

- **Covers**: origin §3.8 (D29–D31), findings F25/F23-part + §2
  decision 2 (**decision gate**: at/in kind confirmed). This is the
  round's only crate-crossing chunk (domain + state + storage
  migration 0018 + ui). Non-goals: GUI insistence picker (backlog),
  escalation ladder changes, timer surfaces (chunk 9).
- **Acceptance**: (a) domain: the reminder-rule kind enum
  (`DueOffset` | `At` with a minted-instant newtype), planner arm,
  evaluator `At` pass (fires at `instant <= now` independent of
  duedate-remindability), settlement pruning of fired+dismissed `At`
  rules (adhoc precedent) while `DueOffset` rules persist — unit +
  property tests at the domain layer; (b) storage migration 0018
  round-trips existing rules as `DueOffset` (upgrade pin, no loss);
  (c) UI: the add grammar "in 10m"/"at 17:00" mints `At`, bare "10m"
  stays `DueOffset` (parse shared with `parse_timer_when`'s
  prefixes — one parser, not two); rows render per kind — `At` rows
  show live d/h/m/s countdown + the fixed fire time (local,
  `YYYY-MM-DD HH:MM:SS`), `DueOffset` rows show armed-with-effective-
  time or the explicit needs-a-due-date state with the jump
  affordance; the per-second tick rides chunk 5's seam; (d) the
  un-mute affordance + muted indicator land (round-trip journey);
  (e) the end-to-end firing journey on the paused clock: `At` rule →
  tick at T → in-app row → dismiss → pruned from the list (this is
  F25's regression harness); (f) validity on the offset field rides
  chunk 7's wiring (already landed) — no duplication.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of GUI review round 4 ("task reminders that fire", branch feat/ui-r4-task-reminders) in /home/viktoria/src/taskboard.

DECISION GATE: design round §2 decision 2 (at/in reminder kind) must be confirmed; if still PROPOSED with no user verdict, STOP and report.

GATE: git fetch origin --prune; verify chunk 6 merged (insert engagement) and that no other migration-carrying backlog chunk is in flight (check open PRs for migrations). If blocked: STOP and report.

1. Read AGENTS.md, design round §3.8 (D29–D31, normative) + §2 decision 2 + §1 findings 23/25 + §7-R5, dispatch §1/§9. Also read the fences: the backlog round-2 dispatch's migration fence (next free number 0018, one migration in flight) and its chunks 3.5/1.9/4.2 (adjacent domain files — rebase expectations).
2. Read the seams: taskboard-domain/src/reminders.rs (ReminderOffset 22-56, parse_text_form 108-135, rules/occurrence arithmetic 758-776, due_firings 792-845, is_remindable 947-949, prune_settled_alarms 678-693, active_notifications 863-913, AdhocAlarm 272/324-336), taskboard-domain/src/messages.rs (AddReminderRule 247-255, remove/clear 259-268), taskboard-domain/src/command.rs (rule planner 1928, enqueue pin 6356-6383), taskboard-state/src/actor.rs (tick 371-386/557-586, reminder_tick 1145-1173), taskboard-storage-sqlite/src/repo.rs (rule upsert 1715, migrations dir — last number), crates/taskboard-ui-slint/ui/compositions/rule-list.slint + src/adapter.rs (rule seams 4295-4330), src/router.rs (ReminderRuleAdd 396-405, mute paths 384/803), src/presentation.rs (parse_timer_when 788-830, countdown_phrase 472-479), taskboard-app/src/commands/reminders.rs (the CLI un-mute path 291-299 — the affordance precedent). Tests: domain reminder suites, actor tick tests, rule-list journeys.
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-task-reminders ../taskboard-WT-feat-ui-r4-task-reminders off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-task-reminders-plan.md settling: the kind enum shape (enum-with-data over the rule, the At instant newtype per house A1 — minting constructor, no derive crates); command surface (AddReminderRule grows the kind vs a new verb — one call, recorded; planner + enqueue pin updated); the evaluator At arm (ordering with the offset pass, delivery-set semantics, no-due-date independence); settlement (prune condition mirroring prune_settled_alarms; what a snooze means for an At rule — reuse the existing snooze lanes); migration 0018 (column shape, upgrade pin test, existing rules → DueOffset); the UI grammar split (shared prefix parse with parse_timer_when — where the shared parser lives so there is exactly one); row presentation per kind (the countdown/ISO formatter shared with chunk 9's D32 — place it in presentation.rs now; the needs-due-date state + jump affordance; the muted indicator + un-mute dispatch); test catalogue (domain unit/property, actor journey on the paused clock, UI journeys, migration round-trip).
5. Commit: docs(plan): task reminders that fire. No implement, no push.
Report: plan path, decisions (esp. command surface + settlement semantics), risks, diff size.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of GUI review round 4 ("task reminders that fire").
1. Read AGENTS.md, design §3.8, dispatch §1/§9, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-task-reminders-plan.md) in ../taskboard-WT-feat-ui-r4-task-reminders (recreate if missing).
2. Implement per the plan, TDD, standalone commits, domain-first (SPDX MIT OR Apache-2.0, #![forbid(unsafe_code)]): the kind + planner + evaluator + property tests, then migration 0018, then the actor wiring, then the UI grammar/rows/un-mute. Time-dependent tests use pause/advance, never sleeps; no text assertions (countdown/ISO formatters tested structurally); every commit builds standalone.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items (set an "in 1m" reminder live and observe fire+settle), deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of GUI review round 4 (branch feat/ui-r4-task-reminders).
1. Read AGENTS.md, design §3.8/§7-R5, dispatch §1/§9 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) DueOffset semantics are byte-compatible (existing rule journeys green; migration round-trip pinned — an existing "10m" rule still fires 10m before due); (b) the At pass cannot double-fire (delivery-set semantics pinned; idempotence on repeated ticks); (c) settlement prunes only fired+dismissed At rules (a live At rule or an offset rule is never pruned — property test); (d) the shared parser claim holds (grep: exactly one in/at prefix parser); (e) the needs-due-date state is derived, not stored (no state desync possible); (f) crate boundaries per AGENTS §2 (no IO in domain; the ui crate owns only presentation); (g) migration is additive + upgrade-pinned; no text assertions.
3-7. Mandate, gates, fixes, ship, teardown per §1. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 10. Chunk 9 — Timer surfaces (`feat/ui-r4-timer-surfaces`)

- **Covers**: origin §3.9 (D32–D34), findings F13/F23 + §2 decision 4
  (**decision gate**: backlog absorption confirmed). Absorbs backlog
  round-2 chunk 2.5 (#352) remainder. Non-goals: task-view reminder
  rows (chunk 8), tracking (chunk 10).
- **Acceptance**: (a) timer rows and the status chip show d/h/m/s
  remaining + "@ YYYY-MM-DD HH:MM:SS" fire time, ticking per second
  (paused-clock journeys; kiosk gates pinned unchanged); (b) the
  mouse setter popover lands in the timers header: quick presets,
  label, custom time via the chunk-7 time combobox, commit through
  the existing `timers-create-accepted` seam (popover ≡ text form ≡
  same command — journey); the text form survives untouched for
  keyboard users; (c) the #352 remainder table (landed vs missing,
  per the DR-3 rulings) is recorded in the stage-1 plan with
  evidence, and every missing item lands (or is filed with the
  recorded reason if genuinely obsolete); (d) blanket rule (hover +
  walk stops on every new element); (e) validity on the form fields
  rides chunk 7's wiring.

**Prompt 9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 9 of GUI review round 4 ("timer surfaces", branch feat/ui-r4-timer-surfaces) in /home/viktoria/src/taskboard.

DECISION GATE: design round §2 decision 4 (backlog absorption) must be confirmed; if still PROPOSED, STOP and report.

GATE: git fetch origin --prune; verify chunks 3 and 7 merged (icons; time combobox). If not: STOP and report.

1. Read AGENTS.md, design round §3.9 (D32–D34, normative) + §1 findings 13/23 + dispatch §1/§10. Also: GitHub issue #352 and the DR-3 rulings it cites (.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md — the timers rulings), plus the landed timers plan (.artifacts/plans/2026-10-27-timers-alarm-surface-plan.md §2 fences — the minute-repaint fence this round amends via design §2 decision 5).
2. Read the seams: ui/compositions/timers.slint (header 54-74, rows 76-93, creation form 95-121, the commit seam 31-33), ui/views/today-view.slint (section mount 107-116), src/adapter.rs (timer seams 4701-4787: form engage, create parse 4727-4757, cancel 4770-4787), src/presentation.rs (countdown_phrase 472-479, parse_timer_when 788-830, timer_rows 715-735, chip 756), src/models.rs (timers-visible 482, chip 1275-1289, timer rows 455-491), src/lib.rs (repaint dirty conditions 534-540 — chunk 5's seconds seam), the landed alarm window + escalation driver (ui/alarm-window.slint, taskboard-app/src/escalation.rs:475-522 — what already covers #352), ui/widgets/time-field.slint (chunk 7's combobox), ui/widgets/icon-button.slint + assets (chunk 3's timer icon kind).
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-timer-surfaces ../taskboard-WT-feat-ui-r4-timer-surfaces off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-timer-surfaces-plan.md settling: the #352 remainder table (DR-3 ruling vs landed code, item by item, with file:line evidence — what this chunk owes); the two-part countdown formatter (shared with chunk 8's D30 if it landed, else place it in presentation.rs now — structural tests only); the popover anatomy (presets list — which durations; label field; the custom row composing the time combobox + day offset for "tomorrow 09:30" — exact grammar vs parse_timer_when; anchored mount like the menu overlay; dismissal); the seam unification (popover commit and text-form commit funnel into timers-create-accepted — one parser); kiosk gating reaffirmed; test catalogue + Tier-4 (popover feel, tick smoothness).
5. Commit: docs(plan): timer surfaces. No implement, no push.
Report: plan path, the #352 remainder table, decisions, risks, diff size.
```

**Prompt 9.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 9 of GUI review round 4 ("timer surfaces").
1. Read AGENTS.md, design §3.9, dispatch §1/§10, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-timer-surfaces-plan.md) in ../taskboard-WT-feat-ui-r4-timer-surfaces (recreate if missing).
2. Implement per the plan, TDD, standalone commits; no new commands (existing timer create/cancel lanes); no text assertions; paused-clock tests only.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, what the #352 remainder turned out to be, Tier-4 items, deviations.
```

**Prompt 9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 9 of GUI review round 4 (branch feat/ui-r4-timer-surfaces).
1. Read AGENTS.md, design §3.9, dispatch §1/§10 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) the popover and the text form hit the identical commit seam (journey parity; grep: no second parser); (b) the countdown cannot drift from the engine's fire instants (the row derives from state + now, never from a UI-held timer); (c) kiosk unchanged (pins); (d) the #352 remainder is fully accounted (every DR-3 ruling row either landed before, lands here, or carries a filed/recorded reason); (e) no dead form residue; (f) empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Then post #352's ✅ DONE comment with the PR link (per origin §8). Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 11. Chunk 10 — Tracking surfaces (`feat/ui-r4-tracking-surfaces`)

- **Covers**: origin §3.10 (D35–D37), finding F17 + §2 decision 4
  (**decision gate**: backlog absorption confirmed). Absorbs backlog
  round-2 chunk 3.4 (#490 + #491). Non-goals: no domain/state/
  storage changes (all verbs exist), no respite/break features
  (backlog 3.5), no tracker task kind (backlog 4.2).
- **Acceptance**: (a) the entries overlay opens from the task view:
  total at top (seconds precision), rows of date/start/end/spent
  with the running entry marked (journeys + golden); (b) add form
  (date picker + two time comboboxes) → `LogTime`; edit (seeded form)
  → `AdjustTimeEntry`; delete → `DeleteTimeEntry` — each a journey
  asserting the engine state transition; (c) the end≤start rejection
  surfaces as field validity (the planner's typed error, never
  swallowed silence); (d) full input-mode support: every element a
  walk stop, Enter activates, Escape backs out, hover tips
  throughout (structural inventory + journeys per preset); (e) the
  #490 today "Tracked" section lands per the issue's scope (stage-1
  verdict recorded; remainder filed if larger); (f) the tracking-ui
  round's fences are updated (`task-view.slint` fence comments, the
  plan's non-goal notes annotated in the issue, not edited
  retroactively).

**Prompt 10.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 10 of GUI review round 4 ("tracking surfaces", branch feat/ui-r4-tracking-surfaces) in /home/viktoria/src/taskboard.

DECISION GATE: design round §2 decision 4 (backlog absorption) must be confirmed; if still PROPOSED, STOP and report.

GATE: git fetch origin --prune; verify chunks 3, 5 and 7 merged (icons; summary row home; time combobox). If not: STOP and report.

1. Read AGENTS.md, design round §3.10 (D35–D37, normative) + §1 finding 17 + dispatch §1/§11. Also: GitHub issues #490 and #491, the tracking-ui plan's fences (.artifacts/plans/2026-10-28-tracking-ui-plan.md §2 non-goals — this chunk lifts them), and the backlog round-2 dispatch's chunk 3.4 row (being absorbed — do not follow its prompt, but honour its gates: expect models.rs render-lane rebase friction).
2. Read the seams: taskboard-domain/src/tracking.rs (TimeEntry 36-66, ClosedSpan::try_new 77-108, duration 110-124, newest_open 126-138, tracked_in_window 141-166), taskboard-domain/src/messages.rs (LogTime 321-333, AdjustTimeEntry 334-348, DeleteTimeEntry 349-356), taskboard-domain/src/command.rs (planner arms 2061/2128/2175/2213), taskboard-app/src/commands/track.rs (the CLI forms — log 100, adjust 116, delete 133: the field semantics to mirror), ui/views/task-view.slint (the summary row from chunk 5), src/adapter.rs (tracking seams — detail_tracking_verb routes, IdTables pattern for entry ids), src/models.rs (task detail push, the today section pattern for #490), ui/compositions/ (the overlay mount pattern app-window.slint 348-369), ui/widgets/time-field.slint + ui/compositions/date-picker.slint (chunk 7's). Tests: tracking engine journeys, CLI parity tests, today-view section goldens.
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-tracking-surfaces ../taskboard-WT-feat-ui-r4-tracking-surfaces off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-tracking-surfaces-plan.md settling: the overlay's mount + dismissal (anchored to the task view; Escape/E ladder with chunk 6's focus rules); the row/total presentation (helpers beside chunk 5's total — structural tests); the add/edit form (fields, seeding, the validity mapping from the planner's typed rejection — which error maps to which field), the delete affordance (confirm-on-second-activate or undo-adjacent? record the choice against the undo round's surfaces); new ActionIds + router arms + IdTables rows (naming per registry conventions); the #490 verdict (read the issue; scope the section; file the remainder if larger); walk-stop inventory + hover inventory; test catalogue + Tier-4 (form feel, list scrolling with many entries).
5. Commit: docs(plan): tracking surfaces. No implement, no push.
Report: plan path, decisions (esp. delete affordance + #490 verdict), risks, diff size.
```

**Prompt 10.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 10 of GUI review round 4 ("tracking surfaces").
1. Read AGENTS.md, design §3.10, dispatch §1/§11, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-tracking-surfaces-plan.md) in ../taskboard-WT-feat-ui-r4-tracking-surfaces (recreate if missing).
2. Implement per the plan, TDD, standalone commits; dispatch ONLY existing commands (LogTime/AdjustTimeEntry/DeleteTimeEntry — a new command variant is a deviation to report, never make); no domain diffs; time forms via chunk 7's widgets; no text assertions.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items, deviations.
```

**Prompt 10.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 10 of GUI review round 4 (branch feat/ui-r4-tracking-surfaces).
1. Read AGENTS.md, design §3.10, dispatch §1/§11 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) no command-surface additions (grep the diff over messages.rs/command.rs — must be empty); (b) editing a running entry is either prevented or well-defined (the end=None mask semantics of AdjustTimeEntry respected — journey); (c) totals reconcile: the summary row, the overlay total, and tracked_in_window agree (property/journey); (d) the overlay cannot strand focus when the task closes underneath it (journey: open overlay → back to board); (e) #490/#491 scope fully accounted; (f) no text assertions; empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Then post #490's and #491's ✅ DONE comments with the PR link (or the annotation if #490 filed a remainder). Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 12. Chunk 11 — Settings: scale + connections (`feat/ui-r4-settings-connections`)

- **Covers**: origin §3.11 (D38–D40), findings F28/F29 + issue #411.
  Non-goals: no connection/bind semantics changes, no keymap editing.
- **Acceptance**: (a) scale: no live preview — `base-scale` moves
  only on commit/revert; the drag gesture completes without any
  re-layout mid-gesture (journey: full drag → release → one relayout
  + persisted); the echo field no longer tracks the thumb; commit
  scroll anchoring still works; (b) connections: the board selector
  is a dropdown with row hover feedback, keyboard operation (open/
  move/select/close), a connected/not-connected indicator per option,
  the label positioned per the responsive rule; selection drives the
  existing `settings.selected_board` seam and gates Bind (journeys);
  the board list refreshes on settings mount; connector rows gain
  hover feedback; (c) #411: the board bar adopts the shared dropdown
  per the issue's ruling (stage-1 verdict recorded; if the full
  scope exceeds the widget + adoption, the remainder is filed/annotated);
  (d) blanket rule (hover + walk stops).

**Prompt 11.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 11 of GUI review round 4 ("settings: scale + connections", branch feat/ui-r4-settings-connections) in /home/viktoria/src/taskboard.

GATE: git fetch origin --prune; verify chunk 3 merged (icons — the link/link-off indicator). Also check whether backlog chunk 2.4 (#412 window-geometry) has landed or is open — if open, note the app-window.slint rebase expectation in the plan.

1. Read AGENTS.md, design round §3.11 (D38–D40, normative) + §1 findings 28/29 + dispatch §1/§12. Also GitHub issue #411 (the board-switcher dropdown ruling).
2. Read the seams: src/adapter.rs (scale_drag 2801-2806, settings_scale_preview 2813-2833, released 2863-2878, apply_scale_commit 2738-2766, scroll anchor 1984-1988, echo-follows-thumb 2828-2832, settings_board_selected 2621-2627, settings_bind_row 2632-2647, connector_row_status 2383-2399, boards list staleness 2105), ui/views/settings-view.slint (scale row 705-719, connections 447-554), ui/widgets/slider.slint (pressed/preview/committed), ui/widgets/icon-button.slint + assets (chunk 3), ui/compositions/board-bar.slint (the board switcher — #411's adopter), ui/responsive.slint (the label-placement rule pattern), src/models.rs (settings boards/connector rows push 2374/2405-2410). Tests: settings journeys (scale trio from round 3 — expectations change: preview dies), connection journeys.
3. scripts/worktree-doctor.sh; then git worktree add -b feat/ui-r4-settings-connections ../taskboard-WT-feat-ui-r4-settings-connections off main.
4. Plan at .artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md settling: the preview removal inventory (adapter arm, markup hook, echo logic — and which round-3 D20 tests/pins die vs get rewritten as release-commit pins); the dropdown widget anatomy (ui/widgets/dropdown.slint: field+chevron+list, anchored mount, dismissal, keyboard ladder, hover rows, walk stops — generic over model rows so settings and board-bar both use it); the connections composition (label + responsive placement rule; the indicator mapping from connector_row_status to icon kind + tone; the "selected for binding" copy as a presentation constant — no behavior change to Bind); mount-refresh of the board list; the #411 adoption verdict (read the issue; what the board bar gains and when — threshold vs always; file the remainder if larger); test catalogue + Tier-4 (dropdown feel, scale drag feel post-change).
5. Commit: docs(plan): settings scale and connections. No implement, no push.
Report: plan path, decisions (esp. #411 verdict + which D20 pins die), risks, diff size.
```

**Prompt 11.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 11 of GUI review round 4 ("settings: scale + connections").
1. Read AGENTS.md, design §3.11, dispatch §1/§12, stage-1 plan (.artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md) in ../taskboard-WT-feat-ui-r4-settings-connections (recreate if missing).
2. Implement per the plan, TDD, standalone commits; the dropdown is a generic widget with structural tests before either adoption; preview-removal pins rewritten (never silently dropped — each dead pin gets its release-commit successor in the same commit); no text assertions.
3. Full gate suite via scripts/run-quality-gates.sh green; CHANGELOG updated.
4. No push, no PR. Never touch main.
Report: per-gate results, Tier-4 items, deviations.
```

**Prompt 11.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 11 of GUI review round 4 (branch feat/ui-r4-settings-connections).
1. Read AGENTS.md, design §3.11, dispatch §1/§12 (code-quality mandate), stage-1 plan; review the full post-rebase diff.
2. Functional focus: (a) no preview residue (grep settings_scale_preview; the only scale writes are commit/revert); the flicker repro from finding 28 is dead by construction; (b) the dropdown is used by BOTH consumers or the divergence is recorded (no near-duplicate widget); (c) selection cannot change the viewed board (the seam stays Bind-only — journey pin); (d) keyboard operation of the dropdown works under all three presets (walk/journeys); (e) #411 fully accounted (landed or filed+annotated); (f) empty diff over domain/state/storage/sync/app.
3-7. Mandate, gates, fixes, ship, teardown per §1. Then post #411's ✅ DONE or annotation comment. Report: verdict, findings, fixes, PR URL, confirmations, Tier-4 items.
```

## 13. Completion bookkeeping

- **Closes on merge** (stage 3 posts ✅ DONE + PR link): #352
  (chunk 9), #490 + #491 (chunk 10 — #490 annotated if a remainder
  was filed), #411 (chunk 11 — annotated if partial).
- **Amends**: the backlog round-2 dispatch plan drops chunks 2.5 and
  3.4 with pointers to this round's chunks 9/10 — a small docs
  commit (`docs(plan): backlog round-2 drops the absorbed timers and
  tracking chunks`) once the user confirms §2 decision 4 and either
  chunk opens; the commissioning record in
  `.artifacts/reports/2026-10-31-backlog-readiness-triage-report.md`
  gets a one-line note.
- **New issues to file** (chunk stage discoveries per AGENTS §7):
  the GUI insistence picker (origin D31c) as `backlog:` ui/p2;
  whatever D15 (hints), D22 (flip), D34 (#352 remainder) surface.
- **Stays open, untouched**: #210/#489 (tracking card play/pause
  badge fence), #444, #412 (unless landed — chunk 11 notes it),
  #463/#464 (perf lane), every other backlog round-2 chunk.
- **What opens next**: GUI review round 5 over this round's merges;
  the respite-break and pomodoro backlog chunks (3.5, 5.1) ride the
  tracking/reminders seams this round lays down.

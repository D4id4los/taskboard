# [Plan] GUI Phase 7 — UI Architecture Design Round (v0.1.0 Foundation)

- **Date**: 2026-10-14
- **Kind**: design round (chunk 3, stage 2 of the wave-0 dispatch plan) —
  **no production code in this commit**. Produces the architecture design
  + decision agenda + draft ADR the user reviews in the design session
  before any Phase 7 implementation is planned (chunk 4 derives the
  implementation dispatch plan from the accepted version of this
  document).
- **Origin**: GUI requirements brief
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`
  (normative input — R1–R10, §3 feature surface, §5 criteria); Slint
  stress-test report
  `.artifacts/reports/2026-10-14-slint-gui-stress-test-report.md`
  (version-pinned facts, Slint 1.18.1); wave analysis
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §6 Wave 0 / §3.7 view inventory; ADHD design-principles report
  `.artifacts/reports/2026-10-14-adhd-support-research-report.md` §5
  (P1/P2/P7/P9/P12 land inside Phase 7's foundation); growth roadmap
  `.artifacts/plans/2026-10-08-growth-roadmap-plan.md` Phase 7; dispatch
  plan `.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
  §1/§4.
- **Target**: this document only. Implementation targets (§12):
  `taskboard-ui-slint`, `taskboard-app` (bootstrap/config), root
  `Cargo.toml` (slint pin), docs (ADR, architecture.org,
  testing_strategy.org), CI.
- **Branch / worktree**: `docs/gui-phase7-design` in
  `../taskboard-WT-docs-gui-phase7-design`, off `main` @ `fa76762` (+ the
  chunk-3 stage-1 commits: stress-test plan `841a2c2`, report `23129dd`).
- **Goal**: a decision-ready UI architecture for Phase 7 (M2 → v0.1.0)
  that (a) bakes the GUI brief's [FOUNDATION] requirements into the
  crate's skeleton — theming, component layering, action-dispatch
  keybindings, scaling, keyboard journeys, automation seam, first
  usability checks — without scope-creeping the fixed v0.1.0 exit
  criterion; (b) resolves every open question into either a recorded
  proposal (normative sections) or an agenda row with a proposed default
  (§11); (c) hands chunk 4 a chunk decomposition it can turn into
  dispatch prompts verbatim.
- **Status**: ACCEPTED 2026-10-14 — design session round 1 resolved
  all 23 agenda rows (§16 filled; the redirected rows Q3/Q15/Q17 had
  their normative sections amended in the same commit). PR review
  rounds remain the merge gate; further amendments are fresh
  stage-2-style invocations against this branch.

---

## 1. Context & verified facts (anchored — do not re-derive)

1. **The UI crate today** is a doc-comment stub
   (`crates/taskboard-ui-slint/src/lib.rs`): the `slint` dependency is
   deliberately absent (backlog "[2026-10-07] Add the `slint`
   dependency to taskboard-ui-slint" owns the landing); the crate has
   no `#![forbid(unsafe_code)]` by design — the only workspace crate
   with that exception, because Slint's generated code requires
   `unsafe`.
2. **The UI crate contract** (AGENTS §1/§2, `docs/architecture.org`):
   presentation only — reads `ArcSwap<AppState>` lock-free on the
   rendering thread, subscribes to `EngineSignal`/`SystemEvent`
   broadcasts, emits domain-typed commands over a plain `mpsc`; **no IO
   crates, no business logic, no state mutation**. The app-side
   forwarder adapting that channel onto `EngineHandle::dispatch` is the
   existing "[2026-10-09] UI command forwarder bootstrap task" backlog
   entry (a `tokio::spawn` forwarder in bootstrap).
3. **Slint 1.18.1 verdicts** (stress-test report §2, all
   version-pinned): focus traversal + programmatic focus **supported**
   (FocusScope discipline, `focus()`/`focus_next_item()`,
   focus-observability callbacks); global key interception
   **supported** (root FocusScope + `forward-focus`, tunneling capture
   phase; new declarative `KeyBinding` element is single-stroke-only,
   so chords/modals stay in our action layer); runtime theming via
   exported globals **supported** (per-window init caveat only); scale
   factor **partial** — no runtime window-scale setter; public surface
   is `SLINT_SCALE_FACTOR` env (read once at window creation) and a
   one-shot `set_const_scale_factor`; testing API **partial, different
   shape** — no public `slint::testing`; the harness lives in the
   internal, non-semver `i-slint-backend-testing` crate (`=x.y.z` pin
   mandatory) offering headless init, mock time, input injection,
   accessible-role/label element queries, and an embedded **MCP
   server** with `take_screenshot`; accessibility **supported**
   (AccessKit over AT-SPI, default-on); rich-text **partial** as the
   org-mode reports recorded (`StyledText` render yes, edit upstream
   open); renderer question **resolved as defaults**: femtovg +
   software ship together in default features with runtime fallback —
   no skia now.
4. **Reminders surface for the UI** (reminders design plan §7/§8,
   landed): the engine publishes an overdue-banner signal path; the UI
   reads it as state + signal, snooze/mute are `StateCommand`s;
   desktop toasts are the separate reminders "plan 5".
5. **ADHD principles that land in this foundation** (report §5, reading
   guide: P1, P2, P7, P9, P12 "land inside Phase 7's foundation
   (R1/R2/R4/R5/R7 surfaces)"): P1 capture-at-the-speed-of-thought
   (the global `capture.quick` action seam), P2 externalise-time
   (time-bearing surfaces must show time without an internal clock —
   a lens on the display-role and view-shell work; the concrete
   time surfaces are W1/W2), P7 constrain-by-default (keymap
   presets, theme discipline — "treat configuration as a hazard"),
   P9 forgiveness (global undo reservation), P12 gentle-by-default
   (focus/attention discipline in the view shells). They shape *how*
   the foundation is built; no ADHD-specific *feature* enters v0.1.0.
6. **ADR numbering state** (verified 2026-10-14): `docs/adr/` on
   `main` holds 0001–0012 **and 0014** (connector attach, landed via
   `f482ae0`). **0013 is reserved-but-unlanded**: the reminders design
   round drafted "ADR 0013: Reminders — Local Due-Anchored Rules,
   Engine-Tick Evaluation, Ported Delivery" (its plan §12, to be
   "formalized by stage 2") but no `docs/adr/0013-*` file exists on
   any branch — the reminders round's formalization step never landed
   its ADR. **The UI architecture ADR therefore takes 0015** (next
   free), and the 0013 gap is recorded as bookkeeping debt (§13.1,
   agenda Q21). No plan or branch references a number ≥ 0015 yet, so
   no further collision exists.

## 2. The v0.1.0 scope fence

The growth roadmap's Phase 7 exit criterion is **fixed** and is the
fence: *"tagged v0.1.0 with desktop UI, released binaries"*, with the
phase's own content list — board/stack/task views reading `ArcSwap`
and dispatching `StateCommand`s, a sync-status/offline indicator,
Slint component tests. Everything the GUI brief adds is either a
**foundation seam** built *inside* that surface (invisible plumbing
that later waves snap into) or explicitly **POST-1.0**. No §3
feature-surface item (views, overlays, palettes) enters v0.1.0.

### 2.1 What v0.1.0 ships

- The slint dependency at the stress-report pin (agenda Q10/Q12;
  §12 chunk 1), default feature set (winit, femtovg, software,
  accessibility).
- A window shell + app-side wiring: UI actor mount, the UI-command
  forwarder (backlog [2026-10-09]), config hand-off (`[ui]` table).
- The theme/token module with text roles, the no-literal-tokens lint
  gate, and the contrast CI check (§3).
- The component library skeleton under the layering rules (§4)
  and exactly **three views**: board (kanban columns), stack detail,
  task detail — plus the sync-status/offline indicator (reads
  `SystemEvent` broadcast) and the persistent overdue banner (Q23
  accepted in round 1; rides the already-designed reminders signal
  path).
- The action-dispatch keybinding layer with the `standard`, `emacs`,
  and `vi` presets at core-verb coverage, plus its one R8
  projection: the generated `app.menu` (§5.9).
- `[ui] scale` as a live user setting via the root-multiplier model
  (§6); breakpoints for the board view (§6.3).
- The logical automation harness (journeys through
  `i-slint-backend-testing`) and the two usability checks (§8, §9).

### 2.2 Requirement fence (brief tagging × stress verdicts)

| Req | Brief tag | Stress verdict shaping it | v0.1.0 builds | POST-1.0 |
|---|---|---|---|---|
| R1 components | FOUNDATION | theming supported | layering rules + primitives inventory (§4) | the growing per-wave component set |
| R2 theming | FOUNDATION | supported (globals) | token module, text roles, lint gate, contrast check (§3) | runtime theme switching, light/high-contrast themes |
| R3 scaling | FOUNDATION | **partial** — no runtime window API | root-multiplier `ui-scale` + env escape hatch (§6) | — |
| R4 keyboard | FOUNDATION | focus supported | focus architecture + journey acceptance bar (§7) | — |
| R5 keybindings | FOUNDATION | interception supported; no sequences | full action layer, 3 presets, chords+modes (§5) | conflict-detection UI polish; per-user preset sharing |
| R6 automation | FOUNDATION seam / POST-1.0 full | testing backend internal + MCP surprise | logical harness now; scriptability rules (§8) | MCP/LLM driver tooling, compositor capture polish |
| R7 accessibility | FOUNDATION choices / POST-1.0 audits | AccessKit supported | a11y widget discipline + first two checks (§7.2, §9) | research-flavoured audits (§9.3) |
| R8 discoverability | POST-1.0 + seam | — | actions carry id/label/context; menus generated from actions (§5.9) | keyboard visualiser, contextual cheat-sheet, hint graphics |
| R9 responsive | FOUNDATION responsive / research adaptive | — | breakpoints + board degeneration (§6.3–6.4) | adaptive UI — **parked research, no telemetry** (§6.5) |
| R10 markup help | POST-1.0 | rich-text edit still open upstream | nothing | help view when markup rendering lands (org-mode report phasing) |

### 2.3 §3 feature surface → seam traceability (all deferred, none lost)

Every brief §3 item is a Wave 1–3 deliverable that rides a seam this
design builds. The seam column is what v0.1.0 must get right; the wave
column matches the analysis report §6.

| §3 surface | Wave | Seam it rides (this plan) |
|---|---|---|
| Today view; organiser view | W1/W3 | list-composition over task-card primitive (§4); day-boundary pref is domain-side |
| Importance–urgency matrix + decorations | W1/W3 | task-card decoration slots (§4.2 rule D) |
| Overview display (two-column) | W2 | responsive composition rules (§6.3) |
| Kiosk kanban ⇄ upcoming switching | W1+ | display-mode state machine over one view tree (§7 of agenda Q4; kiosk mode = chrome-hiding mode, shared components) |
| Inbox capture + triage | W1 | global `capture.quick` action (§5.8); command mpsc |
| Fuzzy search palette + filter views | W1 | overlay shell pattern + `palette.search` reserved action (§5.8); pure domain ranking |
| Pomodoro focus overlay | W2 | "one window, few exits" is expressible: focus scopes + action contexts disable global binds per view (§5.2 contexts); early sketch validated cheaply by this seam existing |
| EOD review/journal/statistics | W2 | form-view composition; chart story verified when the wave opens |
| Undo tree view | W3 (design round D4 earlier) | `app.undo`/`app.redo` global reserved actions (§5.8); undo is global-not-per-view by construction |
| Reminder surfaces (overdue banner, toasts w/ snooze/mute, insistent alarm) + the generic toast surface | banner: v0.1.0 (Q23 accepted); toasts: reminders plan 5 + error surfacing; alarm: later | toasts are a **generic** transient-message composition — *reminders are toasts*, not the reverse: warnings and recoverable errors are sources too (§16 Q3); reminder toast actions (snooze/mute) = UI-originated `StateCommand`s; `NotificationSink` stays outbound-only; banner rides the designed signal path (§1.4); alarm = overlay over the pomodoro-grade focus seam |
| Org/markup rendering in fields | per org-mode report phasing | editor widgets stay swappable behind the orgedit adapter plan; `StyledText` render + source toggle is the MVP shape (stress §3.7 confirms 1.18.1 unchanged) |
| Markup help page (R10) | with markup feature | help content as data next to the parser crate (brief R10; not a UI-crate concern until then) |
| Keymap visualiser (R8) | POST-1.0 | pure projection of keymap data + action registry (§5.9) |
| PDF/print preview | W2 | no v0.1.0 seam needed (print is a platform-dialog concern); noted for the W2 plan |

### 2.4 Fence invariants

- The **exit criterion text is untouched**; a foundation seam is
  allowed to be *visible* in v0.1.0 only as: the scale setting, the
  keymap presets, the generated `app.menu` (§5.9, the one R8
  projection), and the overdue banner (Q23 accepted). Anything else
  user-visible from §2.3 would be scope creep.
- If an implementation chunk discovers it needs a §2.3 surface to make
  a seam real (e.g. "we need the search palette to test the action
  contexts"), that is a design failure to bring back to the session,
  not a licence to build the surface.
- Wave-1 leakage guard from the analysis report §7 stays in force:
  the fence table above is the reference the review agent diffs
  against.

## 3. Theming, design tokens & text-role vocabulary (R2) — normative

### 3.1 Token architecture

One theme module, `ui/theme.slint`, exporting three globals — the
**only** place colour values, font attributes, and base dimension
arithmetic live:

- `global Theme` — colour tokens + the scaling root (`base-scale`,
  §6.1) + spacing scale derived from it.
- `global Typography` — text-role structs (§3.2), each a
  `{size, weight, family}` triple; views reference the *role*, never
  attributes.
- `global Semantic` — state colours with meaning (success, warning,
  danger, offline, sync-active, overdue, focus-ring) mapped onto
  Theme palette entries, so state views never name palette colours
  directly.

Facts this rests on (stress §3.3): exported globals are
readable/writable from Rust via generated `Global` traits — the
compile-time-theme-first strategy is fully expressible, and the
built-in `Palette.color-scheme` runtime flip proves the follow-up
path. The per-window caveat (globals not shared across windows) is
recorded as a rule: **theme init is a window-construction step**; any
future second window (pomodoro popout) re-initialises globals — one
line in the window shell, not a crisis.

### 3.2 Initial text-role vocabulary (closed set)

| Role | Intended use | Default attributes (session may tune) |
|---|---|---|
| `display` | kiosk-distance headings, clock surfaces | 28–32 px, weight bold |
| `title` | window/view titles | 20 px, bold |
| `heading` | section headings, column headers | 16 px, semibold |
| `body` | task titles, general text | 14 px, normal |
| `body-emphasized` | selected/focused item text | 14 px, semibold |
| `caption` | secondary metadata, helper text | 12 px, normal, secondary colour |
| `badge` | counters, labels, chips | 11 px, semibold |
| `mono` | ids, timestamps, key hints (R8 kbd hints) | 12–13 px, monospace family |

Adding a role is a **theme-level decision** (a change to this table +
the ADR-0015 amendment note), never a per-view invention — that is
the decay mode the brief names, and the lint (§3.4) makes it
mechanical.

### 3.3 Token naming rules

- Colours are named by **function**, never by hue (`text-secondary`,
  not `gray-400`): restyling must not force renames.
- Two layers only: palette layer (raw values, theme module private to
  itself where possible) → semantic layer (what everything else
  reads). Views and components reference semantic/role tokens only.
- Spacing tokens are a stepped scale (`space-1 … space-8`) derived
  from `base-scale`; border-radii and focus-ring width likewise.

### 3.4 Enforcement — the token lint gate (brief §5.1, mechanical)

A repo-side check (script in `scripts/`, wired into CI + pre-commit
alongside fmt/clippy) scanning `*.slint` sources:

1. **No literal colours** (`#rgb`/`#rrggbb(aa)`, named CSS colours)
   outside `ui/theme.slint`.
2. **No literal font attributes** (`font-size:`, `font-weight:`,
   `font-family:` outside `Typography` definitions).
3. **No literal dimension literals** in views/compositions for
   spacing-relevant sizes — spacing must come from the spacing scale
   (proportional/relative layout expressions are exempt). This third
   rule is what makes the scaling model (§6.1) total rather than
   aspirational; it is stricter than the brief's letter but is the
   direct consequence of R3+R2 composing — agenda Q8 confirms it.

This is not a Slint feature (stress §3.3 notes no obstruction — it is
our lint over our sources). A violation is a CI failure, full stop;
there is no inline-suppression escape hatch in v0.1.0 (add one only
when a real forced-use case appears, with a recorded reason).

### 3.5 Theme strategy

- **One theme at v0.1.0**, compile-time, dark-first (kiosk distance
  viewing + desktop nighttime bias; agenda Q13). Light and
  high-contrast themes, and runtime switching, are POST-1.0 and
  already proven expressible (stress §3.3).
- The contrast check (§9.1) runs over the theme from day one, so
  theme *development* (the brief's "powerful tool" framing) always
  has the safety net on.

### 3.6 Fonts

Bundle the UI font(s) in the crate and register them at startup
(agenda Q18): kiosk/offline determinism, Windows release consistency,
and — decisively — golden-screenshot determinism in CI (§10, golden
tier).
Default proposal: one variable-weight sans family + one monospace
family, both tri-licensed (e.g. Inter/JetBrains-Mono class; final
pick is an implementation-chunk detail recorded there).

## 4. Component library & layering (R1) — normative

### 4.1 The three layers

```
ui/
  theme.slint          # §3 tokens (the only style source)
  widgets/             # L1 primitives: task-card, badge, kbd-hint, icon-button,
                       #    list-row, field-input, empty-state, status-dot …
  compositions/        # L2 layout owners: board-layout, sidebar, toolbar,
                       #    status-bar, view-stack, overlay-host …
  views/               # L3: board-view, stack-view, task-detail (+ chrome)
```

### 4.2 Layering rules

- **Rule A — styles flow from tokens only**: no layer restyles another
  layer's colours/fonts; a view may not "locally tweak" a primitive.
  Visual variation between uses of a primitive is expressed through
  the primitive's declared properties (tone: default/warning/danger;
  emphasis: normal/selected), which map to `Semantic` tokens
  internally.
- **Rule B — primitives take domain-meaningful properties** (a task
  card takes task display data, not a bag of strings); compositions
  take layout parameters, not style parameters; views take the app
  state slice + action dispatchers.
- **Rule C — one direction of reuse**: views → compositions →
  primitives. A primitive may not reference a composition; nothing
  below L3 may reference a view.
- **Rule D — future-surface slots are properties, not forks**: where a
  §3 surface will later decorate a primitive (importance/urgency
  badges, habit markers, reminder state), the primitive exposes a
  slot/property from day one (e.g. `decorations: [TaskDecoration]`)
  even if v0.1.0 only ever passes an empty/none value — the fence
  against component forks later.
- **Rule E — granularity**: a primitive is a leaf (one interactive or
  presentational responsibility); if a primitive grows a second
  responsibility it splits. Compositions own layout only and hold no
  domain knowledge.

### 4.3 Presentation helpers (the brief's R1 open question, resolved)

Display-only mapping — formatting due dates relative to now, mapping
sync status → semantic state, computing list sort order for display —
lives in a Rust `presentation` module inside the UI crate as **pure
functions** over domain types (`fn due_label(task, now) -> DueLabel`).
They never mutate, never IO, never invent domain truth (ordering
semantics the domain already defines — e.g. overdue/due-soon helpers —
are reused, not reimplemented; a reimplementation in presentation
code would be an AGENTS §5 "inline reimplementations" violation).
Pure ⇒ unit-testable without rendering ⇒ proptest-able where the
mapping is interesting (relative-time formatting at boundaries).

### 4.4 v0.1.0 component inventory (seed — grows per wave)

L1: `task-card` (+decoration slots), `badge`, `kbd-hint` (R8 seam
made structural: a primitive now), `icon-button`, `text-field`,
`list-row`, `status-dot`, `empty-state`.
L2: `board-layout` (responsive kanban), `view-stack` (view
navigation + focus hand-off), `toolbar`, `status-bar` (sync status +
overdue banner slot), `overlay-host` (the search-palette/pomodoro
shape, unused-but-present is *not* required — it arrives with its
wave; what v0.1.0 needs is only that `view-stack` + action contexts
make an overlay expressible, §2.3).

## 5. Action-dispatch keybinding architecture (R5; feeds R4/R6/R8) — normative

The single most consequential foundation (brief R5; stress §3.2
confirms the interception seam and that chords/modals must live on
our side of it). Pipeline:

```
key event (root FocusScope, capture phase)
  → KeymapResolver (Rust state machine: contexts, prefix/chord
    accumulation, vi-mode state)      -- pure
  → ActionId                          -- stable, namespaced
  → ActionRouter:
      ui-internal actions  → focus moves, view switches, mode flips (UI-crate)
      domain intents       → StateCommand over the UI mpsc (forwarder → engine)
```

### 5.1 Views never see keys

Views bind to **actions**, never keys. The only key-event handler in
the tree is the root scope. Text input is the exception by Slint
necessity (`TextInput` receives text input natively); the root scope
intercepts in the capture phase only for events text fields do not
consume, so typing never fights the keymap.

### 5.2 Action registry & namespace (brief §6 Q1, proposed)

- `ActionId` = dotted `scope.verb`, four top-level scopes at v0.1.0:
  `app.*` (global: `app.undo`, `app.redo`, `app.menu`, `app.help`,
  `app.settings`, `app.exit`), `nav.*` (view/focus navigation:
  `nav.next-view`, `nav.focus-board`, `nav.focus-sidebar`,
  `palette.search` reserved), `task.*` (verbs on the focused task:
  `task.complete`, `task.edit`, `task.move`, `task.create`,
  `task.delete`), `capture.quick` (global; the ADHD P1 seam).
- Registry entry = `{ id, label, contexts, category }` — the label
  and context tags are the R8 discoverability seam (§5.9). The
  registry is a const Rust table; a dumped registry (unit test
  snapshot via `insta`) **is** the action documentation the brief's
  §5.2 criterion wants.
- **Contexts** are declared per view (e.g. `board`, `task-edit`,
  `modal`); resolution order: active view context → global. A context
  can *mask* a global binding (the pomodoro-grade "few exits"
  requirement): masking is declared in the keymap data, not hardcoded.
- Undo/redo and capture are **reserved global actions from day one**
  (brief §3 undo row: "undo is global, not per-view"). In v0.1.0
  `app.undo`/`app.redo` dispatch nothing yet (engine-side undo is the
  D4 round) — they exist in the registry, disabled; `capture.quick`
  likewise (inbox is W1). Reservation now is one line; retrofit is
  the brief's "extreme".

### 5.3 Keymap as data (brief §6 Q1, proposed)

- Format: TOML inside the existing config file (figment hierarchy
  applies for free):
  `keymap = { preset = "standard", overrides = { "task.complete" = ["Ctrl+Space"] } }`
  under `[ui]`. A binding spec is a **sequence of strokes**
  (`"Ctrl+x", "Ctrl+c"` = an emacs chord; a single stroke is the
  degenerate case), so chords are not a special syntax.
- **Merge model**: effective keymap = preset table (code, versioned
  with the app) ⊕ user overrides (config file, per-action). Because
  the user file carries only deltas, **preset upgrades never clobber
  user customisations**, and new actions in an upgrade simply appear
  (unbound unless the preset binds them) — the brief's merge-on-
  upgrade question resolved structurally.
- Validation at load (app-side, where figment lives) produces typed
  findings — unknown action id, malformed stroke, binding to a
  masked context, **intra-context conflicts** — listing all
  offenders, and then **drops the offending entries and starts
  anyway**: the effective keymap falls back to the preset binding
  for every dropped override. A keyboard config never blocks the UI
  from starting (§16 Q15; fail-fast was rejected as hostile
  usability). v0.1.0 surfaces the findings via `tracing` warnings
  plus a startup-warning state; the recorded **end-goal journey** —
  UI starts, shows a warning with a button into the keyboard
  settings page, which opens pre-filtered on the conflicting
  bindings — lands with the settings view (POST-1.0). The UI crate
  receives the validated `Keymap` value (valid subset + findings) —
  keeping the crate IO-free.
- The stroke parsing/merging/validation functions are pure Rust and
  fully unit-testable without Slint — this is *why* the layer is
  Rust-side.

### 5.4 Presets (brief R5 "from the get-go", scope proposed in Q14)

`standard` (CUA-ish: arrows, Ctrl+C/V/Z, Alt+numbers), `emacs`
(C-x prefixes as chord sequences, M- modifiers), `vi` (normal/insert
modes as keymap **contexts** with `vi-normal`/`vi-insert` mode-flip
actions — modes are not a special subsystem, they are contexts +
masking). All three ship in v0.1.0; a preset-coverage unit test
asserts every *core verb* (§7.3 list + undo/redo/capture/menu) has a
binding in each shipped preset, so no preset is a second-class
citizen.

### 5.5 The Rust-side module decision (brief §6 Q2, proposed)

**A `keymap` + `actions` module inside `taskboard-ui-slint`.** Rationale:
UI-only, no IO, no async runtime dependency — the brief's own note
says crate-boundary rules permit it; a separate framework-agnostic
crate buys testability we already have (pure modules in the UI crate
test without rendering) at the cost of a new public surface to keep
stable. The module boundary stays clean (no Slint types in
`keymap`'s public API — key events enter via a small adapter), so
extraction remains a cheap future refactor — per session round 1
(§16 Q2), moving the module out into its own crate **when a second
project/crate wants it** is the intended trigger, and the boundary
is kept exactly for that move. The alternative (tiny crate beside
the UI crate now) was considered and declined for v0.1.0.

### 5.6 Slint integration

- Root `FocusScope` with `forward-focus`, `focus-on-click` discipline
  per view; `focus()` / focus-callbacks are the programmatic seam
  (stress §3.1).
- The declarative `KeyBinding` element (stress §3.2 bonus) is
  **single-stroke, compile-time keys** — it cannot follow a
  user-rebindable keymap. Proposed role (agenda Q9): **not used** for
  any rebindable action in v0.1.0; the root-scope interception is
  the single path. It stays on record as the implementation vehicle
  if we ever want static-by-design binds (e.g. Escape in modals) as
  a compiler-checked fast path — accepting that such binds are then
  non-rebindable by construction.
- Keymap application is one-way data flow: valid `Keymap` in (from
  app config) → resolver behaviour out. Live re-map during a session
  is a settings-view feature (POST-1.0); v0.1.0 applies the keymap
  at UI start.

### 5.7 Conflict detection (brief R5 open question)

Three layers, all cheap because the keymap is data: (a) unit tests
assert shipped presets are conflict-free (the compiler-side duplicate
detection `KeyBinding` offers for free does not apply to data-driven
maps — ours replaces it); (b) config-load validation *detects* user
conflicts and drops the offending overrides with a typed warning —
it never blocks UI start (§16 Q15; end-goal: the settings-page
journey described in §5.3); (c) the in-app conflict-resolution *UI*
(the warning + settings deep-link + conflict filter) is POST-1.0
(R8-adjacent polish, recorded as the design target). Sequences
make "conflict" subtle (a prefix of another binding): the resolver
treats "user pressed a prefix and then a non-matching key" as
prefix-abort (beep/flash via a status signal), and validation flags
a stroke-sequence that is a strict prefix of another **in the same
context** as a warning, not an error (it is usable, just lossy).

### 5.8 Reserved global actions (day-one seams)

`app.undo`, `app.redo` (D4 round later), `capture.quick` (W1 inbox;
ADHD P1), `palette.search` (W1 fuzzy search), `app.settings` (POST-
1.0 settings view), `app.help` (R10 later), `app.exit`,
`app.menu`. All present in the registry at v0.1.0 with labels;
those without an implementation dispatch a disabled no-op (visible in
the dumped registry as disabled — honest, and it keeps the R8 menu
generation total).

### 5.9 R8 discoverability seam (structural, brief R8)

Menus, context menus, and hint surfaces are **generated from the
action registry** (id + label + current-context bindings), never
hand-written: hint attachment is then structural (the `kbd-hint`
primitive renders a binding for an ActionId), and the future
keyboard-image visualiser / "what's relevant now" cheat-sheet are
pure projections of keymap data over the active context. v0.1.0
implements exactly one such projection: the `app.menu` action opens
a generated menu listing the active context's actions with their
`kbd-hint`s — the acceptance bar (§7.3) includes it.

## 6. Scaling & responsive composition (R3 × R9) — normative

### 6.1 The scaling model (stress §3.4's open decision; agenda Q7)

**Application-level root multiplier through the token system.** One
`Theme.base-scale` value (default 1.0); every text-role size and
every spacing token is defined as
`base-px × base-scale` inside the theme module; because the token
lint (§3.4) guarantees nothing outside the theme multiplies raw
dimensions, scaling the root **is** scaling the UI. It is
live-changeable (set the global from Rust; all bindings re-evaluate
— the mechanism stress §3.3 verifies for globals generally).

- Config surface (Q22): `[ui] scale = 1.0` (float, clamped 0.5–3.0;
  out-of-range is a typed config error). The app reads it via figment
  and pushes it into the theme global at UI start; a live slider is
  POST-1.0 (settings view).
- This is the brief's R3 "user preference in config, application
  pure UI-side" shape exactly.

### 6.2 OS DPI alignment & the escape hatch

The windowing system's own scale factor (fractional-scale correctness
improved in 1.18, stress §3.4) keeps doing its job independently.
`SLINT_SCALE_FACTOR` ( honoured at window creation, winit backend)
remains the documented escape hatch for a wrong OS-DPI environment:
the app exposes it as an env-var override documented in README, read
*before* Slint init in bootstrap. `set_const_scale_factor` (one-shot,
irreversible) is **not used** — it composes badly with a user
setting.

### 6.3 Responsive composition rules (R9 foundation)

- **Breakpoints are window-logical-width bands, thresholds are
  constants *not* scaled by `base-scale`** (Q16): `compact` < 720,
  `regular` 720–1439, `expansive` ≥ 1440 (session may tune the
  numbers; the *rule* is the design). Scale and responsiveness
  thereby compose instead of fighting: a kiosk at distance is
  `expansive` + high `base-scale`; a phone-class window is `compact`
  + default scale; both render "the most important functionality"
  (brief R9) because the degeneration policy below is per-breakpoint,
  not per-device.
- Layouts are written against the breakpoint property exposed by the
  window shell (a single `Responsive` global updated by the shell —
  views read `current-breakpoint`, never window px).
- Compositions declare which of their panels are
  primary/secondary/tertiary; demotion order under `compact` follows
  that declaration — one rule, applied uniformly.

### 6.4 Board view degeneration sketch (brief R9's named minimum)

- `expansive`/`regular`: kanban columns per stack, horizontal scroll,
  `task-card`s at full decoration.
- `compact`: single-column = **one stack at a time**; a stack
  switcher (segmented control / vi `J`/`K` friendly) replaces the
  column row; task-cards drop tertiary decorations (rule D slots
  collapse); drag-to-move degrades to a `task.move` action flow
  (keyboard-complete by construction — R4 holds at every size).
- The kiosk rotation mode (W1+) reuses the same composition: rotation
  is an app-side display-mode policy swapping the *view*, never
  custom layout code (agenda Q4).

### 6.5 Adaptive UI — parked (brief R9 research tag)

Reordering/hiding by usage patterns needs usage telemetry, which is
itself a privacy decision this project has not made and will not
make implicitly. Parked as research with an explicit re-entry
trigger (a privacy round + evidence users want it). No telemetry
hook is built, not even a disabled one.

## 7. Keyboard-first & focus architecture (R4) — normative

### 7.1 Focus architecture

- One root FocusScope (the interception point, §5), per-view focus
  scopes with `focus-on-tab-navigation` set only on genuinely
  focusable elements; tab order is declaration order in each view —
  **designed, reviewed, and asserted by the journeys**, not
  accidental.
- Programmatic focus goes through the `.slint`-level `focus()`
  functions (the intended seam per stress §3.1); Rust-side
  `focus_next_item` is available for the harness.
- The visible focus ring is a `Semantic` token (`focus-ring`) and is
  **always** rendered when a scope holds focus — no
  focus-ring-only-on-keyboard heuristics in v0.1.0 (they leak state
  and complicate journeys).
- No mouse-only gesture exists without an action equivalent: every
  drag/click affordance has an action (drag-to-move ⇄ `task.move`),
  reviewable by diffing the action registry against interaction
  handlers — a review-agent checklist item, mechanised where cheap.

### 7.2 Accessibility widget discipline (R7a — the screen-reader half)

- `accessible-role` + `accessible-label` on **every** interactive
  element and every information-bearing text region; compound
  widgets expose semantic children only (1.18 hides redundant
  internals, stress §3.6 — keep our widgets lean the same way).
- This discipline is **mechanically enforced**: the automation
  harness addresses elements by accessible role/label (stress §3.5) —
  an unlabelled control is invisible to the journeys and therefore
  fails CI. One investment, three requirements (R4 journeys, R6
  addressing, R7 semantics) — the strongest synergy in the design.
- Advanced a11y relationships (labelled-by graphs) don't exist in
  1.18.1's property surface; we design within role/label/value and
  re-audit per Slint upgrade.

### 7.3 The keyboard-journey acceptance bar (brief §5.3, concretised)

A scripted, keyboard-only journey suite (run in CI through the
testing backend, §8) must perform, in one continuous session, the
**core verbs of v0.1.0's shipped surface**:

1. launch → focus lands in the board view (assert via focus
   callbacks / accessible query);
2. navigate: boards ↔ stacks ↔ tasks; next/previous at each level;
3. `task.create` — create a task with title via keyboard;
4. `task.edit` — open task detail, edit title, save;
5. `task.complete` — complete the focused task;
6. `task.move` — move a task to another stack;
7. `task.delete` — delete with the confirmation-free flow (undo is
   reserved-but-noop; deletion is immediate, matching the CLI
   philosophy);
8. `app.menu` — open the generated menu, invoke an entry by keyboard;
9. reach the sync-status indicator and read its state;
10. `app.exit` — clean exit path from keyboard.

Scale is config-side in v0.1.0: journeys do not exercise a scale
control; scale *rendering* correctness is the golden tests' job
(§10), not an interaction.

The suite asserts terminal state + focus ownership + the commands
that reached the forwarder (typed outcomes; no label-text
assertions beyond using labels as element *addresses* — §10.3). The
same journeys produce the keystroke-count metrics for check #2
(§9.2).

## 8. Automation-harness seam (R6) — normative

### 8.1 Logical harness — now (Phase 7)

- Dev-dependency `i-slint-backend-testing = "=1.18.1"` beside the
  slint pin (exact pin mandatory — internal crate, no semver; both
  pins move together on every Slint bump, and every bump re-audits
  its API surface — stress §3.5/§5). Acceptance of this
  non-semver dev-dependency is agenda Q8; the pin shape is Q12.
- Journeys run headless (no display, mock time —
  `init_integration_test_with_mock_time` aligns with AGENTS §5's
  no-sleeps rule by construction), address elements via
  accessible-role/label queries, inject keys through the same root
  focus path real users hit (so the journeys exercise the action
  layer end-to-end), and assert component-level state through the
  generated getters plus the commands captured at the forwarder
  boundary (a test fake stands in for the engine — fakes over
  mocks, AGENTS §5).
- **Scriptability rules (the day-one seam)**: every view is reachable
  by action (no state you can only get into by mouse); navigation is
  expressible as `nav.*` actions; no dead-end states. These are
  reviewable invariants, enforced by a journey that walks the full
  view graph from a cold start.

### 8.2 Visual half — follow-up tooling (stress §4.1/Q6)

- The MCP server embedded in the testing backend (feature
  `slint/mcp` + `SLINT_EMIT_DEBUG_INFO=1` + `SLINT_MCP_PORT`;
  localhost, unauthenticated, dev-only) is the recommended LLM-
  debugging lane: element-tree queries, input injection, and
  `take_screenshot` **while the real app runs, compositor-
  independently** — which satisfies the brief's "Wayland and X11"
  requirement for *our app* without external capture plumbing
  (stress §3.5/§3.9).
- v0.1.0 wires only: the dev feature flag + one CI smoke test that
  boots the app headless-with-renderer and captures one screenshot
  through it (1–2 dev-days per stress §6; de-risks the follow-up).
  The polished driver (an LLM-facing script/daemon wrapper, goldens
  management) is POST-1.0 dev tooling.
- External compositor capture (grim/wlroots, X11 xwd, portals)
  stays surveyed-not-built (stress §3.9): per-dev-machine tooling
  for compositor-level screenshots, never a shipped-app dependency.
- **Process model** (Q19): external driver over MCP; no test/automation
  code in the shipped binary beyond the feature-gated MCP init. The
  daemon IPC (ADR 0012) as an engine-side driver lane is noted as a
  future option for driving the *core* headlessly; the UI harness
  does not depend on it.

## 9. First two usability checks (R7) — normative

### 9.1 Colour-contrast over theme token pairs

A pure Rust unit test (day one, CI): the theme module declares its
legible pairs (each text role × the surface tokens it appears on,
plus `Semantic` state colours × their surfaces) as data; the check
computes WCAG 2.x contrast ratios and enforces the recorded
minimums (≥ 4.5:1 body, ≥ 3:1 large/display, per role class). This
is possible *because* R2 is real (brief R7's own note). New token
pairs without a contrast assertion fail a completeness check — the
pair list cannot silently lag the theme.

### 9.2 Maximum keypresses per common action

(Renamed from the draft's "keystroke budgets" after session round 1
found the phrasing confusing — §16 Q17.) Plain statement of what
this is: for each core verb, the journey suite (§7.3) counts the
physical key presses a keyboard-only user needs to perform it (a
chord such as `Ctrl+x Ctrl+c` counts as two presses), and CI fails
if a change pushes a verb above its recorded ceiling (proposed:
complete ≤ 2, move ≤ 4, create ≤ 3, edit ≤ 3, menu-invoke ≤ 3;
session tunes). **What it protects against**: quiet usability
regressions — a refactor that adds a focus hop or an extra
confirmation step, or drops a context binding, so "complete the
task" silently goes from one key to tab-tab-tab-enter. A strictly
better UI (fewer presses) still passes; the check fires exactly
when a common action gets harder. **AGENTS §5 interplay, stated
openly** (agenda Q17): the "no quality ceilings" rule targets caps
on things that should ideally succeed arbitrarily; this is a
*regression ceiling on degradation*. The ceilings live in this plan
(session-approved design contracts), not in test code, so they are
design artefacts, not incidental pins.

Deferred audits (POST-1.0, per brief R7): colour-blindness
simulation over token pairs (cheap once the pair list exists —
likely first follow-up), text-size floor checks, semantic-distance
scoring of labels, interaction-count-per-task-frequency heuristics
(needs journey breadth first). A separate audit report owns them
when picked up.

## 10. UI-crate testing strategy

| Tier | What | How | Rules |
|---|---|---|---|
| Unit (pure Rust) | keymap parse/merge/validate, resolver state machine (chords, prefixes, modes, masking), preset coverage + conflict-freeness, contrast pairs, presentation helpers | plain `#[test]` + `proptest` where inputs are interesting (resolver sequences, time-boundary formatting) | no Slint types needed (§5.5 boundary); AGENTS §5 fully applies |
| Component / journeys | focus ownership, action dispatch, view graph reachability, journeys §7.3 | `i-slint-backend-testing`, mock time, accessible queries, generated getters; engine replaced by a forwarder-boundary fake | deterministic by construction; assert terminal state + typed command outcomes, never transient intermediates |
| Golden visuals | board view at scale extremes + each breakpoint | software renderer, headless, bundled fonts (§3.6); golden images compared with a perceptual-hash tolerance; regenerating goldens is an explicit reviewed step | no GPU in CI; renderer-presence assertion first (stress §3.5's silent-fallback warning) |
| Harness smoke | MCP screenshot path | dev-feature CI job: boot, capture one screenshot, assert non-empty/changes-after-interaction | guards the POST-1.0 tooling lane from rotting |

Notes binding the strategy to house rules:

- **No sleeps**: mock-time APIs exist in the testing backend precisely
  for this; wall-clock waits are a review finding.
- **No text assertions** (AGENTS §5): journeys use accessible labels
  as element *addresses* (structural facts) and assert typed
  outcomes; error/status *messages* are never string-matched. The
  distinction is enforced at review.
- **Snapshot testing** (`insta`) owns the action-registry dump and
  the resolved default keymap — the "documentation as a test" for
  brief §5.2. `AppState`-level behaviour stays in the engine's
  existing insta suite (AGENTS §5); the UI crate never re-tests
  engine semantics — journeys assert *routing only* (higher layers
  test routing; one behaviour, one test, lowest layer).
- **Crate rules restated for tests**: AGPL headers; `unsafe` only via
  Slint-generated code; dev-dependencies (testing backend, insta,
  proptest) do not leak into the shipped feature set; the MCP
  feature is dev-gated.

## 11. Decision agenda for the session

Every row: proposed default, alternatives, consequences. Rows 1–6
are the brief's §6 questions verbatim; 7–12 are the stress-test
surprises; 13–23 are this design's own load-bearing choices.

> **Session round 1 (2026-10-14) resolved all 23 rows** — the
> resolutions in §16 are authoritative. The table below preserves
> the proposals as drafted; the three redirected rows (Q3, Q15,
> Q17) carry a marker and had their normative sections amended in
> the same commit.

| # | Question | Proposed default | Alternatives | Consequences of the default |
|---|---|---|---|---|
| Q1 | Action namespace & keymap storage (brief §6-1) | `scope.verb` ids (§5.2); TOML in `[ui]` config: `preset` + per-action `overrides` (§5.3) | separate `keymap.toml` file; full-copy user keymap | merge-on-upgrade is structural (deltas never clobbered); figment hierarchy applies for free; alternative file = one more config surface + a merge story to invent |
| Q2 | Action/keymap layer location (brief §6-2) | Rust module inside `taskboard-ui-slint` (§5.5) | tiny framework-agnostic crate beside it | no new public surface; testability already achieved via purity; extraction stays cheap later (no Slint types in the module API) |
| Q3 | Notification-with-actions routing (brief §6-3) | toasts are reminders plan-5; when built, snooze/mute from a toast = **UI-originated `StateCommand`s** over the existing mpsc; `NotificationSink` stays outbound-only | reply channel inside `NotificationSink` port | no port widening; toast actions reuse the exact command vocabulary the banner/CLI already use; alternative couples the port to UI interaction semantics **→ redirected round 1 (§16): toasts are a generic surface; reminders are one source.** |
| Q4 | Kiosk vs desktop view architecture (brief §6-4) | **one view tree**; kiosk = display-mode state machine (hides chrome, rotates views) over shared components (§6.4) | two view sets | R1 layering pays off twice; risk: mode state touches every composition — mitigated by chrome ownership living in the window shell only |
| Q5 | First two usability checks (brief §6-4/R7) | contrast-over-tokens + keystroke budgets (§9) | token-pair size floors; label semantic distance | both are day-one CI-cheap and mechanically tied to R2/R5; the research-flavoured options stay deferred |
| Q6 | R6 visual harness split (brief §6-6) | seam now (scriptability rules + dev-gated MCP smoke), driver tooling POST-1.0 (§8.2) | full MCP driver tooling in Phase 7 | Phase 7 keeps 1–2 dev-days only; the LLM-debugging lane is proven by the smoke test without building tooling |
| Q7 | Scaling mechanism (stress §3.4) | root multiplier `Theme.base-scale` + `SLINT_SCALE_FACTOR` escape hatch (§6.1–6.2) | env-var only (restart to change); `set_const_scale_factor` before show | live user setting (R3 first-class); costs the token-discipline lint (Q8); alternatives give up live scaling or compose badly |
| Q8 | Testing-backend internal-crate acceptance (stress §3.5) | accept `i-slint-backend-testing = "=1.18.1"` as dev-dep with upgrade discipline | write our own input-injection harness; skip logical harness | exact-pin + re-audit on every Slint bump (chore recorded in ADR); own harness = far more code than the seam is worth |
| Q9 | Declarative `KeyBinding` element role (stress §3.2) | unused for rebindable actions; root-scope interception is the single path (§5.6) | use it for static binds (modal Escape etc.) | one key path = simpler journeys and conflict story; forgone compiler duplicate-check is replaced by preset/validation tests |
| Q10 | Renderer confirmation (stress §3.8) | adopt default features (winit + femtovg + software, runtime fallback); no skia; `backend-linuxkms` deferred to a real kiosk target | software-only everywhere; skia | one binary serves GPU and no-GPU kiosks; CI consequence: build links GL (linux job keeps system-lib step) or a software-only CI profile (chunk 1 decides, recorded there) |
| Q11 | Slint runtime licensing (stress §4.5) | proceed: AGPL-3.0-only app × Slint tri-license runtime is permitted (AGPLv3 §13 path); one ADR paragraph records it (§14) | royalty-free/commercial tier purchase | no distribution change for this project's posture; revisit only if distribution terms change |
| Q12 | Pin shape (stress §5) | `slint = "1.18.1"` (caret) + `i-slint-backend-testing = "=1.18.1"`; Cargo.lock guards drift; bumps move both together | exact-pin both | patch fixes arrive via caret within lockfile discipline; the internal-crate contract is the constraint actually needing exactness |
| Q13 | Theme direction | one compile-time **dark** theme first; light/high-contrast + runtime switching POST-1.0 (§3.5) | light-first; ship both | kiosk-distance and nighttime bias; the runtime-switch follow-up is already proven expressible |
| Q14 | Preset shipping scope | all three presets (standard/emacs/vi) ship at core-verb coverage, enforced by test (§5.4) | standard only; vi/emacs as downloadable later | "from the get-go" (R5) read as capability+coverage, not three fully-polished maps; per-preset completeness beyond core verbs grows per wave |
| Q15 | User keymap conflict handling | typed config error at load, fail-fast, listing conflicts; prefix-of-sequence = warning (§5.7) | warn + last-wins | explicit keymaps, no silent shadowing; cost: a hostile config blocks UI start (consistent with config-fail-fast elsewhere) **→ redirected round 1 (§16): never block start — drop-and-warn; settings journey is the end-goal.** |
| Q16 | Breakpoint vocabulary & board degeneration | compact <720 / regular / expansive ≥1440, thresholds unscaled; compact board = one stack + switcher (§6.3–6.4) | 2 breakpoints; scaled thresholds | unscaled thresholds keep scale×responsive orthogonal; numbers are tunable later without architecture change |
| Q17 | Keystroke budgets vs "no quality ceilings" | budgets as session-approved designed contracts living in this plan (§9.2) | metric-report only (no assertion) | regression protection; the AGENTS §5 reading is recorded openly (improvements still pass) **→ accepted in principle round 1 (§16); draft phrasing found confusing — renamed + rewritten in plain language (§9.2).** |
| Q18 | Font bundling | bundle sans + mono in-crate (§3.6) | system fonts + snapshot tolerance | offline kiosk, release consistency, deterministic goldens; cost: repo size + a licence-record step |
| Q19 | MCP wiring scope | dev feature + one CI screenshot smoke now; driver tooling POST-1.0 (§8.2) | full driver now; nothing now | smoke keeps the lane alive for ≤2 dev-days |
| Q20 | Reserved global actions | `app.undo/redo`, `capture.quick`, `palette.search`, `app.settings/help/menu/exit` in registry day one, disabled where unimplemented (§5.8) | add with their waves | one-line cost now vs the brief's "retrofit cost is extreme"; dumped registry documents them as disabled |
| Q21 | ADR numbering gap | UI ADR = **0015**; record 0013 debt; small docs follow-up formalises 0013 from the reminders plan §12/§13 (§13.1) | reuse 0013 for UI (collision — rejected); formalise 0013 first, UI = 0014 is taken anyway | numbering stays honest; the debt is one small chunk, not blocking |
| Q22 | `[ui]` config surface v0.1.0 | `scale` (0.5–3.0) + `keymap.preset` + `keymap.overrides`; nothing else | add theme/preset-picker toggles now | minimal config; every knob has a foundation consumer already |
| Q23 | Overdue banner in v0.1.0 | include: rides the already-designed reminders signal path, reads state, snooze/mute via actions (§2.1) | defer to toasts round | first *visible* payoff of the action layer + signal path; cost is one composition + two action bindings |

**Session mechanics** (connect-flow precedent): iterate on this plan
in PR review rounds; each resolved row lands as a §16 decision entry
(+ section edit where it changes a normative section); the header
flips to ACCEPTED when no row is left open. Amendment rounds are
fresh stage-2-style invocations against this branch.

## 12. Proposed implementation chunk decomposition (chunk 4's input)

Reshapes the dispatch plan §5's provisional shapes (a)–(e) against
this design. Sizes at the repo's established pace (analysis §2
scale); full-gate suite green per chunk; one branch/PR per chunk;
stacked only where noted.

| # | Branch (`feat/ui-*`) | Covers (normative sections) | Size | Notes |
|---|---|---|---|---|
| 1 | `feat/ui-shell-theme` | slint dep + pins + features (§5.5/Q10/Q12), window shell + app wiring + `[ui]` config hand-off, UI-command forwarder (backlog [2026-10-09]), theme/token module + text roles + fonts (§3), token lint gate, contrast check (§9.1) | M | closes backlog [2026-10-07] (renderer question answered by stress §3.8); forwarder lands here so chunk 2 can dispatch real commands |
| 2 | `feat/ui-action-keymap` | action registry + resolver + presets + keymap config merge (§5), root FocusScope integration, reserved actions, registry/keymap insta snapshots, `app.menu` generated menu | M–L | pure-module TDD first, Slint adapter last |
| 3 | `feat/ui-views-board` | component library L1/L2 + layering rules (§4), board/stack/task views, sync-status indicator, overdue banner (Q23 accepted), focus architecture + a11y discipline (§7.1–7.2) | L | the v0.1.0 surface itself |
| 4 | `feat/ui-harness-quality` | logical harness + full journey suite (§7.3, §8.1), keypress-count guard (§9.2), golden screenshots at scale extremes × breakpoints (§6, §10), MCP dev-feature smoke (§8.2) | M | the exit-criterion "Slint component tests" line lands here |
| 5 | `docs/ui-adr-release` | ADR 0015 formalised (§13), architecture.org + testing_strategy.org amendments, backlog ✅ bookkeeping ([2026-10-07], [2026-10-09], Q21's 0013-debt note), v0.1.0 release via `bump_version.sh` + release pipeline (roadmap M2) | S | docs + release chore |

Dependency graph: 1 → 2 → 3 → 4 → 5 strictly (each consumes the
previous seam); no parallelisable pair — the value of this wave is
the seams compounding. **Total estimate ≈ 5–7 weeks** (chunk 1: 4–6
d; 2: 8–12 d; 3: 10–15 d; 4: 5–8 d; 5: 2–3 d — consistent with the
stress report §6's foundation-only 15–26 dev-days plus view work).

## 13. Draft ADR 0015 (formalised by chunk 5 on acceptance)

### 13.1 The numbering gap (recorded)

`main` holds ADRs 0001–0012 and 0014. **0013 is a gap**: reserved and
drafted by the reminders design round (plan §12, "formalised by
stage 2") but never landed — no branch carries the file. This plan
takes **0015** for the UI architecture and records the 0013 debt as
a recommended one-chunk docs follow-up (formalise from reminders plan
§12 + §13's approved decisions). 0013 is *not* reused, to keep the
reserved number honest for the reminders ADR.

### 13.2 Draft text

```org
#+title: ADR 0015: Slint UI — Dumb Presentation Client, Action-Dispatch Keymap, Token Theming
- *Date:* 2026-10-14
- *Status:* Draft (design session pending)
- *Depends on:* ADR 0001 (multi-actor core), ADR 0003 (unsafe exception for the UI crate)

* Context

Phase 7 (M2 → v0.1.0) builds the first real frontend over the M1
headless core. The GUI requirements brief (2026-10-14) names
retrofit-hostile foundations — reusable components (R1), theming
(R2), scaling (R3), keyboard-first (R4), fully-rebindable keybindings
with chords and modes (R5), an LLM automation harness seam (R6),
accessibility (R7) — which the Slint 1.18.1 stress test verified as
expressible, with two partials: no runtime window-scale API (→
application-level root multiplier) and no public =slint::testing=
(→ the internal =i-slint-backend-testing= crate + embedded MCP
server).

* Decision

1. *The UI crate stays a dumb presentation client* (ADR 0001):
   ArcSwap reads, broadcast subscriptions, command mpsc out. No IO
   crates, no business logic, no state mutation; config is read
   app-side (figment) and handed over as typed values. AGPL-3.0-only;
   =unsafe= only via Slint-generated code (ADR 0003).
2. *All styling flows from one token module* (colour/text-role/
   spacing globals); a repo lint forbids literal colours, font
   attributes, and spacing literals outside it; contrast over token
   pairs is a CI check. Text roles are a closed, theme-level
   vocabulary.
3. *Views bind to actions, never keys.* A Rust-side action registry
   (stable =scope.verb= ids + labels + contexts) and keymap resolver
   (chords, vi-mode contexts, masking) live inside the UI crate as
   pure modules; the keymap is data (preset ⊕ user overrides in
   [ui] config); standard/emacs/vi presets ship with enforced
   core-verb coverage; a single root FocusScope intercepts keys.
   Undo/redo, quick-capture, and search are reserved global actions
   from day one.
4. *Scaling is an application-level root multiplier* over the token
   system (live, config-driven), with the SLINT_SCALE_FACTOR env
   var as the OS-DPI escape hatch; responsive breakpoints are
   unscaled window-width bands; adaptive UI is parked (no telemetry).
5. *The automation seam is logical-first*: journeys via
   =i-slint-backend-testing= (exact-pinned beside the slint pin;
   both move together and are re-audited per bump), addressing
   elements by accessible role/label — which simultaneously enforces
   the accessibility discipline. Visual automation rides the
   dev-gated embedded MCP server (screenshot smoke in CI; driver
   tooling post-1.0).
6. *Renderer set = slint default features* (winit + femtovg +
   software with runtime fallback); no skia; linuxkms deferred to a
   real kiosk target.
7. *Licensing*: AGPL-3.0-only application combined with the Slint
   runtime's GPL-3.0-only OR Royalty-free-2.0 OR Software-3.0
   tri-license is acceptable for this project's posture (AGPLv3 §13
   combination path); recorded here so the distribution question has
   one home. Revisit only if distribution terms change.

* Consequences

- Every later GUI wave (search palette, inbox, pomodoro overlay,
  kiosk rotation, undo tree, keymap visualiser) lands as a
  projection over these seams rather than a subsystem.
- The internal testing-crate pin adds a per-Slint-upgrade audit
  chore; accepted for a headless, mock-time, CI-friendly harness.
- Strict token discipline (incl. spacing literals) is authoring
  overhead traded for live scaling, theme development, and
  mechanical style enforcement.
```

## 14. Risks & mitigations

- **Token-discipline lint is stricter than the brief's letter** (it
  adds spacing literals) → if it proves unworkable in chunk 1, the
  fallback is colours+fonts only, and scaling drops "live" (Q7
  revisited) — recorded now so the retreat is a decision, not drift.
- **Internal testing-crate churn** (no semver, exact pin) → upgrade
  runbook line in ADR 0015 + chunk 4's harness isolates its API
  behind a thin journey-driver module.
- **MCP server binds localhost unauthenticated** → dev-feature-gated
  only, never in release builds; CI job asserts the feature is off
  in the release profile.
- **Journey brittleness** (focus assertions vs widget churn) →
  journeys assert focus ownership and typed outcomes at stable
  seams (root scope, forwarder boundary), not per-widget internals.
- **Scope creep through "just one more view"** → §2 fence + review
  agent diffs against §2.2/§2.3 tables (wave-0 acceptance already
  mandates this).
- **Slint 1.19+ changing verdicts** (a11y, KeyBinding, scale API) →
  every foundation section cites its stress-test §; any bump
  re-checks the cited sections before merging.

## 15. Verification of this round

- Traceability matrix below: every brief R1–R10, §3 item, §5
  criterion → a section or an explicit deferral with reason. A
  silent drop is a review finding (dispatch acceptance).
- Every fact attributed to Slint carries the stress-test report
  section it comes from; nothing was re-derived against the
  framework here.
- Every open question from the brief (per-R "Open questions" +
  §6) and every stress-test surprise resolves to a §11 row or a
  recorded proposal inside a normative section — nothing implicit.

### Traceability matrix

| Brief item | Where |
|---|---|
| R1 | §4 (layering, helpers resolved §4.3, granularity §4.2-E) |
| R2 | §3 (format/naming/compile-time-first resolved §3.1/§3.3/§3.5; vocabulary §3.2; lint §3.4) |
| R3 | §6.1–6.2 (scale × responsive interplay §6.3; config `[ui] scale` Q22) |
| R4 | §7 (acceptance bar §7.3; FocusScope discipline §7.1) |
| R5 | §5 (namespace §5.2; storage/merge §5.3; presets §5.4; Rust-side decision §5.5; conflicts §5.7) |
| R6 | §8 (logical §8.1; visual/MCP §8.2; process model Q19; daemon-IPC lane noted §8.2) |
| R7 | §7.2 + §9 (a11y status: supported, §1.3; contrast-as-unit-test: yes §9.1; first-two-picks §9/Q5) |
| R8 | seam §5.9; surfaces deferred §2.2 |
| R9 | §6.3–6.5 (breakpoints Q16; board sketch §6.4; adaptive parked §6.5) |
| R10 | deferred §2.2 (markup timeline; content-as-data noted §2.3) |
| §3 feature surface (every bullet) | §2.3 table (each: wave + seam) |
| §5.1 no-literal gate | §3.4 |
| §5.2 registry-as-documentation | §5.2 + §10 (insta snapshot) |
| §5.3 scripted keyboard journey | §7.3 + §8.1 |
| §5.4 contrast in CI | §9.1 |
| §5.5 scale-extreme rendering (goldens) | §6.1 + §10 golden tier |
| Stress §4 checklist 1–9 | §1.3 (adopted verdicts); renderer decision §2.1/Q10; capture survey §8.2 |
| ADHD P1/P2/P7/P9/P12 | §1.5 → seams: `capture.quick` §5.8, scaling §6.1, presets/§5.4+theme §3.5, undo reservation §5.8, focus discipline §7.1 |

## 16. Decisions — resolved by user review 2026-10-14 (round 1)

All 23 agenda rows resolved in one review round. Rows not named
below were **accepted as proposed** (Q1, Q4, Q5, Q7–Q14, Q16, Q18,
Q19, Q20, Q21, Q22). The three redirected rows had their normative
sections amended in the same commit; accepted rows with user notes
record them here.

- **Q2 — accepted, with an extraction trigger recorded**: moving the
  action/keymap module out of `taskboard-ui-slint` into its own
  crate is the *intended* move **when a second project/crate wants
  it**; the Slint-free module boundary (§5.5) exists exactly so that
  move stays a cheap refactor.
- **Q3 — redirected: toasts are a generic surface; reminders are
  one source.** Not all toasts are reminders — warnings and
  recoverable errors are toast messages too. The accepted direction
  is "reminders are toasts": the reminders desktop sink (plan 5)
  targets the UI's generic transient-message composition rather
  than owning a reminder-specific surface. Reminder actions from a
  toast (snooze/mute) remain UI-originated `StateCommand`s over the
  existing mpsc; `NotificationSink` stays outbound-only. §2.3's seam
  row amended accordingly.
- **Q6 — accepted, with a bookkeeping action**: the POST-1.0 MCP/LLM
  driver tooling must not be forgotten — appended to the backlog as
  "[2026-10-14] LLM/MCP UI-debugging driver tooling" (this plan
  §8.2 is its origin) in the same session.
- **Q15 — redirected: a keyboard config never blocks the UI from
  starting.** Fail-fast was rejected as hostile usability ("bad
  usability" — user review). v0.1.0 behaviour: invalid/conflicting
  overrides are *dropped* (the preset binding wins for every dropped
  action) with typed findings surfaced via `tracing` warnings and a
  startup-warning state. The **end-goal journey** is recorded as the
  design target and lands with the settings view (POST-1.0): the UI
  starts, displays a warning with a button into the keyboard
  settings page, which opens pre-filtered to show the two (or more)
  conflicting bindings. End-goal state: no app breakage because of
  keyboard configs. §5.3/§5.7 amended.
- **Q17 — accepted in principle; the draft's phrasing was confusing
  and is rewritten.** What the draft called a "keystroke budget" and
  §9.2 now states plainly as **maximum keypresses per common
  action**: for each core verb, count the key presses a
  keyboard-only user needs (a chord like `Ctrl+x Ctrl+c` = 2
  presses); CI fails if a change pushes a verb above its recorded
  ceiling. It protects against quiet usability regressions — added
  focus hops, extra confirmation steps, dropped context bindings
  that turn "complete the task" from one keypress into
  tab-tab-tab-enter. Improvements (fewer presses) always pass; the
  AGENTS §5 "no quality ceilings" interplay note stands (§9.2).
  Ceilings live in this plan, not in test code.

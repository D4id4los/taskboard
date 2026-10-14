# [Plan] GUI Design Session — Requirements Input (Brainstorm Distillation)

- **Date**: 2026-10-14
- **Status**: PROPOSED — an *input document* for the upcoming GUI design
  planning session (Phase 7 / `taskboard-ui-slint`). It distils the
  "GUI: Technical Requirements" section of the user's 2026-10-14
  brainstorm, adds the app-level feature surface the GUI must
  accommodate, and lists the questions the session should decide. It is
  not itself an implementation plan; the session's output plan supersedes
  it.
- **Companion**: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (full categorisation/effort/ordering analysis of the whole brainstorm;
  this doc is its GUI-focused extraction, expanded).

---

## 1. How to use this document

Each requirement below states: the requirement, why it exists, what it
implies architecturally inside `taskboard-ui-slint` (never outside —
the UI stays a dumb presentation client per AGENTS.md §1/§2), and open
questions for the session. Requirements are tagged:

- **[FOUNDATION]** — must be designed into Phase 7's architecture from
  day one; retrofitting later is the classic trap (these shape the
  widget/action/theming skeleton everything else hangs on).
- **[POST-1.0]** — can arrive after v0.1.0 without rework, *provided*
  the foundation items left the right seams.

## 2. Requirement groups

### R1. Reusable custom widgets / widget compositions — [FOUNDATION]

**Requirement**: define custom widgets and widget combinations in a way
that can be re-used at different places in the UI code, so new views
stay consistent and stylistic changes propagate without gaps.

**Why**: consistency across a growing view set; cheap global restyling.

**Implication**: a deliberate component library layer *inside* the UI
crate (in Slint terms: shared `.slint` component files / a component
naming discipline), not copy-pasted view-local markup. The session
should define the layering: primitives (e.g. task-card, badge, list
row) → compositions → views, and the rule that views may not restyle
primitives locally.

**Open questions**: component granularity; where pure presentation
helpers (sorting for display? formatting?) live — display-only mapping
functions are presentation logic and allowed, but must not mutate.

### R2. Theming: colours and typography — [FOUNDATION]

**Requirement**: every colour comes from a theming system — no raw
colour values in drawing code. Most font styling likewise: no raw
"bold"/"font-size" in views; instead named text *classes/types*, each
with a shared attribute set, defined as part of the theme.

**Why**: consistency, later restyling, theme development as a powerful
tool; also a prerequisite for contrast auditing (R7) and any future
light/dark or high-contrast themes.

**Implication**: a single theme definition module (Slint: global
singletons / design tokens) exposing colour tokens and text-role
tokens; a review rule that no view file mentions literal colours or
font attributes. The session should enumerate the initial text-role
vocabulary (title, body, caption, badge, mono/meta, …) — inventing
roles per view is how this decays.

**Open questions**: theme file format; token naming scheme; whether
themes are compile-time only at first (recommended) with runtime
switching as a follow-up.

### R3. First-class UI scaling — [FOUNDATION]

**Requirement**: UI scaling fully supported as a user setting; some
users need bigger text, and this deserves first-class support.

**Why**: accessibility and target-range breadth (kiosk distance
viewing → 4k desktops to small screens).

**Implication**: all layout must be in logical units scaled by one
root factor (Slint has a scale-factor notion the session must verify
against current version behaviour); user preference lives in config
(`figment`, `[ui]` table) but the *application* of it is pure UI-side.
Needs a decision on how scale interacts with the responsive layout
system (R9) so they compose instead of fight.

### R4. Complete keyboard navigability — [FOUNDATION]

**Requirement**: the whole UI completely usable and navigable by
keyboard alone; keyboard-only and keyboard+mouse are equally
first-class interaction styles.

**Why**: stated as a hard requirement; also feeds power-user speed and
accessibility (R7).

**Implication**: every interactive element focus-reachable and
activatable; focus order designed, not accidental; no mouse-only
gestures without keyboard equivalents. This needs an explicit focus
architecture in the view tree (Slint `FocusScope` discipline) and
keyboard handlers that *never* bypass the action layer (R5). The
session should define the acceptance bar (e.g. "a scripted keyboard
journey performs every core verb: navigate boards/stacks/tasks, edit,
complete, move, search, invoke menus").

### R5. Keybinding system: full rebinding, chords, vi modes — [FOUNDATION]

**Requirement**: every keybind in the app rebindable; the system
supports standard combos, emacs-style key-chords, and vi-style modes
from the get-go.

**Why**: user preference diversity is a stated product value; retrofit
cost is extreme.

**Implication**: the single most architecturally significant GUI
requirement. Raw key events must translate through an **action
dispatch layer**: keymap config → resolved action → handler. Views
bind to *actions* ("task.complete", "view.next", "menu.open"), never
to keys. The keymap is data (serialisable, user-editable, default
presets: standard/emacs/vi), living in config storage — but binding
*resolution and dispatch* is UI-internal presentation logic. Modal
state (vi modes, prefix/chord accumulation) is a small state machine
in this layer. This layer is also exactly what R6 (automation) and
R8 (discoverability) want to hook: actions are scriptable and
documentable by construction.

**Open questions**: action namespace scheme; per-view vs global
keymap contexts; conflict detection UX; where preset definitions
live; whether the action layer is a Rust-side module (recommended —
testable without rendering) vs Slint-side logic (avoid: untestable).

### R6. LLM-driven UI automation & debugging harness — [FOUNDATION (seam) / POST-1.0 (full)]

**Requirement**: a way for LLMs evaluating and debugging the UI to
navigate to any view, send inputs to interactable widgets, and take
screenshots of UI state; ideally under both Wayland and X11.

**Why**: the user has seen vision-enabled LLM systems work well on
such harnesses; they switch dev environments between compositors.

**Implication**: two complementary seams. (a) *Logical*: the action
layer (R5) + a driver API ("navigate to view X", "invoke action Y",
"read focus/active view") — this part is UI-crate test surface
(Slint's testing API is the candidate to verify). (b) *Visual*:
screenshot capture of the running window under Wayland and X11 —
compositor tooling (e.g. wlr-screencopy/portal APIs vs X11 grabs)
that must be scoped by the session; it is external-to-Slint plumbing,
probably living in dev tooling rather than the shipped app. The
foundation-day-one part is keeping views reachable and scriptable
(no dead-end states unreachable programmatically); the capture
tooling can follow.

**Open questions**: harness process model (in-app test feature vs
external driver); screenshot fidelity/permissions under Wayland
portals; whether the daemon IPC (ADR 0012) can serve as the
engine-side driver lane with the UI harness riding on top.

### R7. Accessibility & automated usability checks — [FOUNDATION (choices) / POST-1.0 (audits)]

**Requirement**: screen-reader support (what does making it work well
require?); auto-detection of usability issues — colour contrast, text
size, colour-blindness safety, semantic distance of menu/button
labels, interaction-count-per-task-frequency heuristics, and more.

**Why**: stated requirement plus the user's curiosity about how much
can be automated.

**Implication**: two layers. (a) *Screen readers*: Slint's
accessibility story (AccessKit integration status in the pinned
version) must be verified in the session — it constrains widget
implementation patterns (semantic roles, labels on everything). (b)
*Automated audits*: most heuristics are static analyses over the
theme (contrast: token pairs), the widget tree (labels, target
sizes), and action flows (keystroke counts per core verb from R6
journeys). Several are implementable as CI checks on the theme +
action definitions — cheap and re-runnable; the session should pick
the first two (recommend: colour-contrast over theme tokens;
keyboard-journey length for the core verbs) and defer the
research-flavoured ones (semantic-distance scoring) to a separate
audit report.

**Open questions**: Slint accessibility maturity in the pinned
version; whether theme-token contrast can be a unit test from day
one (it can, if R2 is real).

### R8. Keybinding discoverability surfaces — [POST-1.0, with one FOUNDATION seam]

**Requirement**: (a) visualise a selected keybinding set on a keyboard
image, possibly interactive; (b) a "show me the keybindings" function
listing what is relevant *right now* (current view/widget); (c)
keybind hints displayed next to menus/context entries/button hovers,
with nice graphics beyond plain text.

**Why**: learnability of a keyboard-first app is adoption-critical.

**Implication**: all three are *projections of the keymap data over
the action context* (R5) — if the action layer is real, they are
views, not subsystems. The foundation seam: actions carry stable
ids + labels + context tags from day one, and every menu entry is
generated from an action (so hint attachment is structural). The
keyboard-image visualiser is a standalone fun widget (post-1.0).

### R9. Responsive & adaptive UI — [FOUNDATION (responsive) / research (adaptive)]

**Requirement**: responsive from small mobile screens to 4k without
useless whitespace, always keeping the most important functionality
accessible; adaptive UI intriguing but needing further analysis of
what is usefully possible.

**Why**: target-range breadth (kiosk/tablet/desktop/handheld).

**Implication**: layout components must be written against
breakpoint/container behaviour, not fixed sizes; combined with R3
scaling, size the same view for a kiosk at distance and a phone.
*Adaptive* UI (reordering/hiding by usage patterns, context) is a
research item: park it with a note that usage telemetry would be a
prerequisite and is itself a privacy decision — do not build
telemetry for this now.

**Open questions**: breakpoint vocabulary; which views collapse to
what (the session should sketch the board view's small-screen
degeneration at minimum, since it is v0.1.0's centrepiece).

### R10. Markup help page — [POST-1.0]

**Requirement**: a help page explaining the available markup syntax
concisely but understandably.

**Implication**: one doc view; content derived from the active parser
setting (org/markdown). Depends on the markup rendering feature's
timeline (see the org-mode viability report); trivial once that
lands. Keep the help *content* as data next to the parser crate so it
cannot drift.

## 3. Feature surface the GUI must plan for (from the app-level brainstorm)

The design session does not build these, but the foundation must not
paint them into a corner. Views and surfaces named by the brainstorm
(with the analysis report's wave numbers):

- **Today view** and the **organiser view** (next days + calendar) —
  W1/W3: list-shaped views over dated tasks; need the day-boundary
  preference in the data model.
- **Importance–urgency matrix visualisation** + task-list decorations
  — W1/W3: decorations belong to the task-card primitive (R1).
- **Overview display** (scheduled column + habits/time-tracking
  column) for desktop & tablets — W2: two-column dashboard shape.
- **Kiosk kanban ⇄ upcoming-list switching** — W1+; kiosk mode is a
  display-mode state machine over existing views; rotation policy is
  app-side config, not UI logic.
- **Inbox capture + triage interface** — W1: a quick-capture surface
  (global hotkey → small input, no navigation) and a triage view;
  the capture hotkey is an action (R5) available in every context.
- **Fuzzy search palette + filter views** — W1: an overlay-style
  search palette driven by actions; results jump to items.
- **Pomodoro focus overlay** — W2: a full-attention surface (countdown,
  current task, task switch, quick-note, task details, exit); the most
  widget-dense single view in the roadmap; early sketching is cheap
  insurance against an action/focus architecture that cannot express
  "one window, few exits".
- **End-of-day review / journal / statistics views** — W2: form-ish
  views with templated prompts; statistics implies a chart story
  (verify Slint charting approach when the time comes).
- **Undo tree view** — W3: a tree-navigation surface; exists only if
  the undo design (report D4) chooses the tree; the action layer
  must reserve undo/redo as first-class actions from day one
  (standard UX: undo is global, not per-view).
- **Reminder surfaces** — the overdue banner (already a designed
  engine signal path), toast notifications with snooze/mute actions
  (reminders "plan 5"), and eventually insistent-alarm escalation
  (an overlay that demands interaction). Notifications with
  *actions* need a UI↔notification integration decision.
- **Org/markup rendering** in text fields — per the org-mode
  viability report: rendered view + source-edit toggle is the MVP
  shape on current Slint (`StyledText`), WYSIWYG gated upstream. The
  session should verify the pinned Slint version's rich-text status
  and keep editor widgets swappable behind the orgedit adapter plan.
- **Markup help page** (R10), **keymap visualiser** (R8), **PDF/print
  preview** (W2, platform print dialogs).

## 4. Slint stress-test list (verify in the session, do not assume)

The brainstorm's requirements stress the framework choice in specific,
checkable ways. The session should confirm each against the pinned
Slint version and record results in the Phase 7 plan:

1. Focus traversal control & programmatic focus (R4) — needed for
   keyboard journeys.
2. Key event interception at a global layer without per-widget
   handlers (R5) — the action dispatch seam.
3. Runtime theming via singletons/design tokens; text-role abstraction
   feasibility (R2).
4. Scale factor control as a user setting (R3).
5. `slint::testing` API coverage: component instantiation, input
   injection, property reads — enough for the R6 logical harness?
6. Accessibility integration status (screen readers) in the pinned
   version (R7).
7. Rich-text: `StyledText` rendering now; editing upstream status —
   affects the org-editing roadmap (per the viability reports).
8. Renderer choice for kiosk hardware (existing backlog entry:
   skia vs femtovg) — decide in this session, it is Phase 7's
   first commit's business.
9. Screenshot/window-capture path under Wayland *and* X11 for the
   visual half of R6 — external tooling scope, not Slint's.

## 5. Suggested foundation acceptance criteria (for the session to refine)

1. No literal colour or font attribute appears outside the theme
   module (mechanically checkable — make it a grep/lint gate).
2. Every interactive element in the shipped views is focus-reachable,
   and every user-visible verb is an action with a stable id, label,
   and keymap entry — a dumped action registry reads as complete
   documentation.
3. A scripted keyboard-only journey (driven through the test API)
   performs the core verbs of v0.1.0's views.
4. The theme's text-role/token pairs pass a contrast check in CI.
5. Scaling the root factor renders the board view correctly at the
   smallest and largest supported sizes (golden screenshots).

## 6. Open questions for the session

- Action namespace & keymap storage format (R5) — and how presets
  (standard/emacs/vi) ship and merge on upgrade.
- Whether the keymap/action layer lives as a Rust module inside
  `taskboard-ui-slint` or a tiny framework-agnostic crate beside it
  (tempting for testability; watch crate-boundary rules — UI-only,
  no IO, so likely fine inside the UI crate).
- Notification-with-actions OS integration (snooze/mute from a toast)
  — does it route through the existing `NotificationSink` port with a
  reply channel, or a new UI-originated command path?
- Kiosk vs desktop as one view tree with mode switches or two view
  sets over shared components (R1 discipline decides the cost).
- First two automated usability checks to implement (recommendation
  in R7).
- How much of R6's visual harness is Phase 7 work vs a follow-up
  tooling plan (recommendation: seam now, tooling post-1.0).

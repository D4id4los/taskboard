<!--
Recurring review template — EXECUTED IN PLACE by an unattended ZCode
idle-time run. Unlike the other files in this folder it is never
copied and filled in. Queue it from a lean session with the one-line
prompt in README.md; the run reads this file, the common rules it
binds, and AGENTS.md. Cadence and mode are in §1. Shared rules live in
_common-rules.md; the binding prelude below is the one block restated
in every template — change all four copies together. Never edit this
file while a run that uses it is queued.
-->

# UI/UX Design Review

Does the interface work for the person using it? Five lenses over one
slice of the UI: contrast, names and labels, icons, layout facts
(alignment, spacing, size across layouts), and Nielsen's heuristics.
It reviews what accumulates across features; the stage-3 walkthrough
(`docs/ux_planning.org` §2.4) judges each new feature at review time.

> **Binding prelude (the common rules' safety frame, restated).**
> 1. Analysis only: write nothing but this run's report under
>    `.artifacts/reports/` and files in a `/tmp` scratch directory.
> 2. Git: `git fetch` and the §3.2 fast-forward are the only writes —
>    no commit, push, PR, merge, branch, stash, checkout, or reset.
> 3. Cargo: never bare — `scripts/memory-gate.sh` for every command,
>    `scripts/run-quality-gates.sh` for the coverage trio.
> 4. GitHub: create `backlog:` issues and comment; never close,
>    reopen, relabel, edit, or touch pull requests.
> 5. Nobody answers questions: record them in the report and proceed.
> 6. Processes: stop only PIDs this run recorded — never by name.

Read first: `AGENTS.md`, then
`.artifacts/templates/reviews/_common-rules.md` (all of it binds this
run), then §3–§4 below.

## 1. Cadence and modes

- Slug: `ui-ux-design`. Report:
  `.artifacts/reports/YYYY-MM-DD-ui-ux-design-report.md`. One run per
  ticket, fortnightly.
- **Baseline mode** (no previous `*-ui-ux-design-report.md`): all
  eight slices of §4, in this order — S1, S2, S3, S8, S4, S5, S6, S7 —
  so an interrupted run has covered the primary views first.
- **Rotation mode**: exactly one slice — the one with the most commits
  touching its files since its last review. Each slice's
  `reviewed_commit` and file list are in the previous report's Metrics;
  count with `git rev-list --count <reviewed_commit>..HEAD -- <files>`.
  Ties go to the slice longest unreviewed.

## 2. Preconditions and stop conditions

- **Baseline only.** Round 7 of the backlog reduction prepared this
  review's material and must have landed: the issues #1239, #1241,
  #1312, and #1313 are all closed, and no open pull request has a head
  branch named in the chunk table of
  `.artifacts/plans/2026-11-25-backlog-reduction-round7-dispatch-plan.md`.
  If not, write an ABORTED report naming what is still open, file
  nothing, and end.
- Rotation mode: anchoring per the common rules §3.
- A slice whose files moved since the last report is re-mapped by
  name; record the new mapping in Metrics.

## 3. Standards and normative inputs

**Standards.** WCAG 2.2 (W3C, *Web Content Accessibility Guidelines*),
applied to a desktop app: SC 1.4.1 Use of Color · 1.4.3 Contrast
(Minimum) · 1.4.4 Resize Text · 1.4.11 Non-text Contrast · 2.4.7 Focus
Visible · 2.5.8 Target Size (Minimum) · 3.3.2 Labels or Instructions ·
4.1.2 Name, Role, Value. And Nielsen's ten usability heuristics.

**House documents, which decide where they are more specific:**

- `docs/ux-surface-census.org` — the slice's rows, its rulings, and its
  unruled register.
- `docs/widget_interactions.org` — taxonomy, sizing defaults, the four
  interaction laws, usage rules.
- `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md` — §4.2
  layering rules, §6.1 base scale, §6.3 breakpoints (`compact` < 720,
  `regular` 720–1439, `expansive` ≥ 1440), §6.4 degeneration, §7.1
  click affordances have action equivalents, §7.2 accessibility
  discipline, §9.1 contrast over token pairs, §9.2 keypress ceilings,
  and the deferred audits it names (colour-vision simulation,
  text-size floors, label semantics).
- `crates/taskboard-ui-slint/src/token_tests.rs` (the declared contrast
  pairs) and `crates/taskboard-ui-slint/ui/theme.slint`.
- `crates/taskboard-ui-slint/tests/goldens/manifest.toml` and the
  goldens beside it; `docs/ui_testing_instrumentation.org`.
- `.artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md`
  and `docs/kiosk_bringup_runbook.org` — the kiosk's real geometry.
- `.artifacts/reports/2026-11-21-golden-coverage-review-report.md` —
  what the golden set was built to show.
- `docs/coding_guidelines.org` §5 — section gating mounts, it never
  hides with `visible:`.

## 4. Slices

Verify the file lists with `ls`; they are a starting map, not a
contract. A widget is judged in situ by every slice that mounts it and
as a primitive by S8.

| Id | Slice | Files (under `crates/taskboard-ui-slint/ui/`) | Goldens | Census |
|---|---|---|---|---|
| S1 | Board and stacks | `views/board-view.slint`, `views/stack-view.slint`; `compositions/board-layout`, `board-bar`, `toolbar`; `widgets/task-card`, `list-row`, `listview` | `board-*`, `kiosk-board-*` | Slice: Board |
| S2 | Today, habits, timers | `views/today-view.slint`; `compositions/habits`, `timers`, `timer-setter`, `rule-list`, `tracked-today`, `track-entries`; `widgets/stat-bars`, `toggle-strip`, `slider` | `today-*`, `kiosk-today-*`, `track-entries-*` | Slice: Today |
| S3 | Task detail and editors | `views/task-view.slint`; `compositions/due-editor`, `date-picker`, `recur-editor`, `labels-overlay`; `widgets/dropdown`, `dropdown-flyout`, `time-field`, `check-box`, `text-field`, `multi-line-editor`, `tipped-field`, `activation-stop` | `task-view-*` | Slice: Task detail |
| S4 | Capture, search, menu | `compositions/capture-overlay`, `search-palette`, `action-menu`; `widgets/menu-shell`, `menu-row`, `quick-list-flyout`, `kbd-hint`, `ghost-opener`, `plus-glyph` | `capture-overlay-*`, `search-palette-*`, `action-menu-*` | Slice: Capture |
| S5 | Review and planning | `views/review-view`, `review-walk-overlay`, `plan-next-day-view`, `habit-history-view`; `widgets/labeled-value-row`, `section-header` | `review-*`, `plan-next-day-*` | Slice: Review |
| S6 | Settings and export | `views/settings-view.slint`; `compositions/export-dialog`, `status-panel`, `board-ceremony-letter`; `widgets/info-tip`, `empty-state`, `status-dot`, `status-notice`, `confirm-strip` | `settings-*` | Slice: Settings |
| S7 | Alarm, notification, focus | `alarm-window.slint`; `compositions/notification-surface`, `focus-overlay` | `alarm-window-*`, `notification-*`, `focus-overlay-*` | Slice: Alarm window |
| S8 | Shell and primitives | `app-window.slint`; every widget not named above, `icon-button`, `text-button`, `icon`, `tooltip`, `badge`, `action-chip`, `anchored-overlay`, `modal-dialog-host`, `focus-stops` first | `icon-probe-*` | Slice: Focus restoration |

## 5. Method — per slice

Append the slice's section to the report as soon as it is complete.

**A. Coverage preflight.** Parse the manifest with `tomllib`. For the
slice, tabulate each state — populated, empty, degraded or error,
focus and interaction, each open overlay — against the layout axes
(scale, width, height). Mark each cell `seen`, `single` (one image at
one layout), or `missing`. Every `single` and `missing` cell becomes a
row of **Unjudgeable this run**, carrying the exact question it leaves
open. Do not file a coverage gap: #1312 tracks the known ones; add a
row to that issue's context only if a comment there carries new
information.

**B. Contrast.**

1. Read the declared pairs and their classes in `token_tests.rs`:
   Body ≥ 4.5:1, Large ≥ 3.0:1, Non-text ≥ 3.0:1. A pair the list
   declares is CI's business, not this run's.
2. Derive the pairs the slice's markup *uses*: for each text colour
   (`color:`, `text-color:`) and each boundary (`border-color:`), the
   nearest preceding `background:` at an enclosing or equal brace
   depth, with conditional backgrounds expanded into their arms and
   tone functions (`tone-color(`, `tone-fill(`) resolved through
   `theme.slint`. The derivation is lexical, so every result is a
   candidate until confirmed by reading the element.
3. Compute the ratio of each used pair the declared list does not
   contain; report those below their class minimum.
4. Text over a model-supplied fill — label and board colours. Find
   every text drawn on a `background:` bound to a model colour rather
   than a token. For each, name the posture (solid fill, alpha tint, a
   swatch beside the text), what chooses the text colour, and the
   ratio for every entry of `presentation::SWATCHES` plus black and
   white — the bounds of a lenient stored colour. The token tests
   cannot see these pairs; the task-card label chip is the worked
   example (#1415).
5. States the token matrix cannot see — the translucent scrim, hover,
   disabled opacity, selected rows: sample pixel colours from a golden
   that captures the state (the software renderer draws exact values)
   and compute; otherwise reason from the bindings and label the
   confidence.
6. Colour as the only carrier of meaning (SC 1.4.1): simulate
   deuteranopia, protanopia, and tritanopia over the used pairs, list
   the ones that become indistinguishable, and check in the markup
   whether an icon, a text, or a position also carries the distinction.

**C. Names, labels, descriptions.** Inventory every interactive element
of the slice — widgets instantiated with a callback, `TouchArea`,
`FocusScope` — with: location · `accessible-role` · label source ·
tooltip · visible text · key hint. Then check:

1. An interactive element with no role or no label, or an
   information-bearing text region without one (design §7.2).
2. An icon-only control without a tooltip.
3. A label that names the implementation rather than the user's goal;
   unexplained jargon and abbreviations.
4. One verb under different words, icons, or tones on different
   surfaces — compare with the census rows and the labels of the action
   registry (`crates/taskboard-ui-slint/src/actions.rs`).
5. A state change with no visible feedback in any golden or binding
   you can find.

**D. Icons.** For each `IconKind` the slice uses (`widgets/icon.slint`;
assets in `assets/icons/`): what it depicts and whether a reader would
take it to mean that; ambiguous pairs used near each other; icons
standing alone without a label. Opportunities — text-only controls that
would scan better with an icon — form one list per slice.

**E. Layout facts.** Every reported fact names its basis: `source` (a
binding that decides it), `measured` (a number read from a golden), or
`impression` (what the image looks like). Never report a 1–2 px
misalignment on impression alone: measure it or say it is unmeasured.

- *Source pass:* fixed widths and heights; caps against the sizing
  defaults of `docs/widget_interactions.org`; `visible:` bindings that
  gate a section but keep its layout slot; spacing off the
  `Theme.space-*` ladder; breakpoint handling against design §6.3–6.4.
- *Measured pass:* the goldens are flat token colours, so element
  rectangles segment cleanly. With Pillow, ImageMagick, or numpy, find
  the runs of a token colour and report edges, gaps, and text boxes.
- If the offline layout-review analyzer (#1313) has landed —
  `gh issue view 1313` is closed and its closing PR names the entry
  point — run it as its usage documents, outputs directed to the
  scratch directory, any cargo call through `scripts/memory-gate.sh`.
  Its output is candidates; confirm each against source or pixels. If
  it must write inside the repository, do not run it and record that.

Checks, at every layout the slice has a golden for:

1. **Alignment** — do elements that should share an edge or a baseline
   (tolerance: one physical pixel); is centred content centred.
2. **Spacing** — cramped (below `space-1` at the base scale), excessive,
   or inconsistent between siblings; dead gaps.
3. **Size** — click targets against the house minimum of 32 logical px
   (WCAG 2.5.8 needs 24); text against the `Typography` ladder;
   clipped, elided, or overlapping content; at base scale 3.0 on the
   narrowest width, content that overflows or cannot be reached
   (SC 1.4.4).
4. **Across layouts** — where a scale × width matrix exists, the same
   element compared cell to cell; where none exists, the question goes
   to **Needs the layout atlas**.

**F. Heuristics pass.** One entry per Nielsen heuristic: what you
checked in this slice, and what you found or why it does not apply.
Draw on the goldens, the markup, the census rows, and the journey
tests' names. Include the house checks:

- Every pointer affordance has a keyboard or action equivalent (design
  §7.1).
- Escape and undo exist for this slice's destructive and long verbs.
- Destructive verbs route through `ConfirmStrip` or an edit-kind modal;
  the destructive role never takes autofocus; refusals are phrased for
  the user.
- Keypress ceilings (design §9.2): is the guard in
  `tests/support/guard.rs` exercised for this slice's verbs?
- Kiosk posture: in the `kiosk-*` goldens, what is hidden or disabled,
  and whether text is legible at the kiosk's real geometry.

Check open `ui` issues before filing: the walkthrough's MINOR findings
are already there.

**G. Findings, ledger, issues, report** (common rules §4–§6, §9).

## 6. Severity and issue shape

Classes as in the walkthrough protocol: **BLOCKER** (a user cannot
complete or leave a journey; a control or its state is hidden), **MAJOR**
(a heuristic is violated or a success criterion fails in a way that
impedes use), **MINOR** (polish). Default priorities: BLOCKER `p1` when
a core journey is affected, else `p2`; MAJOR `p2` when a WCAG Level A
or AA criterion fails, else `p3`; MINOR `p3`. Add `accessibility` to
`ui` whenever a success criterion is cited.

One issue per distinct problem, carrying the golden name with crop
coordinates or the file:line, the criterion or heuristic, and a
proposed direction. MINOR items of one slice share one "polish list"
issue. A suggestion that is a new capability rather than a defect is
filed with the `design` label; one that would overturn a census ruling
is a Question, not an issue.

## 7. Family metrics

TOML with, per slice: `id`, `files`, `reviewed_commit`, `goldens`,
`used_pairs_undeclared`, `unnamed_interactive`, findings by class,
`unjudgeable`. Then two tables: **Unjudgeable this run** and **Needs
the layout atlas**, each row with its exact question.

## 8. Out of scope

Fixing. Keyboard mechanics beyond what goldens, markup, and the census
show — the journeys own those. Rulings recorded in the census. Code
quality — the architecture review owns it. Behaviour on real hardware.

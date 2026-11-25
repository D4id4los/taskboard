# Centralization Residue — Chunk B2 Plan (round 7)

- **Date**: 2026-11-25
- **Kind**: stage-1 chunk plan (planning only; stage 2 implements)
- **Chunk**: B2 of `.artifacts/plans/2026-11-25-backlog-reduction-round7-dispatch-plan.md` §8
- **Branch**: `refactor/centralization-residue` (worktree `../taskboard-WT-refactor-centralization-residue`, start `origin/main` @ 9aa7f03d)
- **Covers** (normative): #1350, #1359, #1352, #1298, #1361, #1358
- **Governing laws**: O-6 centralized-grammar law; round law [L](a) behavior-identical (the 63-golden deterministic corpus + the test corpus are the oracle); [L](c) lint additions must not flag golden fixtures, absorptions respect the third-consumer rule; [L](d) out-of-scope findings become `backlog:` issues.
- **No UX journey plan**: refactor-only, no new user-visible capability (dispatch plan §1: "no UX plan").

## 0. Diagnosis summary (sites re-verified on main @ 9aa7f03d, 2026-11-25)

Issue line numbers have drifted modestly (the 2026-11-24 audit predates
several merges); every named site still exists. File/line below are
current. All UI paths relative to
`crates/taskboard-ui-slint/`.

---

## 1. #1350 — hand-rolled activation blocks, backdrops, VerbButton (p2)

### 1.1 Activation blocks (FocusScope+TouchArea+Return/Space+a11y)

| site (current) | grammar target | decision |
|---|---|---|
| `ui/views/plan-next-day-view.slint:112-145` `box-activate` (task-card activation; **no modifier guards**) | `TextButton`-class pressable wrap → the shared pressable-row/wrap overlay convention (§1.2) + base-widget key grammar | migrate |
| `:169-200` `earlier-hit`, `:204-235` `later-hit`, `:239-270` `remove-hit` (glyph buttons, **no guards**) | `IconButton` (chevron/plus/trash glyphs) | migrate |
| `:316-336` `pool-hit` (**no guards**) | `GhostOpener`/`TextButton` grammar | migrate |
| `ui/compositions/track-entries.slint:~350` entry-row `row-focus` + "Add entry" row (**no guards**) | entry rows → shared pressable-row overlay; "Add entry" → `GhostOpener` | migrate |
| `ui/compositions/tracked-today.slint:~85-129` entry row (**no guards**) | shared pressable-row overlay | migrate |
| `ui/compositions/focus-overlay.slint:~122` `stop-focus` "Stop session" (guards present; pre-primitive residue) | `TextButton` | migrate |
| `ui/compositions/action-menu.slint:~187` `back-row` "Back" (a11y-only, no keys) | `MenuRow` | migrate |
| `ui/compositions/status-bar.slint:~429-455` `stop-touch` "Stop tracking" | **KEEP — recorded deviation, no migration** (see below) | drop from scope |
| `ui/views/review-view.slint:~582-606` reflection flag chip | `ActionChip` (the clickable chip; the site is a toggle-editor chip) | migrate |

**status-bar `stop-touch` — KEEP (decision).** The site's own comment
(`status-bar.slint:~421-431`, the D-T16 letter deviation) records *why*
it is deliberately a bare `TouchArea` and not an `IconButton`: a focused
chrome `IconButton`'s FocusScope swallows every unhandled key and
dead-ends the §5.1 single-key path (observed on the base tree). The
issue's "last hand-drawn duplicate" quote refers to the stop *glyph*,
which the r4 icon-assets chunk already resolved (the site uses the
shared `IconKind.stop`). Migrating the wrapper would reintroduce the
key-sink bug against the behavior-identical law. Stage 2 comments the
disposition on #1350 with the D-T16 citation; the acceptance-criterion
"No FocusScope+TouchArea activation block remains" is satisfied *for
the raw-key holders* — this site has no raw keys (a11y + pointer only).

**Modifier-guard delta (deliberate, recorded).** Adding the base
widgets' Ctrl/Alt modifier guards changes reachable input at the six
unguarded sites (Ctrl+Return previously activated). This is the exact
diff #1350 asks to make deliberately: record each site in the commit
body and in the plan-completion note. No golden renders a focused
Ctrl+Return state; goldens are expected unchanged by the guards alone.

### 1.2 Shared pressable-row overlay convention

The row-activation overlay is currently re-invented per site (8
shapes). Consumers for one shared convention: plan-next-day
(box rows ×2 shapes + pool), track-entries (entry rows + add row),
tracked-today (entry row) — **≥3 live consumers → minting is
sanctioned** under the third-consumer rule. Shape: a small
`ui/widgets/` pressable-row/wrap primitive (name at stage 2:
`PressableRow`) carrying the FocusScope+TouchArea pairing, the
base-widget Return/Space grammar *with modifier guards*, focus ring,
and a11y button role; `ListRow` stays render-only per Rule E.
`MenuRow` covers the action-menu Back row without a new knob.

### 1.3 Backdrops outside the hosts

| site (current) | target | decision |
|---|---|---|
| `ui/app-window.slint:~1729` `menu-backdrop` | `AnchoredOverlayHost` light-dismiss (menu is anchor-shaped, not dialog) | migrate |
| `:~1827` `undo-picker-backdrop` | `ModalDialogHost` (dialog-shaped panel) | migrate |
| `:~2137` `focus-overlay-backdrop` | `ModalDialogHost` | migrate |
| `:~1160-1198` the four-strip light-dismiss band (habits-pause over a live rect) | `AnchoredOverlayHost` light-dismiss | migrate |

Sequencing: the backdrop migrations land **last** within #1350 — they
are the largest structural moves in `app-window.slint` and the most
golden-visible (light-dismiss tint/extent must stay pixel-identical;
verify against the affected goldens before committing each).

### 1.4 VerbButton retirement

- `ui/views/review-walk-overlay.slint:124` still exports `VerbButton`
  (12 in-file uses); `ui/widgets/text-button.slint` already documents
  the absorption (compact variant carries the VerbButton treatment).
- Decision: retire the export — in-file uses become `TextButton`
  (compact); delete the `VerbButton` component. **Probe/census tail
  (must-do)**: `tests/inventory/severing_census.rs:84,369` registers
  and asserts `VerbButton`/`ReviewWalkOverlay`; the #1362 build.rs
  probe registry and `tests/probes/widget_framework_probe_tests.rs:685`
  reference it. Stage 2 updates the registry + census expectations in
  the same commit. `TextButton`'s compact geometry is documented as
  VerbButton's, so goldens should be unchanged; if the comparator
  disagrees, the delta is diagnosed, not pinned over.

---

## 2. #1359 — small-widget absorptions (third-consumer rule calls)

| row | consumers counted (current tree) | call |
|---|---|---|
| SegmentButton (board-bar segment grammar vs board-layout segment grammar) | **2** (`compositions/board-bar.slint:~300-370`, `compositions/board-layout.slint:~820-855`) | **DROP** — two-consumer absorption is speculative per [L](c). Stage 2 comments the count on #1359; the twin stays until a third segment-grammar consumer appears. |
| ScrollBar (scroll-indicator twins) | **2 verbatim** (`compositions/action-menu.slint:~282-289`, `views/settings-view.slint:~1965-1983`) | **DROP** on the same rule — despite the settings-view "revisit at the third consumer" comment, the recorded trigger condition in the round laws is ≥3 consumers. Comment the count on #1359. |
| review-view section skeleton ×3 + heading rows | **3 sections** in-file (`views/review-view.slint:~157-281`) | **KEEP** — parameterize the three sections in-file; headings adopt the existing `SectionHeader` (already used by timers/habits). No new widget minted. |
| StatCell ×10 (`views/review-view.slint:~139`) | existing-widget adoption | **KEEP** — restage `StatCell` as a thin `LabeledValueRow` display preset. |
| Caption + Badge "!" sidecar chips ×3 (`compositions/status-bar.slint:~370-422`) | **3** | **KEEP** — one passive chip variant (display-only; `ActionChip` stays clickable-only). New small widget, 3 named consumers. |
| SwatchButton twins (`app-window.slint:~1621-1633` vs `compositions/labels-overlay.slint:~278-289`) | 2 sites, but the `ActionChip` `has-swatch` knob **already exists** (`widgets/action-chip.slint:194`) | **KEEP as adoption, not mint** — both sites mount `ActionChip` with `has-swatch`; no `SwatchButton` file is created (a new widget would be a two-consumer mint). |
| Caption + mono-value header twins (`compositions/tracked-today.slint:~43-79` vs `compositions/track-entries.slint:~315-332`) | existing-widget adoption | **KEEP** — both mount `LabeledValueRow` (the mono-value knob exists). |

---

## 3. #1352 — settings-view family

All sites re-verified in `ui/views/settings-view.slint` (drift < 10
lines from the issue).

1. **Twin pickers** (`:507-545` `RemotePicker` vs `:556-594`
   `DeckPublishPicker`): merge into one parameterized component
   (`SettingsPicker`), differing only in copy global, `DropdownConsumer`
   kind, and callback — all already properties. Decision: in-view merge
   (two consumers, in-file dedup — not a widget promotion).
2. **Dot trio** (`RowDot :355`, `StatusDot :371` — shadowing the real
   widget's name — `BusyDot :386`): delete the trio; settings-view
   imports `widgets/status-dot.slint`; add a `busy` knob to the real
   `StatusDot` (knob consumer: the BusyDot sites — ≥4 live uses at
   `:1013, :1072, :1150, :1247`).
3. **Field-row family** (`LabeledField :409`, `GeneralField :601`):
   decision — **record the stay against #1096**. #1096's recorded
   ruling declines the form-field wrapper family pending a third
   label+editor consumer; today's consumers are settings-view (one
   view) and the habit editor rows (one family) — the third consumer
   has not appeared. Stage 2 keeps both components in-view, and
   comments the reaffirmed stay on #1352/#1096. No promotion.
4. **StatCell → LabeledValueRow**: covered in §2 (review-view).

Acceptance oracle: settings goldens + journeys re-pass unchanged.

---

## 4. #1298 — habit editor row anatomy

Sites: `ui/views/task-view.slint:530-537` (habit title row, bare
`TextField`) and `:541-545` (habit description row, bare `TextField`)
inside the habit-variant block (`:521-…`), still in the hand-rolled
grammar the block's own comment records ("W4 absorbs later").

Decision: mount the task variant's editor row anatomy — title →
`TippedField` (the task variant's title row is `TippedField` at
`:776`), description → the shared multi-line editor row
(`MultiLineEditor`, imported at `:16`) if the task variant's
description row uses it; otherwise mirror the task variant's exact
row widget. The habit callbacks (`habit-title-edited` /
`habit-description-edited`) and flow grammar are untouched — this is
row-anatomy only, per the issue. Both variants then present the
identical row anatomy. Oracle: the habit-editor goldens/journeys.

---

## 5. #1361 — token-lint residue

### 5.1 Drop-shadow blur

- Current: three bare `drop-shadow-blur: 12px` sites
  (`compositions/focus-overlay.slint:61`, `compositions/recur-editor.slint:99`,
  `compositions/search-palette.slint:48`); `theme.slint:104` already
  carries `shadow-blur: 12px * base-scale` (unused by these three).
- Decision: the three sites adopt `Theme.shadow-blur`. This is a
  **deliberate golden-visible micro-diff at base-scale ≠ 1** (the
  literal 12px was unscaled — a latent inconsistency; the goldens ride
  base-scale ∈ {0.5, 3.0} per `tests/journeys/golden_visuals.rs:10`).
  Affected goldens (the three panels) are re-recorded with the delta
  recorded in the commit; at base-scale 1 the render is identical.
- The other blur sites (`Theme.space-2` at `widgets/menu-shell.slint:34`,
  `compositions/date-picker.slint:104`, `app-window.slint:1648`) are
  tokenized but use a *different* recipe (8px vs 12px) — out of #1361's
  named scope; recorded as a backlog issue (§7).
- **Lint extension**: extend `scripts/check_slint_tokens.py` rule 3's
  property set with `drop-shadow-blur` (and `drop-shadow-offset`) so a
  bare px blur fails the gate. Safety: the lint scans only
  `crates/taskboard-ui-slint/**/*.slint`; the golden fixtures are PNGs
  + Rust test code outside the scan root — **cannot flag golden
  fixtures**. Run `--self-test` plus the lint after the extension.

### 5.2 Bare hairlines — recorded premise correction

The five bare sites (`compositions/board-layout.slint:603, 658, 705`,
`compositions/habits.slint:474, 502`) are `2px * Theme.base-scale`,
but `Theme.hairline-width` is **1px** (`theme.slint:111`). The issue's
"same job" premise is wrong: adopting `hairline-width` would halve the
rule thickness — a golden-visible change and wrong semantics. Decision:
mint `Theme.rule-width: 2px * base-scale` (5 consumers ≥ 2 — passes
the #1121-style naming bar), consume at the five sites; value-identical
at every scale → goldens unchanged. The premise correction is commented
on #1361.

### 5.3 Raw panel/body widths (~40 sites)

Decision rule (recorded): cluster the raw `Npx * Theme.base-scale`
widths in `app-window.slint` (13+ sites), `views/review-view.slint`
(13 journal-editor sites), and the five composition sites
(`action-menu.slint:379`, `track-entries.slint:151`,
`timer-setter.slint:91`, `board-bar.slint:278`,
`habit-history-view.slint:122-123`) by value+role; **a value family
with ≥2 sites becomes a named theme token** (panel-width families,
e.g. overlay-panel / journal-field widths); **genuine one-offs are
accepted and recorded** in the completion note (no token per
#1121). `views/today-view.slint:427`'s raw `0.5px` epsilon: accept and
record (single-site rounding epsilon). The lint is **not** extended to
general width expressions (rule 3 already covers layout props; the
tokens carry the meaning once adopted) — extension stays at §5.1's
scope.

---

## 6. #1358 — settings_form.rs validation mirror pin

- Mirror: `crates/taskboard-ui-slint/src/settings_form.rs:150-174`
  (`is_kebab` + `url_host_ok` re-implementation); authoritative mint:
  `reqwest_host_ok` in taskboard-app's config validation.
- Dependency direction permits the pin: `taskboard-app` depends on
  `taskboard-ui-slint` (`crates/taskboard-app/Cargo.toml:37`).
- Decision: **cross-crate parity pin test, not extraction**. The
  host-validation predicate is reqwest-shaped (scheme/userinfo policy),
  not domain logic — extracting into `taskboard-domain` would drag an
  HTTP-adjacent concern into the pure domain crate. The pin lives in
  `crates/taskboard-app/tests/` (or a `#[cfg(test)]` module if the
  harness prefers), asserting a shared fixture table (valid/invalid
  hosts, kebab names — including the table's edge rows: empty host,
  userinfo, non-http schemes) against **both** predicates. The mirror
  predicates get a minimal `#[doc(hidden)] pub` test exposure with the
  advisory-only framing kept in their docs. **The pin fails on
  divergence** — that is the acceptance.

---

## 7. Execution order (stage 2)

Lowest-risk first, goldens re-checked per group:

1. **#1358** (Rust-only parity pin; no UI surface).
2. **#1361** §5.2 (rule-width token, value-identical) → §5.1 (shadow
   token + lint extension + golden re-records) → §5.3 (width sweep).
3. **#1352** (settings-view: pickers, dot trio + busy knob; field-row
   stay recorded).
4. **#1298** (habit rows onto TippedField/MultiLineEditor).
5. **#1359** keep-set (review-view parameterization + SectionHeader +
   StatCell; caption chips; ActionChip swatch adoption; LabeledValueRow
   headers).
6. **#1350** activation blocks + PressableRow + VerbButton retirement
   (+ census/registry updates) → backdrops last.

Commits per item (`refactor(ui): ...` style), each standing alone
(green build, hooks pass).

## 8. Acceptance (chunk level)

- [ ] No raw-key activation block in `ui/views/`/`ui/compositions/`
      outside `ui/widgets/` (status-bar `stop-touch` excepted per §1.1
      recorded deviation)
- [ ] No hand `TouchArea` backdrop outside the two hosts
- [ ] `VerbButton` no longer exported; census/registry updated
- [ ] Token lint green with the §5.1 extension; `--self-test` passes
- [ ] #1358's pin fails on injected divergence (self-check)
- [ ] Golden corpus + test corpus green; every deliberate delta
      (modifier guards, scaled shadow blur) recorded in commit bodies
- [ ] The two DROP dispositions commented on #1359 with consumer counts

## 9. Risks

- **Backdrop migrations** (§1.3) are the largest structural moves;
  light-dismiss tint/extent must stay pixel-identical or goldens break.
  Mitigation: per-migration golden comparison, land last.
- **VerbButton retirement** touches the severing census + probe
  registry (#1362's build.rs) — stale registry entries fail the build
  tests; updated in the same commit.
- **PressableRow mint**: the one new abstraction; consumers named in
  §1.2 (≥3). If stage 2 finds the row shapes too divergent, fall back
  to per-site base-widget adoption and record — do not force the mint.
- **Golden re-records** (§5.1) are the only intended golden deltas;
  anything else that moves a golden is a bug to diagnose, not re-record.
- **app-window.slint congestion**: B1/B3-class parallel chunks may
  touch the same file — rebase discipline per dispatch-plan §1.

## 10. Estimated diff size

M/L per the dispatch plan: ~1 200–1 800 lines changed across ~20 files
(9 views/compositions heavy: app-window, settings-view, review-view,
task-view, plan-next-day-view, track-entries, tracked-today,
status-bar, focus-overlay; theme.slint; check_slint_tokens.py;
settings_form.rs + app tests; severing census + probe registry). New
files: ≤3 small widgets (`PressableRow`, passive chip, `SettingsPicker`
is in-view) + 1 pin test.

# The label chip contrast chunk plan (round 8, chunk L4, #1415)

Stage-1 artefact. Branch `fix/label-chip-contrast`, worktree
`../taskboard-WT-fix-label-chip-contrast`, start point `origin/main`
(977863d9). Normative inputs: the round-8 scope report §A-L4
(`.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md`),
issue #1415, the landed contrast method (#1494: `token_tests.rs` +
`scripts/layout_review.py` Q1, byte-for-byte WCAG transcription,
known-answer pins 21.0 / 1.0), and the #1410 invariant family.

## 1. Diagnosis — the chip today

The only text-on-model-fill site in the UI tree:

- `crates/taskboard-ui-slint/ui/widgets/task-card.slint` (the
  `chips-row`, tag-color plan D-9/US-3): each chip is
  `Rectangle { background: chip.color; RoleText { role:
  Typography.badge; text-color: Theme.text-secondary; } }`. The fill is
  the label's own `swatch_hex` echo; the text is a **fixed light grey**
  (`#a6abbd`) regardless of the fill. Measured over
  `presentation::SWATCHES` (issue #1415's table, re-measured at
  stage-1): 1.10:1 (green) to 2.05:1 (blue) — all far below WCAG 2.2
  SC 1.4.3's 4.5:1. The badge role is 11 px, so no large-text
  allowance.

Why the existing gates miss it: `token_tests.rs` asserts minimums over
*declared token pairs*; this pair is (token, model colour) — the label
colour is not a token (#1414's blind-spot class). The analyzer's Q1
(#1494) is pixel-side and would flag it, but is advisory-only by design
(plan D6: no CI wiring).

The corpus is otherwise clean:

- `task-view.slint` label `ActionChip` (`has-swatch: true`,
  `chip-background: Theme.surface`) — swatch-dot posture, text on a
  theme surface, no text-on-fill.
- `board-bar.slint` and `compositions/labels-overlay.slint` draw the
  swatch as a dot/square beside the title on the surface — same safe
  posture.

So the rule lands at **one site**: the task-card chip. The single
`TaskLabelChipModel` producer is
`models.rs::task_card_label_chips` (plus the detail-view twin
`label_chip_rows`, which feeds the already-safe `ActionChip` and must
**not** be touched).

## 2. The threshold decision + its WCAG anchor

**Anchor**: WCAG 2.2 SC 1.4.3 Contrast (Minimum), 4.5:1 for normal
text; the chip's `Typography.badge` role is 11 px, so the large-text
relaxation (3.0:1) never applies.

**The rule (typed)**: `presentation::chip_ink(fill_body) -> ChipInk`,
where `ChipInk` is a two-variant enum (`Light` → `#ffffff`, `Dark` →
`#000000`), decided by the fill's WCAG relative luminance `L` against
the threshold

```
L* = sqrt(0.05 × 1.05) − 0.05 ≈ 0.17913
```

— the unique luminance at which black and white inks give **equal**
contrast (4.58:1 each). `L ≥ L*` → `Dark` (dark ink on a light fill);
`L < L*` → `Light`. Property (provable, and pinned by proptest): for
**any** opaque fill, `max(contrast(fill, black), contrast(fill,
white))` is minimized exactly at `L*` and equals 4.58:1 there, so the
rule clears 4.5:1 for every opaque fill — a guarantee, not a
heuristic.

**Palette outcome** (matches #1415's fix-direction-1 measurement): the
blue `3a70d6` (`L = 0.1734 < L*`) takes white ink (4.70:1); the other
five swatches take black ink — green 10.06, red 5.74, yellow 12.14,
purple 5.42, grey 6.49. (Choosing black everywhere would strand the
blue at 4.47:1 — below the floor; the threshold is what makes the
guarantee hold.)

**Ink values are consts, not theme tokens.** The pair being judged is
(model fill, model-independent ink); a theme token for pure black/white
does not exist and introducing one is the #1437 theme-design round's
territory. The consts carry a documented rationale; the rule is
fill-relative and composes with any future theme switch.

**Formula single-definition.** `relative_luminance` / `contrast_ratio`
move from `src/token_tests.rs` (test code) to `presentation.rs` as the
one production definition; `token_tests.rs` consumes them. This keeps
`layout_review.py`'s "the token test suite is the single definition of
the formula" comment true (its pointer comment is updated to the new
path); the Python side stays a byte-for-byte *transcription* — do not
rewrite it to import anything.

**Leniency, documented at the type.** Non-hex domain `Color` bodies
never reach the chip fill (`swatch_hex` renders them transparent — the
D-5 hygiene), so `chip_ink` is only ever called with a validated
six-hex body; the enum carries no validate-and-forget. #1440's future
HSL/HSV free-form colours land in the same lenient `Color` and compose:
the rule handles any opaque hex by construction.

## 3. The fix shape (stage-2's work list)

1. `presentation.rs`: hoist `relative_luminance` / `contrast_ratio`
   from `token_tests.rs`; add `ChipInk` (enum, `#[must_use]`-style
   pure API) + `chip_ink(body: &str) -> ChipInk` + `ChipInk::color()`
   returning the const `slint::Color`. SPDX header already present in
   the file; the crate already `#![forbid(unsafe_code)]`.
2. `models.rs::task_card_label_chips`: each `TaskLabelChipModel` gains
   `ink: chip_ink(body).color()` — the sole producer site.
3. `task-card.slint`: `TaskLabelChipModel` struct gains the `ink`
   property; the chip's `RoleText` reads `text-color: chip.ink;` —
   **one render site**, no per-consumer copies.
4. `token_tests.rs`: switch to the hoisted fns; known-answer pins
   unchanged.
5. `layout_review.py`: update the pointer comment only (file/line of
   the single definition); the transcription itself untouched.
6. Golden regeneration: the board goldens carry label chips (the
   enriched fixture, `fixture.rs::golden_state_enriched_at`), so the
   ink flip is a visible render change — regenerate with
   `TASKBOARD_REGEN_GOLDENS=1`, review the PNG diff as an
   intended-change (text-colour-only, localized to chip text areas),
   manifest unchanged.
7. Commit split: `refactor(ui): hoist the WCAG colour math into
   presentation` (no behaviour change) → `feat(ui): derive the label
   chip's ink from the fill's luminance` (rule + render site + goldens)
   → `test(ui): pin the chip ink rule both ways` (or pins in the feat
   commit — stage-2's call, TDD order: the pins are written first and
   must fail pre-fix).

## 4. The pins' design (fail pre-fix, per the #1410/#1494 method)

All typed outcomes; no text assertions anywhere.

- **Known answers** (mirror `wcag_math_is_sanity_anchored`):
  luminance `#000000` = 0.0, `#ffffff` = 1.0; ratio black/white = 21.0;
  same colour = 1.0. (These pass pre-fix — they pin the hoist.)
- **The deployed-ink pin (fails pre-fix)**: for every
  `presentation::SWATCHES` entry, the ink actually carried on the chip
  model clears 4.5:1 against the fill. Pre-fix the model carries no
  ink and the render pins `text-secondary` (1.10–2.05:1) — the pin is
  written against the *model output* (`chip.ink`), so it fails to
  compile-exist pre-fix and fails its assertion if `ink` is stubbed to
  the old grey.
- **Constructed cases both ways**:
  - dark-fill case: `3a70d6` → `matches!(chip_ink(..), ChipInk::Light)`
    and 4.70:1 ≥ 4.5;
  - light-fill case: `edbf45` → `ChipInk::Dark`, 12.14:1 ≥ 4.5.
- **Boundary both ways**: a constructed fill just below `L*` picks
  `Light`, one just above picks `Dark` (compute two hex bodies around
  the threshold; exact-inequality, no floats-in-assertions tricks).
- **Minimax property pin**: for every swatch, the chosen ink's ratio ≥
  the rejected ink's ratio.
- **Proptest** (`proptest`, pure domain logic): for any opaque fill
  (arbitrary `(u8, u8, u8)`), `chip_ink(...).color()` vs the fill is ≥
  4.5:1. Plus the guarantee's edge: `max(black-ratio, white-ratio)` ≥
  4.5 for the same inputs (the rule's licence).
- **Structural pin**: `task_card_label_chips`' output `ink` equals
  `chip_ink(swatch body).color()` (typed equality, no reimplementation
  inline in the test).
- **Unit tests live next to the rule** (`presentation.rs` `#[cfg(test)]`
  module or `token_tests.rs` — stage-2 picks the lower layer; one
  behaviour, one test, lowest layer).

## 5. Stage-3 traps

- **Golden budgets are never widened** — the intended diff is
  chip-text-localized; a broad luma/chroma diff means the fix leaked
  somewhere else.
- **One site**: the rule must not leak into `ActionChip`, the board
  bar, or the labels roster (swatch-dot posture, already correct).
  `label_chip_rows` (detail view) stays untouched.
- **`layout_review.py` stays a byte-for-byte transcription** — a
  pointer-comment update only; its self-test pins (21.0 / 1.0) must
  stay green.
- **No theme token for the inks** — that decision belongs to #1437's
  design round; the consts are documented, not smuggled into
  `theme.slint`.
- **No text assertions, no quality ceilings** — the 4.5:1 floor is a
  *floor*, fine; never assert "≤ X" on success paths.
- Advisory bonus (optional, report-only): a post-fix analyzer corpus
  re-run should show the label-chip Q1 findings class shrink; no CI
  wiring, per plan D6.
- Gate discipline: every cargo invocation through
  `scripts/memory-gate.sh`; the full suite through
  `scripts/run-quality-gates.sh`; commit landed work before any wait.

## 6. Out-of-scope

None filed at stage-1: the corpus scan found no second text-on-fill
site, the overflow chip is a token pair already inside
`token_tests.rs`'s PAIRS coverage, and #1440 composition is handled by
the rule's any-opaque-hex construction (recorded in §2, not an issue).
The hue/tone-ladder half of #1410 remains open elsewhere (#1494's
deferral stands).

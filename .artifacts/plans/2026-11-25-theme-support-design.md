# Theme Support Design Round (#1437) — Design

- **Date**: 2026-11-25
- **Status**: DECIDED (design round; implementation gated on operator acceptance — GATE-PAUSED)
- **Origin**: issue #1437 via the round-8 scope report
  (`.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md` §C-C1)
- **Scope of this document**: the theme token lattice, the dark/light pair,
  the switch surface, the persistence key, the file-driven extension
  path, and the migration shape. Not an implementation plan — the C1-impl
  skeleton lives beside it
  (`2026-11-25-theme-support-impl-plan.md`, completed at gate clearance).
- **UX law note**: the theme switch is a user-visible setting, so a UX
  journey plan (from `.artifacts/templates/ux-journey-plan.md`) is
  **mandatory before the dispatch plan may chunk the work**
  (docs/ux_planning.org). It is listed as a gate-clearance item in the
  impl skeleton.

## 0. The verdict

**Build the two-theme switch now (small, well-isolated); defer
file-defined custom themes to a backlog item gated on demand.** This is
the issue's own evaluation and this round confirms it against the code:

- The entire colour surface is ~10 `Theme` palette hexes + 7 `Semantic`
  state hexes in one file (`crates/taskboard-ui-slint/ui/theme.slint`);
  zero literal colours exist outside it (the token lint enforces this;
  the #1361 census confirmed zero residue).
- Views reference semantic/role tokens only, so a palette swap touches
  **no view file**. The switch is: make palettes a data structure, add a
  light palette, extend the contrast gates and the golden manifest.
- The cost grows monotonically with every new token; doing it while the
  palette surface is this small is the cheap moment.

What was evaluated and rejected is recorded per decision below; there is
exactly one recommendation per open question.

## 1. The token lattice — what becomes a token, what stays

### 1.1 Current shape (inventory)

`theme.slint` today holds three layers:

| Layer | Contents | Literal hexes |
|---|---|---|
| `Theme` palette | background, surface, surface-raised, surface-hover, text-primary, text-secondary, border, focus-ring, scrim | 9 (scrim is 8-digit RGBA) |
| `Semantic` state | success, warning, danger, overdue, sync-active, offline(=derived), focus-ring(=derived) | 5 literals |
| Everything else | spacing scale, radii, widths (hairline/rule/resize/panel/journal), typography (sizes + families), shadow-blur, window defaults, layout meta-constants | 0 colours |

The #1361 residue families (raw `Npx * base-scale` widths) are
**dimension tokens, not colours** — they are theme-invariant and stay
out of this round entirely (the panel-width families already landed;
the remainder is recorded per-family in that sweep).

### 1.2 Decision D1 — the lattice is "every colour literal becomes a palette field; nothing else moves"

**Recommendation**: the palette data structure covers exactly the two
literal-hex layers: the 9 `Theme` palette colours and the 5 `Semantic`
state literals (success, warning, danger, overdue, sync-active). The two
derived `Semantic` entries (offline→text-secondary,
focus-ring→Theme.focus-ring) stay derived — they are conditionals, not
data, and keep working unchanged under any palette.

Explicitly **not** tokens (theme-invariant, recorded so the boundary is
a decision, not an accident):

- **Typography sizes/weights** — scale-driven, palette-independent.
  Leuven's typographic character is *not* imported (see D2).
- **Font families** (Inter / JetBrains Mono) — stay global constants.
- **Spacing, radii, widths, window dimensions, layout meta-constants** —
  geometry, not colour.
- **`scrim`** — it *is* a palette field (it must darken on light), but
  stays 8-digit RGBA and invisible to the contrast extractor (the
  existing law: a translucent wash is not a text carrier).

Consequence: `theme.slint`'s hex literals become the **dark palette's
defaults** — the pre-push fallback values, still literal, still exactly
what the token-lint and the contrast extractor read today. The token
lint (`scripts/check_slint_tokens.py`) needs no change; literal colours
remain legal only in theme.slint.

### 1.3 Decision D2 — the palette lives in Rust as data; .slint keeps the dark defaults

Three shapes were evaluated:

- **(a) Slint-side conditionals** (`in-out property <ThemeKind>` plus a
  ternary per token): no Rust-side duplicate, but 14 ternaries per
  token line breaks the token_tests extractor (it parses literal
  `out property <color> x: #hex;` lines), and a third theme means
  editing 14 expressions. Rejected.
- **(b) Slint struct/array of palettes in .slint**: same extractor
  problem; Slint's story for iterating labelled colour sets is weak.
  Rejected.
- **(c) Rust-side palette table pushed into the `Theme` global** —
  **recommended**. A `Palette` struct in `taskboard-ui-slint`
  (e.g. `src/palette.rs`) with two `const` values, `DARK` (verbatim
  current values) and `LIGHT` (leuven-derived); pushed per-property via
  the generated global's setters — the established pattern is the
  `base-scale` push at window construction (models.rs / the `UiScale`
  lane), and `SetScale` shows the live-push precedent.

The one real cost of (c) is a **deliberate duplication**: the dark hexes
exist both as theme.slint defaults and as `Palette::DARK`. This is the
house mirror-parity pattern (settings_form.rs mirrors config validation;
drift breaks a test): a parity test asserts the extractor's read of
theme.slint equals `Palette::DARK` field-by-field. The .slint defaults
are the boot-time pre-push fallback (a window is rendered before the
push on slow paths only; in practice the push happens at construction,
same as base-scale).

### 1.4 The mirror types (A1 pattern)

Per the type-driven design policy (AGENTS.md §2): a hand-rolled private
field newtype `ThemeName` with fallible minting at the trust boundary
(config decode / env parse), holding `Dark | Light`. No derive crates.
Unknown theme strings fail config validation with a typed
`ConfigError::InvalidTheme` (the `InvalidScale` precedent,
config.rs:1036). Leniency: none — a two-value closed set; the file-driven
future widens it later deliberately (§5).

## 2. The dark/light pair

### 2.1 Dark = the current palette, verbatim

`Palette::DARK` is byte-identical to today's values (the 9 `Theme` + 5
`Semantic` hexes above). Golden dark images do not re-record; the goldens
suite stays green untouched for dark.

### 2.2 Light = emacs leuven-derived, tuned to pass the gates

`Palette::LIGHT` derives from emacs leuven's chroma (white page, warm
grey surfaces, muted accent), as *starting values*, not contract:

| Token | Dark (today) | Light (leuven-derived seed) |
|---|---|---|
| background | `#14161c` | `#ffffff` (leuven page) |
| surface | `#1d2028` | `#f7f8f8` |
| surface-raised | `#262a35` | `#eef0f2` |
| surface-hover | `#303545` | `#e2e6ea` |
| text-primary | `#e8eaf0` | `#333333` (leuven body) |
| text-secondary | `#a6abbd` | `#5f6368` |
| border | `#646c85` | `#b4b8c0` |
| focus-ring | `#7aa2f7` | `#4a6dbe` (leuven-blue adjusted) |
| scrim | `#0b0d1280` | `#1a1c2280` (dark wash on light) |
| success | `#6fd98b` | darker green, gate-tuned (seed `#2e8b57`) |
| warning | `#e8c268` | darker amber, gate-tuned (seed `#b58900`) |
| danger | `#e8756f` | darker red, gate-tuned (seed `#c0483f`) |
| overdue | `#ff9e64` | darker orange, gate-tuned (seed `#a0522d`) |
| sync-active | `#7aa2f7` | gate-tuned blue (seed `#4a6dbe`) |

The seeds are implementation-round tuning inputs; **the WCAG contrast
floors are the contract**, not the seeds. The surface ladder's invariants
hold per palette (each step paints only over lower steps; hover ≥ the
surface→raised step against both parents — the existing distinctness
guard, parameterized over both palettes).

**Known interaction — #1414**: the contrast pair list currently skips
the raised/hover surfaces for state colours and border, and those pairs
measure below minimum on dark. The light palette makes this worse, not
better (dark accents on mid-grey raised surfaces). C1-impl either fixes
the state-colour mapping for the raised/hover pairs (preferred — it is a
#1414-shaped production fix) or carries the exemption forward with the
issue referenced; it may not silently widen the exemption. #1415 (chip
text-on-swatch 1.1:1–2.1:1) is likewise a live bug the light palette
inherits; both issues get a cross-link comment when impl starts.

### 2.3 The contrast gates become palette-parameterized

`token_tests.rs` currently extracts literal hexes from theme.slint and
checks the declared pair list once. It keeps doing exactly that (the
.slint defaults are a real palette — dark), and gains a second loop over
`Palette::LIGHT` from the Rust mirror: every declared pair × both
palettes must meet its role-class minimum, plus the hover-distinctness
guard × both. The completeness rules (every token in a pair or
exemption) run over the union.

## 3. The switch surface

### 3.1 Decision D3 — settings-view appearance row + config/env for the ops surface (O-5 parity)

The O-5 standing ruling (2026-11-18): no feature may be CLI-only. The
switch therefore lands on both lanes in one chunk:

- **GUI**: the settings view's general section gains a theme selector
  row beside the scale row, following the scale field's exact grammar
  (committed-value echo, validity mirror in `settings_form.rs` /
  `settings_status.rs`, a `theme-committed` callback; the
  `UiRequest::SetTheme { choice, reply }` request patterned on
  `SetScale` at requests.rs:893).
- **Ops/kiosk**: `[ui] theme` in `taskboard.toml` plus the
  `TASKBOARD_UI_THEME` env override through the existing figment ladder
  (defaults → file → env). Kiosk machines are configured by file; that
  is their "CLI".

### 3.2 Decision D4 — live apply, no restart

Unlike `scale` (a window-construction multiplier), colour properties are
plain globals: pushing a palette at runtime re-renders every view
(consistent with the SetScale live push). The switch applies
immediately: push the 14 palette properties (+ scrim), no window
rebuild, no restart. Recorded alternative (rejected): restart-to-apply —
simpler but a worse kiosk/operator experience for zero real savings; the
push is ~14 setter calls.

## 4. Persistence

### 4.1 Decision D5 — `[ui] theme = "dark"`, persisted immediately via the SetScale writeback lane

- Key: `[ui] theme`, `serde` string, **default `"dark"`** (dark stays
  the zero-config look; existing configs upgrade by absence — the
  serde default is the migration).
- Values: `"dark" | "light"` only; anything else is a typed config
  validation error (D1's newtype mint).
- Writeback: the GUI switch persists through the same lane scale uses —
  `toml_edit` overlay (`overlay_document` / `write_config_atomic`,
  config.rs:2062+), so comments and unrelated tables are preserved.
  Choosing a theme in the settings view writes the key immediately
  (the scale lane's established ceremony); the config file remains the
  single source of truth across restarts.
- Env precedence: `TASKBOARD_UI_THEME` wins over the file (figment
  ladder order) — the kiosk override story, consistent with
  `TASKBOARD_UI_SCALE`-class overrides.

## 5. The file-driven extension path (deferred, but not blocked)

**Decision D6**: file-defined custom themes stay a backlog issue gated
on evidence of demand (the issue's own #218-style framing). The design
spends nothing on a loader but shapes the data so one is additive:

- The future theme file format **is** the `Palette` struct serialized
  (TOML: one field per token, the same 14 keys). A loader then = read
  file → deserialize `Palette` → contrast-check at load time (the same
  gate code as token_tests, extracted to a shared pure function) →
  push. None of this year's work gets rewritten.
- The registry grows from `Dark | Light` to a name→Palette map; the
  `ThemeName` newtype's closed set is the deliberate leniency point to
  revisit then (its API churn is acceptable while internal, per the
  type-driven policy).
- Backlog issue opened: see §7.

## 6. Migration shape

The honest answer is that **there is almost no migration**: the #1361
census found zero literal colours outside theme.slint, and views bind
semantic tokens only.

1. **No view edits.** All ~40 hard-coded *width* sites from #1361 are
   dimension tokens — theme-invariant, untouched by this round (their
   sweep already landed or is that issue's own ledger, e.g. #1416 for
   the 8px shadow-blur residue).
2. **theme.slint**: the 14 hex literals become documented as the dark
   defaults (values unchanged); the palette properties gain `in-out`
   (or keep `out` + setter — the base-scale precedent decides at impl;
   either keeps the lint green). The "no switching machinery (POST-1.0)"
   header comment is deleted — the deferral it records is now void.
3. **Rust**: `src/palette.rs` (`Palette`, `DARK`, `LIGHT`, push fn);
   `ThemeName` newtype; config field + validation; `SetTheme` request;
   settings row + adapter wiring. All new code, no rewiring of existing
   call sites beyond the construction-time push joining `base-scale`.
4. **Tests**: token_tests gains the light loop; the parity test pins
   .slint-defaults ≡ `DARK`; goldens gain a light dimension (§6.1).
5. **Quality gates**: the full gate sequence is the dispatcher's job
   (dispatch discipline); C1-impl gates on pre-commit hooks + flock-wrapped
   nextest per docs/agent_dispatch.org.

### 6.1 Decision D7 — light goldens: a representative subset, not a corpus doubling

The golden corpus doubles if every case gets a light variant. Decide:
**a named subset** — one case per breakpoint class and one per major
surface (board, today, detail, settings, focus overlay ≈ 6–8 cases) —
committed as light goldens at impl time with the frozen render clock law
(testing_strategy §6.1: the clock seam rules apply per capture). The
rest of the corpus stays dark-only, recorded in
`tests/goldens/manifest.toml` so the asymmetry is a stated decision, not
an omission. Rationale: the palette is global; once the subset proves
the light push renders everywhere, per-view light goldens add coverage
of the same push, not new behaviour. Revisit only if a light-only visual
bug class appears.

## 7. Out-of-scope findings → backlog

- **File-defined theme loader** (the deferred subsystem of §5) — issue
  opened this round; number listed in the final report.
- #1414 / #1415 interactions are *existing* issues, not new findings;
  the impl chunk cross-links them.

## 8. Decision ledger

| # | Question | Decision | Alternative rejected |
|---|---|---|---|
| D1 | What becomes a token | All 14 colour literals (9 Theme + 5 Semantic); geometry/typography stay invariant | Tokenizing widths alongside (out of round; #1361's ledger) |
| D2 | Where palettes live | Rust `Palette` data, pushed into the Theme global; .slint keeps dark defaults + parity test | Slint-side conditionals (breaks extractor); Slint struct table |
| D3 | Switch surface | Settings-view row (scale grammar) + `[ui] theme`/env (O-5 parity, both lanes one chunk) | GUI-only (violates O-5); config-file-only (violates O-5's preferred state) |
| D4 | Apply semantics | Live push, no restart | Restart-to-apply |
| D5 | Persistence | `[ui] theme = "dark"`, serde default dark, toml_edit writeback, env override | New `[theme]` table (no precedent); lazy writeback on quit |
| D6 | File-driven themes | Deferred to backlog; Palette struct pre-shapes the format | Building the loader now (unjustified subsystem) |
| D7 | Light goldens | Representative subset (~6–8), manifest-recorded asymmetry | Full corpus doubling |

**Gate**: this design is operator-gated. C1-impl may be dispatched only
after acceptance, together with the mandatory UX journey plan
(`2026-11-25-theme-switch-ux-journey-plan.md`, from the template) —
journey first, then dispatch chunking per docs/ux_planning.org.

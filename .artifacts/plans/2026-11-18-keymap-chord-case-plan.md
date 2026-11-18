# [Plan] Keymap Chord Case Law (Round-9 Chunk K1)

- **Date**: 2026-11-18
- **Kind**: TDD implementation plan — the chorded-letter case-seam fix
  (binding-identity normalization, shift-bit preservation, preset
  collision sweep + re-home, re-spelling convention, seam tests).
- **Origin**: J3 UX journey plan
  `.artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md` ruling **R-6**
  + story **US-3** (both ACCEPTED, normative); round plan
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §1
  **O-8**; research
  `.artifacts/reports/2026-11-18-round9-keyboard-focus-research.md` §5
  finding 5; #855 (comment 6979361380).
- **Branch**: `fix/ui-keymap-chord-case` (this worktree).
- **Status**: PLANNED (chunk K1; K2 must not start until this lands —
  same seam).
- **Scope**: `crates/taskboard-ui-slint` — `keymap/stroke.rs`,
  `keymap/mod.rs`, `keymap/presets.rs`, plus display-string updates in
  existing tests. No UI markup, no domain, no app crate.

## 1. The seam (what and why)

`parse("Ctrl+T")` mints `key: 'T'` (chorded printables keep their
case — the D6 shift-fold applies only to *bare* printables,
`stroke.rs:402-407`), but a real Ctrl+T press delivers Slint
`text: "t"`; `to_stroke` → `Stroke::from_event('t', ctrl=true,
shift=false)` (`stroke.rs:256-264`) keeps `'t'`. `'t' ≠ 'T'` → the
exact pass misses, and `chord_shift_partners` deliberately returns
nothing for letters (`stroke.rs:189-195`). So **neither Ctrl+T nor
Ctrl+Shift+T can ever resolve an uppercase-spelled chorded-letter
token**. The defect family: every uppercase-spelled chorded-letter row
in all three presets (Standard `Ctrl+Z/Q/K/F/J/T/G/R/L`, Standard and
emacs `Ctrl+J`/`Ctrl+T` Global rows, vi `Ctrl+T`, plus every
`Ctrl+Shift+M/P/R/S/B/N/H` row — the shift-row tokens mint `'M'` while
the event delivers `'m'` with `shift: true`, the same mismatch on the
glyph axis). The emacs motion rows (`Ctrl+x`, `Ctrl+n`, …) are
lowercase-spelled and unaffected — matching the operator's
emacs-centric report history.

**Why no test caught it**: journeys and keymap tests feed `parse()`
tokens straight into `resolve`/`KeymapResolver::feed`
(`journeys.rs` ~:5530 `parse("Ctrl+T")`; `mod.rs` tests likewise). The
parse∘resolve round trip is self-consistent by construction; the seam
lives only at the event boundary (`from_event`), which no keymap test
exercises for cased letters. The one `from_event`-based test
(`alt_shifted_punct_tokens_match_base_glyph_events`) covers the
punctuation twin and explicitly pins letters as partner-less — correct
for the punct map, silent on the case seam.

## 2. The law (R-6, normative)

- Ctrl+T ≡ Ctrl+Shift+t (one binding identity) — no modern OS can
  deliver Ctrl+uppercase without Shift, so the uppercase glyph *is*
  shift evidence for chorded letters.
- Ctrl+T and Ctrl+t are **distinct** bindings (shift bit preserved).
- Canonical spelling: **Ctrl+Shift+\<lowercase letter\>**.
- `parse()` accepts both spellings as the same binding identity.
- Consequence (this plan's settlement): Ctrl+Z ≡ Ctrl+Shift+z ≠ Ctrl+z.

## 3. Decisions

### D1 — Normalization point: the minting constructors, symmetrically

Normalize in **both** `parse` and `from_event` (the two trust
boundaries that mint `Stroke`s; house A1 pattern — fallible/total
minters, no validate-and-forget downstream). Not a resolver-level
candidate pass (`chord_shift_partners` extension): R-6 says *binding
identity* — equality at the `Stroke` level — so the exact-match pass
resolves with zero resolver changes and `has_prefix` stays exact.

Rule (chorded = `ctrl || alt || meta`, key is an ASCII letter — extend
to any single-scalar-`to_lowercase` letter; multi-scalar letters keep
the character unchanged, mirroring `fold_shift`'s conservatism):

```text
key   = to_lowercase(key)            (single-scalar only)
shift = shift_flag || key_was_uppercase
```

Bare printables, named keys, digits, and the punctuation table are
**untouched**. `chord_shift_partners` stays byte-identical (letters
still yield no candidates — after this fix they need none: the shift
case-mismatch is healed at minting).

Display canonicalization falls out for free: `Display` renders mods in
Ctrl, Alt, Shift, Meta order plus the lowercased letter, so the undo
binding displays `Ctrl+Shift+z` — the canonical spelling per R-6, and
the kbd-hint/menu projections (D19) update automatically since they are
projections of keymap data.

### D2 — Shift-bit preservation table (the identity law, tested)

| Press (physical event)           | Old token mint       | New identity        |
|----------------------------------|----------------------|---------------------|
| Ctrl+Shift+t (`'t'`, shift=T)    | `('t', shift=T)`     | `('t', shift=T)`    |
| Ctrl+T spelled token             | `('T', shift=F)` ← broken | `('t', shift=T)` ≡ above |
| Ctrl+t spelled token / bare ctrl+t press | `('t', shift=F)` | `('t', shift=F)` — distinct |
| Ctrl+Shift+z press (`'z'`, T)    | `('z', T)`           | `('z', T)`          |
| Ctrl+Z spelled token             | `('Z', F)`           | `('z', T)` ≡ redo's old identity — **the voided pair, see D3** |
| Ctrl+z token/press               | `('z', F)`           | `('z', F)` — distinct |

### D3 — Collision sweep over presets.rs and the re-home table

Sweep method (stage-2 executes it as a test, not by eye): under the
new identity, re-run the intra-context conflict check (T7's predicate)
over all three presets' parsed rows. Predicted result, verified
against the current tables:

- **One voided pair**: Standard Global `AppUndo = "Ctrl+Z"` and
  `AppRedo = "Ctrl+Shift+Z"` collapse to the same identity
  `('z', shift=T)`. Physically forced: an undo press (`'z'`,
  shift=F) and a redo press (`'Z'`, shift=T) normalize to the same
  identity under R-6 — the pair cannot survive as spelled.
  **Re-home**: `AppRedo → "Ctrl+Y"` in Standard Global (the
  Windows/GTK conventional redo; free — `Ctrl+y` appears in presets
  only inside the emacs `Ctrl+c Ctrl+y` family, a different preset and
  a two-stroke sequence). Undo keeps its chord under the canonical
  spelling `Ctrl+Shift+z`.
- Emacs `AppRedo = Ctrl+g Ctrl+/`, vi `AppRedo = "Ctrl+r"`: unaffected
  (lowercase-spelled; no uppercase twin rows).
- All `Ctrl+Shift+<letter>` rows: after re-spelling (D4) they are
  canonical; no lowercase-glyph twin rows exist in the same context
  (`Ctrl+m`, `Ctrl+p` etc. are absent from the affected contexts, and
  emacs's lowercase motion chords live in Global while the
  shift-rows concerned are Today/Board/TaskDetail/FocusMode contexts
  or non-colliding letters — swept and clean).
- Vi-shape contract (`vi_preset_shape_matches_the_mode_model`): vi's
  single Global row re-spells (`Ctrl+T` → `Ctrl+Shift+t`); the
  len==1 and `ViewToday` assertions survive unchanged.

| Preset | Action | Old spelling(s) | New spelling |
|---|---|---|---|
| Standard | AppRedo | `Ctrl+Shift+Z` | **`Ctrl+Y`** (re-home) |
| Standard | AppUndo | `Ctrl+Z` | `Ctrl+Shift+z` |
| Standard | AppExit | `Ctrl+Q` | `Ctrl+Shift+q` |
| Standard | CaptureQuick | `Ctrl+K` | `Ctrl+Shift+k` |
| Standard | PaletteSearch | `Ctrl+F` | `Ctrl+Shift+f` |
| Standard | FocusModeToggle (Global + FocusMode) | `Ctrl+J` | `Ctrl+Shift+j` |
| Standard | ViewToday | `Ctrl+T` | `Ctrl+Shift+t` (the headline: US-3/S1) |
| Standard | BoardSelect | `Ctrl+G` | `Ctrl+Shift+g` |
| Standard | SeriesEdit | `Ctrl+R` | `Ctrl+Shift+r` |
| Standard | LabelManage (TaskDetail + Board) | `Ctrl+L` | `Ctrl+Shift+l` |
| Standard | AppMenu / ReviewOpen / AppSettings(seq) / ReminderMute / BoardCreate / HabitCreate | `Ctrl+Shift+M/R/S/B/N/H` | lowercase-letter form (`Ctrl+Shift+m` …) — canonical spelling, not a semantic change |
| Emacs | FocusModeToggle, ViewToday (Global) | `Ctrl+J`, `Ctrl+T` | `Ctrl+Shift+j`, `Ctrl+Shift+t` |
| Emacs | LabelManage (TaskDetail + Board), HabitCreate | `Ctrl+L`, `Ctrl+Shift+H` | lowercase form |
| Vi | ViewToday (Global) | `Ctrl+T` | `Ctrl+Shift+t` |
| Vi | HabitCreate (Today) | `Ctrl+Shift+H` | `Ctrl+Shift+h` |

Note: under the new minting, `parse("Ctrl+Shift+M")` and
`parse("Ctrl+Shift+m")` are *already* the same identity — the
uppercase-glyph shift rows would keep resolving even unrespelled —
but AC-3 demands the canonical spelling in the tables, so all rows
re-spell regardless.

### D4 — Re-spelling convention

Canonical form for any chorded uppercase-letter binding:
`Ctrl[+Alt|+Meta]+Shift+<single lowercase ascii letter>`. Enforced by
a fold test (§5 T4), not by convention alone.

## 4. TDD order (each step: red → green → commit)

1. **T1 (red) — identity seam tests in `stroke.rs`**:
   `parse("Ctrl+T") == parse("Ctrl+Shift+t")`;
   `parse("Ctrl+T") != parse("Ctrl+t")`;
   `from_event('t', true, false, false, false) == parse("Ctrl+Shift+t")`
   (the real Ctrl+T press);
   `from_event('T', true, false, true, false) == parse("Ctrl+Shift+t")`;
   `parse("Ctrl+Z") == parse("Ctrl+Shift+z") != parse("Ctrl+z")`;
   `from_event('z', true, false, false, false) == parse("Ctrl+Shift+z")`;
   bare printables and named keys unchanged (existing tests already
   pin those — they must stay green).
2. **T2 (red) — the voided-pair sweep**: extend the intra-context
   conflict test to run over *post-normalization* identities and add
   the explicit assertion that the Standard undo/redo pair collapsed
   (documents the forced re-home).
3. **T3 — presets**: re-home AppRedo → `Ctrl+Y`; re-spell all rows per
   the D3 table (green T2).
4. **T4 (red→green) — canonical-spelling fold test**: every preset
   chorded-letter spelling matches the canonical form (D4); parse
   still accepts the uppercase alias (US-3 AC-3).
5. **T5 — the `from_event` delivery contract (US-3 AC-2, the
   regression bar)**: for **every preset**, for **every row** spelled
   with a chorded letter, synthesize the two real deliveries (lower
   glyph + shift, upper glyph + shift — plus shift-off for lowercase
   rows) via `from_event` and assert `resolve` returns that row's
   action; and assert a shift-off lowercase delivery does **not**
   resolve a shift-bearing binding (the Ctrl+t ≠ Ctrl+T half).
   Structural assertions on `ActionId`s only (AGENTS §5 — no text).
6. **T6 — repair the display-dependent tests**: the `Ctrl+Q` hint
   expectation (`first_binding_display_skips_unbound_chain_contexts`),
   the undo/redo display assertions in
   `shift_folds_for_bare_printables…` and
   `normalization_is_additive_and_exact_match_wins` (its "letters have
   no partner" arm stays — the punct map is untouched), and the 13
   journey `parse("Ctrl+X")` call sites only where the *displayed*
   spelling is asserted (resolve-path call sites keep working — both
   spellings mint the same identity). Proptest
   `parse_display_parse_is_idempotent` must pass unchanged (canonical
   display reparses identically under the new minting).
7. Full local gate for the touched crate:
   `scripts/run-quality-gates.sh cargo nextest run -p taskboard-ui-slint`
   (dispatcher owns the full §10 suite per dispatch discipline).

## 5. Acceptance (US-3)

- AC-1: T1 identity + delivery assertions green.
- AC-2: T5 contract test over all three presets green.
- AC-3: T4 canonical-spelling fold test green.
- Journey US-3/S1/S2 ride T5's resolve-level assertions; the full
  window-level journey test (`keymap_case_seams.rs::us3_s1` per J3 W-1)
  is K2-adjacent — if the journeys harness mount is cheap, land
  `us3_s1` here feeding the adapter's `to_stroke` with a real
  `KeyboardModifiers` shape; otherwise note the deferral on #855 and
  in the handoff.

## 6. Risks

- **Hint-text churn**: menus/tooltips show `Ctrl+Shift+z`-style hints
  after this lands; they are projections (D19), so no hand-written
  string changes — but any snapshot or doc that embeds old hint text
  needs a sweep (`grep -rn "Ctrl+[A-Z]"` outside keymap/).
- **User configs**: an override spelled `Ctrl+T` now mints the same
  identity as the preset's `Ctrl+Shift+t` row — override-vs-preset
  conflict detection (place_override) compares identities, so the
  user's intent ("replace the today jump") is *better* served than
  before (previously it silently never matched). Documented in the
  plan; no migration needed.
- **Non-ASCII letters**: single-scalar `to_lowercase` only
  (`İ`, `ẛ` keep their character) — deliberate leniency, documented at
  the minting site, mirroring `fold_shift`.
- **The undo/redo re-home is user-visible** (Ctrl+Shift+Z → Ctrl+Y in
  Standard): forced by R-6's physics; called out in the PR body and
  CHANGELOG entry for the operator to veto before merge.

## 7. Estimated diff

~300–400 lines: presets.rs ~30 row edits; stroke.rs ~25 production
lines + ~60 test lines; mod.rs contract tests ~90 lines; journeys/
display-test updates ~40 lines.

# [Plan] UI contrast fixes (dark theme, extended pairs) — chunk plan (#1450, #1458)

- **Date**: 2026-11-28
- **Kind**: chunk plan — one branch (`fix/ui-contrast-pairs`), one PR,
  two backlog items executed as one small chunk (both are remediations
  of the same token table in the same crate; #1458's fix is a
  precondition discovery of #1450's fix space — see D2). Three-stage
  execution: this planning stage, then implementation, then review
  (prompts below are paste-ready and verbatim-binding).
- **Origin**: issues #1450 (dark extended-pair waivers: border on
  raised/hover below 3:1, danger on hover below 4.5:1) and #1458
  (tone-fill chip text pairs below 4.5:1); context #1414 (the extended
  pair gate), #1415/#1410 (chip-ink rule L4, the `ChipInk` machinery),
  theme design `.artifacts/plans/2026-11-25-theme-support-design.md`
  (§2.2/§2.3 normative), journey plan
  `.artifacts/plans/2026-11-27-theme-switch-ux-plan.md` (the theme
  surfaces' journeys; §346: "the automated floors are the contract").
  All on `main` at `21f0a52b`.
- **Status**: READY — no gates, no dependencies; dispatchable now.
  Deliberately **not** in this chunk: the runtime contrast warning for
  user-chosen label colours (**#1445** — see D4: it needs its own UX
  journey plan and stays a separate follow-up; commented on the issue
  at stage 1).

## 1. Recorded reality (file:line at origin/main, `21f0a52b`)

### The pair gate as it stands

`crates/taskboard-ui-slint/src/token_tests.rs` is the contrast gate: a
pure-Rust extractor reads the colour literals out of
`crates/taskboard-ui-slint/ui/theme.slint` (the boot-time dark
defaults), and `every_pair_meets_its_role_class_minimum` (token_tests.rs:521)
runs the `PAIRS` table (token_tests.rs:195) over **both** palettes —
the `.slint` dark defaults (`PaletteTag::SlintDefaults`) and
`Palette::LIGHT` (`crates/taskboard-ui-slint/src/palette.rs:100`).
WCAG maths live in the one production definition
`contrast_ratio` (`crates/taskboard-ui-slint/src/presentation.rs:915`,
`#[cfg(test)]` consumers only).

Three dark legs are **waived debt** (`MinRatio::Waived`,
token_tests.rs:114), pinned at their measured floors by the tripwire
`every_waived_pair_is_still_below_its_owed_minimum` (token_tests.rs:561);
under light the waiver never applies — the owed class floor is
enforced directly (`effective_minimum`, token_tests.rs:293). The gate
is green at HEAD **because of the waivers**, not because the pairs
pass. Recomputed at stage 1 (WCAG 2.x, `contrast_ratio`'s exact
maths):

| pair (foreground × surface) | dark measured | dark floor owed | light measured | gate state at HEAD |
|---|---|---|---|---|
| `Theme.border` × `surface-raised` | **2.75:1** | NonText 3.0 | 4.23:1 pass | dark waived at 2.74 (token_tests.rs:252) |
| `Theme.border` × `surface-hover` | **2.34:1** | NonText 3.0 | 3.85:1 pass | dark waived at 2.33 (token_tests.rs:261) |
| `Semantic.danger` × `surface-hover` | **4.19:1** | Body 4.5 | 5.32:1 pass | dark waived at 4.18 (token_tests.rs:223) |

Token provenance of each failing leg:

- `Theme.border` — dark literal `#646c85`
  (`crates/taskboard-ui-slint/ui/theme.slint:54`, mirrored verbatim by
  `Palette::DARK.border = 0x646c85`,
  `crates/taskboard-ui-slint/src/palette.rs:82`); light
  `Palette::LIGHT.border = 0x6b7280` (palette.rs:107) already passes.
- `Semantic.danger` — dark literal `#e8756f`
  (`ui/theme.slint:237`, mirrored `Palette::DARK.danger`,
  palette.rs:87); light `0xa03a32` (palette.rs:112) already passes.
- Consuming markup (from #1450's survey, verified present):
  `ui/widgets/tooltip.slint:134-136`, `ui/compositions/labels-overlay.slint:107`,
  `ui/compositions/timer-setter.slint:86`, `ui/compositions/track-entries.slint:128`,
  `ui/compositions/action-menu.slint:292` (border on raised/hover),
  `ui/compositions/status-bar.slint:375`, `ui/views/review-view.slint:693`
  (danger text over hover surfaces).

### The chip fill pairs (#1458) — invisible to the gate today

`Semantic.tone-fill(t)` (`ui/theme.slint:265-267`) derives
`tone-color(t).with-alpha(0.2)`; the 8-digit alpha form is invisible to
the extractor by construction, so **no pair entry exists** for chip
text over its own fill. Consumers:
`ui/widgets/badge.slint:26-28` (fill) + `:43` (text =
`tone-color(t)`), `ui/widgets/action-chip.slint:104` (text over a
chip-background from `tone-fill`), `ui/views/habit-history-view.slint:121-138`
(day-cell fill + `tone-color` text). Measured at stage 1 (fill =
straight sRGB composite of tone at 0.2 over the surface; chip text is
badge-role body text, 4.5 floor):

| tone text over own fill | dark: surface / raised / hover | light: surface / raised / hover |
|---|---|---|
| danger | **4.11 / 3.64 / 3.17 FAIL** | 4.61 pass / **4.33 / 3.98 FAIL** |
| sync-active | 4.51 pass / **3.98 / 3.44 FAIL** | **4.22 / 3.98 / 3.66 FAIL** |
| success | 5.90 / 5.20 / 4.51 pass | **4.37 / 4.08 / 3.76 FAIL** |
| warning | 6.04 / 5.34 / 4.63 pass | **4.29 / 4.04 / 3.71 FAIL** |
| overdue | 5.36 / 4.75 / 4.12 pass (hover leg below 4.5) | 4.96 / 4.67 / **4.29 FAIL** |
| offline (= text-secondary) | 4.82 / 4.25 / 3.68 | 4.32 / 4.06 / 3.75 |

Two facts drive the decisions:

1. **On light the class fails broadly** — every tone fails at least
   one surface leg (blending toward the light surface raises the
   fill's luminance against the dark inks). This was flagged, unmeasured,
   in #1458's C1-impl cross-link; measured now and posted to the issue.
2. **Raising the tone tokens cannot fix the chip class**: the fill is
   derived from the same tone colour, so lightening dark `danger`
   raises the fill with it — dark danger never reaches 4.5 over its own
   0.2 fill at any lightening up to +40% luminance. Lowering
   `tone-fill`'s alpha toward 0 approaches the flat pair (which
   #1450's danger fix makes pass) but erases the chip visual. The fix
   must therefore **decouple the chip ink from the tone colour**.

### The goldens

`crates/taskboard-ui-slint/tests/goldens/` holds 73 PNGs (dark corpus
plus `-light` variants), compared by
`tests/journeys/golden_visuals.rs` behind
`tests/support/golden_compare.rs` (scale-aware cell grids, luma+chroma,
budget 8/255 — never widened to admit a failure). Regen is an explicit,
designed-to-fail step: `TASKBOARD_REGEN_GOLDENS=1` (golden_visuals.rs:80-91).
A dark token change **will** change dark goldens; a chip-ink change
changes both corpora wherever badges/chips/habit day-cells appear
(board, today, kiosk-*, task-view, review-ledger, settings, …).

## 2. Decisions

### D1 — #1450, `border`: raise the dark token's luminance (option a of #1450)

Change **only the dark palette**: `Palette::DARK.border`
`0x646c85` → **`0x7b8297`** (same hue family, one step lighter) and the
mirrored literal at `ui/theme.slint:54`. Measured effect on dark:
background 4.72, surface 4.25, raised 3.74, hover **3.19** — every
border pair clears NonText 3.0 with ≥ 0.19 margin. Light is untouched
(already 3.85 on its worst surface). A distinct control-boundary token
(option b) was rejected: the *pair* is what owes the floor, and every
surface sits darker than any qualifying border, so a split token would
need the same luminance wherever it renders on raised/hover — two
names for one number, with consumers forced to know their surface.

### D2 — #1450, `danger`: raise the dark token's luminance

`Palette::DARK.danger` `0xe8756f` → **`0xeb8680`** (and
`ui/theme.slint:237`). Measured on dark: background 7.11, surface
6.40, raised 5.63, hover **4.80** — clears Body 4.5 with ≥ 0.30
margin; every other dark danger pair only improves. Light untouched.
(No markup renders text over solid danger as a background — verified
by grep, so raising danger breaks no inverse pair.)

### D3 — #1458, chip ink: decouple to `Theme.text-primary`, fill unchanged

The chip text ink stops being the tone colour and becomes the theme's
primary text colour, which measures over **every** tone fill (0.2 over
every surface) in **both** palettes: dark worst 6.54:1, light worst
7.53:1 — a guarantee with real margin, from tokens that already exist.
The pair the tokens never meant to pair (tone text × own translucent
fill) is exactly the design-token discipline's recorded exception —
the consumer composes it, so the consumer-level change is the
token-conformant fix. Concretely:

- `ui/widgets/badge.slint:43` — `text-color` → `Theme.text-primary`
  (keep the fill at `tone-fill`), with a comment naming the chip pair
  rule (§1's table) and this plan.
- `ui/widgets/action-chip.slint:104` — same substitution.
- `ui/views/habit-history-view.slint:134` — same substitution.
- `tone-color(t)` itself stays: list-row, status-notice, icon-button,
  dropdown, status-dot, confirm-strip all render tone text over flat
  surfaces (gated flat pairs) — untouched.
- The neutral/info badge chips already ride `surface-raised` as their
  fill with `tone-color` text — those are the gated flat pairs
  (`text-secondary`/`text-primary` × `surface-raised`), already
  enforced; not chip-fill pairs, not touched.

Rejected alternatives, recorded: per-tone dedicated chip-ink tokens
(works — dark inks lighter than the tone fail as shown in §1, so it
converges to "near-text-primary per tone" without a payoff over one
shared token); `tone-fill` alpha reduction (erases the chip visual and
still depends on D2 for the danger hover leg). **This is the chunk's
look-changing ruling** — see §7 Rulings.

### D4 — the gate gets STRICTER-or-equal (never loosened)

- The three `MinRatio::Waived` entries (token_tests.rs:223, 252, 261)
  are **deleted**: the legs flip to their real class floors
  (`danger×hover` Body, `border×raised`/`border×hover` NonText). Per
  the waiver's own law ("the waiver is deleted once #1450 lands; the
  real class minimum flips on"). With no waived pairs left, the
  `Owed` enum, `debt_issue`, `effective_minimum`'s waiver arm and the
  tripwire test `every_waived_pair_is_still_below_its_owed_minimum`
  become dead code — remove them (clippy `-D warnings` enforces).
- **New computed chip-pair gate** in `token_tests.rs` (the issue's
  proposed direction): a `CHIP_FILL_PAIRS` const enumerating every
  chip ink × tone-fill × surface leg — after D3 the ink is
  `Theme.text-primary` for all tones, but the table stays explicit
  per tone (`danger`, `warning`, `success`, `overdue`, `sync-active`,
  `offline`) × surface (`surface`, `surface-raised`, `surface-hover`)
  so a future fill or tone change cannot silently shrink the class.
  The test computes the 0.2 straight-sRGB composite in Rust
  (documenting the blend model in a comment — slint composites
  straight alpha over 8-bit sRGB), then asserts
  `contrast_ratio(ink, fill) >= 4.5` over both palettes. Completeness:
  every `Tone` the markup vocabulary names must appear — assert the
  tone list against the tones declared in `theme.slint`'s `Tone` enum
  so a new tone fails the gate until paired.
- Net: at HEAD the waivers *loosen* three legs; after this chunk every
  leg runs at its real floor and the chip class — ungated today — is
  gated. Strictly stricter.

### D5 — UX-plan decision: #1450/#1458 ride without a journey plan; #1445 does not ride

The trigger rule (AGENTS §7; `docs/ux_planning.org` §1): a plan that
"adds or changes a **user-visible capability** — any new verb, badge,
view, or setting a human can perceive or act on" needs an accepted UX
journey plan; "pure engine/storage/sync internals with no surface
delta are exempt". These two items add no verb, badge, view, or
setting: the same badges, chips, borders and surfaces render the same
content at token values that now clear the accessibility contract the
theme design already accepted (design §2.2: "the WCAG contrast floors
are the contract"; the theme-switch journey plan §346 defers taste to
those automated floors). This is contract *restoration* on existing
surfaces — a bug-fix class remediation, not a capability delta — and
the acceptance criteria to satisfy are the design doc's own floors, no
journey amendment needed. **#1445 is the opposite case**: a runtime
contrast warning is a new warning surface a human perceives and acts
on — a new capability — so it requires an accepted UX journey plan
before a dispatch plan may chunk it. It does **not** ride this chunk;
stage 1 commented the disposition on the issue. The build-time chip
pair gate from D4 is the machinery #1445's runtime check will reuse.

### D6 — golden regeneration and per-golden justification

Procedure (stage 2, after the token + markup changes land):

1. `scripts/memory-gate.sh env TASKBOARD_REGEN_GOLDENS=1 cargo nextest
   run -p taskboard-ui-slint --test journey_suites` — backgrounded
   (heavy), and it **fails by design** after rewriting (the regen
   marker); the run is never mistaken for a pass.
2. The regen rewrites the dark-corpus PNGs that show borders, danger
   text, or chips, and both corpora where chips render; `-light`
   goldens change only via D3 (chip ink) since D1/D2 touch dark only.
   `tests/goldens/manifest.toml` entries are upserted by the harness.
3. **Every changed PNG is reviewed in the PR diff** (GitHub renders
   images): each must show only the expected delta — lighter dark
   borders (D1), lighter dark danger text (D2), chip text now
   text-primary over an unchanged tinted fill (D3) — and no layout,
   geometry, or unrelated colour drift. A golden whose diff shows
   anything else is a finding, fixed on the branch; budgets are never
   widened (`golden_compare.rs` law) and goldens are never deleted to
   make a compare pass.

## 3. Invariants (stage 2 must hold, stage 3 must verify)

- The pair gate gets **strictly stricter**: the three waiver entries
  are deleted (real floors flip on) and the computed chip-pair gate
  lands; no `PAIRS`/`CHIP_FILL_PAIRS` entry is removed or its class
  lowered. A pair that fails is fixed in the tokens, never removed
  from the gate.
- No currently passing pair regresses in either palette (the gate
  runs every pair × both palettes and must exit green).
- No consumer-level colour literals: `Palette::DARK` and the
  `theme.slint` literals change together (the mirror-parity test
  `the_slint_defaults_mirror_the_dark_palette` enforces byte-equality);
  `Palette::LIGHT` is untouched by D1/D2; markup picks tokens, never
  hex.
- `#![forbid(unsafe_code)]` holds; the UI crate stays a presentation
  client (AGENTS §2) — this chunk touches only token data, theme
  functions/literals, and markup colour bindings; no business logic,
  no IO, no new dependencies.
- AGENTS §5: no text assertions, no quality ceilings (the gate asserts
  the real WCAG floors, which is a floor contract, not a ceiling);
  test additions live at the lowest layer (`token_tests.rs`) and do
  not duplicate through journeys.
- Golden budgets untouched; regen runs are explicit and fail-by-design.

## 4. Test catalogue (lowest layer)

- `token_tests.rs`: delete the three `Waived` entries → the legs carry
  `MinRatio::Body`/`MinRatio::NonText` (existing
  `every_pair_meets_its_role_class_minimum` now enforces the real
  floors — no new test needed for D1/D2, the gate *is* the test).
- `token_tests.rs`: new `every_chip_fill_pair_meets_body_minimum`
  over the new `CHIP_FILL_PAIRS` const (D4), both palettes, with the
  tone-completeness assertion (every declared `Tone` appears in the
  table). Pure unit test; no new scaffolding module (SonarCloud <3%
  duplicated new code — the WCAG maths is reused from
  `presentation::contrast_ratio`, the blend is ~6 lines).
- `token_tests.rs`: the waiver tripwire + `Owed`/`debt_issue`/
  `effective_minimum` waiver arm removed with their last consumer
  (dead-code law).
- No journey/probe test changes: `the_preset_chips_hold_their_token_floor`
  and `p2_the_hover_lift_pair_reads_on_both_surfaces` read tokens, not
  literals — they must stay green unchanged (a change there would
  signal a violated invariant).

## 5. Verification commands (expected outcomes)

```sh
# from the worktree; every cargo call through the memory gate
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint -E 'test(token_tests)'
#   → every token_tests test passes; in particular
#   every_pair_meets_its_role_class_minimum green with NO waiver legs,
#   every_chip_fill_pair_meets_body_minimum green over 6 tones × 3 surfaces × 2 palettes

scripts/memory-gate.sh cargo clippy -p taskboard-ui-slint --all-targets -- -D warnings
#   → clean (the removed waiver machinery produces no dead-code warnings)

cargo fmt --all -- --check
#   → clean

scripts/memory-gate.sh env TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint --test journey_suites
#   → rewrites PNGs + manifest, then FAILS with the regen marker (by design);
#     followed by a plain compare run:
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journey_suites golden
#   → green (goldens match the regenerated corpus)

git -C . diff --stat -- crates/taskboard-ui-slint/tests/goldens/
#   → only expected PNGs + manifest.toml; each reviewed per D6
```

The dispatcher owns the full §10 suite (including the coverage trio)
between stages, per the dispatch discipline.

## 6. Diff-shape rules

- Allowed to change: `crates/taskboard-ui-slint/src/palette.rs` (two
  `DARK` fields), `crates/taskboard-ui-slint/ui/theme.slint` (two
  literals), `crates/taskboard-ui-slint/src/token_tests.rs` (waiver
  removal + chip gate), `ui/widgets/badge.slint`,
  `ui/widgets/action-chip.slint`, `ui/views/habit-history-view.slint`
  (ink bindings + rule comments), `tests/goldens/*.png` +
  `tests/goldens/manifest.toml` (regen), this plan's status field.
- Nothing else: no new files, no new dependencies, no `Cargo.toml`
  edits, no changes to `Palette::LIGHT`, `tone-color`, `tone-fill`
  (beyond nothing — the alpha stays 0.2), or any non-chip consumer of
  tone text.
- Comments state constraints (the pair rule, the blend model, the
  waiver-deletion law), never narration.

## 7. Rulings requested

1. **The chip look change (D3)** — the only fix that keeps the 0.2
   fill and clears 4.5:1 everywhere is chip text in `text-primary`
   instead of the tone colour:
   before: badge/chip/day-cell text in the tone colour (e.g. dark
   `#e8756f` danger red on a faint red-tinted fill); after: text in
   the theme's primary text colour (dark `#e8eaf0`, light `#333333`)
   on the same faint tone tint — the tone remains legible from the
   fill, the text goes monochrome. **Default: proceed with D3.** The
   alternative (per-tone dedicated chip inks) converges to nearly the
   same look with more tokens; dropping the fill instead would erase
   the chip visual entirely.
2. **Dark border lightening (D1)** — every dark-theme border lightens
   one step: `#646c85` → `#7b8297` (visibly lighter blue-grey frames
   around raised panels/tooltips/cards). Any single border token that
   clears 3:1 on `surface-hover` needs roughly this luminance; the
   alternative (a second boundary token used only on raised/hover
   consumers) was rejected in D1 as two names for one number.
   **Default: proceed with D1.**
3. **Dark danger lightening (D2)** — dark danger text lightens
   `#e8756f` → `#eb8680` (coral, slightly paler). Needed for the 4.5:1
   hover leg; also lifts danger on raised 4.92 → 5.63. **Default:
   proceed with D2.**

## 8. Prompt 2 (stage 2 — IMPLEMENTATION) — paste-ready, verbatim

You are the stage-2 IMPLEMENTATION agent for the chunk "UI contrast fixes (dark theme, extended pairs)" (branch fix/ui-contrast-pairs) in the taskboard repo (GitHub: D4id4los/taskboard), working in /home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-ui-contrast-pairs. Recreate the worktree on that branch if missing (git -C /home/viktoria/src/taskboard worktree add ../taskboard-WT-fix-ui-contrast-pairs fix/ui-contrast-pairs — or, per AGENTS §8, re-branch off fetched origin/main if the branch's PR already landed; never git worktree add <path> main). Work ONLY by absolute paths inside that worktree; do not touch /home/viktoria/src/taskboard or any other taskboard-WT-* directory.

**Long-command discipline (binding, restate-verbatim law)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp).

Your authority is the accepted chunk plan `.artifacts/plans/2026-11-28-ui-contrast-pairs-plan.md` in the worktree (read it first; its D1–D6, invariants, diff-shape rules and test catalogue are binding). Read AGENTS.md before writing code.

Implement exactly:

1. **D1** — `crates/taskboard-ui-slint/src/palette.rs`: `Palette::DARK.border` 0x646c85 → 0x7b8297, and the mirrored literal at `crates/taskboard-ui-slint/ui/theme.slint:54` `#646c85` → `#7b8297`. The mirror-parity test `the_slint_defaults_mirror_the_dark_palette` must stay green.
2. **D2** — same two files: `Palette::DARK.danger` 0xe8756f → 0xeb8680 and `ui/theme.slint:237` `#e8756f` → `#eb8680`.
3. **D3** — chip text ink becomes `Theme.text-primary` in exactly three consumers, fill unchanged: `ui/widgets/badge.slint` (the `tone-color()` helper feeding `text-color` at the text primitive — return `Theme.text-primary`), `ui/widgets/action-chip.slint:104`, `ui/views/habit-history-view.slint:134`. Each carries a comment stating the constraint (chip text × own 0.2-alpha tone-fill ≥ 4.5:1 in both palettes; cite the plan). Do NOT touch `Semantic.tone-color`/`tone-fill`, list-row, status-notice, icon-button, dropdown, status-dot, or confirm-strip.
4. **D4** — `crates/taskboard-ui-slint/src/token_tests.rs`: (a) delete the three `MinRatio::Waived` entries (#1450 legs) so the pairs carry their real `MinRatio::Body`/`MinRatio::NonText` floors; (b) remove the now-dead waiver machinery (`Owed`, `debt_issue`, the waiver arm of `effective_minimum`/`minimum`/`label`, and the tripwire test `every_waived_pair_is_still_below_its_owed_minimum`) — clippy `-D warnings` is the enforcer; (c) add `CHIP_FILL_PAIRS` (every declared tone: success, warning, danger, overdue, sync-active, offline × surfaces surface, surface-raised, surface-hover) and a test `every_chip_fill_pair_meets_body_minimum` that computes the 0.2 straight-sRGB composite (ink 20% over surface 80%, per-channel, rounding — document the blend model in a comment) and asserts `contrast_ratio(ink, fill) >= 4.5` over both palettes, with a completeness assertion that every `Tone` declared in theme.slint's `Tone` enum appears in the table. Reuse `crate::presentation::contrast_ratio`; no new scaffolding module, no duplicated WCAG maths (SonarCloud <3% duplicated new code).
5. **D6** — regenerate goldens: `scripts/memory-gate.sh env TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint --test journey_suites` (backgrounded; it rewrites `tests/goldens/*.png` + `manifest.toml` and then FAILS by design — that is the regen marker, not a bug). Then run the plain compare: `scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journey_suites golden` — must be green. Review `git diff --stat` over the goldens: every changed PNG must show only the expected delta (lighter dark borders, lighter dark danger text, chip text now primary-ink over unchanged tinted fills); any other drift is a finding to fix on the branch — never widen `golden_compare.rs` budgets, never delete a golden.

Gate (in order, all through `scripts/memory-gate.sh`): `cargo fmt --all` then the pre-commit hooks carry clippy; run `scripts/run-quality-gates.sh cargo nextest run --workspace` backgrounded and poll — commit all landed work BEFORE any wait. If the gate lock is contended and no useful work remains, commit and report "gate deferred: lock held by <holder>" and end the stage. Commit message: `fix(ui-slint): clear the dark contrast waivers and gate the chip fill pairs (#1450, #1458)` with a body naming D1–D4 and the golden regen. Do NOT push, do NOT open the PR — that is stage 3's job. Do not remove the worktree.

Report: the commit SHA, the final failing-pairs table as measured (all pairs green, no waivers), the list of changed goldens with one-line justification each, and any deviation from the plan (there should be none; deviations require a reported ruling, not a silent edit).

## 9. Prompt 3 (stage 3 — REVIEW) — paste-ready, verbatim

You are the stage-3 REVIEW agent for the chunk "UI contrast fixes (dark theme, extended pairs)" (branch fix/ui-contrast-pairs) in the taskboard repo (GitHub: D4id4los/taskboard), working in /home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-ui-contrast-pairs. Recreate the worktree on that branch if missing (git -C /home/viktoria/src/taskboard worktree add ../taskboard-WT-fix-ui-contrast-pairs fix/ui-contrast-pairs — or re-branch off fetched origin/main per AGENTS §8 if a PR already landed). Work ONLY by absolute paths inside that worktree.

**Long-command discipline (binding, restate-verbatim law)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp).

**Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client; (c) a single level of abstraction per function body. Findings are fixed on the branch.

Review against the accepted chunk plan `.artifacts/plans/2026-11-28-ui-contrast-pairs-plan.md` (D1–D6, §3 invariants, §6 diff-shape rules are binding) and AGENTS.md. Verify specifically:

1. **The gate is strictly stricter, never loosened**: the three #1450 waiver entries are gone with the pairs at their real floors (`Semantic.danger × surface-hover` Body 4.5, `Theme.border × surface-raised`/`× surface-hover` NonText 3.0); no `PAIRS` entry removed or class lowered; the new `every_chip_fill_pair_meets_body_minimum` gates all six tones × three surfaces × both palettes with the Tone-completeness assertion; the dead waiver machinery is fully removed. A failing pair is fixed in the tokens — if any pair was removed from the gate to make this pass, that is a hard blocking finding.
2. **Tests that must not have been weakened**: `every_pair_meets_its_role_class_minimum`, `every_waived_pair_is_still_below_its_owed_minimum` (deleted only because its subject is gone — verify nothing else weakened it first), `the_slint_defaults_mirror_the_dark_palette`, `hover_token_is_distinct_from_its_parents`, `the_preset_chips_hold_their_token_floor`, `p2_the_hover_lift_pair_reads_on_both_surfaces` — all present and green, none loosened.
3. **Token discipline**: only `Palette::DARK.border`/`danger` and the two mirrored theme.slint literals changed colour; `Palette::LIGHT` untouched; the three chip consumers bind `Theme.text-primary` (no hex literals, no new tokens, `tone-fill` alpha still 0.2); mirror-parity test green.
4. **Goldens**: regen explicit, compare green, budgets untouched (`MAX_CELL_DIFF`/`MAX_CHROMA_CELL_DIFF` still 8), manifest bijects with the corpus, and every changed PNG's diff shows only the expected delta (§D6) — spot-check several via `git diff --stat` and the PR preview.
5. **Scope/diff-shape**: nothing outside §6's allowed file list; no new dependencies; `#![forbid(unsafe_code)]` intact; comments state constraints; no speculative generality in the chip-pair test (no scaffolding module).
6. Run: `scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint -E 'test(token_tests)'` and the golden compare (both green); `cargo fmt --all -- --check` clean; clippy via the pre-commit hooks. If `scripts/run-quality-gates.sh cargo nextest run --workspace` is contended and no useful work remains, apply the flock-deferral rule.

Then ship — your scope ENDS at PR-open: commit any findings fixed on the branch (`fix(ui-slint): <finding>` style), push `fix/ui-contrast-pairs` (`git push --force-with-lease origin fix/ui-contrast-pairs` if the branch already had a remote), open the PR to main with `gh pr create` (title `fix(ui-slint): clear the dark contrast waivers and gate the chip fill pairs`, body: closes #1450, closes #1458; summary of D1–D4, the measured before/after pair table, and the golden-regen note so the PNG diff is reviewable) — the PR body closes #1450 and #1458 at merge (the dispatcher owns the closures). Do NOT run gh pr checks --watch, do NOT verify mergeStateStatus, do NOT remove the worktree — the dispatcher owns the CI watch, the CLEAN verification, teardown, and at-merge issue closures. Report the PR URL and your review findings.

## 10. Completion bookkeeping

- Dispatcher, between stage 2 and stage 3: full AGENTS §10 suite via
  `scripts/run-quality-gates.sh` (the coverage trio included).
- At merge (dispatcher): close #1450 and #1458 via the PR body;
  #1445 stays open as its own follow-up (needs a UX journey plan
  first — see D5 and the stage-1 comment on the issue).
- Worktree teardown after merge-green per AGENTS §8 (dispatcher).

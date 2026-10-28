# [Plan] UI R3 Chunk 4 — Settings Polish: validity flavour, eye toggle, InfoTip width, status placement, scale trio

- **Date**: 2026-10-26 (stage-1 plan written 2026-10-28)
- **Branch**: `feat/ui-r3-settings-polish` (worktree
  `../taskboard-WT-feat-ui-r3-settings-polish`, cut from `origin/main`
  @ `5cf6f02`; precondition verified — chunk 3 `feat/ui-r3-taskview-dates`
  merged (remote branch deleted post-merge, its commits on `origin/main`:
  `dd73019`…`722fa43`/`7380bfa`), chunks 1–2 on `origin/main` before it).
- **Normative inputs**: design round
  `2026-10-26-gui-review-round3-plan.md` §3.4 (D16–D20) + §1 findings
  3/19/20/21/23/24/25 + §4 + §7-R2/R3; dispatch plan §1/§5 (chunk 4,
  acceptance a–g). Non-goals (dispatch §5): no connection logic, no
  migration (chunk 5 — `LegacyConfig` stays untouched here), **no
  status-text changes** (`settings_status::status_text` mapping
  unchanged). Acceptance includes an **empty `taskboard-app` diff**
  and empty `taskboard-domain` diff.
- **All `file:line` anchors are current-branch** (verified on
  `feat/ui-r3-settings-polish` @ `5cf6f02`; the dispatch prompt cited
  pre-chunk-1/2/3 numbers, which drifted in `adapter.rs` but not in
  the settings slint files). Slint pinned **1.18.1**
  (`Cargo.toml:44`); the two gesture findings below cite the pinned
  `i-slint-core-1.18.1` sources from the local cargo registry.

## 0. TDD order (binding for stage 2)

1. **Validators first** (§1): `settings_form.rs` per-field predicates +
   `FieldStatus` + `scale_status` + `format_scale`, each red→green at
   the unit layer. The existing ladder test
   (`the_ladder_reports_the_first_failing_field`,
   `settings_form.rs:182`) must pass **unmodified** after the refactor
   — it pins that `validate()` and the per-field predicates are one
   source of truth (review focus (a) by construction).
2. Then the probe (§6) — it decides the slider fix before any slider
   code moves. The probe tests run against today's code and **P1 must
   reproduce the steal**; the P3 fix-assertion test fails until the
   Flickable property lands.
3. Then validity flavour (§1 slint half), eye (§2), InfoTip width (§3),
   status placement (§4), scale echo + anchoring (§5) — each red→green.
4. Full AGENTS §10 gate suite green; CHANGELOG Unreleased updated;
   zero Slint deprecation warnings (chunk-2 sweep must not regress).

## 1. D19 — the validity flavour API

### 1.1 States

Tri-state, per field. Markup enum in `ui/views/settings-view.slint`
(the `SettingsOutcome` export precedent, `settings-view.slint:28`),
mirrored Rust-side with an exhaustive match:

```slint
export enum FieldValidity { neutral, invalid, valid }
```

- **neutral** — the field's text is empty (or, for the password after a
  send, the VM draft is empty). No border, no error line. The user's
  acceptance wording: "neutral when empty".
- **invalid** — non-empty and the per-field predicate fails. Danger
  border + error line, focused or not.
- **valid** — non-empty and the predicate passes. Success border, no
  error line.

### 1.2 Border tokens (`ui/widgets/text-field.slint`)

`TextField` gains `in property <FieldValidity> validity` (default
`neutral`; the enum imported from settings-view would be a layering
inversion — declare the enum in `text-field.slint` instead, next to the
widget that owns the flavour, and let settings-view re-export the
import; stage 2 picks one home and keeps a single declaration). The
border binding (`text-field.slint:67-68`) becomes:

- `validity == invalid` → `border-width: Theme.focus-ring-width`,
  `border-color: Semantic.danger` (always shown — instant feedback does
  not wait for focus; finding 21 "active border red when invalid").
- `validity == valid` → same width, `Semantic.success`.
- `neutral` → today's contract verbatim (focus ? ring-width : 0,
  `Semantic.focus-ring`). Focus never changes the colour of an
  invalid/valid border.

Both tokens already exist (`theme.slint:109,111`). No new colours.

### 1.3 Error-line slot — at the row wrapper, not inside the primitive

Deliberate deviation from D19's letter ("TextField gains … an inline
error line below the field"): the line mounts in `LabeledField`
(`settings-view.slint:174-211`), which restructures from a bare
`HorizontalLayout` into `VerticalLayout { HorizontalLayout {…},
if error != "": ErrorLine {} }`. Reasons, recorded: (i) the line must
span label + InfoTip + field, not the field cell alone; (ii) the
primitive's height contract is pinned (r2 D22 layout-cell citizenship,
`text-field.slint:74-79`; the multiline growth binding is a recorded
Slint hazard `:69-72`) and must not grow per keystroke.

`ErrorLine` is a small markup component in settings-view.slint:
hover-box styling (surface-raised background, `Theme.border` border,
`radius-small`), a `Typography.caption` `RoleText` in
`Semantic.danger`, `horizontal-stretch: 1`. The **text is presentation
copy pushed by the adapter** — never asserted by tests (§7-R3).

`LabeledField` gains `in property <FieldValidity> validity` +
`in property <string> error-text`, forwarded to the TextField and the
line. The plain scale TextField (not a LabeledField) gets its own
error-line instance beside it (same component, same contract).

### 1.4 Per-field validators from the settings_form ladder
(`src/settings_form.rs`)

Refactor, zero behaviour change to `validate()`:

```rust
pub enum FieldStatus { Neutral, Valid, Invalid(InvalidKind) }
pub fn field_invalid(field: Field, draft: &ConnectionDraft) -> Option<InvalidKind>
pub fn field_status(field: Field, draft: &ConnectionDraft) -> FieldStatus
```

- `field_invalid` is the per-field predicate: `Name` → empty→`Empty`,
  else kebab class; `ServerUrl` → `url_host_ok` → `BadUrl`;
  `Username`/`Password` → trimmed-empty → `Empty`.
- `validate()` is **recomposed from** `field_invalid` in the existing
  ladder order (name → username → password → url). One source of truth;
  the Save gate (`draft_valid`, `adapter.rs:1565`) and the flavour
  cannot diverge.
- `field_status` is the flavour mapping: empty → `Neutral`,
  `Some(kind)` → `Invalid(kind)`, `None` → `Valid`.

New scale validator beside them (the dispatch: "validators live beside
settings_form, pure, unit-tested"):

```rust
pub fn scale_invalid_kind(text: &str) -> Option<InvalidKind>  // trimmed-empty → None
pub fn scale_status(text: &str) -> FieldStatus
pub fn format_scale(v: f64) -> String
```

- Parity with the authoritative mint `UiScale::new`
  (`src/scale.rs:42-50`): parse-fail → `InvalidKind::NotKebab`,
  non-finite → `NotFinite`, outside `0.5..=3.0` → `OutOfRange`,
  else valid. The NotKebab reuse is deliberate: the lane lattice
  (`requests::InvalidKind`) already reserves the arm and
  `settings_status::invalid_text` already carries its sentence
  (`settings_status.rs:122`) — no enum extension, no protocol change.
- `format_scale`: Rust `{}` Display is shortest-round-trip; append
  ".0" when the rendering carries no fraction (1 → "1.0",
  2.25 → "2.25", 0.5 → "0.5"). The echo must never lie about the
  applied value (the field path accepts non-grid values like 2.25 —
  `UiScale` enforces only the window, not the 0.1 grid).

### 1.5 When validation runs

Per keystroke, at the existing render pass: `settings_field_edited`
already mirrors + `push_settings` per keystroke (`adapter.rs:1966-1993`);
`push_settings` additionally computes each field's `FieldStatus` from
the VM draft and the per-field error string, and pushes them. The error
string for a draft field is
`settings_status::invalid_text(form_field, kind)` — the same sentence
the status line would render (zero new presentation code; the
"chunk-5 mapping" comment at `adapter.rs:2032` anticipated this).

Password nuance: arm 3 mirrors into `draft.password` per keystroke and
clears it on send (`adapter.rs:1971-1978`, `:2070`); validity is
computed from the **draft**, so the field never shows a stale red
state after a successful save (field text survives, draft is empty →
neutral). Redaction unchanged: the password still never lands on any
pushed model property (D26b pin).

The Save gate is untouched: `save_enabled` stays
`pending.is_none() && draft_valid(&draft)` — the flavour is display,
the ladder is the gate.

## 2. D16 — the eye toggle anatomy

All inside `ui/widgets/text-field.slint`:

- Markup-local state: `in-out property <bool> revealed: false;` — no
  adapter involvement, no model property (nothing secret-adjacent may
  leave the field). Lifetime: `SettingsView` is `if`-mounted
  (`app-window.slint:290`), so `revealed` resets when the form
  unmounts — exactly "persists while the form lives" (D16).
- `input-type` (`text-field.slint:133`):
  `root.secret && !root.revealed ? InputType.password : InputType.text`.
- Trailing eye affordance: `if (root.secret): eye-button := Rectangle`
  in the established affordance pattern (TouchArea-only, no tab stop —
  the recorded Slint negative "a TouchArea click never moves focus"
  (`text-field.slint:50-56`) is the feature here: toggling never steals
  focus from the input mid-typing). Square `Theme.space-6`, hover token,
  `AccessibleRole.button`, a11y label "Reveal"/"Conceal" by state
  (inline strings — the "Commit"/"Clear" precedent, `:156,194`),
  `accessible-action-default` fires the same toggle (click ≡ AT).
- Glyphs: two geometric `Path`s (closed eye / open eye — the D19
  no-icon-font vocabulary), stroke tokens, swapped on `revealed`.
- Slot arithmetic: the eye owns the rightmost slot
  (`x: parent.width - self.width - Theme.space-1`); the focus-gated
  commit/clear affordances (`:148,186`) shift left by
  `Theme.space-6 + Theme.space-1` when `root.secret` — the password
  field carries `commit-affordance: true`
  (`settings-view.slint:460-472`), so the two must coexist without
  overlap.
- **Redaction pins untouched** (review focus (b)):
  `accessible-value` stays `root.secret ? "" : root.text`
  (`:94`) — revealed or not, the AT surface keeps the write-only
  contract. No log, no model, no Debug carries the value (the draft
  mirror + clear-on-send flow is unchanged). Journeys pin the
  emptiness oracle before and after a toggle.
- Copy: the placeholder "write-only; never shown"
  (`settings-view.slint:462`) is now false advertising — becomes a
  markup-owned string that no longer claims never-shown (exact wording
  stage 2; markup text, untested per the no-text-assertions rule).

## 3. D17 — InfoTip/tooltip width mode

**Decision: the help text stays in the description slot; the *width
binding* gains the description term** (D17's "width-from-description
mode" alternative). Rejected: feeding `help` as the caption term — the
caption is the label/subject slot (`tooltip.slint:53-56` guards
non-empty label semantics); duplication and contract breakage.

`ui/widgets/tooltip.slint`:

- The description `RoleText` (`:140-146`) stops being a conditional
  element: always mounted, `visible: TooltipState.description != ""` —
  the `hint-row` precedent verbatim (`:152-155`; a conditional element
  cannot be referenced outside its branch).
- The width binding (`:95-97`) gains the term:

```slint
width: min(max(caption.preferred-width,
               description-text.preferred-width,
               hint-natural()) + 2 * Theme.space-2,
           min(root.window-width * 0.6, 420px * Theme.base-scale));
```

- Mechanism note: probe P5 (the wrap-probe comment, `:89-94`)
  established a wrapped `RoleText`'s preferred width is the *unwrapped*
  natural width — so a long help sentence drives the panel to the
  60 %-window / 420·scale cap and wraps inside it. `info-tip.slint`
  is **unchanged** (`:57-65` already passes `about, help`).
- Blast radius: every tooltip with a description widens (menu rows,
  IconButtons). That is the generalized mechanism behaving as designed;
  `src/tooltip.rs` placement already consumes the realized size and
  clamps into the window. Existing hand-off pins
  (`usability_components.rs:237`) assert placement-inside-window and
  parts presence, not caption-derived width — expected green; if any
  pin turns out to encode caption-only width, it pinned today's
  limitation → recorded-decision rewrite citing this §.

## 4. D18 — status line placement (layout shape)

Move the status `HorizontalLayout` (`settings-view.slint:573-589`) out
of `content` (the Flickable's VerticalLayout, `:288`) into the outer
VerticalLayout **between the pinned header and `flick`**:

```
VerticalLayout
├── header (back + "Settings")        stretch 0, pinned   (:242-257)
├── status line (conditional)         stretch 0, pinned   ← moved here
└── flick := Flickable                stretch 1           (:263)
    └── content (…form…)              (status row REMOVED from here)
```

Contract kept verbatim: `visible: last-outcome != none`, tone dot
(`StatusDot`), elided mapped text, `min-height == max-height` extent
pin, `vertical-stretch: 0` (the F-C3 dead-mechanism comment moves with
the row). Layouts skip invisible children (the `hint-row` fact), so a
hidden status leaves the viewport identical to today; a visible one
shrinks the viewport — which is the fix (finding 20: the user thought
Save was dead because the status scrolled away). Give the row an
element id (`status-line`) for the journeys' structural lookup.

`SettingsScrollFacts` mirrors follow automatically (`changed height`
fires when the status appears/disappears). The r2 scroll journey
(`settings_scroll_reaches_the_scale_controls_at_scale_3`,
`journeys.rs:3116`) runs with no outcome set → unaffected.

## 5. D20 — the scale trio

### 5.1 (a) The echo property + writer points

Model: `SettingsModel.scale-echo: string` (settings-view.slint global);
the scale TextField binds `text <=> SettingsModel.scale-echo` (today it
has **no** text binding at all — only a placeholder, `:555-565`; that
is the sync bug, finding 23). VM state:
`SettingsVm.scale_draft: Option<String>` — the field's staged buffer.

The rendered echo is **draft-when-present, live-scale-otherwise**;
`push_settings` (`adapter.rs:1884`) writes
`echo = scale_draft.clone().unwrap_or_else(|| format_scale(live))` on
every pass. Writer points, enumerated:

| # | Writer | Site | Writes |
|---|---|---|---|
| 1 | mount/refresh | `push_settings` | draft ∨ `format_scale(Theme.base_scale)` |
| 2 | slider preview | `settings_scale_preview` (`:2331`) | `Theme.base_scale = v` **and**, only when `scale_draft.is_none()`, echo = `format_scale(v)` (direct global write — preview deliberately pushes nothing, the one-request-per-gesture proof `:2326-2330` must hold) |
| 3 | slider release | `settings_scale_released` (`:2347`) | success → draft untouched (none), push renders committed; gate-fail revert → `Theme` back to `previous` (`:2352-2355`) + push → echo follows live |
| 4 | lane Err reply | drain `PendingReply::Scale` (`:1751-1764`) | `Theme` revert + `set_settings_outcome` → push → echo follows reverted live |
| 5 | field Return | `settings_scale_committed` (`:2300`) | success → `scale_draft = None`, apply-commit, push → canonical echo; parse/gate fail → **draft kept** (the typo stays visible under the invalid flavour + error line; nothing clobbers it) |
| 6 | field edited (NEW) | `settings_field_edited` arm 4 (`:1966`) | `scale_draft = Some(text)`; insert-mode engage; push (echo == draft — no visible change, validity recomputes) |
| 7 | focus loss (NEW) | scale field's `focus-lost()` → `settings_scale_focus_lost` | `scale_draft = None`; push → echo re-syncs to live. A Return-less edit never applies; the chunk-1 seam (`text-field.slint:57-63`) is the mechanism |

Draft hygiene (W-SM-c buffer invariants): the draft dies at settings
mount (cleared where `SettingsVm` seeds its mount state — the
`preset`-seeded-at-mount precedent, `adapter.rs:1557-1560`), on commit,
and on focus loss. It cannot outlive its surface.

Both directions of finding 23: slider → field via points 2/3;
field → slider because `Slider.value: Theme.base-scale` is one-way
model truth (`slider.slint:5-7`, `settings-view.slint:544`) — a field
commit moves the thumb with zero extra code. Journeys pin both.

### 5.2 (b) The slider-grab probe — mechanism verified, probe design

**Mechanism (verified against the pinned `i-slint-core-1.18.1`
`items/flickable.rs`, local registry source):**

- A left-press inside a Flickable is **held back from children** for
  `FORWARD_DELAY = 100 ms` (`flickable.rs:386`, `DelayForwarding`,
  `:728`).
- Within `DURATION_THRESHOLD = 500 ms` of the press (`:384`), any
  `Moved` whose delta exceeds `DISTANCE_THRESHOLD = 8 logical px`
  (`:382`) on a pannable axis is **intercepted** (`:740-748`) — the
  Flickable takes the gesture and the child TouchArea never sees it.
  The settings Flickable pans vertically (content overflows), so a
  slider drag with ≥8 px of vertical wobble inside 500 ms is stolen.
- The user's "instant preview refresh kills the gesture" is the
  coincidence: the first preview tick lands at the same event-turn the
  intercept fires. The re-layout is *not* the required trigger — the
  probe falsifies this explicitly (P2).
- Corollary found while reading: the built-in out-of-bounds clamp
  (`ensure_in_bound`, `:86-112`) explicitly skips a `content-y` that
  **has a binding** — our two-way journeys binding
  (`settings-view.slint:272`) disables it, which is *why* downward
  scale changes leave the blank screen (finding 25). §5.3's anchor
  must do the clamping itself; it does.

**Probe**: `tests/slider_grab_probe.slint` +
`slider_grab_probe_tests.rs`, the `focus_seam_probe` shape exactly
(`build.rs:37-45` registration, OUT_DIR include, testing backend,
`support::driver::run` + `dispatch_event` pointer synthesis +
`advance_probe_clock` — no sleeps, deterministic). Host: Window →
Flickable (content taller than viewport via filler rows) → the **real
`Slider`**; globals count `pressed/previews/committed` and mirror
flick `content-y`.

- **P1 reproduce**: press thumb centre → moves accumulating ≥ 8 px
  vertical within < 500 ms → release. Assert the steal signature
  (previews ≪ move count / zero commits while the flick pans). This
  test is green **before** any fix — the recorded reproduction.
- **P2 isolate the re-layout**: identical drag with the preview
  callback writing a probe counter instead of `Theme.base-scale`. If
  the steal persists (expected), the preview re-layout is exonerated —
  recorded in the test comment.
- **P3 fix A (preferred)**: `mouse-drag-pan-enabled: false` on the
  probe Flickable → same drag yields the full preview run + exactly
  one commit; a post-fix flick-drag on the filler area pans *not at
  all* via mouse drag (and the probe pins wheel/`content-y` semantics
  unchanged). Basis: `accepts_pan_event`
  (`flickable.rs:247-259`) gates mouse-drag panning on this property
  while **touch panning stays alive** (`is_from_touch()` arm) and
  wheel scrolling ignores both properties (builtin doc,
  `i-slint-compiler-1.18.1/builtin_elements.rs:1587`) — the kiosk
  touch scroll and desktop wheel scroll are untouched; only the
  mouse-drag-pan gesture (the thief) dies.
- **P4 fix B (only if P3 leaves residue)**: the while-pressed gate
  `mouse-drag-pan-enabled: !slider-pressed` (pressed state surfaced as
  a facts-global property). More moving parts; probe-decided.
- **Fallback (behaviour, last)**: preview-on-release — only if P3/P4
  both fail in the probe. Then preview stops writing `Theme.base_scale`
  mid-gesture (writes a shadow), release commits; the design records
  this as the user-approved fallback order. A `..._currently_pins_…`
  test names the debt.

**Production change on P3**: one property + comment on the settings
`flick` (`settings-view.slint:263`). The verdict (kept live preview vs
fallback) is pasted into the stage-2 report per dispatch §5.

### 5.3 (c) Scroll anchoring on the scale row

Facts extension (`ui/settings-facts.slint`):

```slint
in-out property <length> scale-row-y;      // markup → all: the scale row's content-space offset
in-out property <length> anchor-offset;    // adapter → markup: captured viewport offset at request time
in-out property <bool>   anchor-requested; // adapter → markup: one-shot flag
```

- The scale row (the HorizontalLayout at `settings-view.slint:524`, a
  direct child of `content`) mirrors
  `changed y => SettingsScrollFacts.scale-row-y = self.y` — `y` within
  the content is the layout offset, scroll-independent.
- **Capture** (adapter, at commit): `apply_scale_commit` (`:2275`) and
  both revert sites (release gate-fail `:2352`; drain Err `:1756`) call
  a tiny helper `request_scale_anchor(window)`: reads
  `scale-row-y + viewport-y` (both still **pre-layout** — the
  `Theme.base_scale` write above it re-lays out on the next pass) and
  stores `anchor-offset`, sets `anchor-requested = true`.
- **Restore** (markup, one-shot): the existing
  `changed content-height => mirror-facts()` hook (`:278-280`) gains:
  if `anchor-requested` →
  `viewport-y = clamp(anchor-offset - scale-row-y, min(0, viewport-height - content-height), 0)`,
  consume the flag. The new row offset is **measured**, not predicted —
  no scale-ratio math, exact for the uniformly scaled content
  (`Theme.space-*`/type all × `base-scale`, `theme.slint:52+`).
- Why the clamp matters: the built-in in-bounds put skips bound
  `content-y` (§5.2 corollary) — the downward blank screen is our
  binding suppressing it; the anchor's clamp is the replacement, and
  it holds for both directions (acceptance g: journey includes a
  downward change).
- Does not fight manual scrolling: the flag is consumed once per
  commit; no continuous correction exists (review focus (d) — journey
  drives `viewport-y` freely after an anchored commit).
- Revert re-anchors too (same helper on the revert path) — content
  height returns and the row stays where it was.

## 6. Test catalogue

**Unit — `settings_form.rs`** (pure, no slint):
`field_status` truth table per field (empty→Neutral; each kind→Invalid;
pass→Valid); the recomposition equivalence (ladder test green
unmodified + a direct `field_invalid`×`validate` agreement loop);
`scale_status` truth table with the `UiScale` parity fixtures
("0.5"/"3.0" valid; "abc"→NotKebab; "nan"/"inf"→NotFinite; "0.4"/"3.01"
→OutOfRange; ""/"   "→Neutral); `format_scale` truth table
(1→"1.0", 2.25→"2.25", 0.5→"0.5").

**Unit — `settings_status.rs`**: unchanged (the Scale lattice already
mapped; no new text).

**Widget probe — `text-field`** (if needed for the eye): covered by
journeys instead — the eye's structural pins ride the a11y tree:
the eye is discoverable as an accessible button beside the password
field; toggling changes **no** model property, keeps
`accessible-value` empty before/after (the emptiness oracle,
`text-field.slint:92-94`), and produces zero lane traffic. The
dots-vs-plaintext switch itself is Tier-4 visual (+ optional golden if
the harness makes it cheap — not required by acceptance).

**Journeys (`tests/journeys.rs`, settings section)**:
1. Validity per field (acceptance d — one journey, four field passes):
   type an invalid name → `validity-name == invalid` +
   `error-name` element exists (non-empty, content never asserted);
   fix it → valid + line gone; empty → neutral. Same grammar for
   server/username/password. Save enablement tracks the ladder
   throughout (existing gate test green).
2. Scale commit failure now carries the typed detail: "abc" Return →
   status Rejected **with** `Invalid{Field::Scale, NotKebab}` detail
   (the error line renders the same round); the existing
   `a_scale_parse_miss…` pins (`:2747-2840`) get the recorded-decision
   comment where they pinned detail-less Rejected.
3. Echo sync (acceptance e): mount echo "1.0"; slider previews move
   echo + thumb (`Theme.base_scale`); typed draft survives a
   concurrent preview (no clobber, point 2 gate); focus-lost discards;
   commit "2" → echo "2.0" + thumb at 2.0; Err reply → echo back.
4. Eye toggle (acceptance a): pins above; plus the placeholder/copy
   never asserted.
5. Status placement (acceptance c): at scale 3.0 in the min-floor
   window, trigger an outcome → the `status-line` element is present,
   positioned outside the Flickable's band (structural: its window y
   + height ≤ flick y), and the pinned header unchanged; hidden state
   leaves the viewport geometry as before (facts mirrors).
6. Scroll anchoring (acceptance g): scroll so the scale row is at a
   known viewport offset → commit scale **up** via the field → after
   settle, the row's viewport offset preserved (± row-height);
   commit **down** → `viewport-y` within bounds (no blank: ≥
   `-(content-height - viewport-height)`); manual `viewport-y` writes
   afterwards stick (no anchor interference).
7. Existing journeys that must stay green unchanged: the three r2
   slider gesture journeys (`:2949`, `:3010`, `:3059`) — the adapter
   seams are untouched by the fix (the probe's Flickable property is
   pure markup); the save/preset/bind/attach suite (`:2408+`).

**New probe tests**: `slider_grab_probe_tests.rs` P1–P3 (§5.2), P4
conditional.

**Goldens**: no new cases planned; existing board/today/kiosk goldens
unaffected (settings surface is not goldened today).

**Unchanged by contract**: `crates/taskboard-app/tests/ui_settings.rs`
(no seam it pins changes; `taskboard-app` diff stays **empty**),
`keymap_snapshots`, `pin_guard`, `virtualisation`.

## 7. Tier-4 (manual, this machine — listed for the stage-2 report)

1. **Drag feel**: mouse-drag the thumb 0.5→3.0 in one gesture — no
   tick-skips, no scroll hijack; wheel-scroll the form during/after;
   (touch device, if at hand) touch-pan still scrolls with
   `mouse-drag-pan-enabled: false`.
2. **Validation feel**: type through invalid → valid → empty on name;
   borders/error line appear per keystroke; the error line's content
   shift below feels acceptable (not jumpy) at 1.0 and 3.0.
3. **Status feel**: trigger a refusal at min window × 3.0 — the status
   is visible without scrolling; auto-clear at 5 s still lands.
4. **Eye feel**: toggle mid-typing — focus stays, keystrokes continue
   into the field; reveal survives blur, dies on settings close.
5. **Anchor feel**: both scale directions from a mid-form scroll — the
   slider row stays put; no fighting the wheel afterwards.

## 8. Risks

- **R2 (design §7) — the gesture probe could falsify the hypothesis**
  (e.g. the re-layout *is* required for the steal, or P3 leaves
  residual loss): the probe matrix P1/P2/P3 answers it before code
  moves; fallbacks are pre-ordered (P4 gate → preview-on-release) and
  the verdict is a stage-2 report item, not a plan rewrite.
- **R3 (design §7) — inline validation invites text assertions**: the
  error copy is pushed presentation text; every test pins
  `FieldValidity` + element presence/absence + non-emptiness only.
- **Tooltip widening blast radius**: all descriptive tooltips widen;
  hand-off/placement pins expected green (they pin placement, not
  caption width) — any pin that encoded caption-derived width gets the
  recorded-decision rewrite naming §3.
- **`changed content-height` coalescing**: if a commit's re-layout
  lands in the same event turn as the request (observed order
  unspecified), the anchor consumes on the first fire after the flag
  is set — the flag is set *before* the `Theme` write's layout pass,
  so ordering is safe; if a Slint quirk surfaces (no fire), the
  journey catches it and the fallback is anchoring in the adapter via
  a queued post-layout `slint::Timer` tick (mock-clock safe) — named
  here so stage 2 does not improvise.
- **Status row + min viewport**: a visible status consumes viewport
  height at kiosk min-window; the r2 extent pin (min==max, stretch 0)
  bounds it to one control row — the journey in §6.5 pins it.
- **Scope fences**: chunk 5 owns `LegacyConfig` removal — the
  `settings_status.rs` LegacyConfig arm and the app-side gates stay
  untouched here; `taskboard-app` diff must be empty (acceptance).

## 9. Estimated diff

| Area | Files | ≈ lines |
|---|---|---|
| Slint widgets | `text-field.slint`, `tooltip.slint` | +75 |
| Slint view/facts | `settings-view.slint`, `settings-facts.slint` | +110/−25 |
| Rust form | `settings_form.rs` | +95 |
| Rust adapter | `adapter.rs` | +85 |
| Probes | `slider_grab_probe.slint` + tests + `build.rs` | +170 |
| Journeys | `journeys.rs` | +230/−20 |
| CHANGELOG | `CHANGELOG.md` | +6 |
| **Total** | ~11 files | **≈ +770/−45** |

`taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
`taskboard-sync-nextcloud`, `taskboard-app`: **empty diffs by
acceptance**.

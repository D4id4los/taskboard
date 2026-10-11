# Emacs-flavoured Org Editing Backend Crate — Viability Report

- **Date**: 2026-10-11
- **Status**: Analysis only — no code, no decisions implemented
- **Question**: Assuming the org parser conceptualised in
  `.artifacts/reports/2026-10-11-org-mode-support-viability-report.md`
  (the `taskboard-org` two-layer, span-oriented parser), is it viable to
  build a specialised **emacs/org-style edit-backend crate** that owns the
  buffer, the org markup presentation, and the emacs interaction model
  (movement, SPC marking, kill-ring, yank, …), and exposes an interface
  complete enough that the per-GUI-framework widget (egui, iced, Slint)
  stays *thin*? Or do the frameworks differ so much that most of the work
  lands in framework-specific code anyway?
- **Method**: architecture analysis of the backend/adapter seam; ecosystem
  survey as of 2026-10-11 (egui 0.36.2, iced 0.14.0, Slint 1.18.1,
  parley 0.12, cosmic-text 0.19, ropey 1.6.1, floem-editor-core 0.2),
  Emacs Manual as the normative semantics source; all sources in §12.

---

## 1. Executive summary

**Verdict: viable, and worth building. This is not a pipedream — with one
non-negotiable discipline: the seam between backend and widget must be
drawn at logical text positions and abstract events, never at pixels.**
The backend speaks byte offsets, styled spans, and key chords; the widget
speaks glyph runs, rectangles, and raw platform events. Everything that
makes an emacs/org editor *hard and jank-prone* — the command semantics,
kill-ring append rules, mark/transient-mark behaviour, undo chaining,
universal-argument state machine, goal-column motion, org structure
editing, markup re-decoration after each edit — is 100 %
framework-independent. Everything that is irreducibly framework-specific
— text shaping and wrapping, caret/selection painting, IME candidate
windows, focus, clipboard — is *bounded, well-understood plumbing*, not
design risk.

Five findings dominate:

1. **The ecosystem gap is real: no reusable emacs-semantics editing core
   exists in Rust.** crates.io searches surface only line editors:
   `rustyline` has a genuine internal `KillRing` (append/prepend,
   yank-pop) but does not export it; `reedline` ships an `Emacs` edit
   mode with **no kill ring at all** (single cut buffer, `C-u` =
   cut-to-start, not universal argument). The closest architectural
   precedent for "editing core + GUI adapters" is
   **floem-editor-core 0.2** (MIT, published) — Lapce's engine, extracted
   exactly so the Floem editor widget and Lapce share it — but it is
   modal/multi-cursor (Kakoune-style), not emacs point/mark. **Helix's
   `helix-core` is not even published** (crates.io placeholder; git dep
   only; MPL-2.0). Your crate fills an empty niche.
2. **All three target frameworks sit on winit**, so the abstract input
   event enum is authored once and mapped trivially per framework; even
   the fiddly parts align (`Key {modifiers, repeat}`, IME events).
   Conversely, the three text stacks are genuinely different
   (egui: own `LayoutJob`/`Galley` + harfrust shaping; iced:
   cosmic-text; Slint: internal + `StyledText` since 1.16) — this is the
   one area that *cannot* be shared in v1 (§6 shows the shared-layout
   alternative and why to defer it).
3. **Slint cannot host live-decorated (WYSIWYG) editing today.**
   Rich-text *rendering* is done (`StyledText`, completed upstream in
   1.16), rich-text *editing* is still open (issue #2723, last activity
   2026-06-11), `StyledText` exposes no position/hit-testing/caret APIs,
   and **IME only works through real `TextInput` elements** — a
   fully custom `FocusScope` editor gets zero IME. The sane Slint adapter
   is therefore the same shape the org-mode report already recommends
   for taskboard: real `TextInput`/`TextEdit` for the editing surface
   (keeping IME/caret/selection for free) with emacs keys intercepted,
   plus a `StyledText` rendered preview. egui and iced have no such
   constraint and can host single-view decorated editors.
4. **Three design problems are genuinely hard and must be in the API from
   day one** (§3): visual motion feedback (C-n/C-p/C-v need wrapped-line
   geometry the backend doesn't have), folding/hidden markers as a
   *projection* (org TAB cycling breaks the 1:1 text↔view mapping), and
   IME composition ownership. All three have known solutions; none can
   be retrofitted cheaply — this is where the current janky iced
   implementation presumably cuts corners.
5. **The work splits roughly 60–75 % backend / 25–40 % adapters by line
   count** (backend ≈ 6–10 kLOC incl. tests; adapters ≈ 1–4 kLOC each),
   and *by design risk* the split is even more lopsided. A fourth
   framework adapter is routine work once three exist.

**Effort** (experienced Rust dev, AI-assisted, TDD, dev-day ≈ 6 focused
hours, same convention as the org-mode report): semantics conformance
spec + core crate ≈ 12–22 dev-days; org integration on top of the parser
≈ 5–10; first adapter (egui recommended, or harvest the iced one) ≈
4–8; each further adapter ≈ 3–6 (Slint source+preview) / 6–10 (Slint
single-view WYSIWYG, gated upstream). **Core + one adapter ≈ 3.5–6.5
weeks; core + all three adapters ≈ 6–11 weeks.** The alternative — a
janky one-off per GUI project, forever — is exactly what this replaces,
and the third copy would already cost more than the backend.

---

## 2. The architecture: what the backend owns vs what the widget owns

```
┌────────────────────────────────────────────────────────────────┐
│ orgedit-core (new, pure, no async/IO/UI deps, forbid unsafe)   │
│                                                                │
│  Buffer (ropey or String, field-sized inputs)                  │
│  Point / Mark / Region + transient-mark state machine          │
│  KillRing (shared app-wide) · MarkRing (per buffer)            │
│  Undo (emacs chained semantics)                                │
│  UniversalArgument state machine                               │
│  Keymap: abstract KeyEvent chords → Command (prefix state)     │
│  Command interpreter: movement/kill/yank/mark/org-structure    │
│  GoalColumn bookkeeping (desired-x for C-n/C-p)                │
│  MarkupProvider: parser spans → StyleSpan{range, face, invis}  │
│  RenderView: folding projection (segments + view↔buf mapping)  │
│  Ports: Clipboard · LayoutMap (frontend-supplied) · Viewport   │
│  Effects out: Redraw · ScrollTo(pos) · Status(msg) · Beep      │
├────────────────────────────────────────────────────────────────┤
│ taskboard-org parser (per the org-mode report: two-layer,      │
│ span-oriented `(kind, byte_range)`, tolerant, never errors)    │
└────────────────────────────────────────────────────────────────┘
            ▲ abstract events / offsets / spans      ▲ ports
┌───────────┴──────────────┬───────────┴─────────────┬───────────┐
│ orgedit-egui adapter     │ orgedit-iced adapter    │ orgedit-  │
│ event translation,       │ key_binding closure,    │ slint     │
│ Galley layout+painting,  │ text_editor/highlight   │ FocusScope│
│ IME rect, caret/selection│ _with or custom Widget, │ +TextInput│
│ rects, clipboard impl    │ Paragraph layout, IME   │ +Styled-  │
│                          │                         │ Text view │
└──────────────────────────┴─────────────────────────┴───────────┘
```

**Invariants that make the seam work** (violating any of these is where
jank comes from):

- **I1 — offsets, not pixels.** The backend never sees a coordinate.
  All geometry flows through a frontend-implemented `LayoutMap` (§3.1).
- **I2 — never remove characters, restyle them.** The rendered string is
  always byte-for-byte the buffer (egui's `LayoutJob` must byte-cover
  the text; hiding org markers is a *style* — invisible/dimmed — or a
  *projection* (§3.2), never a string edit).
- **I3 — edits flow one way.** Raw events in → `Effect`s out. The widget
  never mutates buffer state except through commands; there is no shared
  mutable cursor the widget also pokes. (This is exactly the taskboard
  "UI is a dumb client" rule, re-applied.)
- **I4 — composition (preedit) is frontend-owned overlay state**; only
  `Commit` becomes `insert_text` (§3.3).
- **I5 — the kill ring is app-scoped, not widget-scoped** (Emacs shares
  it across buffers; a `KillRing` handle passed to every session gets
  cross-field yank for free).

The abstract input vocabulary is small and stable — all three frameworks
run on winit, so their native events already agree on shape:

```rust
pub struct KeyEvent { pub key: Key, pub mods: Modifiers,
                      pub text: Option<String>, pub repeat: bool }
pub enum Key { Char(char), Enter, Backspace, Tab, Esc, Delete,
               Left, Right, Up, Down, Home, End, PageUp, PageDown, F(u8) }
pub struct Modifiers { pub shift: bool, pub ctrl: bool,
                       pub alt: bool, pub meta: bool }  // mac: ⌘→meta
```

(macOS convention — Slint already maps ⌘ onto `control` for emacs-style
apps; make the Ctrl/Meta swap a config flag.)

---

## 3. The three genuinely hard problems

### 3.1 Visual motion needs layout feedback (C-n, C-p, C-v, M-v, C-e)

Vertical movement targets *wrapped* lines and viewport pages — geometry
the backend does not have. Emacs solves this with **goal column**
(desired-x): vertical moves aim at the remembered column and clamp to
line end. The backend can therefore own the *policy* if the frontend
supplies the *geometry*:

```rust
pub trait LayoutMap {
    fn pos_to_visual(&self, pos: usize) -> VisualPos;   // (vline, x)
    fn visual_to_pos(&self, vline: usize, x: f32) -> usize; // clamped
    fn vline_extent(&self, vline: usize) -> Option<Range<usize>>;
    fn viewport(&self) -> Viewport; // visible vlines, height
}
```

Per key event: widget calls `session.handle_key(key, &map, &mut ctx)` with
the map built from the **last completed layout** (the standard retained-
geometry-for-input invariant every real editor uses; emacs itself computes
motion against current window state). `C-v`/`M-v` compute their page size
from `viewport()` minus `next-screen-context-lines` (2, as in emacs) and
emit `Effect::ScrollTo`. **This protocol is the single most important
thing to get right on day one** — retrofitting it after commands were
written against logical lines only is a rewrite of the movement layer.

### 3.2 Folding and hidden markers are a projection, not an edit

Org TAB cycling (fold/children/subtree) and hidden emphasis markers mean
the *view text* is no longer the buffer text, breaking naive
offset↔pixel mapping. Emacs's own answer (invisible text properties +
selective display) is the model: the backend exposes a **RenderView** —

```rust
pub struct Segment { pub source: Range<usize>, pub face: Face,
                     pub ellipsis_after: bool /* folded tail */ }
pub fn render_view(&self) -> &[Segment];
pub fn view_offset_to_buffer(&self, v: usize) -> usize;
pub fn buffer_offset_to_view(&self, b: usize) -> usize;
```

— and owns both mappings, so hit-testing and caret placement survive
folding. Within *unfolded* text, invariant I2 keeps the 1:1 mapping; the
projection layer only engages for folded subtrees. Hidden emphasis
markers (`*bold*` → bold with invisible asterisks) ride the same
mechanism as a degenerate segment style. Real-world constraint check:
egui `TextFormat` cannot zero-width a run — markers get a tiny-font or
dimmed style (polish, not MVP); iced `Highlight` formats are
color-centric (exact span power unverified — spike item); Slint sidesteps
it entirely in source+preview mode.

### 3.3 IME composition

Preedit text must render inline with an underline but must **not** enter
the buffer (point must not move) until commit. Frontend status, 2026-10:

| Framework | Composition events | Candidate window | Notes |
|---|---|---|---|
| egui ≥ 0.28 | `Event::Ime{Preedit,Commit,…}`, underline visuals since 0.35 | `ViewportCommand::IMERect` | best of the three |
| iced ≥ 0.14 | `Event::InputMethod{Preedit,Commit}` via `Widget::update` | widget-declared IME area | over-the-spot preedit only; on-the-spot is future work |
| Slint 1.18 | **only through real `TextInput` elements** | ditto | custom `FocusScope` editor = zero IME (platform IME API still open, #3811) |

Design consequence: the backend's contract is simply `insert_text(&str)`
on commit plus a preedit overlay the frontend renders from its own
state. The Slint column is the reason the Slint adapter keeps a real
`TextInput` in the loop (§5.3) rather than a pure custom widget.

---

## 4. Backend crate design (`orgedit-core`, working name)

**Shape**: standalone repo/crate (it outlives any one app), MIT OR
Apache-2.0, `#![forbid(unsafe_code)]`, zero async/IO/UI deps; `thiserror`
for the (tiny) error surface; `ropey` 1.6 for the buffer (mature,
char-index-native — right currency for point/grapheme semantics; 2.0 is
in beta and relicensed to MIT/Apache, pin 1.x stable. For *field-sized*
buffers a `String` would honestly do — ropey earns its keep the day an
adapter points at a whole `.org` file; see the org-sync synergy in §11).

```rust
pub struct Session { /* buffer, point, mark, rings, undo, keymap,
                        goal column, projection, markup provider */ }

impl Session {
    pub fn new(text: &str, cfg: Config) -> Self;
    pub fn handle_key(&mut self, key: KeyEvent,
                      map: &dyn LayoutMap, ctx: &mut dyn Ctx) -> Effect;
    pub fn handle_mouse(&mut self, ev: MouseEvent,
                        map: &dyn LayoutMap, ctx: &mut Ctx) -> Effect;
    pub fn insert_text(&mut self, text: &str) -> Effect; // IME commit
    pub fn text(&self) -> …;              // rope/slice access
    pub fn styled_spans(&self) -> &[StyleSpan]; // (range, face, invis)
    pub fn render_view(&self) -> &[Segment];    // folding projection
    pub fn point(&self) -> usize;  pub fn mark(&self) -> Option<usize>;
    pub fn regions(&self) -> …;    // active region for selection paint
    pub fn preedit_hint(&self);    // none — I4: frontend-owned
}

pub trait Ctx {                          // ports, frontend-implemented
    fn clipboard(&mut self) -> &mut dyn Clipboard;
    fn status(&mut self, msg: StatusMsg) -> …;   // echo area
    fn beep(&mut self) -> …;
}
pub enum Effect { Redraw { layout_changed: bool }, ScrollTo(usize), None }
```

**Semantics, sourced normatively from the Emacs Manual** (each bullet
becomes named conformance tests):

- *Region/mark*: region = text between point and mark regardless of
  order; `C-SPC` sets+activates; mark deactivates on `C-g` and after
  most region commands; transient-mark-mode on by default (highlight
  only while active). `C-x C-x` exchanges. Mark ring per buffer
  (default 16 deep; every set pushes; `C-u C-SPC` pops/cycles).
- *Kill ring*: shared app-wide, capped (configurable `kill-ring-max`).
  **Append rules** (the classic jank source): consecutive kill commands
  — only numeric arguments may intervene — form one entry; forward
  kills append to its end, backward kills prepend; `C-M-w` forces the
  next kill to append; a kill after `M-w` never appends. `C-w`/`M-w`
  also write the system clipboard via the `Clipboard` port (emacs
  `select-enable-clipboard` behaviour).
- *Yank*: `C-y` inserts the head, point at end, inactive mark at start;
  `C-u C-y` variant; `M-y` immediately after a yank swaps in the
  previous entry (wraps; numeric arg skips N); otherwise `M-y` is an
  error/beep.
- *Universal argument*: bare `C-u` ×4 (stacking), `M-<digit>`/`C-u
  <digits>` numerics, `M--`/`C-u -` negatives, second `C-u` terminates
  digits; a bare `C-u` is a per-command flag; applies to self-insert
  (`C-u 4 x` → `xxxx`) and to kill/yank/movement.
- *Goal column*: `C-x C-n` sets from point, `C-u C-x C-n` cancels;
  `C-n`/`C-p` aim at it via `LayoutMap`, clamp at buffer ends (emacs
  moves to end-of-buffer with a status message — reproduce that).
- *Undo*: one command (`C-/`, `C-_`, `C-x u`); consecutive undos chain,
  any other command breaks the chain; undos are themselves recorded
  (emacs-style "undo the undo"); region-active undo is region-local
  (defer to v2). An undo-*tree* is a documented possible later mode,
  not v1.
- *Case, rectangles, registers, isearch, keyboard macros*: explicitly
  out of v1 (macros are nearly free later — record `KeyEvent` sequences
  — call it out as the cheap v2 win; isearch needs minibuffer-like UI
  state, defer).

**Org commands** (consume the parser's element tree, per the org-mode
report's LayoutTree/visibility model): TAB visibility cycling +
S-TAB/global cycling (folds = projection state, §3.2), `M-RET` insert
same-level heading/item, `M-←/→` promote/demote, `M-↑/↓` move subtree,
checkbox toggle, electric `RET`/`TAB` in lists. These are why basing the
crate on the shared parser matters: markup faces, structure commands,
and (in taskboard) file sync all read the same spans.

**Testing** (mirrors the repo's existing strategy, and this crate being
pure is what makes it testable at all):

- Conformance suite: every manual rule above as named unit tests
  against `(text, key_sequence) → (text, point, mark, ring)` snapshots
  — insta-shaped, no UI anywhere.
- proptest invariants: kill-then-yank round-trips; append-rule state
  machine over random command sequences; undo∘command inverse over
  random edits; projection mapping is total and monotonic.
- Optional differential oracle: batch-mode Emacs in docker as the
  reference for `point` after command sequences (organic's method, the
  org-mode report's Tier-2 pattern) — start without it, add when a
  conformance dispute appears.
- The existing janky iced implementation: harvest it as an *executable
  spec* — port its behaviours as tests where they match Emacs, fix
  where they don't (do not enshrine the jank).

**Licensing note**: implement from the Emacs *Manual's described
behaviour* (spec), not from GNU Elisp source, keeping the crate
unambiguously clean-room MIT/Apache. The manual URLs are normative
enough for every rule above.

---

## 5. Per-framework adapter assessment

### 5.1 egui (0.36.2) — best host; recommended first adapter

Two routes, both proven:

- **Route A — `TextEdit` + custom `layouter`** (official pattern; the
  demo's `code_editor` does syntect highlighting this way): ~0.5–1.5 kLOC.
  Chords via `TextEdit::event_filter` (new in 0.36.2) +
  `InputState::consume_key`. **But** TextEdit's editing semantics are
  internal and not pluggable (`CursorRange::on_event`, state in
  `Memory`): kill-ring/mark/universal-argument end up as buffer surgery
  *around* its cursor model — the exact "fighting the framework" that
  produces jank. Layouter sections must byte-cover the text (I2 again).
  Good for a spike; wrong home for the real thing.
- **Route B — custom widget** (Ferrite, 1.8 k★, is the existence proof
  of a serious editor engine on egui: ropey, multi-cursor, IME/CJK,
  rebindable shortcuts, 80 MB files): build `LayoutJob` per frame from
  `styled_spans()`, `Painter::galley`, `Galley`'s cursor↔pos API for
  caret/selection rects (TextEdit uses exactly these), focus via
  `request_focus`/`memory`. ~2–5 kLOC with full control and no fights.

Known gaps: `TextFormat` has *italics* but **no bold/weight axis** —
register a bold TTF as a named family (or variation coords) once per
app; every-frame relayout cost needs a galley cache keyed on
`(buffer-version, width, fold-version)`. IME is the best of the three
(§3.3). AccessKit: role/label/value only, no caret/selection ranges —
acceptable for personal tooling, worth a note.

### 5.2 iced (0.14.0) — good host; `key_binding` was built for this

The stock `text_editor` widget has exactly the right hook:
`.key_binding(|KeyPress| -> Option<Binding>)` (since 0.13, PR #2522,
explicitly aimed at "custom key mappings … like Vim or Kakoune") — an
emacs prefix-chord state machine drops straight in, returning
`Binding::Custom`/`None`. Org decoration via `.highlight_with` custom
highlighter (0.14 API; **master refactored it to a `text::Parser`/
`Highlighter` shape — pin the version, expect churn**). Folding and
gutters don't exist in `text_editor` — either approximated via
programmatic `Content` edits (fragile) or by graduating to a custom
`advanced::Widget` over `Paragraph` (`with_spans`, `hit_test`,
`grapheme_span_position`, incremental `compare`) with `Tree` state,
`Clipboard`, and `Event::InputMethod` for IME — the iced docs even ship
an `advanced::text::Editor` trait of multi-line primitives. Estimates:
text_editor route 1.5–3 kLOC; full custom widget 5–10 kLOC worst case.
Caveats: **zero accessibility in stock iced** (comprehensive PR #3281
closed unmerged 2026-03); over-the-spot IME only.

### 5.3 Slint (1.18.1) — thinnest *editing* adapter, least decoration

Given §3.3 (IME only via real `TextInput`; `StyledText` has no geometry
APIs; rich-text editing open upstream), the architecture that works
*today*:

- **Editing surface = stock `TextEdit`/`TextInput`** (multi-line,
  wrap, selection styling, IME, `cursor-position-changed` for caret
  pixels). Intercept `key-pressed` (KeyEvent carries `text`,
  `modifiers`, `repeat`; Ctrl-x arrives as `\x18` — chord parsing is
  trivial), translate to backend commands, apply resulting edits to the
  text property, and sync the cursor back via `set-selection-offsets`
  (UTF-8 offsets, exactly the backend's currency). Mouse selection
  gestures likewise via `TouchArea`/`set-selection-offsets`.
- **Rendered org view = `StyledText::from_markdown`** fed by an
  org→markdown span escape pass (the org-mode report §3.3 already
  specifies this mapping and its escaping pitfalls), refreshed
  debounce-per-keystroke; TAB folding belongs to the rendered pane.
- Single-view WYSIWYG (custom `FocusScope` + own caret painting via
  `Path`, or the absolutely-positioned per-token `Text`-runs pattern of
  `lpmwfx/slint-codeeditor`, or CPU-raster via `SharedPixelBuffer`):
  buildable but **you inherit dual layout** (StyledText gives you no
  offset↔pixel mapping, so you'd run parley/cosmic-text yourself and
  keep it metrically consistent with Slint's renderer) **and lose IME**
  until #2723/#3811 move. One community rich-text editor project
  responded by bypassing Slint text for Skia/skparagraph entirely —
  telling. Gate single-view on upstream; the backend API needs zero
  changes either way (that's the point of I1/I2).

Estimates: source+preview adapter ≈ 1–2 kLOC; single-view custom ≈
3–5 kLOC + no IME. This matches the org-mode report's taskboard MVP
(rendered view + source-edit toggle) — same conclusion reached from the
editor-widget direction.

### 5.4 Adapter commonalities (why the third one is routine)

Every adapter is the same four jobs: (1) event table
`framework-key → KeyEvent` (~50 lines); (2) build native styled text
from `styled_spans()` + `render_view()`; (3) caret/selection/scroll
from the native layout API, implementing `LayoutMap` back for the
backend; (4) `Clipboard` port + IME plumbing. Framework majors churn
(iced 0.13→0.14 rewrote the highlighter seam) — that churn is exactly
what the seam quarantines: adapter breaks, semantics don't.

---

## 6. Alternative: backend owns layout too (defer)

parley 0.12 (2026-10-09) and cosmic-text 0.19 are both capable of being
the *shared* styled-layout engine: full style axes (weight, style,
underline/strikethrough, brushes; parley adds `editing::Cursor`
hit-tests and caret geometry; cosmic-text ships click detection and is
what iced already uses). A "backend does layout, frontends just paint
glyph runs" design would thin the adapters further and is the only route
to Slint single-view WYSIWYG with geometry. But: glyph rasterization and
GPU upload remain frontend-side regardless (swash/skrifa/vello pairings),
egui would have to consume foreign layouts it has no API for, and you'd
own font-stack consistency forever. **Recommendation: offsets-not-pixels
in v1 (§2), keep `StyleSpan` portable, adopt parley per-adapter only
where a framework forces it (Slint WYSIWYG).**

## 7. Prior art map

| Artefact | What it proves | Reuse? |
|---|---|---|
| **floem-editor-core** 0.2 (MIT, published) | the exact crate shape works: buffer/cursor/movement/selection/register extracted from Lapce, shared with the Floem editor widget | mine the design; semantics are modal/multi-cursor, not emacs |
| helix-core | selection/transaction/movement layering is frontend-agnostic | reference only: unpublished on crates.io, MPL-2.0, no emacs semantics |
| rustyline `KillRing` | correct append/prepend/yank-pop rules exist in Rust | internal, unexported — spec reference |
| reedline `Emacs` mode | chord parsing as a library | no kill ring; C-u repurposed; line-editor scoped |
| organice (React/Redux, AGPL) | full org *editor* product shape: round-trip-safe parser, agenda, sync | spec mine only (license); validates demand |
| xi-editor / Kakoune / Zed | core↔frontend splits at scale (JSON-RPC clients; server+remote UIs; crate zoo) | architecture precedent |

## 8. Effort & phasing

| Phase | Dev-days | Notes |
|---|---|---|
| 0. Semantics spec + conformance corpus from the Emacs manual | 2–4 | the rules in §4, as failing-first tests |
| 1. `orgedit-core` v1 (buffer, point/mark, movement + `LayoutMap` protocol, kill-ring, yank, mark-ring, undo, universal arg, keymap) | 10–18 | pure crate; the semantics are the product |
| 2. Org integration (`MarkupProvider` over the parser, projection/folding, org command set) | 5–10 | assumes the parser per the org-mode report (itself 15–25 d standalone, or orgize behind a wrapper) |
| 3. Adapter #1 — egui custom widget (harvest the iced jank as tests first) | 4–8 | Ferrite as reference |
| 4. Adapter #2 — iced `text_editor`+`key_binding` route | 3–6 | pin the iced version |
| 5. Adapter #3 — Slint source+preview | 3–6 | single-view gated upstream |
| **Core + one adapter** | **≈ 21–35 d (3.5–6.5 weeks)** | the personal-use deliverable |
| **Core + all three** | **≈ 27–48 d (6–11 weeks)** | |

## 9. Risks & mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Visual-motion protocol designed wrong (jank returns) | High | §3.1 in day-one API; conformance tests incl. wrap + goal-column cases |
| Slint WYSIWYG timeline (open #2723, IME #3811) | Medium (Slint only) | source+preview mode is the deliverable; backend unaffected (I1/I2) |
| iced highlighter API churn (0.14 vs master); egui parley migration someday | Medium | adapters are the quarantined layer; pin majors; Ferrite shows egui route B survives migration |
| egui lacks per-span weight | Low | register bold family once; variation coords exist |
| IME variance (iced over-the-spot; Slint custom = none) | Low–Med (personal use) | I4 keeps the backend clean; Slint adapter keeps a real TextInput |
| Accessibility absent (iced none; egui partial) | Low (personal) | documented non-goal v1 |
| Emulation drift vs real emacs | Medium | manual-derived conformance suite; optional docker-Emacs differential oracle; no Elisp-source reading (clean-room) |
| Scope creep (isearch, rectangles, registers, tables, images) | High (it's emacs…) | §4 v1 list is closed; everything else named-and-deferred |
| Solo maintenance across 3 framework majors | Medium | adapters are small and semantics centralised; this beats N janky forks regardless |

## 10. Recommendation

**Build it**, as a standalone MIT/Apache crate in its own repo (it will
outlive any single app; taskboard then consumes it — §11). Sequence per
§8; hard rules: draw the seam at offsets/events (I1–I5), write the
emacs-manual conformance corpus *before* the command interpreter, and
harvest the existing iced implementation as tests, not as code. Expect
the egui adapter to be the most capable host today, iced a close second
with the purpose-built `key_binding` hook, and Slint a first-class
*source+preview* citizen until upstream rich-text editing lands.

## 11. Taskboard tie-in

If the org-mode report's `taskboard-org` parser is built, `orgedit-core`
is its natural second consumer (faces, folding, structure commands over
the same spans — and the org-file sync connector makes whole-file
editing the eventual buffer size, justifying ropey). The taskboard Slint
editor fields should then be a thin `orgedit-slint` widget instead of
bespoke editing logic. Backlog entry appended to
`.artifacts/taskboard-potential-improvements-backlog.md`.

## 12. References

- Prior analysis: `.artifacts/reports/2026-10-11-org-mode-support-viability-report.md`
- egui: releases/changelog 0.33–0.36 —
  github.com/emilk/egui/releases; `epaint::TextFormat` —
  docs.rs/epaint/latest/epaint/text/struct.TextFormat.html;
  `TextEdit::event_filter`, `InputState::consume_key`, `Event::Ime` —
  docs.rs/egui; demo `code_editor` — crates/egui_demo_lib;
  Ferrite — github.com/OlaProeis/Ferrite
- iced: CHANGELOG + releases (0.13 `key_binding` PR #2522; 0.14 IME PR
  #2777) — github.com/iced-rs/iced; `text_editor` module docs —
  docs.rs/iced/latest/iced/widget/text_editor/; a11y issue #552 / PR
  #3281 (closed unmerged); examples/editor, examples/custom_widget
- Slint: releases 1.16–1.18 blogs — slint.dev/blog; `StyledText` —
  docs.slint.dev (elements/styledtext); `FocusScope`, `TextInput`,
  builtin-structs KeyEvent; docs.rs/slint `StyledText`; rich-text
  editing #2723, rendering #9560, platform IME #3811, read-only IME
  PR #2821; community: lpmwfx/slint-codeeditor, lpmwfx/rich-text-editor,
  rdenadai/ehwaz
- Editor cores: floem-editor-core — docs.rs/floem-editor-core; helix-core
  placeholder — crates.io/crates/helix-core + repo (MPL-2.0);
  lapce-core re-export — github.com/lapce/lapce; xi-editor README
  (discontinued); kakoune README (multi-client sessions)
- Buffers: ropey — crates.io/crates/ropey (+2.0-beta relicensing);
  crop — crates.io/crates/crop (benchmarks, byte-offset API)
- Layout engines: parley 0.12 — github.com/linebender/parley
  (`Style` axes, `editing::Cursor`); cosmic-text 0.19 —
  github.com/pop-os/cosmic-text (adopters: iced, gpui-Linux, glyphon)
- Emacs manual (normative semantics): gnu.org/software/emacs/manual/
  html_node/emacs/{Mark,Setting-Mark,Mark-Ring,Kill-Ring,Appending-Kills,
  Yanking,Earlier-Kills,Moving-Point,Arguments,Undo}.html
- Emacs-adjacent Rust: rustyline src/kill_ring.rs; reedline
  edit_mode/emacs.rs; organice — github.com/200ok-ch/organice (AGPL)

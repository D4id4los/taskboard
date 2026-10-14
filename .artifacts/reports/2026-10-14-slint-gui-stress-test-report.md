# Slint GUI Stress Test — Phase 7 Design Round Input

- **Date**: 2026-10-14
- **Status**: Research complete — decision inputs for the Phase 7 design
  session (chunk 3, stage 1 of the wave-0 dispatch plan). No code, no
  decisions implemented.
- **Scope**: the GUI requirements brief's §4 checklist (9 items) executed
  against the current stable Slint release, plus the renderer comparison
  and the Wayland+X11 capture-tooling survey. Each verdict names the
  GUI-brief requirement it gates (R1–R10 per
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`).
- **Companion plan**: `.artifacts/plans/2026-10-14-slint-gui-stress-test-plan.md`
  (committed before this research started).

---

## 1. Method (reproducible)

- **Version establishment**: `curl https://crates.io/api/v1/crates/slint`
  → `max_stable_version: 1.18.1`, updated 2026-09-21; the version list
  shows 1.18.0 (2026-09-16), 1.17.0/1.17.1 (2026-06/07), 1.16.0/1.16.1
  (2026-04). **Survey target: Slint 1.18.1** — the same version the two
  2026-10-11 viability reports assessed, so their Slint facts were
  re-verified rather than re-derived.
- **Source inspection**: the published crates' sources under
  `~/.cargo/registry/src/…/slint-1.18.1`, `i-slint-core-1.18.1`,
  `i-slint-compiler-1.18.1`, `i-slint-backend-testing-1.18.1`,
  `i-slint-backend-winit-1.18.1` (all downloaded via a /tmp probe
  project; file:line citations below). Feature flags via
  docs.rs/crate/slint/1.18.1/features. Changelog 1.16–1.18 via the
  upstream `CHANGELOG.md`. Upstream issues via GitHub.
- **Compile probe** (the only item where docs were ambiguous): a
  throwaway cargo project in `/tmp/slint-probe` with
  `slint = "=1.18.1"`; `cargo check` on a reference to
  `slint::testing::ElementHandle` → `error[E0433]: cannot find 'testing'
  in 'slint'` — establishing that no public `slint::testing` module
  exists at these features (§4.5). Nothing from /tmp was committed.

## 2. Verdict table

| # | GUI-brief §4 item | Gating req | Verdict |
|---|---|---|---|
| 1 | Focus traversal control & programmatic focus | R4 | **supported** |
| 2 | Global key-event interception without per-widget handlers | R5 | **supported** (+ new declarative `KeyBinding`; no sequences — see caveat) |
| 3 | Runtime theming via singletons / design tokens; text roles | R2 | **supported** (globals not shared across windows — minor caveat) |
| 4 | Scale factor as a user setting | R3 | **partial** — no runtime-mutable window scale API; env-var or one-shot constant only |
| 5 | `slint::testing` coverage for the logical harness | R6 | **partial** — no public `slint::testing`; API lives in the version-locked `i-slint-backend-testing` crate + an embedded MCP server (pleasant surprise) |
| 6 | Accessibility / screen-reader integration | R7 | **supported** (AccessKit, default-on, actively maturing) |
| 7 | Rich-text: `StyledText` render vs edit | §3 org rendering | **partial** (unchanged from the org-mode report: rendering done, editing open upstream) |
| 8 | Renderer choice for kiosk hardware | backlog [2026-10-07] | **decision: femtovg (default) + software renderer; no skia now** (§5) |
| 9 | Wayland + X11 screenshot/capture tooling | R6 (visual) | **surveyed** — Slint's own MCP `take_screenshot` is the recommended path; external tooling optional (§6) |

## 3. Per-item evidence

### 3.1 Focus traversal & programmatic focus — supported (R4)

Sources: `i-slint-compiler-1.18.1/builtin_elements.rs:1395-1438`
(FocusScope), `i-slint-core-1.18.1/window.rs:1451,1632,1654`.

- `FocusScope` provides: `has-focus` (out), `enabled`,
  `focus-on-click`, `focus-on-tab-navigation` (both in) — so tab
  traversal participation and click-focus behaviour are per-element
  controlled, exactly what a designed (not accidental) focus order
  needs.
- **Programmatic focus**: documented `focus()` / `clear-focus()`
  functions on FocusScope (compiler doc comments, lines 1410-1416);
  from Rust, `Window::focus_next_item()` / `focus_previous_item()` are
  public (`window.rs:1632/1654`) and `Window::set_focus_item(&ItemRc,
  bool, FocusReason)` is public but takes internal item types
  (`window.rs:1451`) — the .slint-level `focus()` is the intended seam.
- Focus *observability*: `focus-changed-event(reason: FocusReason)`,
  `focus-gained`, `focus-lost` callbacks — a keyboard-journey test can
  assert which scope holds focus without touching pixels.
- Changelog 1.18: "Fixed the focus not being cleared when the focused
  item becomes invisible" and "Fixed moving the focus being slow in a
  window with a very large number of elements" — active maintenance in
  exactly our area of concern.

**Gates R4**: yes — the brief's FocusScope-discipline architecture is
directly expressible; nothing found that blocks keyboard-only UI.

### 3.2 Global key-event interception — supported, with a bonus and one caveat (R5)

Sources: `i-slint-compiler-1.18.1/builtin_elements.rs:1376-1493`
(KeyBinding, FocusScope, key-event-delivery docs),
`passes/key_bindings.rs`, `parser/expressions.rs:512-528`
(`@keys` grammar).

- The pattern the brief assumed works: a root `FocusScope` wrapping all
  views, reached via the Window's `forward-focus`, receives every key
  event not consumed deeper (documented "Key Event Delivery" bubble:
  rejected events travel to parents; `capture-key-pressed/released`
  fire *before* the target's `key-pressed` — a tunneling phase).
- **Bonus**: 1.18 ships a declarative **`KeyBinding`** element inside
  FocusScope: `in property <keys> keys; in property <bool> enabled:
  true; callback activated;`. Keys use *logical* keys via the `@keys()`
  literal — grammar from the parser tests:
  `@keys(Control+Shift+Alt+Meta+"A")`, `@keys(Control+Shift?+Meta+Return)`
  (`?` = modifier ignored). The compiler even diagnoses duplicate
  bindings in one FocusScope as an error
  (`passes/key_bindings.rs:22-45`) — conflict detection partly
  build-time for free. `enabled` is a bindable property, so
  context-sensitive bindings are a state binding away.
- **Caveat**: `@keys` matches a **single key press with modifiers** —
  no sequences/chords. Emacs-style `C-x C-c` prefixes and vi modes
  remain the action-dispatch state machine's job (the brief already
  planned this; confirmed necessary).

**Gates R5**: yes — keymap-as-data should resolve to
action→(KeyBinding | intercepted key event); chords/modals in the
Rust-side action layer, exactly as the brief's R5 recommends.

### 3.3 Runtime theming via singletons / design tokens — supported (R2)

Sources: docs.slint.dev "Globals" guide; docs.rs
`slint_interpreter::Global` trait (the generated-code `Global` trait
has the same shape); discussion #9550 (switching the predefined
`Palette.color-scheme` from Rust at runtime).

- `export global Theme { … }` singletons are readable/writable from
  Rust via the generated `Global` trait implementations
  (`get_/set_` accessors) — colour tokens and text-role structs
  (font-size/weight/family per role) are ordinary properties, so
  compile-time themes with a runtime-switch follow-up (the brief's
  recommended sequencing) is fully expressible. The built-in `Palette`
  global proves the pattern (and `color-scheme` flipping works at
  runtime per #9550).
- **Caveat** (docs, "Globals"): global singletons are *not shared
  between separate windows* — each window instance needs its globals
  initialized. Relevant only if Phase 7+ opens multiple windows
  (pomodoro overlay as separate window would need theme re-init);
  a single-window app never notices.
- Mechanical enforceability of "no literal colours outside the theme"
  (brief §5.1) is a repo-side lint over `.slint` sources — not a Slint
  feature, no obstruction found.

**Gates R2**: yes, fully.

### 3.4 Scale factor as a user setting — partial (R3)

Sources: `i-slint-core-1.18.1/window.rs:2289-2303`,
`i-slint-backend-winit-1.18.1/winitwindowadapter.rs:602,1544`,
changelog 1.18.

- `Window::scale_factor()` is public (windowing-system-provided value).
  The setter `set_scale_factor` is `pub(crate)` — used by backends and
  *testing*. The only public setter is
  `Window::set_const_scale_factor(factor)`, documented: "From that
  point on, the scale factor is constant and cannot be changed
  anymore" — a one-shot override, not a live user setting.
- The winit backend honours a `SLINT_SCALE_FACTOR` env override at
  window creation (`winitwindowadapter.rs:602`) — settable from app
  bootstrap code (config → env var before Slint init) as a
  restart-applied setting.
- Fractional-scale correctness improved in 1.18 ("windows being
  auto-sized below their preferred size with fractional scale factors"
  fixed; femtovg fractional-position blur fixed).

**Gates R3**: partially. The design session must choose between
(a) env-var/bootstrap-applied scale (cheap, restart to change),
(b) an application-level root multiplier (a `ui-scale` theme token
every size binding multiplies by) — live-changeable but touches all
layout arithmetic, or (c) `set_const_scale_factor` before show.
Recommendation for the session agenda: (b) for live user scaling,
(a) as the OS-DPI alignment escape hatch. This is a genuine open
decision the brief flagged ("verify against current version
behaviour" — verified: no runtime window API).

### 3.5 Testing API for the R6 logical harness — partial, different shape than assumed (R6)

Evidence: compile probe (§1, E0433 — no `slint::testing` module);
`slint-1.18.1/lib.rs` re-exports (no testing module; `mcp`/
`system-testing` features call `init_testing_backends()` at
`lib.rs:440-443`); the testing backend's own README and sources.

The GUI brief's assumption ("Slint's testing API is the candidate")
needs restating: in 1.18.1 the harness surface is
**`i-slint-backend-testing`** — a published but *internal* crate whose
README calls the API "preliminary" and instructs pinning it
`=x.y.z` matching the slint version (no semver promises). Inventory
(from its sources):

- **Init**: `init_no_event_loop()` (unit tests), 
  `init_integration_test_with_mock_time()` + `mock_elapsed_time()`,
  `init_integration_test_with_system_time()` — no display needed, no
  pixels rendered, text measured at fixed font size (README) — CI-friendly
  headless by construction, and mock time aligns with the repo's
  deterministic-time testing rules (AGENTS §5).
- **Input injection**: `send_keyboard_string_sequence` (shifts
  uppercase automatically), `send_keyboard_char`,
  `send_keyboard_key_text`, `send_mouse_click`
  (`testing_backend.rs:71-150`); async gesture methods on
  `ElementHandle`: `single_click`, `double_click`, `drag`, mock-time
  `mock_drag`, `scroll` (`search_api.rs:988-1110`, aiming at the
  transformed element centre per changelog 1.18).
- **Element queries**: `ElementQuery` builder — `match_id`,
  `match_type_name`, `match_inherits`, `match_accessible_role`,
  `match_predicate`, `find_first/find_all`; convenience
  `find_by_accessible_label`, `find_by_element_id`
  (`search_api.rs:228-476`). Locating by accessible role/label means
  the a11y properties (R7) double as the harness's element addressing
  — one investment, two requirements.
- **State reads**: `accessible_value`, `accessible_label/checked/
  enabled/description`, `absolute_position`, `layout_kind`; no
  generic element-property getter — component-level property reads go
  through the normal generated getters on the component handle
  (always available), which is the layer journeys should assert at
  anyway.
- **Executable actions**: `invoke_accessible_default_action()`,
  `set_accessible_value`, `set_accessible_selection_offsets` — the
  "drive the UI through its accessibility tree" lane.
- **The surprise**: the testing backend embeds an **MCP server**
  (cargo feature `slint/mcp`, env `SLINT_MCP_PORT` + required
  `SLINT_EMIT_DEBUG_INFO=1`; binds localhost, no auth — dev/testing
  only). Through it an LLM agent can list windows, query the element
  tree, click/drag/type, and **`take_screenshot`** (base64 PNG) over
  JSON-RPC/MCP while the real app runs. Docs warn renderer fallback is
  silent — verify a real renderer is compiled in before trusting
  screenshots. There is also a `system-testing` protobuf/TCP transport
  sharing the same introspection layer.

**Gates R6**: yes — both halves. Logical: fully feasible as Rust
integration tests (headless, mock time, typed queries). Visual: the
MCP `take_screenshot` largely *replaces* the external compositor
capture dependency for LLM-driven UI debugging (§6). Costs to accept:
an internal crate as dev-dependency, version-locked to the slint pin,
re-audited on every Slint upgrade.

### 3.6 Accessibility / screen-reader integration — supported (R7)

Sources: docs.rs feature list (`accessibility` is a **default**
feature); `i-slint-backend-winit-1.18.1/Cargo.toml:268-275`
(accesskit + accesskit_winit deps, `accesskit_unix` on Linux →
AT-SPI); compiler builtin properties `accessible-role/-label/-value/
-description/-enabled/-checked/-read-only`; changelog 1.17–1.18.

- Screen-reader path on Linux is AccessKit-over-AT-SPI via the winit
  backend, compiled in by default — no feature work needed to get the
  platform bridge.
- 1.17 added `accessible-orientation`, `accessible-live-region`,
  landmark roles, and assistive-tech-writable `TextInput` values;
  1.18 exposed text-input *content and selection* to AT and hid
  redundant compound-widget internals from the tree — the trajectory
  is active investment, not maintenance mode.
- Property surface is role/label/value-centric (no advanced
  relationships API like labelled-by graphs) — enough for the brief's
  "semantic roles, labels on everything" widget discipline.
- The R6 ElementQuery matching by accessible role/label (§3.5) means
  a11y discipline is *mechanically enforced* by the journeys: an
  unlabelled control is invisible to the harness.

**Gates R7**: yes; automated-contrast/size audits remain repo-side
static checks over the theme (nothing in Slint provides them — no
obstruction either).

### 3.7 Rich-text: StyledText rendering vs editing — partial, unchanged (§3 org surface)

Sources: `slint-1.18.1/lib.rs:235-237` (`pub use
i_slint_core::styled_text::{StyledText,
StyledTextFromMarkdownError}`); changelog 1.16–1.18; issue #2723
(fetched 2026-10-14).

- Rendering: `StyledText` element + `@markdown()` since 1.16; runtime
  markdown parsing (`StyledText::from_markdown`) exposed since 1.17;
  1.18 styled inline code (monospace + capsule) — confirms the
  org-mode report's facts against 1.18.1 exactly.
- Editing: issue #2723 remains **open**; maintainer (tronical,
  2025-09-29): "our initial goal is to merely add support for rich
  text viewing"; ogoffart (2024-12-30): "not on our short-term
  roadmap". Last activity 2026-06-11 (community links). The
  org-mode report's MVP (rendered view + plain-source edit toggle,
  WYSIWYG gated upstream) stands; re-check per Slint release.

**Gates**: the org/markup rendering feature (§3 of the brief) and the
R10 help page's rendering; no change to the recorded plan.

### 3.8 Renderer comparison — decision: femtovg + software (default set); no skia (backlog [2026-10-07])

Sources: docs.rs/crate/slint/1.18.1/features (default features include
**both** `renderer-femtovg` and `renderer-software`); docs.slint.dev
"Backends & Renderers" and "LinuxKMS Backend" pages; changelog 1.18;
GitHub issues #8708 (no-GPU fallback guidance), #8714 (linuxkms
feature wiring).

| Renderer | Requires | Strengths | Costs | Kiosk fit |
|---|---|---|---|---|
| femtovg (default) | GPU: OpenGL/OpenGL ES | light binary, default, GL-on-kiosk fine | GPU driver must work; 1.18 fixed fractional-pixel blur | good where a working GL stack exists |
| software (default) | CPU only | runs anywhere, zero GPU deps, 1.18 perf work (opaque-cover skipping) | slowest; no cursor rendering under linuxkms | the no-GPU kiosk answer |
| skia | GPU: OpenGL/Metal/Vulkan/D3D | "best performance and largest feature set" (docs) | "heavy disk-footprint compared to other renderers" (docs); big build | only if a GPU kiosk needs advanced effects |
| vello | GPU (experimental, 1.18) | future | experimental | no |

- The backlog entry framed this as "skia vs femtovg"; the verified
  fact reframes it: the *default feature set already ships femtovg +
  software* with runtime selection/fallback via `SLINT_BACKEND`
  (issue #8708 documents the fallback behaviour maintainers recommend
  — software when no GPU). Taking **defaults** gives both kiosk
  profiles in one binary.
- `backend-linuxkms` exists for KMS/DRM-direct kiosk deployments (no
  X/Wayland); cursor rendering there needs skia or femtovg, not
  software (LinuxKMS docs). Optional feature — decide when a real
  kiosk target exists.
- CI note: femtovg pulls the OpenGL/EGL system libraries on
  ubuntu-latest (the original reason the UI crate shipped
  dependency-free); tests that run headless via the testing backend or
  `SLINT_BACKEND=headless` software rendering avoid needing a GPU, but
  the *build* still links GL unless software-only is chosen for CI
  jobs.
- Licensing: all renderer crates carry the same tri-license as the
  rest of the runtime (§7) — no differentiator.

**Recommendation (one)**: pin default features (winit backend,
femtovg, software, accessibility) for the shipped binary; add
`slint/mcp` only as a dev/testing feature toggle; revisit skia only
when a GPU kiosk with effects requirements materialises. Consequence:
the [2026-10-07] backlog entry's renderer question is answered
(nothing to decide beyond confirming defaults); the entry closes when
the dependency actually lands.

### 3.9 Wayland + X11 capture-tooling survey — external to Slint (R6 visual half)

The MCP `take_screenshot` (§3.5) is compositor-independent and
permission-free — for *LLM debugging our own app* it is the
recommended path and mostly obsoletes the external survey. Recorded
for completeness / compositor-level capture needs:

- **X11**: `xwd`/`maim`/`scrot`/ImageMagick `import` — XGetImage-based,
  no permission prompts, fully scriptable. Trivial story.
- **Wayland (wlroots family: Sway etc.)**: `grim` (+`slurp` for
  region) via `wlr-screencopy-unstable-v1`; no portal permission
  needed, scriptable — but only on compositors implementing the
  wlroots protocols.
- **Wayland (GNOME/KDE)**: `xdg-desktop-portal` Screenshot/ScreenCast
  APIs; backed by `xdg-desktop-portal-gnome/-kde`. Permissions story:
  interactive user consent dialogs by design; headless/CI use requires
  portal-specific allowlisting — hostile to automation. GNOME's legacy
  `org.gnome.Shell.Screenshot` D-Bus API is deprecated/restricted.
- **Consequence for R6**: the "ideally under both Wayland and X11"
  requirement is satisfied *for our app* by the MCP path on any
  compositor; external capture remains a dev-tooling option for
  compositor-level screenshots, chosen per the dev machine's
  compositor family.

## 4. Surprises

1. **No public `slint::testing`** (probe-confirmed) — the harness API
   is an internal, non-semver crate that must be `=x.y.z`-pinned
   beside the slint pin, **plus** an embedded MCP server purpose-built
   for AI-driven UI inspection and screenshots. Net effect on R6 is
   *better than the brief assumed*, at the cost of an internal-crate
   dev-dependency.
2. **Declarative `KeyBinding` element (new)** with logical keys,
   bindable `enabled`, and compile-time duplicate detection — a
   ready-made home for simple action bindings; single-stroke only.
3. **No runtime window-scale setter** — the brief's "must verify"
   instinct was right; scaling strategy becomes a real design-session
   agenda row (§3.4 options).
4. **The renderer question was partly a false dichotomy**: defaults
   already bundle femtovg + software with runtime fallback.
5. **Licensing**: the Slint *runtime* crates (slint, renderers, core,
   testing backend) are `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0
   OR LicenseRef-Slint-Software-3.0` (Cargo.toml license fields,
   §3.8 sources). Our `taskboard-ui-slint` is AGPL-3.0-only; AGPLv3
   §13 permits combining AGPL code with GPL-3.0-only works, with the
   combined work governed by GPLv3 for that combination (i.e. the
   network clause does not extend to the Slint runtime). Flagging for
   the design session/ADR: acceptable for this project's licensing
   posture, worth one ADR paragraph; the royalty-free/commercial
   tiers exist if distribution terms ever change.

## 5. Pin proposal

In the root `Cargo.toml` `[workspace.dependencies]` (when the
[2026-10-07] entry executes):

```toml
slint = "1.18.1"            # caret: pin minor, take patch fixes
# dev-only, in crates/taskboard-ui-slint:
i-slint-backend-testing = "=1.18.1"   # internal crate, no semver — exact pin
```

- Features: **defaults** (backend-default/winit, renderer-femtovg,
  renderer-software, accessibility, std, compat-1-2/1-18,
  system-tray — system-tray is harmless; strip via
  `default-features = false` re-listing if binary size ever matters).
- Dev/test toggle: `slint/mcp` feature + `SLINT_EMIT_DEBUG_INFO=1` +
  `SLINT_MCP_PORT` in a dev feature/runner script for the R6 harness.
- Upgrade discipline: both pins move together, every Slint bump
  re-audits the testing-backend API surface (its README mandates the
  exact-match pin).

## 6. Effort estimate (informing the design session, dev-days ≈ 6 focused hours)

| Follow-on (Phase 7 foundation work this report de-risks) | Dev-days |
|---|---|
| Slint dependency + renderer defaults + window shell + theme token global (incl. contrast CI check) | 3–5 |
| Action-dispatch/keymap layer (Rust-side, incl. chord/modal state machine; KeyBinding for simple binds) | 6–10 |
| Logical test harness on i-slint-backend-testing (journeys, mock time) | 3–5 |
| MCP harness wiring (dev feature, screenshot gate) | 1–2 |
| Scaling strategy (root-multiplier token vs env bootstrap) | 2–4 |

No blocking risk to the Phase 7 design round was found; the two
"partial" verdicts (scale factor, rich-text editing) map to already-
recorded plans rather than new obstacles.

## 7. References

- crates.io slint metadata/versions (fetched 2026-10-14):
  `https://crates.io/api/v1/crates/slint` — 1.18.1 stable 2026-09-21.
- Local pinned sources (via /tmp probe download):
  `slint-1.18.1/{lib.rs,Cargo.toml}`,
  `i-slint-core-1.18.1/{window.rs,api.rs}`,
  `i-slint-compiler-1.18.1/{builtin_elements.rs,parser/expressions.rs,
  passes/{key_bindings,resolving}.rs}`,
  `i-slint-backend-testing-1.18.1/{lib.rs,README.md,testing_backend.rs,
  search_api.rs,docs/development/mcp-server.md}`,
  `i-slint-backend-winit-1.18.1/{Cargo.toml,winitwindowadapter.rs}`.
- docs.rs: slint 1.18.1 feature flags; `i-slint-backend-testing`;
  `slint_interpreter::Global`.
- docs.slint.dev: Backends & Renderers; LinuxKMS Backend; Globals;
  FocusScope/KeyBinding reference (via compiler sources).
- Slint CHANGELOG.md 1.16–1.18 (StyledText timeline, a11y, testing,
  focus, renderer fixes).
- GitHub: issue #2723 (rich-text editing, open; maintainer statements
  2024-12/2025-09; activity 2026-06); issue #8708 (GPU fallback
  guidance); issue #8714 (linuxkms features); discussion #9550
  (Palette.color-scheme runtime switch).
- Prior art verified, not re-derived:
  `.artifacts/reports/2026-10-11-org-mode-support-viability-report.md`
  (§3.2 A6 StyledText facts), 
  `.artifacts/reports/2026-10-11-emacs-org-edit-backend-viability-report.md`
  (§3.3 IME, §5.3 Slint adapter) — both consistent with 1.18.1
  findings above.
- Capture tooling: github.com/emersion/grim (wlr-screencopy);
  xdg-desktop-portal Screenshot/ScreenCast docs
  (flatpak.github.io/xdg-desktop-portal); X11 `xwd`(1)/`maim`.

# [Plan] UI Shell, Theme & Tokens Foundation (GUI Phase 7, Chunk 1)

- **Date**: 2026-10-14
- **Kind**: implementation plan (chunk 1, stage 1 of the GUI Phase 7
  dispatch — TDD-first spec for the stage-2 implementation agent). No
  production code in this commit.
- **Origin**: accepted design round
  `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md` (Status:
  ACCEPTED 2026-10-14; normative here: §1.1–1.2 crate facts, §2 scope
  fence, §3 theming/tokens all subsections, §6.1–6.2 scaling, §9.1
  contrast check, §12 row 1; decisions Q7, Q10, Q12, Q13, Q18, Q22 as
  resolved in §11/§16); stress-test report
  `.artifacts/reports/2026-10-14-slint-gui-stress-test-report.md`
  (§3.3 theming facts, §3.8 renderer decision + CI consequence, §4.5
  licensing, §5 pin proposal); dispatch plan
  `.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  §1 (execution model, code-quality mandate, UI-crate hard rules) and
  §2 (chunk-1 covers/acceptance — binding).
- **Target**: `taskboard-ui-slint` (slint dep, build wiring, theme/token
  module, fonts, window shell, mount API), `taskboard-app` (`[ui]`
  config, UI lifecycle + entry composition, command forwarder), root
  `Cargo.toml` (pins), `deny.toml` (Slint tri-license), CI +
  pre-commit (lint gate wiring, GL system libs), README (escape hatch).
- **Branch / worktree**: `feat/ui-shell-theme` in
  `../taskboard-WT-feat-ui-shell-theme`, off `main` @ `c501301`.
- **Goal**: the GUI foundation seams land — the slint dependency at the
  stress pin, a window shell an app can mount, the theme/token module
  with its lint gate and contrast check, bundled fonts, the `[ui]
  scale` config hand-off, and the UI-command forwarder — with every
  open question resolved by a recorded decision (§3) and nothing from
  the design §2.3 feature surface built (chunk-1 non-goals, §2.2).
- **Status**: READY — stage-2 implements exactly this plan; deviations
  from the accepted design plan carry a recorded reason (§3 marks the
  two that exist; any further deviation must add its own).

---

## 1. Context & verified facts (anchored — do not re-derive)

1. **The UI crate today** is a dependency-free doc-comment stub
   (`crates/taskboard-ui-slint/src/lib.rs` + the NOTE in its
   `Cargo.toml`): deps are `taskboard-domain` + `arc-swap` only; no
   `build.rs`; the crate deliberately lacks `#![forbid(unsafe_code)]`
   (ADR 0003's sole documented exception — Slint-generated code
   requires `unsafe`; the exception note in `lib.rs` *stays*, the
   dependency-absence NOTE goes).
2. **The app side** already depends on `taskboard-ui-slint`
   (`crates/taskboard-app/Cargo.toml`, workspace-inherited). Bootstrap
   (`crates/taskboard-app/src/bootstrap.rs`) returns an `App` with
   `engine: EngineHandle`, `system: broadcast::Sender<SystemEvent>`.
   `EngineHandle` exposes `dispatch(StateCommand)` (fire-and-forget
   `try_send`, logs drops itself), `subscribe() ->
   broadcast::Receiver<EngineSignal>`, and `shared_state() ->
   Arc<ArcSwap<AppState>>` — exactly the seams the UI hand-over needs
   (`crates/taskboard-state/src/actor.rs:107-201`). The engine's own
   command inbox is 256 deep (`COMMAND_BUFFER`, actor.rs:36).
3. **Entry composition today** (`crates/taskboard-app/src/main.rs`
   `run()` + `repl::entry_point`): subcommand → dispatch/IPC; bare
   invocation → TTY ⇒ REPL, non-TTY ⇒ `[app] mode` resolution ⇒
   daemon lifecycle; the daemon lifecycle *refuses* `Desktop`/`Kiosk`
   ("not implemented yet"). `entry_point(false, true, Desktop)`
   currently answers `Repl` — a TTY under `mode = "desktop"` opens the
   REPL today (ADR 0011: the REPL serves any mode's console).
   `AppMode { Daemon, Desktop, Kiosk }` already exists in config
   (`crates/taskboard-app/src/config.rs:71-81`).
4. **Config patterns** (`config.rs`): every table is a `#[serde(
   deny_unknown_fields)]` section on `AppConfig`; semantic failures are
   typed `ConfigError` variants matched by `matches!` in tests; figment
   chain = defaults → toml → `TASKBOARD_*` (`__` nests) — a `[ui]`
   table inherits all of this for free. `ConfigError::kind()` is the
   stable per-variant discriminant surface (`config print`, `--json`).
5. **CI/pre-commit wiring points**: `.github/workflows/ci.yml` jobs
   `check`/`msrv`/`test`/`coverage`/`benchmark` each install system
   deps via apt (currently `libdbus-1-dev pkg-config`); the deny job
   runs `cargo deny check` against `deny.toml` (license allow-list +
   per-crate `[[licenses.exceptions]]` syntax available; `[graph]
   all-features = false`); `.githooks/pre-commit` runs fmt + clippy
   only. `.github/workflows/release.yml` builds
   `x86_64-unknown-linux-gnu` (ubuntu, apt step at line 86-88) and
   `x86_64-pc-windows-msvc` (windows) — the Linux release arm links GL
   once slint lands too. Python3 is established repo tooling
   (`scripts/changelog.py` with a `self-test` mode; CI runs it).
6. **Slint 1.18.1 facts** (stress report, version-pinned): default
   features = winit + femtovg + software renderer + accessibility
   (femtovg links GL/EGL system libs at build time; tests never need a
   GPU — nothing in chunk 1 runs an event loop in CI); globals are
   read/writable from Rust via generated `Global` traits; no runtime
   window-scale setter — `SLINT_SCALE_FACTOR` env is honoured at window
   creation (winit backend), `set_const_scale_factor` is one-shot and
   composes badly (design §6.2: unused); no public `slint::testing`
   (harness = chunk 4, explicitly *not* this chunk — dispatch §1.4
   item 5); runtime licensing = `GPL-3.0-only OR
   LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`,
   AGPLv3 §13-compatible with this project's posture (stress §4.5).

## 2. Scope

### 2.1 In scope (dispatch §2 "covers" — binding)

- slint dependency landing (pin + default features + `slint-build`
  build wiring) and the recorded CI consequence (Q10's open half).
- `cargo deny` configuration for the Slint tri-license runtime.
- Window shell + app wiring: event-loop ownership, the hand-over set,
  GUI entry composition (recorded decision), UI lifecycle.
- UI-command forwarder (backlog [2026-10-09] substance).
- `[ui]` config table with `scale` only (Q22), typed out-of-range
  error, push into `Theme.base-scale` at UI start (§6.1).
- Theme/token module (`Theme`/`Typography`/`Semantic` globals, closed
  §3.2 role table, §3.3 naming, spacing scale from `base-scale`,
  dark-first per Q13, theme-init-as-window-construction-step).
- Font bundling + registration (Q18; final pick recorded in §3.11).
- Token lint gate (three §3.4 rules, CI + pre-commit, no suppression).
- Contrast check (§9.1: pair list as data, WCAG ratios, minimums per
  role class, completeness rule).
- `SLINT_SCALE_FACTOR` escape hatch (§6.2: read before Slint init,
  README documentation, `set_const_scale_factor` unused).

### 2.2 Non-goals (dispatch §2 — do not build)

No views, no action/keymap code, no `i-slint-backend-testing` (chunk
4), no keymap config fields, no runtime theme switching or
light/high-contrast themes, no second window, no
`#![forbid(unsafe_code)]` in the UI crate, no domain/state-crate
changes, no ADR/architecture.org/testing_strategy.org amendments
(chunk 5 owns docs), no backlog edits (chunk 5 owns the ✅ marks), no
`app.menu`, no MCP feature, no golden tests. If any §2.3 surface seems
required to make a seam real, STOP and escalate (design §2.4).

## 3. Decisions (each binding for stage 2; D-numbers cited below)

### 3.1 D1 — Dependency landing & pin shape (Q10, Q12; stress §5)

Root `Cargo.toml` `[workspace.dependencies]` gains:

```toml
# GUI framework (Phase 7; Q10: default features = winit backend,
# femtovg + software renderers with runtime fallback, accessibility)
slint = "1.18.1"
# Companion build-time crate for slint::include_modules!() codegen;
# same pin discipline as slint (moves together on every bump)
slint-build = "1.18.1"
```

- `crates/taskboard-ui-slint/Cargo.toml`: `slint.workspace = true`
  (default features, verbatim Q10), `[build-dependencies]
  slint-build.workspace = true`, plus `tokio.workspace = true` — the
  crate names `tokio::sync::{mpsc, broadcast}` channel halves in its
  mount API (the designed hand-over, D6); only sync types are
  consumed; this follows the `taskboard-state` precedent (lib crates
  use the workspace tokio). tokio is not an IO crate in the AGENTS §2
  sense (it is the mandated channel fabric, AGENTS §3); a one-line
  comment in the manifest records "sync types only".
- New `crates/taskboard-ui-slint/build.rs`: `slint_build::compile(
  "ui/app-window.slint")` with default config (embedded font
  registration happens at runtime, not via the compiler's
  `embed-resources` — D11). The `lib.rs` gains `slint::include_modules!()`
  and the dependency-absence NOTE in `Cargo.toml` is removed; the
  unsafe-exception doc note in `lib.rs` **stays** (ADR 0003).
- `i-slint-backend-testing` is **not** added (chunk 4, dispatch §1.4
  item 5). The workspace `Cargo.lock` commits the resolved tree
  (lockfile discipline guards caret drift, Q12).

### 3.2 D2 — CI consequence: GL system libs, no software-only profile (Q10)

**Decision: ubuntu-latest jobs install the GL/EGL/winit system-library
set; no software-only CI profile exists.**

- Why: CI must compile what ships (default features per Q10). A
  software-only feature-split profile would build a *different*
  feature set than the released binary — the tested ≠ shipped drift is
  exactly the hazard a CI profile must not introduce, and maintaining
  a second feature recipe invites divergence at every future feature
  change. The apt cost is four packages on jobs that already apt-install.
- Change: extend the existing "Install system deps" apt lines in
  `ci.yml` (`check`, `msrv`, `test`, `coverage`, `benchmark`) and
  `release.yml` (Linux arm of the build matrix; the `if: runner.os ==
  'Linux'` guard already scopes it) to also install:
  `libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libwayland-dev`.
  (Stress §3.8: the *build* links GL unless software-only is chosen;
  tests themselves need no GPU or display — chunk 1 runs no event loop
  in CI, and link-time is the only GL touchpoint.)
- Recorded completion rule: if linking surfaces further missing
  system libs (the winit/x11 dlopen story may move libs to runtime),
  adding them to the same apt lines is the mechanical completion of
  this decision, not a new one.

### 3.3 D3 — cargo-deny: per-crate exceptions for the Slint tri-license

`deny.toml` `[licenses]` gains **per-crate exceptions**, not a global
`GPL-3.0-only` allow:

```toml
# Slint runtime family (stress §4.5): tri-licensed
# GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR
# LicenseRef-Slint-Software-3.0 — AGPLv3 §13-compatible with this
# project's posture (design Q11). Per-crate exceptions, never a
# global GPL allow: the dual-tier library crates (MIT OR Apache-2.0)
# must never silently accept a GPL-3.0-only dependency.
[[licenses.exceptions]]
allow = ["GPL-3.0-only", "LicenseRef-Slint-Royalty-free-2.0", "LicenseRef-Slint-Software-3.0"]
crate = "slint"
# … repeated verbatim for each crate of the family …
```

- Seed list (completed mechanically from `cargo deny check licenses`
  output — adding a further slint-family crate the diagnostic names is
  the recorded completion of this decision): `slint`, `slint-macros`,
  `slint-build`, `i-slint-common`, `i-slint-compiler`, `i-slint-core`,
  `i-slint-backend-selector`, `i-slint-backend-winit`,
  `i-slint-renderer-femtovg`. (`i-slint-backend-testing` is absent —
  chunk 4; `[graph] all-features = false` keeps feature-gated crates
  out of the checked graph.)
- Recorded fallback (only if cargo-deny rejects `LicenseRef-*` inside
  exceptions): move the two `LicenseRef-Slint-*` ids to the global
  `allow` (they are unambiguous — namespaced to Slint) and keep
  `GPL-3.0-only` per-crate. The hazard being avoided — globally
  licensing-permitting GPL-3.0-only into the dual-tier library crates
  — stays avoided either way.
- Fonts are data files, not crates: OFL-1.1 is **not** added to
  cargo-deny; the licence record lives in-repo (D11). `confidence-
  threshold` untouched (the slint crates carry precise SPDX license
  fields — stress §3.8/§4.5 sources).

### 3.4 D4 — GUI entry composition: `[app] mode = "desktop"` boots the GUI

**Decision: on a bare invocation, `mode = "desktop"` routes to the UI
lifecycle regardless of TTY; no new CLI subcommand; `kiosk` stays
refused.**

- Rationale: `AppMode` is *frontend selection* (config.rs docs;
  ADR 0008) — `Desktop` exists precisely for this. TTY-sensitivity is
  the REPL-vs-daemon rule for `mode = "daemon"` (ADR 0011); letting a
  TTY override an explicit desktop mode would mean `taskboard` from a
  terminal (the normal way to launch a desktop app) opens a REPL
  instead of the configured frontend. Non-TTY + desktop (launched
  from a `.desktop` file) must obviously GUI.
- Consequence (behaviour change, recorded): bare+TTY under
  `mode = "desktop"` moves from REPL to GUI. The REPL remains
  reachable under `mode = "daemon"` (+ TTY); an explicit
  `taskboard daemon` still runs the daemon lifecycle, whose
  Desktop/Kiosk refusal gate stays (an explicit daemon under a
  desktop-mode config is a misconfiguration, loudly reported).
- Rejected alternative: a `taskboard ui` subcommand — new CLI surface
  (ADR 0009 scope), redundant with the designed config selector, and
  it would leave `[app] mode = "desktop"` pointing at nothing.
- `kiosk`: stays "not implemented" — the kiosk display-mode policy is
  W1+ (design Q4/§6.4) and v0.1.0 ships the desktop UI only. When that
  wave opens, kiosk routes to the same shell with a display-mode
  policy.
- Mechanics (`main.rs` `run()`, bare-invocation arm, before the REPL
  branch): `match config.app.mode { Desktop => ui::run_desktop(…),
  Kiosk => existing bail, Daemon => existing entry_point flow }`.
  `repl::entry_point` itself is unchanged (its tests stay green; its
  ModeResolution arm just becomes unreachable for Desktop/Kiosk from
  `run()` — the non-TTY desktop case is intercepted above it).

### 3.5 D5 — Event-loop ownership & the desktop lifecycle

**The slint event loop runs on the process main thread.**

- `main()` keeps its single multi-thread runtime and `block_on(run(…))`
  structure. `taskboard_app::ui::run_desktop(config, config_path)` is
  an async fn that:
  1. reads `SLINT_SCALE_FACTOR` (D13) — *before anything slint-side*;
  2. resolves credentials + `bootstrap(…)` exactly like the daemon
     path (same store selection, `LogSink` notifier);
  3. builds the hand-over (D6) and spawns the forwarder (D7);
  4. calls `taskboard_ui_slint::mount(…)` (creates the window,
     registers fonts, applies theme + scale);
  5. spawns the exit watcher (D5b);
  6. calls `taskboard_ui_slint::run()` — a blocking
     `slint::run_event_loop()` **on the main thread** (we are inside
     `block_on` on the main thread; the multi-thread runtime's workers
     keep driving every actor — blocking the block_on participant is
     safe, and main-thread ownership is winit's macOS requirement,
     a first-class release target);
  7. when the loop returns (window closed or watcher quit it):
     `app.shutdown().await` and return its outcome.
- **D5b — exit paths**: (a) window close ends the event loop; (b)
  ctrl_c/SIGTERM: a spawned watcher awaits `wait_for_signal()` (reuse
  of `bootstrap.rs`'s existing helper) and calls
  `taskboard_ui_slint::quit()` (thin wrapper over
  `slint::quit_event_loop()`), landing in the same graceful shutdown.
  (`app.exit` as an *action* is chunk 2; the physical exit paths are
  wired now so shutdown is always graceful.) The watcher + `quit()`
  wrappers are thin glue in the "deliberately untested" main-glue
  class (main.rs precedent); the forwarder and config carry the tests.
- The app crate takes **no direct slint dependency**: every slint
  surface (`mount`, `run`, `quit`) crosses the `taskboard-ui-slint`
  API boundary, keeping the presentation framework in exactly one
  crate.

### 3.6 D6 — The hand-over (what bootstrap hands the UI)

`taskboard_app::ui::run_desktop` constructs and passes, in one
`mount` call, the full designed seam set (design §1.2; dispatch §2):

```rust
// taskboard-ui-slint public API (chunk-1 shape; chunks 2–3 consume)
pub struct UiSettings { pub scale: f64 }            // validated config value
pub struct UiHandle { /* component instance + parked seam halves */ }

pub fn mount(
    state: Arc<ArcSwap<AppState>>,                   // engine.shared_state()
    signals: broadcast::Receiver<EngineSignal>,       // engine.subscribe()
    system: broadcast::Receiver<SystemEvent>,         // system.subscribe()
    commands: mpsc::Sender<StateCommand>,             // UI emits; app forwards
    settings: UiSettings,
) -> Result<UiHandle, UiMountError>;
pub fn run(handle: UiHandle) -> Result<(), UiRunError>; // event loop
pub fn quit();                                           // quit_event_loop
```

- The UI crate *stores* the state handle + both receivers + the
  command sender in `UiHandle` — they are the seam chunks 2–3 snap
  into (the resolver dispatches through `commands`, the views read
  `state`, the status-bar reads `system`). No machinery is built
  around them in chunk 1 (no repaint loop, no drain task — those are
  chunk 3's consumers; undrained broadcast receivers merely go
  `Lagged`, which is inert). This is the dispatch-listed hand-over,
  not speculative generality.
- The app keeps the *receiver* half of the command channel and the
  `EngineHandle`; the UI never sees an `EngineHandle` (crate boundary:
  the UI reads domain state + emits domain commands, it does not know
  the engine exists).
- Channel: `mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY)` with
  `UI_COMMAND_CAPACITY = 64` (engine inbox is 256; UI emission is
  human-input-scale; a full engine-side inbox is already logged by
  `EngineHandle::dispatch`'s own drop logging).
- `UiMountError` is a typed enum (`FontRegistration`, `WindowCreation`)
  — thiserror, per the app-crate error convention.

### 3.7 D7 — UI-command forwarder (backlog [2026-10-09] substance)

Lives in a new `crates/taskboard-app/src/ui.rs` (bootstrap.rs is
already at capacity; SRP: `ui.rs` owns the UI lifecycle + adaptation):

```rust
/// The engine-side sink the forwarder adapts onto. A trait so the
/// forwarder is hermetically testable without a state engine.
pub trait CommandSink: Send + 'static {
    fn dispatch(&self, command: StateCommand);
}
// CommandSink for EngineHandle delegates to EngineHandle::dispatch —
// whose own try_send drop logging (full/closed inbox) is the state
// crate's tested behaviour, not re-tested here (one behaviour, one
// test, lowest layer).

pub fn spawn_ui_command_forwarder<T: CommandSink>(
    commands: mpsc::Receiver<StateCommand>,
    sink: T,
) -> tokio::task::JoinHandle<()>;
// tokio::spawn: while let Some(cmd) = recv().await { sink.dispatch(cmd) }
// Channel close (UI gone) exits the loop with a tracing::info line —
// the forwarder's own "dropped" surface; command *drops* on a full or
// closed engine inbox remain EngineHandle::dispatch's logged behaviour.
```

Hermetic test (in `ui.rs`, `#[cfg(test)]`): a fake `CommandSink`
collecting into an mpsc/`Arc<Mutex<Vec<_>>>`; send a sequence of
distinct typed `StateCommand`s, drop the sender, await the join
handle, assert the collected sequence matches exactly (order +
content) — join/channel based, no sleeps, no engine spawned.

### 3.8 D8 — `[ui]` config table: `scale` only (Q22, §6.1)

- `config.rs`: `UiSection { #[serde(default = "default_ui_scale")]
  pub scale: f64 }` with `#[serde(deny_unknown_fields)]`,
  `AppConfig.ui: UiSection` (`#[serde(default)]`),
  `DEFAULT_UI_SCALE: f64 = 1.0`.
- Validation (`AppConfig::validate`): `scale` must be finite and
  `0.5 <= scale <= 3.0`, else the **new typed variant**
  `ConfigError::InvalidScale { field: &'static str }` (field
  `"ui.scale"`), `kind()` discriminant `"invalid-scale"`. "Clamped
  0.5–3.0" (design §6.1) reads as *the range is enforced*: fail-fast
  typed error, never a silent clamp — consistent with every other
  semantic config check; Q15's "never block start" redirect is
  keymap-specific (§16) and does not extend here. NaN/±inf are
  rejected (finite check first — no comparison-only loophole).
- figment gives `TASKBOARD_UI__SCALE` and toml `[ui] scale = …` for
  free; unknown `[ui]` keys fail decode (strict-table rule).
- Tests (figment-driven, existing style): default 1.0 round-trips;
  boundaries 0.5/3.0 accepted; 0.49/3.01 → `InvalidScale`; `nan` →
  `InvalidScale`; env override; unknown key decode failure; `kind()`
  stability.
- Hand-off: `run_desktop` passes `config.ui.scale` into
  `UiSettings` — the UI crate receives the typed value, reads no
  config (design §1.2: IO-free crate).

### 3.9 D9 — Theme module structure (§3.1–3.3, §3.5; Q13)

`crates/taskboard-ui-slint/ui/theme.slint` — the only place colours,
font attributes, and base-dimension arithmetic live — exporting three
globals:

- `global Theme` — the palette layer (raw values, referenced by the
  semantic layer only), `base-scale` (`property <float> base-scale:
  1.0;` — the §6.1 root multiplier, default 1.0), the spacing scale
  `space-1…space-8` (4/8/12/16/24/32/48/64 `px × base-scale`), border
  radii, focus-ring width, and the window default dimensions (see
  below) — every dimension defined as `base-px * Theme.base-scale`.
- `global Typography` — the **closed §3.2 role table**, each role a
  struct `{ size, weight, family }`: `display` (30px, 700), `title`
  (20px, 700), `heading` (16px, 600), `body` (14px, 400),
  `body-emphasized` (14px, 600), `caption` (12px, 400),
  `badge` (11px, 600), `mono` (12.5px, 400, mono family) — sizes ×
  `base-scale`; families reference the bundled fonts (D11). Adding a
  role is a theme-level decision (table + ADR-0015 amendment note,
  chunk 5's ADR) — the lint makes per-view invention impossible.
- `global Semantic` — state colours mapped onto palette entries:
  `success`, `warning`, `danger`, `overdue`, `offline`, `sync-active`,
  `focus-ring` (§3.1) — nothing outside the theme names palette
  colours.
- Naming (§3.3): function names (`text-secondary`, `surface-raised`),
  never hue names; two layers only (palette → semantic); views (when
  they exist) reference semantic/role tokens only.
- **Dark-first** (Q13): one compile-time dark theme; no
  light/high-contrast values, no switching machinery (POST-1.0).
- **Theme init is a window-construction step** (§3.1's per-window
  caveat rule): `mount()` applies `base-scale` (+ future theme state)
  to the freshly created instance — one line in the window shell; a
  future second window re-runs it. No global-once init exists.
- Palette starting values (implementation may tune; the contrast
  check gates the result — D10): `background #14161c`,
  `surface #1d2028`, `surface-raised #262a35`, `text-primary #e8eaf0`,
  `text-secondary #a6abbd`, `border #3b4050`, `focus-ring #7aa2f7`;
  Semantic: `success #6fd98b`, `warning #e8c268`, `danger #e8756f`,
  `overdue #ff9e64`, `sync-active #7aa2f7`, `offline` mapped to the
  text-secondary entry.
- Window shell (`ui/app-window.slint`): `export component AppWindow
  inherits Window` — `background: Theme.background`, `title:
  "taskboard"`, `preferred-width/height` from Theme tokens (the window
  default size is a spacing-class dimension and belongs in the theme,
  keeping rule 3 of the lint total). No content, no focus scopes, no
  text (views are chunk 3).

### 3.10 D10 — Token source of truth & the contrast check (§9.1)

- **Single rendering source** is `theme.slint`. The contrast check is
  a pure Rust unit test (`#[cfg(test)] mod token_tests` in the UI
  crate) that reads the file via `include_str!("../ui/theme.slint")`
  and extracts declared colour literals with a small purpose-built
  extractor (name → hex), unit-tested itself on fixture strings —
  the same class of code as the lint scanner (D12), not a slint API
  dependency. No Rust-side duplicate of the palette exists to drift.
- **Pair list as const data**: `[(foreground: TokenName, surface:
  TokenName, MinRatio)]` with `MinRatio::{Body => 4.5, Large => 3.0,
  NonText => 3.0}` per §9.1's role classes. Seed pairs (complete over
  the D9 token set): text-primary × background (Body), text-primary ×
  surface (Body), text-primary × surface-raised (Body),
  text-secondary × background (Body), text-secondary × surface
  (Body), text-secondary × surface-raised (Body), each Semantic
  text-bearing colour (success/warning/danger/overdue) × background
  and × surface (Body — state text renders at caption/body size),
  focus-ring × background (NonText), focus-ring × surface (NonText),
  border × background (NonText), border × surface (NonText). The
  display/title roles ride text-primary at the Large class via the
  pair list entry `(text-primary, background, Large)` — the check
  asserts *both* classes where a token serves both role classes.
- **WCAG 2.x math**: relative luminance per spec, ratio
  `(L1+0.05)/(L2+0.05)`, pure `f64` — sanity-anchored by a test that
  black-on-white yields 21.
- **Completeness rules** (§9.1 "the pair list cannot silently lag the
  theme"): (1) every colour token the extractor finds declared in
  `Theme`/`Semantic` appears in ≥1 pair or in an explicitly recorded
  no-text-duty exemption list (empty at seed; any entry carries a
  reason comment); (2) every `Typography` role name appears in ≥1
  role-class mapping; (3) a pair referencing an undeclared token
  fails. A new token without pairs therefore fails the test.
- Font-size/weight values are not contrast-checked (§9.1 is colours;
  text-size floor checks are the recorded POST-1.0 audit).

### 3.11 D11 — Fonts: pick, licence record, registration (§3.6; Q18)

- **Final pick**: **Inter** static instances (Regular 400, SemiBold
  600, Bold 700) + **JetBrains Mono** Regular 400 — the design's own
  example class ("Inter/JetBrains-Mono class"), both **SIL OFL 1.1**.
  OFL 1.1 is the licence class the design calls tri-licensed-compatible
  here: distributable and embeddable under all three of the project's
  licensing tiers (AGPL-3.0-only app; MIT OR Apache-2.0 dual-tier
  libs) with the Reserved Font Names respected (files shipped
  unmodified, never renamed). OFL is not added to cargo-deny (D3).
- **Static instances, not variable fonts**: the stress round verified
  no font facts; slint 1.18.1's variable-font weight-axis selection is
  unverified, and golden-test determinism (the decisive reason, §3.6)
  plus the role table's fixed weights are guaranteed by static
  instances. Cost ≈ 4 TTFs ≈ 1.1–1.3 MB repo + binary — accepted (Q18).
- Files: `crates/taskboard-ui-slint/assets/fonts/` —
  `Inter-Regular.ttf`, `Inter-SemiBold.ttf`, `Inter-Bold.ttf`,
  `JetBrainsMono-Regular.ttf`, plus `LICENSES.md` recording each
  font's version, upstream release URL, SHA-256, copyright line, and
  the OFL-1.1 text (one copy) — the Q18 licence-record step. Sourced
  at implementation time from the official GitHub releases
  (rsms/inter; JetBrains/JetBrainsMono); nothing downloads at build
  time.
- Registration at startup: embedded via `include_bytes!` +
  `slint::register_font_from_memory` in `mount()` before instance
  creation (offline, single-binary, deterministic). If the memory API
  is absent under this name at 1.18.1, the recorded fallback is
  `slint::register_font_from_path` against extracted bytes — same
  semantics, one extra tempfile step; the implementation verifies and
  records which landed. Registration is idempotent per process
  (registered set guarded) so re-mounts in tests are safe.

### 3.12 D12 — Token lint gate (§3.4): script, rules, wiring, no escape

- `scripts/check_slint_tokens.py` (python3 — repo precedent
  `changelog.py`; fast enough for pre-commit; CI runs it as a plain
  step). Interface: `check_slint_tokens.py [--self-test]`; exit 0/1;
  scans every `*.slint` under `crates/taskboard-ui-slint/` (any future
  .slint location joins by construction — "no .slint file exists that
  the lint gate does not cover").
- **Rule 1 — no literal colours** outside `ui/theme.slint`: hex forms
  `#rgb`, `#rrggbb`, `#rrggbbaa`/`#aarrggbb`; `rgb(`/`rgba(`/`hsl(`
  calls; `Colors.*` named constants. (Slint comments `//` and
  `/* */` are stripped before matching.)
- **Rule 2 — no literal font attributes** outside `theme.slint`:
  property writes to `font-size`, `font-weight`, `font-family`.
- **Rule 3 — no literal spacing dimensions** in non-theme files:
  `px`-suffixed literals assigned to layout properties
  (`width`, `height`, `min-/max-` variants, `x`, `y`, `padding*`,
  `margin*`, column/row spacing) **unless** the value expression
  references `Theme.`, `Typography.`, or `Semantic.` tokens or is
  proportional/relative (percent values, `fr` units, pure arithmetic
  over other lengths — the §3.4 exemption).
- **No suppression escape hatch** — no comment pragma exists in the
  script (design §3.4; adding one later needs a recorded reason).
- `--self-test` (the `changelog.py` pattern): fixtures per rule — a
  violating snippet per rule fails, the theme-file exemption passes,
  the proportional exemption passes, a token-referencing expression
  passes. CI: new step "Slint token lint" in the `check` job.
  Pre-commit: added after the fmt step (runs in milliseconds).
- Design §14's fallback stands on record: if rule 3 proves unworkable
  during implementation, retreating to colours+fonts only is a
  *decision* to bring back (dropping live-scaling totality with it) —
  not silent drift. The plan's regex set is conservative on purpose:
  prefer a miss (a literal slips through) over a false hit (a
  compliant expression fails); the reviewer greps new .slint diffs as
  the second net.

### 3.13 D13 — `SLINT_SCALE_FACTOR` escape hatch (§6.2)

- Documented in `README.org` (Configuration section): what it does
  (winit-backend window-scale override honoured at window creation),
  when to use it (wrong OS-DPI environment), and that it composes
  with `[ui] scale` (OS-DPI alignment × token multiplier — they are
  orthogonal by design, §6.1–6.2; changing it requires a restart).
- "Read before Slint init": `run_desktop` reads the env var **before**
  any slint-touching call and `tracing::info!`s its presence/value
  when set — the app never *sets* it programmatically (the user's env
  is the input; observability is the app's whole duty toward it).
- `set_const_scale_factor` is **unused** — verbatim §6.2 (one-shot,
  irreversible, composes badly with a user setting). Stage 3 greps the
  diff for it as a negative check.

### 3.14 D14 — Testing the scale push (and any component instantiation) headlessly

- The dispatch acceptance ("a valid scale reaches `Theme.base-scale`
  via the generated `Global` setter") needs a component instance, and
  chunk 1 has no testing backend. **Decision: a dev-only
  `NullPlatform`** in the UI crate's test code implements
  `slint::platform::{Platform, WindowAdapter}` (public, semver-stable
  surface; `Renderer`'s methods are no-op-provided) and registers via
  `slint::platform::set_platform`. nextest's process-per-test model
  makes the process-global registration safe; `cargo test --doc` never
  instantiates components (doc examples stay pure).
- Tests riding it: mount-with-scale → `global::<Theme>(
  ).get_base_scale() == scale`; font registration smoke (register,
  instantiate, drop — no panic); the window instance exists. These run
  in the ordinary `test` CI job (no display, no GPU — nothing
  renders).
- Recorded fallback: if the `Platform` trait surface at 1.18.1 is
  heavier than a ~40-line null impl, the scale-assertion degrades to a
  recorded coverage gap (glue line type-checked, behaviour covered by
  chunk 4's journeys/goldens at scale extremes) — stated in the chunk
  report, never silently skipped.

### 3.15 D15 — Deliberate omissions (anti-scope, recorded)

No `Responsive` global or breakpoints (§6.3 — chunk 3; the window
shell only carries theme + scale). No `SLINT_BACKEND` handling in
code (env-var surface, user-owned). No repo README section on the GUI
beyond the escape hatch + `[ui] scale` (user-facing docs ride the
release chunk). No `unsafe` beyond Slint-generated code; every other
crate keeps `forbid`.

## 4. Implementation steps (TDD order; every commit standalone)

Rebase-merge hygiene: each commit compiles, passes its own tests, and
carries a `type(module): subject` line. Sizes are the added/changed
line estimates.

1. **`feat(config): [ui] table with scale and typed range validation`**
   (~140 lines: config.rs + tests). Tests first (D8 catalogue), then
   `UiSection` + `InvalidScale` + `kind()`. No consumers yet —
   `AppConfig` simply carries the table; existing tests untouched.
2. **`feat(app): ui command forwarder with fake-sink test`** (~120
   lines: new `src/ui.rs` + registration in `lib.rs`). Tests first
   (hermetic fake `CommandSink`), then the trait + spawn fn (D7).
3. **`build(ui-slint): land slint 1.18.1 with theme module and token gate`**
   (~480 lines + Cargo.lock). One commit, three concerns, one reason:
   a compiling slint build *requires* at least one `.slint` file, and a
   `.slint` file without the token gate on the branch would violate the
   chunk's own end-state rule ("no .slint file exists that the lint
   gate does not cover") mid-branch. Carries: root `Cargo.toml` pins
   (D1), crate manifest + `build.rs` + NOTE removal, `deny.toml`
   exceptions (D3), ci.yml/release.yml apt lines (D2), pre-commit gate
   call (D12), `scripts/check_slint_tokens.py` + its self-test,
   `ui/theme.slint` (D9) and `ui/app-window.slint`. The python
   self-test and the .slint fixtures are written before the script
   passes them (the TDD loop inside the commit).
4. **`test(ui-slint): token contrast check over theme pairs`**
   (~260 lines: `#[cfg(test)]` module — extractor + pair data +
   completeness tests). Tests-first by construction: the check is
   itself the deliverable; the D9 palette values are tuned until the
   pairs pass (the values are proposals, the check is the contract).
5. **`feat(ui-slint): bundled fonts with runtime registration`**
   (~1.2 MB binaries + ~90 lines text/code: assets, LICENSES.md,
   registration in the mount preamble, smoke test on NullPlatform (D14
   — the NullPlatform test-support module lands here, its first
   consumer)).
6. **`feat(ui-slint): window shell mount api with scale push`**
   (~220 lines: lib.rs mount/run/quit, `UiHandle`, `UiSettings`,
   `UiMountError`, the base-scale push + the D14 scale tests).
7. **`feat(app): desktop entry lifecycle and ui hand-over`** (~230
   lines: `ui.rs` grows `run_desktop` + the hand-over + exit watcher,
   main.rs mode routing (D4), the SLINT_SCALE_FACTOR read; README
   section included here — one user-visible behaviour, one commit
   (~30 lines of README)).

Commits: 6 total, each passing fmt + clippy + nextest + doc-tests +
deny locally (the llvm-cov gate runs at stage-2 completion over the
branch).

## 5. Test catalogue (TDD ledger)

| # | Test (location) | Asserts | Rule hooks |
|---|---|---|---|
| 1 | config: default round-trip (`config.rs`) | `scale == 1.0` after figment defaults | D8 |
| 2 | config: boundaries 0.5/3.0 accepted | `validate()` Ok | D8 |
| 3 | config: 0.49/3.01/NaN rejected | `matches!(.., InvalidScale{field:"ui.scale"})` — typed, no text | AGENTS §5, D8 |
| 4 | config: unknown `[ui]` key decode error | extract fails | strict tables, D8 |
| 5 | config: `TASKBOARD_UI__SCALE` override | figment chain | D8 |
| 6 | config: `kind()` discriminant | `"invalid-scale"` | D8 |
| 7 | forwarder: forwards sequence in order, exits on close (fake sink) | collected typed commands == sent; join completes | AGENTS §5 (fakes, no sleeps), D7 |
| 8 | lint gate self-test (python fixtures) | per-rule violations fail; theme-file + proportional + token-ref exemptions pass | D12 |
| 9 | contrast: extractor on fixtures (rust) | name→hex extraction correct | D10 |
| 10 | contrast: all declared tokens extracted | extractor set == expected declared set | D10 completeness (1) |
| 11 | contrast: every pair ≥ min ratio | WCAG math over extracted values | §9.1, D10 |
| 12 | contrast: completeness rules | undeclared-token pair fails; unpaired token fails; role coverage holds | §9.1, D10 |
| 13 | contrast: WCAG anchor | black/white ratio == 21 | D10 |
| 14 | fonts: registration + instantiate smoke (NullPlatform) | no panic; instance drops cleanly | D11, D14 |
| 15 | mount: scale reaches Theme.base-scale (NullPlatform) | `get_base_scale() == 2.0` after mount(2.0) | §6.1, D14, acceptance |
| 16 | mount: default scale is 1.0 | same path, default settings | §6.1 |
| 17 | ui.rs: run_desktop routing unit — mode→entry mapping where testable without a display | Desktop routes to the UI lifecycle arm (pure match extraction if needed) | D4 |

Not tested here (recorded): `quit()`/exit watcher/`run()` (thin main
glue class); `EngineHandle::dispatch` drop logging (state crate's
layer); doc-tests avoid component instantiation (D14).

## 6. File manifest (end state)

```
Cargo.toml                                  (+slint, +slint-build pins)
Cargo.lock                                  (resolved tree)
deny.toml                                   (+per-crate tri-license exceptions)
.github/workflows/ci.yml                    (apt GL libs ×5 jobs; lint-gate step in check)
.github/workflows/release.yml               (apt GL libs, linux build arm)
.githooks/pre-commit                        (+token lint call)
README.org                                  (+[ui] scale + SLINT_SCALE_FACTOR section)
scripts/check_slint_tokens.py               (new, +self-test)
crates/taskboard-ui-slint/
  Cargo.toml                                (+slint, +slint-build, +tokio; NOTE removed)
  build.rs                                  (new: slint_build::compile)
  assets/fonts/                             (4 TTFs + LICENSES.md)
  ui/theme.slint                            (new: Theme/Typography/Semantic)
  ui/app-window.slint                       (new: minimal shell)
  src/lib.rs                                (mount/run/quit, UiHandle, errors, include_modules!)
  src/test_platform.rs (#[cfg(test)])       (NullPlatform + tests 14–16)
  src/token_tests.rs (#[cfg(test)])         (extractor, pairs, contrast, tests 9–13)
crates/taskboard-app/
  src/config.rs                             (UiSection, InvalidScale + tests 1–6)
  src/ui.rs                                 (forwarder + CommandSink + run_desktop + hand-over)
  src/lib.rs                                (mod ui)
  src/main.rs                               (mode routing D4)
```

## 7. Acceptance mapping (dispatch §2 → this plan)

| Dispatch acceptance line | Where |
|---|---|
| gates green incl. deny with Slint license config | §4 steps, D3 |
| lint gate: three rules, CI + pre-commit, no escape | D12, steps 3/8 wiring |
| contrast check: §9.1 minimums, complete pair list | D10, tests 10–12 |
| out-of-range `[ui] scale` = typed config error | D8, tests 3/6 |
| valid scale reaches Theme.base-scale | D6/D14, tests 15–16 |
| forwarder hermetic, typed commands, no sleeps | D7, test 7 |
| SLINT_SCALE_FACTOR documented + read before init; set_const_scale_factor unused | D13, step 8/9 |
| UI crate's only new runtime deps: slint (+tokio sync types) | D1/D6 |
| no .slint file outside lint-gate cover | D12 + step-3 merge decision |
| renderer/CI-profile decision recorded | D2 |

## 8. Risks & fallbacks

- **GL link set larger than the four apt packages** → D2's completion
  rule (extend the same apt lines). Low risk, mechanical.
- **cargo-deny vs `LicenseRef-*` in exceptions** → D3's recorded
  fallback (global allow of the two Slint LicenseRefs, GPL stays
  per-crate).
- **NullPlatform trait surface heavier than expected** → D14's
  recorded degradation (coverage gap stated in the report; chunk 4
  covers behaviour at scale extremes).
- **Lint rule 3 false positives** → conservative regex bias (miss over
  false hit) + reviewer grep; design §14's retreat is a decision, not
  drift.
- **Font weight rendering** → static instances (D11) remove the
  variable-axis unknown; golden determinism is chunk 4's assertion.
- **Build-time cost everywhere** (slint + femtovg + winit in every
  workspace build, incl. the weekly mutation job) → Swatinem cache
  absorbs steady-state; the one-time cost is the accepted Q10
  consequence of ending the dependency-free stub era.
- **Behaviour change: bare+TTY `mode=desktop` now GUIs instead of
  REPL** (D4) — deliberate, recorded; reviewer doubles the release
  notes mention it (chunk 5).
- **register_font_from_memory absent** → D11's path-based fallback.

## 9. Verification of this round

- Every dispatch §2 "covers" bullet maps to a §3 decision or an
  explicit anti-scope entry (D15); every normative design section
  named for chunk 1 (§3, §6.1–6.2, §9.1, §1.1–1.2, Q7/Q10/Q12/Q13/
  Q18/Q22) is consumed or explicitly deferred to its owning chunk.
- One deviation from the design plan's letter, recorded with its
  reason: the fonts are **static instances** rather than the §3.6
  default proposal's "one variable-weight sans family" (slint 1.18.1's
  variable-font weight-axis behaviour is unverified by the stress
  round; golden determinism — the design's own decisive reason — is
  guaranteed by statics; D11). The dependency/theme/gate merge into
  one commit (§4 step 3) is a commit-sequencing choice the branch's
  own end-state rule forces, not a design deviation. Everything else
  follows the accepted design verbatim.
- The scope fence holds: nothing from design §2.2/§2.3 ships here
  beyond the §2.1-sanctioned plumbing (no visible surface beyond an
  empty dark window).
```

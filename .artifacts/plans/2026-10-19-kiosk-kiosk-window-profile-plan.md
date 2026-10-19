# [Plan] Kiosk Chunk 2 — kiosk window profile (TDD-first)

- **Date**: 2026-10-19
- **Kind**: stage-1 implementation plan (dispatch plan §3, chunk 2) for
  `feat/ui-kiosk-profile` — the `UiSettings.kiosk` flag and the
  mount-time window profile. TDD-first; no implementation in this
  commit.
- **Origin (normative, MD-7 rank)**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 **KDF-5**, §3 **K2**, §4–§6); the accepted design
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  (§3 Option A's window row — *"reuse the desktop `AppWindow` behind
  a kiosk profile first (fullscreen, kiosk scale preset)"* — and §6
  **Q5**, ACCEPTED); the dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1/§3 (MD-1: no round gate; MD-6: this path); ADR 0015 (the mount
  seam: app-side config handed over as typed `UiSettings`; the
  `i-slint-backend-testing`/null-platform harness conventions;
  decision 6's renderer/backend set — winit is the shipped backend).
  Deviations from prose are recorded inline with reasons; none
  touches an ACCEPTED ruling.
- **Status**: READY — every open question settled below (§3 decisions
  D1–D9); the slint 1.18.1 API question settled with checked-out
  source evidence (§2); symbols re-located at plan time (base
  `d098cc6` = `origin/main`, fetched 2026-10-19; chunk 1's gui verb
  landed there via PR rebase-merge, remote branch deleted).

## 1. What this chunk delivers

`UiSettings` gains `kiosk: bool` (default `false`); when set,
`mount()` applies the kiosk window profile — **fullscreen** — to the
shared desktop `AppWindow`. On the shipped winit backend, fullscreen
suppresses decorations **by construction** (§2 EV-3), so fullscreen
alone *is* the complete Q5 profile: the KDF-5 "if decorations cannot
be suppressed programmatically, fullscreen alone is acceptable"
fallback turns out to be lossless, not a compromise. The in-crate
mount tests assert the window property both ways on the null
platform. No consumer yet (K3's `run_kiosk` passes the flag), no
config key, no scale knob, no `.slint` diff.

**Q5's "scale preset" is already served by the existing mechanism**
(KDF-5): `[ui] scale` → `Theme.base-scale` at mount (`lib.rs:141-144`)
applies identically in kiosk mode — the kiosk deployment pins its
scale in config. This chunk adds no scale logic, exactly per K2's
non-goals.

**Non-goals** (other chunks' / out of scope): no consumer wiring,
`run_kiosk`, or routing (K3); no `[kiosk]`/`[ui]` config change (the
profile derives from the mode — KDF-5; `[ui]` stays `scale` +
`keymap` under `deny_unknown_fields`, `config.rs:258-267`); no
`.desktop`/docs (K4); no `no-frame` static binding (rejected, D3);
no renderer/backend selection (ADR 0015 decision 6 unchanged; the
spike rules).

### Seams re-located at plan time (base `d098cc6`)

| Symbol | Where (plan-day lines) | Role |
|---|---|---|
| `UiSettings` + `Default` | `crates/taskboard-ui-slint/src/lib.rs:52-72` | the struct to extend; manual `Default` (scale 1.0, standard preset) |
| `mount()` | `lib.rs:128-165` | the profile's application point; fonts → `AppWindow::new()` → `Theme.base-scale` push (141-144) → `adapter::install` (147) → handle → repaint driver → first refresh |
| `mount_tests` | `lib.rs:279-374` | `handover()` + `on_slint_thread` + the two scale tests — the exact pattern the kiosk assertions mirror (`mount_pushes_the_configured_scale_into_the_theme`, `mount_defaults_the_theme_scale_to_one`) |
| `test_platform.rs` | `src/test_platform.rs:18-57` (NullPlatform/NullWindowAdapter), `:73-103` (`on_slint_thread`) | the headless platform; `NullWindowAdapter` implements only `window`/`renderer`/`size` — `update_window_properties` is the trait's no-op default (EV-4) |
| `adapter::install` | `lib.rs:147-153` | untouched — the adapter never touches window-level state |
| `app-window.slint` Window root | `ui/app-window.slint:55-59` | `background`/`title`/`preferred-width`/`preferred-height` only; zero fullscreen/no-frame usage in the crate (grep-verified) |
| `Cargo.toml` | `crates/taskboard-ui-slint/Cargo.toml:21` | slint workspace dep + `unstable-fontique-011`; **no feature change** — `set_fullscreen` is un-gated core API (EV-1) |
| `resolve_ui_settings` | `crates/taskboard-app/src/ui.rs:134-158` | the **single exhaustive** `UiSettings` construction site (grep-verified; all others spread `..UiSettings::default()` — lib.rs:314/431/787, golden_visuals.rs:101, journeys uses `default()`) → D6's one-line compile fix |
| Harness mounts | `tests/journeys.rs:60-67`, `tests/golden_visuals.rs:95-107` | mount via the public `mount()` with defaults/spread — source-compatible with the new field, behavior unchanged (flag off) |

## 2. The slint 1.18.1 API verdict (checked-out sources, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`)

The chunk's central question — KDF-5's "verify the 1.18.1 API"
clause — settled with file-path evidence:

- **EV-1 — fullscreen is programmatic and public.**
  `slint::Window::set_fullscreen(bool)` / `is_fullscreen()`:
  `i-slint-core-1.18.1/api.rs:573-581` (public facade), impl
  `i-slint-core-1.18.1/window.rs:2426-2440`: sets the root
  `WindowItem.full_screen` property (`items.rs:1270`) then calls
  `WindowInner::update_window_properties()`. Not feature-gated;
  needs no manifest change. The compiler-side property is `in-out`
  (`i-slint-compiler-1.18.1/builtin_elements.rs:2109-2111`), which
  is why a runtime setter exists at all.
- **EV-2 — there is no public decoration API.** `no-frame` is a
  plain `in property <bool>` on the Window builtin element
  (`builtin_elements.rs:2132-2134`), backed by
  `WindowItem.no_frame` (`items.rs:1268`) — settable only as a
  declaration-time `.slint` binding. The public `slint::Window`
  method list (api.rs:499-838) contains no decoration setter.
- **EV-3 — winit suppresses decorations under fullscreen by
  construction.** The winit adapter's `update_window_properties`
  computes `set_decorations(!window_item.no_frame() ||
  winit_window_or_none.fullscreen().is_some())`
  (`i-slint-backend-winit-1.18.1/winitwindowadapter.rs:1881-1883`):
  **while fullscreen is active, decorations are always off**,
  regardless of `no-frame`. Fullscreen entry applies
  `Fullscreen::Borderless(None)` (`winitwindowadapter.rs:1959-1969`),
  and `set_fullscreen`/`set_decorations` both handle the pre-show
  window-attributes path (`:272-290`) — calling `set_fullscreen`
  in `mount()` (before `run()`'s `show()`) is fully supported.
  winit is the only backend compiled in (slint default features,
  ADR 0015 decision 6). **Verdict: `set_fullscreen(true)` alone
  delivers the complete kiosk profile.**
- **EV-4 — the null platform makes both-way harness assertions
  clean.** `WindowAdapter::update_window_properties` has a no-op
  default (`window.rs:151`); `NullWindowAdapter` overrides only
  `window`/`renderer`/`size`, so `set_fullscreen` is inert-safe
  headlessly while `is_fullscreen()` still reads the property back
  from the root `WindowItem` (`window.rs:2426-2432`). The generated
  `AppWindow::new()` attaches the root item tree at instantiation —
  `SharedGlobals::window_adapter_ref` → `set_component(&root_rc)`
  (verified in the generated
  `target/debug/build/taskboard-ui-slint-*/out/app-window.rs:18568`)
  — so the property is writable and readable inside `mount()`
  before any `show()`. No display, no event loop, no renderer
  involved.
- **EV-5 — upstream env default (observed, untouched).**
  `WindowItem::init` seeds `full_screen` from the presence of
  `SLINT_FULLSCREEN` (`items.rs:1282-1285`). Zero references to
  that variable anywhere in this repo (code, CI, docs —
  grep-verified): it is a pre-existing upstream OS escape hatch
  (the `SLINT_SCALE_FACTOR` pattern), identical on the desktop path
  before and after this chunk. D2's guard form keeps it that way.

## 3. Decisions (each open question settled)

### D1 — the `UiSettings` field: name, type, default, doc

```rust
/// The kiosk window profile (Q5): mount applies fullscreen to the
/// shared `AppWindow`. Derives from the run mode — never a config
/// key; the desktop hand-over leaves it `false`.
pub kiosk: bool,
```

`Default::default()` sets `kiosk: false` (appended to the manual
`Default` impl, `lib.rs:64-72`). Field order: appended last, after
`startup` — the struct is positional-only in exhaustive literals
(one site, D6), so ordering is a readability choice; the flag groups
naturally after the hand-over payloads.

Rejected: a `WindowProfile` enum (`Windowed | Kiosk`) — speculative
generality; one bool is the entire current vocabulary, K13's ambient
view is view-tree work, not a window mode, and KDF-5 names the field
`kiosk: bool` outright.

### D2 — application point and guard form in `mount()`

Immediately after the `Theme.base-scale` push (`lib.rs:141-144`),
before `adapter::install` — the two mount-time window-construction
profile steps sit together, and the adapter/repaint/refresh sequence
is untouched:

```rust
// The kiosk window profile (Q5): fullscreen on the shared desktop
// window. Slint 1.18.1 has no independent decoration control; on
// the winit backend fullscreen suppresses decorations by
// construction, so fullscreen alone is the complete profile.
if settings.kiosk {
    window.window().set_fullscreen(true);
}
```

(`ComponentHandle::window()` is already imported, `lib.rs:38`.)

Guard form (`if settings.kiosk { … }`), not unconditional
`set_fullscreen(settings.kiosk)`: with the flag off the mount path
executes **zero new statements** — the acceptance criterion's
"desktop path byte-identical" read literally. The unconditional form
would additionally *clear* the upstream `SLINT_FULLSCREEN` env
default on the desktop path (EV-5) — a behavior change outside this
chunk's mandate. `set_fullscreen(true)` after `AppWindow::new()`
wins over that env seed when kiosk is set (ordering: init runs
inside `new()`).

### D3 — decoration suppression: nothing to call, and no `no-frame` binding

No API exists (EV-2); none is needed (EV-3). Explicitly rejected: a
static `no-frame: true;` in `app-window.slint` — it cannot be
conditional on `UiSettings` (declaration-time binding), would strip
the frame from the **desktop** path too, and is redundant under
fullscreen on winit. The KDF-5 caveat clause is therefore triggered
in its milder form — "fullscreen alone" — and the caveat text for
ADR 0016 (chunk 3's doc) is settled verbatim in D4.

### D4 — the caveat text for ADR 0016 (handed to chunk 3)

> **Kiosk window profile (chunk 2).** The profile is
> `slint::Window::set_fullscreen(true)` applied at mount when
> `UiSettings.kiosk` is set. Slint 1.18.1 exposes no public runtime
> API for decoration control — `no-frame` is a declaration-time
> `.slint` binding — but on the winit backend, the only backend
> compiled into the release (ADR 0015 decision 6), decorations are
> suppressed by construction while fullscreen is active
> (`set_decorations(!no_frame || fullscreen)`,
> `i-slint-backend-winit` 1.18.1 `winitwindowadapter.rs`). The
> profile is complete as shipped. A hypothetical windowed-but-
> undecorated variant would need a static `no-frame` binding (a
> second window declaration) or a backend change; fullscreen
> semantics under any future backend switch (`renderer-software`/
> `backend-fb`, the spike's question) are the spike's to verify
> on-device.

### D5 — harness assertion mechanics and the two tests

Both tests live in `mount_tests` (`lib.rs:279-374`) — the mount seam
is the lowest layer that owns the behavior; the journeys/goldens
harnesses test routing only and mount with defaults (flag off, no
diff). Mechanics per EV-4: `on_slint_thread` + `handover()` +
property read-back — no display, no event loop.

1. `mount_applies_the_kiosk_fullscreen_profile` (red first) —
   `mount(..., UiSettings { kiosk: true, ..UiSettings::default() })`
   → `assert!(handle.window.window().is_fullscreen())`.
2. `mount_without_the_kiosk_flag_stays_windowed` (red first) —
   `mount(..., UiSettings::default())` →
   `assert!(!handle.window.window().is_fullscreen())`. This one test
   covers both the field's default and the guard's no-op (a separate
   `default().kiosk == false` pin would duplicate a behavior).

No defensive `SLINT_FULLSCREEN` env handling in the tests: CI never
sets it (EV-5, grep-verified); a dev machine that exports it gets a
loud, explained failure — honest beats machinery.

### D6 — the one app-crate line (recorded deviation from K2's file list)

K2's file list names only `crates/taskboard-ui-slint`, but
`resolve_ui_settings` (`crates/taskboard-app/src/ui.rs:150-156`)
constructs `UiSettings` **exhaustively** — a new field breaks the
build. The single exhaustive site (grep-verified, §1 table). Fix:

```rust
kiosk: false, // the desktop hand-over; the kiosk appliance (K3) sets this
```

This is a compile necessity, not a consumer: the value is `false`
on every path that exists today, so the desktop behaviour is
unchanged. Recorded as a deviation-in-prose with reason (MD-7
allows this; no ACCEPTED ruling touched) so stage 3 does not read
it as scope creep.

### D7 — no manifest, feature, or `.slint` changes

`set_fullscreen` is un-gated core API (EV-1): no `Cargo.toml` diff,
no `slint` feature additions, no `app-window.slint` diff — the
token/theme lint gates pass untouched by absence (the repo lint
governs `.slint` content; there is none new). The
`i-slint-backend-testing` exact-pin and `tests/pin_guard.rs` are
unaffected (no version bump rides this chunk).

### D8 — bookkeeping

One CHANGELOG `Unreleased` line (feat: kiosk window profile flag —
`UiSettings.kiosk` mounts fullscreen; no consumer yet). No tracker
edits (MD-4 — stage 3 posts the one #237 annotation when the PR
opens). No README/docs (K4 owns launch docs; ADR 0016 is K3's).

### D9 — acceptance mapping (dispatch §3, chunk 2)

| Acceptance clause | How this plan meets it |
|---|---|
| flag defaults off — desktop path byte-identical | D1 default `false` + D2 guard form (zero new statements when off) + D6 `kiosk: false` |
| fullscreen + decoration suppression per the 1.18.1 API, evidence recorded | §2 EV-1…EV-5 (checked-out source paths) |
| caveat for ADR 0016 if fullscreen-only | D4 verbatim text (the fallback is lossless on winit — EV-3) |
| harness assertions both ways | D5 |
| token/theme lint gates unchanged | D7 (no `.slint` diff) |
| no consumer / no config key / no scale knob | §1 non-goals, D1 doc, D6 comment |

## 4. TDD order (red → green; house rules throughout)

Typed/structural outcomes only (`is_fullscreen()` is a typed bool —
no text assertions anywhere by construction); no sleeps, no timers —
nothing here is time-dependent; one behavior, one test, lowest layer
(§3 D5).

1. **Red 1**: `mount_applies_the_kiosk_fullscreen_profile` — fails
   to compile at first (no `kiosk` field), the house red.
2. **Red 2**: `mount_without_the_kiosk_flag_stays_windowed`.
3. **Green**: D1 field + Default arm → D2 mount application → D6
   app-side `kiosk: false` (workspace builds again) → D8 CHANGELOG.
4. Full gate suite (§5).

## 5. Gates & done

Full AGENTS §10 suite, green: `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`cargo nextest run --workspace`; `cargo test --doc --workspace`;
`cargo deny check`; the CI-identical three-command llvm-cov sequence
(+ quarantine + report + `rm -rf target/llvm-cov-target`). Expected
clean by construction: the diff adds one bool field, one guarded
setter call, two tests, one app-crate literal field, one CHANGELOG
line. Commits standalone, rebase-merge-hygienic
(`docs(plan)` already on the branch from stage 1; stage 2 lands
`feat(ui): kiosk window profile flag`-shaped work as one
self-contained commit).

## 6. Risks

- **Property read-back vs component attachment ordering** — settled
  by evidence against the actual generated code (EV-4); residual
  risk ≈ none. If stage 2 nevertheless observes `is_fullscreen() ==
  false` after a `true` set, that is a bug to investigate (AGENTS §5:
  a failing new test implicates production and test equally), not a
  test to weaken.
- **`SLINT_FULLSCREEN` on a dev machine** flips test 2 (EV-5) —
  loud, explained, CI-clean; accepted (D5).
- **Backend coupling of the decorations fact** — EV-3 is
  winit-specific; any backend change is the spike's, and D4's caveat
  text says so explicitly.
- **Diff touches two crates** (D6) — one line in `taskboard-app`;
  called out here so stage 3 treats it as planned, not as finding.

## 7. Estimated diff

`lib.rs` +~70 (field + Default arm + doc, 4-line guarded
application + comment, two tests ≈ 50 lines incl. doc comments);
`taskboard-app/src/ui.rs` +1; `CHANGELOG.md` +1. **Total ≈ +72/−0
across 3 files**, single implementation commit after this plan's
`docs(plan)` commit.

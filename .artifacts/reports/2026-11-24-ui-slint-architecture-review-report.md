# taskboard-ui-slint Architecture Review — Purity, Widget Policy, Keyboard, Crate Shape

- **Date**: 2026-11-24
- **Kind**: read-only architecture review of
  `crates/taskboard-ui-slint/`; no production code written. Backlog
  updates filed per AGENTS §7's standing authorization (numbers in
  §9).
- **Mandate**: audit (1) "dumb presentation client" compliance and
  architectural health, (2) the widget-reusability policy
  (`docs/widget_interactions.org`), (3) keyboard-input handling
  quality, (4) whether the crate should be split — with file/line
  evidence for every finding.
- **Inputs**: full sweep of `src/` (82 328 lines incl. in-file tests;
  ≈45 k production) and `ui/` (22 452 lines of `.slint` across
  widgets/ 5 888, compositions/ 6 990, views/ 6 021, top-level
  3 553), `tests/` (53 374 lines, 31 binaries), `Cargo.toml`,
  `build.rs`, the workspace manifest, and the context docs
  (`AGENTS.md`, `docs/architecture.org`,
  `docs/coding_guidelines.org`, `docs/widget_interactions.org`,
  `docs/ux-surface-census.org`). Line numbers are from the working
  tree at review time (2026-11-24, `main` @ df5ae1a0).
- **Method**: three parallel evidence sweeps (purity/IO/async;
  widget-policy; keyboard trace) over `#[cfg(test)]`-bounded
  production spans, plus direct verification of every headline
  citation; compile-time facts from
  `.artifacts/reports/2026-11-19-build-test-runtime-analysis-report.md`.

---

## 1. Executive verdict

The crate is **architecturally healthy at its seams and over-concentrated
in its middle**. The "dumb presentation client" rule holds everywhere it
was auditable: no IO, no env reads, no locks, no tokio runtime coupling,
zero `async fn`, 100 % non-blocking `try_send` dispatch, wall-clock reads
seamed and cadence-bounded, and domain semantics (overdue/due-soon,
today sections, palette ranking, effort parsing) delegated to
`taskboard-domain` rather than re-implemented. The keyboard stack is the
quiet success of the round-9 waves: one capture point, one pure resolver,
masking as keymap data, hosts handler-free, and a new action is a
3-file mechanical diff pinned by the compiler. What remains is
**concentration debt, not boundary debt**: `adapter.rs` at ≈18.8 k
production lines is the crate's god-file and its one real refactoring
priority; nine hand-rolled activation blocks and a handful of hand
backdrops in views/compositions are review-blocker-class residue the
round-9 base widgets already absorb; two duplicated order/midpoint
policies (UI vs domain) are the only business-logic drift risks found.
The crate should **not** be split: the dependency graph is already
minimal, the dominant compile cost is test-binary linking (not crate
count), and a `taskboard-ui-core` extraction would serve exactly one
consumer today.

## 2. Compliance audit — what is clean (evidence-backed)

Every claim below was verified by sweep + spot-check against the
production spans (production/test boundaries established per file via
`#[cfg(test)]` gates).

| Rule | Verdict | Key evidence |
|---|---|---|
| No IO crates / env reads | **CLEAN** | Zero `std::fs/net/process/env`, `reqwest`, `sqlx`, `tokio::fs/net/io` in `src/` production. Config arrives as typed hand-over ("the UI crate reads no configuration itself — IO-free by design", `src/lib.rs:66-69`); env-override facts ride `requests.rs:298` (`env_overridden: BTreeSet<String>`) sampled app-side. Fonts are `include_bytes!` (`src/fonts.rs:26-36`). The only `std::env` is build-script (`build.rs:22`) — sanctioned. |
| No async-runtime coupling | **CLEAN** | Zero `tokio::spawn`/`select!`/`interval`/`sleep`/`Runtime::new`/`block_on`; zero `async fn` in the crate. Production tokio inventory is channel halves only: `mpsc::Sender<StateCommand>` + `mpsc::Sender<UiRequest>` (`src/adapter.rs:443-455`), `broadcast::Receiver<EngineSignal>/<SystemEvent>` (`src/lib.rs:281-284`), oneshot reply halves parked in VMs (`src/adapter.rs:8309-8313`), `watch::Receiver` for delivery-health/files-sync (`src/lib.rs:282-283`). `tokio::time::Instant` appears only as a clock type for `AutoListBackoff` (`src/adapter.rs:8231-8264`, chosen for `pause`/`advance` testability). All dwell timers are `slint::Timer` (`src/lib.rs:409-435`). |
| Non-blocking dispatch | **CLEAN** | No `.send().await`/`blocking_send` anywhere; every dispatch is `try_send` warn-and-drop (convention documented at `src/adapter.rs:1682`, `src/requests.rs:6`); replies settle via per-tick `try_recv` drains (`src/adapter.rs:9549-9560`). |
| No shared-mutable-state locks | **CLEAN** | Zero `Mutex`/`RwLock` in production; the one hit is a test-only capturing tracing layer (`src/lib.rs:2387-2451`, inside `mod first_frame_tests`). Adapter mutability is `Rc<RefCell<Adapter>>` on the slint thread with a documented re-entrancy law ("the callback fires OUTSIDE the adapter borrow", `src/lib.rs:666-668`). |
| No business logic in the UI | **LARGELY CLEAN** — three gray zones, §3.3 | Overdue/due-soon id sets come from `taskboard_domain::overdue_tasks`/`due_soon` (`src/presentation.rs:306-320`, comment "the overdue/due-soon semantics exist only there"); today sections from `taskboard_domain::days::today_sections` (`src/presentation.rs:3148`, `src/view_state.rs:2535`); palette ranking is `taskboard_domain::find` (`src/models.rs:561` → `taskboard-domain/src/find.rs:92`); effort text parsing pins "one predicate, two fronts" against `Effort::parse_text_form` (`src/adapter.rs:18838-18852`). Stack sorts materialize the engine's canonical `(order, id)` key (`src/presentation.rs:1329,2677`) — mechanical, not re-owned. |
| Clock discipline | **CLEAN with one wrinkle** | `presentation.rs` production (1–3425) is wall-clock-free — every helper takes `now` as a parameter (`:306`, `:685`, `:893`, `:2326`). Production reads concentrate in the adapter/mount layer: cadence (`src/lib.rs:693` "One clock read per tick feeds both boundaries"), the frozen-clock seam `frozen_now.unwrap_or_else(chrono::Utc::now)` (`src/adapter.rs:2707`), and interaction-time captures that honor the seam (`src/adapter.rs:3008` + comment, #921). The wrinkle: **two reads in one derive pass** (`src/adapter.rs:2977` and `:3024`, the latter literally named `now2`) — §3.3 F-8. |
| State mutation confined to view state | **CLEAN** | `ViewState` holds selection/cursors/surfaces only, every cursor id-keyed and clamped against live rosters each reconcile (`src/view_state.rs:744-746`); settings-derived values deliberately re-derived per refresh, "never hand-over state" (`src/adapter.rs:336-351`); each cached surface (settings snapshot, remotes, delivery health) has exactly one update path — no unchecked drift channel found. |
| Widget layers and tone gates | **CLEAN** | No literal colors in `ui/` outside `theme.slint` (token lint + sweep agree); `models.rs` only "ferries typed data" between pure presentation enums and generated slint enums (`src/models.rs:5-10`); no dead widgets (every `ui/widgets/*.slint` has ≥1 import). |

## 3. Architecture & Refactoring Findings

Severity classes: **High** = actively taxes every change; **Medium** =
drift/correctness risk or recurring friction; **Low** = hygiene.

### High

**F-1 — `adapter.rs` is a god-file (≈18.8 k production lines, one
struct, five continuation impls).**
`src/adapter.rs` is 18 961 lines total; test material is only the
file tail (`:18837`→EOF) plus small interleaved `#[cfg(test)]` seams
(`:385`, `:866`, `:2552`, `:3052`, `:3716-3799`, `:4454-4538`,
`:12323`) — ≈18.8 k lines are production. Contents: the `Adapter`
struct (`:330-810`, ~60 fields), `impl Adapter` at `:883` plus four
continuation impls (`:8672`, `:13876`, `:13984`, `:14910`, `:16564`),
the settings VM (`:8290-8383`), the refresh/derive pass
(`:2900-3800`), the `(Flow, UiEffect)` interpretation table
(`:4042-6600`), request-lane drains (`:9540-9800`), notice machinery
(`:8684-8770`), and the slint-callback installation region
(~11 800–16 500, an `install` entry at `:12127` plus twelve
`install_*_seams` free functions — 356 `.on_*` registrations total).
Navigation is by impl-block position; there are two section banners in
19 k lines. Two single functions are ≈1 k+ lines each:
`interpret` (`:4703-5984`, ≈1 281 lines) and `apply_effect`
(`:7019-8114`, ≈1 096 lines).
*Why it matters*: every UI feature lands in this one file; the
`install_*_seams` free functions already sketch the natural family
boundaries but live in the same file, so file-level incrementality,
review locality, and onboarding all suffer.
*Refactor shape*: convert `adapter.rs` into an `adapter/` directory
module tree preserving privacy (`adapter/mod.rs` keeps `Adapter` +
`install`; `settings_vm.rs`, `interpret.rs` (the flow table),
`refresh.rs` (the derive pass), `seams/{view,recur,habit,drag,export,
status_panel,...}.rs` (the existing install families), `replies.rs`
(the drains), `notices.rs`). Pure mechanical moves — no behavior
change, no API change outside the crate; the crate boundary and the
`UiHandle` hand-over stay untouched. Pair with moving the tail test
mods to `#[cfg(test)]`-gated sibling files.

### Medium

**F-2 — Post-round-9 widget-policy residue: hand-rolled activation
blocks, backdrops outside the hosts, and twin components.**
The round-9 centralization landed the base-widget vocabulary
(`docs/widget_interactions.org`), and the sweep confirms the hosts,
menus, flyouts, and all seven L3 views carry **zero** key handlers and
correctly import the base widgets. What remains is enumerated in the
§5 audit table; the review-blocker-class items are: nine
FocusScope+TouchArea+Return/Space activation blocks in
views/compositions (`ui/views/plan-next-day-view.slint:112-139,169-200,
204-235,239-270,316-336`; `ui/compositions/track-entries.slint:345-395,
455-515`; `ui/compositions/tracked-today.slint:85-129`;
`ui/compositions/focus-overlay.slint:110-147`) — four of which lack the
modifier guards the base widgets carry, so **Ctrl+Return activates
there but is rejected on `TextButton`/`ActivationStop`** (behavior
divergence, verified at `plan-next-day-view.slint:176-179` vs
`ui/widgets/text-button.slint:154-167`); hand light-dismiss backdrops
outside `AnchoredOverlayHost`/`ModalDialogHost`
(`ui/app-window.slint:1697`, `:1795`, `:2105`, and the four-strip band
`:1134-1184`); the `VerbButton` component exported from a view
(`ui/views/review-walk-overlay.slint:124` — `ui/widgets/text-button.slint`
already documents it as absorbed); and the twin dropdown pickers
(`ui/views/settings-view.slint:507-545` vs `:556-594`, ≈39 verbatim
lines each differing only in copy global, consumer kind, and callback
name). Filed as backlog issue (§9).

**F-3 — Two order/midpoint policies: UI-minted move keys vs the
domain's `floor_midpoint_order`.**
`src/view_state.rs:806-955` mints order keys UI-side: `append_order`
(max+1, `:806-812`), `slot_order` (midpoint `a + (b - a) / 2`,
`:822-844`), `multi_move_commands` (relative-order-preserving batch
with gap-splice fallback, `:852-941`), `append_stack_order`
(`:944-955`). The domain owns its own midpoint policy —
`floor_midpoint_order` (`crates/taskboard-domain/src/command.rs:749`)
— which the UI does not call. Two policies now exist for "the order
value a move command carries"; the UI's doc comment argues tie
determinism via the engine's `(order, id)` sort, but nothing pins the
two together, and a future domain-side change to midpoint semantics
cannot see the UI's copy. *Fix*: re-home the pure command-parameter
functions (`slot_order`/`multi_move_commands`/`append_order`) into
`taskboard-domain` (they are pure `order`-arithmetic over slices —
no slint, no IO), or at minimum add a parity pin test in the same
spirit as `effort_parity_tests`. Filed as backlog issue (§9).

**F-4 — `lib.rs` layout: production code sandwiched between test
modules.**
`src/lib.rs` (4 722 lines) runs production 1–762, then
`mod second_tick_tests` (`:763-814`), then **production again**
(`run_alarm_pass` `:815`, `install_alarm_callbacks` `:890`,
`impl UiHandle` `:939-1152`, `run` `:1153`, `quit` `:1200`), then five
more test mods to EOF (`:1206`, `:2383`, `:2470`, `:2497`, `:2751`,
`:4391`; ~3 500 test lines). `quit()`/`run()` — the crate's public
lifecycle — sit between test modules. *Fix*: move the six in-file test
mods to `src/*_tests.rs` siblings (the `flow_table_tests.rs` pattern
already in use) and extract the repaint driver/second-tick and alarm
pass into `timers.rs`/`alarms.rs`.

**F-5 — `settings_form.rs` validation mirror without a shared
contract.**
`src/settings_form.rs:150-170` re-implements `is_kebab` +
`url_host_ok` (scheme http/https, non-empty host, no userinfo),
self-described as "the config layer's `reqwest_host_ok` mirror"
(`:161`); parity intent is comment-only (`:171-174`). The authoritative
mint stays app-side by design (the Save ceremony re-validates), so this
is advisory pre-validation — but the two predicates can drift silently.
*Fix direction*: a parity pin test (mirroring the two implementations
against a shared fixture table, cross-crate via a workspace test) or a
shared pure helper in a crate both may depend on. Lower priority than
F-3 because the failure mode is a stale *advisory* hint, never a bad
write.

### Low

**F-6 — Duplicate clock read within one derive pass.**
`src/adapter.rs:2977` captures `Utc::now()` for the seconds-surfaces
gate; `:3024` captures `Utc::now()` again (`now2`) for the due-editor
push within the same pass. Per coding guidelines §2.2 the caller
captures once at the invocation boundary; a second boundary crossing
mid-pass can derive one frame's content at two instants. Fix: capture
once at pass head and thread it (the pass already threads `DayCtx`).

**F-7 — Advisory UI-side date gate in the habit pause seam.**
`src/adapter.rs:13227-13236` compares the draft date against
`a.calendar().today(a.day_ctx().now)` and suppresses dispatch; the
comment correctly keeps the planner authoritative (`InvalidHabitPause`
re-validates across rollover). Acceptable per-keystroke feedback;
recorded so the pattern doesn't spread.

**F-8 — Hand-maintained probe registry in `build.rs`.**
`build.rs:34-63` lists 30 probes by name; a new probe file requires a
`build.rs` edit and forgetting one fails only at the missing binary.
Sibling of #854's hand-maintained-list law. A `tests/*.probe.slint`
glob (or a build-time dir read) would remove the sync burden at the
cost of build-script determinism-by-enumeration — record, don't push.

**F-9 — Token-lint edge cases.**
`drop-shadow-blur: 12px` ×3 (`ui/compositions/focus-overlay.slint:61`,
`ui/compositions/search-palette.slint:48`,
`ui/compositions/recur-editor.slint:99`), bare `2px * Theme.base-scale`
hairlines ×5 (`ui/compositions/board-layout.slint:603,658,705`,
`ui/compositions/habits.slint:438,466`) where
`ui/views/habit-history-view.slint:119` uses `Theme.hairline-width` for
the same job, and ~40 raw `Npx * base-scale` panel widths (13 in
`ui/views/review-view.slint` alone). The token lint passes because
scaled expressions are legal; a `shadow-blur`/spacing-token pass would
cover the residue.

**F-10 — `UiHandle`'s test-seam public surface.**
~10 public `drain_*`/`observe_*`/`quit_requests` methods
(`src/lib.rs:965-1130`) exist for the component journeys ("the
established `UiHandle`-seam pattern", per their docs). Deliberate and
documented; noted because the public API is test-shaped — if the
surface grows further, a `#[doc(hidden)]` test-support extension trait
would keep the production impl block clean.

## 4. Keyboard-input handling — verdict: the model holds; residue is at the leaves

Evaluated against `docs/widget_interactions.org` ("hosts have no key
handlers; leaves own Return/Space; dropdown owns ladder keys").

**The trace (one key press, winit → `StateCommand`):**
`ui/app-window.slint:616-619` — the root `key-root` FocusScope's
`capture-key-pressed` tunnels the event to one interception point
(the whole view tree lives inside it; `reject` preserves Tab
traversal) → `window.on_intercept_key` (`src/adapter.rs:12426-12445`,
walk-reentrancy guard) → pure stroke mint (`Stroke::from_event`,
`src/keymap/stroke.rs:241-279`) → `Adapter::intercept`
(`src/adapter.rs:1189-1333`): insert-mode pulls from `EditingFacts`
(`:1201-1215`), printable monopoly decline (`:1223-1234` →
`src/keyscope.rs:222-235`), editor-scope arm (`:1402-1431` →
`keyscope::editor_table` `:453-485`), special scope arms
(`:1438-1540`: dropdown ladder decline, panel Escape dismissals, due
Down arm), chain build (`keyscope::scope_stack`, `src/keyscope.rs:93-175`),
activation reservation (`keyscope.rs:338-349`), pure resolve
(`KeymapResolver::feed`, `src/resolver.rs:60-105` over
`Keymap::resolve`, `src/keymap/mod.rs:99-137`) → `Adapter::route`
(`src/adapter.rs:1689-2012`) → `ActionRouter::dispatch`
(`src/router.rs:1534+`, exhaustive match — "adding an action without a
routing decision is a compile error", `:1524-1525`) → `try_send`
command or `UiEffect` → interpretation table → `view_state` mutation.

**The evidence that lower levels need minimal extra code:**
- **Zero key handlers** in both overlay hosts, the menu shell/rows/
  flyouts, the date picker, the due editor, the labels/recur/timer
  editors, and all seven L3 views — `grep key-pressed ui/` hits only
  sanctioned leaves (button/checkbox/chip/activation-stop/slider/
  multi-line-editor/dropdown ladders) plus the residues in F-2 and the
  alarm window below.
- **Masking is keymap data**: `Keymap::visible_chain`
  (`src/keymap/mod.rs:72-81`) truncates the context chain at the first
  masking context; hosts own only walk arming
  (`ui/mask-walk-facts.slint:12-22`); modal Escape resolves through
  the modal's own preset context, never a host handler.
- **Adding an action is a 3-file mechanical diff**: `ActionId` variant +
  id string + `REGISTRY` row (`src/actions.rs:302`, `:828`,
  `:1496-1505`; menu rows and kbd hints derive from the registry), a
  preset row per preset (`src/keymap/presets.rs`), and the
  compile-pinned `router.rs` arm — plus an adapter interpretation arm
  only when a new `UiEffect` is coined. No keymap-module loop to close,
  no view edits.
- **Keymap-as-data quality**: presets are `&[(ActionId, ContextId,
  &[&str])]` const tables composed per context (`presets.rs:43-89`);
  override merge stages the whole pass and emits typed findings with
  drop-both conflict semantics (`keymap/mod.rs:311-407`); a hostile
  keymap can never block UI start (Q15, `mod.rs:240-243`); preset
  intra-context conflict-freedom is contract-tested (`mod.rs:1274-1284`,
  `:1431-1449`). Chords/prefixes/vi modes are ordinary data: `pending`
  prefix state machine (`resolver.rs:36-112`), chain-flip reset (T20),
  `C-x C-c` emacs exit, vi `g`/`Z`/`Ctrl+w` families as plain context
  rows (`presets.rs:~1560-1660`), `ViInsert` shadowing without masking.
  Zero TODO/FIXME in keymap/ — recorded limitations only (#855 glyph
  seam, AltGr, logical-vs-visual editor motions).

**Residual smells (ordered):**
1. **Alarm-window duplicated ladder** — `ui/alarm-window.slint:69-91`
   hardcodes Up/Down/d/s/c/Escape while the same grammar lives as
   keymap rows (`presets.rs:262-270`, totality-pinned by
   `keymap/mod.rs:689-715`). Structurally forced (a separate top-level
   `Window` is invisible to the AppWindow capture; documented
   `src/lib.rs:494-504`) and both fronts converge on
   `presentation::alarm_command` — but it remains a two-truth-source
   surface. Any alarm-window key change must touch both.
2. **The nine view/composition activation blocks** (F-2) — four lack
   modifier guards; all predate/ignore `TextButton`/`IconButton`.
3. **Hardcoded action-layer key vocabularies** — slider
   arrows/Home/End (`ui/widgets/slider.slint:73-101`), the switcher
   flyout ladder in Rust (`src/adapter.rs:14915-14967`), the
   notification verb keys (`src/keyscope.rs:191-208`), the three
   Escape dismissal predicates (`keyscope.rs:288-312`, documented
   shadowing rationale), the editor command table (`keyscope.rs:453-485`,
   deliberately a second vocabulary). Each is documented and
   decline-arm-shaped rather than a routing special case; they matter
   for #1280's coverage audit, which should sweep them.

**Verdict**: the keyboard architecture is implemented well enough that
views, compositions, and hosts need zero key code — the context chain
*is* the view router (no match-on-view-id key dispatch exists). The
residue is leaf-level (F-2's blocks) and two structurally-forced
duplication pockets (alarm window, editor table), both documented.

## 5. Widget Policy Audit

Policy: all widgets live in `ui/widgets/` and are imported; inline
ad-hoc widgets in views/compositions are prohibited. The sweep covered
every `TouchArea` (37), every `FocusScope` (25), every component
declaration in views/compositions, and widget import counts.

**Context**: the round-9 centralization (2026-11-18..20) landed 36
widget files; every one has ≥1 live consumer (no dead widgets), the
hosts/menus/flyouts are mounted per the mount checklist, and the
healthy core is heavily imported (text-field ×26, icon-button ×23,
tooltip ×17, text-button ×10, badge ×9). The findings below are the
*residue*.

| # | Inline / duplicated widget | Locations (file:line) | Proposed reusable abstraction | Class |
|---|---|---|---|---|
| 1 | Hand-rolled glyph "pressable" blocks (↑ ↓ ✕: FocusScope Return/Space w/o guards + TouchArea + hover + icon) | `views/plan-next-day-view.slint:169-200, 204-235, 239-270` | `IconButton` (exists — carries the full grammar incl. guards) | review-blocker |
| 2 | Hand-rolled card activation blocks (FocusScope+TouchArea+a11y) | `views/plan-next-day-view.slint:112-139, 316-336` | ListRow-overlay convention (see #9) | review-blocker |
| 3 | Hand-rolled text button ("Stop session": FocusScope+TouchArea+guards+border) | `compositions/focus-overlay.slint:110-147` | `TextButton` (exists; comment at `:100-103` admits pre-primitive residue) | review-blocker |
| 4 | Hand-rolled add-row affordance ("Add entry" hover row) | `compositions/track-entries.slint:455-515` | `GhostOpener` (exists — imported by board-layout but not here) or `TextButton` | review-blocker |
| 5 | Hand-rolled entry-row activation (×2, no modifier guards) | `compositions/track-entries.slint:345-395`; `compositions/tracked-today.slint:85-129` | pressable-row overlay (see #9) | review-blocker |
| 6 | Hand-rolled icon button duplicate ("Stop tracking") — its own comment calls it "the last hand-drawn duplicate in the status bar" | `compositions/status-bar.slint:424-455` | `IconButton` (exists) | review-blocker |
| 7 | Hand-rolled reflection flag chip (caption + click toggle) | `views/review-view.slint:582-606` | `CheckBox` / `ActionChip` (exist) | review-blocker |
| 8 | MenuRow-shaped "Back" row (hover + a11y + click, no Return/Space — inconsistent with the other activation blocks) | `compositions/action-menu.slint:184-206` | `MenuRow` (exists) | review-blocker |
| 9 | Hand-rolled interactive-row grammar (hover bg + focus ring + Return/Space + TouchArea + tooltip), 8 shapes | `tracked-today.slint:85-129`, `track-entries.slint:345-395`, `status-bar.slint:160-213`, `rule-list.slint:117-128`, `labels-overlay.slint:443-515`, `plan-next-day-view.slint:112-139` | a shared pressable-row overlay host (ListRow stays render-only per Rule E; the consumer overlay is currently re-invented per site — one `PressableRow` or a documented overlay convention) | refactor |
| 10 | Hand light-dismiss backdrops outside the hosts | `app-window.slint:1697` (menu), `:1795` (undo picker), `:2105` (focus overlay), `:1134-1184` (4-strip band over a live rect) | `AnchoredOverlayHost` light-dismiss / `ModalDialogHost` scrim (the undo-picker and focus-overlay panels are dialog-shaped) | review-blocker |
| 11 | Twin dropdown pickers (≈39 verbatim lines: facts mirroring + tooltip wiring; differ in copy global, consumer kind, callback) | `views/settings-view.slint:507-545` (RemotePicker) vs `:556-594` (DeckPublishPicker) | one parameterized component (copy + `DropdownConsumer` + callback as properties) or push the facts-mirroring into `Dropdown` | refactor |
| 12 | `VerbButton` — a general-purpose button exported from a view | `views/review-walk-overlay.slint:124` (12 in-file uses + probes) | pure `TextButton`-compact wrapper; retire or relocate (text-button.slint already records it as absorbed) | refactor |
| 13 | Settings dot trio re-rolling `StatusDot`'s shape | `views/settings-view.slint:355-365` (RowDot), `:371-380` (StatusDot — shadows the widget name), `:386-398` (BusyDot) | adopt `widgets/status-dot.slint` + a `busy` knob | refactor |
| 14 | `LabeledField` ×4 + `GeneralField` ×12 (label + InfoTip + field + error row) — general field-row family living in a view | `views/settings-view.slint:409`, `:601` | promote toward `widgets/` (overlaps `TippedField`; form-row variant already homed at #1096) | refactor |
| 15 | Restore-section twins (armed strip + deleted-rows hover list) | `compositions/track-entries.slint:528-596` vs `compositions/labels-overlay.slint:~370-439` | `ConfirmStrip` (exists, imported by both files' neighbours) + ListRow | refactor |
| 16 | Switcher segment grammar twins (title + hover + click + right-click context) | `compositions/board-bar.slint:300-370` vs `compositions/board-layout.slint:820-855` | `SegmentButton` (SegmentedControl lacks context/hint knobs; third-consumer rule satisfied) | refactor |
| 17 | Scroll-indicator twins (verbatim) | `compositions/action-menu.slint:282-289` vs `views/settings-view.slint:1965-1983` | `ScrollBar` widget (settings-view `:1960-1963` records "revisit at the third consumer" — now satisfied) | refactor |
| 18 | Review section skeleton ×3 + `StatCell` ×10 + 5 hand heading rows + 13 raw px widths | `views/review-view.slint:157-191, 199-231, 239-281`, `:139`, headings `:163-166, 205-208, 245-248, 405-407, 633-635` | parameterize sections in-file; adopt `SectionHeader` (timers/habits already do); tokenize widths (F-9) | refactor |
| 19 | Caption + `Badge "!"` sidecar chip ×3 | `compositions/status-bar.slint:370-389, 396-408, 410-422` | display-only chip variant (ActionChip is clickable; these are static) | minor |
| 20 | Color swatch button twins | `app-window.slint:1621-1633` vs `compositions/labels-overlay.slint:278-289` | `SwatchButton` (or an ActionChip `has-swatch` reuse — the knob exists) | minor |
| 21 | Settings private dot/badge + census "caption+value header" twins | `compositions/tracked-today.slint:43-79` vs `compositions/track-entries.slint:315-332` | `LabeledValueRow` (mono-value knob exists) | minor |

Not repeated here (already homed): labels roster rows → #1127;
form-row variant → #1096; rule-list hint caption → #1121;
DropdownTone → #1126; ActionMenu hand mount → #1125.

## 6. Crate Splitting Analysis

**Current shape.** One crate, `taskboard-ui-slint`, AGPL-3.0-only, the
only workspace member `taskboard-app` depends on. Direct deps:
`taskboard-domain`, `arc-swap`, `chrono`, `thiserror`, `slint`
(+`slint-build`), `tokio` (channel halves only), `tracing`, `uuid`,
`serde` (`crates/taskboard-ui-slint/Cargo.toml:11-45`) — no IO crates;
dev-deps carry the testing backend and harness. Internal layering is
already a purity gradient: pure Rust kernel with no slint types
(`view_state`, `router`, `surfaces`, `level_state`, `shell`, `keymap/`,
`resolver`, `keyscope`, `actions`, `menu`, `due_edit`,
`settings_form`, `presentation`, `requests` payloads, `geometry`,
`widths`, `responsive`, `scale`) → slint-boundary layer (`models.rs`
model assembly, `adapter.rs` glue, `lib.rs` mount) → markup (`ui/`).

**Size & cost data.** Production ≈45 k Rust lines + 22.5 k `.slint`
lines in one codegen unit. Measured compile profile
(2026-11-19 build report): ui-slint lib dev build **128 s**, as test
target **232 s**, build script 17.7 s; the slint runtime is compiled
once but **linked 31 times** (once per test binary, 20–71 s each) —
integration-test binaries are 70 % of the workspace's 2 104 s
incremental `--all-targets` CPU. The named structural levers were
linker choice and test-binary consolidation — neither involves crate
boundaries.

**Option A — keep the single crate (recommended).**
The crate boundary *is* the framework-swap seam (ADR 0015), and it is
already exercised by a second frontend (the web dashboard) which
consumes state through the app layer without any of this crate's
code. The dependency set is minimal and rule-clean. Splitting Rust
modules into a sibling crate would not reduce the dominant cost (the
slint codegen compiles `ui/` as one unit; test binaries would still
link both crates), would add two-crate ceremony to every internal
change (the type-driven policy explicitly discounts API churn as a
decision variable, but ceremony still taxes velocity), and its one
real benefit — a reusable "presentation kernel" — has exactly one
consumer today. Speculative generality is the anti-goal both in
AGENTS §2 and in the widget policy's own knob-consumers rule.

**Option B — `taskboard-ui-core` (pure Rust kernel) +
`taskboard-ui-slint` (glue + markup).**
Pros: sharpens the swap seam at the type level; Rust-side incremental
compiles of kernel-only changes skip slint codegen (bounded win — the
codegen is 17.7 s of the 128 s lib build); the kernel could later be
unit-tested without the testing backend. Cons: ~30 k lines move for
zero behavior change; every kernel/glue signature touch becomes a
cross-crate edit; `models.rs`/`requests.rs` straddle the seam (they
reference generated slint types and `slint::ModelRc`) and would need
another seam or to stay behind; test binaries get marginally cheaper
per link but the 31-binary structure is untouched. **Not recommended
now**; becomes the designated move only if a second *interactive*
frontend that wants the view machine/keymap lands (the recorded
Emacs/org edit-backend idea, #185, is an *edit backend*, not a
frontend, and does not trigger this).

**Option C — split by feature domain (board/today/habits/settings
crates).**
Rejected outright: `IdTables`, `ViewState`, `PushModels`, and
`theme.slint` are cross-domain shared; the `.slint` tree is one
compiler invocation; feature crates would invert the L1→L2→L3 layering
and multiply the ceremony for a maintainability win F-1's in-crate
module split already delivers.

**Recommendation: no split.** Spend the effort on the in-crate
`adapter/` decomposition (F-1) and the `lib.rs` layout fix (F-4);
revisit Option B only on a concrete second-frontend commitment.

## 7. Refactoring order (concrete, by leverage)

1. `adapter/` module-directory decomposition (F-1) — mechanical,
   family-by-family, each move behavior-preserving under the existing
   journey/golden suites.
2. Widget-residue sweep (F-2/§5 rows 1–8, 10) — the plan-next-day and
   track-entries sites also fix the modifier-guard divergence; the
   review-walk `VerbButton` retirement rides the same wave.
3. `lib.rs` test-mod extraction + `timers.rs`/`alarms.rs` (F-4).
4. Order-key re-homing or parity pin (F-3).
5. The small absorptions with satisfied third-consumer rules:
   pickers merge (§5 #11), SegmentButton (#16), ScrollBar (#17),
   StatusDot adoption (#13), restore-section ConfirmStrip (#15).

## 8. Non-findings (audited, clean — recorded to prevent re-litigation)

No IO/env in production; no locks; no runtime coupling; no blocking
sends; no `async fn`; no literal colors outside `theme.slint`; no dead
widgets; no key handlers in hosts/views/compositions beyond the
sanctioned leaves and F-2's enumerated residue; no match-on-view-id key
routing; overdue/due-soon/today-sections/palette-ranking/effort-parse
all delegated to the domain; all cached surfaces single-update-path.

## 9. Backlog actions taken (this run)

Every finding above is homed: F-1/F-2/F-3 + §5 rows 1–15 in new issues
and #1127/#1096 comments, F-4–F-9 in new issues, F-6 additionally as a
#680 comment, and the §4 residual vocabularies as a #1280 comment.
Nothing from §3–§5 exists only in this report.

New issues (searched first for duplicates; none covered these):
- **#1349** — adapter.rs decomposition into an `adapter/` module
  directory by seam family (F-1).
- **#1350** — post-round-9 widget residue: hand-rolled activation
  blocks (incl. the modifier-guard divergence), hand backdrops, the
  shared pressable-row convention, `VerbButton` (F-2, §5 rows 1–10).
- **#1351** — order-key minting re-homing / parity pin against the
  domain's `floor_midpoint_order` (F-3).
- **#1352** — settings-view field/picker/dot family promotions
  (§5 rows 11–14), grouped as one issue and coordinated with #1096.
- **#1357** — `lib.rs` restructure: extract the six in-file test
  modules, `timers.rs`/`alarms.rs`; `UiHandle` test-seam surface
  noted (F-4 + F-10).
- **#1358** — parity pin for the `settings_form.rs` validation mirror
  (F-5).
- **#1359** — small-widget absorptions with the third-consumer rule
  satisfied: SegmentButton, ScrollBar, review-view skeleton,
  caption chips, SwatchButton (§5 rows 16–21).
- **#1360** — the alarm window's two-truth-source ladder (§4 smell 1).
- **#1361** — token-lint residue: shadow-blur, bare hairlines, raw
  panel widths (F-9).
- **#1362** — build.rs's hand-maintained 30-probe registry (F-8).

Comments on existing issues (new evidence, not duplicates):
- **#1280** (keybinding-coverage audit): the hardcoded action-layer
  vocabularies inventory (slider, switcher flyout ladder, notification
  verbs, editor table, alarm window) as sweep inputs, plus #1350's
  modifier-guard deviations.
- **#1127** (labels roster rows): the track-entries restore-section
  twin now makes the shape a two-site family (§5 row 15).
- **#680** (deferred clock-drift clusters): F-6 recorded as cluster
  C7 (two wall reads in one derive pass) with its unlock condition.
- **#1096** referenced: #1352's field-row promotion is explicitly
  coordinated with the form-row ruling there.

Deliberately not filed: F-7 (the advisory habit-pause date gate) —
reviewed and judged correct as-is (the planner stays authoritative;
the gate is per-keystroke feedback), recorded in §3 so the pattern
doesn't spread silently; no improvement action exists to forget.

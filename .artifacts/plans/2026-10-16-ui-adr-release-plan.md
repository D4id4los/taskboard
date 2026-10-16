# [Plan] UI ADR, Docs Amendments & v0.1.0 Release (GUI Phase 7, Chunk 5)

- **Date**: 2026-10-16
- **Kind**: stage-1 chunk plan (docs + release-chore spec) for
  `docs/ui-adr-release` — planning only; no implementation in this
  commit.
- **Origin** (normative unless marked): accepted design plan
  `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md` — §13 draft
  ADR 0015 + §13.1 numbering gap, §10 UI-crate testing strategy, §11/§16
  decisions Q11 (Slint licensing) and Q21 (ADR numbering gap), §12 row 5;
  growth roadmap `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  Phase 7 (exit: "tagged v0.1.0 with desktop UI, released binaries");
  reminders design plan `.artifacts/plans/2026-10-12-reminders-design-plan.md`
  §12 (ADR 0013 draft) + §13 (decisions) — source material for the 0013
  backlog entry only; `scripts/bump_version.sh` + `scripts/changelog.py`
  (release mechanics); `.github/workflows/release.yml`; dispatch plan
  `.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  §1/§6. Amended 2026-10-16 after a user note: `main` is
  branch-protected and `bump_version.sh` predates the protection — the
  release cut rides a branch + PR, and the tag is created on main only
  after the merge (§6.3); a backlog entry tracks the script's
  adaptation (§5.4).
- **Precondition verified 2026-10-16**: chunks 1–4 are on `main`
  (`2e9d824` == `origin/main`). Chunk evidence: chunk 1
  `0b507c4..20e3ca3` (plan → slint 1.18.1 + theme module + token gate →
  contrast check → bundled fonts → window shell mount API → review
  report), chunk 2 `4d826d7..7aa6e93` (plan → action registry → stroke
  grammar → presets → merge/validation → resolver → router/shell state →
  generated menu → root FocusScope → keymap config handover → review
  round), chunk 3 `0ad1e8d..d01978c` (plan → breakpoints/Responsive →
  presentation helpers → view-state machine → reminder actions + D9
  table → L1/L2/L3 library + chrome → model assembly/refresh/dispatch →
  component catalogue → review round → follow-ups backlog), chunk 4
  `d88bd0a..2c7b3ec` (plan → testing backend pin + guard/hash units →
  journey driver + quit/window seams → session fixture + steps 1–10 →
  view-graph walk → golden tier → MCP wiring + CI smoke → one-platform
  fix → harness trim → CI system deps). Two later docs commits sit on
  top (`92a4519` kiosk backlog entries, `2e9d824` v0.1.0 audit plan
  acceptance) — both read; both interact with this chunk (§5, §9).
- **Branch / worktree**: `docs/ui-adr-release` in
  `../taskboard-WT-docs-ui-adr-release`, off `origin/main` @ `2e9d824`
  (local `main` was stale; the branch bases on the fetched tip. Upstream
  tracking deliberately unset — the branch must never push to `main`).
- **Status**: PROPOSED (this document) — implementation follows per the
  three-stage dispatch protocol.

---

## 0. Scope fence (what this chunk does and does not build)

Builds (design §12 row 5 + dispatch §6):

1. `docs/adr/0015-slint-ui-architecture.org` — the design §13.2 draft
   formalised: Status Accepted, the licensing paragraph (Q11), the
   testing-pin upgrade runbook, amendments against the landed system
   (§2).
2. `docs/architecture.org` amendments — six sections restating the
   landed UI system (§3).
3. `docs/testing_strategy.org` §7 rewrite — the four landed UI test
   tiers + the Slint pin discipline (§4).
4. Backlog bookkeeping in
   `.artifacts/taskboard-potential-improvements-backlog.md` — two ✅
   DONE notes + two follow-up entries: the ADR-0013 formalisation debt
   and the `bump_version.sh` branch-protection adaptation (§5).
5. `CHANGELOG.md` `[Unreleased]` — the GUI release notes that promote
   into the v0.1.0 section at release time (§6.2).
6. Release-readiness verification with recorded evidence + the exact
   post-merge, user-executed release command sequence (§6.1, §6.3).

Does **not** build (dispatch §6 non-goals): no code changes of any kind
(a release-pipeline gap, if verification finds one, is its own `fix/` or
`ci/` PR — this chunk records and escalates, §9); no ADR renumbering;
no formalisation of ADR 0013 itself (the backlog entry owns the debt);
no touch to "[2026-10-14] LLM/MCP UI-debugging driver tooling" or any
other open backlog entry; no `bump_version.sh` execution, no tag
creation, no push — the release is the user's, after this PR merges.

---

## 1. Landed-shape survey (what the docs must describe)

The dispatch mandate: describe the **landed** system, not the design's
predictions. Inventory verified against `origin/main` @ `2e9d824` on
2026-10-16 — this section is the factual source of truth for §2–§4;
every docs claim must trace to a line here.

**UI crate** (`crates/taskboard-ui-slint/`, ~9.7k rs-lines + ~1.6k
slint-lines): modules `actions` (registry: dotted `scope.verb` ids,
contexts, reserved globals disabled where unimplemented), `keymap`
(stroke grammar, standard/emacs/vi presets, merge with drop-and-warn
validation), `resolver` (pure state machine: contexts, chords/prefixes,
vi modes, masking), `adapter` (root-FocusScope capture interception,
shell state, D9 interpretation table, `StartupWarnings`), `router` /
`view_state` (view machine: Board → StackDetail → TaskDetail,
reconcile-pops), `models` / `presentation` (pure display helpers reusing
domain overdue/due-soon), `responsive` (unscaled breakpoints:
compact < 720 / regular / expansive ≥ 1440), `menu` (generated
action menu), `shell` / `fonts` (bundled tri-licensed fonts, runtime
registration), `lib.rs` (`mount()` hand-over set: `ArcSwap<AppState>` +
`EngineSignal`/`SystemEvent` broadcast receivers + command
`mpsc::Sender<StateCommand>` + `UiSettings{scale, keymap, startup}`;
the D1 repaint driver — a slint `Timer` draining both broadcasts plus
the wall-minute boundary; `run()` on the calling thread; `quit()`).
Markup: `ui/theme.slint` (Theme/Typography/Semantic globals),
`ui/widgets/` (8 L1 primitives), `ui/compositions/` (board-layout,
toolbar, status-bar, action-menu), `ui/views/` (board, stack, task).
Manifest: `slint = { workspace = true, features =
["unstable-fontique-011"] }` — **Q10's default renderer features plus
this one additive font-registration feature** (the 1.18-documented
runtime font path, `src/fonts.rs`); tokio is channel-types-only; the
dev-gated `mcp = ["slint/mcp"]` feature is non-default; dev-deps:
`i-slint-backend-testing` (workspace exact pin), insta, proptest, png.

**App crate** (`crates/taskboard-app/src/ui.rs`): the UI-command
forwarder `spawn_ui_command_forwarder` (drains the UI mpsc onto
`EngineHandle::dispatch` via the `CommandSink` trait, capacity 64,
hermetic fake-sink test); the D4 bare-invocation routing
(`DesktopUi` boots the GUI regardless of TTY — `AppMode` is frontend
selection; `Daemon`+TTY → REPL; else daemon lifecycle); and
`resolve_ui_settings` (figment `[ui]` → typed `UiSettings` + keymap
`Finding`s; a hostile keymap never blocks start, Q15). `config.rs`:
`[ui] scale` (0.5–3.0 enforced, typed `InvalidScale` error) +
`[ui.keymap]` preset/overrides.

**Harness** (chunk 4): `tests/support/` — `driver.rs` (the thin
journey-driver isolating `i-slint-backend-testing`), `fake_engine.rs`
(forwarder-boundary command capture), `fixture.rs`, `golden_compare.rs`
(perceptual-hash), `guard.rs` (keypress ceilings from design §9.2);
`tests/journeys.rs` (ten §7.3 steps + cold-start view-graph walk),
`tests/golden_visuals.rs` (scale extremes × breakpoints, software
renderer, renderer-presence assertion), `tests/keymap_snapshots.rs`
(registry + default-keymap insta), `tests/pin_guard.rs` (testing-backend
pin == slint pin).

**Gates**: `scripts/check_slint_tokens.py` (no literal colours, font
attributes, or spacing dimensions outside `theme.slint`; no suppression
escape) wired into `ci.yml` (line ~48, with `--self-test`) and
`.githooks/pre-commit` (`core.hooksPath = .githooks`); the contrast
check is a cargo unit test (`src/token_tests.rs`), riding nextest.

**MCP-off assertions** (two): `ci.yml` line ~61 and `release.yml` build
job — both `! cargo tree -e features -p taskboard-app | grep
'feature "mcp"'` on the default/release resolution.

**Reminders signal path** (landed by the reminders round, consumed by
chunk 3): the overdue banner dispatches `StateCommand::SnoozeReminder`
(`ReminderRef::Default` anchor) and `SetTaskRemindersMuted` over the
command mpsc — real typed commands, not UI-local state.

**Release pipeline**: `release.yml` — tag-triggered (`v*`), validate
(tag format; **tag version == `[workspace.package]` version**; release
notes = `changelog.py extract <version>` from CHANGELOG.md), build
matrix x86_64-unknown-linux-gnu (tar.gz; installs
libgl1-mesa-dev/libegl1-mesa-dev/libxkbcommon-dev/libwayland-dev/libfontconfig1-dev)
+ x86_64-pc-windows-msvc (zip), MCP-off assertion per target,
`package_release.py` builds **taskboard-app** (which depends on
`taskboard-ui-slint` at `crates/taskboard-app/Cargo.toml:31` → **the
release binaries carry the UI by construction**), publish creates the
GitHub Release with the changelog section as body. `workflow_dispatch`
dry-run mode exists (package + upload artifacts, no Release).

**Version reality**: `[workspace.package] version = "0.1.0"` since the
initial manifest (`968aeff`, repo bootstrap); **zero git tags exist**;
CHANGELOG.md has only `[Unreleased]` (no versioned sections) and
**contains no GUI entries** — chunks 1–4 added none. Consequence: §6.3's
deviation decision.

---

## 2. ADR 0015 — file spec (`docs/adr/0015-slint-ui-architecture.org`)

### 2.1 Number verification (Q21, done at planning time)

`docs/adr/` holds 0001–0012 and 0014 — **0013 is the reserved gap**
(reminders round), **0015 is next-free**. Grepping `.artifacts/plans/`
for referenced-but-unlanded numbers ≥ 0013 finds exactly two: 0013
(reminders design plan §12 — the reservation itself) and 0015 (this
round). No open branch carries an ADR file (`ci/miri-cap-probe` is
Miri tooling). Stage 2 re-verifies by listing `docs/adr/` before
writing (dispatch §6 instruction).

### 2.2 Header (house style per ADR 0014)

```org
#+title: ADR 0015: Slint UI — Dumb Presentation Client, Action-Dispatch Keymap, Token Theming
- *Date:* 2026-10-14
- *Status:* Accepted (implemented across GUI Phase 7 chunks 1–4:
  shell/theme, action-keymap, views, harness)
- *Depends on:* ADR 0001 (multi-actor core), ADR 0003 (unsafe exception for the UI crate)
```

Date = 2026-10-14 (the round's decision date — all 23 agenda rows
resolved then, §16; matches ADR 0014's convention of dating the
decision, not the file). Status flip Draft → Accepted: the decisions
were accepted 2026-10-14 and the implementation is merged; the
implementation note follows ADR 0014's wording pattern.

### 2.3 Body: the §13.2 draft carried, with recorded amendments

The Context / Decision (7 numbered items) / Consequences skeleton is
taken **as drafted** — the decisions are the accepted round's, not
renegotiated. Amendments (each required by the dispatch acceptance
clause "amendments need reasons"; all verified against §1):

- **A1 — Decision 6 restated accurately** ("renderer set"): the landed
  manifest is default renderer features (winit + femtovg + software,
  runtime fallback — Q10) **plus the additive
  `unstable-fontique-011` font-registration feature** (Q18's bundled
  fonts need the 1.18-documented runtime path, `src/fonts.rs`). The
  draft's "slint default features" is true of renderer/backend
  selection only. Reason: the ADR must match
  `crates/taskboard-ui-slint/Cargo.toml` as merged; a reader following
  the draft would mis-predict the manifest.
- **A2 — Context: the numbering-gap sentence.** One sentence noting
  0013 is reserved by the reminders round (2026-10-12) and its
  formalisation is tracked in the backlog — not a withdrawn or
  accidental number. Reason: the accepted v0.1.0 audit plan
  (2026-10-16) explicitly flags "numbering skips 0013 — verify whether
  that is a withdrawn ADR or an accident"; recording the answer at the
  point a reader notices the gap closes the question durably.
- **A3 — Consequences: the kiosk renderer corollary.** One short
  paragraph: the 2026-10-16 kiosk hardware assessment (gen3-class
  netbooks; modern Mesa ships no gen3 hardware GL) makes the software
  renderer the kiosk path by fallback; skia/femtovg remain the
  desktop-GPU choices; `backend-linuxkms` still deferred. Reason:
  postdates the design round; already recorded in the backlog's
  [2026-10-16] kiosk entry and the [2026-10-07] slint entry's update
  paragraph — the ADR's Consequences is where the distribution story
  has one home (mirroring how decision 7 homes licensing).
- **A4 — Decision 7 (licensing, Q11) kept and tightened**: the
  AGPL-3.0-only application combined with the Slint runtime's
  GPL-3.0-only OR Royalty-free-2.0 OR Software-3.0 tri-license is
  acceptable for this project's posture (AGPLv3 §13 combination
  path); revisit only if distribution terms change. Named per stress
  report §4.5. No change of substance — the draft paragraph already
  carries Q11's resolution; stage 2 verifies the licence names against
  the stress report's exact wording.
- **A5 — Consequences: the testing-pin upgrade runbook** (replaces the
  draft's one-line "per-Slint-upgrade audit chore" mention with the
  explicit runbook the dispatch §6 names). On any Slint version bump:
  1. move `slint = "<new>"` (caret) and `i-slint-backend-testing
     = "=<new>"` (exact) **together** — Q12's move-together rule;
  2. `tests/pin_guard.rs` must stay green (it asserts pin equality);
  3. re-audit the stress-report sections the design cites (§3.2
     interception, §3.4 scaling, §3.5 testing backend, §3.8 renderer)
     before merging;
  4. re-run the golden tier; regenerate goldens as an explicit reviewed
     step (never CI-blessed).
  Reason: design §14/Q8 named the chore but not its steps; chunk 4's
  pin guard and regeneration procedure exist now and the ADR is their
  durable home.

Decision items 1–5 otherwise land as drafted (verified: the landed
system matches — dumb-client hand-over set, token module + lint gate +
contrast check, action registry/resolver/presets/root scope, root
multiplier scaling + unscaled breakpoints, logical-first harness with
accessible-query addressing).

---

## 3. `docs/architecture.org` amendments (section by section)

Every amendment restates a §1 fact; the section list is exhaustive —
anything not named stays untouched.

1. **"Framework-Agnostic 'Thin Client' UI" (line ~95)** — rewrite the
   3-paragraph section: Slint 1.18.1 is the shipped framework (ADR
   0015); the dumb-presentation contract restated with landed shapes —
   `ArcSwap` reads, `EngineSignal`/`SystemEvent` broadcast receivers
   drained by the UI's repaint timer, typed `UiSettings` hand-over
   (config read app-side, figment), commands out on the state-command
   `mpsc` drained by the bootstrap forwarder; the framework-swap seam
   survives as the crate boundary (presentation logic only, no IO, no
   business logic), not as an unexercised "spiking Iced/egui" claim.
2. **Inter-Component Communication table (line ~110)** — amend the
   "*UI → State Engine*" row: the mpsc is drained by the
   bootstrap-owned UI-command forwarder
   (`taskboard_app::ui::spawn_ui_command_forwarder`: `recv()` →
   `EngineHandle::dispatch`, dropped-command logging, capacity 64).
   No other row changes.
3. **Crate Roles item 5 — `taskboard-ui-slint` (line ~198)** — replace
   the stale "slint (deferred)" role with the landed inventory: the
   module map (theme/token module with closed text roles + bundled
   fonts; action registry with reserved globals; keymap-as-data with
   standard/emacs/vi presets and drop-and-warn validation; pure
   resolver; root-FocusScope adapter + generated action menu; view
   machine + three views; L1/L2/L3 component layering; presentation
   helpers reusing domain ordering semantics; unscaled responsive
   breakpoints), the no-IO rule (`UiSettings` typed hand-over), AGPL
   + the `unsafe` exception (ADR 0003), and the dependency list as
   landed: `taskboard-domain`, `slint` (+`unstable-fontique-011`),
   `arc-swap`, `tokio` (channel types only — the mandated fabric, not
   an IO dependency), `chrono`, `thiserror`, `tracing`; dev-only:
   `i-slint-backend-testing` (exact pin), insta, proptest.
4. **Crate Roles item 6 — `taskboard-app` (line ~202)** — extend the
   description with the GUI lifecycle facts: the `ui` module (D4
   bare-invocation routing — `mode = "desktop"` boots the GUI
   regardless of TTY, `AppMode` is frontend selection; the
   UI-command forwarder; `resolve_ui_settings`), the `[ui]` config
   surface (`scale` 0.5–3.0 typed-validated, `keymap.preset` +
   `keymap.overrides`, keymap findings surfaced as startup warnings,
   never blocking — Q15).
5. **Tech Stack table (line ~259)** — add one row: *UI Toolkit* =
   `slint` — declarative UI with a software-renderer fallback for
   low-power kiosks; pinned 1.18.1 in `[workspace.dependencies]`
   (renderer default features + additive font-registration feature;
   the exact-pinned `i-slint-backend-testing` is dev-only and moves
   with it, ADR 0015).
6. **Quality Gate table (line ~285)** — add one row: *UI token lint* =
   `scripts/check_slint_tokens.py` — no literal colours, font
   attributes, or spacing dimensions outside the theme module; runs in
   CI and the pre-commit hook; no suppression escape. (The contrast
   check rides nextest as an ordinary unit test — no separate row.)

Non-amendments (recorded): the Versioning section keeps its
`bump_version.sh` text verbatim (the script remains the release tool
going forward — conditional on the §5.4 backlog adaptation to the
protected-main flow; the one-off v0.1.0 direct cut is release chore,
not a versioning-policy change); the comms-table rows for non-UI
paths; the kiosk/desktop target statements (already accurate).

---

## 4. `docs/testing_strategy.org` amendments

1. **§7 "UI Unit/Component Tests" (line ~85) — full rewrite**, retitled
   "UI Testing: Tiers & Pins (ADR 0015)". The current text cites
   `slint::testing`, which does not exist (stress report §3.5 — the
   design round's recorded finding); the rewrite replaces the fiction
   with the landed four-tier table (design §10 as implemented):
   - *Unit (pure Rust)*: keymap parse/merge/validate, resolver
     proptests (chords, prefixes, modes, masking), preset coverage +
     conflict-freeness, contrast pairs over token pairs,
     presentation helpers (time-boundary formatting). No Slint types.
   - *Component / journeys*: `i-slint-backend-testing` behind
     `tests/support/driver.rs`, mock time, accessible-role/label
     element addressing, generated getters; the engine faked at the
     forwarder boundary (`support/fake_engine.rs`); the ten design
     §7.3 journey steps as one continuous session + the cold-start
     view-graph walk; the keypress-count guard (`support/guard.rs`)
     with the §9.2 ceilings as design contracts (improvements always
     pass).
   - *Golden visuals*: board view at scale extremes × each breakpoint,
     software renderer, headless, bundled fonts; perceptual-hash
     tolerance; renderer-presence assertion ahead of any comparison;
     regeneration is an explicit reviewed step, never CI-blessed.
   - *Harness smoke*: the dev-gated `slint/mcp` feature CI job boots
     headless-with-renderer, captures one screenshot, asserts
     non-empty and changes-after-interaction; the feature-off
     assertions in `ci.yml`/`release.yml` keep the MCP server out of
     release resolutions.
   Plus the house-rule restatement: no sleeps (mock time only);
   accessible labels are element *addresses*, message text is never
   matched; journeys assert routing only (engine semantics live in the
   engine's own suites).
2. **Pin-discipline paragraph** (inside §7): `slint = "1.18.1"` (caret)
   + `i-slint-backend-testing = "=1.18.1"` (exact) move together;
   `tests/pin_guard.rs` asserts equality in CI; every Slint bump
   follows ADR 0015's upgrade runbook (§2.3 A5 here) — a per-bump
   chore, recorded where the ADR lives.

Non-amendments: §5's dumb-client framing (still true); §6 tokio-time
rules (already binding); Miri (§10), fuzz, sanitizer, coverage sections
untouched.

---

## 5. Backlog bookkeeping

File: `.artifacts/taskboard-potential-improvements-backlog.md`. Style
per the house precedent (MSRV/Bencher/Retry-After/Miri entries):
heading gains "— ✅ DONE (<date>)" and the metadata block gains a
`- **Status**: ✅ DONE (...)` line; the body stays for history.

1. **"[2026-10-07] Add the `slint` dependency to taskboard-ui-slint"**
   (line ~175) → ✅ DONE (2026-10-16). Status note (one line, accurate):
   implemented via GUI Phase 7 chunk 1 (`feat/ui-shell-theme`):
   `slint = "1.18.1"` in `[workspace.dependencies]`, renderer question
   answered by the stress report §3.8 (default features — femtovg +
   software with runtime fallback); CI consequence: the Linux legs
   install the GL/font system deps. The existing "*Update 2026-10-16*"
   kiosk-renderer paragraph **stays** (complementary, not superseded —
   it names the kiosk software path; the default-features choice is
   what makes that fallback work).
2. **"[2026-10-09] UI command forwarder bootstrap task"** (line ~900) →
   ✅ DONE (2026-10-16). Status note: implemented in chunk 1 as
   `spawn_ui_command_forwarder` in `crates/taskboard-app/src/ui.rs`
   (`CommandSink` trait over `EngineHandle::dispatch`; hermetic
   fake-sink test).
3. **New entry** (exact text below), inserted **after the last
   [2026-10-14] entry ("Push-abort `Retry-After` floor") and before
   the [2026-10-15] block** — the file is chronologically ordered;
   appending at the end would break that order.

   ```markdown
   ## [2026-10-14] Formalise ADR 0013 (reminders) from the reminders design plan §12/§13

   - **Category**: `Architecture` / `DX`
   - **Originating Plan/Report**: `.artifacts/plans/2026-10-12-reminders-design-plan.md`
     (§12 draft + §13 decisions, approved 2026-10-12); surfaced by the GUI
     Phase 7 design plan §13.1/Q21 (2026-10-14)
   - **Target Area**: `docs/adr/` (0013 file)

   ### Context & Description
   The reminders design round drafted ADR 0013 (§12) and resolved every
   decision (§13, user review 2026-10-12), but the ADR file never landed
   — the number is reserved by that round, deliberately not reused by
   ADR 0014 or ADR 0015 (numbering honesty per Q21), so `docs/adr/`
   skips 0013. The 2026-10-16 v0.1.0 audit plan flags the gap as a
   standing docs question. The implementation itself (rules, tick arm,
   storage, CLI verbs, UI banner consumption) is merged; only the
   decision record is missing.

   ### Proposed Approach
   One small docs chunk: formalise the §12 draft into
   `docs/adr/0013-reminders-local-due-anchored-rules.org` with the §13
   decisions folded in, Status Accepted, dated 2026-10-12 (the decision
   date), noting the landed implementation. No code, no renumbering.
   ```

4. **New entry** (exact text below), appended at the **end of the
   file** (dated 2026-10-16, after the kiosk entries — chronological
   order). Reason this chunk adds a second entry beyond the dispatch's
   bookkeeping list: the branch-protection conflict was surfaced by the
   user's 2026-10-16 note on this plan; AGENTS §7 mandates appending
   out-of-scope improvement ideas, and the script's documented flow
   ("exclusively via `bump_version.sh`", AGENTS §9) is stale for every
   future release, not just v0.1.0. Recorded here so the review agent
   sees the addition is deliberate plan-level scope, not drift.

   ```markdown
   ## [2026-10-16] `bump_version.sh`: adapt the release cut to branch-protected `main`

   - **Category**: `DX`
   - **Originating Plan/Report**: user note on the GUI Phase 7 chunk-5
     plan (`.artifacts/plans/2026-10-16-ui-adr-release-plan.md` §6.3)
   - **Target Area**: `scripts/bump_version.sh`,
     `docs/coding_guidelines.org` §7, `AGENTS.md` §9

   ### Context & Description
   The script predates branch protection on `main`: it aborts unless
   run on `main` and then commits + tags directly there — both now
   impossible (every change enters `main` through a PR, AGENTS §8).
   v0.1.0 is cut by hand instead (changelog promote on a release branch
   → PR → post-merge tag on main's rebased commit). Until the script
   is adapted, every future release faces the same manual dance, and
   the "exclusively via bump_version.sh" wording in AGENTS §9 /
   coding_guidelines §7 is stale.

   ### Proposed Approach
   Rework the script for the protected-main reality: verify a clean
   tree, create `chore/release-v<X.Y.Z>` off the fetched `origin/main`,
   do the bump/promote commit on the branch, push and open the PR with
   `gh` (CI must pass), and print the user's post-merge steps — fetch
   main, tag the merged commit, push the tag (which triggers
   `release.yml`). The tag step stays outside the script (agents never
   push; the merge is the user's on GitHub). Update AGENTS §9 and
   coding_guidelines §7 in the same commit.
   ```

5. **Untouched**: every other entry — explicitly including
   "[2026-10-14] LLM/MCP UI-debugging driver tooling" (stays open,
   POST-1.0) and the [2026-10-15] UI follow-ups (drag-to-move,
   due-date/description editors, reopen action, due-soon horizon,
   reminder preset bindings, first/last-item focus navigation) and the
   two [2026-10-16] kiosk entries.

---

## 6. v0.1.0 release chore (roadmap Phase 7 exit)

### 6.1 Release-readiness checklist (stage 2 verifies; evidence in the chunk report)

| # | Check | Where it holds | Evidence to record |
|---|---|---|---|
| 1 | Release binaries carry the UI | `package_release.py` builds `taskboard-app`; `crates/taskboard-app/Cargo.toml:31` depends on `taskboard-ui-slint` | the dependency line + the packaging invocation in `release.yml` |
| 2 | GUI links on the Linux release leg | `release.yml` build job installs libgl1-mesa-dev, libegl1-mesa-dev, libxkbcommon-dev, libwayland-dev, libfontconfig1-dev | the workflow step |
| 3 | MCP feature off in the release profile | **two assertions**: `ci.yml` (~line 61) and `release.yml` (build job, per target) — `! cargo tree -e features -p taskboard-app \| grep 'feature "mcp"'`; the `mcp` feature is non-default and dev-gated | both step definitions |
| 4 | Tag/version match | `[workspace.package] version = "0.1.0"`; `release.yml` validate job asserts tag == workspace version | the manifest line + validate step |
| 5 | Release notes exist for 0.1.0 at the tag | this chunk writes the GUI entries into CHANGELOG `[Unreleased]` (§6.2); the user's promote (§6.3) creates the `[0.1.0]` section; `release.yml` extracts it for the Release body | local `changelog.py extract` dry check after promote |
| 6 | Workflow-on-main precondition | `release.yml` header note ("must be merged to main before the first release tag") — the workflow predates the tag | trivially satisfied; note it |
| 7 | Optional pre-tag validation | `workflow_dispatch` dry-run (build + package + artifacts, no Release) | mention in the handoff |

### 6.2 CHANGELOG `[Unreleased]` GUI entries (content spec)

Keep-a-Changelog `### Added` bullets (house style: each bullet
self-contained, user-visible behaviour, no internal codenames). Content
— 5 bullets:

1. The desktop UI (ADR 0015): `mode = "desktop"` boots a Slint GUI —
   kanban board, stack detail, task detail views driven from the same
   headless core (lock-free state reads, no business logic in the UI);
   bare `taskboard` routes by the configured app mode.
2. Keyboard-first operation: every verb is an action with a rebindable
   keybinding — standard/emacs/vi presets, `[ui.keymap]` preset +
   per-action overrides (invalid overrides are dropped with a startup
   warning, never blocking); a generated action menu lists the current
   context's verbs with their key hints.
3. Display scaling & responsiveness: `[ui] scale` (0.5–3.0) as a live
   token multiplier, `SLINT_SCALE_FACTOR` as the OS-DPI escape hatch;
   unscaled responsive breakpoints (compact < 720 / regular /
   expansive ≥ 1440) with compact board degeneration.
4. Sync status + overdue banner: the persistent status-bar sync
   indicator and the overdue-task banner (snooze / mute ride the
   reminders command path).
5. One bullet for the quality machinery, user-relevant framing: the
   WCAG contrast-checked dark theme, screen-reader labels on every
   control, and the headless keyboard-journey test suite that keeps
   every core verb within its keypress budget.

### 6.3 The post-merge release command sequence (user-executed)

**Recorded decision (deviation from dispatch §6's assumption).** The
dispatch plan says "the bump + tag run via `./scripts/bump_version.sh`
after this PR merges" — that assumption fails for v0.1.0 twice over:

1. **No bump is needed**: the workspace version has been `0.1.0` since
   the initial manifest (`968aeff`), no tag exists, and the script
   only *increments* — running it would cut `v0.2.0` or `v0.1.1`,
   contradicting the roadmap exit criterion "tagged v0.1.0" and
   failing the release.yml tag==version validation.
2. **The script's flow is impossible under branch protection**
   (user note 2026-10-16): `bump_version.sh` predates the protection —
   it aborts unless run on `main` and then commits + tags directly on
   `main`. Both are now forbidden: every change enters `main` through
   a GitHub PR (AGENTS §8).

Therefore the v0.1.0 release is cut by **hand-mirroring the script's
promote/commit steps on a release branch, merged by PR, with the tag
created on `main` only after the merge**. Two structural rules of that
flow, both consequences of the house merge method:

- **Tag after merge, on the rebased commit.** `main` is rebase-merged
  (AGENTS §8): the PR's commits get new SHAs when it lands. A tag
  created on the pre-merge branch tip would point at a commit that is
  not reachable from `main` — violating release.yml's "must be merged
  to `main` before the first release tag" contract and potentially
  differing from what `main` ships. The tag is therefore the *last*
  step, cut on the fetched `main` tip (which contains the promoted
  changelog; the validate job reads tag version == workspace version
  and extracts the `[0.1.0]` section at the tagged commit).
- **Anything merged between the release PR and the tag rides along** —
  tagging `main`'s tip includes later commits. Harmless here (the
  checks read the changelog + manifest at the tagged commit); noted so
  it reads as intended, not accident.

`bump_version.sh` resumes duty from v0.2.0 onward **once the §5.4
backlog adaptation lands** (release-branch + PR + printed post-merge
tag steps). Alternatives rejected: (a) version-down-then-bump
(`0.1.0 → 0.0.0 → 0.1.0`) — two misleading commits for no benefit;
(b) pre-promoting the changelog inside this chunk's PR — it would
stamp the release date at this PR's merge time (wrong if the release
PR is delayed) and break the at-release-time promote semantic the
pipeline establishes; (c) exempting the release path from branch
protection (e.g. allow direct pushes for tags/commits) — a repository
policy the project deliberately installed; the release flow adapts to
it, not the reverse.

The sequence, printed verbatim for the user (also carried verbatim in
the stage-3 PR description):

```sh
# 0) after the docs/ui-adr-release PR merges — cut the release branch:
git checkout main
git pull --ff-only origin main
git status --short                  # must be empty before branching
git switch -c chore/release-v0.1.0

# 1) promote the changelog (substitute the actual release date):
python3 scripts/changelog.py promote 0.1.0 2026-10-XX CHANGELOG.md

# 2) commit the release cut (mirrors bump_version.sh's commit shape):
git add CHANGELOG.md
git commit -m "chore(release): cut the v0.1.0 changelog section

Promote [Unreleased] into the [0.1.0] release entry. The single global
semver is already 0.1.0 (initial manifest); the release rides this
branch + PR because main is branch-protected (bump_version.sh
adaptation tracked in the backlog)."

# 3) push the branch and open the release PR (required checks must pass):
git push -u origin chore/release-v0.1.0
gh pr create --base main --title "chore(release): cut the v0.1.0 changelog section" \
  --body "Release cut for v0.1.0 (roadmap Phase 7 / M2 exit). After merge: fetch main, tag v0.1.0 on the merged commit, push the tag — the Release workflow takes over."

# 4) after CI is green and the PR is merged on GitHub (rebase-merge
#    rewrites SHAs — the tag MUST be cut on main, never on the branch):
git checkout main
git pull --ff-only origin main
git tag -a v0.1.0 -m "taskboard 0.1.0"
git push origin v0.1.0             # the tag push triggers the Release workflow
```

Optional pre-branch dry run: GitHub → Actions → Release → Run workflow
(dry-run) — builds and uploads artifacts without creating a Release.
Post-tag verification: `gh run watch` on the triggered run, then
`gh release view v0.1.0`.

### 6.4 Guards (what this chunk itself does not do)

No `bump_version.sh` execution, no `changelog.py promote` on the
branch, no tag creation, no push of any ref (dispatch §6; AGENTS §8).
The only CHANGELOG edit is adding `[Unreleased]` content (§6.2) —
promote, the release branch, its PR, and the post-merge tag are all
the user's steps (§6.3).

---

## 7. Execution order & commits (stage 2)

Docs-only; every commit standalone (rebase-merge hygiene), subjects in
house convention (`docs(adr)`, `docs(architecture)`, `docs(testing)`,
`docs(backlog)` — all three first two seen in history; `docs(adr)` and
`docs(testing)` verbatim at `f482ae0`, `31e1aa5`):

1. `docs(adr): formalise ADR 0015 slint ui architecture` — the ADR
   (§2), re-verifying the number by listing `docs/adr/` first.
2. `docs(architecture): the landed slint ui crate and desktop entry` —
   the six-section amendment set (§3).
3. `docs(testing): the ui test tiers and slint pin discipline` — the
   §7 rewrite (§4).
4. `docs(backlog): slint dependency and ui forwarder done, adr 0013 and
   bump script follow-ups` — the bookkeeping (§5).
5. `docs(changelog): the v0.1.0 gui release notes` — `[Unreleased]`
   entries (§6.2).

Gates: docs-only tree, but run the full AGENTS §10 suite anyway (the
pre-commit hook fires on every commit; `changelog.py self-test` after
the CHANGELOG edit proves the promote/extract round-trip still holds;
`cargo nextest`/clippy confirm no accidental code drift). The
release-readiness checklist (§6.1) is executed and its evidence table
filled into the stage-2 report.

---

## 8. Decisions (settled open questions)

- **D1 — ADR number**: 0015, verified next-free (§2.1); stage 2
  re-verifies before writing.
- **D2 — ADR dates/status**: Date 2026-10-14 (decision date, ADR 0014
  convention); Status Accepted with an "implemented across chunks 1–4"
  note.
- **D3 — ADR amendments**: exactly five (A1–A5, §2.3), each with its
  reason; decisions 1–5 land as drafted.
- **D4 — architecture.org scope**: six sections amended, everything
  else recorded as non-amendment (§3) — including keeping the
  Versioning text verbatim.
- **D5 — testing_strategy.org**: §7 rewritten + retitled (the
  fictional `slint::testing` reference removed); pin discipline inside
  §7; no other section touched (§4).
- **D6 — backlog**: in-place ✅ DONE (heading suffix + Status line);
  the kiosk Update paragraph retained; new 0013 entry inserted
  chronologically after the last [2026-10-14] entry (§5).
- **D7 — release notes home**: CHANGELOG `[Unreleased]`, added by this
  chunk; promoted to `[0.1.0]` by the user at release time (§6.2/§6.3)
  — not a separate notes file, not GitHub-release-only prose.
- **D8 — bump_version.sh not used for v0.1.0** (the dispatch-plan
  deviation, §6.3): version already 0.1.0 since the initial manifest,
  and the script's on-main commit+tag flow is impossible under branch
  protection — the cut rides `chore/release-v0.1.0` → PR → post-merge
  tag on main's rebased commit; the script resumes duty from v0.2.0
  once the §5.4 adaptation lands.
- **D9 — the release is the user's**: this chunk stops at PR + printed
  commands; the PR description carries §6.3 verbatim. The tag is cut
  strictly after the release PR merges (rebase-merge rewrites SHAs).
- **D10 — the script-adaptation backlog entry** (§5.4): added beyond
  the dispatch's bookkeeping list because the branch-protection
  conflict (user note 2026-10-16) makes the documented release flow
  stale for every future release; AGENTS §7 mandates the append.

---

## 9. Risks & mitigations

1. **Docs/design paraphrase drift** (describing predictions, not the
   landed code) — mitigated: §1 is the verified inventory; stage 3
   spot-checks every claim against the merged tree (dispatch §6
   acceptance).
2. **Concurrent audit round**: the accepted v0.1.0 quality-audit plan
   (2026-10-16) dispatches read-only audit chunks against the **v0.1.0
   tag** and later reconciles the backlog. It runs after this chunk
   merges and the user tags (its §1 fixes the target). If the user
   dispatches audit chunks before tagging, its backlog edits could
   textually collide with §5 — mitigation: this chunk lands first (it
   is the wave's final chunk); the audit's backlog reconciliation
   happens post-tag and re-reads the file.
3. **ADR amendment creep**: stage 2 adding "improvements" to the draft
   beyond A1–A5 — mitigated: §2.3 is exhaustive; dispatch §6 makes
   unrecorded divergence a review finding.
4. **Release-notes inaccuracy** (over/under-claiming the GUI) —
   mitigated: §6.2's five bullets are the contract; stage 3 checks
   each against §1.
5. **Promote-date is a manual argument** (`2026-10-XX`) and, under the
   branch+PR flow, the stamped date is branch-cut day while the
   effective release day is the merge day — acceptable drift if the PR
   sits; the promoted heading makes it human-visible.
6. **The orphaned-tag trap** (rebase-merge): tagging the release
   branch's pre-merge SHA would point the tag at a commit unreachable
   from `main` and break release.yml's merged-before-tag contract —
   mitigated: §6.3 makes the tag the explicit last step, on fetched
   `main`, with the reason printed in the sequence itself.
7. **A real pipeline gap surfaced by §6.1** (e.g. a Windows-leg dep
   issue): out of scope — recorded as a finding with a proposed
   `fix/`/`ci/` prompt; the release sequence in §6.3 is handed over
   with the caveat that the checklist must be green first.

---

## 10. Estimated diff

Five files, docs-only, ≈ 295–375 changed lines:

| File | Change |
|---|---|
| `docs/adr/0015-slint-ui-architecture.org` | new, ~90–110 lines |
| `docs/architecture.org` | 6 sections, ~50–70 lines |
| `docs/testing_strategy.org` | §7 rewrite, ~45–60 lines |
| `.artifacts/taskboard-potential-improvements-backlog.md` | 2 ✅ + 2 new entries, ~70 lines |
| `CHANGELOG.md` | `[Unreleased]` GUI bullets, ~25–35 lines |

Size grade **S** (2–3 dev-days), matching dispatch §6 — mostly writing;
the verification pass (§6.1) is reading.

# [Plan] Settings live-apply honesty — chunk plan (#1515, #1512, #1511)

- **Date**: 2026-11-27
- **Kind**: chunk plan — one branch (`fix/ui-settings-honesty`), one PR,
  three backlog items executed as one small chunk (they touch the same
  two seams and the same settings view; splitting them would three-way
  interleave the same files). Three-stage execution: this planning
  stage, then implementation, then review (prompts below are
  paste-ready and verbatim-binding).
- **Origin**: issues #1515 (typed-Failed send rejection leaves the
  optimistic apply in place), #1512 (the Theme row has no env-locked
  honesty), #1511 (`TASKBOARD_UI_THEME` vs `TASKBOARD_UI__THEME`
  spelling); theme design
  `.artifacts/plans/2026-11-25-theme-support-design.md` (D4/D5/D9
  normative) and journey plan
  `.artifacts/plans/2026-11-27-theme-switch-ux-plan.md` (US-1/US-2/US-3/US-5
  normative). Both artefacts are on `main`.
- **Status**: READY — no gates, no dependencies; dispatchable now.
  Deliberately **not** in this chunk: the Scale row's env-locked
  honesty (pre-existing grammar gap — **issue #1528**) and the
  app-side `SetTheme` guard-key bypass (no `Field::Theme`, no typed
  `EnvLocked` refusal — **issue #1529**).

## 1. Recorded reality (file:line at origin/main, `03274e11`)

### #1515 — the send-rejection honesty gap

- `crates/taskboard-ui-slint/src/adapter/settings.rs:1865`
  `settings_theme_picked`: `crate::palette::apply(window,
  chosen.palette())` (line 1877) runs **before** `self.settings.send(...)`
  (line 1881). On `sent == false` the seam only degrades to
  `SettingsOutcome::Failed` at the choke point (`set_settings_outcome`,
  line 211) — the pushed palette stays and the echo
  (`self.settings.theme`, committed only by the `Ok` reply at line
  141) keeps the persisted value. Rendered state diverges from
  persisted state.
- `crates/taskboard-ui-slint/src/adapter/settings.rs:1906`
  `apply_scale_commit`: `window.global::<crate::Theme>().set_base_scale(...)`
  (line 1913) runs before `send` (line 1915). Same shape on
  `sent == false`: the base-scale stays moved, the echo waits for a
  reply that never comes.
- The `Err`-reply path **does** revert (the D4/D9 law):
  `PendingReply::Theme`'s `Ok(Err(err))` arm (settings.rs:161-168,
  `crate::palette::apply(window, previous.palette())`) and
  `PendingReply::Scale`'s `Ok(Err(err))` arm (settings.rs:172-181,
  `request_scale_anchor` + `set_base_scale(*previous as f32)`).
- The reply-loop `TryRecvError::Closed` arms for Theme (settings.rs:177-181)
  and Scale (settings.rs:194-198) set `SettingsOutcome::Failed` but
  **also do not revert** — the same honesty-gap class as the
  send-rejection path (the reply channel died without a verdict; the
  optimistic apply must go back). In scope: fold both paths into one
  revert law per seam.
- The send seam itself: `crates/taskboard-ui-slint/src/adapter/settings_vm.rs:162`
  `SettingsVm::send` — `requests.try_send(request).is_err()` returns
  `false` (lane full or closed) without parking any pending state, so
  nothing later in the reply loop will ever settle the seam. The revert
  must happen at the call site, synchronously.

### #1512 — the Theme row's env-locked posture

- The precedent grammar: every env-locked row derives from
  `vm.snapshot.env_overridden` (a `BTreeSet<String>` of dotted config
  keys, `crates/taskboard-ui-slint/src/requests.rs:298`), pushed per
  row in `crates/taskboard-ui-slint/src/adapter/settings.rs:963-1101`
  (e.g. `global.set_focus_enabled_env_locked(
  vm.snapshot.env_overridden.contains("focus.enabled"))`, line 1085)
  and consumed in `crates/taskboard-ui-slint/ui/views/settings-view.slint`
  (e.g. `focus-enabled-env-locked` at lines 1517-1533: control
  disabled + a conditional env-locked `RoleText` annotation;
  `GeneralField`'s `env-locked` property at line 534 drives
  `editable: !root.env-locked` + the annotation).
- The set already carries the theme key: `ui.theme` is produced by the
  dotted leaf diff `AppConfig::env_overridden_keys`
  (`crates/taskboard-app/src/config.rs:1392`, seeded at hand-over in
  `crates/taskboard-app/src/ui.rs:333`). No app-side change is needed
  for the row honesty.
- The gap: the Theme row (`settings-view.slint:1444-1464`) renders
  `SegmentedControl { options: ["Dark","Light"], selected-index:
  SettingsModel.theme-index, picked(i) => root.theme-committed(i) }`
  with **no** env-lock property and no annotation; the adapter pushes
  `set_theme_index` (settings.rs:878) but no `theme-env-locked`.
- The write path the row would call: `UiRequest::SetTheme` →
  `set_theme` (`crates/taskboard-app/src/ui_requests.rs:1745`) →
  `overlay_set_ui_theme` — no env-lock guard (see #1529). With the row
  disabled the GUI path is unreachable; the typed-refusal gap stays
  filed as #1529.

### #1511 — the env spelling

- The ladder: `crates/taskboard-app/src/config.rs:2000-2005` —
  `Env::prefixed(ENV_PREFIX).split("__")`. `TASKBOARD_UI_THEME`
  (single underscore) contains no `__`, so figment decodes it as the
  root key `ui_theme`, which no `AppConfig` field binds; figment merges
  unknown keys silently — the override is dropped without complaint.
  The working spelling is `TASKBOARD_UI__THEME` (matching
  `TASKBOARD_UI__SCALE` and every other `[ui]` key).
- The design doc's D5 (`2026-11-25-theme-support-design.md:190,219,221`)
  spells it `TASKBOARD_UI_THEME` — a recorded deviation in the C1-impl
  report; the journey plan (theme-switch-ux-plan.md:41,72,169,219,240)
  already records `TASKBOARD_UI__THEME` as canonical per #1511.
- No user-facing doc in `README.md`/`docs/` mentions either spelling
  (verified by grep at origin/main); the only stale spellings are the
  design doc's three lines.

## 2. Decisions

### D1 — revert-on-failure semantics (#1515)

**The law: the UI never claims what was not persisted — for every lane
exit, not just the `Err` reply.** Concretely:

- **When**: synchronously at the seam on `sent == false` (nothing is
  parked, no later event will arrive); in the reply-drain loop's
  `TryRecvError::Closed` arm (the reply channel died without a
  verdict). The `Ok(Err(_))` arm already reverts.
- **What is restored**: exactly what the optimistic apply moved, to
  exactly the pre-pick value the seam already captured in
  `previous` — for the theme seam, `crate::palette::apply(window,
  previous.palette())`; for the scale seam, `request_scale_anchor(window)`
  + `set_base_scale(*previous as f32)`. The echo
  (`self.settings.theme`) and the scale draft need no touch — neither
  was mutated on these paths (the reply owns the echo).
- **Typed outcome**: unchanged — `SettingsOutcome::Failed` at the
  choke point, as today. The fix adds the revert beside it, not a new
  outcome class.
- **Shape**: two private helpers on the adapter —
  `revert_theme_live(window, previous)` and `revert_scale_live(window,
  previous)` — each called from three sites (the seam's `sent == false`
  branch, the `Ok(Err(_))` arm, the `Closed` arm). One law, one
  helper, no per-site divergence. (Rejected alternative: reverting
  inside `SettingsVm::send` — it has no window access; the adapter
  layer owns rendering.)

### D2 — the Theme row's env-lock representation (#1512)

**Reuse the existing row grammar verbatim** — no new representation:

- `SettingsModel` gains one in-out property `theme-env-locked: bool`
  (default false), pushed in the adapter's env-lock block next to
  `set_theme_index` (settings.rs:878 vicinity) from
  `vm.snapshot.env_overridden.contains("ui.theme")`.
- The view's theme row: when `theme-env-locked`, the
  `SegmentedControl` is disabled (its `picked` must not fire) and a
  conditional `RoleText` annotation renders — the
  `focus-enabled-env-locked` pattern (settings-view.slint:1526-1533),
  not a new widget. The annotation states the posture (env wins over
  the GUI), consistent in *structure* with the sibling rows.
- No snapshot-shape change: `env_overridden` already carries
  `"ui.theme"`. No app-side refusal in this chunk (#1529 owns that).

### D3 — #1511: document the canonical spelling; no alias

**Decision: option (a) — `TASKBOARD_UI__THEME` is the only supported
spelling, plus a boot-time near-miss warning. No figment alias.**

Reasoning:

- figment's `split("__")` nesting is a **global** convention: every
  nested key (`ui.scale`, `ui.due_soon_days`, `reminders.tick_secs`,
  …) uses it. An alias layer that maps only `ui_theme` → `ui.theme`
  makes one key's spelling diverge from every other key's; the
  consistency cost lands on every future config key and every doc
  sentence about the ladder.
- The alias machinery (a custom `figment::Provider` renaming the
  key between `Toml` and `Env` merges) is speculative generality for a
  typo in one design document — the A1 house policy (AGENTS §2) says
  machinery enters when shapes stop sufficing; a documentation
  correction suffices.
- Conflict check: an alias would also be precedence-fragile —
  `TASKBOARD_UI_THEME` decodes under the *root* (`ui_theme`), outside
  the `[ui]` subtree a `Provider` rename would have to splice into,
  and a root-level `ui_theme` key already merges silently today. The
  rename provider would silently change behaviour for configs that
  (accidentally) set the root key.
- **What ships instead**: (i) the design doc's D5 lines
  (190, 219, 221) corrected to `TASKBOARD_UI__THEME` with a
  one-line recorded-correction note citing #1511; (ii) a boot-time
  `tracing::warn!` when `TASKBOARD_UI_THEME` is set (and
  `TASKBOARD_UI__THEME` is not), so an operator copying the old
  spelling is told at boot instead of being silently ignored. The
  warning lives in `taskboard-app` (config reading is the app crate's
  role, AGENTS §2); its detection core is a small pure function
  returning the near-miss names (structurally testable — see T3), and
  the `warn!` itself is a one-line consumer (log text is never
  asserted, AGENTS §5).

### D4 — UX-plan decision per issue

- **#1515 — amend the theme-switch journey plan.** Rule cited:
  `docs/ux_planning.org` §"Mandatory (activated 2026-11-16)" — every
  new user-visible capability includes (or cites) an accepted UX
  journey plan; #1515 changes the user-perceivable escape behaviour of
  a *landed* journey (a failed live-apply now visibly reverts instead
  of wedging the palette). The honest answer is an **amendment** to
  `.artifacts/plans/2026-11-27-theme-switch-ux-plan.md`, not a new
  plan: US-1/S2's and US-2/S1's escape cells currently record the
  typed-`Failed`-without-revert posture this issue refines. Stage 2
  amends: US-1/S2 + US-2/S1 escape cells (send rejection reverts to
  `previous`, typed `Failed`), US-3 gains a step (US-3/S4: the
  send-rejected/Closed pick — revert is the terminal state), and the
  journey test names embed the amended step tokens. The scale twin is
  **not** journey-amended: its journey plan
  (`.artifacts/plans/2026-11-06-app-live-settings-plan.md`) predates
  the 2026-11-16 activation (advisory era — no amendment obligation);
  the shared law is recorded once in this plan §D1.
- **#1512 — same amendment.** The env-locked row posture is
  user-perceivable (a disabled, annotated control); the journey plan
  gains a story **US-6 — the env-locked Theme row** (kiosk posture:
  under `TASKBOARD_UI__THEME` the GUI pick is structurally refused
  before it happens) with P2 steps, and US-5/S2's cell notes the
  GUI-side consequence. Coverage gate applies: `scripts/ux-coverage.sh
  .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate` must be
  green on the claimed rows before stage 2 reports done.
- **#1511 — no UX journey plan.** Configuration spelling is not a
  user-visible capability under `docs/ux_planning.org`'s definition
  ("any new verb, badge, view, or setting a human can perceive or act
  on" — no surface delta; the env override already exists and works).
  Pure configuration DX, exempt.

## 3. Invariants (stage 2 must hold, stage 3 must verify)

1. **The revert law is total**: for both seams (theme, scale), every
   lane exit that follows an optimistic apply reverts it — `Ok(Ok)`
   commits, `Ok(Err)` reverts (existing), `Closed` reverts (new),
   send-rejection reverts (new). No exit leaves the optimistic apply
   standing.
2. **The reply still owns the echo**: a send-rejected or `Closed` pick
   never mutates `self.settings.theme`; the echoed index changes only
   on the `Ok` reply (the preset contract, journey US-1/S3).
3. **One revert helper per seam**, used by all three sites — no
   duplicated revert bodies (single level of abstraction; a future
   third live-apply seam copies the shape).
4. **The choke point stays single**: every outcome assignment still
   routes through `set_settings_outcome` (r2 D28) — the new branches
   assign no outcome directly.
5. **The env-lock grammar is reused, not forked**: the theme row's
   posture is driven by `env_overridden.contains("ui.theme")` pushed
   through a `SettingsModel` property, exactly like the sibling rows;
   no new config read, no UI-side env inspection (config reading lives
   in `taskboard-app`, AGENTS §2).
6. **No production behaviour change when env is not set**: without
   `TASKBOARD_UI__THEME` the row renders enabled and behaves exactly
   as today; without a lane failure the seams behave exactly as today.
7. Crate boundaries: all Rust changes live in `taskboard-ui-slint`
   (adapter + view + tests) and `taskboard-app` (the near-miss warning
   only); no new dependencies; workspace-pinned versions only.

## 4. Test catalogue (lowest layer, AGENTS §5 — typed outcomes, no text assertions, no clock dependence)

Existing tests that **must not be weakened**:
`crates/taskboard-ui-slint/tests/journeys.rs::a_theme_pick_routes_one_set_theme_and_an_err_reply_reverts_the_palette`
(the Err-reply revert, line 3137) and the scale commit journeys
(`a_scale_commit_anchors_the_scroll_on_the_scale_row` and the echo/draft
journeys around it); `crates/taskboard-app/src/config.rs`'s
`env_overridden_keys_truth_table` (line 2207). None of these may be
relaxed, renamed-away, or re-scoped to the new behaviour.

New tests:

- **T1 (journey, theme send-rejection)** — `tests/journeys.rs`: with
  the request lane closed, `invoke_settings_theme_picked(1)` asserts:
  background stays the previous palette (typed `Color` comparison, the
  existing journey's helper), the echo index keeps the persisted value,
  the outcome property reads `Failed` (typed enum via the
  `SettingsModel` outcome getter), and no pending state is left (a
  follow-up pick routes a fresh request). Lane-closure mechanism: the
  harness owns `requests_rx` (`tests/support/harness.rs:144,184`) —
  add a test-helper method that drops the stored receiver
  (`close_request_lane()`), closing the lane for the remainder of that
  journey only.
- **T2 (journey, scale send-rejection)** — same shape for the scale
  commit: closed lane, commit `3.0`, assert `base_scale` reads the
  previous value and the outcome reads `Failed`. Reuse the existing
  scale commit journeys' driving (field commit + drain).
- **T3 (unit, near-miss detector)** — `crates/taskboard-app/src/config.rs`
  unit module: the pure near-miss function returns the single-underscore
  spelling when set-and-canonical-absent, returns empty when the
  canonical form is set, and returns empty when neither is set (env
  manipulated per test, no cross-test leakage).
- **T4 (unit/structural, env-locked push)** — the adapter's env-lock
  push for `theme-env-locked`: a snapshot whose `env_overridden`
  contains `"ui.theme"` pushes `true`; without it, `false` (the same
  layer the sibling rows' pushes are covered at; mirror that existing
  coverage's layer — if the siblings are covered through a
  `push_settings` unit, follow that precedent, do not invent a new
  one).
- **T5 (journey, env-locked row posture)** — a snapshot seeded with
  `env_overridden: {"ui.theme"}` renders the row disabled and the
  annotation present; a pick attempt through the seam is structurally
  inert (no `UiRequest::SetTheme` drained). Asserted structurally
  (control's enabled state / annotation visibility binding, request
  count), never by label text.
- Journey-step tokens: T1 → `US-3/S4`, T2 → the scale law (no journey
  token — pre-activation plan; name it for the D1 law), T5 → `US-6/S1`.

## 5. Verification commands (expected outcomes)

```sh
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journeys a_theme_pick   # existing Err-revert journey + T1 green
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journeys                # full UI journey suite green
scripts/memory-gate.sh cargo nextest run -p taskboard-app theme                               # T3 green (near-miss detector)
scripts/memory-gate.sh cargo clippy -p taskboard-ui-slint -p taskboard-app --all-targets -- -D warnings   # clean
scripts/ux-coverage.sh .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate             # PASS on the claimed rows (US-3/S4, US-6/*)
scripts/run-quality-gates.sh cargo nextest run --workspace                                    # the stage gate (flock-safe)
```

Full §10 suite is the **dispatcher's** between stage 2 and stage 3 and
CI's on the PR — never inside a subagent (discipline, item 1).

## 6. Diff-shape rules

- Allowed: `crates/taskboard-ui-slint/src/adapter/settings.rs`
  (revert helpers + three call sites per seam + the `theme-env-locked`
  push), `crates/taskboard-ui-slint/src/presentation.rs` or the model
  push path *only if* the property routing needs it,
  `crates/taskboard-ui-slint/ui/views/settings-view.slint` (row
  posture + annotation), `crates/taskboard-ui-slint/SettingsModel`
  slint property, `crates/taskboard-ui-slint/tests/{journeys.rs,
  support/harness.rs}`, `crates/taskboard-app/src/config.rs` (near-miss
  detector + boot wiring), the two `.artifacts/plans/*.md` amendments,
  this plan's companions.
- Not allowed: changes to `taskboard-domain`, `taskboard-state`, any
  sync/storage crate, `Cargo.toml` dependency edits, new outcome
  variants, new `Field` variants (#1529 owns that), snapshot-shape
  changes, slint widget additions (reuse `RoleText`), status/log text
  changes asserted anywhere.
- Every commit standalone (`<type>(<module>): <desc>` + body;
  rebase-merge hygiene). Suggested split: `fix(ui)` the revert law
  (#1515), `feat(ui)` the env-locked row (#1512), `feat(app)` +
  `docs(plan)` the spelling correction + warning (#1511) — or one
  coherent commit if the interleaving is small; no WIP noise either way.

## 7. Rulings requested

- **R1 — #1511 alias vs documentation (D3)**: recommended default is
  **documentation + near-miss boot warning** (option a). The
  alternative (a figment rename provider aliasing `TASKBOARD_UI_THEME`)
  proceeds only on an explicit operator ruling that the design doc's
  spelling is contractual. **Proceeded on the default.**
- **R2 — `Closed`-arm reverts in scope (D1)**: the issue names only
  the send-rejection path; the `TryRecvError::Closed` arms have the
  identical honesty gap. Default: **both in scope** (one law, one
  helper). If the operator wants the minimal issue-literal diff, drop
  the two `Closed`-arm changes. **Proceeded on the default.**
- **R3 — scale journey amendment**: the scale seam's journey plan
  (2026-11-06) predates the UX mandate, so no amendment is made.
  Default: **no amendment**; the law is recorded here. **Proceeded on
  the default.**
  **OVERRULED by the operator (2026-11-27):** the scale journeys' home
  plan (`.artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md`,
  §1.4) is amended — its §11 records the total revert law and the pins;
  the law is about the user-visible change, not the plan's age.

## 8. Prompt 2 (stage 2 — IMPLEMENTATION) — paste-ready, verbatim

```text
You are the stage-2 IMPLEMENTATION agent for the chunk "settings live-apply honesty" (branch fix/ui-settings-honesty) in the taskboard repo, working in /home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-ui-settings-honesty (same directory as /home/viktoria/taskboard-WT-fix-ui-settings-honesty). Recreate the worktree on that branch if missing (git worktree add ../taskboard-WT-fix-ui-settings-honesty fix/ui-settings-honesty from the main checkout /home/viktoria/src/taskboard; if the branch's PR already landed, create fix/ui-settings-honesty-v2 off fetched origin/main per AGENTS §8 — never git worktree add <path> main).

**Long-command discipline (binding, restate-verbatim law)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.*

1. Read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-27-settings-honesty-plan.md (normative: §2 decisions D1–D4, §3 invariants, §4 test catalogue, §6 diff-shape rules), the origin issues (#1515, #1512, #1511), the theme design .artifacts/plans/2026-11-25-theme-support-design.md (D4/D9), and the journey plan .artifacts/plans/2026-11-27-theme-switch-ux-plan.md (US-1/US-2/US-3/US-5, to be amended per D4).
2. Implement exactly per the approved plan, TDD where code is involved, every commit standalone (rebase-merge hygiene). Concretely: (a) #1515 — private revert helpers revert_theme_live / revert_scale_live on the adapter in crates/taskboard-ui-slint/src/adapter/settings.rs, wired at THREE sites each: the seam's sent == false branch (settings_theme_picked ~line 1888, apply_scale_commit ~line 1921), the PendingReply::Theme and PendingReply::Scale Ok(Err(_)) arms (~lines 161-181), and their TryRecvError::Closed arms (~lines 177-181, 194-198); outcomes still route only through set_settings_outcome; the echo (self.settings.theme) is never mutated on the failure paths. (b) #1512 — SettingsModel gains theme-env-locked: bool, pushed from vm.snapshot.env_overridden.contains("ui.theme") beside set_theme_index (~settings.rs:878); the theme row in crates/taskboard-ui-slint/ui/views/settings-view.slint (~lines 1444-1464) disables the SegmentedControl and renders the conditional env-locked RoleText annotation exactly per the focus-enabled-env-locked pattern (settings-view.slint:1517-1533). (c) #1511 — NO alias: correct the three TASKBOARD_UI_THEME spellings in .artifacts/plans/2026-11-25-theme-support-design.md (lines 190, 219, 221) to TASKBOARD_UI__THEME with a one-line recorded-correction note citing #1511, and add the pure near-miss detector + boot-time tracing::warn! in crates/taskboard-app/src/config.rs per plan D3. (d) Amend the journey plan .artifacts/plans/2026-11-27-theme-switch-ux-plan.md per plan D4: US-1/S2 + US-2/S1 escape cells (send rejection reverts, typed Failed), new US-3/S4 step, new US-6 story (env-locked Theme row) with steps, US-5/S2 note.
3. Tests per the plan §4 catalogue: T1/T2 journey tests via a new close_request_lane() test-helper on Harness (tests/support/harness.rs:144,184 — drops the stored requests_rx); T3 unit test on the pure near-miss detector; T4/T5 per the catalogue's layer precedent. Typed outcomes only; no text assertions; no sleeps (tokio::time::pause/advance if any timing is involved); journey test names embed the step tokens US-3/S4 and US-6/S1. Do NOT weaken a_theme_pick_routes_one_set_theme_and_an_err_reply_reverts_the_palette, the scale commit journeys, or env_overridden_keys_truth_table — they are the regression oracle for the paths you extend.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate and make it PASS on the claimed rows (US-3/S4, US-6/*) — MISSING on a claimed row is not done. Then run the stage gate: scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; flock-deferral rule above applies). Every cargo invocation goes through scripts/memory-gate.sh.
5. SPDX headers on any new .rs file (AGPL-3.0-only for taskboard-ui-slint and taskboard-app); #![forbid(unsafe_code)] already crate-level. Respect the crate boundaries: no network/IO in the UI crate beyond what exists; config reading stays in taskboard-app.
6. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report. Known pre-filed out-of-scope homes: #1528 (scale row env-lock honesty), #1529 (SetTheme guard-key bypass) — comment on these rather than duplicating if you learn something relevant.

Report: per-gate results, the amended journey rows, any deviation from the plan and why.
```

## 9. Prompt 3 (stage 3 — REVIEW) — paste-ready, verbatim

```text
You are the stage-3 REVIEW agent for the chunk "settings live-apply honesty" (branch fix/ui-settings-honesty), working in /home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-ui-settings-honesty.

**Long-command discipline (binding, restate-verbatim law)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.*

1. Begin by rebasing onto main: git fetch origin --prune, rebase the branch onto origin/main (AGENTS §8's rebase dance: drop already-merged duplicate commits; resolve only real conflicts), push --force-with-lease if the rebase rewrote history. Then read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-27-settings-honesty-plan.md (§3 invariants and §6 diff-shape rules are your acceptance criteria), the origin issues (#1515, #1512, #1511), and the amended journey plan; then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD).
2. Functional focus (this chunk's specific traps): (a) the revert law is total — check ALL FOUR lane exits per seam in crates/taskboard-ui-slint/src/adapter/settings.rs: settings_theme_picked and apply_scale_commit's sent == false branches, PendingReply::Theme/Scale's Ok(Err(_)) arms, and their TryRecvError::Closed arms; a missing exit is a BLOCKER. (b) the echo (self.settings.theme) changes only on the Ok reply — a failure path that touches it breaks the preset contract. (c) every outcome assignment routes through set_settings_outcome — a direct .outcome = is a finding. (d) the env-locked push reads vm.snapshot.env_overridden.contains("ui.theme") — any UI-side env inspection or new config read is a boundary violation (config reading lives in taskboard-app, AGENTS §2). (e) #1511 ships documentation + warning, NOT a figment alias — an alias provider on the branch contradicts recorded decision D3. (f) the tests that must not have been weakened: a_theme_pick_routes_one_set_theme_and_an_err_reply_reverts_the_palette, the scale commit journeys in tests/journeys.rs, env_overridden_keys_truth_table in crates/taskboard-app/src/config.rs — verify none was relaxed, re-scoped, or asserted weaker. (g) no text assertions anywhere in the new tests (typed outcomes and structural facts only), no sleeps, no wall-clock dependence.
3. **Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client; config reading lives in taskboard-app; (c) a single level of abstraction per function body. Findings are fixed on the branch.
4. Re-run the stage gate yourself: scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; flock-deferral rule above applies), plus scripts/memory-gate.sh cargo clippy --workspace --all-targets -- -D warnings and scripts/memory-gate.sh cargo fmt --all -- --check; any failure is a finding. Re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate; MISSING on a claimed row is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups; every commit stands alone for the rebase merge).
6. Ship — your scope ENDS at PR-open: push the branch, open the PR to main with gh (body: closes #1515, closes #1512, closes #1511; summary of D1–D4), and report the URL. Do NOT run gh pr checks --watch, do NOT verify mergeStateStatus, do NOT remove the worktree — the dispatcher owns the CI watch, the CLEAN verification, teardown, and at-merge issue closures. main is branch-protected: never merge; the user merges on GitHub.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report. Known pre-filed homes: #1528, #1529 — comment, don't duplicate.

Report: verdict, findings (functional + mandate), what you fixed, PR URL.
```

## 10. Completion bookkeeping

- The PR body closes #1515, #1512, #1511 at merge (the dispatcher owns
  the at-merge closures). #1528 and #1529 stay open as this chunk's
  recorded out-of-scope neighbours.
- The design-doc correction (D5 lines) and the journey-plan amendment
  land on this branch — both artefacts live in `.artifacts/plans/` and
  are versioned with the code they describe.
- Post-merge: nothing gated on this chunk; #1528/#1529 re-enter the
  unscheduled queue (no milestone — unscheduled per AGENTS §7).

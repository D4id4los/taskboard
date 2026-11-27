# [Plan] ux-coverage evidence-root honesty + Scale row env-lock — chunk plan (#1531, #1528)

- **Date**: 2026-11-27
- **Kind**: chunk plan — one branch (`fix/coverage-root-scale-envlock`),
  one PR, two backlog items executed as one small chunk. The issues
  share nothing but size: each is a small, independently reviewable
  fix in a disjoint file set, so they ship as **two commits on one
  branch / one PR** (`fix(ci)` the script root, `feat(ui)` the scale
  row) — splitting into two PRs would cost two review cycles for
  ~200 net lines. Three-stage execution: this planning stage, then
  implementation, then review (prompts below are paste-ready and
  verbatim-binding).
- **Origin**: issues #1531 (ux-coverage.sh silently scans the main
  checkout when invoked from a worktree without `--root`) and #1528
  (the settings Scale row has no env-locked honesty); the landed
  Theme-row precedent (PR #1535, #1512) as recorded in
  `.artifacts/plans/2026-11-27-settings-honesty-plan.md` (D2/D4 and
  its R3 OVERRULING are normative for this chunk's shapes).
- **Status**: READY — no gates, no dependencies; dispatchable now.
  Deliberately **not** in this chunk: the app-side `SetTheme`
  guard-key bypass (**#1529**, still open), any broader `ux-coverage`
  machinery beyond the root resolution and its self-test pin.

## 1. Recorded reality (file:line at origin/main, `76e78b48`)

### #1531 — the default evidence root resolves from the script's repo

- `scripts/ux-coverage.sh:956-958` (in `main()`):
  `root="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"`
  — the default root is the **toplevel of the directory the script
  file lives in**, never the caller's checkout. An agent in a
  `taskboard-WT-*` worktree invoking the main checkout's script by
  absolute path gets the main checkout's sources as the evidence
  universe: worktree-only tests are invisible (claimed rows read
  MISSING) or, worse, the gate validates against stale main-checkout
  sources and accepts a false green. Two agents hit this today;
  one nearly accepted a false green (#1531).
- The root is consumed downstream through the `#1073`/`#1186`
  canonicalization at `scripts/ux-coverage.sh:984-991`
  (`script_dir` via `pwd -P`; `root=$(cd "$root" && pwd -P)`;
  `EVID_ROOT=$root`; the script-relative `FX_ROOT` fixture prune).
  Any fix only has to produce a better *initial* `root` — the
  canonicalization and prune stay untouched.
- The `--root` option exists (`scripts/ux-coverage.sh:933`) and
  works; the bug is only the default.
- The self-test matrix is at **27 cases** (`self_test`,
  `scripts/ux-coverage.sh:445-923`; case 27 is the #1514 modular
  adapter tree). Every existing invocation inside `self_test` passes
  an explicit `--root` (fixture tree or a constructed temp repo) —
  **no current self-test exercises the no-`--root` default**, which
  is why the bug survived.
- Invocation guidance lives in: `docs/ux_planning.org:167-168`
  (the usage block), `docs/ux_planning.org:141` and `:315` (the
  gate's role rows), `.artifacts/templates/dispatch-plan.md:171,198`
  (the stage-2/3 mandates: `scripts/ux-coverage.sh <plan> --gate` —
  note: no `--root`), and
  `.artifacts/templates/ux-journey-plan.md:141` (the author
  checklist). The template mandates advise the bare form, which is
  exactly the form that lies from a worktree today.
- Precedent law in the same script: #1073 (realpath both roots —
  invoked spellings lie under symlinks), #1186 (relative `--root`
  spellings must canonicalize before the prune), #967/#970 (the
  evidence-scoped fixture/build-output prune). The fix must not
  disturb any of these; self-tests 1–27 pin them.

### #1528 — the Scale row renders no env-locked posture

- The snapshot fact exists: `vm.snapshot.env_overridden` (a
  `BTreeSet<String>` of dotted config keys) already carries
  `"ui.scale"` when `TASKBOARD_UI__SCALE` is set — the same
  `AppConfig::env_overridden_keys` leaf diff that carries
  `"ui.theme"` (`crates/taskboard-app/src/config.rs`, seeded at
  hand-over in `crates/taskboard-app/src/ui.rs`). No app-side change
  is needed for the row honesty.
- The landed Theme-row pattern (#1512, PR #1535) to mirror verbatim:
  - Push site: `crates/taskboard-ui-slint/src/adapter/settings.rs:904`
    — `global.set_theme_env_locked(self.settings.snapshot.env_overridden.contains("ui.theme"))`,
    inside the mount-time push block beside `set_theme_index`
    (settings.rs:903), with the comment stating the constraint (same
    snapshot fact as the sibling rows, never a UI-side env read).
  - Model property: `crates/taskboard-ui-slint/ui/views/settings-view.slint:228-231`
    — `in-out property <bool> theme-env-locked: false;` with the
    constraint comment.
  - View posture: `settings-view.slint:1444-1495` — the segmented
    control renders `segment-enabled: [!theme-env-locked, !theme-env-locked]`
    (lines 1471-1472) and the conditional annotation
    `if SettingsModel.theme-env-locked: RoleText { … text: "Overridden
    by an environment variable; edit the file or the env."; text-color:
    Semantic.warning }` (lines 1481-1485) — the focus-enabled pattern.
- The Scale row today (`settings-view.slint:1361-1441`): a
  `RoleText` title ("Interface scale"), the `scale-row` layout with
  the "Scale (0.5–3.0)" label + `help-scale` InfoTip, the project
  `Slider` primitive (`crates/taskboard-ui-slint/ui/widgets/slider.slint:26`,
  label "Scale slider", `value: Theme.base-scale`, `pressed()` →
  `root.scale-pressed()`, `committed(v)` → `root.scale-released(v)`),
  and a `TextField` (label "Scale", `text <=> SettingsModel.scale-echo`,
  `accepted(t)` → `root.scale-committed(t)`, `focus-lost()` →
  `root.scale-focus-lost()`), plus the conditional error
  `StatusNotice`. **None** of it reads any env-lock property.
- The commit choke point: `apply_scale_commit`
  (`crates/taskboard-ui-slint/src/adapter/settings.rs:1932`) — both
  the slider-release path (`settings_scale_released` →
  `apply_scale_commit`) and the field path (`settings_scale_committed`
  → `apply_scale_commit`, settings.rs:1995) funnel through it. With
  `TASKBOARD_UI__SCALE` set, a GUI commit today persists to
  `[ui] scale` and live-applies for the session; the env value wins
  again at next boot with **no structural signal and no typed
  refusal** — a silent absorb, against the T-16 doctrine.
- The defence-arm precedent for typed refusals at commit seams:
  the general-commit arm (`settings.rs:2052-2066`: snapshot contains
  the row key → `SettingsOutcome::Rejected` +
  `SettingsError::EnvLocked { field }` via `set_settings_outcome`)
  and the choice-commit arms (settings.rs:2200-2215,
  2256-2268). `Field::Scale` already exists
  (`crates/taskboard-ui-slint/src/requests.rs:200` enum `Field`,
  variant `Scale` at line 205) — unlike the Theme row (whose missing
  variant is #1529), **no new `Field` variant is needed** for a typed
  scale refusal.
- Widget capability facts (decide the disabled rendering):
  - `TextField` (`crates/taskboard-ui-slint/ui/widgets/text-field.slint:45`)
    already has `in property <bool> read-only` — the field half needs
    no widget change.
  - The `Slider` primitive (`ui/widgets/slider.slint:26-42`) has
    **no** `enabled`/lock property — its gesture entries are
    `pressed()` (the FocusScope's pointer press and `key-step`'s
    first stroke both open the gesture), `preview`, `committed`. An
    env-locked slider needs a new `in property <bool> enabled: true`
    (or equivalently named lock) on the primitive guarding every
    commit entry point.
- Tests that must not be weakened (the scale regression oracle):
  `crates/taskboard-ui-slint/tests/journeys.rs` —
  `the_scale_drag_moves_nothing_until_release`,
  `the_scale_echo_renders_the_committed_scale`,
  `a_scale_commit_anchors_the_scroll_on_the_scale_row`,
  `a_scale_parse_miss_renders_the_invalid_flavour_and_typed_detail`,
  `a_scale_send_rejection_reverts_the_live_scale_and_reports_failed`,
  `settings_scroll_reaches_the_scale_controls_at_scale_3`; plus the
  slider grab probes (`slider_grab_probe_tests.rs` p1/p2/p3).
- UX-journey home: the scale journeys live in
  `.artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md`
  (§1.4; §11 is today's #1515 amendment — the operator's ruling that
  the plan is amended despite predating the 2026-11-16 activation,
  "the law is about the change, not the plan's age"). That plan is
  **not** in the ux-journey template format (no `## 2.`/`## 4.`/`## 6.`/
  `## 7.` anchors), so `scripts/ux-coverage.sh` cannot parse it —
  the coverage gate cannot be mandated against it (the #1512 chunk
  could run the gate only because the theme journeys had a
  template-format plan, `2026-11-27-theme-switch-ux-plan.md`).

## 2. Decisions

### D1 — #1531: derive the default root from the caller's toplevel; notice on divergence

**Decision: option (a) — when `--root` is omitted, the evidence root
is the git toplevel of the *caller's* working directory
(`git -C "$PWD" rev-parse --show-toplevel`), falling back to the
script's own repo toplevel only when `$PWD` is outside any git repo.
Plus a one-line stderr notice when the resolved root differs from the
script's repo toplevel. NOT option (b) (refuse on mismatch).**

Reasoning:

- The silent-failure class is "the gate scanned a different checkout
  than the one whose plan is under test". Deriving from `$PWD` fixes
  the class: from the main checkout the behaviour is byte-identical
  to today (toplevel of `$PWD` == toplevel of the script's repo);
  from a worktree (any subdirectory of it — `git rev-parse` resolves
  the toplevel from the cwd) it scans the worktree; from a third repo
  entirely it scans that repo, which is the honest interpretation of
  "the checkout I am standing in".
- Refusal-on-mismatch (option b) also fixes the class but at a
  cost: it breaks the legitimate main-checkout invocation from an
  arbitrary cwd only because the *script* happens to live in a
  different checkout on multi-checkout boxes, and it converts the
  second-priority honest case (a deliberately foreign cwd) into a
  hard error where a correct scan is available. The stderr notice
  keeps the honesty (the run announces whose tree it scanned) without
  the cliff.
- Edge cases, recorded:
  - **Subdirectory cwd**: `git rev-parse --show-toplevel` from any
    subdirectory of the checkout resolves the same toplevel —
    correct, no special handling.
  - **Symlinked invocation path / cwd**: `git rev-parse --show-toplevel`
    already returns the canonical toplevel, and the existing
    `#1073`/`#1186` pass (`root=$(cd "$root" && pwd -P)`,
    scripts/ux-coverage.sh:985) canonicalizes again downstream — the
    fix inherits both disciplines untouched.
  - **CI**: a single checkout; `$PWD`'s toplevel == the script repo's
    toplevel → behaviour identical to today, no notice printed.
  - **Outside any git repo** (e.g. a bare scratch dir): the `git`
    call fails; fall back to the script's repo toplevel — the
    historical behaviour, preserved for `--self-test`-style and
    ad-hoc invocations.
  - **The fixture-based self-tests**: every existing case (1–27)
    passes an explicit `--root` (fixture tree or constructed temp
    repo), so the default path is never taken by cases 1–27 —
    expectations stay byte-stable. Case 16 `cd`s into its repo but
    still passes `--root .` explicitly. The new case 28 (below)
    exercises the default on purpose.
- **The notice is informational stderr**, never a text assertion
  target (the self-test asserts exit codes and row counts only —
  the header law at scripts/ux-coverage.sh:440-443).
- **Doc edits** (the invocation guidance must match the new
  default): `docs/ux_planning.org` §2.3's usage block (lines
  167-168 vicinity) gains one sentence: the default evidence root is
  the invoking checkout's git toplevel (the caller's `$PWD`), not
  the script's own repo; pass `--root` to pin. The script's own
  usage text (`scripts/ux-coverage.sh:55-56` and the `usage()`
  heredoc lines 75-76) is corrected the same way.
  `.artifacts/templates/dispatch-plan.md:171,198` and
  `.artifacts/templates/ux-journey-plan.md:141` need **no edit**:
  their bare `scripts/ux-coverage.sh <plan> --gate` form becomes
  correct by the new default — that is the point of the fix.

### D2 — #1531: self-test case 28 pins the default root

One new case appended to `self_test` (matrix grows 27 → 28; cases
1–27 byte-stable in expectations):

- **28. #1531 worktree-cwd default root**: build `tmp/repo28` as
  today's copied-script cases do (`scripts/ux-coverage.sh` +
  `scripts/fixtures/ux-coverage-fixture` + `tests/fixture_journeys.rs`
  with the honest bound-test fns), and `tmp/wt28` as a second git
  repo whose `tests/fixture_journeys.rs` carries the same fns (the
  "worktree" stand-in — a real `git worktree add` is unnecessary; the
  pinned fact is *a different checkout's toplevel under the cwd*,
  which any second repo proves). Then `cd "$tmp/wt28" && bash
  "$repo28/scripts/ux-coverage.sh" "$fx/plan-targetpin.md" --gate`
  with **no `--root`**: the bound row must read BOUND out of
  `wt28/tests/fixture_journeys.rs` (exit 0) — pre-fix this reads
  MISSING (the script scanned repo28, whose `tests/` lacks the fns…
  and whose fixture-pruned universe cannot credit them). Assert exit
  code and row counts only (the BOUND-count grep pattern of cases
  1/5). The same invocation from `cd "$repo28"` (cwd == script repo)
  is already pinned transitively by the no-`--root` fallback path
  staying exercised — one case, one behaviour (AGENTS §5: one
  behavior, one test, lowest layer).

### D3 — #1528: the Scale row's env-lock mirrors the Theme-row grammar exactly

**Reuse the landed #1512 shape; no new representation:**

- `SettingsModel` gains one in-out property `scale-env-locked: bool`
  (default false), with the constraint comment, beside
  `theme-env-locked` (`settings-view.slint:231` vicinity).
- Push site: the adapter's mount-time push block, beside
  `set_theme_env_locked` (`settings.rs:904` vicinity) —
  `global.set_scale_env_locked(self.settings.snapshot.env_overridden.contains("ui.scale"))`,
  same comment constraint (same snapshot fact, never a UI-side env
  read — config lives in the app crate, AGENTS §2).
- View posture (`settings-view.slint:1361-1441`), **all three
  controls lock** (the issue's explicit question):
  1. the `Slider` gets the primitive's new lock property (D4) bound
     to `!SettingsModel.scale-env-locked` — the slider is a commit
     surface, it must not stay live;
  2. the `TextField` gets `read-only: SettingsModel.scale-env-locked`
     (the widget's existing property — no change to text-field.slint);
  3. the conditional `RoleText` annotation renders the same posture
     sentence as the Theme row ("Overridden by an environment
     variable; edit the file or the env.", `Semantic.warning`,
     `Typography.caption`) — identical structure to
     `settings-view.slint:1481-1485`. The InfoTip and the error
     `StatusNotice` stay untouched (help and refusal rendering are
     not commit surfaces).
- **Typed defence arm at the choke point** (in scope, unlike
  #1512's view-only fix): `apply_scale_commit`
  (`settings.rs:1932`) opens with the general-commit defence shape —
  when `self.settings.snapshot.env_overridden.contains("ui.scale")`,
  emit `tracing::debug!("scale commit on an env-locked row; refused
  typed")` and `set_settings_outcome(window, Rejected,
  Some(SettingsError::EnvLocked { field: Field::Scale }))`, return.
  Reasoning: both GUI commit paths funnel through this one function
  (settings.rs:1995 and the release path), `Field::Scale` already
  exists (requests.rs:205 — no #1529-style prerequisite), the
  T-16 doctrine says never a silent absorb, and defence-in-depth is
  the house shape for every other row's commit seam. The disabled
  controls make the arm unreachable through the GUI; it exists for
  the seam-level honesty the sibling rows already carry.

### D4 — the Slider primitive gains an `enabled` property

**Decision: add `in property <bool> enabled: true;` to the Slider
primitive (`ui/widgets/slider.slint`), guarding every commit entry
point, and render the locked state dimmed. NOT view-level callback
gating.**

Reasoning: the honest disabled posture is a *visual* fact (a greyed
thumb), not just an inert callback; a view-level `if` around the
callbacks would leave the control looking live while doing nothing —
the exact dishonesty class #1528 files. Mechanically: when
`!enabled`, the pointer press does not open the gesture (no
`pressed()` emission), `key-step` returns without opening one, and
consequently no `preview`/`committed` fires (they only fire inside a
gesture — verify in stage 2 that every commit entry routes through
the gesture-open guard; the grab probes p1/p2/p3 are the oracle that
the unlocked gesture is unchanged). One new property, default true —
every existing call site (only the scale row today) is unaffected.

### D5 — UX-plan decision (#1528): amend the scale journeys' home plan, §12

**Decision: a dated `## 12. Amendment — the Scale row's env-locked
posture (2026-11-27, #1528)` section in
`.artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md`,
NOT a new UX journey plan.**

Reasoning: the env-locked row is user-perceivable (a disabled,
annotated control), so the AGENTS §7 mandatory law applies — but the
operator's R3 OVERRULING in the settings-honesty chunk
(`2026-11-27-settings-honesty-plan.md` §7) already fixed the pattern
for exactly this situation: the scale journeys' home plan is amended
even though it predates the activation ("the law is about the
user-visible change, not the plan's age"). §11 set the template
today. Stage 2 amends: a short behaviour statement (under
`TASKBOARD_UI__SCALE` the slider and the field render disabled with
the annotation; a commit attempt through the seam is refused typed
`EnvLocked{Field::Scale}`), the success path unchanged note, and the
pins (T6/T7 below). **No coverage-gate mandate**: the r4 plan is not
in the ux-journey template format (no §2/§4/§6/§7 anchors), so
`scripts/ux-coverage.sh` cannot parse it — unlike #1512 (which had
`2026-11-27-theme-switch-ux-plan.md` to gate against), this chunk
records the posture in the home plan and pins it in code tests; the
gate is simply not applicable, and no gate run is faked.

## 3. Invariants (stage 2 must hold, stage 3 must verify)

1. **Self-tests 1–27 byte-stable in expectations**: the #1531 change
   touches only the no-`--root` default derivation; every existing
   case passes explicit `--root`. No existing case's expectations,
   fixture files, or comment contracts may change; case 28 is purely
   additive.
2. **No prune/canonicalization regression**: the `#1073`/`#1186`
   realpath pass (`scripts/ux-coverage.sh:984-991`) and the
   `FX_ROOT` fixture prune operate on whatever root D1 produces —
   the fix must not reorder or duplicate that logic.
3. **No test weakened**: the scale journey oracle list (§1) and the
   slider grab probes stay intact — none relaxed, renamed-away, or
   re-scoped. The new lock must leave every unlocked-path journey
   green unchanged.
4. **The env-lock grammar is reused, not forked** (settings-honesty
   D2's invariant, inherited): the scale row's posture is driven by
   `env_overridden.contains("ui.scale")` pushed through a
   `SettingsModel` property, exactly like the sibling rows; no new
   config read, no UI-side env inspection; the annotation text
   matches the Theme row's verbatim (structure-consistent, and no
   test asserts it — text is never asserted, AGENTS §5).
5. **One choke point, one defence arm**: the scale refusal lives in
   `apply_scale_commit` only; `settings_scale_committed` and
   `settings_scale_released` gain no parallel checks; every outcome
   assignment still routes through `set_settings_outcome`.
6. **No production behaviour change when env is not set**: without
   `TASKBOARD_UI__SCALE` the row renders enabled and behaves exactly
   as today; the Slider's new property defaults true so the unlocked
   gesture is untouched (grab probes prove it).
7. Crate boundaries: all Rust changes live in `taskboard-ui-slint`
   (adapter + view + two widgets + tests) and `scripts/` +
   `docs/ux_planning.org` for #1531; no new dependencies;
   workspace-pinned versions only; `scripts/ux-coverage.sh` stays a
   standalone bash tool (no repo-coupling beyond the fixtures it
   already ships).

## 4. Test catalogue (lowest layer, AGENTS §5 — typed outcomes, no text assertions, no clock dependence)

Existing tests that **must not be weakened**: §1's scale journey
list + `slider_grab_probe_tests.rs` p1/p2/p3; for #1531 the entire
`--self-test` matrix (cases 1–27 — run it, expectations unchanged).

New tests:

- **T28 (self-test case, #1531)** — `self_test` case 28 per D2:
  exit 0, exactly the plan-targetpin bound row BOUND, when invoked
  with no `--root` from a foreign-repo cwd. Structural assertions
  only (exit code, row-count greps).
- **T6 (unit/structural, env-locked push)** — the adapter's
  `scale-env-locked` push: a snapshot whose `env_overridden`
  contains `"ui.scale"` pushes `true`; without it, `false` — at the
  same layer the theme push is covered (mirror the #1512 chunk's T4
  precedent; if that coverage rides a `push_settings` unit, follow
  it, do not invent a new layer).
- **T7 (journey, env-locked row posture)** — `tests/journeys.rs`: a
  snapshot seeded with `env_overridden: {"ui.scale"}` renders the
  locked posture (the `scale-env-locked` property reads true —
  asserted structurally via the model property, never by label
  text) and a commit attempt through the seam
  (`settings_scale_committed` with a valid value) is structurally
  inert: no `UiRequest::SetScale` drained, the outcome reads
  `Rejected`, the retained detail projects typed `EnvLocked` (via
  the existing `last-error-env-locked` fact — the T-16 structural
  projection, no sentence text). Name embeds no journey token (the
  r4 plan has no §4 step grid to nominate from; name it for the
  env-lock law, mirroring how the #1515 scale journey was named).
- **T8 (grab-probe guard, slider)** — extend the existing grab-probe
  layer: with the slider's `enabled` false, a full press/move/
  release gesture emits no `pressed`/`committed` (request count 0);
  with it true, the existing probe behaviour is unchanged (that half
  is T-not-new — the existing probes are the pin). One new probe,
  same file/layer as p1/p2/p3.

## 5. Verification commands (expected outcomes)

```sh
scripts/ux-coverage.sh --self-test        # exit 0; 28 PASS lines (1–27 byte-identical verdicts + new case 28)
bash scripts/ux-coverage.sh .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate --root "$PWD"
                                          # from a worktree: still exit 0 — explicit --root keeps working
(cd /tmp && bash /home/viktoria/src/taskboard/scripts/ux-coverage.sh \
   .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate)   # demonstration only: outside a repo → script-repo fallback, unchanged verdicts
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journeys   # full UI journey suite green incl. T7
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint slider_grab_probe   # p1/p2/p3 + T8 green
scripts/memory-gate.sh cargo clippy -p taskboard-ui-slint --all-targets -- -D warnings  # clean
scripts/run-quality-gates.sh cargo nextest run --workspace    # the stage gate (flock-safe)
```

Stage 2 always invokes the gate with an explicit
`--root "$(pwd)"` while the fix is unlanded on the branch (the
running script is the main checkout's pre-fix copy); after the fix
lands locally the bare form is correct — the trap #1531 records is
about *other* worktrees, and this branch's own script copy is
fixed. Full §10 suite is the **dispatcher's** between stage 2 and
stage 3 and CI's on the PR — never inside a subagent (discipline,
item 1).

## 6. Diff-shape rules

- Allowed: `scripts/ux-coverage.sh` (default-root derivation + usage
  text + self-test case 28), `docs/ux_planning.org` (§2.3 usage
  sentence), `crates/taskboard-ui-slint/ui/views/settings-view.slint`
  (`scale-env-locked` property + row posture + annotation),
  `crates/taskboard-ui-slint/ui/widgets/slider.slint` (`enabled`
  property + gesture guards + dimmed render),
  `crates/taskboard-ui-slint/src/adapter/settings.rs` (the push +
  the `apply_scale_commit` defence arm),
  `crates/taskboard-ui-slint/tests/{journeys.rs,
  slider_grab_probe_tests.rs}` (T7/T8),
  `.artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md`
  (§12 amendment), this plan's companions.
- Not allowed: changes to `taskboard-domain`, `taskboard-state`, any
  sync/storage/app crate (`ui.scale` already rides the snapshot),
  `Cargo.toml` dependency edits, new `Field` variants
  (`Field::Scale` exists), new `SettingsError` variants
  (`EnvLocked` exists), snapshot-shape changes, new slint components
  (only the Slider property), text-field.slint changes
  (`read-only` exists), status/log text changes asserted anywhere,
  edits to any other `taskboard-WT-*` directory or the main
  checkout.
- Every commit standalone (`<type>(<module>): <desc>` + body;
  rebase-merge hygiene). **Two commits**: `fix(ci): the ux-coverage
  default evidence root resolves from the caller's checkout
  (#1531)` and `feat(ui): the settings Scale row renders the
  env-locked posture (#1528)`; the §12 amendment rides the second
  (it documents that behaviour). No WIP noise either way.

## 7. Rulings requested

- **R1 — #1531 derive vs refuse (D1)**: recommended default is
  **derive-from-`$PWD` + divergence notice** (option a). The
  alternative (refuse with usage error 64 when the cwd toplevel
  differs from the script repo's and no `--root` is given) proceeds
  only on an explicit operator ruling that a foreign-cwd invocation
  should always be a hard stop. **Proceeded on the default.**
- **R2 — #1528 typed defence arm in scope (D3)**: recommended
  default is **yes** (`Field::Scale` exists; T-16 doctrine; single
  choke point). The minimal view-only alternative (mirror #1512
  literally: disabled + annotation, no refusal arm) proceeds only on
  an operator ruling that seam honesty waits for a per-row lattice
  round. **Proceeded on the default.**
- **R3 — Slider lock as a primitive property (D4)**: recommended
  default is **`enabled` on the primitive** (visual honesty +
  guarded gesture). View-level callback gating is the fallback and
  is rejected in-plan for leaving the control looking live.
  **Proceeded on the default.**
- **R4 — UX-plan vehicle for #1528 (D5)**: recommended default is
  **amend the r4 home plan (§12)**, following the operator's R3
  OVERRULING precedent from the settings-honesty chunk; no new UX
  journey plan, and no coverage-gate mandate (the home plan is not
  gate-parseable). A new template-format UX plan for the scale row
  alone proceeds only on an operator ruling. **Proceeded on the
  default.**

## 8. Prompt 2 (stage 2 — IMPLEMENTATION) — paste-ready, verbatim

```text
You are the stage-2 IMPLEMENTATION agent for the chunk "coverage-tool root + scale row env-lock honesty" (branch fix/coverage-root-scale-envlock) in the taskboard repo, working in /home/viktoria/src/taskboard-WT-fix-coverage-root-scale-envlock. Recreate the worktree on that branch if missing (git fetch origin --prune; git -C /home/viktoria/src/taskboard worktree add ../taskboard-WT-fix-coverage-root-scale-envlock origin/main is NOT your case — the branch exists; recreate with git worktree add ../taskboard-WT-fix-coverage-root-scale-envlock fix/coverage-root-scale-envlock from the main checkout; if the branch's PR already landed, create fix/coverage-root-scale-envlock-v2 off fetched origin/main per AGENTS §8 — never git worktree add <path> main).

**Long-command discipline (binding, restate-verbatim law)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.*

1. Read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-27-coverage-root-scale-envlock-plan.md (normative: §2 decisions D1–D5, §3 invariants, §4 test catalogue, §6 diff-shape rules), and the origin issues (#1531, #1528). Work ONLY inside /home/viktoria/src/taskboard-WT-fix-coverage-root-scale-envlock, by absolute path; never modify /home/viktoria/src/taskboard or any other taskboard-WT-* directory. While the running script is a pre-fix copy (i.e. any script copy older than your own commit), invoke it as scripts/ux-coverage.sh <plan> --gate --root "$(pwd)" — the #1531 trap is exactly the missing --root from a worktree.
2. Implement exactly per the approved plan, TDD where code is involved, every commit standalone (rebase-merge hygiene). TWO commits: (a) fix(ci): in scripts/ux-coverage.sh replace the default-root derivation (lines 956-958) per plan D1 — the caller's `git -C "$PWD" rev-parse --show-toplevel` with the outside-a-repo fallback to the script's repo toplevel, the one-line stderr divergence notice, the corrected usage text (lines 55-56 and the usage() heredoc), and self-test case 28 per plan D2 (structural assertions only: exit code + row-count greps; cases 1-27's expectations byte-stable); plus the one-sentence docs/ux_planning.org §2.3 usage amendment (lines 167-168 vicinity). (b) feat(ui): SettingsModel gains scale-env-locked: bool (settings-view.slint, beside theme-env-locked ~line 231); the adapter pushes it from vm.snapshot.env_overridden.contains("ui.scale") beside set_theme_env_locked (~settings.rs:904); the Slider primitive gains in property <bool> enabled: true with gesture-entry guards (no pressed/preview/committed when locked; dimmed render — plan D4); the scale row (settings-view.slint ~1361-1441) binds the slider lock, the TextField read-only: SettingsModel.scale-env-locked, and the conditional env-locked RoleText annotation with the Theme row's verbatim sentence and Semantic.warning (~settings-view.slint:1481-1485 shape); apply_scale_commit (~settings.rs:1932) opens with the typed defence arm — env_overridden.contains("ui.scale") → tracing::debug! + set_settings_outcome(Rejected, EnvLocked { field: Field::Scale }) + return (plan D3); amend .artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md with §12 per plan D5.
3. Tests per the plan §4 catalogue: self-test 28 (bash, in self_test); T6 structural push test at the theme push's coverage layer; T7 journey in tests/journeys.rs (snapshot seeded env_overridden {"ui.scale"}; property reads true; commit attempt drains no UiRequest::SetScale; outcome Rejected; the EnvLocked class projects through the existing last-error-env-locked fact); T8 grab-probe guard (enabled=false emits no pressed/committed; p1/p2/p3 stay the unlocked oracle). Typed outcomes and structural facts only; no text assertions; no sleeps; no wall-clock dependence. Do NOT weaken the scale journey oracle (the_scale_drag_moves_nothing_until_release, the_scale_echo_renders_the_committed_scale, a_scale_commit_anchors_the_scroll_on_the_scale_row, a_scale_parse_miss_renders_the_invalid_flavour_and_typed_detail, a_scale_send_rejection_reverts_the_live_scale_and_reports_failed, settings_scroll_reaches_the_scale_controls_at_scale_3) or slider_grab_probe_tests p1/p2/p3 — they are the regression oracle for the paths you extend.
4. Run scripts/ux-coverage.sh --self-test (exit 0, 28 PASS lines, 1-27 byte-identical verdicts). Then run the stage gate: scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; flock-deferral rule above applies). Every cargo invocation goes through scripts/memory-gate.sh.
5. SPDX headers on any new .rs file (AGPL-3.0-only for taskboard-ui-slint); #![forbid(unsafe_code)] already crate-level; the script stays standalone bash (no new fixture files unless case 28 needs one — prefer constructing temp repos like cases 9/12/13/16 do). Respect the crate boundaries: no app/domain/sync changes; no UI-side env inspection.
6. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report. Known pre-filed homes: #1531, #1528 (this chunk), #1529 (SetTheme guard-key bypass — out of scope) — comment, don't duplicate.

Report: per-gate results, the self-test matrix verdicts, any deviation from the plan and why.
```

## 9. Prompt 3 (stage 3 — REVIEW) — paste-ready, verbatim

```text
You are the stage-3 REVIEW agent for the chunk "coverage-tool root + scale row env-lock honesty" (branch fix/coverage-root-scale-envlock), working in /home/viktoria/src/taskboard-WT-fix-coverage-root-scale-envlock.

**Long-command discipline (binding, restate-verbatim law)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.*

1. Begin by rebasing onto main: git fetch origin --prune, rebase the branch onto origin/main (AGENTS §8's rebase dance: drop already-merged duplicate commits; resolve only real conflicts), push --force-with-lease if the rebase rewrote history. Then read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-27-coverage-root-scale-envlock-plan.md (§3 invariants and §6 diff-shape rules are your acceptance criteria), and the origin issues (#1531, #1528); then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD).
2. Functional focus (this chunk's specific traps): (a) #1531 — the default-root derivation reads the CALLER's toplevel (`git -C "$PWD" rev-parse --show-toplevel`) with the outside-a-repo fallback; a leftover `git -C "$(dirname "${BASH_SOURCE[0]}")"` as the primary derivation is the unfixed bug; the #1073/#1186 canonicalization pass (root=$(cd "$root" && pwd -P)) and the FX_ROOT fixture prune must be untouched and downstream of the new derivation; the divergence notice is stderr-only and asserted nowhere. (b) self-test cases 1-27 are byte-stable in expectations — run scripts/ux-coverage.sh --self-test and verify 28 PASS lines with case 28 new and purely additive; any changed expectation in an existing case is a BLOCKER (a weakened self-test is a weakened test). (c) #1528 — the env-lock push reads vm.snapshot.env_overridden.contains("ui.scale"); any UI-side env inspection or new config read is a boundary violation (config reading lives in taskboard-app, AGENTS §2). (d) ALL commit surfaces lock: the Slider via the primitive's enabled property (verify the gesture-entry guard covers pointer press AND key-step, so no preview/committed can fire when locked), the TextField via its existing read-only property, plus the annotation — a live commit surface under TASKBOARD_UI__SCALE is a BLOCKER. (e) the typed defence arm sits at apply_scale_commit only (both GUI paths funnel there), emits Rejected + EnvLocked { field: Field::Scale } through set_settings_outcome — a parallel check in settings_scale_committed/settings_scale_released, a direct outcome assignment, or a new Field/SettingsError variant is a finding. (f) the tests that must not have been weakened: the scale journey oracle (the_scale_drag_moves_nothing_until_release, the_scale_echo_renders_the_committed_scale, a_scale_commit_anchors_the_scroll_on_the_scale_row, a_scale_parse_miss_renders_the_invalid_flavour_and_typed_detail, a_scale_send_rejection_reverts_the_live_scale_and_reports_failed, settings_scroll_reaches_the_scale_controls_at_scale_3) and slider_grab_probe_tests p1/p2/p3 — verify none was relaxed, re-scoped, or asserted weaker. (g) no text assertions anywhere in the new tests (typed outcomes and structural facts only), no sleeps, no wall-clock dependence.
3. **Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client; (c) a single level of abstraction per function body. Findings are fixed on the branch.
4. Re-run the stage gate yourself: scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; flock-deferral rule above applies), plus scripts/memory-gate.sh cargo clippy --workspace --all-targets -- -D warnings and scripts/memory-gate.sh cargo fmt --all -- --check; any failure is a finding. Re-run scripts/ux-coverage.sh --self-test; any non-28-case verdict change is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups; every commit stands alone for the rebase merge).
6. Ship — your scope ENDS at PR-open: push the branch, open the PR to main with gh (body: closes #1531, closes #1528; summary of D1–D5), and report the URL. Do NOT run gh pr checks --watch, do NOT verify mergeStateStatus, do NOT remove the worktree — the dispatcher owns the CI watch, the CLEAN verification, teardown, and at-merge issue closures. main is branch-protected: never merge; the user merges on GitHub.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report. Known pre-filed homes: #1529 — comment, don't duplicate.

Report: verdict, findings (functional + mandate), what you fixed, PR URL.
```

## 10. Completion bookkeeping

- The PR body closes #1531 and #1528 at merge (the dispatcher owns
  the at-merge closures). #1529 stays open as this chunk's recorded
  out-of-scope neighbour (the Theme-side app refusal — a different
  lattice round if the operator wants one).
- The §12 amendment and the `docs/ux_planning.org` sentence land on
  this branch — both artefacts are versioned with the behaviour they
  describe.
- Post-merge: the bare `scripts/ux-coverage.sh <plan> --gate` form
  in `.artifacts/templates/dispatch-plan.md` and
  `docs/ux_planning.org` becomes correct from any worktree — no
  follow-up work gated on this chunk.

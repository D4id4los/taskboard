# [Plan] GUI Phase 7 Implementation — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-14
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the five implementation chunks of the
  accepted GUI Phase 7 design round (M2 → v0.1.0 foundation). No code
  in this commit.
- **Origin**: `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md`
  (Status: ACCEPTED 2026-10-14, all 23 agenda rows resolved in §16 —
  normative: its §12 chunk decomposition **adopted as-is** here, §2
  scope fence, §3–§10 foundation sections, §13 draft ADR 0015, §14
  risks), derived per wave-0 dispatch plan
  `.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
  §5 chunk 4 (its provisional shapes (a)–(e) are reshaped only where
  the accepted design says otherwise — every reshaping recorded in
  §1.4). Version-pinned facts rest on the stress-test report
  `.artifacts/reports/2026-10-14-slint-gui-stress-test-report.md`
  (Slint 1.18.1); the requirements brief
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`
  remains the traceability target (design §15 matrix).
- **Status**: READY — dispatch chunks 1 → 2 → 3 → 4 → 5 **strictly
  serially** (§1.2). Deliberately **not** dispatched here: §7.

## 1. Chunks & ordering

### 1.1 Chunks table

| Chunk | Design §12 slice | Branch | Covers (normative design sections) | Size |
|---|---|---|---|---|
| 1 | shell + theme foundation | `feat/ui-shell-theme` | slint dep + pins + features (Q10/Q12), window shell + app wiring + `[ui] scale` hand-off (Q22, §6.1), UI-command forwarder (backlog [2026-10-09], §1.2), theme/token module + text roles + fonts (§3, Q13/Q18), token lint gate (§3.4), contrast check (§9.1), scale escape hatch (§6.2) | M (4–6 d) |
| 2 | action-dispatch keymap layer | `feat/ui-action-keymap` | action registry + contexts + reserved actions (§5.1–5.2, Q20), keymap-as-data + merge + drop-and-warn validation (§5.3, Q15), presets standard/emacs/vi (§5.4, Q14), Rust-side module location (§5.5, Q2), Slint integration (§5.6, Q9), conflict detection (§5.7), `app.menu` generated menu (§5.9), registry/keymap insta snapshots (§10) | M–L (8–12 d) |
| 3 | component library + v0.1.0 views | `feat/ui-views-board` | component layers + rules A–E + presentation helpers (§4), responsive composition + board degeneration (§6.3–6.4, Q16/Q4), focus architecture + a11y discipline (§7.1–7.2), the three views + sync-status indicator + overdue banner (§2.1, Q23) | L (10–15 d) |
| 4 | harness + quality gates | `feat/ui-harness-quality` | logical harness on `i-slint-backend-testing` (§8.1, Q8/Q12), full keyboard-journey suite (§7.3), keypress-count guard (§9.2, Q17), golden screenshots at scale extremes × breakpoints (§10, §6), MCP dev-feature smoke (§8.2, Q19) | M (5–8 d) |
| 5 | ADR + docs + release | `docs/ui-adr-release` | ADR 0015 formalised (§13), architecture.org + testing_strategy.org amendments, backlog ✅ bookkeeping ([2026-10-07], [2026-10-09], 0013-debt note Q21), v0.1.0 release chore (roadmap M2 exit) | S (2–3 d) |

Total estimate ≈ 29–44 dev-days ≈ **5–7 weeks** (design §12; consistent
with stress report §6).

### 1.2 Dependency graph

```
chunk 1 (shell/theme) → chunk 2 (action/keymap) → chunk 3 (views)
      → chunk 4 (harness/quality) → chunk 5 (ADR/docs/release)
```

Strictly serial, per the design plan §12 ("each consumes the previous
seam; no parallelisable pair — the value of this wave is the seams
compounding"). Concretely: chunk 2's resolver dispatches real commands
through chunk 1's forwarder; chunk 3's views bind to chunk 2's actions
and style through chunk 1's tokens; chunk 4's journeys drive the whole
1–3 stack through the testing backend; chunk 5 records what 1–4 built
and ships the release. **There is no order-independent pair** — do not
dispatch any two chunks concurrently. Dispatch chunk N+1 only after
chunk N's PR has merged, so every branch starts from a `main` that
already contains its prerequisites and no PR is ever stacked (no
phantom-conflict rebase dance). If the user chooses to stack anyway
(dispatch before merge): every branch still targets `main`, and each
stage-3 agent expects the AGENTS §8 rebase dance after an upstream
merge (`git fetch origin --prune && git rebase --onto origin/main
<tip-of-the-branch-that-just-landed> <my-branch>` then push
`--force-with-lease`).

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); serially across chunks per the
graph above; never let two agents share one worktree. Every chunk
works in its own worktree `../taskboard-WT-<branch>` next to the
project root (AGENTS §8), branch off the then-current `main`, no PR
ever stacked by default.

### 1.3 Shared execution model & mandates (binding for every chunk)

**Shared execution model** (copied from the wave-0 dispatch plan §1;
the three kind-specific clauses marked † are taken verbatim from the
connect-flow dispatch plan §1 — the house's implementation variant —
because the wave-0 wording is research-flavoured and would be false
for `feat/*` chunks; see the copy note below):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the †TDD-first implementation plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). †No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — †implementation agent**: continues in that worktree
   (recreates it on the branch if missing), †implements per the
   approved plan, every commit standalone (rebase-merge hygiene),
   runs the full gate suite (AGENTS §10: fmt, clippy `-D warnings`,
   nextest, doc-tests, deny, llvm-cov) and makes it green. Does
   **not** push or open a PR. †Reports gate results and deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   its base against the chunk's acceptance criteria + AGENTS rules
   **plus the †code-quality mandate below**, re-runs whatever gates
   apply, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Its last task is
   shipping: push the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`.

**Copy note** (the recorded reason for the three † substitutions):
the wave-0 dispatch plan §1 model is the research/docs variant — its
"no execution of the investigation" (stage 1), "research,
experiments, and the deliverable report/doc" (stage 2), and
"evidence & artefact mandate below" (stage 3) clauses describe
research chunks. This plan dispatches implementation chunks; the
connect-flow dispatch plan §1 is the canonical implementation
variant of the same three-stage protocol, and its corresponding
clauses are copied verbatim at the three marked positions. Wave 0's
chunk-4 instruction ("copy the execution model verbatim from §1 …
together with the code-quality mandate copied verbatim from
connect-flow §1, where it becomes binding again") is honoured in
intent: the protocol is unchanged, only the kind-specific clauses
come from the implementation exemplar. Everything else is
byte-identical to wave-0 §1.

**Code-quality mandate (stage 3, every chunk — user-directed):**
beyond functional review, the stage-3 agent evaluates:

- **Code quality**: naming, comment discipline (comments state
  constraints, not narration), idiomatic use of the approved stack,
  no dead code or speculative generality, test quality per AGENTS §5.
- **Single Responsibility Principle**: every module, actor, function,
  and type has exactly one reason to change; domain stays pure (no
  IO, no runtime), the engine orchestrates without legislating, the
  connector actor only syncs, the CLI only parses/dispatches — the
  crate-role boundaries of AGENTS §2 are the SRP line, and a
  violation of the boundary is a violation of SRP.
- **Single level of abstraction per function body**: every function
  body works at one level — a function either orchestrates
  (calls named operations) or implements (works in primitive data),
  never mixes; guard clauses and the happy path at the top level,
  detail pushed into named private functions. A body that interleaves
  channel-half plumbing with actor spawning and config surgery is a
  finding, even if it passes tests.

Findings from the mandate are fixed on the branch like any other
finding (refactor commits with proper `refactor(<module>):` subjects,
or amend where the commit is not yet shipped).

*(Source note: copied verbatim from
`.artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md`
§1 — diff against it; paraphrase drift is a finding.)*

*Wave instantiation of the SRP line (addendum, not part of the
verbatim copy): the connect-flow bullet's crate examples predate this
wave; here the AGENTS §2 boundary instantiates as — the UI crate only
presents (reads `ArcSwap`/broadcasts, emits commands, styles from
tokens; no IO, no business logic, no state mutation), the app crate
resolves config (figment) and wires actors, the domain crate is
untouched by UI work. The boundary line itself is unchanged.*

**UI-crate hard rules** (binding supplement, from the design plan and
AGENTS; every stage of every chunk):

- SPDX: AGPL-3.0-only header on every new file in
  `taskboard-ui-slint` and `taskboard-app`; MIT OR Apache-2.0 if any
  other crate is ever touched.
- `taskboard-ui-slint` keeps its deliberate **no**
  `#![forbid(unsafe_code)]` exception (ADR 0003); `unsafe` appears
  only via Slint-generated code. Every other crate keeps `forbid`.
- The UI crate gains **no IO crates** — config is read app-side
  (figment) and handed over as typed values (design §1.2).
- Slint pin discipline (Q12): `slint = "1.18.1"` (caret) +
  `i-slint-backend-testing = "=1.18.1"` (exact) move together; any
  Slint bump re-audits the stress-report sections the design cites
  (design §14, stress §5) — v0.1.0 does not bump.
- **Scope fence**: every chunk's review agent diffs the branch
  against design §2.2/§2.3 — a §3 feature-surface item appearing in
  v0.1.0 is scope creep (design §2.4; wave analysis §7 leakage
  guard). If a chunk believes it needs a §2.3 surface to make a seam
  real, that is escalated, not built.
- AGENTS §5 in full: no sleeps (mock time), typed outcomes (no text
  assertions), fakes over mocks, journeys assert routing only,
  deterministic by construction, no quality ceilings (the §9.2
  keypress guard is a *regression* ceiling living in the design plan,
  not test code — design §9.2 records the interplay).

### 1.4 Reshaping record — provisional shapes (wave-0 §5) vs adopted decomposition

The accepted design's §12 decomposition is adopted as-is. Where it
differs from the wave-0 §5 provisional shapes (a)–(e), the design
wins; the differences and their design-plan reasons:

1. **(a) widened → chunk 1** absorbs the UI-command forwarder
   (backlog [2026-10-09]), the `[ui]` config hand-off, the token lint
   gate, bundled fonts, and the §6.1/6.2 scaling half — none of which
   the provisional shape named. Reason: design §12 row 1 ("forwarder
   lands here so chunk 2 can dispatch real commands"); the lint and
   fonts must exist before any view markup (chunk 3) so the gate
   guards from the first line; scale is a token/config mechanism
   (§6.1) inseparable from the theme module.
2. **(c) narrowed, (d) widened → the keyboard-journey suite moved from
   the views chunk to the harness chunk.** Provisional (c) said
   "component library + board/stack/task views + focus/keyboard
   journeys"; the design's chunk 3 is views + focus/a11y discipline
   (§7.1–7.2) and chunk 4 owns "logical harness + full journey suite"
   (§7.3, §8.1). Reason: journeys require the
   `i-slint-backend-testing` harness, which is chunk 4's own
   deliverable — building journeys inside chunk 3 would mean importing
   the harness early or hand-rolling input injection (rejected by
   §8.1); the §9.2 keypress counts are also computed from the
   journeys, so they land with them.
3. **(b) widened → chunk 2** additionally owns the Slint integration
   half (root FocusScope capture-phase interception, §5.6), the
   reserved disabled actions (§5.8/Q20), and the `app.menu` generated
   menu (§5.9 — the one R8 projection v0.1.0 ships). Reason: design
   §12 row 2; the menu is a projection of the registry, and shipping
   it with the action layer keeps chunk 3 free of action-layer
   concerns (views bind to actions only).
4. **(e) widened → chunk 5** additionally lands the
   testing_strategy.org amendments, the ADR-0013 debt note (Q21), and
   the v0.1.0 release itself (`bump_version.sh` + release pipeline).
   Reason: design §12 row 5; the roadmap Phase 7 exit criterion is
   "tagged v0.1.0 with desktop UI, released binaries" — the release
   chore is part of the phase, not an afterthought.
5. **Micro-reshaping inside design §12 (the one deviation from the
   design's letter, recorded):** the `i-slint-backend-testing`
   dev-dependency lands in **chunk 4** (when the harness first
   consumes it), not in chunk 1's "pins". Reason: an exact-pinned
   dev-dependency with no consumer for three chunks is speculative
   addition (code-quality mandate) and invites accidental
   semver-drift review noise; Q12's move-together discipline is
   preserved — chunk 4 asserts pin equality with the slint pin, and
   ADR 0015 (chunk 5) records the upgrade runbook line the design's
   §13 draft already carries.

## 2. Chunk 1 — Shell, theme & tokens foundation (`feat/ui-shell-theme`)

- **Covers** (normative: design §3 all; §6.1–6.2; §9.1; §1.1–1.2 crate
  facts; §11/§16 Q7, Q10, Q12, Q13, Q18, Q22; §12 row 1):
  - slint dependency in root `[workspace.dependencies]` at the stress
    pin (`slint = "1.18.1"`, caret — Q12), default features (winit,
    femtovg, software, accessibility — Q10); remove the
    deliberate-absence NOTE in `crates/taskboard-ui-slint/Cargo.toml`
    and `src/lib.rs`; the CI consequence Q10 leaves to this chunk
    (GL system libs on ubuntu-latest vs a software-only CI profile)
    decided and recorded here; `cargo deny` license configuration for
    the Slint tri-license runtime (stress §4.5).
  - Window shell + app wiring: the UI's window component; app-side
    mount (event-loop ownership), hand-over of the `ArcSwap<AppState>`
    handle + `EngineSignal`/`SystemEvent` broadcast subscriptions +
    the UI command `mpsc`; **the UI-command forwarder** (backlog
    [2026-10-09]): `tokio::spawn` in bootstrap, `recv()` →
    `EngineHandle::dispatch()`, dropped-command logging.
  - `[ui]` config table (figment, strict tables) with `scale` only
    (Q22): float clamped 0.5–3.0, out-of-range typed config error,
    read app-side, pushed into `Theme.base-scale` at UI start (§6.1);
    `SLINT_SCALE_FACTOR` read *before* Slint init as the OS-DPI
    escape hatch, documented in README (§6.2);
    `set_const_scale_factor` explicitly unused.
  - Theme/token module `ui/theme.slint` (§3.1): `Theme` (colour
    tokens + `base-scale` + spacing scale), `Typography` (text-role
    structs), `Semantic` (state colours); the closed text-role
    vocabulary §3.2; naming rules §3.3 (function-named colours, two
    layers); one compile-time dark-first theme (§3.5/Q13); theme init
    as a window-construction step; fonts bundled + registered at
    startup (§3.6/Q18 — tri-licensed sans + mono, final pick recorded
    in the chunk plan).
  - Token lint gate (§3.4): `scripts/` scanner over `*.slint` — no
    literal colours, no literal font attributes, no literal spacing
    dimensions outside the theme module (proportional/relative layout
    expressions exempt); wired into CI + pre-commit; no suppression
    escape hatch.
  - Contrast check (§9.1): pure Rust unit test over the declared
    legible token pairs (text roles × surfaces, `Semantic` ×
    surfaces), WCAG 2.x ratios, minimums ≥ 4.5:1 body / ≥ 3:1
    large-display per role class, completeness rule (a new pair
    without an assertion fails).
  - **Non-goals**: no views, no action/keymap code, no
    `i-slint-backend-testing` (chunk 4), no keymap config fields, no
    runtime theme switching or light/high-contrast themes, no second
    window, no `#![forbid(unsafe_code)]` in the UI crate (exception
    stands), no domain/state-crate changes.
- **Acceptance**: gates green (fmt, clippy, nextest, doc-tests, deny
  with the Slint license config, llvm-cov); the lint gate implements
  all three rules, is wired into CI + pre-commit, and has no escape
  hatch; the contrast check enforces the recorded minimums over a
  complete pair list; out-of-range `[ui] scale` is a typed config
  error (variant-matched, not text-matched); a valid scale reaches
  `Theme.base-scale` (generated `Global` setter); the forwarder is
  proven hermetically with a fake engine handle (typed commands
  forwarded; no sleeps); `SLINT_SCALE_FACTOR` documented and read
  before Slint init; the UI crate's only new runtime deps are slint
  (config arrives as typed values); no `.slint` file exists that the
  lint gate does not cover; the renderer/CI-profile decision is
  recorded in the chunk plan.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the GUI Phase 7 implementation ("Shell, theme & tokens foundation", branch feat/ui-shell-theme) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: .artifacts/plans/2026-10-14-gui-phase7-design-plan.md is on main with Status: ACCEPTED (verify the header; if not, STOP and report).

1. Read AGENTS.md; the accepted design plan (normative for this chunk: §1.1–1.2 crate facts, §2 scope fence, §3 theming/tokens all subsections, §6.1–6.2, §9.1, §12 row 1; decisions Q7, Q10, Q12, Q13, Q18, Q22 in §11/§16); the stress-test report .artifacts/reports/2026-10-14-slint-gui-stress-test-report.md (§3.3 theming facts, §3.8 renderer decision + CI consequence, §4.5 licensing, §5 pin proposal); and the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§2.
2. Read the current seams you will extend: crates/taskboard-ui-slint/{src/lib.rs,Cargo.toml} (the dependency-free stub whose NOTE comment this chunk removes), the root Cargo.toml [workspace.dependencies], crates/taskboard-app/src/bootstrap.rs (forwarder + UI mount land here; the app already depends on taskboard-ui-slint), crates/taskboard-app/src/config.rs (figment patterns, strict-table rule), the pre-commit hook and CI workflows (where the lint gate wires in), and docs/adr/0003 (the UI crate's unsafe exception).
3. Create the worktree ../taskboard-WT-feat-ui-shell-theme on a new branch feat/ui-shell-theme off main.
4. Write a detailed TDD-first implementation plan at .artifacts/plans/<today>-ui-shell-theme-plan.md settling at minimum: the dependency landing (slint = "1.18.1" caret in [workspace.dependencies], default features per Q10) and the recorded CI consequence (GL system libs on ubuntu-latest vs a software-only CI profile — Q10 says this chunk decides and records); the cargo-deny configuration for the Slint tri-license runtime; the window shell + app wiring (which thread owns the slint event loop; what bootstrap hands over — ArcSwap handle, EngineSignal/SystemEvent subscriptions, the command mpsc; and how the GUI entry composes with the existing CLI/REPL/daemon entries — an open question this plan must resolve with a recorded decision); the UI-command forwarder (tokio::spawn recv → EngineHandle::dispatch, dropped-command logging, hermetic fake-handle test); the [ui] config table with scale only (clamp 0.5–3.0, out-of-range typed error, push into Theme.base-scale at UI start); the theme module structure (Theme/Typography/Semantic globals; the closed §3.2 text-role table; §3.3 naming; spacing scale derived from base-scale; dark-first per Q13; theme-init-as-window-construction-step rule); font bundling + registration (tri-licensed sans + mono; final pick recorded here; golden-test determinism is the decisive reason); the token lint gate (the three §3.4 rules, CI + pre-commit wiring, no suppression escape); the contrast check (pair list as data, WCAG ratios, minimums per role class, completeness rule); and the SLINT_SCALE_FACTOR escape hatch (read before Slint init, README documentation, set_const_scale_factor unused). Resolve every open question with a recorded decision; deviations from the design plan need a recorded reason.
5. Commit it as: docs(plan): ui shell theme and tokens foundation. Do not implement; do not push.

Report: plan path, decisions (especially the CI profile and the GUI entry mode), risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the GUI Phase 7 implementation ("Shell, theme & tokens foundation").

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§1.1–1.2, §2, §3, §6.1–6.2, §9.1, §11/§16 Q7/Q10/Q12/Q13/Q18/Q22), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§2, and the stage-1 chunk plan (.artifacts/plans/*ui-shell-theme*-plan.md) in the worktree ../taskboard-WT-feat-ui-shell-theme (branch feat/ui-shell-theme). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: AGPL-3.0-only SPDX header on new .rs files in taskboard-ui-slint and taskboard-app; the UI crate keeps its deliberate no-forbid(unsafe_code) exception and gains no IO crates (config is read app-side and handed over as typed values); no tokio::time::sleep in tests (poll predicates / channel asserts); typed outcomes, no text assertions; the token lint gate lands in the same chunk as the theme module — no .slint source may exist outside its cover.
3. Run the full gate suite from AGENTS.md §10 and make it green (cargo deny included — the Slint runtime license configuration is part of this chunk).
4. Do NOT touch the backlog — "[2026-10-07] Add the `slint` dependency to taskboard-ui-slint" and "[2026-10-09] UI command forwarder bootstrap task" are marked ✅ by chunk 5, not here. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the renderer/CI-profile decision as executed, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the GUI Phase 7 implementation (branch feat/ui-shell-theme).

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§1–§3, §6.1–6.2, §9.1, §11/§16), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§2 (the code-quality mandate and UI-crate hard rules apply to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-shell-theme.
2. Functional focus: the slint pin is the caret 1.18.1 with default features (Q10) and the CI consequence is decided, recorded, and wired; the token lint gate implements all three §3.4 rules (literal colours, literal font attributes, literal spacing dimensions outside the theme module) with no suppression escape, and runs in CI + pre-commit; the contrast check enforces §9.1 minimums over a complete pair list (a new token pair without an assertion fails the completeness rule); out-of-range [ui] scale is a typed config error; SLINT_SCALE_FACTOR is read before Slint init and documented, set_const_scale_factor is unused; the forwarder is a bootstrap-owned spawn adapting the UI mpsc onto EngineHandle::dispatch with dropped-command logging and a hermetic fake-handle test; the theme module is the only place colours, font attributes, and base dimension arithmetic live; text roles are exactly the closed §3.2 set; fonts are bundled and registered at startup; the UI crate gained no IO dependency and no business logic.
3. Code-quality mandate (§1.3): evaluate code quality (naming, comment discipline, no dead code, no speculative generality — runtime theme-switching machinery and second-window plumbing are POST-1.0 and must not appear); SRP (the UI crate only presents; the app crate resolves config and wires; the forwarder only forwards); single level of abstraction per function body.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — Action-dispatch keymap layer (`feat/ui-action-keymap`)

- **Covers** (normative: design §5 all subsections; §10 unit/snapshot
  rows; §11/§16 Q1, Q2, Q9, Q14, Q15 (redirected), Q20, Q22 keymap
  half; §12 row 2):
  - `actions` module: `ActionId` dotted `scope.verb`; the four
    v0.1.0 scopes (`app.*`, `nav.*`, `task.*`, `capture.quick`);
    the const registry `{ id, label, contexts, category }`;
    contexts declared per view; masking declared in keymap data;
    reserved global actions present day one, disabled where
    unimplemented (`app.undo`/`app.redo`, `capture.quick`,
    `palette.search`, `app.settings`, `app.help`; `app.menu` and
    `app.exit` get their handlers here).
  - `keymap` module: stroke parsing (a binding is a sequence of
    strokes — chords are the multi-stroke case), the three preset
    tables (`standard`, `emacs` C-x prefixes as chord sequences,
    `vi` with `vi-normal`/`vi-insert` as keymap contexts + mode-flip
    actions), merge preset ⊕ user overrides, validation producing
    typed findings (unknown action id, malformed stroke,
    masked-context binding, intra-context conflict) with the Q15
    redirect semantics: **drop the offending overrides and start
    anyway** (preset binding wins for every dropped action);
    findings surface via `tracing` warnings + a startup-warning
    state; prefix-of-sequence in the same context is a warning, not
    an error.
  - Resolver: pure state machine — context resolution
    (active view context → global), prefix/chord accumulation,
    vi-mode state, masking, prefix-abort with a status signal.
  - Slint integration (adapter last, after the pure modules are
    green): root `FocusScope` + `forward-focus` (§5.6), capture-phase
    interception for events text fields do not consume; the
    declarative `KeyBinding` element unused for any rebindable
    action (Q9); keymap applied at UI start, one-way.
  - App-side: the `[ui]` keymap config surface (`preset` +
    `overrides`, Q22) parsed + validated where figment lives; the UI
    crate receives the validated `Keymap` + findings as typed values.
  - `app.menu` generated menu (§5.9): opens a menu generated from the
    action registry listing the active context's actions with their
    `kbd-hint`s — this seeds the `kbd-hint` primitive (§4.4
    inventory) under the §4 layering rules; chunk 3 owns the rest of
    the library.
  - Insta snapshots (§10): the action-registry dump (reserved
    actions visible as disabled) and the resolved default keymap —
    "documentation as a test".
  - **Non-goals**: no views beyond the menu projection, no focus
    architecture beyond the root scope (chunk 3), no conflict-
    resolution UI or settings page (POST-1.0), no live re-map during
    a session, no new config surface beyond Q22.
- **Acceptance**: the `keymap`/`actions` modules' public APIs contain
  no Slint types (§5.5 extraction seam); preset-coverage test green
  for all three presets over the core-verb list (§7.3 verbs +
  undo/redo/capture/menu); shipped presets conflict-free; validation
  is drop-and-warn — a hostile keymap never blocks UI start (Q15),
  dropped overrides fall back to preset bindings, findings are typed
  enum values (no text assertions); the registry snapshot shows the
  reserved actions as disabled; the root scope is the single key
  path (no view binds keys; `KeyBinding` unused); resolver behaviour
  proptested over chord/prefix/mode/masking sequences; gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the GUI Phase 7 implementation ("Action-dispatch keymap layer", branch feat/ui-action-keymap) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 1 (feat/ui-shell-theme) is merged into main — the slint dependency, theme module, window shell, and UI-command forwarder exist (verify; if not, STOP and report).

1. Read AGENTS.md; the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (normative for this chunk: §5 all subsections, §10 unit/snapshot tier rows, §12 row 2; decisions Q1, Q2, Q9, Q14, Q15 — redirected in §16, Q20); the stress-test report §3.2 (interception seam: capture phase, KeyBinding single-stroke caveat); and the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§3.
2. Read the seams chunk 1 landed: crates/taskboard-ui-slint/ (theme module, window shell, the mpsc command boundary), crates/taskboard-app/src/{bootstrap.rs,config.rs} (figment [ui] table, where keymap validation lands), and the existing insta usage in crates/taskboard-state for the snapshot conventions.
3. Create the worktree ../taskboard-WT-feat-ui-action-keymap on a new branch feat/ui-action-keymap off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-action-keymap-plan.md settling at minimum: the ActionId namespace and the full v0.1.0 registry table (ids, labels, contexts, categories — the reserved actions per §5.8 with their disabled markers); the keymap data model (stroke grammar, sequence encoding, preset tables for standard/emacs/vi incl. the vi-normal/vi-insert contexts and mode-flip actions); merge + validation semantics with the Q15 drop-and-warn rule (typed findings enum; preset fallback; tracing surfacing; the startup-warning state value the status-bar will render in chunk 3); the resolver state machine design (contexts, prefix accumulation, masking, prefix-abort + status signal); the app-side [ui] keymap config surface (Q22) and the typed hand-over into the UI crate; the Slint adapter (root FocusScope + forward-focus, capture-phase interception, the TextInput exception, KeyBinding unused per Q9); the app.menu generated-menu composition + the kbd-hint primitive seed; the insta snapshots (registry dump, resolved default keymap); and the test catalogue (unit: parse/merge/validate; proptest: resolver sequences incl. chords, prefixes, modes, masking; preset coverage over core verbs; preset conflict-freeness). Resolve every open question with a recorded decision; deviations from the design plan need a recorded reason.
5. Commit it as: docs(plan): ui action dispatch and keymap layer. Do not implement; do not push.

Report: plan path, decisions (especially the stroke grammar and vi-mode modelling), risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the GUI Phase 7 implementation ("Action-dispatch keymap layer").

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§5, §10, §11/§16 Q1/Q2/Q9/Q14/Q15/Q20), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§3, and the stage-1 chunk plan (.artifacts/plans/*ui-action-keymap*-plan.md) in the worktree ../taskboard-WT-feat-ui-action-keymap (branch feat/ui-action-keymap). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD — the pure keymap/actions modules and their tests land BEFORE any Slint-side code, every commit standalone (rebase-merge hygiene). Hard rules: no Slint types in the keymap/actions modules' public APIs (§5.5); AGPL-3.0-only SPDX headers; no IO in the UI crate (keymap validation is app-side; the crate receives typed values); typed outcomes only — validation findings are enum variants, never matched strings; proptest where inputs are interesting (resolver sequences, stroke parsing); the vi modes are contexts + masking, not a subsystem.
3. Run the full gate suite from AGENTS.md §10 and make it green; insta snapshots reviewed, not blindly accepted (the registry snapshot is the action documentation — check it reads as such).
4. Do NOT touch the backlog. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, preset coverage summary, any deviation from the plan and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the GUI Phase 7 implementation (branch feat/ui-action-keymap).

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§5, §10, §11/§16), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§3 (the code-quality mandate and UI-crate hard rules apply to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-action-keymap.
2. Functional focus: the Q15 redirect holds — no keymap input can block UI start; offending overrides are dropped with typed findings and the preset binding wins (assert the fallback, not just the drop); the shipped presets are conflict-free and each covers the core-verb list (all three, not just standard); reserved global actions exist in the registry and read as disabled in the snapshot; the root FocusScope is the single key path — grep for key-pressed handlers outside it and for any KeyBinding use on rebindable actions (Q9 violation); the resolver's prefix-abort is a defined transition, not an implicit drop; text input is never starved by the interception (capture phase only for unconsumed events); no Slint types leaked into the pure modules' public APIs.
3. Code-quality mandate (§1.3): evaluate code quality (naming, comment discipline, no speculative generality — no conflict-resolution UI, no settings page, no live re-map machinery: all POST-1.0); SRP (the resolver only resolves, the registry is data, the adapter only adapts, config validation lives app-side); single level of abstraction per function body — the resolver's transition function in particular must read as one level.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — Component library & the v0.1.0 views (`feat/ui-views-board`)

- **Covers** (normative: design §4 all; §6.3–6.4; §7.1–7.2; §2.1
  shipped surface; §11/§16 Q4, Q16, Q23; §12 row 3):
  - Component library under the §4.1 layout (`ui/widgets/`,
    `ui/compositions/`, `ui/views/`) with the layering rules A–E
    (§4.2) enforced from the first file: styles flow from tokens
    only; primitives take domain-meaningful properties; one
    direction of reuse; future-surface slots are properties
    (`decorations: [TaskDecoration]` — empty in v0.1.0), not forks;
    primitives are leaves, compositions own layout only.
  - L1 inventory (§4.4): `task-card` (+decoration slots), `badge`,
    `kbd-hint` (formalised from chunk 2's seed), `icon-button`,
    `text-field`, `list-row`, `status-dot`, `empty-state`. L2:
    `board-layout`, `view-stack` (navigation + focus hand-off),
    `toolbar`, `status-bar` (sync status + the overdue-banner slot).
    `overlay-host` is explicitly **not** built (arrives with its
    wave, §4.4).
  - L3 — exactly the three views (§2.1): board (kanban columns),
    stack detail, task detail (+ chrome); plus the
    sync-status/offline indicator (reads the `SystemEvent`
    broadcast + state) and the persistent overdue banner (Q23:
    rides the already-designed reminders signal path — engine
    signal + state read; snooze/mute dispatch `StateCommand`s over
    the existing mpsc).
  - Presentation helpers (§4.3): a Rust `presentation` module of
    pure functions over domain types (`due_label(task, now)`-shaped);
    reuse the domain's ordering semantics (overdue/due-soon
    helpers) — never reimplement them; proptest the time-boundary
    formatting.
  - Responsive composition (§6.3): the `Responsive` global updated
    by the window shell, unscaled breakpoint thresholds (compact
    < 720 / regular / expansive ≥ 1440 — Q16), compositions declare
    panel primary/secondary/tertiary with uniform demotion order;
    board degeneration (§6.4): compact = one stack at a time with a
    stack switcher, tertiary decorations collapse, drag-to-move
    degrades to the `task.move` action flow; kiosk rotation stays a
    later display-mode policy (Q4 — one view tree, no kiosk mode
    now).
  - Focus architecture (§7.1): per-view focus scopes,
    `focus-on-tab-navigation` only on genuinely focusable elements,
    tab order = declaration order, the visible focus ring always
    rendered (a `Semantic` token) when a scope holds focus,
    programmatic focus via the `.slint` `focus()` seam; every
    mouse-only gesture has an action equivalent (drag ⇄
    `task.move`).
  - A11y discipline (§7.2): `accessible-role` + `accessible-label`
    on every interactive element and information-bearing text
    region; compound widgets expose semantic children only.
  - **Non-goals**: nothing from the §2.3 feature-surface table
    (search palette, capture input, today view, overlays, toasts…);
    no telemetry hook of any kind (§6.5); no new actions beyond the
    chunk 2 registry; no engine/domain changes (snooze/mute commands
    already exist).
- **Acceptance**: the three views + two indicators render from an
  injected `AppState` slice (fake data; no engine needed); the board
  degenerates per breakpoint (unit-assert the breakpoint mapping and
  demotion order; visual proof is chunk 4's goldens); every
  interactive element is focus-reachable, declared-order tab
  navigation per view, and carries role + label (a11y discipline —
  unlabelled controls are invisible to chunk 4's journeys); every
  user-visible verb routes through the action layer (no key handlers
  in views); drag affordances have action equivalents; presentation
  helpers are pure and reuse domain helpers (grep for reimplemented
  overdue/due-soon logic — a finding); token lint + contrast checks
  stay green over all new markup; the banner renders from the
  reminders signal path and dispatches typed `StateCommand`s; gates
  green; the fence holds (review diff against §2.2/§2.3).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the GUI Phase 7 implementation ("Component library & the v0.1.0 views", branch feat/ui-views-board) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 1 and 2 (feat/ui-shell-theme, feat/ui-action-keymap) are merged into main — the theme/tokens, forwarder, action registry, keymap resolver, and root FocusScope exist (verify; if not, STOP and report).

1. Read AGENTS.md; the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (normative for this chunk: §4 all, §6.3–6.4, §7.1–7.2, §2.1 shipped surface, §2.3–2.4 fence, §12 row 3; decisions Q4, Q16, Q23); the GUI requirements brief .artifacts/plans/2026-10-14-gui-design-requirements-plan.md (§3 feature surface — the things you must NOT build); and the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§4.
2. Read the seams chunks 1–2 landed: crates/taskboard-ui-slint/ (theme module + lint gate, window shell, actions/keymap modules, root FocusScope, the mpsc command boundary, the kbd-hint seed), plus the domain surfaces the views read: crates/taskboard-domain/src/{entities.rs,state.rs,messages.rs} (Board/Stack/Task display data, EngineSignal vocabulary incl. the reminders overdue path, StateCommand snooze/mute) and the reminders design plan §7/§8 signal path (.artifacts/plans/2026-10-12-reminders-design-plan.md).
3. Create the worktree ../taskboard-WT-feat-ui-views-board on a new branch feat/ui-views-board off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-views-board-plan.md settling at minimum: the widget/composition/view file layout per §4.1 and the per-component property contracts (rule B — domain-meaningful properties for primitives, layout parameters for compositions, state slices + action dispatchers for views); the task-card decoration-slot design (rule D); the presentation-helper module (pure function signatures, which domain helpers it reuses, the proptest catalogue for time-boundary formatting); the Responsive global + breakpoint wiring and each composition's panel demotion declaration; the board degeneration mechanics (stack switcher, collapsed decorations, drag → task.move flow); the per-view focus-scope maps (tab order as designed declaration order) and the a11y role/label inventory per element; the status-bar design (sync-status states, the overdue-banner slot, the startup-warning surface from chunk 2); how views subscribe/poll state (ArcSwap reads, broadcast signals) without mutating anything; and the test catalogue (unit tests for helpers + breakpoint mapping; a11y inventory checklist). Resolve every open question with a recorded decision; deviations from the design plan need a recorded reason.
5. Commit it as: docs(plan): ui component library and board views. Do not implement; do not push.

Report: plan path, decisions, risks (especially focus-order and banner-signal traps), estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the GUI Phase 7 implementation ("Component library & the v0.1.0 views").

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§2 fence, §4, §6.3–6.4, §7.1–7.2, §11/§16 Q4/Q16/Q23), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§4, and the stage-1 chunk plan (.artifacts/plans/*ui-views-board*-plan.md) in the worktree ../taskboard-WT-feat-ui-views-board (branch feat/ui-views-board). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD where Rust-side (presentation helpers, breakpoint mapping), every commit standalone (rebase-merge hygiene). Hard rules: AGPL-3.0-only SPDX headers; nothing from the design §2.3 feature-surface table gets built — if you believe a surface is needed to make a seam real, STOP and report it as a design question (design §2.4); no literal colours/fonts/spacing in any new .slint file (the lint gate is now active — proportional layout expressions are the exemption, do not abuse it); views bind to actions, never keys; the UI crate never mutates state and does no IO.
3. Run the full gate suite from AGENTS.md §10 and make it green (the token lint gate runs in CI — violations are failures, and there is no suppression escape).
4. Do NOT touch the backlog. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, component inventory as landed, any deviation from the plan and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the GUI Phase 7 implementation (branch feat/ui-views-board).

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§2, §4, §6.3–6.4, §7.1–7.2, §11/§16), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§4 (the code-quality mandate and UI-crate hard rules apply to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-views-board.
2. Functional focus: the scope fence — diff the landed views against design §2.2/§2.3 (anything beyond the three views + sync indicator + overdue banner is scope creep; overlay-host must NOT exist); layering rules hold (no view restyles a primitive, no primitive references a composition, compositions hold no domain knowledge, task-card decoration slots exist and receive empty values); every interactive element has accessible-role + label and a designed tab position (grep the a11y inventory vs the diff); every mouse gesture has an action equivalent; the banner rides the reminders signal path and dispatches typed StateCommands (snooze/mute already in the domain — no new command invented); presentation helpers reuse the domain's overdue/due-soon helpers (a reimplementation is an AGENTS §5 finding); breakpoint thresholds are unscaled constants; the focus ring is always rendered when a scope holds focus.
3. Code-quality mandate (§1.3): evaluate code quality (naming, comment discipline, no speculative generality — rule-D slots are properties with empty values, not pre-built decorations); SRP (views present, compositions lay out, primitives render; no business logic anywhere in the crate); single level of abstraction per function body.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 5. Chunk 4 — Test harness & quality gates (`feat/ui-harness-quality`)

- **Covers** (normative: design §7.3; §8.1–8.2; §9.2; §10 component/
  journey + golden + smoke tiers; §11/§16 Q5, Q8, Q17, Q19; §12
  row 4; the pin deviation recorded in §1.4 item 5 above):
  - The `i-slint-backend-testing = "=1.18.1"` dev-dependency lands
    here (exact pin, equal to the slint pin — both move together,
    Q8/Q12), isolated behind a thin journey-driver module (design
    §14 churn-isolation).
  - Logical harness (§8.1): headless journeys with mock time
    (`init_integration_test_with_mock_time`), element addressing via
    accessible-role/label queries, key injection through the same
    root focus path real users hit, component-level state via
    generated getters, commands captured at the forwarder boundary
    with a fake engine (fakes over mocks).
  - The full keyboard-journey suite (§7.3, one continuous session):
    (1) launch → focus in board view; (2) navigate boards/stacks/
    tasks both directions; (3) `task.create` with title;
    (4) `task.edit` open-edit-save; (5) `task.complete`;
    (6) `task.move` to another stack; (7) `task.delete`
    confirmation-free; (8) `app.menu` open + invoke an entry by
    keyboard; (9) reach + read the sync-status indicator;
    (10) `app.exit` clean. Asserts terminal state + focus ownership
    + the typed commands that reached the forwarder. Plus the
    scriptability journey (§8.1): walk the full view graph from a
    cold start — every view reachable by action, no dead ends.
  - Keypress-count guard (§9.2/Q17): per core verb, count physical
    presses from the journeys (a chord = its strokes); CI fails when
    a verb exceeds its recorded ceiling (complete ≤ 2, move ≤ 4,
    create ≤ 3, edit ≤ 3, menu-invoke ≤ 3 — the ceilings are design
    contracts from §9.2, referenced not reinvented); improvements
    always pass.
  - Golden tier (§10): board view at scale extremes × each
    breakpoint, software renderer, headless, bundled fonts;
    perceptual-hash tolerance; renderer-presence assertion first
    (the silent-fallback warning); regeneration is an explicit
    reviewed step, never CI-auto-blessed.
  - MCP harness smoke (§8.2/Q19): the dev-gated `slint/mcp` feature
    + `SLINT_EMIT_DEBUG_INFO=1`/`SLINT_MCP_PORT` wiring in a dev
    runner; one CI job boots the app headless-with-renderer,
    captures one screenshot, asserts non-empty and
    changes-after-interaction; a CI assertion that the release
    profile has the MCP feature off.
  - **Non-goals**: no new views or actions; no driver tooling (the
    backlog "[2026-10-14] LLM/MCP UI-debugging driver tooling" owns
    the polished POST-1.0 wrapper); no external compositor capture;
    no `slint::testing` (it does not exist — stress §3.5); no Slint
    version bump.
- **Acceptance**: all ten journey steps + the view-graph walk green
  in CI, deterministic by construction (mock time; a
  `tokio::time::sleep` anywhere in the harness is a finding);
  journeys assert routing only (typed commands at the forwarder
  boundary — engine semantics are never re-tested; labels are
  element addresses, never message-text assertions); the keypress
  guard enforces the §9.2 ceilings and a strictly-better UI still
  passes; goldens cover the scale-extreme × breakpoint matrix with
  the renderer-presence assertion ahead of any comparison; the MCP
  smoke job is green and the release-profile feature-off assertion
  holds; the exact pin equals the slint pin; the exit-criterion
  "Slint component tests" line of the growth roadmap is satisfied by
  this chunk; gates green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the GUI Phase 7 implementation ("Test harness & quality gates", branch feat/ui-harness-quality) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 1–3 (feat/ui-shell-theme, feat/ui-action-keymap, feat/ui-views-board) are merged into main — the full 1–3 stack exists to be driven (verify; if not, STOP and report).

1. Read AGENTS.md; the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (normative for this chunk: §7.3 journey bar, §8.1–8.2 harness seam, §9.2 keypress guard, §10 tier table, §12 row 4, §14 risks incl. journey brittleness + internal-crate churn; decisions Q5, Q8, Q17, Q19); the stress-test report §3.5 (the testing backend's inventory: init functions, input injection, ElementQuery, mock time, the MCP server) and §5 (pin shape); and the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§5 (note §1.4 item 5: the testing-backend dev-pin lands in THIS chunk).
2. Read the stack you will drive: crates/taskboard-ui-slint/ (window shell, theme, actions/keymap resolver, the three views, status-bar/banner, the command mpsc boundary) and crates/taskboard-app/src/bootstrap.rs (the forwarder you will fake at its boundary).
3. Create the worktree ../taskboard-WT-feat-ui-harness-quality on a new branch feat/ui-harness-quality off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-harness-quality-plan.md settling at minimum: the journey-driver module design (the thin wrapper isolating i-slint-backend-testing's API — init with mock time, key injection through the root focus path, accessible-role/label queries, screenshot capture; everything behind our own small trait so a Slint bump touches one file); the fake engine at the forwarder boundary (typed command capture); the journey suite structure (the ten §7.3 steps as one continuous session; the cold-start view-graph walk; per-step terminal-state + focus-ownership assertions); the keypress-count guard mechanics (counting physical presses incl. chord strokes; reading the §9.2 ceilings from a recorded constants source; failing on exceedance, passing on improvement); the golden-test design (scale-extreme × breakpoint matrix, software renderer selection, bundled-font determinism, perceptual-hash tolerance choice, renderer-presence assertion, the explicit regeneration procedure); the MCP dev-feature wiring + CI smoke job + the release-profile feature-off assertion; and where each test tier lives in the workspace. Resolve every open question with a recorded decision; deviations from the design plan need a recorded reason.
5. Commit it as: docs(plan): ui test harness and quality gates. Do not implement; do not push.

Report: plan path, decisions (especially the perceptual-hash tolerance and driver-module shape), risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the GUI Phase 7 implementation ("Test harness & quality gates").

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§7.3, §8.1–8.2, §9.2, §10, §11/§16 Q5/Q8/Q17/Q19), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§5, and the stage-1 chunk plan (.artifacts/plans/*ui-harness-quality*-plan.md) in the worktree ../taskboard-WT-feat-ui-harness-quality (branch feat/ui-harness-quality). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: the dev-dependency is i-slint-backend-testing = "=1.18.1" (exact pin — verify it equals the slint pin in the root Cargo.toml); NO tokio::time::sleep anywhere — mock time only; journeys assert terminal state + focus ownership + typed commands (accessible labels are element addresses; error/status message text is never matched); the MCP feature is dev-gated only and never in the release feature set; AGPL-3.0-only SPDX headers; no production-behaviour changes to the UI stack (if a journey exposes a real bug, fix the production code on the branch with its own commit — never weaken the journey).
3. Land the golden images with their regeneration procedure documented (regeneration is an explicit reviewed step; CI compares, never blesses).
4. Run the full gate suite from AGENTS.md §10 and make it green. Do NOT touch the backlog. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, journey outcomes, any deviation from the plan and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the GUI Phase 7 implementation (branch feat/ui-harness-quality).

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§7.3, §8, §9.2, §10, §11/§16), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§5 (the code-quality mandate and UI-crate hard rules apply to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-harness-quality.
2. Functional focus: all ten §7.3 steps and the view-graph walk are real assertions (a journey that drives without asserting terminal state or commands is a finding); no sleep exists anywhere in the harness (grep for tokio::time::sleep and std::thread::sleep — mock time is the only clock); the keypress guard fails on exceeding a §9.2 ceiling and cannot fail on improvement; goldens cover the full scale-extreme × breakpoint matrix behind a renderer-presence assertion; the testing-backend pin is exact and equals the slint pin; the MCP feature is absent from release builds (the CI assertion exists and is meaningful); journeys exercise the action layer end-to-end (keys injected at the root focus path, not via internal APIs); the engine is faked at the forwarder boundary only — no engine semantics re-tested (one behaviour, one test, lowest layer).
3. Code-quality mandate (§1.3): evaluate code quality (naming, comment discipline, no speculative generality — no driver tooling beyond the smoke; that is the parked backlog entry's); SRP (the driver module only drives, the fake only captures, guards only guard); single level of abstraction per function body — journey steps should read as orchestrations.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding. Additionally force-fail one golden and one keypress ceiling locally (scratch, never committed) to prove the gates actually bite — restore afterwards and report the proof.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 6. Chunk 5 — ADR, docs amendments & v0.1.0 release (`docs/ui-adr-release`)

- **Covers** (normative: design §13 (ADR draft + numbering gap), §10
  (docs amendments), §11/§16 Q21 (0013 debt), Q11 (licensing
  paragraph); §12 row 5; roadmap Phase 7 exit):
  - `docs/adr/0015-slint-ui-architecture.org`: formalise the §13.2
    draft (verify 0015 is still the next free number at execution
    time by listing `docs/adr/` and grepping recent plans/branches
    for referenced-but-unlanded numbers); the licensing paragraph
    (Q11, stress §4.5); the testing-crate upgrade runbook line;
    Status flip from Draft to Accepted.
  - `docs/architecture.org`: UI-crate section amendments — token
    theming, the action-dispatch layer, component layering, scaling
    model, harness seam; restate the dumb-presentation contract with
    the landed shapes.
  - `docs/testing_strategy.org`: the UI tier table (unit /
    component-journey / golden / MCP smoke — design §10), the pin
    discipline + per-Slint-bump re-audit chore.
  - Backlog bookkeeping: mark
    "[2026-10-07] Add the `slint` dependency to taskboard-ui-slint"
    ✅ DONE with a one-line note (renderer question answered by the
    stress report §3.8; dependency landed in chunk 1); mark
    "[2026-10-09] UI command forwarder bootstrap task" ✅ DONE with a
    one-line note (landed in chunk 1's bootstrap); append the
    ADR-0013 follow-up entry per template ("[2026-10-14] Formalise
    ADR 0013 (reminders) from the reminders design plan §12/§13" —
    the Q21 debt: the number is reserved but no file exists).
  - v0.1.0 release chore (roadmap M2 exit: "tagged v0.1.0 with
    desktop UI, released binaries"): verify the release pipeline is
    GUI-ready (the release binaries carry the UI; the MCP
    feature-off assertion from chunk 4 covers the release profile);
    prepare release notes; print the exact post-merge release
    commands for the user. The bump + tag run via
    `./scripts/bump_version.sh` **after** this PR merges (the script
    commits + tags — running it on the branch would leave the tag
    pointing at a pre-rebase SHA), on a clean tree; the tag push
    (which triggers the release workflow) is the **user's** action —
    agents never push without an explicit instruction (AGENTS §8).
  - **Non-goals**: no code changes (a pipeline fix, if genuinely
    needed, is its own `fix/` or `ci/` PR, not smuggled into a docs
    chunk); no ADR renumbering; no closing of the LLM/MCP driver
    tooling entry.
- **Acceptance**: ADR 0015 lands with the draft's decisions intact
  (amendments need reasons) and the licensing + runbook paragraphs;
  architecture.org and testing_strategy.org reflect the landed
  system (spot-check against the chunks' actual shapes, not the
  design's predictions); both backlog entries carry ✅ DONE with
  accurate notes; the 0013 follow-up entry exists per template; the
  release-readiness verification is evidenced in the plan/report;
  the release command sequence is printed verbatim for the user;
  gates green (docs-only PR).

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the GUI Phase 7 implementation ("ADR, docs amendments & v0.1.0 release", branch docs/ui-adr-release) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 1–4 are merged into main — the full UI foundation, views, and harness exist (verify; if not, STOP and report).

1. Read AGENTS.md; the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (normative for this chunk: §13 draft ADR + §13.1 numbering gap, §10, §11/§16 Q11/Q21, §12 row 5); the growth roadmap .artifacts/plans/2026-10-08-growth-roadmap-plan.md Phase 7 (the fixed exit criterion); the reminders design plan §12/§13 (the ADR-0013 source material); scripts/bump_version.sh (its clean-tree/commit/tag behaviour); the release workflow; and the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§6.
2. Survey what chunks 1–4 actually landed (git log over the four merged branches; read the final shapes of the theme module, action layer, views, harness) — the docs must describe the landed system, not the design's predictions.
3. Create the worktree ../taskboard-WT-docs-ui-adr-release on a new branch docs/ui-adr-release off main.
4. Write the chunk plan at .artifacts/plans/<today>-ui-adr-release-plan.md settling: the ADR-0015 file (finalise §13.2's draft — Status Accepted, date, the licensing paragraph per Q11, the testing-pin upgrade runbook; verify 0015 is still next-free by listing docs/adr AND grepping plans/branches for referenced-but-unlanded numbers ≥0013); the exact architecture.org and testing_strategy.org amendment lists (section by section); the two backlog ✅ DONE notes and the new 0013 follow-up entry text; the release-readiness checklist (release binaries include the UI; MCP feature off in release profile — where that is asserted; release-notes content); and the exact post-merge release command sequence to hand the user (bump_version.sh on a clean tree, tag, tag push — user-executed). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): ui adr docs and v0.1.0 release chore. Do not implement; do not push.

Report: plan path, decisions, the verified ADR number, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the GUI Phase 7 implementation ("ADR, docs amendments & v0.1.0 release").

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§10, §11/§16 Q11/Q21, §13), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§6, and the stage-1 chunk plan (.artifacts/plans/*ui-adr-release*-plan.md) in the worktree ../taskboard-WT-docs-ui-adr-release (branch docs/ui-adr-release). Recreate the worktree on that branch if missing.
2. Execute per the approved plan: write docs/adr/0015-*.org (the design's §13.2 draft formalised — amendments to its decision list need recorded reasons), amend docs/architecture.org and docs/testing_strategy.org against the landed shapes you verified in the survey, edit the backlog (.artifacts/taskboard-potential-improvements-backlog.md): mark "[2026-10-07] Add the `slint` dependency to taskboard-ui-slint" and "[2026-10-09] UI command forwarder bootstrap task" ✅ DONE with one-line notes, append the 0013 follow-up entry per the backlog's template. Every commit standalone with docs(...)/docs(backlog) subjects.
3. Release chore: run the release-readiness checklist from the plan and record the evidence (which workflow builds the UI-carrying binaries; where the release-profile MCP-off assertion lives); write the release notes; do NOT run bump_version.sh on this branch and do NOT create or push any tag — the release commands are printed for the user in the chunk report, to run after this PR merges.
4. Run whatever gates apply (docs-only PR; the pre-commit fmt/clippy hooks run regardless). No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: files landed, backlog edits, release-readiness evidence, the exact user command sequence.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the GUI Phase 7 implementation (branch docs/ui-adr-release).

1. Read AGENTS.md, the accepted design plan .artifacts/plans/2026-10-14-gui-phase7-design-plan.md (§10, §13, §11/§16 Q11/Q21), the dispatch plan .artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md §1/§6, and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-docs-ui-adr-release.
2. Docs review: the ADR matches the §13.2 draft's decisions (any divergence carries a recorded reason) and carries the licensing paragraph + upgrade runbook; architecture.org/testing_strategy.org amendments match the landed code (spot-check the claims against crates/taskboard-ui-slint as merged — paraphrase drift is a finding); the ADR number is 0015 and no collision was introduced (re-verify by listing docs/adr).
3. Bookkeeping review: both backlog entries are marked ✅ DONE with accurate one-line notes naming chunk 1; the 0013 follow-up entry follows the backlog template; "[2026-10-14] LLM/MCP UI-debugging driver tooling" and every other open entry are untouched; no tag was created, no bump_version.sh run happened on the branch.
4. Re-run whatever gates apply (docs-only PR; pre-commit hooks). Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
5. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch until green. The PR description must carry the user's post-merge release commands verbatim (bump_version.sh, tag, tag push). Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation.
```

## 7. Completion bookkeeping

- **Backlog ✅ mapping** (chunk 5 lands every ✅ in one end-of-wave
  audit commit — the design §12 row 5 batches the backlog edits;
  the entries' *substance* closed earlier):
  - "[2026-10-07] Add the `slint` dependency to taskboard-ui-slint"
    — substance closed by **chunk 1** (dependency + renderer
    defaults per stress §3.8); ✅ + note by chunk 5.
  - "[2026-10-09] UI command forwarder bootstrap task" — substance
    closed by **chunk 1** (the bootstrap forwarder); ✅ + note by
    chunk 5.
  - New entry appended by **chunk 5**: "[2026-10-14] Formalise ADR
    0013 (reminders) from the reminders design plan §12/§13" (the
    Q21 debt — reserved number, no file).
  - **Stays open**: "[2026-10-14] LLM/MCP UI-debugging driver
    tooling" (POST-1.0, Q6 resolution — chunk 4 wires only the
    smoke), and every §2.3-related future-work entry.
- **ADR + docs landing**: ADR 0015 (from design §13), the
  architecture.org UI-crate amendments, and the testing_strategy.org
  UI tier table all land in **chunk 5**; chunks 1–4 touch none of
  them (their plans reference the design plan instead).
- **Release**: chunk 5 prepares and verifies; the user executes the
  printed sequence after the chunk-5 PR merges (bump via
  `./scripts/bump_version.sh`, tag `v0.1.0`, tag push triggers the
  release pipeline). Agents never push tags without an explicit
  instruction.
- **Deliberately not dispatched here** (so nobody re-derives the
  boundary): every §2.3 feature surface (Waves 1–3 own them), the
  settings view + in-app conflict-resolution journey (POST-1.0, the
  Q15 end-goal), light/high-contrast themes + runtime theme
  switching (Q13 follow-up), runtime live re-map of keybindings, the
  MCP/LLM driver tooling (backlog entry above), reminders desktop
  toasts (reminders "plan 5" — and toasts are the generic surface,
  Q3), adaptive UI (parked, §6.5 — no telemetry ever), kiosk
  display-mode rotation (Q4 policy, W1+), `backend-linuxkms` and
  skia (deferred to a real kiosk target, Q10), undo/redo
  implementation (D4 round; `app.undo`/`app.redo` stay disabled
  no-ops), rich-text editing (upstream-gated, org-mode report
  phasing), and ADR 0013 formalisation (the chunk-5 backlog entry
  owns it).
- **What opens next**: after chunk 5 merges and the user tags
  v0.1.0, Phase 7 / M2 is complete — wave 1 opens with the
  D1-informed field-family plan (wave-0 dispatch plan §6; the
  deck-roundtrip report is its input).

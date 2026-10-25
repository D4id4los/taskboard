# Audit docs-drift pass (#267 #268 #269 #278 #281 #283/F11-05 #437) — implementation plan

- **Date**: 2026-10-25
- **Chunk**: 5 of the backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` —
  "audit docs drift pass")
- **Branch**: `docs/audit-drift-pass` (base: `origin/main` @ `1c0000b`
  — local `main` was 4 commits behind at planning time, so the fresh
  branch was fast-forwarded to the fetched tip before any commit, the
  chunk-2 precedent)
- **Worktree**: `../taskboard-WT-docs-audit-drift-pass` (flat suffix)
- **Normative sources** (each finding cited by F-number):
  - Issue #267 — F15-04, F15-05, F15-06, F15-07
  - Issue #268 — F15-09, F15-10, F15-11, F15-12, F15-13
  - Issue #269 — F15-15, F15-17, F15-18, F15-19, F15-21, F15-22, F15-23
  - Issue #278 — F8-07
  - Issue #281 — F10-01, F10-03, F10-04
  - Issue #283 (CLOSED completed) — F11-05, the documentation half
    (the issue's code halves F11-03/F11-04 are done or recorded, not
    this chunk's work — §3.7)
  - Issue #437 — the README Download sentence
  - Origin reports: `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-docs-drift-report.md`
    (§9 findings F15-01..23), `…audit-theme-sync-persistence-report.md`
    (F10-01/03/04), `…audit-crate-app-report.md` (F8-07),
    `…audit-theme-time-dst-report.md` (F11-03/04/05)

## 1. Re-verification table (2026-10-25, against `origin/main` @ `1c0000b`)

Every in-scope finding, re-checked against the current files. This
table is the plan's core: it decides fix vs annotate per finding.
Legend: **fixed-already** (no action), **still-drifted** (fix as
specified in §3), **drifted-further** (still-drifted *and* new stale
facts since the 2026-10-16 audit — the fix below covers the new
facts).

| Finding | Verdict | Current evidence (what holds / what rotted) |
|---|---|---|
| F15-04 (signal row UI-only) | **still-drifted** | `docs/architecture.org:187` documents the `EngineSignal` broadcast UI-ward only. Code: `wait_for_boot_pull` subscribes (`crates/taskboard-app/src/bootstrap.rs:1075` `engine.subscribe()`), and a plain daemon legally runs the bus with zero subscribers — the engine publishes fire-and-forget (`crates/taskboard-state/src/actor.rs:561` `let _ = signals.send(...)`). |
| F15-05 (incomplete dep lists) | **drifted-further** | All three audit sites unchanged: state `architecture.org:334` lists 4 deps, manifest has 6 (`thiserror`, `tracing` missing); storage `:404` lists 3, manifest has 7 (`chrono`, `uuid`, `thiserror`, `tracing` missing); app `:563` lists 14, manifest has ~28 (audit flagged 8 missing; now also `notify-rust`, `hyper`, `hyper-util`, `http-body-util`, `chacha20poly1305`, `getrandom`). **New rot since the audit:** the domain list `:291` omits `serde_json` (added by the daily-driver wave — the doc's own prose at `:288` names it!) and the optional `proptest`/`test-support` feature; the ui-slint list `:445` omits `uuid` from the main deps (manifest has it in both sections) and `criterion`/`i-slint-core`/`tracing-subscriber` from dev-only. Five of six crate lists are now stale. |
| F15-06 (versioning example v0.2.3) | **still-drifted** | `docs/architecture.org:614-622` still shows `taskboard v0.2.3` / all crates `@ 0.2.3`; root `Cargo.toml` `version = "0.1.0"` (the only tag). The example now under-states by two released-less waves of work. |
| F15-07 ("task sorting" bench) | **drifted-further** | `architecture.org:603` and `docs/testing_strategy.org:549` still claim criterion tracks "task sorting". No such bench exists. The bench inventory *grew* to three targets — `crates/taskboard-storage-sqlite/benches/hot_path.rs`, `crates/taskboard-sync-nextcloud/benches/mapping.rs`, `crates/taskboard-ui-slint/benches/refresh_draw.rs` — none of which the architecture.org parenthetical names. |
| F15-09 (DI mechanics + type name) | **still-drifted** | `docs/testing_strategy.org:20-24` verbatim: "generics (`T: TaskRepository`) or dynamic dispatch (`Box<dyn TaskRepository>`)" and "the real `SqliteRepository`". Production: `Arc<dyn TaskRepository>` (`crates/taskboard-state/src/actor.rs:255`), and the type is `SqliteTaskRepository` (`crates/taskboard-storage-sqlite/src/repo.rs:33`). No `Box<dyn>` and no generic repository anywhere in production. |
| F15-10 (`Command::TaskCompleted`) | **still-drifted** | `docs/testing_strategy.org:34` verbatim. The real vocabulary: `StateCommand::SetTaskDone { id, done }` (`crates/taskboard-domain/src/messages.rs:50`) inside `EngineEnvelope::Execute` (`crates/taskboard-state/src/actor.rs:49`); no `Command` enum and no `TaskCompleted` variant exist. |
| F15-11 (undecided proptest examples) | **still-drifted** | `docs/testing_strategy.org:63-65` verbatim: "task sorting rules, recurrence generators, or Kiosk rotation schedules". Recurrence remains unscheduled; kiosk shipped *without* rotation schedules (ADR 0016). Shipped proptest targets: the conflict-policy merge algebra (`crates/taskboard-domain/src/merge.rs`), the push-plan laws (`plan_pushes`), and the UI keymap stroke resolver (ui-slint Tier-1 proptests). |
| F15-12 (mutants report convention) | **still-drifted** | `docs/testing_strategy.org:546-548` verbatim: "Surviving mutants are recorded as `.artifacts/reports/YYYY-MM-DD-mutation-testing-report.md`". `ls .artifacts/reports/` shows zero such files — ever. Survivors are CI artifacts of `mutation-testing.yml` (weekly + manual dispatch), per that workflow and the CHANGELOG's own 0.1.0 CI-fix entry. |
| F15-13 (journeys in one file) | **drifted-further** | `docs/testing_strategy.org:286` still attributes the `it_nextcloud_docker_cli_*` journeys to `it_cli.rs` alone. They now number **six in two files**: `it_cli.rs` 4 (`…_login_select_add_verify` :113, `…_remote_create_pull_list` :266, `…_login_rejects_bad_token` :363, `…_connect_journey` :399) and `it_docker.rs` 2 (`…_cli_standalone_publish` :288, `…_cli_editing_push` :414). |
| F15-15 (foreign examples) | **still-drifted** | `docs/coding_guidelines.org:156-162`: the `feat(decoder): Reject BCD-invalid timecode values…ltc_integrity…` commit example is verbatim-foreign (zero `ltc`/`BCD`/`timecode` hits in the repo). `:180`: the branch example `feat/state-kiosk-rotation-timer` still names a subsystem that does not exist — kiosk shipped (ADR 0016) *without* a rotation timer. The other five branch examples are plausible/real-shaped; only this one is the finding. |
| F15-17 (ADR 0011 kiosk consequence) | **drifted-further** | `docs/adr/0011…:32-36` verbatim: kiosk-with-console "now gets a REPL — strictly more useful pre-Phase-7". Overtaken **twice** since the audit: Phase 7's routing (TTY-independent), then the kiosk wave — bare kiosk now routes to the *appliance* (`BareRoute::KioskAppliance`, `crates/taskboard-app/src/ui.rs:172`; ADR 0016), and the daemon gate refuses only `Desktop` (`crates/taskboard-app/src/main.rs:291-295`). The amendment must say "appliance", not "daemon refusal". |
| F15-18 (ADR 0010 "no board-edit commands") | **drifted-further** | `docs/adr/0010…:84-86` verbatim. Superseded, then superseded again: WI-3 shipped `boards rename` (ADR 0004 amendment 2026-10-11), and ADR 0004's amendment 2026-10-18 generalized field-wise merging to all four board fields with local verbs (`boards color/archive/unarchive/delete/reset`). Adjacent rot in the same decision block: `CommandError::BoardExists` (named at `:80`) was *removed* by the multi-board wave (`boards create` always mints — CHANGELOG [Unreleased] "Changed"). |
| F15-19 (ADR 0003 dependency-free clause) | **still-drifted** | `docs/adr/0003…:37-39` verbatim: "`taskboard-ui-slint` ships dependency-free until UI work starts". The condition completed: `slint = "1.18.1"` + `slint-build` + the `unstable-fontique-011` feature are declared in the UI crate's manifest. |
| F15-21 (contradictory daemon entries) | **still-drifted** | `CHANGELOG.md:936-940` still lists `daemon` among the `sync-disabled` verbs (and claims `login` refuses with kind `sync-disabled`) while `:901-904` in the same section says the daemon no longer refuses standalone. Code truth: the daemon boots under standalone (`main.rs:297-301`, the 2026-10-11 amendment comment), and login's explicit-standalone refusal is `backend-explicit-standalone` (`commands/mod.rs:433`). |
| F15-22 (interleaved CHANGELOG categories) | **fixed-already** | `[0.1.0]` now carries exactly one `### Added` (:621), one `### Fixed` (:848), one `### Changed` (:900); `[Unreleased]` has one of each in canonical order (Added :12, Changed :384, Fixed :549). Residual cosmetic nit: `[0.1.0]`'s subsection order is Added→Fixed→Changed, not Keep-a-Changelog's example order. **Decision: annotate only** — reordering a released section for cosmetics is not worth the diff; KaC 1.1 mandates one section per category, not their order. No re-order edit. |
| F15-23 (zero MCP mentions) | **still-drifted** | `grep -ci mcp CHANGELOG.md` = 0. The harness is alive: the `mcp = ["slint/mcp"]` feature (ui-slint manifest), `crates/taskboard-app/tests/mcp_smoke.rs`, the `ui-mcp-smoke` CI job (`ci.yml:189`), and the feature-off release assertions. |
| F8-07 (color-eyre entry-path ruling) | **still-open** | `docs/coding_guidelines.org` §3 has no entry-path paragraph. The ruling's facts moved: `run_desktop` is now `crates/taskboard-app/src/ui.rs:359` with **two** callers, both in the binary's dispatch (`main.rs:118` the explicit `gui` verb, `main.rs:192` the bare-`DesktopUi` route) — the issue's "single binary caller" wording must be codified as "callers live only in the binary's own entry dispatch". One `color_eyre::install()` at `main.rs:73`. |
| F10-01 (ADR 0004 docker-tier citation) | **still-drifted** | `docs/adr/0004…:49` verbatim: "a Phase 4 docker-tier scenario pins the behavior". Still no tier-4 granularity journey (no granularity/entity-touch test in `it_docker.rs`). The unit pin survives at `crates/taskboard-domain/src/merge.rs:1272` (`granularity_trap_remote_entity_touch_overwrites_local_title`). Note: the audit's hermetic citation (`e2e_hermetic.rs:555` I2) is stale — no `e2e_hermetic.rs` exists anymore (the hermetic suite was reorganized into the `cli_*.rs` files); stage 2 greps `crates/taskboard-app/tests/` before citing any hermetic site, and drops the hermetic mention if none pins it. |
| F10-03 (ADR 0010 Bootstrap pre-WI-H) | **still-drifted** | `docs/adr/0010…:67-75` verbatim: sync halves "dropped immediately", "`App` holds the sync actor as `Option<SyncActor>`". Code: the reports sender parks in `App` as the fan-in twin (`bootstrap.rs:967`), and `App` holds `connectors: Arc<Mutex<Vec<SyncActor>>>` (`bootstrap.rs:142`). |
| F10-04 (ADR 0010 SyncDisabled stale) | **still-drifted** | `docs/adr/0010…:127-133` verbatim. The 2026-10-11 daemon amendment still lives only as a code comment (`main.rs:297`), and login refuses with `backend-explicit-standalone` (`commands/mod.rs:433`), not `sync-disabled`. |
| (new, re-verification find) ADR 0010 ceremonies name retired `SetBoard` | **drifted-further** (adjacent to F10-03/F10-04) | `docs/adr/0010…:114` publish flow: "→ `BindBoard` → `SetBoard` → wait". `SetBoard` is retired — `SyncCommand` is `{SyncNow, Detach}`; the ceremony nudges via the engine fanning a broadcast `SyncNow` (`commands/boards.rs:857`) and waits on the bound board's own sync row. Fixing an ADR we are already amending while leaving a known-false command name in it would be malpractice — one correcting sentence rides the same amendment section. |
| F11-05 (UTC-only semantics implicit) | **still-open** (docs half of closed #283) | The `--due` help says "midnight UTC" for the bare date only (`crates/taskboard-app/src/cli.rs:727`); nothing states that offset-carrying RFC-3339 is normalized to UTC (`cli.rs:445-447`), that offset-less timestamps are refused, or that all storage and display is UTC. No config guide exists as a file — README.org's Configuration section is the user-facing config documentation. |
| #437 (README stale release sentence) | **still-drifted** | `README.org:45` verbatim: "Until the first release is cut, build from source:". v0.1.0 shipped 2026-10-16 with attached release archives (named two paragraphs above). |
| F11-03 / F11-04 (#283 code halves) | **out of scope — recorded** | F11-03: addressed in code — `ReminderOffset::before` is fallible and sub-second leniency is documented at the type (truncation, `crates/taskboard-domain/src/reminders.rs:65-69`), the type-driven A1 pattern. F11-04: the freshness check is still the wall-stamp comparison (`commands/sync.rs:162`); the issue closed COMPLETED nonetheless — this chunk does **not** touch that site; the observation stands recorded here for whoever revisits #283. |
| Context (out-of-scope findings already fixed) | fixed-already | F15-01/02/03 (architecture.org exec-summary/ownership/reminders — rewritten, notifier row :194 present), F15-08 (mockall paragraph — replaced by the no-mocking-crate rule, testing_strategy :46-54), F15-14 (merge procedure — rewritten to PR flow, guidelines :190-211), F15-16 (ADR 0013 — formalized, file exists), F15-20 (reminders CHANGELOG entries — present at :827-846). None are this chunk's scope; recorded so stage 2/3 do not re-flag them. |
| ADR 0007 "one oneshot hop" (:156) | **annotate only** (per #281) | The issue routes this wording to the duplicate-create backlog entry's E1 note, not to this chunk. Not edited here. |

## 2. Scope

Files this chunk may touch (the entire allowed diff):

1. `docs/architecture.org` — F15-04, F15-05, F15-06, F15-07.
2. `docs/testing_strategy.org` — F15-07 (shared), F15-09, F15-10,
   F15-11, F15-12, F15-13.
3. `docs/coding_guidelines.org` — F15-15, F8-07.
4. `docs/adr/0003-toolchain-edition-and-quality-gates.org` — F15-19.
5. `docs/adr/0004-sync-conflict-policy.org` — F10-01.
6. `docs/adr/0010-standalone-first-optional-sync-connector.org` —
   F10-03, F10-04, F15-18, plus the `SetBoard` sentence.
7. `docs/adr/0011-interactive-repl-frontend-no-arg-entry.org` — F15-17.
8. `CHANGELOG.md` — F15-21 (merge), F15-23 (MCP entry), F11-05
   (docs entry).
9. `README.org` — #437 (Download sentence), F11-05 (UTC-only note).
10. `crates/taskboard-app/src/cli.rs` — F11-05 help-text doc comments
    **only** (the `--due`/`--scheduled` grammar docs; no behavior, no
    signature, no new test surface — AGENTS §5's text-assertion ban
    means no test can or may pin this text).

Non-goals (binding):

- No code changes beyond the cli.rs doc comments. F11-04's
  freshness site, F11-03's type, and every other audit finding not in
  the seven issues are untouched.
- No ADR 0007 edit (annotate-only, §1).
- No CHANGELOG re-ordering of the released `[0.1.0]` subsections
  (F15-22 residual: annotate-only).
- No `.artifacts/` report or plan files beyond this plan.
- No issue edits/closures from stage 2; the issue-closure note in §6
  is bookkeeping guidance for after the merge.
- No backlog/dispatch-plan edits (the dispatch owns its own
  bookkeeping).

## 3. Per-finding fix decisions

### 3.1 `docs/architecture.org`

- **F15-04** — extend the *State Engine → UI (Signal)* row's
  description with one sentence: the boot-pull waiter also subscribes
  (`wait_for_boot_pull`, ADR 0014's boot ceremony), and the bus
  legally runs with zero subscribers — a plain daemon's publishes are
  fire-and-forget (the result is deliberately ignored). Keep the row's
  existing text; append, don't rewrite.
- **F15-05** — **pointer-ize all six crate dep lists** (the report's
  own preference: "lists rot; pointers don't" — and §1 shows exactly
  that: five of six lists are stale, three of them newly since the
  audit). Each "Dependencies: …" sentence becomes a pointer to the
  crate's `Cargo.toml`, *keeping the normative commentary that rides
  the list*:
  - domain: keep the purity sentence (no I/O, no subscriber, no
    runtime; hand-rolled `BoxFuture` ports; envelopes live in actor
    crates) — drop the enumerated crate names and the
    "(serde, chrono, …)" parenthetical, point at the manifest.
  - state, storage, sync-nextcloud: "Dependencies: see the crate's
    =Cargo.toml=." (sync's list happens to be accurate today; uniform
    treatment is the point).
  - app: pointer + keep the platform-conditional note on `keyring`
    (target-specific feature gates) and the optional
    `console-subscriber`/`tokio-console` feature mention.
  - ui-slint: pointer + keep the two normative notes (tokio is the
    mandated channel fabric, not an IO dependency; the fontique
    feature is additive; exact-pinned `i-slint-backend-testing` is
    dev-only and moves with the slint pin).
  The §5 Tech Stack table (crate purposes) is untouched — it audits
  clean.
- **F15-06** — example tree `0.2.3` → `0.1.0` (six occurrences), plus
  one sentence after the block: the numbers are illustrative —
  `[workspace.package] version` in the root manifest is the only
  truth. This is the anti-rot half so the example cannot silently
  drift again.
- **F15-07** — the criterion row's parenthetical
  "(SQLite caching, task sorting)" → a rot-resistant phrasing naming
  today's three benches without pretending to be an exhaustive
  contract: "(currently the storage hot path, the sync mapping
  planner, and the UI refresh/draw — see the crates' =benches/=)".

### 3.2 `docs/testing_strategy.org`

- **F15-09** — §1 bullets: "via dynamic dispatch
  (=Arc<dyn TaskRepository>=; the engine consumes the port as a trait
  object — no `Box<dyn>`, no generic repository parameter in
  production)" and "taskboard-app injects the real
  =SqliteTaskRepository=" (via the storage port adapter).
- **F15-10** — §2 Execution bullet: "Send an explicit command (e.g.,
  =StateCommand::SetTaskDone { id, done: true }= wrapped in an
  =EngineEnvelope::Execute=) down the engine inbox." Keep the rest of
  the pattern description.
- **F15-11** — §4 Property-Based Testing bullet: swap the three
  examples for shipped ones: "the conflict-policy merge algebra, the
  push-plan laws, or the keymap stroke resolver" (all three have real
  proptest suites today).
- **F15-12** — §13 Mutation Testing bullet: reword the convention to
  the artifact truth: survivors are retained as CI artifacts of the
  `mutation-testing.yml` runs (weekly + manual dispatch); no report
  file is committed under `.artifacts/`.
- **F15-13** — App-level E2E intro: "(=crates/taskboard-app/tests/it_cli.rs=
  and =crates/taskboard-app/tests/it_docker.rs=)" — both files, and
  drop the singular-file phrasing.
- **F15-07 (shared)** — §13 Benchmarks bullet: same parenthetical
  replacement as architecture.org (§3.1), worded once here, once
  there, both naming the three benches + the `benches/` pointer.

### 3.3 `docs/coding_guidelines.org`

- **F15-15** — replace the commit example with a *real* taskboard
  commit, lifted verbatim from `git log` (a `feat(...)` subject plus a
  trimmed body excerpt; stage 2 picks one whose subject survives the
  rebase-merge history — e.g. from the reminders or multi-board
  waves) and replace `feat/state-kiosk-rotation-timer` with a
  real-shaped taskboard branch name from actual history (the
  remaining five examples stand — only the kiosk-rotation one names a
  non-existent subsystem).
- **F8-07** — add one bullet to §3 Error Handling codifying the
  entry-path reading (exact wording in §5 below).

### 3.4 ADRs — amendment-not-rewrite

The house style is a dated amendment section (0004 has two; 0011 has
an inline amendment blockquote; 0010 has the connector-port section
recording the multi-board decision). Every edit below **appends** a
dated amendment section (or blockquote, where the ADR's own convention
is inline) and never rewrites the original decision text. Full
wording in §5. One amendment section per ADR covering that ADR's
findings, one labeled paragraph per finding:

- **0003** — F15-19: the Slint deferral completed on its own condition.
- **0004** — F10-01: the granularity pin's tier corrected (unit-tier
  merge property is the binding enforcement; no docker-tier scenario
  ever existed); the decision (documented granularity data loss)
  stands.
- **0010** — F10-03 (bootstrap shape: parked reports sender + registry
  Vec, cross-link ADR 0014), F10-04 (daemon off the refusal list with
  the 2026-10-11 rationale; login's distinct
  `backend-explicit-standalone` kind), F15-18 (board-edit commands
  exist; `BoardExists` gone; only the creation decision stands), and
  the `SetBoard` ceremony-flow sentence (re-verification find).
- **0011** — F15-17: the kiosk consequence superseded twice over —
  today bare kiosk boots the appliance (ADR 0016); the routing formula
  and default-mode decision stand.

### 3.5 `CHANGELOG.md`

- **F15-21** — merge the contradictory pair inside `[0.1.0]` `###
  Changed` (recipe in §4: edit `:936-940`'s sync-disabled entry to
  drop `daemon`, fix the login kind, and cross-reference the
  daemon-serves-standalone entry, which stays as-is).
- **F15-23** — one dev-facing `### Added` entry in `[Unreleased]`
  documenting the MCP harness (the dev-gated `slint/mcp` feature, the
  headless smoke test it powers, the `ui-mcp-smoke` CI job, and the
  feature-off release assertions). Category mapping: a shipped
  dev-facing capability → `Added` (the file's own precedent: CI jobs
  and dev tooling entries live in `Added`).
- **F11-05** — one short `### Added` entry in `[Unreleased]`
  documenting the UTC-only due-date semantics (category precedent: the
  "README Launching section" docs entry, CHANGELOG `:352-355`).

### 3.6 `README.org`

- **#437** — `:45` "Until the first release is cut, build from
  source:" → "Build from source (the development path — release
  archives carry a pre-built binary):" or equivalent rewording that
  keeps the recipe as the developer alternative. The recipe block
  itself stays.
- **F11-05** — a short "Due dates are UTC-only" note in the
  stand-alone editing quickstart (after the `tasks edit --due`
  example, `~:242`): bare `YYYY-MM-DD` means midnight UTC; RFC-3339
  with an explicit offset is accepted and normalized to UTC;
  offset-less timestamps are refused; storage and all list/banner
  display are UTC (overdue/due-soon comparisons included). Two to four
  sentences, user voice, no ADR citations needed.

### 3.7 `crates/taskboard-app/src/cli.rs` (F11-05 help text)

- The `due` field doc (`:727`) and the `parse_due` doc (`:437-439`,
  `:427-428`): state the full grammar and the UTC-only contract —
  "RFC-3339 with an explicit offset (normalized to UTC), a bare
  =YYYY-MM-DD= (midnight UTC), or =none= (clears). Due dates are
  stored and displayed in UTC only; offset-less timestamps are
  refused." The `--scheduled` field doc says "the `--due`
  vocabulary" and needs no separate statement. Doc comments only —
  no signature, visibility, or behavior change; the existing
  `parse_due` unit tests are untouched and must stay green.

## 4. CHANGELOG normalization recipe

There is no release in this pass, so **no `promote`/`extract` run
happens**. The recipe is:

1. **Edit** the `[0.1.0]` `### Changed` sync-disabled entry
   (`:936-940`) in place:
   - remove `daemon` from the verb list;
   - state login's distinct refusal kind
     (`backend-explicit-standalone`) for the *explicit* standalone
     case;
   - append one clause cross-referencing the sibling entry ("the
     daemon itself no longer refuses under standalone — the Changed
     entry above"), so the section carries one consistent story.
   The daemon entry (`:901-904`) is already correct and stays.
2. **Append** the two new `[Unreleased]` `### Added` entries (F15-23
   MCP, F11-05 UTC docs) at the end of the existing `### Added` block
   (:12–382) — never opening a second block (F15-22's lesson).
3. **Category mapping** (binding for this pass): user/dev-facing
   capabilities, tooling, CI jobs, and docs entries → `### Added`;
   behavior/semantic changes → `### Changed` (the F15-21 merge stays
   `Changed`); no `Deprecated`/`Removed`/`Security` blocks are needed
   and none may be invented. One block per category per section,
   canonical order (Added → Changed → Fixed).
4. **Validate**: `python3 scripts/changelog.py self-test` (the golden
   self-tests; the same check CI's `check` job runs), plus a
   structural render check — `grep -n '^## \|^### ' CHANGELOG.md`
   must show exactly one `###` block per category in each `##`
   section.
5. **Accepted risk** (recorded): editing the released `[0.1.0]`
   section changes what a future `changelog.py extract 0.1.0` yields,
   and the already-published GitHub release body is not regenerated.
   The file is the going-forward source of truth and the edit removes
   a self-contradiction; this is the audit's own proposed action.

## 5. ADR amendment wordings (verbatim drafts)

Stage 2 may adjust line-level phrasing; the semantics are binding.
Each amendment carries today's date and names its audit finding.

**ADR 0003 — append after the existing "Amendment (2026-10-12)"
section:**

> * Amendment (2026-10-25): the Slint deferral completed (audit
>   F15-19). The Decision's "dependency-free until UI work starts"
>   clause completed on its own condition: UI work started (Phase 7,
>   ADR 0015) and =slint= 1.18.1 — plus =slint-build= and the additive
>   font-registration feature — is pinned in =[workspace.dependencies]=
>   and declared by the UI crate. The deferral did its job (headless
>   builds and CI ran Slint-free until Phase 7) and is recorded here
>   as completed, not reverted; the =#![forbid(unsafe_code)]= exception
>   for Slint-generated code is its surviving legacy.

**ADR 0004 — append after the "Amendment 2026-10-18" section:**

> * Amendment 2026-10-25 — the granularity pin's tier (audit F10-01).
>   The Decision's sentence "a Phase 4 docker-tier scenario pins the
>   behavior" never matched the tree: no tier-4 journey drives the
>   granularity-loss case (the docker journeys pin round-trips, moves,
>   archive, cascade, labels, and field writes). The binding
>   enforcement is the pure-merge tier —
>   =granularity_trap_remote_entity_touch_overwrites_local_title= in
>   =crates/taskboard-domain/src/merge.rs= (stage 2: re-verify the
>   hermetic-tier citation before naming one; the audit's
>   =e2e_hermetic.rs= site no longer exists). That tier is the right
>   one: the trap is a property of the merge algebra, not of the actor
>   wiring a docker journey exercises. This amendment corrects the
>   citation only; the decision — granularity data loss documented and
>   deliberate — stands unchanged.

**ADR 0010 — append one "Amendment (2026-10-25) — audit corrections"
section with four labeled paragraphs:**

> * Amendment (2026-10-25) — audit corrections (issues #281, #269).
>   Four facts in this ADR lagged the code; the decisions stand, the
>   text is corrected:
> * *Bootstrap after WI-H (F10-03).* The Bootstrap section's "dropped
>   immediately" and "=App= holds the sync actor as =Option<SyncActor>="
>   describe the pre-attach shape. Since WI-H (ADR 0014) the
>   sync-command *receiver* is dropped under standalone, but the
>   sync-report *sender* deliberately parks in =App= as the fan-in
>   inbox's twin, so an in-session attach needs no rewiring; =App=
>   holds a connector registry (=Mutex<Vec<SyncActor>>=, at-most-one
>   in v1) rather than an =Option=. Fail-closed sends when no
>   connector is live and join-only-live-actors shutdown are unchanged.
> * *The SyncDisabled surface (F10-04).* =daemon= no longer refuses
>   under standalone — the 2026-10-11 amendment (recorded until now
>   only in the daemon gate's code comment) made the daemon
>   backend-agnostic: its purpose is "own the core, serve commands
>   over IPC", and a standalone daemon gives scripters the same
>   no-boot-per-command latency. =login= under an *explicit*
>   standalone backend refuses with the distinct kind
>   =backend-explicit-standalone= — it guards an explicitly written
>   dead config, not a connector-absent default. =sync= and
>   =boards list/select/publish= refuse =sync-disabled= at handler
>   entry exactly as decided.
> * *Board-edit commands exist (F15-18).* The CreateBoard section's
>   "there are still no local board-edit commands" is superseded:
>   WI-3 shipped =boards rename= (ADR 0004's 2026-10-11 amendment) and
>   ADR 0004's 2026-10-18 amendment generalized field-wise merging to
>   all four board fields with local verbs. The named
>   =BoardExists= refusal is also gone (the multi-board wave lifted
>   the single-board invariant; =boards create= always mints). Only
>   the creation decision — =CreateBoard= mints locally, no outbox op
>   — stands.
> * *Ceremonies flow.* The publish/select flows' "→ =BindBoard= →
>   =SetBoard= → wait" step names the retired =SetBoard= command: the
>   bind is now followed by the engine fanning a broadcast =SyncNow=
>   to every registered connector, and the ceremony waits on the
>   bound board's own sync row (=SyncCommand= is ={SyncNow, Detach}=).

**ADR 0011 — an inline amendment blockquote right after the accepted-
consequence sentence (:32-36), matching the ADR's existing 2026-10-14
blockquote convention:**

> #+begin_quote
> *Amendment (2026-10-25):* the accepted consequence above — a kiosk
> machine with a console "now gets a REPL" — described the
> pre-Phase-7 world and was superseded twice: Phase 7's routing made
> =Kiosk= TTY-independent, and the kiosk wave (ADR 0016) now routes
> bare kiosk to the *appliance* lifecycle (the display-gated
> fullscreen window with a loud headless fallback) in every TTY cell.
> A kiosk console gets neither a REPL nor a daemon refusal; =Desktop=
> is the mode the daemon gate refuses. The routing formula
> (=has_command × stdin_tty × mode=) and the default-mode decision
> stand. (Audit F15-17.)
> #+end_quote

**coding_guidelines §3 — new bullet (F8-07), appended after the
color-eyre bullet:**

> - *The entry-path reading of the binary rule:* =color-eyre= is for
>   the functions that *are* the binary's entry paths, not every line
>   physically inside =taskboard-app=. Entry paths are =main.rs= and
>   lib-located lifecycle entrypoints whose only callers live in the
>   binary's own entry dispatch — e.g. =run_desktop=, the desktop
>   lifecycle's =main=-equivalent, whose =color_eyre::Result= is
>   formatted by the same top-level =color_eyre::install()=. Everything
>   below an entry path returns typed errors (=thiserror=); a
>   lib-located function with any caller outside the binary's entry
>   dispatch does not qualify. (The 2026-10-16 audit's recorded
>   ruling, F8-07.)

## 6. Acceptance, gates, bookkeeping

**Acceptance** (stage 3 reviews against this list):

- Every §1 "still-drifted"/"drifted-further" row has a corresponding
  edit in the allowed file set; every §1 "fixed-already"/"annotate"
  row has **no** diff.
- The diff is docs-only plus cli.rs doc comments; `git diff --stat`
  names exactly the ten files of §2.
- ADR diffs are purely additive amendment sections/blockquotes —
  `git diff` shows zero deletions inside the original decision text
  (the amendment-not-rewrite check).
- CHANGELOG: one `###` block per category per section
  (`grep -n '^## \|^### ' CHANGELOG.md`); `changelog.py self-test`
  green.
- No test asserts the changed help text; `cargo nextest run
  --workspace` and the full AGENTS §10 gate suite are green
  (fmt/clippy run over the doc-comment change too).

**Bookkeeping after merge** (not stage 2's job; the user/dispatch
owns it): #267, #268, #269, #278, #281, #437 are closable once this
lands (each with a one-line note pointing at the PR); #283 stays
closed — optionally a comment noting F11-05's docs landed here and
F11-04's freshness site remains wall-stamp-based as recorded in §1.

**Risks:**

1. **Amendment-as-rewrite**: a heavy hand could turn corrections into
   decision changes. Mitigation: the acceptance check above (zero
   deletions in original ADR text) and the verbatim drafts in §5.
2. **Pointer-izing dep lists** removes a grep target some readers
   prefer. Mitigation: the normative commentary sentences stay
   adjacent to every pointer (§3.1); the manifests are the truth the
   pointers name.
3. **Parallel-chunk conflicts in CHANGELOG `[Unreleased]`**: sibling
   waves append to the same section. Mitigation: append-only at block
   ends; stage 3's rebase resolves trivially.
4. **cli.rs doc-comment drift**: help text is user-facing; wording
   must stay consistent with the README note (§3.6) — same three
   facts, same order (parse → normalize → display).
5. **Hermetic-citation trap in ADR 0004**: the audit's `e2e_hermetic`
   site is gone; citing it verbatim would re-plant the exact drift
   class this pass fixes. Stage 2 greps before citing (drafted in §5).
6. **Branch base**: the branch was cut at `origin/main` @ `1c0000b`
   (local `main` lagged); stage 3's standard rebase dance covers any
   further upstream movement.

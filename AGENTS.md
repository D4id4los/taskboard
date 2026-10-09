# AGENTS.md — Taskboard Project Guidelines

Instructions for LLM coding agents (and humans) working in this repository.
Read this file **before** writing any code or opening a pull request. The
full engineering context lives in `docs/architecture.org`,
`docs/coding_guidelines.org`, and `docs/testing_strategy.org`.

## 1. Project Identity & Architecture

- **Project Context**: `taskboard` is a cross-platform task management and
  reminder system designed for both passive Kiosk displays (low-power,
  offline-resilient) and interactive Desktop environments.
- **Core Architecture**: Headless Core with an Event-Driven Multi-Actor
  Architecture running on the `tokio` async runtime.
- **Framework Agnostic**: The UI layer acts strictly as a "dumb presentation
  client" that reads from a central state and dispatches events. Do not
  embed business logic or state mutation inside the UI layer.
- **Target Platforms**: x86_64 Linux is the dev platform; first-class binary
  releases for x86_64 Linux & Windows; second priority for 64-bit MacOS,
  64-bit ARM Linux & Windows.
- **Target Hardware**: The app targets not just desktop PCs but low-powered
  kiosk machines, so good performance and modest memory usage are project
  goals.

## 2. Workspace Crate Boundaries

The project is a Cargo workspace using the `taskboard-*` prefix. Adhere
strictly to these dependency boundaries:

- `taskboard-domain`: Pure Rust data structures, inter-actor message enums,
  and trait interfaces (Ports). **Rule:** zero network, UI, or asynchronous
  runtime dependencies. Pure functions only.
- `taskboard-state`: The State Engine actor. **Rule:** processes incoming
  commands and updates local state. Sole source of truth for application
  state; owns the write side of `ArcSwap<AppState>`.
- `taskboard-sync-nextcloud`: Nextcloud REST/WebDAV API actor.
- `taskboard-storage-sqlite`: Local disk caching actor using `sqlx`.
- `taskboard-ui-slint`: Swappable UI layer. **Rule:** cannot depend on IO
  crates; presentation logic only.
- `taskboard-app`: The main executable runner that bootstraps channels,
  reads config via `figment`, and spawns Tokio actors.

## 3. Inter-Actor Communication Protocols

Never use orchestrator bottlenecks or shared mutable state locks (`RwLock`
or `Mutex`) for cross-actor communication. Use these typed primitives:

- **State → UI (reads)**: `arc_swap::ArcSwap<AppState>` — lock-free,
  zero-contention state reads on the UI rendering thread.
- **Commands to actors**: `tokio::sync::mpsc` for point-to-point commands
  (e.g., UI sending user intents to the State Engine, State Engine
  commanding the database).
- **System notifications**: `tokio::sync::broadcast` for global status
  changes (e.g., `SystemEvent::NetworkLost`).
- **Asynchronous queries**: embed a `tokio::sync::oneshot::Sender` inside an
  `mpsc` message payload for request-response cycles.

## 4. Tech Stack & Implementation Rules

When generating code, rely exclusively on the approved project stack:

| Category | Approved Crate/Tool | Implementation Rule |
|---|---|---|
| Error handling (domain/libs) | `thiserror` | Derive static, strongly-typed error enums (`#[derive(Error)]`). |
| Error handling (binary) | `color-eyre` | Use in `taskboard-app` to capture and format top-level dynamic errors. |
| Time & dates | `chrono` | Use for time, schedules, timezone management, and API formatting. |
| Identifiers | `uuid` | Generate v4 (random) or v7 (time-ordered) UUIDs. |
| Configuration | `figment` | Hierarchical: defaults → `taskboard.toml` → `TASKBOARD_*` env. |
| Logging | `tracing` | Contextual spans across `.await` boundaries; include the `log` compatibility feature flag. |

All shared dependency versions are pinned once in the root `Cargo.toml`
`[workspace.dependencies]` — never add an unpinned duplicate to a member
crate.

## 5. TDD & Testing Directives

Testability is a primary design goal. Use TDD when developing new features
and fixing bugs. Full strategy: `docs/testing_strategy.org`. Hard rules:

- **Time manipulation**: never use `tokio::time::sleep` in tests. Use
  `tokio::time::pause()` and `tokio::time::advance()` to fast-forward the
  async runtime clock deterministically.
- **Snapshot testing**: do not test the UI framework's state internally. Use
  `insta` to snapshot test the `StateEngine`: feed a sequence of commands
  and verify the resulting `AppState` tree.
- **Fakes over mocks**: favor handwritten fakes (e.g., a `HashMap`
  implementing a repository trait) over pre-programmed mocking libraries for
  stateful components.
- **Property testing**: use `proptest` to validate pure domain logic
  (sorting, scheduling) with randomized inputs.
- **A new test failing can be a bug**: when a newly written test fails,
  treat a bug in the production code as equally likely as a bug in the
  test. Evaluate both before deciding what to change.
- **Do not weaken a correct test to make it pass**: when a test found an
  actual bug, you may not weaken it. If fixing production code is
  out-of-scope, leave the test failing and note the bug in your final
  report plus a proposed fix prompt for a future agent.
- **No quality ceilings**: never assert upper bounds on success (decode
  rate ≤ X%, "at most N" for things that should ideally succeed). Floors
  and ceilings on garbage input are fine. Litmus test: if the code got
  strictly better, would this test still pass?
- **No text assertions**: never assert error message, status, log, or UI
  label text (including substrings and disjunctions). Assert typed outcomes
  (`matches!(err, E::Variant)`) or structural facts instead.
- **One behavior, one test, lowest layer**: don't duplicate a unit test
  through the engine thread or the integration suite; higher layers test
  routing only. Don't test test-helpers, derived trait impls, or inline
  reimplementations of production code.
- **Deterministic by construction**: no fixed sleeps waiting for state;
  poll a predicate with a deadline, join handles, or inject a clock seam.
  Assert terminal state, never transient intermediates or wall-clock time.
- **No pins of "today's behavior"**: if a comment says "current behavior",
  the assertion is probably a limitation, not a contract. Test the desired
  semantics; if undesired behavior must be tolerated temporarily, name the
  test `..._currently_pins_...` so it reads as debt.

## 6. Licensing Headers

The workspace uses a Dual-Tier Licensing model. Include the appropriate
SPDX header at the top of every new `.rs` file:

- Library/Domain crates (`taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud`):
  `// SPDX-License-Identifier: MIT OR Apache-2.0`
- Application/UI crates (`taskboard-app`, `taskboard-ui-slint`):
  `// SPDX-License-Identifier: AGPL-3.0-only`

Every crate except `taskboard-ui-slint` must carry
`#![forbid(unsafe_code)]`.

## 7. Artefact Management: Plans & Reports

All AI-generated non-code artefacts MUST be stored under `.artifacts/` at
the repository root. Never dump temporary notes, analysis files, or
implementation specs into the root directory or source folders.

```
.artifacts/
├── plans/     # Forward-looking implementation specs (YYYY-MM-DD-<topic>-plan.md)
└── reports/   # Backward-looking analyses, benchmarks, audits (YYYY-MM-DD-<topic>-report.md)
```

- Generate a plan **before** multi-file refactors, complex feature
  additions, or architectural changes; follow its template.
- Large plans split into phases use:
  `YYYY-MM-DD-<overall-name>-phase<N>-<phase-name>-plan.md`.
- Improvement ideas that are out-of-scope must also be appended to
  `.artifacts/taskboard-potential-improvements-backlog.md` using the entry
  template in that file.

## 8. Git & Workflow Rules

- **Never push to a remote** without an explicit instruction from the user.
- **Never merge a feature branch into `main`** (double-rebase style) without
  an explicit order from the user; the user reviews changes before they
  enter `main`.
- Do create a feature branch for every code change:
  `<category>/<workspace-scope>-<short-description>` (categories: `feat/`,
  `fix/`, `refactor/`, `test/`, `docs/`, `ci/`, `chore/`).
- Do commit freely to feature branches as work progresses; never work
  directly on `main`. (Exception: repository-bootstrap commits recorded at
  the very start of the project's history.)
- Commits use `<type>(<module>): <short-desc>` plus a body. Types: `feat`,
  `fix`, `refactor`, `test`, `docs`, `ci`, `chore`.
- Parallel worktrees are created next to the project root as
  `<project-root>-WT-<branch-name>`.
- `gh` is available for CI triage (`gh run list/watch/view --log-failed`,
  `gh pr checks`).
- **PR granularity**: default to **one branch/PR per plan** when the plan is
  one coherent deliverable. Use **stacked PRs** (each branch based on the
  previous one, all PRs targeted at `main`) only when a single PR does not
  make sense — e.g. independently reviewable slices of a large effort, or
  parallelizable work that must land in order.
- **Stacked PRs + squash merges require the rebase dance.** `main` is
  squash-merged, so when an upstream PR in a stack lands, every descendant
  branch still contains the original (now duplicate) commits and GitHub
  reports phantom conflicts. The agent doing stacked-PR work should *expect*
  this after each merge and fix it proactively — rebasing the branch onto
  `origin/main` while dropping the already-merged commits:

  ```sh
  git fetch origin --prune
  git rebase --onto origin/main <tip-of-the-branch-that-just-landed> <my-branch>
  # resolve nothing unless real conflicts appear; then:
  git push --force-with-lease origin <my-branch>
  ```

  When handing the merge to the user, print the exact commands so they can
  be copy-pasted. `--force-with-lease` is mandatory; never plain `--force`.
  PRs are not affected by any of this if nothing is stacked behind them.

## 9. Versioning

Single global semver across all crates, defined in
`[workspace.package] version` in the root `Cargo.toml` and inherited by
every member. Bump releases exclusively via `./scripts/bump_version.sh
<major|minor|patch>`, which verifies a clean tree, bumps the version,
stages and commits all changes, and tags `vX.Y.Z`.

## 10. Code Quality Gates (non-negotiable)

Before every handoff:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
cargo deny check --workspace
cargo llvm-cov --workspace
```

Do not offer a result containing `clippy` warnings or formatting drift.

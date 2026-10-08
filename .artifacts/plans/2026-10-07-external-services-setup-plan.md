# [Plan] External Services Setup & Repository Handoff Runbook

- **Date**: 2026-10-07
- **Target**: Repository infrastructure (GitHub, CI integrations) — no application code
- **Branch**: N/A (runbook). Agent fixes at handback land on feature branches per
  `docs/coding_guidelines.org` §7 (main is frozen after bootstrap).
- **Goal**: Get the bootstrapped repository onto GitHub with CI, protection, and the
  free-tier integration services configured; clearly split manual (human) steps from
  agent-executable steps, and define the exact handback format.
- **Status**: ✅ COMPLETE (updated 2026-10-08). All tasks implemented and
  merged: remote pushed, CI green, ruleset + secrets + hooks done, Bencher
  (PR #7) and SonarCloud (PRs #6/#9) integrated. Exception: optional §3.7
  Snyk stays deliberately skipped (backlog entry).

---

## 1. Context & Objectives

The bootstrap commits (5 on `main`, up to `9ccd573`) created the workspace, docs,
CI workflows, and tooling. Everything that requires a *remote* or *external
account* could not be done locally:

- the repository URL is still the placeholder `https://github.com/your-username/taskboard`
  (found in `Cargo.toml:17` and `README.org:4`),
- CI has never run (workflows only execute after the first push),
- Codecov / SonarCloud / Bencher need accounts, repo registration, and secrets,
- branch protection can only be configured once the repo exists.

Two defects were discovered during plan preparation and MUST be repaired before
pushing (agent task, §5.0):

1. `.github/workflows/ci.yml` references `dtolnay/rust-toolchain@v1` (3 jobs).
   **That ref does not exist** (the action uses `@stable` / `@nightly` / version
   refs), so all three jobs would fail on the very first run.
2. `LICENSE-MIT` line 1 says `Copyright (c) The Rust Project Contributors`
   (artefact of fetching the canonical text). Must become the project's copyright
   holder.

## 2. Division of Labour (Overview)

| Step | Owner | Topic |
|---|---|---|
| §3.1–3.3 | **YOU** | Visibility decision, GitHub repo creation, `gh` auth check |
| §3.4 | **YOU** (or agent after §4) | Branch protection ruleset for `main` |
| §3.5 | **YOU** | GitHub Actions settings |
| §3.6 | **YOU** | Codecov registration + secret |
| §3.7 | **YOU** (optional) | SonarCloud, Bencher, Snyk |
| §4 | **YOU → agent** | Fill the handback report (copy-paste template) |
| §5.0–5.7 | **AGENT** | Repairs, placeholder replacement, push, CI verification, integrations |

**Security rule (hard):** never paste secret *values* (tokens, app passwords) into
the chat, this plan, or any repo file. I only ever need to know *that* a secret is
set, under which name. You create secrets yourself in the GitHub web UI.

---

## 3. YOUR Tasks (manual, in order)

### 3.1 Decide the repository visibility

- **Public** (recommended): free branch protection & rulesets, tokenless Codecov
  uploads (token optional but recommended), all services from the notes usable on
  free tiers. Remember the dual-tier licensing is designed for this.
- **Private**: branch protection requires a paid GitHub plan (Pro/Team); Codecov
  requires the upload token (step 3.6 mandatory).

Record your choice — it goes into the handback report.

### 3.2 Create the GitHub repository

1. Go to **https://github.com/new** (Sign in → top-right `+` → *New repository*).
2. Repository name: `taskboard`
3. Description (suggestion): *Cross-platform task management & kiosk display app in
   Rust — async multi-actor core, Nextcloud sync, offline-first.*
4. Choose Public / Private per §3.1.
5. **Leave every checkbox under "Initialize this repository with" UNCHECKED**
   (no README, no .gitignore, no license — the repo already has all three; a
   remote commit would diverge history and force a merge).
6. Click *Create repository*. GitHub then shows a "…or push an existing
   repository" box — note the URLs it offers (SSH / HTTPS) but **do not push
   yet** (repairs from §5.0 go first).

### 3.3 Check `gh` CLI authentication (local, in a terminal)

```sh
gh auth status
```

- If it shows you logged in: nothing to do.
- If not: run `gh auth login` and follow the prompts (choose GitHub.com → SSH or
  HTTPS → authenticate via browser). This is interactive, so it is a human step.

### 3.4 Branch protection for `main`

Can be done immediately after 3.2. In the repo on GitHub:

1. **Settings → Rules → Rulesets → New ruleset → New branch ruleset**
   (classic equivalent: *Settings → Branches → Add branch protection rule*).
2. Ruleset Name: `protect-main`, Enforcement status: **Active**.
3. Target branches: *Add target* → *By name* → `~DEFAULT_BRANCH` (or literal `main`).
4. Enable rules:
   - ✅ *Require a pull request before merging*: **Required approvals: 0**
     (solo project — you cannot approve your own PR; 0 approvals still forces
     the PR + checks flow that the double-rebase workflow expects).
   - ✅ *Require status checks to pass* → *Add checks* → add **after the first
     CI run exists** (§3.4a below), the exact check names from `ci.yml`:
     - `Code Quality & Linting`
     - `Security & License Audit`
     - `Test Execution`
     - `Code Coverage`
     - (`Performance Benchmarking` — optional, it is a no-op until benches exist)
   - ✅ *Require branches to be up to date before merging* (matches the
     double-rebase style).
   - ✅ *Block force pushes* and ✅ *Block deletions*.
5. If you are on a private/free plan and Rulesets are unavailable: skip and note
   `branch_protection: skipped (plan limitation)` in the handback report.

**3.4a** The status-check list can only be filled after the first CI run has
executed. Easiest sequence: leave "Require status checks" OFF for now → after the
agent reports the first green run, come back and add the checks → flip the
ruleset to Active. Alternatively let the agent do this step via `gh api` at
handback (say so in the report: `branch_protection: agent-task`).

### 3.5 GitHub Actions settings

Repo → **Settings → Actions → General**:

1. *Actions permissions*: **Allow all actions and reusable workflows** (we use
   third-party actions: `dtolnay/rust-toolchain`, `Swatinem/rust-cache`,
   `taiki-e/install-action`, `EmbarkStudios/cargo-deny-action`,
   `codecov/codecov-action`). Private repos restrict this by default; public
   repos already allow it.
2. *Workflow permissions*: keep **Read repository contents and packages
   permissions** (the default). None of our workflows need write.
3. *Allow GitHub Actions to create and approve pull requests*: leave **unchecked**
   (revisit only if we ever add Dependabot auto-merge).

Optional quality-of-life, *Settings → General → Pull Requests*:
✅ *Automatically delete head branches*.

### 3.6 Codecov (coverage service)

1. Go to **https://app.codecov.io** → *Sign in with GitHub* → authorize the
   Codecov OAuth app (it needs read access to check runs/statuses).
2. The repo should appear automatically under your account after the first push.
   For **public** repos uploads work tokenlessly; the token is still recommended
   (avoids upload rate limits).
3. If you want the token (mandatory for private repos): select the *taskboard*
   repo in Codecov → **Settings → Global Upload Token** → copy.
4. Create the GitHub secret: repo → **Settings → Secrets and variables → Actions
   → New repository secret** → Name: `CODECOV_TOKEN`, Value: *paste the token*.
5. In the handback report, answer only: repo visible in Codecov yes/no, and
   whether the secret is set. Never paste the token itself.

### 3.7 Optional services (can also be skipped entirely)

Each is already recorded in
`.artifacts/taskboard-potential-improvements-backlog.md`; doing them now or
later makes no difference to the core setup.

**SonarCloud** (static analysis):
1. **https://sonarcloud.io** → *Sign in with GitHub* → authorize the SonarCloud
   GitHub App (grant repo access).
2. *Create project* → *With GitHub Actions* → pick organization (create one if
   asked) → set project display name `taskboard`, main branch `main`.
3. SonarCloud shows the analysis setup: note your **organization key** and
   **project key** (both non-secret — these I do need as text).
4. Where it says *SONAR_TOKEN*: create the repo secret `SONAR_TOKEN` on GitHub
   (Settings → Secrets and variables → Actions) yourself.

**Bencher** (benchmark tracking):
1. **https://bencher.dev** → *Sign up with GitHub* → Console → *Create project*,
   name `taskboard`.
2. Console → **API Tokens** → *Add token* → copy → GitHub secret
   `BENCHER_API_TOKEN`.
3. Note the project slug (usually `taskboard`) in the report. Deferring is fine —
   the `benchmark` job is a green no-op until criterion benches exist.

**Snyk** (vulnerable-dependency PRs):
1. **https://snyk.io** → sign in with GitHub → *Integrations → GitHub* → grant
   access to the repo. Snyk then opens fix PRs from its own app; no GitHub
   secret is needed. Fully skippable for now.

### 3.8 Enable the git hooks (local; or let the agent do it)

```sh
cd /home/viktoria/src/taskboard && git config core.hooksPath .githooks
```

Alternatively answer `hooks: agent-task` in the report and I will set it.

---

## 4. What the Agent Needs Back — and in What Form

Copy the block below into the chat, fill in every `<...>`, and send it. This is
the complete handback contract; with it (and only with it) the agent proceeds
with §5.

```text
# taskboard setup — handback report
github_username: <your GitHub username>
remote_url: <git@github.com:USER/taskboard.git  or  https://github.com/USER/taskboard.git>
ssh_or_https: <ssh | https>
visibility: <public | private>
push_authorized: <yes | no>            # "yes" = agent may create origin & push main
gh_cli_authenticated: <yes | no>
hooks: <done-by-me | agent-task>
branch_protection: <configured-by-me | agent-task | skipped (reason)>
codecov_repo_added: <yes | no>
CODECOV_TOKEN_secret: <set | not set | not-needed-public>
sonarcloud: <skip | done>
  sonar_org_key: <text, non-secret>
  sonar_project_key: <text, non-secret>
  SONAR_TOKEN_secret: <set | not set>
bencher: <skip | done>
  bencher_project: <slug>
  BENCHER_API_TOKEN_secret: <set | not set>
snyk: <skip | connected>
notes: <anything else, one line>
```

Reminder: `push_authorized: yes` is the explicit direction required by the git
usage policy (AGENTS.md §8) for the first push; without it the agent stops
after local repairs and reports readiness.

---

## 5. Agent Tasks (after handback report arrives)

### 5.0 Pre-push repairs (feature branch `fix/ci-action-refs-licenses`, merged on
user's order, or folded into the first `ci/` push as the user prefers)

- [ ] `ci.yml`: replace `dtolnay/rust-toolchain@v1` → `@stable` (drop the
  `toolchain:` input; keep `components:`); miri.yml already correctly uses `@nightly`.
- [ ] `LICENSE-MIT` line 1 → `Copyright (c) 2026 Viktoria Pogrzebacz`.
- [ ] Replace `your-username` placeholder: `Cargo.toml` `repository`,
  `README.org` CI badge.
- [ ] Re-verify: `cargo deny check`, `cargo fmt --check`, clippy (metadata-only
  changes, but cheap).
- [ ] Commit this plan file (`.artifacts/plans/2026-10-07-external-services-setup-plan.md`)
  with the repairs, in the appropriate grouping.

### 5.1 Remote & push

- [ ] `git remote add origin <remote_url>` (verify with `git remote -v`),
  `git push -u origin main` — **only if** `push_authorized: yes`.
- [ ] Confirm `gh repo view` shows the pushed branch.

### 5.2 First CI run verification & triage

- [ ] `gh run list`, `gh run watch` (or poll) the push-triggered `CI` run.
- [ ] On failure: `gh run view <id> --log-failed`, diagnose, fix on a
  `ci/...` branch, re-present for review (no direct `main` commits).
- [ ] Known watch items: Codecov tokenless upload on public repos (rate-limit
  failures would need the token from §3.6), the no-op `benchmark` job.

### 5.3 Branch protection consistency

- [ ] Once CI job names are confirmed by a real run, cross-check them against
  the status checks configured in §3.4 (mismatched names silently never gate).
  If `branch_protection: agent-task`, configure the ruleset via `gh api`.

### 5.4 Scheduled workflows smoke test

- [ ] `gh workflow list` (mutation-testing.yml, miri.yml should be listed).
- [ ] `gh workflow run mutation-testing.yml` and `gh workflow run miri.yml` —
  dispatchable only because they sit on the default branch (runs execute the
  file from `main`); watch the first runs, triage failures (mutants/Miri are
  slow by design — confirm they at least start and report artifacts).

### 5.5 Optional integration wiring (only for services marked `done`)

- [ ] SonarCloud: add `sonar-project.properties` (org/project keys) and a
  `sonarqube` job step to `ci.yml` on a `ci/sonarcloud-integration` branch.
- [ ] Bencher: add the `bencherdev/bencher-action` step to the `benchmark` job
  (project slug from report) on a `ci/bencher-integration` branch.
- [ ] Both only merged on the user's explicit order (double-rebase policy).

### 5.6 Local & documentation follow-ups

- [ ] `git config core.hooksPath .githooks` (if `hooks: agent-task`).
- [ ] CHANGELOG `[Unreleased]`: add the CI-integration entries.
- [ ] Backlog housekeeping: strike/annotate backlog items that §3.7 completed;
  keep the release-pipeline item open.

### 5.7 Final handback report to the user

- [ ] Summarize: commit/branch state, CI job results, service links (Actions,
  Codecov project, …), anything left open.

## 6. Not Touched / Out-of-Scope

- Release binary pipeline (cargo-dist vs. matrix workflow) — backlog item, its
  own future plan.
- `slint` dependency, MSRV CI job — backlog items (ADR 0003).
- Nextcloud *runtime* configuration (`taskboard.toml`, app passwords) — that is
  application config for a running instance, not repository setup.
- Any secret values (never handled by the agent), and pushing/merging without
  the explicit authorizations defined above.

## 7. Verification & Acceptance Strategy

Done means, verifiably:

1. `git remote -v` shows `origin`; `main` (5 bootstrap commits + repair
   commits) is pushed; `git status` clean.
2. `gh run list --workflow ci.yml` shows a green run on `main` with the five
   jobs (check, deny, test, coverage, benchmark) executed.
3. Codecov shows the repo/coverage (if selected in the report).
4. The `protect-main` ruleset is Active with status checks whose names match
   real check-run names.
5. Optional services marked `done` in the report have a green run/artifact or
   dashboard entry; those marked `skip` remain untouched and stay in the backlog.
6. `gh workflow run mutation-testing.yml` / `miri.yml` have completed without
   infrastructural failures (long runtimes expected).

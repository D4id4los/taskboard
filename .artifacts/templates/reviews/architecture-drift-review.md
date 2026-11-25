<!--
Recurring review template — EXECUTED IN PLACE by an unattended ZCode
idle-time run. Unlike the other files in this folder it is never
copied and filled in. Queue it from a lean session with the one-line
prompt in README.md; the run reads this file, the common rules it
binds, and AGENTS.md. Cadence and mode are in §1. Shared rules live in
_common-rules.md; the binding prelude below is the one block restated
in every template — change all four copies together. Never edit this
file while a run that uses it is queued.
-->

# Architecture Drift Review

Did the code that landed since this crate slice was last reviewed keep
the house architecture? One slice per run, chosen by churn: mechanical
sweeps over the whole slice for the rules a pattern can find, then
reading for the rules it cannot. It complements the stage-3 review of
each chunk, which sees one plan at a time.

> **Binding prelude (the common rules' safety frame, restated).**
> 1. Analysis only: write nothing but this run's report under
>    `.artifacts/reports/` and files in a `/tmp` scratch directory.
> 2. Git: `git fetch` and the §3.2 fast-forward are the only writes —
>    no commit, push, PR, merge, branch, stash, checkout, or reset.
> 3. Cargo: never bare — `scripts/memory-gate.sh` for every command,
>    `scripts/run-quality-gates.sh` for the coverage trio.
> 4. GitHub: create `backlog:` issues and comment; never close,
>    reopen, relabel, edit, or touch pull requests.
> 5. Nobody answers questions: record them in the report and proceed.
> 6. Processes: stop only PIDs this run recorded — never by name.

Read first: `AGENTS.md`, then
`.artifacts/templates/reviews/_common-rules.md` (all of it binds this
run), then §3 below.

## 1. Cadence and slices

- Slug: `architecture-drift`. Report:
  `.artifacts/reports/YYYY-MM-DD-architecture-drift-report.md`. One run
  per ticket, monthly.
- Exactly one slice per run — the one with the most commits since its
  last review (`git rev-list --count <reviewed_commit>..HEAD --
  <paths>`; each slice's `reviewed_commit` is in the previous report's
  Metrics). Ties go to the slice longest unreviewed.

| Id | Slice | Paths | Last review |
|---|---|---|---|
| A | UI | `crates/taskboard-ui-slint/` | `.artifacts/reports/2026-11-24-ui-slint-architecture-review-report.md` (`main` @ `df5ae1a0`; F-1…F-10 ↔ #1349–#1362, comments on #1280, #1127, #680) |
| B | App | `crates/taskboard-app/` | none — baseline mode |
| C | Core | `crates/taskboard-domain/`, `crates/taskboard-state/` | none — baseline mode |
| D | Sync and storage | `crates/taskboard-sync-nextcloud/`, `crates/taskboard-sync-files/`, `crates/taskboard-sync-files-nextcloud/`, `crates/taskboard-storage-sqlite/` | none — baseline mode |

Baseline mode for B–D: their base is HEAD and the delta is empty, so
the run is the structure read of step 5. The 2026-10-16 audit reports
(`.artifacts/reports/2026-10-16-v0.1.0-audit-*-report.md`) describe
these crates at a sixth of today's size; read the crate and theme
reports for the slice, and treat their rulings as standing.

## 2. Preconditions

Anchoring per the common rules §3. A slice whose crate was added,
split, or renamed since the last report: re-map and record it.

## 3. Normative inputs

- `AGENTS.md` §1–§4 and §6.
- `docs/coding_guidelines.org` §2 (boundaries and purity), §2.1 (the
  type-driven policy), §2.2 (clock discipline), §3 (errors), §4
  (logging), §5 (state sharing).
- `docs/architecture.org` — Inter-Component Communication Protocol;
  Crate Roles & Dependencies. `docs/flow_model.org`. `docs/adr/`.
- Slice A only: `docs/widget_interactions.org` (usage rules, the
  mount checklist); `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md`
  §4.2 (layering rules A–E) and §5.1 (views never see keys).
- The exemplar to imitate in form: the 2026-11-24 UI review —
  compliance table, findings with file:line, non-findings.

## 4. The rule set

A pattern decides only what the pattern can; every hit is then read.

| # | Rule | Origin | How |
|---|---|---|---|
| R1 | Crate boundaries hold: the domain stays pure, the UI does no IO, each actor crate keeps to its role, the app is the only composition root | AGENTS §2; coding guidelines §2; architecture.org, Crate Roles | `scripts/memory-gate.sh cargo tree -p <crate> --depth 1`; cross-crate `use` of another crate's internals. The dependency bans are CI's — report only what the graph cannot show |
| R2 | Actors talk through typed channels; `Mutex`/`RwLock` never cross actors | AGENTS §3; coding guidelines §2, §5 | outside tests: `Mutex|RwLock|parking_lot` — each hit is an intra-actor internal (allowed) or a cross-actor share (finding) |
| R3 | The UI reads snapshots and dispatches without blocking | coding guidelines §5 | slice A: `\.send\(\)\s*\.await`, `blocking_send`, `block_on`, `async fn`, `tokio::spawn`, `tokio::time::(sleep\|interval)`, `Runtime::new` |
| R4 | The UI is a dumb presentation client | AGENTS §1; coding guidelines §5; design §4.2 | slice A, by reading: functions in `src/` that compute domain semantics — sort policy, overdue and due-soon sets, domain text parsing — instead of calling `taskboard_domain` |
| R5 | Clock discipline: one sanctioned wall read, injected clocks, `now` as data | coding guidelines §2.2; testing strategy §6.1–6.2 | outside tests and `crates/taskboard-domain/src/clock.rs`: `Utc::now\(`, `Local::now\(`, `SystemTime::now\(`, `Instant::now(` — each hit is the sanctioned cadence or render clock, or a stamp |
| R6 | Values are minted at the trust boundary and stay in the minted type | coding guidelines §2.1 | by reading new inputs since base — argv, environment and config, network responses, database rows, file content: a fallible mint, or a check followed by a bare `String` or integer. Also any dependency on `nutype`, `bounded-integer`, `refined` |
| R7 | Typed errors in libraries; `color-eyre` only at entry paths; no `unwrap`/`expect` in non-test library code unless proven and documented | coding guidelines §3 | outside tests in library crates: `\.unwrap\(\)`, `\.expect\(` — each judged; and `color_eyre` below an entry path |
| R8 | `tracing` only, under the level contract; user content never above `trace` | coding guidelines §4 | library crates: `println!`, `eprintln!`, `dbg!`; new `info!`/`warn!`/`error!` sites read for user content |
| R9 | Approved stack, versions pinned once | AGENTS §4 | member `Cargo.toml`s: an inline dependency version instead of `workspace = true`; a crate outside the approved set for its role |
| R10 | Licence header on every `.rs` file; `#![forbid(unsafe_code)]` on every crate root | AGENTS §6 | a census with `command grep` — the shell's `grep` skips binary files |
| R11 | No hand-maintained list that can lag silently | the #854 law; the two-sided rule in `tests/inventory/view_mount_inventory.rs` | by reading: new registries, exemption lists, and match tables — what pins their totality? |
| R12 | A mirrored policy carries a parity pin | the 2026-11-24 review §3 (F-3, F-5) | by reading: new pure functions that restate another crate's predicate |
| R13 | Markup: views never see keys, no hand-rolled activation block outside `ui/widgets/`, sections gated by mount rather than `visible:` | widget_interactions.org usage rules; design §4.2, §5.1; coding guidelines §5 | slice A, in `ui/` outside `ui/widgets/`: `key-pressed`, `TouchArea\s*\{`, `FocusScope\s*\{`, `visible:\s*false`, conditional `visible:`. Sanctioned leaves are those the 2026-11-24 review §4 lists |
| R14 | Concentration | the 2026-11-24 review §3 | metrics: files over 2,000 lines and their growth, the largest functions |

## 5. Method

**Step 1 — Select the slice**, then anchor on that slice's
`reviewed_commit` (common rules §3).

**Step 2 — Reconcile the ledger** (common rules §4). For each finding
of the previous report, the code at HEAD decides the status, not the
issue state: still present, partly migrated, grown, gone.

**Step 3 — Map the delta.** `git log --oneline <base>..HEAD --
<paths>`; `git diff --stat <base>..HEAD -- <paths>`; new files
(`--diff-filter=A --name-only`); new public items
(`git diff <base>..HEAD -- <paths> | rg '^\+.*\bpub\b'`); the files
that grew most. Baseline mode skips to step 5.

**Step 4 — Mechanical sweeps over the whole slice**, not only its
delta. A hit is *known* (listed as accepted residue in the previous
report), *new*, or *gone*. Known hits that shrank are ledger progress;
new hits are findings until read and accepted with a reason. Exclude
`#[cfg(test)]` spans and `tests/` directories from the Rust patterns.

**Step 5 — Read for the rules a pattern cannot decide**, in this
order: new files; new public items; the changed regions of files
already flagged as over-concentrated; new trust-boundary paths; new
registries. For each, name the rule it tests and whether the code
holds.

Baseline mode is a structure read instead: crate roles and dependency
graph; the module map with line counts and `//!` headers; the public
surface; then a deep read of the ten largest modules' production code.

When the delta's changed production lines exceed 10,000, read by this
priority and list under Reproduction what was not read. The run is then
PARTIAL, and that list is the next run's head start.

**Step 6 — Findings, ledger, report** (common rules §4–§6, §9). Each
finding carries file:line, the rule and its origin, why it matters,
and a refactor shape. Rules that hold go to Non-findings with their
evidence, so no later run re-derives them. Where a rule and the code
disagree and the code may be right, write a Question.

**Recorded rulings are not re-litigated without new facts**: no crate
split of `taskboard-ui-slint` (the 2026-11-24 review §6); A1 as the
house pattern with A2–A4 conditional (coding guidelines §2.1); the
chrono posture (#891); every ADR in `docs/adr/`.

## 6. Family metrics

TOML with, per slice: `id`, `paths`, `reviewed_commit`, file count,
an estimate of production lines, a `[[large_file]]` table (path,
lines), hit counts per rule, and — when PARTIAL — the unreviewed part
of the delta.

## 7. Out of scope

Fixing. Test quality — the test-strength review owns it. Visual and
interaction quality — the design review owns it. Dependency advisories
and licences — `cargo deny` decides them. Anything an inventory test
or lint already gates, except to report that a new hit appeared.

# Chunk Plan — N-F: chrono time-stack evaluation

- **Chunk**: post-round-9 parallel round, §8 N-F
- **Branch**: `docs/chrono-time-stack-evaluation` (worktree
  `../taskboard-WT-docs-chrono-time-stack-evaluation`)
- **Normative sources**: recommendation report §Tier-1 N-F
  (`.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`),
  dispatch plan §1/§8/§9
  (`.artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md`),
  issue #891, AGENTS §4 (chrono = approved pinned dependency).
- **Type**: research. **Deliverable**: evidence report + ruling
  recommendation, ending at an operator gate. Zero production code.

## Hard boundary: zero production code

No `.rs` file, `Cargo.toml` entry, `deny.toml` edit, or Slint file may
change on this branch. The PR diff is exactly: the chunk plan (already
committed), the report, and the ruling doc. Any discovered code defect
goes to a `backlog:` issue, not into the branch. The existing
`deny.toml` ignore of RUSTSEC-2026-0543 stays untouched by this chunk
(it is the interim state the ruling evaluates).

## Evidence mandate (binding, dispatch plan §1 verbatim)

Every factual claim carries evidence (the command run + observed
outcome, or a verifiable source + what it states); experiments are
reproducible from the report's method section; recommendations are
decision-grade (options, trade-offs, exactly one recommendation,
consequences for later work); repo hygiene (scratch code and captured
payloads never land — evidence is excerpts inside the report); scope
discipline (out-of-scope ideas go to the backlog, not into the report).

## 1. Verbatim advisory record (already captured at planning; re-verified by stage 2)

Source: https://RustSec.org/advisories/RUSTSEC-2026-0543.html
(observed 2026-11-18).

- **ID**: RUSTSEC-2026-0543; **type**: INFO / Unmaintained
- **Package**: chrono; **affected**: all versions; **patched**: none
- **Reported**: 2026-09-14; **issued**: 2026-11-17
- **Description** (verbatim): "The chrono crate is unmaintained; all
  versions are affected."
- **Reference**: https://github.com/chronotope/chrono/issues/1768
- Stage 2 fetches the upstream issue #1768 too and quotes its
  load-bearing statements verbatim (development-paused claim, stated
  reasons, any successor recommendation).

## 2. Call-site inventory method

Purpose: size the migration surface with recorded, re-runnable
commands. All commands run from the worktree root and recorded with
their observed output in the report.

### 2a. Dependency-graph position

```sh
cargo tree -i chrono          # inverse tree: every path into chrono
grep -n chrono Cargo.toml     # the workspace pin: chrono 0.4, serde+clock
grep -rn "chrono" crates/*/Cargo.toml
```

**Recorded at planning (2026-11-18, worktree @ cd9d00fa):**

- Pin: `chrono = { version = "0.4", features = ["serde", "clock"] }`
  in root `[workspace.dependencies]`; all 8 member crates consume it
  via `chrono.workspace = true` (domain, state, sync-nextcloud,
  sync-files, sync-files-nextcloud, storage-sqlite, ui-slint, app).
- **Material finding**: `cargo tree -i chrono` shows chrono 0.4.45 is
  *also* an indirect dependency of Slint (`i-slint-core v1.18.1`).
  Even a full workspace migration does not remove chrono from the
  dependency graph unless/until Slint itself moves — the exposure
  assessment must separate "our code touching chrono" (controllable)
  from "chrono in the graph via Slint" (only removable upstream). This
  bounds what any migration ruling can actually achieve and must be
  checked for both jiff and time (does i-slint-core depend on either?).

### 2b. Call-site census

Count by crate and by module so the migration-cost surface is
weighted, not just totalled:

```sh
# total occurrences + touched files
grep -rn "chrono::" crates --include="*.rs" | wc -l
grep -rln "chrono::" crates --include="*.rs" | wc -l

# per crate, and within each crate per module directory
for c in domain state sync-nextcloud sync-files sync-files-nextcloud \
         storage-sqlite ui-slint app; do
  grep -rn "chrono::" crates/taskboard-$c --include="*.rs" | wc -l
done
```

**Recorded at planning**: 2230 `chrono::` occurrences across 174 files
(worktree @ cd9d00fa). Per-crate/per-module split, plus the
type-occurrence census below, is stage-2 work.

### 2c. Type-occurrence census (beyond `chrono::` paths)

The `chrono::` grep misses `use chrono::...` names used bare
(`NaiveDateTime`, `DateTime<Utc>`, etc.). The census therefore also
counts the concrete type/method names that a migration would rewrite:

- Types: `DateTime`, `NaiveDate`, `NaiveDateTime`, `NaiveTime`,
  `Utc`, `Local`, `TimeZone`, `Duration`, `Month`, `Weekday`
  (`chrono::Duration` vs `std::time::Duration` ambiguity included).
- Serialization boundaries: `serde` impls touching chrono types
  (`grep -rn "serialize\|Deserialize" crates --include="*.rs"` filtered
  to files also matching the type census), sqlx column mappings
  (`crates/taskboard-storage-sqlite`), and Nextcloud Deck REST
  payload parsing (`crates/taskboard-sync-nextcloud`).
- Deliverable: a table crate × module × dominant chrono types, which
  feeds the migration-cost surface (§4).

## 3. Exposure-assessment frame

What does "unmaintained" actually risk for *this* crate's usage? The
report answers each with evidence, not vibes:

1. **What the advisory claims** — verbatim (§1 above); what it does
   NOT claim: no vulnerability is known; it is an INFO/unmaintained
   advisory, not a security defect. Quote the upstream issue's stated
   reasons for pausing.
2. **Y2K-style correctness gates** — chrono's known historical
   correctness edge (pre-1970/post-ceiling date arithmetic). Assess
   whether taskboard's domain (task schedules, reminders, recurrence)
   ever exercises those edges; evidence = which chrono APIs the census
   found.
3. **tzdata staleness** — chrono embeds/toggles tz handling; assess
   which tz APIs the workspace uses (`Utc`, `Local`, named zones?)
   and what staleness would mean for reminders. Evidence from the
   census + the clock-seam inventory
   (`.artifacts/reports/2026-11-06-clock-seam-audit-report.md`).
4. **No security patches** — the practical consequence: if a
   vulnerability surfaces post-pause, fixes depend on community forks.
   Frame the *trigger* question: what would force a move (a real
   RUSTSEC vuln, a distro flag, Slint itself migrating)? Record the
   revisit trigger explicitly (issue #891 asks for it).
5. **Controllability split** — from §2a: our call sites vs the Slint
   transitive path. A ruling that migrates workspace code still leaves
   the deny-ignore in place while Slint depends on chrono; the ruling
   must say so.

## 4. Alternative set (minimum; extend only if evidence warrants)

Each alternative gets: fit against the census surface, maturity/
maintenance evidence (crates.io activity, RustSec DB, ecosystem
adoption — with sources), and a migration-cost surface checklist:

- **A. Stay on chrono** with the deny-ignore formalized, a dated
  re-evaluation trigger, and (optionally) an internal wrapper seam so
  a later move is cheaper. Trade-off: zero churn now vs unmaintained
  exposure persists.
- **B. jiff** (the advisory's recommended alternative). Fit: civil
  datetime, tzdb handling, serde support; check recurrence/schedule
  arithmetic and `Duration` semantics against the census.
- **C. time** (the other advisory-named candidate). Fit: narrower,
  `time::Date`/`OffsetDateTime`; check tz and sqlx support against
  the census (sqlx 0.9 `chrono` feature is already in use — what does
  sqlx 0.9 offer for jiff/time?).
- Others (e.g. `hifitime`, internal minimal types) only if stage-2
  evidence shows A–C inadequate.

**Migration-cost surface checklist** (each alternative assessed
against each item; "not covered" is a finding):

- [ ] sync HTTP-date handling (`crates/taskboard-sync-nextcloud`,
      `sync-files-nextcloud` — HTTP `Date:` headers, Deck timestamps)
- [ ] recurrence / schedule math (domain scheduling, reminder
      triggers)
- [ ] habit windows (domain habit streak/window arithmetic)
- [ ] the #133 skew constants (clock-skew handling, seconds-domain
      `chrono::Duration` arithmetic)
- [ ] serialization boundaries: serde (app state persistence), sqlx
      (sqlite storage), Nextcloud REST payloads
- [ ] UI formatting/format-strings (`crates/taskboard-ui-slint`,
      format templates fed from state)
- [ ] test fixtures & snapshot expectations that embed chrono-derived
      text (clock discipline §6.1–6.2: freeze the render clock at
      migration time)
- [ ] workspace pin + AGENTS §4 stack-table update (operator ruling
      required; migration changes the approved table)
- [ ] the Slint transitive chrono path (§2a) — what remains regardless

## 5. Report skeleton

`.artifacts/reports/2026-11-18-chrono-time-stack-evaluation-report.md`:

1. Method (this §2, verbatim commands + observed outcomes)
2. Advisory record (verbatim §1)
3. Dependency-graph position (§2a findings)
4. Call-site census tables (§2b–c)
5. Exposure assessment (§3, item by item)
6. Alternatives A–C with migration-cost surface checklists (§4)
7. Recommendation: exactly one, with consequences
8. Gate handoff (§6 below)
9. Backlog issues raised during the chunk

## 6. Ruling skeleton

`.artifacts/plans/2026-11-18-chrono-time-stack-ruling.md`:

1. The recommendation (one sentence) + the alternatives rejected, with
   the one-line reason each
2. Consequences of the chosen path (what changes, what churn, what
   risk remains accepted)
3. The revisit trigger if "stay" (dated re-evaluation, explicit
   forcing conditions); the dispatch shape if "move" (what a migration
   dispatch would chunk, its risk profile, its PR granularity)
4. The operator gate: what the operator is asked to accept; note that
   a migration changes the AGENTS §4 approved-stack table (operator
   ruling required — an agent cannot edit that table)
5. Status: PROPOSED until the operator accepts (mirrors the #708
   design-round pattern; issue #891 stays open through the gate)

## 7. Long-command discipline

`cargo tree` and the grep census are the only potentially slow
commands observed so far (both completed in seconds at planning). If
stage 2 adds a build-dependent check (e.g. `cargo metadata`), run it
backgrounded with useful work between polls; commit landed work before
any wait. Gate runs: docs-only chunk — pre-commit hooks are the gate;
defer per the #879/#894 flock-deferral rule if contended, reporting
"gate deferred: lock held by <holder>".

## 8. Risks carried into stage 2

- **Slint transitive dependency** (§2a): the strongest migration
  narrative ("remove chrono") is only partly achievable; the ruling
  must not overclaim.
- **Evidence-only claims**: any capability claim about jiff/time
  without a cited source is a review finding (stage 3 checks this).
- **Snapshot/clock coupling**: migration cost includes clock-frozen
  fixtures (AGENTS §5 clock discipline); census must catch them.

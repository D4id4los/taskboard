# Ruling — chrono time stack (issue #891, RUSTSEC-2026-0543)

- **Status: PROPOSED** — until the operator accepts it at the gate
  (§4). Mirrors the #708 design-round pattern; issue #891 stays open
  through the gate.
- **Evidence base**: `.artifacts/reports/2026-11-18-chrono-time-stack-evaluation-report.md`
  (every claim below traces to a § there).

## 1. The recommendation (one sentence)

**Stay on chrono 0.4, keep the existing `deny.toml`
RUSTSEC-2026-0543 ignore, and record the dated revisit triggers in
§3 — no migration dispatch now.**

Alternatives rejected, one line each:

- **jiff** — rejected today: sqlx 0.9.0 has no jiff feature (docs.rs
  sqlx 0.9.0 features page), so the sqlite codec seam has no drop-in
  support; and the graph-level payoff is nil while Slint pulls chrono
  (`i-slint-core v1.18.1`).
- **time** — rejected today: no built-in IANA tzdb support (docs.rs
  time 0.3.55) against the workspace's `Local`-day seams, and the
  same zero graph-level payoff.
- **Others** — not assessed: no forcing condition met (report §6).

## 2. Consequences of staying

- **What changes**: nothing in code, `Cargo.toml`, or `deny.toml`.
  AGENTS §4's approved-stack table stays as-is (chrono remains
  approved; no operator table edit needed for this path).
- **What churn is avoided**: the 2230 `chrono::` occurrences across
  174 files (report §4a), the 45 serde-touching files, the
  clock-frozen fixture rework (202 snapshot files), the duration-type
  semantic rewrite (`TimeDelta` 1017 → `Span`/`SignedDuration`).
- **Risk accepted**: chrono gets no upstream security fixes (report
  §5.4); the concrete exposure is the parsing surface fed by the
  user's own Nextcloud server (RFC 2822 HTTP-date at
  `taskboard-sync-nextcloud/src/client.rs:344`, RFC 3339 payloads,
  sqlite text columns) and the system-tzdata dependence through
  `chrono::Local` (85 occurrences, 11 files). Slint keeps chrono in
  the dependency graph regardless, so even a full migration would not
  remove the deny-ignore today (report §3 controllability split).
- **Prep option** (non-blocking): an internal wrapper seam to make a
  later move cheaper — filed as a backlog issue, not scheduled.

## 3. Revisit triggers (binding on this ruling)

Re-evaluate — and only then consider a migration dispatch — when any
of these fire:

1. **A real RUSTSEC vulnerability advisory** against chrono (not an
   unmaintained INFO) with no fork patch available.
2. **sqlx gains jiff support** (or the workspace's storage seam
   otherwise unblocks) — removes the jiff blocker.
3. **Slint migrates off chrono** (or a Slint release drops
   `i-slint-core`'s chrono dependency) — makes the graph-level
   payoff real.
4. **A distro/security flag** (Debian/RH cargo-vet, cargo-auditable
   pipeline, or an enterprise consumer) forces removal of unmaintained
   dependencies.
5. **Dated re-evaluation**: 2027-11-18 (12 months) — whether or not a
   trigger fired, re-check the upstream issue
   (chronotope/chrono#1768) for handoff (a qualified maintainer taking
   over would void the unmaintained advisory) and the sqlx/Slint
   posture.

Trigger tracking home: the backlog issue(s) filed with this ruling
(numbers in the final report / issue #891 thread).

## 4. Operator gate

The operator is asked to accept exactly this: **stay on chrono under
the §3 revisit triggers.** Notes:

- This path requires no AGENTS §4 table edit (chrono remains the
  approved stack).
- A *move* ruling would have required an operator edit of the AGENTS §4
  approved-stack table (an agent cannot change that table) plus the
  dispatch shape below — recorded here so the cost of the rejected
  path stays visible.
- **If the operator overrides with "move"** (recorded for
  completeness, not recommended): the migration dispatch would chunk
  as (a) domain + state (the recurrence/calendar core, largest and
  most property-tested surface), (b) storage-sqlite codec + sqlx
  feature switch, (c) sync crates (HTTP-date, Deck payloads, skew
  band), (d) ui-slint + app presentation/formatting + clock-frozen
  fixture rework, (e) pin/AGENTS-table/deny-ignore update. Stacked
  PRs in that order; risk profile: high in (a)/(d) (semantic
  duration rewrite + 202 snapshot files under clock discipline
  §6.1–6.2), medium in (b)/(c), with the known cap that Slint keeps
  chrono in the graph until trigger 3 fires.

## 5. Status

- **PROPOSED** — awaiting operator acceptance at the gate; issue #891
  remains open through the gate; this doc plus the evaluation report
  are the artefacts the operator reviews.

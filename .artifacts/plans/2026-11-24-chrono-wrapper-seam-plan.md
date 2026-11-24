# Chunk E2 Plan — the chrono wrapper seam (#987)

Round 6, chunk E2 · branch `refactor/chrono-wrapper-seam` · 2026-11-24
Stage-1 planning artefact. Normative inputs: issue #987 (scoped first
step), the #891 ruling (`stay on chrono`, cheapened exit),
`.artifacts/reports/2026-11-18-chrono-time-stack-evaluation-report.md`
§4c/§6, scope report §E, dispatch plan §14.

## 1. The ruling and the binding scope law

#891's ruling is **stay on chrono** (RUSTSEC-2026-0543 is
unmaintained-only, no vuln; sqlx has no jiff feature; Slint keeps
chrono in the graph regardless). #987 records the *optional prep*:
"a thin internal time-wrapper seam in taskboard-domain so a later
move to jiff/time rewrites one seam instead of 2230 `chrono::`
occurrences across 174 files."

**Scope discipline (binding):** this chunk lands the seam + the call
sites the issue's scoped first step names — NOT a workspace migration.
The #891 ruling was "stay with a cheapened exit"; the seam *is* that
exit. Every other chrono call site stays direct chrono, recorded
below. **Behavior-identical is a hard requirement** — serialized
corpora (insta snapshots, sqlite fixtures, wire recordings) unchanged.

## 2. Census at the chunk's start point (`c2b74486`)

The #987 body's 2230/174 figure was observed 2026-11-18; the tree has
grown. Fresh census (stage-2 re-runs the same commands; drift between
planning and implementation is expected and fine — the *scope*, not
the count, is the contract):

```
grep -rn "chrono::" crates --include="*.rs" | wc -l   → 2684
grep -rln "chrono::" crates --include="*.rs" | wc -l  →  192
domain 999 · ui-slint 813 · app 439 · storage-sqlite 103 ·
state 132 · sync-files-nextcloud 77 · sync-nextcloud 95 ·
sync-files 26
```

## 3. Existing seams — coordinate, don't duplicate

- **Injection seam (exists):** `crates/taskboard-domain/src/clock.rs`
  — the `Arc<dyn Clock>` trait (`now() -> DateTime<Utc>`) with
  `SystemClock`; merge functions take time as data. Round D1's
  test-support work (folding the ~7 local `FakeClock`/`FrozenClock`
  doubles onto one shared class) is *injection-level* and orthogonal:
  a shared frozen clock still implements `domain::Clock`; an alias
  retarget of the trait's signature types changes nothing for it.
  This chunk must not touch D1's doubles, and vice versa.
- **No time-formatting helper module exists** — UI formats chrono
  values inline (~24 `format!` sites in ui-slint per the census §4c).
- **No `chrono-tz`** — tz exposure is `chrono::Local` + `FixedOffset`
  only.

## 4. Decision — the seam's shape

**A new pure module `crates/taskboard-domain/src/time.rs`**, re-exported
from `lib.rs` as `pub mod time;`. It is a *facade*, not a newtype layer:
chrono types under aliases plus thin function wrappers at the exact
operations a migration would have to translate. No behavior, no
serialization change — an alias is the same type, so this is
type-identical by construction, stronger than behavior-identical.

Contents (exactly what the in-scope sites need; grow only when a site
migrates):

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The time-type seam (#987). ...
pub type Timestamp = chrono::DateTime<chrono::Utc>;
pub type LocalDate = chrono::NaiveDate;
pub type LocalTime = chrono::NaiveTime;
pub type Delta     = chrono::TimeDelta;

pub fn delta_seconds(secs: i64) -> Delta { chrono::TimeDelta::seconds(secs) }
pub fn parse_http_date(s: &str) -> Option<Timestamp> {
    chrono::DateTime::parse_from_rfc2822(s).ok().map(|d| d.with_timezone(&chrono::Utc))
}
```

Plus the module doc stating the retarget policy: a future migration
(jiff/time, when a #891 revisit trigger fires) rewrites *this module*
and the sites listed in §5; everything in §6 is re-decided then.

**Rejected shapes** (recorded):
- *Newtype wrappers* (`struct Timestamp(chrono::DateTime<Utc>)`) —
  real churn at every method call site, serialization rewrites; that
  is the migration itself, done early. Violates scope law.
- *Full facade of all 12 census types* — speculative; most sites
  (calendar math) may never migrate (§6). A1 policy: no machinery
  before the shape that needs it exists.
- *Seam in a new `taskboard-time` crate* — dependency-boundary churn
  for zero gain; domain is the right home (issue names taskboard-domain;
  every crate already depends on it).

## 5. Decision — in-scope call sites (route through the seam)

Stage-2 migrates exactly these sites, each a one-line-ish mechanical
retarget. Chosen because the evaluation §4c flags them as the tiny,
type-sensitive surfaces where a wrapper costs nothing and buys the
most (parse + duration + the Clock signature), and they are
behavior/type-identical by construction:

| # | Site | Change |
|---|---|---|
| 1 | `crates/taskboard-domain/src/clock.rs` — `Clock::now` signature + `SystemClock` + its test | `DateTime<Utc>` → `time::Timestamp` |
| 2 | `crates/taskboard-sync-nextcloud/src/client.rs` `parse_retry_after` (~:344) + its inline tests (:1770/:1778/:1788) | `chrono::DateTime::parse_from_rfc2822` + `with_timezone(&Utc)` → `time::parse_http_date`; `chrono::Duration::zero()` stays direct (see §6) or routes via a `Delta` alias — stage-2 picks whichever reads better, recorded in the PR body |
| 3 | `crates/taskboard-sync-files-nextcloud/src/etag.rs` `SKEW_BAND_SECS` arithmetic (:158) + inline test (:248) | `chrono::Duration::seconds(SKEW_BAND_SECS)` → `time::delta_seconds(SKEW_BAND_SECS)` |
| 4 | `crates/taskboard-domain/src/days.rs` — signature-level retarget only: `DateTime<Utc>`/`NaiveDate` in public signatures → `time::Timestamp`/`time::LocalDate` aliases | aliases only; no logic touched |

Site 4 is included so the alias types are exercised beyond the seam's
own crate (proving the aliases work across the crate boundary) while
touching zero logic — `days.rs` is pure functions over the same types.
If stage-2 finds it drags imports into more than a handful of
files (any sibling module that *names* the types), it is dropped — the
scope is sites 1–3 plus "as much of 4 as stays mechanical".

All test files anchoring these behaviors (etag inline tests, client
inline tests, days tests) migrate *only* where they name the same
types/arithmetic — no fixture or snapshot input changes.

## 6. Decision — stays direct chrono (recorded, not migrated)

Every other `chrono::` occurrence (~2680 of them), explicitly:

- **Calendar arithmetic** — `recur.rs`/`days.rs` logic/`habit.rs`
  (`Month`/`Weekday`/`Days`, 175+ occurrences): migration-sensitive
  semantics (jiff `Span` split); retarget only when a trigger fires.
- **UI formatting** — ui-slint's ~24 `format!` sites + `Local::now`
  seams (adapter.rs etc.): presentation reads the domain types;
  retargeting them is the migration's tail, not the seam.
- **`Local`/`FixedOffset` seams** — app cli/config/ui, escalation,
  ui-slint (85-class occurrences): tz resolution differs per target
  stack; a wrapper here would be a redesign, not a rename.
- **Serialization boundaries** — serde derives (45 files), sqlite
  codec RFC 3339 columns, sync-nextcloud `ser_opt_datetime`, RFC 3339
  parse/format (135 occurrences): must stay on the *concrete* chrono
  types until the migration swaps codecs wholesale.
- **All `Utc::now()` sites behind `Clock`** already route through the
  injection seam — nothing to do.
- **The std `Duration` sites** (`parse_retry_after`'s `Duration::ZERO`,
  `to_std()`) are not chrono — untouched.

New chrono call sites added after this chunk do **not** retroactively
migrate; the seam grows only when a trigger fires and the migration
dispatch happens.

## 7. Corpus-unchanged verification

- Aliases are type-identical, so the strongest check is compilation +
  the full gate. Stage-2 runs pre-commit hooks (fmt, clippy `-D
  warnings`) per commit and the dispatcher owns the §10 full suite
  (nextest, doc tests, deny, llvm-cov three-command sequence) per the
  long-command discipline.
- **Corpus law:** zero insta snapshot changes, zero sqlite fixture
  changes, zero wire-recording changes. `git status` on snapshot
  dirs after the suite must be clean; any snapshot diff is a bug in
  the chunk (aliases cannot legitimately change output).
- **Scope guard:** a diff-scope review that no file outside §5's site
  list + `time.rs` + `lib.rs` re-export changes. The census count
  *will* drop slightly (sites 2–3 lose direct `chrono::` paths) —
  that is expected, not corpus drift; stage-2 records before/after
  counts in the PR body.
- **Negative test:** `time.rs` carries a unit test asserting
  `delta_seconds(2) == chrono::Duration::seconds(2)` and
  `parse_http_date` round-trips an IMF-fixdate header — proving the
  wrappers are transparent (a seam that transforms is a bug).

## 8. Stage-2 step order

1. `time.rs` + `lib.rs` re-export + negative-test unit tests; commit.
2. Site 1 (clock.rs retarget); commit.
3. Site 2 (client.rs); commit.
4. Site 3 (etag.rs); commit.
5. Site 4 (days.rs, mechanical-only, droppable per §5); commit.
6. Fresh census + snapshot-clean check; record in PR body; open PR.

Each commit stands alone (rebase-merge hygiene, AGENTS §8).
`#![forbid(unsafe_code)]` already inherits; SPDX header per §6.

## 9. Risks

- **Scope creep toward migration** — the trap named in dispatch §14.
  Mitigation: §5/§6 are the contract; the §7 diff-scope review is the
  enforcement.
- **`days.rs` signature ripple** (site 4) — aliases can drag imports
  through sibling modules. Droppable mid-flight per §5 without
  harming the chunk's acceptance.
- **Drift from the 2230/174 figures** — recorded fresh census (§2);
  the count is informational, never an acceptance criterion (no
  quality ceilings).
- **Coordination with D1** — D1 lands clock test doubles; both touch
  `clock.rs`-adjacent code. E2 changes only signature types; rebase
  order: E2 after D1's merge, or rebase per the stacked-PR dance if
  D1 lands first.

## 10. Estimated diff size

~150–220 lines: `time.rs` ≈ 100–120 (docs + aliases + 2 wrappers +
tests), `lib.rs` +1, clock.rs ~10, client.rs ~12, etag.rs ~6,
days.rs ~20–40 (signature lines only). Zero fixture/snapshot lines.

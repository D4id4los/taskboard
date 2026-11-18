# Report — N-F: chrono time-stack evaluation (issue #891)

- **Chunk**: post-round-9 parallel round §N-F; branch
  `docs/chrono-time-stack-evaluation` (worktree
  `../taskboard-WT-docs-chrono-time-stack-evaluation`).
- **Type**: research, docs-only. Zero production code (chunk plan
  §"Hard boundary"). Ends at an operator gate — see the companion
  ruling doc `.artifacts/plans/2026-11-18-chrono-time-stack-ruling.md`.
- **Evidence mandate**: dispatch plan §1 applied in full; every claim
  below carries the command run + observed outcome, or a cited source
  + what it states.

## 1. Method

All commands run from the worktree root at commit `c2bb8e80`
(2026-11-18) unless noted. Commands are re-runnable verbatim; outputs
are excerpted, not paraphrased. Web sources fetched 2026-11-18.

- Advisory re-verification: fetch of
  `https://RustSec.org/advisories/RUSTSEC-2026-0543.html`.
- Upstream record: fetch of
  `https://github.com/chronotope/chrono/issues/1768`.
- Dependency graph: `cargo tree -i chrono`, `cargo tree -i jiff`,
  `cargo tree -i time`, feature inspection via
  `https://docs.rs/crate/sqlx/0.9.0/features`.
- Census: the greps in §4 below (all sub-second; no backgrounding
  needed — `cargo tree` completed in seconds, matching the chunk plan
  §7 expectation).
- Seam checks: targeted greps against the named seams (sync
  HTTP-date, recurrence/schedule math, habit windows, skew constants,
  serialization, UI formatting, clock-frozen fixtures) plus the
  clock-seam audit `.artifacts/reports/2026-11-06-clock-seam-audit-report.md`.

## 2. Advisory record (verbatim)

Source: https://RustSec.org/advisories/RUSTSEC-2026-0543.html
(observed 2026-11-18; matches the stage-1 planning capture).

- **ID**: RUSTSEC-2026-0543; **type**: INFO / Unmaintained.
- **Package**: chrono; **affected**: all versions; **patched**: none.
- **Reported**: 2026-09-14; **issued**: 2026-11-17.
- **Description** (verbatim): "The chrono crate is unmaintained; all
  versions are affected."
- **Alternatives named by the advisory**: jiff and time.
- **Reference**: https://github.com/chronotope/chrono/issues/1768.

What the advisory does **not** claim: no vulnerability is known; this
is an INFO/unmaintained advisory, not a security defect.

### 2a. Upstream issue #1768 (verbatim load-bearing statements)

Source: https://github.com/chronotope/chrono/issues/1768 (observed
2026-11-18). Verbatim quotes:

- Development status (djc): "I'm inclined to wind down maintenance of
  chrono and chrono-tz in the coming months."
- Stated reason: "The API design for these crates is quite dated,
  revising it would be a lot of effort" — plus a comfortable
  alternative now exists to recommend.
- Successor recommendation: [jiff](https://crates.io/crates/jiff) —
  djc calls it "an alternative I feel comfortable recommending."
- Handoff possibility: "If someone wants to take over and they have a
  reasonable amount of reputation/references/standing, I will consider
  it (but strictly on my discretion)."
- chrono-tz tzdb updates: when asked whether chrono-tz would stop
  being updated for timezone database changes, djc confirmed "Yes,
  that's right."
- Forks: none announced; a commenter volunteered, no maintainer
  commitment shown.

## 3. Dependency-graph position

`cargo tree -i chrono` (observed): chrono 0.4.45 enters the graph
through **three** roots, not one:

1. **Direct workspace pin** — `Cargo.toml:92`:
   `chrono = { version = "0.4", features = ["serde", "clock"] }` in
   `[workspace.dependencies]`; all 8 member crates consume it via
   `chrono.workspace = true` (observed in each member's Cargo.toml;
   matching the stage-1 record).
2. **Slint (upstream, not controllable)** — `i-slint-core v1.18.1` →
   `slint v1.18.1` → `taskboard-ui-slint` → `taskboard-app`.
3. **sqlx (feature-controllable)** — `sqlx-core v0.9.0` and
   `sqlx-sqlite v0.9.0`, pulled because the workspace enables the
   `chrono` feature (`Cargo.toml:73`: `sqlx = { version = "0.9",
   default-features = false, features = ["sqlite", "runtime-tokio",
   "chrono", "uuid", "migrate", "macros"] }`).

`cargo tree -i jiff` → error: "package ID specification `jiff` did not
match any packages" — jiff is absent from the graph.
`cargo tree -i time` → nothing printed for the resolved graph (no
workspace path into `time`).

**Open check resolved — sqlx 0.9 jiff/time support** (cited source:
`https://docs.rs/crate/sqlx/0.9.0/features`, observed 2026-11-18):
sqlx 0.9.0 has a `chrono` feature and a `time` feature; it has **no**
jiff feature among its 43 flags. Consequence: a jiff migration has no
sqlx column-mapping support today; a `time` migration does.

### Controllability split (binding for the ruling)

| Path | Removable by workspace migration? |
|---|---|
| Direct workspace pin | yes |
| sqlx `chrono` feature | yes (switch feature / decode by hand) |
| `i-slint-core` chrono | **no** — only if Slint itself migrates |

Even a complete workspace migration leaves chrono 0.4.45 in the
dependency graph through Slint, so the `deny.toml` RUSTSEC-2026-0543
ignore stays in place regardless of what we do to our own code. The
ruling must not overclaim: "remove chrono" is not achievable from this
workspace today.

## 4. Call-site census

Commands (from the chunk plan §2b/2c; observed at `c2bb8e80`):

```sh
grep -rn "chrono::" crates --include="*.rs" | wc -l     # → 2230
grep -rln "chrono::" crates --include="*.rs" | wc -l    # → 174
for c in domain state sync-nextcloud sync-files sync-files-nextcloud \
         storage-sqlite ui-slint app; do
  grep -rn "chrono::" crates/taskboard-$c --include="*.rs" | wc -l
done
```

### 4a. `chrono::` path occurrences per crate

| Crate | Occurrences |
|---|---|
| taskboard-domain | 848 |
| taskboard-ui-slint | 664 |
| taskboard-app | 340 |
| taskboard-state | 103 |
| taskboard-sync-nextcloud | 93 |
| taskboard-storage-sqlite | 81 |
| taskboard-sync-files-nextcloud | 75 |
| taskboard-sync-files | 26 |
| **Total** | **2230 across 174 files** |

Matches the stage-1 record (2230/174 at `cd9d00fa`); the tree did not
shift between the planning and research commits.

### 4b. Type-occurrence census

```sh
grep -rn "\b<TYPE>\b" crates --include="*.rs" | wc -l   # per type below
```

| Type | Occurrences | Note |
|---|---|---|
| `Utc` | 1774 | mostly `DateTime<Utc>` annotations |
| `DateTime` | 1270 | |
| `Local` | 1252 | **caveat**: `\bLocal\b` also matches prose/"local identity" doc comments; real `chrono::Local` uses (incl. `Local::now`/`Local::today`) are far fewer — `grep -rn "chrono::Local\|Local::now\|Local .now"` → 85 occurrences across 11 files (app cli/config/ui, ui-slint adapter/presentation/view_state/models, three test files) |
| `TimeDelta` | 1017 | chrono's duration type (alias of `chrono::Duration`) |
| `NaiveDate` | 519 | |
| `Month` | 276 | calendar arithmetic |
| `NaiveTime` | 295 | |
| `Weekday` | 172 | |
| `TimeZone` | 148 | trait bound/usage |
| `chrono::Duration` | 24 | alongside `TimeDelta` |
| `NaiveDateTime` | 15 | |

Census exclusions noted: `std::time::Duration` matches were filtered
out of the duration rows (grep filter recorded above).

### 4c. Per-seam surface (the migration-cost checklist inputs)

- **sync HTTP-date handling**: the RFC 2822/HTTP-date leg is one call
  site — `crates/taskboard-sync-nextcloud/src/client.rs:344`:
  `let date = chrono::DateTime::parse_from_rfc2822(header).ok()?;`
  (doc comment at `:330` names the IMF-fixdate form). Deck timestamps
  flow through serde with custom `serialize_with = "ser_opt_datetime"`
  (`crates/taskboard-sync-nextcloud/src/model.rs:122,126`) and
  RFC 3339 conversions (`DateTime::parse_from_rfc3339|to_rfc3339` →
  135 occurrences workspace-wide).
- **recurrence / schedule math**: heavy calendar-arithmetic use —
  175 `Weekday|Month::|Days::` occurrences in
  `crates/taskboard-domain/src/{recur,days,habit}.rs` alone; `Month`
  (276) and `Weekday` (172) workspace-wide are almost all here.
- **habit windows**: `crates/taskboard-domain/src/habit.rs` shares the
  recur/days arithmetic above (included in the 175); storage codec
  stores `NaiveDate` windows (`crates/taskboard-storage-sqlite/src/codec.rs:1136`).
- **#133 skew constants**: `SKEW_BAND_SECS: i64 = 2` in
  `crates/taskboard-sync-nextcloud/src/actor.rs:225` and
  `crates/taskboard-sync-files-nextcloud/src/etag.rs:29`; the
  seconds-domain arithmetic is `chrono::Duration::seconds` /
  `TimeDelta::seconds` (`etag.rs:158,248`,
  `tests/actor_wiremock.rs:3066,3081`). Tiny but type-sensitive:
  a replacement duration type must be inter-operable with unix-second
  stamps.
- **serialization boundaries**: 45 files touch both chrono types and
  serde derives (`grep -rln "chrono::" ... | xargs grep -ln
  "Serialize\|Deserialize"` → 45); sqlite storage keeps RFC 3339 text
  columns decoded via chrono (`codec.rs:525-527` and throughout);
  Nextcloud REST payloads carry the custom serde impls above.
- **UI formatting**: ~24 chrono-fed `format(` sites in
  `crates/taskboard-ui-slint/src` (grep `format(` filtered to
  date/time/`%` lines); 8 files format chrono values directly.
- **clock-frozen fixtures**: 202 files reference insta/snapshot
  (`grep -rln "insta\|snapshot"` → 202); the clock-seam audit
  documents the `Local::now().date_naive()` / `Utc::now()` seams that
  fixtures derive expectations from — a migration rewrites the types
  under these pinned fixtures and would require the §6.1–6.2
  discipline re-applied at migration time.
- **tz posture**: no `chrono-tz` dependency and no named-zone use
  (`grep chrono_tz|chrono-tz|Tz::` → 0); the workspace uses `Utc`,
  `chrono::Local` (85 occurrences, 11 files) and `FixedOffset`
  (`crates/taskboard-app/src/escalation.rs:259,487,931-932`,
  `commands/track.rs:1955,1992`, `ui.rs:195`). tzdata-staleness
  exposure is therefore limited to what `Local` resolves for the
  running user's zone — reminders/local-day windows, not named-zone
  scheduling.

## 5. Exposure assessment

1. **What the advisory claims** — see §2: INFO/unmaintained only, no
   known vulnerability, no patched version (nothing to patch). The
   upstream record (§2a) shows the maintainer intends to wind down and
   would consider a qualified handoff; chrono-tz tzdb refreshes
   confirmed stopping.
2. **Y2K-style correctness edges** — the workspace's calendar
   arithmetic is anchored in near-present dates (task schedules,
   habit windows, reminder triggers; test fixtures like
   `from_ymd_opt(2026, 10, 28)` in the `habit.rs` test module,
   `crates/taskboard-domain/src/habit.rs:1118`). No pre-1970 or
   far-future arithmetic surface was found in the census (no
   `from_ymd_opt` with years outside the task-planning range in
   production code). The advisory does not allege a correctness bug;
   the historical edge is not exercised here.
3. **tzdata staleness** — with no `chrono-tz` use, the only tzdata
   dependence is the system zone via `chrono::Local` (85 occurrences,
   11 files, §4c). Staleness risk materializes only if the OS tzdata
   changes the user's zone rules while the binary is old — the same
   exposure any pinned binary has; reminders and local-day windows are
   the affected features. The clock-seam audit's inventory already
   treats these seams as pinned/frozen in tests.
4. **No security patches** — the real residual risk: a future CVE in
   chrono would have no upstream fix. Mitigation posture: our attack
   surface is parsing (RFC 2822 HTTP-date at `client.rs:344`, RFC 3339
   payloads, sqlite text columns) — all fed from the user's own
   Nextcloud server, not hostile third-party input. This is the trigger
   question the ruling records.
5. **Controllability split** — §3 table: Slint keeps chrono in the
   graph no matter what we do; sqlx's path is feature-controllable;
   the direct pin is ours. Any "move" ruling buys a partial exit only,
   while paying the full 2230-site migration cost.

## 6. Alternatives

### A. Stay on chrono (formalized)

- **Fit**: perfect by definition — zero churn.
- **Cost**: none now; unmaintained exposure persists (§5.4).
- **Prep option**: an internal wrapper seam so a later move is cheaper
  — filed as backlog (§9); not required for the recommendation to hold.

### B. jiff (the advisory's and upstream's recommended alternative)

- **Fit evidence** (source: https://docs.rs/jiff/latest/jiff/,
  observed 2026-11-18): serde support via a feature ("all of the
  datetime and span types in Jiff implement serde's Serialize and
  Deserialize traits"); IANA tzdb integration built in; types
  `Timestamp`/`ZonedDateTime`/`civil::Date`/`civil::DateTime`;
  DST-aware calendar arithmetic with `Span`/`SignedDuration`;
  `Timestamp::series`/`ZonedDateTime::series` iterators for recurring
  timestamps.
- **Against the census**: recurrence/schedule math (§4c) maps to
  jiff's civil calendar arithmetic reasonably; but the duration types
  (`TimeDelta` 1017 + `chrono::Duration` 24) map to jiff's
  `Span`/`SignedDuration` split — a semantic rewrite, not a rename
  (`chrono::Duration::seconds(2)` on unix-second stamps becomes
  `SignedDuration` arithmetic).
- **Blocker (cited)**: sqlx 0.9.0 has **no jiff feature**
  (https://docs.rs/crate/sqlx/0.9.0/features) — the sqlite codec's
  chrono column mapping has no drop-in replacement; hand-rolled
  `Decode`/`Encode` across the storage surface would be required.
  jiff's own docs list ecosystem integration via Serde/logging/defmt/
  arbitrary — no database crates.
- **Upstream graph**: does not remove chrono from the graph while
  Slint depends on it (§3).

### C. time

- **Fit evidence** (source: https://docs.rs/time/latest/time/,
  observed 2026-11-18, time 0.3.55): `OffsetDateTime`/`Date`/`Time`
  civil types; `local-offset` feature "enables a number of methods
  that allow obtaining the system's UTC offset"; **no built-in IANA
  time zone database** — named zones are absent from the feature list
  and modules.
- **Against the census**: sqlx 0.9 has a `time` feature (cited §3) —
  the storage seam has drop-in support. But the workspace's
  `Local`-day arithmetic (`Local::now().date_naive()` seams, the
  clock-audit's seam inventory) has no direct equivalent without a
  third-party tzdb crate; and the calendar-arithmetic surface
  (`Month`/`Weekday`/`Days` in recur/days/habit) is thinner in `time`
  than in chrono or jiff.
- **Upstream graph**: same Slint limitation as B.

### Others

`hifitime` etc. not assessed further: A is viable (no forcing
condition met, §5.4), so the plan's "extend only if evidence warrants"
clause does not trigger.

## 7. Recommendation

Exactly one — **A: stay on chrono**, formalized (keep the `deny.toml`
RUSTSEC-2026-0543 ignore, record dated revisit triggers, no migration
dispatch). Full statement, rejected alternatives, consequences, and
the operator gate: see the ruling doc
`.artifacts/plans/2026-11-18-chrono-time-stack-ruling.md`.

Rationale in one paragraph: the advisory alleges no defect (§2); the
workspace's actual exposure is narrow (no chrono-tz, parse inputs come
from the user's own server, §5); a migration pays the full 2230-site /
174-file cost yet cannot remove chrono from the dependency graph while
Slint depends on it (§3) — so the "remove the unmaintained crate"
narrative, the strongest argument for moving, is not purchasable at
any price we control; and the two candidate stacks each carry a
concrete blocker today (jiff: no sqlx 0.9 support, cited; time: no
IANA tzdb, cited) while the ecosystem posture that would unblock a
move (sqlx gaining jiff support, Slint migrating) is exactly the
revisit trigger the ruling records.

## 8. Gate handoff

This report + the ruling doc are the stage-2 deliverable. The ruling
ends **PROPOSED** at an operator gate: the operator accepts the ruling
(or overrides it), and only then is issue #891 closable. Pre-commit
hooks are this docs-only chunk's gate; the dispatcher owns the full
§10 suite per the dispatch plan.

## 9. Backlog issues raised during the chunk

- #987 — internal chrono wrapper seam (prep-for-migration option from
  alternative A, out of scope for this chunk).
- #988 — monitor sqlx/Slint upstream chrono-dependency posture as the
  revisit-trigger tracking home (the ruling §3 trigger-tracking home).

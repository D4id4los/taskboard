# [Plan] Daily-Driver Chunk 2 — Day-Boundary Preference (`[preferences] day_start_time`) + `days.rs::day_window`

- **Date**: 2026-10-21
- **Kind**: stage-1 TDD implementation plan (dispatch plan §3, prompt 2.1;
  MD-6 path). Chunk 2 of the daily-driver implementation round; branch
  `feat/domain-day-boundary`; **closes #345** (PR body carries it —
  stage 3's job, MD-4).
- **Origin (normative, in MD-7 rank)**: implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-11, §3 chunk 2, §4 (gates), §5 (sequencing: chunk 2 is S and
  independent, landable any time before chunk 10), §6 (bookkeeping);
  design doc `.artifacts/plans/2026-10-18-field-family-design-plan.md`
  §3.6 D-F6 + §5's D-F6 row and the 2026-10-18 decision-session record
  (both sub-points ACCEPTED: the config section, the fixed-offset
  computation; ruled addition: GUI settings editor → backlog **#351**,
  unscheduled); dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1 (MD-1…MD-10) and §3 (chunk 2 acceptance + traps).
- **Status**: precondition **MD-9 verified 2026-10-21** — chunk 1 is
  merged on `origin/main` @ `ae804de`: the remote branch
  `feat/domain-task-field-family` was deleted post-merge, so the
  catalogue check is the operative one and passes
  (`git cat-file -e origin/main:crates/taskboard-storage-sqlite/migrations/0007_task_field_family.sql`;
  chunk 1's own stage-1 plan
  `.artifacts/plans/2026-10-21-daily-driver-task-field-family-plan.md`
  is on `origin/main` too, commit `7401101`).
  `scripts/worktree-doctor.sh` green (118G free ≥ 25G floor, exit 0).
  Worktree `../taskboard-WT-feat-domain-day-boundary` created on branch
  `feat/domain-day-boundary` off **`origin/main`** (local `main` was 1
  commit stale at `dc0e9ca`; the dispatch plan mandates "the
  then-current `main`" — same recorded rationale as chunk 1's plan).

---

## 0. Vocabulary relocations (MD-8 — symbols are the stable keys)

None. Chunk 2 introduces only new symbols; grep-verified absent from the
base (`crates/` + `docs/`): `day_start_time`, `day_window`,
`PreferencesSection`, `InvalidDayStartTime`, `days.rs` — zero hits. The
design doc's anchors re-locate to today's names unchanged:

| D-F6 anchor | Today's symbol |
|---|---|
| config.rs's typed-validation precedent (:45–53, :97+) | the `[reminders]` raw-String + parse-in-`validate` pattern (`config.rs:257–296`) and its `InvalidDuration` rejection (`:566–572`, checked at `:876–880`) |
| the `Clock` seam's purity rule (clock.rs :11) | `crates/taskboard-domain/src/clock.rs:4–9` — *"Nothing in this crate calls `Utc::now()` directly"*; time is data |
| the `UserPreferences` deferral | `docs/architecture.org:208` (the only mention; the Phase-1 deferral stands — D-F6's home ruling) |
| `AppConfig::write_path` (the #351 write machinery) | `config.rs:716` (read-only context; the GUI settings editor is backlog #351, out of wave) |

One more base fact: the domain crate has **zero** `chrono::Local` uses
today (grep-verified); `days.rs` must not become the first.

## 1. Verified baseline (re-located on `origin/main` @ `ae804de`)

| Seam | Symbol (file) | Lines | Notes |
|---|---|---|---|
| Config root | `AppConfig` (app `config.rs`) | 62–90 | eight `#[serde(default)]` sections; `preferences` slots in as the ninth (D1) |
| Section template | `RemindersSection` (config.rs) | 257–296 | raw `String` field + `default_*` fn + parse-in-`validate` + typed error; always serializes — **the** template for `day_start_time` |
| Always-serialize scalar precedent | `UiSection.scale` (config.rs) | 298–326 | a defaulted scalar with no `skip_serializing_if`; the precedent D1 cites |
| Skip-machinery contrast | `WebSection`/`KioskSection` (config.rs) | 374–484 | `is_default`/`skip_serializing_if` exist for *opt-in* surfaces and optional pins, not for scalar knobs |
| Error enum | `ConfigError` (config.rs) | 545–641 | fine-grained typed variants; `#[allow(clippy::result_large_err)]` sits on the enum, not per-variant |
| Kind discriminant | `ConfigError::kind` (config.rs) | 643–668 | stable per-variant vocabulary for `config print` + the `--json` envelope; **new variant ⇒ new arm** |
| Validation ladder | `AppConfig::validate` (config.rs) | 777–911 | the `[reminders]` checks at `:876–885`; the new check inserts immediately after `:885` (D3) |
| Constants block | `DEFAULT_*` consts (config.rs) | 35–58 | `DEFAULT_DAY_START_TIME` joins them (D1) |
| Purity rule | module doc (domain `clock.rs`) | 4–9 | "Merge functions never read the clock: time is always *data* … Nothing in this crate calls `Utc::now()` directly" — the offset inherits the rule (DDF-11 trap: no `chrono::Local`) |
| Module list | domain `lib.rs` | 20–26 (map), 30–44 (mods), 54–97 (re-exports) | `pub mod days;` after `command`; `pub use days::day_window;` with the function re-exports |
| Timestamp strategy | `timestamp_strategy` (domain `test_support/mod.rs`) | 116–118 | whole seconds 1971‑01‑01…2100‑01‑01 — reused by the proptests (D5); `adversarial_timestamp_strategy` :177 exists but adds edge coverage this chunk's fixed-offset math does not consume |
| Test-support availability | `lib.rs` | 48–49 | `#[cfg(any(test, feature = "test-support"))]` — in-crate `#[cfg(test)]` proptests may `use crate::test_support::…` (the sibling modules' precedent) |
| chrono idiom | house duration construction | — | `chrono::TimeDelta::try_hours(n).unwrap()` for infallibly-small n (test-side, e.g. `reminders.rs:1293`); production `days.rs` avoids even that (D4) |
| No-stray-table note | `AppConfig` has no `deny_unknown_fields` | 62–63 | a stray top-level `[preferences]` table is silently dropped *today*; this chunk activates it (risk R2) |

## 2. Decisions (every open question settled; D-numbers are this plan's)

- **D1 — the section's exact shape** (`config.rs`, template =
  `RemindersSection`): a new `PreferencesSection` —
  `#[derive(Debug, Clone, Serialize, Deserialize)]` +
  `#[serde(deny_unknown_fields)]` (the secret-free-by-construction
  invariant: every table rejects unknown keys); one field
  `pub day_start_time: String` with
  `#[serde(default = "default_day_start_time")]`; manual
  `impl Default` mirroring the field default; `fn
  default_day_start_time() -> String { DEFAULT_DAY_START_TIME.to_owned() }`;
  `pub const DEFAULT_DAY_START_TIME: &str = "00:00";` in the constants
  block (with the D-F6 TOML example's night-owl value `"05:00"` as the
  doc-comment example). `AppConfig` grows
  `#[serde(default)] pub preferences: PreferencesSection` appended
  **last** (after `web` — the field order fixes serialization order
  only; the section set has no semantics). Raw `String`, **not**
  `chrono::NaiveTime` + serde: an unparseable value must surface as the
  typed validation error, not a generic `ConfigError::Decode`, and the
  file's own form must survive for `config print` to echo — exactly the
  `reminders.default_before` rationale the section doc states
  (`config.rs:257–260`). **Always serializes** (no skip machinery):
  the `ui.scale` precedent — the skip machinery exists for opt-in
  surfaces (`web`) and optional pins (`kiosk.fixed_board`), not for
  scalar knobs; accepted consequence: whole-config serialization
  (`config print`, login-write overlays aside) gains one
  `[preferences]` line for every install (risk R1). **No accessor this
  chunk**: the parse-once bootstrap seam and the `chrono::Local`
  offset resolution are consumer wiring (chunk 10's, DDF-11's
  contract); a caller-free accessor is dead code.
- **D2 — the validation error type**: a new fine-grained variant
  `ConfigError::InvalidDayStartTime { field: &'static str }`
  (`#[error("day start time invalid")]`, payload = the dotted key,
  per the `InvalidScale`/`InvalidWebConfig` shape) +
  `kind() => "invalid-day-start-time"`. Not a reuse of
  `InvalidDuration` (a time-of-day is not a duration; one variant per
  semantic domain is the house pattern) and not `Decode` (the
  `RemindersSection` doc pins the direction: *"a bad value must
  surface as the typed kind, not a generic decode failure"*). Within
  the ruling's "typed validation — else the config error path"; not a
  deviation.
- **D3 — grammar + validation arm**: the parse is
  `self.preferences.day_start_time.parse::<chrono::NaiveTime>()`
  (chrono's documented `FromStr` grammar: `HH:MM`, `HH:MM:SS`,
  `HH:MM:SS.fff`; rejects out-of-range and garbage). No trim, no
  bespoke parser: `parse_before` is hand-rolled because its
  suffix-composition grammar is bespoke, but time-of-day is a stock
  chrono format — a hand-rolled parser would be speculative
  generality, and the strict-`%H:%M` alternative would buy nothing
  (the ruling demands "must parse as a time-of-day", which `FromStr`
  is). No shared `parse_day_start_time` helper yet: `parse_before` is
  a shared fn because a second consumer exists (the CLI `--before`
  flag); this field's second consumer (chunk 10's bootstrap parse)
  can extract one when it lands. The check inserts in
  `AppConfig::validate` immediately after the `[reminders]` block
  (after `:885`, grouping the two string-parsed fields; ladder order
  only decides which typed error wins when several fields are invalid
  — nothing depends on cross-section order today):
  `if self.preferences.day_start_time.parse::<chrono::NaiveTime>().is_err() { return Err(ConfigError::InvalidDayStartTime { field: "preferences.day_start_time" }); }`
  Env override `TASKBOARD_PREFERENCES__DAY_START_TIME="05:00"` works
  through the standard chain — stated in the section doc comment.
- **D4 — `days.rs` + `day_window` (verbatim from D-F6; no deviation)**:
  new domain module `crates/taskboard-domain/src/days.rs`
  (SPDX `MIT OR Apache-2.0`; the crate's `#![forbid(unsafe_code)]`
  covers it), wired into `lib.rs`: the module-map doc line gains
  "day boundaries (`days`)", `pub mod days;` alphabetically after
  `command`, and `pub use days::day_window;`. Signature exactly as
  ruled:

  ```rust
  #[must_use]
  pub fn day_window(
      now: DateTime<Utc>,
      day_start_time: chrono::NaiveTime,
      offset: chrono::FixedOffset,
  ) -> (DateTime<Utc>, DateTime<Utc>)
  ```

  Algorithm (the ruled four steps, made total-by-construction):
  1. `let local_date = now.with_timezone(&offset).date_naive();`
     (offset-local date);
  2. candidate = `local_date.and_time(day_start_time)` back-converted;
  3. `if candidate > now` → step the **date** back one day
     (`local_date - Days::new(1)`) and re-convert; equality
     (`candidate == now`) takes the candidate — the window opens
     *at* `now` (this is what makes the proptests' `start ≤ now` hold);
  4. `end` = `start_date + Days::new(1)` at the same time-of-day —
     identical to `start + 24h` because the offset is fixed (the
     "+24h/day in fixed offset" definition, encoded structurally).

  Back-conversion is **naive arithmetic, not `LocalResult`**: a private
  `fn offset_local_to_utc(local: NaiveDateTime, offset: FixedOffset)
  -> DateTime<Utc> { DateTime::from_naive_utc_and_offset(local -
  offset.fix(), Utc) }` — `utc = local − offset` is exact for every
  representable reading because a fixed offset has no transitions (the
  D-F6 "gap-free by construction" point), so no `.single()`/`.unwrap()`
  panic path exists anywhere in the function body. The doc comment
  carries the **consumers contract verbatim** (DDF-11): bootstrap
  parses `[preferences]` once + resolves the device offset via
  `chrono::Local` at derivation time; consumers receive
  `(day_start_time, offset)` explicitly and call `day_window`; nothing
  reads figment mid-flight, nothing mutates `AppState` for it — plus
  the **DST ruling**: fixed-offset arithmetic, the ≤1h twice-yearly
  wall-clock-label wobble accepted, no tz-database lookups in pure
  functions. `#345`'s contract sentence ("the most recent instant
  whose local wall-clock time-of-day equals `day_start_time`") is the
  first doc line.
- **D5 — proptest strategies** (local to `days.rs`'s `#[cfg(test)]`
  module; **no `test_support` growth** — nothing else consumes
  `NaiveTime`/`FixedOffset` strategies yet, and growing the shared
  corpus is not this chunk's need): `now` reuses
  `crate::test_support::timestamp_strategy()` (1971–2100, the shared
  corpus's bound — risk R4); `day_start_time` =
  `(0_u32..=86_399).prop_map(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0).unwrap())`
  (whole seconds; fractional nanos add no behavioral surface);
  `offset` = `(-86_399_i32..=86_399).prop_map(|s| FixedOffset::east_opt(s).unwrap())`
  (chrono's full legal domain, well past any real device's ±14h).
  Test-side boundary construction in the flip proptest may use
  `offset.from_local_datetime(&ndt).single().unwrap()` — tests
  construct inputs; the production path stays `LocalResult`-free.
- **D6 — the proptest catalogue** (the ruled set, one behavior per
  test):
  1. `window_contains_now` — `start ≤ now < end` (ruled clause 1);
  2. `window_is_exactly_24_hours` — `end − start == 24h` (the
     length corollary of clause 3, split out per one-behavior-one-test);
  3. `stepping_now_across_the_boundary_flips_the_window` — for an
     independently constructed boundary instant `B` (today's
     offset-local date at `day_start_time`): `day_window(B).0 == B`,
     `day_window(B − 1s).0 == B − 24h`, `day_window(B + 1s).0 == B`
     (ruled clause 2, sampled at both flanks of the flip — the trap
     list names a missing flip property a finding);
  4. `one_day_later_the_window_advances_one_day` —
     `day_window(now + 24h)` = window shifted by exactly +24h in both
     components (ruled clause 3).
- **D7 — deterministic unit examples** (before the proptests, TDD):
  (a) midnight start at `+02:00`, afternoon instant → the window opens
  at that day's local midnight (an earlier UTC instant than `now`'s
  local date suggests — the offset arithmetic pinned numerically);
  (b) night-owl `"05:00"`, local 03:00 → the window opened *yesterday*
  05:00 local (the step-back rule pinned); (c) `now` exactly at the
  candidate → the new window opens at `now` (the equality branch);
  (d) `"00:00"` at `+00:00` → `[local midnight, +24h)` in UTC. All
  constructed from literal `Utc.timestamp_opt`/`FixedOffset::east_opt`
  values with hand-computed expected pairs — no reimplementation of
  the conversion math.
- **D8 — config test set** (`config.rs` tests module, all typed-outcome,
  zero text assertions):
  - defaults round-trip: extend the existing
    `serde_round_trip_preserves_defaults` with the one
    `preferences.day_start_time == DEFAULT_DAY_START_TIME` assert
    (its established per-section assert list) — plus the dedicated
    behaviors below, not folded into it;
  - `[preferences] day_start_time = "05:30"` (and `"23:59:59"`,
    `"05:00"`) decodes through the figment chain and `validate()`s;
  - unparseable values (`"25:00"`, `"5:60"`, `""`, `"5m"`,
    `"tomorrow"`, `"024:00"`, `"5 m"`) →
    `Err(InvalidDayStartTime { field: "preferences.day_start_time" })`
    (loop style, per `ui_scale_rejects_out_of_range_and_non_finite`);
    **decode always succeeds** (raw String) — the typed failure fires
    in `validate()`, which is precisely the ruling's "never silently
    defaulted";
  - unknown key in `[preferences]` → `ConfigError::Decode`
    (`deny_unknown_fields`; mirrors the reminders/ui unknown-key
    tests);
  - `InvalidDayStartTime { .. }.kind() == "invalid-day-start-time"`
    (the stable-discriminant test, per
    `ui_scale_error_kind_is_the_stable_discriminant`).
- **D9 — no insta pass, no fixtures**: nothing this chunk widens is
  snapshot-tested (`AppState`, `PersistedState`, `SyncReport` fixtures
  untouched; config is not snapshotted). The §4 snapshot rule's
  "widening chunk owns its one deliberate pass" is vacuously satisfied;
  "no insta deltas anywhere" is an acceptance criterion. CHANGELOG
  gets one Unreleased line (stage 2 bookkeeping).
- **MD-7 check**: D1–D9 refine the ACCEPTED ruling's prose with
  recorded reasons; the ruled artifacts (signature, algorithm,
  consumers contract, proptest clauses, no-`UserPreferences`) are
  copied verbatim. **No ACCEPTED-ruling deviation — no user round-trip
  required.**

## 3. Exact shapes (illustrative, not a diff)

```rust
// config.rs — the section (template: RemindersSection)

/// `[preferences] day_start_time`: midnight rollover, the D-F6 default.
pub const DEFAULT_DAY_START_TIME: &str = "00:00";

/// `[preferences]`: device-local user preferences. Never synced (no
/// document key, no lane) until the D5 `settings` section exists; each
/// device keeps its own. Overrides via
/// `TASKBOARD_PREFERENCES__DAY_START_TIME`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreferencesSection {
    /// Local wall-clock time-of-day at which "today" begins
    /// (night-owl rollover: `"05:00"`). Validated typed — an
    /// unparseable value is [`ConfigError::InvalidDayStartTime`],
    /// never a silent default; the raw form stays stored so
    /// `config print` echoes it.
    #[serde(default = "default_day_start_time")]
    pub day_start_time: String,
}

impl Default for PreferencesSection {
    fn default() -> Self {
        Self { day_start_time: default_day_start_time() }
    }
}

fn default_day_start_time() -> String {
    DEFAULT_DAY_START_TIME.to_owned()
}

// ConfigError — new variant (fine-grained, per-house) + kind arm:
/// `[preferences] day_start_time` does not parse as a time-of-day.
#[error("day start time invalid")]
InvalidDayStartTime {
    /// The dotted config key that is invalid.
    field: &'static str,
},
// kind(): Self::InvalidDayStartTime { .. } => "invalid-day-start-time",

// validate() — one arm, after the [reminders] block:
if self
    .preferences
    .day_start_time
    .parse::<chrono::NaiveTime>()
    .is_err()
{
    return Err(ConfigError::InvalidDayStartTime {
        field: "preferences.day_start_time",
    });
}
```

```rust
// days.rs — the function (the doc comment is the consumers contract)

/// The 24h window `[start, start + 24h)` containing `now`, where
/// `start` is the most recent instant whose wall-clock time-of-day in
/// the fixed UTC offset `offset` equals `day_start_time` (#345's
/// contract; D-F6).
///
/// Pure, total, deterministic: time *and* offset are data — the
/// crate's clock rule (`clock.rs`: nothing reads the environment)
/// applies to the offset too, so `chrono::Local` and tz-database
/// lookups never appear here. The consumer passes its *current* fixed
/// offset; on a DST transition day the boundary's wall-clock label may
/// wobble ≤1h from the tz-database answer — accepted by ruling (the
/// instants stay unambiguous; a fixed offset has no transitions, so
/// the instant math is gap-free by construction).
///
/// # Consumers contract (D-F6)
///
/// App bootstrap parses `[preferences] day_start_time` once (typed
/// config validation) and resolves the device offset via
/// `chrono::Local` at derivation time; consumers (UI today view, CLI
/// markers, later engine features via bootstrap injection) receive
/// `(day_start_time, offset)` explicitly and call this function.
/// Nothing reads figment mid-flight; nothing mutates [`AppState`] for
/// it.
///
/// Total for every representable `now` at least one day inside
/// chrono's date range (the `Days` arithmetic saturates only at
/// `NaiveDate`'s extremes).
#[must_use]
pub fn day_window(
    now: DateTime<Utc>,
    day_start_time: chrono::NaiveTime,
    offset: chrono::FixedOffset,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let local_date = now.with_timezone(&offset).date_naive();
    let candidate = offset_local_to_utc(local_date.and_time(day_start_time), offset);
    let start_date = if candidate > now {
        local_date - Days::new(1)
    } else {
        local_date
    };
    let start = offset_local_to_utc(start_date.and_time(day_start_time), offset);
    let end = offset_local_to_utc((start_date + Days::new(1)).and_time(day_start_time), offset);
    (start, end)
}

/// Back-converts an offset-local wall-clock reading to its instant:
/// `utc = local − offset`, exact for every representable reading
/// because a fixed offset has no transitions (no `LocalResult`
/// ambiguity can arise).
fn offset_local_to_utc(local: NaiveDateTime, offset: FixedOffset) -> DateTime<Utc> {
    DateTime::from_naive_utc_and_offset(local - offset.fix(), Utc)
}
```

```rust
// lib.rs — three lines:
//   module map doc: "… conflict-policy algebra (merge) …, reminders
//   (reminders), day boundaries (days)"
pub mod days;          // after `pub mod command;`
pub use days::day_window;
```

## 4. TDD ordering (test first at every step; commits standalone)

1. **`feat(domain): day_window, the fixed-offset day boundary`** —
   write `days.rs` tests first (D7's unit examples, then D5/D6's
   proptests), watch them fail against a stub, implement D4, hook the
   three `lib.rs` lines. One standalone commit (correct build, tests
   green).
2. **`feat(app): [preferences] day_start_time with typed validation`**
   — config tests first (D8), then the section (D1), the error variant
   + kind arm (D2), the validate arm (D3). One standalone commit.
3. **`docs(changelog): the day-boundary preference`** — the Unreleased
   line (chunk 1's `9dd2a6a` precedent of a separate changelog commit).
4. Full AGENTS §10 gate suite (`fmt --check`, `clippy
   --workspace --all-targets -D warnings`, `nextest`, doc tests,
   `cargo deny`, the CI-identical three-command llvm-cov sequence +
   quarantine + trailing `rm`). Stage 2 runs it; stage 3 re-runs it.

Stage-3 grep gates (from the dispatch trap list): `grep -rn
"chrono::Local\|Utc::now\|LocalResult" crates/taskboard-domain/src/days.rs`
must be empty; any diff outside `days.rs`/`lib.rs`/`config.rs`/
`CHANGELOG.md` is scope creep.

## 5. Typed-outcome test matrix (behaviour → test → layer)

| Behaviour | Test | Layer |
|---|---|---|
| Midnight-start window in a `+02:00` offset | `day_window` unit (D7a) | domain `days.rs` |
| Night-owl start rolls to yesterday (step-back) | unit (D7b) | domain |
| Equality at the boundary opens the new window | unit (D7c) | domain |
| `start ≤ now < end` | proptest (D6.1) | domain |
| Window length exactly 24h | proptest (D6.2) | domain |
| Flip at the boundary (both flanks + at it) | proptest (D6.3) | domain |
| +24h shift advances the window +24h | proptest (D6.4) | domain |
| Default `"00:00"` round-trips | config defaults round-trip (D8) | app `config.rs` |
| Valid values decode + validate | config toml round-trip (D8) | app |
| Unparseable values are the typed error | config validate loop (D8) | app |
| Unknown `[preferences]` key is a decode error | config decode test (D8) | app |
| Stable error discriminant | `kind()` test (D8) | app |
| No other crate's behavior changes | gates green; zero insta deltas (D9) | workspace |

## 6. Risks & mitigations

- **R1 — serialization delta**: every always-serialized config gains a
  `[preferences]` line on `config print` (login-write overlays only
  touch ceremony keys — unaffected). Accepted per D1 (the `ui.scale`
  precedent); mitigation: the app test suite confirms no
  whole-config byte-identity pin exists. If one surfaces, **stop and
  re-decide** (the fallback is a `WebSection`-style `is_default` skip —
  a recorded-decision change, not a ruling deviation).
- **R2 — stray-table activation**: `AppConfig` itself has no
  `deny_unknown_fields`, so a `[preferences]` table written before
  this chunk was silently dropped; afterwards it decodes — a
  well-formed `day_start_time` starts being honored (the feature
  working as intended), a garbage key now fails loudly at boot (the
  house secret-free invariant). Worth one CHANGELOG-clause thought;
  no code mitigation.
- **R3 — chrono `FromStr` leniency**: the accepted grammar is
  "whatever chrono's documented `NaiveTime` grammar accepts" (it may
  admit forms like single-digit hours); the ruling only demands
  parse-or-typed-error, and the test set pins representative accepted
  and rejected forms without asserting the full grammar.
- **R4 — date extremes**: `Days` arithmetic and +24h shifts are
  unrepresentable within one day of chrono's date limits; the shared
  strategy bounds `now` to 1971–2100 and the doc comment scopes the
  totality claim. No production input can reach the extreme (and
  bounds on garbage input are permitted — this is a representability
  limit, not a quality ceiling).
- **R5 — environment reads**: the offset is a parameter; the stage-3
  grep gate (§4) plus the proptest's total determinism make any
  `chrono::Local`/tz lookup an immediate finding.
- **R6 — scope creep**: consumer wiring, the `UserPreferences`
  entity, sync/settings serialization, an accessor, bootstrap offset
  resolution — all non-goals (§8); the trap list makes each a stage-3
  finding.

## 7. Estimated diff size

| File | Delta |
|---|---|
| `crates/taskboard-domain/src/days.rs` (new) | ≈ +230 (impl + doc ≈ 60, unit tests ≈ 70, proptests ≈ 90, module doc ≈ 10) |
| `crates/taskboard-domain/src/lib.rs` | +3 |
| `crates/taskboard-app/src/config.rs` | ≈ +95 (section + const + default fn ≈ 30, error variant + kind arm ≈ 8, validate arm ≈ 6, tests ≈ 50) |
| `CHANGELOG.md` | +1–2 |

Total ≈ **330 added lines, 0 deletions, 4 files** — S-sized, as
dispatched. No migrations, no `.sqlx` regeneration, no dependency
changes (chrono is already a dependency of both crates with the needed
features: `clock` is irrelevant to the domain math; no serde need
beyond what's enabled).

## 8. Out of scope (chunk boundaries, restated)

No consumer wiring (chunk 10 owns the today view's `day_window` call
and the bootstrap `(NaiveTime, FixedOffset)` derivation; CLI list
markers have no surface today), no `UserPreferences` entity (the
Phase-1 deferral stands; D5's `settings` section is the later
vehicle), no accessor/bootstrap offset resolution, no sync, no
`AppState`/storage/engine growth, no GUI settings editor (backlog
**#351**, unscheduled), no tz-database-aware boundaries (backlog
candidate in the design doc's §7 — a consumer must observe the wobble
first).

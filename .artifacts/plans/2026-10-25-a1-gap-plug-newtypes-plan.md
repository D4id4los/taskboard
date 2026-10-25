# [Plan] A1 Gap-Plug Newtypes — `ConnectorId` + `UiScale` (backlog-triage chunk 4)

- **Date**: 2026-10-25
- **Kind**: chunk plan — stage-1 planning record for chunk 4 of the
  backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §5).
  Refactor: `taskboard-domain` (mint + predicate), `taskboard-app`
  (config mint sites, config→UI hand-over), `taskboard-ui-slint`
  (`UiScale`, mount seam). No behaviour change: the accept/reject
  semantics of config validation are pinned unchanged.
- **Branch / worktree**: `refactor/a1-gap-plug-newtypes` /
  `../taskboard-WT-refactor-a1-gap-plug-newtypes` (doctor exit 0,
  91 G free, floor 25 G).
- **Base**: `f7b1ac8` = fetched `origin/main` tip (the r2 chunks 5–8 —
  settings-form `ff7e8f4`, connection-management `8a9316b`,
  visual-identity `54c5564`/`894c1d6`, drag-to-move `f7b1ac8` — are all
  in; the gate held). The branch is cut off the fetched tip; the local
  `main` ref was at the same commit this session.
- **GATE, recorded**: verified before planning — all four r2 chunks
  merged into `origin/main` (rebase-merges; no `feat/ui-r2-*` remote
  branches remain, `origin/feat/ui-r2-drag-to-move` was pruned on
  fetch).
- **Normative origins**: issue #425 (the exact sites and success
  criteria), `docs/coding_guidelines.org` §2.1 (the A1 house pattern:
  hand-rolled private-field newtype, fallible mint at the trust
  boundary, documented leniency, no validate-and-forget, no derive
  crates), suitability report §3 A1 + §5 Pilot 1
  (`.artifacts/reports/2026-10-24-type-driven-formal-methods-suitability-report.md`),
  maintainer rulings §7 item 1 (API churn pre-approved), dispatch plan
  §5 (covers + acceptance + the code-quality mandate).
- **Line numbers below are plan-day references** (base `f7b1ac8`),
  never load-bearing.

## 0. What the harness reading established

- **The gap is real and exactly as #425 states.** `ConnectorId(String)`
  (`crates/taskboard-domain/src/ids.rs:100-131`) mints through total
  `From<&str>`/`From<String>`; the kebab check lives only in
  `AppConfig::validate` (`crates/taskboard-app/src/config.rs:872`, via
  the predicate `is_kebab_name` at `:1122-1130`), so downstream the
  type carries no proof. Its own doc comment says so: "Kebab-case and
  uniqueness are validated at config load; this constructor cannot
  fail, so `From` stays total" — the comment must flip from describing
  the gap to documenting the lenient path.
- **`ui.scale` is checked once and forgotten.** The finite + range
  check at `config.rs:978-980` (against `UI_SCALE_MIN`/`UI_SCALE_MAX`,
  `:51-53`) hands a bare `f64` to `UiSection.scale` (`:369`), which
  `resolve_ui_settings` copies verbatim into
  `taskboard_ui_slint::UiSettings.scale` (`crates/taskboard-app/src/ui.rs:206`),
  consumed at the mount seam `window.global::<Theme>().set_base_scale(settings.scale as f32)`
  (`crates/taskboard-ui-slint/src/lib.rs:196-199`).
- **There is a second scale boundary the issue predates**: the r2 D29
  slider makes scale mutable at runtime. The UI adapter pre-checks the
  draft (`crates/taskboard-ui-slint/src/adapter.rs:1887-1898`, literal
  `0.5..=3.0`), sends `UiRequest::SetScale`, and the app-side handler
  `set_scale` re-validates typed — `NotFinite`/`OutOfRange` split,
  `crates/taskboard-app/src/ui_requests.rs:649-664` — then persists via
  `connections::overlay_set_ui_scale` (`crates/taskboard-app/src/connections.rs:292-303`,
  writes the raw `f64` into TOML). `UiScale` must be minted at *both*
  app-side boundaries; the adapter's check is a presentation mirror
  only.
- **The attach path reads raw config**: `attach_connection` re-reads
  the config *file* via `read_raw_config` (`ui_requests.rs:443-454`) —
  a hand-edited non-kebab name minted today through the total `From`
  flows into `attach_connector`/`BindBoard` unchecked. The mint is
  load-bearing there, not ceremony.
- **The save path already re-validates**: `validated_draft`
  (`ui_requests.rs:713-732`) shapes the UI profile into the connector
  list and runs `AppConfig::validate()` before writing the file. It is
  the natural mint site for the save choreography.
- **Deserialization leniency is deliberate and already named**: the
  storage decoder maps a persisted row tag with
  `row.connector.map_or_else(|| default_connector.clone(), ConnectorId::from)`
  (`crates/taskboard-storage-sqlite/src/repo.rs:233-235`), commented
  per D-3: "a non-NULL tag naming an unconfigured connector decodes
  as-is … the owning actor simply never finds the board". This is the
  path the total `From` exists for.
- **The `expect` precedent for validated-config accessors is
  established in this very file**: `AppConfig::server_url()`/`username()`
  (`config.rs:1095-1114`) use `.expect("… validated")` with `#
  Panics — never on a validated config` docs. The hand-over mint rides
  the same rule.
- **The scale escape hatch does not touch the mint**: `observe_scale_escape_hatch`
  (`ui.rs:300-315`) only *logs* `SLINT_SCALE_FACTOR`; Slint's winit
  backend applies it at window creation, outside `UiSettings`. No
  out-of-range value can enter `UiScale` through it.
- **`cargo-mutants` is installed** (`~/.cargo/bin/cargo-mutants`); no
  repo-level `mutants.toml` exists — the evidence run scopes itself
  with `--file` (D5).
- **Both target crates already depend on `thiserror`** (workspace
  pin): `taskboard-domain/Cargo.toml:16`, `taskboard-ui-slint/Cargo.toml:17`.
  No new dependencies of any kind.

## 1. Decisions

### D1 — the `ConnectorId` fallible mint (domain)

In `crates/taskboard-domain/src/ids.rs`, next to the type:

```rust
/// A `[[sync.connectors]]` name that failed the kebab predicate;
/// carries the offending name verbatim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("connector name invalid")]
pub struct InvalidConnectorName {
    /// The name that did not pass.
    pub name: String,
}

impl ConnectorId {
    /// The kebab predicate (`[a-z0-9-]`, non-empty, no leading or
    /// trailing `-`, no `--`) — moved verbatim from `AppConfig`'s
    /// `is_kebab_name` so there is exactly one copy.
    #[must_use]
    pub fn is_kebab_name(name: &str) -> bool { /* unchanged body */ }

    /// The mint: the only construction path for a `ConnectorId` that
    /// enters from user-controlled text (config load, settings forms,
    /// raw config re-reads).
    pub fn from_kebab(name: &str) -> Result<Self, InvalidConnectorName>;
}
```

- The predicate body is byte-identical to `config.rs:1123-1129` — the
  boundary tests pin it, so "verbatim move" is checkable by the accept/
  reject suite staying green.
- `AppConfig`'s `pub(crate) fn is_kebab_name` (`config.rs:1122`) is
  deleted; `validate` (`:872`) and the predicate table test
  (`:2025-2028`) call `ConnectorId::is_kebab_name`. Its doc comment
  ("the domain newtype itself stays unfiltered — config validates,
  domain trusts") is now false and dies with the move.
- **The total `From<&str>`/`From<String>` impls stay**, per #425, with
  the type-level doc rewritten to carry the A1 leniency ruling:
  deserialization-only (serde payloads of already-validated persisted
  rows — the `repo.rs` D-3 decode — plus test fixtures), *not* an
  input-adjacent mint. The per-impl `From` docs shrink to a pointer at
  the type doc. This is the "documented leniency at the type" the
  guidelines §2.1 requires — it lives on `ConnectorId` itself, not in
  a distant comment.
- **Where the uniqueness half lives: `AppConfig::validate` stays its
  only home.** Uniqueness is a set-level property over the entries
  array; a per-value constructor cannot see the set. The dispatch row's
  "fallible mint (kebab + uniqueness)" is satisfied exactly as the
  prompt's own wording anticipates: the mint owns the kebab half, the
  uniqueness check stays in `validate`'s `seen` loop (`config.rs:870-882`)
  and rides along automatically in every path that re-validates a draft
  config (the settings `validated_draft`, `attach_from_persisted`'s
  `AppConfig::load`). No duplicate check is duplicated.
- `InvalidConnectorName` mirrors the payload `ConfigError::InvalidConnectorName`
  already carries (`name: String`), so the app-side mapping is
  mechanical.
- The serde derives, `Display`, `as_str`, and the ordering/hash derives
  are untouched.

### D2 — construction-site census: who mints, who decodes

Every production construction site of `ConnectorId` (test/bench/
example sites keep the lenient `From` untouched — they are fixtures):

| Site | Today | Becomes |
|---|---|---|
| `connectors.rs:174` (`configured_connectors`, array arm) | `From(entry.name)` | **Mint** — `from_kebab(&entry.name).map_err(\|_\| ConfigError::InvalidConnectorName { name: entry.name.clone() })?`; `configured_connectors` returns `Result<Vec<ConnectorSpec>, ConfigError>` (D2a) |
| `connectors.rs:183` (legacy synthesis) | `From(DEFAULT_CONNECTOR_NAME)` | **Mint + documented expect** — `from_kebab(DEFAULT_CONNECTOR_NAME).expect("DEFAULT_CONNECTOR_NAME is kebab")`; the const is `"nextcloud"` (`connectors.rs:32`), proven at the definition |
| `bootstrap.rs:830-833` (standalone default tag) | `From(const)` | Same const-mint treatment with the same documented expect (the surrounding comment already explains the inert-tag semantics) |
| `ui_requests.rs:362` (settings save) | `From(profile.name)` | **Mint at the boundary** — `validated_draft` returns `(ConnectorCfg, ConnectorId)`: it already ran `candidate.validate()` at `:730`, so it parses once and hands the proof forward; `:362`'s re-mint is deleted |
| `ui_requests.rs:508, :530` (attach) | two `From(connector_name)` | **Mint from the raw-config entry** — one `from_kebab(&entry.name).map_err(\|_\| map_config_error(&ConfigError::InvalidConnectorName { name: entry.name.clone() }))?` after the `:447-454` lookup; `:530` reuses `id.clone()` instead of constructing twice |
| `ui_requests.rs:578` (detach) | `From(connector_name)` | **Mint, fail `Gone`** — the name cannot match any minted binding or configured actor unless it is kebab; `from_kebab(...).map_err(\|_\| SettingsError::Gone)?` preserves the observable outcome (nothing by that name exists → the actor stop is a no-op either way) |
| `repo.rs:233-235` (storage decode) | `From(row.connector)` | **Stays `From`** — the deserialization path; the D-3 comment at `:230-232` stays; the type doc now names this as the lenient scope |
| serde `Deserialize` on `ConnectorId` | derive | **Stays** — covered by the type doc |
| `adapter.rs:1500` (UI view-model row re-mint) | `From(row.name)` | **Stays `From`** — a trusted round-trip of a name the app minted moments earlier (`ConnectorInfo.name` is `spec.name.to_string()` at hand-over); covered by the type doc's leniency scope. Rejected alternative: carrying `ConnectorId` in `ConnectorInfo` — ripples the reply payload and the journeys tests for zero proof gain |
| `test_support/`, `#[cfg(test)]` modules, benches, examples, doc examples | `From` | **Stay** — test fixtures on the lenient path |

**D2a — `configured_connectors` becomes fallible.**
`pub(crate) fn configured_connectors(config: &AppConfig) -> Result<Vec<ConnectorSpec>, ConfigError>`.
This is the compiler-enforced heart of the chunk: every caller must
decide. Caller inventory (production, 16 sites — the Pilot 1
"measurement bonus" data point; each adaptation is 1–3 lines):

- `session_ceremonies.rs:196` `default_connector_id` → propagate (`?`
  into a `Result` return; its caller is the connect ceremony, already
  `Result`-shaped).
- `session_ceremonies.rs:206` `default_spec` → `?` (returns
  `Result<_, CommandError>`; `From<ConfigError> for CommandError`
  exists).
- `session_ceremonies.rs:261` `attach_from_persisted` → the config was
  `AppConfig::load`ed at `:248` (validated), so the mint cannot fail;
  map the error to the typed wire outcome `W::NotConfigured` beside the
  existing `tracing::warn` pattern.
- `ui.rs:258` `resolve_connector_infos` → `Result<Vec<ConnectorInfo>,
  ConfigError>`; hand-over callers (`run_desktop` `ui.rs:366`, kiosk
  `main.rs:497-502`) are entry-path `color_eyre` contexts → `?`.
- `ui_requests.rs:674-682` `current_connectors` → the existing
  warn-and-empty fallback for an unreadable raw config extends to the
  mint failure (same failure class: "the fresh connector list is
  unreadable"; display-only surface, loud in the log).
- `ipc/server.rs:413` → typed wire outcome, as `attach_from_persisted`.
- `repl.rs:809` → the function returns `u8`; a mint failure is a config
  failure — report via the existing `report_to`/`CommandError` pattern
  and return the error exit code, *before* `bootstrap` (which would
  fail with the same config error anyway; failing at the read is
  strictly earlier and typed).
- `repl.rs:815` → after a successful `bootstrap`, `app.config` is
  validated; map errors to the same report-and-exit path (mechanically
  unreachable, kept typed).
- `commands/mod.rs:54` `require_connector`, `:72` `resolve_connector`,
  `:919`, `:1672`; `commands/status.rs:207`, `:254`; `commands/tasks.rs:49`
  → `?` (all `Result<_, CommandError>` handlers).
- `bootstrap.rs:757` → propagate into `BootstrapError` (add a
  `Config(#[from] ConfigError)` variant if none composes — check at
  implementation; the error enum already fronts config failures at
  boot).

**The rule for every adaptation: propagate typed, never swallow.** No
`.ok()`, no `unwrap_or_default` on the mint error. Stage 3 greps for
exactly that.

### D3 — the `UiScale` shape and its home

New module `crates/taskboard-ui-slint/src/scale.rs` (module precedent:
`keymap` — typed values built app-side, living in the UI crate because
the UI crate is their only consumer and `taskboard-app` already
depends on `taskboard-ui-slint`; the domain crate stays
presentation-free per AGENTS §2):

```rust
/// The `[ui] scale` quantity: finite and within `MIN..=MAX`, minted
/// app-side at the config boundary and at the settings `SetScale`
/// request; carried in `UiSettings` to the mount seam. Not serde —
/// `UiSection.scale` stays the raw `f64` carrier (see D6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiScale(f64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UiScaleError {
    /// NaN or ±inf — fails closed before any range comparison.
    #[error("ui scale not finite")]
    NonFinite,
    /// Outside the inclusive `MIN..=MAX` window.
    #[error("ui scale out of range")]
    OutOfRange,
}

impl UiScale {
    /// `[ui] scale`'s enforced floor, inclusive (was app-side
    /// `UI_SCALE_MIN`).
    pub const MIN: f64 = 0.5;
    /// See [`Self::MIN`] — the enforced ceiling, inclusive (was
    /// `UI_SCALE_MAX`).
    pub const MAX: f64 = 3.0;
    /// The mint: finite first (NaN compares false both ways), then
    /// the inclusive range.
    pub fn new(raw: f64) -> Result<Self, UiScaleError>;
    /// The quantity as `f64` (TOML persistence, layout arithmetic).
    #[must_use]
    pub const fn get(self) -> f64;
    /// Slint `float` properties are f32; the validated `0.5–3.0`
    /// window converts with precision far below any visible scale
    /// step. The one `as f32` cast in the crate.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn get_f32(self) -> f32;
}
impl Default for UiScale { /* 1.0, the designed baseline */ }
```

Re-exported at the lib root (`pub use scale::{UiScale, UiScaleError};`).

- `UiSettings.scale: f64` → `UiScale` (`lib.rs:62`); `UiSettings`'s
  field doc updates from "validated app-side to the 0.5–3.0 range" to
  "minted app-side (`UiScale`) — validated by construction".
- `UiSettings::default` uses `UiScale::default()` (`lib.rs:101`).
- The mount seam (`lib.rs:196-199`) becomes
  `set_base_scale(settings.scale.get_f32())`; the existing precision
  comment and the `#[allow(clippy::cast_possible_truncation)]` move
  into `get_f32` — one copy, at the conversion.
- **Const ownership moves to the type**: app's `pub const
  UI_SCALE_MIN`/`UI_SCALE_MAX` (`config.rs:51-53`) become delegating
  aliases of `UiScale::MIN`/`MAX` (values identical by a pinned unit
  test), so every existing user — the boundary test
  (`config.rs:1377-1384`), `config print` docs, the error doc — sees
  the same numbers from the same source. `DEFAULT_UI_SCALE`
  (`config.rs:47`) stays `1.0` with a one-line unit test pinning
  `UiScale::default().get() == DEFAULT_UI_SCALE`.
- The UI adapter's presentation mirror (`adapter.rs:1893`) swaps its
  literals for `UiScale::MIN..=UiScale::MAX` — same crate now; a
  magic-number drift dies for free. The mirror remains
  presentation-only (fast user feedback before the round-trip); the
  authoritative check is the app-side mint.

### D4 — the `UiScale` journey (both boundaries)

1. **Config load** (`AppConfig::validate`, `config.rs:978-980`):
   `UiSection.scale` stays a raw `f64` (D6 — why), and the check
   becomes `UiScale::new(self.ui.scale).is_err()` →
   `ConfigError::InvalidScale { field: "ui.scale" }`. Identical
   predicate, identical typed rejection — pure delegation.
2. **Hand-over** (`resolve_ui_settings`, `ui.rs:206`):
   `scale: UiScale::new(ui.scale).expect("ui.scale validated by
   AppConfig::validate — call only on a validated config")`, with a
   `# Panics` section mirroring the `server_url()`/`username()`
   precedent (`config.rs:1095-1114`). Both call sites
   (`run_desktop` `ui.rs:366`, the kiosk hand-over `main.rs:497`) run
   on `app.config` — always `AppConfig::load`ed. The compiler now
   forces every future `UiSettings` builder through the mint or the
   default; that is the validate-and-forget plug.
3. **Runtime `SetScale`** (`set_scale`, `ui_requests.rs:649-664`):
   the two hand-rolled checks become
   `UiScale::new(scale).map_err(...)` matched 1:1 onto the existing
   typed `SettingsError::Invalid { field: Field::Scale, kind:
   NotFinite | OutOfRange }` (`NonFinite`→`NotFinite`,
   `OutOfRange`→`OutOfRange`). Accept/reject parity is structural:
   same split, same variants, same field tag.
4. **Persistence**: `connections::overlay_set_ui_scale` takes
   `UiScale` and writes `scale.get()` (`connections.rs:292-303`) — a
   persisted `[ui] scale` value is range-proof by construction. Its
   tests (`connections.rs:494-506`) construct the input via
   `UiScale::new(2.0).unwrap()` (tests may unwrap); the round-trip
   assertion against `loaded.ui.scale` (raw `f64`) keeps its value.
5. **Display-only surfaces** (`BoardInfo`, `ConnectionRow`, slider
   arithmetic reading `Theme.base_scale` back as `f64`,
   `adapter.rs:476-492`) stay bare floats — presentation arithmetic on
   live widget state, downstream of every boundary; not minted, per
   the "no presentation logic in app, no config knowledge in ui"
   boundary.

### D5 — mutant-kill test catalogue (both validators)

The Pilot 1 success criterion is "cargo mutants finds no surviving
mutant in the validators". After this chunk each validator predicate
exists in exactly one place — `ConnectorId::is_kebab_name`/
`from_kebab` (`ids.rs`) and `UiScale::new` (`scale.rs`) — so the
mutant surface is two functions.

**`ConnectorId` (in `ids.rs` `#[cfg(test)]`):**

1. *Accept table* (deterministic — kills boundary mutants without
   proptest luck): accepts `"a"`, `"0"`, `"nextcloud"`, `"nc-2"`,
   `"a-1-b"`, `"abc123"`; rejects `""`, `"-"`, `"--"`, `"-a"`, `"a-"`,
   `"a--b"`, `"A"`, `"aB"`, `"a_b"`, `"a b"`, `"ä"`, `"a.b"`, `"/"`.
   Assert via `matches!` on the `Result`, never text.
2. *Predicate≡mint equivalence proptest* (256 cases): for arbitrary
   strings, `is_kebab_name(s) == from_kebab(s).is_ok()` — any mutation
   of either half flips some string's classification and dies.
3. *Identity*: `from_kebab(s)` → `as_str() == s`, `Display == s`,
   `to_string` round-trip.
4. *Error payload*: the offending name is carried verbatim
   (`err.name == input`).
5. *Leniency is real but scoped*: the total `From` still constructs a
   non-kebab value (`From("NOT_KEBAB")`) — a deliberate pin that the
   lenient path exists for decode/fixtures; named
   `..._documents_the_lenient_decode_path` so it reads as policy, not
   endorsement.
6. Existing `bindings_roundtrip_through_serde` (`ids.rs:214-226`)
   stays untouched.

**`UiScale` (in `scale.rs` `#[cfg(test)]`):**

1. *Boundary accepts*: `new(MIN)`, `new(MAX)`, `new(1.0)` → `Ok`;
   inclusive-window mutants (`<` vs `<=`) die here.
2. *Reject table with variant split*: `0.49` and `3.01` →
   `OutOfRange`; `NaN`, `+inf`, `-inf` → `NonFinite` — `matches!` on
   the exact variant, mirroring `config.rs:1387-1399`'s corpus. The
   finite-first ordering is pinned: NaN rejects as `NonFinite`, not
   `OutOfRange`.
3. *Identity*: `get()` returns the exact input; `get_f32()` equals the
   input converted; `Default == UiScale::new(1.0)`; `Copy` semantics.
4. *Const agreement*: `UiScale::MIN == 0.5`, `MAX == 3.0` (the
   delegated app consts are then equal by construction).

**Evidence procedure (stage 2)**:

```sh
cargo mutants -f crates/taskboard-domain/src/ids.rs \
              -f crates/taskboard-ui-slint/src/scale.rs
```

(cargo-mutants is installed; no repo `mutants.toml` — scope via
`--file`.) Record the run's zero-survivors summary in the stage-2
report. `config.rs`'s scale check is post-refactor a one-line
delegation (no predicate left to mutate); `validate`'s kebab call
likewise delegates.

### D6 — accept/reject parity: what "unchanged" means

**The key parity decision: `UiSection.scale` stays `f64` and
`ConnectorCfg.name` stays `String`.** Moving the newtypes into the
serde structs would move the rejection from `validate` (typed
`InvalidScale`/`InvalidConnectorName`, stable discriminants
`"invalid-scale"`/`"invalid-name"`) into figment decode
(`ConfigError::Decode`) — a different error surface, breaking the
reject-path contract and the `config print` `invalid_kind` vocabulary.
The serde fields are the raw carriers; the mints sit post-decode.
This is the "documented deliberate shape" the guidelines require —
recorded here and in the field docs.

Existing tests that must pass **unchanged** (assertion content
byte-stable; only mechanical call adaptation — `.unwrap()` on a
now-`Result` helper, `.get()` on a now-`UiScale` — is permitted, and
if any assertion itself must change, that is a parity violation to
escalate, not to paper over):

- `config.rs`: the whole `validate` accept/reject suite —
  `validate_accepts_a_complete_config` (`:1283`), the reject family
  (`:1288-:1362`, `:1513`, `:1567`), `ui_scale_defaults_to_one_and_round_trips`
  (`:1369`), `ui_scale_accepts_the_range_boundaries` (`:1377`),
  `ui_scale_rejects_out_of_range_and_non_finite` (`:1387`),
  `ui_scale_error_kind_is_the_stable_discriminant` (`:1402`), the kebab
  predicate table (`:2020-2028`), the duplicate/invalid connector name
  paths, the TOML merge/roundtrip tests (`:1436`, `:2379`).
- `connectors.rs`: the `configured_connectors` shape tests
  (`:207-269`) — mechanical `.unwrap()` on the now-`Result` return
  only.
- `ui.rs:612-689` hand-over tests and `:621` (`settings.scale == 1.25`
  → `settings.scale.get() == 1.25` — same value assertion).
- `connections.rs` overlay tests (`:340-506`).
- `ids.rs` serde proptests; the storage roundtrip/contract suites
  (decode path untouched).
- insta snapshots: `ConnectorId`'s `Debug`/`Display` are untouched and
  `AppState` snapshots do not embed `UiSettings` — nil diff expected;
  any diff is mechanical regen, reviewed before accepting.

### D7 — commit and PR shape

One branch, one PR (dispatch default). Commit series (each standalone
under rebase-merge):

1. `feat(domain): the ConnectorId kebab mint` — ids.rs mint +
   predicate + tests (+ the app predicate deleted, validate delegating —
   or split as commit 2 if the diff reads better; decide by review).
2. `feat(app): configured_connectors mints the connector names` — the
   Result ripple, all 16 callers.
3. `feat(ui-slint): the UiScale quantity through the hand-over and mount`
   — scale.rs, UiSettings, mount seam, adapter mirror.
4. `feat(app): UiScale mints at the set-scale boundary` — validate
   delegation, resolve_ui_settings, set_scale, overlay_set_ui_scale.
5. `test(domain,ui-slint): the validator mutant catalogue` — if not
   already riding commits 1/3.
6. `docs(changelog): the A1 gap-plug newtypes round` — per the
   dispatch prompt 4.2: an Unreleased/Changed-internal entry only if
   the CHANGELOG's convention carries internal refactors, else none.

## 2. Stage-2 work order

1. D1 first (domain mint + predicate move + catalogue tests) — the
   compiler then walks the rest.
2. D2a ripple (connectors.rs + the 16 callers), minting per the census.
3. D3/D4 (UiScale module, UiSettings, mount, both boundaries).
4. D5 catalogue tests + the cargo-mutants evidence run.
5. Full AGENTS §10 gate suite; CHANGELOG decision per D7.
6. Record the call-site count the compiler forced (~16) in the
   stage-2 report — the Pilot 1 "types as agent scaffolding" data
   point the report asks for.

## 3. Acceptance → evidence map

| #425 criterion | Evidence |
|---|---|
| No construction path outside deserialization bypasses the mint | The D2 census (every production site classified); stage 3's grep over `ConnectorId::from(`/`From` impls finds only the census's "stays `From`" rows + fixtures. Honest scope note: the total `From` stays (per #425), so this criterion is audit-enforced for the residual lenient sites and compiler-enforced for the config/settings paths (`configured_connectors`/`validated_draft`/`set_scale` are `Result`-shaped — the caller cannot forget) |
| `cargo mutants`: no surviving mutant in the validators | The D5 evidence run output (two files, zero survivors) |
| Existing config accept/reject semantics unchanged | The D6 suite list green with unchanged assertions; `ConfigError` variants and `kind()` discriminants untouched |
| Documented leniency at the deserialization-only total `From` | The rewritten type doc on `ConnectorId` (D1), naming the serde/storage D-3 scope and the adapter round-trip |

Dispatch §5 review traps, pre-answered: no derive crates (none added —
both crates' `Cargo.toml` untouched); crate boundaries (the predicate
and mint in domain, `UiScale` in ui-slint with the keymap precedent,
no config knowledge in the UI crate, no presentation logic in app);
minimal hand-over (the mount seam is one accessor call).

## 4. Risks

1. **The `configured_connectors` Result ripple is the widest surface**
   (16 sites, 9 files). All adaptations are mechanical, but three
   sites sit in non-`Result` contexts (`repl.rs` `u8`,
   `attach_from_persisted`'s wire outcome, `current_connectors`'s
   display list) where a lazy hand would swallow. Mitigation: each has
   a named adaptation in D2a; stage 3 greps for `.ok()`/`unwrap_or`
   swallows of the mint error as THE finding class on the app side.
2. **Behaviour delta on raw-config paths is possible and intended**:
   `attach_connection` on a hand-edited non-kebab config file fails
   typed (`Gone`/`NotKebab`) where it previously minted garbage that
   matched nothing. Observable outcome unchanged in the matched-name
   case; the garbage-name case now refuses earlier and louder. This is
   the plug working, not a parity break — called out here so stage 3
   does not flag it as drift.
3. **`bootstrap.rs:757`'s error mapping** needs a `BootstrapError`
   composition check (D2a); if no variant composes, adding one is a
   mechanical, internally-visible change.
4. **From-stays-total means a future bypass is compile-possible** —
   the issue chose this explicitly (serde + fixtures need the lenient
   path). The type doc + census + stage-3 grep are the enforcement;
   the stronger `from_persisted(String)` rename was rejected as churn
   on ~60 fixture sites for naming only.
5. **`UiScale` home (ui-slint, not domain)** is a judgment call —
   recorded with its precedent (keymap) and reasoning (D3); if the
   maintainer prefers domain, the move is mechanical post-merge.
6. **Mutant-run runtime**: `cargo mutants` rebuilds per mutant; two
   `--file` scopes keep it bounded, but the first run may take
   minutes — expected, not a hang.

## 5. Estimated diff size

Medium, churn-heavy but shallow: ~15 files production-side.
`ids.rs` +~90 (mint, error, tests); `scale.rs` new ~120 incl. tests;
`connectors.rs` + the 16 callers ~60 lines of mechanical adaptation
across `session_ceremonies.rs`, `ui.rs`, `ui_requests.rs`,
`connections.rs`, `bootstrap.rs`, `repl.rs`, `ipc/server.rs`,
`commands/*`; `config.rs` ~30 (predicate deletion, delegations,
check-to-mint); `lib.rs`/`adapter.rs` ~20. **~450–550 lines including
tests**, no new dependencies, no serde/format changes, insta snapshots
nil.

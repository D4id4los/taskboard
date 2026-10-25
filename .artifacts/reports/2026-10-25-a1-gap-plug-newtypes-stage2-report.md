# [Report] A1 Gap-Plug Newtypes — Stage 2 (backlog-triage chunk 4, #425)

- **Date**: 2026-10-25
- **Branch / worktree**: `refactor/a1-gap-plug-newtypes` /
  `../taskboard-WT-refactor-a1-gap-plug-newtypes` (pre-existing, on the
  stage-1 plan commit `2ae01af`; doctor not needed — nothing recreated).
- **Commits** (each standalone under rebase-merge; **no push, no PR** —
  hand-off at gate-green per the dispatch):
  - `60f9127` feat(domain): the ConnectorId kebab mint
  - `7541f20` feat(ui-slint): the UiScale quantity through the hand-over and mount
  - `11c81a2` feat(app): the mints at the config boundary (A1 gap-plug)
  - `1bd97a5` docs(changelog): the A1 gap-plug newtypes round

## 1. What was implemented (per the stage-1 plan D1–D6)

- **D1** — `ConnectorId::from_kebab` + `ConnectorId::is_kebab_name`
  (predicate moved byte-identical from `AppConfig`'s private filter,
  which is deleted); `InvalidConnectorName { name }` thiserror payload;
  the total `From<&str>`/`From<String>` stay with the leniency ruling
  in the **type doc** (serde/storage D-3 decode, adapter trusted
  re-mint, fixtures — never input-adjacent).
- **D2/D2a** — `configured_connectors` → `Result<Vec<ConnectorSpec>,
  ConfigError>`; every production caller adapted (census below). Mint
  sites: `validated_draft` returns `(ConnectorCfg, ConnectorId)` so the
  save choreography parses once; the attach choreography mints from the
  raw-config entry; detach mints and fails `SettingsError::Gone`;
  storage decode `repo.rs`, the adapter re-mint, and serde derives stay
  `From` per the census. `DEFAULT_CONNECTOR_NAME`'s legacy-synthesis
  arm mints with the documented `expect` + a pinned unit test.
- **D3/D4** — new `taskboard_ui_slint::scale` module: hand-rolled
  private-field `UiScale(f64)`, `UiScaleError::{NonFinite, OutOfRange}`
  (finite-first ordering), `MIN`/`MAX` consts, `get`, `get_f32` (the
  crate's one scale cast, comment moved in), `Default = 1.0`.
  `UiSettings.scale: UiScale`; mount seam reads `get_f32`; the
  adapter's presentation mirror swaps its literals for the mint.
  App-side: `validate`'s check delegates to `UiScale::new` (identical
  predicate, identical `ConfigError::InvalidScale`);
  `resolve_ui_settings` mints with the `# Panics` doc (the
  `server_url()` precedent); `set_scale`'s two hand-rolled checks
  become one mint matched 1:1 onto `NotFinite`/`OutOfRange`;
  `overlay_set_ui_scale` takes `UiScale` and writes `get()`;
  `UI_SCALE_MIN`/`MAX` become delegating aliases. `UiSection.scale`
  stays raw `f64` (D6 parity decision — no error-surface shift into
  figment decode).
- **D5** — mutant catalogue: kebab accept/reject tables, the
  predicate≡mint equivalence proptest (256 cases), identity +
  error-payload pins, the deliberate-leniency pin
  (`..._documents_the_lenient_decode_path`); `UiScale` boundary/reject
  variant-split/identity/const tables.

## 2. The compiler-forced call-site census (Pilot 1 data point)

`configured_connectors`' `Result` forced a decision at **16 production
call sites across 9 files** (`connectors.rs`, `session_ceremonies.rs`
×3, `ui.rs` ×2 + `resolve_connector_infos`' 3 callers, `ui_requests.rs`,
`bootstrap.rs`, `repl.rs` ×2, `ipc/server.rs`, `commands/mod.rs` ×4,
`commands/status.rs` ×2, `commands/tasks.rs`). Adaptations:
typed `?` via `From<ConfigError> for CommandError` where the handler is
`Result`-shaped; report-and-exit in the REPL's `u8` context (failing at
the read, before bootstrap); `W::NotConfigured` beside the existing
warn in `attach_from_persisted`; the warn-and-empty fallback extended
to the mint failure in the display-only `current_connectors`. No
`.ok()`/`unwrap_or_default` swallow of a mint error anywhere (the
`current_connectors` fallback is the plan's own named display-surface
adaptation, loud in the log).

## 3. Evidence

### 3.1 Mutants (the #425 success criterion)

```
cargo mutants --test-tool nextest \
  -f crates/taskboard-domain/src/ids.rs \
  -f crates/taskboard-ui-slint/src/scale.rs
```

- **25 mutants: 22 caught, 3 unviable, 0 survived, 0 timed out**
  (`mutants.out/missed.txt` is empty; baseline: 105 s build + 50 s test,
  auto-timeout 255 s). The 3 unviable are `replace … with
  Ok(Default::default())`/`Default::default()` substitutes over types
  with no `Default` impl — they fail to compile and are not escapes.

### 3.2 Config accept/reject parity

The whole pinned suite (D6 list: the `validate` accept/reject family,
both scale boundary tests, the non-finite reject family, the stable
discriminant test, the kebab predicate table, duplicate/invalid-name
paths, TOML merge/roundtrips, `connections.rs` overlay tests, the
storage suites, ids serde proptests) runs **green with unchanged
assertions** — the only permitted mechanical adaptations applied:
`.unwrap()` on the now-`Result` `configured_connectors` in
`connectors.rs` tests and `.get()` on the now-`UiScale` in the
hand-over test. One added pin test:
`ui_scale_default_agrees_with_the_quantity_default`. No insta snapshot
diffs.

### 3.3 Full gate (AGENTS §10) — all green

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo nextest run --workspace` | 1860 passed, 40 skipped |
| `cargo test --doc --workspace` | 0 failures |
| `cargo deny check` | advisories/bans/licenses/sources ok |
| `cargo llvm-cov nextest … --no-report` + quarantine + lcov | 2130 profraws all valid; report written; `target/llvm-cov-target` removed |

No new dependencies; no `Cargo.toml` touched; no derive crates.

## 4. Deviations from the plan (mechanism, not spirit)

1. **Validated-config contexts use the house `expect` precedent instead
   of `?`.** The plan's D2a guessed `commands/mod.rs:919/:1672` and
   `commands/status.rs:207/:254` were `Result` handlers; they are `u8`/
   `Self` contexts holding a provably validated `AppConfig`
   (`AppConfig::load` validates). Those use
   `.expect("… validated …")` with the `server_url()`-style `# Panics`
   docs (three new ones: `StatusView::server`, `StatusView::in_session`,
   `commands::dispatch`). Raw-config contexts (`repl.rs`,
   `attach_from_persisted`, `current_connectors`) propagate/report typed
   exactly as planned. `resolve_connector_infos` did become `Result`
   (plan-faithful) — its two entry-path callers `?` into `color_eyre`.
2. **Clippy `result_large_err` grants** on the two new
   `Result<_, ConfigError>`-returning functions (`configured_connectors`,
   `resolve_connector_infos`) — mirrors the enum's existing grants
   (`ConfigError` carries the figment source); pre-commit `-D warnings`
   forced it.
3. **Commit shape**: plan D7's commits 2 and 4 merged into one
   `feat(app)` commit — the ripple and the scale-boundary work share
   files (`ui_requests.rs`, `config.rs`), and per-file staging would
   have produced non-building intermediates. Series: domain → ui-slint
   → app → changelog, each standalone-building.
4. **Mutant runner**: plain `cargo test` aborted on the unmutated
   baseline — `golden_visuals::support::driver::tests::
   driver_reads_element_geometry_and_queries_all_instances` failed under
   parallel `cargo test` (passes under nextest). Known flake, **issue
   #431** — observation recorded as a comment on it
   (2026-10-25, OnceLock collision signature). Evidence run used
   `--test-tool nextest`.
5. **Stage-1 leftover in this tree**: the stage-1 plan commit `2ae01af`
   sits under the four work commits — intentional, it is the chunk's
   planning record on the chunk branch.

## 5. Stage-3 pointers

- THE grep: `ConnectorId::from(` should only remain at the census's
  "stays `From`" rows (`repo.rs` decode, `adapter.rs:~1500` re-mint,
  `settings_form`/`presentation`/`sync-nextcloud`-actor fixture paths,
  tests/benches/examples) — each covered by the type doc's leniency
  scope. Production input-adjacent paths all mint.
- Behaviour delta by design (plan risk 2): a hand-edited non-kebab name
  in the config file now refuses typed at the attach/save boundary
  instead of minting a value that matched nothing.
- `mutants.out/` is gitignored; the evidence summary above is the
  record.

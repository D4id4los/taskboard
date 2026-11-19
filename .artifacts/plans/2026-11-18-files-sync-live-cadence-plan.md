# Chunk Plan — files-sync live cadence (#673)

Chunk P8 of the tier-2 round (dispatch plan
`.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §9–§10; recommendation
report `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
§Tier-2, normative). Branch `feat/files-sync-live-cadence`, base `origin/main`
@ `890fc507`.

Closes #673 at merge: the files-sync poll actor's cadence follows live
settings (the doorbell re-arm), replacing the recorded restart-scoped
limitation (round-5 live-settings plan D-5.5 item 7).

## 0. P7 sequencing verdict

**P7 has landed; P8 re-anchors on it.** Verified on the fetched
`origin/main`: `565df7d9 feat(sync-files-nextcloud): the transport DELETE
verb, typed and idempotent` and `890fc507 feat(sync-files-nextcloud): the
owner's post-snapshot GC pass and the device-scoped unbind retirement`
(P7's PR #1044, merged 2026-11-19). Same crate
(`taskboard-sync-files-nextcloud`), but P7's diff lives in
`transport.rs`/`board_cycle.rs`/`board_gc.rs`; P8's diff lives in
`actor.rs` (+ the domain snapshot + the app mint) — file-disjoint on the
landed main. No sequencing constraint remains. The dispatch plan's
"re-anchor on P7's landed shape" is satisfied by basing on `890fc507`.

## 1. The seams (as landed)

- **Files-sync actor** `crates/taskboard-sync-files-nextcloud/src/actor.rs`:
  `FilesSyncConfig` carries `poll_interval` / `backoff_initial` /
  `backoff_max` minted **once at boot** by
  `AppConfig::files_sync_config()` (`crates/taskboard-app/src/config.rs`
  ~1898: `poll_secs.unwrap_or(sync.poll_interval_secs)` — the
  `[files_sync] poll_secs` override over the `[sync]` triple fallback).
  The loop arms `deadline = reschedule(&st, &cfg)` after each cycle;
  `reschedule` picks `cfg.poll_interval` on streak 0, `poll_backoff(...)`
  over the cfg backoff knobs otherwise.
- **The ignore arm** (`actor.rs:174–193`): `SystemEvent::SettingsChanged`
  rides the multi-variant ignore arm with the documented no-op comment
  naming the D-5.5-item-7 limitation and #673.
- **The connector pattern to mirror**
  (`crates/taskboard-sync-nextcloud/src/actor.rs:312–318, 393–410`): the
  `SettingsChanged` arm applies the **min rule** —
  `deadline = deadline.min(reschedule(&st, &state.read_settings()))` —
  and `reschedule` takes the per-cycle `SettingsSnapshot` read
  (`poll_interval_secs` on streak 0, the backoff knobs otherwise).
- **The reader**: `SyncStateReader::read_settings`
  (`crates/taskboard-domain/src/persistence.rs:84`) — sync-free,
  error-free, "a stopped engine still has a last published value". The
  files-sync actor **already receives** this handle (`state_reader`,
  `actor.rs:82`, wired through `SyncContext`); the settings reader facet
  needs no new spawn parameter.
- **The broadcast path**: the engine's `UpdateSettings` apply
  (`crates/taskboard-state/src/actor.rs` ~864) sets the snapshot,
  publishes, then `system_out.send(SystemEvent::SettingsChanged)`. The
  app write lane (`run_settings_write`, `ui_requests.rs` ~645) re-decodes
  and **validates** the effective config before minting the snapshot —
  the live path's validation guard.
- **The snapshot mint**: `AppConfig::settings_snapshot()`
  (`config.rs:1780`) maps the `[sync]` triple into
  `SettingsSnapshot::poll_interval_secs/backoff_*_secs`; it does **not**
  carry `files_sync.poll_secs`. The engine is seeded with this snapshot
  at boot (`bootstrap.rs:1364`) and replaces it wholesale on every live
  write (a value, not a patch — `messages.rs:655`).

## 2. Decisions (all open questions resolved)

### D1 — Reader facet: the existing `state_reader` handle, no new seam

The actor already holds `Arc<dyn SyncStateReader>`; `read_settings()` is
the settings reader facet the connector uses. No new spawn parameter, no
new trait, no connector-family membership — **DR-8's boundary stays**:
the actor grows a *read*, not an engine facet beyond the ADR-0005 reader
it already consumes. The `SyncContext`-resident copy is untouched (the
cycle's reads are unrelated); the loop reads through the loop-local
`state_reader` binding like the connector does.

### D2 — The snapshot carries the files-sync override: new domain field

`SettingsSnapshot` gains

```rust
/// `[files_sync] poll_secs` — this actor's override over
/// `[sync] poll_interval_secs` (the boot mint's unwrap_or rule,
/// carried live so boot and live reads share one cadence rule).
pub files_sync_poll_secs: Option<TickSecs>,
```

(`#[serde(default)]` already on the type; `Default::default()` gets
`None` — the test-policy snapshot then means "no override".)

Rationale: without it, a live re-read would silently drop the
`[files_sync] poll_secs` override and fall back to `[sync]`, changing
boot semantics — the dispatch normative pins "the boot fallback (the
`[sync]` triple) keeps its semantics". Carrying the override in the
snapshot makes **one cadence rule** shared by boot and live:

```
effective_poll_interval(snapshot) =
    snapshot.files_sync_poll_secs.unwrap_or(snapshot.poll_interval_secs)
```

The mint (`AppConfig::settings_snapshot()`) maps
`self.files_sync.poll_secs.map(TickSecs::new…)` — the config's
`validate()` ladder already rejects `poll_secs == Some(0)` typed
(`config.rs:1635`), and the mint runs only on a validated config (boot
seed and `run_settings_write`'s re-decode-and-validate, the stage-3
trap (b) guard), so the mint's `expect` follows the
`poll_interval_secs` precedent verbatim. A `TickSecs` newtype mint at
the trust boundary per the A1 pattern; no validate-and-forget — the
config ladder *is* the validation and it guards both paths.

`settings_snapshot_from_domain` / the UI wire snapshot are untouched:
`files_sync.poll_secs` is not an editable UI key (absent from the apply
registry — `files_sync.enabled`/`anchor` are the only files-sync keys,
both OnRestart), so the wire carries nothing new.

### D3 — Re-arm rule: the connector's min rule verbatim

The `SettingsChanged` arm leaves the ignore arm (its comment is
superseded; the remaining network vocabulary stays ignored) and becomes:

```rust
Ok(SystemEvent::SettingsChanged) => {
    // The min rule (the Deck connector's semantics, D-5.5 item 6):
    // a shortened cadence applies at once, a lengthened one never
    // postpones an armed deadline. Through `reschedule` so a failure
    // streak keeps its backoff curve instead of collapsing to the
    // poll interval.
    deadline = deadline.min(reschedule(&st, &reader.read_settings()));
}
```

And both wake arms re-read per cycle:
`deadline = reschedule(&st, &reader.read_settings())`. Backoff knobs
(`backoff_initial_secs`/`backoff_max_secs`) come from the snapshot too
(the `[sync]` triple is live-editable; the connector already treats
them so). `FilesSyncConfig` loses `poll_interval`, `backoff_initial`,
`backoff_max` (dead once the read is authoritative); it keeps
`connector`, `clock`, `app_version`, `deck_origins`.
`AppConfig::files_sync_config()` drops the three mints; the
`poll_secs.unwrap_or` rule moves into the snapshot (D2).

**No double-schedule**: one `deadline`, re-assigned by `min` — the
min rule only ever shortens, and the post-cycle `reschedule`
re-arms from the current read; a rapid settings-edit burst collapses
to at most one wake per shortening. Doorbell-loss semantics are the
round-5 accepted lag story (`Lagged` costs one stale-cadence cycle;
the next wake re-reads anyway).

**In-flight cycle**: a doorbell during `run_cycle` is queued by the
broadcast and applied when the select re-enters — the *next* wake
honors it, never mid-cycle (the connector's pinned semantics, stage-3
trap (a)).

### D4 — Clock seam: the harness that exists, no new seam

The actor's tests already run `#[tokio::test(start_paused = true)]`
with `tokio::time::advance` (the #868/#133-era precedent for the Deck
actor's own live-cadence tests): the deadline math is
`tokio::time::Instant`, so paused-time advance drives every re-arm
deterministically — no injected clock, no sleep. The `cfg.clock` stamp
seam is untouched by this chunk. Round law (b) satisfied by the
existing pattern.

### D5 — Config mint ownership

`files_sync_config()` keeps minting everything the cfg still carries
(connector/clock/app_version/deck_origins); its three cadence fields
are deleted, and its existing tests (`config.rs:3058–3065`) are
rewritten to assert the **snapshot** mapping
(`settings_snapshot().files_sync_poll_secs == Some(7)` /
`None` + the `unwrap_or` rule exercised through the actor tests), not
the deleted fields.

## 3. Test matrix (all in `actor.rs`'s test module, the D-FT12 one-behavior-one-test rule)

Fixture: the harness's `EmptyPersistedState` grows a settable snapshot
(`ArcSwap<SettingsSnapshot>` or a `Mutex<Option<…>>`-free
`std::sync::RwLock`-free shape — an `arc_swap::ArcSwap` field, matching
the engine's own mechanism) so a test can publish a new snapshot before
sending the doorbell. `POLL` stays 3 600 s; the new snapshot's
`poll_interval_secs` mints short values.

1. **shorten** — after the attach cycle, publish a snapshot with a
   short `files_sync_poll_secs` override, send
   `SettingsChanged`, `advance(short)` ⇒ the next cycle has run
   (request count grows) — the shortened cadence applied without
   advancing the old `POLL`.
2. **lengthen** — publish a snapshot with a long interval, send
   `SettingsChanged`, `advance(POLL)` ⇒ the cycle still fires at the
   old armed deadline (the min rule: a lengthened interval never
   postpones an armed poll).
3. **doorbell during a cycle** — stage a transport effect that parks
   the cycle (the `Effect::Status(503)` staged-request pattern or the
   x1 lane-full trick), send `SettingsChanged` while the cycle is
   in flight, let the cycle finish ⇒ the *next* wake honors the new
   value and exactly one timer is armed (request-count arithmetic
   proves no double-fire within the advanced horizon).
4. **override carried live** — boot snapshot with
   `files_sync_poll_secs = Some(S)` and a *different* live
   `poll_interval_secs`; doorbell + advance ⇒ the override wins (D2's
   `unwrap_or` rule observed live, boot-vs-live parity).
5. **backoff preserved across a doorbell** — after a failed cycle
   (streak ≥ 1), doorbell with a shortened poll ⇒ the next wake is
   still bounded by the backoff curve, not the raw poll interval
   (the min rule goes through `reschedule`; mirror of the connector's
   recorded comment).
6. **existing matrix untouched** — `matrix_1..23`/x-series keep
   passing: their harness snapshot defaults to `None` override +
   default triple, so every request-count observable is unchanged
   (a deliberate consequence of D2/D3: the boot-default behavior is
   bit-identical).

No text assertions (typed request counts / status outcomes only); no
sleeps (paused time only).

## 4. Implementation steps (TDD order)

1. Domain: `SettingsSnapshot::files_sync_poll_secs` + `Default` arm +
   `settings.rs` test touch-up. Red: D5's snapshot-mapping test.
2. App: `AppConfig::settings_snapshot()` maps the override;
   `files_sync_config()` drops the three fields; delete their tests,
   add the snapshot mapping tests (red→green).
3. Actor: `reschedule(&st, &SettingsSnapshot)` with the `unwrap_or`
   rule; per-cycle reads; the doorbell arm; cfg field removal;
   bootstrap call-site untouched (cfg has fewer fields — no signature
   change). Harness snapshot setter + the matrix above (red→green).
4. `cargo fmt` / `clippy -D warnings` / `run-quality-gates.sh cargo
   nextest run --workspace` (flock-safe; defer per the rule if
   contended).

Diff confinement: `taskboard-domain/src/settings.rs`,
`taskboard-app/src/config.rs` (mint + tests),
`taskboard-sync-files-nextcloud/src/actor.rs` (+ tests). Estimated
**~300–400 lines** including tests. No UI, no state-crate logic change
(the engine's wholesale replace carries the new field for free).

## 5. Risks

- **Serde compatibility**: `SettingsSnapshot` derives
  `Serialize/Deserialize` with `#[serde(default)]` — the new `Option`
  field defaults on old payloads; the snapshot is never journaled
  anyway (memory-only), so the risk is nil, recorded for completeness.
- **Harness churn**: every existing actor test constructs
  `FilesSyncConfig` via `cfg()` — field deletion touches one fixture
  function, not the call sites (spawn signature unchanged).
- **Test-policy snapshot change**: `SettingsSnapshot::default()` gains
  `files_sync_poll_secs: None` — engine-only fixtures see no behavior
  change (the override absent = the `[sync]` fallback, identical to
  today's boot-default mint).
- **The apply registry**: no entry needed (the key is not UI-editable);
  if `files_sync.poll_secs` ever becomes editable, the registry entry
  and the wire field are the follow-up — a candidate backlog note if
  stage 2 finds the overlay's per-field writer already able to touch
  it (verify during implementation; file the issue only if the lane
  actually reaches the key).

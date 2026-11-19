# Config Reload & Batch Chunk Plan (tier-2 round, chunk P6, #909 + #910)

- **Date**: 2026-11-18
- **Branch**: `feat/config-reload-batch` (worktree
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-config-reload-batch`,
  branched off `origin/main` @ 5d6bd327)
- **Origin**: tier-2 dispatch plan
  `.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §6 (normative)
  + the UX journey plan
  `.artifacts/plans/2026-11-18-config-reload-batch-ux-plan.md`
  (written alongside this plan; stage 2 starts only on its
  acceptance) + the accepted #845 journey
  `.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md`
  (rulings R-3/R-4 scoped these two out; this chunk scopes them in).
- **Issues**: #909 (live-reload watcher), #910 (batch config set).
- **Status**: ACCEPTED (operator, 2026-11-19) — journey gate +
  `notify` stack addition both accepted; stage 2 proceeds per this
  plan's rulings R-1..R-8 and decisions D-STACK…D-SCOPE.

## 1. Seams (re-anchored, file:symbol)

- `crates/taskboard-app/src/settings_overlay.rs:prove_and_write_with`
  — the shared prove skeleton (one document load, edit closure, one
  verdict, one atomic rename). Its `section`-addressed shape serves
  single-section edits; the batch needs a doc-level multi-section
  generalization (D3 below). The prove gate itself
  (`prove_verdict` → `decode_file_only_str` + `validate`) is reused
  verbatim by both new lanes.
- `crates/taskboard-app/src/commands/config.rs:set_with_locked` /
  `unset_with_locked` — the lane+env seam (`run_settings_write` with
  `Option<&EngineHandle>`); the batch surface composes the same port
  (one call, one mint, one apply).
- `crates/taskboard-app/src/ui_requests.rs:run_settings_write` — the
  write→prove→mint→apply→echo choreography; its apply half (mint the
  whole snapshot from the proven file, `StateCommand::UpdateSettings`)
  is exactly what the reload path reuses (the reuse law).
- `crates/taskboard-app/src/config.rs:AppConfig::extract_file_only` /
  `decode_file_only_str` — the file-layer decode (no env provider)
  the prove gate's read half uses; the reload's read half calls the
  same decode over the watched file's text.
- `crates/taskboard-app/src/config.rs:AppConfig::write_path` /
  `config_path` — the single path-resolution rule the watcher must
  watch (explicit `--config` / `TASKBOARD_CONFIG` / XDG-if-present /
  default).
- `crates/taskboard-app/src/config.rs:SyncSection` + `validate` — the
  `[sync]` boot-fallback precedent: figment decodes the file layer,
  and semantic loudness lives in typed `validate` rejections — the
  reload's prove step keeps that shape (decode failure and
  validation failure are both typed `ConfigError` classes).
- `crates/taskboard-app/src/bootstrap.rs` — the actor spawn seam; the
  watcher task spawns beside the existing actors, receives the
  engine handle + resolved path, and honors shutdown ordering.
- `crates/taskboard-app/src/commands/mod.rs:route_for_ipc` — the #845
  relay pin (`Command::Config` → `Request::Exec`); batch rides the
  same wire — no new wire variant, the daemon is the single parse
  authority.

## 2. Decisions (each resolves an open question)

### D-STACK: the `notify` crate joins the stack (the AGENTS §4 entry — operator-gated)

**Decision: add `notify` (latest stable of the 7.x/8.x line at
dispatch) to the root `[workspace.dependencies]`, version-pinned
once, consumed by `taskboard-app` only.** Features: the default
cross-platform backend (inotify on Linux, ReadDirectoryChangesW on
Windows — both first-class release platforms) with raw event
delivery. **No debouncer crate**: `notify-debouncer-*` is rejected
because the debounce must be deterministic under
`tokio::time::pause`/`advance` (AGENTS §5) — a hand-rolled quiet
window inside the watcher actor over tokio timers gives exactly that;
the debouncer crates own their own clock threads and would reopen the
sleep-in-test problem the clock law forbids.

**Alternatives weighed:**

1. **Polling `stat` checks** (mtime/size every interval) —
   **rejected**: (a) latency equals the poll interval (the journey
   promises "within a moment"; a kiosk-tuned interval is seconds), (b)
   constant wakeups contradict the low-power kiosk goal, (c) mtime
   granularity on some filesystems coalesces distinct rapid edits.
   Polling stays the *graceful-degradation* escape if the watcher
   backend fails to set up (US-1/S1 escape) — actually rejected
   there too: the degradation is "watcher disabled + one typed
   broadcast warning", because a polling fallback would mask the
   failure class instead of surfacing it typed.
2. **Debounce inside `notify` (debouncer crates)** — rejected per
   the determinism argument above (D-STACK rationale).
3. **A hand-rolled inotify-sys layer** — rejected: unsafe surface in
   a crate that carries `#![forbid(unsafe_code)]`, plus per-platform
   reimplementation of what `notify` already ships.

The operator confirms this addition at the journey gate (dispatch
plan §6: one gate, both decisions).

### D-DEBOUNCE: window and rename-storm handling

- **Window: 300 ms quiet window** on the config path's events. Editor
  save patterns (`vim`'s rename+create, `nano`'s truncate+write,
  atomic-rename writers) deliver all their events within
  milliseconds; 300 ms is well above any observed storm and well
  below human-imperceptible. Recorded as a named constant with the
  seam to override in tests.
- **Rename storms**: watch the config file's *parent directory*
  (non-recursive), filter every event to the resolved config path
  (`Rename`, `Modify(Name/Content/Data)`, `Create`, `Remove`). Any
  event stream whose trailing edge lands 300 ms after the last
  matching event triggers exactly one reload (journey US-1 AC-2).
- **Partial-write tolerance**: a reload attempt that finds the file
  missing (mid-rename) or unreadable is *skipped and retried on the
  next trailing edge* — never an error loop (journey US-1 AC-3). A
  reload attempt on a *present but invalid* file is the typed
  failure path: engine keeps the last-known-good snapshot, one typed
  broadcast event fires, and a subsequent good write converges
  normally.
- **Self-write suppression (D-SELFWRITE)**: the app's own overlay
  writes are atomic renames — they fire the watcher like any editor.
  Suppression is by *snapshot comparison*, not by a suppression
  timestamp: the reload decodes the (already-proven) document, mints
  the whole snapshot, and applies `UpdateSettings` only when it
  differs from the engine's published settings. This makes self-writes
  and no-op external saves cost nothing (one decode), and removes the
  timestamp race a "recent write" guard would carry. (journey
  US-1 AC-4.)

### D-RELOAD-PATH: reuse of the C10 overlay-merge lane (binding)

The reload is the C10 apply half with no write step: filter →
debounce → `decode_file_only_str`-class decode (file layer only, env
pins standing at boot keep standing — D-R5 above) → `validate` →
whole-snapshot mint → compare → `StateCommand::UpdateSettings`. It
must not mint from a partial projection or invent a second apply
semantic — the journey's reuse law and dispatch plan §6 both make
this normative. Actor stamps ride the injected `Arc<dyn Clock>` seam
per the clock discipline (AGENTS §5, 2026-11-06).

### D-BATCH-SHAPE: #910's atomic transaction

- **Grammar (D-BATCH-GRAMMAR)**: `config set <k1> <v1> [<k2> <v2> …]`
  — an even-length argv pair list. One pair is byte-for-byte
  today's behavior; zero pairs / odd counts are typed refusals
  (`BadValue`-class, nothing touched).
- **Atomicity (D-BATCH-ATOMIC)**: parse *all* pairs (typed refusal
  names the offending key, nothing sent/written) → env-guard *all*
  keys (any pin → typed `EnvLocked` naming the key, nothing written)
  → duplicate-key check (typed `DuplicateKey { key }`; last-wins
  resolution is rejected — ambiguity is never resolved silently) →
  **one document**: load once, apply every section/key mutation, one
  prove verdict, one atomic rename → one whole-snapshot mint → one
  `UpdateSettings`. A failing mid-batch key therefore rolls back by
  construction: the write never happened (journey US-2 AC-2/3/4/5).
- **Cross-section writer**: `prove_and_write_with` is extended to a
  doc-level multi-section variant (one `DocumentMut`, per-pair
  `section/key` addressing, the same base-verdict/attribution rule
  the per-intent writers follow). The per-intent writers migrate to
  nothing — they stay as-is; the batch writer is the new caller of
  the shared `prove_verdict` + `write` halves (no skeleton fork).
- **Typed outcome**: `ConfigBatchSetView { entries:
  Vec<ConfigSetView>, applied_live }` — one lane bit per invocation
  (the batch is one intent), per-key echoes reused from
  `ConfigSetView`.
- **Lane behavior**: identical to #845 — daemon relay when served
  (one apply), typed next-start when not.

### D-SCOPE: what stage 2 does not build

No GUI delta (R-1), no off-switch knob (R-4), no env re-read (R-5),
no batch unset (R-6), no `config reload` verb (R-5/#909 journey), no
watcher for non-config files (R-3). The `[sync]`-triple boot fallback
is precedent only — nothing there changes.

## 3. Implementation order (TDD, each step red→green)

1. **Batch writer first** (`settings_overlay.rs`): the multi-section
   doc-level prove+write variant — tests for one-document atomicity,
   prove refusal, attribution rule parity (`us2_ac1`, `us2_ac4`).
2. **Batch surface** (`commands/config.rs` + `cli.rs`): even-pair
   parse, knob resolve, batch env guard, duplicate check, the
   `run_settings_write`-shaped port call, the typed batch view +
   JSON emit — the truth-table tests (`us2_ac2/3/5/6`), the CLI
   arity parse tests.
3. **Stack addition** (`Cargo.toml`): `notify` pinned in
   `[workspace.dependencies]`; consumed by `taskboard-app`.
   *(Sits behind the gate in effect, but the manifest commit is
   inert until the watcher module uses it — stage 2 may land it
   first so the gate discussion happens on a compiling branch.)*
4. **Watcher actor** (`config_watcher.rs` + `bootstrap.rs` spawn):
   event filter, hand-rolled debounce over tokio timers (pause/
   advance tests — `us1_s2`, `us1_ac2`), partial-write retry
   (`us1_ac3`), prove → mint → compare → apply-or-noop
   (`us1_ac4`, `us1_ac5`), setup-failure typed fallback (US-1/S1
   escape), engine-gone outcome.
5. **Integration**: the daemon relay lane for batch (rides
   `Request::Exec`, same parse authority); `config print` truth
   after batch; the hostile-env pass (the C6/#865 patterns).

## 4. Acceptance (dispatch plan §6, mapped)

- Accepted journeys' wiring rows implemented — W-1…W-7 of the
  journey plan; each row's Test column names the pinning test.
- #909: rename-storm and partial-write scenarios pinned
  deterministically (pause/advance over the debounce seam, never
  sleeps) — order step 4's tests.
- External-edit reload converges to the same state the C10 lane
  produces — the D-RELOAD-PATH reuse + a parity test (same file
  delta via set-with-locked vs via external write → equal published
  snapshots).
- #910: one invocation = one atomic document; a failing mid-batch
  key rolls back the whole write; typed error names the offending
  key — order step 1–2's tests.
- Hostile-env pass green (the C6/#865 patterns).
- Diffs confined to the config command surface + the watcher module
  + tests (+ the two manifest lines of D-STACK).

## 5. Risks & notes

- **inotify descriptor limits / exotic filesystems** (network mounts,
  some overlayfs): watcher setup may fail. The typed fallback
  (watcher disabled + one broadcast warning; restart contract
  degrades to today's behavior) keeps this a non-crash. Kiosk
  targets with NFS config paths are the realistic case — surfaced,
  not silent.
- **Event filter correctness** decides both the storm behavior and
  the no-op cost: tests must pin "foreign file in the same dir
  → no reload scheduled" alongside the storm coalescing.
- **Snapshot comparison cost**: the mint runs on every debounced
  event; it is one TOML decode over a small file — negligible, but
  the no-op path must remain one decode, not a decode + engine round
  trip.
- **`notify` version churn**: pin exactly one version in
  `[workspace.dependencies]` (AGENTS §4); the consumer list stays
  one crate, so future bumps are one-line.
- **Duplicate commit churn**: batch refusals must not emit the file
  writer at all — the parse/guard/duplicate chain runs before the
  document load so the prove skeleton is never invoked on a doomed
  batch (cheaper and keeps the attribution rule's blame clean).
- **Rebase hygiene**: the branch sits off today's `origin/main`; the
  r9 branches do not touch the app config/watch lanes — no
  disjointness verdict required by the dispatch plan, but stage 2
  re-anchors `settings_overlay.rs` against the landed state before
  step 1.

## 6. Estimated diff

- New: `config_watcher.rs` (~250–350 lines incl. tests) + journey/
  chunk plans (docs).
- Modified: `settings_overlay.rs` (+~80), `commands/config.rs`
  (+~150), `cli.rs` (+~30), `bootstrap.rs` (+~30),
  `commands/output.rs` (+~30), root + app `Cargo.toml` (+2 lines).
- Tests: ~10–14 new deterministic tests.
- **Total ≈ 600–750 lines, mostly in `taskboard-app`.**

# Config-Set Propagation Chunk Plan (backlog-reduction round, chunk C10, #845)

- **Date**: 2026-11-17
- **Kind**: chunk plan — stage-1 planning output for C10
  (`feat/cli-config-set-propagation`), TDD-first, written per the
  dispatch plan
  (`.artifacts/plans/2026-11-17-backlog-reduction-dispatch-plan.md`
  §1 C10, §10 stage-1 mandate) against the round plan §3-C10
  (normative).
- **Origin**: the UX journey plan
  `.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md`
  (**journey-gated: stage 2 proceeds only on the operator's
  acceptance of that plan**) + issue #845.
- **Status**: DRAFT — GATE-PAUSED.

## 1. Seams (re-anchored, file:symbol)

- `crates/taskboard-app/src/commands/config.rs` — the entire current
  `config set`/`unset` surface: `ConfigKey` (the 17-key knob table,
  exactly the GUI editor's `Field::key()` vocabulary),
  `ConfigSetError` (typed refusals: `EnvLocked`/`UnknownKey`/
  `BadValue`/`Io`), `set`/`unset`/`set_with_locked` (the lock-set
  seam — tests compose without touching process env),
  `write_dotted` (the `toml_edit` atomic writer — **no prove gate**),
  `handle` (the dispatch arms printing the free-text "applies at the
  next start" line).
- `crates/taskboard-app/src/commands/mod.rs:2304` — `route_for_ipc`:
  `Command::Config { .. }` is pinned `IpcRoute::Local` (the pin this
  chunk deliberately splits).
- `crates/taskboard-app/src/main.rs:126` — the relay dispatch
  (`IpcRoute::IpcOnly` / `IpcOrLocal` / `Local` lanes); `IpcOrLocal`
  already encodes "daemon if serving, else local" — the lane
  semantics US-2 needs.
- `crates/taskboard-app/src/ipc/{protocol,server,client}.rs` —
  `Request::Exec { argv }` ("the server is the single parse
  authority") / `Response::Exec { code, stdout, stderr }`: **no new
  wire variant needed**; the daemon re-parses the same argv.
- `crates/taskboard-app/src/ipc/server.rs` — the daemon-side `Exec`
  handler: it must reach the app handles (engine mpsc + write path +
  config) for the config arm to apply live. Confirm what the Exec
  closure already carries (stage-2 first read; the status arm proves
  engine access exists on the server side).
- `crates/taskboard-app/src/ui_requests.rs:597` —
  `run_editor_write`: the GUI editor's live-write choreography (env
  guard → overlay write → effective re-decode + validate →
  `settings_snapshot()` → `StateCommand::UpdateSettings` → snapshot
  echo; `SettingsError::Gone` when the engine is stopped = the
  "file written, next start applies" precedent). This is the
  choreography W-3 extracts into a shared port.
- `crates/taskboard-app/src/settings_overlay.rs` — the per-intent
  typed writers (one per knob — all 17 CLI keys covered) over
  `prove_and_write` (apply → prove → atomic rename). **No removal
  writer exists** — W-4 adds `remove_key`.
- `crates/taskboard-domain/src/messages.rs:618` +
  `crates/taskboard-state/src/actor.rs:865` — `UpdateSettings`
  (whole-snapshot swap; undo `None`). No domain delta in this chunk.
- `crates/taskboard-app/src/config.rs:1367,2055,2104` —
  `env_overridden_keys` (the env-diff truth table),
  `overlay_document`, `write_config_atomic`. Unchanged.
- **Hostile-env pins (C6-era)**: `config.rs::env_overridden_keys_truth_table`
  and the `set_tests` module's env-lock composition test own the env
  layer; the journey's US-4 composes over the same lock-set seam —
  those pins do not move.
- `crates/taskboard-domain/src/settings.rs` — `SettingsSnapshot`
  already projects all 17 CLI keys; every consuming actor re-reads
  per iteration (the doc's own sentence). No domain delta.

## 2. Decisions (each resolves an open question)

- **D1 — semantics: write-through with typed offline fallback**
  (journey plan §1's recommendation, restated as the binding chunk
  decision). Alternatives rejected: pure restart contract (leaves a
  real divergence over an existing seam), warning-only divergence
  surfacing (absorbed as the typed `applied_live: false` outcome
  instead of a warning).
- **D2 — no new wire variant.** `Request::Exec` already relays argv
  with the daemon as single parse authority; a dedicated
  `ConfigSet` IPC variant would duplicate the grammar on the wire
  for no gain. Additive-only law respected (nothing removed).
- **D3 — route split, not unpin.** `route_for_ipc` keeps
  `Print`/`Path` `Local` (journey R-2: file-authoritative
  diagnostics; after write-through the file is written before the
  engine apply, so local print never lags) and moves only
  `Set`/`Unset` to the relay lane (`IpcOrLocal`: daemon if serving,
  else local — exactly US-2's fallback). The compile-error-forces-a-
  lane convention (the `DaemonCmd` precedent) governs any future
  `ConfigCmd` variant.
- **D4 — one write port, three fronts.** The editor's
  `run_editor_write` choreography is extracted into a shared app-side
  port (`ui_requests.rs` or a sibling module) called by (a) the GUI
  editor arms, (b) the daemon's config-set arm, and (c) the CLI's
  local lane. The current `write_dotted` no-prove-gate path is
  retired onto the overlay writers so **both lanes run the prove
  gate** — today's CLI set can theoretically write a file that fails
  validation at next boot (predicate-per-key only); the port closes
  that too. The CLI keeps its own string→typed-value parsing (the
  UI crate's mirrors stay untouched per the existing boundary
  comment).
- **D5 — unset gains a prove-gate removal writer.**
  `settings_overlay::remove_key(path, section, key)` (mutate → prove
  → atomic rename), and both lanes' unset migrate onto it. The
  env-lock guard stays *ahead* of the write in both lanes (the
  Files-row two-key precedent generalized: guard the key, then the
  writer).
- **D6 — typed outcome view replaces the free-text line.**
  `ConfigSetView { key, value, applied_live }` through `emit_to`
  (text lane: one honest line; JSON lane: the bits). Text is
  presentation; the tests assert the typed view/lane bit, never the
  wording. `applied_live: false` is a **success** (exit 0) — the
  write landed in the file layer; only refusals exit nonzero.
- **D7 — engine-gone mid-flight = the editor's `Gone` precedent.**
  If `UpdateSettings` fails after the file write, the outcome is the
  typed next-start result, not an error (the file is the boot
  authority; no partial repair exists — `run_editor_write`'s own doc
  rules this).
- **D8 — CLI parse stays CLI-side, before any send.** `UnknownKey`/
  `BadValue` fire before socket contact (US-4/S2); the daemon's env
  guard fires after relay (US-4/S1) because env truth is per-process
  — the daemon's env set is authoritative for its own writes, and
  the client-side guard remains for the local lane. Recorded
  asymmetry: an env key pinned in the client's shell but not the
  daemon's is the daemon's truth that governs (the daemon owns the
  running app).

## 3. Implementation order (TDD, each step red→green)

1. **W-4 first (the removal writer)** — `settings_overlay::
   remove_key` red test (journey `us3_s2_remove_key_leaves_the_key_and_proves_bootable`
   + preserves-unrelated-tables + prove-gate refusal), then green.
   Pure file-layer, no lane entanglement — safest first stone.
2. **The shared write port (W-3)** — extract the choreography from
   `run_editor_write` behind a seam; port its existing truth-table
   tests onto the extracted form (behavior-preserving refactor, red
   = the new port's tests against a fake engine handle). The GUI
   arms re-aim at the port; `us4_s1`/`us4_s3` refusals asserted on
   the port.
3. **Local lane migration (US-2)** — `commands/config.rs` set/unset
   parse (D8) → env guard → overlay/removal writer via the port;
   `write_dotted` retired. Tests: the existing `set_tests` corpus
   must stay green (semantics preserved) + new `us2_s2` prove-gate
   test + typed view (`us5_s1`, `us2_s3` — `ConfigSetView`, lane bit
   false).
4. **Route split (W-1, D3)** — `route_for_ipc` arm split; routing
   pin tests (`us1_s1` relay vs local; `Print`/`Path` stay `Local`).
5. **Daemon-side arm (W-2)** — Exec dispatch reaches the app
   handles; the daemon config arm runs guard → port →
   `UpdateSettings`. Tests: `us1_ac1` (live apply through the
   relay), `us1_ac2` (file-before-engine order), `us4_s1` on the
   daemon lane, `us3_ac1` (unset live), engine-gone → typed
   next-start (D7).
6. **Typed view + output (W-5)** — `ConfigSetView` through
   `emit_to`; text render; JSON lane test (structural).
7. **Help text (W-6)** — posture per the accepted semantics; no text
   assertions.

Every step: pre-commit hooks gate; flock-wrapped `cargo nextest`
(defer per the #879 rule if contended — the coverage trio is the
dispatcher's).

## 4. Acceptance (dispatch plan C10, mapped)

- Journey plan written from the template, citing `docs/ux_planning.org`,
  covering the three semantics candidates with a recorded
  recommendation — §1 of the journey plan; D1 here.
- Gate: dispatcher pauses; stages 2–3 only on operator acceptance —
  this chunk **ends stage 1 at the gate**.
- Typed outcomes; no RwLock/Mutex (the port rides the existing
  engine mpsc + oneshot; AGENTS §3); the divergence window closes
  (live lane) or is typed (offline lane) per the accepted design.
- UX completeness mandate (stage 3): `scripts/ux-coverage.sh <journey
  plan> --gate` re-run + walkthrough.

## 5. Risks & notes

- **Exec-handler capability** (W-2): the daemon's Exec arm may not
  currently hold the engine handle — first stage-2 read resolves the
  plumbing; worst case the server closure gains an app-handles
  struct (server-side only, no wire change).
- **Double-env-guard asymmetry** (D8): a test asserting the daemon
  guard wins over a client-side pin needs a process-env-free
  composition (the lock-set seam pattern) — no `env::set_var` in
  tests.
- **`write_dotted` retirement** touches the existing `set_tests`
  corpus; semantics preserved, so the corpus should stay green
  modulo the view change (they assert file/typed facts, not output
  text — verified: the corpus asserts `matches!`/file contents only).
- **Snapshot/journal noise**: `UpdateSettings` is journal-titled and
  undo-`None` already; extra CLI-originated applies must not change
  `PersistedState` (settings are published-only, `state.rs:67`) — a
  free invariant, worth one assertion in the port tests.
- **Divergence residue**: a *stale socket* fallback (US-2 AC-2)
  means a wedged daemon silently degrades to file-only. Typed, but
  the walkthrough should judge whether `IpcOrLocal`'s connect
  timeout is honest enough at kiosk.

## 6. Estimated diff

~450–650 lines across `commands/config.rs` (view + migration),
`commands/mod.rs` (route split + tests), `settings_overlay.rs`
(removal writer + tests), `ui_requests.rs` (port extraction),
`ipc/server.rs` (daemon arm), `cli.rs` (help), `commands/output.rs`
(view plumbing) — tests roughly half. No domain/state crate changes.

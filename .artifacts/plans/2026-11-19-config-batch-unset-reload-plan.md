# Config Batch Unset + Manual Reload Chunk Plan (round 3, chunk B7, #1014 + #1015)

- **Date**: 2026-11-20
- **Branch**: `feat/config-batch-unset-reload` (worktree
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-config-batch-unset-reload`,
  branched off `origin/main` @ 9e307547)
- **Origin**: round-3 dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  §14 (normative) + the UX journey plan
  `.artifacts/plans/2026-11-19-config-batch-unset-reload-ux-plan.md`
  (written alongside this plan; stage 2 starts only on its
  acceptance) + the accepted config family journeys
  `.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md`
  (#845) and `.artifacts/plans/2026-11-18-config-reload-batch-ux-plan.md`
  (#909/#910 — rulings R-5/R-6 scoped these two out; the demand each
  ruling named has appeared, issues #1015/#1014).
- **Issues**: #1014 (batch config unset), #1015 (manual reload CLI
  verb).
- **Status**: DRAFT — awaiting operator acceptance of the journey
  plan at the B7 journey gate (dispatch plan §14). Stage 2 proceeds
  per this plan's decisions D-UNSET-…/D-RELOAD-… only after "GATE
  CLEARED".

## 1. Seams (re-anchored, file:symbol — the landed P6 state)

- `crates/taskboard-app/src/commands/config.rs:set_batch_with_locked`
  — the landed atomic batch: parse ALL → env-guard ALL → duplicate
  check → one `run_settings_write` port call with the doc-level
  writer → one `ConfigBatchSetView` (per-key entries + one lane bit).
  The unset batch mirrors this chain step for step (the parity law);
  `ConfigBatchError` is reused per D-UNSET-ERROR.
- `crates/taskboard-app/src/settings_overlay.rs:prove_and_write_doc`
  + `BatchEdit { section, key, item }` — the doc-level multi-mutation
  writer over the shared prove skeleton. The unset twin needs a
  removals shape over the same skeleton (D-WRITER below) — one
  document load, all removals, one `prove_verdict`, one atomic
  rename.
- `crates/taskboard-app/src/settings_overlay.rs:remove_key(path,
  section, key)` — the landed per-intent removal writer; its
  semantics (absent removal succeeds, prove after removal, the
  section-header cleanup posture) are the per-key truth the doc-level
  twin must preserve.
- `crates/taskboard-app/src/commands/config.rs:unset_with_locked` —
  the landed single unset (`ConfigKey` resolve → env guard →
  `run_settings_write` over `remove_key` → `ConfigSetView { value:
  None }`). The one-key form of the widened grammar must behave
  identically (D-GRAMMAR).
- `crates/taskboard-app/src/config_watcher.rs:reload_once(&watched,
  &engine) -> ConfigReloadOutcome` — the landed converged reload
  choreography (transient skip → decode → validate → snapshot
  compare → `UpdateSettings`/no-op → `EngineGone`). The manual verb
  calls this function directly (D-RELOAD-PATH); `pub(crate)` is
  already the right visibility for the same-crate daemon arm.
- `crates/taskboard-domain/src/messages.rs:ConfigReloadOutcome` —
  the typed outcome (`Applied`/`Unchanged`/`SkippedTransient`/
  `RefusedDecode`/`RefusedValidate`/`EngineGone`/`WatcherDisabled`),
  Copy + Serialize — rides the Exec payload verbatim, no new classes
  (D-RELOAD-OUTCOME).
- `crates/taskboard-app/src/commands/config.rs:handle_daemon` — the
  daemon relay arm (`Set`/`Unset` today; `Print | Path =>
  unreachable`). Gains the `Reload` arm (D-RELOAD-ROUTE) and the
  widened `Unset` arm.
- `crates/taskboard-app/src/repl.rs:resolve_words_impl` — the relay
  resolver's config-verb match (`Set { .. } | Unset { .. } if
  config_relay`); widens to include `Reload` (D-REPL). Note the
  `LineRefusal::ConfigCommand` arm: the reload verb must be accepted
  in relay mode and (like set/unset today) run local-only in REPL
  local mode.
- `crates/taskboard-app/src/commands/mod.rs:route_for_ipc` +
  `IpcOnlyVerb` — the route table (`Status`/`DaemonStop` are the
  IpcOnly precedents with the typed `NoDaemon` refusal,
  commands/mod.rs:384). The reload verb joins as an IpcOnly variant
  (D-RELOAD-ROUTE).
- `crates/taskboard-app/src/cli.rs:ConfigCmd` — the subcommand enum
  (`Set { pairs }`, `Unset { key }` today); `Unset` widens to
  variadic keys, `Reload` joins (D-GRAMMAR/D-RELOAD-VERB).
- `crates/taskboard-app/src/ipc/server.rs` — the Exec dispatch that
  feeds relayed argv through `resolve_words_with_config_relay` into
  `handle_daemon`; no new wire variant (the argv is the wire, ADR
  0012).

## 2. Decisions (each resolves an open question)

### D-GRAMMAR: `config unset` widens to variadic keys
`ConfigCmd::Unset { keys: Vec<String> }` (`<k1> [<k2> …]`). The
one-key form is behaviorally identical to today (same lanes, same
typed classes, same `value: none` echo). Zero keys is a typed
`ConfigBatchError::BadArity` — parity with the set batch's even-pair
grammar refusal. The alternative (a separate `UnsetBatch` variant)
was rejected: two variants for one verb forks the relay resolver,
the daemon arm, and the route table for zero user value; the
internal API is not a stability contract while the grammar is
additive (one key still parses and behaves as before).

### D-ERROR: the unset batch reuses `ConfigBatchError`
No parallel enum. `UnknownKey` / `EnvLocked` / `DuplicateKey` /
`Io` / `Rejected` / `BadArity` all apply verbatim; `BadValue` is
documented as set-only (unset carries no values — no variant is
constructed from the unset path, and the doc comment says so). The
relay's end-to-end typed-class rule (the landed `handle_daemon`
comment: "the relay never relabels an invocation it only carries")
keeps holding: an unset-batch refusal is a `CommandError::ConfigBatch`
report. The set-side `BadArity` doc ("zero pairs or an odd argv
count") gains the unset phrasing ("zero keys").

### D-ATOMIC: the unset batch chain mirrors set_batch_with_locked
Parse ALL keys (knob-table resolve, naming the offender) → env-guard
ALL → duplicate check (`BTreeSet`, naming the repeat) → **one**
`run_settings_write` port call whose writer does the doc-level
removals → one lane bit. The guard chain runs before the document
load, so the prove skeleton is never invoked on a doomed batch (the
landed set-batch invariant). A mid-batch prove failure cannot
partially write — the skeleton's atomic rename makes rollback
by-construction (the #910 AC-4 shape). `DuplicateKey` fires on
repeats even though removal is idempotent: ambiguity is never
resolved silently (parity with set's last-wins rejection).

### D-WRITER: the doc-level removals twin in settings_overlay.rs
New `prove_and_write_doc`-sibling: `prove_and_remove_doc(path,
&removals: &[(section, key)]) -> Result<(), OverlayError>` — one
`DocumentMut` load, one removal per pair reusing `remove_key`'s
in-document semantics (including absent-key success and the
section-header cleanup posture), one `prove_verdict`, one write.
Removals are expressed as a `(section, key)` tuple slice rather than
a `BatchEdit`-style struct: a removal carries no `item`, so the
payload struct would be a `BatchEdit` with a field permanently
`None` — the tuple is the honest shape. Stage 2 may refactor toward
a shared `DocEdit` enum (insert/remove) if the two writers' load/
prove/write plumbing duplicates more than ~15 lines — the shared
skeleton call is already there; do not abstract preemptively.

### D-ABSENT: absent-key removal is idempotent success
Parity with the landed `remove_key` test ("absent removal
succeeds"). A batch naming `focus.work_mins` (present) and
`focus.break_mins` (absent) removes the present one, succeeds
overall, and echoes both entries with `value: none`. No `NoSuchKey`
error class: "unset an already-unset knob" is a legitimate
converge-to-default request, not an ambiguity.

### D-VIEW: `ConfigBatchUnsetView` is a type alias, not a new struct
`pub type ConfigBatchUnsetView = ConfigBatchSetView;` with every
entry `ConfigSetView { key, value: None, applied_live }`. The batch
shape (entries + one lane bit) is already exactly the receipt a
multi-unset needs; a distinct struct would duplicate serde +
rendering for an identical wire shape. The alias keeps docs/greppability
without a fork. `render_batch_set_text` serves both (its
`(true, None)` arm already renders the removed-value line honestly).

### D-RELOAD-VERB: `config reload` — daemon-only, no local lane
`ConfigCmd::Reload` (no arguments). Route: `IpcRoute::IpcOnly
(IpcOnlyVerb::ConfigReload)` — the `Status` precedent. Without a
running daemon the standing typed `NoDaemon` refusal fires; no local
fallback is built (a non-running session's file is already its boot
authority — a local "reload" would be a fiction, and `SkippedTransient`-
style classes mean nothing without an engine to converge). The verb
is documentation in the help text for the `WatcherDisabled` escape
hatch (#1015's founding use case).

### D-RELOAD-PATH: the daemon arm calls `reload_once` directly
`handle_daemon`'s `Reload` arm computes `write_path` (the session's
resolved path, as the Set/Unset arms do) and calls
`config_watcher::reload_once(&write_path, &app.engine)` — the
identical function the watcher lane runs. Equivalence of manual and
watcher convergence is therefore by construction; the journey's
AC-1 pin is a daemon-lane test asserting the same
`ConfigReloadOutcome` the watcher tests assert for the same file
mutations, not a re-derivation. No new broadcast is emitted by the
manual verb (the outcome travels to the invoking terminal as the
Exec payload; the watcher's broadcast lane stays
watcher-triggered-only — double-reporting the same apply is noise,
and the #1045 UI lane is out of scope per the journey's R-5).

### D-RELOAD-OUTCOME: the view wraps the landed enum verbatim
`ConfigReloadView { outcome: ConfigReloadOutcome, path: PathBuf }`
(Serialize; the path names what was reloaded — the diagnostic half
of `config path`'s honesty posture). No new outcome classes: the
enum's seven variants are the whole truth of `reload_once`, and
`SkippedTransient` surfacing to a manual invoker is honest (the file
was mid-save — retry). `WatcherDisabled` never reaches this verb
(the manual path never touches `notify`); it stays a boot-time
broadcast.

### D-REPL: the relay resolver widens to `Set | Unset | Reload`
One parse authority; the daemon's arm executes. In local REPL mode
(no daemon), `Reload` refuses with the existing `LineRefusal::
ConfigCommand` posture (set/unset run local there; reload cannot —
it has no local semantic per D-RELOAD-VERB). Stage 2 maps the
refusal to the typed `NoDaemon` class at the route layer rather than
adding a new refusal text.

### D-SCOPE: what stage 2 does not build
No mixed set+unset batch (journey R-4). No UI lane for outcomes
(R-5/#1045). No watcher re-arm (R-6). No print/path route changes
(R-7). No `ConfigReloadOutcome` variants. No changes to the single
`unset` error family (`ConfigSetError` stays the one-key path's
typed contract — the widened verb's *batch* refusals are
`ConfigBatchError`; a one-key invocation that fails parse/guard
reports through the batch classes from here on, which is a superset
naming the same key; the relay parity comment in `handle_daemon`
updates accordingly). This last point is the one deliberate
behavioral delta of the grammar widening and is covered by the AC
tests; if the operator wants the one-key refusals to stay
`ConfigSetError`-typed, the fallback is a size-1 dispatch in
`unset_batch_with_locked` returning the old view — flagged here so
the gate rules on it explicitly. **Recommended ruling: accept the
batch-class unification** (one verb, one error family).

## 3. Implementation order (TDD, each step red→green)

1. `settings_overlay.rs`: `prove_and_remove_doc` + tests —
   multi-key removal lands one document; absent-key success; prove
   rejection leaves the file byte-identical; section-header cleanup
   preserved (`us1_s4_batch_unset_lands_one_atomic_document`).
2. `commands/config.rs`: widen `ConfigCmd::Unset` to variadic +
   `unset_batch_with_locked` (parse → env guard → duplicate → one
   port call) + `ConfigBatchUnsetView` alias; the one-key form's
   parity tests; the truth-table tests (AC-2/3/5/6/7)
   (`us1_s1_*`, `us1_s2_*`, `us1_s3_*`, `us1_s5_*`).
3. `commands/config.rs` + `config_watcher.rs`: the daemon `Reload`
   arm over `reload_once`; the daemon-lane equivalence tests
   (`us2_s2_manual_reload_uses_the_watcher_lane_choreography`,
   AC-2/3 outcomes typed).
4. Routing: `IpcOnlyVerb::ConfigReload`, `route_for_ipc`, the relay
   resolver widening, the ipc/server dispatch
   (`us2_s1_reload_routes_ipc_only`,
   `us2_s2_relayed_reload_reaches_the_daemon_arm`,
   `us2_s2_reload_rides_the_config_relay`).
5. `cli.rs` grammar + help; `output.rs` JSON emit for both views
   (`us1_s6_*`, `us2_s3_*`); REPL local-mode refusal test.
6. The watcher-disabled manual-reload test (AC-5 — no `notify`
   dependency), the `cargo test --doc` pass, fmt/clippy/deny/nextest
   through `scripts/run-quality-gates.sh`.

## 4. Acceptance (dispatch plan §14, mapped)

- Batch unset atomic (a failing key rolls back the whole write, the
  typed error names the key): AC-2/3/4/5 — tests in step 2.
- The reload verb produces the same converged state as the watcher
  lane: same-function by construction, pinned in step 3 (AC-1).
- Typed outcomes throughout: `ConfigBatchError` / `ConfigReloadView`
  / `NoDaemon` — no text assertions anywhere.
- Hostile-env pass green: the batch unset env-guard tests run under
  the existing hostile-oracle harness posture (the landed
  `clean_ambient` gate; the re-homed legs stay in
  `tests/services/ui_settings.rs` if ambient-TASKBOARD_* legs are
  needed).
- `scripts/ux-coverage.sh <journey-plan> --gate` green on the
  claimed rows: §6 W-1..W-8 each name tests embedding their
  step-ID tokens.

## 5. Risks & notes

- **Grammar-widening blast radius**: `Unset { key }` →
  `Unset { keys }` touches every match site (CLI dispatch, REPL
  resolver, relay command reconstruction, daemon arm, the
  unreachable arm's comment). Mechanical, but the compiler must
  drive it — no `..`-elided matches left silent. The one-key error
  delta is recorded under D-SCOPE for the gate to rule on.
- **`remove_key`'s section-header cleanup at doc level**: removing
  the last key of a section must leave the same document shape the
  per-intent writer produces (the landed posture); the step-1 tests
  pin it against a mixed multi-section batch.
- **`reload_once` visibility/ownership**: it lives in
  `config_watcher.rs` as `pub(crate)` — same crate, fine. If stage 2
  finds the import graph awkward (watcher module pulling notify into
  the daemon arm's compile path — it already does for the binary),
  the function may move to a `reload.rs` sibling; a pure `git mv` +
  re-export, no semantic change.
- **Relay-lane timing**: `reload_once` decodes + mints synchronously
  inside the daemon arm; the file layer is small (the watcher does
  the same work per trailing edge) — no timeout budget concern, but
  stage 2 keeps the arm off the IPC read-budget path's hot verbs
  (it's one Exec like set/unset today).
- **Rebase hygiene**: branch off today's `origin/main`; B7b
  (`feat/files-sync-poll-write-lane`) may land concurrently — it
  touches the apply-registry + `#1051` lanes in `commands/config.rs`
  tests, not the batch/unset/reload surface; a textual conflict there
  resolves in favor of both (different hunks). Stage 2 re-anchors
  against the fetched `origin/main` before step 1.

## 6. Estimated diff

- New: journey plan + chunk plan (docs, this pair).
- Modified: `settings_overlay.rs` (+~60), `commands/config.rs`
  (+~180), `cli.rs` (+~40), `repl.rs` (+~25),
  `commands/mod.rs` (+~20), `ipc/server.rs` (+~15),
  `commands/output.rs` (+~30), `config_watcher.rs` (0–30, only if
  the move happens).
- Tests: ~14–18 new deterministic tests.
- **Total ≈ 400–500 lines, all in `taskboard-app`.**

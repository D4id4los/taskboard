# [Plan] Chunk B7b — `files_sync.poll_secs` live-write lane (#1051)

- **Date**: 2026-11-20
- **Kind**: Implementation chunk plan (round-3 backlog-reduction, chunk
  B7b; dispatch plan §15 normative, scope report §B7). Engine-side,
  **ungated** (no UX journey plan required — the scope report's §B7
  ruling).
- **Branch**: `feat/files-sync-poll-write-lane` (this worktree).
- **Origin**: issue #1051 (the #673 writer twin) + dispatch plan
  §15 + scope report §B7.
- **Accepted base cited**:
  - **C10 journey contract**:
    `.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md`
    — see §C below for the recorded citation verdict.
  - **#673 reader-side re-arm** (landed): the files-sync actor's
    per-cycle settings read — `crates/taskboard-sync-files-nextcloud/
    src/actor.rs`, `reschedule()` (the
    `files_sync_poll_secs.unwrap_or(poll_interval_secs)` rule) and the
    `SettingsChanged` doorbell re-arm (state actor → broadcast).
  - **#845/#915 landed writer lane**: the persist-then-command
    choreography — `crates/taskboard-app/src/commands/config.rs`
    (`ConfigKey` knob table → `TypedSetting` → overlay writer →
    `run_settings_write` → `StateCommand::UpdateSettings`) +
    `crates/taskboard-app/src/ui.rs::settings_apply_registry()`.
  - **#1051's predecessor**: the landed files-sync live-cadence chunk
    (the `files_sync_poll_secs: Option<TickSecs>` field on
    `SettingsSnapshot`, `crates/taskboard-domain/src/settings.rs:245`).

## A. Problem

`[files_sync] poll_secs` is a live-honored key on the reader side: the
actor re-reads the snapshot at every re-arm and the override wins over
`[sync] poll_interval_secs` (#673, landed). But the key has **no write
lane**: the `ConfigKey` knob table has no `files_sync.poll_secs`
variant, `settings_overlay.rs` has no `set_files_sync_poll_secs`
writer, and `settings_apply_registry()` carries no entry. `config set
files_sync.poll_secs <n>` is an `UnknownKey` refusal today; the only
way to change it is a manual file edit plus a restart.

## B. Decisions (every open question resolved)

**D1 — Registry entry shape: `Apply::live()`, no justification.**
Mirroring the `[sync]` triple (`sync.poll_interval_secs` etc.), which
is the closest registered key-class (a per-cycle cadence override
consumed from the snapshot). The live claim is honest: `UpdateSettings`
→ engine replaces the snapshot → `SystemEvent::SettingsChanged`
broadcast → the files-sync actor's doorbell re-arm → `reschedule()`
reads the override at the next wake (#673, landed). The
"doorbell during a cycle applies at the next wake only" semantics are
already pinned by the #673 actor tests — this chunk does not re-test
them (one behavior, one test, lowest layer).

**D2 — Writer shape: a dedicated per-key fn, mirroring
`set_backoff_initial`.** `pub fn set_files_sync_poll_secs(path: &Path,
secs: u64) -> Result<(), OverlayError>` delegating to the existing
`set_secs(path, "files_sync", "poll_secs", secs)` helper. The `> 0`
predicate (and the whole-document prove gate, which already rejects
`poll_secs = Some(0)` — `config.rs:1634`) refuses at the gate,
attributed to `files_sync.poll_secs`. The doc comment records that the
ordering/upper-bound rules are the prove gate's, symmetric to the
backoff pair.

**D3 — Knob-table variant: `ConfigKey::FilesSyncPollSecs`, mirroring
`ConfigKey::PollInterval` in every match arm** — `parse_value`
(`i64 > 0 → TypedSetting::Secs`), `dotted()` →
`("files_sync", "poll_secs")`, `write` → D2's writer, `batch_edit` →
the `Secs` item arm (so `config set` **batch** also carries the key
for free — the batch path is exhaustive over `ConfigKey` and the
compiler forces the arm). `unset` needs no new code: `remove_key(path,
"files_sync", "poll_secs")` falls out of D3's `dotted()`, and removal
yields `None` = the fallback to `[sync] poll_interval_secs` — the
semantics #1051 asks for.

**D4 — Lattice-test handling: the explicit-exception mechanism, not an
editor field.** `the_registry_covers_exactly_the_editor_field_lattice`
(ui.rs) gains `files_sync.poll_secs` in its explicit exception set,
next to the existing `files_sync.anchor` precedent — the key is
CLI-only this chunk (diff confinement: no Slint delta), and the test's
doc comment already sanctions exactly this mechanism. The GUI editor's
Files row does not gain a field; that coverage gap is filed as a
backlog issue (see §F), not silently absorbed.

**D5 — Live-propagation test: the #673 pattern at the lowest
layer that owns the new behavior.** The new behavior is "a written key
reaches the snapshot"; the actor's honoring is #673's existing pin
(the doorbell test
`a_files_sync_poll_secs_override_carried_live` / the
`reschedule` unwrap_or rule). So the chunk's propagation tests are
app-side:
  1. `config set files_sync.poll_secs 12` (engine handed, via the
     existing `run_settings_write` fake-engine harness) → the
     `UpdateSettings` the fake receives carries
     `settings_snapshot().files_sync_poll_secs == Some(12)` **and**
     `ConfigSetView { applied_live: true }` (typed outcome — never
     text). The shorten/lengthen axis: a follow-up set of `90` (lengthen)
     then `2` (shorten) each produce a fresh `UpdateSettings` with the
     new value — the same sequence shape the #673 doorbell tests run
     actor-side.
  2. `config unset files_sync.poll_secs` → the key leaves the file
     and the minted snapshot's `files_sync_poll_secs` is `None`
     (the fallback rule).
  3. Overlay-writer round-trip: `set_files_sync_poll_secs` preserves
     sibling keys (mirroring the existing `poll_secs survives` test in
     `set_files_sync_enabled`'s suite) and the prove gate refuses `0`
     typed (`ConfigSetError::Rejected`, `matches!` on the variant).

**D6 — C10-contract citation: HOLDS, recorded, no amendment.** See §C.

**D7 — Help text: no delta.** The CLI help documents the grammar, not
a key census; the knob table doc comment (config.rs:142) says the
vocabulary is the editor's `Field::key()` set — D3 widens it by one,
so the doc comment gains a sentence recording the second
CLI-only-exception key (`files_sync.anchor` precedent). No clap
attribute churn.

## C. The C10 contract citation (recorded per round law [L](c))

The accepted C10 journey plan
(`.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md`)
§1 states the contract as **semantics, not a key census**: option 1 —
write-through — where "All seventeen CLI knob-table keys are members
of `SettingsSnapshot`'s projection; a live write needs no new engine
capability, only the choreography's reuse". §7's delta table pins the
mechanism (`Config::Set` rides `Request::Exec`; the apply seam is
`StateCommand::UpdateSettings`; typed `ConfigSetView`).

**Verdict**: the contract covers this key-class. `files_sync.poll_secs`
is already a member of `SettingsSnapshot`'s projection (the #673
field, landed before this chunk), so admitting it to the knob table is
an *instance* of the C10 contract, not scope growth: no new wire
variant, no new engine capability, the same choreography. The census
number ("seventeen") was descriptive of the table at acceptance time,
not a closure of the vocabulary — the table itself is the contract's
extension point and D3 extends it by one, deliberately and recorded
here. **No journey amendment needed; the operator is informed via
this plan's citation, per the dispatch plan's stage-1 verification
requirement.** (Scope growth beyond *this* — e.g. a GUI editor field,
or a key whose value is NOT in the snapshot projection — would be a
new contract question and goes to the operator; neither is in this
chunk.)

## D. Implementation steps (TDD; stage-2 executes)

1. **Red — tests first** (all app-crate, per the C10 test-root):
   - D5-1's three tests in
     `crates/taskboard-app/src/commands/config.rs` (the set →
     snapshot-value → `applied_live` test, the unset → `None` test,
     the batch-set arm landing — batch via the existing batch harness
     if one exists, else the per-key lane only, with the batch arm
     covered by the compiler-exhaustive match + one
     file-content assertion).
   - D5-3's overlay tests in
     `crates/taskboard-app/src/settings_overlay.rs` (sibling
     preservation + the `0` refusal, `matches!` on the typed error).
   - The registry tests in `crates/taskboard-app/src/ui.rs`: extend
     the Live-key assertion list with `files_sync.poll_secs` and the
     lattice test's exception set (D4).
2. **Green**:
   - `settings_overlay.rs`: `set_files_sync_poll_secs` (D2).
   - `commands/config.rs`: `ConfigKey::FilesSyncPollSecs` + the four
     match arms (D3) + the knob-table doc sentence (D7).
   - `ui.rs`: the registry entry (D1) + the lattice exception (D4).
3. **Hostile-env pass**: `TASKBOARD_FILES_SYNC_POLL_SECS` (the env
   layer's spelling of the key) pins the key → the set refuses
   `EnvLocked` typed (the existing guard runs before the write; one
   test mirroring the existing env-locked case).
4. **Gate**: pre-commit hooks +
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock; background + poll beyond ~5 min; defer per the rule if
   contended).

**Diff confinement** (dispatch §15 acceptance): `crates/taskboard-app/
src/commands/config.rs`, `crates/taskboard-app/src/settings_overlay.rs`,
`crates/taskboard-app/src/ui.rs` + their test modules. **No** engine,
state, sync, domain, or Slint changes. Estimated diff: **~120–180
lines** (≈45 implementation, the rest tests + doc comments).

## E. Traps (from dispatch §15's stage-3 focus, answered here)

- **Write meets the #673 reader mid-cycle**: a set that lands while a
  cycle is in flight applies at the *next* wake only — the actor's
  doorbell test pins this; the writer side adds no new ordering
  (UpdateSettings → broadcast is the existing choreography). Nothing
  to implement, everything not to break.
- **No other key gains a lane silently**: the only registry addition
  is `files_sync.poll_secs`; the lattice test + the diff audit pin
  this.
- **Invalid values**: `0`, negatives, and non-integers refuse
  `BadValue`/`Rejected` typed at parse/gate before any file write
  (the parse-before-lane rule, D8 of the #845 journey); the prove gate
  re-refuses a `0` that arrived by any other path.

## F. Backlog issues filed (round law [L](d))

- **#1083** — the GUI editor field gap from D4 (`files_sync.poll_secs`
  is CLI-only; the Files row gains no field this chunk; the lattice
  exception shrinks when the field lands). The `#1014` batch-unset
  parity needs no new issue: batch unset is generic over the knob
  table once D3 lands, and #1014 owns it.

## G. Out of scope (recorded)

- A GUI editor `Field` for the key (D4 — backlog issue).
- The `[files_sync] enabled` OnRestart justification's substrate
  spawn-lane (existing recorded deviation — untouched).
- Any change to the actor's `reschedule` or doorbell semantics
  (#673 owns them; landed).

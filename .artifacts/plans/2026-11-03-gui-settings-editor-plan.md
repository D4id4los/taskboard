# [Plan] Backlog Round 3, Chunk 4.1 — GUI Settings Editor (#351)

- **Date**: 2026-11-04 (authored; file name reserved by the dispatch
  prompt as 2026-11-03 — kept verbatim so stages 2/3 find it).
- **Status**: PROPOSED — stage-1 plan for chunk 4.1 of the backlog
  round 3 dispatch. Branch `feat/gui-settings-editor`, cut from
  `origin/main` (323a9bf9; `scripts/worktree-doctor.sh` passed
  pre-creation, 209 G free against the 25 G floor; gate verified:
  round-4 chunk 11 merged — PR #619, rebase-merged 2026-11-04
  20:59 UTC, rewritten commits 005a5129/a30e5211/92b21e2f/75704db6,
  no other open PR touches the settings surface).
- **Origin** (normative): issue #351 (scope, precedence honesty, the
  secrets rule); the origin ruling
  `.artifacts/plans/2026-10-18-field-family-design-plan.md` §3.6 D-F6
  (the `[preferences]` config-section home + the ruled addition that
  ordered this editor); dispatch plan
  `2026-11-03-backlog-round3-dispatch-plan.md` §4 chunk 4.1
  (acceptance + the three stage prompts).
- **Scope guard**: `taskboard-app` (the write port, the precedence
  read, the request handlers), `taskboard-ui-slint` (the settings
  view's new sections, the adapter/VM, the form mirror, the status
  mapping, journeys), plan + CHANGELOG. **Not in scope**: any
  `taskboard-domain` / `taskboard-state` change (no engine concept is
  touched — writes are file-layer, applies are boot-time), the
  connections ceremony (already shipped through r4 chunk 11 — the
  editor joins its surface, never rewrites it), the `[[sync.connectors]`
  and `[ui.stack_widths]`/`[ui.window_geometry]` subtrees (owned by
  their single writers — see D6/D7), keymap *overrides* editing
  (preset stays the only editable keymap key — D10), the REPL/CLI
  `config print` surface (already ships the resolved display).
- **All `file:line` anchors are this worktree at 323a9bf9**; every one
  below was read, not carried over.

## 0. Findings → mechanisms audit (what the code confirms)

The single most load-bearing finding: **the typed config-write port
this chunk is asked to "generalize" already exists and is proven** —
the UI → app request lane (`UiRequest`, `crates/taskboard-ui-slint/src/requests.rs:154-215`)
carries typed requests with `oneshot` reply halves; the app-side
forwarder (`spawn_ui_request_forwarder`, `crates/taskboard-app/src/ui_requests.rs:90-160`)
dispatches every settings request immediately, never debounced
(`handle_settings`, `ui_requests.rs:335-375`); and per-intent overlay
writers persist each field into the file with `toml_edit` preservation
(`overlay_set_ui_scale` `crates/taskboard-app/src/connections.rs:309`,
`overlay_set_keymap_preset` :325, `overlay_upsert_connector` :192,
`overlay_remove_connector` :255). `SetScale` and `SetKeymapPreset`
are *exactly* the shape the editor's new fields need. The chunk's
real work is: (a) the precedence read, (b) the write-gate
generalization, (c) the general-section inventory, (d) the view.

- **The write target rule (the precedent to generalize) is one
  function.** `AppConfig::write_path(explicit_path)` (
  `crates/taskboard-app/src/config.rs:1057-1065`): the resolved config
  path when resolution finds one (explicit `--config` /
  `TASKBOARD_CONFIG` / an existing XDG file), else the first-run
  target (`first_run_write_target` :821-826 — an existing CWD default
  keeps its file; else the XDG candidate; no platform config dir →
  CWD). Desktop boot resolves it once at hand-over
  (`crates/taskboard-app/src/main.rs:217,262`; kiosk :744) and the
  `ConnectionWorkspace.config_write_path` carries it into the
  forwarder (`ui_requests.rs:69-79`). **The editor writes the same
  resolved path through the same workspace — zero new path logic.**
  Its fallback table is already pinned by tests
  (`config.rs:2303-2341`, rows 4–6) — the acceptance criterion "the
  write port reuses the logout machinery's fallback rules (pinned)"
  is satisfied by *construction and citation*, plus one new pin
  asserting the editor's handlers resolve through the workspace field
  (not a fresh resolution), §6 W-8.
- **The file-layer write discipline is one shared skeleton.**
  `overlay_document(path)` (`config.rs:1570-1578`): read → parse as
  `toml_edit::DocumentMut` (absent file = empty document); the writer
  mutates one table subtree; `write_config_atomic(path, contents)`
  (`config.rs:1619-1646`): tmp sidecar 0600 → adopt the destination's
  existing mode masked to `0o777` → rename. Comments, formatting, and
  unrelated tables survive by construction; the mode rule
  (operator territory, #386) is pinned at the shared writer
  (`config.rs:2374-2435`) — every editor writer inherits it by using
  the same skeleton. The two existing writers of this artifact are
  the connect overlay and the logout key strip
  (`connector_ceremonies.rs:85,143`; `commands/logout.rs:209`);
  `AppConfig::write_path`'s doc comment names exactly these — the
  editor joins as the third family, per-intent like the settings
  writers that followed.
- **Precedence is real and already surfaced honestly on the CLI.**
  The provider chain is defaults → toml → `TASKBOARD_*` env
  (`figment()`, `config.rs:1529-1534`; `ENV_PREFIX` :33, `__` nests).
  `taskboard config print` decodes the *effective* config
  (env included) and prints it secret-free
  (`commands/config.rs:20-45`; the `ConfigView` payload
  `commands/output.rs:663-682`). The GUI hand-over resolves the
  effective config once (`UiSettings`, `crates/taskboard-ui-slint/src/lib.rs:66-110`)
  and "nothing reads config mid-flight" (the consumers-contract
  comment, `config.rs:1419-1424`). Env cannot set
  `sync.connectors` (a figment limitation, documented at
  `config.rs:301-303`) — the array shape is file-only.
- **Live vs restart is decidable per section from the boot graph.**
  Boot injects config into actors once: the reminder engine and day
  roll take `reminder_engine_config()` / `day_calendar()` at spawn
  (`bootstrap.rs:1085-1086`), the sync actors take
  `sync_actor_config()` (:1190, :1236), the focus runtime's spawn
  decision takes `focus_config()` (:1137). The UI hand-over carries
  scale, keymap, `day_start`, `reminder_defaults`, `focus`,
  `connectors`, `boards` once (`ui.rs:424-470`, `lib.rs:66-110`).
  The three live-apply exceptions are proven by their reply paths:
  scale commits set `Theme.base_scale` *before* the lane round-trip
  and revert on a typed `Err` reply
  (`adapter.rs` `apply_scale_commit`, :~3230); the keymap preset is
  swapped live from the `KeymapUpdate` reply (D27/D9); connectors
  attach/detach live through `ConnectorControl` (the r2/r4
  ceremonies). Everything else is restart-required — that is the
  honest inventory, §3.
- **The settings view joins an established, heavily-pinned surface.**
  Post-r4c11 layout (`ui/views/settings-view.slint`): pinned header +
  pinned status line, then one scrolled column — Connections (the
  board selector is the shared `Dropdown` since r4 D39, :22/:59-65/:532+,
  options re-derived per push pass from `presentation::boards_of`,
  `presentation.rs:4137`), the connector rows, the add/edit form, the
  keymap preset `SegmentedControl`, the scale slider + field. The
  model (`SettingsModel`, :66-115) is typed data only; the busy
  machine (one op in flight disables every settings op, D34) and the
  outcome/status-line mapping (`settings_status.rs:25-77`, exhaustive
  no-wildcard — new variants fail to compile until mapped) are the
  seams the editor reuses. The form's validation mirror
  (`settings_form.rs`: per-field predicates `field_invalid` :73, the
  ladder `validate` :150 recomposed from them — flavour and Save gate
  are one source of truth) is the pattern for the new fields'
  mirror predicates. **The plan prefers *reusing the domain parser*
  over mirroring where the parser is domain-owned** (`default_before`
  → `taskboard_domain::ReminderOffset::parse_text_form` — the UI
  crate already depends on the domain, so no drift is possible;
  mirroring is reserved for rules that live behind non-UI deps
  like `reqwest`).
- **Two subtrees are single-writer territory and must not gain an
  editor field.** `[ui.stack_widths]` + `[ui.window_geometry]`:
  "this task owns every write of both subtrees — no other code path
  touches them" (the R6 single-writer rule extended,
  `ui_requests.rs:4-15`). `[[sync.connectors]]`: the connections
  ceremony owns it (verify-first Save, the migration backstop,
  `ensure_migrated` `ui_requests.rs:746-777`). An editor field that
  wrote either would be a second writer racing a debounced flush —
  D6/D7 forbid it, and the read-only display of those subtrees is
  the honest rendering.
- **The redaction discipline is structural and reusable.** The lane's
  `Secret` newtype redacts `Debug` and is proptest-pinned
  (`requests.rs:217-260`). The config itself is secret-free by
  construction — every table `deny_unknown_fields`, so a `token` /
  `password` key cannot even decode (`config.rs:4-14`); credentials
  resolve through `crate::secrets` and the credential *store*, never
  the file. The one secret-adjacent config value is
  `[web] token` (`config.rs:663-664`, an explicit capability-token
  override) — §4 handles it.
- **Goldens**: the settings view has no golden
  (`tests/goldens/` holds today/task-view/kiosk/focus/review/track),
  so the new sections cost no golden regeneration. The
  `golden_visuals.rs` harness is untouched.

## 1. The precedence-honesty model — one call, recorded

**Decision D1: the editor edits the FILE layer and displays the
EFFECTIVE resolved value; env-overridden keys are additionally
surfaced per-key, read-only, with their source named.**
(issue #351's two options are not exclusive — this is the second as
the base with the first as the per-key overlay.)

Mechanics, all typed:

- **Display**: every row renders the effective resolved value — the
  same value the running processes actually use (the `UiSettings`
  hand-over is already the resolved config; the editor's snapshot is
  built app-side from it, §5). The row also names its layer source.
- **The env read (new, app-side, pure)**: a
  `env_overridden_keys` function that decodes the config twice —
  once file-only (defaults → toml; a new `extract_file_only`
  sibling of `extract_raw` that omits the `Env` provider, same
  db-path fill), once effective (the existing chain) — serializes
  both to `serde_json::Value` (`AppConfig` is `Serialize`,
  `config.rs:67`) and returns the sorted set of dotted keys whose
  leaves differ. Pure over two decoded trees; the provider
  difference is the only input. Tests inject the env half through a
  hand-built figment provider over a map (never process env — the
  deterministic-by-construction rule; `figment::value::Dict` makes
  this a dozen lines). `TASKBOARD_CONFIG` itself is not a diff key —
  it selects the file the workspace already points at.
- **Enforcement**: a key in the env set renders **read-only** in the
  editor — the row's edit seam is not wired (the adapter refuses the
  dispatch arm on the same typed fact, defence in depth), the row
  renders the env badge, and the value shown is the effective one.
  Editing the file under a live env override would be a lie (the
  change would never take effect while the env stands); surfacing
  the override and disabling the field is the honest behaviour.
- **Sampled-at-boot honesty**: the env set is computed once at
  hand-over (env vars are process-boot state in practice; nothing in
  the codebase mutates its own env). The hand-over carries the set;
  write replies do not re-derive it. Documented at the type.

Why not env-surfacing-only (option one): the vast majority of fields
have no override; hiding or wholly read-only-ing sections because one
key might be overridden is worse than per-key honesty. Why not
resolved-display-only (option two bare): a user who set
`TASKBOARD_UI__SCALE` would see the file edit land and nothing change
— exactly the silent-partial-application the dispatch prompt forbids.
The per-key overlay kills that failure mode at the only place it can
arise.

## 2. The typed config-write port — shape and semantics

**Decision D2: the port IS the existing lane pattern, extended.**
One new `UiRequest` variant per editable *field-group commit* (the
`SetScale`/`SetKeymapPreset` template), one app-side handler arm in
`handle_settings`, one per-intent overlay writer per variant (new
module `crates/taskboard-app/src/settings_overlay.rs`, beside the
`connections.rs` writers in discipline, not in name). No generic
"apply patch" variant: a typed enum of intents is the secrets
boundary (§4) and the single-writer fence (D6/D7) made structural —
there is no variant a secret or a fenced subtree can ride.

- **Payload rule (A1)**: every variant's fields are validated typed
  values minted at the request seam — `chrono::NaiveTime` for the day
  start, `u64`/`u32` for the numerics (mints refuse non-parse text
  before a request is built), the existing kebab-cased enums for
  choices. The lane carries no raw strings that the app must re-parse
  (the `UiScale` precedent: `SetScale` carries `f64` and mints
  app-side — for the editor, minting happens UI-side where the
  mirror predicate lives, and the app re-mints authoritatively at
  the gate; both sides one grammar, §6 pins parity).
- **The write gate (the actual generalization)**: every editor
  writer runs *apply → prove → write*, where prove is: serialize the
  mutated `DocumentMut`, decode it through the figment chain
  (`Toml::string` over `doc.to_string()`, the tests' `decode` helper
  shape, `config.rs:2474-2479`), run `AppConfig::validate`, and gate
  the write on the verdict with **failure attribution** (D14,
  precise rule):
  - result validates → write.
  - result fails and the *base* document (the same decode before the
    mutation) validated → the edit introduced the failure → refuse
    typed (`SettingsError::PersistRejected`), file untouched.
  - result fails and the base already failed → the file was already
    a boot-blocker; refuse only when the failing error's `field`
    (every semantic `ConfigError` variant carries one, or a payload
    naming the offending entry) is inside the section the write
    edits; otherwise warn (log the `kind()`s, base and result) and
    write — the editor must not be bricked by unrelated garbage it
    did not create.
  The typed-input mints make "the edit itself is garbage" mostly
  unrepresentable (the gate's real catch is cross-field rules like
  `backoff_initial ≤ backoff_max` and interaction with values already
  in the file) — that is the point: the gate proves the *file as a
  whole* stays bootable, not just the one field. **The editor never
  turns a bootable file into an unbootable one.** This is a strict
  improvement over the existing per-intent writers (which rely on
  upstream mints alone); the existing writers are NOT retrofitted in
  this chunk (no churn — noted as follow-up material, §8 R5).
- **Atomicity & ordering**: one intent = one read-modify-write = one
  atomic rename (the skeleton's guarantee). The forwarder executes
  requests on its own task; concurrent intents queue on the channel
  (capacity 16, human scale). The lane-serializer discipline means
  no editor write races another editor write; the pre-existing
  one-shot-CLI-vs-daemon window (`ui_requests.rs:16-21`) is
  unchanged by this chunk — same worst case (a lost update of one
  side), never a torn file.
- **What a write means for a running daemon — stated, not faked.**
  A config write takes effect for processes started *after* it: the
  desktop UI re-reads at its next boot, the daemon at its next boot,
  one-shot CLI commands re-read per process. There is no signal, no
  hot reload, no pretend-live application. The only live exceptions
  are the three shipped ones (scale, keymap preset, connectors) and
  the two forwarder-owned subtrees. **Every restart-required row
  carries a typed "applies on restart" fact** — not decoration: the
  hand-over's section snapshot carries `SectionApply::{Live,
  OnRestart}` per editable group (D12), the row renders it, and the
  *write reply* for an `OnRestart` field echoes the fresh file value
  so the row proves what the file now says (the value will truly be
  live after the next restart).

## 3. Section inventory — editable / read-only / fenced

The apply column is derived from §0's boot graph, stated honestly:

| Section / key | Editor | Apply | Writer fence |
|---|---|---|---|
| `[preferences] day_start_time` | **editable** (text, `NaiveTime`) | OnRestart (hand-over `day_start` :96; engine `DayRollConfig` at spawn) | editor |
| `[reminders] enabled` | **editable** (segmented On/Off) | OnRestart (engine config at spawn) | editor |
| `[reminders] tick_secs` | **editable** (integer > 0) | OnRestart | editor |
| `[reminders] default_before` | **editable** (text, the `ReminderOffset` grammar — domain parser reused directly) | OnRestart | editor |
| `[reminders] sink` | **editable** (segmented auto/log/desktop) | OnRestart | editor |
| `[sync] poll_interval_secs` / `backoff_initial_secs` / `backoff_max_secs` | **editable** (integers; the initial ≤ max ordering enforced at the gate, error lands on the offending field) | OnRestart (actor spawn :1190/:1236) | editor |
| `[ui] due_soon_days` | **editable** (integer; 0 legal, ceiling at the gate) | OnRestart (hand-over once) | editor |
| `[ui] scale` | already editable (slider + field, r3) | Live (`Theme.base_scale`) | editor (shipped) |
| `[ui.keymap] preset` | already editable (segmented, r2) | Live (`KeymapUpdate` reply) | editor (shipped) |
| `[[sync.connectors]]` | already managed (form + dropdown, r2–r4) | Live (attach/detach) | **connections ceremony only** |
| `[ui.stack_widths]`, `[ui.window_geometry]` | read-only display (presence facts) | Live (forwarder owns) | **forwarder only (R6)** — D6 |
| `[ui.keymap] overrides` | read-only count display | OnRestart | editor-out (D10: drop-and-warn semantics at UI start, Q15 — a table editor is its own chunk) |
| `[app] mode`, `[app] shutdown_timeout_secs` | read-only | OnRestart | — |
| `[storage] db_path` | read-only (relocating it under a running app is ceremony-adjacent — display only) | OnRestart | — |
| `[nextcloud] *` (legacy scalar world) | read-only (the connections ceremony is the identity surface; `credential_store` display only) | OnRestart | ceremony |
| `[web] *` | read-only; `token` renders presence only (§4) | OnRestart (boot-spawned surface) | — |
| `[kiosk] fixed_board`, `[kiosk] rotation` | read-only (boot-time typed resolution, `resolve_fixed_board` :1334) | OnRestart | — |
| `[ipc] *` | read-only | OnRestart | — |
| `[focus] *` | read-only (the pomodoro surface owns its knobs' UX; adding them later is a small follow-up — D11) | OnRestart (spawn decision :1137) | — |
| secrets | **hidden — unrepresentable** (§4) | — | — |

Read-only rows still render (the view is a *viewer* too — issue #351's
"title"), each with its layer source and apply fact. The inventory is
the typed `SettingsSnapshot` handed over at mount (§5).

## 4. The secrets boundary — typed, not omission

Three layers, each structural:

1. **The file layer** (pre-existing, cited not re-pinned): the config
   is secret-free by construction — `deny_unknown_fields` on every
   section means a credential key cannot decode (`config.rs:4-14`);
   credentials live in the credential store, and the lane's `Secret`
   newtype is write-only (proptest-pinned, `requests.rs:217-260`).
2. **The port layer (this chunk's enforcement)**: the write port is a
   closed enum of intents over the §3 editable set. **There is no
   variant whose payload could carry a credential**, because no
   editable field is a secret; a generic string-map variant is
   rejected at design review by this sentence. The §6 catalogue pins
   the enum's closedness structurally (a test iterating the
   `UiRequest` variants' `Debug` output asserting no redaction
   marker is needed because no secret type occurs — the lane already
   has that test shape for `ConnectionProfile`).
3. **The display layer**: `[web] token` (the one secret-adjacent
   config value, `config.rs:663-664`) renders as a presence fact
   (set/unset) in a read-only row — never its value; `nextcloud`
   credential-adjacent fields do not render values at all (the
   connections rows already render identity only). `config print`'s
   secret-free-display precedent covers everything else.

**No "raw TOML edit" mode.** A free-form editor would void layer 2
entirely (it is the one variant that can carry anything). Power users
edit the file with a text editor — the editor is for typed fields.
Recorded as D13 so stage 2 doesn't "helpfully" add a raw pane.

## 5. UI shape — the editor joins the settings view

New sections in the existing scrolled column, below "Interface
scale" (the view's file gains sections; no new route, no new view
file):

- **General** (`[preferences]` + `[app]` display): the day-start text
  field (the `LabeledField`/`TippedField` row grammar — label +
  InfoTip + field + error line, the round-3 D19 contract), commit
  grammar Return-only (the scale field's rule: per-keystroke commits
  would spam the lane).
- **Reminders**: enabled segmented, tick + before text fields, sink
  segmented — choice controls commit on pick (the keymap preset's
  grammar), text fields on Return.
- **Sync timing**: the three integer fields.
- **Display**: due-soon-days integer field. (Scale/keymap stay where
  they are — shipped sections do not move.)
- **Read-only groups**: one "Deployment" group (app/storage/nextcloud/
  web/kiosk/ipc/focus rows, compact `RoleText` rows — value +
  apply badge), with the fenced subtrees' presence rows.

State and seams, following the surface's own contracts:

- **Hand-over**: `UiSettings` gains a `general: SettingsSnapshot` —
  the typed values of §3 (editable + read-only), the env-override
  key set (§1), and the per-group `SectionApply` map (D12). Built
  app-side at the two hand-over sites (`ui.rs:428-440`, kiosk's
  mirror) by one constructor (one source, both lifecycles).
- **VM**: the adapter's `SettingsVm` (adapter.rs:2317) gains the
  staged-draft buffers per text field (the `scale_draft` precedent,
  :2344-2350) and the snapshot; the busy machine extends unchanged
  (one op in flight still disables every settings op — the editor's
  ops join the same `pending`/`busy_target` machinery).
- **Requests/replies**: one `UiRequest` variant per commit
  (D2); every reply carries the fresh `SettingsSnapshot`
  (the `ConnectionsSnapshot` precedent, r2 D32 — rows re-render from
  the reply, nothing goes stale). `SettingsError`'s `Field` and
  `InvalidKind` enums grow the new fields/kinds
  (`requests.rs:120-137`); `settings_status.rs`'s exhaustive
  no-wildcard lattice then *forces* the sentence mapping to compile
  (the review trap (c), already compiler-enforced :26-77).
- **The insert-mask contract (D18)** rides every new text field: a
  focused field engages insert mode via the edited seams, commit
  returns to Normal — the same wiring the scale field and the
  connection form already run.
- **Blanket acceptance rule** (the r4 sweep's rule): every new
  interactive element produces a visible reaction — commits reply
  through the status line, refusals render typed, read-only rows
  wire no seam (a dead control is an acceptance failure, not a
  safe default).
- **Journey reachability**: the new sections sit inside the existing
  `SettingsScrollFacts` flickable — page up/down and the scroll
  facts apply unchanged; the journeys' scroll harness drives them
  without new machinery.

## 6. TDD test catalogue (red-first order; no text assertions anywhere — typed outcomes and structural facts only)

**Stage A — the write port (app crate), before any UI:**

- **W-1..W-7, per-intent overlay pins** (one per new writer, the
  `overlay_set_ui_scale` test template): seed a commented file →
  write → decode the file and assert the field's typed value AND the
  unrelated tables + comments byte-preserved; the absent-file write
  creates it; the husk rules (a defaulted value serializes to
  nothing — the section's skip contract) pinned where the writer can
  create a section (`[preferences]`, `[reminders]`, `[sync]`
  already exist in pin-less configs).
- **W-8, the write-target pin**: the editor's handler resolves
  through `workspace.config_write_path` (structural: the workspace
  field is the only path input) — plus one integration-style pin
  that a write lands at the `write_path`-resolved location for the
  three fallback rows (reusing the existing `first_run_write_target`
  fixtures' shape; env-var injection via tempdir + explicit path,
  never process env).
- **W-9, the prove gate (the new generalization, red-first)**: the
  three attribution arms of D14, each pinned as a table row —
  (a) edit introduces a failure on an otherwise-valid file → typed
  refusal, file bytes untouched; (b) pre-existing invalid section,
  edit elsewhere (unrelated) → warn-and-write, both `kind()`s logged,
  file written; (c) pre-existing invalid section, edit *into* that
  same section → typed refusal. The cross-field rule arm (the
  `backoff_initial ≤ backoff_max` ordering introduced by one field's
  edit) is the (a)-shape red test: typed input alone cannot express
  it, only the whole-document prove can.
- **W-10, precedence pins (pure)**: `env_overridden_keys` truth
  tables — no env → empty set; env overriding a file key → that
  dotted key; env setting a key absent in the file → in the set;
  identical values → not in the set; nested-section keys diff
  correctly; the set is sorted/deduped. Provider injection is a
  hand-built provider over a map (deterministic; never process env).
- **W-11, `extract_file_only` pin**: file-only decode equals the
  effective decode when env is empty (the trivial fixed point).

**Stage B — the snapshot + hand-over (app crate + ui-slint lib):**

- **S-1, the snapshot constructor pin**: one constructor, both
  lifecycles; fields project from a decoded `AppConfig` (typed
  values, no strings); the `SectionApply` map matches §3's table
  (pinned exhaustively — a new editable group without an apply fact
  fails the test).
- **S-2, the env-set hand-over pin**: the set travels to `UiSettings`
  (structural equality with W-10's output on a fixture config).

**Stage C — the view (ui-slint), journeys + unit:**

- **J-1, render journeys**: open settings → the new sections render
  the snapshot's values (typed assertions on model properties —
  never label text); read-only groups render with no wired edit seam
  (structural: the seam callback is absent / the adapter arm refuses).
- **J-2, the edit round-trip journey** (the acceptance core, per
  editable group at least once): stage a valid edit → commit →
  assert the `UiRequest` variant dispatches with the minted payload
  → reply the fresh snapshot → the row's echo re-renders from the
  reply (never the click). Uses the journeys' established mock-reply
  harness; time-independent (no sleeps — the reply is injected).
- **J-3, the typed-refusal journey**: an invalid commit (unparseable
  day start, zero tick, `initial > max`) → no request dispatches,
  the field's validity flavour renders the typed kind, the status
  line renders the refusal outcome — text content never asserted.
- **J-4, the env-override journey**: a row whose key is in the env
  set renders read-only (structural: the edit seam is not wired /
  refuses on the typed fact) with the env badge fact present; the
  effective value renders.
- **J-5, the restart-marker journey**: every `OnRestart` group's rows
  carry the apply fact (structural presence); the `Live` groups
  (scale/keymap/connectors) do not.
- **J-6, the insert-mask journey** for one new text field: focus
  engages insert, commit returns to Normal (the existing D18 harness).
- **J-7, the busy-machine journey**: an editor op in flight disables
  the other settings ops (the D34 machine covers the new arms —
  pinned once for one new op).
- **U-1, mirror predicates** (settings_form-style unit tests): the
  day-start grammar (valid/invalid truth tables — parity fixtures
  with `validate`'s rejection set, `config.rs:2048-2062`), the
  before-grammar (delegates to the domain parser — pin the
  *delegation*, not the grammar, one test, lowest layer), the integer
  fields (parse, positivity, the ordering rule's field attribution,
  the due-soon ceiling parity).
- **U-2, the status lattice totality**: rides the compiler (the
  exhaustive no-wildcard matches in `settings_status.rs`) — no new
  test needed beyond the existing coverage/non-emptiness pins.
- **Engine/state layers: nothing.** No engine concept changes; the
  "one behavior, one test, lowest layer" rule puts every behaviour
  in the app or UI layer tests above.

## 7. Decisions register (every open question, resolved)

- **D1** — precedence model: file-layer editing + resolved display +
  per-key env surfacing (read-only, source named); env set sampled
  at hand-over. §1.
- **D2** — port shape: the existing `UiRequest` + `handle_settings` +
  per-intent overlay-writer pattern, extended; closed enum of typed
  intents, no generic patch variant. §2.
- **D3** — the prove gate: every editor write decodes+validates the
  resulting document before the atomic write; typed refusal, file
  untouched. §2, W-9.
- **D4** — pre-existing-garbage files: warn-and-write for unrelated
  pre-existing validation failures (the editor must not be bricked
  by a file that already fails boot), typed refusal only for edits
  that introduce a failure. §6 W-9.
- **D5** — restart semantics: typed `SectionApply` per group travels
  in the hand-over; `OnRestart` rows render the fact; no
  pretend-live application; replies echo the fresh file value. §2/§5.
- **D6** — `[ui.stack_widths]`/`[ui.window_geometry]` are fenced
  (forwarder-only, R6); the editor displays presence, never writes.
  §3.
- **D7** — `[[sync.connectors]]` stays ceremony-owned; the editor
  adds no connector field. §3.
- **D8** — sync timings (`poll/backoff×3`) are editable, OnRestart;
  the `initial ≤ max` ordering is enforced at the gate with the
  error attributed to `backoff_initial_secs`. §3.
- **D9** — `default_before` validation reuses the domain parser
  directly (no mirror drift); the UI pins the delegation. §6 U-1.
- **D10** — keymap *overrides* stay editor-out (drop-and-warn
  semantics, Q15 — its own future chunk); preset remains the only
  editable keymap key. §3.
- **D11** — `[focus]` stays read-only this round (the pomodoro
  surface owns its knobs' UX; a small follow-up can flip it). §3.
- **D12** — the apply semantics' authority is the app (the config
  owner); the UI renders the typed fact, never hardcodes it. §5.
- **D13** — no raw-TOML edit mode, ever, in this surface (it voids
  the typed port's secrets boundary). §4.
- **D14** — the gate's attribution rule (precise, three arms):
  refuse when the edit introduces a failure on a previously-valid
  file; on a pre-existing-invalid file, refuse only when the failing
  field is inside the edited section, else warn-and-write. §2/§6
  W-9.
- **D15** — choice controls (enabled, sink) commit on pick with
  reply-echo (the keymap preset grammar); text fields commit on
  Return with focus-loss discard (the scale field grammar). §5.
- **D16** — the env diff is per-key (not per-section); the diff
  function is pure over two decoded trees; provider injection in
  tests is deterministic (a map-backed provider, never process env).
  §1/§6 W-10.
- **D17** — new writers live in a new `settings_overlay.rs` app
  module (named for what they do); the two existing `[ui]` writers
  stay in `connections.rs` untouched (no churn this chunk; retrofit
  with the prove gate is follow-up material). §2.

## 8. Risks

- **R1 — `SettingsModel` growth**: the model global already carries
  the connections form's facts; the editor adds ~15 properties.
  Mitigation: group the snapshot into one `[]SettingsRow`-style
  typed list where possible rather than fifteen scalars; stage 2
  keeps push passes single-source (the existing push_settings pass).
- **R2 — the adapter's size**: `adapter.rs` is the workspace's
  largest file and chunk 5.1's prompt demands it *quiet* after this
  chunk lands. Mitigation: the editor's adapter logic is VM methods
  following the existing settings seams (mechanical, pinned by
  journeys); no new global passes; the plan's scope guard keeps
  unrelated adapter machinery untouched.
- **R3 — env-diff edge: `TASKBOARD_CONFIG` / `--config` change the
  *file*, not a key** — the workspace already resolves one file; the
  diff is computed over that file's decode pair. An env override of
  `storage.db_path` (the default fill) diffs like any key. Both
  pinned in W-10's tables.
- **R4 — a serialized-document decode differs from figment's env
  chain** in one respect: `doc.to_string()` loses nothing but the
  decode runs defaults → toml only (no env) — which is exactly what
  the gate wants (it proves the *file* will validate as a file;
  env overrides stacking on top are the operator's standing choice,
  surfaced read-only per D1).
- **R5 — follow-up material surfacing from this plan** (recorded for
  the backlog, not this chunk): retrofit the prove gate onto the
  existing per-intent writers; `[focus]` editable rows (D11);
  keymap-overrides table editor (D10).
- **R6 — kiosk parity**: the editor rides the same settings view and
  workspace the kiosk appliance builds; the journeys run in the
  desktop harness. Kiosk-specific layout regression is covered by
  the responsive rule and the existing probes; no kiosk-golden cost
  (no settings golden exists).

## 9. Estimated diff + commit shape

| Area | Files | ~Lines |
|---|---|---|
| Write port + precedence read | `app/src/settings_overlay.rs` (new), `app/src/config.rs` (`extract_file_only`), `app/src/ui_requests.rs` (handler arms), `app/src/connections.rs` (untouched) | +450 (≥ half tests) |
| Lane types | `ui-slint/src/requests.rs` (variants, `Field`/`InvalidKind` growth), `ui-slint/src/lib.rs` (`UiSettings.general`) | +120 |
| View + adapter | `ui/views/settings-view.slint`, `src/adapter.rs` (VM + seams), `src/settings_form.rs` (mirror predicates), `src/settings_status.rs` (lattice arms) | +550 |
| Journeys + probes | `tests/journeys.rs` (J-1..J-7) | +350 |
| Docs | this plan, `CHANGELOG.md` (Unreleased) | +60 |

**Total ≈ 1500 lines (≈ 900 code / 600 tests), size M on the house
scale.** Commit shape (each standalone, rebase-clean, TDD-red-first
where the catalogue says so): (1) `feat(app)` the write port +
precedence read with W-1..W-11; (2) `feat(ui-slint)` the lane types +
hand-over snapshot with S-1/S-2; (3) `feat(ui-slint)` the view +
adapter with U-1/J-*; (4) `docs(changelog)`. One PR for the chunk
(one coherent deliverable, the dispatch plan's default granularity).

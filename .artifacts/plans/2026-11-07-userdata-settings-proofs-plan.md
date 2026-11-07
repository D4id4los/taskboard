# [Plan] Settings & Proofs per-user sections (DR-9 chunk 2, #685) — TDD implementation plan

- **Date**: 2026-11-07
- **Kind**: implementation plan (stage-1 output for dispatch chunk 2)
- **Origin**: the design plan §3 Part B (D-BF3, ruling Q3); dispatch plan
  §3 chunk 2; issue #685; the D5 notes
  (`2026-10-14-future-directions-analysis-report.md` §4 D5). Written by
  the stage-2 agent: the stage-1 session never produced its plan or
  branch (verified 2026-11-07 — no `feat/userdata-settings-proofs`
  branch, no plan artefact anywhere on the box), so this plan does the
  stage-1 settle before implementing it. Deviation reported to the
  dispatcher.
- **Status**: the design settle below is normative for the stage-2
  commits on this branch.

## 1. THE ROAM-SET (the design afternoon, settled)

The `settings` section roams the **user-preference subset** of the
validated settings family. Every entry: name, carried value, one-line
rationale against the D5 notes (user intent vs machine fact).

**Roams** (the classified list, this exact order is the data pin):

| key | value | rationale (D5 lens: user intent, not machine fact) |
|---|---|---|
| `day_start_time` | `NaiveTime` | the design names it explicitly; the classic roam |
| `reminders_enabled` | `bool` | pure user preference |
| `reminder_before` | `ReminderOffset` | the user's default lead time |
| `due_soon_days` | `DueSoonDays` | the user's due-soon horizon |
| `focus_enabled` | `bool` | productivity preference |
| `focus_timing` | `Option<FocusTiming>` | productivity preference (serde exists) |
| `focus_chain` | `bool` | productivity preference |
| `focus_notify` | `bool` | productivity preference |

**Never roams** (each pinned by the same exact-list data assertion —
absence is the pin, plus explicit negatives for the notorious
families):

- `tick_secs`, `poll_interval_secs`, `backoff_initial_secs`,
  `backoff_max_secs` — machine cadence: a low-powered kiosk and a
  desktop legitimately differ (D5's machine-fact lens).
- `reminder_sink` — device notification capability (Auto resolves
  per device); sink semantics are hardware-dependent.
- `ui.scale`, `window_geometry`, `stack_widths` — display hardware /
  per-machine layout; the config already documents them never-synced.
- Any path-, secret-, keyring-, server-, or account-adjacent key:
  none exists in the settings family today; the exact-list pin keeps
  any future one out (a new roamer must extend the list consciously —
  the test fails first).

The pin is a test asserting the classified list as **data** (the
`RoamingSettingValue` key-name const slice, exact names and order) —
never prose, never a count.

## 2. Shapes

### Domain (`taskboard-domain`)

- `settings.rs`: `RoamingSettingValue` — the typed value enum above,
  one variant per roamer, `key_name() -> &'static str`, serde via the
  values' own serde; `ROAMING_SETTING_NAMES: &[&str]` the pinned
  const. `RoamingSettingRow { value, clock }`;
  `RoamingSettings(BTreeMap<String, RoamingSettingRow>)` keyed by
  `key_name()` — the state home.
- `AppState.roaming: RoamingSettings` **and**
  `PersistedState.roaming` — persisted (see §5; a memory-only home
  would need a mutation path outside `interpret`, breaking the
  persist-then-interpret law the journal arm established).
- `messages.rs`: `UserDataReport` grows
  `settings: Vec<ImportedSetting>` and
  `proofs: Vec<ReminderDelivery>` (decoded domain rows; verbatim /
  undecodable wire rows never cross the seam — the connector drops
  and counts them, the journal rule).
  `ImportedSetting { value: RoamingSettingValue, clock }` (the name
  is the map key and equals `value.key_name()` by construction).
- `settings.rs`: `import_settings(&mut RoamingSettings,
  &[ImportedSetting]) -> SettingsImport` — the
  [`import_reflections`] shape verbatim: per-key clock guard `>=`
  (ties to the imported row — the D-ES4 fold-winner adoption argument,
  unchanged), adopt iff `clock >= local.clock && differs`; report
  `{ actions: Vec<UpsertRoamingSetting>, applied, skipped_stale }`.
- `reminders.rs`-adjacent: `import_deliveries(&mut ReminderState,
  &[ReminderDelivery]) -> DeliveryImport` — **set-union**: adopt iff
  `!is_delivered(reminder, occurrence)` (first-write-wins at the
  `(reminder, occurrence)` key — the `RecordReminderDelivery` apply
  guard, so replaying the actions lands the same home; a duplicate
  proof is a no-op, an out-of-order report converges: membership, not
  recency). Report `{ actions: Vec<RecordReminderDelivery>, applied,
  skipped_duplicate }`. Emission scope: only `Rule`- and `Break`-ref
  proofs roam (the dismissals split law — `Default`/`Adhoc`/`Tracker`
  occurrences are per-device facts).
- `persistence.rs`: new action `UpsertRoamingSetting(String,
  RoamingSettingRow)`; `apply_actions` inserts/overwrites
  unconditionally (it carries the fold's adopted winner — the
  `UpsertReflectionEntry` mirror). No new action for proofs —
  `RecordReminderDelivery` already is the set-union action.

### Wire (`taskboard-sync-files`)

- `document.rs`: `UserSections` grows `settings` and `proofs`
  `Option<BTreeMap<String, UserRecord>>` fields, the journal pattern
  verbatim: typed decode when the body is a row map, non-map body
  relays into `unknown` verbatim, never an error. `UserSections`
  serializes every present section. `RESERVED_SECTION_NAMES` becomes
  the forward-reserved slot (now empty — the two names activate).
- `settings.rs` (new codec): `encode_setting(value, clock)` → row
  `{ "value": <serde> }` keyed by `key_name()`, author `None`;
  `decode_setting` — unknown name → `RowRejection::BadKey` (relayed,
  counted, never fatal), unparseable value → `BadBody`, `Verbatim` →
  `Verbatim`; `decode_settings(map) -> (Vec<ImportedSetting>, usize)`.
- `proofs.rs` (new codec): wire key
  `"<reminder text_form>|<rfc3339 occurrence>"` (the
  `dismissal_wire_key` injectivity argument); row clock =
  `delivered_at`, body empty; decode mints the `ReminderRef` (only
  `Rule`/`Break` modeled — any other ref → `BadKey`),
  `decode_proofs(map) -> (Vec<ReminderDelivery>, usize)`.
- `fold.rs`: the two new maps merge with the existing per-key
  `(rank, clock, author, content)` machinery — no new merge logic;
  the proptest laws extend over generated documents carrying the
  sections.
- `test_support.rs`: generators grow settings/proofs rows.

### Transport (`taskboard-sync-files-nextcloud`)

- `PublishedJournal` → `PublishedState`: `document()` now derives all
  three sections from one `load_full()` (settings from the roaming
  home, proofs from the delivered set filtered to `Rule`/`Break`).
- The import delta generalizes per section (rows differing from the
  local-derived rows — the `journal_delta` rule); the fingerprint
  hashes the combined delta; the report carries all three decoded
  vecs; rejections sum. Verbatim rows still relay, never import.

### Engine (`taskboard-state`)

- `ingest_user_data_report` grows the two arms inside the one landed
  shape: fold all three homes on clones; `applied == 0` → return; one
  `repo.apply` + one `interpret` + one publish. No-op reports (all
  rows already folded) stay silent.

### Storage (`taskboard-storage-sqlite`)

- Migration **0024_roaming_settings.sql**:
  `roaming_settings(name TEXT PK, value TEXT, clock TEXT)`. Numbering
  note: chunk 1 takes 0023 on a parallel branch; whichever lands
  second renumbers if the order inverts.
- `repo.rs`: the apply match grows the upsert arm (I-U flow, same
  batch semantics); `load_inner` hydrates `roaming`.

## 3. Ingest journeys (the acceptance set)

1. A report carrying one journal row + one setting + one proof folds
   once, persists once, publishes once; an identical second report is
   a full no-op (no batch, no publish, no stamp).
2. A re-delivered proof (same `(rule, occurrence)`) is a no-op; a
   proof arriving out of order converges (membership semantics).
3. A newer setting key clock wins; an older-or-equal one is skipped.
4. A future-version document with unknown settings keys and verbatim
   proof rows: decodes, relays, counts, never errors, imports nothing.
5. The two-engine convergence journey extends: engine A adopts B's
   setting + proof through the fake transport and converges.

## 4. Test catalogue (key pins)

- **The roam-set data pin** (domain): exact `ROAMING_SETTING_NAMES`
  list; explicit negative membership for the machine/secret families
  (`tick_secs`, `poll_interval_secs`, `backoff_*`, `reminder_sink`,
  and any path/secret/keyring/server/key name) — structural facts over
  the const, no prose.
- **Proofs idempotence + order** (domain): duplicate no-op;
  out-of-order convergence; non-roaming refs never encode (wire
  filter pin).
- **Settings LWW** (domain): newer wins, `>=` tie-to-import, older
  skipped; unknown future key decodes as `BadKey`, never an error.
- **Wire tolerance** (sync-files): section decode row-wise; non-map
  section body relays verbatim; roundtrip laws extended to the new
  sections (the generated fold laws cover them).
- **Engine arm** (state): the journey in §3.1; typed outcomes only.
- **Cycle delta/report** (sync-files-nextcloud): delta per section;
  fingerprint dedupe across the combined delta; rejection counting.
- **Storage** (storage-sqlite): roundtrip through the table; boot
  hydration.

Deterministic throughout (injected clocks, `Utc.timestamp_opt`
fixtures); no text assertions; typed outcomes.

## 5. Deliberate decisions & risks

- **Settings persist** in a new table rather than riding memory-only:
  the ingest arm must advance through `interpret` (the
  persist-then-interpret law), and an action that storage dropped
  would violate the batch ≡ home law. Consumption (feeding
  `SettingsSnapshot` / the config overlay) is explicitly out of scope
  — the app lane stays the snapshot's single writer; a backlog issue
  records the adoption work.
- **Proofs union on membership, not recency**: the delivered_at leg
  is per-device fact detail; behavior (`is_delivered`) converges even
  when two devices' stamps differ, matching the dismissals precedent.
- **AppState/PersistedState churn**: the new persisted field touches
  the engine snapshot corpus (~48 files) — accepted and re-frozen
  once, on this branch; chunk 1 churns the same corpus on its branch
  (expected rebase work, recorded here).
- **Migration 0024** assumes chunk 1 lands first with 0023; renumber
  on rebase if the order inverts.

## 6. Estimated diff

~1100 lines across five crates plus tests; no UI crate, no Deck
pipeline, no entity/command clock territory (chunk-1 files untouched).

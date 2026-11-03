# [Plan] Chunk 1.3 — Storage codec text-form unification (#580 / F5-10)

- **Date**: 2026-11-03
- **Chunk**: 1.3 of backlog round 3
  (dispatch plan: `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md`
  §1 table row 1.3 + §2 "Chunk 1.3"; prompts 1.3.a/b/c)
- **Branch**: `refactor/storage-codec-text-form`, cut off `main` =
  `origin/main` = `c21c0e9c` (identical, verified 2026-11-03; no
  drift at planning time). Worktree:
  `../taskboard-WT-refactor-storage-codec-text-form` (created after
  `scripts/worktree-doctor.sh` passed — 222 G free, floor 25 G).
- **Covers** (normative): #580 → F5-10
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md`
  Axis H; re-verified 2026-11-02 at `a3987d11` in
  `.artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md`
  §2 F5-10 row + D5). Dispatch acceptance: codec.rs delegates for
  every family the audit listed; the clock-parse helper is folded;
  round-trip pins exist per family (existing value byte-identical —
  no migration, no format drift); the suite green with no test
  weakened.
- **Status**: planning complete; every open question resolved in §4
  (D1–D12). Implementation not started.

---

## 1. Method

- Every coordinate below was read at the current tree (`c21c0e9c`),
  not taken from the issue or the re-verification (`a3987d11`). The
  surface moved again since 2026-11-02: codec.rs is 1 033 lines; the
  `ReminderRef` delegation pair is now `codec.rs:471–482` (was
  :397–408); three more text-form families joined codec.rs since the
  re-verification (time-entry kind, task kind, reminder-kind
  discriminant); and the clock-parse census is now **22 inline
  `.parse().map_err(|_| RepositoryError::Corrupted)` sites + the
  habit mapper's local `clock` closure (1 definition, 5 call sites)**
  — up from 18× at `a3987d11`, because decode_task grew to 13 clocks
  and three more single-site parses (membership, journal, habit log)
  joined. The helper fold in §3 covers all of them.
- **The landscape changed in storage's favor since the issue was
  written**: the domain side already owns `text_form`/`parse_text_form`
  pairs for four of the families (TaskKind, TimeEntryKind, Insistence,
  ReminderRef) — the codec hand-writes byte-identical duplicates of
  them. The unification is therefore three different motions per
  family: *delete-and-delegate* (domain pair exists), *add-pair-then-
  delegate* (domain pair missing), and *leave-alone* (not text-tag
  knowledge). The inventory in §2 classifies every family.
- **Two vocabularies coexist for the level enums** (D1): the domain's
  existing `Importance::text_form`/`Urgency::text_form`
  (`entities.rs:149–236`) are the *hyphenated CLI/UI display*
  vocabulary (`"very-low"`), deliberately shared by CLI parse, CLI
  render, and UI labels — while the storage TEXT form and the appdata
  sidecar JSON both carry the *serde unit name* (`"very_low"`,
  snake_case, from `#[serde(rename_all = "snake_case")]`). The
  storage codec's hand-written mirror (`codec.rs:22–45, 50–73`) pins
  the serde name, not the display name. Delegating to the existing
  domain pair would silently change stored bytes (`very_low` →
  `very-low`) — exactly the format drift the dispatch forbids. D1
  resolves this.

## 2. Per-family inventory

"Pin" = the tests that assert the family's stored bytes and must
stay green **unmodified** through commits 2–4 (the byte-identity
evidence, §5). "Motion" = D(elete-and-delegate) / A(dd-pair) /
L(eave) / F(old) per §4.

| # | Family (domain type) | Storage site @ `c21c0e9c` | Domain home today | Motion — target | Pins (byte-identity evidence) |
|---|---|---|---|---|---|
| 1 | `Importance` | `codec.rs:22–45` (`importance_to_text`/`_from_text`) | serde snake_case derive `entities.rs:165–173`; display pair (hyphenated) `entities.rs:188–197` | **A** — new `serde_name`/`parse_serde_name` pair on the shared level table (D1) | none today → **new golden pins** (commit 1): the five literals + one parse reject |
| 2 | `Urgency` | `codec.rs:50–73` | same shape as Importance (`entities.rs:204–236`) | **A** — twins of #1 | none → **new golden pins** |
| 3 | `Insistence` | `codec.rs:79–85, 136–143` | **pair exists**: `reminders.rs:370–386` (`"normal"`/`"alarm"`; serde kebab-case coincides) | **D** — codec bodies → `text_form()`/`parse_text_form()` | codec `insistence_text_roundtrips_and_rejects_unknown` (literals pinned); roundtrip.rs `insistence_round_trips_through_sqlite`, `unknown_insistence_text_is_a_typed_corruption`, `pre_insistence_rows_decode_normal` |
| 4 | `TimeEntryKind` | `codec.rs:91–109` | **pair exists**: `tracking.rs:82–97` (`"work"`/`"respite"`) | **D** | roundtrip.rs `entry_kind_and_break_rule_rows_round_trip_through_sqlite`, `pre_kind_entry_rows_decode_work_and_break_rules_load_empty`; new golden pins (commit 1: the two literals) |
| 5 | `TaskKind` | `codec.rs:114–132` | **pair exists**: `entities.rs:313–328` (`"standard"`/`"tracker"`; the clap seam already delegates there) | **D** | new golden pins (commit 1); roundtrip.rs kind-row tests as above |
| 6 | `ReminderKind` discriminant | `codec.rs:150–170` (`reminder_kind_to_text`/`reminder_kind_is_at`) | serde snake_case derive `reminders.rs:269–284`; no text pair (only `is_due_offset`) | **A** — `discriminant_text_form`/`parse_discriminant_text_form` (D5) | new golden pins (commit 1: the two literals + the `Some(true)`/`Some(false)` truth table); roundtrip.rs `reminder_rule_round_trips_the_write_clock_and_epoch_decodes` |
| 7 | `RecurRule` | `codec.rs:176–186` (serde JSON wrappers) | serde derive only (`recur.rs`) | **A** — `text_form`/`parse_text_form` wrapping serde_json (D6) | codec `recur_text_round_trips_a_rule`, `recur_text_rejects_garbage_and_the_zero_interval`; **new golden pin** (exact JSON of a fixed sample); roundtrip.rs `recur_rule_round_trips_through_sqlite`, `recur_rows_survive_a_close_reopen_through_the_real_codec` |
| 8 | `RemoteTaskBaseline` | `codec.rs:192–196` (encode) + `decode::remote_baseline` `repo.rs:359–364` (decode) | serde derive only (`entities.rs:408–431`) | **A** — same pair shape as #7 (D6) | roundtrip.rs `remote_baseline_roundtrips_through_sqlite`, `corrupt_baseline_json_is_typed_corruption`, `pre_baseline_row_decodes_to_none_and_merges_under_the_fallback`; **new golden pin** |
| 9 | `LocalOp` op tags | `codec.rs:221–243` (`op_kind_to_text`) + `op_from_columns` `:328–379` | serde derive **without** `rename_all` (`outbox.rs:93` — PascalCase wire tags, *not* the storage form); no text pair | **A** — new `LocalOpTag` descriptor enum + `LocalOp::tag()` + `text_form`/`parse_text_form` (D4) | codec `op_columns_roundtrip_every_variant`, `unknown_op_tag_is_rejected`, the two ref-delete tests, `missing_required_op_column_is_rejected` (incl. the `rename_board` legacy rejection); **new golden pins** (all 19 tags); roundtrip.rs outbox order/ref-delete/reapply tests |
| 10 | `SyncPhase` | `codec.rs:383–390` (encode) + `sync_phase_from_columns` `:413–435` (decode + dangling-error guard) | serde derive without rename_all (`state.rs:153–167`); no text pair | **A** — `text_form` + invariant-taking `parse_text_form(tag, Option<SyncErrorKind>)` (D7) | codec `sync_phase_roundtrips_all_forms`, `sync_phase_rejections` (covers all three dangling guards — F5-01's extension already landed), `no_board_error_kind_roundtrips_through_the_codec`; **new golden pins** (the four phase tags); roundtrip.rs `delete_sync_status_roundtrip` |
| 11 | `SyncErrorKind` | `codec.rs:394–404` + inline arms `:422–431` | serde derive (`state.rs:171–188`); no text pair | **A** — plain `text_form`/`parse_text_form` pair (D7) | **new golden pins** (the seven kind tags); codec phase tests as above |
| 12 | `ValidatorKey` | `codec.rs:439–466` | **hand-rolled serde impls** `persistence.rs:144–171` — a *byte-identical duplicate* of the codec pair (`"boards"`, `"stacks:<u64>"`, `"archived_stacks:<u64>"`) | **A** — `text_form`/`parse_text_form` in persistence.rs; **both** the serde impls and the codec delegate (D3) — the audit's "serde can derive from the text form" realized literally | codec `validator_key_roundtrips_and_rejects_garbage` (literal `"archived_stacks:9"` already pinned); roundtrip.rs `per_board_validators_roundtrip`; **new domain-side serde-alignment pin** (`serde_json::to_value(key) == key.text_form()`) |
| 13 | `ReminderRef` | `codec.rs:471–482` | **the precedent**: `reminders.rs:303–342` (`text_form`/`parse_text_form`; the snooze-map key serde and the codec both delegate) | **L** — already done; untouched | codec `reminder_ref_text_roundtrips_and_rejects_garbage`; heavy roundtrip.rs reminder coverage |
| 14 | local ids (`Uuid`) | `codec.rs:206–217` | external `uuid` crate | **L** — id TEXT convention, not domain vocabulary (D9) | codec `uuid_text_roundtrips_and_rejects_garbage` |

**Not text-tag knowledge — deliberately out of the unification** (D9):
`OpColumns`/`op_to_columns` (`codec.rs:248–307`, the outbox schema's
column mapping), `op_payload_to_json` + the ref-delete payload decode
(`:313–319`, `:369–376` — pure serde delegation already),
`remote_*_to_columns`/`remote_columns_present_or_none`/`remote_col`
(`:487–542`, INTEGER columns), the `JournalPayload` pair (`:547–576`,
storage-owned blob around domain serde), `CodecError` (unchanged).

## 3. The clock-parse fold (F5-10's companion)

Add one helper to the existing `decode` module (`repo.rs:251`,
beside `decode::uuid`/`decode::remote_opt` and the D4-era remote
helpers):

```rust
/// An RFC 3339 clock column (`ck_*`, journal stamps, habit-log
/// clocks). `chrono::DateTime<Utc>::from_str` semantics, unchanged.
pub(crate) fn clock(text: &str) -> Result<DateTime<Utc>, RepositoryError> {
    text.parse().map_err(|_| RepositoryError::Corrupted)
}
```

Fold sites (all 22 inline `.parse().map_err(|_| RepositoryError::Corrupted)`
clock parses + the local closure), exact at `c21c0e9c`:

- `decode_stack` ×3 — `repo.rs:413–421` (`ck_title`, `ck_sort_order`, `ck_deleted`)
- `decode_task` ×13 — `repo.rs:473–521` (`ck_title` … `ck_deleted`, the 13 `TaskClocks` slots)
- `decode_label` ×3 — `repo.rs:547–555`
- task-label membership clock — `repo.rs:1056–1059`
- journal `created_at` — `repo.rs:1167–1168`
- habit-log `clock` — `repo.rs:1218–1219`
- the habit mapper's local closure — `repo.rs:683–687` (definition)
  and its five uses (`:689–693`: `HabitClocks`' `title`, `target`,
  `order`, `anchor_time`, `deleted`) — the closure dies; the five
  call sites become `decode::clock(...)?`

**Stays put** (different parse domains, D8): the habit row's
`NaiveTime::parse_from_str(.., "%H:%M:%S")` anchor (`repo.rs:674–679`),
the habit-log window's `NaiveDate` parse (`repo.rs:~1208–1211`), and
every non-clock `.parse()` (`u64` effort, `u32` count, uuid via
`decode::uuid`). The helper is concretely typed `DateTime<Utc>` —
no generics — so stage 3's edge-semantics question (leap seconds,
sub-second, offsets) has one answer: `chrono`'s `FromStr`, unchanged
at every site.

## 4. Decisions (every open question resolved)

### D1 — Importance/Urgency: a serde-name pair, distinct from the display vocabulary

- **Fact**: two vocabularies exist domain-side already —
  `LEVEL_TEXT_FORMS` (hyphenated, `entities.rs:149`: CLI parse, CLI
  render, UI labels) and the serde snake_case unit names (the derive's
  `rename_all`), which is what storage TEXT and the appdata sidecar
  JSON both carry (`appdata.rs:269–273` serializes via the derive).
  The codec mirror pins the serde name.
- **Edit**: a second shared table beside the first:
  `LEVEL_SERDE_NAMES: [&str; 5] = ["very_low", "low", "normal", "high", "very_high"]`,
  indexed by the existing `LEVELS` declaration order, with
  `Importance::serde_name() -> &'static str` /
  `Importance::parse_serde_name(&str) -> Option<Self>` and the
  `Urgency` twins (same shape as the existing pair at
  `entities.rs:188–197`). Codec delegates.
- **Load-bearing alignment pin** (domain-side test): for each of the
  five levels, `serde_json::to_value(level) == level.serde_name()` —
  the pair is proven aligned with the actual derive, not asserted
  parallel to it. Plus parse reversibility and one unknown → `None`.
- **Rejected**: delegating storage to the existing hyphenated
  `text_form` — changes stored bytes (format drift; would strand
  every existing DB row). **Rejected**: renaming the display
  vocabulary to snake_case — a user-visible CLI/UI text change, way
  outside a storage refactor. **Rejected**: leaving the two families
  un-unified — the mirror is exactly the duplication F5-10 names, and
  the serde-name table is 7 lines.

### D2 — Codec fns stay as the storage seam: bodies delegate, signatures and callers unchanged

All `codec::*_to_text`/`*_from_text` fns keep their names, signatures,
and visibility (`pub` in the `pub(crate) mod codec`, `lib.rs:70`);
their bodies become one-liners delegating to the domain pairs — the
`reminder_ref_to_text`/`reminder_ref_from_text` shape (`codec.rs:471–482`).
Consequences: `repo.rs` is untouched by the delegation commit (every
call site, including the `decode::` wrappers at `repo.rs:326–364`,
keeps calling codec fns); the diff lives in codec.rs + domain.
**Rejected**: deleting the codec wrappers and calling domain fns
directly from repo.rs — a 30-site repo.rs diff for zero semantic gain,
and it dissolves the module whose doc is the storage-form seam.

### D3 — ValidatorKey: one definition, three consumers

`persistence.rs` gains `ValidatorKey::text_form(&self) -> String` /
`parse_text_form(&str) -> Option<Self>` with the exact literals; the
hand-rolled `Serialize`/`Deserialize` impls (`:144–171`) delegate to
them (the `Deserialize` error message string stays verbatim —
`"invalid validator key"` — even though nothing asserts it), and the
codec pair delegates too. The codec's duplicate disappears; the
serde impl's duplicate disappears. Byte-identity by construction.
Domain-side test: serde round-trip through `serde_json` for all three
shapes + garbage rejection.

### D4 — LocalOp tags: a `LocalOpTag` descriptor enum; column assembly stays storage

- **Edit** (`outbox.rs`, beside `LocalOp`): a data-less mirror enum
  `LocalOpTag` with one variant per `LocalOp` variant (19);
  `LocalOp::tag(&self) -> LocalOpTag` (exhaustive `match` — a future
  op variant fails to compile until tagged, the F5-03 discipline
  applied at the vocabulary); `LocalOpTag::text_form(self) ->
  &'static str` (the 19 snake_case literals, single home);
  `LocalOpTag::parse_text_form(&str) -> Option<Self>`.
- `op_kind_to_text` → `op.tag().text_form()`. `op_from_columns`
  parses the tag once (`LocalOpTag::parse_text_form(tag).ok_or(CodecError)?`)
  and matches on the descriptor; the id-column requirements
  (`parse_id` calls) and the ref-delete payload decode stay exactly
  as today — which columns an op addresses is the outbox *schema*,
  storage-owned (and `op_to_columns` keeps its own exhaustive match;
  unifying the two storage-side matches into the descriptor is
  declined — D11).
- **Rejected**: `LocalOp::parse_text_form(tag) -> Option<LocalOp>` —
  impossible without the ids (variants carry them) and would drag
  column knowledge into domain. **Rejected**: adding
  `#[serde(rename_all = "snake_case")]` to `LocalOp` to make serde
  *be* the tag vocabulary — changes the serde wire bytes of
  `PendingOp` (journal payloads carry op ids only, but the derive is
  workspace-visible; a format change with no consumer need). The
  storage tag is defined once domain-side; serde stays serde.

### D5 — ReminderKind: discriminant-only pair, the codec's contract verbatim

`reminders.rs` gains `ReminderKind::discriminant_text_form(&self) ->
&'static str` (`"due_offset"`/`"at"`; the arm's *data* rides the
`at_epoch_seconds` column, so this is discriminant-only — documented
at the fn) and `ReminderKind::parse_discriminant_text_form(&str) ->
Option<bool>` with the codec's documented contract moved verbatim
(`Some(true)` = `at` arm, `Some(false)` = due-offset, `None` =
unknown). The `Option<bool>` is kept because the consumer
(`decode_reminder_rule`, `repo.rs:596–617`) branches on exactly that
shape and the arm data comes from separate columns.
**Rejected**: a `ReminderArm` descriptor enum (one consumer, two
arms — churn without a second user). **Rejected**: a full
`text_form`/`parse_text_form` pair carrying `ReminderOffset`/
`AtInstant` — the columns split data from discriminant by design
(0020); a lossless pair would misrepresent the stored form.

### D6 — RecurRule / RemoteTaskBaseline: thin serde wrappers move domain-side

`RecurRule::text_form(&self) -> String` =
`serde_json::to_string(self).unwrap_or_default()` and
`parse_text_form(&str) -> Option<Self>` = `from_str(..).ok()`;
`RemoteTaskBaseline` twins — the exact current codec semantics
(including the defensive `unwrap_or_default` on encode, documented
"never fires for these shapes; kept for byte-identity"), moved to
domain where the serde derives live. Codec fns and
`decode::remote_baseline` (`repo.rs:359–364`, ADR-0004 D-7 comment
kept) delegate. serde_json is already a domain dependency
(`Cargo.toml:13,27`). Golden pins (commit 1) capture today's exact
JSON for one fixed sample each.

### D7 — SyncPhase/SyncErrorKind: vocabulary + the Failed⇔error invariant go domain

- `SyncErrorKind::text_form(self) -> &'static str` /
  `parse_text_form(&str) -> Option<Self>` — the seven tags, plain pair.
- `SyncPhase::text_form(&self) -> &'static str` — the four tags
  (`Failed { .. }` → `"failed"`; the error kind is a separate column).
- `SyncPhase::parse_text_form(tag: &str, last_error: Option<SyncErrorKind>)
  -> Option<SyncPhase>` — takes the *already-parsed* kind and enforces
  the consistency invariant (a `Failed` phase requires the kind; a
  non-failed phase requires `None`). This invariant is intrinsic to
  the domain enum-with-data (a dangling error is not a valid
  `SyncPhase` state, not a storage artifact) — the same judgment that
  keeps `ReminderOffset`'s mints domain-side. The two-TEXT-column
  shape stays codec-side: `sync_phase_from_columns` keeps its
  signature, parses the error tag via the kind pair, and delegates
  the invariant.
- **Naming**: `parse_text_form` kept (the house pair name) with the
  second parameter documented; **rejected** `from_columns` (storage
  vocabulary in domain) and a `PhaseTag` descriptor enum (unneeded —
  unlike `LocalOp`, the phase tags map 1:1 onto parseable states once
  the kind is supplied).

### D8 — One concretely-typed clock helper; heterogeneous parses stay put

`decode::clock(&str) -> Result<DateTime<Utc>, RepositoryError>` as in
§3. No generics, no second helper for the `NaiveDate`/`NaiveTime`
sites (each appears once; a helper over one call site is negative
simplification). Fold list per §3.

### D9 — Families that stay, recorded

- **uuid pair** — the hyphenated-UUID TEXT convention is storage-wide
  (every id column), owned by the external `uuid` crate; no domain
  knowledge to centralize.
- **Structural codecs** (§2 closing paragraph) — column mapping,
  INTEGER remote columns, the journal blob: not text vocabularies.
- **The app crate's third copy of the phase tags**
  (`taskboard-app/src/commands/output.rs:232–233`, the status
  command's `"idle"`/`"syncing"`/… rendering): discovered during
  planning, **out of scope** (different crate, user-visible surface,
  zero storage impact). Recorded as a follow-up candidate: once
  `SyncPhase::text_form` lands, that mapping can delegate to it.
  Not filed by this chunk (stage 3 may mention it on #580's close).

### D10 — codec.rs module doc reworded to the delegation truth

The module doc (`codec.rs:2–9`) still claims the forms "mirror the
domain's serde wire shapes … without pulling serde into this crate" —
stale twice over (serde_json has been a storage dependency since the
journal blob; and after this chunk the forms are *defined* domain-side,
not mirrored). Reword: the stored TEXT forms are defined once,
domain-side (`text_form`/`parse_text_form`/serde-name pairs); this
module is the storage seam delegating to them plus the column-shape
codecs; unknown tags decode to `Corrupted` via `CodecError`; the DB
stays inspectable because the shared forms are plain string
vocabularies. Doc-only, rides commit 3.

### D11 — Declined micro-refactors adjacent to the moved code

Recorded so stage 2 doesn't drift into them: unifying
`op_to_columns`' column mapping with `LocalOpTag` (the two matches
have different exhaustiveness obligations and different change
reasons); reworking `decode::importance`/`urgency`/… wrappers (they
keep calling codec fns, which keep their signatures); touching
`remote_baseline`'s ADR-0004 comment beyond the one-line delegation
swap; extending the Kani harnesses (`domain/src/verify.rs`) to the
new pairs — optional follow-up, not this chunk (chunk 1.4 owns the
Kani lane this round).

### D12 — TDD shape: pins are green-on-arrival; the red step is the domain pairs

This chunk is behavior-preserving, so the classic red→green loop
applies where behavior is *new*: the domain-side pairs of commits 2's
fns are written test-first (the domain tests fail to compile/pass
before their impls). The byte-identity pins (commit 1) are green on
arrival by construction — they pin today's bytes — and become the
load-bearing "after" half of the protocol: stage 3's flip test
("mentally flip an encoder, a pin must fail") passes for every
family because every family has a literal golden pin on its encode
side (and unknown-input rejects on its parse side). The suites that
prove no collateral drift — roundtrip.rs's whole-state proptest (256
cases), the T7 corruption-class pins, and every family test — must
stay green **unmodified** through commits 2–4.

## 5. Byte-identity evidence protocol

1. **Commit 1 (before any behavior-adjacent edit)** adds golden
   literal pins to codec.rs's test modules, written against today's
   fns: importance ×5, urgency ×5, time_entry_kind ×2, task_kind ×2,
   reminder_kind ×2 (+ the `is_at` truth table), op tags ×19, phases
   ×4, error kinds ×7, recur golden JSON ×1, baseline golden JSON ×1;
   plus parse-side unknown-input rejects for the families whose
   rejects aren't pinned yet (importance/urgency/time_entry_kind/
   task_kind/reminder_kind unknowns). Already-literal families
   (insistence, validator keys, uuid, reminder_ref, op/phase
   rejections) are untouched.
2. **Commits 2–4** must leave commit 1's pins, codec.rs's existing
   tests, and `tests/roundtrip.rs` byte-for-byte **unmodified** and
   green. That is the before/after contract: same assertions, same
   literals, new delegation underneath.
3. **Grep audit** (stage 2 exit): after commit 3, every moved
   vocabulary literal (`"very_low"`, `"idle"`, `"create_task"`,
   `"archived_stacks:"`, `"due_offset"`, …) appears in production
   `src/` code exactly once — its domain impl arm; codec.rs retains
   literals only inside `#[cfg(test)]`. After commit 4:
   `grep -n 'parse()\.map_err\|\.parse()' repo.rs` shows only
   `decode::clock`'s body plus the recorded non-clock parses (§3
   stays-put list), and the `map_err(|_| RepositoryError::Corrupted)`
   count drops by ≥22.
4. The whole-state proptest and T7 pins are the belt-and-braces
   layer (decode equivalence over 256 randomized states); they are
   corroboration, not the byte-identity proof — the literal pins are.

## 6. Implementation order & commit breakdown

Every commit standalone and build-green (rebase-merge hygiene,
AGENTS §8). `SQLX_OFFLINE=true` for clippy/checks (sqlx macros).

| # | Commit | Content |
|---|---|---|
| 1 | `test(storage): the codec text-form golden pins` | the §5.1 pin set against today's fns (green on arrival; the refactor's contract) |
| 2 | `feat(domain): the shared text-form vocabulary for the stored families` | entities.rs level serde-name pairs + alignment tests (D1); reminders.rs ReminderKind discriminant pair + tests (D5); recur.rs + entities.rs baseline pairs + tests (D6); state.rs SyncErrorKind/SyncPhase pairs + invariant tests (D7); persistence.rs ValidatorKey pair + serde impls delegating + tests (D3); outbox.rs `LocalOpTag` + tests (D4). Test-first per D12. No storage imports (AGENTS §2). |
| 3 | `refactor(storage): the codec delegates the stored text forms to domain` | codec.rs bodies → one-line delegators (D2); `decode::remote_baseline` swap (D6); module doc reword (D10). Pins and roundtrip.rs unmodified and green. |
| 4 | `refactor(storage): the decode::clock helper folds the clock-parse sites` | the §3 fold, 27 sites → one helper. |

## 7. Verification (stage-2 exit criteria)

- Full AGENTS §10 gate block via `scripts/run-quality-gates.sh`
  (parallel-agent box): fmt, `clippy --workspace --all-targets -D
  warnings`, nextest, doc tests, `cargo deny`, the llvm-cov triple
  with quarantine + trailing rm.
- Byte-identity evidence per §5 (pins green unmodified; grep audits).
- `cargo doc -p taskboard-domain -p taskboard-storage-sqlite --no-deps`
  warning-free (the new fns' intra-doc links resolve — same-crate or
  domain-internal only).
- No CHANGELOG (dispatch: no user-observable change).

## 8. Risks

1. **Level serde-name pair reads as a second vocabulary** (D1). It
   is one — deliberately, because the display vocabulary is
   load-bearing elsewhere (CLI/UI). Mitigation: the name says serde,
   the doc points at `LEVEL_TEXT_FORMS`, and the alignment test pins
   the pair to the actual derive. Low.
2. **`LocalOpTag` can drift from `LocalOp`** when variants are
   added. Mitigation: `LocalOp::tag()` is an exhaustive match — a new
   variant breaks the build, not the vocabulary; the codec's
   variant-enumerating round-trip test breaks at runtime if a tag is
   mislabeled. Low.
3. **The clock fold touches 27 sites** in one commit. Mechanical
   (one typed helper, uniform call shape), clippy- and
   proptest-covered; stage 3's edge-domain question has a one-line
   answer (chrono `FromStr`, unchanged). Low.
4. **`origin/main` moves while this chunk implements** (it moved
   between the issue, the re-verification, and this plan — the
   coordinates in §2 are `c21c0e9c`-pinned). Stage 2 re-verifies
   touched sites before editing (the sibling chunk's practice).
   Medium probability, low cost.
5. **serde-impl delegation for `ValidatorKey`** touches a hand-rolled
   `Deserialize`. Byte-identical by construction (same strings, same
   parse order); the error message stays verbatim; the domain-side
   serde round-trip test pins all three shapes. Low.

## 9. Out of scope (recorded, not done)

- The app crate's phase-tag copy (`output.rs:232–233`) — D9; the
  domain pair landing here is its future delegation target.
- serde attribute changes anywhere (`rename_all` on `LocalOp`/
  `SyncPhase`/`SyncErrorKind`) — D4/D7; wire bytes are not this
  chunk's to move.
- Any stored-data migration — the dispatch forbids it; byte-identity
  is the contract.
- Kani harnesses for the new pairs — D11 (chunk 1.4 owns the Kani
  lane this round; a follow-up can mirror `parse_text_form`
  semantics like `verify.rs:297` does for `ReminderOffset`).
- The `LEVEL_TEXT_FORMS` doc comment's "JSON row" phrase
  (`entities.rs:146–148`) — possibly stale (the sidecar JSON carries
  the serde name, not the display name); not adjudicated here, zero
  behavior impact.
- `codec.rs`'s structural codecs and the `decode::` wrapper layer —
  D9/D11.

## 10. Size estimate

≈ 550–650 changed lines across 7 files (domain: entities.rs,
reminders.rs, recur.rs, state.rs, persistence.rs, outbox.rs; storage:
codec.rs, repo.rs), net ≈ +250 — roughly half the additions are the
domain-side tests; deletions ≈ 220 (codec bodies, the closure, the
persistence.rs duplicate, the inline clock parses). Effort **M** as
dispatched; the individual commits are S-sized and mechanically
reviewable.

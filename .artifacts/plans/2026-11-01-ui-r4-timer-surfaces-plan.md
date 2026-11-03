# [Plan] GUI Review Round 4, Chunk 9 — Timer Surfaces

- **Date**: 2026-11-03
- **Status**: PROPOSED — stage-1 plan for chunk 9 of the round-4
  dispatch. Branch `feat/ui-r4-timer-surfaces`, cut from
  `origin/main` (`d96e5cc6`; `scripts/worktree-doctor.sh` passed
  pre-creation, 212 G free against the 25 G floor; both gate chunks
  verified merged on `origin/main` — chunk 3 icon assets (`timer.svg`
  vendored, `IconKind.timer` declared at
  `ui/widgets/icon.slint:45-46` naming "chunk 9's setter") and chunk
  7 date/time entry (`the time combobox's pure layer` `db2e78d4`,
  `the picker mounts the combobox` `393a8828`, validity
  `123dad5b`). Decision gate: round-4 §2 decision 4 (backlog
  absorption) **CONFIRMED 2026-11-01** (origin plan §7-R6 records
  it; tripwire passed).
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.9 (D32–D34), §2
  decisions 4–5, §1 findings R4-F13/F23, §7-R6/R7; dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1/§10 (acceptance a–e; blanket acceptance rule; code-quality
  mandate per §1). Inherited rulings: DR-3 Part D (D-I6–D-I8) and
  the landed-timers plan's fences
  (`2026-10-27-timers-alarm-surface-plan.md` §2) — the
  minute-repaint fence ("no new `slint::Timer` — the minute repaint
  is the only clock") is **amended by round-4 §2 decision 5**, not
  lifted: the repaint *driver* gains a wall-second boundary behind
  `seconds_surfaces_active`; still no UI-held timer state machine,
  still no new `slint::Timer`. Issue #352 closes via this chunk
  (stage 3 posts ✅ DONE + PR link per origin §8).
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/` +
  its tests). Empty diff over `taskboard-domain/-state/-storage-sqlite/-sync-nextcloud/-app`
  (stage-3 review clause f). No new engine commands (existing
  `TimerCreate`/`TimerCancel` lanes only); no new `UiEffect` (no
  `EffectClass` matrix row); paused-clock journeys only.

---

## 0. Findings → mechanisms audit (what the code confirms)

All anchors read on this branch @ `d96e5cc6` (= `origin/main`).

| Fact | Anchor | Evidence |
|---|---|---|
| The timers section exists; header/rows/form as DR-3/3 landed them | `ui/compositions/timers.slint` | header `:59-76` (caption + count + awaiting badge), rows `:78-95` (ListRow + cancel IconButton), form `:101-128` (label + when TextField, `create-accepted(string, string) -> bool` `:34` — the rule-add return convention), commit seam doc `:32-34` |
| The rows carry exactly two strings today | `TimerRowModel` `timers.slint:21-24` | `{ label, countdown }`; `countdown` is pushed from `countdown_phrase` (the minute bucket) at `models.rs:589` |
| The mount forwards only four of seven section properties | `ui/views/today-view.slint:113-119` | forwards `timers`, `timers-awaiting`, `cancel-clicked`, `create-accepted` — **not** `when-edited` / `when-validity` / `when-error-hint`, although the TodayView root declares and app-window feeds them (`today-view.slint:69-71`, `app-window.slint:333-335`) |
| ⇒ the r4-D27 timers validity half is dead in the mounted UI | journeys `tests/journeys.rs:7438-7506` | `the_offset_and_timer_fields_flip_validity_with_their_grammars` invokes `invoke_timers_when_edited` on the **window root**, so the adapter seam is tested while the section's callback is never wired — the task-view twin *is* wired (`task-view.slint:647` `offset-edited(t) => root.rule-offset-edited(t)`), proving the omission is a bug, not a pattern |
| The one parser and its validity predicate exist | `presentation.rs` | `parse_timer_when` `:1153`, `parse_at_instant` `:1177` (RFC-3339 \| `YYYY-MM-DD` midnight \| `HH:MM[:SS]` today-rolled; tz-injected; DST-fold to `None`), `timer_when_validity` `:1313`, `TIMER_SYNTAX_HINT` `:1276`; the adapter stores `timer_when_draft` (`adapter.rs:251`), pushes the flavour per pass (`:2882-2895`), and commits through `install_timer_seams` (`:5534-5611`: when-edited `:5537`, create-accepted `:5548` — empty-label absorb, parse-miss absorb, draft stash + route, `:5580` cancel, `:5604` chip → `ViewToday`) |
| Chunk 8 landed the shared two-part formatters and named this chunk as the consumer | `presentation.rs:405-476` + commit `64717dcd` | `CountdownParts`/`countdown_parts` `:409-454` (structural d/h/m/s, clamped, re-summing round-trip), `countdown_text` `:456` (`"in {d}d {h}h {m}m {s}s"`, zero segments kept per F23), `iso_local_fire_time` `:469` (`%Y-%m-%d %H:%M:%S`, tz-injected); commit message: "chunk 9's D32 consumes, never re-adds" |
| `countdown_phrase`'s only consumers are the two D32 surfaces | `models.rs:589` (rows), `models.rs:1663` (chip) | after D32 both switch to the shared formatters ⇒ `countdown_phrase` (`presentation.rs:661`) + its bucket tests (`:2152-2165`) delete; `duration_bucket` survives (due/notification phrases) |
| The push passes host-local tz at the edge | `models.rs:1449` | `rule_row_texts(rule, subject_duedate, now, &chrono::Local)` — the timer push mirrors this (the untested-glue edge, D-P7) |
| The rule rows already render the two-part shape | `ui/compositions/rule-list.slint:19-25,161-164` | primary line + conditional `detail` RoleText; `models.rs:1449` projects `RuleRowTexts` — the timer row's anatomy precedent |
| The chip renders one Badge string, assembled Rust-side | `status-bar.slint:249-258`, `models.rs:1651-1667` | `"{label} · {countdown_phrase}"`; kiosk forces `None`; click routes `ViewToday` |
| The per-second repaint machinery exists and names its extension point | `lib.rs:529-655`, `adapter.rs:849-884` | `start_repaint_driver` drains signals/events, tracks wall-minute + `SecondTick::crossed` (`lib.rs:672-688`, fires exactly once per wall second **while active**); `compute_seconds_surfaces_active` `:849` gates on `!self.kiosk` + mounted seconds surfaces (focus overlay, task-view elapsed, At-rule rows) — its doc comment names the timers surfaces as the seconds-changing extension this chunk lands |
| The time combobox is surface-neutral and names this chunk | `ui/widgets/time-field.slint:1-19` | "the timer setter (chunk 9) and the tracking editor (chunk 10) mount the same widget"; intents `edited/accepted/stepped/picked` `:66-73`; quick-list flyout is widget-internal (underlay + FocusScope + Up/Down/Return/Escape, `:142-246`); push re-seed discipline `:90-94` |
| The combobox's pure layer exists and is reusable | `due_edit.rs` (chunk 7) + adapter push half `adapter.rs:928` | lenient parse, slot filter, nearest-slot seek, ±15 wrap — all pure, consumed by the date picker's mount; the setter mounts the same widget over a setter-scoped draft with **no new pure math** |
| The anchored-overlay anatomy to copy | `app-window.slint:456-525` (date picker), `:615-660` (palette) | full-window dismissal TouchArea first (right-click-down idiom `:469-474`), then the panel: flip above/below + clamp into the window (status bar included) `:486-497`; the anchor geometry arrives via a facts global published with `changed` handlers from `absolute-position` (`menu-facts.slint:28-37`, `task-view.slint:160-170`) |
| The insistence datum exists end-to-end; the GUI cannot express it | `reminders.rs:444-456` (`AdhocAlarm.insistence`), `messages.rs:303-310` (`CreateAdhocAlarm.insistence`), `router.rs:468-477` | the router arm hardcodes `insistence: taskboard_domain::Insistence::Normal` `:473` — only the CLI's `--insistence` reaches the `alarm` rung; the D-P1 deferral ("a picker with no command field to carry it is dead UI") is obsolete: the field landed with DR-3/2 |
| The escalation/alarm-window half of #352 is landed | `ui/alarm-window.slint`, `taskboard-app/src/escalation.rs:475-522` | always-on-top awaiting rows with Dismiss/Snooze/Cancel (`D-P11` shape), the driver parks with no armed timer while no `alarm` rung awaits, re-toast cadence + chime ride the pure ladder — nothing here double-lands (stage-3 clause: empty diff over `taskboard-app`) |
| Kiosk gate, three surfaces, model-side | `models.rs:565-600` (`push_timers`), `:1651-1660` (chip), journey J-K2 `journeys.rs:9244-9300` | rows emptied, awaiting zeroed, `timers-visible` false, chip hidden — pinned against non-empty timer state; the banner stays the appliance's only reminder surface |

## 1. The #352 remainder table (D34: DR-3 ruling vs landed code)

Every row is either landed before (with evidence), lands in this
chunk, or carries a recorded reason. Nothing double-lands.

| # | DR-3 ruling (D-I6–D-I8) | Status | Evidence / what lands |
|---|---|---|---|
| 1 | D-I6: today-view timers section (header, rows, awaiting pointer) | **LANDED** | `timers.slint` (whole file); `today-view.slint:111-119` top-of-page mount (D-P18) |
| 2 | D-I6: status-bar chip, nearest fire, click → section | **LANDED** | `timer_chip` `presentation.rs:1121`, `status-bar.slint:249-258`, chip seam `adapter.rs:5604` → `ViewToday` |
| 3 | D-I6: desktop-only, model-side | **LANDED** | kiosk gate `models.rs:574-592`; J-K2 pins `journeys.rs:9273-9300`; reaffirmed §6 here |
| 4 | D-I7: active rows — label, remaining time, cancel | **LANDED** | `timer_rows` `presentation.rs:1080` (canonical `(effective, id)` order, the domain walk), rows `timers.slint:78-95`, cancel seam `adapter.rs:5580` |
| 5 | D-I7: **insistence badge on active rows** | **OWED — lands here** | `AdhocAlarm.insistence` landed (`reminders.rs:453`) but `TimerRow` (`presentation.rs:1033`) drops it; rows render no badge. This chunk projects the rung into the row model and renders a badge (alarm rung only — normal is the silent default, D-I9's ladder) |
| 6 | D-I7: awaiting rows owned by the notification surface, anchor-aware Dismiss/Snooze/Cancel | **LANDED** | `awaiting_row_buttons` + the Part D repair (the landed-timers plan §3 Part D); `alarm-window.slint` mirrors the shape for the `alarm` rung |
| 7 | D-I7: awaiting-count pointer | **LANDED** | `awaiting_alarm_count` `presentation.rs:1106`, badge `timers.slint:72-75` |
| 8 | D-I7: snoozed alarms leave/reappear by derivation | **LANDED** | `alarm_occurrence` walk (domain); rows/chip re-derive per refresh |
| 9 | D-I7: creation affordance — label + `in`/`at` grammar | **LANDED** | form `timers.slint:101-128`, `parse_timer_when` `presentation.rs:1153` |
| 10 | D-I7: **creation insistence picker (default `normal`)** | **OWED — lands here** | deferred by D-P1 (no command field then); the field landed (`messages.rs:309`) and the router hardcodes `Normal` (`router.rs:473`). This chunk lands the picker in both affordances and threads the rung through `TimerDraft` → router (§5) |
| 11 | D-I8: `TimerCreate`/`TimerCancel` action layer, one-shot targets, UI holds no timer state | **LANDED** | registry + router `router.rs:468-481`, `install_timer_seams` `adapter.rs:5534-5611`; derived-per-refresh rows |
| 12 | D-I9 (DR-3/4's half): alarm window, escalation ladder, re-toast, chime | **LANDED** (predecessor chunk) | `alarm-window.slint`, `escalation.rs:475-522`; commits `112bb6b5`/`3d7feb7b` per D34's citation — untouched here |
| 13 | F23 (round-4): rows/chip show d/h/m/s + `@ ISO` fire time, live per second | **OWED — lands here (D32)** | §3: consume chunk 8's formatters; per-second gate term §6 |
| 14 | F13 (round-4): mouse-operable setter, text form survives | **OWED — lands here (D33)** | §4: the header popover |
| 15 | *(found in the seams, in-scope)* r4-D27 timers validity half dead at the mount | **OWED — lands here** | §2: the three missing forwards in `today-view.slint:113-119` + a mount-level regression journey |

## 2. The mount-forwarding fix (finding 15 — lands first)

Three lines in `today-view.slint`'s TimersSection block, the
task-view twin's exact shape (`task-view.slint:647`):

```slint
when-edited(t) => root.timers-when-edited(t);
when-validity: root.timer-when-validity;
when-error-hint: root.timer-when-error-hint;
```

Plus the regression journey that goes **through the section** (type
into the when-field via the harness's typing helper, then assert the
TodayView root property flips) — the existing journey's
`invoke_timers_when_edited` path stays (it pins the adapter seam) and
the new journey pins the wiring, killing the tested-but-dead class.
Small standalone commit so the rebase never mixes it with D32's model
growth.

## 3. D32 — the two-part countdown (rows + chip, live per second)

**No new formatter.** Chunk 8's `countdown_text` +
`iso_local_fire_time` are the renderers (its commit names this chunk
as the consumer); `countdown_phrase` deletes with its tests once both
consumers switch.

- **Row model**: `TimerRowModel { label, countdown, fire-time }` —
  `countdown` = `countdown_text(effective, now)`, `fire-time` =
  `format!("@ {}", iso_local_fire_time(effective, &chrono::Local))`
  assembled in `push_timers` (`models.rs:585-591`), mirroring the
  rule-row push's tz edge (`models.rs:1449`). The row anatomy mirrors
  `rule-list.slint`: primary line (label + mono countdown meta),
  conditional second line (`fire-time`) — `ListRow` is single-slot,
  so the row grows the rule-list's custom two-line shape inside
  `timers.slint` (ListRow retires from this composition only).
- **Chip**: `push_status_bar`'s timers half (`models.rs:1651-1667`)
  assembles `"{label} · {countdown_text} @ {iso}"`. Both parts per
  D32's plain reading. Recorded alternative (rejected): chip keeps
  the countdown only, the `@` instant lives on the rows — rejected
  because D32 names both surfaces and the chip is the always-visible
  one; Tier-4 checks legibility, the compact-breakpoint fallback
  (drop the `@` date part) is recorded, not pre-built.
- **Insistence badge (remainder #5)**: `TimerRow` grows
  `insistence: Insistence`; the model grows `alarm-rung: bool`
  (projected `== Insistence::Alarm`); the row renders a warning-toned
  badge only on the `alarm` rung — `normal` stays the silent default
  (the ladder's whole point, D-I9).
- **Per-second derivation is already owned by the driver** — the rows
  re-push per refresh; the only new mechanism is the gate term (§6).

## 4. D33 — the mouse setter popover

### 4.1 Anatomy (`ui/compositions/timer-setter.slint`, new)

Top-to-bottom in a raised, focus-held panel:

1. **Quick presets** — `SegmentedControl` (`ui/widgets/segmented-control.slint`)
   with the D33-pinned durations **1m / 5m / 10m / 25m / 1h**
   (`TIMER_PRESETS_MIN: [u32; 5] = [1, 5, 10, 25, 60]`).
   *Quick-fill, not auto-commit* (D-T6): picking a preset fills the
   timing part (§4.3's composer) and moves focus to the label field —
   a one-click timer would need a Rust-invented label, which D-P14's
   raw-label rule forbids and the CLI (label required) confirms.
2. **Label field** — `TextField`, "Timer label"; whitespace-empty
   stays absorbed at the seam (D-P14 unchanged).
3. **Custom row** — chunk 7's `TimeField` (minutes-of-day; its
   widget-internal quick-list/stepper grammar carries over verbatim)
   + a two-segment `SegmentedControl` **Today | Tomorrow** (the
   minimal day offset; "tomorrow 09:30" is the F13-cited example).
   Beneath it, a live caption rendering the composed fire instant as
   `@ YYYY-MM-DD HH:MM:SS` (the same `iso_local_fire_time` shape the
   rows show), plus the pushed `FieldValidity` flavour — the custom
   row rides chunk 7's per-keystroke validity wiring exactly as the
   text form does.
4. **Insistence picker** — `SegmentedControl` Normal | Alarm,
   default Normal (remainder #10's picker; D-T7 lands it in the
   popover *and* the inline form — the D-P1 drop-in spot).
5. **Commit row** — a "Set timer" button (plus the kbd hint): routes
   the same commit seam with the composed grammar text and the
   picker's rung. Escape closes (widget-internal, the action layer
   never sees it — the TimeField flyout's ruling).

Blanket acceptance rule: every element above ships a hover tip (label
+ description + keys where bound) and a tab stop; the panel is one
FocusScope with a declared walk order (presets → label → time row →
day → insistence → commit); TimeField's flyout stays non-tab-reached
per its own contract.

### 4.2 Anchored mount + dismissal (the date-picker overlay twin)

- The header gains the designated IconButton (`IconKind.timer`,
  chunk 3's asset; label "Set a timer") at `timers.slint:59-76`.
- Anchor geometry: a `TimerSetterFacts` global in
  `menu-facts.slint` (the `DatePickerFacts` twin: anchor-x/y/width/
  height), published with `changed` handlers from the button's
  `absolute-position` — the `task-view.slint:160-170` idiom.
- The panel mounts **app-window-level** (never inside the scroll
  page — a window-sized underlay inside the Flickable would scroll
  wrong; the date-picker lives at window level for the same reason):
  `in-out property <bool> timer-setter-open` on the window (pure
  presentation state, the `palette-open` precedent — no model push,
  no adapter round-trip), full-window dismissal TouchArea (clicked →
  close; right-click-down → close, the r4-D2 idiom), placement =
  below the anchor when it fits else above, both clamped into the
  window including the status bar (`app-window.slint:486-497`'s
  expression, x-clamped like `:488-489`).
- Dismissal grammar: outside click closes (nothing is committed —
  drafts are widget-local); Escape closes; a successful commit
  closes; a parse/DST miss keeps it open with the invalid flavour
  (the field-keeps-text convention, translated).

### 4.3 The composer and the exact grammar (one parser, end to end)

Pure, in `presentation.rs`, tz-injected like every sibling:

- `pub const TIMER_PRESETS_MIN: [u32; 5]` and
  `preset_when_text(minutes: u32) -> String` — `"in 1h"` for 60,
  `"in {m}m"` otherwise (both parse through
  `ReminderOffset::parse_text_form` verbatim).
- `compose_timer_at(minutes_of_day: u32, day_offset_days: u32, now,
  tz: &impl TimeZone) -> Option<String>` — resolves
  `(today + offset).and_time(H:M:0)` host-local via `tz` with the
  `.single()` fold (a DST-gap pick ⇒ `None` ⇒ the invalid flavour,
  never a guessed instant), then formats **`at <RFC-3339 with local
  offset>`** (`to_rfc3339()`).
- Why RFC-3339 text and not a direct `AdhocWhen`: the dispatch's
  acceptance (b) makes `timers-create-accepted` **the** commit seam —
  popover ≡ text form ≡ same command — so the popover commits by
  composing grammar text and invoking the same callback; the one
  parser between any commit and the engine is `parse_timer_when`
  (`presentation.rs:1153`), whose RFC-3339 arm takes it back to the
  exact instant with **no roll** — the roll-to-tomorrow rule stays a
  bare-time affordance only, and an explicit day choice is honored
  (Today + a past time = legal data that fires next tick, the
  domain's documented `At` semantics; recorded alternative —
  auto-flipping the day — rejected as a silent contradiction of an
  explicit control). The composer returning `None` cannot produce a
  commit (the flavour gates the button), so the funnel never emits a
  miss.
- The live caption + validity mirror re-derives per edit through the
  same push pass that serves the text form's `timer_when_draft` — the
  setter's edits feed the adapter's existing draft (one draft, two
  views); no second validity predicate (chunk 7's `timer_when_validity`
  is the only one).

## 5. Seam unification (acceptance b) + the insistence thread

- **One seam**: `timers-create-accepted` grows the rung:
  `create-accepted(string, string, bool) -> bool` (bool = the
  `alarm` rung — the two-rung enum's structural projection; the
  adapter maps it to `Insistence`). The section form passes its
  picker's state; the popover passes its own. Forwarding grows at
  `today-view.slint:67/118` and `app-window.slint:332/334` only.
- `TimerDraft { label, when }` grows `insistence: Insistence`
  (`router.rs:272`); `create-accepted` stashes it; the router arm
  (`router.rs:468-477`) reads the draft's rung and the hardcoded
  `Insistence::Normal` dies. Router test updated (draft-with-alarm
  routes `Insistence::Alarm`; the existing no-target pins stay).
- **Grep contract for stage 3**: exactly one call site parses timing
  text (`parse_timer_when` inside `install_timer_seams`); the
  popover adds zero parse paths.
- The inline text form gains its own `SegmentedControl` (Normal |
  Alarm, default Normal) beside the timing field — the D-P1 drop-in
  spot — so keyboard-only users keep parity; its value rides the same
  third parameter.

## 6. Per-second gate + kiosk reaffirmation (decision 5's amendment)

- `compute_seconds_surfaces_active` (`adapter.rs:849-884`) gains the
  timers term inside the existing `!self.kiosk` guard:
  `!crate::presentation::timer_rows(state, now).is_empty()` — the
  unfired set is the only thing whose text changes per second (an
  awaiting-only map renders clamped zeros + a fixed instant: static,
  no tick owed). The method takes `now` (its call site in the refresh
  already holds the pass's clock read); one derivation, no second
  list to drift. The doc comment's "timers surfaces" extension-point
  sentence is discharged.
- **Kiosk unchanged, three layers**: the model-side gates
  (`push_timers`, chip) stand untouched; the popover is structurally
  unreachable (its button lives inside the unmounted section); the
  new gate term sits under `!self.kiosk` (layer b, the test-pinned
  one). J-K2's pins hold as-is; the desktop-side per-second behaviour
  is journey-tested with the mock clock (below), never wall-clock.

## 7. Decision summary

| ID | Question | Decision |
|---|---|---|
| D-T1 | Which two-part formatters | Chunk 8's `countdown_text` + `iso_local_fire_time`, consumed — no third formatter; `countdown_phrase` deletes with its tests (its only consumers are the two D32 surfaces) |
| D-T2 | Row shape | `TimerRowModel` grows `fire-time` (+ `alarm-rung`); two-line row mirrors `rule-list.slint`'s primary/detail anatomy; ListRow retires from this composition |
| D-T3 | Chip content | Both parts (`label · countdown @ iso`), per D32's plain reading; compact fallback (drop the date) recorded, not pre-built |
| D-T4 | The custom row's grammar | The composer emits existing-grammar text — presets `"in {m}m"`/`"in 1h"`, custom `"at <RFC-3339 local-offset>"` — re-parsed by the one parser; explicit day honored (no roll); DST gap ⇒ `None` ⇒ invalid flavour |
| D-T5 | Popover mount | App-window-level (the date-picker overlay twin): `TimerSetterFacts` anchor global, window-level `timer-setter-open`, dismissal layer with the right-click idiom, flip + clamp; never inside the scroll page |
| D-T6 | Preset click semantics | Quick-fill + focus-label, not auto-commit — D-P14's raw-label rule forbids Rust-invented labels; the CLI's required label confirms |
| D-T7 | Insistence picker placement | Both affordances (inline form's D-P1 drop-in spot + popover); the rung rides the seam's third `bool` param → `TimerDraft` → router; the hardcoded `Normal` dies |
| D-T8 | The dead validity half | The mount forwarding fixes here (this chunk owns the timers surface); a through-the-section regression journey pins it |
| D-T9 | Per-second gate | `!timer_rows(state, now).is_empty()` under `!self.kiosk`; the driver stays the only clock; no new `slint::Timer` (decision 5's amendment, the minute-repaint fence otherwise intact) |
| D-T10 | Alarm rung badge | Warning-toned badge on `alarm` rows only — `normal` stays silent (D-I9's ladder shape); remainder #5 |

## 8. Test catalogue (TDD; red where the behaviour is new)

### A. Unit / property — `presentation.rs` + `router.rs` (pure tier)

- `compose_timer_at`: today (future) ⇒ today's instant; tomorrow
  ⇒ +1 day's instant; both round-trip through `parse_timer_when`
  (the composed text re-parses to the identical `AdhocWhen::At` —
  the one-parser property); a DST-gap pick in a `FixedOffset`-free
  constructed ambiguity folds to `None` (the `.single()` fold —
  structurally: `None`, never a wrong instant); a past-today pick is
  **Some** (explicit choice honored, no roll).
- `preset_when_text`: the five pinned durations' exact grammar texts;
  60 ⇒ hours form.
- `TimerRow` projection: the insistence rung is carried (alarm
  visible, normal carried as normal — no defaulting at the
  projection).
- Router: `TimerCreate` with an alarm-rung draft routes
  `Insistence::Alarm`; normal-rung and no-target pins as today
  (`router.rs:673-695`'s shape).
- Deleted with `countdown_phrase`: its bucket-boundary tests
  (`presentation.rs:2152-2165`) — the bucket behaviour itself stays
  covered by `due_phrase`/`notification_phrase`'s tests.
- Seconds gate (`compute_seconds_surfaces_active`, unit over the
  adapter): unfired alarm ⇒ true (desktop); all-fired/awaiting-only
  ⇒ false; kiosk with live alarms ⇒ false (the layer-b pin);
  no-alarms ⇒ false.

### B. Journeys — `journeys.rs` (wiring tier; paused/mock clock only)

- **The parity journey (acceptance b)**: create via the popover
  (open → preset → label → commit) and via the text form with an
  equivalent grammar → the captured `CreateAdhocAlarm` commands are
  `PartialEq`-equal (same `AdhocWhen`, same insistence) — one parser,
  one seam, proven.
- Popover grammar: custom row (time picked + Tomorrow) + commit ⇒
  `At(instant)` where the instant is tomorrow at the picked
  minutes-of-day (typed `AdhocWhen` match, never label text).
- Insistence: picker set to the alarm rung ⇒ the captured command
  carries `Insistence::Alarm`; normal default unchanged.
- Absorbed misses: whitespace-empty label (no command dispatched,
  popover stays open); DST-gap composition ⇒ invalid flavour, no
  command.
- Dismissal: scrim click closes (no command); Escape closes;
  successful commit closes; a parse-missed commit keeps it open.
- **Mount-forwarding regression**: type into the *section's* when
  field through the typing helper ⇒ the TodayView root validity
  property flips (kills the tested-but-dead class for the timers
  half; the rule-list twin stays as it is).
- D32 structure: after a create, `get_timers_model` rows carry a
  non-empty `fire-time` and a `countdown` that **changes** when the
  mock clock advances one second (per-second derivation, paused
  clock); the chip's pushed string grows the two-part shape
  (structural: contains-the-instant is *not* asserted textually —
  assert the chip property changed across the clock step and the
  visibility pins).
- Kiosk J-K2: existing pins hold unchanged (`journeys.rs:9273-9300`)
  + one extension: live alarms on kiosk ⇒ the seconds gate reads
  false (the §6 layer-b pin at journey level).

### C. Snapshots / goldens

- Any today-view golden whose fixture grows timers: regenerate
  deliberately (the row-model growth changes the tree shape — the
  R4-risk §7-R4 discipline); alarm-free fixtures untouched.
- The StateEngine's insta snapshots: untouched (no state diff —
  the crate fence).

### D. Review greps (stage 3's clauses, listed for the implementer)

- `grep -rn "parse_timer_when" crates/taskboard-ui-slint/src/` ⇒
  the definition + `timer_when_validity` + exactly one call site
  (`install_timer_seams`).
- `grep -rn "countdown_phrase" crates/taskboard-ui-slint/` ⇒ zero
  hits after §3.
- `grep -rn "Insistence::Normal" crates/taskboard-ui-slint/src/router.rs`
  ⇒ zero hits (the rung comes from the draft).
- `git diff --stat` over the five non-UI crates ⇒ empty.

## 9. Commit slices (stage 2; standalone, rebase-clean)

1. `fix(ui-slint): the timers mount forwards the validity half` —
   §2, with its journey.
2. `feat(ui-slint): timer rows and chip speak the two-part countdown` —
   §3 + §6's gate term + unit tests + goldens regen +
   `countdown_phrase` deletion.
3. `feat(ui-slint): the timers form carries insistence` — §5's
   picker/draft/router thread (inline form mount + seam param +
   router test).
4. `feat(ui-slint): the timers header grows the mouse setter` — §4's
   composer + composition + anchor facts + seams + journeys.

CHANGELOG (`Unreleased`): Added (mouse setter, insistence picker in
the GUI, alarm-rung badge), Changed (two-part countdown rows/chip,
per-second tick while timers count). No push, no PR — stage 3 ships.

## 10. Risks & recorded probes

- **R1 — per-second refresh cost**: the gate term makes any unfired
  timer a full `refresh_forced` per second (decision 5's accepted
  shape; already the rule-row/focus-overlay cost). Tier-4 measures
  1 Hz on the netbook-class reference if available (origin §7-R7);
  the gate is already minimal (awaiting-only sets don't tick).
- **R2 — `models.rs` rebase friction** with the backlog render lane
  (2.1 `#464` / 4.1 `#461`): expected per dispatch §1; standard
  rebase dance.
- **R3 — the seam signature change** touches every
  `invoke_timers_create_accepted` call site in the journeys:
  mechanical, one slice.
- **R4 — chip legibility** with `label · countdown @ iso` on narrow
  windows: Tier-4; the compact fallback (drop the date part) is the
  recorded response, not pre-built.
- **R5 — DST-gap composition**: folded to the invalid flavour; the
  hint copy stays `TIMER_SYNTAX_HINT` (the grammar's honest refusal;
  no new copy constants).
- **R6 — `countdown_phrase` deletion**: verified zero consumers
  outside the two D32 surfaces + its own tests (grep in §0); if a
  consumer appears during rebase, keep the fn and record the
  deviation.

## 11. Tier-4 checklist (manual, on the reference machine)

- **Popover feel**: anchor placement at window corners + narrow
  widths (flip + clamp, never under the status bar); dismissal layers
  (outside click, right-click, Escape, commit-success); walk order
  and focus hand-off (preset → label focus; TimeField flyout's
  Up/Down/Return inside the popover); hover tips on every element.
- **Tick smoothness**: one unfired 90s timer — rows and chip advance
  once per second, no visible lag/jump at 1 Hz; the awaiting-only
  state does not tick (CPU observable in the driver logs).
- **Kiosk spot-check**: timer created via CLI on a kiosk run — no
  section, no chip, no tick (J-K2's live twin).
- **Legibility**: the two-part chip at the compact breakpoint; the
  alarm-rung badge tone against the row.
- **Parity sanity**: the same timer created via popover and via the
  typed grammar fires at the same instant (the journeys prove the
  command; this proves the feel).

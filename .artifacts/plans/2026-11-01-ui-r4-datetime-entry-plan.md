# [Plan] GUI Review Round 4, Chunk 7 — Date & Time Entry

- **Date**: 2026-11-03
- **Status**: PROPOSED — stage-1 plan for chunk 7 of the round-4
  dispatch. Branch `feat/ui-r4-datetime-entry`, cut from `origin/main`
  (58889421; `scripts/worktree-doctor.sh` passed pre-creation, 183 G
  free against the 25 G floor; both gate chunks verified merged —
  chunk 5 PR #585 rebase-merged 2026-11-02, chunk 6 PR #589
  rebase-merged 2026-11-03T00:49Z). Decision gate: round-4 §2
  decision 3 (hybrid time combobox) **CONFIRMED by the user
  2026-11-01** (recorded in the origin plan header; tripwire passed).
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.7 (D22–D28), §2 decision
  3, §1 findings R4-F18/F19/F20/F21/F22/F27, §7-R4; dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1/§8 (acceptance a–g; blanket acceptance rule; code-quality
  mandate per §1).
- **Inherited seam contract** (binding; chunk-6 stage-1 plan §3.6):
  (1) the `DatePicker` chain head stays **non-masking**; its Escape
  contract (clean close through `ModeNormal` from `ViInsert`) is
  pinned and preserved; (2) the field phase's masking is **exactly
  D20** — no new per-field engage seam (a new one is a review
  finding); (3) the picker phase keeps the head-over-insert chain
  (`[DatePicker, ViInsert, Global]`, `shell.rs:76-88`) and the
  disengage pull's grid exclusion (`adapter.rs:470-473`) — the
  two-phase ladder arms on `date_picker_grid`, **not** on new mode
  writes; (4) the entry journey starts at chunk 6's activation
  reservation (emacs Return on the focused due cell →
  `due-edit-opened`).
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/`).
  `taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`, `taskboard-app`: **empty diff** (review
  focus g). `taskboard-domain/src/reminders.rs` is READ-ONLY — the
  offset grammar is consumed, never edited. Non-goals: no reminder
  rows/kind work (chunk 8), no timer surfaces beyond the validity
  wiring on the existing form fields (chunk 9 owns the mouse setter),
  no tracking surfaces (chunk 10), no stored-instant migration (§7).
- **All `file:line` anchors are this worktree at 58889421**; the
  dispatch prompt's anchors predate chunks 5/6 and were re-derived —
  every one below was read, not carried over.

## 0. Findings → mechanisms audit (what the code confirms)

- **F18 (picker flips above↔below while paging months)** — root
  cause confirmed as a live-height feedback loop. The overlay's
  `y` binding re-tests fit on every property evaluation
  (`app-window.slint:474-480`: below when
  `anchor-y + anchor-height + space-1 + self.height <= status.y`,
  else above), and `self.height` is content-derived
  (`date-picker.slint:63` `height: panel.preferred-height`), while
  the grid's row count varies per month — `grid()` pads to a
  multiple of 7 (`due_edit.rs:315-326`), so the model push renders
  4/5/6 rows (`models.rs:1360-1394` chunks by 7). Month paging
  changes the row count → the height → the fit test → the panel
  teleports. The journeys pin exactly these variable shapes
  (`journeys.rs:7242` `(5, 35)`, `:7266` `(6, 42)`).
- **F19 (+/− steppers dead while the time is empty)** — root cause
  confirmed: `step_time` returns early when `self.time` is `None`
  (`due_edit.rs:247-250`), and `open()` seeds the time only from a
  non-midnight anchor (`due_edit.rs:197-206`,
  `filter(|t| *t != NaiveTime::MIN)`). Typed text never feeds the
  picker: `on_task_date_edited` writes only `due_buffer`/`due_echo`
  (`adapter.rs:4753-4766`). The pin
  `date_only_picker_stays_midnight_until_a_time_exists`
  (`due_edit.rs:611-620`) asserts the inertness — it encodes the
  bug's half (steppers do nothing) together with the desired half
  (no phantom `00:00` display), hence the reconciliation in §2, not
  a weakening.
- **F20 (junk text survives click-away)** — root cause confirmed as
  two missing seams. The `DueEditor` forwards no `focus-lost` seam
  (`due-editor.slint:43-60` — the TextField has one,
  `text-field.slint:79/104-106`, unused here), and the settle
  taxonomy clean-closes `Flow::EditDate` without committing
  (`view_state.rs:1678` `Flow::EditDate { .. } => self
  .end_edit_date()`), so a refusal on Return keeps the buffer
  (`adapter.rs:4776-4777`) and clicking the card background only
  drops the picker overlay — the editor and its junk stay armed
  (`adapter.rs:4815-4825`). Note the recorded Slint negative: a
  `TouchArea` click never moves focus (`text-field.slint:76-78`),
  so the card-background click can never ride the focus-loss seam —
  it needs its own settle path (§3).
- **F21 (emacs navigation in the picker; selection reflected in the
  field)** — root cause confirmed: the `DatePicker` context rides
  the chain head exactly while the *grid* holds focus
  (`shell.rs:76-88`, `adapter.rs:4714-4727`), the default-open keeps
  focus on the field (`adapter.rs:4879-4883` `open_due_picker(window,
  false)`), grid cursor moves mutate `due_picker` but never write
  `due_buffer` (`adapter.rs:4891-4918` vs `:4758`), and Enter
  (`PickerCommit → Return`, all three presets — `presets.rs:245`,
  `:457`, `:589`) commits immediately. The user's described flow —
  walk to due, Enter, choose a day with C-p/n/f/b, type a time,
  Enter — is unreachable: after grid-Enter the editor is already
  closed; there is no phase that returns to typing.
- **F22 (mouse time entry)** — decision 3 (CONFIRMED): the hybrid
  time combobox, §5. Today the panel's time half is two IconButtons
  around a readout (`date-picker.slint:124-144`) — steppers only,
  no typed entry, no quick list.
- **F27 (syntax check on date/offset fields)** — the machinery
  exists and is reused, not reimplemented: the tri-state
  `FieldValidity` flavour (`text-field.slint:22-26`, border driver
  `:118-125`), the settings validator pattern (per-field predicate →
  flavour mapping, one source of truth — `settings_form.rs:62-124`,
  the scale precedent `:209-235`), and the two authoritative
  parsers: the due grammar `due_edit::parse` (`due_edit.rs:92-117`)
  and the domain `ReminderOffset::parse_text_form`
  (`taskboard-domain/src/reminders.rs:109-136`, READ-ONLY), already
  shared by the timer grammar's `in` arm
  (`presentation.rs:889`). None of the three fields consumes them
  per-keystroke today.
- **D28 (zone unification)** — the split confirmed at the anchors:
  the due grammar commits UTC wall-clock (`due_edit.rs:12-30`
  module doc, `:105` `.and_utc()`, `:111`), the due picker seeds and
  commits UTC instants (`open()` `:197-206`, `commit_instant()`
  `:274-284`, `open_due_picker`'s `chrono::Utc::now().date_naive()`
  `adapter.rs:4943`), the echo renders UTC (`format_entry` `:158-164`,
  `at_time_suffix` `presentation.rs:182-189`), while the timer
  grammar parses host-local through an injected tz seam
  (`parse_timer_when` `presentation.rs:881-923`, called with
  `&chrono::Local` at `adapter.rs:5093`). For a UTC+1/+2 user every
  due-anchored reminder computes its offset from a due that *meant*
  local wall-clock but *got* UTC — the ranked root cause of F25's
  "fires hours late" (fixed structurally by chunk 8's `At` kind, but
  the due-anchored rows stay wrong until this lands).

## 1. D22 — the flip fix: six-row reserve (primary), freeze-at-open recorded

**Decision: ship the six-row reserve.** `grid()` pads the trailing
blanks to **42 cells / 6 week-rows always** (change the
`is_multiple_of(7)` loop at `due_edit.rs:322-324` into an
`extend` to 42 — or equivalently pad while `len < 42`). The panel's
preferred height becomes constant while open, the `y` binding's
`self.height` term stops moving, and the fit test cannot re-flip:
the loop is broken at the feed, not at the reader. This is also the
common calendar convention (every month renders the same grid
height; trailing-week blanks are empty cells that already render
nothing — `date-picker.slint:18`, `:178`).

- The models push and the Slint side need **no change**: the push
  chunks whatever `grid()` returns by 7 (`models.rs:1367`), and
  blank cells render empty.
- The journeys' grid-shape assertions regenerate deliberately:
  `(5, 35)` → `(6, 42)` everywhere; **every month now pins
  `(6, 42)`**.
- **The leap-February / 5-week pin**: unit tests pin all three
  source shapes under the reserve —
  (a) a 4-row month (February non-leap starting Monday: Feb 2028
  starts Monday? verify in-test via the API, not by hand: assert
  `grid().len() == 42` for a *known* 28-cell month, e.g. Feb 2021 —
  the test picks the month by construction with
  `NaiveDate::from_ymd_opt` and asserts the invariant, not the
  calendar trivia);
  (b) a 5-cell-row month (35 cells, e.g. the January 2030 the
  journeys already use) → 42;
  (c) a 6-row month (42 native, e.g. December 2029 from the
  journey) → 42 unchanged.
  The existing `grid_invariants` proptest gains: `grid().len() ==
  42` unconditionally, leading blanks < 7, trailing blanks ≥ the
  reserve slack.
- **Probe/record (stage 2, Tier-4)**: at the minimum viewport the
  panel is now taller for 4/5-row months than before. The `y`
  clamp (`app-window.slint:480`,
  `status.y - self.height - Theme.space-2`) already keeps it inside
  the window, degrading to overlap-the-field when the window is too
  short. Stage 2 records the min-viewport verdict; if the clamp
  proves worse than the disease (measured, not guessed), the
  **recorded fallback** is freeze-at-open: snapshot the placement
  side into an adapter-held `DatePickerFacts` field at open and pin
  `y` to it — no live re-test. The reserve ships first; the
  fallback lands only if the probe says so.

## 2. D23 — steppers seed 00:00; the midnight pin reconciled

**Decision: the seed is 00:00 (the design's recommendation).**
`step_time` on an unset time seeds `NaiveTime::MIN` and then steps:
the first `+` click yields 00:15, the first `−` click yields 23:45
**with the date following backward** (the wrap rule is unchanged —
a step is an instant move, `due_edit.rs:254-258`).

- "now" is recorded as the rejected alternative: it makes the first
  click's result depend on wall-clock time (unpredictable commit),
  it is not zone-free (the seed would change meaning across the
  D28 zone seam), and it makes the unit pins clock-dependent.
- **Implementation shape**: inside `step_time`, before the early
  return — `let time = *self.time.get_or_insert(NaiveTime::MIN);`
  then the existing math. `TIME_STEP_MINUTES` stays 15.
- **The reconciled pin** (`due_edit.rs:611-620`): the test
  `date_only_picker_stays_midnight_until_a_time_exists` is rewritten
  as `stepping_an_unset_time_seeds_midnight_then_steps`: time is
  `None` before; after `step_time(1)` time is `Some(00:15)` and
  `commit_instant()` is the cursor at 00:15; after `step_time(-1)`
  from a fresh seed it is 23:45 on the previous day. The desired
  half of the old pin survives as a separate assertion:
  `commit_instant()` stays the cursor's midnight **while** time is
  `None` (no click yet) — the pin's *contract* ("no phantom time
  exists until something creates one") holds; only the "steppers
  are inert" clause dies, because that clause was F19.
- **Display stays `--:--` while `time == None`** (`models.rs:1459-1462`
  unchanged) — a stepper click is exactly what makes a time exist,
  and the readout flips from `--:--` to the stepped value.
- **Typed text seeds the picker live** (the D25 sync, §4): a
  parseable buffer with a time part sets `picker.time` per
  keystroke; this is the *other* way a time comes to exist, and the
  pin covers it via the sync helper's property.

## 3. D24 — click-away closes the interaction: the commit/discard rule

**The rule (round-3 R1 extended to date fields):** when the armed
date editor's field loses focus *or* the user clicks away onto the
card background — non-empty **and parseable** buffer commits (the
same `commit_date` funnel as Return); **empty** or
**non-conforming** discards (no command; the flow settles; the
prior value survives; the field clears because the editor
collapses). F20's junk can no longer survive any exit.

**One funnel, three seams.** A single adapter helper
`settle_date_editor(a, window) -> bool` (true = committed) owns the
rule; the seams are:

1. **Field focus loss** — `DueEditor` gains a `focus-lost()`
   forwarding seam (`due-editor.slint`, the TextField's existing
   callback at `text-field.slint:79`), the window forwards it, the
   adapter calls the funnel. A TouchArea click never moves focus
   (the recorded negative), so this seam fires on real focus steals
   only: Tab, clicking another focusable, programmatic steals (see
   the suppression guard below).
2. **Card-background click (the dismissal layer)** — the
   `date-picker-dismiss-requested` handler (`adapter.rs:4815-4825`)
   stops being picker-only: it runs the same funnel *and* drops the
   picker overlay. The round-3 D10 "outside click drops the picker
   only" rule is superseded by D24 (the design says so; the
   `the_picker_overlay_dismisses_without_disturbing_the_editor`
   journey reworks deliberately). In picker phase the buffer
   mirrors the picker (§4), so a background click commits the
   current selection — click-away ≡ done, consistent with the
   field-phase rule.
3. **Return** — unchanged shape (`on_task_date_accepted`), but a
   refusal now *additionally* shows the invalid state (§6) instead
   of only keeping the buffer silently.

**Exact state transitions on `Flow::EditDate`** (the enumeration
stage 3 reviews against):

| Trigger | Buffer | Command | Flow | Buffer/echo/picker after | Focus |
|---|---|---|---|---|---|
| focus-lost / click-away, parseable non-empty | e.g. `2030-02-28` | `UpdateTask{duedate:…}` via `commit_date` | `end_edit_date()` (outcomes applied) | cleared by `commit_date` (`adapter.rs:4977-4979`) | editor collapsed |
| focus-lost / click-away, empty | `""` | none | `end_edit_date()` | cleared (the enforce invariant, `adapter.rs:901-913`) | editor collapsed; **the stored due survives** — empty on focus-loss never means clear |
| focus-lost / click-away, junk | e.g. `soon` | none | `end_edit_date()` | cleared; prior value re-seeds on next `begin_date_edit` (`adapter.rs:4876`) | editor collapsed |
| Escape, picker phase | mirrors picker | none | stays armed | unchanged | one-shot grid-focus flag → **field takes focus** (phase step, §4) |
| Escape, field phase | any | none | stays armed | unchanged | grid-focus flag set → **grid takes focus** (phase step, §4) — the *second* Escape (picker phase) clean-closes |
| Escape, picker phase, second press | mirrors picker | none | `due_picker = None` + `discard_edit()` (`adapter.rs:1155-1158`, the clean close) | cleared | editor collapsed; **R-c preserved** (the view Escape still ends the editor) |
| leaving effect (view switch, foreign flow) | any | per `settle_flow` | `end_edit_date()` (`view_state.rs:1678` — **unchanged**) | cleared | — |

Two deliberate asymmetries, recorded: (a) empty-on-focus-loss
discards rather than committing the grammar's "clear" — a stray
focus steal must never nuke a due date (Enter with an empty buffer
still clears, `adapter.rs:4778`); (b) `settle_flow`'s EditDate arm
stays a clean close — the r2 rule "never commits on an unrelated
settle" keeps governing view switches; the R1 rule lives only on
the field's own seams (1)–(2).

**The suppression guard**: the Escape phase-step (row 4→field focus
hand-over is Enter; the *field→grid* step is Escape/Down) moves
focus programmatically, which fires the field's `focus-lost`. The
adapter sets a one-shot `due_focus_steal` latch before handing
focus to the grid (the `walk_reentry` latch precedent,
`adapter.rs:424-426/458-460`); the funnel consumes the latch and
skips the rule for that steal, leaving the half-typed buffer intact
— Escape stepping back a phase must not commit or discard the text
the user is mid-way through. Enter's field hand-over does not need
it (the field *gains* focus; nothing is lost).

**Cannot fight chunk 6's insert engagement**: the funnel runs only
under an armed `Flow::EditDate` (it reads the flow first — same
guard `commit_date` has, `adapter.rs:4958-4961`); every other
field's focus loss resolves through the D20 pull exactly as chunk 6
shipped. The journeys include a foreign-field focus steal with the
due editor armed (the description field takes focus → the due
commits/discards per the rule; the description's own engage flow is
untouched).

## 4. D25 — the two-phase machine + the live sync helper

### 4.1 The phases and the flag

Two phases inside the armed editor; **no new mode writes, no new
flow datum** (the seam contract, item 3):

- **Picker phase** — the grid holds Slint focus;
  `shell.date_picker_grid == true`; chain
  `[DatePicker, ViInsert, Global]` (`shell.rs:80-86`); C-p/C-n/
  C-f/C-b (emacs), arrows (standard), h/j/k/l (vi) drive the cursor
  via the existing `picker.*` rows; the month follows the cursor
  (round-3 D12, unchanged).
- **Field (text) phase** — the field holds Slint focus; the
  `DatePicker` context disarms via the existing grid-focus mirror
  (`adapter.rs:4714-4727`); masking is **exactly D20** — the field
  is a mounted TextField publishing into `EditingFacts.text-focus-count`,
  the insert chain holds, C-f/C-b are caret movement, printables
  reach the field. Zero new engage wiring (contract item 2).

The phase *is* `date_picker_grid` — the live mirror chunk 3 (r3)
installed. The ladder arms on it; nothing else stores phase. The
in-panel tab stops (the month IconButtons, the §5 combobox parts)
sit between the phases: while one holds focus the mirror reads
false, so the ladder treats them as field-side (Escape returns to
the grid first, then closes). Recorded as accepted behavior.

### 4.2 The Enter/Escape ladder

| Stroke | Picker phase (grid focused) | Field phase (field focused) |
|---|---|---|
| **Enter** | **phase hand-over, never commits** (review focus b): `apply_picker_key`'s `K::Commit` arm (`adapter.rs:4907-4911`) stops calling `commit_date`; it sets the one-shot field-focus flag (the `focus-on-mount` mechanism-A idiom run in reverse — a new `date-field-focus` one-shot pushed like `date_grid_focus`, the panel-side `changed` handler calls `due-field.focus()`) | the field's `accepted` → parse → commit or refuse-with-validity (§6) — unchanged funnel |
| **Escape** | the clean close (today's body, `adapter.rs:1155-1158`) — the flow table branches on `shell.date_picker_grid`: true → close | phase step back: set the grid-focus one-shot (`picker_focus_on_mount = true`, the existing Down-in-field path at `adapter.rs:493-496`) + the suppression latch (§3); buffer preserved |
| **Down** | (grid-side arrows already drive) | the existing intercept seam (`adapter.rs:487-498`): focus hand-over to the grid — this *is* the text→picker step |
| **day-cell click** | `set_cursor` + `commit_date` — **unchanged** (`adapter.rs:4829-4856`); the mouse path keeps its one-click commit, deliberately diverging from grid-Enter (the user's flow makes Enter the *typing* hand-over; the mouse never needed it). The r3 "click ≡ cursor+Enter" comment is reworded: click ≡ cursor+commit, direct. | same (clicks work in both phases; the panel is click-armed whenever mounted) |

The Escape row keeps contract item 1: Escape still resolves
`ModeNormal` from `ViInsert` everywhere (the `DatePicker` context
binds no Escape — `presets.rs:229-245/441-457/569-589` bind no
Escape row); only the flow-table arm gained the branch. R-c holds:
the view Escape ends the editor from either phase (two presses from
the field phase — the first steps back, the second closes; from a
panel tab stop likewise).

This is exactly the user's F21 flow: walk to the due cell, Enter
(chunk 6's activation reservation opens the editor default-open),
Down (or Escape) into the grid, C-p/n/f/b to the day, Enter to the
field, type the time, Enter to finish. The
`the_control_walk_reaches_fields_buttons_and_the_picker` journey
(`journeys.rs:6647`) extends to the full ladder; the emacs rows'
conflict contracts are chunk-6 material and untouched.

### 4.3 The sync helper (pure, `due_edit.rs`)

```rust
/// One bidirectional sync step (r4 D25). `Picker` source: the
/// buffer re-renders from the picker's selection (cursor moves).
/// `Buffer` source: a parseable buffer re-seeds the picker (typing).
/// Returns `None` when the pair is already consistent — the fixed
/// point the no-oscillation property pins.
pub enum SyncSource { Picker, Buffer }
pub fn sync_buffer_picker(
    buffer: &str,
    picker: &DuePickerState,
    source: SyncSource,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<(String, Option<DuePickerState>)>
```

Direction rules (pinned):

- **Picker → buffer** (`SyncSource::Picker`): buffer :=
  `format_entry_local(picker, tz)` (the local rendering, §7) —
  date-only when `time == None`, `date HH:MM` otherwise. Picker
  unchanged. Called from every `apply_picker_key` move and every
  day-cell *hover-less* path (day clicks commit, no sync needed),
  and on entering the picker phase (Down/Escape-step): **the picker
  wins on entry** — half-typed junk yields to the selection of
  record.
- **Buffer → picker** (`SyncSource::Buffer`): `parse_local`
  (§7) — `Ok(Some(instant))` → picker := state-of(instant): cursor
  = the local date, month follows, `time = Some(local HH:MM)` when
  the text carried `HH:MM` (an **explicit `00:00` counts — a typed
  time exists**, per D23's reconciled pin), `None` for date-only
  text. `Ok(None)` (empty buffer) leaves the picker untouched —
  clearing is a commit-time act, not a live one. `Err` (junk or a
  partial prefix like `2030-01-3`) leaves the picker untouched; the
  validity flavour shows the state (§6). Called from
  `on_task_date_edited` per keystroke — **the field wins while
  typing**; and on entering the field phase (nothing rewrites the
  user's text).
- **The two-midnights convention** (recorded): `None` (date-only)
  and `Some(00:00)` (explicitly typed/stepped) commit the same
  instant but render differently (`date` vs `date 00:00`). The
  convention keeps the sync a fixed point *without clobbering
  visible text*: normalizing typed `00:00` back to `None` would
  rewrite the user's buffer on the next picker-side sync — rejected.

**The no-oscillation property** (proptest, the review's focus c):
for arbitrary `(buffer, picker)` pairs — including junk, empties,
partial prefixes, and both conventions of midnight —
`sync(src)(sync(src)(x)) == sync(src)(x)` for each source (one-step
idempotence), and the round trip `Picker` then `Buffer` then
`Picker` converges: `sync(Picker)` after a `Buffer` step re-renders
exactly the buffer the `Buffer` step produced (the formatting is
`parse`'s inverse modulo the two-midnights convention, which the
property exercises explicitly). A manual loop-guard audit rides
stage 3 (the helper is the only writer of both sides; the callers
pass the fresh values back verbatim; no caller re-enters sync on
its own output within one pass).

**Where the calls land** (all adapter-side, the single writer):
`apply_picker_key` (every move arm, Picker source),
`on_task_date_edited` (Buffer source), `begin_date_edit` /
`open_due_picker` (initial seeding — the anchor instant → picker,
Picker source), the two phase hand-overs (Picker source on entering
the picker phase; no-op on entering the field phase).

## 5. D26 — the time combobox (`ui/widgets/time-field.slint`)

The CONFIRMED decision-3 widget, shared by this chunk (the date
picker's time half), chunk 9 (the timer setter), and chunk 10 (the
tracking-entry editor). **Surface-neutral API** — no due-specific
bits inside the widget.

### 5.1 Anatomy

One row, the replaced stepper row's footprint (`date-picker.slint:124-144`):

- **The typable field** — the `TextField` primitive (validity
  flavour §6; placeholder `HH:MM`; `--:--` rendered as the *value*
  echo while `time == None`, per D23's display rule).
- **Embedded steppers** — trailing − / + IconButtons (the
  chevron/minus vocabulary already imported); each press = ±15 min
  with midnight wrap (§5.3); click ≡ the picker's `TimeBack`/
  `TimeForward` codes via the existing `picker-key(9/10)` seams.
- **The dropdown toggle** — a trailing chevron-down IconButton
  opening the quick-list flyout (below the row; an anchored flyout
  in the panel's coordinate space — the panel is itself the
  window-level overlay, so the flyout is a plain conditional
  sibling *inside* the panel; no second window-level layer, no
  dismissal-layer interaction: clicking elsewhere in the panel
  closes it via its own outside-click TouchArea, mirroring the
  panel's pattern at panel scale).
- **The quick list** — 96 quarter-hour slots (`00:00`…`23:45`),
  `ListRow`-rendered in a scrollable column, scrolled to the
  nearest slot of the current value on open (00:00 when unset),
  filter-as-you-type (§5.4), click-to-pick.

Blanket acceptance (dispatch §1): every interactive part carries a
hover tip (label + description + hint keys where bound), is a tab
stop (the input, −, +, the toggle; the flyout is not tab-reached —
it is an open-state list navigated by its own Up/Down), and
dispatches through callbacks — **no inline side effects in the
widget**.

### 5.2 The lenient parse table (pure, `due_edit.rs`)

`pub fn parse_time_lenient(text: &str) -> Option<NaiveTime>` —
the widget's parser, zone-free, total on arbitrary input:

| Input | Result | Note |
|---|---|---|
| `9`, `09` | 09:00 | hour only |
| `930` | 09:30 | 3 digits: H + MM |
| `0930` | 09:30 | 4 digits: HH + MM |
| `9:45`, `09:45` | 09:45 | the strict form |
| ` 9:45 `, `9:45` | 09:45 | trimmed |
| `9:5` | refuse | minutes need two digits (deliberate leniency boundary) |
| `2450`, `24:00` | refuse | hour > 23 — **never clamp** (the grammar's rule) |
| `9:60`, `9x`, `` | refuse | — |

Rules: digits only after trim; 1–2 digits = hour; 3–4 digits = last
two are minutes, the rest the hour; the `:` form splits on the
colon; every field out of range refuses. Seconds are not the
grammar. Property: total — never panics, never clamps (the
`junk_never_panics` shape).

`pub fn step_minutes(time: NaiveTime, n: i32) -> NaiveTime` — the
wrapping ±15 math generalized out of `DuePickerState::step_time`
(which keeps its date-following behavior and delegates the wrap to
the helper — one implementation, two callers; chunks 9/10 consume
the helper directly).

`pub fn quarter_slots() -> [NaiveTime; 96]` or a `slots()` Vec —
the quick list's pure source.

### 5.3 Stepper semantics

±`TIME_STEP_MINUTES` (15), `rem_euclid(24*60)` wrap, no date (the
widget has none — the wrap is wall-clock-only here). Inside the
date picker the *adapter* keeps the date-following behavior: the
picker's `step_time` remains the instant-move (the existing
`due_edit.rs:254-258` date-follow stays; only the wrap math
deduplicates). Stepping an unset value seeds 00:00 first (§2) —
the widget exposes `stepped(n)` and the adapter applies the seed+
step; the widget never holds the None/seed decision (it renders
what it is pushed).

### 5.4 Quick-list behavior

- **Generation**: `slots()` — 96 rows, static.
- **Scroll-to-nearest**: on open, the list scrolls so the slot
  nearest the current value (or 00:00) is visible; the nearest slot
  is highlighted (the cursor-ring visual language).
- **Filter**: digit-prefix — strip non-digits from the typed text;
  a slot matches when its digit rendering (`"0930"` for 09:30)
  starts with the typed digits (`9` → all 09-hundred slots… refined:
  `"9"` matches `"0900"`/`"0915"`/…/`"0945"` *and* `"1900"`-family?
  **Pinned: leading-zero-insensitive prefix on the 3-or-4-digit
  canonical form** — the typed digits `d` match a slot `s` when
  `s.digits.starts_with(d) || s.digits_no_leading_zero.starts_with(d)`
  (e.g. `9` matches `09:00` via the de-zeroed `"900"`, `930` matches
  `09:30`, `0` matches everything starting `0…`). Empty text = all
  96. The exact predicate is one pure function with its own truth
  table — no second parser (review focus f).
- **Commit**: clicking a slot, or Enter on the highlighted slot,
  fires `picked(minutes_of_day: i32)` — the adapter sets
  `picker.time` and runs the Picker-source sync (the date field's
  buffer visibly gains/updates its `HH:MM`). Escape closes the
  flyout only (the flyout's own FocusScope binds Escape locally to
  close — it is a *widget-internal* Escape, not a keymap context;
  the action layer never sees it; the picker's clean-close Escape
  contract is untouched). Wheel support rides the ListView's native
  scrolling (the decision-3 "where the backend provides it" clause).

### 5.5 Keyboard operation & walk stops

The combobox inside the date picker participates in both phases:

- **Picker phase**: the grid holds the `DatePicker` context; Tab
  from the grid walks the panel (month buttons → the combobox's − /
  field / + / toggle, the existing widget-level walk the journey
  already exercises for the month buttons, `journeys.rs:7260-7268`).
  Return on a focused stepper/toggle activates it (widget-level).
- **Field phase**: the date field holds focus (D20 masking); the
  combobox is Tab-forward from it (the natural panel order). Up/
  Down on the date field stay the grid hand-over (§4.2) — they are
  **not** stepper shortcuts inside the date picker (recorded
  divergence from the QTimeEdit convention; the date picker's
  Down-in-field grammar predates and wins). Chunks 9/10, whose
  fields have no grid, may bind Up/Down locally in the widget's own
  FocusScope — the widget accepts an optional `arrows-step: bool`
  (default false) so the date picker's mount keeps today's grammar
  and the standalone mounts get the convention. Open flyout: Up/
  Down move the highlight, Enter picks, Escape closes, Tab closes
  and moves on (widget-local, same mechanism).
- **Combobox Enter in the date picker**: the field's `accepted`
  fires the commit funnel (`commit_date` with
  `picker.commit_instant()`) — typing a time in the combobox and
  pressing Enter finishes the editor (the user's "type a time,
  Enter to finish", now reachable without ever entering the date
  field).

### 5.6 How the date picker mounts it

The stepper row (`date-picker.slint:124-144`) is **replaced** — no
orphaned steppers survive (review focus d). The panel's time row
becomes:

```slint
TimeField {
    time-text: root.time-text;          // pushed, --:-- while None
    validity: root.time-validity;        // pushed (§6)
    stepped(n) => root.picker-key(n < 0 ? 9 : 10);
    picked(minutes) => root.time-picked(minutes);   // new seam
    accepted(text) => root.time-accepted(text);     // commit funnel
}
```

The widget's own text buffer is widget-local (`in-out property`);
the adapter's `DuePickerState.time` stays the single truth — the
push is the mirror. The window forwards the two new callbacks
(`task-date-time-picked(i32)`, `task-date-time-accepted(string)`)
into the adapter: `picked` = parse-from-minutes → `picker.time` →
Picker-source sync → refresh; `accepted` = `parse_time_lenient` →
set time → `commit_date` (refuse → validity, buffer kept). Chunk 9
replaces its `HH:MM` parse sites with the same widget + parser; its
plan owns that swap (nothing here reaches into timers.slint's
grammar beyond §6's validity).

## 6. D27 — validity wiring (F27)

**The pattern** is the settings form's, verbatim in shape
(`settings_form.rs:62-124`): a per-field predicate over the
*authoritative* parser → a tri-state flavour → the border + error
line. **No second parser** — the predicates *call* `due_edit::parse`
/ `parse_time_lenient` / `ReminderOffset::parse_text_form` /
`parse_timer_when` (review focus f greps exactly this).

**Home**: `due_edit.rs` gains the due-field and time-field
predicates (they are the picker grammar's own flavour);
`presentation.rs` gains the offset and timer-when predicates beside
the parsers they wrap (`parse_timer_when` already lives there). The
slint-side enum is the existing `FieldValidity` (`text-field.slint:22-26`)
— pushed as `validity` properties; the error line is a conditional
caption below the field rendering a **presentation constant**.
Tests pin the tri-state only — never the copy (the house
no-text-assertions rule; the constants live in one module so a
wording pass never touches a test).

**The field list** (complete — stage 2 touches exactly these):

| Field | File | Predicate | Copy constant |
|---|---|---|---|
| due/scheduled editor field | `due-editor.slint:45-53` | `due_edit::field_validity(text, now, tz)`: empty → neutral; parse ok → valid; else invalid | `DUE_SYNTAX_HINT` |
| date-picker time combobox | `time-field.slint` (new) | `due_edit::time_validity(text)`: empty → neutral; `parse_time_lenient` ok → valid; else invalid | `TIME_SYNTAX_HINT` |
| reminder custom offset | `rule-list.slint:149-151` | `presentation::offset_validity(text)`: empty → neutral; `ReminderOffset::parse_text_form` some → valid; else invalid | `OFFSET_SYNTAX_HINT` |
| timers `when` field | `timers.slint:103-114` | `presentation::timer_when_validity(text, now, tz)`: empty → neutral; `parse_timer_when` some → valid; else invalid | `TIMER_SYNTAX_HINT` |

Wiring: per-keystroke `edited(text)` → adapter predicate → push the
flavour (the settings view's push pattern — `field_status` mapped
onto the model). The due field's echo row (`due-editor.slint:62-66`)
doubles as the error line: empty echo + invalid → render
`DUE_SYNTAX_HINT` in the danger tone (one row, no layout growth).
The other three gain a caption row each, same conditional shape.
The commit affordances compose unchanged (the checkmark fires
`accepted`, which refuses-and-keeps exactly as today; validity
makes the refusal *visible*, which is F27's point).

## 7. D28 — zone unification: parse local, store UTC, echo local

**The change**: the due grammar's zone becomes the **device-local**
zone; storage stays UTC instants. The grammar's "UTC is the
grammar's zone" doc (`due_edit.rs:27-30`) is rewritten; the two
cannot disagree by an offset any more *because both go through the
same injected tz*.

**The seam**: tz-injection, the `parse_timer_when` precedent
(`presentation.rs:884` `tz: &impl chrono::TimeZone`) — not a
hard-coded `Local`. Signatures:

- `parse(text, now, tz)` → local naive →
  `tz.from_local_datetime(&naive).single()` → UTC instant;
  `LocalResult::None` (spring-forward gap) and `Ambiguous`
  (fall-back) → **`DueParseError`** — the same refusal discipline
  the timer grammar chose (`.single()` → `None`), documented at the
  type: a wall-clock that does not exist locally is a refusal, not
  a guess. (This makes the DST pins total: every edge is a pinned
  accept or a pinned refuse.)
- `echo(text, now, tz)`, `format_entry(value, tz)` → instant →
  `value.with_timezone(tz)` → `%Y-%m-%d[ %H:%M]`.
- `DuePickerState` stays **zone-free** (cursor/month are
  `NaiveDate`s, time is `NaiveTime` — the *local* wall-clock by
  convention); the conversions live at the adapter seam:
  `open_due_picker` seeds from `anchor.with_timezone(&Local)`
  (local date/time) and `chrono::Local::now().date_naive()`; the
  commit funnel converts
  `Local.from_local_datetime(&cursor.and_time(time)).single()` →
  UTC — with the same refusal rule (a gap/ambiguous commit refuses:
  no dispatch, buffer kept, validity shows the state; recorded as
  the once-a-year cost of honesty — a commit that *guesses* a
  wrong instant is worse than one the user re-aims).
- `cell_kind`'s midnights become date-only comparisons (local
  dates on both sides — the UTC midnight construction
  `due_edit.rs:342-346` simplifies to date math).
- **Echo surfaces**: `at_time_suffix` (`presentation.rs:182-189`)
  gains `tz` and renders the local `HH:MM`; `due_phrase`/
  `scheduled_phrase` thread it (the models push passes `&Local`).
  The bucket math (`delta = duedate - now`) is instant math —
  untouched.
- **Grid/caption labels**: already render the zone-free picker
  state (`models.rs:1453-1462`) — correct under the local
  convention with no change beyond the seeds.

**DST edge cases pinned** (unit/property, `due_edit.rs` + a
hand-rolled test `TimeZone` — the workspace has no `chrono-tz`;
the test tz is ~30 lines implementing `TimeZone` with a
fixed-offset switch at a pinned instant, deterministic forever):

1. A spring-forward gap time (`02:30` on the gap date) → parse
   refuses; commit refuses; validity shows invalid.
2. A fall-back ambiguous time (both 02:00s exist) → parse refuses;
   commit refuses (no `earliest()` guessing — symmetric with the
   gap).
3. Non-edge times across the transition dates parse to the correct
   UTC instants (offset A before, offset B after).
4. The round trip `parse(format_entry(x, tz), now, tz) == x` holds
   for every minute of a DST-transition year **except** the
   ambiguous/gap renderings, which are refusals — the property
   encodes the exception, it does not paper over it.
5. `format_entry` renders the local wall-clock: a UTC instant of
   23:30Z in a +02:00 tz renders `01:30` next-day (the date rolls).

**NO stored-instant migration — the recorded rationale (§7-R4)**:
stored dues are UTC instants and *correct* — the bug was always in
the grammar's reading and the echo's rendering, not the data.
Rewriting stored instants (00:00Z → local midnight) would change
due-anchored reminder arithmetic for every existing task, corrupt
synced Nextcloud Deck copies, and need migration 0018-style
machinery for zero truth gained. The observable change is
deliberate: a date-only due stored as `00:00Z` now *echoes* as
`01:00` (local) instead of `00:00` — the display finally tells the
truth about when the task is due; the instant, the reminders, and
the sync payload are byte-identical. Journeys/goldens regenerate
deliberately; the parse→store→echo round trips through a fixed
offset and across a DST boundary are the acceptance harness
(review focus a — "cannot shift existing stored dues": the
journeys assert the *stored* value is unchanged when re-committing
an untouched seeded buffer).

**Adapter tz plumbing**: the seams call `&chrono::Local` at the
three sites (`edited`, `accepted`, `begin/open`) exactly like the
timer parse does (`adapter.rs:5093`) — one live-local convention;
adapter-level journeys assert **tz-invariant structural facts**
(the buffer round-trips: `parse(format_entry(x)) == x` under the
host tz; the stored value of a re-commit equals the original), and
all zone-*specific* pins live in the pure layer with the injected
test tz. No host-tz-dependent assertion ships anywhere.

## 8. Test catalogue (TDD; red where the behaviour is new)

### A. Unit / property — `due_edit.rs` (the pure layer)

1. `grid_is_always_six_week_rows` — the three source shapes (28/35/
   42-cell months) all render 42; leading blanks < 7; the
   `grid_invariants` proptest gains the 42 invariant. (D22; red.)
2. `stepping_an_unset_time_seeds_midnight_then_steps` — the
   reconciled pin (§2; replaces the old inertness pin; red).
3. `step_minutes_wraps` / `parse_time_lenient` truth table +
   totality property (§5.2/5.3; red).
4. `sync_buffer_picker` no-oscillation property + direction rules +
   the two-midnights convention (§4.3; red).
5. The zone suite (§7's five pins) with the hand-rolled test tz;
   the existing UTC pins (`iso_parses_to_midnight_utc`,
   `date_time_parses_to_the_utc_wall_clock`,
   `entry_round_trips_without_date_shift`, `format_entry_seeds…`,
   `picker_opens_on_the_due_anchor`) rewrite to injected-tz local
   expectations (red by intent — the zone change is the point).
6. The validity predicates' truth tables (§6): empty → neutral,
   parseable → valid, junk/partial/gap → invalid; tri-state only,
   no copy.

### B. Journeys — `journeys.rs` (the wiring layer)

7. `the_due_picker_journeys_reach_the_boundary` reworks: every
   grid shape `(6, 42)`; grid-Enter hands focus to the field
   (model flag asserted) and dispatches **nothing**; the typed
   path and day-click keep their commands; the Escape ladder
   (field → grid → close; two presses) — no command until the
   close; the buffer mirrors cursor moves while in the grid
   (read `due_text`/the field's pushed text after C-f). (Red.)
8. `steppers_seed_and_step_from_empty` (F19): an empty-time picker
   steps to 00:15; the readout flips from `--:--`; the second
   backward step lands 23:45 prev-day; the eventual commit carries
   the stepped instant. (Red.)
9. `click_away_commits_parseable_and_discards_junk` (F20, both
   fields — due AND scheduled): (a) type junk → click the card
   background → no command, editor collapsed, prior due intact;
   (b) type parseable → click away → the `UpdateTask` command,
   editor collapsed; (c) empty → click away → no command, no
   clear. The dismissal journey
   (`the_picker_overlay_dismisses_without_disturbing_the_editor`,
   `journeys.rs:7369`) reworks: the editor settles now (recorded
   supersession of the r3 rule). (Red.)
10. `the_combobox_is_the_only_time_path` (review focus d): the
    panel renders the TimeField; no minus/plus IconButtons remain
    (structural: `should-be-total` grep + a walk probe over the
    panel's stops); a quick-pick sets the time and re-renders the
    buffer; combobox-Enter commits the editor. (Red.)
11. `the_zone_round_trip_holds` (review focus a): seed a due,
    open the editor, re-commit the untouched buffer → the stored
    instant is unchanged; type a local wall-clock → the stored UTC
    instant matches the fixed-offset expectation pushed through the
    pure layer's own pins (the journey asserts the round trip, the
    pure layer owns the zone numbers). (Red.)
12. Validity journeys (F27): junk in the due field flips the
    pushed validity to invalid (model read), typing it back to a
    date flips valid; the offset field and the timers field likewise
    (structurally: the model property, never the label text).
13. The foreign-flow guard: focus steal into the description field
    while the due editor is armed settles the due per the rule and
    leaves the description's own engage flow intact (chunk-6
    acceptance e). (Red.)

### C. Snapshots / goldens

14. The keymap registry/preset snapshots regenerate **only if** a
    preset row changed — the plan changes none (the ladder rides
    existing rows); if stage 2 finds a row it needs, it is a plan
    deviation to record, not a silent edit.
15. New golden: the task view with the due editor open +
    default-open picker (`task-view-picker-1.0-900.png`) — pins the
    six-row panel, the combobox row, and the echo line at one
    scale; the existing task-view goldens regenerate only if the
    echo/validity rows shift their closed-state layout (they must
    not — closed state renders none of it).

### D. Review greps (stage 3's clauses, listed for the implementer)

16. No `parse`-alike: `grep -n "fn parse" src/` shows the grammar
    parsers only — validators *call* them (§6; review focus f).
17. `grep -n "and_utc\|Utc::now" src/due_edit.rs` — the grammar is
    tz-injected; `Utc` survives only in test fixtures and the
    commit funnel's final `.with_timezone(&Utc)`.
18. Domain/state/storage/sync/app diffs empty (review focus g):
    `git diff --stat origin/main -- crates/taskboard-domain
    crates/taskboard-state crates/taskboard-storage-sqlite
    crates/taskboard-sync-nextcloud crates/taskboard-app` is empty.

## 9. Commit slices (stage 2; standalone, rebase-clean)

1. `feat(ui-slint): the six-row grid reserve and the seed-on-step`
   — §1 + §2 with their unit pins (the journeys still green except
   the shape asserts, which regenerate here).
2. `feat(ui-slint): the time combobox widget` — §5: the widget, the
   lenient parser, steppers, slots, filter, its unit/property
   suite; not yet mounted.
3. `feat(ui-slint): the picker mounts the combobox` — §5.6 + §4.3's
   Picker-source plumbing + the two new seams; journeys 10.
4. `feat(ui-slint): the two-phase picker ladder and the live sync`
   — §4 with the sync helper, the Enter/Escape ladder, the
   suppression latch; journeys 7.
5. `fix(ui-slint): click-away settles the date editor` — §3 + the
   validity wiring of §6 for the date fields; journeys 9, 12 (date
   halves), 13.
6. `feat(ui-slint): offset and timer field validity` — §6's
   rule-list + timers halves; journeys 12 (remaining halves).
7. `feat(ui-slint): the due grammar speaks local time` — §7 last
   (the largest journey surface), goldens regenerated in the same
   commit; journeys 5-rewrites, 11.

CHANGELOG: one Unreleased entry per user-visible group (picker
fixes, combobox, validity, local-time dues).

## 10. Risks & recorded probes

- **R-a The six-row reserve at min viewport** (§1): the panel is
  ~2 row-heights taller for most months; the `y` clamp degrades to
  field-overlap on short windows. Probe recorded in stage 2
  (Tier-4); fallback = freeze-at-open (recorded, not shipped
  unless measured).
- **R-b The Enter semantic change ripples**: grid-Enter no longer
  commits — every journey/test that commits via grid-Enter
  (`journeys.rs:7297-7307`) reworks to day-click or field-Enter.
  A missed call site is a finding, not a reweakened test.
- **R-c The sync helper fights the Slint field text**: the field's
  text is the TextInput's own buffer; an adapter-driven rewrite
  (Picker source) must reach it through `initial-text` re-seed or a
  pushed text property — the push-model text mirror needs the same
  mechanism the settings fields use. If the pinned Slint makes a
  live text rewrite fight the cursor position, the recorded
  fallback: the buffer mirror stays adapter-side (the a11y value +
  the model) and the *visible* text only re-renders on phase entry
  (grid→field), not per cursor move — the user sees the mirror on
  entry, the validity line carries the live state meanwhile. Probe
  in stage 2 commit 4.
- **R-d DST refusals may surprise**: a user in a DST zone typing
  the one nonexistent hour per year gets a refusal with the hint
  line. Accepted (recorded in §7): guessing an instant is worse.
  The hint copy is a constant; a future refinement can special-case
  it without a protocol change.
- **R-e `at_time_suffix`'s tz parameter touches phrase call sites**
  beyond the editor (cards, banner rows, notifications): the
  signature change is mechanical (`&Local` at the push), but it
  widens the diff into rows this chunk otherwise doesn't touch.
  Contained: the suffix is only rendered where a non-midnight
  instant exists; goldens regenerate only if a fixture row shows a
  time (the today fixture's stamps are relative — verify, don't
  assume).
- **R-f Chunk 9/10 rebases**: both consume the widget/parser; their
  branches cut after this lands (the dispatch gates them on 7) —
  no concurrent friction, but their plans' anchors will shift;
  the standard dance covers it.

## 11. Tier-4 checklist (manual, on the reference machine)

- Picker feel at the **minimum viewport**: open the editor on a
  600-px-wide/short window; page months through a 4-row, a 5-row,
  and a 6-row month — the panel must not move, flip, or clip the
  caption; record the verdict (R-a).
- Combobox feel: type `9`, `930`, `9:45` — the flyout filter and
  the validity follow; scroll-to-nearest lands on the current
  value; the steppers wrap midnight both ways with the date
  following; the flyout opens/closes by mouse, Enter, and Escape;
  the full walk (Tab from the grid through the panel) reads
  sensibly with hover tips everywhere.
- The two-phase flow end-to-end on the emacs preset, exactly the
  user's F21 sentence: walk → Enter → Down → C-p/n/f/b → Enter →
  type → Enter; then the same with a mid-flow Escape step-back
  (the half-typed time survives).
- Local-time honesty: with the device on a non-UTC offset, a
  `14:30` typed due echoes `14:30` (not `12:30`), the card suffix
  reads `14:30`, and the stored instant shifts accordingly
  (devtools/`sqlite3` on the cache, not a test).

## 12. Bookkeeping

- Closes nothing (no issues filed for F18–F22/F27 individually;
  they close with the round's stage-3 ✅ protocol).
- Feeds chunk 9 (the combobox + `step_minutes`/`parse_time_lenient`)
  and chunk 10 (the same widget in the tracking editor); both gate
  on this chunk per dispatch §1.
- Feeds chunk 8's due-anchored rows: the armed-state effective-fire
  times render local and correct only after §7 lands (their gate is
  chunk 6, but the *honest* fire times need this chunk — noted for
  their stage 1).
- Potential new issue (stage 3's call, per origin §8): the DST-gap
  refusal's hint copy could name the transition explicitly once a
  tz database becomes available.

# [Plan] UX Remediation — Chunk 5: surface friction (timers, review, capture, settings)

- **Date**: 2026-11-15
- **Kind**: stage-1 chunk plan for chunk 5 of the UX remediation phase
  (framework plan §6 phase 5; the dress rehearsal's fifth chunk).
  Planning only — no application code in this stage.
- **Origin**: the remediation dispatch plan
  `.artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md` §1/§6
  (the shared execution model, the long-command discipline, and the
  dress-rehearsal mandate are binding); the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` §3 rows
  T-14, T-15, T-16, T-17, T-18, T-19, T-22, T-23; home issues #797,
  #798, #799, #800, #801, #802, #805, #806; the owning UX plans
  `2026-11-14-timers-alarms-ux-plan.md` (T-14, T-22),
  `2026-11-14-review-walk-reflection-ux-plan.md` (T-15),
  `2026-11-14-capture-search-ux-plan.md` (T-19),
  `2026-11-14-settings-connections-ux-plan.md` (T-16, T-17, T-18),
  `2026-11-14-habit-family-ux-plan.md` (T-23); the census
  `docs/ux-surface-census.org`; `docs/ux_planning.org` §2; the landed
  chunks' patterns (chunk 1's visible-gating grammar and kiosk hints,
  chunk 2's armed-confirm staging — the one-armed-subject rule, the
  visible typed refusal ceremony, and the settings armed identity,
  chunk 3's overlay / ActionId / ContextId patterns, chunk 4's
  preset-binding families).
- **Status**: READY — preconditions verified by the dispatcher
  (chunks 1–4 merged; chunk 5 branches off post-chunk-4 `main`
  `1c3b077c`, which carries chunk 2's settings-confirmation state this
  chunk rebases on); worktree
  `../taskboard-WT-fix-ux-r5-surface-friction` on branch
  `fix/ux-r5-surface-friction`.
- **Binding inputs of stage 2**: every decision D-1..D-8 below, the
  amended journey steps §4, the command delta §5, the census cells §6,
  the test catalogue §7, and the commit sequencing §8. Deviations need
  a recorded reason in the stage-2 report. Line anchors cited from the
  triage report were verified against this worktree's HEAD where the
  file had drifted (noted inline); stage 2 re-anchors exactly, never
  edits by stale line number.
- **Long-command discipline** (dispatch §1, binding): the coverage
  trio never runs inside a subagent. Stage 2's gate is the pre-commit
  hooks plus `scripts/run-quality-gates.sh cargo nextest run
  --workspace`; all landed work is committed before any wait.

## 1. Posture (inherited, restated for the chunk)

Three finding classes meet in this chunk:

- **M-b** (CLI-only or GUI-absent mutation of user data without a
  recorded justification): T-14 break rules, T-15 reflection
  edit/delete. The remedy is either an affordance or a dated ruling
  with justification; both land as affordances here (the entities are
  user data the GUI already displays — display-without-control on a
  mutating family is exactly the letter the acceptance resolves
  against).
- **Feedback-class M-a** (the outcome exists but renders nowhere or
  absorbs): T-16 env-locked silent absorb, T-19 disabled menu rows,
  T-23 the habit repair's conditional invert. The remedy is an honest
  rendered outcome at the point of action — no new grammar, the
  surface's own doctrine ("never a silent absorb", `settings_save`)
  applied to its two absorb paths, the status line's notice applied to
  the menu's absorb, a typed row-model signal applied to the repair.
- **Posture/parity-class M-a**: T-17 the settings lane's missing
  escape, T-18 the settings CLI write path, T-22 the alarm window's
  mouse-only modality. T-17 resolves as a **ruling** (D-4 — the
  chunk's architectural decision); T-18 and T-22 as affordances.

The chunk's binding rule from the dispatch: **a silently skipped
finding is a BLOCKER; a ruling without recorded justification is a
BLOCKER**. Every finding below lands as affordance or ruling exactly
as decided here.

## 2. Code-seam inventory (verified at this worktree's HEAD `1c3b077c`)

| Seam | Live anchor | What it gives this chunk |
|---|---|---|
| `CreateBreakRule` / `DeleteBreakRule` | `crates/taskboard-domain/src/messages.rs:324,343` (board roam, `WorkThreshold` mint, cascade pruning) | T-14's write paths exist engine-side; zero UI dispatch sites (the finding's sweep evidence) |
| `breaks add/list/remove` CLI | `crates/taskboard-app/src/cli.rs:967-1004` (the `BreaksCmd` family) | T-14's CLI twins stand unchanged; the GUI parallels them |
| Timers section | `ui/compositions/timers.slint` (`TimerRowModel`, inline creation form with the rule-add commit-seam convention, `absence-hint` kiosk grammar) mounted top-of-page in `ui/views/today-view.slint:75-83`, gated model-side by `timers-visible` | T-14's mount site: the timers composition is the timers family's owning surface (D-1) |
| Armed-confirm staging | chunk 2's landed pattern: `settings_arm_item` / `settings_remove_item` (`adapter.rs:6061-6080`), the board-stack strips (`adapter.rs:8880+`), the recur confirm (`recur_confirm_armed`, `view_state.rs:317-321`) — arm records the one armed subject, confirm routes, stale absorbs | T-14 remove and T-15 delete inherit the staging verbatim |
| Visible typed refusal | `settings_save` (`adapter.rs:5968-6010`: busy → `SettingsOutcome::Rejected`, validate → rejected-with-detail, "never a silent absorb") and the reply mapping (`adapter.rs:4585` `EnvLocked → Rejected`) | T-16 renders through the existing `SettingsOutcome`/`SettingsError` machinery — the typed refusal sentence already exists (`settings_status.rs:72`, "set by an environment variable; change it there") |
| The absorb paths | `settings_files_picked` env-locked early return (`adapter.rs:6439-6448`, tracing-only); the general-field knob commit paths' `env_locked` gates (`adapter.rs:5724+` feeds the badges; the knob-commit absorb sites stage 2 sweeps under the same pattern) | T-16's two fix sites |
| `SettingsError::EnvLocked { field }` | `crates/taskboard-ui-slint/src/requests.rs:158`; render text `settings_status.rs:72` | T-16's typed detail — no new error variant needed |
| The `UiRequest` lane | `adapter.rs:4117-4129` (outside the journal); handlers `crates/taskboard-app/src/ui_requests.rs` (the `config_write_path` overlay writers `:222-279`, connector upsert/remove `:823,:966`) | T-17's ruling evidence; T-18's file-layer write precedent |
| Config read family | `cli.rs:1507-1516` (`ConfigCmd::Print`, `ConfigCmd::Path`); `write_config_atomic` + the overlay helpers (`ui_requests.rs:279`, `connections.rs`) | T-18's conventions: read = print, write = the atomic toml overlay the request handlers already use |
| The knob key set | `crates/taskboard-ui-slint/src/requests.rs:471-573` (the 18 `Set*` variants + `SetFilesSyncEnabled`), `Field::key()` dotted keys | T-18's verb key vocabulary — the CLI mirrors the UI's own keys (one vocabulary, two modality fronts) |
| Menu entry model + dispatch | `menu.rs::row` (`:62-80`, `enabled` from `entry_states`), `MenuEntry { action, label, description, hint }`, the invoke seam `adapter.rs:7415+`, the status-line notice (`adapter.rs:102-103,435` — #130's transient line) | T-19: the description is the per-row why-text; the status line is the feedback channel that already exists |
| Alarm window | `ui/alarm-window.slint` (rows + Dismiss/Snooze/Cancel `IconButton`s, callbacks `dismiss/snooze/cancel(int)`); lanes `lib.rs:478-490` (`install_alarm_lanes`, the "no new commands, a mouse-only surface" note), lazy build `lib.rs:775-790` | T-22: the commands exist; the window needs focus + a context + ids |
| `ContextId` | `actions.rs:26-56` (`Global..FocusMode`, total-mask precedents `Menu`/`Palette`/`FocusMode`) | T-22's new window context slots in beside them |
| Presets | `crates/taskboard-ui-slint/src/keymap/presets.rs` (Standard/Emacs/Vi); the keymap contract suite is the totality oracle | T-22 binds all three presets or fails the gate |
| Review ledger | `ui/views/review-view.slint:340-360+` ("read-only (D-B10): no activation, no walk targets"), the landed ledger-row pattern inline (family-4 paging, the #373 viewport bound); `ContextId::Review` binding-free (`actions.rs:33-36`); `journal edit/delete` CLI (`cli.rs:1425,1455`), `UpdateReflection` change-mask / `DeleteReflection` tombstone (`messages.rs:540-590`) | T-15's mount + its command semantics |
| Reflection undo class | `crates/taskboard-domain/src/undo.rs:238-245` (`NotUndoable(ReflectionSurface)`, DR-7 D-J7 — the family's own verbs are the recovery surface) | T-15's delete stages; the tombstone posture is authority, not debt |
| `AdjustHabitLog` invert condition | `crates/taskboard-domain/src/undo.rs:199-215` (pre-row present ⇒ exact invert; absent ⇒ `NotUndoable(HabitSurface)`) | T-23's condition is derivable from live state at the row |
| Habit repair target | `presentation.rs::habit_repair_target` + the row seam (`adapter.rs:7131-7157` family, `habits-repair-clicked`), the habits composition `ui/compositions/habits.slint` | T-23's signal derives next to the existing target computation |
| Blank-record guard | `adapter.rs:2295-2345` region (the walk's typed guards — no silent-commit posture) | context only; T-47 is chunk 6's row, untouched here |

## 3. Decisions

### D-1 — T-14 (#797): the break-rule roster renders in the timers composition; create rides a form; remove stages through the armed confirm

The timers composition (`timers.slint`, today-view top-of-page) is the
timers family's owning surface: active timers, the creation form, and
the awaiting pointer already live there — a break rule is a
duration-trigger timer ("the `timers` recipe", `cli.rs:964-966`), so
it mounts with its family, not in Settings (break rules are user data
that roams with a board, `messages.rs:317-323`; Settings is the knobs
surface, ruling R-1's grammar).

- **Roster**: a "Break rules" subsection under the active-timer rows —
  one row per rule: label, threshold phrase, the insistence rung bit
  (the `TimerRowModel.alarm-rung` convention), the roaming board's
  name (a break rule roams via the board's synced `break-rules`
  section, #588). Model-side derived per refresh, index-keyed targets
  (the IdTables convention). Kiosk: the whole section rides the
  existing `timers-visible` gate + `absence-hint` grammar — the
  display hint names timers and break rules together (chunk 1's
  letter: the fence is visible, not silent).
- **Create**: an "Add break rule" affordance opening a small form
  (label, threshold, insistence) following the inline creation-form
  convention the timers setter already established (absorbed parse
  miss keeps the field text; valid commit clears). Threshold parsing
  uses the `WorkThreshold` mint — a zero/negative value is
  unrepresentable, so the form's validity predicate mirrors
  `parse_after_flag`'s acceptance. Dispatches `CreateBreakRule` with
  the rule's board = the single live board when unambiguous (the CLI
  `--board` default's letter, `cli.rs:978-983`).
- **Remove**: staged — the row's remove affordance **arms** (chunk 2's
  one-armed-subject rule; arming touches no engine state), the strip's
  Confirm routes `DeleteBreakRule`. Justification for staging:
  `DeleteBreakRule` cascade-prunes deliveries, dismissals, and snooze
  links (`messages.rs:343-349`) and is `NotUndoable` (break-rule
  commands never re-plan, `messages.rs:328-332`) — data-at-risk ⇒ the
  chunk-2 staging is the escape, exactly T-4's resolved posture.
- **ActionIds**: `BreakRuleCreate` (opens the form) and
  `BreakRuleRemove` + `BreakRuleRemoveConfirm` (the staging pair), in
  a `ContextId::Today`-adjacent registration consistent with how the
  timers setter's seams register today (stage 2 follows the landed
  form-commit pattern; keyboard bindings are *not* required by the
  finding — pointer parity with the sibling timer rows is the bar —
  and none are added, keeping the keymap surface unchanged).
- **Journey amendments**: timers plan US-9 gains GUI steps
  (US-9/S2 roster display, /S3 create, /S4 staged remove); the CLI
  steps stand as the parallel front.

### D-2 — T-15 (#798): ledger rows activate into an inline editor; delete stages; the tombstone posture is DR-7 D-J7 authority

- **Activation**: the journal-entries ledger's rows gain an activation
  affordance (click; the row is the target, index-keyed per the
  ledger's own row convention). The "read-only (D-B10)" ledger note is
  superseded by this chunk — recorded in the amended review plan.
- **Edit shape**: an **inline editor under the activated row** (the
  recur-row inline editor precedent, `view_state.rs:317-321` — not a
  global overlay: the ledger is the entity's owning mount and the
  editor is one row tall). Fields mirror the `journal edit` change
  masks exactly (`cli.rs:1427-1450`): text (+ explicit clear), mood /
  energy (set / clear), flags by delta. Commit routes `UpdateReflection`
  with the change mask; an all-unchanged editor commits the accepted
  no-op (clocks untouched, `messages.rs:540-547`).
- **Delete**: staged — the row's delete affordance **arms**, the strip
  confirms, `DeleteReflection` routes. Tombstone semantics: the row
  tombstones in place (DR-7 D-J2) and leaves the live ledger (it
  renders live entries only); the undo class stays
  `NotUndoable(ReflectionSurface)` (`undo.rs:238-245`, D-J7 — the
  family's own verbs are the recovery surface; this chunk *is* that
  recovery surface arriving in the GUI). No journal-arm workaround is
  attempted — the recorded design rationale already names the tombstone
  as the family's inverse-of-record.
- **Keyboard**: `ContextId::Review` stays binding-free
  (`actions.rs:33-36`); the finding's letter is GUI parity with the
  CLI verbs, and pointer activation satisfies it. Recorded as the
  smallest honest fix; a reflection context is chunk-6-scale polish,
  not owed by T-15.
- **Journey amendments**: review plan US-9 gains GUI steps
  (US-9/S2 activate+edit, /S3 staged delete); the census
  `reflection.edit/delete` GUI cells retire their `U (RW-1)` markers.

### D-3 — T-16 (#799): the env-locked absorb becomes the rendered typed refusal — the doctrine holds at both absorb sites

The surface's own doctrine ("never a silent absorb",
`adapter.rs:5981-5983`) already owns the machinery: the typed
`SettingsError::EnvLocked { field }` variant, its refusal mapping
(`adapter.rs:4585`), and its recovery sentence
(`settings_status.rs:72`, "set by an environment variable; change it
there") all exist — the absorb sites simply never call them.

- **Files pick**: `settings_files_picked`'s env-locked early return
  (`adapter.rs:6439-6448`, currently tracing-only) renders
  `set_settings_outcome(window, SettingsOutcome::Rejected,
  Some(SettingsError::EnvLocked { field: Field::FilesSyncEnabled }))`
  before returning. The pick still mutates nothing (the lane stays
  clean — the `settings_save` ceremony's letter).
- **Knob edits**: the general-field knob commit path's env-locked gate
  gets the same outcome render (stage 2 sweeps every knob-commit
  absorb branch under the identical pattern — the finding names the
  files pick as the instance and the knob family as the class; the
  fix is the class, not one site). The facts row's env-locked badge
  stands (US-8's display story) — the refusal renders *at the knob*,
  the badge explains *why it is locked*; T-36's badge-vs-knob distance
  note is chunk 6's row and untouched.
- **Tests are typed**: outcome render assertions assert
  `SettingsOutcome::Rejected` + the `EnvLocked` detail variant — never
  the sentence text (AGENTS §5).

### D-4 — T-17 (#800): **recorded justification + the chunk-2 confirmations as the recovery surface — journal arms for the `UiRequest` lane are rejected**

This is the chunk's architectural decision, taken with the dispatch's
bias to the smallest honest fix.

**The ruling**: the settings surface's recovery posture is *staged
confirmation + immediate re-editability*, not journal undo. The
`UiRequest` lane stays outside the undo journal by design, and the
design is now *recorded* — dated 2026-11-15, authority = this plan +
the user's acceptance of the triage report (which classified T-17
M-a and handed the posture call to stage 1).

**Why journal arms lose** (each point independently sufficient):

1. **The lane's remote halves cannot be honestly inverted.**
   Connection save/remove, board attach/detach/publish, and the files
   sync re-anchor all mutate remote or cross-actor state (the
   Nextcloud subtree, stored credentials, the published board's sync
   actor). A local `PreImage` journal arm would restore the *local*
   fact while the remote half keeps the effect — an undo button that
   lies. The undo journal's contract is exact inversion of stored
   local facts (`undo.rs` passim); these operations are not that.
2. **Chunk 2 already bought the escape that matters.** Every
   destructive settings journey (connection remove, board detach,
   Stop-Sync — T-4's family) now stages through the armed confirm
   (`settings_arm_item` / the confirmed strips). The audit's escape
   mechanism (d) — the confirmation stage — is in place for exactly
   the journeys where a mis-stroke destroys something.
3. **The non-destructive rest is self-healing.** Knob edits and the
   sync toggle are idempotent, individually re-editable at the same
   seam (the knob itself is the undo — re-set the previous value),
   and the facts row shows the effective value. Journal-arming
   `SetReminderTick` would add an undo entry for a change the user can
   reverse in one click at the very control that made it.
4. **Cost shape**: journal arms for the lane would need new undo
   classes per request family, NotUndoable carve-outs for every
   remote-effecting variant (most of them), and a reply-ordering
   contract between the journal and the oneshot reply lane — an
   architectural program, not a remediation, for a surface whose
   data-at-risk rows are already staged.

**What lands as code**: nothing new for T-17 beyond D-3's refusal
render (which makes the lane's outcomes fully visible — the honest
posture's other half). **What lands as record**: (a) this decision,
restated in the amended settings-connections plan §2 (the SC-6 finding
section gains the ruling paragraph and cites chunk 2's confirmed
strips as the recovery surface per journey); (b) the census Settings
slice's ruling row: the settings journeys' escape column reads
"staged confirm (destructive rows) / re-edit at the knob (the rest) —
journal undo deliberately absent (R-17, 2026-11-15)"; (c) the
`adapter.rs:4117-4129` lane comment gains one sentence pointing at the
ruling. Home issue #800 gets a closing comment citing this plan when
the chunk lands (stage 3).

### D-5 — T-18 (#801): `taskboard config set <key> <value>` — the file layer write, figment-respecting, env-locked refusals typed

- **Verb shape**: `config set <key> <value>` (repeatable? no — one
  key/value per invocation, the `boards reset --confirm` school: one
  mutation, one verdict). `key` is the dotted config key, exactly the
  vocabulary `Field::key()` uses (`reminders.tick`,
  `focus.long_break_every`, `files_sync.enabled`, ...) — one
  vocabulary, two modality fronts. `value` parses per key against the
  same predicates the settings form uses (`settings_form::validate`
  equivalents where a shared predicate exists; key-specific parsers
  where it does not). `config unset <key>` rides the same commit
  (removes the override from the toml layer) — without it a set value
  cannot return to the default from the CLI, an asymmetric grammar.
- **Figment layering respect** (the binding constraint): the verb
  writes **only the `taskboard.toml` layer**, via the atomic overlay
  machinery the request handlers already use
  (`ui_requests.rs:222-279`, `write_config_atomic`) — defaults and
  env are never touched. A key overridden by `TASKBOARD_*` env is a
  **typed refusal** (distinct exit code + `matches!`-able error): the
  file write would be dead text under the env layer, and refusing is
  the same honesty the GUI's D-3 refusal renders. `config print`
  remains the read front; `config path` already names the write
  target.
- **App-arm tests mirror the move-arm shapes**: stage 2 locates the
  landed CLI app-arm test pattern (the move-arm family boots the app
  actor against a temp workspace and asserts the typed engine
  outcome). `config set`'s arms: a known key writes the toml layer
  (assert the file + a re-read through the figment stack), an
  env-overridden key refuses typed without writing, an unknown key or
  malformed value refuses typed without writing, `config unset`
  removes the key. Hermetic: temp dirs, no network, no sleeps.
- **Known posture, recorded**: a concurrently running app holds knobs
  in memory and will not see a CLI file write until restart — the
  figment reality for a headless write. The verb's help text says the
  file layer is authoritative at next start (help text is not UI
  grammar; no census cell). The running-app divergence is filed to the
  backlog (§9).
- **Census**: the `settings.knob_set` and `files.sync_enabled_set`
  CLI cells retire `U (SC-1)` to `E` citing the verb.

### D-6 — T-19 (#802): an invoked disabled menu row sets the status-line notice with the row's own description — why-text, not tooltip, not grey-only

Decided honestly against the doctrine ("every click reacts") and the
menu grammar:

- **Why-text over tooltip**: a tooltip is hover-only and pointer-only
  — it fails the keyboard route (firing a disabled row from the menu
  walk) and the kiosk posture. The status line (#130's transient
  notice) is the app's existing non-blocking feedback channel and is
  modality-neutral. Grey-only fails the doctrine's letter — the
  invocation *happened* and must react.
- **The why-text source**: the entry's own registry `description`
  (`menu.rs:69-71` — every row carries one; no new presentation text
  catalogue). The notice sentence is built presentation-side from the
  typed fact (entry disabled) + the description; tests assert the
  typed fact routes and the status line armed, never the sentence.
- **Mechanics**: the menu invoke path currently absorbs a disabled
  entry before routing; the absorb branch instead sets the status
  notice (stage 2 anchors the exact absorb site in the
  `on_menu_entry_invoked` handler and its submenu twin). No new
  ActionId, no keymap change, no menu-grammar change — the row still
  renders grey (`entry_states` stands; T-46's doctrine bar keeps the
  grey as a recorded positive).
- **Journey amendment**: capture plan W-N3 gains its remediated note
  (the chunk-1 pattern: remediation stamped over the walkthrough
  note, test cited).

### D-7 — T-22 (#805): the alarm window gains keyboard parity through a window-local total-mask context and per-row ids — the "mouse-only surface" note is superseded

- **Context**: `ContextId::AlarmWindow` (new), a **total mask** (the
  `Palette`/`FocusMode` letter): while the alarm window is open it
  owns the keyboard exactly as a modal should; nothing underneath
  fires. The context activates when the window is shown and
  deactivates when it closes (the router's window-lifecycle seam;
  stage 2 anchors the show/hide pair `lib.rs:775-790` drives).
- **Ids**: `AlarmDismiss` / `AlarmSnooze` / `AlarmCancel` (new
  ActionIds, contexts `&[ContextId::AlarmWindow]`), routing to the
  same engine commands the buttons already dispatch (`lib.rs:478-490`
  — the commands exist; "no new commands" stays true, only the
  modality note is superseded). A row-selection walk: ArrowUp/Down
  moves the selected row (a window-local `selected-row` property);
  the three verbs apply to the selected row; with one row it is
  pre-selected — **all three modal buttons are reachable by keyboard
  in every preset**, the acceptance's letter.
- **Bindings**: every preset binds the three ids (single-key
  mnemonics where free per preset: Standard `d/s/c`; Emacs and Vi get
  their preset-natural keys — stage 2 picks from each preset's free
  single-key set and the keymap contract suite proves totality).
  Escape closes the window (bind `Escape` to window-close in the
  context; a dismiss-adjacent exit but committing nothing — the
  buttons remain the only verbs that mutate).
- **The recorded note**: `lib.rs:483-485`'s "a mouse-only surface"
  comment is rewritten to cite this chunk (the fence R3.5.3 — no
  window on kiosk — is untouched; keyboard parity is a desktop
  capability, and the kiosk absence hint from chunk 1 already covers
  the fence honestly).
- **Tests**: keymap totality per preset; a seam test asserting each
  verb id routes the same command the button callback sends (click ≡
  key); a presentation test for the selection walk's clamp behavior.

### D-8 — T-23 (#806): the habit row model carries a typed repair-undoability bit; the row captions which side of the invert condition the repair is on

- **Typed state in the row model**: the habit row model gains
  `repair-undoable: bool`, derived presentation-side next to the
  existing `habit_repair_target` computation — true exactly when the
  target window already holds a log row (the `undo.rs:199-215`
  condition, restated as a pure predicate over live state; a
  no-target row renders no repair affordance change and the bit is
  meaningless there, matching today's absorbed no-op posture).
- **The signal**: the repair affordance's caption splits on the bit —
  the undoable side reads as it does today (an adjust to the window's
  count, invertible); the not-undoable side gains the honest caption
  that the make-up *creates* the window's row and cannot be undone
  (`NotUndoable(HabitSurface)` — the lossy-inverse rationale is the
  engine's, recorded at `undo.rs:202-207`). One bit, one caption,
  zero new commands; the recognized-positive bar (T-46) is met — the
  feedback names the consequence before the stroke, not after.
- **Tests**: a typed presentation test per side of the predicate
  (row-with-log ⇒ bit true; empty-window ⇒ bit false + the caption
  flag flips structurally), never caption-text assertions.
- **Journey amendment**: habit plan HB-2 gains its remediated note
  citing the row-model bit; the census `habit.repair` feedback cell
  gains the signal evidence.

## 4. Amended journey steps (stage 2 writes these into the owning plans)

| Plan | Amendment | Steps |
|---|---|---|
| timers-alarms | US-9 rewritten from CLI-only to dual-front; new GUI steps + wiring rows W-8 supplemented | US-9/S2 roster display (timers composition subsection); US-9/S3 create form → `CreateBreakRule`; US-9/S4 armed remove → `DeleteBreakRule`; kiosk hint names the fence |
| review-walk-reflection | US-9 GUI steps; D-B10 ledger note superseded; W-9 supplemented | US-9/S2 activate+inline-edit → `UpdateReflection` change mask; US-9/S3 armed delete → `DeleteReflection`; tombstone posture per D-J7 |
| settings-connections | US-6/US-7 escape columns; SC-3 remediated; SC-6 ruling paragraph (D-4); SC-1 CLI steps | US-6/S2 env-locked pick → typed refusal render; US-7/S2 env-locked knob → typed refusal render; US-10 (new): CLI `config set`/`unset` steps |
| capture-search | W-N3 remediated note | menu-row disabled invocation → status notice (typed test cited) |
| habit-family | HB-2 remediated note | the repair bit + caption split; census cell evidence |

All amendments cite the triage finding ids and home issues as
authority; `scripts/ux-coverage.sh <plan> --gate` must find every new
wiring row.

## 5. Command delta

- **New StateCommands**: none (all four engine commands exist).
- **New UiRequests**: none.
- **New ActionIds**: `BreakRuleCreate`, `BreakRuleRemove`,
  `BreakRuleRemoveConfirm`, `AlarmDismiss`, `AlarmSnooze`,
  `AlarmCancel` (registry entries + router arms; menu rows only where
  decided — the break-rule pair mounts in the timers composition, not
  the action menu).
- **New ContextId**: `AlarmWindow` (total mask).
- **New CLI verbs**: `config set <key> <value>`, `config unset <key>`
  (`ConfigCmd` variants; app arm + figment-respecting write path per
  D-5).

## 6. Census cell updates (ride the implementation commits)

| Cell | From | To |
|---|---|---|
| `break_rule.create` / `break_rule.remove` / `break_rule.roster_display` GUI cells (Today column + register) | `U (TA-1)` | `E` citing the timers-composition roster, the form, the staged remove |
| `reflection.edit` / `reflection.delete` GUI cells (Review column) | `U (RW-1)` | `E` citing the ledger activation, inline editor, staged delete |
| `files.sync_enabled_set` / `settings.knob_set` CLI cells | `U (SC-1)` | `E` citing `config set`/`unset` |
| Settings escape/ruling row | — | the R-17 ruling text (D-4) |
| `app.menu` disabled-row cell | feedback column | the status-notice evidence |
| `alarm.dismiss/snooze/cancel` keyboard columns | mouse-only | per-preset bindings + the total-mask context |
| `habit.repair` feedback cell | — | the typed invert-condition signal |

## 7. Test catalogue

- **App-arm (CLI)**: `config set` happy path writes the toml layer
  (file + figment re-read); env-overridden key typed refusal, no
  write; unknown key / malformed value typed refusal, no write;
  `config unset` removes the key (D-5's four arms, move-arm shapes).
- **Seam tests (adapter)**: env-locked files pick → outcome render
  typed (`Rejected` + `EnvLocked`); env-locked knob commit → same;
  disabled menu invocation → status notice armed (typed fact);
  armed break-rule remove confirm routes `DeleteBreakRule`; armed
  reflection delete confirm routes `DeleteReflection`; reflection edit
  commits the exact change mask (including the accepted no-op);
  alarm verbs: keyboard id ≡ button callback command (structural
  equality of the routed command).
- **Presentation tests (typed)**: break-rule row model fields from
  live state; `repair-undoable` bit per predicate side; alarm-window
  selection walk clamps.
- **Keymap totality**: the contract suite proves `AlarmWindow`
  bindings exist in Standard/Emacs/Vi and the total mask holds.
- **Kiosk gates**: the timers subsection rides `timers-visible` +
  `absence-hint` (chunk-1 grammar); the alarm window never mounts
  (fence R3.5.3 untouched — asserted by the existing fence test
  standing).
- **Hermeticity**: no sleeps (pause/advance where timing exists), no
  wall clock, typed assertions only (no text assertions).

## 8. Commit sequencing (every commit standalone)

1. `feat(app): config set and unset write the toml layer` — the CLI
   verbs + app arms + typed refusals (D-5). Self-contained: no UI.
2. `fix(ui): env-locked settings edits render the typed refusal` —
   D-3, both absorb sites + seam tests.
3. `fix(ui): an invoked disabled menu row sets the status notice` —
   D-6 + seam test. Small, independent.
4. `feat(ui): the break-rule roster mounts in the timers composition`
   — D-1 (roster + create form + ActionIds + presentation tests).
5. `feat(ui): break-rule remove stages through the armed confirm` —
   D-1's staging leg + seam tests.
6. `feat(ui): journal ledger rows activate into an inline editor` —
   D-2's edit leg.
7. `feat(ui): reflection delete stages through the armed confirm` —
   D-2's staging leg.
8. `feat(ui): the alarm window gains keyboard parity` — D-7
   (context, ids, selection walk, presets, superseded note).
9. `feat(ui): the habit repair row signals its undo side` — D-8.
10. `docs(ux): the r5 amendments — journey steps, R-17 ruling, census
    cells` — the five amended plans + census (D-4's record included).
11. Gate: pre-commit hooks ran per commit;
    `scripts/run-quality-gates.sh cargo nextest run --workspace`
    after the final code commit (and re-run after the docs commit is
    trivially unaffected). Coverage trio: dispatcher-owned.

Order rationale: CLI first (no UI deps, app-arm tests green
independently); the two feedback fixes next (smallest, isolate the
doctrined surfaces); the three affordance families after; the
row-model signal last (depends on nothing above); docs amendment
last so the gate run sees the final code.

## 9. Backlog

- The running-app vs `config set` divergence (D-5's recorded posture)
  is filed as a `backlog:` issue (live-reload or IPC nudge) —
  out of scope for this chunk. Issue number recorded in the stage-2
  report.

## 10. Risks

- **Adapter anchor drift**: the triage's line anchors predate chunk
  2's settings edits; stage 2 re-anchors by symbol, never by line.
- **Knob-commit sweep breadth** (D-3): the knob family's absorb
  branches may not share one literal site — the sweep is by pattern
  (`env_locked` gates on commit paths); the seam test catalogue
  covers files-pick plus one knob representative per commit-path
  shape, not 18 per-variant copies.
- **Alarm-window focus**: a `no-frame` always-on-top window's focus
  behavior is WM-dependent (the slint header's Wayland caveat);
  bindings fire through the app's keymap pipeline only while the
  window holds focus — the selection walk + ids are correct by
  construction, but grab semantics are recorded as WM-dependent
  posture, not guaranteed.
- **`config set` predicate reuse**: the settings form's predicates
  live in the UI crate; the CLI cannot depend on it (workspace
  boundaries). Shared predicates move to `taskboard-domain` or are
  re-expressed there — stage 2 records which, keeping the vocabulary
  identical.
- **Estimated diff**: L — roughly 1,200–1,700 lines across
  `cli.rs`, `ui_requests.rs`/config plumbing, `adapter.rs`,
  `actions.rs`, `menu.rs`, `lib.rs`, `presets.rs` ×3,
  `timers.slint`, `today-view.slint`, `review-view.slint`,
  `alarm-window.slint`, `habits.slint`, `models.rs`,
  `presentation.rs`, plus tests and the five amended plans/census.

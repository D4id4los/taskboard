# Task-Detail Shortcut Completion — Implementation Plan (round 9, chunk 11)

- **Date**: 2026-11-19
- **Kind**: stage-1 implementation plan (dispatch chunk 11, J3 US-4/US-5 + R-8)
- **Branch**: `feat/ui-taskdetail-shortcuts` (this worktree)
- **Origin**: J3 `.artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md`
  (US-4/US-5, R-8, W-4/W-5 normative); dispatch plan
  `.artifacts/plans/2026-11-18-gui-review-round9-dispatch-plan.md` §12;
  research `.artifacts/reports/2026-11-18-round9-keyboard-focus-research.md`
  §6–§7 (the 22-verb inventory).
- **Precondition verified**: chunk 9 merged (PR #1061, 2026-11-19) —
  `nav.focus-*` vocabulary, preset retune, subject-walk retirement are on
  `origin/main` (be5403c7). Chunk 10 (PR #1064, habit editor on the shared
  detail surface) is also merged — the `is-habit` mount discriminator and
  the habit-editor variant rows exist and are out of scope here.

## 1. Scope

Complete the task-detail shortcut table so **every regular activatable
widget is reachable without Tab** (US-4), and the tracker verbs bind in
**Board/StackDetail** too (US-5), with **one smart track toggle**
(R-8) — start-if-idle, stop-if-running — plus tooltip hint coverage for
every new binding.

Non-goals (dispatch §12): directional ids (chunk 9, landed), the
rule-list cursor (chunk 12), habit-editor verbs (chunk 10, landed),
any preset retune beyond the new rows.

## 2. The taken-letter set (collision survey, current `presets.rs`)

Enumerated per context before any assignment (rows verified against
`crates/taskboard-ui-slint/src/keymap/presets.rs` on be5403c7):

- **TaskDetail**: Std — F2..F6, F8..F11, arrows (nav.focus carriers,
  chunk 9), PageUp, Tab. Emacs — Return, C-c r, F3/F4, C-c C-d,
  C-c ', C-c C-s, C-c C-u, C-c C-i/o, C-c C-x, C-c C-y, C-c C-n,
  Ctrl+Shift+l, C-f/b/n/p, C-v/Alt-v, Tab. Vi (view rows) — r, F3/F4,
  d, s, e, a, I, O, y, X, Tab; **plus the `[TaskDetail, ViNormal,
  Global]` chain**: Space (TaskComplete), U (TaskReopen), h/j/k/l,
  i/a (ModeInsert), and every other ViNormal letter is therefore
  *taken in the chain* even where the TaskDetail rows are silent.
- **Board**: Std — arrows, Shift+Up/Down (extend), Ctrl+Home/End,
  Space, Ctrl+Space, N, S, Return, M, Delete, b/B, Ctrl+Shift+g/n/l,
  Ctrl+F2/F3, P, Ctrl+Shift+Left/Right, F2, Ctrl+Alt+m, Ctrl+Delete,
  Ctrl+Left/Right, F11, PageUp/Down, Tab. Emacs — C-n/p, C-f/b,
  C-c {c,g,s,r,m,b,C-i,o,C-w,C-y,C-n,C-x}, Ctrl+d, Return, b/B,
  Alt+</>, Ctrl+Left/Right, Ctrl+Shift+l, C-g, Tab. Vi — J/K, y/Y,
  X, b/B, Ctrl+Shift+l + the ViNormal chain (Space, U, i, a, dd,
  g *, Z *, u, Ctrl+r, …).
- **StackDetail**: Std — arrows, Ctrl+End, N, y? no (vi only), F11,
  PageUp, Tab. Vi — y, X + chain. Emacs — C-n/p/f/b/o, Alt+</>,
  C-c C-y/C-n/C-x, Tab.
- **Today**: Std — arrows, Ctrl+Home/End, Space, Ctrl+Space, Return,
  M, Delete, T, F9/F10/F11, Ctrl+Shift+h/u, Shift+Up/Down, PageUp/Down.
- **Global**: Ctrl+Shift+z/m/q/k/t/r/b/g/u/h(l), Ctrl+y, Ctrl+,
  Ctrl+PageUp/Down, Alt+1, F1, Escape (move-cancel), Ctrl+Shift+t
  (all presets).

Free-set conclusions drawn below cite these; the totality/
no-collision contract tests in `crate::keymap` are the mechanical
oracle at stage 2.

## 3. The binding table (D-table, settled)

### 3.1 New ActionIds (2)

| ActionId | slug | contexts (registry) | semantics |
|---|---|---|---|
| `TaskTrackToggle` | `task.track-toggle` | GLOBAL (family precedent) | smart start/stop over the focused/subject task: a running entry **on this task** stops; otherwise tracking starts (any other running entry closes first — `TaskTrackStart`'s own law) |
| `TaskEffortEdit` / `TaskEffortClear` | `task.effort-edit` / `task.effort-clear` | TaskDetail | focus+arm the effort row's always-on textbox (M9) / clear the effort estimate (confirmation-free, mirrors `TaskUnschedule`) |

Naming ruling: R-8's smart verb is `task.track-toggle` — **distinct**
from the existing `TaskToggleTracker` (`task.toggle-tracker`, the
tracker-**kind** flag toggle, #492), which the round-8 research §6
conflated. The detail card's play/pause control is the smart verb, not
the kind toggle (verified: `adapter.rs::send_card_track_toggle` already
implements exactly the R-8 semantics; stage 2 re-roots that helper and
the three card seams on the new registry id — one semantics, one home).

### 3.2 The assignment table

Std = Standard, E = emacs, V = vi. Every row verified free against §2.

| Verb | Std | Emacs | Vi | Rationale |
|---|---|---|---|---|
| `TaskComplete` @TaskDetail | `Space` | `C-c C-t` | — (chain: ViNormal `Space` already fires) | mirrors each preset's Board/Today spelling; org's C-c C-t TODO-cycle mnemonic |
| `TaskReopen` @TaskDetail | `Ctrl+Space` | `Ctrl+Space` | — (chain: ViNormal `U`) | mirrors Board/Today |
| `TaskTitleEdit` @TaskDetail (vi gap) | F2 (landed) | Return (landed) | `g i` | vim `gi` = insert at last edit position; `g` second stroke `i` free |
| `TaskTrackToggle` (new) @ TaskDetail+Board+StackDetail+Today | `Ctrl+F9` | `C-c i` (bare second stroke) | `c` | reads as the F9/F10 family's toggle; bare-vs-chord second stroke is the house shape (C-c c capture vs C-x c); vi `c` = clock, free in all four chains |
| `TaskTrackStart` @ Board+StackDetail (new contexts, same spellings) | F9 | C-c C-i | I | r8 landed TaskDetail+Today only; US-5 extends verbatim |
| `TaskTrackStop` @ Board+StackDetail (new contexts) | F10 | C-c C-o | O | twin of the row above |
| `TaskTrackEntries` @TaskDetail | `F12` | `C-c e` | `E` | F12 is the only free F-key in Std; emacs `e` free as second stroke (vi's bare `e` is the description editor — uppercase twin there); vi `E` free (uppercase twins `I/O`) |
| `TaskEffortEdit` (new) @TaskDetail | `Ctrl+e` | `C-c C-e` | `f` | std Ctrl+e free; emacs C-c C-e free (C-c C-f is Board color @Board only, and `f` is load-bearing in emacs); vi `f` free at detail |
| `TaskEffortClear` (new) @TaskDetail | `Ctrl+Shift+E` | `C-c X` (bare-shift second stroke) | `F` | distinct-from-Ctrl+e per R-6 case law; emacs C-c C-x taken (unschedule) → bare-shift twin of vi X; vi `F` free and the natural twin of `f` |
| Back / close-subject | — no new id | — | — | **Ruling: reuse `NavFocusBoard`** (it IS the back verb: "returns to the board view from any deeper view"; the nav-back button's hint already projects it). All three presets gain `Alt+Left` as an **alternate** row @Global (D7 alternates shape): Std keeps Alt+1, vi keeps `g g`, emacs — which binds NO board-return today (the nav-back hint is empty in emacs TaskDetail, a real gap) — gets `Alt+Left` as its primary |

Emacs-confusability note (recorded risk, accepted): `C-c i` (toggle)
vs `C-c C-i` (start) are distinct bindings by the bare/chord rule with
existing precedent (`C-c c` capture); tooltips disambiguate.

### 3.3 R-8 smart-toggle semantics (settled)

`TaskTrackToggle` over the **subject task at TaskDetail, the focused
card at Board/StackDetail/Today** (the same target resolution the
`send_card_track_toggle` seams already use):

- running entry whose task == target → dispatch `TaskTrackStop`
  outcomes (close the open entry);
- otherwise → dispatch `TaskTrackStart` outcomes (any other running
  entry closes; this task's opens) with the track-target seeded.

Router arm mirrors `send_card_track_toggle`'s read (`
presentation::tracking_open_task`) and emits the two existing verbs'
outcomes — **no new StateCommand** (the fence: zero new domain
commands; the planner's idempotent NoOps own the refusals). Stage 2
re-roots the pointer seams on the registry id so the semantics has
exactly one home (dedupe, not duplication).

### 3.4 Tab-only verb rulings (the 22-verb table closed)

| Verb | Ruling |
|---|---|
| Done checkbox pointer seam (`task-done-toggled`) | stays pointer-side; keyboard is `TaskComplete`/`TaskReopen` (the §3.2 rows) — the seam's subject-vs-focused raw-send reason stands |
| Chip unassign (per-chip, indexed) | **ruled pointer/Tab permanently** — indexed targets, the rule-row precedent |
| Rule remove / presets / unmute / jump-to-due | **ruled Tab until chunk 12** lands the rule-list cursor; re-ruled there, not here |
| Series pause/resume | **ruled Tab (moderate use)**; backlog issue filed for a future binding candidate |
| Series/habit conversion family | **exempt** — the operator's rare-verb carve-out (research §6, standing) |
| `TaskToggleTracker` (kind flag, #492) | **ruled menu-reachable only** — rare verb, near the conversion carve-out; backlog issue filed |
| Effort commit/clear inside the armed editor | Enter/commit + focus-loss commit already own the editor (M9 law); the new chords arm/dismiss, not type |
| Priority dismiss (`:371`) | works (`CancelMove` routing, research §6) — no change |
| Habit-editor verbs (chunk 10's variant rows) | owned by chunk 10/H4 — out of scope |

**Fold-test law**: after landing, zero *regular* activatable detail
widgets are Tab-only — the fold test enumerates the verb table and
asserts every non-exempt row above resolves a binding in every preset
at TaskDetail.

## 4. Hint coverage (W-4 tail)

- Existing projections populate automatically from bindings:
  `task-done-hint` (state-correct Complete/Reopen pick,
  `models.rs::push_hint_models`), `task-track-hint`
  (Start/Stop pick via `presentation::detail_tracking_verb` — stays
  pair-based; the toggle is reachable but the card's tooltip keeps
  naming the state-correct verb), `nav-back-hint` (NavFocusBoard —
  becomes non-empty in emacs, the §3.2 gap fix).
- **New hint properties** in `task-view.slint` + projection rows in
  `models.rs`: `task-entries-hint` (TaskTrackEntries),
  `task-effort-hint` (TaskEffortEdit, shown on the effort row's
  opener affordance; the clear twin rides the same tooltip's second
  line where the row renders both verbs).
- Fold test: every detail widget's tooltip hint model resolves a
  non-empty projection in every preset (zero empty-where-bound and
  bound-where-unprojected drift).

## 5. TDD phase plan

Order is test-first per phase; every phase ends green on
`cargo nextest run -p taskboard-ui-slint` before the next starts.

1. **Phase T1 — registry + collision fold tests** (`actions.rs`,
   `keymap`): the three new ActionIds with slugs/labels/menu ranks
   (Task family, ranks ~187/104/104), contexts per §3.1; the
   verb-table fold test (§3.4 law) and the per-preset no-collision
   contract runs (existing totality gates — they fail until §3.2 rows
   land, which is the point). Registry-only in this phase; snapshots
   regenerate at T2.
2. **Phase T2 — preset rows** (`presets.rs`): every §3.2 row in the
   right const block (Std S0/S1 tail, Emacs S0/S1 tail, Vi S1 + the
   new-context pairs), each with the ruling comment citing this
   plan's table cell. Snapshot regeneration is part of the phase.
3. **Phase T3 — smart toggle router arm** (`router.rs` +
   `adapter.rs`): the `TaskTrackToggle` dispatch arm (both
   directions), the `send_card_track_toggle` re-rooting (the four
   call sites route the registry id), dispatch-time gating tests:
   idle→starts, running-on-target→stops, running-elsewhere→closes
   that and starts this, no-target→NoTarget. Deterministic — no
   sleeps; assert terminal state.
4. **Phase T4 — effort + entries verbs** (`router.rs`,
   `view_state.rs` seam if needed): effort edit focuses the M9
   textbox (the UiEffect/command seam per the M9 commit-on-focus-loss
   law — edit is a *focus* verb, commit stays the field's), effort
   clear sends the sparse `TaskChanges` clear (confirmation-free).
   Typed outcomes (`matches!`), no text assertions.
5. **Phase T5 — hint projections** (`models.rs`, `task-view.slint`):
   the two new hint properties + projection rows; the hint fold test.
6. **Phase T6 — journeys** (evidence files per W-4/W-5):
   `detail_shortcuts_journeys.rs::us4_s1` (done via the new
   TaskDetail binding), `us4_s2` (smart toggle in both directions +
   Board context), `tracker_journeys.rs::us5_s1` (the Board/
   StackDetail F9/C-c C-i/I extensions and the toggle in Board
   context). Tokens embedded in test names/doc comments;
   `on_slint_thread`; the shared directional-journey harness module
   (be5403c7) is the rig precedent.
7. **Phase T7 — census + docs**: dated census entries with **symbol
   anchors** per #775 (`docs/ux-surface-census.org` — the new ids and
   the context extensions; any superseded round-8 rulings dated, D-2
   style); `docs/widget_interactions.org` untouched (chunk 18 owns).

## 6. Files touched (diff estimate)

| File | Change | ~Lines |
|---|---|---|
| `crates/taskboard-ui-slint/src/actions.rs` | 3 new ids + entries + slugs | +45 |
| `crates/taskboard-ui-slint/src/keymap/presets.rs` | ~26 new rows + comments | +140 |
| `crates/taskboard-ui-slint/src/router.rs` | toggle arm + effort arms | +45 |
| `crates/taskboard-ui-slint/src/adapter.rs` | seam re-rooting, effort/entries funnels | ±40 |
| `crates/taskboard-ui-slint/src/models.rs` | 2 hint projections | +25 |
| `crates/taskboard-ui-slint/ui/views/task-view.slint` | 2 hint props + tooltip wiring | +12 |
| keymap snapshots (regenerated) | mechanical | ~30 |
| tests (fold, router, journeys, hints) | T1/T3/T4/T5/T6 | +450 |
| census/docs | dated entries | +25 |

**Total ≈ 800 diff lines** (~60% tests). Size M per dispatch §1.

## 7. Risks

- **R1 — vi chain take-rate**: bare `c` and `F`/`f` are free today,
  but the ViNormal chain is dense; the totality contract tests are the
  oracle and any late collision re-picks within the same mnemonic
  family (`c`→`v` clock? recorded at re-pick). Low.
- **R2 — `C-c i` vs `C-c C-i`** (emacs): distinct by the bare/chord
  rule with precedent, but a mis-stroke hazard; tooltips + the
  recorded comment mitigate. Accepted by this plan (D-table).
- **R3 — effort-edit as a focus verb**: the M9 commit-on-focus-loss
  law means arming via keyboard then Tabbing away must not spuriously
  commit an untouched draft; T4 pins the unchanged-draft no-op.
- **R4 — toggle target ambiguity at Board**: focus vs the quick-bar's
  card; the seams already resolve subject/focused per surface — T3
  tests both contexts explicitly.
- **R5 — hint-model churn**: `task-track-hint` stays Start/Stop-based
  while the toggle binds; the fold test must encode that deliberate
  choice (tooltip names the state-correct verb) so it doesn't read as
  a coverage miss.

## 8. Verification

- Journeys: us4_s1, us4_s2, us5_s1 (+ the fold/hint/seam tests above).
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate`
  (US-4/US-5; MISSING is not done).
- Full quality gates through `scripts/run-quality-gates.sh`
  (subagent discipline: the coverage trio never runs in-stage beyond
  the flock-wrapped nextest; the dispatcher owns full-suite runs).
- Manual (stage-3 note): preset feel on a real keyboard, Dvorak
  re-check per #855.

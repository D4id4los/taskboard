# [Report] Future Directions Analysis — 2026-10-14 Brainstorm

- **Date**: 2026-10-14
- **Context**: the user ran a brainstorming session on where `taskboard`
  should develop after the current milestone (M1 headless core complete;
  Phase 7 / the Slint GUI, tagged v0.1.0, is the next planned work). This
  report turns that raw brainstorm into a structured analysis:
  categorisation by subsystem, architecture fit against the built system,
  effort estimates, and a recommended pursuit order.
- **Method**: every brainstorm item was mapped against (a) the current
  code (entities, commands, reminders model, sync actor, app surface —
  verified by reading the sources, not just the plans), (b) the accepted
  roadmap and phase plans, (c) the existing backlog, and (d) the three
  2026-10-11 viability reports (org-mode support, emacs-style org edit
  backend, generic file-sync connector protocols). Items already tracked
  elsewhere are cross-referenced rather than re-analysed.
- **Companion artefacts**:
  - `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md` —
    the GUI technical requirements distilled as input for the upcoming
    GUI design planning session (the brainstorm's explicit ask).
  - New backlog entries appended 2026-10-14 (see §8) for the items this
    report recommends acting on first.

---

## 1. Where the app stands today (the baseline all fit-judgements use)

Verified in code on `main` @ `1e08ee0`:

- **Core chain complete (M1)**: domain entities (`Board`/`Stack`/`Task`/
  `Label` with per-field LWW write clocks, tombstones, outbox) →
  sqlite storage (actor + migrations + compile-checked queries) →
  state engine (sequential command loop, `ArcSwap<AppState>` publish,
  oneshot receipts) → Nextcloud Deck sync connector (conditional-read
  poll, evidence-then-verdict push) → app bootstrap (figment config,
  keyring secrets, instance lockfile).
- **Frontends over one core**: one-shot CLI (full editing surface:
  boards/stacks/tasks/labels/sync/config/reminders), interactive REPL
  (ADR 0011), daemon IPC over a local socket (ADR 0012). Stand-alone
  by default, sync optional (ADR 0010).
- **Reminders subsystem freshly landed** (design plan of 2026-10-12;
  app wiring merged 2026-10-14): due-anchored reminder rules — the
  domain already models **N rules per task** — delivery proofs
  (set-union-mergeable by construction, i.e. designed with future sync
  in mind), snooze/mute commands, overdue/due-soon helpers, a
  `NotificationSink` port with a `LogSink` v1, and the `tasks list`
  due marker. Desktop toast delivery ("plan 5") and reminder-state
  sync are recorded future work (backlog entries exist).
- **What the Task entity does NOT have yet** (relevant to ~half the
  brainstorm): no scheduled-start (only a single `duedate`), no
  subtasks, no parent/child anywhere, no time tracking, no effort
  estimate, no importance/urgency, no recurrence, no attachments, no
  comments. `description` is a plain string (Deck stores markdown).
- **Structural MVP invariants still in force**: the engine serves
  **one live board** (`CreateBoard` refuses when one exists;
  `AppState` is already map-shaped "so multi-board stays additive"),
  and the app spawns **one sync connector** bound to **one remote
  board** (the outbox gained its `board_id` column 2026-10-12 —
  groundwork, not multi-board itself). `SyncCommand`/`SyncReport`
  vocabulary is connector-agnostic.
- **UI**: `taskboard-ui-slint` is still a doc-comment stub. Phase 7
  (M2 → v0.1.0) has not started — which is exactly why the GUI
  design session is imminent and why its requirements brief matters.

## 2. Effort & fit scales used throughout

Effort (calendar time for this codebase's established pace, one
implementer, TDD included):

| Size | Meaning |
|---|---|
| **S** | ≤ ~3 days; one crate, additive, no design round needed |
| **M** | ~1–3 weeks; 2–3 crates or one focused plan doc |
| **L** | ~3–8 weeks; own design round + phased plan (a "WI"-scale item) |
| **XL** | multi-month epic; strategic decision + several stacked plans |

Architecture fit (against the built system):

| Rating | Meaning |
|---|---|
| *additive* | lands in existing seams (new field+clock, new command, new view) without touching invariants |
| *extension* | new subsystem inside the established vocabulary (new port, new actor, new connector slot) |
| *tension* | requires changing a standing invariant (single-board, single-connector, task-anchored reminders, per-field LWW granularity) |
| *open* | needs research before the architecture question can even be posed |

## 3. Categorised inventory with fit & effort

Every brainstorm item appears exactly once below (GUI-technical items
are excluded here — they live in the companion GUI brief).

### 3.1 Domain & data model (`taskboard-domain` first, then storage/engine)

| Item | Fit | Effort | Notes |
|---|---|---|---|
| Split deadline vs scheduled start | additive | **M** | New `scheduled: Option<DateTime<Utc>>` + clock, mirrors `duedate` exactly (sparse changeset, merge policy, storage column, outbox). Deck has no such field → needs the D1 round-trip decision. Bonus: org mapping already wants it (`SCHEDULED:` lines are preserved verbatim today for lack of a field). |
| Effort estimation field | additive | **S–M** | Scalar duration field + clock; same D1 dependency. Feeds decision-support and pomodoro planning. |
| Importance–urgency matrix (+ derived priority?) | additive | **S–M** + UI | Two small enums (or scores) + decorations. The derived-priority question belongs to decision-support (§3.4), keep the fields dumb. |
| Tags-vs-fields policy (`scheduled`, importance, urgency, habit, respite…) | decision | — | See D1 (§4.1). Must be settled *before* the field family lands, not after. |
| Subtasks (arbitrary depth) + split-into-subtasks + parent state derivation | extension | **L** | `parent: Option<TaskId>` + acyclicity + move semantics + derived parent state (pure domain → proptest heaven). Deck has no subtasks → local-only or custom-field carried. Needs its own design round (backlog entry appended). |
| Hierarchy above boards: Projects ⊃ Boards, recursive Projects | tension | **XL** | Blocked on multi-board (§3.5). Revisit as part of that epic's design; do not pre-build. |
| Repeatable tasks/habits as first-class citizens | tension | **XL** | The hardest pure-domain problem in the brainstorm: recurrence rules, materialisation policy (generate-on-complete vs virtual future instances), standalone-vs-repeating identity, history consistency (the brainstorm itself flags tracking/state-transitions/done-timestamps). Own design round; see backlog entry. |
| Time-tracker task type (never "done") | additive | **S** | A `TaskKind`/flag once time tracking exists (§3.3). |
| "Respite" marking (counts as break activity) | additive | **S** | Flag consumed by break reminders (§3.3) and review statistics (§3.4). Cheap, but only meaningful after those exist. |

### 3.2 Reminders, alarms, timers, calendar

| Item | Fit | Effort | Notes |
|---|---|---|---|
| Two reminder flavours (notification vs insistent alarm) | extension | **M** | Backlog entry exists ("Reminder insistence levels"). Insistence (escalating volume, foreground push) is OS/UI work: needs a real toast sink (the "plan 5" desktop toasts are the prerequisite) plus an overlay-capable surface. Domain change is a rule field. |
| Multiple reminders per task (3 days before / morning-of / at-time) | additive | **S–M** | The domain already stores N rules per task; the CLI `SetReminderRule` verb replaces-with-one — widening the surface is CLI/UI work plus UX design (backlog entry exists). |
| Reminder/notification state synced across devices | tension | **L** | Backlog entry exists ("Sync of reminder rules and delivery proofs"). Delivery proofs were designed set-union-mergeable for exactly this. Deck cannot carry them → depends on the D5 app-data sync substrate. |
| Standalone timers & alarms (tea timer, meditation minimum, wake-up alarm) | extension | **M** | `ReminderRef` is task-anchored today (`Rule(id)` | `Default(task)`); add an `Adhoc` anchor kind + a tiny AdhocAlarm entity. The whole occurrence/snooze/sink pipeline is reusable. Highly synergistic with the existing subsystem; backlog entry appended. |
| User-configurable sounds, pleasant defaults | extension | **S–M** | Asset selection + an audio output path on the sink side; new dependency (audio crate) needs stack approval. |
| Calendar integration + reminders for events | extension | **XL** | New domain entity family (`CalendarEvent`, subscriptions) + recurrence (shared with §3.1) + a CalDAV/WebDAV connector crate. Long-term; parks until the sync expansion wave. |
| WebDAV calendar sync replacing Android calendar apps | extension | **XL** | The above **plus** the Android platform story (§3.8). Explicitly a "replace other apps" ambition — treat as the calendar epic's exit criterion, not a driver. |

### 3.3 Time tracking & focus

| Item | Fit | Effort | Notes |
|---|---|---|---|
| Time tracking for tasks + good export | extension | **M** | New `TimeEntry { task, start, end, user? }` entity + start/stop commands + export (JSON/CSV from pure projections). The *running* timer is app-runtime state, not engine truth (stop stamps on end) — keeps the engine's durability model intact. Natural v0.2 anchor feature. |
| Break reminder system | extension | **M** | Sits on the reminders pipeline with a different trigger source (tracked-work duration, not task due dates). Needs time tracking first. Respite flags feed it. |
| Pomodoro focus mode + concentration overlay | extension | **L** | A focus-session runtime state machine (app layer) + the busiest single view in the brainstorm (countdown, current task, task switcher, quick-note-to-inbox, description/attachments/links, exit). Configurable timings. Depends: reminders flavours, time tracking, inbox capture. UI-heavy. |
| Habit progress tracking + pleasant reminders | tension | **L→XL** | Rides recurrence (§3.1) + day-boundary setting (§3.4) + statistics views. The "pleasant reminder" part is cheap once reminder flavours exist; the history/streak part is the recurrence epic. |

### 3.4 Review, journaling, planning, decision support

| Item | Fit | Effort | Notes |
|---|---|---|---|
| End-of-day review/reflection (text + happiness/energy etc.) + custom prompt templates | extension | **M** | New `JournalEntry`/`ReflectionTemplate` entities + a review command flow + view. Pure additive domain; export rides §3.3's export machinery. |
| Plan-for-next-day function | extension | **M** | Mostly a view + a planner workflow over scheduled/due tasks; cheap after the scheduled field and organiser view exist. |
| Richer journalling with health flags feeding habit/review stats | extension | **M–L** | Typed wellbeing fields (migraine, flu, …) as structured journal attributes consumed by statistics. Extends the review item, not separate architecture. |
| Past-entries view + export + statistics view | extension | **M** | Read-only projections over journal/time entries; pure functions + views. Statistics wants a chart story in the GUI (brief mentions it). |
| Day-start-time setting (night-owl 05:00 rollover) | additive | **S–M** | One preference; but *many* consumers (today view, habit day boundaries, daily task generation) must all consult it — land it **before** the features that depend on it, as part of the deferred `UserPreferences` domain work. |
| Decision support ("what next?", "I have 1 hour") | extension | **S–M** | A pure scoring/ranking function over tasks (importance × urgency × effort-fit × available time) + a view. Excellent proptest target. Needs the §3.1 scalar fields first. |
| Long-term planning: Goals/Projects/Values + review cadence | extension | **L** | New entity family above tasks; conceptually blocked on the hierarchy decision (§3.1) to avoid building two overlapping trees. |
| ADHD support (novelty, interest, challenge, urgency) | open | **S** spike | A research report first (the brainstorm itself says so): survey the space, distil design principles. Afterwards it is a *lens* applied to features (focus mode, novelty surfacing, urgency games) rather than one feature. Cheap, high leverage, do early as input to GUI/feature design. |

### 3.5 Sync & connectivity expansion

| Item | Fit | Effort | Notes |
|---|---|---|---|
| Round-trip app-only fields to Deck (custom fields? attachments?) | open → extension | **S spike**, then **M–L** | **The keystone item** — see D1 (§4.1). A cheap investigation that unblocks ~8 field-carrying features; the brainstorm correctly guesses both candidate vehicles. |
| Multiple simultaneous sync connections (converged) | tension | **XL** epic | Requires: multi-board engine (AppState maps already prepare this; `CreateBoard`'s single-live-board refusal and `min(BoardId)`-style semantics must be redesigned), a connector registry in bootstrap (N actors, N report channels — engine ingestion must fan in), and per-board routing. The biggest structural unlock in the whole brainstorm: projects, work/private separation, and team decks all sit on it. |
| Per-project/deck connector routing + two connectors to one endpoint | tension | (part of above) | A routing table board→connector; the outbox `board_id` column is already the right shape. |
| Multi-user shared-deck operation + per-user time attribution | extension | **M** after multi-board | A user identity (display name) on time entries; Deck's own multi-user editing covers the shared-editing part. Personal vs project content = routing (above). |
| Collaborative seamless editing (CRDT-grade) | open | **XL** | True real-time collaboration needs OT/CRDT field semantics — a different merge granularity than today's per-field LWW. The generic-file-sync report already surveyed `yrs`/`automerge`. Interim reality: Deck's native sharing + per-field LWW already gives *coarse* collaboration between taskboard instances and the web UI; state that as the v1 answer, keep CRDT research parked. |
| Settings sync (which settings roam?) | extension | **M–L** | Classification is a design afternoon (S); transport depends on the D5 substrate. |
| LAN sync server in daemon + device discovery | open | **L** | A taskboard↔taskboard TCP protocol is a new connector kind; the sync-semantics groundwork is the file-sync report's per-device journal design. Security questions the brainstorm raises are real (pairing, trust) — design round required. Parks until multi-device sync semantics exist. |
| Thunderbird e-mail→task integration | open | **M–L** | Candidate shapes: a TB add-on talking to the existing daemon IPC socket (cheapest, reuses ADR 0012), or deep-link/mailto handling. Research spike before committing. |
| Org-mode file 2-way sync | extension | **L** | Already fully analysed: viability report verdict **high viability, 4–7 weeks**, connector slot exists by design (ADR 0010), gated on the orgize spike (2–4 days). Backlog entry exists. Sequencing per the report: org read → render → write-back. |

### 3.6 Text, markup & files

| Item | Fit | Effort | Notes |
|---|---|---|---|
| Org-mode markup text fields (multi-frontend) + org/markdown visualisation | extension | **M** (2.5–5 wks per report) | Viability report Scenario A: rendered-view + source-edit toggle MVP on Slint `StyledText`; WYSIWYG gated on Slint upstream rich-text editing. Shared parser core with the file-sync feature. Backlog entry exists. |
| Parser choice as a synced user setting (org default) | additive | **S** after settings sync | Trivial once D5 exists; default org per the brainstorm. |
| Spell checker for title/description | extension | **M** | New dependency (hunspell/enchant-family) + per-field UI integration. Slint text-widget integration is the risky half — verify in the GUI session. |
| File attachments + file links + named base directories (per-device path mapping) | extension | **M–L** | Deck attachment writes are an existing backlog entry (client surface partial); local blob storage + link metadata + the named-base-dir config map are new. The base-dir indirection is a small, elegant config feature — do it from day one of the attachments design so link paths are portable by construction. |
| Markup help page | additive | **S** | A doc view in the GUI; belongs to the GUI brief's feature surface. |

### 3.7 Views & interaction surfaces (GUI features over core capabilities)

These are GUI deliverables; their *core* prerequisites are the items
above. Effort here is the view work only.

| Item | Prerequisite | Effort | Notes |
|---|---|---|---|
| Today view | scheduled field, day-boundary pref | **M** | The single highest-value desktop view in the brainstorm. |
| Organiser view (next days + calendar entries) | scheduled field; calendar for full value | **M** | Day/week horizon over tasks; calendar column arrives with CalDAV much later. |
| Desktop/tablet overview (scheduled column + habits/time-tracking column) | habits, time tracking | **M** | The brainstorm's two-column layout; kiosk cousin of the organiser. |
| Kiosk kanban ⇄ upcoming-list switching | Phase 7 UI | **S–M** | A display-mode state machine + rotation policy on the kiosk surface. |
| Inbox capture + triage interface | inbox-as-stack convention + quick-capture command | **M** | Core-side cost is tiny (a designated stack + a capture verb); the triage UX is the work. GTD-flavoured; strong synergy with fuzzy search. |
| Fuzzy search over tasks/decks/projects + type limits + jump-to | — (works today on AppState) | **S–M** | Pure domain ranking function (proptest target) + UI palette. Also valuable in the CLI/REPL (`tasks find`). No prerequisites — can land any time. |
| Filter view (tags/labels/duedate/…) | — | **M** | Saved-filter queries as pure predicates over `AppState`. |
| Importance–urgency matrix visualisation | §3.1 fields | **S–M** | One view + task-list decorations (the decorations are the more used surface of the two). |
| PDF export of the day's task list + user-editable template + print | today view data | **M** | New PDF dependency (stack approval); template editability suggests a templating mini-design (data → layout). Print preview is per-platform. |
| Full undo system (leaning undo **tree** + tree view) | — | **L** core + **M** view | Backlog entry exists ("Undo/redo functionality"). The tree variant is the right ambition but is a real design round: command journalling, inverse-command synthesis, interaction with sync (undo *after* a push?), tree navigation UI. Big usability win (confirmation-free destructive verbs — already the CLI's philosophy). |

### 3.8 Platform expansion

| Item | Fit | Effort | Notes |
|---|---|---|---|
| Android target (brainstorm guesses a separate Java UI) | extension | **XL** | The headless-core/thin-client architecture was built for exactly this (four frontends already share one core). Real options: (a) Rust core as an Android library (UniFFI-style bridge) + Kotlin UI — the brainstorm's guess, most conventional; (b) Slint itself renders on Android — least new code, but conflicts with the brainstorm's separate-UI instinct and the keyboard-heavy GUI requirements; (c) a pure-sync relationship (Android app as just another connector client). Prerequisites regardless: multi-device sync story (reminder/settings state included) and OS notification/alarm integration. Strategic decision; park with explicit re-entry triggers (§6). |

## 4. Cross-cutting decisions to settle early (D1–D6)

These five-plus decisions each gate a cluster of features. Settling
them late means migrating data; settling them early costs a design
conversation.

### D1. Field-carrying strategy: dedicated fields vs labels, and the Deck round-trip (the keystone)

The brainstorm asks directly: should `scheduled`, importance, urgency,
habit, respite be tags or struct fields? Analysis:

- **Labels sync to Deck for free** (they *are* Deck labels) but carry no
  semantics: no typed queries, no range checks, no per-field clocks,
  no matrix math, and label sets are whole-set LWW (coarser merge).
- **Dedicated fields** are typed, merge per-field, proptest-able, and
  feed decision-support — but Deck has no native slots for them, so
  they are app-only until a round-trip exists.
- **Recommended shape**: dedicated fields for anything with semantics
  (dates, scores, kinds, recurrence), labels only for free-form
  taxonomy; *bridge* the two worlds with a Deck round-trip strategy:
  Deck custom fields if the API surface suffices, else a sidecar
  (attachment-stored structured document) — exactly the brainstorm's
  two guesses. The **spike is S-effort** (read the Deck API surface,
  write a proof-of-concept against the docker tier) and it unblocks
  ~8 features in §3.1/§3.2. **Do it before the field family lands.**
  The sidecar variant generalises into the D5 substrate.

### D2. Multi-board + hierarchy epic

One epic, strictly ordered: multi-board engine semantics → connector
registry + per-board routing → hierarchy above boards (Projects) →
multi-user team decks. AppState's map shapes and the outbox
`board_id` column were laid for this. The redesign centre is the
single-live-board invariant and board binding/adoption ceremonies.
Nothing in the brainstorm needs the *hierarchy* soon; several things
need *multi-board* eventually. Design round when Wave 4 opens, not
before — but avoid adding anything until then that assumes
single-board forever.

### D3. Recurrence semantics

Materialise-on-completion (history = real completed tasks, future =
rules) vs virtual-instance generation (today's list shows derived
instances). The brainstorm's history-consistency worries (tracking,
stack transitions, done timestamps per occurrence) all get *easier*
with materialise-on-completion + a recurrence rule that re-arms —
which also matches the reminders pipeline's occurrence arithmetic
philosophy. Recommend that bias going into the design round; the
round itself is mandatory before any habit/repeat UI exists.

### D4. Undo architecture

Decide early whether the engine journals commands (replay/invert) or
state snapshots. The undo-tree ambition argues for a command journal
+ inverse synthesis, which also gives the "log the undo tree for
debugging" idea for free. interacts with sync: an undo that crosses a
push boundary is a *new edit*, not a reversion — define that honestly
rather than pretending sync is transactional.

### D5. App-data sync substrate (reminder proofs, settings, journal)

Reminder-state sync, settings sync, and journal/export roaming all
need to sync data Deck cannot represent. Options: the D1 sidecar
(ride the existing connector), the org-file connector (once it
exists), or the file-sync report's per-device journal design (the
long-term multi-device answer, also the LAN-sync groundwork). Pick
one substrate when Wave 4 opens; until then, keep such data
device-local and merge-friendly (the delivery proofs already are).

### D6. ADHD/productivity design principles as an input, not a feature

Run the small research spike early; let its principles (novelty,
interest, challenge, urgency; friction reduction; forgiveness in UX)
inform the GUI and the focus/habit features as they are designed —
not as a bolt-on module afterwards.

## 5. What the brainstorm surfaced that is already tracked

To keep single sources of truth (no duplicate tracking):

| Brainstorm item | Existing home |
|---|---|
| Reminder flavours (insistent alarm) | Backlog "[2026-10-12] Reminder insistence levels" |
| Multiple reminders per task | Backlog "[2026-10-12] Multi-reminder UX surfaces" (+ domain already N-rules) |
| Reminder-state sync | Backlog "[2026-10-12] Sync of reminder rules and delivery proofs" |
| Org markup fields + org-file 2-way sync | Backlog "[2026-10-11] Org-mode markup fields…" + the two 2026-10-11 viability reports |
| Emacs-style org editing backend | Backlog "[2026-10-11] Emacs/org edit-backend crate" + its viability report |
| Undo/redo | Backlog "[2026-10-11] Undo/redo functionality" |
| Multi-board | Backlog "[2026-10-08] Multi-board support" (subsumed by D2 epic when it opens) |
| Deck attachments (writes/download) | Backlog "[2026-10-08] Deck attachment writes and content download" |
| Reminder-delivery pruning | Backlog "[2026-10-12] Age-based pruning of reminder deliveries" |
| Kiosk mode / rotation, second-priority OS targets, OAuth2 | Respective backlog entries |

## 6. Recommended pursuit order

Principles applied: (1) the GUI milestone (v0.1.0) is already
committed and should not be scope-crept — but its *foundation* must
absorb the GUI technical requirements now or retrofit them painfully
(theming/actions/automation are precisely the retrofit-hostile class);
(2) cheap decisions that de-risk expensive features come first (D1
spike); (3) daily-driver value beats ambition until the app is
someone's actual daily driver; (4) structural epics get design rounds
on the eve of their wave, not earlier and not later.

| Wave | Contents | Why here |
|---|---|---|
| **0 — now** | **Phase 7 GUI (M2 → v0.1.0)** with the companion GUI requirements brief baked into the design: theming, widget composition, keyboard-first action dispatch, scaling, the LLM automation harness seam. Plus the **D6 ADHD research spike** (S; informs GUI vocabulary) — and the **D1 Deck round-trip spike** (S; informs whether Wave-1 fields can sync on day one). | The design session is imminent; these are its inputs. Neither spike touches product code. |
| **1 — v0.1.x "daily driver"** | Scheduled-vs-deadline split, effort field, importance/urgency fields + list decorations, day-boundary preference (`UserPreferences` resurrection), N-reminder CLI/UI surface, standalone timers & alarms, today view, inbox stack + quick capture, fuzzy search (`tasks find` first — core-only), reminder toast sink (the reminders "plan 5"), undo *design round* (D4). | All additive, all high daily value, all made safe by the D1 answer. This is the wave that makes the desktop app replace other tools. |
| **2 — v0.2 "focus & reflection"** | Time tracking + export, tracker-type tasks, respite, break reminders, pomodoro focus mode + overlay, end-of-day review + templates, plan-next-day, journal health flags, past-entries/statistics views, PDF export + print. | One coherent theme; rides the reminders flavours and fields from Wave 1. UI-heavy, core-light. |
| **3 — v0.3 "structure"** | Subtasks + split command (design round first), recurrence/habits epic (design round first — D3), habit tracking + streak views, matrix view, organiser view, filter/saved views, undo implementation (if not already in Wave 1–2). | The hard pure-domain problems, each behind its own design round, after the app has daily users to feel the pain they solve. |
| **4 — sync expansion** | Multi-board + connector registry + routing epic (D2), org-file connector (per its report phasing), D5 substrate → reminder-state sync + settings sync, multi-user team decks + time attribution. | The structural unlock wave; everything before it deliberately avoided depending on it. |
| **5 — platform & ecosystem** | Android (strategic call: bridge vs Slint-on-Android vs sync-client), CalDAV calendar epic, LAN sync server, Thunderbird, collaborative-editing research. | Each is XL, each has Wave-4 prerequisites, none loses value by waiting. |

Explicitly **not yet**: collaborative CRDT editing, Projects hierarchy,
calendar replacement ambitions, adaptive UI (parked as research; the
GUI brief lists it as a session question).

## 7. Risks & mitigations

- **Field sprawl before the round-trip answer** → D1 spike is the gate
  for Wave 1's field family; if the spike says "no good vehicle", the
  fields still land (local-only) *knowing* the migration story.
- **Wave 1 leaking into the v0.1.0 GUI milestone** → the GUI brief
  marks which requirements are foundation (must be in Phase 7's
  design) vs feature surface (post-1.0 views). Keep Phase 7's exit
  criterion unchanged.
- **Recurrence designed under time pressure** → it is the item most
  likely to be "needed yesterday" once habits appear in the UI; the
  design round is scheduled in Wave 3 *before* any habit UI, and D3's
  recommended bias is recorded so the round starts from a position.
- **Undo interacting with sync** → D4 must state the pushed-edit rule
  (undo after push = new edit) up front; do not let the undo tree
  imply transactional sync it cannot have.
- **Brainstorm items silently tracked twice** → §5 table is the
  cross-reference; new backlog entries (§8) cite this report.

## 8. Backlog entries appended with this report

1. `[2026-10-14] Deck round-trip strategy for app-only fields` (the
   D1 spike — recommended for Wave 0).
2. `[2026-10-14] Scheduled-start field split from deadline` (Wave 1).
3. `[2026-10-14] Scheduling scalar fields: effort, importance, urgency`
   (Wave 1; folds the matrix-field question).
4. `[2026-10-14] Time tracking subsystem` (Wave 2 anchor).
5. `[2026-10-14] Standalone timers & alarms without tasks` (Wave 1).
6. `[2026-10-14] Recurring tasks & habits — domain design round`
   (Wave 3 gate).
7. `[2026-10-14] Subtask hierarchy & task split — design round`
   (Wave 3 gate).

Long-tail items (Android, calendar, LAN sync, Thunderbird, CRDT
collaboration, adaptive UI, spell checker, …) deliberately stay
*only* in this report's §3 tables until their wave approaches —
appending ~15 stub entries now would duplicate the roadmap without
adding information.

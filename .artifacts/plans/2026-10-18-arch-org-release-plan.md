# [Plan] Architecture & Release Docs — rewrite, ADR 0013, CHANGELOG, flow model

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 12 of the remediation-wave-1
  dispatch (dispatch §1/§13, prompt 12.1). Planning only — no
  implementation in this commit.
- **Origin (normative)**: issues **#264** (F15-01/F15-02/F15-03),
  **#266** (F15-20, plus its explicit instruction to fold the
  F15-21/F15-22 normalization into this pass), **#301** (F2-01,
  standing decision (b) of the audit session). The ADR-0013
  formalization rides this chunk per the dispatch acceptance and
  agenda row P-3 (F15-16, folded on the pre-existing backlog entry
  → issue **#223**, unscheduled — this PR closes it). Owning
  report: `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-docs-drift-report.md`
  (§2, §6, §7, §9). Flow-model origin (normative for the
  promotion): `.artifacts/reports/2026-10-16-v0.1.0-audit-flow-model-report.md`
  (§1 method, §3 actors, §4 graph, §5 shutdown, §6 edge table,
  §10 mode matrix). ADR source text:
  `.artifacts/plans/2026-10-12-reminders-design-plan.md` §12 (draft
  ADR) + §13 (user-resolved decisions, 2026-10-12).
- **Branch / worktree**: `docs/arch-org-release` in
  `../taskboard-WT-docs-arch-org-release`, cut off `main` @
  `76838a1` (= `origin/main`).
- **Anchor base**: every `file:line` below was re-verified against
  `76838a1` at planning time (the audit's evidence anchors are @
  `v0.1.0` = `925f50e`; **101 commits intervened** — the audit's
  line numbers are *not* transcribed into the rewrites; §reality
  carries the HEAD-verified replacements). Stage 2 re-verifies each
  anchor the day it writes (dispatch prompt 12.2 rule).
- **Size**: M (per the dispatch's grade).

---

## §shape — the four deliverables (and only these)

1. **`docs/architecture.org` rewrite** — three findings, three
   scoped edits: F15-01 (Executive Summary oversells modes), F15-02
   (channel-ownership sentence + forwarder attribution), F15-03
   (reminders subsystem absent). Issue #264.
2. **ADR 0013 formalized** — `docs/adr/0013-…org` from the reminders
   design plan §12/§13, plus the consequential one-line amendment of
   ADR 0015's reservation note. Issues #223 (closes) / F15-16.
3. **CHANGELOG 0.1.0-section corrections** — the reminders feature
   family entries (F15-20), the F15-21 contradictory-pair merge, the
   F15-22 category normalization. Issue #266.
4. **Flow model promoted** — `docs/flow_model.org`, re-derived at
   HEAD from the audit chunk-2 report, with the documented
   six-pass regeneration procedure (release-anchored). Issue #301.

Docs-only: the diff contains no code, no `Cargo.toml`, no workflow,
no test changes. Gates still run in full (AGENTS §10; pre-commit
hooks fire on docs-only trees too).

## §reality — the code facts the rewrites must match (anchors @ 76838a1)

Stage-1-verified replacements for the audit's tag anchors. Stage 2
re-checks each before writing prose that depends on it.

| Fact (finding) | Anchor @ HEAD | Was @ v0.1.0 |
|---|---|---|
| Mode gate: explicit `daemon` refuses `Desktop`/`Kiosk`, typed bail "not implemented yet" (F15-01) | `main.rs:251-257` | :252-258 (same shape) |
| `AppMode`: `Daemon` = `#[default]`, `Desktop`, `Kiosk` (F15-01) | `config.rs:87-94` | :89-90 |
| Bare-invocation routing: Desktop→GUI, Daemon+TTY→REPL, else daemon lifecycle (F15-01) | `ui.rs:79-84` (`bare_invocation_route`) | :79-87 |
| Bootstrap-created channels: system broadcast + both sync pipes + notifier feed (4 constructors) (F15-02) | `bootstrap.rs:591-595` | :568-572 |
| Engine inbox (cap 256) + engine-signal bus (cap 64) constructed in `spawn_state_engine` (F15-02) | `state/actor.rs:259-260` | :259-260 (unchanged) |
| Storage inbox (cap 64) constructed in `spawn_storage_actor` (F15-02) | `storage-sqlite/actor.rs:85` | :85 (unchanged) |
| UI-command channel (cap 64) constructed in `run_desktop`; forwarder spawned there, *not* by bootstrap (F15-02) | `ui.rs:179-180` (`spawn_ui_command_forwarder` fn :46) | :179-180 |
| IPC inbound + stop watch (F15-02) | `ipc/server.rs:155-156` | :155-156 |
| REPL handshake pairs + headless orphan (F15-02) | `repl.rs:176-177`, `:377` | same |
| Reminder tick arm (`sleep_until`, first tick immediate, disarmed in drain) (F15-03) | `state/actor.rs:463`, fn `reminder_tick` :779 | :463 |
| Durable-then-notify: markers persisted before send; `send` (not `try_send`) backpressures the tick (F15-03) | `state/actor.rs:830` (`send_notifications`) | :832 |
| Notifier consumer task + `NotificationSink` port + `LogSink` v1 + cap 64 (F15-03) | `notifications.rs:71-73`, `NOTIFIER_CAPACITY` :17, `LogSink` :46-48 | :75, :49-62 |
| `[reminders]` config: `enabled`, `tick_secs`, `default_before` (+ validation) (F15-03) | `config.rs:212-228`, `:595-600` | :212-228 |
| `tasks remind` verb family: `--before`/`--list`/`--clear`/`--off`/`--on` (F15-03) | `cli.rs:390-412` | :394-406 |
| Four reminder tables (F15-03) | `crates/taskboard-storage-sqlite/migrations/0004_reminders.sql:12,19,26,33` — note the real path carries the crate prefix; the audit's `migrations/…` shorthand is not a repo path | `0004_reminders.sql:12-33` |
| Engine-envelope vocabulary post-chunk-1 rename: `EngineEnvelope::{Execute, Flush, ReadState, AttachConnector}` (all findings' prose) | `state/actor.rs:43`, uses :126-215 | `EngineCommand` |
| Daemon shutdown tail is now the typed path (flow artifact §shutdown) | `main.rs:323-325` `shutdown_from_arc` | `Arc::try_unwrap` expect |

Also re-verified at HEAD: chunk 5's clock seam (`SyncActorConfig`)
and chunks 6/10 added **no** channel constructors or spawn sites —
the topology itself (13 actors / 36 edge rows / 23 constructors per
the audit) is expected to be unchanged; only anchors and names
moved. Stage 2 confirms by re-running pass 1/2 of the method, not by
transcribing.

## §arch-outline — `docs/architecture.org` per-finding edit outlines

### F15-01 — Executive Summary (P2)

Wrong today (architecture.org:17-25): the summary opens "designed to
operate in two distinct operational modes: 1. *Passive Kiosk Mode* …
2. *Interactive Desktop Mode*" — presenting both as current
operation. Reality: `Daemon` is the default mode, `Desktop` shipped
(ADR 0015 GUI), `Kiosk` is refused at the explicit-daemon gate with a
typed error (`main.rs:251-257`); bare-invocation routing
(`ui.rs:79-84`) never launches a kiosk frontend.

Rewrite decision (recorded per the finding's either/or): **shipped
reality first, vision kept with an explicit status sentence** — the
summary names the three `AppMode` values and their status
(daemon-default, desktop GUI shipped per ADR 0015, kiosk planned
[the accepted kiosk-wave decisions, backlog notes of 2026-10-16] and
refused at the gate until it lands), then keeps the two-mode product
vision paragraph as vision. The header block (:4-13, title/target-HW
lines) stays: it is the product's identity statement (AGENTS §1),
not an operational claim. The standalone-first paragraph (:32-39)
already matches code (audit §2.1 verified) — untouched.

### F15-02 — channel ownership (P2)

Wrong today, three sites:

- :137-141 — "All inter-actor channels — the system broadcast, both
  sync =mpsc= pipes, and the storage/engine command inboxes — are
  *bootstrap-created* and owned by =taskboard_app::bootstrap= (ADR
  0008); actors receive their halves as injected constructor
  arguments and never construct channels between themselves."
  False for two of its four enumerated families: the engine inbox +
  engine-signal bus are constructed in `spawn_state_engine`
  (`state/actor.rs:259-260`, the state crate), the storage inbox in
  `spawn_storage_actor` (`storage-sqlite/actor.rs:85`).
- :105-106 — "the bootstrap-owned UI-command forwarder".
- :122 — comm-table row "drained by the bootstrap-owned UI-command
  forwarder": the forwarder is spawned in `run_desktop`
  (`ui.rs:180`), the app crate's `ui` module, and the channel it
  drains is created there too (`ui.rs:179`).

Rewrite (the finding's prescribed remedy, kept verbatim in intent):
reword to **"wired and handed over by bootstrap; constructed in the
owning actor crates' spawn functions"** — the paragraph keeps the
true invariants (actors receive halves as injected constructor
arguments; no actor constructs channels between itself and another
actor) and states the real construction split: `bootstrap()`
constructs the system broadcast, both sync pipes, and the notifier
feed (`bootstrap.rs:591-595`); engine/storage/UI-command/IPC/REPL
channels are constructed inside their owning spawn/hand-over
functions (`state/actor.rs:259-260`, `storage-sqlite/actor.rs:85`,
`ui.rs:179`, `ipc/server.rs:155-156`, `repl.rs:176-177/:377`) —
every one a function the app layer calls. Fix both forwarder
attributions (:105-106, :122) to name `run_desktop` /
`taskboard_app::ui::spawn_ui_command_forwarder` honestly.

### F15-03 — reminders subsystem absent (P2)

Wrong today: zero reminders content — no notifier row in the comm
table (:120-135), no tick arm in the state bullet (:166-180), no
reminder tables in the storage bullet's schema enumeration (:199-201),
no `[reminders]` config / `tasks remind` verbs in the app bullet
(:235-300; the CLI enumeration at :253-254 stops at
"label/unlabel").

Additive edits (four sites):

1. **Comm table row** (after the "State Engine → Sync Actor (reads)"
   row, matching the table's mechanism/type/description shape):
   *State Engine → Notifier* — =tokio::sync::mpsc= — `Notification`
   payloads (cap 64, `NOTIFIER_CAPACITY` `notifications.rs:17`),
   constructed in `bootstrap` (`bootstrap.rs:594-595`), consumed by
   the notifier task (`spawn_notifier` `notifications.rs:71`);
   delivery is durable-then-notify (markers persisted before any
   send; a failed batch fires nothing), `send` backpressures the
   tick; the consumer exits when the engine's exit closes the
   channel; v1 sink = `LogSink` (ADR 0013).
2. **State crate bullet** (taskboard-state, after the SyncReport /
   nudge sentence): the reminder tick arm — a `sleep_until` select
   arm (first tick immediate so pre-boot-undelivered reminders fire
   on the next boot; disarmed when the engine drains) that runs the
   pure evaluator over `EngineCore`, persists delivery markers,
   interprets, publishes, and only then notifies (ADR 0013).
3. **Storage crate bullet**: extend the schema enumeration with the
   four reminder tables of `0004_reminders.sql`
   (`reminder_rules`/`reminder_deliveries`/`reminder_snoozes`/`reminder_muted`).
4. **App crate bullet**: `notifications` (the `NotificationSink`
   port + `LogSink` v1, desktop toasts are a follow-up behind the
   port), the `[reminders]` config table
   (`enabled`/`tick_secs`/`default_before`, validated), and
   `tasks remind`/`reminders`/`clear-reminders` verbs
   (`--before`/`--list`/`--clear`/`--off`/`--on`) in the CLI
   enumeration.

Each site cross-links ADR 0013; the comm-table row and app bullet
also point at `docs/flow_model.org` (E-20/NTF).

**Boundary**: #267's findings (F15-04 boot-pull subscriber,
F15-05 dep lists, F15-06 versioning example, F15-07 task-sorting
bench row) share the file but stay **untouched** — adjacent lines
are not drive-by fixed; #267 keeps them.

## §adr-0013 — formalization skeleton + recorded amendment reasons

File: `docs/adr/0013-reminders-due-anchored-rules-engine-tick-ported-delivery.org`
(house naming: `NNNN-kebab-title.org`; length in line with 0012's).
Format per 0004/0015: org, `#+title:`, `#+options: toc:nil`,
`-*Date:*` / `-*Status:*` / `-*Deciders:*` / `-*Depends on:*`,
then `* Context`, `* Decision` (numbered), `* Consequences`.

- **Date**: 2026-10-12 (the decision date — §13 "resolved by user
  review 2026-10-12").
- **Status**: `Accepted (decided 2026-10-12, design round WI-I;
  formalised 2026-10-18 — the late landing is the F15-16
  execution accident, see
  .artifacts/reports/2026-10-16-v0.1.0-audit-theme-docs-drift-report.md §6)`.
  Honest history, per the audit's resolution (execution accident,
  not withdrawal).
- **Depends on**: ADR 0001 (multi-actor core), ADR 0005 (persistence
  port precedent), ADR 0006 (engine interpret loop).
- **Context**: the §12 draft's context (taskboard's identity
  includes "reminder system"; `duedate` is synced per-field LWW; the
  engine owns transitions; Deck has no reminder field; kiosk reboots
  at arbitrary moments), plus one added sentence anchoring the
  design-round origin (plan §12/§13 permalink).
- **Decision** — the §12 eight-point list, **amended where §13
  resolved differently or more precisely than the draft**, each
  amendment carrying its recorded reason:
  1. Rules are domain entities anchored on `Task.duedate`
     (offset ≤ 0), local in v1, sync-ready model (portable ids,
     occurrence-keyed additive delivery proofs, per-field write
     clocks as the additive path). *Amendment (§13 round-2
     commitments): rule/delivery sync is desired-but-deferred with
     the sync-ready model — recorded as a design commitment, not a
     v1 feature.*
  2. Occurrence-keyed suppression: a `ReminderDelivery` row is the
     single at-most-once guard, persisted before any notification;
     due-date moves re-arm by derivation; snooze reschedules the
     occurrence through the same pure evaluator. *Confirmed by D4
     (single fire per occurrence + persistent banner + snooze as
     the re-surface mechanism; repeating nags deferred).*
  3. Scheduling is an engine tick arm (`sleep_until`), not a second
     actor. *Confirmed by D2 (Option A of the design plan's §5.1).*
  4. Delivery is a port (`NotificationSink`); v1 ships `LogSink`.
     *Confirmed by D3 (desktop toasts remain follow-up plan 5).*
  5. Kiosk overdue banner is scheduler-free, a pure derived view.
  6. `AppState` grows persisted reminder facts only;
     `EngineSignal::StateUpdated` stays the sole repaint signal; no
     new `SystemEvent` variant.
  7. Implicit default reminder per duedated task at
     `[reminders] default_before` (default 5 m), per-device; explicit
     rules replace the default; `--off` mutes firings without
     touching the banner. *Amendment (D5): shipped as a config item
     from day one (figment already carries the hierarchy; the
     evaluator takes the offset as a pure parameter) — reason
     recorded from §13.*
  8. N rules per task and insistence levels are additive future
     extensions; no surface assumes one-reminder-per-task. *Confirmed
     by §13 round-2 (backlog entries added that round).*
  Plus the D1 restriction stated up front: **v1 rule catalog is
  offset-before-due only** (fixed-time and per-board defaults
  deferred — reason recorded from §13).
- **Consequences** — as shipped (HEAD-verified): the tick arm
  (`state/actor.rs:463`, first tick immediate, disarmed in drain),
  durable-then-notify with backpressure (`send_notifications`
  :830), four tables (`0004_reminders.sql`), `[reminders]` config,
  the `tasks remind` verb family, the GUI banner/snooze riding the
  reminders command path; deferred: desktop toasts
  (`DesktopSink` behind the port), rule/delivery sync, N rules,
  insistence levels. Cross-links: `docs/architecture.org` (comm
  table row + crate bullets), `docs/flow_model.org` (E-20/NTF).

**Consequential edit — ADR 0015:19-23**: the reservation note
("The ADR numbering skips 0013 deliberately … not reused here to
keep the reservation honest") gains one amendment line after it:
`Amendment 2026-10-18 — ADR 0013 is formalised from the reminders
design plan §12/§13; the reservation is retired exactly as
intended.` Recorded reason: leaving the note unamended would assert
0013 still does not exist. The historical note text itself is not
rewritten. Post-landing sweep: `grep -rnE '0013' docs/ CHANGELOG.md`
— every hit must resolve to an existing file or the 0015 amendment.

## §changelog — 0.1.0-section entries + normalization (issue #266)

Voice (matched from the existing section): full sentences, present
tense, ADR refs in parentheses, concrete verb/config names, no
internal file paths.

1. **Reminders entries (F15-20)** — added under the 0.1.0 `Added`
   block, one entry per user-visible surface (final phrasing at
   implementation, facts HEAD-verified):
   - the subsystem entry: due-anchored reminder rules (ADR 0013),
     engine-tick evaluation with the first tick immediate
     (pre-boot-undelivered reminders fire on next boot),
     durable-then-notify delivery (markers persist before any
     notification; a failed batch fires nothing);
   - the verb entry: `tasks remind` family — set/replace with
     `--before`, list, clear, mute/unmute `--off`/`--on`; implicit
     default reminder per duedated task;
   - the config entry: `[reminders]` table — `enabled`,
     `tick_secs`, `default_before`;
   - the delivery entry: notifier v1 logs one line per delivery
     (`LogSink`); desktop toasts are a follow-up behind the same
     port.
2. **F15-21 merge**: the pair is `:136-138` ("The daemon no longer
   refuses the `standalone` backend — ADR 0010 decision 9 amended",
   the final behavior) vs `:187-189` ("The daemon refuses to start
   (typed `sync-disabled`) when no connector is configured", the
   superseded statement). The stale entry is removed and its
   content folded into the surviving entry so the section states
   the final behavior once.
3. **F15-22 normalization**: the 0.1.0 section's seven category
   blocks (`Added` :20, `Fixed` :124, `Changed` :140, `Changed`
   :182, `Changed` :325, `Fixed` :346, `Added` :377) merge into one
   block per category in the section's existing order (Added,
   Fixed, Changed). Entries move verbatim — no rewording beyond the
   F15-21 merge and the new reminders entries. **Integrity proof**:
   `scripts/changelog.py self-test` green and
   `scripts/changelog.py extract 0.1.0` renders the merged section
   (the tooling has no normalize subcommand — `promote`/`extract`
   parse full-line `## ` headings only, so `### ` block moves are
   provably safe for it); additionally count bullets per category
   before/after (must be equal + the new entries).
4. **No `[Unreleased]` entry for this chunk** — docs-only changes
   do not enter the changelog (established practice: the ADR 0007
   amendment and EngineEnvelope doc-sweep commits added none). The
   `[Unreleased]` section is left exactly as found.

Out-of-scope here: F15-23 (optional dev-facing MCP entry) — stays
with #269.

## §flow-artifact — `docs/flow_model.org` (issue #301)

Filename/title per house convention (snake_case like
`coding_guidelines.org`): `docs/flow_model.org`,
`#+title: taskboard Data & Control Flow Model`, `#+options: toc:t`.

Section map — promoted from the audit chunk-2 report, **re-derived
at HEAD** (the promotion base is post-v0.1.0 per issue #301; the
report's tables are the v0.1.0 derivation, the artifact's are the
HEAD derivation, diffed against the report so drift is visible):

1. **Preamble** — what this is: the derived actor/edge model of the
   shipped runtime; release-anchored (regenerated at each tag);
   anchor-base commit of this derivation; origin permalink (the
   audit chunk-2 report) + the derivation date.
2. **Method — the regeneration procedure** (the heart of #301's
   "regeneration procedure written down"): the six passes from the
   report's §1, each with its exact grep and the exclusion rule
   (ignore `#[cfg(test)]` modules, `tests/` dirs, doc comments):
   pass 1 channel constructors (five constructor greps), pass 2
   spawn sites, pass 3 wiring trace from the three roots
   (`bootstrap`, `run_desktop`, `main::run`), pass 4 ArcSwap
   writer/reader enumeration, pass 5 shutdown routes, pass 6 mode
   matrix; plus the mermaid validation fallback (conservative label
   subset + structural lint when no `mmdc`). Written so a fresh
   agent can execute it end-to-end at the next tag with no other
   context.
3. **Actor inventory** — the 13-row table (ID, actor, crate·module,
   substrate, spawn/entry site, inboxes, join owner), anchors at
   HEAD.
4. **Main runtime graph** — the mermaid `graph LR` with hub nodes,
   the ArcSwap shared-state node, dashed reads, dotted
   shutdown-only flow.
5. **Shutdown routes** — trigger table + `App::shutdown` sequence
   diagram, **updated for the typed tail**: `shutdown_from_arc`
   (`main.rs:323-325`) replaced the `Arc::try_unwrap` expect
   (chunk 7); the ordered teardown steps are otherwise as derived.
6. **Edge table** — E-01..E-36 with constructor/send/recv anchors
   at HEAD; payloads say `EngineEnvelope` (post-rename); any
   `push_exec.rs`-style path cites the module tree (post-chunk-10).
7. **Process-mode matrix** — the six mode rows.
8. **Maintenance** — the release-anchor rule: re-derive at every
   `vX.Y.Z` tag as part of the release cut; diff against the
   previous derivation; record the diff in the preamble's
   derivation log; budget = six greps + a reading pass; a topology
   change that contradicts `docs/architecture.org` is either fixed
   in the same pass (if it is a row this doc owns) or filed as an
   issue — never silently absorbed.

Stage-2 obligations: re-run passes 1/2 (constructor + spawn greps)
at HEAD and reconcile against the report's 23 constructors / 13
actors before promoting any table row; every line anchor
re-resolved (known drift: bootstrap +23 lines, the rename, the
shutdown tail, push_exec module tree); mermaid blocks validated
(conservative subset + structural lint at minimum).

## §commits — stage-2 commit series (each standalone, rebase-safe)

1. `docs(adr): formalize ADR 0013 from the reminders design plan` —
   the ADR + the ADR 0015 amendment line (one coherent landing).
2. `docs: promote the audit flow model into docs/flow_model.org` —
   the re-derived artifact (self-contained; no inbound links yet).
3. `docs(arch): state shipped modes and kiosk status in the summary`
   — F15-01.
4. `docs(arch): correct the channel-ownership and forwarder claims`
   — F15-02 (all three sites).
5. `docs(arch): document the reminders subsystem` — F15-03 (four
   sites; cross-links ADR 0013 + flow_model.org, both already
   landed — no dangling links at any commit).
6. `docs(changelog): reminders entries, sync-disabled merge, KaC
   normalization` — F15-20/F15-21/F15-22 with the self-test proof
   in the commit body.

## §non-goals

- **No code edits whatsoever** — not even comment-only ones (chunk
  13's #246 owns those).
- **No #267 fixes** (F15-04/05/06/07) — same file, different issue;
  adjacent lines stay untouched even where an edit sits nearby.
- **No F15-23** MCP changelog entry (optional; #269's call).
- **No new architecture claims** beyond the findings' remedies and
  the HEAD-verified reads in §reality — every added sentence traces
  to a findings row, the design plan §12/§13, the flow-model
  report, or an anchor re-verified the day it is written.
- **No CHANGELOG `[Unreleased]` edits**; no rewording of existing
  0.1.0 entries beyond the F15-21 merge (released history is
  corrected additively per the session ruling, not rewritten).
- **No redesign of the reminders story** — deferred items (toasts,
  rule sync, N rules, insistence) are named as deferred, never
  promised.
- **No backlog/issue edits**; tracker closure rides the PR body.

## §risks — claims needing re-verification against HEAD at stage 2

- **Anchor drift is ongoing**: 101 commits landed between the audit
  tag and this plan; more may land before stage 2 runs (chunk 11's
  branch is still open — CI files only, no anchor risk; chunk 13
  will touch testing docs, not these anchors). Stage 2 re-verifies
  every anchor the day it writes; the plan's §reality table is the
  checklist.
- **Flow-model re-derivation may not match the v0.1.0 model** — if
  pass 1/2 greps at HEAD yield ≠ 23 constructors / ≠ 13 actors,
  that is either real topology drift (record in the artifact's
  derivation log; fix the artifact, file an issue if
  architecture.org now contradicts it) or a derivation error
  (fix the derivation). Never silently transcribe the old table.
- **Mermaid rot**: the artifact carries two diagram blocks; without
  render tooling, regressions hide. Mitigation: the report's
  validated blocks are the base (parse-checked at audit time), the
  edits are anchor/label-only, and the structural lint is re-run.
- **CHANGELOG normalization** is a large mechanical reorder of a
  447-line file — the risk is silent content loss. Mitigation
  (§changelog.3): per-category bullet counts before/after +
  `changelog.py self-test` + `extract 0.1.0`.
- **F15-01 status-sentence wording**: the gate's bail string still
  says "(Phase 7)" while Phase 7 shipped the desktop GUI — the doc
  must describe the gate's *behavior* (typed refusal of the kiosk
  frontend; kiosk is planned), not transcribe the string. The
  kiosk-wave reference is backed by the accepted kiosk-wave-1
  decisions (2026-10-16 backlog notes) — cite the plan artifact,
  not a milestone promise.
- **ADR 0013 must not read as retroactive invention**: status,
  date, and the F15-16 pointer make the late formalization
  explicit; decisions carry their §13 confirmation/amendment
  reasons so the record is auditable against the design plan.
- **Bookkeeping (stage 2/3, wave rule)**: while in the CHANGELOG,
  the wave's *behavior* fixes since v0.1.0 (stale-binding
  re-resolve, userinfo/cleartext hardening, typed shutdown,
  subsumed-label fix, release overflow-checks) have no
  `[Unreleased]` entries — file **one** new GitHub issue
  (documentation, p2) covering the gap so the next release cut
  doesn't silently miss them; do not fix it inside this chunk.

## §acceptance — mapped to dispatch §13

- architecture.org states the true channel topology (F15-02:
  quote-contrast old vs new in the PR body), documents the
  reminders subsystem (F15-03: notifier row + three crate-bullet
  extensions), and stops overselling modes (F15-01: shipped-reality
  summary + status sentence).
- ADR 0013 exists in house format; every decision traces to design
  plan §12/§13 with recorded reasons for amendments; it
  cross-links architecture.org; ADR 0015's note is amended; the
  `0013` grep sweep resolves clean.
- CHANGELOG 0.1.0 section carries the reminders entries; the
  contradictory daemon pair is merged; one category block per
  category; `changelog.py self-test` green.
- `docs/flow_model.org` exists with the six-pass regeneration
  procedure a fresh agent can execute; tables re-derived at HEAD.
- Full gate suite green; diff is docs-only; PR body: summary +
  `Closes #264` + `Closes #266` + `Closes #301` + `Closes #223`.

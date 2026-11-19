# Round 9 CLI-Parity Sweep — Inventory Report

- **Date**: 2026-11-18
- **Kind**: round-9 sweep (operator ruling → complete inventory of
  CLI-only / GUI-absent-with-CLI-justification capabilities; grouping
  into round-9 joins vs. backlog issues; census supersession text).
- **Status**: inventory + backlog filing only — no code, no rulings
  made; supersession notes are *proposed text* for the round-9 plan.

## 1. The ruling (verbatim, operator, 2026-11-18)

> "CLI-only features were only acceptable while we had no GUI crate or
> during the journey to full GUI parity. That point is reached. No
> feature is allowed to be CLI-only, none whatsoever. Going forward:
> complete GUI↔CLI feature parity (preferred), or GUI-only in special
> cases where a CLI implementation proves too onerous. Every existing
> 'the CLI covers it' deferral is invalid."

**Scope**: every capability whose GUI absence was justified by CLI
coverage (recorded ruling, plan scope line, or CLI-only register
entry) is invalidated regardless of the recording date. The ruling is
asymmetric by its own letter: GUI-only absences ("special cases where
a CLI implementation proves too onerous", plus pure GUI-interaction
state) remain permissible; the *reverse-direction* `U` cells (GUI
exists, CLI verb absent — the T-24/#807, RW-2/4/5, SC-2, HB-4, RC-3,
BF-8 families) are **not** invalidated.

## 2. Invalidation table

IDs CP-1… assigned by this sweep. "CLI verb" cites
`crates/taskboard-app/src/cli.rs`; "GUI status" cites the census row
(`docs/ux-surface-census.org`) unless noted.

| ID | Capability | CLI verb (cli.rs) | GUI status (census) | Authority that justified the absence | Invalidated? |
|---|---|---|---|---|---|
| CP-1 | habit.edit (GUI editing incl. target retargeting) | `habits update` `cli.rs:1340` | Today row `— (R-2)`; all GUI cells `— (R-2)` | Today-slice ruling R-2, 2026-11-13, `.artifacts/plans/2026-11-13-r7-habit-create-ux-plan.md` §2 ("the CLI's `habits update` covers; not requested this round"); triage T-25 ledger #808 | **YES** — but already re-homed: the round-9 habit-parity UX plan overturns R-2 (`2026-11-18-habit-parity-ux-plan.md` supersession table); implementation = chunk H4 |
| CP-2 | habit.remove (GUI deletion) | `habits rm` `cli.rs:1388` | all GUI cells `— (R-3)` | Today-slice ruling R-3, 2026-11-13, same plan ("the CLI's `habits rm= covers; the Today row grammar stays log/reorder/repair/convert"); T-25/#808 | **YES — NOT homed anywhere.** The round-9 habit-parity plan's R-3 is a *different* ruling (convert-submenu placement); research Q6 ("is GUI habit removal in scope?") was answered by omission. → backlog issue + #808 comment |
| CP-3 | habit description (optional field) | `habits update --description` enters with H1 (`cli.rs:1262+` family) | no field anywhere (domain → UI); HAB-2, `2026-11-18-round9-habit-parity-research.md` §2 | No prior ruling — plain absence; the CLI would have been the only front | YES by the ruling's letter → round-9 chunks H1 (domain+CLI) + H4 (GUI editor) |
| CP-4 | habit optional day (per-week/per-month anchor day) | CLI flags enter with H1 | not representable; HAB-4, research §2/§5 Q1 | plain absence (domain gap) | YES → round-9 H1 (O-1 anchor-day windows) + H4 |
| CP-5 | habit optional time (GUI edit + chip upgrade) | `habits update` anchor flags (`cli.rs:1525` Set/NaiveTime grammar) | anchor chip exists, GUI edit absent; HAB-3 | plain absence | YES → round-9 H4 |
| CP-6 | board.archive — the **archive direction** (GUI can only *un*archive via the US-11 roster) | `boards archive` / `boards unarchive` `cli.rs:396-402` | Board cell E (roster restore only); TaskDetail/Today/Review/Settings/Capture cells `U (BF-2)` | Board-stack plan US-10 history note, `.artifacts/plans/2026-11-14-board-stack-management-ux-plan.md:456` — "the archive verb itself stays CLI-only (chunk 4's board-verb family)"; chunk 4 never landed the archive direction; census Board slice row `board.archive` carries `U (BF-2)` on the five non-Board cells | **YES — the 'until chunk 4' deferral is exactly a 'the CLI covers it' deferral.** → backlog issue (board context-menu archive verb; organic small chunk) |
| CP-7 | board.reset | `boards reset --confirm` `cli.rs:424` | all GUI cells `— (R-10)` | R-10, `.artifacts/plans/2026-11-15-ux-r4-board-entry-points-chunk-plan.md` D-2, 2026-11-15 ("GUI absence is by justification… revisit note") | **YES** — the justification reduces to CLI coverage ("revisit" note = the deferral this ruling voids). Destructive verb → armed-confirm grammar; → backlog issue |
| CP-8 | board.reconcile | `boards reconcile --confirm` `cli.rs:450` | all GUI cells `— (R-11)` | R-11, same chunk plan D-3, 2026-11-15 ("informed-consent letter… revisit note") | **YES** — informed-consent is a *presentation* requirement, not a GUI-absence justification; the letter should be built, not deferred. → backlog issue |
| CP-9 | export (tracking CSV/JSON; pdf/fold leg) | `export` `cli.rs:161` | none — CLI-only register (entity-less, chunk-2 D-10), `docs/ux-surface-census.org` "The CLI-only register" | Census chunk-2 D-10 recorded it CLI-only-by-shape ("a matrix row of one cell defeats the surface-scoped grammar") — a *bookkeeping* ruling, not a capability ruling | **YES — the largest genuine gap**: a whole feature family (pdf/csv/json export with filters) has no GUI front. → backlog issue |
| CP-10 | status (diagnostic status line) | `status` `cli.rs:225` | none as a *capability* (GUI status chips exist piecemeal: files-sync chip, tracking chip) | CLI-only register D-10 ("pure read, entity-less") | YES (weakly) — a GUI "sync/service status" surface or deliberate GUI-only waiver is needed; → backlog issue p3 |
| CP-11 | config print / config path | `config print` `cli.rs:1544`, `config path` `cli.rs:1548` | GUI `settings.facts_display` (Settings slice) covers the resolved-config substance; the file *path* is GUI-absent | CLI-only register D-10 | Effectively covered — the GUI facts panel is the parity twin; the path line is trivia. **No action** beyond the census note (§6) |
| CP-12 | sync | `sync` `cli.rs:204` | GUI twin exists (the `RequestSync` arm, per the register's own cross-reference) | CLI-only register D-10 (cross-referenced, not duplicated) | Covered — no action |
| CP-13 | daemon stop | `DaemonCmd::Stop` `cli.rs:264` | none | CLI-only register D-10 | **Legitimately out of scope** — process lifecycle: the GUI runs *inside* the process the verb stops; there is no honest GUI front (the ruling's own "special cases" letter covers the inverse posture). See §4 |
| CP-14 | gui (launch verb) | `gui` `cli.rs:244` | is the GUI | CLI-only register D-10 ("launches the UI; no engine verb") | Trivially fine — the verb's payload *is* the GUI. See §4 |
| CP-15 | boards use (dead verb) | `BoardsCmd::Use` `cli.rs:379` | n/a — parses but is unreachable | CLI-only register; backlog **#776** | Not a parity item — stays on #776 |

## 3. Already-fixed cases (the deferral existed, the GUI landed since)

| Former deferral | Authority that recorded it | Retired by |
|---|---|---|
| r3 D-5 free-form hex colour input ("no free-form hex in the GUI; the CLI covers it") | `.artifacts/plans/2026-11-15-ux-r3-task-detail-controls-chunk-plan.md:383` (echoed `…r4-board-entry-points-chunk-plan.md:425`) | Round-8 C-3 → tag-color UX plan US-1: the free-form six-hex field minted via the domain `Color::parse` landed 2026-11-18 (#839 closed); census Task-detail `label.create/edit` cells carry the supersession. **No action.** |
| 2026-10-15 board plan D13: "Due date: … date parsing/validation UX is its own surface; CLI covers it in v0.1.0" and "Stacks: create/rename/delete are CLI-only in v0.1.0" | `.artifacts/plans/2026-10-15-ui-views-board-plan.md:531-537` | GUI date editors (due/scheduled, the N-4 stamping law, 2026-11-17) and GUI stack verbs (Board slice `stack.create/rename/delete` cells E) landed long since. **No action.** |
| Habit log/reorder/repair/convert GUI absences | habit-family plan era | remediated 2026-11-15 (T-7/#790 etc.); only edit/remove (CP-1/CP-2) remained. |
| SC-1 (CLI `config set` absent for files-sync + knobs) | `.artifacts/plans/2026-11-14-settings-connections-ux-plan.md` | T-18 remediation #801, 2026-11-15 (`config set`/`unset`). Reverse direction anyway. |

## 4. Legitimately out of scope under the ruling (recorded, with reasons)

| Item | Reason it survives the ruling |
|---|---|
| `task.select` CLI absence (Board-slice R-2 on the row) | GUI-interaction-only state with no persisted effect — nothing dispatches until a verb fires; explicitly named by the sweep brief as staying ruled |
| `app.menu`, `app.focus_toggle`, `capture.dismiss`, `capture.hint_display`, `palette.jump`, `palette.results_display`, `nav.item_walk` CLI cells | Pure GUI-interaction surface grammar; a CLI front would be onerous-to-meaningless — the ruling's own "GUI-only in special cases" letter |
| CP-13 `daemon stop` | Process lifecycle outside any GUI process's honest reach |
| CP-14 `gui` verb | Its effect is launching the GUI itself |
| Reverse-direction `U` cells: task.open/TF-2, task.due_today/T-24 + #807, task.back, review.* / plan.* (RW-2/RW-4/RW-5), alarm.dismiss/snooze, notification.dismiss/snooze/open, SC-2 family (app.settings, rosters, facts, env-override), HB-4 habit.convert CLI, RC-3 recur-convert CLI, BF-8 CLI cells (board.default_preset, stack.resize, stack.count_display CLI) | GUI exists, CLI absent — the ruling permits GUI-only; the read-only CLI twins already live on #807. Not CLI-only features. |

## 5. Grouping: round-9 joins vs. backlog issues

Round 9 already carries 11 chunks (W1/W2, H1–H4, K1–K3, M1, D2; the
round plan §4 ledger). Only small/organic fits join; the rest become
`backlog:` issues (opened 2026-11-18 per AGENTS §7 standing
authorization — see §7 for numbers).

**Join round 9 (already homed or organic):**
- CP-1 habit.edit — chunk H4 (supersession already written in the
  habit-parity UX plan).
- CP-3 habit.description — chunks H1 + H4.
- CP-4 habit optional day — chunk H1 (O-1) + H4.
- CP-5 habit optional time — chunk H4.
- CP-6 board.archive GUI verb — *candidate* organic join for M1 (the
  context-menu chunk): one menu row + armed strip routing the existing
  `SetBoardArchived`. If M1's investigation budget is full, the backlog
  issue is the home; the round plan decides at dispatch time.

**Backlog issues (new, opened by this sweep):**
- CP-2 GUI habit remove (p2, ui) — with a comment on #808.
- CP-6 GUI board archive verb (p2, ui) — the fallback home if not M1.
- CP-7 GUI board reset (p2, ui).
- CP-8 GUI board reconcile (p2, ui).
- CP-9 GUI export — pdf/csv/json/fold with the filter set (p2, ui).
- CP-10 GUI status surface (p3, ui).

## 6. Proposed census supersession note text

For `docs/ux-surface-census.org` (header posture note + the affected
rows), to be landed by the round-9 plan's census pass:

> *Parity ruling (operator, 2026-11-18)*: "No feature is allowed to be
> CLI-only, none whatsoever; every existing 'the CLI covers it'
> deferral is invalid." Every GUI absence whose recorded reason is CLI
> coverage is hereby an unruled absence again — the authority that
> recorded it is superseded, not silently dropped. Affected: Board
> slice R-10 (board.reset) and R-11 (board.reconcile) revert to
> scheduled GUI work; the `board.archive` BF-2 archive-direction cells
> revert to open; Today slice R-2 is already superseded by the
> habit-parity plan (2026-11-18) and R-3 (habit.remove) reopens; the
> CLI-only register's `export` and `status` rows become open GUI work
> (config print/path and sync are covered by their GUI twins; `gui`/
> `daemon stop` stay process-lifecycle exceptions). GUI-only absences
> remain permissible where a CLI front is onerous or the capability is
> pure interaction state (task.select et al.). Authority:
> `.artifacts/reports/2026-11-18-round9-cli-parity-sweep-report.md`.

## 7. Backlog issues opened / commented by this sweep

(kept in sync with the GitHub state at filing time)

- **#944** `backlog: GUI habit removal (habit.remove)` — p2, ui — CP-2; plus a comment on closed #808 recording that its "record, do not re-litigate" posture is voided by the 2026-11-18 operator ruling.
- **#945** `backlog: GUI board archive verb (BF-2 archive direction)` — p2, ui — CP-6.
- **#946** `backlog: GUI board reset verb (R-10 superseded)` — p2, ui — CP-7.
- **#947** `backlog: GUI board reconcile verb with informed-consent letter (R-11 superseded)` — p2, ui — CP-8.
- **#948** `backlog: GUI export (pdf/csv/json tracking export)` — p2, ui — CP-9.
- **#949** `backlog: GUI status surface` — p3, ui — CP-10.

## 8. Sources swept

`docs/ux-surface-census.org` (all slices + unruled registers + the
CLI-only register + appendix), `grep -rn -i "CLI covers"` over
`.artifacts/plans` + `.artifacts/reports` + `docs` (9 hits, all
dispositioned above), `.artifacts/reports/2026-11-18-round9-habit-parity-research.md`
(§2 gap list, §5 open questions), `crates/taskboard-app/src/cli.rs`
(`Command` + every sub-enum: Boards/Stacks/Tasks/Labels/Timers/Breaks/
Track/Habits/journal/Export/Config/Daemon), the round-9 round plan
chunk ledger, `gh issue list` duplicate checks (#807/#808/#934/#935
/#776/#839 inspected).

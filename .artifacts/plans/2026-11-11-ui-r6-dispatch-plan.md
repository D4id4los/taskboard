# [Plan] GUI Review Round 6 Implementation — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-11
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 6 implementation chunks of the round-6
  GUI review remediation (reminder interactions, settings save/listing,
  files-sync anchor + backend labels, board publish, files-boards UI
  surface, empty-repeater audit). No code in this commit.
- **Origin**: `.artifacts/plans/2026-11-11-gui-review-round6-plan.md`
  (the accepted round-6 plan; §2 findings table, §3 solutions D-6.1 …
  D-6.9, §4 non-goals), plus the user's 2026-11-11 acceptance ("I accept
  your proposal for how to implement a board over files gui" — D-6.8
  phase 2's visibility + Move-to-Files shape) and the dispatch request
  ("produce a dispatch plan that covers all the touched upon topics").
  Issue origins: #721 (chunk 2), #699 (chunk 4), #722 (chunk 5), #725
  (chunk 6).
- **Status**: READY — all six chunks are dispatchable once the
  groundwork PR (`fix/ui-r6-card-title-width`: the D-6.4 fix + probe +
  this plan + the origin plan) lands; chunk prompts cite origin sections
  by number and resolve against the landed tree. Deliberately **not**
  dispatched here: multi-lane files sync (#723), keyboard activation
  parity on notification/alarm rows (#724), DR-9 engine work (landed).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | D-6.1 + D-6.2 | `fix/ui-r6-reminder-interactions` | Double-click opens the task view on banner + notification rows; banner rows reflect muted/snoozed state. `status-bar.slint`, `notification-surface.slint`, `adapter.rs` seams, `presentation.rs` banner model. No keymap changes, no alarm-window changes, no banner-visibility semantics change | S–M |
| 2 | D-6.5 (#721) + D-6.7 | `fix/app-settings-save-and-listing` | Settings save into a sync-less config (writer creates the `[sync]` subtree; `ensure_migrated` distinguishes "nothing to migrate" from the identity-husk corner); settings mount auto-lists connectors; bindings render without a listing. `ui_requests.rs`, `connections.rs`, `migration.rs`, `adapter.rs`, `settings-view.slint`. No config-format changes beyond creating the documented subtree | M |
| 3 | D-6.6 + D-6.8 phase 1 | `feat/ui-files-sync-anchor` | The files-sync anchor account becomes a choice; every Nextcloud account card gets the Files row; `ConnectorInfo` gains the backend; connector rows label backend-aware. `config.rs`, `bootstrap.rs`, `settings_overlay.rs`, `requests.rs`, `ui.rs`, `adapter.rs`, `settings-view.slint`, `settings_status.rs`. No multi-lane sync (#723 stays), no restart-semantics change | M |
| 4 | D-6.9 (#699) | `feat/ui-r6-publish-board` | "Publish a local board…" on each Deck connection card: `UiRequest::PublishBoard`, app arm reusing the `boards publish` ceremony, board-picker reuse. `requests.rs`, `ui_requests.rs`, `commands/boards.rs`, `adapter.rs`, `settings-view.slint`. No new Deck API calls (create_board exists), no files-target publish (that's chunk 5's move flow) | S–M |
| 5 | D-6.8 phase 2 (#722) | `feat/ui-files-boards-surface` | Files-boards visibility (bindings-derived) + "Move to Files…" UI flow wiring the landed move ceremony. **Gated on chunk 3's merge** (`ConnectorInfo.backend`, labels). `adapter.rs`, `settings-view.slint`, `requests.rs`, `ui_requests.rs`, `commands/boards.rs`. No engine work (landed), no connect picker (adoption is automatic — the accepted design) | M |
| 6 | #725 | `fix/ui-slint-empty-repeater-audit` | Verdicts + fixes for the ~14 empty-repeater-in-layout sites listed in #725 (the TaskCard site is already fixed by the groundwork PR). Audit report + targeted fixes. No speculative refactors of non-broken sites | S–M |

**Dependency graph**: chunks 1, 2, 4, 6 are order-independent (all
branch off `main`, disjoint files) — serial or parallel as separate
sessions. Chunk 3 is independent too; chunk 5 is **gated on chunk 3's
merge** (`ConnectorInfo.backend` + the backend-word label seam land
there). Default dispatch order: 2 → 1 → 3 → 4 → 5 → 6. Never two agents
in one worktree; chunk 5 starts only after 3's PR is merged (its stage-1
prompt verifies).

**Dispatch mechanics**: one agent session per prompt, in order (N.1 →
N.2 → N.3 within a chunk); independent chunks may run concurrently as
separate sessions; never two agents share a worktree. Every chunk works
in `../taskboard-WT-<branch>` next to the project root (AGENTS §8),
branched off the then-current `main`, no PR ever stacked. **Branch names
are never reused after their PR lands** (AGENTS §8, issue #415): a
resume/second round of a chunk works on `<branch>-v2` (or re-branches
off the fetched `origin/main`), never on the landed branch. Create
worktrees with `git worktree add -b <branch> ../taskboard-WT-<branch>`
(never `git worktree add <path> main`). Disk hygiene (AGENTS §8
worktree lifecycle): any agent creating or recreating a worktree runs
`scripts/worktree-doctor.sh` from the main checkout first — nonzero
exit (free disk below the floor) stops the chunk; stage 3 removes the
worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current `main`,
   writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.

2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create `<branch>-v2` instead per
   AGENTS §8; never `git worktree add <path> main`), implements per the
   approved plan — TDD, every commit standalone (rebase-merge hygiene),
   full AGENTS §10 gate suite green. Does **not** push or open a PR.
   Reports deviations.

3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*: **begins
   by rebasing onto `main`** — `git fetch origin --prune`, rebase the
   branch onto `origin/main` (AGENTS §8's rebase dance: drop
   already-merged duplicate commits; resolve only real conflicts), push
   `--force-with-lease` if the rebase rewrote history — then reviews
   the entire branch diff versus the **post-rebase** base against the
   chunk's acceptance criteria + AGENTS rules **plus the code-quality
   mandate below**, re-runs whatever gates apply, implements fixes for
   every finding on the branch (amend/reword, never duplicate-subject
   fixups). Its last tasks are shipping: push the branch, open the PR
   to `main` with `gh`, watch `gh pr checks --watch` to completion —
   any failing check is a finding; diagnose, fix, push
   `--force-with-lease`, repeat until green — **then verify the PR's
   merge state**: `gh pr view --json mergeStateStatus` must report
   `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, let the checks re-run, repeat; any other state
   ⇒ diagnose before handing off. **Then teardown**: remove the
   now-disposable worktree — from the project root,
   `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8 — never a
   bare relative path; everything precious is pushed). Confirm via
   `git worktree list` that the removed path was the intended one. The
   handoff contract is "PR open + checks green + merge state `CLEAN` +
   worktree removed". Reports a verdict. Never merges; never touches
   `main`.

**Code-quality mandate** (stage 3, every chunk): beyond functional
review, evaluate (a) code quality: naming, comment discipline (comments
state constraints, not narration), idiomatic use of the approved stack,
no dead code or speculative generality, test quality per AGENTS §5;
(b) the Single Responsibility Principle: the crate-role boundaries of
AGENTS §2 are the SRP line — a boundary violation is an SRP violation;
(c) a single level of abstraction per function body: a function either
orchestrates named operations or works in primitive data, never mixes.
Findings are fixed on the branch like any other finding.

## 2. Chunk 1 — Reminder interactions (`fix/ui-r6-reminder-interactions`)

- **Covers** (normative: origin plan §3 D-6.1 + D-6.2): the overdue
  banner rows (`ui/compositions/status-bar.slint` — `row-activated` stays
  the focus+arm seam; a new `row-open-requested(int)` fired from
  `TouchArea.double-clicked`) and the notification-surface rows
  (`ui/compositions/notification-surface.slint` — the row becomes
  interactive via a TouchArea; `NotificationRowModel` gains `can-open`,
  true iff task-anchored per `notification_target`,
  `adapter.rs:9173-9190`); adapter seams mirror
  `on_today_row_activated` (`adapter.rs:7088-7093`): resolve the row,
  `view_state.set_focused`, arm `reminder_target`,
  `route(ActionId::TaskEdit)`. D-6.2: `BannerRowModel` gains `muted`
  (task in `state.reminders.muted`) and a snoozed phrase ("snoozed until
  HH:MM" — derive from the journaled snooze state; the persistence apply
  for `RecordReminderSnooze`, `domain/src/persistence.rs:264, 499`, is
  the seam — verify where snoozes surface in `AppState` first and record
  the finding if they don't); `status-bar.slint` renders muted/snoozed
  rows (dimmed tone / mute glyph / snoozed meta). **Non-goals**: no
  keyboard-activation work (#724), no alarm-window changes, no change to
  banner visibility semantics (board truth — `presentation.rs:649-651`),
  no reply channels on the commands.
- **Acceptance**: double-click on a banner row and on a task-anchored
  notification row opens `TaskView` (seam tests asserting
  `view_state.focused_task` + the task-detail view, mirroring
  `banner_actions_send_exact_commands`' harness, `src/lib.rs:2792-2843`);
  non-task notification rows don't advertise opening; the banner row
  model reflects muted/snoozed state from a mutated `AppState`
  (presentation-level test); existing seam tests stay green; full gate
  suite green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the GUI review round 6
implementation ("reminder interactions", branch fix/ui-r6-reminder-interactions)
in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-11-11-gui-review-round6-plan.md
   §3 D-6.1 and D-6.2 (normative) and §2 rows R6-1/R6-2, and the dispatch
   plan .artifacts/plans/2026-11-11-ui-r6-dispatch-plan.md §1/§2.
2. Read the seams you will extend: ui/compositions/status-bar.slint (the
   row-activated seam, TouchArea at ~164-169), ui/compositions/
   notification-surface.slint (the non-interactive ListRow, ~55-60, and
   NotificationRowModel ~16-29), src/adapter.rs (banner seams ~8269-8307,
   notification seams ~9119-9189, the today double-click precedent
   ~7088-7093), src/presentation.rs (BannerRow ~652-697), and the tests
   you must not break: src/lib.rs banner/notification seam tests
   (~2792-2843). Also verify against Slint 1.18.1 that TouchArea's
   double-clicked callback exists (it does — ui/widgets/task-card.slint:106
   uses it).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first
   (nonzero exit = free disk below the floor: STOP and report), then create
   the worktree: git worktree add -b fix/ui-r6-reminder-interactions
   ../taskboard-WT-fix-ui-r6-reminder-interactions off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-ui-r6-reminder-interactions-plan.md
   settling: the exact seam/callback names and signatures; where the
   snooze state surfaces in AppState (read the RecordReminderSnooze apply
   path in taskboard-domain/src/persistence.rs and the reminders state
   tree — if journaled snoozes are not retained in AppState, record the
   decision to retain them there); the muted/snoozed row rendering
   (tokens, copy); the test catalogue (seam tests + presentation tests).
   Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): the ui-r6 reminder interactions chunk. Do not
   implement; do not push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously
   for any out-of-scope finding (AGENTS §7) — list the numbers in your
   report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the GUI review
round 6 implementation ("reminder interactions").

1. Read AGENTS.md, .artifacts/plans/2026-11-11-gui-review-round6-plan.md
   §3 D-6.1/D-6.2, the dispatch plan §1/§2, and the stage-1 plan in
   ../taskboard-WT-fix-ui-r6-reminder-interactions (recreate the worktree
   on the branch if missing).
2. Implement exactly per the approved plan, TDD, every commit standalone.
   Hard rules: SPDX AGPL-3.0-only headers on new files; no tokio sleeps in
   tests; no text assertions (typed outcomes/structural facts only); the
   one-shot reminder_target semantics stay (each button seam arms it
   itself); double-click fires clicked first (the today-view precedent) —
   the clicked handler's focus+arm must stay idempotent.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: none this chunk (the origin plan owns it).
5. No push, no PR — shipping is stage 3's last task. Never push to or
   merge main.
6. Backlog issues autonomously for out-of-scope findings (AGENTS §7);
   list the numbers in your report.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the GUI review round 6
implementation (branch fix/ui-r6-reminder-interactions).

1. Read AGENTS.md, the origin plan §3 D-6.1/D-6.2, the dispatch plan
   §1/§2 (the code-quality mandate applies to you), and the stage-1 plan;
   then review the full branch diff (git diff main...HEAD) in
   ../taskboard-WT-fix-ui-r6-reminder-interactions.
2. Functional focus: the reminder_target one-shot arm (each seam arms it
   itself before routing — a second snooze without re-arming must
   NoTarget, not misfire); non-task anchors (alarm/break/tracker) must not
   open a task view nor advertise can-open; the banner row's muted/
   snoozed reflection must derive from state truth, never UI-local flags;
   the Enter path (focus → TaskEdit) must behave exactly as before; the
   existing banner/notification seam tests must be un-weakened.
3. Code-quality mandate: (copy verbatim from the dispatch plan §1.)
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then
   gh pr checks --watch; any failing check is a finding — diagnose, fix,
   push --force-with-lease, repeat until green; verify mergeStateStatus
   CLEAN per the dispatch plan §1.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-fix-ui-r6-reminder-interactions; confirm with
   git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
Never merge; never touch main.
```

## 3. Chunk 2 — Settings save + listing (`fix/app-settings-save-and-listing`)

- **Covers** (normative: origin plan §3 D-6.5 + D-6.7; issue #721):
  (a) `ensure_migrated` (`crates/taskboard-app/src/ui_requests.rs:1236-1259`)
  treats `migrate_config_file → Ok(false)` as "nothing legacy to migrate"
  and proceeds, EXCEPT when the raw config carries a `[nextcloud]`
  identity husk (either `server_url` or `username` set — the same
  predicate `connectors_item` keys on, `connections.rs:109-112`) ⇒ typed
  `PersistRejected` stands; (b) `overlay_upsert_connector`
  (`connections.rs:192-243`) **creates** the `[sync]`/`[[sync.connectors]]`
  subtree when absent (mirror `migrate_document_inner`'s insertion,
  `migration.rs:121-128`); the identity-husk and present-but-scalar
  refusals stay; (c) `settings_view_mounted` (`adapter.rs:5890-5906`)
  fires `UiRequest::ListRemoteBoards` for every connector lacking a
  `remotes` entry (guard: not already pending for that connector —
  rapid remounts must not double-fire); (d) `connection_items_model`
  (`adapter.rs:4655-4683`) emits a `Remote` item for live bindings absent
  from the listing (`bound: true`, titled by the bound local board's
  title). **Non-goals**: no config-format migration beyond the created
  subtree; no persistent listing cache; no changes to the ceremony order
  in `save_connection`.
- **Acceptance**: the #721 repro (config with no `[sync]`/`[nextcloud]`;
  and the no-config-file case) saves successfully — handler-level test
  plus `ensure_migrated` table (no file; sync-less file; env-injected
  identity ⇒ `PersistRejected`; legacy full identity ⇒ migrates then
  proceeds) plus `overlay_upsert_connector` from absent/sync-less files
  (subtree created, comments preserved); settings mount auto-lists each
  unlisted connector exactly once (seam/request test; pending guard
  test); bindings render without a listing (adapter-model test); full
  gate suite green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the GUI review round 6
implementation ("settings save + listing", branch
fix/app-settings-save-and-listing) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md, .artifacts/plans/2026-11-11-gui-review-round6-plan.md
   §3 D-6.5 and D-6.7 (normative) and §2 rows R6-5/R6-7, GitHub issue
   #721 (its root-cause section is verified), and the dispatch plan
   §1/§3.
2. Read the seams: crates/taskboard-app/src/ui_requests.rs
   (ensure_migrated ~1236-1259, save_connection ~727-780,
   list_remote_boards ~835-885, the settings dispatch arm ~352-355),
   crates/taskboard-app/src/connections.rs (connectors_item ~99-127,
   overlay_upsert_connector ~192-243), crates/taskboard-app/src/
   migration.rs (migrate_document_inner ~109-141, migrate_config_file
   ~193-214), crates/taskboard-ui-slint/src/adapter.rs
   (settings_view_mounted ~5890-5906, connection_items_model ~4655-4683,
   PendingReply::Remotes error path ~4265-4280, PendingKind), and the
   settings-request tests in taskboard-app (search tests/ for
   save_connection/settings) you must not break.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard, then
   git worktree add -b fix/app-settings-save-and-listing
   ../taskboard-WT-fix-app-settings-save-and-listing off main.
4. Write the TDD-first plan at
   .artifacts/plans/<today>-ui-r6-settings-save-and-listing-plan.md
   settling: the exact identity-husk predicate ensure_migrated uses (name
   the shared helper vs the duplicated predicate — record the decision);
   the subtree-creation code path in overlay_upsert_connector (mirror
   migrate_document_inner; preserve decor); the mount auto-list trigger
   and its pending guard (which state marks a connector's listing
   in-flight — reuse PendingKind::ListRemote + vm.busy_target or add
   per-connector pending — record the decision); the bindings-without-
   listing Remote item shape (title source, ItemAddr::Remote address);
   the test catalogue. Resolve every open question with a recorded
   decision.
5. Commit as: docs(plan): the ui-r6 settings save and listing chunk. No
   implementation; no push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 ("settings save +
listing").

1. Read AGENTS.md, the origin plan §3 D-6.5/D-6.7, issue #721, the
   dispatch plan §1/§3, and the stage-1 plan in
   ../taskboard-WT-fix-app-settings-save-and-listing.
2. Implement per the approved plan, TDD, standalone commits. Hard rules:
   toml_edit read-modify-write discipline (comments/formatting/mode
   preserved; atomic temp+rename); the writer refusals
   (identity husk, present-but-scalar connectors) stay; typed errors only
   (SettingsError variants, no string errors); the auto-list must not
   fire at boot — only on settings-mount; one listing request per
   connector until its reply lands.
3. Run the full gate suite (AGENTS §10); make it green.
4. Bookkeeping: comment on #721 with the branch/PR once stage 3 ships
   (stage 3 owns the comment if you prefer — coordinate in your report).
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 (branch
fix/app-settings-save-and-listing).

1. Read AGENTS.md, origin plan §3 D-6.5/D-6.7, #721, dispatch plan
   §1/§3 (code-quality mandate applies), the stage-1 plan; review the
   full branch diff in ../taskboard-WT-fix-app-settings-save-and-listing.
2. Functional focus: the PersistRejected corner must SURVIVE for
   env-injected identity and partial legacy keys (a test weakening here
   is a finding); the created subtree must byte-preserve every other
   table and comment (a round-trip test must exist); the auto-list's
   pending guard (double-mount must not double-fire — the flake class
   the guard exists for); a failed listing keeps the refresh hint and
   clears the pending flag (the existing error outcome path
   adapter.rs:4271-4279 must be exercised); bound boards render without
   a listing and do not duplicate once the listing lands.
3. Code-quality mandate: verbatim from dispatch plan §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix on the branch (amend/reword).
6. Ship per dispatch plan §1 stage 3 (push, PR, checks watch, merge
   state CLEAN); comment the shipped PR URL on #721.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-fix-app-settings-save-and-listing; confirm via
   git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree
removed, #721 commented. Never merge; never touch main.
```

## 4. Chunk 3 — Files anchor + backend labels (`feat/ui-files-sync-anchor`)

- **Covers** (normative: origin plan §3 D-6.6 + D-6.8 phase 1):
  `FilesSyncSection` gains `anchor: Option<String>` (absent ⇒ first
  Nextcloud connector = today's behaviour); validation (a set anchor
  must name an existing Nextcloud connector; `enabled` requires a
  resolvable anchor — replaces the default-backend check at
  `config.rs:1556-1558`); bootstrap resolves the anchor by name
  (`bootstrap.rs:118-136`); `settings_overlay::set_files_sync_anchor`
  beside `set_files_sync_enabled`; `UiRequest::SetFilesSyncConnector`;
  `SettingsSnapshot` gains `files_sync_connector` + applies/env metadata
  for the new key; `ConnectorInfo` gains the backend
  (`requests.rs:37-41`, fill at `ui.rs:612-623`); the Files item is
  emitted for EVERY Nextcloud account card with per-card semantics
  `files_on = enabled && card is the anchor` (toggling On on a non-anchor
  card re-anchors + enables; Off on the anchor disables);
  connector-row labels backend-aware ("Deck · name" vs bare name —
  `settings-view.slint:917`, backend word on `ConnectionItem`); the
  Files caption copy updated (`settings_status.rs:261` replaced).
  **Non-goals**: no N-lane sync (#723); no restart-semantics change
  (actor still spawns at boot); no files-boards surface (chunk 5).
- **Acceptance**: config round-trip tests for the new key (absent = old
  behaviour); validation refusals typed (unknown anchor, non-Nextcloud
  anchor, enabled without resolvable anchor); bootstrap spawn decision
  test (anchor resolution + fallback); settings model test (Files row on
  every Nextcloud card, per-card files_on semantics); label test
  (backend word on ConnectionItem; the UI reads it); env_overridden/
  applies metadata wired for the new key; full gate suite green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 ("files anchor + backend
labels", branch feat/ui-files-sync-anchor) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, .artifacts/plans/2026-11-11-gui-review-round6-plan.md
   §3 D-6.6 and D-6.8 phase 1 (normative) and §2 rows R6-6/R6-8, issue
   #723 (the non-goal boundary), and the dispatch plan §1/§4.
2. Read the seams: crates/taskboard-app/src/config.rs (FilesSyncSection
   ~118-119, 274-283, validation ~1556-1558, resolved_default_backend
   ~1365-1372), crates/taskboard-app/src/bootstrap.rs
   (files_sync_spawn_decision ~118-136, files_sync_spawn ~193-243),
   crates/taskboard-app/src/settings_overlay.rs (~309-316),
   crates/taskboard-ui-slint/src/requests.rs (ConnectorInfo ~37-41,
   FilesSyncEnabled ~154-155, snapshot ~204-206, SetFilesSyncEnabled
   ~471-477), crates/taskboard-app/src/ui.rs (resolve_connector_infos
   ~612-623), crates/taskboard-ui-slint/src/adapter.rs
   (connection_items_model ~4600-4632, settings_files_picked
   ~5341-5376), ui/views/settings-view.slint (Files card ~878-910,
   connector label ~917), src/settings_status.rs (~261). Tests you must
   not break: the settings/config test files around these seams.
3. Run scripts/worktree-doctor.sh, then git worktree add -b
   feat/ui-files-sync-anchor ../taskboard-WT-feat-ui-files-sync-anchor
   off main.
4. Write the TDD-first plan at
   .artifacts/plans/<today>-ui-r6-files-anchor-plan.md settling: the
   config key name and serde shape; the anchor-resolution rule and its
   validation refusals (typed variants); the snapshot fields and the
   applies/env metadata wiring; the per-card Files row semantics
   (files_on = enabled && anchor; the toggle's request payload carries
   the intended anchor) and its caption copy; ConnectorInfo.backend
   (bool vs enum — record the decision; the UI gets a backend word
   string on ConnectionItem); the label rendering; the test catalogue.
   Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ui-r6 files anchor chunk. No
   implementation; no push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 ("files anchor +
backend labels").

1. Read AGENTS.md, origin plan §3 D-6.6/D-6.8 phase 1, the dispatch plan
   §1/§4, and the stage-1 plan in
   ../taskboard-WT-feat-ui-files-sync-anchor.
2. Implement per the approved plan, TDD, standalone commits. Hard rules:
   the files-sync actor stays restart-scoped and single-lane; the absent
   anchor key must reproduce today's behaviour bit-for-bit (existing
   configs churn nothing); overlay writes preserve formatting; typed
   validation refusals only; the UI stays a dumb presentation client
   (the anchor decision rides requests, not UI state).
3. Run the full gate suite (AGENTS §10); green.
4. Bookkeeping: none this chunk.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 (branch
feat/ui-files-sync-anchor).

1. Read AGENTS.md, origin plan §3 D-6.6/D-6.8 phase 1, dispatch plan
   §1/§4 (code-quality mandate applies), the stage-1 plan; review the
   full branch diff in ../taskboard-WT-feat-ui-files-sync-anchor.
2. Functional focus: absent-anchor configs must behave bit-for-bit as
   before (a regression here breaks every existing files-sync user);
   the anchor validation must refuse unknown AND non-Nextcloud anchors
   typed; the per-card toggle semantics (toggling On on card B
   re-anchors from A atomically — one request, one overlay write); the
   env_overridden lock and the applies/on-restart metadata for the new
   key must match the existing key's wiring; standalone connectors must
   not render a Files row; the label change must not alter
   non-connector surfaces.
3. Code-quality mandate: verbatim from dispatch plan §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix on the branch (amend/reword).
6. Ship per dispatch plan §1 stage 3.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-feat-ui-files-sync-anchor; confirm via
   git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree
removed. Never merge; never touch main.
```

## 5. Chunk 4 — Publish board (`feat/ui-r6-publish-board`)

- **Covers** (normative: origin plan §3 D-6.9; issue #699):
  `UiRequest::PublishBoard { board, connector, reply }` (`requests.rs`,
  the enum at `:300` region); the app arm (`ui_requests.rs`) builds the
  connector's `DeckClient` (the `live_remote_boards` pattern,
  `:868-885`) and reuses the publish ceremony — extract the shared
  helper from `publish_on` (`commands/boards.rs:667`, the session-core
  variant the REPL uses, `repl.rs:499`) so CLI and UI share it; guards
  (unbound-only) come from the ceremony; the settings Deck connection
  card gains a "Publish a local board…" row opening the existing
  unbound-boards picker (`board-options`, re-derived every push pass,
  `adapter.rs:4699-4704`); new `PendingKind::Publish` + busy flag; the
  reply refreshes via the standard `ConnectionsSnapshot` path.
  **Non-goals**: no new Deck API surface (create_board exists,
  `client.rs:525`); no files-target publish (chunk 5's move flow); no
  changes to the CLI verb's behaviour.
- **Acceptance**: the extracted helper keeps the CLI `boards publish`
  tests green unchanged; the app-arm test (mock/wiremock level: create
  board on the server + `BindBoard` lands + reconcile wait); a
  bound-board refusal surfaces typed through the settings outcome path;
  the picker lists only unbound boards; busy/pending states render; full
  gate suite green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 ("publish board", branch
feat/ui-r6-publish-board) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, .artifacts/plans/2026-11-11-gui-review-round6-plan.md
   §3 D-6.9 (normative) and §2 row R6-9, issue #699, and the dispatch
   plan §1/§5.
2. Read the seams: crates/taskboard-app/src/commands/boards.rs
   (publish ~628, publish_prelude ~652, publish_on ~667,
   sweep_publish_orphans, bind_nudge_wait_reconcile ~874),
   crates/taskboard-app/src/repl.rs (~499), crates/taskboard-app/src/
   ui_requests.rs (live_remote_boards ~868-885, create_local_board
   ~1074, attach_connection ~983, bind_through ~1028),
   crates/taskboard-ui-slint/src/requests.rs (the UiRequest enum ~300),
   crates/taskboard-ui-slint/src/adapter.rs (settings_create_item
   ~5278, settings_remote_bind_picked ~5238, push_settings board-options
   ~4699-4704, PendingKind), ui/views/settings-view.slint (the Deck
   connection card's rows). Tests you must not break: the boards publish
   CLI/REPL tests and the settings seam tests.
3. Run scripts/worktree-doctor.sh, then git worktree add -b
   feat/ui-r6-publish-board ../taskboard-WT-feat-ui-r6-publish-board
   off main.
4. Write the TDD-first plan at
   .artifacts/plans/<today>-ui-r6-publish-board-plan.md settling: the
   extracted helper's signature and call sites (CLI one-shot, REPL
   session, the new UI arm — one shared core, record the seam); the
   request payload and reply shape; the UI row's placement and the
   picker wiring (which existing picker component, its options source);
   pending/busy states; the reconcile wait budget; the test catalogue.
   Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ui-r6 publish board chunk. No
   implementation; no push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 ("publish board").

1. Read AGENTS.md, origin plan §3 D-6.9, #699, dispatch plan §1/§5, the
   stage-1 plan in ../taskboard-WT-feat-ui-r6-publish-board.
2. Implement per the approved plan, TDD, standalone commits. Hard rules:
   the CLI/REPL behaviour must not drift (the extraction is a refactor
   with the existing tests as the pin); the orphan sweep and the
   reconcile wait ride along (no new crash windows — the W2/#150
   sweep discipline); typed refusals through the settings outcome path;
   the UI stays a dumb client.
3. Run the full gate suite (AGENTS §10); green.
4. Bookkeeping: none this chunk (#699's close happens at the user's
   merge).
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 (branch
feat/ui-r6-publish-board).

1. Read AGENTS.md, origin plan §3 D-6.9, #699, dispatch plan §1/§5
   (code-quality mandate applies), the stage-1 plan; review the full
   branch diff in ../taskboard-WT-feat-ui-r6-publish-board.
2. Functional focus: the extraction must be behaviour-preserving for the
   CLI (diff the ceremony order: create → bind → sweep → nudge/wait);
   a mid-publish failure must leave the resumable state the CLI
   documents (no new half-published husks); the UI picker must list
   unbound live boards only and re-derive from engine state; the busy
   flag must clear on both success and typed refusal; double-submit
   (click twice during publish) must not double-create on the server.
3. Code-quality mandate: verbatim from dispatch plan §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix on the branch (amend/reword).
6. Ship per dispatch plan §1 stage 3.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-feat-ui-r6-publish-board; confirm via
   git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree
removed. Never merge; never touch main.
```

## 6. Chunk 5 — Files-boards UI surface (`feat/ui-files-boards-surface`)

- **Covers** (normative: origin plan §3 D-6.8 phase 2; issue #722;
  **gated on chunk 3's merge**): the Files account card gains a
  boards-over-Files visibility section — derived from the engine's
  bindings (`RemoteBoardId::Files`, `ids.rs:133`; bindings tagged with
  the files connector, `bootstrap.rs` FILES_SYNC_CONNECTOR_NAME) — no
  listing request (adoption is automatic; the accepted design, #722
  comment); a "Move to Files…" flow per eligible local board wiring the
  landed move ceremony (`boards move --to-files`, `cli.rs:269-289`,
  `commands/boards.rs`, `sync-nextcloud/src/move_marker.rs`) through a
  new `UiRequest` + session-core reuse (chunk 4's extraction pattern);
  resumable/undo semantics surfaced honestly (the ceremony is
  interruptible; the UI reflects in-flight state, not a fake
  progress). **Non-goals**: no engine work; no connect/bind picker for
  files boards; no Deck→files bulk operations beyond single-board moves;
  no mixed-fleet exit ceremony UI beyond the move verb.
- **Acceptance**: files-bound boards render on the Files card (adapter
  model test from a mutated AppState with files bindings); the move
  flow's request → ceremony → reply path tested at the app layer; busy/
  pending states; a files-bound board does not offer Deck bind actions;
  full gate suite green.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 ("files-boards UI
surface", branch feat/ui-files-boards-surface) in
/home/viktoria/src/taskboard.

0. GATE: verify chunk 3 (feat/ui-files-sync-anchor) is MERGED — git
   fetch origin --prune; git log origin/main --oneline | grep -i
   files-sync-anchor or check ConnectorInfo carries the backend field on
   origin/main. If not merged, STOP and report.
1. Read AGENTS.md, .artifacts/plans/2026-11-11-gui-review-round6-plan.md
   §3 D-6.8 phase 2 (normative) and §2 row R6-8, issue #722 (the
   reshaped scope comment), and the dispatch plan §1/§6.
2. Read the seams: crates/taskboard-domain/src/ids.rs (RemoteBoardId::Files
   ~133, wire forms ~110-180), crates/taskboard-app/src/bootstrap.rs
   (FILES_SYNC_CONNECTOR_NAME), crates/taskboard-app/src/cli.rs
   (boards move ~269-289), crates/taskboard-app/src/commands/boards.rs
   (the move ceremony), crates/taskboard-sync-nextcloud/src/move_marker.rs,
   crates/taskboard-state/src/actor.rs (adoption ~1008-1026),
   crates/taskboard-ui-slint/src/adapter.rs (connection_items_model,
   push_settings), ui/views/settings-view.slint (the Files card), plus
   docs/boards-move-runbook.org (the ceremony's user contract). Tests
   you must not break: the move ceremony tests and the settings tests.
3. Run scripts/worktree-doctor.sh, then git worktree add -b
   feat/ui-files-boards-surface
   ../taskboard-WT-feat-ui-files-boards-surface off main.
4. Write the TDD-first plan at
   .artifacts/plans/<today>-ui-r6-files-boards-surface-plan.md settling:
   the Files card's section shape (which bindings render, ordering,
   labels); the eligible-board set for Move-to-Files (unbound + Deck-
   bound? record the decision — the ceremony's own guards are
   normative); the request payload, pending/busy states, and how the
   resumable ceremony's interruption surfaces in the UI; the test
   catalogue. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ui-r6 files boards surface chunk. No
   implementation; no push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 ("files-boards UI
surface").

1. Read AGENTS.md, origin plan §3 D-6.8 phase 2, #722, dispatch plan
   §1/§6, the stage-1 plan in
   ../taskboard-WT-feat-ui-files-boards-surface.
2. Implement per the approved plan, TDD, standalone commits. Hard rules:
   the ceremony invocation must reuse the CLI's session-core path (no
   reimplemented move logic in the UI arm); the UI never invents move
   state — in-flight/copy follows the runbook's contract; adoption is
   automatic — no connect/bind UI for files boards; typed errors only.
3. Run the full gate suite (AGENTS §10); green.
4. Bookkeeping: none this chunk (#722's close happens at the user's
   merge).
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 (branch
feat/ui-files-boards-surface).

1. Read AGENTS.md, origin plan §3 D-6.8 phase 2, #722, dispatch plan
   §1/§6 (code-quality mandate applies), the stage-1 plan; review the
   full branch diff in ../taskboard-WT-feat-ui-files-boards-surface.
2. Functional focus: the visibility section must derive from engine
   bindings only (no substrate scanning from the UI); a board bound to
   Deck must not render under Files and vice versa (binding-scoped
   equality, D6-2); the move flow must refuse exactly what the
   ceremony refuses (double-check the guards weren't widened); the UI
   must not promise automatic arrival timing (adoption is eventual —
   copy honest); chunk 3's backend labels must still render for pure
   Deck connectors.
3. Code-quality mandate: verbatim from dispatch plan §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix on the branch (amend/reword).
6. Ship per dispatch plan §1 stage 3.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-feat-ui-files-boards-surface; confirm via
   git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree
removed. Never merge; never touch main.
```

## 7. Chunk 6 — Empty-repeater audit (`fix/ui-slint-empty-repeater-audit`)

- **Covers** (normative: issue #725; origin plan §3 D-6.4's pattern-audit
  bullet): for each of the ~14 sites listed in #725 (status-bar banner,
  notification-surface, habits, timers, track-entries, focus-overlay,
  action-menu, search-palette, stat-bars, dropdown-flyout, date-picker,
  board-bar, board-layout ×2 — the TaskCard site is already fixed): a
  recorded verdict — (a) the parent is conditionally mounted when empty
  (fine), (b) the collapse is harmless (no flexible child depends on the
  ancestor's max), or (c) a live bug — fixed by mounting with content or
  by severing (the TaskCard precedent: plain-container carrier + explicit
  cap; **the single-cell lowering defeats a bare wrapper**). The deliverable
  includes the audit table inside the chunk plan; fixes only for (c).
  **Non-goals**: no speculative refactors of (a)/(b) sites; no new
  abstraction for the pattern.
- **Acceptance**: the chunk plan carries the complete verdict table
  (site → verdict → evidence); every (c) fix has a regression probe in
  the `task_card_title_probe_tests` style (structural width/height
  relations); full gate suite green.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 ("empty-repeater audit",
branch fix/ui-slint-empty-repeater-audit) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #725 (the site list + the mechanism), and
   .artifacts/plans/2026-11-11-gui-review-round6-plan.md §3 D-6.4 (the
   verified mechanism + the TaskCard precedent, normative for fixes).
2. For each #725 site, read the component and its parent chain: ui/
   compositions/status-bar.slint (~130), notification-surface.slint
   (~52), habits.slint (~78), timers.slint (~116), track-entries.slint
   (~293), focus-overlay.slint (~135), action-menu.slint (~145),
   search-palette.slint (~59), widgets/stat-bars.slint (~48),
   widgets/dropdown-flyout.slint (~72), compositions/date-picker.slint
   (~187), board-bar.slint (~121), board-layout.slint (~297, ~750).
   The mechanism: an empty stretch-aligned layout reports max = min = 0
   (i-slint-core box_layout_info); LayoutInfo::merge takes
   max = min(child maxes); watch for the single-cell lowering (a plain
   element wrapping exactly one layout inherits the child's constraints).
3. Run scripts/worktree-doctor.sh, then git worktree add -b
   fix/ui-slint-empty-repeater-audit
   ../taskboard-WT-fix-ui-slint-empty-repeater-audit off main.
4. Write the plan at .artifacts/plans/<today>-ui-slint-empty-repeater-audit-plan.md:
   the complete verdict table (site → verdict → evidence — reason about
   the parent chain concretely; where uncertain, mark "needs probe" and
   include the probe in the test catalogue); the (c) fixes with their
   regression probes; resolve every open question with a recorded
   decision.
5. Commit as: docs(plan): the ui-slint empty repeater audit chunk. No
   implementation; no push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, the verdict table summary, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 ("empty-repeater
audit").

1. Read AGENTS.md, #725, the dispatch plan §1/§7, and the stage-1 plan in
   ../taskboard-WT-fix-ui-slint-empty-repeater-audit.
2. Implement per the approved plan: only (c)-verdict sites get fixes;
   every fix carries its structural regression probe (the
   task_card_title_probe_tests pattern — real component, controlled
   sizes, relation assertions, no font-metric pins). TDD; standalone
   commits. The goldens may shift where a fix changes geometry — the
   golden regeneration flow applies (record any regen in the commit).
3. Run the full gate suite (AGENTS §10); green.
4. Bookkeeping: none this chunk (#725's close happens at the user's
   merge).
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 (branch
fix/ui-slint-empty-repeater-audit).

1. Read AGENTS.md, #725, origin plan §3 D-6.4, dispatch plan §1/§7
   (code-quality mandate applies), the stage-1 plan; review the full
   branch diff in ../taskboard-WT-fix-ui-slint-empty-repeater-audit.
2. Functional focus: every site in #725 must have a verdict in the plan
   (none skipped); (a)/(b) sites must be UNCHANGED in the diff (a fix at
   a harmless site is scope creep — revert it); every (c) fix's probe
   must fail on the pre-fix structure (verify by reasoning through the
   merge chain, or by a recorded stash-run); no new abstractions.
3. Code-quality mandate: verbatim from dispatch plan §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix on the branch (amend/reword).
6. Ship per dispatch plan §1 stage 3.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-fix-ui-slint-empty-repeater-audit; confirm via
   git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree
removed. Never merge; never touch main.
```

## 8. Completion bookkeeping

- **#721** closes with chunk 2's merge (✅ DONE + the PR note); **#699**
  with chunk 4; **#722** with chunk 5; **#725** with chunk 6. The
  groundwork PR references #725 (its TaskCard site is the first fixed
  entry) but does not close it.
- **Deliberately not dispatched**: #723 (multi-lane files sync), #724
  (keyboard activation parity), #673 (files-sync cadence follow-up) —
  they stay in the backlog with their own triggers.
- **What opens next**: once chunks 3 + 5 land, the round-6 review's
  sync-surface observations are fully retired; a follow-up GUI review
  round (round 7) re-verifies the whole surface against the origin
  plan's §2 table.

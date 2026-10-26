# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The workspace carries a single global version (see `docs/architecture.org`);
`scripts/bump_version.sh <major|minor|patch>` cuts release entries.

## [Unreleased]
### Fixed

- `taskboard sync --wait` no longer misreports a completed, successful
  cycle as `TimedOut` after the host clock steps backward (#283
  F11-04): cycle freshness is detected structurally — the sync
  status's generation counter, advanced once per ingested report —
  instead of comparing two wall stamps; `last_success` remains a
  display fact. Reminder offsets are now whole-second by construction
  (the `ReminderOffset` mint rejects sub-second durations, and corrupt
  serialized tuples are rejected, never clamped), closing the
  storage-truncation divergence that could re-fire a delivered
  reminder after a reload (#283 F11-03).

### Added
- Kiosk ambient display rotation (#363, K13): the kiosk appliance's
  window is now a passive ambient display — a display-mode state
  machine over the one pinned board rotates the board's kanban
  reading and the today view's upcoming reading on a UI-side dwell
  timer, with zero input and zero engine commands. Configured under
  `[kiosk.rotation]` (`modes` + `dwell_secs`, floor 10 s, typed
  errors for every invalid value); an absent table keeps every
  existing config byte-identical, and a pin-less multi-board kiosk
  boot now refuses typed (`kiosk-pin-required`) instead of showing
  an arbitrary board.
- Virtualised task lists (#373): every surface that draws N tasks —
  the board columns (regular and compact legs), and the Today and
  stack-detail lists — now renders through an in-house `ListView`
  built on the slint compiler's virtual repeater, so a column with
  10 000 tasks instantiates only a viewport-scale window of cards
  (the eager board at that scale could not render at all — the #462
  i16 ceiling). Keyboard focus moves scroll the focused card fully
  into view; scroll positions and indices are untouched by the
  change (all seams keep global model indices). On the board the
  Add-Task ghost and inline create field sit at the column's bottom
  edge instead of hugging the last card.
- Pointer drag-to-move on the board (#226, #444's D38, round-2
  chunk 8): pressing a task card and moving past an 8-px slop lifts a
  ghost — the card's own snapshot at half opacity, centred under the
  cursor — dims the source card, and highlights the hovered stack
  column. Releasing over a column moves the task to the end of that
  stack by dispatching the exact `MoveTask` command the keyboard move
  flow confirms (click ≡ key, one shared construction site; dropping
  on the source column reorders to its own end, the keyboard ring's
  legal move). Releasing outside any column, Escape (the D16 cancel
  binding), or switching views cancels with nothing dispatched, and a
  release after a cancel never degrades into a click. Below the slop
  the old pointer semantics are untouched: single click selects,
  double click opens, right click opens the context menu. The ghost,
  dim, and highlight are model-push derived, so interleaved refreshes
  cannot disturb an in-flight drag; a refresh that drops the dragged
  task or its target stack cancels cleanly. Built on the Slint 1.18
  native drag surface (the #226 re-check overturned the hand-rolled
  fallback: `DragArea`/`DropArea` ship slop, grab, click-preservation
  and cancel semantics); touch rides the synthesized-mouse path.
  Compact mode has no drag (the keyboard move flow remains its move
  affordance).
- Board-scoped connection management in the settings surface
  (#390, round-2 chunk 6; D32–D34): the Connections section becomes a
  board selector above the connector list above the add form. The
  selector lists every live local board with its binding badge
  (bound-to-connector name or unbound); clicking selects (radio
  semantics), a bound row carries Detach, and each connector row
  gains Bind-to-this-board — attaching the connector if needed,
  adopting the remote board by exact title (lowest id wins on
  duplicates), binding through the engine command path, and settling
  the first pull. Re-issuing a bind on the same connector is
  idempotent; a board bound elsewhere refuses typed (detach first —
  no destructive auto-rebind). Detach unbinds first and stops the
  connector's actor only when no other live board remains bound to
  it. Every state-changing reply carries the refreshed
  boards+connectors pair, so neither surface goes stale (F18). The
  save ceremony becomes visible: every Save click reacts (invalid or
  busy clicks render their typed refusal through the status line
  instead of absorbing silently), in-flight ceremonies mark their row
  with a busy pulse and disable the other settings ops (one op at a
  time), and the app logs ceremony start/success with connector name
  and server host only — never credentials. The add form gains a
  backend picker where Nextcloud is enabled and Standalone renders as
  the disabled "coming soon" segment.


- The today view, global Inbox, quick capture, and the cross-board
  move ring (#346, #347, daily-driver wave): the pure `today_sections`
  classifier over the configured day window (nothing derived is
  stored; every "today" question routes through `day_window` at read
  time), the `Today` view as the desktop default launch view (kiosk
  still launches on the board) with overdue / scheduled / due-today /
  done-today sections across all boards, `task.due-today`
  (confirmation-free re-commit stamping the day window's end), the
  `CaptureTask` command with the app-global Inbox (a real board
  created unbound on first capture — never auto-bound) and the
  board-directed Backlog variant, the `capture` CLI/REPL verb plus
  the capture overlay (Ctrl+K; commits-and-continues into the global
  Inbox), the toolbar inbox chip routing `view.inbox`, and the move
  ring widened to the flat cross-board candidate list with
  `board › stack` hints.
  **Deliberate behavior changes**: ordinary task moves now continue
  across the board edge (the ring is uniform, no Inbox
  special-casing); `tasks add` without `--stack` lands in the single
  live board's Backlog (created on demand) instead of the ambiguous
  all-stacks resolution — zero boards refuse typed as before,
  multiple boards are a typed `AmbiguousBoard`; and `tasks move`
  gains `--board` as a stack-selector pre-filter.
- Fuzzy search (#348, daily-driver wave): the pure domain ranking core
  (`find.rs` — class-ordered subsequence matching over live boards,
  stacks, and tasks, ranked by `(quality, title_len, target)`, no
  numeric score), the top-level `taskboard find` verb + REPL line
  (`--type`, `--limit`), and the GUI jump-to palette
  (Ctrl+F / Ctrl+s / `/`): per-keystroke re-ranking capped at 8 rows
  with an overflow count, jump-to board/stack/task per hit kind, and
  the anchor card hidden from palette rows.
- Desktop reminder toasts via `[reminders] sink = auto|log|desktop`
  (#344, daily-driver wave): `notify-rust` 4.18.2 enters the approved stack
  (D-R10, `z-with-tokio` executor adopted by measurement), and
  `DesktopSink` renders fired reminders as passive toasts behind the
  unmoved `NotificationSink` port (D-R11) — no action buttons, no
  sound, no escalation (D-R12). `auto` (the default) resolves by
  `[app] mode` at boot: desktop gets toasts with zero configuration,
  daemon/kiosk keep the journald `LogSink` path byte-for-byte (D-R13).
  A failed delivery is a logged `NotifyError` — never retried, never
  fallback-sent; the sink choice is boot-time config, not
  runtime-switchable.
- Standalone timers & alarms, `taskboard timers add/list/cancel`
  (#211, daily-driver wave): task-independent one-shot alarms (`--in <dur>` or
  `--at <instant|date|HH:MM[:SS]>` host-local at the CLI edge; a bare
  time already past means tomorrow), resolved by uuid/uuid-prefix/
  label-prefix and cancelled typedly. An alarm fires within one tick
  of its (snooze-chain-moved) instant, notifies with its label, and
  dies by settlement — the post-batch prune removes it with its
  delivery proofs and snooze links. Device-local: never synced, no UI
  surface (the banner and the reminder center are unchanged). The
  one-tick cadence is the firing precision bound, stated honestly.
  Migration 0008 adds the additive `adhoc_alarms` table (0004
  conventions).
- The task-field surfaces + anchor hiding + the sidecar badge (#209,
  daily-driver wave): `tasks edit` grows `--scheduled` (the `--due` vocabulary),
  `--effort` (bare digits = minutes, `s`/`m`/`h` compose), and
  `--importance`/`--urgency` (`very-low`…`very-high`), each clearing
  on `none`, in the CLI and the REPL alike with completion for the
  literals. `tasks list` rows (`--json` and human) carry the new
  fields in their canonical text forms. The UI card renders
  scheduled/effort phrases and importance/urgency chips from the
  presentation layer only (no slint-side semantics). The app-data
  anchor card disappears from list surfaces (CLI rows and counts, UI
  board/stack cards, the overdue banner) while staying resolvable
  behind its typed guard, and the status bar renders the selected
  board's sidecar health as an update-this-app warning when the
  document is unreadable.
- The app-data sidecar fold, flush, and rules section (#207 + #244,
  daily-driver wave): app-field edits and rule mutations now enqueue the
  `UpdateAppData` op (mixed edits enqueue both lanes; `scheduled` is
  flag-routed per the capability seam) and converge through the
  document lane end to end — the engine folds the downloaded document
  into local state behind the self-clobber guard (value **and** clock
  copy; a rule's remote identity is its UUID; absence is never a
  deletion; a too-new/corrupt/foreign document folds nothing louder),
  and the sync actor flushes local truth back as a read-modify-write
  union between each cycle's pull and its report (one PUT per board,
  embedded-clock LWW with verbatim preservation, lane GC while the
  connector owns `startdate`, snapshot-absence GC; rebuild triggers
  re-seed the document from local state; the lazy gate keeps
  native-only boards anchor-free). Fence rows land typed: transport,
  decode, format-too-new, and moved-card evidence never re-encodes
  the document and warns through CLI `status`
  (`sidecar.reason`, additive — no protocol bump); permanent
  bad-requests dead-letter as counted `FailOp`s. The per-board anchor
  card is guarded app-layer against rename/delete
  (`anchor-task-reserved`). Manifest assertion 4 pins every
  `TASK_FIELDS` row's lane through the planner; tier-2 convergence
  fixtures (two engine+actor pairs on one board) cover the field,
  rule, echo-idempotence, and Deck-1.17.5 `scheduled` fallback
  journeys.
- The app-data sidecar transport + the Deck capability seam (#207,
  daily-driver wave): one capabilities probe per sync-actor lifetime
  (`GET ocs/v2.php/cloud/capabilities`) derives a typed
  `DeckFeatures` flag table (the codebase's only version switch —
  unknown versions and probe failures degrade to the conservative
  arm), and `scheduled`'s card-write encoding gates on it: native
  `startdate` on a capable server, an explicit wire-null everywhere
  else. The attachment-create form carries the additive `data` field
  unconditionally (required by Deck 1.17.5, tolerated by 1.19.0) and
  the create/update echo decode folds `extendedData.info.filename`
  into the stored filename, so one wire form serves every measured
  server; tier-2 gate fixtures pin both tiers (Deck 1.19.0 round trip,
  the Deck 1.17.5 measured silent write-drop, and the capabilities
  probe end-to-end). The sidecar half lands the versioned document
  codec (`format` + board envelope over the `tasks`/`reminder-rules`
  registry, unknown content preserved verbatim, `scheduled` as the
  fourth modeled key), the typed `AppDataEvidence` on completed sync
  reports, the `UpdateAppData` outbox op + storage tag (nothing
  enqueues it yet), and the anchor-card ceremony's discovery/pick/
  evidence half with the per-board in-memory document cache — the
  document fold/flush and the scheduled lane rule are the next chunk.
- The day-boundary preference (#345, daily-driver wave): `[preferences] day_start_time`
  (default `"00:00"`, device-local, never synced) sets the
  wall-clock time at which "today" begins — an unparseable value is
  the typed `invalid-day-start-time` config error, never a silent
  default — plus the pure domain `day_window(now, day_start_time,
  offset)` primitive computing the 24h "today" window in any fixed
  UTC offset (fixed-offset arithmetic per the DST ruling; consumers
  receive time and offset as data). No consumer wiring yet: the today
  view and CLI markers hook it up in a later chunk.
- The task field family (#208, #209, daily-driver wave): `scheduled` (the "when do I
  intend to begin" half of the deadline split), `effort` (whole-second
  estimate), and `importance`/`urgency` (two distinct five-level enums,
  sortable by severity) on every task — editable data model only in
  this increment: values stamp per-field write clocks, persist through
  migration 0007 (additive, epoch-defaulted so pre-migration rows keep
  the remote-wins merge behavior), and `scheduled` rides Deck's native
  `startdate` card slot end-to-end (per-field last-writer-wins merge,
  create/update wire pins). Effort/importance/urgency are local-only
  until the app-data sidecar lane lands; no user surface exposes them
  yet.
- The `TASK_FIELDS` lockstep manifest (#244, daily-driver wave): one exported table
  mapping every mergeable task field to its clock set and Deck sync
  lane, pinned by domain and storage assertions so the field skeleton
  cannot grow half-landed.
- The GUI review round-2 action registry round-up & object context
  menus (#390, chunk 3; D16–D21): twelve new actions — `task.reopen`
  (Ctrl+Space / emacs `C-c C-Space` / vi `U`), `task.move.cancel`
  (Escape everywhere, emacs additionally a lone `C-g` in the board
  view — kept out of Global so `C-g C-/` redo stays reachable),
  `stack.next`/`stack.prev` (Ctrl+Shift+Left/Right, emacs `C-f`/`C-b`,
  vi `g h`/`g l` — wrapping, landing on the new stack's first task),
  `stack.rename`/`stack.move`/`stack.delete` (std `F2`/`Ctrl+Alt+M`/
  `Ctrl+Delete`; emacs org-mode `C-c` family; vi `g r`/`g v`/`g D`),
  `board.create`/`board.rename`/`board.delete` (std
  `Ctrl+Shift+N`/`Ctrl+F2`/`Ctrl+Shift+Delete`; emacs `C-c C-b`/
  `C-c b`/`C-c C-d`; vi `g n`/`g R`/`g d`), and `nav.first-item`/
  `nav.last-item` (std `Ctrl+Home`/`Ctrl+End`, emacs `Alt+<`/`Alt+>`,
  vi `H`/`G`). Every preset binds every new action (the R3 contract);
  emacs also gains `stack.create` = `C-c s`, closing the add-stack
  tooltip's empty-hint gap. The two-tier menu's "Board & stack" group
  splits into Stack and Board. Stack and board context menus join the
  task card's: right-click a stack header, a compact switcher
  segment, or a board-bar segment selects it and opens a flat menu
  whose every bound row renders its keymap hint (the round-1 F-B7
  empty-hint bug's fix — one `[Board, Global]` chain for all object
  menus). New inline flows: board create/rename fields (window-level
  slot), stack rename inside the locked header, and a stack-move flow
  with insertion-slot highlight, wrap cycling, Escape/C-g cancel, and
  a minimal re-normalization confirm; `SetMode(Normal)` during a task
  move no longer leaves the flow dangling. Deletes stay
  confirmation-free by design (D18's scope cut) — the engine's typed
  refusals are the guard.

- The encrypted-file credential store: `[nextcloud] credential_store =
  "file"` selects a third `CredentialStore` implementation for kiosk
  machines without a Secret Service — a machine-generated 0600 key
  file beside the database plus per-account ChaCha20-Poly1305
  envelopes under `<db-dir>/credentials/`, with typed failure fences
  for every corruption shape (missing/insecure/corrupt key file,
  corrupt envelope, unknown version, auth failure) and no silent
  fallback (kiosk wave K12, #126/#159; ADR 0008 amendment).
- The `[kiosk]` config section with `fixed_board`: an optional board
  UUID resolved typed against the live boards at kiosk boot — a pin
  naming no live board refuses the boot with the new
  `unknown-fixed-board` config error; the section's absence leaves
  existing configs and behaviour unchanged (kiosk wave K9, the
  fixed-board pin the multi-board design deferred).
- `contrib/taskboard-kiosk.service`: a system-level unit for the
  kiosk appliance — the config file's `mode = "kiosk"` is the only
  mode authority (never an environment variable), `Restart=always`
  owns crash recovery (ADR 0016), the seat's display environment is
  imported, and the unit anchors at `graphical.target`.
- `docs/kiosk_bringup_runbook.org`: the kiosk bring-up runbook —
  minimal image checklist, software-renderer selection seam,
  on-device harness invocation, the four budget-verdict recording
  slots, and the D250's i686 cross-build lane (target, toolchain,
  build, transfer).
- Per-board cumulative sync counters on the published sync status
  (kiosk wave K8): cycles, pushes, pulls, failures, offline episodes,
  and the last cycle's wall duration — populated at the existing
  `SyncReport` ingestion points, carried by the `status` answer's
  aggregate, connector-health, and per-board folds
  (`last_cycle_ms` as a flat number). Counters are cumulative per
  process run: they live only in the engine's memory and are never
  persisted — a restart resets them by construction.
- Engine activity counters (`status`'s new `engine` object, present in
  daemon and REPL answers): accepted commands, typed rejections, and
  publish signals — handle-side atomics read off the engine handle,
  never part of the published state.
- The UI board dimension: a board-switcher strip above the kanban
  (one segment per live board, regular/expansive layouts; compact
  hides it — keyboard switching still works), `board.next`/`board.
  previous` keys (`b`/`B` in every preset's board scope, no wrap),
  and a per-board sync badge whose phase, pending depth, and recency
  follow the selected board. Switching is pure view state: nothing is
  dispatched to the engine.
- The additive per-connector/per-board `status` answer (daemon IPC and
  REPL sessions alike): `connectors: […]` (name, derived health,
  bound boards) and `boards: […]` (flat, board-id order, local-only
  boards with null attribution) — attribution derived from bindings,
  never stored; a zero-bound connector lists configured-and-idle. The
  `sync` field's byte shape is unchanged; old binaries decode the new
  answer, new binaries decode old answers.
- `--connector <name>` on the targeting verbs: `boards list`,
  `boards select`, `boards publish`, and `sync` (the reported status —
  and the wait's watched aggregate — scope to that connector's bound
  boards; the nudge stays broadcast). An unknown name refuses typed
  `unknown-connector`; cross-connector binds mint the chosen
  connector's binding tag (the move-board surface).
- REPL `boards use <uuid>` (query form answers the selection): the
  session's board selection that later board-scoped lines pre-fill
  `--board`/`--local-board` from; unknown/deleted ids refuse typed.
  One-shot and daemon-socket invocations refuse (`session-only`).
- Completion for board ids (`--board`, `--local-board`, `boards use`),
  configured connector names (`--connector`), and the new
  verbs/flags — offered only at their own positions.
- Multiple active connectors (multi-board wave): a
  `[[sync.connectors]]` array of named entries — `name` (kebab-case,
  unique; the connector id everywhere), `backend`, `server_url`,
  `username` — whose first entry is the default connector; the legacy
  scalar shape loads unchanged (a resolved `nextcloud` backend still
  synthesizes one connector named `nextcloud`), and any mix of the two
  representations is a typed config rejection (`connector-shape-mixed`,
  `nextcloud-table-with-connectors`, `duplicate-connector-name`,
  `invalid-connector-name`, `invalid-connector-entry`) naming the
  offending key. Note: setting `TASKBOARD_SYNC__BACKEND` over an array
  config is now a loud error where it was previously meaningless-but-
  decodable.
- Named connector registry: attach/detach are per-name
  (`ConnectorNameInUse` names the offender; `connector_commands(id)`
  is the named lookup; the daemon attach verb carries an additive
  `name` field whose absent form is byte-identical on the wire), and
  the daemon detach matches against the resolved connector set.
- Per-connector network events with an all-down authority: sync actors
  emit `ConnectorLost`/`ConnectorRestored` for their own connector
  (exactly once per offline episode, report-then-restore order); an
  app-layer counting task emits the global `NetworkLost` when the
  last configured connector flips down and `NetworkRestored` when the
  first comes back. One connector down is normal operation: only its
  own boards flip `Offline` (the engine scopes the write by binding
  tags) while the global banner stays silent. Single-connector
  installs behave exactly as before. `require_connector` verbs now
  refuse with `sync-disabled` when *no* connector is configured
  (including an explicit empty `[[sync.connectors]]` array).
- Connector identity end-to-end on the wire (multi-board wave): board
  bindings carry their owning connector
  (`RemoteBoardBinding { connector, board }` — a v0.1.x database
  upgrades in place, its NULL tags decoding as the synthesized
  `nextcloud` connector), and a connector syncs *every* board bound
  to it, derived from the persisted bindings each cycle with
  per-board pull caches — no cache-clear on rebind, unchanged boards
  stay cheap via per-board validators. The new `ConnectorModule`
  seam (ADR 0010 amended) is how a connector kind plugs in; still
  one connector per install until the plural config lands.
- Board ceremonies at N (multi-board wave): `boards unbind
  [--board <uuid>] [--confirm]` detaches a board's remote binding
  (content and queued ops stay — they strand honestly and drain on a
  re-bind; the sync-status row goes), and `boards reconcile
  [--board <uuid>] [--confirm]` deterministically merges same-title
  duplicates the bind ceremony's pull adopted (survivors keep the
  local ids; duplicate rows neutralize; duplicate create-ops cancel).
  Both verbs refuse without `--confirm`, stating the scope (unbind:
  the bound remote, the ops that will strand, the kept refs;
  reconcile: the match counts). `boards select` gained `--local-board
  <uuid>` (pin the local board), binds a board-less install by
  minting a shell from the remote's title, no longer needs `SetBoard`
  (the engine nudge drives the adoption wait, now scoped to that
  board's own sync row), runs the reconcile automatically after
  adoption, and surfaces the count of stale remote refs the bind
  wiped in its receipt; `boards publish` rides the same per-board
  wait.
- The S2 refresh+draw criterion bench (`cargo bench -p
  taskboard-ui-slint --bench refresh_draw`): the UI refresh pass and
  the headless software draw leg timed as separate lanes (full-frame
  and dirty-tracked partial, region areas reported) at board scale
  N ∈ {100, 1 000, 10 000} on the deterministic fixture, over the
  extended dev-only test platform whose software renderer now draws
  headlessly through an explicit `render_into` pass-through (still
  no implicit draws; sizes 0×0 until `Window::set_size`). The
  renderer-software path is pure CPU over a caller buffer — no
  GL/llvmpipe anywhere. Dev-machine references, never gates; the
  on-device verdict belongs to the phase-B spike (#300).
- Process smoke harnesses (`scripts/process_smoke.py`, local manual
  only): the S1 RSS steady-state run (spawns the standalone daemon on
  the N = 10 000 bench fixture, polls readiness over `status --json`,
  soaks /proc VmRSS to a bounded plateau, reports steady/peak kB
  against the 200 MB budget reference) and the S3 cold-start
  stopwatch (spawns `taskboard gui`, times spawn → the
  `moment = "first-frame"` structured tracing event on the child's
  JSON log stream, against the 5 s reference). References, never
  gates: no CI wiring, exit codes never encode budget comparisons;
  the fixture example `build-bench-fixture` reuses the hot-path bench
  builder verbatim. Runbook: `docs/testing_strategy.org` §14.
- `TASKBOARD_LOG_FORMAT=json`: one-line JSON log events on stderr
  (same writer, same EnvFilter default; output byte-identical when
  unset) — the machine-readable stream the cold-start harness
  selects structured fields on.
- Release archives ship a `taskboard.desktop` launcher
  (`Exec=taskboard gui`, `TryExec` PATH guard, theme-fallback icon)
  and the repo carries the installable copy in `contrib/`; the
  double-click story completes with the `gui` verb and the #236
  display-aware routing (#237).
- README "Launching" section: the bare-invocation routing table
  (mode × TTY × display, plus the `gui`/`daemon` verbs and the
  `TASKBOARD_APP__MODE` escape hatch) verified against the shipped
  routing.
- `mode = "kiosk"` boots the kiosk appliance (ADR 0016): the daemon
  core plus the display-gated fullscreen window — TTY-independent
  bare routing, a loud headless fallback that keeps serving over
  IPC, window-close/signal/`daemon stop` all converging on one
  graceful shutdown; the explicit `daemon` verb stays headless under
  any mode (#237).
- Kiosk window profile flag: `UiSettings.kiosk` (default `false`)
  makes the UI mount apply fullscreen to the shared desktop window
  when set (on the winit backend fullscreen also suppresses
  decorations); derived from the run mode, not a config key, and no
  consumer yet — the kiosk appliance lifecycle hands it over (#237).
- `taskboard gui`: explicit desktop-frontend verb — boots the core
  and opens the desktop window in this process regardless of
  `[app] mode` and TTY; never relayed over IPC and refused as a
  REPL/daemon-socket line (a process entry, like `daemon`) (#237).
- Local board editing: `boards color <hex>`, `boards archive`,
  `boards unarchive`, `boards delete [--confirm]`, and
  `boards reset [--confirm]` (#132, #171, #177). Edits stamp per-field
  clocks and sync through the existing full-send board PUT; delete
  cascades locally and pushes one board delete; reset is a local-only
  repair that also clears sync validators so the next pull
  re-materializes. The destructive verbs ask for `--confirm` and
  print what would be destroyed otherwise.
- GUI stack creation on the live board (#304): a "New stack" action
  (standard preset `S`, vi preset `O`) opens the inline create row on
  an empty or populated board and appends the stack after the
  existing ones.
- Developer-facing MCP harness documentation: the dev-gated
  `mcp = ["slint/mcp"]` feature, the headless `mcp_smoke` integration
  test it powers, the `ui-mcp-smoke` CI job, and the feature-off
  release assertions that keep the shipped binary MCP-free.
- Documentation of the UTC-only due-date semantics: a bare
  `YYYY-MM-DD` due date means midnight UTC, an RFC-3339 timestamp
  with an explicit offset is accepted and normalized to UTC,
  offset-less timestamps are refused, and storage plus all display
  (list/banner, overdue/due-soon comparisons) are UTC — stated in the
  `--due` CLI help and the README quickstart.

### Changed
- Config-validated quantities become minted newtypes (#425; internal —
  no user-visible behavior or output changed): `ConnectorId` gains the
  fallible `from_kebab` mint (the kebab predicate moves into
  `taskboard-domain` as `ConnectorId::is_kebab_name`, one copy), and
  every production construction path — config resolution, the settings
  save/attach/detach choreographies, the attach re-read — mints through
  it, so a hand-edited non-kebab connector name refuses typed at the
  boundary instead of minting a value that can match nothing. The total
  `From` impls stay, documented as the deserialization-and-fixtures
  leniency scope. `ui.scale` travels as `UiScale` (finite, 0.5–3.0,
  minted at the config check and the runtime set-scale request) through
  the UI hand-over to the mount seam; accept/reject semantics, typed
  errors, and config error discriminants are unchanged.
- Stack columns render as cards on the re-anchored surface ladder
  (GUI review round-2 chunk 7; D35): each regular-mode column wraps
  in a `surface` card (`radius-medium`, `space-2` inner padding)
  with the bare board background showing through as the strip
  between and around cards; task cards re-step from `surface` to
  `surface-raised` and the neutral badge chip from `surface` to
  `surface-raised` so both read one step up from their container
  (done cards become recessed wells inside the card). The columns
  row gains a `space-3` background strip (side/bottom padding) and
  the leg's rhythm below the board bar widens to `space-2`. The
  compact leg keeps the bare single-column layout, and the trailing
  add-column affordance stays on the strip, outside the cards.
- The board bar renders as a tab strip (GUI review round-2 chunk 7;
  D36): a `border`-colour hairline baseline rule runs under the whole
  strip; the active tab is merged with the content below — full-height
  board-background paint with rounded top corners that covers the rule
  and flows into the board background — while inactive tabs are
  bordered hairline markers ending on the rule with a `surface-hover`
  lift. Keyboard focus swaps the frame to the focus ring in both
  states. Fixed one-control-row height, the 160·scale elide cap,
  tooltips, right-click menus, and the #411 scalability deferral are
  untouched; a new `Theme.hairline-width` dimension token carries the
  1·scale rule and border width (no colour token added).
- The GUI review round-2 settings form, feedback, and help (issue
  #390, chunk 5; D26–D31): the keymap preset picker becomes a real
  **segmented control** (one grouped container, exactly-one-selected;
  the inverted-`enabled` three-button hack is gone) and the selection
  now **echoes the replied preset** — `KeymapUpdate` carries the
  preset it persisted, so the picker always reflects the last reply,
  whether the change came from this UI, the REPL, or a config reload.
  The bare outcome bar is replaced by a **status line**: a tone dot
  plus a mapped human sentence (busy/done/refused/unreachable/failed
  over every `SettingsError` variant — the mapping is exhaustive by
  compiler construction in `settings_status.rs`, reversing round-1's
  tones-only rule) that **auto-clears after 5 s**, holds while an
  operation is in flight, and re-arms on every new outcome. The UI
  scale row gains a **slider** (0.5–3.0, step 0.1) beside the
  authoritative text field: dragging previews live, release commits
  exactly one `SetScale` per gesture, and a refused commit reverts
  the whole preview. The settings content **scrolls** in a Flickable
  under a pinned header with a conditional scroll indicator — the
  scale controls and status line are reachable at scale 3.0 in a
  minimum window (the window never resizes). Every settings field
  (Name, Server, Username, App password, Scale, Keymap, and each
  connection row) carries an **InfoTip** help affordance: hovering
  the circle-i square shows the help text from presentation
  constants; the same fact feeds the field's accessible description.

- The GUI review round-2 task view & text-field ergonomics (issue
  #390, chunk 4; D22–D25): the `TextField` primitive stops feeding
  its text-derived preferred width into the ancestor layouts — the
  input is absolutely positioned over an explicit cell with the new
  `Theme.space-15` token floor, so **text fields no longer grow or
  slide leftward while typing** (the settings form's F20 symptom;
  all ~15 call sites inherit). The primitive gains the **checkmark
  commit affordance**: a trailing check square renders while a field
  holds focus with a non-empty buffer and fires the same
  `accepted()` seam as Return (click ≡ key) — enabled on the task
  title, the reminder custom offset, the board/stack create rows,
  the column create-task rows, and the settings fields. The reminder
  presets row absorbs the custom-offset entry: preset chips gain
  side padding over a `space-6` floor and a parse-missed custom
  entry now **clears its buffer** (the parse outcome rides the
  `rule-add-custom-accepted` return; the D12 silent absorb stays).
  The
  task view's Due cell becomes a real **due-date editor** (#227;
  both paths, fully keyboard-operable): typed entry (`YYYY-MM-DD` at
  00:00 UTC, `+N<unit>` relative, empty clears, refusals keep the
  buffer) with a live echo, plus an inline calendar picker (month
  stepping, today marker, DueKind tint) whose grid's
  arrows/PageUp/PageDown/Home/End/Return map onto the same seams the
  day cells' clicks use — one `UpdateTask` duedate dispatch for
  every path. The R1-D13 scope-cut comment is retired (W2).

- Every stack column ends in an **Add Task** button (r2
  board-surface, D8): a ghost row in the column's visual language —
  empty stacks show it instead of blank space. Clicking it selects
  that stack and opens the same create flow as the keyboard action;
  typing + Return creates the task there.
- The add-stack affordance has its **tooltip** (r2 board-surface,
  D6): hovering the trailing add-column shows "New stack" plus the
  `stack.create` binding hint (empty hint renders without keys —
  emacs gains its binding in a later chunk). In compact mode the
  New-stack button now sits **inline at the end of the stack
  switcher** instead of its own row (D7).
- The add-stack affordance draws a **square plus glyph** (r2
  board-surface, D5) instead of a near-full-height bar; the affordance
  stays a full-height hit target.
- Clicking a stack header now **selects** it visibly (r2
  board-surface, D4): the selected stack's header paints raised with
  its focus ring in regular/expansive mode (previously only the
  compact switcher showed a selection); during a move flow the target
  highlight still wins and the selection returns when the flow ends.
- Stack headers are one fixed-height control row (r2 board-surface,
  D3): a column's title no longer grows tall on an empty stack — the
  leftover column height flows to the task area instead.
- The stack-width floor rises 96 → **160 logical px** (r2
  board-surface, D2) and persisted widths below the floor heal once at
  load (existing configs with narrower columns re-clamp on first
  launch; healing never writes config back — it reaches the stored
  overlay only when you next drag or step a column).
- Board columns fill the window (r2 board-surface, D1): a stored
  stack width is now a **floor**, not a pin — every column keeps an
  equal stretch, spare window width distributes in equal shares, and
  a board whose columns were all narrowed still consumes the full
  window at any size. The board surface itself now actually receives
  the window width (previously the columns row collapsed to
  min-content; a pre-existing layout gap the chunk fixes en route).
- **Reminder rules are N-rule additive** (#204, daily-driver wave) — a semantic
  change to a live verb: `tasks remind <task> --before <dur>` (and the
  REPL/UI equivalents) now **adds** one reminder rule instead of
  replacing the task's rules; a rule at the same offset is unique (a
  repeat add is a no-op naming the surviving rule, `added: false` in
  the `--json` receipt — `ReminderSetReceipt` renames to
  `ReminderAddReceipt` and gains `added`), a different offset grows a
  second rule, and removal is explicit: `--remove <task> <selector>`
  (rule uuid | unique ≥8-char prefix | the offset text form, e.g.
  `30m`), `--clear` removes all N. A rule-less task's first remind is
  behaviorally identical; the overdue banner and per-firing snooze are
  untouched. The UI task editor grows the rule-list composition
  (preset chips + custom offset + per-row remove).
- `boards select` and the sync actor no longer exchange a targeted
  `SetBoard` command (multi-board wave): the binding tags + per-cycle
  derived board sets replace it entirely (the broadcast `SyncNow`
  nudge remains the only sync signal). Internal vocabulary change; no
  user-visible flag or output changed.
- Board-scoped commands carry their target (multi-board wave): the
  nine board-scoped verbs gained a leading `board: BoardId`
  (inter-actor `StateCommand` wire), and the CLI gained `--board
  <uuid>` on `boards rename/publish/color/archive/unarchive/delete/
  reset`, `stacks add`, and `labels list/add`. Omitting the flag
  resolves the single live board; zero or several live boards refuse
  with the new typed `ambiguous-board` kind (candidates listed). The
  engine's refusals are typed per id now (`unknown-board` replaces
  `no-board`; `board-exists` is gone — `boards create` always mints,
  titles are not uniqueness keys), so the `no-board` and
  `board-exists` JSON error kinds no longer exist (breaking for
  external scripts pinning them). `boards create` also no longer
  refuses a second board.
- Sync status is tracked per board (multi-board wave): the
  storage schema's singleton `sync_status` row becomes a per-board
  table (migration 0006, which also tags board bindings with their
  connector; the legacy `NULL` tag keeps its meaning until the
  board's next edit), and the engine state exposes
  `sync.aggregate()` — every existing surface (CLI receipts, wait
  decisions, UI badge) reads that fold and behaves as before on a
  single-board install.
- A bare `taskboard` launch in `daemon` mode (the default) without a
  terminal but with a graphical session (a file-manager double-click)
  now boots the desktop UI instead of silently starting a headless
  daemon; ssh/systemd/cron contexts (no TTY, no display) keep the
  daemon lifecycle, and the explicit `daemon` subcommand stays the
  headless entry (#236).
- The engine inbox envelope enum re-exported by `taskboard-state` is
  renamed to `EngineEnvelope` — a mechanical rename so the type name
  matches the payload/envelope vocabulary the docs already use; no
  variant, field, or behavior change (#302).
- The `connect` ceremony and daemon bootstrap warn once per process
  when the Nextcloud server URL is cleartext `http`, and the connect
  receipt records a `cleartext_transport` flag (#261).
- Release builds enable overflow-checks: arithmetic overflow in the
  shipped binary panics loudly instead of wrapping silently (dev and
  test builds already checked) (#303).

### Fixed

- `connect`, `logout`, `connections`, and the UI settings writers no
  longer tighten an existing config file to owner-only 0600 on
  rewrite: the file's existing permission bits are preserved (a
  created config still lands 0600). A root-run `connect` against the
  kiosk runbook's `install -m 0644 /etc/taskboard/taskboard.toml` no
  longer leaves the `taskboard` appliance user unable to read its own
  config — the unit restart-looped on that (issue #386). An unreadable
  config file now also surfaces as its own typed error kind
  (`unreadable-file`) instead of being misreported as a decode
  failure ("configuration rejected").
- The GUI review round-2 interaction repairs (issue #390 and the
  round-2 ledger's F7/F8/F11/F13/F28 family): refreshes no longer
  rebuild every slint model wholesale — the push pass diffs the
  desired rows onto persistent per-surface models, so an unchanged
  refresh notifies nothing and small changes touch exactly one row.
  With that transport: double-clicking a card opens Edit Task (the
  first click's focus refresh no longer destroys the clicked card's
  TouchArea), dragging a column's edge tracks the pointer through the
  whole gesture however long (refreshes no longer kill the drag),
  clicking a parent row in the command menu opens its submenu page at
  every window size (the pointer path now syncs what the keyboard
  path syncs) and the compact replace page's rows invoke their own
  page, tooltips no longer stick around after a view switch, a menu
  open/close, or the row's own click, and a tooltip with a keyboard
  hint is wide enough for the keycaps (the width includes the hint
  row; the 60%-window cap holds). View switches leave the window size
  alone at every scale (0.5–3.0) — probed on the real backend, no
  content-min offender found.
- A shutdown signal (or `daemon stop`) landing while the desktop
  window is still booting no longer strands the process: the winit
  backend drops a quit sent before the event loop runs, so the exit
  watchers now park pre-loop quits (`run` skips the loop and the
  graceful tail runs) and deliver the quit through the live proxy
  once it runs. Found by the cold-start smoke harness.
- A push raced by another client's move no longer loses the local
  intent or re-adopts the card as a twin: a 404 at the stale stack
  binding re-resolves the card once against the fresh listings and
  retries at the fresh location — update/move re-fetch before
  classifying (#258), delete retries at the fresh stack (#319); a
  card absent from every stack still converges `RemoteMissing`, so
  the local tombstone stays correct (ADR 0007).
- Label-delta outcomes on task updates are decided per operation: a
  failing `assignLabel`/`removeLabel` sub-call no longer completes
  the primary edit and its other label ops with the primary's
  result — ops whose write landed stay `Applied`, the failing
  label's ops carry the classified outcome per op, and an
  unresolvable label reaches `Rejected { LocalData }` instead of
  being silently shadowed (#310, #314).
- A stray shared reference to the running core no longer panics the
  daemon mid-shutdown: the shutdown tail logs the advisory refcount,
  runs the ownership-free salvage steps (engine flush, `Shutdown`
  broadcast), and exits through the regular error path (#257).
- Server URLs carrying credentials (`https://user:password@host`)
  are rejected with a typed error at config validation and client
  construction, instead of validating and carrying the credential
  through resolved-config logs and the client's base URL (#260).
- Files taskboard creates are owner-only (`0600`): the SQLite
  database with its WAL/SHM siblings, the daemon lock sidecar, and
  the config file rewritten by `connect` and `logout`; pre-existing
  permissive copies are tightened on the next run (#297).
- REPL history is written through taskboard's own `0600` writer
  instead of rustyline's save, whose process-global umask window
  could strip permission bits from files sibling threads created
  concurrently (#289).
- The state engine exits when every input source is closed: the run
  loop no longer pins its own system-event sender, which had made
  the all-sources-closed exit structurally unreachable (#316).

## [0.1.0] - 2026-10-16

### Added
- The desktop GUI (ADR 0015): `mode = "desktop"` boots a Slint
  application — kanban board, stack detail, and task detail views
  driven by the same headless core as the CLI (lock-free state reads;
  no business logic in the UI). Bare `taskboard` routes by the
  configured app mode.

- Keyboard-first operation: every verb is an action with a rebindable
  keybinding — `standard`, `emacs`, and `vi` presets plus per-action
  overrides under `[ui.keymap]` (invalid overrides are dropped with a
  startup warning, never blocking); a generated action menu lists the
  current context's verbs with their key hints.

- Display scaling and responsiveness: `[ui] scale` (0.5–3.0) applies
  as a live token multiplier, with `SLINT_SCALE_FACTOR` as the OS-DPI
  escape hatch; unscaled responsive breakpoints (compact below 720 px,
  regular, expansive at 1440 px and up) degenerate the board for
  narrow windows.

- Sync status and overdue banner: a persistent status-bar sync
  indicator, and an overdue-task banner whose snooze/mute actions ride
  the reminders command path.

- Accessibility and quality machinery: a WCAG contrast-checked dark
  theme, screen-reader labels on every control, and a headless
  keyboard-journey test suite that keeps every core verb within its
  keypress budget.

- REPL completion disambiguates hard title collisions: when two live
  stacks, tasks, or labels share a title, the menu replaces the
  (guaranteed-`Ambiguous*`) title row with one `title - uuid` row per
  entity that inserts the full uuid; unambiguous titles keep the plain
  row.

- File-backed REPL history: submitted session lines persist in
  `<data-dir>/taskboard/history` (0600, following the db-path data-dir
  precedent) and are recalled in later sessions; persistence is
  best-effort — a missing, corrupt, or unwritable history file degrades
  to in-memory recall with a warning and self-heals on the next
  recorded line, and prompted ceremony answers (secrets, confirms)
  never enter it.

- `logout` detaches a running daemon's connector (WI-H, ADR 0012
  amendment): after the local logout succeeds, the client best-effort
  asks the daemon serving the same database to drop its sync actor —
  the booted credential dies in memory and the daemon keeps serving
  local edits connector-less (not a standalone flip; the config is
  never rewritten). The nudge never changes the exit code: the receipt
  gains the optional `daemon_connector` field, degradations are stderr
  warnings, and no daemon at all is no warning. The verb is additive
  on the protocol-v1 wire (recorded policy: additive variants never
  bump; shape changes do), matched against the daemon's booted
  identity (`identity-mismatch` otherwise), and idempotent. Honesty
  riders: `sync` on a detached engine refuses typed
  `sync-unavailable` instead of lying via `sync-timed-out`, and
  daemon-side `status` reports `connector_attached` so a detached
  daemon no longer masquerades as a syncing one.

- REPL tab-completion (WI-G): inside the interactive session, Tab
  completes verbs, subverbs, long flags, and the live stack/task/label
  titles at the cursor — including the attached `--stack=<Tab>` value
  form and quoting of multi-word titles. Candidates are derived from
  the static clap surface plus a lock-free `ArcSwap<AppState>` read on
  the editor thread (strictly read-only), parity the selector
  semantics; the in-session-refused verbs (`daemon`, `login`,
  `logout`, `config`) and `--config` are never offered.

- Daemon IPC (ADR 0012, WI-5): while `taskboard daemon` runs it serves
  a per-database local socket, and every core verb (`tasks`/`stacks`/
  `labels`, `boards select/create/rename/publish`, `sync`) is
  transparently served over it — no per-command core boot, no
  `instance-locked` friction; without a daemon the invocation boots
  in-process exactly as before (byte-identical output, silent
  routing). New operator verbs: `taskboard status [--json]` (live
  daemon/backend/pid/sync view, or the lock holder pid — never boots a
  core, never triggers a cycle) and `taskboard daemon stop` (graceful
  remote shutdown; typed `no-daemon`, exit 1, when none serves). A
  versioned wire envelope (protocol v1) crosses the socket; the
  systemd user-unit template ships in `contrib/taskboard.service`.

- The interactive REPL frontend (ADR 0011): bare `taskboard` with a
  TTY on stdin opens a parted-style session over one booted core —
  each line is one CLI invocation (`shell-words` quoting, per-line
  `--json`), `exit`/`quit`/Ctrl-D shut the core down gracefully, and
  failed lines never abort the session. `daemon`, `login`, and
  per-line `--config` are refused in-session; the per-database
  instance lock keeps the session and the daemon mutually exclusive.

- The full CLI editing surface (WI-3): `tasks edit/delete/label/
  unlabel`, `stacks rename/delete/move`, `labels list/add/edit/
  delete`, and `boards rename <title>` (title-only, both backends —
  the board title pushes when the board is bound and rides the next
  `boards publish` otherwise). `boards rename` carries
  `BoardClocks.title` (migration 0002, epoch default: pre-existing
  rows keep the remote-wins merge behavior bit-for-bit) and the
  field-wise board-title merge (ADR 0004 amendment).

- The cascading task selector: exact uuid → unique ≥8-char uuid
  prefix → exact title; ambiguity is the typed `ambiguous-task` error
  listing candidate titles.

- `logout`: deletes the stored secret and — under the auto backend —
  strips the config's server/user keys (an explicit
  `backend = "nextcloud"` is never rewritten; ADR 0010).

- `config print`: a decode-only diagnostic that always prints; the
  payload carries `valid` plus the typed `invalid_kind` (exit 0 unless
  the file is undecodable).

- XDG config discovery: `--config` > `TASKBOARD_CONFIG` >
  `$XDG_CONFIG_HOME/taskboard/taskboard.toml` (when present) >
  `./taskboard.toml`.

- The `StorageFailed` system event: edge-triggered at the state
  engine's storage-failure reply site (one event per failure epoch);
  the daemon logs it, the Phase 7 UI banner will consume it.

- The J5 tier-2 journey (`it_nextcloud_docker_cli_editing_push`): the
  editing surface lands server-side after one sync.

- Conditional reads in the Deck client (`taskboard-sync-nextcloud`):
  `fetch_boards`/`fetch_stacks`/`fetch_card` take opaque `Validators`
  (ETag / `Last-Modified`, re-emitted verbatim) and return `Fetch<T>`;
  `304 Not Modified` surfaces as `data: None` — mapped before any body
  decoding, never an error. Tier 1 pins the 304-with-empty-body contract
  and a mocked fetch→304→mutate→refetch cycle; Tier 2/3 gain a real
  `conditional_read_cycle`.

- Deck client write surface in `taskboard-sync-nextcloud`: board
  update/restore/clone (sparse `BoardChanges`, all-false-default
  `CloneOptions`), stack create/update/delete, card create/update (full
  round-trip)/delete/archive/reorder-move/label assign-remove, and label
  create/update/delete — every `PUT`/`POST` body pinned by contract tests.
  Tier 2/3 gain a `full_tree_lifecycle` live suite behavior and the fixture
  harvest example now builds the same run-id tree and writes per-resource
  `*_live.json` fixtures.

- Deck client read surface in `taskboard-sync-nextcloud`: full wire model
  (`Board`, `Stack`, `Card`, `Label`, `Acl`, `Participant`, `Attachment`,
  `BoardPermissions`, `StackFilter`) with typed read endpoints for boards,
  stacks (active/archived), cards, labels, and attachment metadata; ISO-8601
  `duedate`/`done` decode via chrono. `DeckError` gains `BadRequest`,
  `Conflict`, and `PreconditionFailed` for a complete typed status matrix;
  Tier 0 fixtures and Tier 1 contract tests cover every endpoint.

- `DeckColor` validated type in `taskboard-sync-nextcloud`: six hex digits
  (no `#`), case-normalized on construction, proptest-validated; reads stay
  lenient so a malformed server color cannot fail a whole listing. The
  `create_board` color parameter and `Board.color` now carry the type.

- Nextcloud sync testing build-out: `DeckClient` boards slice (list/create/
  delete over the Deck OCS REST API) with typed errors and bounded retries,
  plus all four test tiers — Tier 0 fixtures, Tier 1 `wiremock` contract
  tests (hermetic, default suite), Tier 2 secretless dockerized Nextcloud
  (`scripts/nextcloud_it_setup.sh`, new `Nextcloud Integration (docker)` CI
  job), and Tier 3 live-server tests loaded from a git-ignored `.env`
  (`.env.example` template; `examples/harvest_deck_fixtures.rs` refreshes
  the recorded fixtures).

- Testing strategy: tiered integration-test model for the Nextcloud sync
  client (Deck OCS REST API) — recorded fixtures, `wiremock` contract
  tests, a secretless dockerized-Nextcloud tier in CI, and a local-only
  live tier against a private server; no Nextcloud secrets exist in CI by
  design, so the server address cannot leak into public CI logs.

- SonarCloud static analysis: `sonar-project.properties` and a
  `SonarCloud Analysis` CI job (token via the `SONAR_TOKEN` secret).

- Binary release pipeline: tag-triggered (`v*`) matrix workflow building
  `x86_64-unknown-linux-gnu` (tar.gz) and `x86_64-pc-windows-msvc` (zip)
  archives plus SHA256 checksums, publishing a GitHub Release whose body
  is the version's CHANGELOG section; `workflow_dispatch` dry-run mode
  for pipeline validation without a Release.

- `scripts/package_release.py`: shared packaging script used identically
  by CI and locally (byte-identical artifacts) — builds `taskboard-app`,
  stages binary + licenses + README + changelog into `target/dist/`,
  archives, and writes checksums; `--check` verifies existing archives.

- `scripts/changelog.py`: Keep-a-Changelog tooling (`promote`, `extract`,
  `self-test`) used by `bump_version.sh` and the release workflow.

- Bencher continuous benchmark tracking: the `benchmark` job records
  criterion results from `main` pushes via the Bencher CLI
  (`BENCHER_API_TOKEN` secret); tolerant until first benches exist.

- Cargo workspace with six crates: `taskboard-domain`, `taskboard-state`,
  `taskboard-sync-nextcloud`, `taskboard-storage-sqlite`,
  `taskboard-ui-slint`, `taskboard-app` (edition 2024, single global semver).

- Dual-tier licensing: `MIT OR Apache-2.0` for library crates,
  `AGPL-3.0-only` for application/UI crates.

- CI pipeline (fmt/clippy, cargo-deny, nextest + doc tests, llvm-cov
  coverage, criterion benchmarks) plus weekly mutation-testing and Miri
  workflows.

- Project documentation: `docs/architecture.org`,
  `docs/coding_guidelines.org`, `docs/testing_strategy.org`, and ADRs
  0001–0003; `README.org` and `AGENTS.md` agent guidance.

- Release tooling: `scripts/bump_version.sh`, `CHANGELOG.md`.

- Repository hygiene: `.githooks/pre-commit` (fmt + clippy), Dependabot
  config, issue & PR templates, `.editorconfig`, `.gitattributes`.

- The reminders subsystem (ADR 0013): due-anchored reminder rules on
  duedated tasks, evaluated by the state engine's tick (the first tick
  is immediate, so reminders left undelivered before a shutdown fire
  on the next boot) with durable-then-notify delivery — delivery
  markers persist before any notification is sent, and a failed batch
  fires nothing. Every duedated task carries an implicit default
  reminder at the configured offset; explicit rules replace it.

- `tasks remind`: set or replace a task's reminder with `--before`,
  list the board's rules with `--list`, remove them with `--clear`,
  and mute/unmute a task's firings with `--off`/`--on` (the overdue
  banner is unaffected by mute).

- The `[reminders]` config table: `enabled`, `tick_secs`, and
  `default_before` (validated; a bad duration is a typed
  `invalid-duration` rejection).

- Reminder delivery v1 logs one line per notification through the
  `NotificationSink` port (`LogSink`); desktop toasts are a follow-up
  behind the same port.

### Fixed
- The interactive REPL no longer leaves the terminal in raw mode when
  the session ends via `exit`/`quit`: the session saves stdin's line
  discipline at boot and restores it on exit (previously, a shutdown
  racing the editor thread's fresh `readline()` could exit the process
  mid-raw-mode, and the pane's shell then read input without echoing
  it — invisible typing until `stty sane`). A signal kill
  mid-`readline` remains unrestoreable (no destructors run).

- Label assignment on an *existing* card now actually lands: Deck 1.19
  rejects card-update PUTs carrying a non-empty `labels` field (HTTP
  400), so the push executor strips the label list from the field PUT
  and applies the desired set through the dedicated
  `assignLabel`/`removeLabel` sub-endpoints (tier-2 verified by the J5
  journey; previously the op dead-lettered as `bad-request`).

- Deck client wire-model fixes found by the live tiers: explicitly `null`
  collections (e.g. a stack's `cards` once empty) decode as empty vectors;
  card `labels` accept both bare ids and inlined label objects (writes
  re-emit ids); `Participant` reads accept an id string or a full object,
  and writes re-emit the id string (Deck's card PUT requires the owner as
  the bare user id); `reorder_card` no longer decodes the response body
  (Deck versions return a card object, an array, or nothing) and returns
  `()`, reading back via `card()` when needed.

- CI: coverage job uses `cargo llvm-cov nextest` (the `--nextest` flag was
  removed in llvm-cov 0.9) with `--no-tests=warn`; benchmark job runs plain
  `cargo bench` (`--output-format bencher` is a nightly-only libtest option).

- CI: use the valid `dtolnay/rust-toolchain@stable` ref (the previous `@v1`
  ref does not exist and would have failed every toolchain job on first run).

- CI: mutation-testing workflow reports surviving mutants via artifacts
  instead of failing the scheduled job.

- `LICENSE-MIT` copyright holder corrected to the project author; repository
  URL placeholder (`your-username`) replaced with the real remote in
  `Cargo.toml` and the README badge.

- CI: SonarCloud job is skipped for Dependabot PR runs (they receive no
  repository secrets, so the scan could only fail with "Not authorized");
  main-push analysis remains the authoritative record.

- CI: replaced the deprecated `SonarSource/sonarcloud-github-action@v5`
  with its successor `SonarSource/sonarqube-scan-action@v8`.

- `scripts/bump_version.sh`: the changelog promotion no longer corrupts
  `### ` subsections (stray `#` lines, H2-ified `### Fixed`), nests the
  promoted section under Unreleased, or crashes on an empty Unreleased
  section — reimplemented on `scripts/changelog.py` with golden self-tests
  wired into the CI `check` job.

### Changed
- The daemon no longer refuses the `standalone` backend (ADR 0010
  decision 9 amended): it owns the core and serves commands over IPC
  under both backends; under `standalone` the sync actor is not
  spawned at all and no credential store is consulted.

- `taskboard <core verb>` now prefers a running daemon (served over
  its socket) and boots in-process only when none answers.

- `tasks done/move` accept prefixes and exact titles (exact uuid stays
  first, so existing scripts keep working); task resolution now
  happens in-core against the published state.

- The `MutationReceipt` JSON payload gains the additive `labels` key
  (the attached label titles after the mutation).

- Growth roadmap Phase 6b (ADR 0010): *stand-alone-first* — the app
  works fully stand-alone with no server configured, and Nextcloud sync
  becomes the first optional connector selected by `[sync] backend =
  "standalone" | "nextcloud"` (absent = auto: `nextcloud` iff
  `[nextcloud] server_url` is present, so a half-configured file stays
  loud). Fresh installs boot and author with zero network and zero
  keyring access; existing `[nextcloud]` configs behave exactly as
  before.

- `boards create <title> [--color <hex>]` and a `stacks` group with
  `stacks add <title> [--order <n>]`: local board/stack authoring
  (the engine's `CreateBoard`/`BindBoard` domain commands, no outbox
  op for board creation; `stacks add` appends by default).

- The attach ceremonies for migrating stand-alone data onto a remote
  board: `boards publish` (create a fresh Deck board from the local
  one, bind, and let the first cycle drain the outbox) and
  `boards select --bind-local-board` (bind onto an existing board;
  union-by-identity semantics — see ADR 0010).

- The typed `sync-disabled` refusal (`--json` kind `sync-disabled`,
  exit 1) for the connector-only verbs — `sync`, `boards
  list/select/publish` — enforced at handler entry before any core
  boot or client construction. `login` under an *explicit*
  `standalone` backend refuses with the distinct kind
  `backend-explicit-standalone`; the daemon itself no longer refuses
  under standalone (the Changed entry above).

- The human `tasks list` stand-alone footer (`stand-alone: N changes
  waiting for a sync connector`); presentation-only, the `--json`
  payload is unchanged.

- The J4 tier-2 journey (`it_nextcloud_docker_cli_standalone_publish`):
  stand-alone authoring → login → publish → server-side verification
  → local ids preserved.

- Bare `taskboard` with a TTY on stdin now opens the interactive REPL
  instead of resolving `[app] mode` (the daemon); non-TTY invocations
  are unchanged, and every explicit subcommand behaves exactly as
  before.

- Fresh installs default to the `standalone` backend: `[sync]
  backend` is optional and `[nextcloud] server_url`/`username` are
  required only when the resolved backend is `nextcloud` (a present
  but malformed value is still a loud typed rejection). Configs with
  `[nextcloud] server_url` are unchanged (auto resolves `nextcloud`).

- Growth roadmap Phase 6 (ADR 0009): the operator CLI over the
  in-process core — `login` (verify-then-persist: flags/prompted
  server+user, app password via `rpassword` on a terminal or piped
  stdin, non-secrets overlaid into `taskboard.toml` comment-preserving
  via `toml_edit`, the secret into the credential store; the env store
  reports `stored: "env"` with nothing written to any file), `boards
  list/select` (discovery plus the one-time single-board binding with
  pre-validation, rebind refusal, and an adoption wait), `tasks
  list/add/done/move` (exact selectors, append-by-default orders,
  `NoOp` idempotency), and `sync` (bounded wait, honest
  Succeeded/Failed/TimedOut outcome, `--no-wait`). A global `--json`
  flag switches every command to typed JSON payloads on stdout with
  kebab-case error envelopes on stderr; exit codes are 0 success /
  1 runtime failure / 2 clap usage / 130 second-signal force exit.

- The per-database instance lock: an OS advisory exclusive lock on a
  `<db>.lock` sidecar (fd-lock), acquired in `bootstrap()` before the
  database opens and held for the process lifetime — a second core on
  the same database fails with the typed `InstanceLocked` (db path +
  holder pid), machine-distinguishable as `instance-locked`; released
  automatically on process death.

- The J-series binary-driving E2E tier (`it_nextcloud_docker_cli_*`):
  the real `taskboard` binary against the dockerized Nextcloud —
  login (stdin-piped password) → boards select → tasks add → sync
  with server-side push/pull verification, and the bad-token rejection
  journey; picked up by the existing `nextcloud-integration.yml`
  filter with no workflow change.

- Growth roadmap Phase 5 (ADR 0008): the `taskboard-app` bootstrap
  library — `AppConfig` (figment: defaults → `taskboard.toml` →
  `TASKBOARD_*` env, strict tables enforcing no-secrets-in-TOML), the
  `CredentialStore` port with `KeyringStore` (OS keyring, default) and
  `EnvCredentialStore` (`TASKBOARD_APP_PASSWORD` escape hatch,
  config-selected, never a silent fallback), the redacting
  `AppPassword` newtype, and `bootstrap()` wiring the full actor graph
  into an `App` handle with a graceful bounded shutdown sequence
  (`App::shutdown()`, `run_until_signal` with ctrl_c/SIGTERM). The
  `taskboard` binary is now lib+bin: `main.rs` is thin glue, the
  library is importable by tests and every later frontend.

- Docker-tier app tests (`it_nextcloud_docker_*` in `taskboard-app`):
  the daemon pull/push/shutdown journey against the dockerized
  Nextcloud — the automated Phase 5 exit demo; wired into
  `nextcloud-integration.yml` alongside the sync crate's tier.

- Growth roadmap Phase 4 (ADR 0007): the sync actor in
  `taskboard-sync-nextcloud` — `spawn_sync_actor` (scheduled push-first
  cycle: read via the `SyncStateReader` port → push → pull → report),
  the push executor (binding overlay, fetch-before-write, typed
  `DeckError` → `PushResult` classification), the pull assembler
  (three conditional reads, both stack listings, per-card detail
  refresh), the offline FSM (`NetworkLost`/`NetworkRestored`
  ownership, failure backoff, cycle-in-flight `SyncNow` dedupe,
  `SetBoard` consumption with binding-by-adoption), and the D-series
  two-way sync demonstration against dockerized Nextcloud (the phase
  exit).

- Domain additions feeding the above: `push::plan_pushes` (the pure
  push-coalescing planner with proptest partition/order laws), the
  `SyncStateReader` port (+ `EngineCommand::ReadState`, engine and
  fake implementations), `apply_push_report` (the pipeline's push-only
  composition for evidence-then-verdict `Failed` reports), the
  `SyncReport` reshape (`Completed { validators, read_at }`, `Failed {
  pushes, read_at }`), `SyncErrorKind::NoBoard`, and the storage
  codec's `ValidatorKey::ArchivedStacks` tag.

- The self-clobber guard (ADR 0007): sync reports carry `read_at`, and
  the merge/echo adoption protects fields covered by outbox ops queued
  after the cycle's read — a local edit landing between the cycle's
  state read and its push can no longer be silently wiped by the
  pull's whole-card stamps (observed end-to-end in the D6 docker
  scenario).

- Doc-test usage examples on `spawn_sync_actor` and `plan_pushes`.

- Growth roadmap Phase 3 (ADR 0006): the State Engine in
  `taskboard-state` — `EngineCore` (working state advancing only by
  interpreting persisted batches through the shared `apply_actions`),
  the `spawn_state_engine` actor loop (biased `select!` over
  `EngineCommand::{Execute, Flush}`, `SyncReport`s, and `SystemEvent`s),
  `EngineHandle` (`execute` receipts, fire-and-forget `dispatch`, the
  `flush` durability barrier, `subscribe`, `shared_state`),
  apply-before-interpret failure semantics (no rollback: memory never
  advances on storage failure), the never-persisted `Syncing` transient,
  and the `SyncNow` nudge contract — plus the S-series insta snapshot
  suite defining the engine's behavioral contract, the A-series routing
  tests, and the P-series architectural proptests.

- Domain additions feeding the above: `command::plan_command` (the full
  local-command semantics catalogue with typed `CommandError`
  rejections, idempotent no-ops, bound/unbound delete split, and the
  `DeleteStack` cascade), `persistence::apply_actions` (the port
  contract's batch semantics as one production function, now also
  delegated to by the `InMemoryRepository` fake), and the promoted
  `test_support::CountingIds` + `state_command_strategy`.

- `taskboard-storage-sqlite`: `impl TaskRepository for StorageHandle`
  (the port adapter letting the state engine drive the storage actor
  without a crate dependency).

- Growth roadmap Phase 2 (ADR 0005): the SQLite local persistence crate
  `taskboard-storage-sqlite` — normalized schema (`boards`/`stacks`/
  `tasks`/`labels`, `task_labels`, `outbox`, `sync_metadata`,
  `sync_status`) with embedded boot-time migrations, WAL +
  `synchronous=NORMAL` on a single connection, `SqliteTaskRepository`
  implementing the `TaskRepository` port with compile-time checked
  `query!` SQL (one transaction per batch, FK-safe fixed order, derived
  `pending_ops`, retained tombstones), the `StorageCommand` actor with
  reply-carrying `StorageHandle`, and the offline sqlx workflow
  (`scripts/sqlx-prepare.sh`, committed `.sqlx/`, `SQLX_OFFLINE` +
  freshness check in CI).

- Domain additions feeding the above: async contract harness
  (`assert_task_repository_contract_async`), `PersistenceAction::
  UpsertSyncStatus` (with derived-`pending_ops` parity in the
  `InMemoryRepository` fake).

- Growth roadmap Phase 1 (ADR 0004): the canonical domain model in
  `taskboard-domain` — `Board`/`Stack`/`Task`/`Label` entities with
  per-field write clocks, typed local (UUIDv7) and remote (Deck) ids with
  board-scoped composite refs, the real `AppState` with sync status, the
  inter-actor message payloads (`StateCommand`, `SystemEvent`,
  `EngineSignal`, `SyncCommand`, `SyncReport` + changesets), the outbox
  operation types (`OpId`, `LocalOp`, `PendingOp`), the async-shaped
  `TaskRepository` port (`BoxFuture`, no runtime dependency) with
  `RepositoryError`, `PersistedState`, and batched `PersistenceAction`s,
  the `Clock`/`IdGenerator` seams, and the pure sync conflict policy
  (`apply_sync_report` + R1–R9 per-entity merge primitives) with a
  property-test suite (idempotence, strict-newness tie-break, clock
  monotonicity, convergence, no-panic bombardment).

- Non-default `test-support` cargo feature on `taskboard-domain`:
  property strategies for all entities and remote views, an
  `InMemoryRepository` fake, and the repository contract harness
  (`assert_task_repository_contract`) that Phase 2's sqlite adapter will
  re-run verbatim.

- ADR 0004 "Sync conflict policy" documenting the per-field LWW model,
  tie-to-local bias, deletion rules (timestamped soft deletes for
  boards/stacks/labels, delete-wins fallback for cards), unconditional
  echo adoption, and the engine-executes-merge split.

- `login` replaces the manual `secret-tool` dance of the Phase 5 M1
  quickstart: the ceremony verifies against the server before anything
  is persisted and stores the app password itself.

- The sync merge fixes a duplicate-adoption bug: a card created and
  pulled within one sync cycle was adopted twice (the create's echo
  binding was invisible to the pull's card-to-local matching map,
  which read pre-echo state), wedging subsequent update pushes.

- The `taskboard` binary now accepts `daemon` (subcommand), a global
  `--config <path>`, and `--version`; no arguments still runs the
  daemon, per `[app] mode` (ADR 0008).

- The State Engine's `Shutdown` semantics: the engine no longer exits
  immediately — it enters a shutdown drain (new work declined with
  `EngineGone`, flush still answers) and keeps ingesting sync reports
  until the reports channel closes, so the sync actor's final cycle
  evidence lands before the process exits (the graceful-shutdown half
  of the duplicate-create window, ADR 0007/0008).

- `DeckClient`'s `Debug` impl redacts the credentials field (a
  `?client` log line used to print the app password).

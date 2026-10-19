# [Report] Kiosk ∥ Daily-Driver Parallelism Analysis

- **Date**: 2026-10-19
- **Question assessed**: during the daily-driver planning round
  (2026-10-18) the user was advised that the daily-driver and kiosk
  milestones "could be implemented in parallel without major
  conflicts expected" (daily-driver framing
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`
  §3: "Kiosk wave 1 is READY-unstarted … with minimal overlap (ui
  shell/window profile vs this wave's new views)" and §7.5: "whether
  kiosk or daily-driver implements first is the user's scheduling
  call — both follow multi-board cleanly"). The user asked whether
  that assessment holds **given the detailed daily-driver plan**
  (`.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  + its dispatch plan) and the now-detailed kiosk plan
  (`.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`).
- **Method**: per-chunk file footprints of all three in-flight or
  imminent rounds — multi-board (7 chunks remaining of 8, dispatch
  plan chunk table), daily-driver (11 chunks, gated on multi-board
  by its MD-1), kiosk (14 chunks, phases A/C) — compared pairwise;
  dependency (not merely conflict) edges separated from textual
  collision zones; capacity constraints from AGENTS §8 measured
  data. Code anchors verified on `main` @ `5480869`.
- **Verdict**: **agree, with two qualifications and one correction
  of scope.** The claim was made about *kiosk wave 1* (the
  appliance) and it holds exactly: the appliance chunks are
  file-disjoint from every daily-driver chunk. The full kiosk
  **milestone** adds phase-C items where (a) one item is a
  *dependency*, not a conflict — the ambient view **needs**
  daily-driver chunk 10's today view and shares its files, so
  "parallel" is wrong for it in either direction; and (b) one
  phase-A item (sync metrics) needs a dispatch *window*, not
  freedom — it edits the one file (`sync/actor.rs`) both other
  rounds rewrite. Everything else parallels cleanly, including
  against the in-flight multi-board round. Details and mitigations
  below.

## 1. The rounds and their file footprints (summary)

- **Multi-board** (in flight; 1/8 landed): chunks 2–7 rewrite the
  domain vocabulary (`command.rs`, `messages.rs`, `entities.rs`),
  engine ceremonies (`state/`), the sync actor (`sync/actor.rs`,
  connector modules), app config/bootstrap (registry, authority,
  `[[sync.connectors]]`), app CLI/REPL verbs (`cli.rs`, `repl.rs`,
  `completion.rs`, `output.rs`), and the UI board dimension
  (`view_state.rs`, `actions.rs`, `router.rs`, `models.rs`,
  `board-bar.slint`); chunk 8 is the docs exit.
- **Daily-driver** (gated on multi-board completing — its MD-1):
  chunks 1/3/4 own the domain/storage/sync data plane
  (`entities.rs`, `pipeline.rs`, `merge.rs`, `push.rs`, `outbox.rs`,
  `appdata.rs`, `sync/actor.rs`, storage codec/repo/migrations
  0007–0008); chunks 5–8 the app surfaces + sink (`cli.rs`,
  `commands/*`, `repl.rs`, `config.rs` sections, `notify-rust`);
  chunks 9–10 the find + today/inbox UI (`view_state.rs`,
  `actions.rs`, `router.rs`, `models.rs`, `presentation.rs`, new
  slint compositions, `cli.rs`, `repl.rs`); chunk 11 the docs exit.
- **Kiosk** (this wave): the appliance chunks K1–K4 own app
  lifecycle (`ui.rs`, `main.rs`, `cli.rs` one variant,
  `completion.rs`) + the mount seam (`ui-slint lib.rs`,
  `UiSettings`) + packaging (`contrib/`, `scripts/package_release.
  py`); the instrumentation chunks K5–K7 own **new** files
  (`scripts/`, `benches/`, the `test_platform.rs` seam); K8 owns
  domain `state.rs` status fields + the `sync/actor.rs` emission
  site + `output.rs`; K9 owns `config.rs`'s new `[kiosk]` section +
  `contrib/` unit; K10 is docs-only; K11–K13 (phase C) own a new
  HTTP module, `secrets.rs`/`config.rs`, and the UI kiosk surface
  (`view_state.rs`/`actions.rs`/`router.rs`/`models.rs`/new slint).

## 2. Pairwise findings

### 2.1 Kiosk phase A vs daily-driver — clean, as advised

The appliance (K1–K4) and the instrumentation (K5–K7, K9, K10) are
file-disjoint from **every** daily-driver chunk: DD touches no app
lifecycle file (`ui.rs`/`main.rs`), no mount/window code, no
packaging, no bench/script surface it does not itself create. The
daily-driver plan's own file lists confirm the framing's phrase —
its UI work is *views/actions/state*, the kiosk appliance's is
*shell/lifecycle/profile*. The two waves even cooperate by design:
DD chunk 10 builds the today view **rotation-ready** and explicitly
lands "no kiosk-specific code" (D-V1/D-V3), reserving the kiosk
surface for this wave.

### 2.2 The two qualifications

1. **K13 (ambient view + rotation) is post-DD-10, not parallel.**
   It consumes `ViewId::Today` and the rotation-ready design (a
   dependency), and it edits the exact files DD chunks 9–10 edit
   (`view_state.rs`, `actions.rs`, `models.rs`, plus `cli.rs`/
   `repl.rs` adjacency) — the same collision the DD plan itself
   flags between its own chunks 9 and 10 ("dispatch them serially
   … or accept the rebase dance"). This is the one place the
   original "no major conflicts" advice would be *wrong* if read as
   covering the whole milestone: dispatch K13 only after DD chunk
   10 merges. (It also wants multi-board chunk 7's board-selection
   vocabulary for the pin — which the sequencing gives it for
   free.)
2. **K8 (sync metrics) needs a dispatch window.** Its emission site
   is `sync/actor.rs` — rewritten by multi-board chunk 4
   (connector modules) and again by DD chunks 3–4 (sidecar
   transport + fold). Dispatching K8 concurrently with either buys
   a genuine mid-file conflict on the round's most-contested file.
   The window: **before multi-board chunk 4 starts**, or — the
   natural slot — **in the multi-board→daily-driver gap** (after
   the multi-board round closes, before DD chunk 3 dispatches; the
   gap exists by DD's own round gate). Domain-side (the status
   fields on `SyncStates`, multi-board chunk 1's landed shape) is
   additive and safe anywhere.

### 2.3 Kiosk phase A vs multi-board (in flight) — parallel-safe with two rebase-scale adjacencies

The kiosk appliance needs **no** multi-board result (the framing's
"both follow multi-board cleanly" verified: the appliance touches
no domain contract — the arch-fit audit's own words — and no file
multi-board chunks 2–5 rewrite). Two same-file adjacencies,
different regions, rebase-dance scale, no design interaction:
K1's `Command::Gui` in `cli.rs` vs multi-board chunk 6's connector
verbs; K9's `[kiosk]` section vs chunk 5's `[[sync.connectors]]` in
`config.rs` (both `deny_unknown_fields` sections coexist; K9 after
chunk 5 merges avoids even the rebase). K8's window (above) is the
only hard rule against this round.

### 2.4 Phase C remainder

K11 (dashboard) is new-module/new-dependency work — disjoint from
everything except the root `Cargo.toml` dependency block, where DD
chunk 8 adds `notify-rust` (different lines; trivial). K12 (file
credential store) edits `secrets.rs` — untouched by both other
rounds — plus a `config.rs` section (same adjacency class as K9).

### 2.5 The shared loose ends (all waves, worth stating once)

`docs/architecture.org`, `README.org`, and `CHANGELOG.md` are
touched by every round's docs/exit chunk (MB-8, DD-11, K4/K14) and
by per-chunk Unreleased lines — serialise the three exit chunks;
the Unreleased-line traffic merges as distinct appends. The UI
`test_platform.rs` seam (K6) is also exercised by DD chunk 10's
golden-visual regen — additive extension, no conflict expected.

## 3. Capacity and process constraints (the practical half of "parallel")

- **Worktree disk**: each concurrent chunk's worktree costs a
  measured 36–55 G (`2026-10-18-agent-worktree-disk-usage-report.
  md`); three rounds dispatching freely could hold 4–6 worktrees at
  once. The floor check (`scripts/worktree-doctor.sh`) is a hard
  stop per AGENTS §8 — a parallel plan is only as good as its
  discipline here. Recommended cap: **two concurrent implementation
  chunks per host** (e.g. one kiosk + one multi-board/DD), lifted
  only while free disk is comfortably above the floor.
- **Session capacity**: full three-stage dispatch of everything
  outstanding is ~22 multi-board-remaining + 33 DD + ~42 kiosk
  agent sessions — the serialised spine each plan states is the
  pacing tool; "parallel" means *interleaved rounds*, not more
  concurrent agents than the review cadence can absorb.
- **main is rebase-merged**: interleaved rounds see each other's
  merges land under them — every stage-3 rebase (the 2026-10-18
  protocol amendment) already handles this; the adjacency zones in
  §2.3 are exactly where it will fire.

## 4. Recommended dispatch pattern

1. Dispatch kiosk phase A now, alongside the multi-board round:
   `K1 ∥ K2 → K3 → K4` on the appliance lane; `K5 ∥ K6 ∥ K7` on
   the instrumentation lane (startable even before the appliance);
   `K10` (dashboard design, docs-only) immediately — it is the
   long-lead item for phase C; `K9` once K3 + multi-board chunk 5
   are in.
2. Hold `K8` for its window (before MB-4, or the MB→DD gap) —
   otherwise it is the single most likely mid-flight conflict in
   the whole programme.
3. When multi-board closes, start the DD round per its MD-1 and run
   the kiosk remainder alongside it — with `K13` strictly sequenced
   after DD-10 and the three docs-exit chunks serialised.
4. The spike (phase B) is user time and can overlap any code lane
   once its prep (K5–K7, K9) is in.

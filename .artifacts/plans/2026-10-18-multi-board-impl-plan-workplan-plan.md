# [Plan] Multi-board Implementation-Plan Round — Stage-1 Work Plan (dispatch chunk 2)

- **Date**: 2026-10-18
- **Kind**: stage-1 work plan for dispatch chunk 2
  (`docs/multi-board-impl-plan`) of the multi-board design-round
  dispatch. It fixes **how** the stage-2 research agent finalises
  acceptance and writes the wave implementation plan: the chunk
  decomposition proposed for that plan (§4, mapping every chunk to the
  accepted design's normative sections and to the milestone issues it
  closes/annotates), the two edit lists for the existing artefacts
  (§5/§6), the implementation plan's outline and design-facts content
  (§7), and the risk list. Every method-level open question met while
  planning this round is resolved below as a recorded decision
  (MD-1…MD-10). **This plan does not write any deliverable** — stage 2
  executes it.
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-18-multi-board-design-round-dispatch-plan.md`
  §1 (shared execution model) + §3 (chunk 2 covers/acceptance/prompts
  2.1–2.3); framing plan
  `.artifacts/plans/2026-10-18-multi-board-wave-design-plan.md`
  (§4/§5 normative, §6 advisory, §7 sequencing); accepted design
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  (merged `main` @ `b24446e`); kiosk exemplar
  `.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md` (the style to
  imitate).
- **Status**: DONE (stage 1). Stage 2 executes this plan in
  `../taskboard-WT-docs-multi-board-impl-plan` on branch
  `docs/multi-board-impl-plan`.
- **Verified baseline (plan day)**: `main` @ `b24446e` (=
  `origin/main` tip; fast-forwarded from `361ab0b` before the worktree
  was cut — the two-design-commit delta `62dd839..b24446e` is
  docs-only: the chunk-1 work plan + the design doc).
  **Precondition check (recorded, dispatch §3's human gate)**: (a)
  chunk 1's PR is merged — the design doc exists on `origin/main`
  (`git ls-tree origin/main .artifacts/plans/` →
  `2026-10-18-multi-board-detailed-design-plan.md`, delivered by
  `b24446e`; the `docs/multi-board-detailed-design` remote branch is
  deleted = merged-and-cleaned); (b) the user's decision session is
  recorded — the dispatcher appended the rulings verbatim to this
  prompt (§2 below), exactly §3's recording mechanism. The doc's own
  Status header still reads PROPOSED **by design**: flipping it to
  ACCEPTED is chunk 2 deliverable (a) (dispatch §3 Covers). Stage 2
  re-checks both facts at its start.

---

## 1. Mission (what stage 2 produces)

Three artefact edits — the chunk's entire allowed diff, zero production
code, zero tracker edits, nothing else in the tree:

1. **(a) The ACCEPTED decision record** written into the design doc
   (edit list §5): Status header PROPOSED → ACCEPTED; §5's table rows
   flip to ACCEPTED with Q2/D-W9's ruling recorded; the user's ruling
   quoted verbatim with its date. No recommendation text mutates; no
   §3.x prose rewrites (trap (a)/(b) of prompt 2.3).
2. **(b) The framing plan update** (edit list §6): §5's nine questions
   marked resolved with pointers into the design doc; §4 appends (never
   rewrites) the D-W9 ruling; §7 sequencing progressed; header Status
   line refreshed (including its off-by-one section refs, see §6).
3. **(c) The wave implementation plan**
   `.artifacts/plans/<today>-multi-board-wave-implementation-plan.md`
   in the kiosk-exemplar style per dispatch §3(c): "design facts every
   chunk relies on" distilled from the accepted design, one section per
   implementation chunk with branch name (AGENTS §8), files/behaviours/
   explicit non-goals, and size; the quality-gates section; the
   sequencing section (incl. wave exit criteria and their verifying
   chunks); and a completion-bookkeeping section naming which milestone
   issues (#125/#170/#179/#187/#298) each chunk closes or annotates.
   Outline in §7; proposed decomposition in §4.

The implementation round itself is **not** dispatched here: its own
dispatch plan is written later off the accepted implementation plan
(dispatch §4 "what opens next"), after remediation-wave-1 closes and
this chunk's PR merges.

## 2. The user's rulings (verbatim, dispatcher-appended; the source of §5's record)

> User ruling (decision session, 2026-10-18, on chunk 1's stage-2
> report): "I accept all your recommendations."   i.e. Q1 explicit
> board: BoardId on all board-scoped commands with caller-owned
> selection; Q2/D-W9 per-board status keying, attribution derived from
> bindings, global last_updated unchanged; Q3 app-layer named routing,
> domain ConnectorId from config names, SetBoard retired; Q4 split
> ports (domain vocabulary + app ConnectorModule); Q5 typed
> multi-binding variants, UnbindBoard, ReconcileByTitle, parameterised
> destruct_on; Q6 [[sync.connectors]] with legacy-scalar synthesis and
> loud mix rejection; Q7 per-connector events + app-layer all-down
> authority; Q8 ViewState.board + board.next/previous/select; Q9
> migration 0006 as sketched, zero breaking steps. No deviations from
> any recommendation.

**Fidelity check (done at plan time, repeated by stage 2)**: each
ruling above matches the design doc §5's Recommendation column verbatim
in substance — all nine accepted, none overruled, none modified. The
ACCEPTED record therefore flips every row; nothing is silently adopted
beyond the rulings (prompt 2.3 trap (a)).

## 3. Method decisions (recorded; binding for stage 2)

- **MD-1 Ruling record placement.** The verbatim ruling block lives in
  the design doc §5 (introduced by a dated acceptance note above the
  table) and as an appended dated row in the framing plan §4 for D-W9.
  The design doc's §3.2 MD-8 paragraph ("open — user rules") stays
  untouched — it describes the round's method and is discharged by the
  §5 note, not rewritten; body prose is historical reasoning.
- **MD-2 Anchor policy for the implementation plan.** Per-chunk
  "files" lists cite **file paths + symbol names** (the stable keys,
  per the round's finding-ID discipline); today's line numbers appear
  only as plan-day references inherited from design §2's re-verified
  table, and each chunk's implementing agent re-locates them at its
  own start. This keeps the implementation plan from going stale the
  way line-pinned prose does.
- **MD-3 Chunk decomposition policy.** Keep the framing §6 sketch's
  8-chunk spine and numbering — the accepted design's own
  cross-references endorse it (§3.9 "wave chunk 1"; §3.4 "chunk 4 of
  the sketch implements the trait"; §3.1/§3.8 "chunk 6"/"chunk 7 of
  the implementation sketch") — and re-slice only where green-`main`
  sequencing forces it, recording each re-slice inside the
  implementation plan's own notes. The four load-bearing resolutions:
  (i) **0006 lands whole in chunk 1** (status rebuild *and* the
  `boards.connector` tag column — design §3.9's one-file sketch);
  (ii) the binding tag arrives in **three coordinated steps**, each
  green: storage column + default-writing codec + `ConnectorId`
  newtype + `spawn_storage_actor(default_connector)` in chunk 1
  (design §3.9's resolution home), the domain binding-type widening +
  actor-derived board sets in chunk 4 (the sketch's own placement of
  "Board.remote connector tag"), plural name minting in chunk 5;
  (iii) **`SetBoard` retires in chunk 4** — the ceremonies chunk (3)
  stops *sending* it (broadcast-nudge redesign per design §3.5) and
  chunk 4 removes variant + actor handler + `App::sync_commands()`
  "in the same motion that tags the bindings" (design §3.3); the
  one-chunk unused pub variant is compile-clean and preferable to
  dragging actor work into 3; (iv) design §4's "landing chunk" column
  is advisory by its own header ("framing §6, advisory") — where it
  conflicts with sequencing (SyncCommand shown at "2"), the design's
  *decisions* are honored and only the landing number follows the
  green-`main` order recorded here.
- **MD-4 Self-sufficiency bar.** Dispatch §3's acceptance: "a reader
  who never saw the design doc can plan an implementation chunk from
  the implementation plan alone." The design-facts section (§7 below)
  must therefore carry the operative shapes — the four-vocabulary
  table, the `ConnectorId` mint chain, the binding-tag decode rule,
  the ceremony rules (wipe rule, honest stranding, reconcile
  semantics), the config resolution + typed-rejection list, the Q7
  authority mechanics, the UI integration points, the 0006 sketch
  pointer + snapshot rule — each with its design-§ citation so
  per-chunk sections can cite "design fact N (← design §3.x)" and the
  normative-origin requirement is met without the reader opening the
  design doc.
- **MD-5 Non-goals are mandatory per chunk.** Every chunk section
  carries explicit non-goals naming what looks adjacent but is not in
  that chunk (prompt 2.3 trap (c)); seeds are in §4's per-chunk notes.
- **MD-6 Scope discipline.** This chunk is docs-only: no code, no
  tracker edits, no dispatch prompts for the implementation round (a
  pointer to the template suffices), no backlog promotion — the
  design's §7 backlog candidates stay listed for the user, and the
  implementation plan may restate them as wave non-goals only.
- **MD-7 Gates.** The full AGENTS §10 suite runs on the docs-only
  diff (chunk-1 precedent: the hooks run regardless; nothing here
  should be able to fail them, and a failure is a finding).
- **MD-8 Kiosk-exemplar conformance.** The implementation plan's
  section skeleton mirrors `2026-10-16-kiosk-mode-wave1-plan.md`
  (Scope / Design facts / Chunks / Quality gates / Sequencing) plus
  the dispatch-mandated completion-bookkeeping section and a wave
  exit-criteria mapping; header carries baked-in decisions with their
  design-§ origins and a READY-gated status line (implementation
  dispatch is a separate later round — the plan must not read as
  self-dispatching).
- **MD-9 Dating and naming.** The implementation plan's filename uses
  stage-2's execution date; the design doc and framing plan are edited
  in place (no renames — their paths are cited across the artefact
  corpus). The ruling is dated 2026-10-18 wherever recorded.
- **MD-10 Interim-state honesty.** The implementation plan's
  sequencing section records the wave's one deliberate interim: after
  chunk 2 lifts `CreateBoard`'s refusal, multiple boards and bindings
  are engine-possible while the actor still syncs its first bound
  board only, until chunk 4's derived board set; the interim is
  *visible* (per-board `pending_ops` climbs; chunk 1's map), never
  silent, and multi-board is not user-advertised until chunk 5+
  (config) lands. This is sequencing fact, not an accepted-design
  deviation, and is stated as such.

## 4. Proposed implementation-plan chunk decomposition

The table stage 2 transcribes (adapting wording into the kiosk style);
branch names per AGENTS §8 (`<category>/<scope>-<description>` —
stage 2 sanity-checks against recent merged practice); sizes on the
house scale; "design" = the accepted design doc; issues = milestone
bookkeeping (closes at that chunk's merge, annotate = progress note).

| # | Chunk | Branch | Design origin (normative) | Issues | Size |
|---|---|---|---|---|---|
| 1 | SyncStatus per-board map + migration 0006 (whole: status rebuild + `boards.connector` column + `ConnectorId` newtype + `default_connector` decode home + aggregate fold for existing surfaces + snapshot pass 1 + migration round-trip test) | `feat/state-sync-status-map` | §3.2 (opt a), §3.9 (0006, decode home, rollback), §3.3 (`ConnectorId` mint) | **closes #298**; annotates #125 | M |
| 2 | Domain vocabulary motion: `board: BoardId` on all nine arms, `CreateBoard` refusal lifted, error set (−`NoBoard` −`BoardExists`, +`UnknownBoard` +`BoardDeleted`, `BoardAlreadyBound` widened), app mirrors, CLI `--board` flag + single-live default with typed ambiguity refusal, mechanical interim fills at REPL/UI call sites, snapshot pass 2 (command corpus) | `feat/domain-board-scoped-commands` | §3.1 (all four vocabularies; caller-side burden), §3.9 (snapshot rule) | annotates #125 | L |
| 3 | Engine ceremonies at N: `UnbindBoard` (keep content, bind-wipe rule, honest stranding, receipt), `ReconcileByTitle` (trim+casefold, tasks-in-matched-stacks, op cancellation, idempotent), `RemoteBoardInUse` variant pair, `destruct_on`/`read_scope` parameterised, `select_on` folded into engine refusals + broadcast nudge + `wait_for_adoption(board)`, `boards unbind`/`boards reconcile` ceremony verbs; `SetBoard` no longer *sent* | `feat/state-board-ceremonies` | §3.5 (all five sub-questions), §3.1 sub-decision (ceremony error list) | **closes #170, #179**; annotates #125 | M |
| 4 | Connector identity end-to-end on the wire: `ConnectorModule` trait + `NextcloudModule` adapter + legacy-scalar **synthesis-of-one** + bootstrap registry loop (N=1) + `spawn_sync_actor(id)` + derived board set from tagged bindings + per-board cache keys (no clear-on-retarget) + push ownership filter + resume parameterisation + `SetBoard` retired (variant/handler/`sync_commands()`) + domain binding-type widening + **ADR 0010 amendment lands here** + the v0.1.x additive-upgrade boot test | `feat/sync-connector-modules` | §3.4 (trait, amendment), §3.3 (derived sets, `SetBoard` retirement, `#157` guardrail), §3.6 (synthesis rule subset), §3.9 (upgrade test) | annotates #187, #125 | M |
| 5 | App plural world: `[[sync.connectors]]` decode + typed mix/shape rejections + entry `deny_unknown_fields` + name validation, shape-aware ceremony writer, `ensure_vacant` → `ConnectorNameInUse`, N-actor spawn, `connector_commands(id)` lookup, attach-verb name parameter, Q7 whole motion (per-connector `ConnectorLost/Restored` events from actors, engine per-board `go_offline` scoping, app-layer all-down authority emitting the global events); tier-2 two-connector dockerized test | `feat/app-connector-registry` | §3.6 (full), §3.7 (full), §3.3 consequences (registry/naming) | **closes #187**; annotates #125 | L |
| 6 | CLI/REPL verb growth: per-connector `status` rendering (JSON answer's additive `connectors`/`boards` arrays + human output), `--connector` flags, REPL `ReplSession.current_board` + `boards use` + completions, `require_connector` pluralised | `feat/app-cli-connector-verbs` | §3.2 wire consequence, §3.3 CLI consequences, §3.6 `require_connector` | annotates #125 | M |
| 7 | UI board dimension: `ViewState.board` (seed rule, replaces chunk-2's interim fills) + `select_board` + `board.next/previous/select` actions + keymap presets + `board-bar.slint` (compact hides strip) + `IdTables` from `ViewState.board` + selected-board badge + scripted keyboard-journey leg | `feat/ui-board-switcher` | §3.8 (all integration points), §3.2 badge interplay | annotates #125 | M |
| 8 | Docs: architecture.org (connector registry, per-board status, authority), README (config shape, multi-board usage), CHANGELOG; wave exit-criteria verification record | `docs/multi-board-wave` | §4 coherence map, §3.9 compat summary | **closes #125** (wave exit) | S |

**Dependency spine**: `1 → 2 → 3 → 4 → 5 → {6 ∥ 7} → 8`, mirroring the
sketch: chunk 1 alone first (standing decision P1-1 — nothing stacks
on the singleton it removes); 7's true dependency is 2 (the map from 1
and board-scoped commands from 2), so it *may* run parallel to 3–5 at
dispatch time — the implementation plan presents the serial spine as
default and notes the parallel option (stacked-PR rebase dance applies
either way, AGENTS §8).

**Wave exit criteria → verifying chunks** (framing §6 draft, refined by
design §3.9's test specs): two connectors live simultaneously
(tier-2, chunk 5); multiple boards syncing across them (5, exercised
by 6); one board moved between connectors (6); v0.1.x config + DB
upgrade additively keeps working (1's round-trip test + 4's boot
test); board switching by scripted keyboard journey alone (7);
`cargo deny` green with the promoted port (every chunk — trivially, no
new deps, design §3.4).

**Per-chunk non-goal seeds** (MD-5; stage 2 words them per chunk): 1 —
no plural connectors, no domain binding-type widening, no command
changes; 2 — no ceremony redesign, no real selection homes (interim
fills only), `SetBoard` kept; 3 — no actor/routing changes, no config
shape; 4 — no `[[sync.connectors]]` plural, no authority, no CLI
verbs; 5 — no UI, no new connector *kind* (nextcloud only); 6 — no UI;
7 — no engine commands from switching (dumb-client rule), no
config-pinned kiosk (backlog); 8 — no ADR 0010 text (landed in 4), no
new drift amendments (F10-03/04, F15-18 stay backlog).

## 5. Design-doc edit list (deliverable (a))

1. Header **Status** bullet: "PROPOSED — awaiting the user's decision
   session…" → "ACCEPTED — the user's decision session (2026-10-18)
   accepted all nine recommendations verbatim, no deviations; record
   in §5." The **Kind** bullet's "(draft, status PROPOSED)" → "(status
   ACCEPTED)". Nothing else in the header block changes.
2. §5 heading: "Decision record (PROPOSED — the user's session rules)"
   → "Decision record (ACCEPTED 2026-10-18)".
3. §5 gains a dated acceptance note above the table: the ruling quote
   ("I accept all your recommendations.") + session date + "no
   deviations from any recommendation" + the pointer that D-W9 is
   thereby ruled option (a) (framing §4 records the same, appended).
4. §5 table: every row's Status → **ACCEPTED (2026-10-18)**; Q2's
   "**OPEN — user rules**" → "**RULED (a) — ACCEPTED (2026-10-18)**".
   Recommendation column text untouched.
5. Everything else — §1–§4, §6–§8 prose, the §2 anchor table — stays
   byte-identical (MD-1).

## 6. Framing-plan edit list (deliverable (b))

Trap (d) governs: §4 is a historical record — **append, never rewrite
existing rows**; §5's question *texts* are not edited (pointers are
added); §9's 2026-10-18 bookkeeping stays as written.

1. Header **Status** bullet: append the round outcome — design
   questions §5 resolved 2026-10-18, decision record ACCEPTED in the
   design doc §5, implementation plan written (path). While editing
   this line, fix its pre-existing off-by-one refs (it cites "(§5)"
   for the wave-level decisions and "(§6)" for the questions; the
   body's actual sections are §4 and §5) — a same-line correction,
   visible in the diff, recorded in the PR body.
2. §4: append one dated row after D-W9 — "D-W9 (ruled 2026-10-18):
   option (a) per-board keying — the user's session accepted all
   design-round recommendations; see the design doc §5." Existing
   D-W1–D-W9 rows untouched.
3. §5: under each of the nine questions, add one pointer line —
   "**Resolved (2026-10-18)** → design §3.N (decision record: ACCEPTED)".
4. §6: add a lead-in note — advisory sketch **superseded** by the wave
   implementation plan (path), which re-chunks per the accepted
   design; the sketch text itself remains for the historical record.
5. §7: mark item 2 (Stage-0 design round) done with dates; progress
   item 3 to name the now-written implementation plan as the current
   artefact and the implementation dispatch plan as the next (gated on
   this chunk's merge + remediation-wave-1 closing, per dispatch §4).

## 7. Implementation-plan outline (deliverable (c); kiosk-exemplar mapping)

- **Header block**: Date; Kind (implementation plan for the accepted
  multi-board design — origin: design doc path, decision record §5);
  Status (READY, gated: implementation dispatch is its own dispatch
  round after this plan's PR merges and remediation-wave-1 closes);
  "Decisions baked in" list = the nine ACCEPTED rulings with design-§
  origins (kiosk exemplar's pattern).
- **§1 Scope**: in/out distilled from framing §2 + design §7 backlog
  candidates restated as out-of-scope (no promotion — MD-6).
- **§2 Design facts every chunk relies on** (MD-4 checklist — each
  fact cites its design §): the four-vocabulary shape table (§3.1/§4:
  nine command arms, error set diff, unchanged outbox ops + push
  filter, per-board status map + `SyncCommand` → `{SyncNow, Detach}`);
  selection homes (CLI flag default / REPL session / `ViewState.board`
  — none in the engine); the `ConnectorId` mint chain (config `name` →
  domain newtype → binding tag → `SyncActor.id` → module id → CLI
  `--connector`, design §4); the binding-tag decode rule (NULL =
  default connector at decode; non-NULL written on every upsert,
  §3.9); ceremony rules (bind-wipe on cross-remote rebind, honest op
  stranding, reconcile = trim+casefold post-adoption idempotent,
  `destruct_on` typed-refusal shape, §3.5); config resolution +
  the typed-rejection list + shape-aware writer (§3.6, incl. the
  figment probe facts that motivated it); Q7 mechanics (episode-edge
  per-connector events, counted all-down authority, per-board
  `goOffline` scoping, §3.7); UI integration points (`ViewState.board`
  seed rule, three ActionIds + contexts, keymap presence, board-bar
  compact rule, IdTables derivation, §3.8); 0006 sketch pointer +
  snapshot rule (a widening chunk owns its one deliberate insta pass,
  §3.9); the topology invariant as the standing acceptance test (§1).
- **§3 Chunks**: one subsection per §4 row — branch, files (paths +
  symbols, MD-2), behaviours, explicit non-goals (MD-5), size; the
  interim-state note (MD-10) where chunk 2's section needs it.
- **§4 Quality gates & conventions**: full AGENTS §10 suite per chunk;
  TDD per testing_strategy with the house rules restated kiosk-style
  (typed outcomes, no text asserts, no sleeps); snapshot-regeneration
  rule; tier notes for the dockerized tier-2 tests (chunks 4–5).
- **§5 Sequencing & exit criteria**: the spine + parallel option +
  rebase-dance warning; the exit-criteria → verifying-chunk map (§4);
  the MD-10 interim state.
- **§6 Completion bookkeeping**: the per-chunk issue table (#298→1,
  #170/#179→3, #187→5, #125→8 with annotations from 1/2/3/6/7); ADR
  0010 amendment → chunk 4; CHANGELOG/architecture.org/README → 8;
  closing happens in the implementation round (issues are closed as
  chunks merge; this chunk itself edits no tracker — AGENTS §7).

## 8. Risks (planning-round)

- **Chunk-1 scope creep** (the migration carries the tag column +
  `ConnectorId` + codec, not just the map): contained by MD-3(ii)'s
  three-step binding-tag plan — chunk 1 stays storage+codec+map, the
  domain type widening waits for 4. If stage 2 finds the codec cannot
  write the column without the domain field, it records the deviation
  and pulls the minimal field in — the design's decode home (§3.9) is
  the contract, the type mechanics are chunk-1's to choose.
- **Review friction on re-slicing vs design §4's landing column**
  (`SetBoard` at "2", trait at sketch-4): MD-3(iv) records the
  advisory-column reading with quotes; the implementation plan repeats
  the reasoning inline where the re-slice occurs. Residual risk: the
  stage-3 reviewer reads the column as normative — the fix is the
  recorded reasoning, not a re-slice back to a non-green ordering.
- **Chunk 3 is the densest M** (two new engine commands + ceremony
  verbs + reconcile semantics): if stage 2's outline strains, the
  sanctioned relief is splitting `ReconcileByTitle` (+`boards
  reconcile`) into its own chunk after 3 — issue mapping unchanged
  (#170 rides the split chunk). Decide at outline time, not mid-write.
- **Interim multi-board gap** (MD-10): a user who force-explores
  chunks 2–3's world sees unsynced second boards until 4; honest and
  visible, but the implementation plan must say it loudly so the
  implementation round doesn't "fix" it out of order.
- **Artefact size**: deliverable (c) at eight chunks × self-sufficiency
  bar is an L document; the design-facts section is what keeps it from
  ballooning — facts once, chunks reference (MD-4), never re-derived
  per chunk.
- **Framing-header off-by-one fix** could read as scope creep in the
  diff; mitigated by §6 item 1's same-line-only rule + PR-body note.

## 9. Stage-2 self-check before handoff (stage 3 reviews all of this)

1. Precondition re-verified (design doc on `main`, rulings in prompt).
2. Diff = exactly the three artefacts (§1); no code, no tracker edits.
3. §5/§6 edit lists executed exactly (trap (a)/(d) fidelity: verbatim
   ruling, appended-not-rewritten records).
4. Implementation plan: every chunk names normative origin sections,
   valid branch names, explicit non-goals (trap (c)); no accepted
   decision re-opened, none new smuggled (trap (b)); self-sufficiency
   spot-check — plan an unfamiliar chunk from the plan alone.
5. Full AGENTS §10 suite green on the docs-only diff (MD-7).

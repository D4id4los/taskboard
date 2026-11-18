# Round 8 → Round 9 Analysis Handoff

- **Date**: 2026-11-18
- **Kind**: dispatcher handoff — everything the next review round's
  analysis agent (fresh operator pass → findings) needs to start
  without re-deriving round-8 history.
- **Status**: round 8 is CLOSED. All eight chunks merged; the census
  Status entry is PR #943.

## 1. What round 8 shipped (merged, in order)

| PR | Chunk | Closes |
|---|---|---|
| #862 | keymap parity (emacs motions, M-</M->, task-action shortcuts) | — (census only) |
| #872 | focus restoration & modal containment | #668 #669 #670 #724 |
| #918 | board interaction (reorder, compact movement, multi-select) | #823 #834 |
| #922 | task detail editor (date-only day-boundary law, editor repairs) | #838 #759 #706 #840 |
| #925 | files sync status & Settings-Files family (incl. the Deck→Deck rebind) | #741 #743 #745 #748 #750 #746 #756; #749 evidence-close |
| #933 | task card controls & menu grammar (quick-actions bar, N-11 fix) | #882 #811 (C-1 supersession) |
| #939 | tag colour (Color::parse, roster-only mount, card chips) | #839 (C-3 supersession) |
| #940 | cross-surface friction (conversion arms, OneShotSet, disarm, alias, Replace pre-pass) | #753 #832 #836 #772 #763 #712 |
| #936 | operator-reported fix: the punctuation partner matches only the held shift | — (partial closure of #855) |
| #942 | files ops-dir incident fix (see §4) | #941 (closes at merge) |
| #943 | census Status round-8 entry | — |

Post-merge human checks **confirmed**: M-< / M-> on Dvorak
International (recorded on #855). **Outstanding**: the N-19 re-test on
the real Nextcloud — blocked until #942 merges (see §4).

## 2. Where the round's authorities live

- Dispatch plan (the chunks' normative IDs, §10 ledger):
  `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
- Origin journey plans (all ACCEPTED 2026-11-16, same directory):
  `2026-11-16-{keymap-parity,focus-restoration,task-detail-editor,
  board-interaction,task-card-controls,files-sync-status,tag-color,
  cross-surface-friction}-ux-plan.md`
- Chunk plans (the landed design decisions, D-ids bind):
  `.artifacts/plans/2026-11-17-ui-r8-*-plan.md`
- Census: `docs/ux-surface-census.org` — every cell now carries
  round-8 evidence (US/W ids + journey test names) and the dated
  supersessions (C-1, C-3, R-5 overturn, R-F1/R-F2 rulings in the
  focus slice).

## 3. Open threads the analysis should know about

1. **#941 / PR #942** (the real-Nextcloud files incident — the round's
   one live incident): `attach_board` never MKCOL'd
   `devices/<self>/ops`, so the drain PUT 404'd forever and the
   operator's outbox grew to 231 ops. Root cause + fix plan:
   `.artifacts/reports/2026-11-18-n19-residue-real-nextcloud-incident-report.md`.
   PR #942 fixes the ladder, enforces fake-transport parent
   existence, adds the ladder-completeness test, and lands the
   per-board lane-error honesty fix (chip lifts to CycleFailed).
   **After it merges**: the operator restarts the app and re-tests —
   the 231 ops drain in one pass (drain materializes from published
   state; no triage).
2. **#855** (base-keysym seam): *partially* closed by #936's
   directional partner map; a layout composing a third glyph under
   Alt+Shift stays unreachable at the text-only seam. The operator
   confirmed the Dvorak case works.
3. **#927** (stale pinned object-menu rows until dismissal): chunk 5's
   MenuKind pin is the hook its reconcile-disarm fix rides; open.
4. **#934/#935** (habit-section keyboard traversal; route()'s uniform
   typed-refusal pass), **#937** (a pre-existing duplicated six-hex
   validator in sync-nextcloud/src/color.rs — landed on main in
   90c10442, predates the round), **#938** (a Labels row on the card
   popup — W-2 deliberately scoped it to BOARD_CONTEXT).
5. **Standing deferrals** (open without milestones, per the round
   plan's C-6): #807 #809 #812 #813 #819 #820. Machinery lane:
   #762/#715/#716 (chunk 8 re-checked #762 — orthogonal, stays
   homed); #768 (coverage gate step-token sharpness — related to
   #920). #920 itself: the gate's step-token matching resolves
   spuriously across plans sharing `us<n>_s<k>` prefixes.
6. **#921**: a millisecond-wide wall-clock race in one chunk-3
   journey's oracle (seed-seam or UiSettings::clock routing proposed).
7. **The keymap p3 language limitation** (#873, strptime leniency) and
   #874 (unify the fenced `--to-deck` arm with the landed
   `--to-connection` twin).

## 4. Process learnings this round (already partially landed on main)

- **Memory guards are live on main**: the pre-commit memory-gate
  (MemAvailable floor + TMPDIR→/var/tmp) and the #906/#923
  follow-ups. Two host OOMs and a /tmp-tmpfs fill drove these; see
  the incident comments on #879 and
  `.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`.
  Dispatch consequence: never launch cargo-mutants from an agent
  without an explicit job/tmp cap; sweep for orphaned build processes
  when recovering a dead session.
- **Tests-on-the-slint-thread rule**: every window()/rig-mounting test
  wraps in `on_slint_thread` (two CI-only failures on chunk 5 taught
  this; the pattern is `flow_table_tests.rs`' first-frame-hints test).
  The analysis round's suggested journeys must follow it.
- **Coverage-gate grammar**: wiring cells want a single greppable
  ActionId token and a bare `—` no-commands cell (found three times:
  #929, chunk-5's grammar fix, chunk-8's D-10). The gate script should
  own the convention (#929's point).
- **Serialized dispatch**: one agent at a time is the safe default
  until the lock-floor guards' coverage is complete; two-agent
  parallelism (file-disjoint, post-guards) worked for C7/C8.

## 5. What the analysis round should NOT re-litigate

- The round-8 rulings and supersessions (C-1, C-3, R-5 overturn,
  R-F1, R-F2, D-4's +Nd duration semantics, D-2's empty-domain inert,
  the fallback-based notification arming a2e45241) — all recorded in
  the census with dates and authorities; a fresh round overturns them
  only through a new operator pass at UX-plan level.
- The N-11 fix shape (MenuKind discriminator), the OneShotSet design,
  and the day-boundary law — accepted, merged, journey-pinned.

## 6. Suggested next-round inputs (for the operator pass)

The fresh operator pass that seeds round 9 should walk, at minimum,
the surfaces round 8 reshaped: the board (multi-select + gap-drop +
compact leg), the task card quick bar and its menu grammar, the
settings Files card (chip + phases + roster rows + rebind), the label
chips, and the notification-surface keyboard grammar. The known raw
spots worth a deliberate look: #927's stale object-menu rows, #934's
habit traversal, the #935 refusal uniformity, and whatever the
files-sync chip shows post-#942 under real load.

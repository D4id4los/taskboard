# Post-Round-9 Parallel Recommendation (2026-11-18)

Question: which open issues are **not addressed by GUI review round 9**
and make sense to implement now, while r9 (ACCEPTED; 15 chunks: W1–W7,
H1–H4, K1–K4, M1–M2, D2) is starting implementation?

Method: round-9's §4 chunk ledger + §5 deferral list are the exclusion
set, minus the hygiene-r2 close-out's still-open-by-design set
(#327, #650, #697, the filed follow-ups, #930/#904/#907/#890), minus
the standing deferrals (perf family, feature-scale, registers,
C-6 triage deferrals, no-recurrence flakies).

## Collision framing

r9 owns: the widget/overlay framework chain (W1→W2→D2, `ui/widgets/`,
all overlay mounts), the habit chain (H1–H4, habits surfaces +
additive domain fields), the keyboard chain (K1–K4, presets/keymap/
walk vocabulary), the menus chain (M1–M2, menu.slint/action-menu.slint,
Dropdown de-siting), the day-review redesign, task-details polish, and
the census supersession writes. Anything touching those files collides.
Everything below is chosen to live in `taskboard-state`, `taskboard-sync-nextcloud`,
`taskboard-app` test/CLI seams, CI scripts, and docs — the lanes r9
does not enter.

## Tier 1 — implement now (small, high value, r9-disjoint)

**N-A. Coverage-gate correctness bundle** — #920 + #768 + #760
(testing/dx, scripts). r9 will run `scripts/ux-coverage.sh --gate`
across five journey plans and ~15 chunks; #920 (us<n>_s<k> prefix
collisions resolve spuriously across shared-prefix plans — a real
false-green risk for exactly this round's plan family), #768
(bound-test-name mapping for retro-fitted plans), and #760 (wiring-row
↔ seam-inventory cross-check) are the gate's three known correctness
debts. Landing them first protects r9's own verification machinery.
One chunk, scripts + docs, journey-exempt.

**N-B. Restore-machinery follow-ups** — #913 + #900 + #901
(`taskboard-state`, engine-side). #913 is a live bug in the
just-landed RestoreStack (undoing a RestoreTask/RestoreStack entry
refuses NothingToUndo — the CreatedTask synthesis arm), #900 adds
reminder-rule pre-images, #901 extends restore-class machinery to
DeleteLabel/DeleteTimeEntry/DeleteHabit. Fixing the fresh bug now and
completing the restore family while C8/C9's context is recent is the
cheap moment. r9's only state-crate work is H1's additive habit fields
— disjoint files, but stage-1 confirms symbol-level disjointness.

**N-C. Docs/chore compressed chunk** — #904 (AGENTS.md memory-gate
wording, actionable since #887 landed) + #892 (#682 property comment
cross-reference) + #859 (`UiHandle::refresh` doc invariant) + #851
(stray `#…#` editor backups under `.artifacts/plans/` — cleanup +
a hygiene rule) + two bookkeeping actions: verify-and-close #924
(systemd-oomd is live per the close-out; the issue is still open) and
#778 (file the upstream Slint window-max report — the evidence report
already exists in-repo). Compressed, dispatcher-owned.

**N-D. Test/IPC seam hygiene** — two small chunks:
- **N-D1 test-fixture seams**: #884 (journey fixtures derive the day
  vocabulary in hardcoded UTC while the adapter's calendar differs —
  land BEFORE r9's journey-heavy chunks stack fixture extensions and
  make the rebase cost grow), #897 (extract_raw validity-only
  assertions ambient-exposed), #921 (us2_s2 oracle wall-clock race),
  #914 (C10 daemon-lane fixture dedupe, mem::forget tempdir leak),
  #905 (the env-lock test's own ambient TASKBOARD_CONFIG observation —
  small diagnosis on C6's landed seam).
- **N-D2 stop ordering + mutation survivors**: #886 (graceful-stop
  ordering — `daemon:false` client-visible before the instance lock
  releases; production lifecycle, app crate), #917 (IpcServerHandle
  join drain-budget seam), #902 (try_connect latency-seam test).

**N-E. Sync small refactors** — #937 (DeckColor validate re-implements
the domain's six-hex grammar — the O-6 centralization principle applied
sync-side; delete the re-implementation) + #885 (cadence-style
revalidation band replacing the single-constant SKEW_BAND_SECS shape —
rides #868's landed seam). One chunk, `taskboard-sync-nextcloud`.

**N-F. #891 — the chrono ruling (p2, architecture/security).**
RUSTSEC-2026-0543 marks chrono unmaintained; the workspace pins it
everywhere (AGENTS §4 approves it). This needs a decision-grade
evaluation: the advisory's real exposure, the alternative time stacks,
the migration cost surface (sync HTTP-date handling, schedules, the
#133 skew constants), and exactly one recommendation (stay-with-evidence
vs staged migration). Research chunk, ends at an operator gate — same
shape as the #708 design round. Security-adjacent p2s don't wait well.

## Tier 2 — next-in-line (bigger, or judgment calls)

- **#873 + #874** — r9 §5 explicitly homed them, but both are small,
  CLI/domain-side, and r9-free: #873 (the date-entry grammar's
  strptime accepts non-padded day fields — an undocumented leniency,
  exactly the A1 lens) and #874 (unify the fenced `boards move
  --to-deck` arm with the rebind ceremony's `--to-connection` twin).
  Natural first pull-ins if Tier 1 drains early.
- **#751** (p2: inject the move ceremony's device identity path —
  unblocks hermetic UI-arm ceremony tests; medium, app/state seam).
- **#718 + #755** (transport DELETE GC execution + the unbind subtree
  GC it enables) — a coherent sync-architecture pair, medium-large;
  worth their own mini-round slot.
- **#673** (files-sync cadence follows live settings — r8-files
  follow-up, sync crate, medium).
- **#733** (notification Unmute toggle — small UI feature, r9-free
  surfaces, but user-visible ⇒ journey plan first, C10/C11 pattern).
- **#909 + #910** (config live-reload watcher + multi-key batch set —
  dx features on the fresh #915 surface; medium).
- **#890** (shared build cache evaluation — sccache/shared
  CARGO_TARGET_DIR; strategically valuable given the OOM incidents'
  per-worktree cold builds; evaluation-first, rollout is
  operator-coordinated).
- **#846** (golden_visuals fails *alone*, passes in suite — inverse
  flaky, deterministically reproducible by running solo; cheap
  diagnosis, but the golden files sit near r9's widget churn —
  diagnose now, fix before W-chunks move the surfaces).
- **#893** (TaskDueToday exclusive-end bug — a just-recommitted task
  immediately leaves Today; real user-facing bug, but the Today
  surface is r9 K2/D2-adjacent: include only with a stage-1
  disjointness verdict, else first post-r9).

## Fold into r9 instead of a parallel round

- **#775** (census anchor drift) — r9's §6 already writes dated
  supersessions into the census; enforce the symbol-anchor + baseline
  convention there and close #775 with that evidence.
- **#881** (FakeEngine missing DeleteLabel arm) — r9's journeys extend
  the FakeEngine heavily; the arm rides the first chunk that needs it.
- **#880** (effort row Escape rung) — superseded by r9 W2's
  always-mounted Effort Estimate textbox; close-with-evidence when W2
  lands.
- **#870, #875, #938** — menu/keyboard/walk-vocabulary surfaces that
  M2/W6/K2–K4 own or abut; defer to post-r9 to avoid double churn.

## Still waiting (unchanged standing deferrals)

Armed flakies (#697, #650 — close on next CI recurrence + telemetry);
no-recurrence flakies (#336, #287, #384, #883); #327's `commands/`
mutants lane (dedicated sharded pass, ~10–20 h machine time — schedule
it in a quiet window, not beside r9); habit feature backlog
#955–#963 (approved, journey plans + post-H-chain sequencing); O-5
CLI-parity remainder #946–#949 (#945 stays a possible M1 pull-in);
#919 (usage-evidence conditional); the perf family
(#131/#137/#138/#139/#140/#141/#142/#146/#148/#152/#153/#154/#155/#157);
feature-scale items (#122, #121, #127, #181, #135, #138, #142, #152/#153,
#193/#195, #213, #184/#185, #215–#217, #218 + #639–#642, #700/#701/#723,
#494, #444, #440, #117, #113/#114/#115, #167/#180/#191/#198/#219/#224/#225,
#680, #428/#429, the audit registers); C-6 triage deferrals
(#807/#809/#812/#813/#819/#820); #926 (root-caused, watchlist).

## Resource note

r9 runs under the ≤3-agent cap with the memory gate and the two-mode
law (close-out §2/§3). A parallel round must hold **at most 1–2
concurrent chunks**, staggered ≥3 min, never holding the gate lock
long (N-F's research chunk and N-C's compressed chunk are the safest
neighbors; the mutants-style long runs stay exclusive). The N-A/N-D1
placements are deliberately early: both change machinery r9 consumes
(the gate; the journey fixtures), and landing them first is cheaper
than rebasing r9's tail around them.

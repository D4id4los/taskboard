# Kiosk Milestone (`wave-kiosk`) — Completion Assessment & Remaining Steps — 2026-10-21

- **Kind**: backward-looking wave-status assessment + the forward step
  list that closes the milestone.
- **Origin**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§3 chunks K1–K14, §5 sequencing/exit criteria, §6 bookkeeping),
  the completed dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`,
  and the netbook test-environment + measurement report
  `.artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md`
  (merged as `241402a` via PR #387).
- **Evidence day**: 2026-10-21, surveyed against `origin/main` =
  `241402a` and live GitHub tracker state (milestone `wave-kiosk`,
  #4). The local main checkout is one commit behind `origin/main`
  (fast-forward when convenient; untracked local files are unaffected).

## 1. Where the wave stands

Every phase-A chunk and both phase-C code chunks that were dispatched
have landed and their milestone issues closed:

| Wave item | Evidence landed | Tracker |
|---|---|---|
| K1–K4 appliance spine (gui verb, window profile, lifecycle + ADR 0016, `.desktop` + launch docs) | merged (phase-A round) | #237 closed |
| K5/K7 harnesses + benches (RSS smoke, cold-start stopwatch, mapping bench) | merged; dev-machine numbers recorded | #299 #262 #263 closed |
| K6 refresh+draw bench | merged; headless draw works; **D6 dirt finding recorded** (partial region ≈ 97 % of frame) | #300 closed |
| K8 sync counters + engine metrics | `25122c2`, `84dce32` | #156 #145 closed |
| K9 deployment profile, kiosk unit, bring-up runbook | `b8bb610`, `6a7a5ca`, `e2bc3f7` | #233 closed (on the spike evidence, below) |
| K10 dashboard design round + decision session | `703ca61`, `46f5a43`, `47398b3` (hyper + capability token + reconnect script ruled in) | #234 closed |
| K11 web dashboard implementation | `88e99bb`…`9eebbb7` (config/`[web]`, route table, renderers, token ceremony, server, integration journey) | #234 closed |
| K12 encrypted-file credential store | `9de114a`…`ed56cc9` (chacha20poly1305 + getrandom pins, store, S8–S13, ADR 0008 amendment) | #159 #126 closed |
| Phase-B spike, D250 leg | environment + i686 cross-build lane + all four §7 measurement slots, `241402a` | #233 closed with the verdict table |

**Spike verdicts recorded (D250, `aspire-net-srv`)**: RSS 49.7/50.2 MB
(≤ 200 MB — within); cold start 2.11 s (≤ 5 s — within); sync cycle
685 ms recorded; full-frame repaint within budget at board scale ≤
~1 000 tasks (31/46 ms at N=100/1 000; 203 ms at the 10 k extreme);
partial repaint **fails as measured but is bounded, not truly
measured** — the D6 dirt makes "partial" ≈ full frame, so a true
partial verdict must wait for the dirt-tracking change. The dev-machine
reference already shows `draw-full` at N=10 000 over the 50 ms line
(60.5 ms) — the exit report must weigh that against the realistic
one-board wall scale.

**Milestone issue state**: of the wave's thirteen issues, eleven are
closed. Open: **#363** (ambient view + rotation — chunk K13) and
**#160** (systemd/journald packaging — the sd_notify ruling that was
deferred to the spike; it has no decision comment yet).

## 2. What the netbook report leaves open

The report settles the environment, the cross-build lane, and the four
slots, but explicitly hands five items forward (its findings F1–F8):

1. **F1/F2/F5 — runbook fixes** (agent-dispatchable now): §2 must add
   `xserver-xorg-video-intel` (945GME cannot boot the glamor-requiring
   modesetting driver) and `libxkbcommon-x11-0` (winit's runtime
   dlopen); §6 must spell out the X11 seat-session mechanism
   (autologin getty → `.bash_profile` guard → `startx` → `.xinitrc`)
   that §2 promises but never specifies. F4 (stale `/tmp/.X0-lock`
   pitfall) is worth a one-line note in the same pass.
2. **F3/F6 — the renderer/backend decision** (user ruling, spike
   exit): the "no gen3 hardware GL" premise is wrong on this unit
   (Mesa 22.3.6 still exposes GL 2.1 on the 945GME), and today the
   software path is reached by *permission* (the `taskboard` user
   cannot open `/dev/dri/card0`), not by selection. The spike exit
   must rule the sanctioned posture: keep the permission-based
   software fallback as the supported lane, or measure/sanction the
   hardware-GL path too. ADR 0016 / runbook §1 get the outcome.
3. **F7** — the `connect` 0600-rewrite bug is already filed as
   #386; it is a follow-up bug, not a milestone gate.
4. **F8** — cross-build hygiene lesson (never build from a live tree
   mid-commit); belongs in the runbook's §5 lane notes with the same
   docs pass as item 1.
5. **The true-partial repaint verdict** — needs the D6 dirt-tracking
   change (incremental model push instead of whole-`VecModel`
   replacement). See step 3 below.

Not blocking, for completeness: the **NF310 leg** (binary leg A,
x86_64) has its runbook lane and on-device 64-bit boot already
verified; the four-slot measurement campaign was executed on the D250
and #233 closed on exactly that evidence. If the user wants the NF310
soak/soak-equivalent numbers too, it is an optional user-executed leg
using the same runbook §7 — the milestone's exit criteria as written
are satisfied by the recorded D250 verdicts.

## 3. The remaining steps to milestone closure

Status update 2026-10-21 (post-decision session): steps 1–3 are done;
steps 4–5 remain, gated as described.

1. **Spike-exit rulings — ALL RULED 2026-10-21:**
   - Renderer posture: **the measured permission-based fallback is
     sanctioned** — the appliance user stays out of `video`/`render`;
     premise correction (hardware GL 2.1 exists, femtovg needs ≥ 3.0)
     recorded; no env pin, no build flip. ADR 0016 amendment + runbook
     §1/§8 in PR #388.
   - i686 release target: **stays a documented on-demand lane**, no
     release-matrix leg. Runbook §5.5 in PR #388.
   - #160 (sd_notify): **ruled not-needed** under `Type=simple` +
     `Restart=always` with the D250 evidence; decision comment posted
     and the issue closed 2026-10-21.
2. **Runbook-feedback docs PR — OPEN: PR #388** (`docs/
   kiosk-runbook-spike-findings`): the F1/F2/F4/F5/F8 edits, the F3
   premise correction, the two rulings above, the #386 operational
   caution, **plus the K11 stage-1 plan doc cherry-picked from the
   superseded `docs/kiosk-dashboard-impl-plan` branch** (bookkeeping).
3. **True-partial repaint follow-up — TRACKED: issue #390**
   (incremental model push / dirt tracking; the partial budget stays
   "bounded above, verdict pending" until it lands). K14's exit report
   links it.
4. **K13 — ambient view + display rotation (agent; the last code)**
   — stays gated exactly as planned: **strictly after daily-driver
   chunk 10** (`feat/ui-today-inbox`, the Today view + inbox + capture
   + move ring, closes #346 + #347). That gate is real, not
   bureaucratic: K13's rotation consumes `ViewId::Today` and the
   rotation-ready today view, and collides with its file set. **This
   is the milestone's critical path**: the daily-driver wave has 12 of
   13 issues open and chunk 10 is its second-to-last chunk. Nothing in
   the kiosk milestone can shorten that.
5. **K14 — docs + wave exit (agent; last, after K13)** —
   `docs/kiosk-wave`: architecture.org/README promotion, CHANGELOG
   release-line promotion, the exit report at
   `.artifacts/reports/<date>-kiosk-wave-exit-report.md` carrying the
   D250 verdicts and the three now-ruled decisions, and the Q6
   default-mode input (Q6 — the post-wave default `[app] mode`
   question — gets its on-device input recorded there for the user's
   separate ruling; the wave itself does not decide it). K13's PR
   closes #363; with #160 already closed, the milestone empties.

## 4. Hygiene observed during this assessment (no action forced)

- The local main checkout is one commit behind `origin/main`
  (`241402a`); a `git pull --ff-only` syncs it. This report is written
  against `origin/main` content.
- The worktree `../taskboard-WT-docs-kiosk-dashboard-impl-plan` (branch
  `docs/kiosk-dashboard-impl-plan`, tip `1341ba0` "K11 stage 1") was
  **not merged** — resolved 2026-10-21: the plan doc is cherry-picked
  into PR #388, the worktree is removed; delete the local branch after
  #388 merges.
- `scripts/worktree-doctor.sh` crashes (exit 128, "not a git
  repository") when a sibling directory named `taskboard-WT-*` is not
  itself a worktree — observed 2026-10-21 with the benchmark round's
  nested `taskboard-WT-ci/benchmark-lane-off-pr` layout (branch names
  with a slash produce a plain parent dir). Filed as #395 (`dx`).
- The in-flight benchmark-lane round (`.artifacts/plans/2026-10-21-
  benchmark-lane-dispatch-plan.md`, its CI-cost report untracked in
  the main checkout) is independent CI work; it touches the K6/K7
  benches' CI lane but nothing in this milestone's remaining steps.

## 5. Bottom line

As of the 2026-10-21 decision session, every spike-exit decision is
ruled and recorded (PR #388, closed #160), the partial-repaint debt is
tracked (#390), and the runbook has the spike's findings folded back
in. The wave's code is done except **K13**, and the docs are done
except **K14**. Both wait on the daily-driver wave reaching chunk 10
(`feat/ui-today-inbox`); K14 closes the milestone behind K13.

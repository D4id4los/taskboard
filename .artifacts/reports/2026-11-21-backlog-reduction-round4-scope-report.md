# Backlog-Reduction Round 4 — Scope Recommendation (2026-11-21)

Context: rounds 3 (runtime + sweep, 15/15 chunks) and GUI review 9 are
fully landed; one docs PR open (#1142, the golden-coverage review —
should merge before this round's golden wave). 178 open issues. **This
is the first round where GUI feature work is unblocked** (r9's surfaces
are stable), so the journey-gated feature families deferred since
round 8 are the natural centerpiece.

## A. The centerpiece: the deferred feature families (journey-gated)

### A1. The habit feature family (#955–#963 — approved 2026-11-18, never scheduled)

Nine approved items; the research report
(`2026-11-18-round9-habit-parity-research.md` §4) is the shared origin.
Recommended grouping into four implementation chunks under one journey
wave:

| Chunk | Items | Shape |
|---|---|---|
| HF1 | #955 skip days (p2) + #958 pause (paused_until twin) | window/streak-engine semantics (domain + row display); the O-1 ruling's window law binds |
| HF2 | #956 undo completion (p2) + #961 inline quota edit | row interactions; #956 touches the NotUndoable(HabitSurface) fence — a recorded ruling to supersede deliberately |
| HF3 | #957 heatmap/history (p2, the prime motivator) + #962 streak badge + #963 statistics | the history/visualization view on the W1 ModalDialog/base-widget layer |
| HF4 | #959 per-habit reminder overrides + #960 per-weekday schedules | schedule/reminder engine; rides the O-1 nudge law and the reminder engine's anchor_time |

Journey wave first: one habit-family journey plan (or per-chunk plans
— the framework's call at stage 1), operator gate, then the four
chunks. Domain work is additive (O-2's law: no TaskKind variant).

### A2. The O-5 CLI-parity remainder (#945–#949)

| Chunk | Items | Shape |
|---|---|---|
| OP1 | #945 board archive + #946 board reset + #947 reconcile w/ consent letter | the destructive-ceremony GUI surfaces; R-10/R-11 supersessions land with them; consent-letter grammar per the sweep report |
| OP2 | #948 export dialog (p2 by size — the largest CLI-only feature) | settings/menu-anchored export dialog (format, filters, pdf destination, open-after-export) |
| OP3 | #949 GUI status surface + #1111 (the draining line in status text) | one status-surface chunk |

### A3. Restore/undo surfaces

- **#979** — the restore verbs' first dispatch surface (CLI first per
  the issue's proposal) — journey + small chunk; pairs naturally with
  OP1's CLI-adjacent work.
- **#353/#354 (undo v2 UI legs)** — ⚠ **operator decision gate first**:
  both carry the 2026-10-19 ruling notes ("needs further
  consideration" — #353: v1's line discipline may suffice; #354: rules
  are cheap to re-add by hand). Round 4 includes them **only if the
  operator reaffirms**; otherwise they stay parked and the decision is
  recorded on the issues. If reaffirmed: one journey plan, #354 first
  (domain synthesis catalogue) then #353 (branch-picker UI).

### A4. The small gated bundle (#976/#1011/#1045/#1083/#1093)

One journey plan, one or two chunks: **#976** (p2 — the R-4
outside-click flip for capture/recur, owed by the W2 migration),
**#1011** (banner Unmute direction), **#1045** (ConfigReload outcomes
in a read-only UI lane), **#1083** (poll_secs GUI field on the Files
row — retires #1083's CLI-only exception), **#1093** (the
journal-degraded boot signal surfaced in the UI — completes B2's
hybrid policy).

### A5. The r9-parity remainder sweep

**#1030/#1031/#1032/#1041/#1057/#1066/#1067/#1068/#1091** — the
keyboard/parity small items r9's chunks filed. One journey plan (the
O-8 parity law is the standing authority), two implementation chunks
(keyboard bindings; cursor/walk affordances).

## B. The support wave (ungated)

- **S1: #1080 (p2 bug)** — push vs sync report board attribution
  divergence (R3's generator found it); the equivalence property's
  cross-board class. Sync crate; the fix decides which attribution is
  correct and pins the property.
- **S2: #1090 (p2) then #1087** — the run-side nextest wall regression
  tuning, then phase-2 consolidation (state/sqlite/sync binaries).
  #1090 first: it decides whether #1087 is even wanted.
- **S3: #327** — the sharded mutants lane over `commands/` (~10–20h
  machine time), unblocked by R4; **exclusive lane**, survives as the
  round's background work.
- **S4: test-infra residue** — #1113 (p2, ui_requests port/guard legs
  compose process env), #1075 (drop the subprocess fixtures' explicit
  shutdown budgets — actionable since R2c), #1134 (paused-time clock
  breaks sqlx pool connect), #1120 (boot-pull settle burns budget on
  deconfigured boards), #1106 (default_device_json injection seam),
  #1100 (exactly-once push-seam split).
- **S5: gate/harness residue** — #1073/#1074/#1076 (ux-coverage
  universe/matching residues), #1108 (the 2026-11-14 plans resolve
  AMBIGUOUS/MISSING against the consolidated binaries — bound-test
  mapping for the retro cohort), #1112 (habits_journey onto the B4a
  helper), #1097 (ci.yml comment names a nonexistent file), #1133
  (rename/split ConfigBatchError), #1137 (Mutex cell → watch channel),
  #1110 (gate-lock hung-holder liveness — the round-3 incident law).
- **S6: docs/operator** — #1078 (p2: document the lld prerequisite for
  local builds), #1103 (the unregistered
  `taskboard-WT-feat-reflection-domain` directory — operator cleanup,
  dispatcher-verified).
- **S7: golden coverage** — #1141 (7 zero-golden surfaces + the sparse
  fixture), #1140 (self-describing manifest), #1139 (comparator:
  chroma-aware + scale-aware) — **after #1142 merges**; one or two
  chunks; the review report's D1–D10/R1–R6 priority lists are the
  normative input.
- **S8: widget refactor bundle** — #1096 (form-row variant), #1121
  (hint caption anatomy), #1125 (AnchoredOverlayHost pointer-anchor +
  fill-to-floor), #1126 (DropdownTone → Tone ladder), #1127 (labels
  roster form-row), #1128 (MenuShell cap-rows dead knob — remove).
  Plus the two bigger refactors finally unblocked: **#1029** (severing
  max-width rule generalization) and **#1037** (flow-settle FK
  vocabulary) — include if the round has capacity; they have waited
  three rounds.

## C. Habit-log LWW follow-ups (from B1's landed clock bump)

- **#1095** (whole-second RFC 3339 strings as the LWW guard) — small
  arch; include in S-wave.
- **#1094** (cross-device same-second merge loses one increment) — a
  counter-CRDT design question; **design round, not a fix chunk** —
  defer unless the operator wants it this round.

## D. Deferred (standing)

Watches (#988/#983/#987 chrono, #975/#972 Slint upstream, #1021,
#1052, #919, #980); flaky watchlist (#697/#650 armed, #336/#287/#384/
#883/#1020, #926, #968 armed-hypothesis); the perf family
(#131–#157 set); feature-scale items (#122/#121/#127/#181/#184/#185/
#191/#193/#195/#198/#213/#215–#219/#224/#225/#444/#440/#494/#117/
#639–#642/#680/#691/#700/#701/#715/#716/#723/#762/#769/#770/#775/#778/
#782); C-6 deferrals (#807/#809/#812/#813/#819/#820); #113/#114/#115
(operator intent gate); #357 (exploratory).

## E. Recommended round shape

Two-stage round, ~20 chunks:

1. **Journey wave (stage-1 agents only, one gate sitting)**: the habit
   family plan(s), the O-5 parity plans, #979's plan, the small
   gated bundle's plan, the parity-remainder plan; the #353/#354
   operator decision lands here.
2. **Implementation waves** after acceptance: HF1→HF4 (serialized
   where the engine overlaps: HF1 → HF2 → HF4 → HF3 display-side),
   OP1–OP3, the small bundle, the parity remainder — beside the
   ungated support wave S1–S7 under the standing resource law
   (≤3 agents, memory gate, flock-deferral; S3 exclusive).

**Recommended cut line if the round must shrink**: keep A1 (the habit
family — the approved motivator), A2's OP2 (#948, the largest parity
gap), A4 (#976 is a p2 owed by W2), S1/S2/S3/S5/S7; defer A2's OP1/OP3,
A5, S4-tail, S6, S8, and #1029/#1037 to round 5.

The dispatch plan follows on acceptance.

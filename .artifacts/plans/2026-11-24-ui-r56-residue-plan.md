# Chunk C1 — UI r5/r6 residue bundle — Stage-1 Plan (2026-11-24)

- **Round**: backlog-reduction round 6, chunk C1 (dispatch plan
  `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
  §1/§10; scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §B — normative).
- **Branch**: `fix/ui-r56-residue` (worktree
  `../taskboard-WT-fix-ui-r56-residue`, branched off fetched `origin/main`
  per #1185).
- **Issues**: #1289, #1184, #1115, #1221, #1190, #1191, #1194, #1286.
- **Verdicts**: **7 fix, 1 dropped** (#1184 — CLOSED on the tracker; its
  root-page-budget ask was resolved by D4's census guard
  `us25_s1_root_quick_rows_within_budget` and the follow-up decision was
  re-filed as #1289, which this chunk owns; dropping per the dispatch
  plan's round law (d), reason recorded here, closure is the
  dispatcher's bookkeeping anyway).
- **Laws**: (a) typed/structural pins — never text assertions, except
  #1190's explicit carve-out (the human label is presentation text; the
  tests keep asserting the typed index); (c) diagnosis-first — every
  named mechanism below was verified against the worktree at
  `a6971aba` before the decision was recorded; (d) out-of-scope
  findings become `backlog:` issues immediately.

## 0. Stage-1 diagnosis ledger (verified mechanisms)

| Issue | Named mechanism | Verified at |
|---|---|---|
| #1115 | `apply_close_menu` (adapter.rs ~7486) clears menu models/subject but never touches `habit_remove_armed`; a later `habit.remove-confirm` still routes `DeleteHabit` (the confirm consumes the one-shot at adapter.rs ~1901) | yes |
| #1221 | the commit funnel (`on_recur_commit_clicked`, adapter.rs ~12934) and the outside-click accept (~18054) both `return` silently when `assemble_recur_draft` fails; no flag, no mirror, no notice | yes |
| #1190 | `export-dialog.slint:269` renders `"The export refused (kind " + root.model.refusal + ")"` where `refusal` is the `REFUSAL_KINDS` index (`src/requests.rs:505`) | yes |
| #1191 | `app-window.slint:1922-1923` wires both `dismissed` and `accepted` to `export-dialog-close()`; the ModalDialogHost kind law fires `accepted` only for `edit` kind — dead arm on this confirm-kind dialog | yes |
| #1194 | `StatusPanelOpen` has bindings only as the **close** (Escape in `ContextId::StatusPanel`, all three presets at presets.rs 759/1347/1621); no global open chord exists. `AppSettings`' twin carries `Ctrl+,` global (presets.rs 351/847) and `Ctrl+x c` (emacs) | yes |
| #1286 | `task-card.slint:217-218` hard-codes `checked-label: "Mark task not done"; unchecked-label: "Mark task done";`; `habits.slint:344` instantiates `TaskCard` with no label override and routes `done-toggled => log-clicked` | yes |
| #1289 | the D4 census guard (`menu.rs:1035`) enumerates the measured group-less registry (dated absorption 2026-11-24) and enforces "curated five lead"; G6 passes with the page as landed; the law's *text* still carries the false "five == the group-less set" premise (the dated erratum in the misc-gated-journeys plan §1) | yes |

## 1. #1289 (p2) — re-base the quick-row budget law on the measured registry

**Decision: re-base the law's text; do NOT re-curate.**

- G6 (`the_open_menu_closes_its_box_above_the_status_bar_at_every_geometry`)
  passes with the full root page as landed — the "fifth row clips"
  measurement no longer reproduces, so the re-curation branch (moving
  `AppUndoBranches` / the chord families into group pages) would be
  motion without a defect. The registry stays as measured.
- The fix is **docs/law-text only**: (i) the R-3/T-1 doc comments
  (`menu.rs`, the census guard's doc block) restate the budget law as
  the *measured* one: the curated five LEAD every root page; the
  group-less set is exactly the enumerated census (the 2026-11-23
  measurement, absorbed 2026-11-24); new group-less registrations still
  fail the guard and must join a group page or amend the census with a
  dated note; (ii) a dated closure note appended to the erratum in
  `.artifacts/plans/2026-11-22-misc-gated-journeys-ux-plan.md` §1
  citing this chunk; (iii) the kiosk 1024×600 re-measure with the full
  page is **already evidenced** — G6's passing run at every geometry
  including the kiosk panel is the measurement; the law text cites it
  (per round law (c): recorded with the measurement cited, no new
  harness).
- **Pin**: none new beyond what the census guard already enforces (its
  teeth are the pin). Any test added would pin a comment — banned by
  the no-text-assertions law.

## 2. #1115 (p3) — the armed habit-remove staging survives the menu's Escape close

**Decision: fix the close paths to clear the staging (the
doc-comment law), not the comment.**

- The code comments (`src/actions.rs` MenuDismiss area) already state
  the intended law ("disarmed by the Escape close"); the survival is a
  safety hole, not a contract (a hidden menu that still deletes on a
  later confirm is exactly the #1059 armed-confirm grammar's failure
  mode). Fixing the code honors the documented law.
- **Fix**: `apply_close_menu` (and the `MenuBack` arm's close path —
  verify it funnels through `apply_close_menu` or add the disarm there)
  clears `habit_remove_armed` (and its sibling `habit_convert_armed` /
  `habit_restore_armed` staging for the same one-subject law — the
  convert staging already clears on menu open; the remove/restore
  disarm joins it, recorded as one rule: *a menu close disarms every
  habit confirm staging*).
- **Pin**: extend/replace the existing `us5_s3_escape_cancels_the_armed_menu`
  coverage (habits_journey.rs): after Escape closes the armed menu, a
  `habit.remove-confirm` dispatch is the disarmed `NoTarget` path —
  typed outcome (`matches!` on the dispatched command absence / the
  one-shot remaining `None`), never text. Red pre-fix (the staging
  survives), green post-fix.

## 3. #1221 (p3) — the recur editor's invalid staged draft is silent on both accept paths

**Decision: surface a typed invalid-draft signal on both paths; keep
the no-op dispatch (the SB1 contract is untouched).**

- The dispatch no-op is deliberate (the plan's trap: outside click on
  an invalid staged draft is a no-op, not a forced accept) — only the
  *silence* is the defect.
- **Fix**: an adapter-owned `recur_draft_invalid: bool` mirrored onto
  the window (`recur-draft-invalid` property, the existing mirror
  pattern at adapter.rs ~959): set when `assemble_recur_draft` fails
  on either accept path (commit button ~12957, outside-click accept
  ~18085), cleared on any draft field edit (`on_recur_count_edited` and
  siblings) and on editor close. The recur editor composition renders
  the caption when the flag is set (the dialog stays open — unchanged).
- **Pin**: journey test — an invalid draft + commit attempt leaves the
  editor open AND sets the invalid flag (typed mirror read); a valid
  edit clears it. No text assertion on the caption itself (the flag is
  the typed outcome; the caption's existence is structural — bound to
  the flag — and may be checked as `visible: root.recur-draft-invalid`).

## 4. #1190 (p3) — the export refusal renders a bare kind index

**Decision: a presentation-side static label table per
`REFUSAL_KINDS`, rendered as the caption; tests keep asserting the
typed index.**

- One table, one place: a `REFUSAL_LABELS` string array in
  `export-dialog.slint` (or a presentation helper feeding it —
  stage-2 picks the slot that keeps the UI layer dumb; the array
  indexed by `root.model.refusal` is the recorded default), line 269's
  caption becomes `REFUSAL_LABELS[root.model.refusal]` composed into
  the refusal sentence. Human phrasing per kind (e.g. BadTaskFilter →
  "the task filter matched nothing"), recognition-over-recall per the
  issue.
- **Pin**: structural — the labels array length equals
  `REFUSAL_KINDS.len()` (a drift between the canonical table and the
  labels fails); the journey tests continue asserting the refusal
  **index** (unchanged pins — round law (a) holds; the label text is
  never asserted).

## 5. #1191 (p3) — the export dialog's unreachable `accepted` arm

**Decision: drop the arm, with the palette-precedent comment.**

- Behavior-neutral polish; the palette dialog's precedent (#1026)
  explicitly omits it. `app-window.slint:1923` removed; the wiring
  carries the comment: confirm-kind dialog, `accepted` fires only for
  `edit` kind (the ModalDialogHost header law) — never here.
- **Pin**: none new (dead-wiring removal has no behavior to pin; the
  existing dismissed/close pins unregress).

## 6. #1194 (p3) — no direct keyboard binding for `ActionId::StatusPanelOpen`

**Decision: a global open chord `Ctrl+Shift+,` in all three presets
(the settings opener's grammar mirrored; the chord is unbound today —
verified against the presets), plus the keymap-snapshot documentation
pins refreshed.**

- Twin semantics with the close (Escape in StatusPanel, the D28b
  shape) already landed; this adds the open. `Ctrl+,` is taken by
  AppSettings (standard + emacs; ViNormal carries `Z s`); `Ctrl+Shift+,`
  has no binding anywhere (grep-clean at `a6971aba`).
- Sites: standard preset, emacs preset (`Ctrl+Shift+,`; the emacs
  preset's AppSettings is `Ctrl+x c` — the diagnostics twin still gets
  the same mnemonic chord, recorded as a deliberate deviation from
  strict emacs grammar for cross-preset consistency), ViNormal.
- **Pin**: the keymap snapshot/documentation pins (whatever mechanism
  pins the presets today — the keymap_case_seam / snapshot rows)
  refreshed; a typed assertion that the chord resolves to
  `StatusPanelOpen` in all three presets at `ContextId::Global`. No
  text assertions.

## 7. #1286 (p3) — the habit done slot keeps the task-verb a11y label

**Decision: per-instantiation done-slot labels on `TaskCard`;
`habits.slint` supplies the habit phrasing; task surfaces unchanged.**

- `TaskCard` gains optional `done-label` / `done-undone-label`
  properties (names per the existing `checked-label`/`unchecked-label`
  pair — stage-2 may align the names; recorded default: keep the
  CheckBox literals as the property defaults so every existing
  instantiation is behavior-identical with zero edits). The slot stays
  pure geometry per the W-1 design — only the label literals become
  overridable.
- `habits.slint`'s `habit-card` instantiation supplies the habit
  phrasing matching the row's progress-phrase vocabulary (US-1; the
  +1 log verb, e.g. "Log habit +1" / the met-bit wording already in
  the surface's phrase table — stage-2 reads the vocabulary and
  matches it; the exact string is presentation, unasserted).
- **Pin**: typed — the habit row's done slot label mirror (whatever
  a11y-label address the corpus uses) reads the habit verb, not the
  task verb; the task surfaces' labels unchanged (existing pins
  unregress). **Trap**: the journey corpus uses labels as addresses —
  the label *change* on the habit row may break label-addressed
  lookups; stage-2 inventories `habits*_journeys.rs` /
  `habit_family_journeys.rs` / `habit_surface_journeys.rs` for the
  old verb as an address and updates the addresses (that is an
  address change, not a weakened test).

## 8. Sequencing & commit plan

One branch, small commits in dependency-free order (no item depends on
another):

1. `fix(ui): the menu close disarms the habit confirm staging` (#1115)
2. `fix(ui): the recur editor surfaces its invalid staged draft` (#1221)
3. `fix(ui): the export refusal renders a human label per kind` (#1190)
4. `fix(ui): drop the export dialog's unreachable accepted arm` (#1191)
5. `feat(ui): a global status-panel open chord in all presets` (#1194)
6. `fix(ui): the habit done slot labels its own verb` (#1286)
7. `docs(ui): the quick-row budget law re-based on the measured registry` (#1289)

Every commit standalone (builds + hooks green); `#1184`'s drop is
recorded here only (it is already closed; no code).

## 9. Acceptance (per dispatch plan §10)

- #1289: the law text re-based with the measurement cited (menu.rs +
  the erratum note); the census guard's teeth unregressed.
- #1115: Escape closes the armed menu AND disarms (typed pin red→green).
- #1221: invalid draft surfaces on both accept paths; the no-op
  dispatch contract unregressed (`us2_s2_invalid_draft_stays_open`
  stays green).
- #1190: refusal caption per typed kind; index pins unchanged.
- #1191: dead arm gone, precedent comment in place.
- #1194: the chord resolves in all three presets; snapshots refreshed.
- #1286: habit row labels the habit verb; task rows unchanged.
- Diffs confined to the named sites + tests + the one erratum doc.

## 10. Risks

- **#1115**: the disarm-on-close must not eat a *legitimate* flow —
  the arm→(menu stays open)→confirm path is unaffected; only close
  paths disarm. The `MenuBack` arm needs verification that it funnels
  through `apply_close_menu` (else double-fix).
- **#1221**: the outside-click accept path lives in a different arm
  (~18054) than the button (~12957) — both must set the flag; missing
  one is the exact bug being fixed.
- **#1194**: the kiosk preset may deliberately withhold global chords
  (kiosk posture) — stage-2 checks the preset's recorded posture
  before adding; if the kiosk preset excludes global application
  chords by law, the deviation is recorded rather than forced.
- **#1286**: label-as-address breakage in the habit journey corpus
  (inventory listed above).
- **#1289**: none material (docs-only); the only risk is scope creep
  into re-curation — explicitly out of scope here.

## 11. Estimated diff size

M as planned: ~7 production sites (adapter.rs, export-dialog.slint,
app-window.slint, task-card.slint, habits.slint, presets.rs ×3
blocks, menu.rs docs) + journey/unit pins across ~5 test files + the
erratum note. Order of 250–450 changed lines.

## 12. Round-5 survival check (dispatch plan §1's per-chunk duty)

All eight issues verified against the tracker 2026-11-24: #1184 CLOSED
(dropped, reason §0/§1); the other seven OPEN and none of their
mechanisms were fixed by round 5 (each named mechanism re-verified in
the ledger above at `a6971aba`, which already carries round 5's D3a
habit-surface polish and the #1306 timer echo fix).

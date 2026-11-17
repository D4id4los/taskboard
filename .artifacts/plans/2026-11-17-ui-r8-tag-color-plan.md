# [Plan] ux-r8 chunk 7 — tag colour: the domain `Color::parse` mint, the overlay's free-form field, the roster-only board mount, the card chips

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan (TDD-first) for dispatch chunk 7 of the
  round-8 GUI review; implements the accepted UX journey plan
  `.artifacts/plans/2026-11-16-tag-color-ux-plan.md` (US-1..US-3,
  W-1..W-3, rulings R-1..R-3 — all normative) per the dispatch plan
  `2026-11-16-gui-review-round8-dispatch-plan.md` §1/§8.
- **Worktree**: `../taskboard-WT-feat-ui-r8-tag-color`, branch
  `feat/ui-r8-tag-color`, off `origin/main` @ `a440eaa5`.
- **Landed context read as landed**: chunk 2's focus-restore seams,
  chunk 3's #840 cleared armed identity
  (`on_labels_overlay_delete_clicked`'s stale-index/dead-label guard),
  chunk 4's board grammar (`BOARD_CONTEXT` opens after
  `select_board`; the selection VM beside `view_state.board`).
- **Status**: READY for stage 2.

## 1. Decisions (every open question the origin left, settled)

### D-1 — `Color::parse`: signature, error type, corpus (US-1, origin §7)

New fallible mint on the existing newtype in
`crates/taskboard-domain/src/entities.rs` — the house A1 pattern
(`docs/coding_guidelines.org` §2.1): hand-rolled, no derive crates, the
lenient `new` documented as it is today.

```rust
/// A colour value failed the six-hex mint.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(thiserror::Error)]
#[error("expected exactly six hex digits (no '#')")]
pub struct InvalidColor;

impl Color {
    /// Mints a colour from user input at a trust boundary: exactly six
    /// ASCII hex digits, any case, normalized to lowercase. The
    /// user-input minting constructor (the A1 pattern) — the CLI's
    /// `parse_color` grammar promoted to the domain so the GUI commits
    /// through the same fn (one behavior, one implementation). The
    /// lenient `new` stays for wire/storage echoes (sync adoption of
    /// remote content); parse at input, never re-validate after.
    pub fn parse(raw: &str) -> Result<Self, InvalidColor> { ... }
}
```

- **Corpus (the CLI's forms verbatim, `cli.rs` `parse_color` + its
  tests)** — accepted: `""`-free, no `#`, exactly 6 bytes, each
  `is_ascii_hexdigit`, lowercased on mint (`"31CC7C"` → `31cc7c`,
  `"AbCdEf"` → `abcdef`). Rejected: `#fff`, `fff`, `ffffff0`,
  `zzzzzz`, `31cc7`, `""` (empty). The domain test module owns this
  corpus (the contract moves with the grammar; the CLI's corpus tests
  are deleted, not duplicated — one behavior, one test, lowest layer).
- `swatch_hex`'s render-side gate (models.rs) is a *display* echo, not
  a validator — it stays as is; the plan deliberately does not
  re-express it via `parse` (the echo must render the lenient stored
  value's transparency, not refuse it). Grep-check after landing: the
  only duplicate of the six-hex shape test is `swatch_hex`'s render
  gate, and it is not a parse.
- `InvalidColor` lives beside `Color` in `entities.rs` (a pure domain
  error; `thiserror` per AGENTS §4). No schema, no message change
  (origin §7's letter).

### D-2 — the CLI refactors onto the mint (one behavior, one implementation)

`crates/taskboard-app/src/cli.rs`: `parse_color` becomes the clap
value-parser wrapper:

```rust
fn parse_color(raw: &str) -> Result<String, String> {
    taskboard_domain::Color::parse(raw)
        .map(|color| color.as_str().to_owned())
        .map_err(|InvalidColor| "expected exactly six hex digits (e.g. 31cc7c, no '#')".to_owned())
}
```

- The usage-error *text* stays (it is the clap help string, not a test
  assertion). The two existing CLI tests reduce to wrapper-mapping
  tests: one accepted case + `matches!`-shaped rejection (typed
  outcome; no corpus duplication). `DEFAULT_LABEL_COLOR` and the
  `labels add/edit` / `boards color` argument wiring are untouched.
- No `Color::parse` callsite anywhere re-implements the grammar; the
  stage-3 grep check is `rg 'is_ascii_hexdigit'` → only `entities.rs`
  and `swatch_hex`'s render gate.

### D-3 — the flow widening: one `Flow::Labels`, two subjects (US-2)

`view_state.rs`: `Flow::Labels { task: TaskId }` (view_state.rs:399)
widens its payload to an enum — one flow, two mounts (the origin §5's
"one grammar"):

```rust
/// The labels overlay's subject: the detail view's task (the
/// assignment-toggles mount) or a board surface's roster-only mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelOverlaySubject {
    /// The detail mount over the viewed task.
    Task(TaskId),
    /// The roster-only mount over the selected board (no toggles —
    /// R-2's letter made visible).
    Board(BoardId),
}
```

- `Flow::Labels { subject: LabelOverlaySubject }`; every
  `Flow::Labels { task }` pattern in `view_state.rs`/`adapter.rs`/
  `models.rs` updates to match on the variant (agile churn — AGENTS
  §2.1 posture). `open_labels` gains a subject parameter (the task
  callers pass `LabelOverlaySubject::Task`); a new
  `open_labels_board(&state, board)` resolves through
  `normalize_board` + `self.board` (the `begin_rename_board`/
  `begin_board_color` shape, view_state.rs:3529/3646) — the subject
  resolution is the *selected board*, the same source the board
  context menu's other rows address. `close_labels` unchanged (the
  ladder's identity survives; the #840 cleared identity composes —
  arming guards already read the live roster).
- **No toggles**: the overlay push (`models.rs` label block) sets
  `assigned: false` for board-subject rows and the new
  `shows-assignment` flag false; the detail mount pushes true. The
  panel's per-row toggle affordance is conditional on the flag (R-2:
  the row-set composition is the visible ruling).

### D-4 — the mount arm: one effect, adapter-side subject resolution (W-2)

`router.rs` keeps `ActionId::LabelManage → UiEffect::LabelsOpen`
unchanged (the registry entry's `contexts` widen instead — D-7), and
the adapter's `LabelsOpen` interpretation arm (adapter.rs:3690) gains
the board leg:

```
kiosk → Vec::new() (unchanged, first)
Flow::Labels already open → Vec::new() (unchanged, the modal discipline)
subject_task() = Some → the task mount (unchanged path)
else → open_labels_board: normalize_board + self.board
       Some(board) → the roster-only mount; None → Vec::new() (inert)
```

- The adapter arm reads the *current* subject surface: the detail view
  has `subject_task()`; the board surface has the selected board. No
  effect-payload widening, no router change — the
  "kiosk/no-subject refusals live in the adapter's arms" precedent
  (the arm's own doc comment). The kiosk gate stays first (origin
  platform scope: kiosk renders neither).
- `app-window.slint`: the existing `View.task-detail && labels-open`
  mount gains a sibling conditional `View.board && labels-open`
  mounting the same `LabelsPanel` with the same callback set (the
  window-level overlay grammar; both mounts read the same pushed
  halves). `labels_open`'s push becomes subject-kind-affine:
  `Task(t) => vs.subject_task() == Some(t)`,
  `Board(b) => vs.board == Some(b)` (a dead board subject renders
  closed — the reconcile pass drops the flow anyway).
- **Focus contract (standing law e)**: the board mount composes with
  chunk 2's restore seams — the overlay exit restores the invoking
  element or its container anchor; the board-bar segment (the menu's
  invoker) is the container anchor. The focus-restoration census's
  overlay-sites table gains the board-mount row in the same commit.

### D-5 — the form's colour field: draft, echo, preview, palette agreement (W-1, US-1/S1)

`labels-overlay.slint` gains the free-form field beside the swatch row
inside the existing form mount:

- **Panel surface**: `in property <string> form-colour`,
  `in property <bool> colour-valid`, `in property <color>
  colour-preview`, and callbacks `colour-edited(string)` —
  `// slint: colour-edited` (the #782 law; every new seam carries the
  comment). A small preview `Rectangle` (the field's draft echo —
  `colour-preview` pushed from the adapter, transparent when the
  draft is invalid: the `swatch_hex` hygiene, never re-derived).
- **Adapter draft**: `LabelFormState.color` (adapter.rs:120) stays the
  single string buffer the swatch click already writes. New
  `on_labels_overlay_colour_edited` seam: writes `form.color` (the
  bare-mut due-buffer pattern), then `refresh_forced`. The push side
  (models.rs label block) computes:
  - `colour-valid = Color::parse(f.color).is_ok()` (typed fact, the
    draft's validity — `FieldValidity`-shaped but a plain bool; the
    field renders the typed invalid echo, no hint text invented),
  - `colour-preview = swatch_hex(&Color::new(f.color.clone()))`
    (invalid → transparent, US-1/AC-3's echo law at draft time),
  - `swatch-selected` unchanged (an exact palette match highlights —
    typing a palette value pre-marks its swatch: one vocabulary, two
    pickers, AC-4's inverse direction).
  - `form-colour = f.color` — but the push **never re-seeds a field
    the user is typing in**: the title's mount-only-read rule (the
    panel doc, labels-overlay.slint:33). The colour field reads
    `form-colour` via `initial-text` on mount and on *swatch-clicked*
    (the arm pushes a forced refresh so the field's text re-seeds the
    swatch's exact value — AC-4's letter: the field reflects the
    swatch). Typed edits flow adapter-ward, never push-ward.
- **Seam naming (the doctrine bar)**: SC-W-N2's `*-picked` consistency
  names the new callback `colour-edited` on the panel and
  `on_labels_overlay_colour_edited` in the adapter (the
  `title-edited` twin — the existing form's own convention, which the
  walkthrough re-checks).

### D-6 — the commit funnel parses at the boundary (US-1/S2, AC-1/AC-2)

`on_labels_overlay_form_commit_requested` (adapter.rs:~11037) changes
in two places, nothing else:

- **Board resolution**: today it derives `board` from
  `subject_task()`'s stack. Widen: read the flow's subject —
  `LabelOverlaySubject::Task(t)` → the existing derive path;
  `LabelOverlaySubject::Board(b)` → `b` directly (the roster-only
  create subject, origin §7's usage note). No subject → the existing
  typed inert (`return false`).
- **The mint**: both create and edit colour changes mint via
  `Color::parse(&form.color)` (not `Color::new`). `Err(InvalidColor)`
  → `return false` **before any command dispatch** (AC-2: commit
  inert, form stays open, nothing dispatched — the editor refusal
  posture, the empty-title inert's twin). The colour is compared to
  `label.color.as_str()` unchanged for the sparse mask (a normalized
  draft equals the stored lowercase string when only case differed —
  the mint's normalization makes that comparison honest).
- The palette path commits the same funnel: `SWATCHES` bodies are all
  six-hex, so `parse` accepts them — one vocabulary, both pickers
  (AC-1's seam test asserts the *exact payload* equality of palette
  and field paths).

### D-7 — the menu row + the widened binding (US-2/S1, AC-4)

- `actions.rs` registry entry (actions.rs:1266):
  `contexts: &[ContextId::TaskDetail]` →
  `&[ContextId::TaskDetail, ContextId::Board]`. `menu_rank: 78`
  stands; the Board context has no rank collision at this rank
  (BoardColorEdit sits nearby — verify visually in the contract
  suite's registry ordering test, not by text).
- `menu.rs` `BOARD_CONTEXT` (menu.rs:231): `ActionId::LabelManage`
  inserted between `BoardColorEdit` and `BoardDelete` (management
  beside configuration, destructive last — the list's own comment).
  `object_entries` projects the row with its binding hint
  automatically; no menu code beyond the const.
- `keymap/presets.rs` — the shared-const pattern, **never re-pasted
  row blocks**: each of the three presets' const tables
  (`presets.rs:404` Standard, `:637` emacs, `:1105` vi) gains ONE row
  `(ActionId::LabelManage, ContextId::Board, &["Ctrl+L"])`. Candidate
  key `Ctrl+L` (the mnemonic consistency with the TaskDetail binding
  — the same action carries the same stroke across its contexts; the
  presets' comments record the free-at-Board check per preset). The
  keymap totality + conflict contract suite is the *verification*, not
  a hope: if any preset's Board context collides at contract time,
  the chunk picks the preset's own free mnemonic stroke and records
  the drift comment in that preset (the emacs `Ctrl+L` no-drift
  comment's pattern at TaskDetail). `ContextId::Labels` Escape rows
  are untouched — the roster-only mount lives in the same
  `Flow::Labels` context identity, the ladder composes unchanged.

### D-8 — the id-table pass widening (W-1's row-addressing; W-2's roster)

`models.rs` id pass (models.rs:2318–2335): `tables.task_labels` stays
task-subject-only (it is the assignment pair's table; the board mount
has no subject — `unwrap_or_default` already handles it). The
`tables.label_ids` resolution gains the subject-kind split:

```
LabelOverlaySubject::Task(t) → the existing derive (t's board)
LabelOverlaySubject::Board(b) → b's live roster
no Labels flow → empty (unchanged)
```

- Canonical order stands: the roster pass iterates `state.labels`
  values (the engine's `BTreeMap` id order — the same canonical order
  `task_labels` rides via `live_labels`' `BTreeSet`; no new ordering
  is invented). The overlay rows block (models.rs:2587) widens its
  match to the two subjects; board-subject rows carry
  `assigned: false` and no toggle affordance renders (D-3's flag).
  The armed delete, edit seeds, and commit funnels read the same
  `label_ids` — zero per-mount duplication.

### D-9 — the card chips: model path, order, overflow (W-3, US-3)

- **Model** (`models.rs` `TaskCardModel` construction, the ~998
  region): new fields
  `label-chips: [TaskLabelChipModel]` and
  `label-chips-overflow: int` on `TaskCardModel` (the slint struct in
  task-card.slint gains the same). New pure presentation fn:

  ```rust
  /// The card's label chips (UX R-8 US-3): the task's live labels in
  /// canonical order (the `BTreeSet`'s id order — `task_labels`' own
  /// order), each as (title, `swatch_hex` echo); beyond
  /// `LABEL_CHIP_CAP` the rest collapse into the overflow count (the
  /// compact leg hides the row entirely — the decoration grammar's
  /// tertiary posture).
  pub(crate) fn task_card_label_chips(
      state: &AppState, task_id: TaskId,
  ) -> (Vec<TaskLabelChipModel>, i32)
  ```

  Implementation: `task.live_labels()` (the `BTreeSet` — the canonical
  order; never strings, AC-1's letter), filter
  `state.labels.get(id)` non-deleted, map `{ title,
  color: swatch_hex(&label.color) }` (the lenient echo — AC-2),
  truncate to `LABEL_CHIP_CAP`, overflow = hidden count as the typed
  flag.
- **`LABEL_CHIP_CAP`**: `presentation.rs` const, `3` — beside
  `SWATCHES` (the shared-const pattern; the decoration row's width
  budget at the narrow breakpoint; the walkthrough judges the
  visual weight per origin §8's manual item).
- **Widget** (`task-card.slint`): a chip row in the text column
  *above* the tertiary deco row, but a **tertiary-class** row —
  `if (!root.compact)` exactly like `deco-row` (compact collapses it;
  the height contract never grows — AC-3). Each chip: small
  rounded `Rectangle` coloured by `chip.color` (transparent echo
  included) with the title in `Typography.badge`; when
  `label-chips-overflow > 0` a final `"+<n>"` mono chip (the number
  is model data, not composed UI text — no text-assertion surface).
  Plain-`Rectangle` host row (the round-6 R6-4 layout-severing rule
  the deco row documents) + `max-width: Theme.layout-severing-max-width`.
- **Surfaces**: `TaskCard` renders both board cards and today rows
  (today-view.slint:412) — one model path serves both (the census's
  Board + Today `D (US-3)` cells). The detail section's chips
  (label_chip_rows) are pre-existing and untouched (US-3's detail cell
  is `pre-existing`).
- `stack-view.slint` cards ride the same `TaskCard` — no per-view code.

### D-10 — the coverage-gate plan-cell normalization (origin §6's two `—` rows)

Baseline at `a440eaa5` (`scripts/ux-coverage.sh
2026-11-16-tag-color-ux-plan.md --gate`): 25 FOUND, 2 MISSING —
`W-1 action — (the forms' existing commit seams)` and
`W-3 command — (reason: pure display state)`. The script's
`is_absent` accepts only the *bare* `—` (ux-coverage.sh:271); the
origin plan wrote the reasons into the `—` cells instead of the
journey-steps column. This is a plan-vs-tool defect, fixed on the
origin plan in chunk 7's first commit: both cells become bare `—`
and their reasons move into the journey-steps cell (`W-1` steps cell
becomes `US-1/S1–S2 (the forms' existing commit seams — no new
action)`, `W-3` becomes `US-3/S1 (reason: pure display state)`).
The dispatch prompt's own law ("rows whose Commands cell is `—`
carry their reason in the journey-steps column") sanctions the
normalization; no row is weakened, no token lost — after stage 2
lands, every row is FOUND.

### D-11 — census cells ride the code commits (standing law c)

Origin §9's census deltas land in the commits that make them true:
`label.create/edit` Board cells `U` → `E (US-2)`, the dated
supersession note on the r3 board-surface scoping;
`label.roster_display` Board/Today → `D/E`; the
`task.label_chips_display` row minted; #839's closure note with the
C-3 supersession. Each commit message cites the owning plan's US/W
ids (the established convention).

## 2. Coverage mapping (W-1..W-3 → decisions)

| Row | Decisions | Test |
|---|---|---|
| W-1 (the colour field) | D-5, D-6, D-1, D-10 | `journeys.rs::us1_ac1_the_form_commits_the_exact_parsed_colour`, `us1_ac2_invalid_colour_renders_the_typed_echo_and_commits_nothing`; models tests (`colour-valid`/preview/swatch-agreement); domain corpus tests |
| W-2 (the roster-only mount) | D-3, D-4, D-7, D-8, D-10 | `journeys.rs::us2_ac1_board_menu_opens_the_roster_only_overlay`, `us2_ac2_roster_only_commits_the_exact_label_commands`, `us2_ac3_roster_only_escape_ladders`; keymap contract suite (widened contexts, no conflicts); adapter seam test (staged delete in roster-only mode — the r3 family's twin) |
| W-3 (the card chips) | D-9, D-10 | `presentation.rs::us3_ac1_the_card_carries_one_chip_per_live_label` (+ order, lenient echo, cap/overflow arithmetic); the existing `swatch_hex_mints_six_hex_and_renders_lenient_transparent` stands as the echo twin |

## 3. Implementation order & commit sequencing (TDD; every commit standalone)

1. `feat(domain): the Color::parse mint with the CLI's six-hex corpus`
   — domain unit tests first (accepted/rejected corpus, normalization,
   the lenient `new` unaffected), then the mint, then the CLI
   refactor (D-2) with its reduced wrapper tests; census: none yet.
   Cites US-1.
2. `feat(ui): the labels overlay's free-form colour field` — the
   panel surface + adapter draft/commit seams (D-5/D-6), the
   palette↔field agreement tests, the invalid-echo inert journey;
   `// slint:` comments; census: label.create/edit TaskDetail cell
   notes ride here. Cites US-1/W-1.
3. `feat(ui): the labels overlay's roster-only board mount` — the
   flow widening (D-3), the adapter arm + app-window board mount
   (D-4), the id-pass split + no-toggle composition (D-8), the menu
   row + registry widening + preset rows (D-7), the us2 journeys;
   focus-restoration census row; the staged-delete roster twin.
   Cites US-2/W-2.
4. `feat(ui): the card label chips with the compact overflow` — the
   presentation fn + widget row (D-9), the us3 presentation tests;
   census: roster_display Board/Today, the chips row, #839 closure
   with the C-3 note (D-11). Cites US-3/W-3.
5. `docs(plan): the tag-color origin §6 gate-cell normalization` (or
   folded into commit 1 — D-10) + any remaining census cells.
6. Gate steady state: `scripts/ux-coverage.sh
   .artifacts/plans/2026-11-16-tag-color-ux-plan.md --gate` all FOUND,
   then `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock; background + poll — the long-command discipline; the
   coverage trio stays with the dispatcher).

## 4. Test catalogue (origin §8 → concrete homes)

- **Domain** (`entities.rs` tests): the corpus verbatim (D-1); parse
  idempotence with `as_str`; the lenient `new` still accepts anything
  (the two constructors' contract split).
- **CLI** (`cli.rs`): the wrapper maps `Ok`→hex string and
  `Err(InvalidColor)`→clap usage error (typed `matches!`/`is_err`
  outcomes; the corpus lives only in domain).
- **Journeys** (`tests/journeys.rs`, the five origin §8 names):
  us1_ac1 exact-payload commit (palette path and field path agree);
  us1_ac2 invalid echo inert (no `CreateLabel`/`UpdateLabel` observed,
  form stays open); us2_ac1 the board menu row (and its key) opens
  the roster-only overlay — row-set composition asserts the toggle
  affordance absent (structural, not text) and the roster present;
  us2_ac2 exact label commands with the board subject (create/edit/
  staged delete); us2_ac3 Escape ladders per the r3 grammar.
- **Adapter/seam** (in-module): the staged delete in roster-only mode
  (the #840 guard twin); the commit funnel's board-subject
  resolution for both mounts.
- **Keymap** (contract suite): the widened `LabelManage` contexts
  bind per preset without conflicts; totality holds.
- **Fold/presentation** (`models.rs`/`presentation.rs` tests): chip
  set from `live_labels` (typed ids → titles, canonical order); the
  lenient transparent echo on a chip; the cap/overflow arithmetic
  (`LABEL_CHIP_CAP` + hidden count); `colour-valid` +
  `swatch-selected` agreement facts. Typed, never text (standing law
  a); no sleeps (law b).
- **Kiosk gates**: none new (origin §8's recorded absence — the
  kiosk refusal in the LabelsOpen arm is already journey-tested).

## 5. Risks

- **R-1 (flow-widening blast radius)**: `Flow::Labels { task }`
  matches in adapter/view_state/models/flow_table_tests — mechanical
  but wide; the enum variant keeps the flow identity (the ladder,
  reconcile, and the #840 guard read the flow, not the payload), so
  the r3 grammar survives unmetamorphosed. Mitigation: commit 3 is
  one coherent commit; the staged-delete twin journey guards it.
- **R-2 (Board-context key collision)**: `Ctrl+L` may collide in some
  preset's Board context; the contract suite decides — the fallback
  rule is written into D-7 so stage 2 is not blocked.
- **R-3 (slint field re-seed timing)**: pushing `form-colour` into a
  field the user is typing in would fight the caret; D-5 pins the
  mount-only-read + swatch-arm re-seed shape (the title field's
  proven rule). The us1 journeys catch a regression here.
- **R-4 (chip row layout)**: the deco row's constraint-merge trap
  (round-6 R6-4) applies verbatim to the chip row — the plain-
  Rectangle host + explicit max is copied, not reinvented.
- **R-5 (gate script strictness)**: D-10's normalization must land
  before the stage-2 gate can be green; it is commit 1's cargo.

## 6. Out-of-scope findings → backlog

- The `boards color` CLI positional also routes `parse_color` — its
  grammar moves with the refactor for free (same fn), but a board
  colour *picker* GUI parity review is out of scope here; noted, no
  issue needed (the existing board picker predates and composes).
- Nothing else outstanding at planning time; stage 2/3 open issues
  per the standing authorization if implementation surfaces findings.

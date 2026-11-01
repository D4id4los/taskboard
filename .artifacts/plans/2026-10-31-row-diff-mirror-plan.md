# [Plan] Row-diff mirror (#464) — chunk 2.1, backlog round 2

- **Date**: 2026-10-31 (planned 2026-11-01 after the batch-1 gate)
- **Kind**: stage-1 implementation plan for dispatch chunk **2.1**
  (#464, branch `perf/ui-row-diff-mirror`, batch 2 — opens when
  batch 1 fully merges; its gate, chunk 1.3, merged as PR **#563**
  on 2026-11-01, merge commit `013eb200`). TDD-first; no production
  code in this commit.
- **Origins** (normative, in precedence order): issue **#464** (the
  ask: `Model::row_data` returns `Option<T>` **by value** — a full
  clone of the current row per comparison, `VecModel`'s impl is
  `get(row).cloned()`; ~13 000 `TaskCardModel` clones at N = 10 000
  per refresh pass, including every unchanged row D9 exists to
  protect; the fix: a Rust-side mirror `Vec<T>` beside each
  persistent handle, the diff compares desired vs. mirror slice, and
  the mirror is push-pass-private); dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §4 chunk 2.1 (acceptance: **no `row_data` reads in the diff path**;
  stable-model identity invariants intact — repetition probe, F8/F11
  journeys; gate suite green; the mirror retains with the handle in
  the same pass so they can never disagree on membership);
  `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`
  §3.7 (the refresh lane 56.6 µs / 774 µs / 20.0 ms at N = 100 /
  1 000 / 10 000 — the pass whose diff half this trims; dev-machine
  reference, never a gate).
- **Composes with** the landed #463 guard (chunk 1.3): the guard's
  skip sits **above** the diff — `Adapter::refresh` short-circuits
  into the cheap tail (`enforce_due_buffer_invariant` +
  `push_settings`) and only a miss reaches `refresh_forced` →
  `models::push` (adapter.rs:634-702 at base). The mirror lives
  entirely after the guard: a skipped pass touches no mirror state.
- **Branch / worktree**: `perf/ui-row-diff-mirror` in
  `../taskboard-WT-perf-ui-row-diff-mirror`, branched off the
  fetched `origin/main` @ `84281108` (2026-11-01; local `main`
  @ `4d8be313` was behind — `git diff 4d8be313..84281108 --
  crates/taskboard-ui-slint/` is empty, so every seam below read at
  the main checkout is byte-identical at the branch base).
- **Status**: **READY** — all seams verified; the mutation audit
  (§5) is exhaustive over the branch base and finds **zero** sites
  that mutate a live model outside the two diff primitives; every
  open question has a recorded decision (§4).

---

## 1. Mission & binding constraints

Remove the clone-per-compared-row cost from the D9 row diff: the
diff's inner loop currently reads each live row back through
`model.row_data(i)` — a full `T` clone per row per pass — only to
run `PartialEq` and (usually) discard it. A mirror `Vec<T>` beside
each persistent handle turns the comparison into a slice compare and
leaves `set_row_data`/`push`/`remove` as the model's only traffic.

Binding, restated against the read seams:

1. **Identical diff semantics** — the mutation counts and the
   notification pattern are the transport contract the repetition
   probe (lib.rs:2650) and the F8/F11 journeys ride on. Same
   positional compare, same truncate-then-append tail, same counts.
   The mirror is an implementation swap under `diff_rows`, not a new
   behavior.
2. **No `row_data` reads in the diff path** (dispatch acceptance,
   grep-verified at stage 3). Today the grep over production `src/`
   has exactly one hit: `diff_rows` itself (models.rs:281) plus the
   `row_count` at models.rs:278 — both die with this chunk; every
   other hit is test code, which stays free to read models back.
3. **The mirror can never diverge from its model** (stage-3 review's
   THE-hazard): one missed mutation site corrupts every subsequent
   diff silently. The fence is structural (§4 D1): the handle and
   the mirror live in one private-field pair; the handle escapes
   only by shared clone; mutation is only expressible through the
   diff. Plus a debug-build witness (§4 D7) and the exhaustive
   audit (§5).
4. **Pruning keeps membership aligned** (dispatch: "retains mirror
   and handle in the same pass so they can never disagree on
   membership"): the two keyed maps' values become the pair, so the
   existing `retain` calls prune both at once — one call, one
   membership decision (§4 D4).
5. **Fresh-slot initialization is atomic** (stage-3 review's
   second hazard): a model installed without its mirror, or vice
   versa, breaks the first diff. The pair is minted in one
   get-or-create (§4 D3): both start empty in lockstep, which is
   exactly the model's own state (`VecModel::default()` is empty).
6. **Existing tests stay green unmodified or are strengthened, never
   weakened** (issue + dispatch): the repetition probe and the
   journeys are untouched byte-for-byte; the three diff unit tests
   in models.rs need a mechanical signature adaptation with every
   assertion verbatim — recorded as a strengthening decision in
   §4 D6, not a quiet edit.
7. **AGENTS §5 discipline**: typed/structural assertions only, no
   timing assertions (the win is argued from the removed clones, not
   timed), one behavior one test at the lowest layer, no quality
   ceilings.

## 2. The diff anatomy (seams read, at base `84281108`)

```
models.rs  PushModels (204-249)   — 16 single slots + 2 keyed maps + [T;4] + 7 hints
           ├─ diff_rows(273-295)  — THE primitive; reads row_data per row (281),
           │                        row_count once (278); the only production
           │                        model reads in src/
           ├─ push_rows(408-419)  — get-or-create (install property once) + diff;
           │                        14 call sites: palette(343), timers(484),
           │                        habits(523), notifications(584), banner(1349),
           │                        hints ×7 (792-851), settings ×2 (1016, 1026)
           ├─ 9 direct diff sites — bar_segments(644), column_tasks[stack](916),
           │                        columns(935), card_decorations[task](1047),
           │                        detail_tasks(1085), task_rule_presets(1201),
           │                        task_rules(1203), today_sections[i](1557),
           │                        today(1573) — all get_or_insert_with then diff
           ├─ push(610-748)       — the pass; sums mutations; traces (746);
           │                        prunes card_decorations via pass_tasks (742-745)
           ├─ push_board retain   — column_tasks retain (963-966)
           └─ fresh-per-pass models — date_grid (1198), flow-hint keys
                                     (string_model 192 → 1376): never diffed,
                                     no mirror
adapter.rs push_palette (1771)     — targeted per-keystroke pass, outside `push`
           push_settings (2470-71)— push_settings_rows/_boards; runs on EVERY
                                    refresh — the guarded tail included
           guard (634-702)        — #463: skip above the diff; mirror untouched
lib.rs     repetition probe (2650)— pinned Slint semantics; untouched
journeys.rs F8 (2040) / F11 (4094) — TouchArea/drag survival; untouched
```

`PushModels` is constructed exactly once (`PushModels::default()`,
adapter.rs:3039) and only ever reached as `&mut self.models` — its
slot types can change without a single adapter edit.

## 3. Non-goals

- No change to `push`'s derive half: desired rows are derived from
  the state snapshot exactly as before — the mirror changes only how
  desired rows are *applied and compared*.
- No per-field comparison machinery, no `PartialEq` skip for the
  decorations handle (§4 D6), no new dependency, no derive crates
  (AGENTS §2 type-driven policy: a hand-rolled pair type suffices).
- No refactor of the fresh-per-pass models (date grid, flow-hint
  keys, menu pages) — they are never diffed and stay as they are.
- No bench lanes, no instrument changes — the bench `refresh/{n}`
  lane picks the win up for free as a dev-reference, never a gate.
- No CHANGELOG entry (no user- or API-visible surface; the #463
  precedent).

## 4. Design decisions

### D1 — The mirror's home: one `MirroredRows<T>` pair per slot (the prompt's "per-slot `Vec<T>` beside each handle", made structural)

```rust
// models.rs, beside diff_rows
/// The persistent model handle of one surface plus its Rust-side
/// mirror (#464): the diff compares desired rows against the
/// mirror slice — never against the model (`Model::row_data`
/// returns by value: a full clone per compared row) — fires
/// `set_row_data` only where the row differs, and updates model
/// and mirror per mutation. The mirror is push-pass-private:
/// slint never sees it, the handle escapes only by shared clone,
/// and the fields are private to this module — mutating a live
/// model outside [`Self::diff`] does not compile.
struct MirroredRows<T> {
    model: Option<Rc<VecModel<T>>>,
    mirror: Vec<T>,
}

impl<T> Default for MirroredRows<T> {
    fn default() -> Self { Self { model: None, mirror: Vec::new() } }
}
```

Manual `Default` (no derived `T: Default` bound — `Option` and
`Vec` are unconditionally defaulting; `HintModels` and `PushModels`
keep deriving). The pair **is** "the mirror beside the handle" —
co-located in one slot rather than a parallel forest of ~30 sibling
fields, which is the reading that makes the review's two hazards
(atomic init, aligned pruning) unrepresentable rather than merely
disciplined. `PushModels` fields become:

| Slot | Today (models.rs) | Becomes |
|---|---|---|
| `bar_segments` 206 | `Option<Rc<VecModel<BoardSegmentModel>>>` | `MirroredRows<BoardSegmentModel>` |
| `columns` 207 | `Option<Rc<VecModel<StackColumnModel>>>` | `MirroredRows<StackColumnModel>` |
| `column_tasks` 211 | `BTreeMap<StackId, Rc<VecModel<TaskCardModel>>>` | `BTreeMap<StackId, MirroredRows<TaskCardModel>>` |
| `card_decorations` 217 | `BTreeMap<TaskId, Rc<VecModel<TaskDecoration>>>` | `BTreeMap<TaskId, MirroredRows<TaskDecoration>>` |
| `detail_tasks` 218 | `Option<Rc<VecModel<TaskCardModel>>>` | `MirroredRows<TaskCardModel>` |
| `task_rules` / `task_rule_presets` 219-220 | `Option<Rc<VecModel<…>>>` | `MirroredRows<…>` |
| `banner` / `notifications` / `timers` / `habits` / `palette` 221-233 | `Option<Rc<VecModel<…>>>` | `MirroredRows<…>` |
| `today_sections` 237 | `[Option<Rc<VecModel<TodayRowModel>>>; 4]` | `[MirroredRows<TodayRowModel>; 4]` |
| `today` 238 | `Option<Rc<VecModel<TodaySectionModel>>>` | `MirroredRows<TodaySectionModel>` |
| `settings_rows` / `settings_boards` 239-242 | `Option<Rc<VecModel<…>>>` | `MirroredRows<…>` |
| `hints` ×7 (`HintModels`) 243/255-263 | `Option<Rc<VecModel<SharedString>>>` | `MirroredRows<SharedString>` |
| `pass_tasks` 248 | `BTreeSet<TaskId>` | unchanged |

API (three faces, all `pub(crate)`-visible only inside the module's
callers — the type itself stays module-private):

```rust
impl<T: PartialEq + Clone + 'static> MirroredRows<T> {
    /// The push_rows shape: get-or-create the handle, installing it
    /// into its property exactly once via `install`, then diff.
    fn push(&mut self, install: impl FnOnce(ModelRc<T>), desired: &[T]) -> usize;

    /// The diff core: compare `desired` against the mirror, mutate
    /// model + mirror per difference; returns the mutation count.
    fn diff(&mut self, desired: &[T]) -> usize;

    /// The struct-embedding sites' shared clone of the handle
    /// (`StackColumnModel.tasks`, `TodaySectionModel.rows`,
    /// `StackDetailModel.tasks`, the bar/detail structs).
    fn handle_or_insert_with(&mut self, make: impl FnOnce() -> Rc<VecModel<T>>)
        -> Rc<VecModel<T>>;
}
```

Borrow shape of `diff`: `let model = self.model.get_or_insert_with(…)`
borrows one field; `diff_rows(&mut self.mirror, model, desired)` takes
the other — disjoint field borrows, one small body. `push` installs on
first creation (the `is_none` check + closure, today's push_rows shape
verbatim), then delegates to `diff`.

### D2 — The new diff algorithm

The free function keeps its name and gains the mirror parameter (the
unit tests' entry point, §6):

```rust
/// Positional diff of `desired` against `mirror` (the model's exact
/// content, #464): `set_row_data` only where the row differs over
/// the common length — compared slice-to-slice, never through
/// `row_data`'s by-value clone — then the tail is truncated, then
/// appended. Model and mirror are updated per mutation, in the same
/// statement. No LCS — a mid-list reorder stays a positional set
/// cascade over the suffix (the repetition-probe pins its shape).
fn diff_rows<T: PartialEq + Clone + 'static>(
    mirror: &mut Vec<T>,
    model: &VecModel<T>,
    desired: &[T],
) -> usize {
    debug_assert_eq!(
        mirror.len(),
        model.row_count(),
        "mirror/model divergence: a live model was mutated outside the diff",
    );
    let mut mutations = 0;
    let current = mirror.len();              // was model.row_count()
    let common = current.min(desired.len());
    for (i, row) in desired[..common].iter().enumerate() {
        if mirror[i] != *row {               // was row_data(i).as_ref() != Some(row)
            model.set_row_data(i, row.clone());
            mirror[i] = row.clone();         // lockstep, same statement
            mutations += 1;
        }
    }
    while mirror.len() > desired.len() {     // was model.remove(desired.len()) loop
        model.remove(desired.len());
        mirror.pop();
        mutations += 1;
    }
    for row in &desired[common..] {
        model.push(row.clone());
        mirror.push(row.clone());
        mutations += 1;
    }
    mutations
}
```

Semantics identical to today: `mirror[i] != *row` is the same
`PartialEq` comparison `row_data(i).as_ref() != Some(row)` performed
(the `Option` wrapper only encoded "index in range", which the slice
bounds give for free); `remove(desired.len())` in a loop pops the
model tail in order, and `mirror.pop()` is its exact image. The
model-side traffic (`set_row_data` / `remove` / `push`) is
byte-for-byte the old sequence — the repeaters see nothing new.

### D3 — Initialization: a fresh slot is minted empty on both sides, atomically

`MirroredRows::default()` = no model + empty mirror. The first
`push`/`handle_or_insert_with` creates `Rc::new(VecModel::default())`
(empty) and leaves the mirror empty — both sides start at length 0 in
the same statement chain, before any diff can observe them. There is
no state in which a model exists and its mirror does not: the fields
are written only in `get_or_insert_with` (one constructor) and in
`diff` (both sides per mutation). The first diff then finds
`common == 0` and appends every desired row — the exact mutation
sequence today's fresh slot produces.

### D4 — Pruning: the map value is the pair, so one `retain` prunes both

- `column_tasks` (models.rs:963-966):
  `models.column_tasks.retain(|stack, _| stack_ids.contains(stack))`
  — unchanged code, new meaning: the retained value now *is*
  handle+mirror, so membership decisions cannot split. A returning
  stack gets a fresh pair via `entry().or_default()` (both empty, D3).
- `card_decorations` (models.rs:742-745): the `pass_tasks` retain —
  same one-call pruning of the pair; `pass_tasks` itself unchanged.
- `today_sections` is fixed arity (its own comment already notes no
  pruning can fire) and the single slots have no pruning.

### D5 — The mutation audit: zero rogue sites; the fence for future ones

The exhaustive audit over the branch base (grep over `src/` for
`set_row_data`, `row_data`, `push`/`remove`/`insert` on models, and
every `PushModels` field touch) is the table below. Conclusion: every
mutation of a live model flows through `diff_rows` (9 direct sites)
or `push_rows` (14 sites) — both become mirror-updating paths by
construction; the remaining model constructions are fresh-per-pass
and never diffed. **No site mutates a live model outside the two
primitives**, so no call site needs a hand-written mirror update —
the audit's "refactor through it" column is empty at this base.

| # | Site (models.rs unless noted) | Surface / `T` | Path today | Action |
|---|---|---|---|---|
| 1 | 644 | bar segments `BoardSegmentModel` | direct `diff_rows` | `handle_or_insert_with` + `diff` |
| 2 | 916 | per-column cards `TaskCardModel` | direct (map entry) | entry→pair, `handle_or_insert_with` + `diff` |
| 3 | 935 | board columns `StackColumnModel` | direct | `handle_or_insert_with` + `diff` |
| 4 | 1047 (`decorations_model`) | per-card decorations `TaskDecoration` | direct (map entry) | entry→pair, `diff` |
| 5 | 1085 | stack-detail cards `TaskCardModel` | direct | `handle_or_insert_with` + `diff` |
| 6 | 1201 | rule presets `SharedString` | direct | `handle_or_insert_with` + `diff` |
| 7 | 1203 | task rules `RuleRowModel` | direct | `handle_or_insert_with` + `diff` |
| 8 | 1557 | today sections `TodayRowModel` | direct | `handle_or_insert_with` + `diff` |
| 9 | 1573 | today section structs `TodaySectionModel` | direct | `handle_or_insert_with` + `diff` |
| 10-23 | 343, 484, 523, 584, 792-851 (×7), 1349, 1016, 1026 | palette/timers/habits/notifications/hints/banner/settings | `push_rows` | `push_rows` body becomes `slot.push(install, desired)` — call sites untouched |
| 24 | adapter.rs:1771 (`push_palette` targeted pass) | palette `PaletteRowModel` | `push_palette` → `push_rows` | none — rides #10 |
| 25 | adapter.rs:2470-71 (`push_settings`, guarded tail included) | settings `ConnectionRow` / `SettingsBoardRow` | `PushModels::push_settings_rows`/`_boards` → `push_rows` | none — rides #22/#23 |
| 26 | 1198 (`date_grid`), 1376 (`string_model`), adapter.rs:389 (menu pages) | fresh-per-pass models | never diffed | none — explicitly out (§3) |

The fence for future sites: `MirroredRows`' fields are private to
models.rs and the `Rc<VecModel<T>>` escapes only by shared clone, so
a mutation of a live model from **outside** this module stops
compiling; inside the module, the debug witness (D7) catches it at
the first diff. This sentence is the invariant `diff_rows`' doc
comment carries forward.

### D6 — The optional `PartialEq` question: keep plain `PartialEq` (decided)

The issue's optional stage-1 lever — skip `PartialEq`'s
decorations-handle comparison by comparing fields — is **declined**.
`ModelRc`'s `PartialEq` is handle identity (the repetition probe pins
it, lib.rs:2890-2900): one pointer compare per card row, no content
walk — there is no measured or argued cost to skip, and a hand-rolled
field comparison would fork the equality semantics the generated
struct, the column-diff test, and the card-diff test all pin. The
mirror changes the *transport* of the comparison, never its meaning.
Keep it simple (the issue's own tiebreaker).

### D7 — The divergence witness: `debug_assert` at the diff's entry

`debug_assert_eq!(mirror.len(), model.row_count(), …)` at the top of
`diff_rows` (D2). O(1) on `VecModel`, debug-only, zero release cost —
and it turns the silent-corruption hazard into a loud panic in every
test run (nextest runs debug) the moment any future path mutates a
model behind the mirror. The witness is itself tested (§6 T3). It is
a safety net, not the fence — D1's privacy fence is the primary
guarantee.

### D8 — Composition with #463 and the settings tail

The guard's skip is above the diff: a guarded pass runs
`enforce_due_buffer_invariant` + `push_settings` only.
`push_settings` → `push_settings_rows`/`_boards` → `push_rows` →
`MirroredRows::push` — **the settings mirrors are updated on guarded
passes too**, exactly as the settings models are today. Same for the
targeted `push_palette` pass (adapter.rs:1771). Both ride the same
primitive, so there is no second mirror-maintenance path to forget.
The mirror holds no cross-pass intent — it is a content snapshot,
rebuilt-for-free by the diff whenever the pass runs — so #463's
"pass-private state that never crosses passes" applies verbatim.

### D9 — The existing diff unit tests: signature-adapted, assertions verbatim (decided)

`diff_rows`' signature changes, so the three models.rs unit tests
(`diff_rows_mutates_exactly_the_differing_rows` 1811,
`a_column_row_with_unchanged_scalars_and_handle_compares_equal` 1835,
`an_unchanged_card_compares_equal_across_refresh_passes` 1869) get a
mechanical adaptation: each constructs a `mirror` vec alongside the
model (seeded equal — the state a lockstep slot is always in) and
passes `&mut mirror` as the new first argument. **Every existing
assertion stays byte-identical** — including the `row_data` /
`row_count` verification reads, which are test-side reads and
explicitly allowed (acceptance bans reads in the *diff path*, not in
tests). Each adaptation then adds one assertion the old shape could
not make: the mirror's own length/content matches the model after the
diff (strengthening). The repetition probe (lib.rs) and the journeys
are untouched. If stage 2 finds itself editing an existing assertion,
that is the dispatch's "weakening is a deviation to revert" — stop
and re-read §1.6. There is no compatibility shim for the old
signature: `diff_rows` is `pub(crate)`, models.rs holds its only
users (grep-verified), and a shim would keep a clone-per-row path
alive.

## 5. TDD plan (test-first; all in models.rs's test module unless noted)

**T0 — adapt the three diff unit tests (mechanical, first)**: the
signature swap of D9, assertions verbatim, plus the mirror-tracking
assertions. Red first (new signature), green with D2. These carry the
*semantics* contract: counts, positional compare, truncate/append,
reorder cascade (models.rs:1811-1829), column/card handle-identity
equality (1835-1903).

**T1 — `a_fresh_slot_installs_once_and_diffs_in_lockstep`** (the
fresh-slot pin, load-bearing):

1. `MirroredRows::default()`, `push(install, &rows)` with an
   install-closure invocation counter (a `Cell<usize>` in the test):
   counter == 1 (install exactly once), mutations == `rows.len()`,
   `row_count() == rows.len()`.
2. The identical `push` again: mutations == **0** — the mirror pin:
   an untracked mirror (still empty) would report `rows.len()`
   mutations. This is the assertion that fails if `diff` reads the
   model or forgets the mirror.
3. One changed row: mutations == **1**; the changed row reads back
   through `row_data`.
4. `handle_or_insert_with` after a push: returns the installed
   handle (same `Rc` identity via `Rc::ptr_eq`) — the struct sites'
   contract.
   Load-bearing proof: delete the `mirror[i] = row.clone()` updates
   (any of the three) → step 2's zero or step 3's one fails.

**T2 — `the_mirror_tracks_truncation_and_append_in_lockstep`**:
grow (mutations == appended), shrink to a prefix (mutations ==
removed; `row_count() == mirror.len()`), re-grow with a changed
returning row (exactly the two tail rows mutate). Pins the
`remove(desired.len())` ↔ `pop()` correspondence that the loop
rewrite (D2) touches.

**T3 — `a_diverged_mirror_panics_at_the_next_diff_in_debug_builds`**
(`#[should_panic(expected...)]`-shaped, message-free per AGENTS §5 —
assert the panic happens, via `should_panic`): push once, then mutate
the live model behind the diff — `handle_or_insert_with(...).push(x)`
clones the shared `Rc`, so the model grows while the mirror does not
(the exact "rogue site" shape of review's hazard) — then `diff` must
panic. Pins D7 stays armed. (Test-side `row_data`/`row_count` reads
are fine everywhere.)

**T4 — `a_pruned_column_slot_returns_as_a_fresh_pair`** (the prune
pin, via the existing `push_pass` rig pattern of the `today_pipeline`
module): a two-stack board pushes twice (second pass zero mutations —
assert on `models.column_tasks` map sizes structurally: 2 entries);
a third pass over a one-stack state retains down to 1 entry (the pair
went with it — membership cannot split: one map, one retain); a
fourth pass with the stack back has 2 entries again and the window's
board model reads back the returning column's cards (structural
`row_count`, typed content). Companion assertion in the same test's
rig: a task vanishing from every surface leaves `card_decorations`
without an entry for it (the `pass_tasks` retain, now pruning pairs).
No mutation-count gymnastics — structural facts only (AGENTS §5).

**T5 — routing at higher layers: no new tests** (one behavior, one
test, lowest layer). The guarded-tail settings delivery is already
pinned by the merged #463 suite (the unchanged-refresh-still-pushes-
liveness test drives `push_settings_rows` through the mirror on a
guarded pass — routing, not a new behavior). The repetition probe
(lib.rs:2650) and the F8/F11 journeys (journeys.rs:2040, 4094) are
the transport-level pins and must pass **unmodified**: `git diff
origin/main` over `tests/` and over lib.rs's probe module must be
empty (stage 3 verifies).

Test-order note: T0's adaptation compiles only after D1/D2 exist —
the red phase is the local watch-it-fail before the standalone
commit; every commit on the branch builds green (AGENTS §8 rebase
hygiene). Expected shape: **one standalone implementation commit**
(`perf(ui-slint): the row-diff mirror`) plus this plan commit; a
second fix commit only if review finds something (amend/reword, never
a duplicate-subject follow-up).

## 6. Implementation checklist (stage 2, in order)

1. D1's `MirroredRows` type + D2's `diff_rows` rewrite + the
   `PushModels` field table swap (mechanical: 16 slots, 2 maps,
   `HintModels` ×7) + `push_rows`'s body collapsing into
   `slot.push(...)` + the 9 direct sites per the §5 table. T0-T3
   written alongside, watched red where feasible, green after.
2. T4; the §5 table's rows 24/25/26 verified untouched (grep: no new
   `row_data`/model-mutation hits outside the diff and tests).
3. `git grep -n "row_data" crates/taskboard-ui-slint/src/` — the only
   non-test hit allowed: none (D2's doc-comment mention is prose,
   keep it out of code). Also grep `row_count`: diff path must not
   read it (the witness's debug read is the only diff-path model
   call, debug-gated).
4. Full gate suite through `scripts/run-quality-gates.sh` (AGENTS
   §10) from the worktree: fmt, clippy `-D warnings`, nextest,
   doc-tests, deny, the llvm-cov sequence with quarantine + trailing
   `rm`.
5. No CHANGELOG (§3). No push, no PR — stage 3 ships.

## 7. Acceptance (dispatch plan §4 chunk 2.1, restated & operationalized)

- **No `row_data` reads in the diff path**: checklist step 3's grep —
  zero production hits (was: exactly one, models.rs:281).
- **Stable-model identity invariants intact**: repetition probe +
  F8/F11 journeys green with `git diff origin/main` empty over their
  files; the diff unit tests' assertions byte-identical (D9).
- **Mirror/model divergence fenced**: D1 privacy fence + D7 witness
  (T3 proves it armed) + §5's exhaustive zero-rogue-site audit.
- **Pruning aligned by construction**: D4 — one retain per map, the
  pair is the value (T4).
- **Gate suite green** (§10 sequence), run through the lock wrapper.

## 8. Risks & traps

1. **A missed mutation site corrupts every later diff silently** —
   THE hazard. Mitigations, in order: the D1 fence (outside models.rs
   it cannot compile), §5's audit (zero rogue sites at this base),
   D7's debug witness (any future in-module miss panics the next test
   run). Residual risk is a future *in-module* site added between
   audits — the witness is the standing tripwire, and `diff_rows`'s
   doc comment carries the invariant sentence.
2. **The settings tail and the targeted palette pass are the easy
   forget-points** — they run outside `models::push` (guarded tail,
   per-keystroke). By construction they ride `push_rows`; a stage-2
   implementation that instead hand-maintains mirrors at call sites
   has already diverged from this plan — the primitive owns the
   mirror, no call site ever touches it.
3. **Memory: one extra `Vec<T>` per live slot** — the mirror is a
   second copy of every live row (same magnitude as the model's own
   storage; ~N card rows per surface). Bounded by the live row set,
   kiosk-scale modest; the win is the ~13 000 clones per pass at
   N = 10 000. If a future profile says otherwise, the mirror is one
   struct to re-think — say so in a report, don't hand-roll around
   it.
4. **Don't "simplify" `debug_assert` away or into `assert`**: debug
   keeps release zero-cost; `assert` would tax every row mutation in
   production for a hazard the fence already prevents.
5. **The witness reads `model.row_count()` on the diff path** —
   debug-gated, so the grep acceptance stays honest: grep counts
   *code* hits, and the one remaining call sits behind
   `debug_assert!`. Stage 3's grep should eyeball, not just count.
6. **Derived `Default` would add a spurious `T: Default`** on
   `MirroredRows` (and might not even compile for the generated
   structs) — the manual impl (D1) is load-bearing; don't "tidy" it
   into a derive.
7. **The adapted unit tests must not drift**: their assertions are
   the semantics contract the journeys lean on indirectly; any edit
   beyond D9's mechanical shape is a deviation (revert), and a
   failing adapted test is a bug in the mirror, not in the test
   (AGENTS §5).
8. **Guard interaction is read-only**: never prewarm or persist
   mirror state outside a pass (D8) — a mirror written by a skipped
   pass would desync from a model the pass never touched.
9. **`[MirroredRows<TodayRowModel>; 4]`** relies on `Default` for
   arrays of any N (const-generic impl) plus the manual pair Default —
   if a toolchain bump ever complains, impl `Default` for the array
   slot explicitly rather than reaching for `Option` again.

## 9. Estimated diff size

One crate (`taskboard-ui-slint`), effectively one file:

- models.rs production: ~90-110 lines touched (type + table swap +
  primitive rewrite + 9 sites + push_rows body), mostly
  line-replacements, no call-site churn in adapter.rs / lib.rs /
  journeys (verified: `PushModels` has exactly one constructor and
  one holder, adapter.rs:3039/208; `push`, `push_palette`,
  `push_settings_rows/_boards` signatures unchanged).
- models.rs tests: ~40 lines adapted (T0) + ~120-150 lines new
  (T1-T4).
- **Total ≈ 250-300 changed lines**, expected as one standalone
  `perf(ui-slint)` commit on `perf/ui-row-diff-mirror` (plus this
  `docs(plan)` commit).

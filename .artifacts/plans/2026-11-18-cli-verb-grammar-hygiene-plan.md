# [Plan] Tier-2 chunk P3 — CLI verb & grammar hygiene (#874 + #873)

- **Date**: 2026-11-18 (stage-1 planning agent)
- **Kind**: chunk plan (three-stage dispatch, chunk P3 of the tier-2
  round) — TDD-first, no implementation in the planning stage.
- **Branch**: `fix/cli-verb-grammar-hygiene` (worktree
  `../taskboard-WT-fix-cli-verb-grammar-hygiene`, branched off
  `origin/main` at `d9651047` after a clean `worktree-doctor.sh` run —
  free 179 G / floor 25 G, MemAvailable 21 G / floor 8 G).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §4 (normative);
  recommendation report
  `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
  §Tier-2; issues #874 (grammar collapse) and #873 (date-grammar
  leniency decision).
- **Standing laws**: typed outcomes only (no help-text assertions);
  never weaken a correct test — the grammar tests' recorded leniency
  comment on `sync_directions_follow_the_pinned_rules` is *superseded
  by the recorded decision below*, rewritten to cite it, not deleted;
  gate = pre-commit hooks + `scripts/run-quality-gates.sh cargo
  nextest run --workspace` (flock-safe; commit landed work before any
  wait; defer on lock contention per the #879/#894 rule).
- **UX coverage**: no UX journey plan in origin (dispatch plan §4) —
  CLI grammar hygiene; help text unchanged in shape (the `--to-deck`
  line disappears with the flag; no new verb, badge, view, or
  setting).

## 1. Verified facts (anchored, `origin/main` @ d9651047)

- **F1 (the dual grammar)** `BoardsCmd::Move`
  (`crates/taskboard-app/src/cli.rs:274-296`) carries a required,
  non-multiple clap `ArgGroup` `move_direction` over `--to_files`,
  `--to-deck` (`to_deck: bool`), `--undo`, and `--to-connection
  <name>` (`Option<String>`). clap's non-multiple group already makes
  any two-member invocation a typed clap error (`ErrorKind::
  ArgumentConflict`); `boards_move_demands_exactly_one_direction`
  (`cli.rs:529`) pins exactly that.
- **F2 (the dead arm)** The dispatch arms
  (`crates/taskboard-app/src/commands/mod.rs:1133-1199` one-shot,
  `:2158-2178` REPL) branch on `to_connection` first (the rebind
  twin), then compute `move_direction(to_files, to_deck)`
  (`mod.rs:2010-2017`) and refuse `ToDeck` with
  `MoveError::ReverseFenced` — one-shot only (`mod.rs:1172-1177`);
  the REPL arm feeds `ToDeck` straight into `run_on`, whose
  `run_until` guard (`board_move_ceremony.rs:590-592`) refuses it.
  The `undo` flag is destructured-but-ignored in both arms — `Undo`
  is `move_direction`'s `_` fallback (with the group required and
  `--to-deck` dead, `--undo` is the only flag that lands there).
- **F3 (the ceremony's refusals cover the reverse direction)** The
  landed rebind (`boards.rs:1383-1430` `rebind_deck_to_deck` + the
  settings twin `ui_requests.rs:1760`) refuses typed *before any
  IO*: unknown connector (`CommandError::UnknownConnector`), non-Deck
  target (`RebindTargetNotDeck`), a board whose binding has no Deck
  half (unbound / files-bound / raced: `RebindRefused`,
  `mod.rs:486-499`). The engine verb `StateCommand::RebindBoard`
  (the files-sync-status plan D-11) is atomic and preserves the
  remote Deck id. No `ReverseFenced`-class refusal exists or is
  needed on this path.
- **F4 (the fenced test)** `crates/taskboard-app/tests/cli_verbs.rs`
  ~1790 pins `run_on(.., MoveDirection::ToDeck, ..)` → error kind
  `move-reverse-fenced`. `MoveError::ReverseFenced` +
  `MoveDirection::ToDeck` also appear at
  `board_move_ceremony.rs:62-65/128/543-547/590`.
- **F5 (the docs pins)** `docs/boards-move-runbook.org` §5 ("The
  reverse direction is fenced … ruling Q4, 2026-11-07") and
  `docs/architecture.org:766` ("`--to-deck` is a typed refusal,
  ruling Q4"). The r8 plan's D-11/§1 references (`--to-deck` stays
  fenced) are historical plan text — not updated (plans are
  point-in-time artefacts).
- **F6 (the date grammar)** `due_edit.rs::parse`
  (`crates/taskboard-ui-slint/src/due_edit.rs:112-140`) accepts the
  two documented absolute forms (`YYYY-MM-DD`, `YYYY-MM-DD HH:MM`;
  r2 D25) via chrono `strptime`/`FromStr`; strptime accepts
  non-padded day/month (`2030-01-3` → day 3). `parse_stamp`
  (`:183-197`) re-checks the bare form via `NaiveDate::FromStr`
  (padded-only) — the existing comment at `:188-192` already records
  how the two interact. The recorded leniency comment sits at the
  end of `sync_directions_follow_the_pinned_rules`
  (`:1262-1265`). The quick-list filter (`quick_list_matches` /
  `filter_digits`, `:341-360`) matches against the canonical
  `%H%M` rendering — it reads *stored/echoed* values, which are
  always padded regardless of input padding.
- **F7 (the CLI twin grammar)** `parse_due` (`cli.rs:594-608`) parses
  the same two absolute forms with the same chrono leniency — a
  second implementation of the same grammar (the D25 GUI grammar and
  the CLI `--due` grammar have drifted into parallel files; see the
  backlog issue in §6).

## 2. Recorded decisions

### D-1 — The collapsed move grammar (#874)

`boards move <board>` carries exactly one direction flag:

- `--to-files` — the files ceremony (unchanged);
- `--undo` — the undo window (unchanged semantics; see D-3);
- `--to-connection <name>` — the Deck-to-Deck rebind (the landed
  twin, unchanged).

**`--to-deck` is deleted from the grammar** (the flag, its doc line,
and the group membership). No deprecation shim: the flag never
worked (every invocation refused), so there is no working invocation
to preserve — an unknown-flag clap error (`ErrorKind::
UnknownArgument`) is the honest outcome.

**Both-targets invocation** (e.g. `--to-files --to-connection alpha`):
stays a clap `ArgGroup` conflict — typed, exit 2, asserted via
`ErrorKind::ArgumentConflict`. No new `AmbiguousFlags` error variant
is minted: clap already produces the typed refusal at the parse
boundary, and a post-parse ambiguity error would be unreachable code
(the group is non-multiple). Recorded decision: reuse the parse
boundary, do not duplicate it.

### D-2 — The fence's replacement semantics (#874)

`MoveDirection::ToDeck` and `MoveError::ReverseFenced` (kind
`move-reverse-fenced`) are **deleted**, together with `run_until`'s
early guard (`board_move_ceremony.rs:590-592`) and the one-shot
dispatch refusal (`mod.rs:1171-1177`). `MoveDirection` becomes
`{ ToFiles, Undo }` — a total enum with no dead variant.

The reverse direction is **served, not fenced**: Deck-to-Deck moves
go through the rebind ceremony, whose own precondition set (F3)
already refuses typed everything the fence used to stand in for
(unknown connector, non-Deck target, no Deck half). A files→Deck
*tier* move has no verb at all — "absence of grammar", not a runtime
refusal; the runbook (D-2's semantics) states this. No replacement error variant
is minted: an error type that cannot be constructed by any code path
is exactly the dead weight this chunk removes.

`tests/cli_verbs.rs`'s `move-reverse-fenced` assertion (F4) is
**removed as superseded by this recorded decision** — the tested
behavior is deliberately deleted; the test's replacement (the new
grammar pin, §4) tests what the fence became. This is the [L](c)
supersession carve-out, not a weakened test.

### D-3 — The direction mapping becomes explicit

`move_direction(to_files: bool, to_deck: bool)` (F2) is deleted. The
collapsed dispatch reads the flags directly:

```rust
// exactly one flag is present (clap's required non-multiple group)
let direction = if undo {
    MoveDirection::Undo
} else {
    MoveDirection::ToFiles
};
```

`--undo` is now named in the mapping instead of being the implicit
`_` fallback — the grammar's documented meaning and the code's
meaning coincide. The `to_connection` branch keeps its precedence
(the rebind twin fires whenever the flag carries a value; the group
makes the combination with the other flags unrepresentable).

### D-4 — The date-grammar leniency: normalize + document (#873)

The round-level decision (**normalize + document**) is **upheld** —
the grammar tests reveal no conflict that would justify overturning
to refuse.

- **Shape**: no new normalization function. The strptime hit already
  yields a canonical `chrono::NaiveDate`; every stored value and
  every echo (`format("%Y-%m-%d")`, `format_entry`) is padded by
  construction. Canonicalization is inherent in the type — the
  "normalization" the decision requires is exactly: accept
  non-padded input at the parse boundary, commit the canonical
  `NaiveDate`. The quick-list filter's digit-prefix assumption (F6)
  reads canonical renderings and is unaffected — the plan records
  this so stage 2 does not "fix" the filter.
- **Documentation** (the A1 documented-leniency rule): `parse` and
  `parse_stamp` each gain a *Deliberate leniency boundary* doc
  paragraph (the `parse_time_lenient` precedent, `due_edit.rs:275`),
  stating: the two absolute forms (r2 D25) are accepted with
  non-padded day/month fields and canonicalized to the padded
  stored form; non-padded input is neither refused nor echoed
  verbatim. `parse_due`'s doc (`cli.rs`) gains the same paragraph —
  the CLI twin keeps the identical chrono behavior (no second
  decision for the same grammar).
- **The recorded comment**: the leniency note closing
  `sync_directions_follow_the_pinned_rules` is **rewritten** (not
  deleted) to cite this decision: "the grammar's documented leniency
  (§ D-4 of the cli-verb-grammar-hygiene plan): non-padded fields
  parse and canonicalize". The test's assertions are unchanged —
  they were correct then and remain correct.

## 3. Implementation sketch (stage 2)

1. **CLI** (`cli.rs`): delete `--to-deck` + its doc line; update the
   `Move` variant doc (the `--to-deck is a typed refusal` sentence
   goes); rewrite `boards_move_demands_exactly_one_direction` and
   add the new parse pins (§4 M1). Delete `move_direction`
   (`mod.rs:2010`) and the one-shot fence block; make the mapping
   explicit (D-3) in both dispatch arms; delete the now-unused
   `to_deck` destructuring.
2. **Ceremony** (`board_move_ceremony.rs`): delete `ToDeck` from
   `MoveDirection`, `ReverseFenced` from `MoveError` + `kind()`,
   and the `run_until` guard. Compile errors enumerate every
   remaining touchpoint (F2/F4).
3. **Tests** (`tests/cli_verbs.rs`): replace the fenced test with
   the §4 M3 additions (the rebind refusals are already pinned by
   the r8 plan's tests — verify, do not duplicate; one behavior,
   one test, lowest layer).
4. **due_edit** (`due_edit.rs`): the D-4 doc paragraphs + the
   rewritten recorded comment + the §4 M4 matrix. `cli.rs`:
   the `parse_due` doc paragraph.
5. **Docs**: `docs/boards-move-runbook.org` §5 rewritten (D-2's
   semantics);
   `docs/architecture.org:766` clause updated. Census: grep for
   `to-deck` / `move-reverse-fenced` in `docs/ux-surface-census.org`
   and update the cell in the same commit if pinned (none found at
   plan time, F5 — verify at stage 2).
6. Diff confined to: `crates/taskboard-app/src/{cli.rs,
   commands/mod.rs, board_move_ceremony.rs,
   tests/cli_verbs.rs}`, `crates/taskboard-ui-slint/src/due_edit.rs`,
   two `docs/*.org` files, this plan. No domain/state/storage/sync
   diffs (the ceremony error enum lives in `taskboard-app`).

## 4. Test matrix

**M1 — clap parse pins (`cli.rs`, extending
`boards_move_demands_exactly_one_direction`; assert
`matches!(err.kind(), ErrorKind::…)` — typed outcomes, never text):**

| invocation | outcome |
|---|---|
| `--to-files` | ok |
| `--undo` | ok |
| `--to-connection alpha` | ok |
| (none) | err — group required |
| `--to-files --undo` | err — `ArgumentConflict` |
| `--to-files --to-connection alpha` | err — `ArgumentConflict` |
| `--undo --to-connection alpha` | err — `ArgumentConflict` |
| `--to-deck` | err — `UnknownArgument` (the flag no longer exists) |

**M2 — ceremony unit (`board_move_ceremony.rs` or its existing test
home):** `MoveDirection` has exactly `{ToFiles, Undo}`; no
`ReverseFenced` variant exists (deletion is the assertion — the
compiler pins it; no test writes to the void). The existing
`run_until` refusals (`NotDeckBound` etc.) are untouched.

**M3 — integration (`tests/cli_verbs.rs`):** the `move-reverse-fenced`
assertion is replaced by: the rebind path's typed refusals on the
CLI route (`UnknownConnector` / `RebindTargetNotDeck` /
`RebindRefused`) resolve as error kinds — *only* where the r8
plan's landed tests do not already pin them (verify first; no
duplication).

**M4 — date grammar (`due_edit.rs` unit matrix):**

| input | outcome |
|---|---|
| `2030-01-3` | parses, canonical `NaiveDate(2030-01-03)`, echoes `2030-01-03` |
| `2030-1-03` | parses, canonical `2030-01-03` |
| `2030-1-3` | parses, canonical `2030-01-03` |
| `2030-01-03` | unchanged (already canonical) |
| `2030-01-` | still junk (the partial-prefix pin stays) |
| `2030-1-3` through `parse_stamp` | commits bare (date-only, no time) |

Plus: the rewritten recorded comment compiles into
`sync_directions_follow_the_pinned_rules` with its assertions
untouched.

## 5. Risks

- **JSON kind removal**: `move-reverse-fenced` disappears from the
  error-kind taxonomy. Consumers are this repo's CLI/REPL output and
  the runbook; no external API contract pins it (the receipts are
  the CLI's own surface). Stage 3 verifies with a repo-wide grep.
- **clap `ErrorKind` stability**: the M1 assertions use
  `clap::error::ErrorKind` variants (typed, but clap-owned).
  Clap is workspace-pinned; acceptable.
- **Test supersession optics**: removing the fenced assertion looks
  like weakening; the plan (D-2) and the commit messages must cite
  the recorded decision explicitly.

## 6. Backlog issues

- **#1010** — `backlog: unify the date-entry grammar across the CLI
  --due parser and the GUI due_edit module` (F7: two chrono
  implementations of the same documented grammar; this chunk
  documents the leniency on both fronts but does not unify —
  one-behavior-one-implementation tension, its own decision).

## 7. Acceptance (restated from the dispatch plan §4, with decisions bound)

- `ReverseFenced` and `MoveDirection::ToDeck` gone; the collapsed
  grammar pinned by M1; both-targets = typed `ArgumentConflict`
  (D-1); the fence's replacement semantics = the rebind ceremony's
  own refusals + grammar-absence for files→Deck (D-2); help/output
  updated with the parse.
- Non-padded dates canonicalize (M4); the grammar doc records the
  leniency as deliberate (D-4); the recorded comment cites the
  decision, not deleted.
- Docs: runbook §5 + architecture clause updated (D-2's semantics).
- Diffs confined per §3.6; gates green or deferred per the flock
  rule.

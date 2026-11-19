# Chunk B5 — gate/law/doc residue — implementation plan

- **Round**: backlog-reduction round 3, chunk B5 (scope report
  `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md`
  §B5; dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  §12 — normative).
- **Branch**: `ci/gate-law-doc-residue` (this worktree).
- **Covers**: #965, #967, #970, #964, #1063, #1043, #1042, #1046, #1047.
- **Acceptance** (dispatch plan §12): each item landed per its body; the
  gate residue fixes carry self-tests (the #920/#768/#760 precedent);
  the law/doc edits are consistent with the existing text (no drift);
  diffs confined to `scripts/`, `docs/`,
  `.artifacts/templates/dispatch-plan.md`, and the named source file.
- **Round laws**: (a) machine-readable gate states preserved (verdict
  labels and exit codes unchanged); (c) no weakened checks — boundary
  matching must not turn honest matches into misses; the #967/#970
  exclusions are evidence-scoped, not blanket; (d) out-of-scope findings
  filed as `backlog:` issues — see §6 (one filed: #1073).
- **Journey gate**: none — no user-visible capability changes (pure
  gate/script internals, incident-law codification, and doc edits are
  exempt per `docs/ux_planning.org`).

## 1. Behavioral items — the gate (`scripts/ux-coverage.sh`)

### 1.1 #965 — boundary-anchor the wiring-token matching

**Bug**: all wiring greps are fixed-substring (`grep -nF`): a callback
`save` matches `on_save_extra`; a command token `LogHabit` matches
`LogHabitOverflow`; the `grep -F tok` in the command check matches any
substring. Same class as #920 (substring resolution without a boundary),
lower stakes: seam names are globally unique by the seam-grammar law,
so the collision is prefix-recursive names within one family.

**Fix shape** (decision D-965-a): introduce one boundary-anchored grep
helper and use it everywhere a *token* is matched, keeping the
`first_match`/`adapter_match` file-order and first-hit semantics:

- `tok_match`: `grep -nE -- "(^|[^A-Za-z0-9_])<tok>([^A-Za-z0-9_]|$)"`
  — token must not be preceded or followed by an identifier character.
  Tokens are identifier-shaped by the plan grammar
  (`[A-Za-z0-9_]+`, enforced by the nomination extraction regex), so no
  regex escaping is needed for the token itself.
- Callback wiring, slint side: `first_match` on the callback name
  switches to `tok_match` (a `callback save;` / `save =>` declaration
  has non-identifier neighbors).
- Callback wiring, adapter side (`adapter_match`): the direct arm
  becomes `grep -nE -- "on_${cb}([^A-Za-z0-9_]|$)"` (matches `on_save(`,
  `on_save)`, `on_save,` — the `install_seam!` arm falls through);
  the `install_seam!` arm's inner `grep -F -- "$cb"` becomes
  boundary-anchored the same way.
- Action/command tokens: `first_match "$tok"` in the §6 action and
  command checks switch to `tok_match`.
- The #760 inventory cross-check keeps its quoted form
  (`grep -qF -- "\"${w_cb}\""`) — a quoted string cannot prefix-match
  (`"save"` ≠ `"save_extra"`); no change.

**No-miss guard** (round law c): a boundary-anchored grep still matches
every *honest* occurrence — an identifier-shaped token in real source
is always preceded/followed by a non-identifier character (punctuation,
whitespace, end of line). The end-of-line alternative
`([^A-Za-z0-9_]|$)` covers trailing hits. The self-test variant below
pins both directions.

**Self-tests** (new fixture variants; row-count assertions only, per
the existing self-test contract):
- variant `plan-prefixmiss.md` (fixture plan whose callback/token rows
  name `save` / `LogHabit`, fixture tree containing only
  `on_save_extra` / `LogHabitExtra`): gate exits 1, the expected number
  of MISSING rows — the prefix collision does not resolve.
- variant `plan-prefixfound.md` (fixture tree containing honest
  `on_save(` / `LogHabit` occurrences *adjacent to* the colliding
  `on_save_extra` / `LogHabitExtra` spellings): gate exits 0 — the
  boundary fix did not turn the honest matches into misses.
Both variants assert exit codes and row counts like self-tests 1–6;
no output-text assertions.

### 1.2 #967 — exclude the self-test fixture tree from the evidence universe

**Bug**: the `find`-based universes (`messages.rs`, `requests.rs`,
`adapter.rs`, `*.slint`, `*.rs`) match the miniature self-test fixture
tree under `scripts/fixtures/ux-coverage-fixture/tree/`, so a delta-
fallback row can read FOUND against a fixture file (observed live in
the 2026-11-18 N-A before-run).

**Fix shape** (decision D-967-a): one shared find predicate, applied to
**all** evidence universes including `seam_inventory.rs` and
`nom_file`'s by-name fallback:

```
find "$root" \
  -path "$fx_root" -prune -o \
  -name target -prune -o \
  -name node_modules -prune -o \
  -path "$root/.git" -prune -o \
  -type f -name '<name>' -print
```

where `fx_root` is the script-relative fixture root resolved as
`<script_dir>/fixtures/ux-coverage-fixture` when it exists (absolute
path — evidence-scoped to exactly the self-test tree, per round law
(c); never a blanket `scripts/` exclusion, and never a pattern that
could hide a real future evidence tree). When the fixture root does not
exist under the script dir (e.g. an installed copy), the prune term is
omitted. Combined with §1.3's target prune (one predicate, one place —
the universes stop duplicating find expressions).

**Self-test**: variant run against a temp-copied fixture tree that
contains a nested `<copy>/scripts/fixtures/ux-coverage-fixture/tree/`
carrying a `messages.rs` with a delta-fallback token (Kind unknown,
e.g. a prose cell `StateCommand variants`): the row must read MISSING —
the nested fixture tree is excluded. Without the exclusion the same run
reads FOUND, so the test is a real regression pin, not a tautology.

### 1.3 #970 — exclude build output from the evidence scan

**Bug**: every universe and the bound-test `grep -rnE ... "$root"` scan
`target/` build output; generated `.rs` under target can satisfy
evidence greps, and the bound-test grep is the largest surface.

**Fix shape** (decision D-970-a): folded into the shared predicate of
§1.2 (prune `-name target` and `-name node_modules`); the bound-test
grep gains `--exclude-dir=target --exclude-dir=node_modules`
(plus `--exclude-dir=.git`). Evidence-scoped: only build-output
directories are pruned, never a blanket name pattern that could hide a
real source dir (no crate or fixture directory is named `target` or
`node_modules` in this repo — verified during planning; the plan is
repo-shaped, not a general tool).

**Self-test**: variant run against a temp-copied fixture tree with a
`<copy>/target/debug/out.rs` containing a token a plan row greps for:
the row reads MISSING (FOUND only from the real evidence file when that
exists — the prefix-found variant of §1.1 doubles as the no-miss guard).

### 1.4 Self-test plumbing

- New fixture plans go next to the existing four
  (`scripts/fixtures/ux-coverage-fixture/plan-*.md`); fixture-tree
  additions (`on_save(`, `LogHabit`, colliding spellings, the pruned
  pollution files) go under `fixtures/ux-coverage-fixture/tree/`.
- The existing self-tests 1–6 must stay green unchanged — they are the
  no-drift guard for the matcher/exclusion changes.
- New self-tests follow the numbering (7, 8, …) with the same
  PASS/FAIL report shape and `rc=1` accumulation.

## 2. Source doc residue

### 2.1 #964 — stroke.rs duplicated doc-comment paragraph

`crates/taskboard-ui-slint/src/keymap/stroke.rs` (~lines 186–193): the
`chord_shift_partners` comment carries the out-of-table consequence
twice. **Decision D-964-a**: keep the lead-in block comment's copy
("Candidate 1 applies to pair members only: outside the table the
shift-distinct chords stay distinct (`Ctrl+Shift+z` ≠ `Ctrl+z` — the
shift bit survives the case law), exactly as before the map existed.")
and reduce the second to its unique content —
`// Out of the table: no candidates.` — deleting the duplicated
"keeps the shift-distinct chords distinct, exactly as before the map
existed." sentence. Pure comment edit, no behavior, no test.

## 3. Law/doc codification

### 3.1 #1063 — the dispatcher suite counts as one slot

Amend the *Resource law* bullet in `docs/agent_dispatch.org` §2 and
item 6 of `.artifacts/templates/dispatch-plan.md` **in lockstep** (the
template restates the law verbatim-by-intent; both must read the same
after this chunk):

- From: "the **dispatcher's full §10 suite runs only in a free window**
  — zero agents mid-build; the llvm-cov build is the heaviest single
  job on the box and counts as an agent slot of its own."
- To: "the **dispatcher's full §10 suite counts as ONE slot toward the
  ≤3 cap** (agents + suite runs ≤ 3 total; `run-quality-gates.sh`'s
  flock still serializes suites; the memory gate still guards every
  agent build) — the suite is the heaviest single slot on the box, so
  stagger it ≥3 minutes from any agent start like any other slot. The
  strict **zero-agent free window is reserved for when
  `memory-gate.sh --status` shows pressure** (MemAvailable below the
  floor)."

**No-contradiction check** (recorded decision D-1063-a): this does not
touch the two-mode routing law (gated-hold vs gated-preflight — that
law governs *who holds the gate lock*, this amendment governs *how many
slots run concurrently*); the flock-deferral rule is unaffected. The
original zero-window rule is demoted from always-on to
pressure-triggered with its origin cited (the 2026-11-17 OOM report's
belt-and-braces framing) so the history stays honest.

### 3.2 #1043 — the tmpfs/ENOSPC law (scratch off /tmp)

- **Law text** in `docs/agent_dispatch.org` §2 (new bullet directly
  after the resource law): "Host `/tmp` is tmpfs (RAM-backed) —
  scratch written there consumes `MemAvailable` directly and is a
  shared, unbounded resource (the 2026-11-19 ENOSPC incident: a
  foreign agent's 5.8G scratch dir on a 6G tmpfs killed three bench
  runs instantly). Large agent scratch (bench batteries, bisects,
  mutants shards, multi-hundred-MB logs) routes through
  `scripts/memory-gate.sh`'s TMPDIR redirection (both modes already
  redirect an unset-or-`/tmp` TMPDIR to `/var/tmp`) or creates its
  scratch under `/var/tmp` explicitly; nothing writes unbounded scratch
  under `/tmp`. Evidence:
  `.artifacts/reports/2026-11-19-build-cache-evaluation-report.md` §3."
- **Script changes** (the "extend to bench/scratch scripts" clause —
  the offending scripts found during planning): `scripts/r4-hover-repro.sh`
  (`mktemp -d /tmp/r4-hover-repro.XXXXXX`) and `scripts/load-repro.sh`
  (`mktemp -d /tmp/load-repro.XXXXXX`) change to
  `mktemp -d "${TMPDIR:-/var/tmp}/<name>.XXXXXX"` — explicit, tmpfs-free
  by default, still honoring an operator-set non-/tmp TMPDIR.
- **Explicitly out of scope** (recorded, do not touch): 
  `scripts/i686-cross-build.sh`'s `CARGO_HOME=/tmp/cargo-home` and
  `scripts/nextcloud_it_setup.sh`'s `/tmp/deck.tar.gz` are *inside
  containers* (container /tmp, not host tmpfs) and the deck tarball is
  megabytes, not gigabytes. Not offenders; leave alone.

### 3.3 #1042 — bench/measurement worktrees vs the doctor sweep

**Decision D-1042-a**: adopt the *recorded convention* the P9
re-dispatch already used — documentation law, no doctor change
(a doctor skip-flag or blanket `bench/` exclusion would weaken the
sweep; round law (c) forbids blanket exclusions, and the doctor's
HEAD-contained + clean predicate is correct for worktrees of record):

- Law text in `docs/agent_dispatch.org` §2 (same bullet as §3.2 or
  adjacent): "Measurement/bench worktrees whose only purpose is keeping
  a warm `target/` for an in-flight battery **live outside the
  `taskboard-WT-*` glob** (e.g. `/var/tmp/taskboard-bench-<arm>`,
  created with plain `git worktree add` / `git clone`), never under
  the standard worktree layout — the worktree-doctor sweep cannot
  distinguish them from disposable worktrees of record (the 2026-11-19
  incident: `taskboard-WT-bench-bench-b` swept mid-battery, invalidating
  its paired-load design; `.artifacts/reports/2026-11-19-build-cache-evaluation-report.md`).
  They are ephemeral the same way: the agent that finishes the battery
  removes them."
- Cross-reference one line in AGENTS.md §8's worktree-lifecycle block
  is **not** made (AGENTS.md is the dispatch-agnostic instructions; the
  law's home is agent_dispatch.org — the dispatcher reads it there, and
  the dispatch template item 6 gains the one clause
  "bench/measurement worktrees live outside the `taskboard-WT-*` glob").

### 3.4 #1046 — the advance-past-deadline law

`docs/testing_strategy.org` §6, appended to the *Time Manipulation*
paragraph (the clock-law section's opening tool paragraph — one place,
not a new subsection, per the issue's "one-line law addition"):

"Under `tokio::time::pause()`, a timer the fixture **re-arms during the
scenario** (a sleep whose deadline is recreated each loop iteration at
`now + window`) does not fire when the final `advance` lands the
virtual clock *exactly on* the deadline — advance strictly past it
(window + 1 ms) for the trailing edge to fire deterministically
(observed tokio 1.54.1, #1046; `config_watcher.rs`
`us1_s2_rename_storm_coalesces_into_one_reload`)."

Version-sensitivity check: a quick probe against the pinned tokio
version only (record the observed result in the plan's §7 report
territory, not the law — the law states the deterministic-advance rule,
which is correct under either tokio behavior and never weakens a test).

### 3.5 #1047 — the `visible: false` layout-slot footgun

**Decision D-1047-a**: home is `docs/coding_guidelines.org`, the UI
presentation rule list (the "Lock-Free UI Reads" / "Non-Blocking
Logic" bullets, §4 tech-stack rules). One bullet:

- "*Section gating mounts, never hides:* gate a layout section with the
  `if (...)` conditional mount — `visible: false` hides the element but
  **keeps its layout slot** (dead gaps; the H4 habit editor's
  priority-open row shifted the task-view goldens ~99px, r9 chunk 10 /
  #1047). `visible:` is only safe where the reserved slot is intended
  or the element has no layout footprint."

`docs/widget_interactions.org` is per-widget interaction behavior, not
general layout law — rejected as the home.

## 4. Execution order (stage 2)

1. Scripts first (highest blast radius, self-test loop is fast):
   §1.2+§1.3 shared predicate → §1.1 boundary matcher → fixture
   variants → `scripts/ux-coverage.sh --self-test` green → run the
   gate for real against one existing journey plan (informational run;
   verdicts must be identical or strictly more honest — any plan row
   that flips FOUND→MISSING is recorded and re-triaged against the real
   tree before commit, the #965 no-miss guard in production).
2. Doc/law edits (§2.1, §3.1–§3.5) — one commit each logical group is
   fine; house commit format `docs(...)`/`ci(...)`/`fix(scripts)`.
3. Gate: pre-commit hooks (fmt + clippy + slint lint) + 
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (background + poll; flock-deferral per the dispatch law if
   contended — commit first, never block).

## 5. Risks

- **R1 (#965)**: an honest wiring occurrence where the token abuts an
  identifier character in real source would become a false MISS. The
  grammar makes tokens identifier-shaped and sources are ordinary Rust/
  Slint (identifiers never abut), and the prefix-found self-test plus
  the production informational run (§4.1) guard it. Mitigation if a
  real miss appears: widen to `grep -nE "\b..."` per call site after
  diagnosing — never revert to bare substring.
- **R2 (#967/#970)**: over-pruning hides real evidence. The predicate
  prunes exactly three things (the fixture root, `target`,
  `node_modules`, `.git`); no repo source dir collides (verified).
- **R3 (#1063)**: template/doc drift if only one home is edited — both
  homes edited in lockstep is a named step; stage-3 reviews the pair.
- **R4**: a journey plan row legitimately credited from a file that
  now lands behind an exclusion → it can only be a fixture/target
  file, which was never legitimate evidence (the issue's live
  observation); re-triage, not revert.
- **R5 (#1043)**: `${TMPDIR:-/var/tmp}` in repro scripts changes their
  scratch location for humans — documented in each script's header
  line, one-line change, no behavior contract elsewhere references the
  old path.

## 6. Backlog issues filed during planning

- **#1073** — by-name evidence universes are order-nondeterministic;
  `crates/taskboard-domain/src/test_support/messages.rs` (a real-source
  duplicate of `messages.rs`) can satisfy real plan rows — same class
  as #967 but not covered by the fixtures/target exclusions.
  Deliberately out of this chunk's scope (the chunk lands exactly the
  #965/#967/#970 bodies); recorded in decision D-967-a.

## 7. Estimated diff size

`scripts/ux-coverage.sh` ~+60/−25 (predicate + boundary helper +
3–4 self-test variants), `scripts/fixtures/ux-coverage-fixture/`
~+90 (two plan variants + tree files), `scripts/r4-hover-repro.sh` +
`scripts/load-repro.sh` ~+4/−2, `crates/.../stroke.rs` ~−4/+1,
`docs/agent_dispatch.org` ~+25/−6, `.artifacts/templates/dispatch-plan.md`
~+6/−2, `docs/testing_strategy.org` ~+5, `docs/coding_guidelines.org` ~+6.
**Total ~200–250 lines**, confined to `scripts/`, `docs/`, the fixture
dir, the template, and the named source file — within the dispatch
plan §12 acceptance boundary.

# [Plan] UX Journey Framework — Phase 1 Install (Chunk 1, Stage 1)

- **Date**: 2026-11-13
- **Status**: STAGE-1 PLAN (TDD-first install spec; no implementation in this
  commit). Stage 2 implements exactly this; stage 3 reviews against it.
- **Origin**: `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  (ACCEPTED; normative §4, §6, §7, §8, Appendices A–C) + dispatch plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-dispatch-plan.md`
  §1/§2 (chunk 1, branch `docs/ux-journey-planning-framework`).
- **Verified against**: worktree HEAD `1b405c3c` (branch
  `docs/ux-journey-planning-framework`, based on `main` @ `35e6b0a1`).
  Anchors read at that SHA.
- **Diff shape (binding)**: exactly the six items of dispatch §2 Covers —
  (1) `.artifacts/templates/ux-journey-plan.md`, (2)
  `.artifacts/templates/dispatch-plan.md` amendment, (3)
  `docs/ux_planning.org`, (4) `AGENTS.md` §7 rule, (5)
  `scripts/ux-coverage.sh`, (6) the fixture. Non-goals: no census file, no
  app-code changes, no CI workflow changes, no mandate activation.

---

## 1. Decisions (each binding for stage 2)

### D1 — Table-header contract (template ↔ script), token-for-token

The template's fixed tables and the script's parser share one contract. Any
mismatch silently zeroes a check, so both sides quote these headers
verbatim, and the script's self-test pins them (§3).

**Heading anchors.** The parser keys on level-2 headings, matched by
*prefix + exact title*, not line numbers:

| Section | Anchor (level-2 heading text) | Table(s) found under it |
|---|---|---|
| §2 | `## 2. Capability × surface matrix` | capability matrix, then rulings |
| §4 | `## 4. Journey specs` | one table per `### US-<n>` subsection |
| §6 | `## 6. Wiring inventory` | wiring table |
| §7 | `## 7. Command-contract delta` | command delta table |

The template's HTML comments already warn "do not rename columns"; the
anchor titles are added to that warning.

**Column contracts (names AND order fixed):**

| Table | Header row (exact) |
|---|---|
| Capability matrix | `| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |` |
| Rulings | `| ID | Ruling | Reason | Home |` |
| Journey steps (per `### US-<n>`) | `| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |` |
| Wiring inventory | `| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |` |
| Command delta | `| Kind | Name | Payload sketch | Journeys |` |

Notes:
- Surface column names are the *matrix's* fixed surface set; platform
  qualification lives inside cells (`E (desktop)`), never in headers.
- Journey step IDs are written `US-1/S2`; their greppable test token is the
  snake_case form `us1_s2` (template §4 comment already states this).

**Malformed table = hard failure.** For each anchored section that exists,
the parser requires: (a) a header row whose cells match the contract
name-for-name and in order (cell trimming of surrounding whitespace only),
(b) a `|---|`-style separator row, (c) ≥ 1 data row. Any violation exits
with code **2** and a `PARSE-ERROR: <section>: <what>` line naming the
offending section — never a silent skip, never a degraded run. A *missing*
section (heading absent) is also exit 2: the template mandates all four.
Cells may of course be empty or `—`; emptiness is data, not malformation.

**Severity model**: exit 0 = all rows FOUND; exit 1 = well-formed plan with
MISSING evidence rows (the `--gate` semantics — a plain run prints the
table and still exits 1 on MISSING, matching Appendix C's "counts" +
gate wording; see D2); exit 2 = parse error. Usage errors exit 64 (bash
convention, mirrors `bump_version.sh`-style scripts).

### D2 — `scripts/ux-coverage.sh` implementation shape

House bash style, imitating `scripts/run-quality-gates.sh`: `#!/usr/bin/env bash`,
`set -euo pipefail`, a `usage()` heredoc, long header comment stating the
honest limits verbatim from framework §4.3:

> This gate proves wiring exists and is claimed-tested — not that the UX is
> good. Quality is the walkthrough protocol's job (docs/ux_planning.org).

**Interface:**

```text
scripts/ux-coverage.sh <plan.md> [--gate] [--root <dir>]
scripts/ux-coverage.sh --self-test
```

- `--gate`: exit nonzero (1) when any row is MISSING. A plain (non-gate)
  run prints the same table and exits 0 unless a parse error occurred —
  the report is informational; the gate is explicit. (Deviation note: C's
  sketch implies plain runs also fail on MISSING; keeping failure opt-in
  preserves "run the report" ergonomics for advisory mode. Recorded here
  as the decided semantics; `--gate` is what stage-2/3 and the template's
  DoD invoke.)
- `--root <dir>`: repository root for evidence greps (default: the git
  toplevel of the script's location). `--self-test` passes the fixture
  tree — this keeps the self-test hermetic and lets PASS rows reference
  tokens that exist only in the fixture tree, since no real UX plan (and
  no `us<n>_s<k>` test) exists yet.
- Checks, per Appendix C, implemented as `awk` table extraction + `grep -rn`
  evidence search:
  - wiring row → Callback token in `ui/**/*.slint` AND adapter handler
    (`on_<callback>` / `install_seam!` in `adapter.rs`); each Commands /
    ActionId token in `messages.rs` / `requests.rs` / `actions.rs` (the
    Kind column's `StateCommand`/`UiRequest`/`ActionId` value selects the
    file); a row with `—` in Commands must carry its reason in the
    Journey-steps column per the template comment (checked as non-empty).
  - journey step → snake_case token `us<n>_s<k>` in a `#[test]`/`#[tokio::test]`
    fn name under `crates/taskboard-ui-slint/tests/` (grep over `fn <token>`
    occurrences; the §8 crate nomination is honoured via an optional
    `Test-root:` plan front-matter line — default the UI crate).
  - capability cell citing `US-n` → that story's §4 table exists and all
    its steps are covered (composition of the journey-step check).
  - rulings row → Reason non-empty.
- Output: one line per check, `FOUND (<evidence file>:<line>)` or `MISSING`,
  then counts; parse errors abort before any table is printed.

**Fixture home (decision; deviation from framework §7 item 5, reason
recorded):** `scripts/fixtures/ux-coverage-fixture/` — *not* the origin's
`tests/fixtures/`. Reason: `tests/` in this repo means Rust integration
test trees owned by crates; a markdown fixture there is invisible to cargo
and misleads. The fixture is script test data, co-located with its tool,
and invoked explicitly via `--self-test` (never by `cargo test`).

Fixture contents:

```text
scripts/fixtures/ux-coverage-fixture/
├── plan.md          # well-formed plan: ≥1 row of every parsed kind,
│                    #   all evidence tokens resolvable inside tree/
├── plan-missing.md  # same plan with 1 wiring Callback, 1 journey-step
│                    #   token, and 1 command token resolvable NOWHERE
│                    #   (deliberate MISSING rows)
└── tree/            # miniature evidence tree (the --root target):
    ├── ui/fixture-view.slint        # contains the PASS callback
    ├── adapter.rs                   # on_<callback> handler
    ├── messages.rs / requests.rs / actions.rs   # the command variants
    └── tests/fixture_journeys.rs    # fn us1_s1_… test name
```

**`--self-test` contract (TDD evidence command, recorded):** the mode runs
the script twice against the fixture with `--root` pointing at `tree/`:

1. `bash scripts/ux-coverage.sh scripts/fixtures/ux-coverage-fixture/plan.md --gate --root scripts/fixtures/ux-coverage-fixture/tree` → every row FOUND, exit **0**.
2. `bash scripts/ux-coverage.sh scripts/fixtures/ux-coverage-fixture/plan-missing.md --gate --root scripts/fixtures/ux-coverage-fixture/tree` → exactly the three deliberate rows MISSING, exit **1**.

`--self-test` executes both, asserts those outcomes (exit codes compared
with `==`/`!=`, no output-text assertions — the no-text-assertion rule
applies to the script's own tests too; it asserts *exit codes and row
counts*, never prose), prints PASS/FAIL per sub-check, and exits 0 only if
both hold. Stage 2 records the observed transcript in its report.

### D3 — AGENTS.md §7 amendment (exact wording)

Appended to §7's artefact-type bullet list (after the dispatch-plans
bullet), advisory-with-activation-condition per framework §7 item 4 and §8
decision 1:

```markdown
- **UX journey plans**: a plan or design round that adds or changes a
  user-visible capability (any new verb, badge, view, or setting a human
  can perceive or act on; pure engine/storage/sync internals exempt)
  includes — or cites — an accepted UX journey plan written from
  `.artifacts/templates/ux-journey-plan.md` **before a dispatch plan may
  chunk the work**. *Advisory during rollout (decided 2026-11-13):* while
  the framework's phases 3–5 (surface census, retrofit analysis,
  remediation) are in flight, the rule binds a chunk only when that
  chunk's origin cites a UX journey plan; it becomes **mandatory for all
  new user-visible work** once those phases complete — a dedicated
  phase-6 docs chunk flips this wording (see `docs/ux_planning.org` for
  the activation state). Stage 2 runs `scripts/ux-coverage.sh <plan>
  --gate` as a self-check and stage 3 re-runs it plus the walkthrough
  protocol whenever the origin cites a UX plan; the gate is never CI.
```

§10 stays untouched (framework §8 decision 2: no CI integration).

### D4 — `docs/ux_planning.org` outline

Org conventions per `docs/testing_strategy.org` / `docs/flow_model.org`:
`#+title:` + `#+options: toc:t`, asterisk headings, verbatim `=code=`
marking, binding-rules prose up top. Outline (stage 2 fills prose from the
cited framework sections — promoted, not paraphrased into vagueness):

1. `* What this document is` — the standing UX planning law; binding for
   agents and humans; companion to `AGENTS.md` §7 (which carries the short
   rule + pointer here); normative source: the accepted framework plan.
2. `* 2. The framework` — promoted from framework §4: the artefact and its
   nine sections (with the gap classes each kills), journey-first
   derivation + the P1-slice rule, the automated completeness gate
   (interface, checks, honest limits — pointing at `scripts/ux-coverage.sh`
   usage), the review-stage walkthrough protocol (four questions +
   six-heuristic rubric + BLOCKER/MAJOR/MINOR classification), the UX
   surface census, and the "what lives where" table (§4.5).
3. `* 3. Activation state machine` — from §6: the six phases as a table
   plus an explicit advisory→mandatory state description (current state:
   **advisory**, phases 1–2; activation condition: phases 3–5 complete;
   the flip: a phase-6 docs chunk rewords AGENTS §7 and records the
   activation date here). Phase-1 completion (this chunk) is recorded as
   the initial state entry.
4. `* 4. Method survey` — framework §3 carried over: adopted (with the
   "kills" table), adapted, rejected, each with its one-line rationale.
5. `* 5. Sources` — the framework plan's Sources section carried over
   **verbatim** (dispatch evidence mandate: the §3 citations must survive
   intact). Mermaid precedent note: UI-flow mermaid follows
   `docs/flow_model.org`'s established GitHub-rendered-diagram practice.

### D5 — `ux-journey-plan.md` template and dispatch-template amendment

- Template: framework Appendix A verbatim minus the "delete after fill-in"
  fate of its how-to comment (the comment *stays*, dispatch-plan-template
  style — the comment instructs copiers; only `<...>` placeholders go
  unfilled). Table headers exactly per D1. DoD checklist keeps the
  `--gate` invocation.
- `dispatch-plan.md` amendment per Appendix B, all mechanics strictly
  conditional on "the origin cites a UX journey plan": (a) §1 chunks table
  gains a **UX coverage** column (documented in the template's fill-in
  guidance: `<US-n/S-k + W-x IDs this chunk implements>` / `n/a — no
  surface delta`); (b) stage 2 in the shared execution model gains the
  gate self-check sentence; (c) stage 3 gains the UX completeness mandate
  (re-run gate + walkthrough protocol + BLOCKER/MAJOR fixed on branch,
  MINOR → backlog issues); (d) fill-in quality bar gains the
  "name the UX plan's sections by ID (US/W/R)" line. The advisory-mode
  conditionality *is* the semantics — never phrase these as unconditional.

### D6 — Commit sequencing (every commit standalone)

| # | Commit | Contents |
|---|---|---|
| 1 | `docs(plan): the ux-journey-framework install chunk plan` | this file (stage 1, done) |
| 2 | `feat(scripts): add the ux-coverage completeness gate with self-test fixture` | items 5+6 — script + fixture together so `--self-test` passes within the commit |
| 3 | `docs(templates): add the ux-journey-plan template and amend the dispatch-plan template` | items 1+2 — both templates; the template's DoD names the script, which exists as of commit 2 |
| 4 | `docs(ux): add docs/ux_planning.org and the AGENTS §7 UX journey plan rule` | items 3+4 — the standing law + its AGENTS pointer land together |

Ordering rationale: no commit references a file added later — the
template's DoD checklist and `ux_planning.org` both name
`scripts/ux-coverage.sh`, so the script lands first. Docs-only commits 3
and 4 keep every gate trivially green; commit 2's test is `--self-test`
(no Rust code).

### D7 — Open questions resolved

- **Matrix-cell checking depth (phase-1 scope)**: the script checks story
  citation → story table exists → steps covered, and ruling reasons
  non-empty. It does NOT diff the census (none exists) and does NOT
  cross-check `tests/seam_inventory.rs` — framework §4.3 says that
  cross-check "should be" possible; deferring it to a backlog issue
  (filed by this stage, listed in §4 below) keeps the chunk minimal.
- **Plain-run exit semantics**: informational (exit 0) unless `--gate`;
  decided in D2 with the deviation note.
- **Fixture home**: `scripts/fixtures/…`, deviating from the origin's
  `tests/fixtures/…` — reason in D2.

## 2. Test catalogue (TDD order for stage 2)

No Rust tests; the executable deliverable is the script, tested by:

1. **RED**: write `plan-missing.md` expectations first — run the
   not-yet-implemented script; observe it cannot fail (missing). 
2. Implement parsing + checks; `--self-test` sub-check 2 (MISSING path,
   expects exit 1) goes green first, then sub-check 1 (PASS path).
3. Negative parse tests folded into the self-test: the self-test also
   feeds a malformed-header variant (generated on the fly under
   `$(mktemp -d)`, never committed) and asserts exit 2 — pinning the
   fail-loudly law.
4. Full AGENTS §10 suite via `scripts/run-quality-gates.sh` (docs-only
   diff; still mandatory).

Evidence command for the handoff report (expected outcomes in brackets):

```sh
scripts/ux-coverage.sh --self-test                                   # → exit 0, both sub-checks PASS
scripts/ux-coverage.sh scripts/fixtures/ux-coverage-fixture/plan-missing.md --gate \
  --root scripts/fixtures/ux-coverage-fixture/tree                   # → exit 1, 3 MISSING rows
```

## 3. Risks

- **Header-contract drift between template and script** is the chunk's
  named trap: mitigated by D1's verbatim table + the self-test pinning
  both paths; stage 3 verifies token-for-token.
- **Awk table parsing edge cases** (escaped pipes in cells, ragged rows):
  template guidance says cells avoid `|`; parser treats a data row whose
  cell count ≠ header count as a parse error (exit 2), not guesswork.
- **No real UX plan exists yet** — the fixture stands in; first real
  exercise is phase 2 (by design, not a gap).
- **AGENTS wording creep**: stage 3 re-checks the advisory +
  activation-condition phrasing is intact (never mandatory-now).

## 4. Estimated diff size & backlog filings

- Template ~110 lines; dispatch-template amendment ~35 changed lines;
  `ux_planning.org` ~300–380; AGENTS.md +16; `ux-coverage.sh` ~250–300;
  fixture ~150. **Total ≈ 900–1000 added lines across 6–7 files.**
- Backlog issue filed by this stage: seam-inventory cross-check
  (see report for the issue number).

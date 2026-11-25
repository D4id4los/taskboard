# Chunk G1 — Golden Determinism + Re-Base — Chunk Plan

- **Date**: 2026-11-25
- **Kind**: chunk plan — stage-1 output for dispatch-plan chunk G1
  (`.artifacts/plans/2026-11-25-backlog-reduction-round7-dispatch-plan.md` §2).
- **Covers**: #1241 (PNG regen byte-nondeterminism) FIRST, then #1239
  (re-base the 27 goldens onto the enriched fixture), plus #1336's
  scope check. Branch `test/golden-determinism-rebase`.
- **Normative origins**: scope report §A (`.artifacts/reports/2026-11-25-backlog-reduction-round7-scope-report.md`),
  dispatch plan §1/§2, S7 wave plan §3–§5
  (`.artifacts/plans/2026-11-21-golden-coverage-wave-plan.md`),
  #1140's landed manifest format
  (`crates/taskboard-ui-slint/tests/support/golden_manifest.rs`),
  #1139's comparator (`golden_compare.rs`).
- **Round laws**: (a) determinism pinned by a test, never prose;
  (c) visual semantics preserved — a re-base delta beyond the enriched
  fixture's intended richer state is a finding; frozen render clock
  law binds (`docs/testing_strategy.org` §6.1–6.2); (d) out-of-scope
  findings become `backlog:` issues immediately.

## 1. Diagnosis of #1241 (recorded)

**Method.** Read the encoder source (`png-0.18.1/src/encoder.rs`,
`common.rs`, `filter/mod.rs` — the exact crates in `Cargo.lock`:
png 0.18.1, flate2 1.1.10, miniz_oxide 0.8.9/0.9.1, fdeflate 0.3.7),
the harness writer (`golden_compare::encode_png`), and a chunk-level
inspection of committed goldens.

**Findings.**

1. `encode_png` uses `png::Encoder` with **all defaults**: color
   RGBA8, depth 8, `Filter::Adaptive` (the crate default,
   `filter/mod.rs` L31–35) and `DeflateCompression::Level(6)`
   (flate2/miniz_oxide — `common.rs` L404–407: `Options::default()`
   → `from_simple(Compression::Balanced)` → `Level(6)`).
2. The crate writes **no metadata chunks**: committed goldens carry
   exactly IHDR + IDAT + IEND (verified by chunk scan on
   `board-0.5-1000.png` and `today-0.5-600.png`). There is no tIME,
   no tEXt, nothing to strip. The single-IDAT layout also confirms
   the whole deflate stream is one buffer.
3. In-process, encoding is a pure function of (pixels, encoder
   settings, crate versions): encode-twice is byte-identical. There
   is **no runtime/session nondeterminism** — no allocator ordering,
   no parallelism in the encode path, no clock input.
4. The crate's own docs state the contract we are hitting:
   *"The implementation details of DEFLATE compression may evolve
   over time, even without a semver-breaking change to the version of
   the `png` crate."* (`common.rs` L363–366). Filter heuristics and
   compressor output (flate2/miniz_oxide/fdeflate) are likewise free
   to change in any lock-file bump.
5. Therefore the observed churn — 7 of 27 rewritten byte-different,
   PIL pixel-diff zero — is **cross-version compressor/filter drift**:
   the recording commit and the later `TASKBOARD_REGEN_GOLDENS=1`
   pass resolved different `png`/`flate2`/`miniz_oxide` versions (or
   the same `png` version with a different deflate backend graph),
   and the *adaptive filter choice + level-6 zlib stream* changed for
   the 7 images whose pixel statistics sit near a heuristic boundary.

**Verdict**: not fixable by "pinning settings" alone — the defaults
are already deterministic at runtime; the instability surface is the
crate graph. The fix is a **recorded byte-contract** (§2): explicit
settings + a pin test that turns any future drift into a loud,
deliberate, single-source corpus re-encode.

## 2. Decisions

| # | Question | Decision |
|---|---|---|
| D1 | Which encoder settings does `encode_png` pin? | State the current defaults **explicitly**: `encoder.set_compression(png::Compression::Balanced)` (documenting it resolves to `Filter::Adaptive` + `Level(6)`), keeping RGBA8/depth-8. Zero byte-behavior change today; the point is the settings are named in our code with a comment naming the drift law and the re-encode pin. Do **not** switch to `FdeflateUltraFast`/`Fast` — it would churn all 34 goldens' bytes for speed we do not need (regen is a rare reviewed step). |
| D2 | What is the determinism pin test ([L](a))? | Two tests: (i) `golden_compare`: `png_encoding_is_byte_deterministic` — a nontrivial noisy synthetic image encoded twice → byte equality (cheap, always runs); (ii) `golden_visuals`: `the_committed_corpus_reencodes_byte_identical` — for **every** committed PNG (27 + 7): decode → `encode_png` → assert byte equality with the file on disk. (ii) is the real contract: a regen over unchanged pixels cannot produce different bytes than committed, so any encoder-graph drift fails CI and forces a deliberate whole-corpus re-encode. It needs no renderer and no clock (decode+encode only), so it is fully deterministic. |
| D3 | What happens when the encoder graph moves? | Recorded law in `golden_compare::encode_png`'s doc: a `png`/`flate2`/`miniz_oxide`/`fdeflate` version bump requires a **whole-corpus regen commit** (`TASKBOARD_REGEN_GOLDENS=1`, single drift source, its own PR discipline like #1239's). Never hand-edit PNGs; never mix encoder-version churn with content churn. |
| D4 | The re-base mechanics (#1239) | Switch the 27 existing goldens' capture tests from `fixture::golden_state_at(PINNED_NOW)` to `fixture::golden_state_enriched_at(PINNED_NOW)` as the **base**, keeping each test's local mutations layered on top (notification/alarm/settings/cleared-pin variants keep their deltas; `golden_state_enriched_at` inherits the #638 review-card pin). One wholesale regen: `TASKBOARD_REGEN_GOLDENS=1` through `scripts/memory-gate.sh`, backgrounded per [D]. **Its own commit(s), never mixed with encoder changes** — but see S5 ordering: the encoder commit lands first, and because the pinned settings are byte-equivalent to the old defaults, the 7 gap goldens do NOT churn bytes in the encoder commit (the re-encode pin already holds on main's settings). |
| D5 | Which of the 27 change visually? (expectation to record pre-regen) | Enrichment-visible surfaces: `board-<s>-<w>` ×6, `task-view-*` ×6, `task-view-focus-cell`, `today-*` ×6, `kiosk-board`, `kiosk-today`, `search-palette`, `capture-overlay`, `focus-overlay`, `action-menu`, `alarm-window`, `plan-next-day`, `settings`, `track-entries`, `notification` — expect the §5 enrichment list where the surface projects it: label chips on task0 rows with the `+1` fold (4 memberships, 3 chips), the recur badge on the stack-b task, the reminder badge on task1, the degraded-sync badge (Offline + 2 pending ops) on the board bar, the second board ("dev") in the board bar/switcher and the settings board list, the long-title ellipsis on the dev board's card. Enrichment-invisible: `review-empty`, `review-ledger`, `review-walk`, `icon-probe` — expect **pixel-identical** output (they project state slices the enrichment does not touch); their PNGs must stay byte-identical post-regen, which the stage-2 diff review verifies file-by-file (`git diff --stat` must show exactly 27 + manifest rows changed, not 31). **Any delta outside this table is a finding per [L](c), diagnosed before landing.** |
| D6 | Manifest rows | Keep #1140's format **unchanged** (name / fixture / state / axes{scale,width,height}): the `fixture` field flips to `golden_state_enriched_at(PINNED_NOW)` for the 27 re-based rows and each `state` note gains the enrichment deltas it now shows. No new fields — the format already carries everything a layout-review consumer needs; widening it is G3's call if the analyzer demands it (see B7 below). The regen upsert path (`record`) preserves row order, so the manifest diff is field-level. |
| D7 | The #1336 scope-check method | After the encoder + re-base land: run the scale-matrix tests (`the_today_goldens_hold_across_the_scale_breakpoint_matrix` and `the_board_goldens_hold_across_the_scale_breakpoint_matrix`) **5 consecutive times** through `scripts/memory-gate.sh scripts/run-quality-gates.sh`-free direct nextest invocations of the `journey_suites` binary (the settle fix a15c1bc5 is already in the base). Record pass counts as evidence. Green ×5 ⇒ verdict "no ordering defect under the deterministic encoder; watchlist stands" (comment on #1336, leave open per the flaky law). Any failure ⇒ genuine ordering defect, fixed in this chunk with its own evidence, and the 0.5-scale today goldens re-recorded to the settled frame. |
| D8 | Expected pixel-delta verification method | The comparator's zero-noise law holds: the frozen clock + byte-deterministic renderer mean an unchanged golden's re-capture diff is exactly 0. Stage 2 records, per changed golden, the comparator's measured max luma/chroma cell diff — nonzero where the enrichment lands, 0 elsewhere — and the PR body summarizes the table. No budget changes. |
| D9 | Gate discipline | Pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run --workspace`, backgrounded with polls after committing; flock-deferral per the rule. Regen runs route through `scripts/memory-gate.sh`, backgrounded (full-suite regen of 34 goldens is a multi-minute render load). |

## 3. Execution order (stage 2)

1. **Commit 1** — `test(goldens): pin the PNG encoder settings and the
   byte-identity contract`: explicit `set_compression` in
   `encode_png` + its doc-comment law (D3) + the two pin tests (D2).
   Verify: the corpus re-encode test passes on main's goldens
   untouched (proves D1's zero-byte-change claim).
2. **Commit 2** — the re-base: fixture swap in the 27 capture tests
   (D4), one `TASKBOARD_REGEN_GOLDENS=1` run, manifest rows (D6),
   `git diff --stat` audited against D5's table (27 PNGs + manifest,
   the four invisible goldens byte-identical).
3. **Commit 3 (evidence)** — the #1336 scope-check record (D7) and
   the per-golden delta table (D8) into the PR body / final report;
   the #1336 comment posted.
4. Gate per D9; defer per the rule if contended.

## 4. Risks & traps

- **R1 — the four "invisible" goldens drift.** If `review-*` or
  `icon-probe` change pixels under the enriched fixture, a surface
  unexpectedly projects enrichment (e.g. the second board in a
  review header). That is a [L](c) finding: either the expectation
  table (D5) is corrected in the plan or the surface has an
  unintended coupling. Diagnose before re-recording.
- **R2 — mutated-variant goldens interact with enrichment.** Tests
  layering mutations (cleared tracking, `+ add_alarm`, Inbox board)
  assert against fixture internals; the enriched base changes indices
  (extra boards/tasks) and may break `state.tasks.get(&fixture.task1)`
  -style accessors. The enriched constructor returns the same
  `Fixture` ids; boards beyond the first are reachable only through
  iteration — any hardcoded "first board" assumption in a capture
  test is exactly the kind of thing to spot. Keep each test's
  mutation layer semantically identical.
- **R3 — regen parallelism.** Nextest runs each golden test as its
  own process; the manifest sidecar lock serializes upserts, and a
  killed run leaves `manifest.toml.lock` behind (delete + rerun —
  documented in `golden_manifest.rs`). Never regen concurrently with
  another worktree's regen (the lock file is per-checkout, so
  worktrees are isolated — no cross-worktree risk).
- **R4 — the pin test vs. deliberate re-encodes.** Test (ii) of D2
  fails on every legitimate corpus re-encode commit *until the goldens
  are rewritten in the same commit*. Commit 1 and the regen must be
  separate commits, so commit 1 must not add test (ii) before the
  goldens it validates exist — they do (main's corpus), which is why
  commit 1 is safe standalone. Stage 3 re-running the pin post-rebase
  validates the new corpus.
- **R5 — #1336's pre-a15c1bc5 history.** The 0.5-scale today goldens
  were re-recorded to the settled frame on the round-6 branch; if
  this chunk's base (origin/main) already contains that, the matrix
  is expected green ×5. A failure reproducing the *mid-settle* frame
  signature means a new settle window opened — fix via the capture
  quiescence seam, not by budget changes.

## 5. Estimated diff size

- Code/tests: `golden_compare.rs` +~40 lines (settings pin, doc law,
  test i); `golden_visuals.rs` ~30 capture-site fixture strings + test
  (ii) +~60 lines; total **~130 lines**.
- Data: 27 PNGs rewritten (binary), 4 byte-identical, `manifest.toml`
  27 rows' `fixture`/`state` fields.
- One PR, three commits, no stacked-PR dance needed.

## 6. Open questions — none remaining

All resolved in §2 (D1–D9). Deferred by design: manifest format
widening → G3's call (B7); #1336's dedicated triage pass stays
watchlist unless D7's scope check reproduces the flake.

## 7. Backlog issues

None opened at stage 1: every finding landed inside the plan's scope
(D1–D9). Candidates deliberately *not* filed: encoder-version regen
law (in-plan, D3), manifest widening (G3 scope).

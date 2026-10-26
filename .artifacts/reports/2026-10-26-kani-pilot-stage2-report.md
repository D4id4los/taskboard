# Kani Pilot — stage-2 implementation report (backlog-triage chunk 6)

- **Date**: 2026-10-26
- **Chunk**: 6 of the backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §7) —
  issue #426, branch `test/kani-pilot`, worktree
  `../taskboard-WT-test-kani-pilot`.
- **Plan**: `.artifacts/plans/2026-10-25-kani-pilot-plan.md` (stage 1);
  this report closes stage 2 (implementation + evidence). No push, no
  PR (stage-2 handoff constraint); handoff state = branch at
  `9b5368d`, all local gates green.

## 1. Verdict

**Pilot success exit achieved**: 4 harnesses **proven** within
documented bounds (bar was ≥3) + weekly `kani.yml` landed (validated
by YAML parse and an identical local CI-shaped run; the scheduled cron
can only be proven by waiting a week, per the plan). Two of the five
catalogued laws terminated honestly per issue #426's sanctioned
containment exits (H2's full round-trip, H5 entirely) — neither blocks
the pilot; both are recorded with full measurement ladders below.

| Harness | Law | Outcome | Unwind | Checks (failed/unreachable) | Verify wall | Peak RSS |
|---|---|---|---|---|---|---|
| H1 `BackoffPolicy::delay` | positive/increasing/capped + None⇔exhausted | **PROVEN** | 12 (doubling @24: identical) | 103 / 2 | 0.56 s | 447 MB (incl. compile) |
| H3 `aggregate_rows` | dominance+first-max, max success, saturating sum, counters, panic-freedom | **PROVEN** | 4 (doubling @8: identical, 690 checks) | 690 / 5 | ~1 s | 490 MB |
| H4 `day_window` | totality/panic-freedom, `start<=now<end`, `end-start==24h` | **PROVEN** | 8 (doubling @16: identical, 447 checks both) | 447 / 8 | 39.4 s | 494 MB |
| H2 `parse_text_form` (Rung C @4 bytes) | grammar totality, Some-iff-well-formed, Σ digit·unit, sign, overflow⇒None | **PROVEN** | 12 (doubling @24: **killed at 6 GiB cap**, analytic bound argument recorded) | 487 / 30 | 138 s | 3.84 GiB |
| H2 full canonical round-trip (Rung A) | `parse(text_form(o))==o` | **killed at 6 GiB cap** (1 guarded attempt; probe had already measured >6 GiB invariantly) | — | — | — | >6 GiB |
| H5 `plan_pushes` grouping | delete-subsumes, partition, kind order, panic-freedom | **honest termination** — killed at cap at every documented ladder rung (5 guarded kills, §4) | — | — | — | >6 GiB |

Final CI-shaped evidence run (exactly what kani.yml executes, under
the 6 GiB guard): `cargo kani -p taskboard-domain` → **all 3 domain
harnesses successful**, wall 2:54, peak RSS 3.93 GiB;
`cargo kani -p taskboard-sync-nextcloud --harness verify::` →
**successful** (H1). Both jobs exit 0.

## 2. What shipped (commits on `test/kani-pilot`)

1. `558d26d` — H1 harness (`crates/taskboard-sync-nextcloud/src/verify.rs`)
   + `#[cfg(kani)] mod verify;` gate in its lib.rs + workspace
   `[workspace.lints.rust]` `check-cfg` registration for `cfg(kani)`
   (deviation, §5).
2. `2267255` — H3 + H4 harnesses (`crates/taskboard-domain/src/verify.rs`)
   + gate line in the domain lib.rs.
3. `9b5368d` — H2 Rung C harness + `.github/workflows/kani.yml`.

`kani.yml` mirrors `miri.yml` per the plan: Mondays 04:00 UTC cron +
`workflow_dispatch`, two per-crate jobs (domain; sync backoff),
120-minute timeouts, Kani pinned `--version 0.68.0`, no
`RUSTUP_TOOLCHAIN` override (Kani selects its own nightly), no PR
trigger, no artifacts. Validation: `python3 -c "yaml.safe_load(...)"`
clean (actionlint is not installed on this machine — noted gap; the
dispatch smoke run post-merge will exercise the real job graph).

## 3. The `cfg(kani)` isolation (hard rule verified)

- Stable builds never see harness code: clippy `--all-targets`
  (the pre-commit hook runs it with `-D warnings`), nextest (1849
  tests, 40 skipped — none Kani), doc tests, `cargo deny`, and the
  coverage gate all pass with zero harness compilation (only
  production-visible diff: two one-line `#[cfg(kani)] mod verify;`
  gates + the lints registration).
- Kani's proof library auto-provisions from its bundle; **zero
  Cargo.toml dependency/feature changes**.

## 4. Containment ladders, measured (the honest-termination record)

### H2 (canonical text round-trip)

- **Rung A** (full law, real `format!` String, unwind 24, ±30 days):
  killed at the 6 GiB cap. Matches the stage-1 probe (>6 GiB
  invariantly across solvers/ranges). The String wall is structural:
  symbolic-content `String` through `format!` + `str::parse` over
  symbolic bytes.
- **Rung B skipped** (deviation): the packed-integer-model stub
  verifies only the harness's own model — no production evidence —
  and Rung C dominates it at the same cost class. Recorded rather
  than executed.
- **Rung C** (real parser over symbolic bytes): 8 symbolic bytes
  killed at cap; **4 symbolic bytes proven** (400 checks). The bomb
  is `u64::from_str` over symbolic digits; 4 bytes (one component)
  fit at 5.19 GiB (pre-dedup; the stage-3 independent spec mirror
  brought the same rung down to 3.84 GiB at 487 checks).
- **Rung D (the disposition)**: the full canonical round-trip law
  stays with its proptest — proptests remain the right tool for it.
  The parse-side grammar laws are now exhaustively proven within the
  documented 4-byte/charset surface, which the proptests only sample.

### H5 (plan_pushes grouping)

Killed at the 6 GiB cap at **every** documented rung (each one
guarded attempt after the ladder's no-blind-retry rule):

1. N=3 ops, full 16-tag space, unwind 16 — killed (plus one
   unguarded probe kill at 10.8 GiB before the guard was restored —
   my process error, §5).
2. N=2 ops — killed.
3. N=2, `UpdateAppData` dropped from the tag space — killed.
4. Single-task universe — killed.
5. Single-task, unwind 8 (Kani's default unwinding assertions make
   under-unwinding loud, so the bound lowering is sound) — killed.

The formula is structural: 16-way symbolic variant matching through
the planner's bucket walk, B-Tree grouping, and sort multiplexed per
op dwarfs the integer-class harnesses at any N ≥ 1. **Disposition**:
the grouping laws stay with their unit/proptest coverage; no harness
shipped (a harness that OOMs would also break the weekly CI job —
the issue's "never project-blocking" containment, `dropped`
branch). A future attempt should model the outbox symbolically as a
packed struct-of-fields (fixed symbolic variant *slots* rather than
`match` on symbolic tags) — recorded as the follow-up idea.

## 5. Deviations from the plan

1. **`[workspace.lints.rust] unexpected_cfgs` registration** (root
   `Cargo.toml`): required — `clippy -D warnings` rejects the
   unregistered `cfg(kani)` as `unexpected_cfgs`. Minimal sanctioned
   edit; registered for the whole workspace.
2. **Rung B skipped in favor of Rung C** (§4): evidence-domination
   argument recorded above.
3. **H2 doubling check incomplete**: the 24-unwind re-run was killed
   at the 6 GiB cap; the bound's justification is the analytic
   loop-need argument (outer ≤2 + inner ≤4 = 6 iterations; 12 ≥ 2×)
   plus the recorded kill. Bump checklist note is in the harness doc.
4. **One unguarded run** (H5, first attempt): I lost the
   `systemd-run` guard once; cbmc reached 10.8 GiB before I killed it
   manually. No collateral damage (30 GiB machine had headroom), and
   every subsequent run was guarded. Recorded as a process miss.
5. **kani.yml run-command quoting**: `--harness verify::` in an
   unquoted YAML scalar parses as a mapping (`: ` sequence); quoted.
6. **No `workflow_dispatch` smoke run**: the workflow file is on an
   unpushed branch; a dispatch run requires the file on `main`
   (pushing is out of stage-2 scope). The local CI-shaped run
   (§1) is the available evidence; the post-merge dispatcher should
   trigger `gh workflow run kani.yml` once after merge.

## 6. Gate status at handoff (branch `test/kani-pilot` @ `9b5368d`)

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo nextest run --workspace` — 1849 passed, 40 skipped, 0 failed
- `cargo test --doc --workspace` — ok
- `cargo deny check` — advisories/bans/licenses/sources ok
- coverage three-command sequence — ok (`lcov.info` written;
  `target/llvm-cov-target` removed)
- Kani: 4/4 shipped harnesses proven (§1)

Handoff state: PR open + `gh pr checks` green remains (stage 3);
no push/PR was performed in stage 2 per the brief.

## 8. Stage-3 re-verification (review pass, 2026-10-26)

The review agent rebased the branch onto `origin/main` (`05a766b`,
clean) and independently re-ran every harness under the §1 guard:

- H1 `prop_backoff_delays_positive_increasing_capped_proven` — 0 of
  103 failed (2 unreachable), verify 0.34 s, job wall 4 s.
- Domain run (`cargo kani -p taskboard-domain`, CI-shaped) — H2
  Rung C 0 of 400 failed (25 unreachable, 185 s); H4 0 of 447 failed
  (8 unreachable, 46 s); H3 0 of 690 failed (5 unreachable, 12 s);
  total wall 4:12, peak RSS 5.19 GiB. All four verdicts and check
  counts identical to §1.
- Contract audit of every harness's assumptions against the
  production code found one defect: H3's doc comment stated the
  doubling check as "`unwind(4)` and `unwind(4)`" (typo for 8);
  corrected in the harness commit. H2 Rung C's charset assumption
  matches the grammar's alphabet exactly (the only shrink is the
  documented 4-byte length); H1/H4's assume-ranges match the
  contracts and proptest ranges verbatim.
- Standard gates re-run on the post-rebase branch: fmt, clippy `-D
  warnings`, nextest (1897 passed / 40 skipped), doc tests, `cargo
  deny`, and the three-command coverage sequence — all green.

### SonarCloud duplication-gate finding (review pass)

The PR's SonarCloud quality gate failed on new-code duplication
(17.5% vs the 3% threshold): (a) the three triplicated symbolic
`mint_row` literals in the H3 harness, (b) `spec_parse_total` being a
line-for-line transcription of `parse_text_form`'s loop. Fixed in
`5162a9a`: (a) collapsed into a `mint_sym_row` helper; (b) rewritten
as an independent single-pass state machine, verified equivalent to
the production parser exhaustively over all 38,416 four-byte
grammar-alphabet strings before the re-proof. Post-fix re-run (the
table above reflects it): H2-C 0 of 487 failed (30 unreachable,
138 s, 3.84 GiB peak — leaner than the pre-fix 5.19 GiB); H3 0 of
690 failed (5 unreachable); H4 0 of 447 failed (8 unreachable); H1
unchanged.

## 7. Counterexamples

None. Every shipped harness proved — no concrete-playback
conversions were needed (the mechanism is documented in the stage-1
plan §5 for future failures).

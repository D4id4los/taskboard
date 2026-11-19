# Chunk R1 Plan — lld linker for dev/test builds (#1069, `ci/dev-lld-linker`)

Stage-1 planning artefact for the round-3 dispatch plan
(`2026-11-19-backlog-reduction-round3-dispatch-plan.md` §2) and the scope
report §A R1. Goal: switch the linux x86_64 dev/test link step from the
default bfd `ld` to `lld` — the report's lever 1 — with a measured
before/after delta and a CI-parity decision backed by evidence.

## 1. Facts verified (stage-1, 2026-11-19)

| Fact | Result | Evidence |
|---|---|---|
| `ld.lld` on PATH locally | **absent** — `/usr/bin/lld -> lld-21 -> /usr/lib/llvm-21/bin/lld`, but no `ld.lld` symlink anywhere on PATH; `dpkg -S /usr/bin/lld` matches no package; `apt-cache policy lld`: **not installed** (candidate 1:21.1.6-71) | `ls -la /usr/bin/lld*`, `dpkg -S`, `apt-cache policy lld` |
| `ld.lld` binary itself | exists at `/usr/lib/llvm-21/bin/ld.lld` | `ls /usr/lib/llvm-21/bin/` |
| rustc 1.99.0 `-Clinker-features` spelling | **the report's premise is wrong**: on stable 1.99.0 the flag only accepts `+lld`/`-lld` and `+lld` is **unstable** (`error: '-C linker-features=+lld' is unstable...`); `-Zunstable-options` is rejected on stable. The `-Clinker-features=-ld,+lld` form from the report is rejected outright | `rustc -C help`, compile probes in `/tmp/lldprobe` |
| Stable spelling that works | **`-Clink-arg=-fuse-ld=lld`** — cc-level link arg, driver-independent, verified end-to-end: `rustc -Clink-arg=-fuse-ld=lld p.rs` links and runs when `ld.lld` is on PATH | compile probe, 2026-11-19 |
| Absolute-path `-fuse-ld=...` | rejected by the local gcc driver — a per-machine path cannot be embedded in the repo config | probe `rustc -Clink-arg=-fuse-ld=/usr/lib/llvm-21/bin/ld.lld` → `linking with cc failed` |
| rustc-bundled `rust-lld` route | present in the toolchain (`rustlib/x86_64-unknown-linux-gnu/bin/rust-lld`) but only reachable via the unstable `linker-features`/`link-self-contained` flags — not usable on stable | `rustup toolchain` inspection |
| CI toolchain | `dtolnay/rust-toolchain@stable` on `ubuntu-latest`; **no workflow installs lld**; system deps come from identical `apt-get install` blocks: 5 in `ci.yml`, 1 each in `benchmark.yml`, `nextcloud-integration.yml`, `release.yml`, 2 in `mutation-testing.yml`, 4 in `miri.yml`, 2 in `kani.yml` | grep over `.github/workflows/*` |
| Release targets | `release.yml` matrix = `x86_64-unknown-linux-gnu` (ubuntu) + `x86_64-pc-windows-msvc` (windows) | `release.yml` lines 74–82 |
| MSRV | workspace MSRV job uses an old toolchain (1.77.2 locally pinned); `-Clink-arg=-fuse-ld=lld` is a plain link arg passed to `cc` — version-independent, MSRV-safe | ci.yml MSRV job + flag semantics |
| Existing config | no `.cargo/config.toml` in the repo — nothing to merge with | runtime report §A + worktree inspection |

## 2. Recorded decisions (every open question resolved)

**D1 — Config stanza (normative for stage 2).** New file
`.cargo/config.toml`:

```toml
# Link with lld instead of the default GNU ld (bfd) on linux x86_64.
# Why: the workspace links the full dependency graph once per
# integration-test target (98+ binaries; 70% of incremental
# --all-targets CPU time is link work) and bfd is the slowest
# mainstream linker for large statically-linked binaries (#1069).
# Constraint: `-Clinker-features=+lld` is still unstable on stable
# rustc (verified on 1.99.0), so the stable spelling is a cc-level
# link arg. Requires `ld.lld` on PATH (CI: the apt `lld` package, added
# to the workflow install steps; dev: `sudo apt install lld` once).
# Scope: rustflags are not profile-scopable in stable cargo, so this
# applies to every profile on this target — release/linux also links
# with lld (decision D3); windows-msvc and mac are untouched because
# the section is target-scoped.
[target.x86_64-unknown-linux-gnu]
rustflags = ["-Clink-arg=-fuse-ld=lld"]
```

**D2 — CI decision: extend the workflow setup (option A).** Evidence:
no runner step installs lld today, and github `ubuntu-latest` images do
not ship `ld.lld` on PATH; without it every CI link fails hard (the
config would be a shipping blocker). Add `lld` to the `apt-get install`
lists of every workflow that produces native linux binaries:
`ci.yml` (5 steps), `benchmark.yml` (1), `nextcloud-integration.yml`
(1), `mutation-testing.yml` (2 — cargo-mutants builds and links mutant
binaries; lld directly shrinks that lane), `release.yml` (1, the linux
install step). **Not** `miri.yml` / `kani.yml`: those interpret or
compile without native linking, and keeping their steps untouched keeps
the diff honest (a redundant apt package would still be harmless, so a
reviewer may add them for uniformity — recorded as acceptable).

**D3 — Release profile.** The dispatch plan's "keep release linkage
unchanged" is **not achievable with a stable, repo-level config**: cargo
has no per-profile rustflags on stable. Decision: accept that
release/linux also links with lld (it is a drop-in GNU linker; the
release.yml linux job gets `lld` via D2; stage-2 verifies one
`cargo build --release -p taskboard-app` links and the binary runs).
The stage-3 trap "(d) no other profile changed" is satisfied as: no
release-*specific* cargo setting changed, and the target-scoped stanza
is documented as necessarily profile-wide. Windows/mac never see the
flag.

**D4 — Dev-machine prerequisite.** `sudo apt install lld` (gives
`/usr/bin/ld.lld`). Passwordless sudo was unavailable to the stage-1
agent, so this one-time user step is a documented prerequisite, not
something the branch can perform. For the stage-2 AFTER measurement the
equivalent environment is `PATH=/usr/lib/llvm-21/bin:$PATH` (the exact
`ld.lld` binary the apt package would expose) — valid because D1's
mechanism is resolution of `ld.lld` on PATH, nothing else.

**D5 — No `rust-lld`/nightly route.** Both are gated on unstable flags
(§1); routing through a nightly toolchain locally would diverge the dev
compiler from CI and violates the "same toolchain everywhere" posture.

## 3. BEFORE measurement (method + numbers)

Method (identical for AFTER, per round law (a)): on the branch worktree,
warm `target/` (a full `cargo build --workspace --all-targets` first),
then:

```sh
scripts/memory-gate.sh /usr/bin/time -v cargo build --workspace --all-targets --timings
```

Record wall time, total CPU (User+Sys from `time -v`), and the
test-binary share from `target/cargo-timings/cargo-timing-*.json`
(sum of per-unit durations, split integration-test units vs others,
mirroring the runtime report's buckets). Environment: rustc 1.99.0,
linker bfd, 4 logical CPUs, warm incremental.

BEFORE numbers (captured 2026-11-19, worktree `taskboard-WT-ci-dev-lld-linker`
@ origin/main `6bd0f4dd`, rustc 1.99.0, linker bfd, 4 logical CPUs, warm
incremental tree; method: `find crates -name '*.rs' -exec touch {} +` to
invalidate only the workspace's own units (deps cached), then
`scripts/memory-gate.sh /usr/bin/time -v cargo build --workspace
--all-targets --timings`):

- **Wall 3m 09s** (`/usr/bin/time -v`: Elapsed 3:09.36), **CPU 692.98s**
  (User 494.46 + Sys 198.52), 365% CPU utilization.
- Per-unit sum from `target/cargo-timings/cargo-timing-*.html`
  (UNIT_DATA, 123 units): **1381.7s**, buckets:
  - integration-test binaries: **98 units, 967.1s = 70.0%** (matches the
    runtime report's 70% share — the method reproduces its baseline)
  - lib-as-test: 8 units, 217.3s (15.7%)
  - lib/bin: 11 units, 145.8s (10.6%); build-script: 27.3s; other-test: 24.2s

(Note: the first timed attempt on the freshly warmed tree ran 0.9s —
nothing to rebuild; the touch-invalidate step above is therefore a
mandatory part of the method, also for the AFTER run.)

## 4. AFTER measurement (stage-2 instructions)

1. Apply D1 (+D2 workflow edits). No other diff.
2. Ensure `ld.lld` resolvable (D4). Re-run the identical §3 command.
   Record wall/CPU/share; the delta is before minus after on the same
   warm-tree discipline (the config change invalidates all final
   binaries but not the compiled libs, so the run is incremental like
   the before).
3. Verify the app edit loop is unaffected: `touch src/main.rs`,
   time `cargo build` (report baseline ≈3s; the acceptance floor is
   "≤ baseline + 1s", i.e. the lld switch must not regress the
   steady-state loop).
4. Verify the release decision D3: `cargo build --release -p taskboard-app`
   links and the produced binary runs.
5. Record both deltas + environment in the chunk report.

## 5. Acceptance numbers

- A link-method change must show a measured improvement in the
  --all-targets incremental wall/CPU (the runtime report's motivation);
  a regression on either wall or CPU is a finding — the stanza would be
  reverted and the issue closed as wontfix-with-evidence.
- No quality-ceiling law applies: we assert an improvement (floor-style
  claim on a baseline), not an upper bound on success.
- CI green on the PR is the CI-parity proof (every linux build links
  through the new arg).
- Diff confined to `.cargo/config.toml` (new, ~12 lines incl. comments)
  + the apt-step edits (7–9 workflow lines). Estimated total diff:
  ~20 lines, 7 files.

## 6. Risks

- **R1**: the local lld install is non-standard (no package owns
  `/usr/bin/lld`; no `ld.lld` on PATH). Mitigated by D4's one-time
  `sudo apt install lld`; until then the standing config breaks local
  builds — stage-2 must land D4 (or measure with the D4-equivalent PATH)
  and the plan/PR body must flag the prerequisite to the user.
- **R2**: profile-wide application (D3) — release/linux linker changes;
  equivalence is verified once in stage-2 step 4, and release.yml's
  linux job installs lld so the release lane cannot break.
- **R3**: gcc-driver `-fuse-ld=lld` resolution depends on `cc` finding
  `ld.lld` in PATH — on any future dev machine lacking it, builds fail
  loudly (not silently), which is acceptable per D1's comment.
- **R4**: the report's "stable on rustc 1.99" premise was false
  (§1); the stanza uses the verified stable spelling. If a future
  rustc stabilizes `linker-features=+lld` differently, the stanza
  still works as-is (plain link arg).

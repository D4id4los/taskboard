# [Plan] Release Tooling & Launch — Chunk 14, Remediation Wave 1

- **Date**: 2026-10-18
- **Kind**: implementation plan (stage 1 of the three-stage dispatch
  protocol). TDD where code changes; the script and workflow changes
  carry a manual verification catalogue instead (no cargo lane covers
  them — recorded as a risk, §8).
- **Origin**: issues **#235** (release cut vs branch-protected `main`),
  **#238** (`release.yml` MCP guard cannot parse on the Windows leg),
  **#236** (file-manager double-click silently starts a headless
  daemon) — all normative — dispatched as chunk 14 of
  `.artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md`
  (§1, §15). Kiosk interplay constrained by the accepted design
  **#237** and its implementation plan
  `.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md`.
- **Status**: READY — every open question settled below with a
  recorded decision. Evidence anchors verified against `main` @
  `76838a1` (the branch point of `fix/release-tooling-launch`).

## 1. The three findings, precisely

1. **#235 — the release cut predates branch protection.**
   `scripts/bump_version.sh:33-41` aborts unless run *on* `main`, and
   `:92-100` commits and tags directly there. `main` is
   branch-protected (AGENTS §8): neither is possible anymore. v0.1.0
   was cut by hand (changelog promote on a side branch → PR →
   post-merge tag on the rebased commit). AGENTS.md:233-237 (§9) and
   `docs/coding_guidelines.org` §7 "Release Versioning" still describe
   the direct-commit contract, so the normative docs are stale too.
2. **#238 — the pwsh parse failure.**
   `.github/workflows/release.yml:100-103` ("MCP feature off in the
   release resolution") runs `run: '! cargo tree … | grep -q …'` under
   the job's default shell. The build matrix includes
   `windows-latest` (`release.yml:79-80`); there the default shell is
   pwsh, which cannot parse the leading `!` — the step exits 1 before
   `cargo tree` runs, the whole v0.1.0 tag run went red, and no
   Release was published. The Linux leg and the `ci.yml` twin step
   (Linux-only) are unaffected. Precedent inside the same file: the
   "Compute Archive Name" step is already pinned `shell: bash`
   (`release.yml:113-118`).
3. **#236 — bare launch from a file manager starts a headless
   daemon.** The release archive ships no config, so `[app] mode`
   resolves to its default `daemon` (`crates/taskboard-app/src/config.rs`,
   `AppMode::Daemon` is `#[default]`); a file-manager launch has no
   TTY on stdin, so the bare-invocation routing sends `daemon` to the
   daemon lifecycle (`ui.rs:79-87`, `main.rs:148-172`) — an IPC-socket
   daemon with no window, all output on discarded stderr. The routing
   today (`main.rs @ 76838a1`):

   | mode | TTY | route |
   |---|---|---|
   | desktop | any | `DesktopUi` (D4, TTY-independent) |
   | daemon (default) | yes | `Repl` |
   | daemon | no | `DaemonLifecycle` ← the trap |
   | kiosk | any | `DaemonLifecycle` (refused by the gate at `main.rs:252-258`) |

Relevant state, verified 2026-10-18: workspace version is **still
0.1.0** (`Cargo.toml` `[workspace.package]`), `CHANGELOG.md` has both
`[Unreleased]` and `[0.1.0]` sections, and tag `v0.1.0` (commit
`925f50e`) exists on origin **and is an ancestor of `main`**. A re-cut
of v0.1.0 is therefore possible (§3.4). No PRs are currently open;
chunks 11–13 of this wave have local branches but have not merged, so
this branch is cut off `main` @ `76838a1` per the dispatch's
"dispatch after chunk 11's PR is open and rebase" note (§6.4).

## 2. Decision A — the release cut on protected `main` (#235)

**Shape** (issue #235's proposed approach, adopted verbatim in
structure): the script does everything up to an open PR; the human
merges and tags; the tag push triggers `release.yml`. Direct writes to
`main` never happen.

### 2.1 The script's new contract (`scripts/bump_version.sh`)

| Step | Old | New |
|---|---|---|
| 1 preconditions | clean tree **and** on `main` | clean tree; `git fetch origin --prune`; `origin/main` must exist; new tag must be free **both** locally (`refs/tags/`) and remotely (`git ls-remote --tags origin`) |
| 2 branch | — (works on `main`) | `git switch -c chore/release-v<NEW> origin/main`; refuse if that branch exists locally or on the remote. The bump is always cut from the fetched `origin/main`, never from whatever `HEAD` happens to be |
| 3 bump | version patch via the embedded python (unchanged), `cargo update --workspace` (unchanged), changelog promote (unchanged, warning path for empty `[Unreleased]` included) | identical |
| 4 commit | on `main` | on the release branch; same message shape, body notes the release-branch flow |
| 5 tag | `git tag -a` locally | **removed from the script** — the tag must target the rebased post-merge `main` commit, which does not exist yet at script time. The script *prints* the human tag steps instead |
| 6 ship | — | unless `--no-push`: `git push -u origin chore/release-v<NEW>`, then `gh pr create` (title `chore(release): v<NEW>`, body: the promoted CHANGELOG section + the post-merge tag checklist). If `gh` is missing or fails after a successful push, print the exact manual `gh pr create` command and exit non-zero — never a half-state without instructions |

`--no-push` (new optional flag; `TASKBOARD_RELEASE_NO_PUSH=1` is not
added — one escape hatch is enough) stops after step 4 and prints the
manual push/PR commands. It exists so agents and the verification
catalogue (§5.4) can exercise the full cut without touching the
remote.

**Push authority note.** AGENTS §8 forbids pushing *without an
explicit instruction from the user*. The script is a user-invoked
tool: running it *is* the instruction, and its docs say so. An agent
may run it only in `--no-push` mode or under an explicit user order
(the wave's stage-3 push authority covers the chunk's own branch, not
release branches).

### 2.2 The human's steps (printed by the script, documented in AGENTS §9 / guidelines §7)

```sh
# 1. merge the release PR on GitHub (rebase merge)
# 2. tag the merged commit and trigger the Release workflow:
git fetch origin --prune
git tag -a vX.Y.Z -m "taskboard X.Y.Z" origin/main
git push origin vX.Y.Z
```

### 2.3 Doc updates (same commit as the script)

- **AGENTS.md §9** — new wording: bump exclusively via
  `./scripts/bump_version.sh <major|minor|patch> [--no-push]`, which
  verifies a clean tree, creates `chore/release-v<X.Y.Z>` off the
  fetched `origin/main`, bumps the version, refreshes `Cargo.lock`,
  promotes the CHANGELOG entry, commits on the branch, and (unless
  `--no-push`) pushes it and opens the release PR; the human merges
  and then tags `vX.Y.Z` on the merged `main` commit and pushes the
  tag, which triggers the Release workflow.
- **`docs/coding_guidelines.org` §7 "Release Versioning"** — the same
  five-step list rewritten to the new flow (steps 1-4 stay
  script-owned; step 5 becomes the printed human step).

**AGENTS §9's contract is preserved in shape** (the review prompt's
binding check): clean tree ✓ (precondition), bump ✓, commit ✓ (on the
release branch), tag ✓ (explicit human step printed by the script —
moved, not dropped, because the tag's target SHA only exists after
the merge).

### 2.4 Edge cases the script must handle (typed refusals, no silent paths)

dirty tree; missing `origin/main` (no remote configured); tag exists
locally; tag exists on the remote; release branch exists locally or
remotely; `[Unreleased]` empty (existing warning — the bump still
stands); `gh` absent/failing post-push (manual commands printed);
running from any branch *including `main`* is accepted — the cut is
always off `origin/main`, so "run on main" is no longer a meaningful
precondition.

## 3. Decision B — the pwsh-safe guard (#238)

### 3.1 The fix

Pin `shell: bash` on the "MCP feature off in the release resolution"
step and keep the `!` pipeline-negation semantics verbatim (issue
#238's normative approach; the precedent is the "Compute Archive
Name" step in the same file). Windows runners ship Git-for-Windows
bash with `grep`, so the pipeline is portable as written once the
shell is pinned. No other step changes.

### 3.2 Whole-Windows-leg scan (the review prompt's trap — pre-checked here)

Every `run:` that can execute on `windows-latest` (`build` job,
matrix `release.yml:72-126`): the MCP guard (fixed above), "Package
Release" / "Verify Package" (plain `python …` invocations — shell
-neutral), "Compute Archive Name" (already `shell: bash`). The
`validate` and `publish` jobs run on `ubuntu-latest` only. **No other
bash-isms exist on the Windows leg**; the fix is exactly one line
(`shell: bash`) plus no comment drift.

### 3.3 Validation without a Windows machine

1. `actionlint` over the repo (stage 2 downloads the static release
   binary; if the sandbox has no network, record that and fall back
   to 2-3 only — never claim actionlint ran when it did not).
2. `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"`
   (PyYAML is installed locally — verified 2026-10-18).
3. A one-off assertion script (committed? **No** — run ad hoc, quoted
   in the stage-2 report): every `run:` block in a job whose matrix
   includes `windows-latest` either carries `shell: bash` or contains
   no `!`-prefix / bash-only substitution.
4. *Optional, review stage:* the workflow supports `workflow_dispatch`
   with a `dry_run` input (build + package, no Release — verified in
   the file and `gh workflow list`). Stage 3 may dispatch it from the
   PR branch to exercise the real Windows leg. Cost: two ~45-min
   builds. Decision rule: do it if the PR's normal CI is green and
   the queue is idle; it is evidence, not a gate.

**Honesty clause** (binding on stages 2/3): none of 1-4 *proves* the
pwsh leg green; only the next real tag run does. The report states
exactly which validations ran.

### 3.4 The v0.1.0 re-cut (recommended follow-through, human-only)

Because `main` is still version 0.1.0 and the old tag is an ancestor
of `main`, the v0.1.0 Release is re-cuttable after this chunk merges
(and before any version-bump PR merges — the validate job requires
tag version == workspace version):

```sh
git push origin :refs/tags/v0.1.0        # delete the old tag (destructive — user's call)
git fetch origin --prune
git tag -f -a v0.1.0 -m "taskboard 0.1.0" origin/main
git push origin v0.1.0
```

This unblocks the M2 exit criterion (published Linux+Windows
binaries) and F13-06's binary-size baseline. No agent performs it
(tag rewrite is destructive); the plan records it and the PR body
surfaces it to the user.

## 4. Decision C — the GUI detection rule (#236)

### 4.1 Signal enumeration and choice

| Signal | Verdict | Why |
|---|---|---|
| **TTY presence × display presence** | **chosen** | The two orthogonal facts of the bug: a file-manager launch is exactly *no TTY + graphical session*. TTY alone can't tell file-manager from systemd/ssh (both no-TTY); display alone can't tell terminal from double-click (both have it). Together they separate all four contexts with zero platform-specific introspection. |
| parent process class (`/proc` walk: dolphin vs systemd vs sshd) | rejected | Linux-only (`/proc`), needs a different mechanism on Windows (toolhelp) and macOS; rename/spoof fragile; violates the "pure decision fn" testability rule for no gain. |
| env-var launch markers (`GIO_LAUNCHED_DESKTOP_FILE`, `TERM`) | rejected | Desktop-specific (GNOME/KDE set different vars, none on Windows); `TERM` is set by file-manager spawns inconsistently; a marker that is merely *sometimes* present produces exactly the class of false positives this chunk exists to remove. |

Display presence = `WAYLAND_DISPLAY` or `DISPLAY` is set **and
non-empty** (an empty `DISPLAY=""` is a broken-session artifact and
counts as absent). This is deliberately the same pure fn the kiosk
wave-1 plan specifies (its chunk 2: `display_available(wayland:
Option<&str>, x11: Option<&str>)` with a trivial env wrapper) — chunk
14 lands it first, in that exact shape, so the kiosk chunk consumes
rather than re-adds it. Windows/macOS never set these vars: there the
rule degrades to the TTY signal alone (§4.4).

### 4.2 The rule (new truth table for `bare_invocation_route`)

`crates/taskboard-app/src/ui.rs`:

- `pub fn display_available(wayland: Option<&str>, x11: Option<&str>) -> bool`
  — pure decision fn (new).
- `fn display_env_available() -> bool` — trivial wrapper reading the
  two vars; thin glue, untested per the repo's main-glue convention.
- `bare_invocation_route(mode, tty: bool, display: bool) -> BareRoute`
  — gains the `display` parameter; the only changed cell is bold:

| mode | TTY | display | route |
|---|---|---|---|
| daemon (default) | yes | any | `Repl` (unchanged, ADR 0011) |
| daemon | no | yes | **`DesktopUi`** — the #236 fix |
| daemon | no | no | `DaemonLifecycle` (unchanged — systemd/ssh/cron) |
| desktop | any | any | `DesktopUi` (D4, unchanged) |
| kiosk | any | any | `DaemonLifecycle` (unchanged; refused by the gate — kiosk wave-1 chunk 2 later flips this to the appliance) |

`main.rs:155-158` passes the wrapper's result as the third argument —
one-line wiring. The `BareRoute` doc comments are updated to the new
semantics ("non-TTY daemon mode" now splits by display).

### 4.3 Behavior per context, and the #237 interplay

- **Terminal** (`taskboard` from a shell): REPL under the default
  mode, GUI under `desktop` — unchanged.
- **File-manager double-click on a seat**: no TTY, `WAYLAND_DISPLAY`/
  `DISPLAY` inherited from the session → **GUI** (the fix). The
  window opens on the same in-process core the desktop mode boots
  (`run_desktop`, `ui.rs:157`); no attach to a foreign daemon (that
  is kiosk wave-2 B, untouched).
- **ssh / systemd / cron** (no TTY, no display): headless daemon
  lifecycle — unchanged. `taskboard daemon` remains the *explicit*
  headless entry (#237's ssh/maintenance path; kiosk wave-1 §2 fact 1
  already assumes this), and systemd units must keep using it — the
  ADR amendment says so explicitly, since a user service importing a
  graphical environment would otherwise flip to the GUI.
- **`ssh -X`** (display forwarded, no TTY): GUI attempt — a forwarded
  display is a display context; acceptable and documented.
- **Broken display** (var set, server dead): `run_desktop` fails
  typed at mount (exit non-zero, stderr visible) — a loud failure,
  strictly better than the silent daemon it replaces. The gate→mount
  race (display vanishing between check and mount) is tolerated and
  documented — mirroring kiosk wave-1 chunk 2's identical tolerance;
  slint 1.18.1 panics without a display rather than erroring (kiosk
  plan §2 fact 2), which is precisely why the gate is env-based and
  pre-mount.
- **Kiosk**: untouched. The daemon-owned core contract of #237 — IPC
  server, `daemon` verb, per-database socket, the mode gate — is
  byte-identical after this chunk.

### 4.4 Windows/macOS limitation (recorded, not solved)

On Windows a double-clicked console-subsystem binary gets a fresh
conhost: stdin *is* a TTY → the REPL opens in a visible console
window. Visible and killable — not the silent-daemon trap — but not
the GUI; the env-based display signal has no Windows equivalent worth
faking. The proper fix (explicit `gui` verb + launcher packaging) is
exactly kiosk wave-1 chunks 1/4 and stays there (§7). macOS is
second-priority and inherits the same story via the verb.

### 4.5 Docs for the rule

- **ADR 0015 amendment** (dated section, the house pattern): signals,
  truth table, rejected alternatives, the Windows limitation, the
  #237 interplay, the gate→mount tolerance.
- **`docs/architecture.org`** `ui` bullet (line ~286): the D4
  parenthetical gains the display split (one sentence). Chunk 12
  rewrites this file — §6.4 rebase note covers it.
- **`CHANGELOG.md`** `[Unreleased]`: one Changed entry.
- **Stage 3 adds a comment on #237**: kiosk wave-1 chunk 4's README
  table row "no-TTY → daemon" is superseded by "no-TTY + display →
  GUI; no-TTY headless → daemon", so the wave-kiosk dispatcher
  writes the table against the new semantics. (A comment, not an
  edit of that wave's plan file — keeps this branch's diff inside
  its own scope.)

## 5. Test catalogue

### 5.1 `display_available` (unit, `ui.rs`)

Decision table: `(Some("wayland-0"), None)` → true;
`(None, Some(":0"))` → true; `(Some(""), Some(""))` → false
(empty = absent); `(None, None)` → false. No IO, no display needed.

### 5.2 `bare_invocation_route` (unit, `ui.rs`)

The existing `bare_invocation_routing_maps_modes_per_d4` test owns
the full routing map and is extended (signature change forces the
edit) to the §4.2 truth table — including the display-blindness rows
(`desktop` → `DesktopUi` for all four tty×display combinations;
`daemon`+TTY → `Repl` under both display values). Typed asserts on
the `BareRoute` enum; no strings, no ceilings.

### 5.3 What must *not* change (regression proof by existing tests)

The `repl::entry_point` decision table (`repl.rs:71`, its tests at
`repl.rs:936+`) is untouched — subcommand/TTY semantics do not move.
All integration spawns (`cli_lock`, `cli_ipc`, `cli_tty`, `it_*`)
pass explicit subcommands or run under a pty, so none reaches bare
routing; CI legs have no `DISPLAY`, so even a bare spawn there would
route headless exactly as before. Verified by census @ `76838a1`
(§1.3 context); stage 2 re-runs the census before editing.

### 5.4 Script exercise (manual catalogue — no cargo lane exists for shell; §8 risk)

In a temp clone whose `origin` is a local bare repo, with a
PATH-shimmed `gh` that records argv: dirty-tree refusal; local-tag
refusal; remote-tag refusal (`ls-remote`); release-branch-exists
refusal; happy path `--no-push` (branch cut off origin/main — *not*
off the current HEAD; commit present; `Cargo.toml` bumped;
`Cargo.lock` refreshed; CHANGELOG promoted; **no tag created**;
manual push/PR commands printed); happy path with push (branch on the
remote; shim recorded `gh pr create` with the expected title shape).
Plus `bash -n` always, `shellcheck` if installable. The stage-2
report quotes the transcript; the review agent re-runs the
`--no-push` happy path and one refusal.

### 5.5 Workflow validation evidence

Per §3.3: actionlint (or its recorded absence), the YAML parse, the
Windows-leg shell assertion, and — if dispatched — the dry-run run
URL. The PR body states which ran.

## 6. Implementation shape & commit series

### 6.1 Sequencing precondition (dispatch §1's note)

Before stage 2 starts: `gh pr list` — if chunk 11
(`ci/guardrails-bencher-deny`) has an open PR that touched
`.github/workflows/`, rebase this branch onto it (or its merged
`main`); else proceed off `main`. Chunk 12 (`docs/arch-org-release`,
`architecture.org` rewrite) likewise: rebase before stage 3 ships if
it merged first.

### 6.2 Commit series (one per issue; each standalone, rebase-merge hygiene)

1. `fix(release): cut releases through a pr on branch-protected main`
   — `scripts/bump_version.sh` rewrite + AGENTS §9 + guidelines §7
   (one coherent doc+tool contract).
2. `ci(release): pin the mcp feature guard to bash for the windows
   leg` — the one-line `shell: bash` pin in `release.yml`.
3. `fix(app): route bare no-tty launches with a display to the gui`
   — `ui.rs` (`display_available`, wrapper, route signature, docs),
   `main.rs` wiring, tests (§5.1-5.2), ADR 0015 amendment,
   `architecture.org` sentence, CHANGELOG entry. TDD: tests land in
   the same commit as the routing change.

### 6.3 Gates (AGENTS §10)

Full suite after each commit; the script/workflow commits have no
cargo surface but the suite still runs (no regressions from doc
edits). No `unsafe`, no new deps — nothing here adds a crate.

### 6.4 Hand-off

Stage 3: push, PR to `main` body `Closes #235` + `Closes #238` +
`Closes #236` (+ the §3.4 re-cut note for the user), `gh pr checks
--watch` to green, comment the §4.5 supersession on #237. Never
merge; never push tags.

## 7. Non-goals (binding)

- No installer/cargo-dist work (**#114**), no new release targets
  (**#115**) — the matrix stays Linux+Windows x86_64.
- No daemon feature changes: the IPC server, lifecycle, `daemon`
  verb, mode gate, and shutdown choreography are untouched.
- No `taskboard gui` verb, no `contrib/taskboard.desktop`, no
  packaging or README "Launching" section — kiosk wave-1 chunks 1/4
  own them (`.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md`).
- No kiosk lifecycle/display-gate consumption beyond landing the pure
  fn — kiosk wave-1 chunk 2 wires it into the appliance.
- No change to the default `[app] mode` (kiosk post-wave Q6) and no
  Windows/macOS GUI-launch semantics (§4.4 — the verb fixes those).
- No tag deletion/creation/push by any agent (§2.2/§3.4 are human
  steps); no edits to the kiosk wave-1 plan file (the #237 comment
  carries the supersession).

## 8. Risks

- **Windows-leg validation is bounded, not proven** (§3.3): actionlint
  + parse + assertion + optional dry-run cannot fully substitute for
  a real tag run; the first re-cut (§3.4) is the acceptance event.
  Mitigation: honesty clause in every report; the dry-run option.
- **GUI-detection false positives**: `setsid taskboard` inside a
  graphical session, `ssh -X`, or a systemd user unit that imports
  `DISPLAY` now route to the GUI where the old code ran headless.
  Mitigation: the explicit `daemon` subcommand is the documented
  headless path (ADR amendment + #237 comment); a broken display
  fails loud and typed rather than hanging silently.
- **Routing change is unit-provable only**: CI has no display, so no
  integration lane can exercise the new GUI cell; a display-ful
  machine's manual check (run the built binary from a file manager)
  is listed as a stage-3 optional confirmation, not a gate.
- **Script testing is manual** (§5.4): no shell test lane exists in
  the gate suite; the shimmed-gh harness is best-effort. A future
  shellspec/shellcheck CI lane is a backlog-shaped idea — filed only
  if stage 3 deems it worth an issue, never silently added.
- **Cross-chunk rebase friction** (§6.1): chunks 11/12 touch
  `.github/workflows/` and `architecture.org`. Bounded by the
  rebase preconditions; conflicts are one-file-local.
- **Re-cut window**: §3.4 is valid only while `main` is still 0.1.0;
  if a release bump merges first, v0.1.0's Release stays uncut and
  the next release becomes the acceptance event instead. Surfaced to
  the user in the PR body either way.

## 9. Acceptance mapping (dispatch §15)

| Dispatch acceptance bullet | Where |
|---|---|
| release cut performable as documented; shape decided and recorded | §2 |
| `release.yml` parses and runs on the Windows leg; guard pwsh-safe; validated (actionlint / parse / dry-run) | §3 |
| GUI binary detects file-manager/no-terminal context and starts the GUI; detection signal decided; kiosk interplay with #237; headless daemon stays reachable explicitly | §4 (esp. §4.1-4.3) |
| tests/gates green where code changed | §5, §6.3 |
| review-stage checks (protected-main walk, whole-leg bash-ism scan, truth-table regression, §9 contract in shape) | §2.3, §3.2, §5.2-5.3, §8 |

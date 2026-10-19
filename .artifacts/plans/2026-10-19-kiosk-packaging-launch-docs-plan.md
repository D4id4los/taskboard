# [Plan] Kiosk Chunk 4 — packaging & launch docs (TDD-first)

- **Date**: 2026-10-19
- **Kind**: stage-1 implementation plan (dispatch plan §5, chunk 4) for
  `feat/packaging-desktop-file` — the `.desktop` launcher, its release
  staging, and the README "Launching" truth table. TDD-first in the
  packaging sense (§2: every artefact enters through a failing
  verification); no implementation in this commit.
- **Origin (normative, MD-7 rank)**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 **KDF-1**, §3 **K4**, §4–§6); the accepted design
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  (§5 items 1–2 — the P1 fix directions, §6 **Q4** — the launcher is
  `Exec=taskboard gui`, a verb not a `--mode` flag, and the env escape
  hatch stays documented as-is); the dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1/§5 (MD-6: this path; the round-wide evidence clauses; the chunk's
  traps list). Deviations from prose are recorded inline; none touches
  an ACCEPTED ruling.
- **Status**: READY — every open question settled (§3 D1–D14);
  symbols and seams re-located at plan time on base `259dca9` =
  `origin/main` (fetched 2026-10-19; local `main` was stale at
  `ac77807`, so the branch is cut from `origin/main` — the
  then-current main per the dispatch plan's mechanics, D14).

## 0. Precondition verified (MD-9)

`git fetch origin --prune`; `git merge-base --is-ancestor 1d51381
origin/main` → contained. Chunk 1 (`feat/app-gui-verb`, tip `1d51381
feat(app): the taskboard gui lifecycle verb`) is merged. Chunks 2
(`279d3b3`) and 3 (`e8257a6` + ADR 0016 `6826f09`) are merged as
well — the appliance spine K1→K2→K3 is complete, so per the
implementation plan §6 map this chunk's PR carries **`Closes #237`**
(the appliance milestone completes here).

## 1. What this chunk delivers

The P1 fix directions' packaging half (design §5 items 1–2, reshaped
by Q4): a `contrib/taskboard.desktop` launcher whose `Exec` is the
`gui` verb (never an env-var mode launch), staged into the release
archive by `scripts/package_release.py`, plus the README "Launching"
section that states the shipped bare-invocation routing as one table —
verified cell-by-cell against `bare_invocation_route` (KDF-1, which
supersedes the pre-`3ad16bb` tables) — with the env escape hatch and
the launcher install hint. With K1–K3 merged, this completes the
archive double-click story (impl plan §5 exit table: "K1, K4").

**Non-goals** (K4's list): no kiosk systemd unit (`[kiosk]` section,
`contrib/taskboard-kiosk.service` — K9), no icon asset (the
theme-fallback reference is the deliverable; the shipped SVG is the
recorded follow-up nit), no release-workflow edits (§3 D10 — none are
needed), no routing/lifecycle code changes (all landed).

### Seams re-located at plan time (base `259dca9`)

| Seam | Where | Role in this chunk |
|---|---|---|
| `bare_invocation_route` / `BareRoute` | `crates/taskboard-app/src/ui.rs:172-182` (`KioskAppliance` arm present; `KioskStart` + `kiosk_start` above it) | **the routing truth the README table mirrors** — read, not touched |
| bare-invocation wiring | `crates/taskboard-app/src/main.rs:172-175` (`display_env_available()` feeds the routing) | row citations for the table |
| `Command::Gui` arm | `main.rs:98`, before `route_for_ipc` at `:110` | the `gui` row's citation (process arm, never relayed) |
| `Command::Daemon` run arm | `main.rs:89` (`daemon_run`), headless via `daemon_lifecycle` | the `daemon`-verb row's citation |
| `STAGED_FILES` | `scripts/package_release.py:28` — flat root-relative list; copied by `shutil.copy2(ROOT/name, build_dir/name)` at `:88-89`; mirrored by `expected_members()` at `:117-121` | the single source the staging diff extends |
| `--check` | `check_archive()` at `:124-152` — membership compared against `expected_members()` | **follows `STAGED_FILES` automatically**; no second list exists |
| release workflow | `.github/workflows/release.yml:108-114` — calls `package_release.py` then `--check` per matrix leg | **zero edits** (the script is the workflow's interface) |
| `contrib/taskboard.service` | the only contrib/ resident | the conventions template: `#`-comment header as the install guide |
| README.org | Download archive sentence at ~:33-35; `gui`-verb note at ~:111-118; REPL section at ~:200 ff. with the stale no-TTY sentence at ~:236-238; `** The daemon and IPC` at ~:250 | where the table lands and what it must de-stale |
| CHANGELOG.md | `[Unreleased]` — the verb (#237) and appliance (ADR 0016) entries already present from K1/K3 | this chunk appends the launcher + docs lines |

## 2. TDD order (red → green per artefact)

This chunk writes no Rust; its "tests" are the packaging validators,
each entering red before its artefact exists (impl plan §4's gates
still run at the end — the handoff contract is unchanged):

1. **Launcher validation (red first)** — with `contrib/taskboard.
   desktop` absent, `desktop-file-validate contrib/taskboard.desktop`
   fails (no such file). Create the file (§3 D2) → validator exits 0
   with **no output** (warnings count as findings). Evidence at plan
   time: the exact §3 D2 content was validated on this dev machine —
   `desktop-file-validate` PASS, exit 0. Availability note: the tool
   (`desktop-file-utils`) is a **dev-machine reference, not a CI
   gate** (no workflow edits — non-goal); the plan's content is
   validator-verified so stage 2 re-running it is confirmatory.
2. **Staging + archive check (red first)** — with `STAGED_FILES`
   unchanged, a local `python3 scripts/package_release.py` produces
   an archive whose members do **not** include the launcher, and the
   membership assertion story is: after the D9 change but with the
   file missing from `contrib/`, `shutil.copy2` fails loudly (the
   script's own guard). Green = full cycle: `package_release.py`
   (build + archive) → `--check` prints `check passed:` →
   `tar -tzf` listing shows `taskboard-<ver>-<target>/taskboard.desktop`
   flat beside the binary. `--check` needs no edit of its own: it
   compares against `expected_members()`, which derives from the
   same list (D10).
3. **README table verification (the anti-trap protocol)** — the
   dispatch plan's #1 trap is a README table documenting pre-`3ad16bb`
   routing. The plan encodes the row↔code mapping (§3 D11's table);
   stage 2 walks it cell-by-cell while writing, stage 3 re-walks it
   at review. No row is written from memory.
4. **Full AGENTS §10 gate suite** — the tree is code-identical to
   `main` (one Python list re-shape, one new data file, docs); the
   suite still runs green end-to-end for the handoff, including the
   CI-identical three-command llvm-cov sequence.

## 3. Decisions (every open question settled)

### D1 — file home: `contrib/taskboard.desktop`

The design's own files list (impl plan §3 K4) and §5.2 ("a `contrib/`
copy for manual installs") pin `contrib/`; it is the established home
for operator-installable templates (`taskboard.service` lives there).

### D2 — the `.desktop` content (exact, validator-passed)

```ini
# taskboard desktop launcher (freedesktop Desktop Entry Specification).
#
# Install (per user, no root; the binary must be on PATH for Exec to
# resolve — e.g. ~/.local/bin):
#   mkdir -p ~/.local/share/applications
#   cp taskboard.desktop ~/.local/share/applications/
#   # some sessions cache: log out/in or re-run kbuildsycoca6 if needed.
#
# Direct double-click also works once the file is executable and
# marked trusted in the file manager.

[Desktop Entry]
Type=Application
Version=1.0
Name=Taskboard
Comment=Task management and reminders for desktop and kiosk displays
Exec=taskboard gui
TryExec=taskboard
Icon=taskboard
Terminal=false
Categories=Office;ProjectManagement;
Keywords=task;kanban;board;todo;reminder;
```

Per-key rationale:

| Key | Decision + reason |
|---|---|
| `Type=Application` | K4's stated key; the only Type that launches. |
| `Version=1.0` | The spec-version key (the entry's dialect, not the app's); conventional and explicit. |
| `Name=Taskboard` | Required key; the binary/project name. |
| `Comment=…` | One line, the README's own description ("task management and reminder") + both surfaces. |
| `Exec=taskboard gui` | **Q4 ACCEPTED ruling** — a verb, not a `--mode` flag, not an env launch; mirrors how the verb presents (`cli.rs`: desktop window in this process regardless of `[app] mode` and TTY). Never `env TASKBOARD_APP__MODE=…` (the #236 unit rule's spirit; the dispatch trap list). |
| `TryExec=taskboard` | **Warranted** (the open question the prompt names): `Exec` resolves over `PATH`, so the entry is dead in any menu until the binary is installed on `PATH`; `TryExec` makes desktop environments disable/hide it instead of showing a broken launcher. Semantics per spec: non-absolute → `PATH` lookup; absent/not-executable → entry *may* be disabled. Trade-off recorded: a DE may hide the entry in menus before the binary is on `PATH` — that is the desired guard, and the README hint (D11) orders the steps binary-first. |
| `Icon=taskboard` | K4's "theme fallback": DEs resolve the name against the icon theme and fall back to a generic application icon when no `taskboard` icon is installed. No asset ships (non-goal; the SVG nit is already recorded in the impl plan). |
| `Terminal=false` | Explicit even though it is the spec default: a double-click must never sprout a terminal; stating it keeps future edits honest. |
| `Categories=Office;ProjectManagement;` | Registered main category (`Office`) + the registered additional category for kanban/task-manager apps (`ProjectManagement`, which sits under Office in the menu-spec registry). Evidence: `desktop-file-validate` **PASS** on this exact line (unknown/unregistered categories warn); real-world precedent on this machine (`/usr/share/applications/calibre-gui.desktop` et al. carry `Office;`). |
| `Keywords=…` | Registered key; menu search sugar at zero cost. Validator-passed. |
| header comments | The `contrib/taskboard.service` convention: the `#` install-guide block *is* the documented install path for manual installs. The validator accepts `#` lines (PASS with them present). |

### D3 — no `StartupWMClass`, no `Path=`, no `MimeType`

`StartupWMClass` would need evidence of the window's actual
`WM_CLASS`/app-id under slint 1.18.1 (unverified — not guessed into a
data file); `Path=` would hard-code an install location the launcher
deliberately does not assume; `MimeType` — the app registers none.
Minimal and verified beats complete and guessed.

### D4 — archive layout: flat `taskboard.desktop` beside the binary

The archive is flat by design (binary + docs in
`taskboard-<version>-<target>/`); a nested `contrib/` directory inside
an extracted archive would be foreign. The repo keeps the template in
`contrib/` (D1); the archive copy flattens the name.

### D5 — `STAGED_FILES` becomes `(source, archive-name)` pairs

Today the list doubles as repo path and archive member name, which is
only true for root files — `shutil.copy2(ROOT / "contrib/taskboard.
desktop", build_dir / "contrib/taskboard.desktop")` would fail
(`build_dir/contrib/` is never created). The minimal honest re-shape:

```python
STAGED_FILES = [
    ("LICENSE-MIT", "LICENSE-MIT"),
    ("LICENSE-APACHE", "LICENSE-APACHE"),
    ("README.org", "README.org"),
    ("CHANGELOG.md", "CHANGELOG.md"),
    ("contrib/taskboard.desktop", "taskboard.desktop"),
]
```

with the copy loop (`for src, name in STAGED_FILES: shutil.copy2(ROOT
/ src, build_dir / name)`) and `expected_members` (iterate `name`)
following. ~10 changed lines; both archive formats share the list, so
the zip and tar.gz legs move together (release.yml's matrix covers
both).

### D6 — `--check` follows with zero separate edits

`expected_members()` is the only membership authority and it derives
from `STAGED_FILES`; `check_archive()` compares against it. Verified
by reading both sites at plan time. The stage-2 proof is the full
package→check cycle (§2 step 2).

### D7 — release workflow: zero edits

`release.yml` shells out to `package_release.py` + `--check`; the
staged list is the script's internal source of truth. No workflow
diff ships in this chunk (K4 non-goal honoured trivially — the
non-goal anticipated edits; the seam needs none).

### D8 — README: new `** Launching taskboard` subsection

Placement: inside the usage body, **immediately before `** Interactive
shell (no arguments on a terminal)`** (~README:200) — the REPL section
currently opens the routing story mid-thought; the table gives it (and
the daemon/IPC section after it) its context. Content, in order:

1. Lead: what a bare invocation runs depends on `[app] mode` (default
   `daemon`), TTY, and display presence; explicit verbs bypass the
   resolution. The table is the shipped truth (ADR 0016's routing
   table, user-facing).
2. The table — ADR 0016's rows verbatim in org syntax (all seven),
   with the row↔code verification recorded below (D9).
3. Kiosk row footnote: display-gated fullscreen appliance; with no
   display it falls back to loud headless serving (ADR 0016). No
   kiosk unit is promised (K9's; only the daemon user-unit in
   `contrib/taskboard.service` exists today — its line already
   stands in `** The daemon and IPC`).
4. Env escape hatch (Q4: "stays documented as-is"):
   `TASKBOARD_APP__MODE=desktop taskboard …` one-shot (figment `__`
   nesting, the README Configuration section's mechanism), or
   `[app] mode = "desktop"` in `taskboard.toml`; the same env form
   takes any mode value.
5. Launcher install hint: the release archive ships
   `taskboard.desktop` (repo copy: `contrib/`); install = binary on
   `PATH` (e.g. `~/.local/bin`) + copy the file to
   `~/.local/share/applications/` (or mark it executable + trusted
   and double-click it in the archive). `Exec=taskboard gui` — the
   double-click path is the `gui` verb row.

### D9 — the row↔code verification (cell-by-cell, re-walked at stages 2 and 3)

| Table row | Code truth (verified @ `259dca9`) |
|---|---|
| bare, `mode = "desktop"` | `AppMode::Desktop => BareRoute::DesktopUi` (`ui.rs`) — no TTY/display guard on the arm |
| bare, `mode = "daemon"`, TTY | `AppMode::Daemon if tty => BareRoute::Repl` — TTY wins over display (arm order) |
| bare, `mode = "daemon"`, no TTY, display | `AppMode::Daemon if display => BareRoute::DesktopUi` — **the #236 display-aware cell** (the classic miss) |
| bare, `mode = "daemon"`, no TTY, no display | `AppMode::Daemon => BareRoute::DaemonLifecycle` — systemd/ssh/cron |
| bare, `mode = "kiosk"` | `AppMode::Kiosk => BareRoute::KioskAppliance` — TTY-independent; the display gate is *inside* the lifecycle (`kiosk_start(display_env_available())`, `main.rs:416`), which is why the row says "any/any" |
| explicit `taskboard daemon` | the run arm at `main.rs:89` bypasses bare routing; headless always (mode gate refuses `Desktop` only — K3) |
| explicit `taskboard gui` | handled at `main.rs:98`, *before* `route_for_ipc`; desktop window in this process regardless of mode/TTY; typed `instance-locked` on a running core (ADR 0009 amendment) |

### D10 — README de-staling edits (the rest of the README diff)

- **The stale sentence** in `** Interactive shell` (~:236-238):
  "Without a TTY (systemd units, pipes), bare =taskboard= keeps the
  old behavior: the =[app] mode= resolution (daemon default)." —
  written before `3ad16bb`; wrong for the no-TTY+display case (the
  #236 cell boots the GUI). Replaced with a one-line pointer to the
  new table (`[[*Launching taskboard]…]` org link, the README's
  existing convention, cf. its `[[*The daemon and IPC]…]` link).
- **Download section** (~:33-35): the archive-contents enumeration
  ("the =taskboard= binary, both licenses, the README, and the
  changelog") gains the launcher — this sentence documents
  `STAGED_FILES` in prose and must match it exactly.
- The existing `gui`-verb note (~:111-118, K1's insertion) stays —
  correct as written; the table now also carries the row.

### D11 — CHANGELOG lines (Unreleased → Added, exact text)

```markdown
- Release archives ship a `taskboard.desktop` launcher
  (`Exec=taskboard gui`, `TryExec` PATH guard, theme-fallback icon)
  and the repo carries the installable copy in `contrib/`; the
  double-click story completes with the `gui` verb and the #236
  display-aware routing (#237).
- README "Launching" section: the bare-invocation routing table
  (mode × TTY × display, plus the `gui`/`daemon` verbs and the
  `TASKBOARD_APP__MODE` escape hatch) verified against the shipped
  routing.
```

(The verb and appliance entries already sit in Unreleased from K1/K3 —
the dispatch trap "missing CHANGELOG entries for verb+appliance+
launcher" resolves as verb✓ appliance✓ from those chunks, launcher✓
from this one; stage 3 re-checks all three are present at PR time.)

### D12 — verification commands (the chunk's acceptance evidence)

1. `desktop-file-validate contrib/taskboard.desktop` → exit 0, no
   output (dev machine; reference, not a CI gate).
2. `python3 scripts/package_release.py` → archive built with the
   launcher; `python3 scripts/package_release.py --check` → `check
   passed:`; `tar -tzf target/dist/taskboard-<ver>-<target>.tar.gz`
   shows `…/taskboard.desktop` flat beside the binary.
3. README table re-walked against `bare_invocation_route` + the two
   verb arms (D9's protocol).
4. Full AGENTS §10 suite green (code-identical tree; the contract
   stands).

### D13 — commit shape

Stage 2 lands standalone commits (rebase-merge hygiene), suggested
split: (i) `feat(packaging): taskboard desktop launcher + release
staging` (`contrib/` + `package_release.py` + CHANGELOG line 1),
(ii) `docs(readme): the launching table` (README + CHANGELOG line 2).
Stage 3's PR body carries `Closes #237` (impl plan §6: K4 closes the
appliance milestone).

### D14 — base branch recorded

`feat/packaging-desktop-file` cut at `origin/main` = `259dca9`
(2026-10-19). Local `main` (`ac77807`) was behind; per the dispatch
plan's mechanics ("branched off the then-current `main`") the fetched
`origin/main` is that main. Not stacked on anything (MD-2 default).

## 4. Risks & mitigations

| Risk | Mitigation |
|---|---|
| README table drifts from routing (the #1 trap) | D9's row↔code table is the protocol; stages 2 and 3 each re-walk it; no row written from memory or from the 2026-10-16 wave-1 plan (superseded by KDF-1) |
| `STAGED_FILES` re-shape breaks a release leg | Both formats share `expected_members`; stage 2 runs the real package→check cycle locally; the change is a pure-Python list re-shape, no Windows-specific path touched |
| TryExec hides the entry on machines without the binary on PATH | Intended semantics; README hint orders binary-first; direct activation still attempts `Exec` where a DE ignores TryExec — no regression vs. no TryExec |
| Icon absent → generic fallback icon in menus | Accepted by design (K4: "theme fallback … shipped SVG icon stays a follow-up nit") |
| `desktop-file-validate` absent on some dev machines | Reference tool only; the content is validator-passed at plan time and the file is plain data — no build or CI path depends on the tool |
| Local `main` staleness confuses the handoff | D14 records the base; stage 3 rebases onto `origin/main` first regardless (its prompt's step 1) |

## 5. Out-of-scope ideas (stage report, not autonomous issues — MD-4)

- A CI leg running `desktop-file-validate` (would need
  `desktop-file-utils` installed on the runner — a workflow edit,
  explicitly a non-goal here).
- The shipped SVG icon asset (already the impl plan's recorded nit).
- A kiosk system unit + `[kiosk]` config (K9's deliverable, not to be
  foreshadowed in README beyond the routing row + ADR 0016 pointer).

## 6. Estimated diff

| File | Shape |
|---|---|
| `contrib/taskboard.desktop` | +~30 lines (new, D2 verbatim) |
| `scripts/package_release.py` | ~10 lines changed (D5) |
| `README.org` | +~55/−3 lines (new subsection + table + two de-staling edits) |
| `CHANGELOG.md` | +~10 lines (D11) |

Total ≈ **105 lines** across four files — an S chunk as dispatched.

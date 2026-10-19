# [Plan] Kiosk Chunk 1 — the `taskboard gui` verb (TDD-first)

- **Date**: 2026-10-19
- **Kind**: stage-1 implementation plan (dispatch plan §2, chunk 1) for
  `feat/app-gui-verb` — the explicit desktop-frontend verb. TDD-first;
  no implementation in this commit.
- **Origin (normative, MD-7 rank)**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 KDF-1/KDF-4/KDF-6, §3 K1, §4, §5, §6); the accepted design
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md` (§4
  recommendation 1, §5 items 3–4, §6 **Q1** (Option A with the verb in
  the wave) and **Q4** (a verb, not a `--mode` flag)); the dispatch
  plan `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1/§2 (MD-1: no round gate; MD-6: this path). Deviations from prose
  are recorded inline with reasons; none touches an ACCEPTED ruling.
- **Status**: READY — every open question settled below (§3 decisions
  D1–D12); symbols re-located at plan time (base `ac77807` =
  `origin/main`, fetched 2026-10-19).

## 1. What this chunk delivers

`taskboard gui` — the explicit desktop-frontend verb (Q4): boots the
core and runs the desktop UI lifecycle **in this process**, regardless
of `[app] mode` and regardless of TTY. A running instance on the same
database fails typed `instance-locked` (the existing ADR 0009 lock
rules; `run_desktop` boots a core, CLI verbs never contend). The verb
is a **process-lifecycle arm** like the `daemon` run arm: handled in
`main.rs::run` *before* the IPC routing decision, never dispatched,
never relayed, refused in-session (REPL and daemon socket). ADR 0009
gains a dated amendment recording the verb and the never-relayed rule.

**Non-goals** (other chunks'): no kiosk routing or `BareRoute` change
(K3), no window profile / `UiSettings.kiosk` (K2), no `.desktop` file
or README launch table rewrite (K4 — this chunk's README touch is two
minimal insertions, §3 D11), no mode-gate change (the gate stays
chunk 3's), no `run_desktop` logic changes (glue stays glue).

### Seams re-located at plan time (base `ac77807`)

| Symbol | Where (plan-day lines) | Role |
|---|---|---|
| `Command` catalogue | `crates/taskboard-app/src/cli.rs:29-114` | the enum to extend; doc-comment conventions on every arm |
| `run` | `crates/taskboard-app/src/main.rs:79-183` | the arm's home; `cli.command.take()` guard at 89-91, `route_for_ipc` call at 95, bare block at 151-176 |
| `route_for_ipc` + `IpcRoute` | `crates/taskboard-app/src/commands/mod.rs:1383-1418` | the exhaustive transport table ("the compiler forces every future verb to pick a lane") + `ipc_routing_tests` at 1507 |
| `dispatch` | `commands/mod.rs:426-801` | exhaustive; `Command::Daemon { .. } => unreachable!("main handles the daemon arms itself")` at 799 is the precedent |
| `dispatch_line` | `commands/mod.rs:1058-1101` | exhaustive; intercepted-verb unreachable group at 1066-1078 |
| `resolve_words` / `LineRefusal` | `crates/taskboard-app/src/repl.rs:397-497` | the shared REPL **and** IPC-server parse layer (ADR 0012: "the argv words *are* the wire"); refusal arms at 490-491 |
| IPC `run_argv` | `crates/taskboard-app/src/ipc/server.rs:486-561` | flows through `resolve_words`; `LineAction::Refuse` → code 2 |
| `CompletionSpec::from_cli` / `REFUSED_VERBS` | `crates/taskboard-app/src/completion.rs:216-239, 319-357` | walks the real clap tree; refused verbs never offered (module invariant, lines 33-41) |
| `bare_invocation_route` + tests | `crates/taskboard-app/src/ui.rs:112-121, 311-351` | **untouched** — the gui verb bypasses bare routing by design |
| `run_desktop` | `ui.rs:191-244` | the lifecycle the arm calls; unchanged |
| Routing pins | `tests/cli_tty.rs` (`tty_explicit_daemon_…` at 789), `tests/dispatch_routing.rs` | binary/pty + in-process routing precedents |
| ADR 0009 | `docs/adr/0009-cli-surface-instance-lock-binary-e2e.org` | amendment section convention (the 2026-10-11 WI-3 amendment at line 167) |

## 2. TDD order (red → green per step; house rules throughout)

Every test asserts typed outcomes / structure — no text assertions on
Display strings, help text, or envelopes beyond parsed-JSON structure.
No sleeps; nothing here is time-dependent. One behavior, one test, at
the lowest layer that owns it.

1. **clap shape (red first)** — `cli.rs` tests: `taskboard gui` parses
   to `Some(Command::Gui)`; global flags compose (`--json gui`,
   `--config <path> gui`); the verb takes no arguments of its own.
2. **never-relayed lane (red first)** — `commands/mod.rs`
   `ipc_routing_tests`: `route_for_ipc(&Command::Gui) ==
   IpcRoute::Local` (D3) — the "no gui lane on the wire" negative.
3. **session refusal (red first)** — `repl.rs` tests:
   `resolve_words(["gui"])` → `LineAction::Refuse(LineRefusal::Gui)`
   (typed enum equality — the Display text is never asserted). By
   construction (shared seam) this also pins the daemon-socket refusal
   — no separate server test (one behavior, one test, lowest layer).
4. **completion refusal (red first)** — `completion.rs` tests: the
   spec walk pins `gui` in the refused set (not offered at word 0;
   `routed(&spec, "gui ", 4) == Kind::None`).
5. **binary arm-reached (red first)** — `tests/it_cli.rs` (J-series):
   `taskboard --json --config <garbage> gui` exits 1 with the parsed
   `{"error":{"kind":"decode"}}` envelope on stderr (D8) — proves the
   verb is recognised, reaches config load, and reports typed —
   **without a TTY** (pipes; contrasting the REPL's TTY rule).
6. **Green**: implement D1–D7 (§3) in the order cli → repl →
   completion → commands/mod arms → main arm; the four matches must
   compile-exhaustive again.
7. Full gate suite (§5).

## 3. Decisions (each open question settled)

### D1 — `Command::Gui` clap shape and doc comment

A unit variant appended **after `Daemon`** in the catalogue (the enum
order is help-output order; appending keeps the diff minimal and the
diffusion-zones — `route_for_ipc`'s grouped arms — unaffected):

```rust
/// Run the desktop UI in this process: explicit frontend selection
/// (the `daemon` verb's mirror) — boots the core and opens the
/// desktop window regardless of `[app] mode` and of TTY. A running
/// instance on the same database fails typed `instance-locked`.
Gui,
```

The doc carries exactly K1's three facts: explicit desktop-frontend
selection; in-process core + window regardless of mode; the typed
`instance-locked` failure. `--json` affects only the config-failure
envelope (same as the bare `DesktopUi` arm); the success path is a
window, not a payload stream — the ADR 0009 "`daemon` ignores `--json`"
line gets its `gui` parallel in the amendment (D9).

### D2 — `main.rs::run` insertion point and ordering rationale

Inside the taken-command guard, as its **first statement** — before
`route_for_ipc(&command)`:

```rust
if let Some(command) = cli.command.take()
    && !daemon_run
{
    // The explicit `gui` verb is a process-lifecycle arm (the daemon
    // run arm's mirror): frontend selection, not a command to relay
    // or dispatch — a relay would try to open the window in the
    // daemon's process. Handled before the transport routing; the
    // lane table pins it `Local` (never relayed).
    if matches!(command, Command::Gui) {
        let config = match AppConfig::load(cli.config.clone()) {
            Ok(config) => config,
            Err(err) => std::process::exit(i32::from(
                report(cli.json, &CommandError::from(err)),
            )),
        };
        return taskboard_app::ui::run_desktop(config).await;
    }
    match route_for_ipc(&command) { /* unchanged */ }
```

Rationale, in order: (1) **Before `route_for_ipc`** — the routing
decision exists to classify relaying/dispatch; the verb must never
enter it (the dispatch plan's ordering trap). (2) **Inside the
taken-`Some(command)` block** — the verb *is* a subcommand;
`bare_invocation` (computed before the take, `false` here) and the
bare-routing block are untouched, so `bare_invocation_route` and its
test matrix stay byte-identical. (3) **Config-failure reporting**
mirrors the bare arm's contract exactly: typed envelope via
`report(cli.json, …)`, exit 1 (`--json` honored) — the pattern the
bare `DesktopUi`/`Repl` arms and `status_command` already use. (4)
`matches!` binds nothing, so `command` stays usable by the later
arms. The block comment states the constraint (why the arm sits
here), per the comment discipline.

### D3 — `route_for_ipc` arm: `IpcRoute::Local` (recorded reading of "gui-blind")

The table's own contract is exhaustiveness ("the compiler forces
every future verb to pick a lane" — the enum doc at
`commands/mod.rs:1390`). A new variant therefore **must** appear in
`route_for_ipc`; "gui-blind / no arm" is unimplementable literally.
Decision: **`Command::Gui => IpcRoute::Local`**, doc-commented:

> `Local` = always in-process: the gui verb is a lifecycle arm `main`
> intercepts before routing ever sees it; pinning `Local` here keeps
> the never-relayed rule compiler- and test-enforced — any refactor
> that routes it onto the wire changes this pinned lane.

`Local` is semantically true (the verb always runs in-process) and
keeps a hypothetical future caller on a non-relaying path instead of
a panic. Rejected: `unreachable!()` — a panic in a pure, unit-pinned
routing function is a landmine, and the `Daemon` precedent doesn't
apply (`daemon stop` legitimately maps `IpcOnly`; `run_argv`'s real
Daemon handling needs the lane). **Stage-3 note**: this arm is *not*
the dispatch plan's "a gui arm in `route_for_ipc`" trap — that trap
names an arm assigning an **IPC lane** (`IpcOrLocal`/`IpcOnly`),
whose consequence ("relaying it would open the window in the daemon
process") `Local` cannot produce. The pinned test (§2 step 2) is the
dispatch acceptance's "negative asserting `route_for_ipc` has no gui
lane": the verb never takes an IPC lane.

### D4 — `dispatch` arm: unreachable, the Daemon-run precedent

`Command::Gui => unreachable!("main handles the gui lifecycle arm
itself")`, beside `Command::Daemon { .. } => unreachable!(…)` (the
exact house precedent, `commands/mod.rs:799`). Accordingly
`tests/dispatch_routing.rs` gains **no** Gui test — that suite's
"one test per `Command` variant" excludes main-owned arms today (the
daemon run arm has none); routing for gui is pinned one layer up
(§2 steps 1–2, 5).

### D5 — `dispatch_line` arm: joins the intercepted-verb unreachable group

`Command::Gui` joins the `Connect|Login|Logout|Config|Daemon`
unreachable group at `commands/mod.rs:1066-1078`, with the arm's
doc string updated to name it. Both session frontends refuse gui at
the parse layer (D6) before `dispatch_line` can ever see it.

### D6 — session refusal: one shared arm in `resolve_words` + `LineRefusal::Gui`

`resolve_words` (the seam the REPL **and** the IPC server both feed —
ADR 0012) gains, **before** the `Some(command) => Execute(…)`
fallthrough (ordering is load-bearing: after it, a `gui` line would
reach `dispatch_line`'s unreachable arm):

```rust
Some(Command::Gui) => LineAction::Refuse(LineRefusal::Gui),
```

plus the variant:

```rust
/// The desktop UI is a process entry, not a session line.
Gui,
```

with a `Display` guidance line in the house voice (≈ "the desktop UI
runs as its own process; exit and run `taskboard gui`") — UX text,
never asserted. Consequences that fall out for free: the daemon
socket refuses a hand-crafted `gui` relay at the parse layer
(`run_argv`'s existing `Refuse` path, code 2) — wire-side enforcement
of never-relayed with **zero** server changes; the module-level
routing invariant in `dispatch_line`'s doc stays true.

Rationale vs. the alternative (executing `gui` in-session): the verb
is frontend *selection* (Q4) — mounting a window from inside a REPL
line would suspend the session loop behind the slint event loop and
contradict the lifecycle-arm model; refusing like `daemon` is the
coherent treatment.

### D7 — completion surface: `gui` joins `REFUSED_VERBS`

`REFUSED_VERBS` becomes `["daemon", "config", "gui"]` (`[&str; 3]`)
and the module-doc invariant sentence (completion.rs:33-41) gains
`gui` with its reason (a lifecycle arm; completing it would
manufacture a guaranteed in-session refusal). The walk itself needs
no code — `from_cli` filters refused verbs generically; gui is a flat
verb with no subcommands/flags/positionals, so `groups`/`flags`/
`positionals` simply never see it. Test updates: the spec-walk pin's
refused loop gains `"gui"`, the offered-verbs assertions stay
gui-free, and the plan-routing pin gains
`routed(&spec, "gui ", 4) == Kind::None`.

**Recorded deviation from dispatch prose** (allowed by MD-7; no
ACCEPTED ruling addresses completion): the dispatch plan's "completion
entry if subcommands are enumerated" read as *an offered entry* would
violate this module's own core honesty invariant ("Session-refused
surface is not offered … completing them would manufacture a
guaranteed in-session refusal") once D6 refuses the verb. The entry
`gui` gets in the completion surface is therefore the **refusal
treatment** — consistent with `daemon`, which the dispatch plan
itself never proposed offering. `completion.rs` is still a touched
file exactly as K1's file list names it.

### D8 — the routing-test matrix (TTY × mode × gui)

The full decision the dispatch acceptance sketches — "the gui arm
chosen regardless of TTY and of `[app] mode`" — is pinned as follows,
because the behavior is *structural*: the D2 arm contains **no**
mode branch and **no** TTY branch (it never calls
`bare_invocation_route`, never reads `is_terminal`) — there is no
branch to truth-table, and inventing a decision seam purely to test
an absent branch is the speculative-generality trap the code-quality
mandate forbids.

| Layer | Pin | What it proves |
|---|---|---|
| Unit (cli.rs) | parse of `gui` + globals | the verb is a first-class subcommand, not mode-dependent input |
| Unit (ipc_routing_tests) | `route_for_ipc(Gui) == Local` | never relayed — no IPC lane under any circumstance |
| Unit (repl.rs) | `Refuse(LineRefusal::Gui)` | the verb is refused in-session (both frontends, shared seam) |
| Unit (completion.rs) | refused-set membership | the menu never manufactures the refusal |
| Binary (it_cli.rs, no TTY) | exit 1 + parsed `{"error":{"kind":"decode"}}` envelope | the arm is reached without a TTY and loads config before anything else |
| Review (stage 2/3) | the D2 arm has no mode/TTY branch; `bare_invocation_route` and its tests are byte-identical | "regardless of mode × TTY" holds by construction |

Deliberately excluded: a display-less `gui` success-path binary test
would pin slint's backend-init **panic** (exit 101, KDF-2 evidence) —
a foreign-crash contract, not ours; the display-gated behavior is
K3's appliance work. Also excluded: distinguishing gui from
bare-routing at the binary level under `mode = "kiosk"`/instance-lock
(every wrong routing also exits 1 — not honestly distinguishable
without text assertions).

### D9 — ADR 0009 amendment text (settled outline, appended section)

`* Amendment 2026-10-19 — the =gui= lifecycle verb` at the end of ADR
0009 (the 2026-10-11 amendment is the format precedent), four short
blocks:

1. **The verb** (Q1/Q4): `taskboard gui` is explicit desktop-frontend
   selection, the `daemon` verb's mirror; boots the core and runs the
   desktop UI lifecycle in-process regardless of `[app] mode` and TTY
   — mode is the *default* frontend, the verb the *override* (ADR
   0008's model). `--json` shapes only the config-failure envelope
   (the `daemon` ignores-`--json` line's parallel). A running
   instance fails typed `instance-locked` unchanged (this section's
   own lock rules; CLI verbs never contend).
2. **A third command class — lifecycle arms**: like the daemon run
   arm, main-owned: handled before transport routing, never
   dispatched (`dispatch`'s unreachable arm), pinned `IpcRoute::Local`
   — **never relayed** (a relay would open the window in the daemon's
   process) — and refused in-session (REPL line and daemon socket:
   `LineRefusal::Gui`, code 2 over the wire; tab completion never
   offers it).
3. **Interplay with #236 routing** (3ad16bb / ADR 0015's amendment):
   the verb is the escape hatch that makes bare-invocation routing
   mode-dependent *safely* — an explicit frontend needs no display
   gate heuristic; the `.desktop` `Exec` line lands on it (K4).
4. **Non-goals named**: kiosk routing/window profile are the kiosk
   wave's later chunks; Q6's default-mode question is untouched.

### D10 — CHANGELOG line (Unreleased → Added)

```markdown
- `taskboard gui`: explicit desktop-frontend verb — boots the core
  and opens the desktop window in this process regardless of
  `[app] mode` and TTY; never relayed over IPC and refused as a
  REPL/daemon-socket line (a process entry, like `daemon`) (#237).
```

### D11 — README touch (minimal; K4 owns the table)

Two insertions only: (1) §"3. Run the Application" gains one line in
the src block (`cargo run -p taskboard-app -- gui`) plus one sentence:
the verb opens the desktop window regardless of `[app] mode` — the
explicit counterpart to the mode-based bare routing (full launch
table is the packaging chunk's). (2) The REPL section's refused list
(`Refused in-session: =daemon=, …`) gains `=gui=` — one word, keeping
the doc true the day the refusal lands. The pre-`3ad16bb` stale
sentence at README.org:234-236 ("Without a TTY … keeps the old
behavior") is **left for K4's verified rewrite** — out of scope here.

### D12 — exhaustive-match ripple (complete list, verified by grep at plan time)

Exactly four exhaustive `Command` match sites exist in `src/`; each
gets exactly the arm decided above: `route_for_ipc` (D3), `dispatch`
(D4), `dispatch_line` (D5), and `main.rs::run`'s guard body (D2 — the
interception itself). Non-exhaustive pattern sites need only the D6
guard (`repl.rs::resolve_words`; the IPC server's terminal-bound
refusal and `strip_matching_config` are unaffected). No other file
matches over `Command` (verified: `Command::Sync|Status|Daemon` grep,
2026-10-19). `raw_argv` capture is untouched (the verb never relays).

## 4. Files & estimated diff

| File | Change | ~Lines |
|---|---|---|
| `crates/taskboard-app/src/cli.rs` | `Command::Gui` + doc (D1); parse tests (§2.1) | 25 |
| `crates/taskboard-app/src/main.rs` | the D2 arm + comment | 15 |
| `crates/taskboard-app/src/commands/mod.rs` | D3 lane + doc, D4, D5 arms; lane test (§2.2) | 25 |
| `crates/taskboard-app/src/repl.rs` | D6 variant + Display + arm; refusal test (§2.3) | 20 |
| `crates/taskboard-app/src/completion.rs` | D7 refused set + module doc; two test updates (§2.4) | 14 |
| `crates/taskboard-app/tests/it_cli.rs` | binary arm-reached test (§2.5) | 30 |
| `docs/adr/0009-…org` | D9 amendment | 35 |
| `README.org` | D11 two insertions | 5 |
| `CHANGELOG.md` | D10 line | 6 |

**Total ≈ 175–230 lines over 9 files** — house scale **S**, as
dispatch-chunk sized. No new dependencies; no `Cargo.toml` changes;
SPDX headers: all touched crates are existing files (AGPL-3.0-only,
`taskboard-app` carries `#![forbid(unsafe_code)]` — nothing here
touches that).

## 5. Quality gates & handoff

The full AGENTS §10 suite, in order: `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo
nextest run --workspace`; `cargo test --doc --workspace`; `cargo deny
check`; the CI-identical three-command llvm-cov sequence +
`scripts/coverage-quarantine-profraw.sh` + `lcov.info` + the trailing
`rm -rf target/llvm-cov-target`. Commits standalone (rebase-merge
hygiene): implementation is expected as 1–2 commits
(`feat(app): the taskboard gui lifecycle verb` + optionally
`docs(adr): gui verb amendment to ADR 0009` if the review prefers the
split) — never WIP noise. No push, no PR, no tracker edits at stage 2
(MD-4: the #237 annotation is stage 3's, exactly one comment).

## 6. Risks

1. **The D3 reading vs. the dispatch trap wording** — the highest
   coordination risk: stage 3 is primed to flag "a gui arm in
   `route_for_ipc`". Mitigated: D3 records the exhaustiveness fact,
   the semantic truth of `Local`, and the distinction (trap = an IPC
   lane); this plan is stage 2/3's normative input (MD-7 rank 4). If
   the user rules D3 wrong at review, the fallback is `unreachable!()`
   — a one-line change.
2. **Coverage of the D2 arm** — the main-binary arm is invisible to
   llvm-cov (subprocess-uninstrumented); the binary test (§2.5)
   carries the duty at the J-series tier, as ADR 0009's tier split
   prescribes.
3. **Stale README sentence** (README.org:234-236) — knowingly left
   (K4 owns the verified table); noted so stage 3 doesn't "fix" it
   out of scope.
4. **`it_cli.rs` flake surface** — the new binary test is a plain
   pipes run (no pty, no wiremock, no sockets): the same class as the
   existing garbage-config binary tests; no new flake vector
   identified.

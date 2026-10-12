# [Plan] App Config DX Follow-ups: XDG Write Target + Config Error Kinds

- **Date**: 2026-10-12
- **Kind**: stage-1 implementation plan (WI-D of the 2026-10-12 backlog
  batch) — two standalone code commits (D1, D2) plus the shared backlog
  bookkeeping commit.
- **Origin**: backlog entries "[2026-10-11] XDG write-target flip for
  `login`" (backlog line 1375) and "[2026-10-11] `CommandError::Config`
  kind refinement" (line 1420); dispatch plan §4 WI-D.
- **Target**: `crates/taskboard-app/src/config.rs`,
  `crates/taskboard-app/src/commands/mod.rs`,
  `crates/taskboard-app/src/commands/config.rs`,
  `crates/taskboard-app/tests/{cli_login.rs, cli_output.rs, config.rs,
  dispatch_routing.rs}`,
  `.artifacts/taskboard-potential-improvements-backlog.md` (final commit).
- **Branch / worktree**: `feat/app-config-dx-followups` in
  `../taskboard-WT-feat-app-config-dx-followups`, off `main` @ `755d0a7`.
- **Goal**: (D1) `login` on a machine with *no config anywhere* persists
  to `<config-dir>/taskboard/taskboard.toml` instead of `./taskboard.toml`,
  with every existing install's read resolution and write target provably
  unchanged; (D2) the `--json` error envelope carries the specific
  `ConfigError::kind()` discriminants instead of the flat `"config"`.
- **Status**: DRAFT — awaiting user review; stage 2 implements only after
  approval.

---

## 1. Context & Findings (verified 2026-10-12 against `main` @ `755d0a7`)

1. **The read side already resolves XDG.** `AppConfig::config_path`
   (`config.rs:282-291`): explicit `--config` → `TASKBOARD_CONFIG` →
   the XDG candidate *when present* (`xdg_if_present`,
   `config.rs:198-200`, over the pure `xdg_candidate`,
   `config.rs:191-194`) → `None`. `None` means the provider chain reads
   `DEFAULT_CONFIG_PATH = "./taskboard.toml"` (`config.rs:27`), an absent
   file contributing nothing.
2. **The write side still falls back to CWD.** `login_command`
   (`commands/mod.rs:653-682`) computes its single `path` once —
   `config_path(...).unwrap_or_else(|| DEFAULT_CONFIG_PATH)`
   (`mod.rs:659-660`) — and feeds the same path to `extract_raw` (read)
   and `login::run` (write). On a fresh machine (no `--config`, no
   `TASKBOARD_CONFIG`, no XDG file) that path is `./taskboard.toml`, and
   `write_config_overlay` (`commands/login.rs:170-194`) creates it
   (already `create_dir_all`-ing the parent, `login.rs:182-186`, so a
   not-yet-existing `<config-dir>/taskboard/` needs **no writer change**).
3. **`login` is the only config-creating path.** `strip_config_keys`
   (`commands/logout.rs:94-134`) returns early when the file is missing;
   `config print` never writes; the REPL refuses `login`/`logout`
   mid-session (`repl.rs:277-278`); every binary-level e2e invocation
   passes `--config` explicitly (`it_cli.rs:35`, `cli_tty.rs:85`), and
   `common::binary_command` strips all `TASKBOARD_*` env from children
   (`tests/common/mod.rs:386-397`). The flip's blast radius is exactly
   `login_command`'s `None` arm.
4. **L7 already pins the explicit-missing rule.**
   `l7_login_creates_an_explicit_missing_config`
   (`tests/cli_login.rs:265-290`): `login --config <missing>` creates
   that file. `TASKBOARD_CONFIG` behaves symmetrically on the read side
   (`config_path` returns it *without* an existence check), but nothing
   today tests that env arm compositionally — nor the XDG-if-present arm
   (only the pure helpers have unit tests, `config.rs:582-608`).
5. **D2's collapse point is one match arm.** `error_kind`
   (`commands/mod.rs:223-258`) maps `Self::Config(_) => "config"`
   (`mod.rs:225`) while `ConfigError::kind()` (`config.rs:241-252`)
   already returns the stable kebab discriminants
   `file-not-found` / `decode` / `invalid-server-url` /
   `empty-username` / `invalid-duration` / `empty-db-path`, pinned by
   `config_error_kinds_are_stable_discriminants` (`config.rs:610-636`).
   The `kind()` doc explicitly reserves the flat behavior to
   `config print` (`config.rs:237-241`) — that sentence dies with D2;
   so does the `print` doc's "the flat `config` kind" phrasing
   (`commands/config.rs:16-18`).
6. **D2's in-tree impact is exactly three test sites** (workspace grep
   for the `"config"` kind): `tests/cli_output.rs:191` (O3's exhaustive
   expected-table arm; the sample list constructs
   `CommandError::from(ConfigError::InvalidServerUrl)` at ~line 240),
   `tests/dispatch_routing.rs:497` (status + garbage file envelope),
   `tests/config.rs:324` (config print + garbage file). **No other JSON
   consumer exists**: the IPC wire (`src/ipc/*`) has its own typed error
   set and never re-maps `CommandError::error_kind`; the REPL reports
   through the same `report_to`; the J-series binary tests assert no
   config kinds.
7. **The wire change is sanctioned.** Backlog "[2026-10-10] Versioned
   `--json` schema" status note (backlog lines 1194-1199): payload
   shapes "stay explicitly unversioned; when an external consumer
   appears, payload versioning copies the envelope pattern". The D2
   origin entry deferred this change "so the wire format changes once,
   together with WI-5's IPC payload pass" — WI-5 (ADR 0012) has landed;
   this *is* the planned single change point. Workspace version is
   pre-0.1.0.
8. **Kind-string collision audit**: none of the six `ConfigError` kinds
   collides with any existing `error_kind` / `domain_error_kind` string
   (nearest neighbor `config-write`, distinct). The flat `"config"`
   string disappears from the envelope space entirely.
9. **Env control is the established hermetic pattern.**
   `common::{env_lock, EnvOverride::set, remove_env}`
   (`tests/common/mod.rs:57-110`) serialize process-env mutation;
   `dirs::config_dir()` on Linux reads `XDG_CONFIG_HOME` (absolute) or
   `$HOME/.config` at call time, so setting `XDG_CONFIG_HOME` to an
   absolute tempdir hermetically steers the resolution.

## 2. Design Decisions

| # | Decision | Choice & rationale |
|---|---|---|
| D1-a | **`TASKBOARD_CONFIG` → missing file: write there, not XDG** (the item's open question) | The env var is the operator's "the config *is* this file" declaration — the read side honors it unconditionally, and every core command hard-fails `FileNotFound` while it stays missing (`AppConfig::load`, `config.rs:265-271`). If `login` wrote XDG instead, the install would *still* fail `FileNotFound` next boot and the login would be invisible. Write-there is also symmetric with `--config` (finding 4, L7) and preserves today's behavior for env-using installs. Pinned by new test L11. |
| D1-b | **CWD-default existence guard** | When `config_path` returns `None` (no explicit, no env, no XDG file) **and** `./taskboard.toml` already exists, `login` still overlays the CWD file — exactly as today. Without the guard the flip would create an XDG file that then *shadows* the active CWD config on every later resolution (XDG-if-present outranks the CWD default), silently abandoning a hand-tuned install. The backlog's own wording — "write XDG-first **when no config exists anywhere**" — requires the guard. Pinned at the pure layer (dispatch-level CWD control is not hermetic). |
| D1-c | **No platform config dir → CWD default** | `dirs::config_dir() == None` (no `XDG_CONFIG_HOME`, no `HOME`) falls back to `./taskboard.toml` — today's shape, degraded but honest; mirrors `default_db_path`'s `None`-tolerance (`config.rs:456-458`). Pure-layer pin. |
| D1-d | **Resolution shape** | New `pub fn AppConfig::write_path(explicit_path: Option<PathBuf>) -> PathBuf` beside `config_path`: `config_path`'s answer when `Some`, else the private pure core `first_run_write_target(config_dir, cwd_default)` implementing D1-b/D1-c. `login_command`'s `unwrap_or_else` line becomes `AppConfig::write_path(config_path)`. Injected-path core = the `xdg_candidate`/`xdg_if_present` precedent for unit testing without env or CWD control. |
| D1-e | **`logout` untouched** | It never creates a config (`strip_config_keys` is write-if-present, finding 3); its resolution already prefers an existing XDG file. No behavior delta exists to fix. |
| D1-f | **No ADR churn** | WI-3 shipped the XDG *read* step without an ADR amendment (backlog line 1028); D1 completes the symmetry. Semantics live in the doc comments on `write_path` / `DEFAULT_CONFIG_PATH`; one line added to `login_command`'s doc. |
| D2-a | **Surface the inner kinds verbatim, no prefix** | `Self::Config(err) => err.kind()` — the envelope reuses the exact strings `config print`'s `invalid_kind` payload already carries (one discriminant vocabulary across surfaces; no collision, finding 8). `Display` stays "configuration rejected" — UX text is untouched, only the `--json` discriminator changes. |
| D2-b | **`error_detail` stays `None` for `Config(_)`** | Detail additions (e.g. the `FileNotFound` path) are not asked for; `config print` remains the path-bearing diagnostic surface. Minimal-diff discipline. |
| D2-c | **O3 keeps one Config sample, delegating** | O3's expected-table arm becomes `CommandError::Config(c) => c.kind()`; the existing `InvalidServerUrl` sample then expects `invalid-server-url`. The full six-kind table is already pinned at the lowest layer (`config_error_kinds_are_stable_discriminants`, finding 5) — duplicating it through O3 would violate one-behavior-one-test. The dispatch/print envelope tests pin `decode` end-to-end. |
| D2-d | **Sanction recorded, not re-litigated** | Finding 7's backlog note + pre-0.1.0 status is the compatibility contract; the plan cites it in the D2 commit body so the intent travels with the change. |

### The D1 write-target truth table (the contract both commits preserve)

| # | `--config` | `TASKBOARD_CONFIG` | XDG file | `./taskboard.toml` | Write target | vs today |
|---|---|---|---|---|---|---|
| 1 | `P` (any state) | – | – | – | `P` (created if missing) | unchanged (L7) |
| 2 | – | `E` (any state) | – | – | `E` (created if missing) | unchanged — **D1-a** (L11) |
| 3 | – | – | exists | – | XDG candidate | unchanged (L12) |
| 4 | – | – | absent | exists | `./taskboard.toml` | unchanged — **D1-b** guard |
| 5 | – | – | absent | absent | **XDG candidate, dir created** | **THE FLIP** (L10) |
| 6 | – | – | absent | absent | no platform config dir → `./taskboard.toml` | unchanged fallback — **D1-c** |

Rows 1–3 are `config_path` returning `Some`; rows 4–6 are the `None`
arm. Row 5 is the only behavioral change in D1 — and after it, the next
read finds the new XDG file via `xdg_if_present`, so read and write
re-converge on the same file.

## 3. D1 — XDG write-target flip (commit 1)

### 3.1 Production code

`crates/taskboard-app/src/config.rs`, after `config_path`:

```rust
/// The `login` write target: [`AppConfig::config_path`]'s answer when
/// resolution finds one (explicit `--config` / `TASKBOARD_CONFIG` —
/// present or not — / an existing XDG file); else — no config anywhere —
/// the XDG candidate with its directory created by the writer, unless a
/// CWD-default file already exists (an existing install keeps its
/// file). No platform config dir → the CWD default.
#[must_use]
pub fn write_path(explicit_path: Option<PathBuf>) -> PathBuf {
    match Self::config_path(explicit_path) {
        Some(path) => path,
        None => Self::first_run_write_target(
            dirs::config_dir().as_deref(),
            Path::new(DEFAULT_CONFIG_PATH),
        ),
    }
}

/// Rows 4–6 of the write-target table: pure over injected dirs, so the
/// decision is unit-testable without env or process-CWD control.
fn first_run_write_target(config_dir: Option<&Path>, cwd_default: &Path) -> PathBuf {
    if cwd_default.is_file() {
        return cwd_default.to_path_buf();
    }
    xdg_candidate(config_dir).unwrap_or_else(|| cwd_default.to_path_buf())
}
```

- `login_command` (`commands/mod.rs:659-660`): the
  `config_path(...).unwrap_or_else(...)` expression becomes
  `AppConfig::write_path(config_path)`; its doc comment gains "first run
  with no config anywhere persists to the XDG config dir".
- `DEFAULT_CONFIG_PATH`'s doc (`config.rs:25-27`) gains: also the write
  fallback when an existing CWD file is the active config or the
  platform has no config dir.
- Nothing else: the writer already creates parents (finding 2), and
  `extract_raw` at an absent path already yields defaults.

### 3.2 Tests (written first, red, then implemented)

**Unit — in `config.rs`'s `#[cfg(test)]` (in-module, private-core access,
tempdir-injected, mirroring the `xdg_*` test style):**

- `first_run_write_target_keeps_an_existing_cwd_default` — row 4:
  `cwd_default` = a tempdir file that exists, `config_dir` = another
  tempdir → returns `cwd_default` even though an XDG base was available.
- `first_run_write_target_writes_the_xdg_candidate_when_nothing_exists`
  — row 5: no `cwd_default` file, `config_dir` = tempdir → returns
  `<config_dir>/taskboard/taskboard.toml`; asserts the target's parent
  does not need to pre-exist (the writer creates it).
- `first_run_write_target_falls_back_to_cwd_without_a_config_dir` —
  row 6: no `cwd_default` file, `config_dir = None` → returns
  `cwd_default`.

**Dispatch-level — new L-series in `tests/cli_login.rs`** (through
`dispatch(Command::Login …)` with `config_path: None`, wiremock-hermetic
like L1–L9; each holds `env_lock()` for the whole call because
`login_command` reads env mid-dispatch, sets `XDG_CONFIG_HOME` to an
*absolute* tempdir — `dirs` ignores relative values — and removes
`TASKBOARD_CONFIG`):

- **L10** `login_with_no_config_anywhere_writes_the_xdg_candidate` —
  row 5, THE FLIP: fresh `XDG_CONFIG_HOME` tempdir, no CWD config →
  exit 0; file exists at `<xdg>/taskboard/taskboard.toml`, contains
  server + username, not the password; `AppConfig::load` of that path
  succeeds; canary: no `./taskboard.toml` appeared in the process CWD
  (a correct guard, not flakiness — a CWD file would violate the test's
  own fresh-machine premise; the repo carries none).
- **L11** `login_honors_a_missing_taskboard_config_target` — row 2,
  pins D1-a: `TASKBOARD_CONFIG` → `<tmpB>/env.toml` (absent) while
  `XDG_CONFIG_HOME` → fresh tempdir → login writes `env.toml` (created);
  the XDG candidate stays absent.
- **L12** `login_overlays_an_existing_xdg_config` — row 3: seed
  `<xdg>/taskboard/taskboard.toml` with a comment + a `[sync]` override
  (the L5 material) → login with `config_path: None` overlays *that*
  file (comment survives, override survives); nothing at `./taskboard.toml`.
  This is the first compositional pin of the XDG-if-present read arm
  (finding 4's coverage gap) through the verb that owns it.

Row 1 stays pinned by L7 (no change); rows 4/6 are pure-layer-only
(hermetic CWD/platform-dir control does not exist at dispatch level —
recorded, deliberate).

## 4. D2 — `CommandError::Config` kinds (commit 2)

### 4.1 The sweep (every touched site, complete per finding 6)

| Site | Change |
|---|---|
| `commands/mod.rs:225` | `Self::Config(err) => err.kind()` (was `"config"`) |
| `config.rs:237-241` (`kind()` doc) | Drop "The other commands' `CommandError::Config` kind stays the flat `"config"` — this accessor is `config print`-scoped"; now the single vocabulary for both surfaces |
| `commands/config.rs:16-18` (`print` doc) | "the flat `config` kind for provider failures (decode)" → "the `decode` kind for provider failures" |
| `tests/cli_output.rs:191` | O3 expected-arm: `CommandError::Config(c) => c.kind()`; the `InvalidServerUrl` sample now expects `invalid-server-url` (D2-c) |
| `tests/dispatch_routing.rs:497` | envelope kind `"config"` → `"decode"` (garbage file = `ConfigError::Decode`) |
| `tests/config.rs:324` | `err.error_kind()` `"config"` → `"decode"` |

No other consumers exist (finding 6); `error_detail`, `Display`, exit
codes, and `config print`'s `invalid_kind` payload are untouched.

### 4.2 TDD ordering

Tighten the three test sites first (red against `main`'s flat arm),
then flip the one match arm and the two doc comments (green). The
six-discriminant table itself needs no new test — already pinned at the
lowest layer (finding 5); the envelope tests prove routing.

## 5. Commit & bookkeeping plan (stage 2)

Three commits, each standalone-green (rebase-merge hygiene; gates run
per commit):

1. `feat(app): login writes the first-run config to the XDG config dir`
   — §3 code + all six D1 tests; body documents the truth table and the
   D1-a/D1-b decisions.
2. `feat(app): surface ConfigError kinds in the --json error envelope`
   — §4 arm + docs + the three tightened test sites; body cites the
   versioned-schema sanction (finding 7) as the compatibility note.
3. `docs(backlog): mark XDG write flip and config error kinds done`
   — "[2026-10-11] XDG write-target flip for `login`" (line 1375) and
   "[2026-10-11] `CommandError::Config` kind refinement" (line 1420)
   each get ✅ DONE + a one-line note naming the mechanism
   (`write_path` truth table; `error_kind` delegation).

Estimated diff: D1 ~+170/−4 across 3 files; D2 ~+12/−10 across 6 files;
backlog +~6. Item size S/M as triaged — the M is test mass, not risk.

## 6. Validation (stage 2, per AGENTS §10)

`cargo fmt --all -- --check` → `cargo clippy --workspace --all-targets
-- -D warnings` → `cargo nextest run --workspace` →
`cargo test --doc --workspace` → `cargo deny check` →
`cargo llvm-cov --workspace`. All D1/D2 tests hermetic (tempdirs + env
under lock + wiremock; no network, no keyring — `FakeCredentialStore` /
env store as L1–L9 do). No MSRV exposure: no new deps, no post-1.96 API
(`write_path` is `Option`/`Path` arithmetic; `EnvOverride`/
`XDG_CONFIG_HOME` handling is test-side).

## 7. Risks

- **Env-race window in the dispatch tests**: `login_command` reads
  `TASKBOARD_CONFIG` and `dirs::config_dir()` mid-call; the `env_lock`
  guard must be held across the whole `dispatch(...).await` (precedent:
  the C-series env tests). Forgetting it = flaky cross-test bleed, not a
  product bug — review focus for stage 3.
- **Relative `XDG_CONFIG_HOME` is ignored by `dirs`** (XDG spec): a
  relative tempdir path would silently fall back to `$HOME/.config` and
  could touch the developer's real config dir. Tests must use absolute
  tempdir paths (they always are) — called out in §3.2 so nobody
  "simplifies" it later.
- **Platform spread untested**: rows 3–6's `dirs` shapes differ on
  Windows (RoamingAppData) / macOS; CI is Linux-only and the code is
  platform-agnostic through `dirs` — behavior there is sensible but
  unpinned. Recorded, not blocking (AGENTS §1: Linux is the dev
  platform; first-class binaries ship per-platform without per-platform
  test farms today).
- **D2 breaks any external `kind == "config"` matcher** — sanctioned
  pre-0.1.0 (finding 7); zero in-tree consumers (finding 6). The
  flat string vanishes entirely rather than lingering as an alias —
  cleaner than a deprecation dance for an unversioned surface.
- **WI-G overlap** (dispatch §2): both touch `taskboard-app` but
  disjoint files (REPL completion vs config/commands/login tests);
  trivial rebase at worst.
- **Backlog-file contention** with parallel wave-1 items (every final
  commit edits it) — land serially or accept the trivial rebase.

## 8. Out of scope (backlog candidates, not this item)

- `error_detail` enrichment for `Config` (e.g. carrying the
  `FileNotFound` path) — D2-b.
- A `taskboard config path` diagnostic verb printing read + write
  targets (would have made D1 observable without login; nice DX, new
  surface — backlog material).
- Versioning the `--json` payload/error shapes (the "[2026-10-10]
  Versioned `--json` schema" entry owns it; trigger = first external
  consumer).
- Doc/ADR work beyond the two doc comments — D1-f.

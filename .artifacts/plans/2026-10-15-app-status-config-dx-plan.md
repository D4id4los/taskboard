# [Plan] App Honesty Micro-Batch: In-Session `status` Flag + `config path` Verb (P4)

- **Date**: 2026-10-15
- **Kind**: stage-1 implementation plan (chunk P4 of the
  parallel-during-GUI dispatch), TDD-first. No production code in this
  commit.
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §1 (execution model, §1.3 code-quality mandate, §1.4 GUI no-go zones)
  and §5 P4; backlog entries "[2026-10-14] In-session `status` should
  surface `connector_attached`" and "[2026-10-12] `taskboard config
  path` diagnostic verb"
  (`.artifacts/taskboard-potential-improvements-backlog.md`).
- **Target**: `taskboard-app` only — `src/commands/status.rs`,
  `src/commands/config.rs`, `src/commands/mod.rs` (the `dispatch_line`
  `Command::Status` arm only), `src/commands/output.rs`,
  `src/cli.rs` (`ConfigCmd::Path`), plus the test tiers
  (`tests/repl.rs`, `tests/config.rs`, `tests/dispatch_routing.rs`).
  **`src/config.rs` is read-only reuse** (dispatch §1.4: a missing seam
  there is a STOP-and-report, never a widening); no `bootstrap.rs`,
  `main.rs`, `ui-slint`, CI, or version diff.
- **Branch / worktree**: `feat/app-status-config-dx`,
  `../taskboard-WT-feat-app-status-config-dx`, off `main` @ `3a881f4`.
- **Goal**: (a) the in-session `status` line answers the live
  `connector_attached` fact (not the hardcoded `None` that predates
  ADR 0014's dynamic attach/detach); (b) a side-effect-free
  `taskboard config path` verb beside `config print` naming the
  resolved read path and the `connect`/`login` write target. Two
  standalone commits, one branch.
- **Status**: READY for stage 2 (every open question resolved in §3
  with a recorded decision).

---

## 1. Context & verified facts (anchored on `main` @ `3a881f4`; do not re-derive)

1. **`StatusView` has three constructors; only the session one lies.**
   `commands/status.rs`: `server(&App)` (daemon-side, :51-71) computes
   `connector_attached` gated on the backend —
   `Nextcloud => Some(app.connector_attached())`, `Standalone => None`
   (:56-59); `no_daemon(db_path)` (:77) is the one-shot client-side
   answer (lock-sidecar PID); `in_session()` (:92-102) hardcodes
   `connector_attached: None` — it predates dynamic attach/detach.
   The field doc (:37-44) currently says "present only in daemon-side
   answers under the `nextcloud` backend" and motivates `false` as the
   anti-masquerading fact ("structurally true, operationally
   misleading" without it).
2. **The flag's source is `App::connector_attached()`**
   (`bootstrap.rs:200-203`): `!self.connectors.lock()….is_empty()` —
   `false` under `standalone` (boot spawns no connector,
   `bootstrap.rs:535-562`), `true` after boot-time attach (a
   `nextcloud` boot constructs `DeckClient` from the injected store
   and spawns the actor, `bootstrap.rs:548`/`607-621`) or a `connect`
   ceremony, `false` after `detach_connector`.
3. **`dispatch_line`'s `Command::Status` arm** (`commands/mod.rs:934-940`)
   currently calls `StatusView::in_session()` with no facts. The arm
   receives both `session: &impl CoreSession` (whose
   `session_config()` is the config the session booted with — the
   semantic source for the backend) and `app: &App` (the flag source).
   The one-shot `dispatch` arm (:650-660) and the IPC server's
   `Request::Status` → `StatusView::server(app)` (`ipc/server.rs:374-376`)
   are untouched by this chunk.
4. **In-session `logout` semantics decide the `Some(false)` row.**
   `logout::run` de-configures the file iff the backend is *auto*
   (`logout.rs:57` — `let auto = raw.sync.backend.is_none()`); under
   an explicit `[sync] backend = "nextcloud"` the `server_url`/
   `username` keys survive (only the secret is deleted). The session
   ceremony `logout_line` runs ceremony → `app.detach_connector()` →
   `refresh_session_config` (`session_ceremonies.rs:53`, :143-148).
   Consequence: after an in-session `logout` under the **auto**
   backend the refreshed session config resolves `standalone`
   (post-logout `sync` answers `sync-disabled` — the backlog's own
   words); under an **explicit** `nextcloud` backend the session stays
   `nextcloud` with a detached connector — the one in-session state
   where `Some(false)` is the honest answer.
5. **The config seams exist read-only.** `AppConfig::config_path`
   (`config.rs:426-434`): explicit `--config` > `TASKBOARD_CONFIG` >
   the XDG candidate *iff it exists* (`xdg_if_present`,
   `config.rs:322-325`) > `None`. `AppConfig::write_path`
   (`config.rs:443-451`): `config_path`'s answer when resolution finds
   one, else `first_run_write_target` (`config.rs:329-334`): an
   existing CWD `./taskboard.toml` wins, else the XDG candidate
   (created by the writer), else the CWD default. **`write_path`
   re-resolves internally from the *explicit* option** — the two
   answers differ exactly in the no-resolution arm (and the CWD-file
   arm of the write fallback).
6. **`config print` establishes the pre-core `Local`-lane pattern.**
   `commands/config.rs`: `print(Option<PathBuf>) -> Result<ConfigView,
   CommandError>` decodes (no validation gate), `render_text` renders
   deterministic `key: value` lines, and `handle` (:106-116) is the
   dispatch glue. `route_for_ipc` matches `Command::Config { .. }` →
   `IpcRoute::Local` wholesale (`commands/mod.rs:1208-1214`) — **a new
   `ConfigCmd` variant needs no routing change**. The REPL session
   loop refuses the whole `config` family (`repl.rs:480`,
   `LineRefusal::ConfigCommand`) and the IPC server refuses it before
   dispatch — both stay as-is.
7. **Test harness facts.** `tests/repl.rs`: the J-series
   (`run_with`, :1013-1050) runs full session journeys over wiremock
   with a scripted prompt peer and an injected store —
   `run_session_with_parts` boots through `bootstrap::bootstrap` with
   that store (`repl.rs:652`), so a `nextcloud` config plus
   `FakeCredentialStore::with(server, user, secret)`
   (`tests/common/mod.rs:161-170`) boots **attached** without any
   ceremony; `mount_standard_pull(&server, json!([]))` is the
   established empty-board mount (J1, repl.rs:1081). The R12 walk
   boots a standalone core and routes `Command::Status` expecting
   exit 0 (repl.rs:818). `tests/config.rs` C-series holds the
   process-env mutex (`env_lock`) and chdirs for cwd-sensitive rows
   (C1); `EnvOverride`/`remove_env` are the env seams.
   `tests/dispatch_routing.rs` drives one-shot `dispatch()` in-process
   (the llvm-cov-visible slice) — `status_routes_client_side_without_
   booting` (:477-498) and `config_print_routes_an_undecodable_
   config_to_exit_one` (:459-471) are the neighbors to extend.

## 2. Design

### 2.1 (a) In-session status flag

`StatusView::in_session` gains the already-gated connector fact:

```rust
/// The in-session answer (REPL `status` line): the session *is* the
/// core holder, so its own pid is the honest client-side view, and —
/// under the `nextcloud` backend — the live attach/detach fact.
#[must_use]
pub fn in_session(connector_attached: Option<bool>) -> Self
```

The **backend gate is applied by the caller**, not re-derived inside
the view: a shared helper in `status.rs` (beside the field whose
presence rule it defines) holds the rule exactly once:

```rust
/// The connector-attached fact under the backend gate: present iff
/// `nextcloud` — a `standalone` core has no connector concept, and
/// `false` there would read as "detached (logged out)".
pub(crate) fn connector_flag(backend: SyncBackend, attached: bool) -> Option<bool>
```

`server()` is refactored to use it for its own gate (pure refactor —
identical output; pinned by the existing IPC journeys), and the
`dispatch_line` arm threads the session's facts:

```rust
Command::Status => {
    let backend = session.session_config().sync_backend();
    let flag = status::connector_flag(backend, app.connector_attached());
    let view = status::StatusView::in_session(flag);
    status::emit(io, &view);
    0
}
```

The field doc becomes "present in daemon-side and in-session answers
under the `nextcloud` backend". `render_text` (status.rs:107-132)
hoists the `— connector detached (logged out)` suffix out of the
daemon-only branch so an in-session `Some(false)` renders it too —
JSON-only honesty would leave the human surface misleading, which is
the exact complaint of the backlog entry (text stays
never-asserted-in-journeys UX).

### 2.2 (b) `taskboard config path` verb

- **clap**: `ConfigCmd::Path` — a unit variant beside `Print`
  (`cli.rs:270-274`), doc comment: side-effect-free resolution
  diagnostic, no decode, no core boot.
- **View** (`commands/output.rs`, beside `ConfigView`):

```rust
/// The `config path` payload: pure path facts — which file reads
/// resolve, and where `connect`/`login` would write. No config
/// content, no decode, no side effects.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConfigPathView {
    /// The file effective-config reads resolve (the `config print`
    /// `config_path` rule); `None` — defaults only.
    pub config_path: Option<String>,
    /// The `connect`/`login` write target (the first-run arm's
    /// candidate when resolution finds no file).
    pub write_path: String,
}
```

  `PartialEq` (plain strings — no provider state) enables typed
  assertions without serde roundtrips.
- **Handler** (`commands/config.rs`):

```rust
/// The resolved-path diagnosis: which file reads resolve and where
/// `connect`/`login` writes. Pure path resolution — no decode, no
/// validation, no lock, no boot; always exits 0.
#[must_use]
pub fn path(explicit: Option<std::path::PathBuf>) -> ConfigPathView
```

  **Both calls take the original `explicit` option** — `write_path`
  re-resolves internally (§1.5); passing the already-resolved
  `Some(read)` would collapse the very divergence this verb exists to
  show. `render_path_text` renders two `key: value` lines
  (`config_path: <path|(none)>`, `write_path: <path>`) in `print`'s
  style. The `handle` arm emits and returns 0 — **infallible**: no
  error path exists, exit 0 even for garbage or absent configs (the
  verb never decodes the file).

## 3. Open questions → recorded decisions

- **D1 — constructor shape: flag parameter vs `&App` read.**
  **Decision: flag parameter (`Option<bool>`), the backlog's own
  proposal.** The presentation-layer rule decides it: the view stays
  dumb data, and `dispatch_line` — the layer that holds both
  `session` and `app` — reads the facts (the CLI layer
  parses/dispatches; views render). `server(&App)` keeps its seam
  because it needs many live facts (ArcSwap load, version, pid); the
  session answer needs exactly one. Two corroborating reasons: a flag
  parameter stays unit-testable without an async sqlite bootstrap
  (the existing zero-arg unit tests adapt by passing `None`, none
  weakened), and the gate's semantic source is the *session's* config
  (`session.session_config()`), which an `&App`-reading constructor
  would silently re-source from `app.config`.
- **D2 — gate the flag on the backend, or thread `app.connector_
  attached()` raw?** **Decision: gate, mirroring `server()`'s
  vocabulary.** The field means the same thing on every surface:
  present iff a connector *concept* exists (`nextcloud`), `false` =
  detached. Ungated, a never-connected standalone session would
  answer `false` — which the field doc (and `render_text`) define as
  "detached (logged out)": dishonest. The post-logout outcomes follow
  the semantics rather than fighting them: auto-backend logout
  refreshes the session to `standalone` → `None` (the session *is*
  standalone-local now — exactly the state whose `sync` answers
  `sync-disabled`); explicit-nextcloud logout keeps the keys →
  `Some(false)` — the honest "configured but detached" fact. Review
  focus satisfied: the flag value is read from `App`
  (`app.connector_attached()`), never re-derived from config; the
  config only gates the field's *presence*.
- **D3 — where the gate lives.** **Decision: `pub(crate)
  connector_flag` in `status.rs`, reused by `server()` and the
  dispatch arm.** The presence rule is view vocabulary (documented on
  the field); keeping it beside the field gives it one home instead
  of two inline matches that could drift. The `server()` change is a
  pure internal refactor — byte-identical output, pinned by the
  existing IPC journeys.
- **D4 — does the human text render the in-session fact?**
  **Decision: yes — hoist the existing `— connector detached (logged
  out)` suffix to both `render_text` branches.** Minimal rider, no
  wording redesign; the suffix already exists for the daemon branch
  and the module's own presentation smoke tests extend additively
  (journeys still never assert text — AGENTS §5).
- **D5 — clap shape.** **Decision: `ConfigCmd::Path`, a bare unit
  variant.** No flags: the verb answers for the invocation's own
  `--config` resolution (the diagnosis *is* "what would this
  invocation read/write"); a `--write`-style toggle would be
  speculative surface. `Local` lane for free (§1.6 — the routing
  table matches the whole `Command::Config`).
- **D6 — output view shape.** **Decision: two flat fields,
  `config_path: Option<String>` + `write_path: String`.** Names align
  with `ConfigView::config_path` (the same resolution rule — one
  vocabulary) and the `AppConfig::write_path` method. `write_path` is
  never `None` (the fallback chain always lands on a concrete
  target). JSON-shape additions are pre-v0.1.0-sanctioned (dispatch
  §5).
- **D7 — in-session/IPC availability of `config path`.**
  **Decision: not available — the family refusals stand.** The REPL
  refuses the `config` family wholesale (`LineRefusal::
  ConfigCommand`) and the IPC server refuses it before dispatch; a
  in-session `config path` line would be answerable, but carving one
  variant out of a family refusal is behavior churn beyond the
  chunk's S size and touches `repl.rs` outside this chunk's file
  list. Recorded here so stage 3 doesn't "fix" it silently.
- **D8 — failure modes.** **Decision: none — the verb is
  infallible.** No decode, no existence requirement on the explicit
  arm, no lock, no boot; exit 0 always. The one-shot `dispatch` arm
  has no `Result` to report. (Contrast pinned by test: the same
  garbage file that fails `config print` with `decode`/exit 1 still
  gets its *path* reported by `config path` with exit 0.)

## 4. TDD test plan (write first; every production change lands green-or-pinpointed)

### Commit (a) — `feat(app): in-session status surfaces connector_attached`

1. **Unit, `status.rs` tests (extend the module's own suite):**
   - `in_session(None)` keeps today's assertions (shape mirror +
     `holder_pid` = own pid) — adapted, not weakened;
   - `in_session(Some(true))` / `in_session(Some(false))` → the field
     carries the fact (`PartialEq` on the struct);
   - serde key-skipping: `to_value(in_session(None))` has no
     `connector_attached` key, `Some(false)` serializes the key with
     `false` (the `skip_serializing_if` contract for the new arm);
   - `connector_flag` rows: `Nextcloud × {true,false}` → `Some(…)`,
     `Standalone × {true,false}` → `None`;
   - `render_text` smoke: the in-session view with `Some(false)`
     contains the detached marker (the module's established
     contains-pattern, D4).
2. **Journey, `tests/repl.rs` — standalone gate row (new R-test,
   r12's boot pattern):** `bootstrap(standalone_config(&db),
   forbidden_store(), LogSink)` + `ReplSession::headless`, route
   `Command::Status` with `json: true`, parse the single line:
   `daemon == false`, **`value.get("connector_attached").is_none()`**
   (the standalone gate, D2).
3. **Journey, `tests/repl.rs` — attached/detached rows (new J-test,
   the `run_with` harness):** seeded config with explicit
   `[sync] backend = 'nextcloud'` + `server_url`/`username` (the
   `%DB%`/`minimal_toml` helpers + a backend-ful body),
   `FakeCredentialStore::with(&url, "alice", …)`, wiremock with
   `mount_standard_pull(json!([]))`; lines `status --json`,
   `logout`, `status --json`, `exit` (no prompt answers — logout
   never prompts). Assert receipts: `status₁
   ["connector_attached"] == true` (boot-attached, §1.4),
   the logout receipt, `status₂["connector_attached"] == false`
   (explicit backend keeps the keys → session stays `nextcloud`,
   connector detached — §1.4's trap made the *point* of the test).

### Commit (b) — `feat(app): config path diagnostic verb`

4. **Handler journey, `tests/config.rs` (C16, env-mutating rows under
   `env_lock`; `remove_env("TASKBOARD_CONFIG")` hygiene):**
   - explicit path (a *nonexistent* file) → `config_path ==
     write_path == that path` (pure resolution — no existence check
     on the explicit arm);
   - `TASKBOARD_CONFIG` env → both answers are the env value;
   - fresh machine: `XDG_CONFIG_HOME` → empty tempdir, no CWD file
     (chdir, C1 precedent) → `config_path == None`, `write_path ==
     <tmp>/taskboard/taskboard.toml` (the divergence the verb
     exists to show);
   - CWD install: `./taskboard.toml` exists, empty XDG →
     `config_path == None`, `write_path == "./taskboard.toml"`
     (the write fallback's row).
5. **Dispatch journey, `tests/dispatch_routing.rs` (new D-test):**
   the garbage-config fixture of
   `config_print_routes_an_undecodable_config_to_exit_one`, but
   `Command::Config { cmd: ConfigCmd::Path }` → **exit 0**, stdout
   JSON names the garbage file in both fields — the same fixture
   pinning the decode-vs-no-decode contrast between the two verbs.

Both commits: existing journeys extended additively only (r12's
`Command::Status` row keeps exit 0 — the standalone gate preserves
it); no test weakened; typed/structural assertions only (no output
text assertions in journeys); hermetic (wiremock loopback, tempdirs,
env-mutex); no sleeps (the J-harness deadline bounds wedges).

## 5. Implementation steps (stage 2)

1. Commit (a): tests of §4.1 → `connector_flag` + `in_session`
   parameter + field-doc update + `render_text` hoist + `server()`
   refactor (status.rs) → the `dispatch_line` arm (mod.rs) →
   journeys §4.2-4.3. Gates.
2. Commit (b): tests §4.4-4.5 → `ConfigCmd::Path` (cli.rs) →
   `ConfigPathView` (output.rs) → `path()` + text + `handle` arm
   (config.rs). Gates.
3. Bookkeeping commit: both backlog entries ✅ DONE with one-line
   notes (execution-model step; entry text verbatim from the
   backlog).
4. No push, no PR (stage 3 ships). Full gate suite per AGENTS §10
   after each commit; every commit standalone (rebase-merge hygiene).

## 6. Acceptance criteria (from dispatch §5)

- Journey-level tests pin both surfaces additively; existing
  status/config journeys extended, none weakened.
- Hermetic tests only; typed outcome assertions.
- `src/config.rs` untouched (read-only reuse of
  `config_path`/`write_path`); no GUI no-go file in the diff
  (`bootstrap.rs`, `main.rs`, `ui-slint`, `ci.yml`, `release.yml`,
  version).
- The flag is read from `App`, never re-derived from config.
- `config path` is side-effect-free: no lock, no boot, no decode, no
  write; exit 0 unconditionally.
- Both commits stand alone; gates green; both backlog entries ✅ DONE.

## 7. Risks & mitigations

- **The auto-backend logout trap** (§1.4): a stage-2 journey that
  logout-detaches under an *auto* backend will observe `None`, not
  `Some(false)` — by design (D2). The plan's J-test pins the
  explicit-nextcloud row; if stage 2 finds the observed value differs
  from this analysis, AGENTS §5's bug-vs-test rule applies before any
  weakening.
- **Adjacency in `commands/mod.rs`** with lane C siblings (P5 `ipc/`,
  P6 `repl.rs`/`completion.rs`): the dispatch-plan expectation is
  trivial rebases (dispatch §1.2); land serially if convenient.
- **`server()` refactor regressions**: pure gate-extraction,
  byte-identical output, pinned by existing IPC journeys — if any
  drifts, the refactor is wrong, not the journey.
- **Process-env hygiene in C16** (`TASKBOARD_CONFIG`, `XDG_CONFIG_HOME`,
  cwd): all rows under `env_lock` with explicit `remove_env` and C1's
  chdir-restore pattern; a leaked override would cross-contaminate
  the C-series (the suite's own discipline).
- **Pre-existing repo observation (out of scope, report-only)**: the
  backlog file carries a committed diff3 conflict marker at line 2054
  (`||||||| parent of a66b9a5 …`) plus an ignored ediff temp file in
  `.artifacts/` — a docs-corruption artifact on `main` this chunk
  must not touch beyond its own two entries (keep marks surgical;
  flag for a docs-housekeeping fix).

## 8. Out of scope (non-goals)

- No `src/config.rs` changes; no seam widening there (STOP-and-report
  per dispatch §1.4 if one proves missing).
- No in-session/IPC `config path` availability (D7), no `config`
  family refusal restructuring.
- No `StatusView` field additions beyond the existing
  `connector_attached` (no `backend` on the in-session view — the
  session answer stays minimal).
- No `render_text` wording redesign beyond the D4 suffix hoist.
- No new `CommandError` variants (the verb is infallible).
- No GUI-owned files, CI, version, or ADR changes; no backlog edits
  beyond the two P4 entries' DONE marks.

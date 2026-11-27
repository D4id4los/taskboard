# [Plan] Configurable Nextcloud HTTP client timeouts (#1540) — chunk plan & agent prompts

- **Date**: 2026-11-27
- **Kind**: chunk plan — stage-1 (planning) output for issue #1540
  ("backlog: make the Nextcloud HTTP client timeouts configurable
  (sync settings) instead of hard-coded consts"), with complete
  stage-2 (implementation) and stage-3 (review) prompts.
- **Origin**: issue #1540 (normative body), operator direction
  2026-11-27 ("the project should move away from hard-coding values
  such as timeouts"), and `.artifacts/plans/2026-11-27-sync-refresh-tail-plan.md`
  (§6 Ruling 1 — the consts were that chunk's recorded default, a
  setting deferred to this issue). Evidence gathered by reading the
  worktree at `origin/main` @ `8b0a8d36`.
- **Status**: READY — one code chunk. One ruling requested and taken
  on the recommended default (file/env-only, no Settings row; see §6)
  — if the operator overturns it, the chunk grows a Settings-fact row
  and a mandatory UX journey plan (§2 D-3, issue #1553).

## 1. Recorded reality (file:line on `origin/main` @ 8b0a8d36)

### 1.1 The two clients and their seam

| Client | Consts | Production ctor | Seam |
|---|---|---|---|
| `DeckClient` | `crates/taskboard-sync-nextcloud/src/client.rs:393,403` — private `CONNECT_TIMEOUT`/`READ_TIMEOUT` (10 s / 30 s idle-read) | `new` (client.rs:417) → `with_timeouts(Some, Some)` | `#[doc(hidden)] pub with_timeouts(…, Option<Duration>, Option<Duration>)` (client.rs:448) — `None` arms no timer, documented paused-clock-harness-only |
| `WebDavFilesTransport` | `crates/taskboard-sync-files-nextcloud/src/client.rs:171,181` — same values, own private copies | `new` (client.rs:191) | same `with_timeouts(Option, Option)` shape (client.rs:222) |

Seam consumers today (all tests): `tests/suite/timeout_wiremock.rs` in
both crates (sub-second injected bounds, typed-timeout oracle),
`taskboard-sync-nextcloud/tests/suite/actor_wiremock.rs:289` and
`tests/suite/e2e_hermetic.rs:148` (`(None, None)` paused-clock
harnesses; #1539 records why no finite value is safe there).

### 1.2 Production construction sites of `DeckClient` / `WebDavFilesTransport`

| Site | Kind | Config reachable? |
|---|---|---|
| `crates/taskboard-app/src/connectors.rs:126` (`NextcloudModule::build_and_spawn`) | **the long-lived Deck sync actor's client** — module minted at `bootstrap.rs:1131` from config specs | yes — bootstrap holds `&AppConfig` there |
| `crates/taskboard-app/src/bootstrap.rs:259` (`WebDavFilesTransport::new`) | **the long-lived files-sync transport** — same fn already reads `config.files_sync_config()` | yes |
| `crates/taskboard-app/src/ui_requests.rs:1220` (settings save → `attach_connector`) | builds a client that becomes a **running connector's client** | yes — the request flow holds the resolved `AppConfig` (`raw`) |
| `crates/taskboard-app/src/connections.rs:33` (`verify_credentials`), `connector_ceremonies.rs:236`, `ui_requests.rs:1308,1475,1654` (`live_remote_boards` et al.), `commands/boards.rs:44` (`client_for_spec`), `board_move_ceremony.rs:637` | **short-lived ceremony/verification clients** — one-shot `boards()` probes, CLI board commands, the one-shot board-move transport | mostly yes, but deliberately not threaded (see D-5) |

### 1.3 The config layer (the pattern to extend)

- `SyncSection` (`crates/taskboard-app/src/config.rs:449`): plain
  `u64` seconds fields, `#[serde(default = "default_*")]`, section
  `deny_unknown_fields`; defaults are pub consts beside the doc
  comment (`config.rs:34-39`).
- Semantic validation in `AppConfig::validate`
  (`config.rs:~1560-1580`): zero → `ConfigError::InvalidDuration {
  field }` (the variant at `config.rs:1045`, doc says "A `[sync]`
  duration is zero, or backoff_initial exceeds backoff_max").
- Env override is figment-free:
  `TASKBOARD_SYNC__POLL_INTERVAL_SECS` → `sync.poll_interval_secs`
  (module doc, config.rs:4-6); the truth-table test lives at
  `config.rs` tests (`env_overridden_keys_truth_table`). A prior
  env-only key precedent exists: `TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS`
  (`crates/taskboard-app/tests/core/config.rs:588-601`).
- `env_overridden_keys` (`config.rs:1392`) feeds the Settings rows'
  env-locked posture (`adapter/settings.rs:904+`) — relevant **only**
  for keys with a Settings row; this chunk's keys have none (D-3), so
  no env-locked work.

### 1.4 The settings registries (why the keys stay out)

- `SettingsSnapshot` (`crates/taskboard-domain/src/settings.rs:222`)
  is the **live-settings** read side — every field there is surfaced
  and/or editable in the Settings UI.
- The apply-semantics registry (`crates/taskboard-app/src/ui.rs:484`,
  the live-settings plan `.artifacts/plans/2026-11-06-app-live-settings-plan.md`
  D-5.6) lists editable keys; `OnRestart` entries carry written
  justifications (`restart_entry`, ui.rs:473). Read-only config keys
  surface as `SettingsFact` rows (`settings_facts`, ui.rs:547 —
  e.g. `app.shutdown_timeout_secs`), all `Apply::OnRestart`.

### 1.5 The UX-plan law (the trigger text)

`docs/ux_planning.org` §1, verbatim: *"a plan or design round that
adds or changes a user-visible capability — any new verb, badge, view,
or setting a human can perceive or act on — includes, or cites, an
accepted UX journey plan written from `.artifacts/templates/ux-journey-plan.md`
before a dispatch plan may chunk the work. Pure engine/storage/sync
internals with no surface delta are exempt."* Phase 6 (mandatory)
activated 2026-11-16 (§3).

## 2. Decisions

### D-1 — Keys, defaults, one source

- `sync.http_connect_timeout_secs: u64` (default 10) and
  `sync.http_read_timeout_secs: u64` (default 30) on `SyncSection`,
  serde-defaulted from new pub consts
  `DEFAULT_HTTP_CONNECT_TIMEOUT_SECS` / `DEFAULT_HTTP_READ_TIMEOUT_SECS`
  in `config.rs` (the `DEFAULT_POLL_INTERVAL_SECS` pattern).
- **Single definition**: the transport crates' private consts become
  `pub` (`taskboard_sync_nextcloud::CONNECT_TIMEOUT`,
  `taskboard_sync_files_nextcloud::CONNECT_TIMEOUT`, same for
  `READ_TIMEOUT`), and the config consts are *derived*:
  `pub const DEFAULT_HTTP_CONNECT_TIMEOUT_SECS: u64 =
  taskboard_sync_nextcloud::CONNECT_TIMEOUT.as_secs();` (const-eval —
  `Duration::as_secs` is `const fn`). Because the two transport crates
  cannot see each other, a compile-time equality assertion in
  `taskboard-app`'s config module pins the two crates' consts together:
  `const _: () = assert!(taskboard_sync_nextcloud::CONNECT_TIMEOUT.as_secs()
  == taskboard_sync_files_nextcloud::CONNECT_TIMEOUT.as_secs() &&
  …subsec_nanos()…);` — a unit-pasting (30 000 "ms") in one crate
  fails the build, not a user's evening. Changing a default means
  editing the consts, once each pair, and config follows.
- Env names for free via figment's `__` nesting:
  `TASKBOARD_SYNC__HTTP_CONNECT_TIMEOUT_SECS` /
  `TASKBOARD_SYNC__HTTP_READ_TIMEOUT_SECS`.

### D-2 — Validation: reuse `ConfigError::InvalidDuration`; no new `SettingsError` kind

In `AppConfig::validate`:
- `0` → `ConfigError::InvalidDuration { field: "sync.http_connect_timeout_secs" }`
  (resp. `…read…`) — the variant's doc grows the two new fields.
- `> 3600` → same variant. **Recorded reason for the upper bound**: a
  unit mistake (milliseconds pasted as seconds) must not silently
  disable transport failure detection — the whole point of #1485 was
  that a hung attempt wedges the sync actor inline (refresh-tail plan
  §1.2); 3600 s (1 h) is far beyond any legitimate dial or idle-read
  bound while still allowing genuinely pathological-but-intentional
  satellite-link installs.
- **No `SettingsError` kind**: `SettingsError` (domain) is the
  *live-settings UI edit* error type; these keys never enter that path
  (D-3). The typed rejection at the trust boundary is
  `ConfigError::InvalidDuration` — the existing range machinery, as
  the issue asks.

### D-3 — UX-plan decision: file/env-only, no Settings row, no UX journey plan (rule cited)

The trigger rule (§1.5 verbatim) binds on "any new verb, badge, view,
**or setting a human can perceive or act on**". This chunk adds two
`taskboard.toml` / env keys with **no** `SettingsSnapshot` field, no
`settings_facts` row, no apply-registry entry, no Slint surface: there
is nothing a human can perceive or act on in the app — the exemption
clause ("pure engine/storage/sync internals with no surface delta")
applies. A config-file key is read by editing a file, the same
perception channel as `TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS`
today, which carries no Settings row. Therefore **no UX journey plan
is written**; writing one would manufacture a surface this chunk
deliberately does not add. The future Settings-fact-row surface (at
which point the law flips on — a fact row *is* perceivable) is homed
in **issue #1553** with the UX-journey-plan-mandatory note.

If the operator overturns this ruling (§6 R-1), the chunk additionally
adds one read-only `SettingsFact` row per key (`Apply::OnRestart`,
value = resolved seconds) and the UX journey plan is written from the
template before dispatch — per the rule, no stage-2 work may start
until it is accepted.

### D-4 — How the connectors read the values

One source, one direction: `AppConfig.sync` → bootstrap/module mint →
production constructor.

- `SyncSection` gains an accessor on `AppConfig`:
  `pub fn http_timeouts(&self) -> (std::time::Duration,
  std::time::Duration)` returning `(connect, read)` from the validated
  fields (pure mapping; the `SyncActorConfig`-style "plain seconds
  converted at the mapping" rule).
- `NextcloudModule` (connectors.rs) gains `connect_timeout: Duration`
  and `read_timeout: Duration` fields, set at the `bootstrap.rs:1131`
  mint from `config.http_timeouts()`; `build_and_spawn` builds via the
  new production constructor (D-6).
- `bootstrap.rs:259` builds `WebDavFilesTransport` via the production
  constructor with `config.http_timeouts()`.
- `ui_requests.rs:1220` (settings save → attach) uses the production
  constructor with the flow's resolved config — this client becomes a
  running connector's client and must obey the same bounds.

### D-5 — Ruling: ceremony/verification clients keep the const defaults

`verify_credentials`, `live_remote_boards`, `client_for_spec`,
`connector_ceremonies`, and the one-shot board-move transport stay on
`new()` (consts). **Recorded reason**: these are interactive
one-shot probes; inheriting a user's "my server is very slow" patience
would let one misconfigured value hang the connect UI for minutes,
which is exactly the failure mode the defaults exist to prevent. A
connect-verify that needs longer than 10 s dial / 30 s idle is a
server worth not connecting to. This is a deliberate, documented
asymmetry, not an oversight; it is named in the plan and checked in
review so a future "thread it everywhere for consistency" pass is a
decision, not a drift.

### D-6 — Seam fate: split; production constructor takes `Duration`s; test seam renamed, not feature-gated

- Production: `pub fn with_timeouts(base_url, user, token,
  connect_timeout: Duration, read_timeout: Duration)` — plain
  `Duration`s, **no `Option` in the signature**: the type system
  makes "arm no timer" unrepresentable on the production path. `new`
  delegates with the consts.
- Test seam: the old `Option`-taking signature moves to
  `#[doc(hidden)] pub fn with_test_timeouts(…, Option<Duration>,
  Option<Duration>)` — same body, same doc (paused-clock law, #1539).
  Existing test call sites rename mechanically; the four
  `(None, None)` harnesses keep `(None, None)` unchanged (invariant,
  §4).
- **Why not `#[cfg(any(test, feature = "test-support"))]`**: the
  convention exists (`test-support = []` on `taskboard-sync-files-nextcloud`,
  `taskboard-domain`), but the crates' own wiremock suites
  (`tests/suite/timeout_wiremock.rs`) live *inside* the crates; a
  cfg-gated seam forces `required-features = ["test-support"]` on
  those `[[test]]` targets, and `cargo nextest run --workspace` (the
  fast gate, AGENTS §10) then **silently skips them** — tests dropping
  out of the primary gate is worse than a grep-able `#[doc(hidden)]`
  name. The production-never-calls-it invariant is enforced by (a) the
  type split (no `Option` on the production constructor), (b) the
  `_test_` infix, (c) stage-3's diff check (§8): any non-test call
  site of `with_test_timeouts` is a BLOCKER finding.
- Both crates' seam doc comments keep the `None`-arms-no-timer
  constraint prose verbatim (moved to `with_test_timeouts`).

## 3. Test catalogue (lowest layer, one behavior one test)

All in `crates/taskboard-app` unless noted; config tests extend the
existing truth tables in `config.rs`'s `#[cfg(test)]` module and/or
`tests/core/config.rs` (wherever the sibling keys' tests live — the
`poll_interval_secs` family is the template).

1. **Decode defaults**: toml without the keys → `http_timeouts() ==
   (Duration::from_secs(10), Duration::from_secs(30))`.
2. **Decode from file**: toml with both keys set → accessor returns
   the set values.
3. **Env override** (`TASKBOARD_SYNC__HTTP_CONNECT_TIMEOUT_SECS=7`,
   cleanup per the existing `remove_env`/`set_env` helpers,
   `tests/core/config.rs:588` pattern) → accessor returns 7 s; the
   `env_overridden_keys` set contains `sync.http_connect_timeout_secs`
   (structural fact; no Settings row consumes it yet).
4. **Zero rejected**: mutate `sync.http_connect_timeout_secs = 0` →
   `validate` errs `InvalidDuration { field:
   "sync.http_connect_timeout_secs" }` (`matches!` on the variant —
   no text assertions). Same for read, and for env-set `"0"`.
5. **Upper bound rejected**: `3601` → `InvalidDuration`; **boundary
   accepted**: `3600` → ok.
6. **Compile-time default equality** (§2 D-1): the `const _` assertion
   *is* the test — build fails on divergence; no runtime duplicate.
7. **Const values pinned** (transport crates, unit test beside the
   consts): `CONNECT_TIMEOUT == Duration::from_secs(10)`,
   `READ_TIMEOUT == Duration::from_secs(30)` — typed-value
   assertions, the issue's stated defaults.
8. **Seam split structural**: existing `timeout_wiremock.rs` suites
   (both crates) compile against `with_test_timeouts` and pass
   unchanged — they already own the "bounds apply to the client"
   behavior; **no new client-layer probe is written** (one behavior,
   one test: the config layer proves the *values*, the wiremock
   suites prove the *application* of a bound — the pair
   `(config.http_timeouts())` → constructor is plumbing covered by
   the type system: a `(Duration, Duration)` cannot silently become
   `(None, None)`).
9. **No-behaviour-change-at-defaults**: `DeckClient::new` /
   `WebDavFilesTransport::new` still delegate with the consts —
   pinned by test 7 plus the existing `new`-using suites passing
   green; no new test duplicates the wiremock oracle at 10 s/30 s
   (it would add minutes to the suite for zero discrimination).
10. **AGENTS §5 compliance**: no sleeps (the wiremock suites
    already ride real wiremock I/O with injected sub-second bounds);
    paused-clock harnesses untouched; no text assertions; no
    upper-bound-on-success assertions (the 3600 bound is a
    garbage-input ceiling — explicitly legal).

## 4. Invariants

- **Paused-clock harnesses keep `(None, None)`** — no finite value is
  safe under `start_paused` auto-advance (#1539); the rename must not
  touch any argument.
- **Conditional-PUT retry safety**: nothing in this chunk touches
  `BackoffPolicy`, `send_with_retries`, or the retry classification;
  a timeout remains a transport-class retryable failure, exactly as
  the refresh-tail plan ruled.
- **No behaviour change at the defaults**: with no config present,
  every client is built with 10 s / 30 s, byte-identical builders.
- **Production never passes `None`**: only `with_test_timeouts` can,
  and §8's review check greps its call sites.
- **Crate roles (AGENTS §2)**: config reading/validating lives in
  `taskboard-app`; the transport crates gain only `pub` consts and
  the constructor split — no config knowledge crosses the boundary
  (plain `Duration`s do).
- **Secret-free config** holds: the new keys are `deny_unknown_fields`-
  clean scalars under the already-declared `[sync]` table.

## 5. Diff shape (what stage 3 reviews against)

- `crates/taskboard-sync-nextcloud/src/client.rs`: consts `pub` +
  doc note "the config default's single definition (with
  taskboard-app's derived consts, §D-1 of the plan)"; `new` unchanged
  in behavior; `with_timeouts` re-signed to `Duration`s;
  `with_test_timeouts` added; tests/suites renamed call sites.
- `crates/taskboard-sync-files-nextcloud/src/client.rs`: same shape.
- `crates/taskboard-app/src/config.rs`: two consts (derived), two
  `SyncSection` fields + serde defaults + `Default` impl arms,
  `http_timeouts()` accessor, `validate` arms, tests.
- `crates/taskboard-app/src/connectors.rs`: two fields + ctor param,
  mint site passes them, `build_and_spawn` uses production ctor.
- `crates/taskboard-app/src/bootstrap.rs`: files-sync transport build
  + module mints pass `config.http_timeouts()`.
- `crates/taskboard-app/src/ui_requests.rs:1220` site: production
  ctor with the flow's config.
- No other files. No `SettingsSnapshot`/`settings.rs`/`ui.rs`/
  Slint/`messages.rs` changes — their absence **is** the diff-shape
  assertion (D-3).

## 6. Rulings requested

| # | Question | Options | Taken (recommended default) |
|---|---|---|---|
| R-1 | Settings row for the new keys? | (a) file/env-only, no row — no UX plan, surface homed in #1553; (b) read-only `SettingsFact` row now — UX journey plan mandatory pre-dispatch | **(a)** — the trigger rule's own exemption fits; a fact row can trail in a small later chunk once its UX plan is accepted. *Overturning (a) grows this chunk; say so before stage 2 dispatches.* |
| R-2 | Ceremony clients thread configured timeouts? | (a) keep const defaults (documented asymmetry); (b) thread everywhere | **(a)** — D-5's reason (interactive probes must not inherit unlimited patience) |
| R-3 | Test-seam exposure | (a) rename split, `#[doc(hidden)]`, no feature gate; (b) `test-support` feature + `required-features` | **(a)** — (b) silently drops the crates' wiremock suites from `cargo nextest run --workspace` (D-6) |
| R-4 | Upper validation bound | 3600 s / 1800 s / no bound | **3600 s** — unit-mistake guard without binding real installs (D-2) |

## 7. Verification commands (stage 2/3; every cargo via `scripts/memory-gate.sh`)

```sh
scripts/memory-gate.sh cargo nextest run -p taskboard-sync-nextcloud -p taskboard-sync-files-nextcloud
scripts/memory-gate.sh cargo nextest run -p taskboard-app   # config tests
scripts/run-quality-gates.sh cargo nextest run --workspace   # the fast gate (flock)
scripts/memory-gate.sh cargo clippy --workspace --all-targets -- -D warnings
scripts/memory-gate.sh cargo fmt --all -- --check
```

The full AGENTS §10 suite is the dispatcher's, between stage 2 and
stage 3 (discipline item 1).

## 8. Chunk

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 (only) | #1540 body + this plan §2 | `feat/configurable-http-timeouts` | config keys + validation + both connectors read configured values + seam split; non-goals: no Settings row, no ceremony threading, no `SettingsSnapshot` change, no behavior change at defaults | `no UX plan in origin` (D-3; §1.5 exemption) | M |

Estimated diff: ~350–450 lines, majority tests.

**Prompt 2 (implementation):**

```text
You are the STAGE 2 (IMPLEMENTATION) agent for the chunk "configurable Nextcloud HTTP client timeouts" in the taskboard repo (GitHub: D4id4los/taskboard), issue #1540 (p2). Work ONLY inside /home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-configurable-http-timeouts (branch feat/configurable-http-timeouts), by absolute path; recreate the worktree on that branch if missing (git worktree add -b feat/configurable-http-timeouts ../taskboard-WT-feat-configurable-http-timeouts origin/main — explicit fetched start point per #1185; if the branch or worktree path's PR already landed, use feat/configurable-http-timeouts-v2 per AGENTS §8). Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first; nonzero exit = STOP and report. The main checkout must not be modified. Do not touch any other taskboard-WT-* directory.

## Discipline (binding — docs/agent_dispatch.org; restated verbatim)
A subagent is killed after ~600s without observable activity. Long-command discipline: (1) The coverage trio never runs inside a subagent: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through `scripts/memory-gate.sh` — nothing bare. (5) Stage prompts restate this discipline verbatim.

1. Read AGENTS.md (§2 crate boundaries, §4 stack, §5 testing), the approved plan .artifacts/plans/2026-11-27-configurable-http-timeouts-plan.md (§2 decisions D-1..D-6 are normative, §3 test catalogue, §4 invariants, §5 diff shape), and docs/coding_guidelines.org §2.1.
2. Implement exactly per the plan, TDD: config decode/validation truth table first (§3 tests 1–5), then the transport-crates seam split (D-6: with_timeouts takes Durations, with_test_timeouts is the renamed Option seam — mechanically rename existing test call sites; (None, None) harnesses keep (None, None)), then the threading (D-4: connectors.rs, bootstrap.rs:259/:1131, ui_requests.rs:1220 via the production constructor with config.http_timeouts()). Ceremony clients stay on new() (D-5). Every commit standalone (rebase-merge hygiene): <type>(<module>): <subject> + body; SPDX headers; #![forbid(unsafe_code)] already at crate level.
3. Gates: pre-commit hooks (automatic) + scripts/run-quality-gates.sh cargo nextest run --workspace per the discipline above (background + poll). No push, no PR. Do not remove the worktree.
4. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, any deviation from the plan and why, issue numbers.
```

**Prompt 3 (review):**

```text
You are the STAGE 3 (REVIEW) agent for the chunk "configurable Nextcloud HTTP client timeouts" in the taskboard repo (GitHub: D4id4los/taskboard), issue #1540 (p2). Branch feat/configurable-http-timeouts, worktree /home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-configurable-http-timeouts; recreate it on the branch if missing (git worktree add -b feat/configurable-http-timeouts ../taskboard-WT-feat-configurable-http-timeouts origin/main; if that branch's PR already landed, feat/configurable-http-timeouts-v2 per AGENTS §8). Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first; nonzero = STOP and report.

## Discipline (binding — docs/agent_dispatch.org; restated verbatim)
A subagent is killed after ~600s without observable activity. Long-command discipline: (1) The coverage trio never runs inside a subagent: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through `scripts/memory-gate.sh` — nothing bare. (5) Stage prompts restate this discipline verbatim.

1. Begin by rebasing onto main: git fetch origin --prune; rebase the branch onto origin/main (AGENTS §8 rebase dance: drop already-merged duplicates; resolve only real conflicts); push --force-with-lease if history was rewritten.
2. Read AGENTS.md, the plan .artifacts/plans/2026-11-27-configurable-http-timeouts-plan.md (§2 D-1..D-6 normative, §4 invariants, §5 diff shape), then review the full branch diff (git diff origin/main...HEAD) against the plan's acceptance shape.
3. Functional focus (this chunk's specific traps): (a) the paused-clock harnesses in both crates' tests suites still pass (None, None) — any argument change there is a BLOCKER (#1539); (b) grep every call site of with_test_timeouts — any non-test production caller is a BLOCKER; (c) BackoffPolicy/send_with_retries/conditional-PUT logic untouched (diff-shape rule §5: no changes outside the listed files; SettingsSnapshot/settings.rs/ui.rs/messages.rs/Slint must not appear in the diff); (d) the compile-time const-equality assertion exists and names both crates; (e) no weakening of the existing wiremock timeout tests; (f) ceremony clients still on new() (D-5).
"Code-quality mandate" (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — config reading lives in taskboard-app; network IO in the transport actors; (c) a single level of abstraction per function body. Findings are fixed on the branch.
4. Re-run the gates that apply per the discipline above (fast gate via scripts/run-quality-gates.sh; clippy/fmt via scripts/memory-gate.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (body: closes #1540, summary, gate evidence), then STOP — do not watch checks, do not remove the worktree (the dispatcher owns watch, merge-state verification, teardown, and at-merge issue closures).
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, issue numbers. Never merge; never touch main; do not remove the worktree.
```

## 9. Completion bookkeeping

- Stage 2/3 close **#1540** via the PR body ("closes #1540"); the PR
  references **#1553** (the homed Settings surface, stays open) and
  **#1539** (paused-clock law, unchanged, stays open — the `(None,
  None)` invariant is restated, not resolved).
- **Deliberately not dispatched here** (homes): Settings fact row +
  UX journey plan → #1553; ceremony-client threading decision →
  recorded as D-5, revisit only on evidence.
- The dispatcher runs the full AGENTS §10 suite between stage 2's
  landing and stage 3's ship, owns CI watch, `mergeStateStatus CLEAN`,
  teardown, and closes #1540 at merge.

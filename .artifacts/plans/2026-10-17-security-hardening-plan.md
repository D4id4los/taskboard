# [Plan] Security Hardening — Sibling Artifact Modes + Transport Hygiene

- **Date**: 2026-10-17
- **Kind**: stage-1 plan for chunk 6 of the remediation-wave-1 dispatch
  (dispatch §1/§7, prompt 6.1). Planning only — no implementation in
  this commit.
- **Origin**: issues #297 (F12-01, P1 — owner-only modes for
  db/lock/config siblings), #260 (F12-02, P2 — reject/strip userinfo in
  `server_url`), #261 (F12-03, P2 — loud cleartext `http://` warning).
  All three normative; agenda rows P1-9 + N-1 (§agenda-outcomes:
  schedule-now, as one hardening chunk). Evidence chains:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-security-report.md`
  §5 (sibling-permission table), §7 (URL assembly chain), §8 (finding
  bodies F12-01/02/03); findings-report rows F12-01/F12-02/F12-03.
- **Branch / worktree**: `fix/security-hardening` in
  `../taskboard-WT-fix-security-hardening`, cut off `main` @ `d8a1585`.
  Every audit anchor below re-verified on this branch (the four crates
  touched are byte-stable vs the audit's evidence base on these seams).
- **Trust story enforced**: ADR 0012 "OS object permissions are the
  whole auth story (v1) … exactly as trusted as the
  `taskboard.toml`, db, and lock files beside it". Today the socket and
  REPL history hold up their end; this chunk makes the other three
  siblings hold theirs, and closes the two transport-hygiene gaps
  (pasted credentials in URLs; silent cleartext) beside them.

---

## §context — the seams as they stand on this branch

### The five siblings (audit §5 table, re-verified)

| Sibling | Creation site @ branch | Carries | Today | After |
|---|---|---|---|---|
| unix socket | `ipc/server.rs:241` `harden` | K4/K5 traffic | 0600 post-bind ✓ | unchanged (model) |
| REPL history | `repl_history.rs:128` `options.mode(0o600)` | K5 command lines | 0600 at create ✓ | unchanged (model) |
| sqlite db (+`-wal`/`-shm`) | `storage-sqlite/connect.rs:24` `create_if_missing(true)` — sqlx owns the create, no mode option | **K5: all board content** | umask-default (0644 typ.) | **0600, post-open tighten incl. siblings** |
| lock sidecar `<db>.lock` | `lockfile.rs:93` `OpenOptions` create, no mode | K6 holder PID | umask-default | **0600 at create + tighten on open** |
| config `taskboard.toml` (connect overlay **and** logout strip) | `commands/connect.rs:499` and `commands/logout.rs:216` — `File::create(tmp)` + `rename` | K4 server_url, username | umask-default; **every connect/logout rewrite resets a pre-hardened file's perms** | **tmp created 0600; rename carries it; each rewrite re-establishes 0600** |

Census note: the findings row names only `connect.rs:497-503` for the
config tmp, but `logout.rs:216` writes the *same artifact* through the
*same tmp+rename mechanism* (config-key strip on logout). Covering it
is the review gate's own requirement ("grep for create/open of db,
lock, config-tmp… none left umask-default"), not scope creep — it is
recorded here as a census addition. No other file-creation site exists
in production `src/` of app or storage (grep for `File::create` /
`OpenOptions`: the five rows above plus `create_dir_all` directory
creations, which are out of scope — the home directory is the
documented trust boundary, §non-goals).

### The URL validators (F12-02)

Two validators parse the same string, by design:

- `config.rs:717` `reqwest_host_ok` — the config-validate half. Splits
  the authority at `@` (`rsplit_once('@')`) and accepts whatever
  follows — **userinfo sails through**. Called from `validate()` at
  `config.rs:555/:569`; typed rejection arm `ConfigError::InvalidServerUrl`
  (`"invalid-server-url"`, config.rs:405/:440).
- `client.rs:328` `DeckClient::new` — the construction half. Scheme
  http/https (:331), host non-empty, **query/fragment stripped**
  (:338-339), trailing slash trimmed. **Userinfo not stripped** — it
  rides `base_url` into the client, whose manual `Debug` (client.rs:66-77)
  redacts `credentials` but prints `base_url` verbatim. Typed rejection
  arm `DeckError::InvalidBaseUrl`.
- The documented parity policy (config.rs doc comment on
  `reqwest_host_ok`): "the sync crate parses the same string again at
  client construction, so a drift here fails closed (bootstrap's
  `Client` variant)". Both validators therefore move together.
- Why rejection is safe: the audit verified (registry source, pinned
  reqwest 0.12.28) that reqwest performs **no URL-userinfo handling**
  (proxy auth only) — a userinfo URL is never applied as auth, so no
  working setup can depend on it; its credential only ever leaks.

### The four default-level log sites (F12-02's leak surface)

`bootstrap.rs:541` (info, resolved config), `commands/logout.rs:64`
(info), `session_ceremonies.rs:256` (warn), `ipc/server.rs:407` (warn)
— all format `server_url` verbatim. After F12-02, a userinfo-bearing
URL cannot enter config or construct a client, so these sites are
structurally closed (they still log the clean URL — K4, by design).

### The cleartext exposure (F12-03)

`client.rs:331` accepts `"http"` alongside `"https"`;
`client.rs:1395` sends the basic-auth header over whatever transport
was configured. No warning anywhere today. The connect receipt
(`output.rs:433` `ConnectReceipt`) is the typed surface where a
cleartext flag belongs; the ceremony builds it in `commands/connect.rs`
(human-mode formatter ~:440-463; JSON field-wise asserted by the
c-series, never full-shape — verified `tests/cli_connect.rs:97-98`).

### Existing tests that bound this chunk (must not break or weaken)

- `repl_history.rs:198-214` `the_history_file_is_owner_read_write_only`
  — the mode-test model (metadata mode bits, `#[cfg(unix)]`).
- `client.rs` in-file tests: `accepts_clean_base_url`,
  `normalizes_trailing_slash_and_strips_path_query_fragment`,
  `rejects_missing_scheme_host_and_bad_scheme` (uses the `err_of`
  helper), leak floor `debug_output_never_contains_the_credentials`
  (:1511).
- `config.rs` in-file: `validate_rejects_bad_urls_like_the_deck_client`
  (:761).
- `tests/cli_connect.rs` c-series: c13/c15 (typed failures, nothing
  persisted — the model for T13), `receipt` field-wise asserts;
  wiremock URIs are `http://127.0.0.1:…` (relevant to F12-03 test
  impact, §D3).
- `tests/lockfile.rs` — lock acquire/Held pins (T4/T5 join here).
- `tests/cli_ipc.rs:394+` — the nextcloud-backend daemon test (wiremock
  http): asserts views/receipts field-wise; the byte-identity pin
  (:201-206) compares **stdout only** (logs go to stderr — main.rs:53),
  so a bootstrap warn cannot break it (verified this branch).

---

## §decisions

### D1 — Pre-existing permissive files: **tighten on open** (no knob, warn-not-fatal on failure)

Every artifact this chunk touches is brought to 0600 whenever the app
opens/creates it, including files that already exist with looser
modes:

- db: the post-open `set_permissions` in `open()` tightens pre-existing
  0644 databases on every boot (D4);
- lock sidecar: tighten after a successful acquire (D6);
- config: the tmp+rename rewrite *recreates* the file on every
  connect/logout, so mode-at-create on the tmp (D5) re-establishes
  0600 on every write — pre-existing permissive configs self-heal at
  the next ceremony, and the "every connect resets a pre-hardened
  file's perms" defect dies with it.

Rationale: (a) ADR 0012's auth story *is* owner-only — "Kiosk is
single-user by design. Multi-user/socket-path knobs are backlogged
behind a real second-user story", so **no kiosk path depends on
group-read** (settled per the dispatch prompt: decide from ADR 0012;
default no); (b) warn-only would leave K5 board content world-readable
on upgrade — the exact defect being fixed; (c) tightening is idempotent
and matches the audited-good `harden` precedent. A chmod/set_permissions
failure **warns and is not fatal** (kiosk boot resilience; exactly the
`server.rs:241` `harden` stance, F12-06 precedent). No config knob for
any of this — a knob is a multi-user feature and stays backlogged per
ADR 0012.

### D2 — Userinfo: **reject, in both validators**, as the existing typed invalid-URL arms

`reqwest_host_ok` returns `false` when the authority contains `@`
(→ existing `ConfigError::InvalidServerUrl`); `DeckClient::new`
returns the existing `DeckError::InvalidBaseUrl` when the parsed URL
carries a username or password (`!url.username().is_empty() ||
url.password().is_some()`).

Reject, not strip: the user who pastes `https://user:app-password@host`
misunderstood the input (the ceremony prompts for the app password
separately; the URL credential was never applied as auth — §context),
so silently discarding it manufactures a broken-looking setup, while
rejecting names the mistake at the earliest typed surface. Rejection
also means the credential can never persist into config or reach any
of the four log sites even transiently — the structural closure the
finding asks for.

Both validators, not one: the parity policy is documented
(config-side drift fails closed at bootstrap), and one-sided rejection
would leave the other validator as a standing leak path.

Degenerate-edge drift, recorded: `https://@host` (empty userinfo) is
rejected by the config validator (the `@` is present in the authority)
but parses as no-userinfo in the url crate (`username()` empty,
`password()` None) and would pass the client check. This is garbage
input; the drift direction is fail-closed at config, consistent with
the documented policy. Not worth aligning char-level.

Valid forms must keep parsing (stage-3 review requirement): plain
http/https hosts, ports, IPv6 authorities (`https://[2001:db8::1]:8443`
— no `@` present), paths, the existing query/fragment strip. Pinned by
T10.

### D3 — Cleartext warning: **by-construction one-time** — a pure scheme predicate consulted at exactly two emission sites; no loopback exemption; no escape hatch

Mechanism:

- `config.rs` gains a pure sibling of `reqwest_host_ok`:
  `pub(crate) fn url_is_cleartext(raw: &str) -> bool` — true iff the
  scheme is exactly `http`. Unit-pinned (T14).
- **Emission site 1 — the connect ceremony**: `ConnectReceipt` gains
  `pub cleartext_transport: bool` (D7); the ceremony sets it from
  `url_is_cleartext(&server)`; the human-mode formatter appends one
  warning line when true; one `tracing::warn!` beside the existing
  post-connect log. Once per connect by construction.
- **Emission site 2 — bootstrap** (`bootstrap.rs` nextcloud arm, next
  to the existing resolved-config info at :541): one `tracing::warn!`
  when `url_is_cleartext(&server)`. Bootstrap runs once per process —
  every process that learns the URL from config (CLI one-shot, REPL,
  daemon) warns exactly once at boot. The sync actor/poll cycle and
  `DeckClient` never consult the predicate — there is no per-request
  or per-cycle site (structural one-time proof for stage 3: `grep
  url_is_cleartext` yields exactly the two emission sites + the
  receipt plumbing; no new warn in pipeline/actor/client).

No global once-cell/`AtomicBool`: one-time-ness is a property of the
call sites (one bootstrap per process, one ceremony per invocation),
which makes it testable structurally (T14/T15/T16 assert the typed
surfaces; log text is never asserted — AGENTS §5) and immune to the
shared-process pitfalls a global would bring (the wiremock suites
construct http clients constantly).

No loopback exemption: the issue's trigger is literal ("when the
configured `server_url` is `http://`"), loopback-exemption would add
host-classification policy for a non-existent threat (cleartext to
loopback never leaves the machine — but the flag is informational, not
a gate), and the test suites are unaffected either way: wiremock-based
tests assert receipts field-wise (the new field is additive), and the
byte-identity pin compares stdout only (verified, §context). Simplest
policy that satisfies the finding.

No config escape hatch: the warning is one stderr line per boot and a
receipt field; silencing it is not worth config surface, docs, and
tests. If LAN-http users object, that is a backlog issue, not this
chunk.

### D4 — db mechanics: post-open tighten of db + `-wal` + `-shm` inside storage `open()` (issue-prescribed; window recorded)

The issue prescribes post-open `set_permissions` "since sqlx owns the
create" — followed literally (no pre-create of the db file before
sqlx; that would change `open()`'s error surface for a marginal window
gain, and `-wal`/`-shm` would still be umask-default during migration
regardless — see below). Shape:

- `storage-sqlite/connect.rs`: new `#[cfg(unix)] fn
  harden_sibling_modes(db: &Path)` — `set_permissions(0o600)` on the
  db file and its `-wal`/`-shm` siblings (OsString-push suffixes);
  `NotFound` on a sibling is skipped silently (the audit itself notes
  wal/shm are created "at sqlite's discretion"; in practice
  `MIGRATOR.run`'s WAL writes guarantee all three exist by the time
  `open_with` returns); any other failure `tracing::warn!`s and is not
  fatal (D1 stance). `#[cfg(not(unix))]` no-op, exactly `harden`'s
  shape.
- Call site: `open()` after `open_with(...).await?`, before returning
  the repo — synchronous, so the ordering in code is trivially
  race-free against everything the caller does afterwards (the
  stage-3 "cannot race the first write" check: the first writes are
  the *migrations inside* `open_with`, and the tighten is on the same
  linear path, not a spawned task).
- Accepted window, recorded: between sqlx's `create_if_missing` create
  and the tighten, the db/wal/shm are umask-default for the duration
  of boot+migration (single-digit ms typical). This is the same
  accepted-risk class as the socket's documented pre-chmod window
  (ADR 0012:110-111, F12-06): kernel-scheduler-visible, owner-booted,
  and closable only via process-global umask mutation — the exact
  rustyline defect `repl_history.rs:11-16` exists to avoid. Not taken.
- `open_memory()` never touches the filesystem — unaffected by
  construction (the helper is only called from the file-backed
  `open()`).

### D5 — config-tmp mechanics: `mode(0o600)` at create via `OpenOptions`, the repl_history idiom, at BOTH overlay writers

`commands/connect.rs:499` and `commands/logout.rs:216` replace
`File::create(&tmp)` with `OpenOptions::new().write(true).create(true)
.truncate(true)` + `#[cfg(unix)] options.mode(0o600)` — byte-for-byte
the `repl_history.rs:123-129` idiom (module-inline `cfg` block, no
process-global umask). `rename` then carries 0600 onto the final
`taskboard.toml` on every ceremony. Windows: platform default applies
(same documented stance as repl_history).

*Superseded in form, recorded (stage 3, PR #321):* this decision
originally kept one inline block per module ("no cross-module helper
for a 4-line cfg idiom"). The SonarCloud quality gate on the PR
(required, new-code duplication ≤ 3%) measured the shared
create-0600 → write → sync → rename choreography at 4.1% duplication
(connect.rs ↔ logout.rs, ~20 lines), invalidating the inline-block
rationale: the duplicated region is the whole writer, not the cfg
idiom. The two writers now call one shared
`config::write_owner_only_atomic(path, contents)` (same mechanics,
same idiom, single home in the config-artifact owner). Mechanics and
mode semantics unchanged.

### D6 — lock sidecar: mode at create + tighten after acquire

`lockfile.rs:93`: `OpenOptions` gains `#[cfg(unix)] .mode(0o600)`
(covers fresh creation). Pre-existing sidecars (a crashed run's file,
or a pre-hardening 0644) are tightened with one `set_permissions`
**after a successful `try_write`** — only the lock's holder touches
the mode; the `Held`/error paths stay byte-identical in behavior.
Failure warns, not fatal (D1).

### D7 — receipt shape: plain `bool`, additive JSON

`ConnectReceipt.cleartext_transport: bool` — always serializable
(`"cleartext_transport": true|false`), unlike the `Option`+
`skip_serializing_if` fields beside it, because unlike `attached` /
`daemon_connector` it does not depend on *which front end ran* — every
receipt knows its URL. Additive field; no test asserts the receipt's
full JSON shape (verified field-wise only), and the in-file receipt
tests construct receipts and gain the field mechanically. Human mode
gains one appended warning line when true (text is UX; never asserted).

### D8 — Windows / non-unix

All mode work is `#[cfg(unix)]` with no-op or platform-default
fallbacks, mirroring `harden` (server.rs:254) and repl_history's
documented stance. No ACL work (non-goal). The mode tests are
`#[cfg(unix)]` (CI lanes are Linux; the Windows release builds compile
the no-ops).

### D9 — the leak-floor test under reject (deviation-in-form from the issue's phrasing, recorded)

The issue phrases the leak floor as "formatting a client built from a
`user:pw@host` URL" — written for the *strip* variant. Under reject
(D2) no client can be built from such a URL, so the floor takes the
reject form: T11 attempts construction, then formats **every** output
the failed construction can produce (`Display` + `Debug` of the
`DeckError`) and asserts the pasted secret substring is absent — the
same absence-of-secret floor form as the existing
`debug_output_never_contains_the_credentials` (which stays as the
format-level floor for constructible clients). Intent honored: no
credential-bearing output path exists. The dispatch acceptance's
"log-capture check at typed level" is satisfied structurally: with
rejection, the four log sites (§context) cannot see a userinfo URL at
all — no capture needed.

---

## §test catalogue (TDD: each lands failing against current code first)

All mode tests are `#[cfg(unix)]`, assert **metadata mode bits**
(`std::fs::metadata(p).permissions().mode() & 0o777 == 0o600`), never
access-as-a-different-user (which root would defeat), and follow the
`repl_history.rs:198-214` model. All error tests assert typed outcomes
(`matches!`), never message text.

### F12-01 — modes

| # | Test | Layer / file | Fixture → typed assertion |
|---|---|---|---|
| T1 | `a_fresh_database_file_is_owner_read_write_only` | storage `connect.rs` tests | `open(tmp .db)` (migrations run) → db metadata mode `& 0o777 == 0o600` |
| T2 | `wal_and_shm_siblings_are_owner_only_while_the_database_is_open` | storage `connect.rs` tests | same, repo alive (pool holds the connection): `<db>-wal` and `<db>-shm` exist and are 0600 |
| T3 | `opening_tightens_a_preexisting_world_readable_database` | storage `connect.rs` tests | pre-create a zero-length db file, `set_permissions(0o644)` (empty file = valid fresh db for sqlite) → `open` → 0600 — pins D1 |
| T4 | `the_lock_sidecar_is_owner_only` | app `tests/lockfile.rs` | `InstanceLock::acquire` on a fresh db path → sidecar 0600 |
| T5 | `acquire_tightens_a_preexisting_world_readable_sidecar` | app `tests/lockfile.rs` | pre-create sidecar (`pid=1\n`, 0o644) → acquire succeeds → sidecar 0600, guard still held |
| T6 | `the_written_config_overlay_is_owner_only` | app `commands/connect.rs` in-file tests | `write_config_overlay` to a fresh path → final config 0600 |
| T7 | `an_overlay_rewrite_tightens_a_preexisting_world_readable_config` | app `commands/connect.rs` in-file tests | pre-create config text at 0o644 → overlay → 0600 (tmp+rename re-establishes; pins D5 + D1) |
| T8 | `the_logout_strip_rewrite_keeps_the_config_owner_only` | app `commands/logout.rs` in-file tests (beside `detach_tests`) | pre-create a config with nextcloud keys at 0o644 → strip fn returns true → file 0600 |

Layer note: T6–T8 test the private writers through in-file `#[cfg(test)]`
modules (house pattern: `detach_tests`, connect's `mod tests`), with
`#[cfg(unix)]` on the mode assertions.

### F12-02 — userinfo rejection

| # | Test | Layer / file | Fixture → typed assertion |
|---|---|---|---|
| T9 | `rejects_userinfo_bearing_base_urls` | sync `client.rs` tests (`err_of`) | `https://user:app-password@cloud.example.com`, `http://user@cloud.example.com` → `Err(DeckError::InvalidBaseUrl)` |
| T10 | `accepts_urls_without_userinfo_across_http_https_ipv6_ports_paths` | sync `client.rs` tests | ok: `http://cloud.example.com`, `https://cloud.example.com:8443`, `https://[2001:db8::1]/nextcloud`, existing trailing-slash/query test unchanged — the common case must keep parsing |
| T11 | `userinfo_rejection_never_formats_the_credential` | sync `client.rs` tests (leak floor, D9) | construct from `https://alice:s3cret-pw@cloud.example.com` → `Err`; `format!("{err}")`, `format!("{err:?}")` (and the `Result`'s Debug) contain neither `alice:s3cret-pw` nor `s3cret-pw` |
| T12 | `validate_rejects_userinfo_urls_like_the_deck_client` | app `config.rs` tests | `server_url = "https://u:pw@example.com"` / `"http://u@example.com"` → `Err(ConfigError::InvalidServerUrl)` |
| T13 | `connect_with_a_userinfo_url_fails_typed_and_persists_nothing` | app `tests/cli_connect.rs` (c15 model) | `run_connect("https://alice:pw@127.0.0.1", …)` → `err.error_kind() == "deck"`; `!config_path.exists()`; fake store holds no secret |

### F12-03 — cleartext warning

| # | Test | Layer / file | Fixture → typed assertion |
|---|---|---|---|
| T14 | `url_is_cleartext_classifies_by_scheme_only` | app `config.rs` tests | true: `http://cloud.example.com`; false: `https://cloud.example.com`, `ftp://x`, `not a url` |
| T15 | `the_cleartext_flag_serializes_as_a_plain_bool` | app `commands/connect.rs` in-file tests (receipt-serialization pattern) | `ConnectReceipt` with the flag true/false → JSON `"cleartext_transport"` is `true`/`false` |
| T16 | `connect_over_http_flags_the_receipt_cleartext` | app `tests/cli_connect.rs` (wiremock `mount_boards_ok`) | `run_connect` against the wiremock URI (http) → `receipt.cleartext_transport == true` |

One-time semantics (D3) are asserted **structurally**: the predicate is
pure and pinned (T14), the receipt plumbing is pinned (T15/T16), and
one-time-ness holds by construction (two emission sites, each on a
once-per-process / once-per-ceremony path; no per-cycle site — stage 3
verifies by diff/grep, since AGENTS §5 forbids log-text assertions).
No test captures or matches log text.

---

## §creation sites touched (production diff census)

| File | Change |
|---|---|
| `crates/taskboard-storage-sqlite/src/connect.rs` | + `harden_sibling_modes` (cfg(unix) + no-op), called from `open()` |
| `crates/taskboard-app/src/lockfile.rs` | `.mode(0o600)` at create (cfg unix) + post-acquire tighten |
| `crates/taskboard-app/src/commands/connect.rs` | overlay write via shared `config::write_owner_only_atomic` (D5); receipt gains the flag + ceremony sets it + warn + human-mode line |
| `crates/taskboard-app/src/commands/logout.rs` | strip-overlay write via shared `config::write_owner_only_atomic` (D5) |
| `crates/taskboard-app/src/commands/output.rs` | `ConnectReceipt.cleartext_transport: bool` (+ doc comment) |
| `crates/taskboard-app/src/config.rs` | `reqwest_host_ok` rejects userinfo authority; + `url_is_cleartext`; + `write_owner_only_atomic` (D5) |
| `crates/taskboard-app/src/bootstrap.rs` | one warn in the nextcloud arm |
| `crates/taskboard-sync-nextcloud/src/client.rs` | `DeckClient::new` rejects userinfo URLs |

No other file changes. Tests join the existing modules/files named in
the catalogue.

## §commit plan (each standalone, rebase-merge hygiene)

1. `fix(storage): owner-only modes for the db and its wal/shm siblings`
   — T1–T3 (failing) then D4.
2. `fix(app): owner-only lock sidecar at create and on acquire` —
   T4–T5 then D6.
3. `fix(app): owner-only config overlay writes in connect and logout`
   — T6–T8 then D5.
4. `fix(sync-nextcloud): reject userinfo-bearing base urls` — T9–T11
   then the client half of D2.
5. `fix(app): reject userinfo server urls in config validation` —
   T12–T13 then the config half of D2.
6. `fix(app): warn loudly on cleartext http server urls` — T14–T16
   then D3 + D7.

Gates: full AGENTS §10 suite; targeted pre-push sanity
`cargo nextest run -p taskboard-storage-sqlite -p taskboard-app -p
taskboard-sync-nextcloud` plus the cli_connect/cli_ipc files
specifically (the warn's test impact lives there).

## §non-goals

- **No keyring/credential-store changes** (F12-02 is URL hygiene).
- **No TLS enforcement** — `http://` stays allowed, but loud
  (dispatch §7; the opt-in gate variant of F12-03 was not chosen).
- **No config knobs** — no warning silence switch, no mode relaxation
  (ADR 0012 backlogs multi-user knobs behind a second-user story).
- **No Windows ACL work**; no directory-mode changes (the home dir is
  the trust boundary); **no ADR edits** (ADR 0012's sibling-trust
  claim becomes enforced-true; chunk 12 owns docs).
- **No db pre-create** before sqlx (issue prescribes post-open; the
  boot window is recorded, D4); **no umask mutation anywhere**.
- **No loopback exemption** for the warning (D3).
- **No F12-04/05/06/07 work** (accepted risks / other owners; F12-07's
  WebDAV naming is chunk 15's).
- Out-of-scope observations become GitHub issues, never drive-bys.

## §risks

- **Mode assertions vs environment**: tmpfs honors chmod (CI runners
  are ext4/tmpfs — safe); running as root still records mode bits in
  metadata, and the tests assert bits, not access. A FAT/NFS dev
  machine would fail T1–T8 — CI-only contract, accepted (the suite has
  no skip-with-reason pattern to extend; per dispatch 6.2 we assert
  via returned/metadata mode, not the fs-access view).
- **wal/shm lifecycle**: T2 asserts the siblings exist while the pool
  connection is alive — guaranteed by WAL-mode migration writes today.
  If a future sqlite defers their creation, T2 fails loudly; that is a
  real behavior change to investigate, never a test to weaken.
- **Warn-on-stderr side effects**: any process booting an http
  connector config now warns once — `tests/cli_ipc.rs:394+`
  (nextcloud daemon over wiremock http) asserts views field-wise and
  byte-identity on **stdout only** (verified); `it_docker.rs` is
  env-gated and field-wise. No test asserts clean stderr on an http
  boot (grepped); if stage 2 finds one, the test's expectation is the
  finding to bring back, not the warning to mute.
- **Receipt JSON additivity**: `"cleartext_transport": false` now
  appears on https connects. One full-shape receipt assertion exists —
  `o1b` in `tests/cli_output.rs` compares the connect receipt's whole
  JSON object — and gains the field in both its constructed receipt
  and its expected object; the c-series and in-file receipt tests are
  field-wise and gain the field mechanically.
- **Userinfo rejection breaking real users**: none can exist with
  working sync — reqwest never applied URL userinfo as auth (audit §7,
  pinned-source verified); such configs were 401ing already. The
  rejection surfaces at the same typed surfaces as any bad URL.
- **Validator drift**: config (char-level `@` check) is marginally
  stricter than client (username/password check) on the degenerate
  empty-userinfo form — fail-closed direction, recorded in D2.

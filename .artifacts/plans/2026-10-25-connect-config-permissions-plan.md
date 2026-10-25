# Connect config-write permissions (#386) — chunk 1, backlog-triage dispatch

Stage-1 planning artefact (three-stage protocol, dispatch plan
`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §2).
Branch: `fix/config-write-preserves-mode`. Scope: `taskboard-app`
only. Normative input: issue #386; finding F7 in
`.artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md`.
No implementation here; stage 2 implements TDD-first (§3 tests red
before §4 code moves).

## 1. Problem

`taskboard connect` (and every other config rewrite: `logout`,
`connections` upsert/remove, `ui_requests`) persists through the single
shared helper `crate::config::write_owner_only_atomic`
(`config.rs:1185`), which creates the tmp sidecar with `.mode(0o600)`
(`config.rs:1198`) and renames it over the destination — so *every*
rewrite re-establishes 0600, deliberately ("self-heal", F12-01
#297). On the kiosk deployment
(`contrib/taskboard-kiosk.service:57`, runbook §6: config installed
`install -D -m 0644 /etc/taskboard/taskboard.toml`, unit runs as the
`taskboard` uid) a root-run `connect` rewrites the config 0600
`root:root`; the appliance user gets EACCES on its own config, and the
boot path reports it as `ConfigError::Decode` → `kind="decode"`
("configuration rejected") because `extract_raw` maps the whole figment
provider chain through `map_err(ConfigError::Decode)` (`config.rs:790`)
— a permission denial dressed as a content bug. `Restart=always` then
restart-loops forever (issue repro).

Second half of the bug: an unreadable config has no typed error of its
own; `ConfigError::kind()` (`config.rs:702`, `Decode` → `"decode"` at
`:705`) is the single vocabulary for `config print`'s `invalid_kind`
and the `--json` error envelope, so the mislabel propagates to both.

## 2. Decisions

### D1 — Mode semantics: **preserve the existing file's mode; 0600 only at creation**

Chosen over the two alternatives the issue offers (floor-0644 outside
the default XDG location; always-0644):

- **Least surprising and most faithful**: the file's mode is operator
  territory — `install -m 0644` in runbook §6 is a deployment decision
  the app should not second-guess. Preserve honors every operator
  choice, including hardening the floor rule would break (a group-read
  `0640 root:taskboard` install is plausible and legal; floor-0644
  would widen it).
- **No path-shape special-casing**: the floor rule needs a "is this the
  default XDG location?" predicate — new coupling from the write path
  to path resolution (`config_path`/`write_path`), more branches, and
  the XDG-vs-explicit distinction is about *who created the file
  first*, not about trust. Preserve is one rule, path-agnostic.
- **The secrets story is unaffected**: the config carries no secrets
  (app passwords live in the credential store, ADR 0012 — the file
  holds `server_url`/`username` and shape). F12-01's self-heal bought
  defense-in-depth on a file with nothing to exfiltrate, at the cost of
  the operational break F7 documents.
- **always-0644 is rejected** outright: it would *widen* an
  operator-tightened 0600 XDG file on every rewrite — the opposite
  direction of F12-01 with no compensating deployment need (the kiosk
  config is world-readable already by install).

Mechanics (in `write_owner_only_atomic`, the one shared choke point —
all seven call sites inherit: `connect.rs:563`, `logout.rs:209`,
`connections.rs:226/287/301/314`, `ui_requests.rs:233`):

1. Stat the destination *before* creating the sidecar. Metadata error
   `NotFound` (or any error) ⇒ "no mode to preserve" ⇒ create path
   stays 0600 (fail-safe toward the stricter mode).
2. Sidecar is still created `.mode(0o600)` (unchanged) — between
   create and finalize the tmp is never wider than the operator's own
   choice.
3. After `write_all`, before `sync`+`rename`:
   `set_permissions(&tmp, existing_mode & 0o777)`. The 0o777 mask
   strips setuid/setgid/sticky — a config never carries them.
4. Rename carries the preserved mode onto the final file (as today it
   carries 0600).

### D2 — Typed unreadable-config error: `ConfigError::UnreadableFile`

- New variant `UnreadableFile { path: PathBuf }` carrying the io error
  as `#[source]`; `kind()` returns `"unreadable-file"` (kebab, same
  vocabulary style as `file-not-found`). `config print`'s
  `invalid_kind` and the `--json` envelope pick it up automatically via
  the single `kind()` function.
- Detection seam: an explicit probe at the top of `extract_raw`
  (`config.rs:790`), before the figment read: when the resolved path
  `is_file()` and `File::open` fails with anything except `NotFound`
  (raced delete ⇒ absent-file path, as today), return
  `UnreadableFile`. `PermissionDenied` is the F7 case; EIO/ELOOP et al.
  fold into the same honest bucket ("exists but cannot be read") — one
  variant, not an errno taxonomy. `AppConfig::load` (`:734`) inherits
  the probe through `extract_raw`, so both the kiosk boot and the CLI
  dispatcher (`connect` reads via `extract_raw` before the ceremony)
  surface the typed error early.
- `CommandError::ConfigWrite` is **unchanged**: writer-side EACCES
  (e.g. a non-root `connect` against a root-owned config) already
  carries the io error as source and names the failing *operation*
  (write), which is correct there. The mislabel being fixed is the
  read/boot side.
- Residual TOCTOU: permissions may change between probe and figment's
  own read; that window is boot-once and accepted (documented on the
  probe).

### D3 — XDG-default contract: unchanged at creation, path-agnostic afterwards

App-created configs stay 0600 at creation — first-run `write_path()`
XDG target, `connect`-created files, every writer — because creation
has no prior mode to preserve. An existing XDG file a user hand-loosened
to 0644 stays 0644 after a rewrite, exactly like `/etc/...`: same rule
everywhere, no special case. **This deliberately reverses the
"self-heal" half of F12-01 (#297)** — recorded here as the decision,
not as drift; the create-half of F12-01 is untouched. The bug-pinning
tests are *replaced*, not weakened (dispatch plan §2 decision (d);
AGENTS §5's no-weakening rule does not apply to pins of the limitation
the normative issue requests removing).

### D4 — Rename the helper: `write_owner_only_atomic` → `write_config_atomic`

The old name lies once create-no-longer-implies-final-0600. `pub(crate)`
mechanical rename: 7 call sites + definition + the two one-word doc
comments that say "(temp + rename, owner-only)" (`connections.rs:168`,
`ui_requests.rs:196`). Doc comment on the helper rewritten to the D1
contract (preserve/create + sidecar-stays-0600-until-finalize detail).

### D5 — Test-layer placement: the mode contract moves to the helper

One behavior, one test, lowest layer: the preserve/create matrix lives
on `write_config_atomic` in `config.rs`. The command-level duplicates
retire (see §3); `connect.rs` keeps exactly one rewritten test that
names the issue's scenario through connect's own entry point
(`write_config_overlay` on a 0644 seed → still 0644 and still
decoding) as the dispatch acceptance's repro-equivalent.

### D6 — Docs leg: the runbook's expiry-keyed caution is deleted here

`docs/kiosk_bringup_runbook.org:259-263` carries "Operational caution
until #386 lands: … After any root-run connect: chmod 0644 …". The
caution becomes false the moment this chunk merges; deleting it is the
issue landing, not a runbook rewrite (dispatch plan's non-goal targets
rewriting §6 guidance; chunk 5's file list does not include the
runbook, so no wave-1 collision). The historical F7 section in the
2026-10-21 report is immutable and stays.

## 3. TDD-first test catalogue (red before green)

All unix-gated mode tests `#[cfg(unix)]` as today; no text assertions —
`matches!`/`kind()`/structural facts only.

In `config.rs` (helper matrix — the contract's home):

1. `a_rewrite_preserves_an_existing_configs_mode` — seed 0644 → write
   → 0644. (Replaces connect.rs:683-699 "must tighten".)
2. `a_rewrite_preserves_a_group_readable_mode` — seed 0640 → 0640
   (pins *preserve* specifically; would fail under any floor/widen
   rule).
3. `a_rewrite_preserves_owner_only` — seed 0600 → 0600 (a tightened
   install stays tightened).
4. `a_created_config_is_owner_only` — absent file → 0600 (F12-01's
   create pin, moved from connect.rs:668-678 to the owning layer;
   command-level copy retires per D5).
5. `the_preserved_mode_is_masked_to_permission_bits` — seed 0o2644
   (setgid+0644) → 0o644 (the 0o777 mask).
6. `an_unreadable_config_file_is_typed_not_decode` — seed file, chmod
   0000, `AppConfig::extract_raw(Some(&path))` ⇒
   `matches!(err, ConfigError::UnreadableFile { .. })`. Self-skip
   guard: if a probe `File::open` still succeeds after chmod 0000
   (euid 0 / ACL override) return early — deterministic on non-root
   CI, honest no-op under root.
7. The `kind()` vocabulary table test (exists at `config.rs:2069` /
   `:2246` style) gains the row `UnreadableFile ⇒ "unreadable-file"`.

In `connect.rs`:

8. `a_connect_overlay_on_a_world_readable_config_leaves_it_readable` —
   the issue scenario through connect's own writer (`write_config_overlay`,
   0644 seed → 0644 + `AppConfig::load` still decodes). Test 1 keeps
   this from being a helper-only pin; this keeps it from being a
   generic-helper-only claim.

In `logout.rs`:

9. `the_logout_strip_removes_the_matched_identity_keys` — rewrite of
   `the_logout_strip_rewrite_keeps_the_config_owner_only` (`:222-247`)
   keeping its strip-correctness assertions (`stripped == true`, keys
   gone on reload); the loosening scaffold and the 0600 mode assertion
   retire to the §3 helper matrix. (It is the only strip test — the
   strip-correctness half must survive the rewrite.)

`connections.rs` / `ui_requests.rs`: no tests today pin the mode;
nothing to rewrite — they inherit via the shared helper.

## 4. Implementation order (stage 2)

1. §3 tests 1-7 in `config.rs` (red: preserve tests fail against the
   tightening helper; test 6 fails with `Decode`).
2. `config.rs`: add `UnreadableFile` variant + `kind()` row + probe in
   `extract_raw`; rewrite helper per D1 + D4 rename; fix doc comments
   claiming self-heal (`config.rs:1180-1184`).
3. Rename ripple: `connections.rs` (4), `connect.rs` (1), `logout.rs`
   (1), `ui_requests.rs` (1) + two doc words.
4. §3 test 8 rewrite in `connect.rs` (retire the create duplicate),
   test 9 rewrite in `logout.rs`.
5. Delete runbook caution paragraph (D6).
6. Full gate suite (AGENTS §10), branch commit(s), PR.

## 5. Non-goals

- No config schema, figment, or provider-chain changes (the probe
  precedes figment; the chain itself untouched).
- No secret/credential-store changes; `credential.key` 0600 posture
  untouched.
- No ownership preservation (rename ⇒ writer's uid:gid): the kiosk
  case is unaffected (mode carries readability; the appliance user
  never writes /etc/taskboard). Known limitation; candidate follow-up
  issue, not this chunk.
- No writer-side error-surface change (`ConfigWrite` stays, D2).
- No `sessions`/`logout`-alias behavior changes beyond the shared
  writer.

## 6. Risks & limitations

- **Root-run CI** silently skips test 6 (documented guard) — a skip is
  visible, not a false green elsewhere; GitHub-hosted runners are
  non-root so the pin is active in the required checks.
- **F12-01 reviewers** may read the self-heal reversal as security
  regression: countered by "no secrets in the config" (ADR 0012) +
  sidecar-stays-0600-until-finalize + D3 record. Cite #386 as
  normative.
- **Parallel writers**: two processes rewriting concurrently still
  share the fixed `taskboard-tmp` sidecar name — pre-existing, out of
  scope.
- **Wave-1 collisions**: no other wave-1 chunk touches
  `taskboard-app` `.rs` files or the runbook; chunk 11 (wave 2) does —
  rebase-merge order applies as usual.

## 7. Acceptance (from dispatch plan §2, restated)

- `connect` on a 0644 config (any path) leaves it readable by the
  owning non-root user — the kiosk uid can boot its appliance.
- An unreadable config surfaces as `ConfigError::UnreadableFile` /
  `kind="unreadable-file"`, never `decode`.
- XDG-default contract pinned (create 0600; preserve otherwise).
- The restart-loop scenario's test-suite equivalents (§3.1/§3.8) pass
  against the new contract; the old tighten pins are gone.
- Gate suite green (fmt, clippy `-D warnings`, nextest, doc tests,
  deny, coverage sequence).

## 8. Estimated diff

≈ +195/−80 lines across 6 files: `config.rs` (variant, kind row, probe,
helper rewrite + rename, ~6 new tests), `connect.rs` (1 rewrite, 1
retire), `logout.rs` (1 rewrite), `connections.rs`/`ui_requests.rs`
(rename ripple + doc words), `docs/kiosk_bringup_runbook.org` (−5).
PR size S — matches the dispatch plan's row.

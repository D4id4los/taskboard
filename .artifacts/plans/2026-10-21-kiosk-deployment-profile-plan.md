# [Plan] Kiosk Chunk 9 — deployment profile + unit + runbook (TDD-first)

- **Date**: 2026-10-21
- **Kind**: stage-1 implementation plan for kiosk dispatch chunk 9
  (implementation plan §3 K9; branch `feat/kiosk-deployment-profile`).
  Stage 2 implements exactly this; deviations from ACCEPTED rulings
  (framing §4, ADR 0015/0016) go back to the user, never silently
  adapted.
- **Origin**:
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  §2 KDF-10, §3 K9, §4 gates, §5 sequencing, §6 bookkeeping;
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1 row 9 + §10 (chunk spec, acceptance, traps);
  `.artifacts/plans/2026-10-19-kiosk-wave-design-plan.md` §3
  post-acceptance note (the D250 i686 finding);
  #233 body + the 2026-10-19 bring-up annotation; ADR 0015's #236
  amendment (units keep explicit modes/verbs); ADR 0016 (the appliance
  the unit supervises).
- **Status**: READY — both MD-9 preconditions merged.

## 0. Preconditions verified (MD-9)

- Kiosk chunk 3 merged: PR #369 `feat/app-kiosk-lifecycle`
  (merged 2026-10-19T18:37Z) — `run_kiosk` lives at
  `crates/taskboard-app/src/main.rs:421`, ADR 0016 exists.
- Multi-board chunk 5 merged: PR #375 `feat/app-connector-registry`
  (merged 2026-10-20T12:46Z) — the `[[sync.connectors]]` shape,
  typed rejections, and the stable-`kind()` convention are in
  `config.rs` on `origin/main`. **No conflict-avoidance section is
  needed**; the branch is off plain `origin/main` (`84dce32`), no
  stacked parent.
- K5's harnesses exist for the runbook to invoke:
  `scripts/process_smoke.py` (`rss` / `cold-start` subcommands,
  `--taskboard`/`--fixture`/`--no-build` options).

## 1. What this chunk delivers

1. **The `[kiosk]` config section** (`crates/taskboard-app/src/config.rs`):
   `fixed_board: Option<uuid::Uuid>`, `deny_unknown_fields`, absent
   section = today's behaviour, plus the typed boot-time resolution
   against the live boards and its refusal error.
2. **`contrib/taskboard-kiosk.service`**: the system-level unit for
   the appliance (config-file mode, `Restart=always`, display-env
   import, `WantedBy=graphical.target`).
3. **`docs/kiosk_bringup_runbook.org`**: the bring-up runbook —
   image checklist, renderer-selection seam, the D250 i686
   cross-build lane, on-device harness invocation, the four
   budget-verdict recording slots.
4. **README pointer** (a "Deploying a kiosk" subsection under
   Launching) + **CHANGELOG** Unreleased lines.

**Non-goals** (dispatch §10): no rotation keys (K13's
`[kiosk]` growth), no `backend-fb`/`renderer-software` feature flip
(the spike rules), no `sd_notify` (#160 stays open, decided at the
spike), no release-workflow/release-matrix changes (the i686
release-target decision rides spike exit), no change to the existing
`contrib/taskboard.service` (the ADR 0015 #236 daemon-verb unit).

### Seams re-located at plan time (base `84dce32`)

- `crates/taskboard-app/src/config.rs` — the section pattern:
  `#[serde(default)]` fields on `AppConfig` (line ~63), every table
  `#[serde(deny_unknown_fields)]`, `ConfigError` variants + the
  stable `kind()` discriminants (lines 427–525), the chunk-5
  `Option<Vec<ConnectorCfg>>` + `skip_serializing_if` presence
  precedent (lines 182–189), the in-module `decode()`/`valid()` test
  helpers (lines 937, 1283).
- `AppMode` already carries `Kiosk` (kebab `"kiosk"`,
  config.rs:85–95); the routing table (README/ADR 0016) sends bare
  kiosk mode to the appliance TTY-independently.
- `crates/taskboard-app/src/main.rs` — `run_kiosk`
  (line 421): `boot_and_serve(...)` (the shared daemon boot, which
  **moves** `config` into the booted `App` — post-boot reads go
  through `app.config`, cf. `&app.config.ui` in the Appliance arm),
  then the `kiosk_start(display_env_available())` gate. The
  mount-failure arm shows the drain convention:
  `drain_and_shutdown(app, server)` then report. `report` is
  imported in `main.rs` (line 27).
- `CommandError::From<ConfigError>` (commands/mod.rs:331) wraps
  boxed — new `ConfigError` variants flow through untouched; the
  JSON envelope's kind comes from `ConfigError::kind()`.
- Domain live boards: `AppState.boards: BTreeMap<BoardId, Board>`
  (domain `state.rs:28`); `BoardId` is a `Uuid` newtype with
  `as_uuid()` (domain `ids.rs:13–40`). Reachable lock-free at boot
  via `app.engine.shared_state().load()` (the same ArcSwap the UI
  mounts).
- `taskboard-app` already depends on `uuid` (workspace, `serde`
  feature). `taskboard-ui-slint` is **not** touched by this chunk.
- No default board is seeded at boot — a fresh standalone DB has
  **zero** live boards (decides D6's test set).
- `contrib/taskboard.service` (user daemon unit) is the formatting/
  comment convention to mirror; it stays byte-identical.
- K8 landed `status`'s per-board counters incl. `last_cycle_ms` —
  the runbook's verdict slot 4 cites it.
- `ConfigView.config: AppConfig` serializes into
  `config print --json` (commands/output.rs:474) — decides D6's
  output-stability mechanics.

## 2. TDD order (red → green per artefact)

All behaviour tests live at the **lowest layer that owns them**
(§4 house rule): the resolution semantics are pure `config.rs` unit
tests through the real figment chain (`decode()` helper); the boot
refusal is one integration test (the wiring); no duplication across
layers.

**Artefact A — the section + resolution** (`config.rs`, then the
`main.rs` wiring; commits green after each artefact):

1. `kiosk_section_absent_decodes_to_none_and_validates` — a config
   with no `[kiosk]` table decodes with `fixed_board` `None` and
   `validate()` passes (the absent-section contract, chunk-5
   `keymap_section_decodes_with_defaults_and_sugar` shape).
2. `fixed_board_decodes_from_a_uuid_string` —
   `[kiosk]\nfixed_board = '<uuid>'` through `decode()` yields
   `Some` of that `Uuid`; serializing a pin-less config emits **no**
   `fixed_board` key (the `skip_serializing_if` contract, asserted
   via `serde_json::to_value` on the section or the whole config —
   structural, no text).
3. `malformed_fixed_board_is_a_decode_error` — a non-UUID string is
   `ConfigError::Decode` (the D13 structure-vs-semantics line, the
   `unknown_preset_is_a_typed_decode_error` shape).
4. `unknown_key_in_the_kiosk_table_is_a_decode_error` —
   `deny_unknown_fields` typo guard (the `unknown_key_in_a_table`
   shape).
5. `resolve_fixed_board_returns_none_without_a_pin` — pin `None`,
   live set empty **and** non-empty → `Ok(None)`.
6. `resolve_fixed_board_resolves_a_live_board` — pin `Some`, id in
   the live set → `Ok(Some(pin))`.
7. `resolve_fixed_board_rejects_an_unknown_board_naming_the_pin` —
   pin `Some`, absent from a non-empty live set, **and** the empty
   live set → `Err(UnknownFixedBoard { id })` with the payload equal
   to the pin (the "payload names the offender" convention).
8. Extend `the_new_kinds_extend_the_stable_discriminants` with
   `UnknownFixedBoard → "unknown-fixed-board"`.
9. Wiring (`main.rs::run_kiosk`, D5) — covered by artefact B's
   integration test plus the untouched K3 tests (below); no unit
   test duplicates the three-line glue.

**Artefact B — the boot refusal** (`crates/taskboard-app/tests/it_cli.rs`):

10. `kiosk_bare_with_a_dangling_pin_refuses_typed` — a bare
    `mode = "kiosk"` config plus
    `[kiosk]\nfixed_board = '<uuid-not-in-db>'` on a fresh standalone
    DB: the child spawns with `--json`, exits **nonzero**, and the
    JSON envelope carries `kind == "unknown-fixed-board"` (a typed
    discriminant, not message text — the c-series precedent). No
    `daemon stop` handshake (nothing serving).
11. **Unchanged-behaviour proof**: the existing
    `kiosk_bare_without_display_serves_headless_and_stops`
    (it_cli.rs:612) must stay green **untouched** — it is the
    absent-pin boot path (a fresh DB, no pin → `Ok(None)` → boot
    proceeds). `explicit_daemon_under_kiosk_mode_stays_headless`
    additionally pins D4's ignore-in-other-modes at the lifecycle
    level (the explicit daemon arm never resolves).

**Artefact C — unit, runbook, README, CHANGELOG** (docs; no tests —
review walks the unit against D7 and the runbook against the actual
`process_smoke.py` interface and the §8 finding texts).

## 3. Decisions (every open question settled)

### D1 — the section shape

```rust
/// `[kiosk]` table: the kiosk appliance's deployment profile.
/// Absent table = today's behaviour; the table grows rotation keys
/// only in the ambient chunk (K13).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KioskSection {
    /// The board the wall display pins to, resolved typed against
    /// the live boards at kiosk boot (see
    /// [`AppConfig::resolve_fixed_board`]); absent pin + N live
    /// boards is the ambient view's rotation scope, absent pin +
    /// 1 board is that board (KDF-10). The field is skipped when
    /// absent so pin-less configs serialize unchanged (the
    /// `sync.connectors` precedent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_board: Option<uuid::Uuid>,
}
```

`AppConfig` gains `#[serde(default)] pub kiosk: KioskSection` (the
`AppSection`/`UiSection` pattern — a defaulted struct, not
`Option<KioskSection>`: the only absence fact today is the inner
`Option`, and a present-but-empty `[kiosk]` table carries nothing).

On `config print`'s surfaces: the JSON payload gains an additive
`"kiosk": {}` key for every install (the section serializes; the
inner field is skipped when `None`); `render_text` (the human
`key: value` lines) is **not** extended — precedent: `[ui]`/`[ui.keymap]`
and `[[sync.connectors]]` landed without render lines. No test pins
the full JSON key set, so nothing else moves.

### D2 — the config speaks raw `uuid::Uuid`, not `BoardId`

`config.rs` deliberately imports no domain types (its only
cross-crate import is `SyncActorConfig`). The pin is
`Option<uuid::Uuid>`; the conversion to/from `BoardId`
(`as_uuid()`/`from_uuid`) happens at the resolution call site.
"Config validates, domain trusts" — the same split as
`is_kebab_name` vs the `ConnectorId` mint.

### D3 — resolution is a pure fn on `AppConfig`, called only in `run_kiosk`

```rust
/// Resolves the `[kiosk] fixed_board` pin against the live board
/// ids captured at boot. Pure — the caller owns capturing the
/// boards. A pin naming no live board (unknown UUID, or a deleted /
/// "dead" board) is the typed [`ConfigError::UnknownFixedBoard`]:
/// never a silent default, never a runtime warning (the deployment
/// contract is that the wall shows the pinned board or nothing
/// boots).
///
/// # Errors
///
/// [`ConfigError::UnknownFixedBoard`] when the pin names no live
/// board.
pub fn resolve_fixed_board(
    &self,
    live_board_ids: &[uuid::Uuid],
) -> Result<Option<uuid::Uuid>, ConfigError> {
    match self.kiosk.fixed_board {
        None => Ok(None),
        Some(pin) if live_board_ids.contains(&pin) => Ok(Some(pin)),
        Some(pin) => Err(ConfigError::UnknownFixedBoard { id: pin }),
    }
}
```

- **Not in `validate()`**: `validate` is board-blind by design — it
  runs pre-core (`config print`, `login`, every IPC client load) where
  live boards do not exist. Folding the check there would refuse
  `taskboard status` on the seat's own config. The pin's semantics
  are a *boot* check (dispatch §10: "typed resolution against live
  boards **at boot**"), so it is a separate, pure, unit-testable
  function with one production call site.
- **Call site** — `run_kiosk`, after `boot_and_serve` (the state is
  loaded; boards are in the ArcSwap), before the `kiosk_start` gate,
  so the refusal covers both the appliance and the loud-headless
  fallback (the deployment contract does not depend on display
  presence):

```rust
let live_boards: Vec<uuid::Uuid> =
    app.engine.shared_state().load().boards.keys().map(|id| id.as_uuid()).collect();
match app.config.resolve_fixed_board(&live_boards) {
    Ok(resolved) => {
        if let Some(id) = resolved {
            tracing::info!(board = %id, "kiosk fixed board resolved");
        }
    }
    Err(err) => {
        // The server is already spawned (boot_and_serve) — drain
        // through the shared tail (the mount-error arm's
        // convention), then report typed.
        let _ = drain_and_shutdown(app, server).await;
        std::process::exit(i32::from(report(json, &CommandError::from(err))));
    }
}
```

### D4 — the typed error set and the structure/semantics line

- One new variant (appended at the end of `ConfigError`, the
  chunk-5 convention):

```rust
/// `[kiosk] fixed_board` names no live board (unknown UUID, or a
/// board deleted before this boot); the payload names the pin.
#[error("kiosk fixed board unknown")]
UnknownFixedBoard {
    /// The pin that does not resolve.
    id: uuid::Uuid,
},
```

- `kind()` → `"unknown-fixed-board"` (stable discriminant, flows
  through `CommandError::Config` unchanged).
- A **malformed** pin (not a UUID) stays `ConfigError::Decode`
  (structure — D13's line, same as an unknown keymap preset).
- **Other modes ignore the pin**: `resolve_fixed_board` is called
  only under `mode = "kiosk"`. One config file legitimately serves
  the appliance *and* the operator's maintenance session
  (ADR 0016: "a kiosk machine's maintenance session gets the
  identical daemon"); the pin is the kiosk deployment's contract,
  not a global invariant. `explicit_daemon_under_kiosk_mode_
  stays_headless` already pins the mode's independence.

### D5 — refusal reporting and the resolved value's fate

- Refusal: drain via `drain_and_shutdown` (the server already
  serves — never drop the socket sidecar and actors silently), then
  the `report(json, &CommandError::from(err))` envelope + exit —
  byte-identical mechanics to the session-boot failure path in
  `main.rs:180-182`. The typed kind survives into `--json`.
- Success with a pin: exactly one `tracing::info!` naming the
  resolved board id (an operational fact for the ssh operator — an
  info trace, never a warning; no text assertions exist against it).
- **The resolved value is otherwise consumed by K13, not now**: no
  `UiSettings` field lands in this chunk (the UI crate is untouched).
  K13 will widen `UiSettings` and set it at the same seam where
  `ui_settings.kiosk = true` is set today, re-deriving via this same
  pure function (one definition site). This is a stated interim of
  the wave (the §5 interim list's pattern: K2's profile flag also
  existed before K3 consumed it).

### D6 — absent-section = unchanged behaviour, proven at both layers

- Unit: test 1 (`kiosk_section_absent_decodes_to_none_and_validates`)
  + the whole existing suite staying green (the section is purely
  additive to `AppConfig`).
- Behavioural: the untouched K3 integration tests
  (`kiosk_bare_without_display_serves_headless_and_stops`,
  `explicit_daemon_under_kiosk_mode_stays_headless`) are the
  boot-path proof.
- Output surfaces: no `render_text` lines; JSON stable per D1's
  `skip_serializing_if`.

### D7 — `contrib/taskboard-kiosk.service` (full text)

The unit **must not** set any `TASKBOARD_*` variable (the #236
trap): the mode's only authority is the config file reached by
`--config`; `EnvironmentFile` carries display plumbing only. The
`#236 unit rule` itself governs *maintenance* units (keep the
explicit `daemon` verb) — this is the appliance unit KDF-10 declares,
a different animal with its own rule (config-file mode, never an
env-var launch).

```ini
# systemd *system*-unit template for the taskboard kiosk appliance
# (ADR 0016; the kiosk deployment profile).
#
# The appliance's frontend comes from the config file's
# [app] mode = "kiosk" — never from an environment variable: the unit
# launches the binary bare with an explicit --config, so the config
# file is the single mode authority (no TASKBOARD_* variable may ever
# appear here; ADR 0015's #236 rule keeps unit launches explicit).
#
# One-time install (root, on the kiosk machine; the bring-up runbook
# walks the whole image):
#   useradd --system --home-dir /var/lib/taskboard \
#       --shell /usr/sbin/nologin taskboard
#   install -m 0755 taskboard /usr/local/bin/taskboard
#   install -D -m 0644 taskboard.toml /etc/taskboard/taskboard.toml
#   #   ... [app] mode = "kiosk"; pin [storage] db_path to
#   #   /var/lib/taskboard/taskboard.db (the StateDirectory below).
#   install -m 0644 taskboard-kiosk.service /etc/systemd/system/
#   # The seat's display session must be reachable from this system
#   # unit: write the display environment into
#   # /etc/default/taskboard-kiosk, e.g. for a wayland seat (adjust
#   # the socket name and the seat UID):
#   #   WAYLAND_DISPLAY=wayland-0
#   #   XDG_RUNTIME_DIR=/run/user/1000
#   # or for X11:  DISPLAY=:0
#   systemctl daemon-reload
#   systemctl enable --now taskboard-kiosk.service
#
# Operate (over ssh):
#   systemctl status taskboard-kiosk      # supervision view
#   taskboard status --json               # taskboard's own view (IPC)
#   taskboard daemon stop                 # graceful stop over IPC
#   journalctl -u taskboard-kiosk -f      # logs (tracing → stderr)
#
# Restart=always is the appliance's supervisor: ADR 0016 accepts that
# a GUI crash takes the core with it (one process), so recovery is
# re-execution, not in-process GUI supervision. The ExecStartPre gate
# mirrors the binary's own display_available predicate (set and
# non-empty): before the seat session is up at boot, the unit fails
# fast and restart-loops every 5 s until the display appears — the
# appliance never lingers headless on a wall. A display variable that
# outlives its server fails typed at mount (nonzero exit → restart),
# the ADR 0016 race tolerance.

[Unit]
Description=taskboard kiosk appliance
Documentation=https://github.com/D4id4los/taskboard
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=taskboard
StateDirectory=taskboard
EnvironmentFile=/etc/default/taskboard-kiosk
ExecStartPre=/bin/sh -c 'test -n "$$WAYLAND_DISPLAY" || test -n "$$DISPLAY"'
ExecStart=/usr/local/bin/taskboard --config /etc/taskboard/taskboard.toml
Restart=always
RestartSec=5
# SIGTERM triggers the graceful drain (flush → broadcast → bounded
# actor joins, ADR 0008 §5); the second signal force-exits (130).
TimeoutStopSec=30

[Install]
WantedBy=graphical.target
```

Rulings embedded here (each traceable):
- `ExecStart` is **bare** (no verb) + explicit `--config`: bare kiosk
  mode routes TTY-independently to the appliance (ADR 0016 table);
  the config file is the mode authority.
- `$$` in ExecStartPre defers expansion to `sh` (the systemd
  quoting idiom — systemd would otherwise expand the variables
  itself); the predicate mirrors `display_available` exactly
  (set **and non-empty** counts).
- `EnvironmentFile` **without** `-`: a missing file fails the start
  loudly instead of booting a headless lurker.
- `User=taskboard` + `StateDirectory=taskboard`: unprivileged; the
  runbook's config pins `db_path` into the state dir. No sandboxing
  directives in this wave (parity with the user unit; a hardening
  pass is a backlog idea, not tracked by the wave's §6 map).
- `WantedBy=graphical.target` per KDF-10; `After/Wants
  network-online.target` mirrors the user unit's convention
  (a want, not a require — standalone kiosks boot without a link).
- `Restart=always` + `RestartSec=5` + `TimeoutStopSec=30` per
  KDF-10/ADR 0016 and the user unit's SIGTERM-drain comment.

### D8 — the runbook: `docs/kiosk_bringup_runbook.org`

Underscore naming follows `docs/testing_strategy.org`/
`docs/flow_model.org` (docs/ = operator/engineering prose in org;
`contrib/` stays deployable artifacts only). Outline, with every
command exact:

1. **Scope and hardware** — the two units: NF310 (x86_64,
   first-class binary; 64-bit BIOS boot verified on-device) and
   D250-0Dk (Atom N270, **no x86_64** — the 2026-10-19 finding on
   #233; Debian 12 i386, kernel `6.1.0-39-686-pae`, driven over
   ssh). The budget envelope is unchanged
   (RSS ≤ 200 MB; repaint ≤ 50/20 ms; cold start ≤ 5 s; sync-cycle
   CPU share) and the software-renderer path is the only viable lane
   on the gen3 GPUs (no hardware OpenGL on modern Mesa → llvmpipe).
2. **The image** — Debian 12 minimal, **no desktop environment**;
   SSD/USB boot note (spinning disks vs SQLite's continuous
   writes); the seat session: a minimal X.org session **or** a kiosk
   wayland compositor (`cage`) — exact `apt install` lines for the
   chosen default plus `libinput`, Mesa DRI drivers (llvmpipe),
   `fonts-dejavu` (the UI's runtime font registration), `python3`
   for the harness.
3. **Renderer selection** — the seam, not a flip: today's binary
   ships winit + femtovg (software GL via Mesa); `SLINT_BACKEND` is
   documented as the selection env; `renderer-software`/`backend-fb`
   build features are **not compiled in** (ADR 0015 deferral) — the
   spike rules and the runbook records the verdict.
4. **Binary leg A — x86_64 (NF310)**: the first-class release
   artifact (or `cargo build --release -p taskboard-app`); transfer
   over ssh.
5. **Binary leg B — the D250 i686 cross-build lane**:
   - Target triple: **`i686-unknown-linux-gnu`** (Debian 12 i386
     userland = i686 baseline — the running `686-pae` kernel proves
     it; the N270's PAE + SSE2 cover the baseline; `i586` is *not*
     needed).
   - Host toolchain (x86_64 dev box):
     `rustup target add i686-unknown-linux-gnu` +
     `sudo apt install gcc-i686-linux-gnu`.
   - Cross env (documented in the shell, **never committed** — no
     `.cargo/config.toml` change in this chunk):
     `CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_LINKER=i686-linux-gnu-gcc`
     and `CC_i686_unknown_linux_gnu=i686-linux-gnu-gcc` — the C
     cross-compiler is required because `libsqlite3-sys` (bundled
     sqlite) and `ring` (rustls' crypto) compile C/asm for the
     target; reqwest is rustls-only, so there is no openssl in the
     tree.
   - Build: `cargo build --release --target i686-unknown-linux-gnu
     -p taskboard-app` → artifact
     `target/i686-unknown-linux-gnu/release/taskboard`.
   - glibc note: the artifact links the **host** glibc; the D250
     runs Debian 12 (glibc 2.36) — build on a matched-userland
     host/container, else the binary fails on-device with a
     `GLIBC_x.yy not found` loader error (the symptom is named so a
     mismatch is diagnosed in one step).
   - Transfer: `scp` the binary (and optionally a fixture DB, §7)
     to the unit over the bring-up ssh.
   - The **i686 release-target decision** (release-matrix leg vs
     documented self-build) rides spike exit — this lane is the
     documented self-build until then (framing §3 post-acceptance
     note).
6. **Config + unit install** — the `[app] mode = "kiosk"` file with
   `[storage] db_path` pinned into `/var/lib/taskboard/`; the
   optional `[kiosk] fixed_board` pin and its resolution contract
   (unknown/dead board → the boot refuses typed, kind
   `unknown-fixed-board`); standalone installs need no credential
   store; the unit install block from D7's header (provisioning
   `/etc/default/taskboard-kiosk` from the live seat:
   `systemctl show-environment`-free, copy the two display vars).
7. **On-device measurement — the four budget-verdict slots** —
   each slot is a fill-in row (device, binary leg, number, verdict,
   date) destined for the spike report
   (`.artifacts/reports/<date>-kiosk-wave-exit-report.md`) and the
   #233 annotation:
   | # | Verdict | Budget | Producer |
   |---|---|---|---|
   | 1 | RSS steady/peak | ≤ 200 MB | `python3
     scripts/process_smoke.py rss --taskboard ./taskboard
     --no-build` on-device; build the N=10 000 fixture once on the
     dev machine and transfer it (`--fixture <db>`) — a 1 GB
     spinning-disk box should not pay fixture construction |
   | 2 | Full-frame / partial repaint | ≤ 50 / ≤ 20 ms @ software
     renderer | the S2 criterion bench's dev-machine reference
     (kiosk K6) recorded alongside the on-device draw observation;
     the on-device draw verdict belongs to the spike (KDF-7's
     API-blocker rule) |
   | 3 | Cold start | ≤ 5 s | `python3
     scripts/process_smoke.py cold-start --taskboard ./taskboard
     --no-build` with the seat session live (spawn → the structured
     `first-frame` moment) |
   | 4 | Sync-cycle CPU share | recorded (no hard budget line) | the
     S4 criterion legs (dev machine) + the on-device cycle wall read
     from `status --json`'s per-board `last_cycle_ms` (kiosk K8)
     during a live sync |
8. **Post-bring-up decisions recorded at spike exit** — the i686
   release-target decision; #160's `sd_notify` ruling (a comment on
   the issue; it stays open until then); the renderer/backend flip
   verdict. The runbook records where each lands, it decides
   nothing.

The runbook invokes only interfaces that exist today
(`process_smoke.py`'s actual subcommands/options — re-verify against
the script at implementation time).

### D9 — README pointer (exact text)

A `** Deploying a kiosk` subsection directly after
`** Launching taskboard` (README.org:233 ff.), whose routing table's
kiosk row this section completes:

```org
** Deploying a kiosk

[[*Launching taskboard][Launching taskboard]]'s =kiosk= row is the
runtime contract; the deployment pieces are:

- =contrib/taskboard-kiosk.service= — a *system* unit that drives
  the appliance from the config file's =mode = "kiosk"=
  (=Restart=always= owns crash recovery, the seat's display
  variables are imported via =/etc/default/taskboard-kiosk=, the
  unit anchors at =graphical.target=); install steps live in the
  file's header comment.
- =docs/kiosk_bringup_runbook.org= — the bring-up runbook: the
  minimal image checklist, the software-renderer selection seam,
  the on-device harness invocation, the four budget verdicts, and
  the D250's i686 cross-build lane.
- =[kiosk] fixed_board= — the optional board pin, resolved typed
  against the live boards at boot; an unknown board refuses the
  boot.
```

### D10 — CHANGELOG (Unreleased → Added; exact lines)

- The `[kiosk]` config section with `fixed_board`: an optional
  board UUID resolved typed against the live boards at kiosk boot —
  a pin naming no live board refuses the boot with the new
  `unknown-fixed-board` config error; the section's absence leaves
  existing configs and behaviour unchanged (kiosk wave K9, the
  fixed-board pin the multi-board design deferred).
- `contrib/taskboard-kiosk.service`: a system-level unit for the
  kiosk appliance — the config file's `mode = "kiosk"` is the only
  mode authority (never an environment variable), `Restart=always`
  owns crash recovery (ADR 0016), the seat's display environment is
  imported, and the unit anchors at `graphical.target`.
- `docs/kiosk_bringup_runbook.org`: the kiosk bring-up runbook —
  minimal image checklist, software-renderer selection seam,
  on-device harness invocation, the four budget-verdict recording
  slots, and the D250's i686 cross-build lane (target, toolchain,
  build, transfer).

### D11 — out-of-scope fence (recorded, not tracked)

Rotation keys, `UiSettings.fixed_board` plumbing, `sd_notify`, the
i686 release-matrix leg, the `renderer-software`/`backend-fb` build
flip, unit sandboxing hardening, a `render_text` kiosk line — none
of these enter the diff. Per §6's map K9 only **annotates #233**
(one note: the deployment profile + i686 lane landed, the on-device
half awaits the spike; no `Closes` line) and creates no issues.

## 4. Gates, commits, size

- Stage-2 commits (each standalone — builds and tests green):
  1. `feat(app): the [kiosk] fixed_board section with typed boot resolution`
     (config.rs + main.rs + unit/integration tests) — artefacts A+B.
  2. `chore(contrib): the taskboard-kiosk system unit` — artefact C
     unit file.
  3. `docs(kiosk): the bring-up runbook and the README pointer`
     (runbook + README + CHANGELOG).
- Handoff gates: the full AGENTS §10 set (fmt/clippy -D warnings/
  nextest/doc tests/cargo deny/the three-command llvm-cov sequence +
  quarantine + trailing local `rm`).
- Estimated diff: `config.rs` ≈ +130 (incl. tests), `main.rs` ≈ +18,
  `it_cli.rs` ≈ +35, `contrib/taskboard-kiosk.service` ≈ +55 (new),
  `docs/kiosk_bringup_runbook.org` ≈ +230 (new), `README.org` ≈ +14,
  `CHANGELOG.md` ≈ +14 → **≈ 500 lines, 7 files (2 new)**.
- Risks:
  - *figment + uuid decode*: `Option<uuid::Uuid>` must decode from a
    TOML string through figment's value layer (serde's uuid visitor
    over `deserialize_str` — figment's supported path); expected to
    work, and test 2 is the canary. If it fails, fix at the decode
    layer (figment value handling), never by changing the field
    shape — KDF-10 rules `Option<Uuid>`; a shape change is a ruling
    deviation and goes back to the user.
  - *ExecStartPre portability*: `/bin/sh` + `$$` expansion is the
    documented systemd idiom; review confirms against
    `systemd.exec(5)` quoting rules.
  - *A board deleted while the appliance runs*: the pin is resolved
    at boot only (KDF-10); post-boot deletion is K13's rotation-
    scope problem, named as an interim in the runbook's config
    section.

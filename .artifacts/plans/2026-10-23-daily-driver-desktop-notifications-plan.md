# [Plan] Daily-Driver Chunk 8 — Reminder Toast Sink

Stage-1 plan for chunk 8 of the daily-driver implementation round
(dispatch §9): `notify-rust` enters the workspace, `DesktopSink`
implements the unmoved `NotificationSink` port, `[reminders] sink =
auto|log|desktop` decodes strictly, and `auto` resolves by `AppMode`
at boot. Closes #344.

Normative input, in MD-7 rank: implementation plan §2 DDF-13 (toast-sink
half), §3 chunk 8, §4 gates; design doc `2026-10-19-reminders-surfaces-design-plan.md`
Part C (§3.10 D-R10 stack ruling, §3.11 D-R11, §3.12 D-R12, §3.13
D-R13) + §5 test hooks + §4 decision record; dispatch plan §1/§9.
No ACCEPTED ruling is deviated from; every recorded decision below is
an engineering consequence of D-R10–D-R13 or a plan-day resolution the
designs left open.

Precondition (MD-9) verified 2026-10-23: chunk 7 (`feat/domain-adhoc-alarms`,
tip 66e9115) is contained in `origin/main`; the remote branch is
pruned post-merge. This branch is cut off that `main`. The stage-1
worktree was created after `scripts/worktree-doctor.sh` exited 0
(97G free, floor 25G).

---

## 1. Verified baseline (symbols re-located at this plan's start; lines are plan-day references)

All re-read on this branch @ 66e9115 (post-chunk-7).

App (`crates/taskboard-app`):

- `notifications.rs` — the port, byte-for-byte the D-R11 contract and
  **it must not move**: `NOTIFIER_CAPACITY = 64` :17; `NotifyError`
  (unit struct) :22–24; `trait NotificationSink` with
  `deliver(&self, &Notification) -> Pin<Box<dyn Future<Output =
  Result<(), NotifyError>> + Send + 'a>>` :30–41; `LogSink` :45–76
  (chunk 7 taught it the `ReminderAnchor` match — :54–72; its comment
  already names "the DesktopSink of the desktop-toast plan"); 
  `spawn_notifier` (log-and-continue consumer) :82–100. **No test
  module exists in this file yet.**
- `config.rs` — `AppMode { Daemon, Desktop, Kiosk }` (kebab-case,
  default `Daemon`) :103–113; `RemindersSection { enabled, tick_secs,
  default_before }` with `deny_unknown_fields` :271–284, its `Default`
  impl :286–294 and default fns :296–306; `ConfigError` :571–671 with
  the stable `kind()` discriminants :680–698; reminders validation
  ladder :907–916. Test precedents: defaults round-trip
  (`serde_round_trip_preserves_defaults`) :1387–1402; unknown-key
  decode error `unknown_key_in_the_reminders_table_is_a_decode_error`
  :1450–1456; kebab-case enum decode `kebab_case_enums_decode_from_toml_values`
  :1544–1564. The `KeymapPreset` doc :374–378 is the ruling precedent
  for unknown enum *values*: no meaningful drop ⇒ typed
  `ConfigError::Decode`.
- `bootstrap.rs` — `bootstrap(config, credentials, notifier_sink:
  Arc<dyn NotificationSink>)` :749; `bootstrap_with_modules` :780;
  the channel pair :836; the consumer spawn :851. The sink is an
  **injected parameter** — every test call site passes
  `Arc::new(LogSink)` explicitly (bootstrap.rs :1300+;
  `tests/bootstrap.rs` ×9; `tests/cli_reminders.rs` `RecordingSink`
  :732; `commands/sync.rs:357`, `commands/status.rs:551`,
  `ui_requests.rs:579` are test modules too). **Production call
  sites hard-coding `LogSink`** (the seven this chunk switches):
  `main.rs:331` (`boot_and_serve` — the daemon *and* kiosk
  lifecycles both enter here), `ui.rs:322` (the desktop UI),
  `repl.rs:812`, `commands/tasks.rs:51`, `commands/boards.rs:626`,
  `commands/boards.rs:782`, `commands/sync.rs:72`.
- Root `Cargo.toml` — `[workspace.dependencies]` :26; pin-comment
  house style (see slint :48–52: caret string here, exact version via
  `Cargo.lock`, Q12); workspace `rust-version = "1.96"` :21;
  `chrono` already carries `clock` (Local available to the app crate).
- `deny.toml` — licenses allow list :8–52 (MIT/Apache-2.0/BSD/…
  covered; per-crate GPL exceptions for slint only); advisories
  `ignore` :55–61 **already carries** RUSTSEC-2026-0370/0371/0372
  (the zbus-5-family unmaintained advisories, via
  slint → accesskit → atspi); `multiple-versions = "warn"` :67;
  all-targets graph (no `targets` restriction).

Domain (read-only for this chunk):

- `reminders.rs` — `AdhocAlarm { id, label, fire_at, created_at }`
  :226; `ReminderAnchor { Task(TaskId) | Alarm(AdhocAlarmId) }` :326;
  `Notification { reminder, anchor, title, occurrence }` :385 (the
  title is the fire-time snapshot — task title or alarm label,
  D-R6.5); `notifications_for` :405.

Dependency tree (this plan-day):

- `Cargo.lock` already carries **zbus 4.4.0 and zbus 5.19.0** (the
  5.x line via atspi 0.29) — the advisories above ride it. notify-rust
  4.18.2 requires zbus ^5, so it reuses the 5.19 line; no third zbus.
- notify-rust 4.18.2 verified via `cargo info` (crates.io) on
  2026-10-23: license `MIT OR Apache-2.0` (library dual-tier match),
  `rust-version = 1.89.0` ≤ workspace 1.96, features: `default =
  ["z"]`; `z = [zbus, serde, async]`; `z-with-tokio = [zbus, serde,
  tokio]`; `image`/`images` **not** default. Matches the D-R10 matrix
  exactly.

---

## 2. Shape of the chunk

Four production surfaces, nothing else:

1. **The pin** — root `Cargo.toml` gains one entry in
   `[workspace.dependencies]`; `crates/taskboard-app/Cargo.toml` gains
   `notify-rust.workspace = true`; `Cargo.lock` records the resolved
   tree.
2. **`DesktopSink`** — a new port implementor in `notifications.rs`,
   with the payload mapping expressed as pure functions and the
   DBus/platform work confined behind the port (D-R11).
3. **Config** — `SinkChoice` decodes `[reminders] sink` (strict, typed
   failure via the existing decode error); no new `ConfigError`
   variant, no `validate()` growth.
4. **Boot-time resolution** — `auto` resolves by `AppMode` in
   `bootstrap.rs`; the seven production call sites switch from
   hand-built `LogSink` to the resolver; test injection points stay
   untouched.

Commit ladder (each standalone, rebase-merge hygiene):

| # | Subject | Carries |
|---|---|---|
| 1 | `docs(plan): desktop notification toast sink` | this plan |
| 2 | `feat(app): the notify-rust workspace pin` | root + app Cargo.toml, Cargo.lock |
| 3 | `feat(app): the desktop toast sink` | `notifications.rs`: `DesktopSink` + `ToastPayload` mapping + `SinkKind`/`resolve_sink` + unit tests |
| 4 | `feat(app): the reminders sink choice` | `config.rs` `SinkChoice` + decode tests; `bootstrap.rs` resolver; the seven call sites; CHANGELOG Unreleased line |
| 5 | `test(app): the notifier consumer survives a failed delivery` | the consumer-continuation unit test |

(3 before 4 because `resolve_sink` needs `SinkChoice`; commit 3 keeps
compiling by declaring `SinkKind` with its own arm only if the split
chafes — otherwise `resolve_sink` lands in 4 and 3 carries only the
sink + mapping. Stage 2 picks the split that keeps both commits
standalone; both orders compile green.)

---

## 3. Decisions (every open question settled here)

### D1 — the pin (D-R10 verbatim)

```toml
# Desktop toast notifications (taskboard-app): the D-R10 stack ruling
# (reminders-surfaces design §3.10, GRANTED 2026-10-19) — one façade
# over zbus (Linux, pure Rust, no libdbus C dependency), 
# mac-notification-sys and tauri-winrt-notification. Default features
# (the `z` zbus backend); the image feature stays off (passive text
# toasts, D-R12); the `z-with-tokio` executor question is decided by
# measurement (D7).
notify-rust = "4.18.2"
```

- Caret string in `[workspace.dependencies]`; the lock pins 4.18.2
  (the slint comment precedent, Q12). No `default-features = false`,
  no explicit feature list — **default features are the ruling**
  (`z`, which pulls `serde`, already a workspace pin; `image` is not
  default, verified D-baseline).
- Placed once, under the platform-facing deps group of
  `[workspace.dependencies]`; the app member declares
  `notify-rust.workspace = true`. Nowhere else (dispatch trap:
  "pinned anywhere but the workspace root").
- Version policy: 4.18.2 is today's tip of the 4.18.x line the ruling
  grants; a future 4.18.x patch bump rides an ordinary lock refresh —
  not this chunk.

### D2 — `DesktopSink`'s structure (D-R11: everything OS-boundary stays behind the port)

In `notifications.rs`, after `LogSink`:

```rust
/// The desktop-toast sink: renders the payload mapping (D-R11) and
/// hands it to the platform notification service. All DBus/session
/// discovery, per-platform quirks, timeouts, and the app identity
/// live here — the port and its other implementors stay OS-free.
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopSink;
```

- **Unit struct**, like `LogSink` — no fields, no configuration: the
  config world ends at `resolve_sink` (D5); the sink itself is
  stateless.
- `impl NotificationSink for DesktopSink`: builds a `ToastPayload`
  (D3), converts it into a `notify_rust::Notification` (plain
  builder — no IO until `show`), and awaits delivery. Primary async
  shape: `toast.show_async().await` mapped over
  `Err(_) => NotifyError`. **Recorded fallback** (decided by a
  one-line compile probe at implementation, outcome reported): if
  `show_async`'s future does not satisfy the port's `Send` bound
  under the chosen feature set, the body switches to
  `tokio::task::spawn_blocking(move || toast.show())` with
  `JoinError` also mapped to `NotifyError`. Both shapes are
  `DesktopSink`-private; the port, the trait, `spawn_notifier`, and
  `NOTIFIER_CAPACITY` are byte-identical in the diff.
- **Passive toasts only (D-R12)**: no `actions`, no sound, no urgency
  hints, default platform timeout. Interactive snooze stays in-app
  (banner). The anchor rides in the payload (the `Notification`
  already carries it) — rendered nowhere, ready for #203's
  click-through.
- **The app identity**: `appname("taskboard")`; `summary` is the
  notification's title snapshot (D3). The identity constant lives in
  `DesktopSink`'s mapping — behind the port, per D-R11's inventory.
- **Failure logging (causes are not swallowed)**: `NotifyError` is a
  frozen unit struct (port discipline), so the *backend cause* would
  be lost to the consumer's typed warn. `DesktopSink` therefore logs
  the underlying backend error itself (`tracing::warn!`, the error's
  `Display`) inside `deliver` before returning `Err(NotifyError)`.
  The consumer's log-and-continue warn is unchanged — two lines per
  lost toast (cause + typed class), never fatal, never retried
  (D-R13). This is implementor-side logging, not a port widening.
- The module doc's "the desktop-toast sink is a later plan" sentence
  (:3–5) is updated to present tense; `LogSink` is not touched
  (byte-for-byte the daemon/kiosk path). LogSink's existing comment
  naming DesktopSink stays accurate.

### D3 — the payload mapping as pure functions (the design's "fake backend seam kept minimal")

The mapping is data-in/data-out, unit-testable without a bus (§5 test
hook); the notify-rust conversion is field-copy glue.

```rust
// BodyKind: the typed decision the tests own (the `duration_bucket`
// precedent: typed intermediate owns the edges, the phrase glue is
// untested).
enum BodyKind { DueNow, AtInstant }

fn body_kind(occurrence: DateTime<Utc>, now: DateTime<Utc>) -> BodyKind
// occurrence <= now  → DueNow   ("due now" — the normal one-tick case)
// occurrence >  now  → AtInstant (clock drift / early tick; the body
//                                  renders the local fire instant)

struct ToastPayload {
    summary: String,          // = notification.title (the snapshot)
    body: String,             // rendered occurrence (local time)
    appname: &'static str,    // "taskboard"
}

fn toast_payload(notification: &Notification, now: DateTime<Utc>) -> ToastPayload
```

- `summary` passthrough for **both anchors** (task-title snapshot and
  alarm label) — the tests construct a `Notification` with
  `ReminderAnchor::Task` and with `ReminderAnchor::Alarm` and assert
  the same struct fields; the chunk-7 widening is the reason the
  mapping must be anchor-agnostic.
- The `now` parameter is the purity seam: `deliver` supplies
  `Utc::now()`; the tests supply fixed instants. No clock trait, no
  new seam (the sink is the OS boundary; determinism is a property of
  the pure functions, not of the delivery).
- `notify_rust::Notification::new().appname(p.appname).summary(p.summary).body(p.body)`
  is the entire conversion — glue, untested (asserting field copies
  through a third-party builder would test the builder).
- Body phrase: "due now" for `DueNow`; for `AtInstant`, the
  occurrence in the viewer's local wall time via `chrono::Local`
  (already available: workspace `chrono` has `clock`). The phrase
  text itself is never asserted (AGENTS §5) — only `BodyKind` edges
  and structural non-emptiness are.

### D4 — config decode (D-R13: strict table, typed failure)

In `config.rs`, beside `RemindersSection`:

```rust
/// `[reminders] sink`: which notifier delivers fired reminders.
/// `auto` — the default — resolves by `[app] mode` at boot (D-R13):
/// desktop gets toasts day one with zero configuration; daemon/kiosk
/// keep the journald trail. An unknown value has no meaningful drop
/// (the `KeymapPreset` rule): it fails the decode, never defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SinkChoice {
    #[default]
    Auto,
    Log,
    Desktop,
}
```

- `RemindersSection` grows `/// Which notifier delivers fired
  reminders (D-R13).` `#[serde(default)] pub sink: SinkChoice` and
  its `Default` impl grows `sink: SinkChoice::default()`. The raw
  string stays stored? — **No: unlike `default_before`/`day_start_time`,
  a sink choice is one of three words, not free-form text; the typed
  enum *is* the stored form** (the `CredentialStoreKind`/`AppMode`
  precedent, not the duration-string precedent). No `validate()`
  growth: every decodable value is valid by construction; an
  undecodable value is `ConfigError::Decode` at the figment layer
  (kind `decode` — the `KeymapPreset` doc's own wording), so
  `ConfigError` and `kind()` gain nothing.
- **`config print` scope**: the human `render_text` view is a curated
  subset that has never listed the `[reminders]` section — it stays
  as is (no chunk-scope creep); the `--json` `ConfigView` payload
  embeds the whole config and gains `reminders.sink` additively. The
  defaults round-trip test (`serde_round_trip_preserves_defaults`)
  grows the `sink == SinkChoice::Auto` assertion — same behavior it
  already pins (defaults survive a round trip).
- Serialization hygiene: a plain `#[serde(default)]` enum field
  serializes `"auto"` in the defaults — acceptable (the section has
  no `skip_serializing_if` culture; the `web`-section off-placeholder
  trick exists for whole-table opt-ins, not for a three-word mode).

### D5 — `auto` resolution by `AppMode` (D-R13's matrix, boot-time only)

`notifications.rs` (pure, unit-tested):

```rust
/// The resolved sink kind — the typed result of the D-R13 matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SinkKind { Desktop, Log }

/// The D-R13 resolution: an explicit choice wins; `auto` consults the
/// mode — Desktop toasts, Daemon/Kiosk keep the log trail.
pub(crate) fn resolve_sink(choice: SinkChoice, mode: AppMode) -> SinkKind
```

Matrix (all cells typed-pinned by the unit tests):

| `sink` \ `mode` | Daemon | Desktop | Kiosk |
|---|---|---|---|
| `auto` (default) | Log | **Desktop** | Log |
| `log` | Log | Log | Log |
| `desktop` | Desktop | Desktop | Desktop |

- Explicit `desktop` under Daemon/Kiosk is honored, not refused: the
  kiosk operator who asks for toasts gets a sink whose failures are
  logged NotifyErrors (D-R13's degradation paragraph governs
  delivery, not resolution).
- `bootstrap.rs` hosts the config entry point (the dispatch's "auto
  resolves by AppMode" lives here):

```rust
/// The boot-time notifier sink: `[reminders] sink` resolved against
/// `[app] mode` (D-R13). One `Arc` per process — the consumer holds
/// it for its lifetime; no live swap exists (D-R13).
#[must_use]
pub fn resolve_notifier_sink(config: &AppConfig) -> Arc<dyn NotificationSink>
```

  — a two-arm exhaustive match over `SinkKind`. The matrix behavior
  is owned by `resolve_sink`'s tests (one behavior, one test, lowest
  layer); the `Arc` glue adds none.

### D6 — the seven production call sites switch to the resolver

`Arc::new(…LogSink)` → `resolve_notifier_sink(&config)` at: `main.rs`
(boot_and_serve — covers daemon *and* kiosk), `ui.rs`, `repl.rs`,
`commands/tasks.rs`, `commands/boards.rs` ×2, `commands/sync.rs`.

- **Uniformity is the point**: every process that boots the core
  honors the same config key — a one-shot `tasks edit` in desktop
  mode and the UI behave identically (the acceptance's "zero
  configuration under `auto`" is a property of the config, not of an
  entry-point subset). The alternative (switching only the three
  frontends) would make the same file mean different sinks per verb —
  rejected.
- **Borrow discipline**: `bootstrap(config, …)` consumes `config`, so
  each site computes the sink first: `let sink =
  crate::bootstrap::resolve_notifier_sink(&config);` then passes it.
  Mechanical, ~2 lines per site.
- **Test call sites are untouched** — they inject `LogSink`/fakes
  deliberately (the suite "keeps running `sink = log`" by
  construction, §5 hook). The doctest in `bootstrap.rs`'s module doc
  keeps its explicit `LogSink` (it teaches injection, not resolution).

### D7 — the `z-with-tokio` measurement (design §3.10: "decided by measurement, not doctrine")

The two candidate feature sets:

- **default**: `notify-rust = "4.18.2"` — zbus 5 with its internal
  async-io executor thread.
- **tokio**: `default-features = false, features = ["z-with-tokio"]`
  — zbus runs on our tokio runtime; no internal executor thread.

Measured, in this order (each outcome recorded even when trivially
decided):

1. **Send probe** (compile-time, both variants): does a
   `Pin<Box<dyn Future<Output = …> + Send>>` wrapping
   `show_async().await` compile? The port's `Send` bound is
   non-negotiable; if only one variant satisfies it, that variant
   wins outright.
2. **Executor-thread presence**: OS thread count of the probe process
   (`/proc/self/status` `Threads:`) before/after the first delivery.
   Expected: default grows one persistent thread; tokio does not.
   This is the kiosk-memory nicety the design names.
3. **RSS delta**: `VmRSS`/`VmHWM` before and after N = 20 deliveries,
   both variants, same machine, back to back.

Harness: a **scratch probe crate in `/tmp`** driving notify-rust
directly with the two feature sets — probe code never lands (the
round's evidence clauses); the numbers and the verdict land in the
stage-2 report, and the root-Cargo.toml pin comment's last line states
the outcome ("z-with-tokio: adopted/rejected per measurement,
report <path>"; or "deferred: <reason>").

Environment honesty (MD-5 style): the dev platform has a D-Bus
session; if a notification *daemon* is present, deliveries are real
toasts (a handful, harmless); if the session bus or daemon is absent,
the connect-failure path still exercises the connection machinery —
the measurement is then recorded with that caveat, or **deferred**
with the reason (no session bus on the runner; follow-up owner: the
kiosk memory-bench lane, cf. the 2026-10-20 kiosk bench plans).
Deferral is an acceptable recorded outcome; silence is not.

### D8 — deny expectations (what the new tree may and may not demand)

- **Expected new packages (Linux)**: `notify-rust` itself; zbus ^
  5 resolves to the **already-locked 5.19.0** (via atspi) — reused,
  not duplicated; possible small additions if the lock's zbus feature
  union shifts (futures/async-io family members already present).
- **Other targets** (deny checks all): `tauri-winrt-notification` +
  the `windows`/`windows-core` family (MIT OR Apache-2.0) on Windows;
  `mac-notification-sys` + the `objc2` family (permissive MIT) on
  macOS — all inside the existing allow list.
- **Licenses**: expected green with **zero new allowances**. If the
  gate demands one, the allowance is added only with the demanding
  output quoted in a deny.toml comment + the stage-2 report (dispatch
  trap: "allowances added without the gate demanding them").
- **Advisories**: the zbus-5-family unmaintained ignores are already
  in place (RUSTSEC-2026-0370/0371/0372); no new advisory expected;
  `yanked = "warn"` unchanged.
- **`multiple-versions = "warn"`**: the tree already carries two zbus
  majors (4.4 + 5.19) at warn; the status quo continues — not a
  failure, but the stage-2 report notes the resolved zbus version
  notify-rust actually got.
- **Gate form**: `cargo deny check` runs over the *new* tree
  explicitly in stage 2 (dispatch acceptance), on top of the standard
  §10 suite.

### D9 — the consumer and capacity stay put; the failure-continuation behavior gains its first pin

- `NOTIFIER_CAPACITY` stays 64. Its comment's revisit trigger — "a
  future OS sink **with backpressure policy**" — does not fire: v1
  toasts are passive fire-and-forget (D-R12); sequential delivery of
  ≤ 64 queued payloads outlives any realistic tick burst. Rewriting
  the comment or the constant is scope creep.
- `spawn_notifier` is untouched. Its failure-continuation behavior
  (one lost delivery must not kill the actor — D-R13's "never
  fatal") has **no test today** (the `GateSink` test pins join
  boundedness, not continuation). Chunk 8 adds the missing pin at
  the lowest layer that owns it — a `FailingSink` unit test in
  `notifications.rs`: first delivery errs, second succeeds; two
  notifications through the channel; sender dropped; join awaited;
  assert the typed counts (failed = 1, delivered = 1). Deterministic:
  channels + join, no sleeps (AGENTS §5).
- "Never retried, never fallback-sent" is an **absence** claim: it is
  enforced structurally (no retry loop exists to call) and by stage-3
  review, not by a test — a test cannot meaningfully assert code that
  must not exist.

### D10 — no test touches a real bus

The design's test hook: "a fake backend seam kept minimal (the
mapping is pure)". Concretely: no test constructs `DesktopSink` for a
real delivery; no test calls `show`/`show_async` (it would side-effect
on dev machines and flake on bus-less CI). The chunk's testable
surface is exactly: the pure mapping (D3), the resolution matrix
(D5), the config decode (D4), and the consumer continuation (D9).
Delivery-path coverage ends at the port boundary — the same line
`LogSink` tests have always drawn. A manual smoke (one real toast on
the dev session under `sink = "desktop"`) is stage-2 evidence for the
report, never a committed test.

### D11 — bookkeeping

- CHANGELOG `Unreleased › Added` gains one bullet (chunk-7 style):
  desktop toasts via `[reminders] sink` (#344, chunk 8) — auto by
  mode, passive toasts (D-R12), failure = logged NotifyError, never
  retried/fallback-sent (D-R13), the `notify-rust` pin.
- No tracker edits beyond the PR body's `Closes #344` (stage 3;
  MD-4). #203/#352 stay untouched.

---

## 4. Implementation inventory (file-by-file)

| File | Change | ~Size |
|---|---|---|
| `Cargo.toml` (root) | the D1 pin + comment | +5 |
| `crates/taskboard-app/Cargo.toml` | `notify-rust.workspace = true` | +1 |
| `Cargo.lock` | generated subtree (notify-rust; zbus reuse) | +15–40 |
| `src/notifications.rs` | module-doc touch-up; `DesktopSink` + `ToastPayload`/`BodyKind`/`toast_payload`/`body_kind`; `SinkKind`/`resolve_sink`; `#[cfg(test)]` module (mapping, matrix, FailingSink) | +~190 |
| `src/config.rs` | `SinkChoice` + field + Default; tests (decode 3 words, unknown decode error, round-trip growth) | +~45 |
| `src/bootstrap.rs` | `resolve_notifier_sink` (+doc) | +~14 |
| `src/main.rs`, `src/ui.rs`, `src/repl.rs`, `src/commands/{tasks,boards,sync}.rs` | the seven sites: pre-compute sink, pass it | +7/−7 |
| `CHANGELOG.md` | the D11 bullet | +9 |

**Estimated diff: ≈ +270/−10 source lines plus lock churn — a solid
M**, matching the dispatch's size call. No new files except none —
everything lands in existing modules (no `notifications/desktop.rs`
split: the sink is ~60 lines of logic plus tests; a second file would
be structure without a reason).

SPDX: no new `.rs` files; touched files already carry their headers
(app crate = AGPL-3.0-only). `#![forbid(unsafe_code)]` unaffected
(notify-rust's internals are its own crate).

---

## 5. Test matrix (TDD order — write each test first, watch it fail, then the code)

| Layer | Test | Owns (house rule) |
|---|---|---|
| config unit | `sink` decodes `auto`/`log`/`desktop` (kebab decode precedent) | the decode surface |
| config unit | unknown sink word is a decode error | strict failure (KeymapPreset rule) |
| config unit | defaults round-trip carries `sink: Auto` (extends the existing test) | default-serializes behavior |
| notifications unit | `resolve_sink` matrix — all 9 cells typed | the D-R13 resolution (one behavior, one test, lowest layer) |
| notifications unit | `toast_payload`: summary passthrough with a `Task` anchor; with an `Alarm` anchor; `appname` constant; body non-empty both anchors | the pure mapping at the port boundary |
| notifications unit | `body_kind` edges: past/at → `DueNow`; future → `AtInstant` | the body decision (typed, no phrase text) |
| notifications unit | `FailingSink`: consumer logs-and-continues (failed=1, delivered=1) | spawn_notifier's never-fatal contract (D9) |
| integration (existing, untouched) | `tests/cli_reminders.rs` e2e keeps injecting `RecordingSink`; `tests/bootstrap.rs` keeps `LogSink` | the suite runs `sink = log` (§5 hook) — regression canary that the port move broke nothing |

Excluded on purpose (D10): any test performing a real notification
delivery; any assertion on toast/log/label *text* (AGENTS §5).

The full AGENTS §10 gate set runs per commit cluster and before
handoff; stage 2 runs `cargo deny check` over the new per-target tree
explicitly and records the outcome.

---

## 6. Non-goals (dispatch §9 restated + this plan's)

- The port does not move: `NotificationSink`, `NotifyError`,
  `spawn_notifier`, `NOTIFIER_CAPACITY` byte-identical (D-R11; the
  stage-3 diff check).
- No action buttons, no sound, no urgency hints, no escalation (#203).
- No retry, no fallback-to-log after a failed delivery (the
  double-delivery ruling).
- No live-swap seam: the sink is chosen once at boot; `resolve_notifier_sink`
  is called once per process start.
- No kiosk-surface change, no banner change, no reminder-center work.
- No `config print` text growth (D4), no new `ConfigError` variant.
- No `z-with-tokio` adoption without the D7 measurement (or a recorded
  deferral).
- No test against a real D-Bus session (D10).

---

## 7. Risks, residual windows, interpretation flags

- **notify-rust's async shape under the port's `Send` bound** — the
  one genuinely open implementation detail. Mitigated by D2's
  recorded fallback (`spawn_blocking` + sync `show`), which is
  robust but likely thread-per-delivery under default features —
  which is exactly why the D7 measurement runs *before* the final
  feature choice. Worst case: default features + spawn_blocking
  (correct, modest thread cost on deliveries only, kiosk uses LogSink
  anyway). Risk: LOW-MED, procedure defined, outcome recorded.
- **zbus feature-union shift** in the lock (notify-rust's `z` may
  activate zbus features atspi didn't). Mitigation: the lock diff is
  reviewed in stage 2; build + deny green is the gate. Risk: LOW.
- **Transitive license drift on macOS/Windows targets** (objc2 /
  windows families). All permissive and expected inside the allow
  list; if not, D8's record-first rule applies. Risk: LOW.
- **A toast-failing desktop** (no notification daemon — e.g. a bare
  window manager): every firing logs cause + typed warn, never
  fatal. Accepted D-R13 posture; the user's remedy is snooze.
  Documented, not coded around.
- **`config print` JSON consumers** see a new `reminders.sink` key —
  additive; the stable `kind()` vocabulary is unchanged. Risk: NONE,
  noted for the wave-exit docs chunk.
- **Interpretation flag**: D-R13's "the consumer holds one `Arc` from
  bootstrap" reads today as *injected through* `bootstrap` (existing
  signature) with the resolver as the production constructor — not as
  "bootstrap grows a config-resolved default parameter". This plan's
  D5/D6 is that reading; it preserves test injection and changes no
  signature. If stage 3 reads D-R13 differently, the divergence is a
  recorded-reason deviation against prose (allowed), not against an
  ACCEPTED ruling.

---

## 8. Acceptance checklist (stage 2/3 verify against this)

- [ ] Pin: single `notify-rust = "4.18.2"` entry in root
      `[workspace.dependencies]`; app member via `.workspace = true`;
      default features; no image feature; comment cites D-R10 + the
      z-with-tokio outcome.
- [ ] Port untouched: trait, `NotifyError`, `spawn_notifier`,
      capacity byte-identical in the diff.
- [ ] `DesktopSink`: passive (no actions/sound/urgency), appname
      `taskboard`, summary = title snapshot, body = occurrence
      rendering, anchor carried not rendered, backend cause logged
      before `Err(NotifyError)`.
- [ ] Mapping pure and unit-tested at the port boundary (both
      anchors); `BodyKind` edges typed-pinned.
- [ ] `SinkChoice` decodes strictly (three words; unknown = decode
      error); default `auto`; round-trip test grown.
- [ ] Resolution matrix: all nine cells typed-tested; explicit
      choice beats mode; `auto` Desktop→Desktop, Daemon/Kiosk→Log.
- [ ] All seven production call sites use the resolver; no test call
      site changed; sink computed before `config` is moved.
- [ ] Consumer-continuation test green (failed=1, delivered=1).
- [ ] `cargo deny check` green over the new per-target tree; zero
      allowances beyond what the gate demanded (any addition quoted).
- [ ] z-with-tokio: measured-and-decided or deferred-with-reason;
      recorded in the pin comment + stage-2 report.
- [ ] Gates (AGENTS §10) green; suite runs log-path throughout.
- [ ] CHANGELOG Unreleased bullet; stage 3's PR body carries
      `Closes #344`; probe crates never landed.

---

## 9. References

- Dispatch plan: `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md` §1 (order, mandate), §9 (this chunk's covers/acceptance/traps/prompts).
- Implementation plan: `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md` §2 DDF-13 (sink half), §3 chunk 8, §4 gates, §5 exit row.
- Design round: `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md` Part C (§3.10–§3.13), §4 decision record (D-R10 APPROVED / D-R12, D-R13 ACCEPTED), §5 test hooks, §6 risks.
- Predecessor plan: `.artifacts/plans/2026-10-23-daily-driver-adhoc-alarms-plan.md` (D13 — the LogSink anchor match this chunk builds beside).
- Upstream verified 2026-10-23 via `cargo info notify-rust`: 4.18.2, MIT OR Apache-2.0, MSRV 1.89, features `default = ["z"]`, `z-with-tokio = [zbus, serde, tokio]`.

# [Plan] Substrate Hardening & Surfacing — DR-8 chunk 4 (TDD implementation plan)

- **Date**: 2026-11-05
- **Kind**: stage-1 implementation plan — chunk 4 of the DR-8
  implementation dispatch (branch `feat/filesync-hardening`, issue
  #632). TDD-first: every commit below names its failing tests before
  the code that satisfies them.
- **Origin** (normative): the accepted design round
  `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  — §3 Part F (D-SF14 + ruling Q8), D-SF15, §5 test hooks, §6 risks,
  §7 chunk 4; the transport facts
  `.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`
  F1–F8 + §2/§3 (**binding** — §3's hosted-drift caveat is this
  chunk's live-pin mandate); the dispatch plan §5 + §1; issue #632;
  ADR 0020 (decisions 3/4 — the fold's convergence and the hash's
  divergence bounds this chunk's finding leans on).
- **Status**: READY for the stage-2 implementation agent — with the
  recorded re-sequencing settlement of D-FH1. Decisions D-FH1…D-FH10
  settle every open question this chunk owns; deviations from a
  normative origin section need a recorded reason in the stage-2
  report.

---

## 1. Context & verified anchors (verified 2026-11-05 against `origin/main` @ `02b5d347`)

| Fact | Anchor | Consequence for this chunk |
|---|---|---|
| Chunk 2's findings vocabulary exists | `crates/taskboard-sync-files-nextcloud/src/status.rs` | `FilesSyncStatus { last_cycle: Option<CycleOutcome>, forked_paths: BTreeSet<String>, parked: BTreeMap<String, ParkKind>, last_heartbeat_written: Option<DateTime<Utc>> }` on a `tokio::sync::watch` (latest-state semantics); `CycleOutcome::{Ok{published, read_devices}, Failed}`; `ParkKind::{NewerFormat{format}, Quarantined}`. Its module doc **names chunk 4 as the consumer** ("the surface that consumes them … is chunk 4's"). This chunk grows exactly this struct — no second vocabulary |
| The actor already stages this chunk's inputs | `cycle.rs` `LoopState` | `heartbeats: BTreeMap<DeviceId, DeviceInfo>` ("chunk 4's freshness input" — decoded tolerantly, garbage ⇒ Quarantined park), `docs: BTreeMap<DeviceId, UserDocument>` (folded peers only — parked peers are removed), `published_hash`, `pending_heartbeat`, `forked_paths`; vanished devices drop from `docs`/`heartbeats` in the same arm that drops their rows. The divergence derivation is pure bookkeeping over facts the loop already holds |
| The app holds the receiver half | `bootstrap.rs` `App.files_sync_status: Option<watch::Receiver<FilesSyncStatus>>` | Comment names chunk 4 as the consumer. Present iff the spawn gate fired (`[files_sync] enabled` ∧ default connector Nextcloud, D-FT10); `None` ⇒ nothing to surface. Hermetic boot harness exists: `files_sync_enabled_config` + `bootstrap_with_modules` + `AcceptingStore` (bootstrap.rs tests) |
| The delivery-health watch precedent | `ui-slint/src/lib.rs` (`drain_delivery_health`), `presentation.rs` (`DeliveryHealth`), `main.rs:834` | The pattern for a non-`AppState` status surface: the UI crate owns the plain payload type, the app-side producer writes the watch, `main.rs` passes `app.<x>.clone()` into `mount(...)`, the adapter drains it per tick. `taskboard-ui-slint` already depends on `tokio` for watch receivers — no dependency-boundary novelty |
| The dependency fence | AGENTS §2 (`taskboard-ui-slint` cannot depend on IO crates) | The UI must never name `taskboard-sync-files-nextcloud` (an IO crate). The `FilesSyncStatus` → UI-model mapping lives in `taskboard-app` (which sees both crates); the orphan rule forbids a `From` impl across the two foreign types, so it is a plain mapping function in the app crate |
| The CLI status verb | `crates/taskboard-app/src/commands/status.rs` | `StatusView` is one serde shape for three projections: `server` (IPC daemon answer), `in_session` (REPL), `no_daemon` (bare `{"daemon": false}` byte pin). Additive growth rides `#[serde(default, skip_serializing_if = "Option::is_none")]` — the `connectors`/`engine` precedents, explicitly "no protocol-version bump". Render tests pin **data survival** (uuids, names, numbers), never prose |
| The status types files-sync must NOT touch | `taskboard-domain/src/state.rs` | `AppState.sync: SyncStates` is **board-keyed Deck status**, written by the state engine (`SyncStatus`/`SyncPhase`/`SyncErrorKind`). Per-device files-sync findings never enter `AppState`: that would require an engine channel (chunk 3's territory). Keeping out of `AppState` is what makes the promotion ahead of chunk 3 sound — and no insta snapshot churns |
| Heartbeat & state hash | `taskboard-sync-files/src/heartbeat.rs`, `hash.rs`; ADR 0020 decisions 3–4 | `DeviceInfo { app_version, format_version, last_clock, state_hash, written_at }`; the hash is an opaque equality fingerprint of the device's whole published view; differently-tagged hashes can never compare equal ("worst case a divergence finding, never a false converged") — the derivation must preserve that safe direction |
| Tier-3 test convention | `docs/testing_strategy.org` §8; `taskboard-sync-nextcloud/tests/common/mod.rs`; `taskboard-sync-files-nextcloud/tests/{common/mod.rs, it_docker.rs}` | `#[ignore]` + `live_config()` reading `TASKBOARD_IT_NEXTCLOUD_URL/USER/TOKEN` after `dotenvy::dotenv()`; **skip-never-fail** when unset; `with_deadline` (30 s); per-run unique, never-deleted paths; `retrying_server_errors` for tier scaffolding only. The files docker tier (`it_nextcloud_docker_files_*`) is the shape to re-point at the hosted tier |
| Docs ownership | `docs/architecture.org`, `README.org`, `docs/testing_strategy.org` | **Zero** files-sync mentions on `main` — chunks 1–2 touched no docs (the dispatch assigned all doc amendments to this chunk). architecture.org "Crate Roles & Dependencies" enumerates crates 1–6; README has Configuration/Repository Structure sections; `docs/kiosk_bringup_runbook.org` is the runbook naming precedent; the landed `it_docker.rs` header already cites §8 (the doc has no files subsection yet — the amendment closes that dangling reference) |
| The device.json home | `bootstrap.rs:169–178` | `<dirs::data_dir()>/taskboard/device.json` (D-SF4's literal home; `~/.local/share/taskboard/device.json` on Linux) — the runbook's rename procedure names this path |

## 2. Scope & non-goals

**In scope**: the findings growth in
`taskboard-sync-files-nextcloud/src/status.rs` (peer freshness +
divergence, D-FH2) and the cycle-side derivation (D-FH3); the CLI
`status` projection + render (D-FH4, D-FH10); the UI status-bar chip
via the delivery-health pattern (D-FH5); the runbook (D-FH6);
architecture.org / README.org / testing_strategy.org amendments
(D-FH7); the live Tier-3 pins (D-FH8); CHANGELOG entry.

**Non-goals** (fences): **any new status taxonomy** — the fuller
"stale peers" vocabulary stays the recorded open question it already
is (design Part F, #632); no `AppState`/`SyncStates`/state-engine
changes and no engine channel (chunk 3's territory — grep pins in §5);
no `SyncReport` change; no notify_push; no settings/proofs; no new UI
*views* (a status-bar chip beside the sync badge, not a view); no
age thresholds anywhere (freshness is an age display, never an alarm);
no quota/throttling *assertions* (§6's "measure" is a stage-2
observation note, never a test — no quality ceilings, AGENTS §5); no
reserved-kind emission; no `taskboard-domain` change at all.

## 3. Design decisions (D-FH1…D-FH10, normative for stage 2)

### D-FH1 — Re-sequencing settlement & issue bookkeeping

The dispatch's precondition was "chunks 1–3 landed". Chunks 1–2 are on
`origin/main`; chunk 3 (`feat/filesync-engine-seam`, #631) is gated on
v0.2's journal entity, which has not landed, and the dispatching user
promoted chunk 4 ahead of it — exactly the promotion dispatch §1
reserves: *"chunk 4's docs/runbook/live-pin work may be promoted ahead
of 3 by the dispatching user (its code surface is independent; note
the re-sequencing in the PR bodies)"*. This plan's code surface
touches none of chunk 3's territory (no engine channel, no
`UserDataReport`, no `AppState` shape change — grep-pinned in §5), so
the promotion is sound.

- **Bookkeeping deviation, recorded reason**: dispatch §5 says this
  PR's body carries `Closes #632` **and** `Closes #626`. With chunk 3
  re-sequenced *behind* this chunk, the round's last chunk is 3, and
  dispatch §6's own rule is "the round completes with its last chunk".
  **Decision**: this PR's body carries `Closes #632` only, plus a line
  noting that #626 (the DR-8 round) closes with chunk 3's PR. Stage 2
  records this deviation in its report; stage 3 verifies the PR body.

### D-FH2 — The findings set (the vocabulary's growth, minimal)

**Decision**: `FilesSyncStatus` grows by exactly two fields and one
new payload struct. No findings-list enum, no severity wrapper, no
second vocabulary — the surfaces read the same latest-state struct.

```rust
/// A peer device's latest decoded heartbeat — the freshness inputs
/// (design Part F's "synced as of N min ago" consumer). Presence
/// facts only; the divergence comparison consumes `info.json`'s
/// `state_hash` actor-side and never surfaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFreshness {
    /// When the peer last wrote its heartbeat (presence freshness).
    pub written_at: chrono::DateTime<chrono::Utc>,
    /// The newest entry clock the peer has folded (data freshness).
    pub last_clock: chrono::DateTime<chrono::Utc>,
}

pub struct FilesSyncStatus {
    // … the four existing fields unchanged …
    /// Latest decoded heartbeat per peer.
    pub peers: BTreeMap<DeviceId, PeerFreshness>,
    /// Peers whose heartbeat state hash disagrees with this device's
    /// current view past the quiescence window (D-FH3). Empty means
    /// converged — or not yet judgeable.
    pub diverged_from: BTreeSet<DeviceId>,
}
```

The **complete typed findings set** the surfaces consume (this is the
"findings set" deliverable of the chunk):

| # | Finding | Type | Origin |
|---|---|---|---|
| 1 | Cycle outcome | `last_cycle: Option<CycleOutcome>` | chunk 2 (unchanged) |
| 2 | The fork alarm (stop-write, keep-read) | `forked_paths: BTreeSet<String>` | chunk 2 (unchanged) |
| 3 | Quarantine / newer-format notices (D-SF15) | `parked: BTreeMap<String, ParkKind>` | chunk 2 (unchanged) |
| 4 | Own heartbeat freshness | `last_heartbeat_written: Option<DateTime<Utc>>` | chunk 2 (unchanged) |
| 5 | Peer freshness (ages derived at render) | `peers: BTreeMap<DeviceId, PeerFreshness>` | **new** |
| 6 | The divergence finding | `diverged_from: BTreeSet<DeviceId>` | **new** |

Settled sub-decisions:

- **Freshness is an age display, never an alarm.** No thresholds, no
  "stale" boolean — a merely-slow peer shows a large age on the CLI
  and can never fire divergence *for being slow* (the review's trap
  (a); freshness and divergence are different findings). The "stale
  peers" taxonomy question stays open and un-invented.
- **`last_clock` rides along** because the design's heartbeat carries
  two consumers — presence freshness (`written_at`) and data-newness
  (`last_clock`) — both plain instants; presentation chooses what to
  show. **`state_hash` and `app_version` deliberately do not
  surface**: the hash's only consumer is the actor-side divergence
  derivation; app-version mismatches surface as the `NewerFormat`
  park's business ("peer runs a newer app", D-SF15's wording).
- **Serde: none added here.** `FilesSyncStatus` stays a `Clone + Eq`
  runtime type; the CLI defines its own serde view (D-FH4).
- **Population/removal** rides the existing arms: `publish_status`
  copies the two new fields from `LoopState` (`heartbeats` mapped to
  `PeerFreshness`; the derived `diverged_from`); the vanish arm
  already removes the device from `heartbeats`/`docs`, and D-FH3's
  re-derivation drops it from `diverged_from` on the next Ok cycle.

### D-FH3 — The quiescence-window derivation (deterministic, cycle-counted)

> **Stage-3 amendment (2026-11-07, recovery review):** the
> clock-freedom argument below does not by itself keep a slow peer out
> of the finding — an offline peer's heartbeat hash describes its
> older view, so after any local edit it disagreed and fired. The
> shipped rule adds one structural precondition: a peer is compared
> only once it has absorbed this device's view (`has_absorbed`:
> folding the view into its published document changes nothing).
> Still clock-free and deterministic; pinned by the pure-rule test
> `a_peer_behind_our_view_is_lagging_not_diverged` and actor matrix
> 23.

**Decision**: the divergence finding is a pure, clock-free rule over
evidence the loop already holds. No wall clock, no `Instant`, no
`Duration` in the derivation — deterministic under paused time by
construction (the acceptance rule).

Per steady/attach cycle (on an `Ok` verdict):

- `view = StateHash::of(&merged)` — this cycle's merged view's hash.
- `quiet` := the cycle published nothing ∧ no heartbeat is pending
  post-cycle (`!published && !st.pending_heartbeat`). "No pending
  writes" is exactly this conjunction: our own writes are settled.
- `disagreeing := { d ∈ docs.keys() ∩ heartbeats.keys() :
  heartbeats[d].state_hash != view }`.

**Exclusions** (each with its recorded reason):

1. *Parked peers* (∉ `docs`): their park finding already says why
   (`Quarantined`/`NewerFormat`); their hash describes content this
   device cannot honestly fold, so disagreement is expected, not
   divergent. Firing there would double-report one condition.
2. *Peers without a decoded heartbeat* (404 or garbage `info.json`):
   no evidence, no finding.
3. *Self*: after a fork the clone's content enters `docs` under self's
   id, but self's heartbeat never enters `heartbeats`
   (`read_dirty_peers` filters self) — and the **fork alarm is the
   specific finding** for that condition. Divergence stays about the
   fleet.
4. *Hash-tag honesty*: the comparison is `StateHash` equality — a peer
   running a future hash scheme can never compare equal (ADR 0020
   decision 4), so it *will* appear disagreeing. That is the recorded
   safe direction ("worst case a divergence finding, never a false
   converged"); no tag parsing — the type is opaque by design.

**The window rule** (the settled semantics of "two cycles + no
pending writes"):

| Cycle | `quiet_streak` | finding (`diverged_from`) |
|---|---|---|
| Ok, quiet, `disagreeing` non-empty | `+= 1` | `disagreeing` once `quiet_streak ≥ 2` |
| Ok, quiet, `disagreeing` empty | reset to 0 | cleared (∅) — agreement is evidence too |
| Ok, busy (published ∨ pending heartbeat) | reset to 0 | cleared — local writes in flight mean the fleet is mid-convergence; firing here is noise |
| Failed | **held** | **held** — no new evidence this cycle; the honest retry follows |

Mechanically: `LoopState` gains two private fields
(`quiet_streak: u32`, `diverged_from: BTreeSet<DeviceId>`); the rule
is a pure helper so the table above is unit-tested without the actor:

```rust
fn advance_divergence(
    quiet_streak: u32,
    diverged: &BTreeSet<DeviceId>,
    quiet: bool,
    view: &StateHash,
    docs: &BTreeMap<DeviceId, UserDocument>,
    heartbeats: &BTreeMap<DeviceId, DeviceInfo>,
) -> (u32, BTreeSet<DeviceId>);
```

called from the Ok-verdict bookkeeping of `run_cycle` (before
`publish_status`, which copies the result into the watch).

**Why the merged-view hash and not `published_hash`**: the design's
predicate is "state hashes disagree" — the honest comparison is each
peer's latest published view against *our current settled view*. The
last-PUT hash would report a device's own convergence lag as
divergence; on a quiet cycle the merged view equals the published view
anyway (the skip-PUT proves it), so the window's "no pending writes"
arm is what makes the two equivalent when it matters.

### D-FH4 — The CLI status projection

**Decision**: mirror the typed status verbatim in a serde view owned
by `taskboard-app`; `StatusView` gains one additive field; present iff
the actor spawned.

```rust
// commands/output.rs — the SyncStatusView precedent's shape
pub struct FilesSyncStatusView {
    pub last_cycle: Option<CycleOutcomeView>,
    pub last_heartbeat_written: Option<chrono::DateTime<chrono::Utc>>,
    pub forked: Vec<String>,          // substrate paths, sorted
    pub parked: Vec<ParkedPathView>,  // { path, kind, format: Option<u32> }, sorted by path
    pub peers: Vec<PeerFreshnessView>,// { device, written_at, last_clock }, sorted by device
    pub diverged_from: Vec<String>,   // device-id strings, sorted
}

#[derive(Serialize, Deserialize, …)]
#[serde(rename_all = "snake_case")]
pub enum CycleOutcomeView { Ok { published: bool, read_devices: usize }, Failed }

// commands/status.rs — additive, the `connectors`/`engine` precedent
pub files_sync: Option<FilesSyncStatusView>,
// #[serde(default, skip_serializing_if = "Option::is_none")]
```

- **Presence rule**: `server` and `in_session` carry it iff
  `app.files_sync_status.is_some()` (read via `borrow().clone()` — the
  receiver is never held across an `.await`; there is no `.await` on
  this path at all). `no_daemon` stays bare — the `{"daemon": false}`
  byte pin survives untouched (the engine-key precedent test).
- **IPC additivity**: the field rides `default` +
  `skip_serializing_if` — old clients decode new answers and vice
  versa (the `connectors` precedent's both-ways pin); no
  protocol-version bump.
- The kind strings are `"quarantined"` / `"newer_format"` — a faithful
  mirror of `ParkKind`, machine-readable, not new taxonomy.

### D-FH10 — The status-line shape (the human form)

One lead line + indented finding lines, appended after the existing
connector/board lines (the array-rendering precedent: existing pins
pass unmodified):

```
files-sync: last cycle ok (published, 2 peers), heartbeat 3m ago
  peer 0193…8a2e — heartbeat 5m ago (data 2h ago)
  fork: taskboard/devices/<id>/appdata.json — writes stopped, reads continue
  parked: taskboard/devices/<id>/appdata.json (newer format 2)
  diverged: <device-id>[, <device-id>…]
```

Ages render from `Utc::now()` at render time (presentation text,
never asserted); the JSON view carries the instants (machine truth).
Text tests follow the house **data-survival** pattern — device uuids,
paths, counts must survive rendering; wording is never pinned (AGENTS
§5, no text assertions).

### D-FH5 — The UI surface: a status-bar chip, delivery-health pattern

**Decision**: a files-sync chip beside the existing sync badge in the
`StatusBar` composition, fed by the delivery-health watch pattern,
with the mapping owned by the app (the dependency fence).

- **ui-slint owns the plain model** (it cannot name the transport
  type): `presentation::FilesSyncChip { state: FilesSyncChipState }`
  with `enum FilesSyncChipState { Ok, CycleFailed, Parked, Diverged,
  Forked }`. No ages on the chip — recorded reason: keeps the chip
  derivation clock-free and the UI test surface deterministic; the
  design's "synced as of N min ago" consumer is served by the CLI
  line (D-FH4/D-FH10). This is the Q8 "minimal surfacing" reading.
- **The mapping is an app-side plain function** (orphan rule: no
  `From` impl across foreign types):
  `fn files_sync_chip(status: &FilesSyncStatus) -> ui_slint::presentation::FilesSyncChip`,
  precedence ladder **Forked > Diverged > Parked > CycleFailed > Ok**
  (the fork halts data flow — loudest; a park is informational;
  a failed cycle is transient). Unit-tested in the app crate over the
  real transport type.
- **The pump**: in the UI boot path (`main.rs`), when
  `app.files_sync_status` is `Some`: create
  `watch::channel(FilesSyncChip::default())`, seed it from the current
  borrow, spawn a forwarding task
  (`while rx.changed().await { send_modify(map(borrow_and_update())) }`)
  that exits when the transport actor drops its sender (shutdown) —
  the same channel-close lifecycle posture as the escalation/focus
  runtimes. `mount(...)` gains a trailing
  `files_sync: Option<watch::Receiver<FilesSyncChip>>` parameter
  (`None` when off ⇒ no chip); all mount call sites (main + UI test
  helpers) propagate it — compiler-driven, mechanical.
- **Rendering**: the chip is a region in `StatusBar` next to the sync
  badge (in-out property fed at mount like `sync-model`), drained per
  tick beside `drain_delivery_health`. Hidden when the receiver is
  `None`. **Existing golden visuals stay byte-stable**: no test or
  golden boots with `[files_sync]` enabled, and the chip is absent
  then — pinned by the visibility test on the `None` arm.
- **Loudness**: the actor's `tracing::error!` (chunk 2) is the loud
  half; the chip renders the fork state distinctively (alarm styling)
  and the CLI carries the full detail. No banners, no dialogs, no new
  view — the non-goal fence holds.

### D-FH6 — The runbook: `docs/files-sync-runbook.org`

**Decision**: one runbook document at the docs root (the
`kiosk_bringup_runbook.org` naming precedent), two procedures in one
operational topic. Every step cites the code symbol it exercises
(the stage-2 hard rule); the procedures must match the code's actual
semantics — a runbook suggesting "delete the other device's folder"
violates readers-never-delete and is a review finding (trap (c)).

**§1 — The fork alarm (device rename / re-identity)**

- *Symptom*: the chip's fork state / the `taskboard status` fork line;
  the `ForkDetected` `error!` log record. What the app is doing: writes
  to the affected own paths are stopped; reads continue
  (`cycle.rs::disambiguate_own_412` → `LoopState.forked_paths`; the
  forked sibling path keeps participating in PROPFIND/GET/fold).
- *Cause*: another writer holds this device's single-writer path — a
  cloned disk/VM image, a restored snapshot, a copied data dir
  (duplicated `device.json`), a copied folder subtree (D-SF4; ADR 0020
  Context). The 412-on-own-path is the only signal the medium can
  give (characterization F4).
- *Recovery* (matches the code's stop-write semantics):
  1. Stop **both** instances (the original and the clone) — quiet the
     fleet first.
  2. Choose which instance keeps the identity (normally the original).
  3. On the instance to re-identify: quit the app, delete its local
     `device.json` (`<dirs::data_dir()>/taskboard/device.json`, i.e.
     `~/.local/share/taskboard/device.json` on Linux), restart — the
     ceremony (`device.rs::load_or_create`) mints a fresh `device_id`,
     the attach ladder creates a new `devices/<new-id>/` subtree
     create-once, and the alarm is gone for that device (fresh paths,
     no contest).
  4. The orphaned old subtree: remove it **by hand in the Files UI, if
     desired** — a user action, never the app's (the verb set is
     closed: no DELETE/MOVE in `FilesTransport`); mistakes are
     trashbin-recoverable (§2). When the subtree vanishes, its rows
     leave the merged view (the tree is the truth — `steady_cycle`'s
     vanish arm).
  5. Verify on each instance: `taskboard status` shows no fork line;
     heartbeats advance.
- *What NOT to do*: never regenerate `device.json` while another live
  writer shares it without stopping both; never delete the *other*
  instance's subtree to "fix" the alarm — the alarm says the path is
  contested, not whose content is right.

**§2 — Trashbin recovery (characterization F8)**

- *Symptom*: a device's rows vanish from all surfaces after someone
  deleted `taskboard/` (or a subtree) in the Files UI; own files
  recreate via the create-once ceremony (`OwnFileVanished` finding;
  matrix tests 10/12's semantics); peers' rows leave the merged view.
- *Recovery*: restore the deleted files from the Nextcloud trashbin
  (Files UI → Deleted files; F8's evidence: deleted collections are
  recoverable). No app action required.
- *After restore*: etags changed ⇒ the next PROPFIND marks everything
  dirty ⇒ re-read + tolerant decode + idempotent fold — data returns
  on the next cycle; the own etag store re-learns tokens via the
  adopting GET before any guarded PUT (the cold-store rule, D-FT4) —
  no stale-token 412 storm.
- *Prevention note*: the app cannot cause this class — readers never
  delete (the transport verb set is closed by construction).

### D-FH7 — The docs amendments

- **`docs/architecture.org`**: "Crate Roles & Dependencies" gains
  entries 7. `taskboard-sync-files` (the pure core: layout, codecs,
  fold + laws, state hash, heartbeat — no IO/async/UI) and 8.
  `taskboard-sync-files-nextcloud` (the actor + WebDAV client + config
  + ceremonies + the status watch); one posture paragraph (server-
  enforced path ownership, the fork alarm's stop-write/keep-read,
  readers-never-delete, mode-agnostic spawn D-SF18, the watch-based
  status surface). **Not** amended here: engine ingestion sections —
  chunk 3 owns those when it lands (the re-sequencing's other face).
- **`README.org`**: Configuration gains a `[files_sync]` subsection
  (the `enabled`/`poll_secs` keys, the one-account rule — shares the
  Deck connector's server/credential, Q4 — and where status shows:
  the chip + `taskboard status`); Repository Structure lists the two
  crates. Minimal; no new top-level sections.
- **`docs/testing_strategy.org` §8**: a short "the files transport"
  subsection — the tier mapping for the files crates (hermetic
  fake-transport matrix + wiremock client tests + the docker
  `it_nextcloud_docker_files_*` tier + the live
  `it_nextcloud_live_files_*` tier under the same `.env` convention),
  closing the §8 reference the landed docker test header already
  makes. Recorded reason for exceeding the acceptance list's literal
  docs ("architecture.org/README amended"): the cross-reference exists
  on `main` and this chunk is the docs chunk; the addition is one
  paragraph-scale subsection.
- **`CHANGELOG.md`**: Unreleased entry (the status surfacing, the
  runbook, the live pins) — stage-2 bookkeeping per the house rule.

### D-FH8 — The live Tier-3 pins

**Decision**:
`crates/taskboard-sync-files-nextcloud/tests/it_live.rs` — the docker
file's shape re-pointed at the hosted tier. The mandate is
characterization §3's caveat: the instance is provider-managed, so
server behaviour can change under the app at upgrade time; the
docker tier pins semantics against the controlled image, **the live
tier re-pins them against reality**.

- `tests/common/mod.rs` gains `live_config()` mirroring the Deck
  crate's helper (`dotenvy::dotenv()` + `TASKBOARD_IT_NEXTCLOUD_URL/
  USER/TOKEN` prefix read; `None` when any is unset — **skip, never
  fail**). `dotenvy` is already a workspace dev-pinned dependency (the
  Deck crate's tests use it) — add the dev-dependency entry referencing
  the workspace pin; **no new external dependency**.
- The pins (test names `it_nextcloud_live_files_*`, each
  `#[tokio::test] #[ignore = "requires the live tier (.env
  TASKBOARD_IT_NEXTCLOUD_*)"]`, body guarded by
  `let Some(cfg) = live_config() else { return };`, wrapped in
  `with_deadline`, unique per-run paths, tier scaffolding retries
  server-500s):
  1. **F3 create-once**: first PUT with `If-None-Match: *` creates;
     second answers typed `PreconditionFailed`; the winner's bytes are
     served.
  2. **F4 lost-update guard**: the served etag echoed **verbatim**
     updates; a stale etag answers 412; the **double-quote munge**
     (the characterization's client bug class) answers 412.
  3. **F5 etag churn**: identical re-PUT churns the etag; GETs in
     between are stable.
  4. **F6 tree listing**: the Depth-1 `devices/` PROPFIND lists the
     run's directory with an etag; a child write churns the collection
     etag.
  5. **F8 trashbin presence** (new, evidences the runbook's recovery
     path against the real tier): PUT a per-run file, DELETE it via
     **test scaffolding** (a raw `reqwest` call in the test only — the
     transport's verb set stays closed; DELETE in tier scaffolding is
     the established posture), then PROPFIND the trashbin and assert
     the deleted name is listed.
- **No quota/throttling assertions** (T8: measure, don't assume; a
  ceiling would violate the no-quality-ceilings rule). Stage 2 records
  any throttling observation in its report; drift goes into §8's
  known-server matrix (the established Deck pattern).
- Invocation, in the file header like the docker file's:
  `cargo nextest run -p taskboard-sync-files-nextcloud --run-ignored
  only -E 'test(it_nextcloud_live_files)'`.
- **Skip-never-fail is structural, not tested**: the
  `let Some(cfg) = … else { return }` shape *is* the pin (reviewed,
  like the docker tier's). No test tests the test helper (AGENTS §5:
  don't test test-helpers).

### D-FH9 — Manifest & licensing facts

- **No new external dependencies** anywhere: `dirs`/`tokio`/`reqwest`
  already in the app/transport crates; `dotenvy` enters
  `taskboard-sync-files-nextcloud`'s **dev-dependencies** from the
  existing workspace pin. `cargo deny` untouched.
- **SPDX headers** on every new file: `it_live.rs` and any new
  transport-crate file → `MIT OR Apache-2.0`; app/UI files follow
  their crate's tier (`AGPL-3.0-only`). The transport crate is
  lib-tier: `#![forbid(unsafe_code)]` already at crate root — new
  modules inherit it.
- **`taskboard-domain` is not touched by this chunk** — no snapshot
  churn, no storage codec change, grep-pinned.

## 4. Decision summary

| ID | Question | Decision |
|---|---|---|
| D-FH1 | Chunk-3 precondition & bookkeeping | Promotion per dispatch §1 invoked by the user; PR body `Closes #632` only — #626's closure moves to chunk 3's PR (recorded deviation) |
| D-FH2 | The findings set | Two fields + `PeerFreshness` on `FilesSyncStatus`; freshness is an age display, never an alarm; hash/app_version never surface; the stale-peers taxonomy stays open |
| D-FH3 | Quiescence window | Pure cycle-counted rule (`advance_divergence`): quiet = no publish ∧ no pending heartbeat; fires at streak ≥ 2 of quiet-and-disagreeing; busy/quiet-agreement resets, Failed holds; parked/heartbeat-less/self excluded |
| D-FH4 | CLI projection | `FilesSyncStatusView` serde mirror; `StatusView.files_sync: Option<…>` additive (default + skip); present in `server`/`in_session` iff the actor spawned; `no_daemon` byte pin intact |
| D-FH5 | UI surface | Status-bar chip beside the sync badge; app-side mapping fn (Forked > Diverged > Parked > CycleFailed > Ok); pump task; `mount` gains an `Option` param; no ages on the chip |
| D-FH6 | Runbook | `docs/files-sync-runbook.org`: fork/re-identity + trashbin recovery; every step cites its code symbol; never delete another device's subtree |
| D-FH7 | Docs | architecture.org crates 7–8 + posture; README `[files_sync]` + repo structure; §8 files-transport subsection (recorded reason); CHANGELOG |
| D-FH8 | Live pins | `it_live.rs`: F3, F4 (incl. the double-quote munge), F5, F6, F8 trashbin presence; skip-never-fail structural; no quota assertions |
| D-FH9 | Manifests | No new external deps; `dotenvy` dev-dep from the workspace pin; per-crate SPDX tiers |
| D-FH10 | Status-line shape | Lead line + indented finding lines after the connector/board lines; ages at render; data-survival tests only |

## 5. Test plan (TDD-first — the failing tests come before each commit's code)

### Commit 1 — `feat(sync-files): the peer freshness and divergence findings`

Failing tests first, in `status.rs`/`cycle.rs`:

- **The rule table** (pure `advance_divergence`, no actor): quiet +
  disagreeing ×2 fires with exactly the disagreeing set; quiet +
  agreement resets both; busy resets both; Failed holds both; a parked
  peer (∉ `docs`) is excluded; a heartbeat-less peer is excluded;
  self is excluded; a *third* quiet disagreeing cycle keeps it active
  with the *current* set (membership follows the latest observation).
- **The interleaving property** (proptest): over random
  cycle-classification sequences, the finding never activates without
  two *consecutive* quiet disagreeing cycles (the window is local —
  no `quiet_streak` leakage across an intervening busy or agreeing
  cycle).
- **Matrix 16** — a peer heartbeat lands in the watch: seed the peer's
  `info.json` (fixture bytes with known `written_at`/`last_clock`);
  `status.peers` carries the mapped `PeerFreshness`.
- **Matrix 17** — divergence never fires while writes pend: force a
  local-side change every cycle (publish every cycle ⇒ busy) while the
  peer's heartbeat disagrees ⇒ `diverged_from` stays empty.
- **Matrix 18** — the window fires: after attach (converged), rewrite
  the peer's `info.json` with a disagreeing hash; two advanced quiet
  cycles later `diverged_from` contains the peer; after the *first*
  quiet cycle it does not (the window is two).
- **Matrix 19** — agreement clears: restore a matching heartbeat; the
  next quiet cycle empties `diverged_from`.
- **Matrix 20** — parked exclusion: garbage peer `appdata.json` +
  healthy disagreeing heartbeat ⇒ `parked` carries the quarantine,
  `diverged_from` stays empty.
- **Matrix 21** — vanish: remove the peer subtree ⇒ `peers` and
  `diverged_from` drop it on the next Ok cycle.

Fixture style: the existing `FakeFilesTransport` harness
(`seed_file(info_path, bytes)`; a disagreeing heartbeat is a
`DeviceInfo` JSON with a different `state_hash` string — the type
decodes any tagged string). Time: `start_paused` + `advance(POLL)`
exactly as the landed matrix does.

### Commit 2 — `feat(app): the status view carries the files-sync findings`

- **The mirror**: `FilesSyncStatusView` conversion correctness (every
  field, sorted orders, `ParkKind` → kind strings, `CycleOutcome` →
  view enum) over a constructed `FilesSyncStatus`.
- **Serde losslessness** (the sidecar precedent's shape): the view
  round-trips through JSON with the typed facts intact; an old-shape
  answer (no `files_sync` key) decodes with `None` (both-ways
  additivity); the no-daemon answer stays exactly `{"daemon": false}`.
- **Presence rule**: `StatusView::server`/`in_session` carry
  `files_sync` iff the receiver exists. Hermetic boots via the
  bootstrap test harness (`files_sync_enabled_config` +
  `bootstrap_with_modules` + `AcceptingStore` ⇒ `Some`; the default
  config ⇒ `None` and the field absent from the JSON).
- **Data-survival render**: `render_text` keeps the device uuids,
  forked paths, parked paths, divergence set, and the counts in the
  text; the daemon-line pins pass unmodified (the files block appends
  after the connector/board lines).

### Commit 3 — `feat(app): the files-sync chip pump` + `feat(ui-slint): the files-sync chip`

- **The mapping ladder** (app-side pure fn): forked ⇒ `Forked` even
  when diverged/parked/failed coexist; each rung of the precedence
  ladder; all-clear ⇒ `Ok`; empty/default status ⇒ `Ok`.
- **The pump journey**: a `watch` of `FilesSyncStatus` → send three
  statuses → the pump's UI-typed watch carries the three mapped chips
  in order; dropping the transport sender ends the pump (join
  resolves). Plain tokio test, no time.
- **Mount seam**: the `Option` parameter propagated (compile-verified);
  `None` ⇒ the chip property reports hidden (the golden-stability
  pin); `Some` ⇒ the drained state reaches the window property
  (the `drain_delivery_health` test pattern, lib.rs ~1100).
- **Rendering**: the chip region reads the model state (structural
  facts via the testing backend — no text pins).

### Commit 4 — `docs: the files-sync runbook`

No test. Acceptance is review: steps cite existing symbols
(`disambiguate_own_412`, `forked_paths`, `load_or_create`,
`OwnFileVanished`, the closed `FilesTransport` verb set) and the
rename procedure matches the stop-write semantics (trap (c)).

### Commit 5 — `test(sync-files): the live-tier pins of F3–F6 and F8`

`it_live.rs`'s five `#[ignore]` pins per D-FH8 + `live_config()` in
`tests/common/mod.rs`. Hermetic suite unchanged (the pins never run
without the env; `cargo nextest run --workspace` stays green and
network-free).

### Commit 6 — `docs: architecture, readme, and testing-strategy amendments` + `docs(changelog): the substrate hardening and surfacing` + gates green

Full AGENTS §10 suite through `scripts/run-quality-gates.sh`:
fmt, clippy `-D warnings`, nextest, doc-tests, deny, the
CI-identical coverage trio. Every commit standalone (rebase-merge
hygiene).

### Review pins (stage 3's checklist, from the dispatch)

- **Freshness ≠ divergence**: no clock input reaches the derivation —
  grep the divergence code path for `now()`/`Instant`/`Duration`; a
  merely-slow peer can only ever surface as an age.
- **No invented taxonomy**: the diff adds exactly `PeerFreshness` +
  the two fields in the transport crate and the serde mirror in the
  app crate; the domain crate is untouched (`git diff main...HEAD --
  crates/taskboard-domain` is empty).
- **Chunk-3 fences hold**: no `UserDataReport`, no engine channel, no
  `AppState`/`SyncStates` change, no `SyncReport` change (grep pins).
- **The runbooks match the code**: every cited symbol exists on the
  branch; the rename procedure stops-both-then-reidentifies (never
  deletes the peer's subtree).
- **Live pins**: assert F3–F6/F8 *semantics* (typed outcomes), skip
  cleanly without the env, and never assert quota/ceilings.
- **Docs match shipped names**: crate names, config keys, test-name
  filters, paths.
- **Byte pins**: `{"daemon": false}` unchanged; existing daemon-line
  and golden-visual outputs unchanged with the chip absent.

## 6. Risks

- **The mount-signature change touches every UI test helper** —
  mechanical, compiler-driven; do it in the chip commit, not piecemeal.
- **Hermetic boots with `[files_sync]` enabled dial the configured
  URL** — the existing bootstrap test's pattern (a dead `example`
  URL: the first cycle fails typed and backs off; the test asserts
  only watch presence) is the sanctioned harness; the projection unit
  tests construct `watch::channel(FilesSyncStatus { .. })` directly
  and never boot at all.
- **Convergence dynamics change when chunk 3 lands** (devices publish
  imported rows): the derivation needs no change — it compares
  published-view hashes and the fold's determinism guarantees
  convergence — but chunk 3's stage 3 must re-verify the exclusions
  against import journeys (recorded here for that agent).
- **Hosted-tier drift** between the characterization and a pin run:
  the pins assert typed outcomes, so drift surfaces as a failing live
  pin to be recorded in §8's known-server matrix — the established
  Deck pattern, never a hermetic-suite break.
- **Chip-lifecycle leaks**: the pump exits on sender drop (shutdown);
  no join bookkeeping beyond the existing runtimes' posture. A wedged
  pump cannot block shutdown (it holds no lock and no join waits on
  it beyond the bounded-shutdown ladder's existing arms).
- **The promotion's honest edge**: this chunk ships divergence
  detection while the local contribution is still `EmptyDocument`
  (chunk 2's production local view). The derivation is exactly as
  meaningful there (peers' published views vs our folded view) and
  needs no rework when chunk 3 feeds real local sections — the seam
  (`LocalDocument`) already exists.

## 7. Estimated diff size

**S** (the dispatch's sizing, confirmed): ≈ 1,300–1,600 lines across
~12 files — transport ~120 + ~420 test lines; app ~260 + ~210 test
lines; ui-slint ~70 + ~90 test lines; `it_live.rs` ~260 + common ~30;
docs ~230 lines (runbook ~120, architecture/README/§8 ~90,
CHANGELOG ~10).

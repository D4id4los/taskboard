# [Report] Per-User App-Data Roaming — Options Analysis (decision input for the v0.2 journal)

- **Date**: 2026-11-04
- **Context**: the v0.2 "focus & reflection" milestone's reflection half
  (#621 journal + review, #623 statistics) needs a home for **per-user data**
  (journal entries first; settings, reminder proofs per D5 later). Every
  roaming lane the app has today is **board-scoped** — the appdata sidecar
  is a JSON attachment on a board's anchor card — and the user has ruled
  this tension real: a personal assistant's data (a journal spans boards,
  belongs to the person, not to any board) does not fit a per-board
  substrate, especially with shared/team boards in the picture (Wave 4).
  This report inventories the options (including the user's two
  proposals), analyses viability/effort/future-fit, argues each option's
  case **against** itself (advocatus diaboli), and ends with a
  recommendation. The user makes the final ruling.
- **Method**: options derived from the user's 2026-11-04 brief plus the
  repo's own prior analyses; every claim about landed behaviour verified
  against `main` @ `323a9bf9` and the cited artefacts. Effort uses the
  house S/M/L/XL scale (future-directions §2). Transport claims about the
  Nextcloud file API are flagged "verify at design time" where not
  covered by an in-repo report.

---

## 1. The problem, precisely

**What roams today, and how.** The Deck connector syncs board content
(cards/stacks/labels) natively. Everything else the app has built roams
via the **appdata sidecar** (DR-1): a per-board JSON attachment
(`taskboard-appdata.json`) on a `"[taskboard] app-data"` anchor card,
with sections — `tasks` (app-only task fields), `reminder-rules`,
`dismissals`, `time-entries` (board-level), break rules (#588), habit
facts (DR-6) — merged by embedded-clock per-key LWW union, flushed once
per sync cycle between pull and report (one PUT per board per cycle).

**Why the journal breaks the model.** Three separate mismatches:

1. **Ownership**: a journal is per-*user*; the sidecar is per-*board*.
   Riding any content board bakes "the user's memory of their day" into
   a container that can be shared, unshared, deleted, or drained
   (dead-board drain cascades exist and are correct for board data).
2. **Span**: a day's reflection touches every board the user worked
   across; a per-board section would fragment the journal across N
   attachments with no cross-board assembly story.
3. **Growth shape**: task-field sections are small and
   retention-windowed (time entries: 180d wire window); a journal is
   ever-growing free text with *no* acceptable wire-expiry — the data
   the user most wants preserved indefinitely is the worst fit for a
   re-uploaded-whole attachment.

**The root cause in one sentence**: the Deck API surface has **no
per-user storage** — no user-scoped files, no app-data API worth the
name (the OCS preferences API is a small key-value store, unsuitable) —
so everything personal the app has synced so far had to squat on a
board. The Nextcloud **Files** API, by contrast, *is* a per-user home
(the user's file tree). That asymmetry, not any mistake in the sidecar
design, is what the user is noticing.

**What is already designed and waiting.** Two 2026-10-11 reports did
the heavy lifting for the alternative:

- The **generic file-sync report** (`.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`)
  designed a transport-agnostic protocol backbone: per-device
  **immutable op-chunks + snapshots** (its §5.5, DecSync-style) over
  per-device path-owned directories, folded by the domain's own
  per-field LWW **extended with author tags** (its F1 repair), with
  tombstones-forever, heartbeat divergence detection, and a four-phase
  build plan. Its transport model assumed an *external sync client*
  (10–20 min latency, arbitrary conflict behaviour).
- The **org-mode viability report** (`.artifacts/reports/2026-10-11-org-mode-support-viability-report.md`)
  priced two-way org-file sync at 4–7 weeks with a known-correct
  architecture (issue #184 open).

The user's proposal (a generic file-based sync core + a connector that
drives the Nextcloud file API directly, folder-sync-client connector as
a later thin transport) is exactly the §5.5 backbone with a **better
transport** than the report assumed. Section 4.B analyses what changes.

## 2. Decision criteria

| # | Criterion | Why it matters here |
|---|---|---|
| K1 | Per-user ownership (not per-board) | The stated problem; team/shared boards make it existential |
| K2 | Correctness under multi-device offline use | The app's standing merge posture (LWW convergence, tombstones, no silent loss beyond documented duels) |
| K3 | Effort to ship **in v0.2's lifetime** | The journal is this milestone's anchor; a substrate that slips past v0.2 decides this question by default |
| K4 | Data growth shape | Journal: unbounded, never-expiring free text |
| K5 | Responsiveness (incl. future mobile) | Review flows read the journal at day boundaries; Wave-5 Android makes push-vs-poll real |
| K6 | Fit with planned waves | D5 (settings/proofs), #184 (org connector), Wave-4 multi-user/team decks, Wave-5 LAN sync + Android |
| K7 | Privacy blast radius | A journal is the most personal data the app will ever hold; shared boards are the wrong drawer |
| K8 | Reversibility / migration cost | Whatever ships first must not paint the final substrate into a corner |

## 3. Options inventory

- **C — Device-local v1** (the D5 default posture; the break-rules
  precedent: device-local first, roaming added later by #588).
- **A1 — Ride one designated content board's sidecar** (the user's
  first option, literally read: pick one existing board connection and
  let its sidecar carry the journal section).
- **A2 — Dedicated app-data "board"** (a refinement of A1: the app
  creates/adopts a hidden-purpose board that exists only to carry the
  per-user sidecar; still the Deck connector, still an attachment).
- **A3 — Journal entries as Deck cards** on an app-data board (the
  other Deck-native reading of A: one card per entry, visible/editable
  in the Deck web UI). Named for completeness.
- **B — Nextcloud-files app-data substrate** (the user's second
  option): a **generic file-sync core** (path-owned per-device files,
  clock-driven fold — the §5.5/§5.4 backbone) + a transport that talks
  the Nextcloud file API (WebDAV) directly; folder-sync-client transport
  later as a sibling. Two scopes:
  - **B-min**: per-user app data only (journal now; settings/proofs
    later). Boards stay on Deck. **This is the slice v0.2 needs.**
  - **B-full**: also sync *board content* through files, with either
    partition (a board is synced by exactly one connector — Deck or
    files) or **coexistence** (both connectors on the same board).
- **F — Journal as an org file** (the org-report convergence: the
  journal is a real `.org` document in the user's tree, synced by the
  org connector; user-visible, Emacs/org-tooling-readable). Distinct
  from B only in *format and audience*: same transport problem.
- **G — CRDT substrate** (`yrs`/`automerge` as the merge engine over
  whatever transport). The file-sync report's documented escape hatch.

Dismissed sidebars (one line each, argued in §6): a sync-server app /
LAN sync now (Wave 5 by roadmap; needs this same core first); E2EE
packaging (orthogonal, later phase per the file-sync report §7.9);
Nextcloud OCS preferences API as the home (size- and semantics-unfit).

## 4. Per-option analysis

### 4.C — Device-local v1 (status quo posture)

**Mechanism.** The journal lands as a domain entity + sqlite table +
`journal` CLI verbs + review surfaces, reading and writing **local
state only**. The entity/wire shape is designed exactly as if it roamed
(embedded clocks per entry, BTreeMap home, tolerant serde rows — the
sidecar recipe's semantics) so a substrate later is a transport bolt-on,
not a migration.

**Viability.** Proven twice: break rules shipped device-local and
roamed later (#588) without entity surgery; adhoc alarms are
permanently device-local by design. Zero new subsystems, zero new
failure domains.

**Effort.** **Zero marginal** beyond #621 itself.

**Future fit.** D5's recorded posture ("until then, keep such data
device-local and merge-friendly") — this *is* the sanctioned interim.
K3 perfect, K1/K5 unmet (data doesn't follow the user), K8 good
(add-only union imports cleanly; see the devil's advocate for the edit
case).

### 4.A1 — Ride a content board's sidecar

**Mechanism.** Config picks one bound board as the app-data carrier; its
sidecar gains a board-level `journal` section (the `time-entries`
recipe, sixth application).

**Viability.** Technically trivial — the proven recipe, ~S effort. And
that is the whole good news.

**Future fit.** K1 unmet (the journal lives *in* a board), K7 badly
unmet: on a shared/team board the journal is a readable attachment for
every board member; even on a personal board it is one unshare-away
from exposure. K4 unmet (whole-attachment PUT per flush, re-downloaded
by every device per change; the journal is the fastest-growing section
and the only one with no acceptable expiry). K8 poor: every day on this
lane deepens the wrong home. Dead-board drain, board deletion, or
unbinding the carrier board all orphan the journal.

### 4.A2 — Dedicated app-data board

**Mechanism.** The app provisions a board (e.g. `[taskboard] journal`)
via the normal create/bind ceremonies; its sidecar carries the journal;
the deck connector syncs it like any board; the UI hides/fences it from
normal board surfaces.

**Viability.** All machinery exists (board CRUD, sidecar, connector
registry). The work is the fencing and ceremony: board-bar/settings
exclusion, kiosk exclusion, dead-board-drain exemption, "user deleted
the app-data board in the Deck web UI" recovery (a real support case —
the board is indistinguishable from a user board server-side). Effort
**M** mostly in edge ceremony, not sync.

**Future fit.** K1 *formally* unmet — it models the user as a board,
the exact conflation the user is recoiling from; it works only while
the Nextcloud account is exclusively personal (a family/team account
re-introduces K7). K5 unchanged (Deck poll cadence). K4 unchanged
(attachment growth). K8: unwinding later = a board funeral + attachment
exhumation. It is, however, the only Deck-native option that keeps
per-user data out of *content* boards — it is the best of the A family,
and the family's ceiling.

### 4.A3 — Journal entries as Deck cards

**Mechanism.** An app-data board whose *cards* are journal entries
(title=day, description=text); health flags as labels or description
markup.

**Viability.** The mapping is poor: description is whole-field LWW free
text (concurrent journal edits on two devices silently lose one side —
the sidecar's per-entry clocks are *finer* than cards are); structured
attributes (mood/energy/flags) have no honest card fields; templates
have no home. The one genuine attraction — the journal becomes
readable/editable in the Deck web UI and any Deck mobile client — buys
read-visibility at the price of schema poverty and the worst write
granularity. **Dismissed** on granularity; noted because "journal
visible in Nextcloud" is a real product pull (see F, which gets it
honestly).

### 4.B — Nextcloud-files app-data substrate (the user's proposal)

**Mechanism.** Two layers, per the user's instinct and the 2026-10-11
report's architecture:

1. **A generic file-sync core** (new crate, pure): per-device
   path-owned files under a `/taskboard/` tree in *the user's file
   home* (`/taskboard/devices/<device-id>/appdata.json` as the v1
   shape — the report's §5.4 per-device current-state design; the
   §5.5 chunk/snapshot lifecycle is the upgrade path when audit/GC/
   incremental-upload needs become real). The fold is **the existing
   appdata union semantics** (embedded per-entry clocks, tombstones,
   tolerant serde) applied across devices' files instead of across pull
   overlays — the merge engine already exists in `appdata.rs`; the core
   generalises its inputs.
2. **A Nextcloud-files transport** (connector crate): WebDAV over the
   *same* reqwest stack and account credentials the Deck connector
   uses (verify at design time: app-password scopes cover WebDAV;
   chunked-upload endpoint for >upload-limit files; ETag stability
   rules). Auth, config, keyring, and the connector-registry slot are
   all existing machinery.

**What changes vs the report's transport model — the analysis the user
asked for** (the report assumed an external sync client; driving the
API directly *improves* the hazard table):

| Report hazard | Via external sync client | Via direct WebDAV |
|---|---|---|
| T1 delivery delay | 10–20 min typical, unbounded | Our poll interval (seconds); optional near-real-time via the server's notify-push app (verify at design time) |
| T4 same-path concurrent writes | Silent-loss or conflict-copy, client-dependent, **unknowable** | **Server-enforced**: conditional PUT (`If-None-Match`/`If-Match`) makes per-device path ownership a server-side invariant, not a hope |
| T3 torn files | Possible (interrupted materialisation) | A GET returns whole-or-error |
| T8 throttling/quota | Client-mediated, opaque | Server quota + rate limits, observable and handleable (`Retry-After` machinery exists) |
| T9 semi-hostile medium | Sync-client quirks, selective-sync misconfig | The folder is **not** app-private in practice — the user *can* open/edit/delete it in the Files UI, a mobile app, or another WebDAV mount; hand-edit/quarantine tolerance (report R8) stays mandatory; server-side trashbin retention is a genuine new *recovery* ally |

The protocol conclusions (path ownership, clock-driven fold, tombstone
discipline, heartbeat freshness) carry over **unchanged or strengthened**
— the design was built to tolerate a worse medium than the one the user
is proposing.

**Effort.** **B-min ≈ M (3–6 weeks)**: core crate with the fold + tolerant
codecs over the existing section vocabulary (~1.5–2 wk), WebDAV transport
with conditional PUTs/PROPFIND listing/heartbeat (~1–1.5 wk), connector
wiring + config + ceremonies + tests riding the existing docker
Nextcloud tier (~1–2 wk). The report's F1 (author-tagged clocks) is
**not** required for B-min v1 if journal entries use the per-entry-clock
union the sidecar already has (ties broken by entry id, the `RuleEntry`
semantics); it becomes required when *task fields* ever ride this
substrate (B-full) — keep it fenced, not built speculatively.
Whole-file re-PUT per flush is fine at journal scale (§5 numbers below).
**B-full ≈ L–XL**: either native task-state chunks (the full §5.5 +
F1 author clocks) or the org mapping (#184, 4–7 wk) — plus the routing/
coexistence questions below. Not v0.2 material.

**The coexistence sub-question (both connectors on one board) — asked,
answered.** Partition (a board synced by exactly one connector) fits
the landed registry (outbox ops are board-attributed; one owner per
board). Coexistence means **two clock authorities for the same task
fields with no shared causality** — Deck's `last_modified` vs the file
fold's clocks — plus outbox fan-out to both connectors, plus echo
suppression (A→Deck→B's files→B's pull→B's report→engine→outbox→…)
for every change. That is a research-grade merge problem wrapped
inside a routing redesign. **Verdict: coexistence is not viable at
present architecture; partition is the only sane B-full shape; and
B-min doesn't need either** (boards stay on Deck, untouched).

**Future fit.** The strongest of any option: K1 by construction (the
user's file home *is* per-user storage); K4 (per-device files, no
whole-fleet re-upload; chunking available later); K5 (poll we control;
push path exists); K6 — this **is** D5's substrate decision being made
rather than deferred: settings sync, reminder-proof roaming, and the
folder-sync-client connector become thin transports over the same core
(the user's stated end-state), and Wave-5 LAN sync builds on the same
fold; #494 per-user attribution gets a real per-user identity seam for
free. K8: importing device-local journals (option C's interim) into the
per-user tree is a trivial fold — C and B compose as stages.

### 4.F — Journal as an org file

**Mechanism.** The journal document is a real `journal.org` in the
user's tree (org-format entries: datetrees are idiomatic org-journal
territory), synced by the org connector (#184's machinery: 4–7 wk
priced, architecture validated).

**Analysis.** Product-beautiful for *this* user (org-native life,
Emacs/org-tooling interop, human-owned plain text forever) and it makes
the journal the one dataset that outlives the app with zero export.
But: it fronts 4–7 weeks of org write-back machinery (idempotent
writer, surgical edits) **before** the first journal entry can roam;
org files carry no per-field clocks, so concurrent text edits degrade
to whole-entry LWW with file-read-time clocks (the org report's own
documented limitation) — for long free text that is the coarsest merge
in the whole toolbox; structured attributes (health flags, ratings)
must live in property drawers (workable, clunkier than a typed
section); and the health-flag/statistics consumption (#623) reads them
back through the org mapping. **Best treated as a future *projection*
of the journal onto org via the org connector — the journal's canonical
form stays typed, an org view renders from it — rather than as the
storage substrate.** (That projection direction also dodges the
writer-idempotency burden: rendering org is the org report's cheaper
feature-A side.)

### 4.G — CRDT substrate

The file-sync report's §5.6 verdict stands and is strengthened by three
weeks of landed LWW machinery since: `yrs`/`automerge` would demote
`AppState` to a projection of an opaque document, orphan `TaskClocks`,
and collide with ADR 0006 — to solve a problem (character-level
concurrent text merge) the journal doesn't have (entries are
single-author-per-entry, append-dominated). Escape hatch only.

## 5. Comparison matrix

✓ met, ~ partial, ✗ unmet. "v0.2" = can realistically ship inside the
milestone.

| Criterion | C local | A1 ride board | A2 app-board | A3 cards | B-min files | F org | G CRDT |
|---|---|---|---|---|---|---|---|
| K1 per-user ownership | ✗ (no roaming) | ✗ | ~ (a board impersonating a user) | ✗ | ✓ | ✓ | ✓ |
| K2 multi-device correctness | n/a | ✓ (recipe) | ✓ | ~ (card-field LWW) | ✓ (recipe, server-enforced) | ~ (coarse clocks) | ✓ |
| K3 v0.2 shippable | ✓ | ✓ | ~ | ~ | ~ (M, parallel-startable) | ✗ | ✗ |
| K4 growth shape | ✓ (local) | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ |
| K5 responsiveness/push path | ✗ | ✗ (Deck poll) | ✗ | ✗ | ✓ | ~ | ~ |
| K6 future waves (D5/#184/LAN/Android/#494) | neutral | ✗ deepens the wrong home | ✗ | ✗ | ✓✓ (is D5) | ✓ (is #184) | ✗ (replaces the engine) |
| K7 privacy blast radius | ✓ | ✗✗ (shared boards) | ~ | ✗ | ✓ (private folder) | ✓ | ✓ |
| K8 reversibility | ✓ | ✗ | ~ (board funeral) | ✗ | ✓ | ✓ | ✗ |

## 6. Advocatus diaboli

The case against each option, argued in good faith:

**Against C (device-local).** You are building a *personal assistant*
whose flagship reflection feature produces the one dataset the user
most wants everywhere — the evening review on the desktop, the morning
read on the phone — and v0.2 ships it chained to one machine. Every day
of two-device journaling under C produces *diverged* journals; the
append-only case unions cleanly later, but the ruled-on editability
(#621's edit/undo question) means edits and deletes diverge visibly and
the eventual import surfaces them as conflicts the user must adjudicate.
C is only painless for a strictly one-device user — which is not the
product direction. Also: "temporary, device-local" has a way of
becoming the format people backup and migrate by hand.

**Against A1 (ride a content board).** You would be storing the user's
private reflections inside a container whose access list is *other
people's* (shared/team boards), whose deletion semantics are board
semantics (dead-board drain is *correct* for board data and would
correctly destroy the journal), and whose sync payload is re-uploaded
whole and re-downloaded by every device on every change — with the
journal as its fastest-growing, never-expiring resident. It is the
cheapest option on the menu and wrong on more criteria than any other.
The only argument for it is "the recipe is proven" — so is digging a
hole with a spoon.

**Against A2 (dedicated app-data board).** You would be papering over
the missing concept (a user) by minting a fake instance of the wrong
concept (a board): a board that must be hidden from the board bar,
excluded from kiosk, exempted from drain, specially recovered when the
user — reasonably, from Deck's side — deletes the weird empty board
they never created on purpose. Every one of those exceptions is a
permanent special case in code and docs, and at the end the journal
still rides Deck poll cadence and whole-attachment re-PUTs, still has
no push path, and still breaks the day the account becomes shared. A2
is a M-sized patch that leaves every structural criterion unmet — the
most expensive way to buy time.

**Against A3 (cards).** Schema poverty: mood/energy/flags/templates
shoehorned into titles, labels and markdown; concurrent-entry edits
resolved at the *coarsest* granularity in the system; the "but it's
visible in Deck!" argument is a siren song — visibility of a private
journal in a shared web UI is a liability dressed as a feature.

**Against B (the files substrate).** The honest case has four prongs.
(1) **Scope**: v0.2 needs one feature; B builds a *subsystem* — a new
core crate, a new connector, a second sync status surface, a second
failure domain (quota, ETag quirks, server-side scanning, the user
deleting `/taskboard/` in the Files UI) — every one of which will
someday send a bug report, maintained by the same one-implementer
pipeline that still has #600/#504 open. (2) **Responsiveness is
oversold**: the 10–20-minute horror in the old report was the *
external sync client*; Deck attachments already sync on *our* poll
interval — B's latency win over the status quo is real but modest at
journal scale, and the push story (notify-push app) is server-config
dependent, not ours. (3) **Nextcloud again**: the stated ambition is a
personal assistant free of board-shaped assumptions, and the response
is to marry the *other* Nextcloud API — the substrate is only as
vendor-neutral as the core/transport split is disciplined; without the
folder-client and LAN transports actually landing, "generic core" is an
architecture claim, not a capability. (4) **The coordination tax**:
once two connectors exist, every future feature must answer "which
lane?" — and the attractive nuisance of B-full (boards over files,
org everywhere) will keep pulling scope until the coexistence
tar-pit, analysed above, eats a quarter. If B-min ships without its
transport siblings following within a wave or two, it is A2 with extra
steps and a folder icon.

**Against F (org journal).** It buys the most poetic storage story by
paying the org connector's full 4–7-week price *before* v0.2's anchor
feature can roam, degrades merge to whole-entry LWW over file-read
clocks, and couples the statistics consumer (#623) to the org mapping.
And the thing it buys — org interop — is buyable later as a
*projection*, which is strictly cheaper than storage-coupling.

**Against G (CRDT).** Replacing a working, tested, landed merge engine
with an opaque binary document to solve a problem the journal does not
have. The file-sync report already wrote this epitaph.

**Against the whole framing (the meta-devil).** One may also ask
whether v0.2 needs a roaming journal *at all* — the reflection
features are complete and shippable device-local (C), and the D5
substrate decision was explicitly parked for Wave 4 with "keep such
data device-local and merge-friendly" as the recorded interim posture.
Deciding D5 now, under milestone pressure, in the same breath as the
journal that needs it, is how architecture gets decided by deadline.
The counter-counter-argument: the decision criteria that matter (K1,
K7, K6) are *already* violated by any A-variant, the analysis is done
now rather than under Wave-4 pressure, and B-min composes with a C
interim — the risk is sequencing, not correctness.

## 7. Recommendation

**B-min, staged over C — with A-rejected and B-full fenced:**

1. **Reject A1 outright** (shared-board privacy, drain semantics,
   attachment growth). **Reject A3** (granularity). Do not build A2 as
   the plan of record — keep it documented only as the fallback if a
   hard roaming requirement lands before B-min does.
2. **Ship #621's journal on the C posture** (device-local, entity/wire
   shaped exactly like a roaming section: embedded per-entry clocks,
   tombstoning deletes, tolerant rows) — this keeps v0.2's anchor
   feature unblocked from the substrate build.
3. **Open the B-min design round now** (issue to be opened on
   acceptance): the generic file-sync core scoped to per-user appdata
   sections (per-device current-state files v1, §5.4 shape, evolving to
   §5.5 chunks when audit/incremental-upload needs are real) + the
   Nextcloud WebDAV transport as its first transport. Journal roaming
   is its first consumer; settings/proofs (D5) and the folder-client
   and LAN transports are its fenced successors. Target: journal
   roaming inside v0.2 if the round starts promptly, else the first
   post-v0.2 chunk — C makes the slip costless.
4. **Fence B-full and coexistence explicitly** (partition-only if
   boards ever ride files; org-journal as a future *projection* over
   the typed journal, not its storage; G stays the escape hatch).

**Why this over pure-C**: C alone leaves the product's stated direction
(per-user, multi-device assistant) unserved by its most personal
dataset and re-asks this question under Wave-4 pressure. **Why over
pure-B-now**: the journal must not wait on a subsystem; the staging
makes each risk independently recoverable. The composition (C interim →
B-min import-fold) is the only path that is both shippable in v0.2 and
architecturally final.

**Scale sanity for B-min's whole-file-PUT v1**: journal ≈ 0.5–2 KB/day
of text → a year ≈ 0.2–0.7 MB per device file; a conditional PUT of
that size per flush is negligible against a Deck card push, and the
§5.5 chunking exists as the measured-need upgrade. Numbers to be
re-measured at design time against real entry shapes.

## 8. Open questions that survive into the B-min design round

1. App-password scopes/behaviour for WebDAV with the existing Deck
   credential (same secret, or a second keyring slot?).
2. ETag semantics across the chunked-upload endpoint; PROPFIND listing
   cost at v1 file counts; quota/trashbin interactions.
3. The notify-push dependency question for near-real-time (K5): poll
   interval first, push as an enhancement?
4. Heartbeat/freshness vocabulary ("peer synced 40 min ago" — the
   report's §10.6 open question, still open).
5. Device identity lifecycle (the `instance_id`/clone-detection
   discipline) — how much of §5.5's hardening v1 carries.
6. Whether the core crate lands as `taskboard-sync-filesync`'s library
   or a standalone `taskboard-sync-core` (naming/structure for the
   sibling transports).

## 9. Addendum (2026-11-04, post-decision): why B-min first even without time pressure

The user ruled the two-track staging (v0.2 journal device-local; the
files substrate as a parallel `nextcloud-files-sync` milestone) and
asked the fair question: the §7 recommendation leaned partly on
"don't block v0.2" — with the pressure gone and both milestones
running in parallel, why still B-min rather than committing to B-full
now? Five arguments survive the loss of the urgency prong, plus one
honest concession:

1. **B-full is two decisions wearing one milestone; B-min is one.**
   B-min decides the substrate (core + transport + per-user data
   home). B-full additionally decides the **board-content format**
   (native per-field chunks vs org files — i.e. re-litigating #184's
   sequencing inside the sync round), the **author-tagged-clocks
   domain rewrite** (F1: every entity's clock struct, every fold
   test), the full **chunk/snapshot/GC lifecycle**, and the
   **partition-vs-coexistence routing policy**. Each is independently
   sizeable; bundling them makes the design round unfocused and the
   milestone XL by construction.
2. **Operate before boarding the crown jewels.** B-min puts the
   substrate into production carrying the most forgiving data class
   the app has (append-mostly journal entries, per-entry clock union —
   the merge that cannot meaningfully corrupt). B-full's first
   passengers would be the user's actual task state, where a fold bug
   is a data-loss event. One wave of operating the substrate (ETag
   quirks, quota, trashbin restores, heartbeat divergence, the
   user-editing-the-folder case) at small blast radius de-risks the
   large one. This is sequencing discipline, not impatience.
3. **B-full is not even clearly the end state.** Deck sync works, is
   battle-tested, and carries native Deck-UI interop. The org
   connector (#184) was always conceived as an *addition* with
   partitioned boards, not a replacement. Choosing B-min-first is
   therefore not "B-full later, so start it now" — it is refusing to
   pre-commit to an optional destination before the mandatory part
   (per-user data has no home today; board sync already has one).
4. **One implementer, shared attention.** "Minimal conflicts" holds
   for code paths (v0.2 touches the journal domain + review UI; the
   substrate touches new crates + sync), but both milestones draw on
   the same design/review capacity. A B-min milestone is shaped so its
   first consumer is a thin slice; a B-full milestone competes with
   v0.2 chunk-for-chunk and loses the parallelism the user just set up.
5. **The devil's-advocate symmetry.** §6's case against B — subsystem
   scope, the attractive nuisance of boards-over-files pulling the
   round into the coexistence tar-pit — applies *more* strongly when
   B-full is the starting commitment. The fence is what keeps the
   substrate milestone honest.

**Concession**: with no time pressure, two B-full arguments gain
weight — front-loading the F1 clock repair (it hardens ADR 0004
regardless) and learning the chunk/snapshot lifecycle on the real
transport early. Both are accommodated *inside* B-min's round as
explicitly fenced, individually dispatchable follow-ons (F1 is
valuable standalone per the file-sync report's own phasing; the
lifecycle enters when audit/incremental-upload needs are measured) —
without making them prerequisites of the journal's roaming.

**Net**: the recommendation is unchanged and is now argued on
sequencing merit alone — B-min is the milestone; B-full's constituent
decisions stay fenced as named follow-ons until the substrate has
operated on real per-user data for a wave.

*Post-addendum ruling (2026-11-04, recorded with the kickoff of DR-8
— issue #626): the user committed to B-full as the **sequenced
successor** once B-min fully lands. The fence changed character — from
"optional destination" to "committed next round" — but the scope
recommendation is unchanged: B-min stays per-user-data-only; B-full's
constituent decisions (board format, F1, lifecycle, partition policy)
move to its own round when it opens. This makes Q1's
transition-smoothness reservation (a 5.5-compatible on-disk layout
from day one) part of B-min's design contract.*

## 10. References

- User brief + rulings session, 2026-11-04 (this conversation; rulings
  recorded on #621–#625).
- `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  (the backbone: T-model, §5.4/§5.5, F1, §7 reference design).
- `.artifacts/reports/2026-10-11-org-mode-support-viability-report.md`
  (F's pricing; the projection-vs-storage distinction).
- `.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  (the sidecar contract B reuses), `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  (board-level section recipe, retention-aging precedent, `export`
  reservation).
- `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (D5, Wave 4–5, effort scales); #588 break-rule roaming (the C→roaming
  migration precedent); #494, #184, #212/#504 (the follow-on consumers).
- Verified against `main` @ `323a9bf9`: `appdata.rs` sections/union,
  connector registry/bootstrap, dead-board drain, `commands/track.rs`
  export lane.

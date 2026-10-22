# [Plan] Deck version capabilities — version-seamed transport + the `scheduled` lane rule (round doc 1 addendum)

- **Date**: 2026-10-22
- **Kind**: design addendum to the app-data sidecar design
  (`.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`) and to
  the field-family design (`2026-10-18-field-family-design-plan.md`
  §3.2 D-F2) / wave plan (DDF-2, DDF-10). Carries the 2026-10-22 user
  ruling that **no sync-functionality degradation is acceptable**:
  where a Deck version lacks a surface, the connector adapts its
  communication so the rest of the app keeps full functionality.
- **Origin**: the stage-2 STOP report
  (`.artifacts/reports/2026-10-22-daily-driver-appdata-transport-stage2-stop-report.md`)
  + the 1.17.5 characterization pass
  (`.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`,
  measured evidence for every decision below).
- **Status**: recommendations **ACCEPTED** (ruled 2026-10-22, user
  session): version-specific communication via a capability seam;
  sync-feature parity between Deck 1.17.5 and 1.19.0; `scheduled`
  rides the native field when the server has it, the sidecar document
  when it does not. DDF-10's stop-and-report protocol remains in
  force for *future* divergences: a new unknown-version divergence is
  still a finding first; the flag table is then extended, never a
  silent adaptation.

---

## 1. D-V1 — the capability seam (how version-specific communication works)

**Decision.** The connector probes the server once per client
lifetime and derives a typed **feature-flag set**; every
version-specific behavior reads a flag, never a raw version
comparison.

- **Probe**: `GET /ocs/v2.php/cloud/capabilities` (OCS JSON) — the
  payload carries `capabilities.deck.version` as a plain semver string
  (verified on both tiers: `"1.17.5"`, `"1.19.0"`). New typed client
  method (sync crate), parsed once in `DeckClient::new`, cached on the
  client. One extra GET per client construction (per connector actor),
  not per cycle.
- **Flag table**: a small pure function `flags_for(version) ->
  DeckFeatures` with one row per known major.minor range and one
  rule: **a version newer than the newest known row behaves like the
  newest known row** (optimistic; the fixture gate is the safety net,
  and a genuinely new divergence re-enters via MD-5 stop-and-report).
  Known rows (2026-10-22):
  - `CARD_STARTDATE: bool` — native card `startdate` write support.
    False for `1.17.x` (create ignores it, PUT silently drops it,
    reads lack the key — all measured), true from `1.19.0` (measured
    by the chunk-1/3 fixtures; the exact introduction point inside
    1.18.x is unverified and deliberately not depended on — the
    range table maps `1.18.x` to false, the conservative arm, until
    measured).
- **Unknown/absent Deck section** (probe fails, or no `deck` key:
  Deck not installed or an ancient build): flags fall back to the
  **conservative arm** (`CARD_STARTDATE = false`), with a typed log.
  This is safe by construction: the conservative arm's wire forms are
  the ones that work on *every* measured version (§3's additive field
  is tolerated everywhere; the sidecar lane works everywhere), so a
  failed probe degrades the *lane choice*, never correctness.
- **Not a flag**: the attachment-create additive `data` field is sent
  **unconditionally** (verified required-by-1.17.5, tolerated-by-1.19.0
  — one request form for all versions; D-V3). Decode-side fallbacks
  (§3) are likewise unconditional.

**Chunk 3 owns**: the probe + flag types + the table + the consumer
wiring (§3/§4). `AnchorCardTitle`/constants are unaffected (no flag
touches the ceremony's conventions).

## 2. D-V2 — the `scheduled` lane rule (native when available, sidecar fallback)

**Decision (the ruling, made precise).** `scheduled`'s sync lane is a
function of the server's capability flag, evaluated per connector:

- **Flag true** (`CARD_STARTDATE`): native lane exactly as DDF-2
  branch A landed — `merge_task`'s `scheduled` verdict runs, card
  writes carry `startdate`, the document's tasks section **ignores**
  any `scheduled` key on fold and the flush **emits none** (and drops
  stale ones — chunk 4's union applies a lane-GC: `scheduled` keys are
  removed from the rebuilt document while the flag is true).
- **Flag false** (1.17.x, unknown, probe failure): sidecar lane — the
  document's tasks section **models `scheduled`** (a fourth modeled
  key; the D-S2/"assertion 3 never-a-scheduled-key" rule is
  **superseded** by this ruling), the flush emits `{v, clock}`
  entries like any app field, and the fold materializes them.
- **Merge-verdict gating (data safety, the sharp edge)**: while the
  flag is false, the native `scheduled` verdict in `merge_task` is
  **skipped entirely** — 1.17.5's remote `startdate` is always
  absent/null (a silent write-drop, §2.2 of the characterization);
  running the verdict would let "remote wins, value null" clobber the
  local `scheduled` on every pull. The field then belongs exclusively
  to the sidecar fold (chunk 4), whose embedded clocks decide.
- **Push encoding while the flag is false**: card writes serialize
  `startdate: null` (the connector-side conversion maps the task's
  `scheduled` to `None` — connector-independence ruling: lane choice
  is a connector concern). Measured harmless either way (silent
  no-op), but sending `null` keeps the wire honest about what the
  server will do with it.
- **Lane migration (server upgrade 1.17.x → 1.19+)**: the flag flips
  at the next client construction; native takes over. The clock slots
  are shared (`clocks.scheduled` stamps both lanes' writes by design —
  one field, one clock), so the first native verdict compares the
  server's entity stamp against the last sidecar fold's clock and LWW
  converges without a special case; the sidecar stops emitting
  `scheduled` and the lane-GC cleans the document on the next flush.
  No dual-lane merge window exists: each cycle reads exactly one lane
  per the flag.
- **Codec shape**: `scheduled`'s document encoding is the field's own
  domain shape (`Option<DateTime<Utc>>` + clock), same `{v, clock}`
  envelope as the other modeled keys — `null` = cleared (D-S2's rule,
  unchanged). Manifest **assertion 3 is reworded**: the modeled key
  set = `{effort, importance, urgency, scheduled}`, with a structural
  check that `scheduled`'s *emission/fold* sites are capability-gated
  (the gate lives in the sync/pipeline layers, not in the pure
  codec). Assertion 4 (chunk 4) routes `EditTask`'s scheduled slot
  through the lane rule (`UpdateTask` when the flag is true,
  `UpdateAppData` when false).

## 3. D-V3 — the unconditional wire changes (work on every measured version)

From the characterization (all measured on both tiers):

1. **Attachment create gains the additive `data` multipart field**
   (value = the client filename; required by 1.17.5, tolerated by
   1.19.0). The in-place update route already sends `data` — verified
   working on 1.17.5 as-is.
2. **Decode fallback for the create/update echo**: when the echoed
   `extendedData` lacks `fileName`, fall back to
   `extendedData.info.filename`, else empty (1.17.5 shape). The
   existing string-or-object `data` decode stands.
3. **Everything else is already version-proof**: the restore
   verb/route (PUT `…/restore`), the duplicate-409 rule, the
   no-validators content route, the stamp-advancement equality, the
   owner-as-string card PUT — all verified identical on 1.17.5 (11/15
   existing fixtures pass unmodified).

**Revised 1.17.5 gate fixture set** (chunk 3's revised ladder): the
attachment lifecycle fixture runs **unchanged** on both tiers (green
once D-V3.1/3.2 land — that *is* the parity proof); the deck1175
startdate wrapper becomes `…_deck1175_startdate_unsupported` pinning
the measured 1.17.5 behavior (create ignored, PUT no-op, read lacks
the key — the server is the fixed party; this pin is the capability
table's evidence, not behavior debt of ours); the regular tier keeps
`startdate_round_trip` unchanged; both tiers gain a capabilities
decode probe (the D-V1 seam pinned against both real servers).

## 4. Obligations by chunk (revised)

- **Chunk 3 (re-run)**: D-V1 probe/flags/table; D-V3 wire changes +
  revised fixtures (gate green on both tiers); capability-gated card
  conversion (`scheduled` → `None` in card writes while flag false);
  domain codec with `scheduled` as the fourth modeled key + reworded
  assertion 3; everything else per the original chunk-3 scope
  (ceremony, evidence, op arm — unchanged by this addendum).
- **Chunk 4**: fold gating (`scheduled` document key folds only while
  flag false; native verdict skipped while false — the
  `merge_task`-adjacent gate), emission gating + the union lane-GC,
  assertion 4's lane routing. The guard mirror needs no changes (it
  already treats all app fields uniformly; `scheduled`'s sidecar
  slots ride along).
- **Chunk 1** (landed): unaffected on the code side; its tier-2
  startdate round-trip pin is now understood as a
  capability-true-server pin (regular docker tier runs 1.19.0 —
  still green, no rework).
- **Docs**: `docs/testing_strategy.org` §8 gains the gate-tier rows
  and the measured 1.17.5 matrix entries (landed with the
  characterization report).

## 5. Risks

1. **1.18.x is unmeasured** — mapped to the conservative arm until a
   probe exists; the flag table makes extending it a fixture-plus-row
   change.
2. **The optimistic "newest-known wins" rule for future versions** —
   a future Deck release may change behavior again; MD-5's
   stop-and-report protocol (unchanged) is the catch, and the flag
   table is the repair path. This is the explicitly accepted
   trade-off of the ruling.
3. **Capabilities-route failure on exotic deployments** (reverse
   proxy stripping OCS, hardened instances): conservative fallback
   keeps the connector correct (sidecar lane + universal wire forms);
   native `scheduled` degrades to the sidecar lane until the probe
   succeeds. Recorded as accepted; an issue can track a
   `TASKBOARD_*` override env if it ever bites.
4. **The transient first-POST `405` on freshly-enabled instances**
   (characterization §3) — watchlist note; the fixture suite never
   reproduced it.

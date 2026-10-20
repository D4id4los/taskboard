# [Report] Multi-board Chunk 5 — Stage-3 Review (`feat/app-connector-registry`)

- **Date**: 2026-10-20
- **Reviewer**: stage-3 review round, dispatch plan §6 prompt 5.3.
- **Scope reviewed**: the full branch diff against `origin/main` @
  `1bcbb5e` (branch was already based on the current tip; rebase was a
  no-op, nothing dropped), against the stage-1 plan
  (`2026-10-19-multi-board-connector-registry-plan.md`), the
  implementation plan (§2 DF-3/DF-6/DF-7/DF-10, §3 chunk 5), the design
  (§3.3, §3.6, §3.7), and the dispatch plan §1/§6.

## Verdict

**PASS with fixes applied.** The branch implements chunk 5 as planned;
every functional focus item checked out; two findings were found and
fixed directly on the branch (folded into their origin commits via
fixup/autosquash — no duplicate-subject commits). Full AGENTS §10 gate
suite green on the final tree.

## Functional focus — verification record

- **DF-6 rejection list**: all six design instances are typed, loud,
  and pinned — `ConnectorShapeMixed` covers scalar+array (file *or*
  env; the later-provider merge-order test pins the env composition,
  probe case 6), `backend="standalone"`+array, and nextcloud+empty-array
  (each with its own test, two sharing the kind per D5-3);
  `NextcloudTableWithConnectors` scopes to the identity keys
  (`credential_store` + array stays valid — pinned);
  `DuplicateConnectorName`/`InvalidConnectorName` carry the offender in
  the payload (pinned structurally, no text assertions);
  `InvalidConnectorEntry` names entry + field for bad URL/username/
  standalone backend. Validation order pinned: array + garbage legacy
  URL fires the shape error, not `InvalidServerUrl`. No silently
  tolerated mix exists — the env path decodes-then-rejects.
- **Ceremony writer**: two paths over `toml_edit` with the plan's
  generic-navigation risk handled (`ArrayOfTables` *and* inline-table
  renderings both pinned); path B never writes any `backend` key or
  `[nextcloud]` keys, entry 1 untouched, comment preservation pinned;
  round-trip through `AppConfig::load` + validate in both shapes; an
  empty `connectors = []` array is refused loudly instead of writing a
  mix-shaped file.
- **Actors emit only per-connector variants**: grep over the sync actor
  finds zero surviving global emit sites; episode edge verbatim
  (`Network`-classified && `!offline`, exactly once; A7 rewritten and
  pinning the actor's own id, the duplicate silence, and
  report-then-restore order); the select-loop ignore arm covers the
  whole network vocabulary; the "this actor is the network authority"
  doc claim is rewritten.
- **Authority**: counts events only (`observe` never infers from
  silence), membership seeded from the module set at boot and updated
  by attach/detach hooks (plain sync fn calls on `App` — no inter-actor
  traffic); emits `NetworkLost` on last-down / `NetworkRestored` on
  first-up, transitions only (structural dedup); non-member events
  ignored; empty-set silence pinned; `Lagged` degrades honestly. The
  unit + real-broadcast + boot-integrated test catalogue (D5-13) is
  complete.
- **Engine**: `ConnectorLost` scopes `go_offline` via the new pure
  `bound_boards_of` (binding tags, tombstones included); global
  `NetworkLost` arm unchanged; zero-board and draining arms pinned.
- **Single-connector journeys byte-for-byte**: CLI journeys, b-series
  (except b11's renamed-but-equivalent refusal), v0.1.x halves, engine
  global-arm tests and snapshots all unmodified; `require_connector`
  keeps `SyncDisabled` with pluralised semantics (one stale doc line
  fixed, see findings).
- **Two-connector tier-2 test**: present and exercising the exit
  criterion's full bar (boards across both connectors, per-connector
  attribution visible in rows + binding tags, one-down flips only its
  boards Offline while the other advances and the global stays silent,
  all-down fires `NetworkLost`). See the tier-2 record below for the
  runner situation (MD-5).

## Findings and fixes

1. **F1 (test gap)**: the pipeline's per-connector attribution scope
   (commit `267db76`'s `(connector, remote board)` adoption key and the
   reporting-board ref-index prune) changed merge behavior with no
   pinning test at the layer that owns it. *Fixed*: added
   `a_foreign_connectors_colliding_board_adopts_fresh_not_merges` in
   `pipeline.rs` (a "beta" report colliding with an alpha-bound remote
   board id adopts fresh under beta's tag; no merge), folded into
   `267db76`.
2. **F2 (doc drift)**: `require_connector`'s doc comment still said
   "when the resolved backend is `standalone`" after D5-8 pluralised
   the semantics. *Fixed*: reworded to the no-connector-configured
   contract, folded into `be9e4a3`.

Recorded, no action: `fail_cycle` carries a
`#[allow(clippy::too_many_arguments)]` (the id is the one added
parameter; the allow is commented with its reason); the plan's D5-19
"e2e_hermetic.rs" pin list entry is vacuous — no such file exists on
`main` (it belongs to an unmerged branch), and its substance (hermetic
network pins with ids asserted) lives in the wiremock A7 rewrite and
the bootstrap authority pins.

## Gate suite (AGENTS §10, final tree)

`cargo fmt --check`, `cargo clippy -D warnings`,
`cargo nextest run --workspace` (1229 tests, includes the new pin),
`cargo test --doc`, `cargo deny check`, and the three-command coverage
sequence (quarantine clean over 1483 profraws, lcov written) — all
green.

## Tier-2 record (MD-5 honest degradation)

Docker is available and the test was attempted **six times** across
harnesses (nextest and libtest). It fails identically every time, and
instrumented forensics establish the cause as this runner's docker
host, not the branch:

- The **first test-owned instance** (`taskboard-it-nextcloud-c5a`) is
  externally stopped ~86 s after its spawn — `docker events` shows a
  `kill`+`stop` API sequence landing ~1 s after the journey's deck
  setup requests, container exits **gracefully (exit code 0,
  OOMKilled=false)**, while the second instance keeps running.
- Refuted empirically as causes: port binding (a plain nginx probe
  serves on the same loopback class), volume sharing (the compose file
  declares none), the second spawn's project teardown (reproduced
  outside the test — the first instance survives), concurrent compose
  (spawn is sequential), and API traffic (a sustained Deck request
  burst on a standalone instance is harmless).
- The test's own behavior during the kill is correct: the surviving
  connector cycles and attributes per board, the killed connector's
  actor classifies the outage `Network`, emits `ConnectorLost` exactly
  once, and only its own boards flip `Offline` — the journey's
  observable half works; the dead server simply never comes back, so
  the settle predicate times out.
- Stage 2 exercised the full journey green once (recorded in the
  commit message of `352a40a`) before the same kill; the hermetic N=2
  half of the exit criterion (registry, named routing, authority
  counting, scoped offline, N=2 fan-out — `bootstrap::
  connector_module_tests`) is green in this suite.

A stable tier-2 runner runs the journey via the documented
`nextcloud-it` filter; no code change is warranted on this evidence.

## Ship record

PR opened to `main` carrying `Closes #187` (the N=2 exit criterion:
verified hermetically in-suite; the dockerized half recorded above per
MD-5). Annotation comment posted on #125. Worktree removed at handoff.

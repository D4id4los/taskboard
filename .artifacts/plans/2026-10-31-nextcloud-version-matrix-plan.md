# [Plan] Nextcloud docker tier version matrix (backlog round 2, chunk 1.8, #116) — stage-1 plan

- **Date**: authored 2026-11-01 (file name date per the dispatch plan's
  naming)
- **Kind**: stage-1 chunk plan for dispatch chunk **1.8** of backlog
  round 2 (`.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §"Chunk 1.8"). CI-only; **no compose, no setup-script, no test-code
  behavior changes land in this chunk** (three stale Rust doc comments
  and one strategy sentence get truthfulness touch-ups — see §8).
- **Branch / worktree**: `ci/nextcloud-version-matrix` in
  `../taskboard-WT-ci-nextcloud-version-matrix` (off `main` @
  `5bf90c63`).
- **Normative origins**: GitHub issue **#116** (the ask: a matrix leg for
  the oldest supported Nextcloud/Deck major on the docker tier); the
  origin plan `.artifacts/plans/2026-10-08-nextcloud-sync-testing-plan.md`
  (the tier model — Tier 2 promises a secretless dockerized Nextcloud in
  CI; §8 of that plan explicitly parked the version matrix as backlog);
  `docs/testing_strategy.org` §8 (tier definitions, the Tier 2 *variant*
  gate instance, the known-server matrix — the supported floor is defined
  there, not guessed); the C-series gate-tier tooling
  (`DECK_VERSION=v1.17.5`, DDF-10,
  `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`).

---

## 1. Mission & binding constraints

Turn the weekly `Nextcloud Integration` workflow's **single** docker leg
(`nextcloud:35-apache` + app-store Deck) into a **two-leg matrix**: the
current stable pair (today's leg, unchanged) plus the **oldest supported
pair** (Nextcloud 33 + Deck 1.17.5 — the Tier 2 *variant* gate instance
that already exists locally and whose CI wiring strategy §8 deliberately
deferred to exactly this chunk). Per-leg failure visibility; parallel-safe
legs; YAML lint clean; **zero secrets** (the origin plan's
zero-secrets-on-GitHub property is binding).

Binding constraints (violating any is a review-blocking finding):

- **Weekly schedule untouched**: cron `0 4 * * 3` (Wednesdays 04:00 UTC)
  and `workflow_dispatch` stay byte-identical; the workflow stays
  out of the per-PR path (the Hub-pull-rate-limit rationale in the
  workflow header stands; two legs make that rationale *more* true, not
  less).
- **No secrets, no server addresses**: the only credentials remain the
  throwaway plaintext compose env (already committed, deliberately not
  secret) and the runtime-minted app password (written to `GITHUB_ENV`
  per runner, never to the repo). No new secrets, no `secrets:` context.
- **The image tag comes from the matrix parameter** — never a sed/patch
  into a shared file (dispatch §3 review trap, verbatim).
- **No soft-fail knob**: no `continue-on-error`, no filter exclusion for
  known-open bugs (dispatch §3: "a soft-fail knob that would hide a NEW
  regression is a finding"). Policy is D4 below.
- **Skip, never fail** (strategy §8 hard rule) is preserved per leg: on
  the stable leg the deck1175 tests still match the filter and skip
  honestly (their prefix env is unset there); on the floor leg the
  default-tier tests are simply not in the filter.

## 2. The seams (facts established — do not re-derive)

- `.github/workflows/nextcloud-integration.yml`: one job
  `nextcloud-it` ("Nextcloud Integration (docker)"), `ubuntu-latest`,
  `timeout-minutes: 30`, `concurrency` group on
  `${{ github.workflow }}-${{ github.ref }}`, `cancel-in-progress: true`,
  triggers `schedule` (Wed 04:00 UTC) + `workflow_dispatch`, top-level env
  `CARGO_TERM_COLOR: always`. Steps: checkout → apt (libdbus-1-dev,
  pkg-config for the keyring feature) → rust-toolchain → rust-cache →
  nextest install → `scripts/nextcloud_it_setup.sh up --ci` → nextest on
  `taskboard-sync-nextcloud` (filter `test(it_nextcloud_docker)`,
  `--run-ignored only`, `--no-tests=fail`) → nextest on `taskboard-app`
  (same filter; builds the bin via `CARGO_BIN_EXE_taskboard`) → teardown
  `down` under `if: always()`. **No artifact-upload steps exist**; failed
  runs are triaged from the runner console log (recorded decision D10).
- `docker/nextcloud-test/compose.yaml`: already parameterized —
  `image: ${TASKBOARD_IT_NC_IMAGE:-nextcloud:35-apache}`,
  `container_name: taskboard-it-nextcloud${SUFFIX:-}`,
  `ports: 127.0.0.1:${PORT:-8370}:80`,
  `SQLITE_DATABASE: taskboard-it${SUFFIX:-}`. Declares **no named
  volumes** (the official image's implicit `/var/www/html` volume is
  container-scoped anonymous, removed by `down -v`).
- `scripts/nextcloud_it_setup.sh`: already parameterized —
  `TASKBOARD_IT_SUFFIX` (compose project `-p taskboard-it${SUFFIX}`,
  container name, DB name all scope off it), `TASKBOARD_IT_PORT`,
  `DECK_VERSION` (empty = `occ app:install deck` app-store latest, with
  one retry; non-empty = pinned `nextcloud-releases/deck` GitHub release
  tarball, extracted to `custom_apps`, **verified via `occ app:list`**,
  fails loud on mismatch), `EXPORT_PREFIX` (default
  `TASKBOARD_IT_DOCKER`; `--ci` writes `${prefix}_URL/_USER/_TOKEN` to
  `GITHUB_ENV`), plus the app-password command drift probe
  (`user:add-app-password` vs `app-password:assign` — asks the image,
  never guesses).
- `crates/taskboard-sync-nextcloud/tests/`: the behavioral docker-tier
  bodies live once in `tests/common/suite` and are tier-bound by
  **credential prefix**. Default tier reads `TASKBOARD_IT_DOCKER_*`
  (`live_docker_config()`); the gate tier reads
  `TASKBOARD_IT_DOCKER_DECK1175_*` (`live_docker_deck1175_config()`).
  Four gate tests exist, all `#[ignore]`, all carrying the
  `it_nextcloud_docker` substring so the default CI filter already
  matches them (they skip when their prefix is unset):
  `it_nextcloud_docker_deck1175_scheduled_fallback_convergence`,
  `it_nextcloud_docker_deck1175_attachment_content_lifecycle`,
  `it_nextcloud_docker_deck1175_startdate_unsupported`,
  `it_nextcloud_docker_deck1175_capabilities`.
  `capabilities_probe` asserts a **floor** (`decoded >= min_version`) plus
  the derived capability-flag row (`flags_for(version).card_startdate`
  must equal the tier's pinned row) — so upward Deck drift on the stable
  leg passes by construction, and only a capability-semantics change
  trips it. The app-daemon journeys
  (`crates/taskboard-app/tests/it_docker.rs`) read the hardcoded
  `TASKBOARD_IT_DOCKER_*` names and have **no** floor-tier variants.
- Known-server matrix (strategy §8): Tier 3 live = Nextcloud 33.0.9 /
  Deck 1.17.5 (the user's real deployment); Tier 2 docker =
  `nextcloud:35` (35.0.1) / Deck 1.19.0; the 1.17.5 divergences are fully
  characterized (2026-10-22 transport report) and handled by the
  capability seam.
- **The dispatch plan's premise "the currently-open C4 bug" is stale**:
  #448 and #449 are both CLOSED/COMPLETED (chunk 1.1, PR #562 — shared
  root cause in `normalize_file_name`; the C4 gate test re-ran green in
  1.6–2.1 s on the gate tier per the closing comment). **No leg has a
  known-open red test today.** This changes D4 from what the dispatch
  text anticipated; the policy below is the recorded supersession.

## 3. Decisions (every open question, resolved)

**D1 — The legs: exactly two; the floor is `nextcloud:33-apache` +
Deck `v1.17.5`.**

| leg | Nextcloud image | Deck | credential prefix | filter |
|---|---|---|---|---|
| `stable` | `nextcloud:35-apache` | app-store latest (`occ app:install deck`, one retry) | `TASKBOARD_IT_DOCKER` | `test(it_nextcloud_docker)` |
| `floor` | `nextcloud:33-apache` | pinned `v1.17.5` (release tarball, `occ app:list`-verified) | `TASKBOARD_IT_DOCKER_DECK1175` | `test(it_nextcloud_docker_deck1175)` |

Why 33/1.17.5 is *the* supported floor: it is the pair the project
actually promises to serve — the user's live deployment (Tier 3:
NC 33.0.9/Deck 1.17.5), the capability table's conservative row
(`flags_for` startdate=false), and the fully characterized 2026-10-22
divergence set. The connector adapts to it via the capability probe;
the floor leg keeps that adaptation from rotting. Why not lower (32 and
below): nothing measured or characterized exists below 33 — an
uncharacterized leg would test a promise nobody made. Why not an
intermediate leg (NC 34, Deck 1.18/1.19 minors): no known-server-matrix
row, marginal signal, doubled Hub pulls; legs enter the matrix when a
pair is characterized, not for completeness. The matrix grows the day the
deployment floor moves or a new divergence row is measured — that is a
two-line `matrix.include` entry by design.

**D2 — Mechanism: the env-parameterized tag (existing seams), NOT a
compose-override file.**

`TASKBOARD_IT_NC_IMAGE`, `DECK_VERSION`, and `EXPORT_PREFIX` already
exist, are already proven by the C-series local gate runs, and are the
exact shape the dispatch review traps sanction ("the image tag must come
from the matrix parameter"). A compose-override file
(`-f compose.yaml -f compose.nc33.yaml`) would duplicate the whole
service block to change one field, creating a second source of truth for
ports/healthcheck/env that drifts under compose's merge semantics — and
it could not express the Deck pin anyway (that is setup-script behavior,
not compose). Net: **the entire implementation diff is
`.github/workflows/nextcloud-integration.yml`** (+ the §8 doc/comment
touch-ups). No compose file changes, no script changes.

**D3 — Floor-leg test surface: the four characterized gate tests, not the
full suite.**

The default suite's expectations are tier-bound by prefix *by design*:
`it_nextcloud_docker_capabilities` pins the 1.19 floor row
(startdate=true), `startdate_round_trip` needs a startdate-capable Deck —
running them against 1.17.5 is a guaranteed red by construction, and
un-binding those pins is test-scope work foreign to this CI-only chunk.
The four `deck1175` tests are precisely the characterized floor surface:
two-way scheduled-lane convergence, attachment content lifecycle, the
measured startdate-unsupported pin (evidence for the capability table),
and the capabilities probe asserting the (1,17,5,false) row. Stable leg
keeps today's filter byte-identical (`test(it_nextcloud_docker)` — the
deck1175 names match it and skip honestly, exactly as today).

**D4 — Pass/fail policy: every leg must pass; no soft-fail knob, no
known-bug exclusion.**

With #448/#449 fixed (chunk 1.1), the floor leg lands **green** and stays
green unless something regresses — a red leg is then always a real
finding, which is the only policy worth having on a weekly drift
detector. If a future known-open bug reddens a leg, the policy is
**fix-forward**: the leg stays red (honest, weekly cadence = minimal
noise), the bug gets a fix chunk, and re-greening the leg is part of that
chunk's acceptance. This supersedes the dispatch plan's stale C4 premise
(§2, last bullet); the stage-3 reviewer reads this plan.

**D5 — Per-leg failure visibility: `fail-fast: false` + the leg named in
the job name.**

`strategy.fail-fast: false` so one leg's failure never cancels the
other's run; `name: Nextcloud Integration (docker) — ${{ matrix.leg }}`
so the check-run name names its leg ("stable"/"floor") everywhere GitHub
surfaces it.

**D6 — App-daemon journeys run on the stable leg only.**

The `taskboard-app` step gets `if: matrix.run_app_journeys` (true only
for `stable`). Its harness reads hardcoded `TASKBOARD_IT_DOCKER_*`, has
no floor-tier variants, and its expectations target the current pair;
putting the daemon layer on the floor is future work for when a
1.17.5-specific daemon behavior actually matters.

**D7 — Leg isolation: separate runners in CI (defaults stay empty);
canonical suffix/port in local validation.**

In CI, matrix legs are separate GitHub-hosted runners — no shared Docker
daemon, so collision is impossible and both legs keep `SUFFIX` unset /
port 8370 (the stable leg stays byte-identical to today's singleton).
Locally, both legs coexist using the **canonical** values strategy §8
already documents: stable = defaults (8370), floor =
`TASKBOARD_IT_SUFFIX=-deck1175 TASKBOARD_IT_PORT=8371`. Isolation holds
by construction: the compose project `-p taskboard-it${SUFFIX}` scopes
the container name (`taskboard-it-nextcloud${SUFFIX}`), the default
network (`taskboard-it${SUFFIX}_default`), and the SQLite DB name; the
compose file declares no named volumes (anonymous, container-scoped,
removed by `down -v`); ports are loopback binds parameterized by `PORT`.
The workflow's `concurrency` group already serializes runs *within* the
workflow. Teardown must carry the same `TASKBOARD_IT_SUFFIX`.

**D8 — Pin granularity: major-pinned rolling image tags; Deck pinned
exactly on the floor, store-latest on stable — deliberately asymmetric.**

`nextcloud:33-apache` / `nextcloud:35-apache` (rolling within the major,
consistent with today's default-tier granularity): weekly patch drift
inside the major is signal (drift detection), not noise. Deck is
asymmetric on purpose: the floor pins **exactly** `v1.17.5` so a red
floor leg can only mean a connector regression, never upstream noise;
the stable leg installs app-store latest so upstream drift *is* the
signal — upward versions ride the `capabilities_probe` floor, and only a
derived-flag semantics change trips the row check, failing loud for a
deliberate capability-row update (the 2026-10-22 characterization flow).

**D9 — YAML lint: actionlint + yamllint as stage-2 validation, no new CI
lint job.**

Stage 2 runs `actionlint` (workflow) and `yamllint -d relaxed` (workflow
+ `docker/nextcloud-test/compose.yaml`) locally and records the tool
versions and clean output. No lint job is added to the workflow: this
workflow is schedule/dispatch-only (never a PR check), so lint-in-CI buys
nothing here, and `ci.yml` has no lint job today — adding repo-wide lint
infrastructure is out of scope for a CI-only chunk.

**D10 — Artifacts: none added.**

The workflow has no artifact-upload steps today; failed legs are triaged
from the runner console log (nextest failure output and setup stderr are
both captured there). The #449-style *inside-the-container* DB inspection
remains a local-repro exercise (strategy §8's documented flow). Artifact
plumbing (`docker compose logs` under `if: failure()`) is noted as a
follow-up if weekly triage ever demands it — not now.

**D11 — Secrets: unchanged, zero.**

No new env beyond the four matrix fields (`nc_image`, `deck_version`,
`export_prefix`, plus the leg name/filter); no `secrets:` context; the
minted app password still lives only in `GITHUB_ENV` per runner. Stage 3
greps the diff for env blocks (dispatch trap) and finds only throwaway
values already public in the compose file.

## 4. Workflow shape (what stage 2 implements)

One file changes: `.github/workflows/nextcloud-integration.yml`.
Sketch (authoritative for field names; stage 2 keeps the surrounding
steps byte-identical):

```yaml
jobs:
  nextcloud-it:
    name: Nextcloud Integration (docker) — ${{ matrix.leg }}
    runs-on: ubuntu-latest
    timeout-minutes: 30
    strategy:
      fail-fast: false
      matrix:
        include:
          - leg: stable
            nc_image: nextcloud:35-apache
            deck_version: ""
            export_prefix: TASKBOARD_IT_DOCKER
            sync_filter: test(it_nextcloud_docker)
            run_app_journeys: true
          - leg: floor
            nc_image: nextcloud:33-apache
            deck_version: v1.17.5
            export_prefix: TASKBOARD_IT_DOCKER_DECK1175
            sync_filter: test(it_nextcloud_docker_deck1175)
            run_app_journeys: false
    steps:
      # checkout / apt / toolchain / rust-cache / nextest — unchanged
      - name: Start dockerized Nextcloud (Deck, test user, app password)
        env:
          TASKBOARD_IT_NC_IMAGE: ${{ matrix.nc_image }}
          DECK_VERSION: ${{ matrix.deck_version }}
          EXPORT_PREFIX: ${{ matrix.export_prefix }}
        run: scripts/nextcloud_it_setup.sh up --ci
      - name: Docker-tier integration tests (sync actor)
        run: >
          cargo nextest run -p taskboard-sync-nextcloud --run-ignored only
          -E '${{ matrix.sync_filter }}' --no-tests=fail
      - name: Docker-tier integration tests (app daemon + CLI journeys)
        if: matrix.run_app_journeys
        run: >
          cargo nextest run -p taskboard-app --run-ignored only
          -E 'test(it_nextcloud_docker)' --no-tests=fail
      - name: Teardown
        if: always()
        run: scripts/nextcloud_it_setup.sh down
```

Also: update the workflow's header comment to describe the two legs
(stable = drift detector; floor = the #116 oldest-supported pin, the
strategy §8 gate instance now CI-wired) while keeping the Hub-pull-limit
and GHCR-mirror paragraphs. Everything else — triggers, concurrency,
timeout, top-level env — untouched.

Expected outcomes per leg: **stable** — today's run, byte-for-byte (the
four deck1175 tests appear as passing skips). **floor** — exactly the
four gate tests matched (`--no-tests=fail` guards the filter), all
passing (post-#562 evidence: C4 in ~2 s). Runtime well inside the 30-min
per-leg timeout.

## 5. Validation protocol (stage 2 executes; record outcomes per leg)

Preconditions: Docker running; 8370/8371 free; nextest on PATH. Both legs
may run in either order or concurrently (D7 isolation makes that safe);
the exact nextest invocations the workflow uses are mandatory.

1. **Lint**: `actionlint .github/workflows/nextcloud-integration.yml`
   and `yamllint -d relaxed .github/workflows/nextcloud-integration.yml
   docker/nextcloud-test/compose.yaml`; record tool versions; both must
   exit 0 with zero findings.
2. **Stable leg (local, canonical defaults)**:

   ```sh
   eval "$(TASKBOARD_IT_NC_IMAGE=nextcloud:35-apache \
     EXPORT_PREFIX=TASKBOARD_IT_DOCKER scripts/nextcloud_it_setup.sh up)"
   cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
     -E 'test(it_nextcloud_docker)' --no-tests=fail
   cargo nextest run -p taskboard-app --run-ignored only \
     -E 'test(it_nextcloud_docker)' --no-tests=fail
   scripts/nextcloud_it_setup.sh down
   ```

   Expect: the full default surface passes (the deck1175 tests pass as
   skips); both workflow steps exercised.
3. **Floor leg (local, canonical gate-tier values per strategy §8)**:

   ```sh
   eval "$(TASKBOARD_IT_SUFFIX=-deck1175 TASKBOARD_IT_PORT=8371 \
     EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175 \
     TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache DECK_VERSION=v1.17.5 \
     scripts/nextcloud_it_setup.sh up)"
   cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
     -E 'test(it_nextcloud_docker_deck1175)' --no-tests=fail
   TASKBOARD_IT_SUFFIX=-deck1175 scripts/nextcloud_it_setup.sh down
   ```

   Expect: exactly 4 tests matched, all pass (setup verifies
   `deck 1.17.5` via `occ app:list` — a mismatch dies loud, never a
   silently wrong tier).
4. **Canary**: `cargo nextest run -p taskboard-sync-nextcloud` (default
   hermetic suite) stays green with Docker stopped;
   `cargo fmt --all -- --check` and
   `cargo clippy --workspace --all-targets -- -D warnings` stay green
   after the §8 comment touch-ups.

## 6. Risks & mitigations

| Risk | Mitigation / policy |
|---|---|
| Docker Hub anonymous-pull 429 (documented in the workflow header) — the matrix pulls two distinct tags per run instead of one | Pre-existing risk at weekly cadence; a 429-red leg is re-run via `workflow_dispatch`. If it bites regularly, the header's own follow-up applies: mirror the pinned images into GHCR (out of scope now) |
| Deck release-tarball download flake on the floor leg (`install_deck_pinned` has no retry) | Fails loud; one-click re-run. Adding a retry is a script change out of scope for this chunk |
| App-store outage or a Deck release breaking `occ app:install deck` reddens the stable leg | Pre-existing (single retry already in the script); fix-forward per D4 |
| Upstream capability-semantics drift trips the stable leg's derived-flag row check | That is the tripwire working; response is a deliberate capability-row update per the 2026-10-22 characterization flow, not a workflow change |
| Upstream removes the `33-apache` manifest someday | Pull fails loud; re-pin (digest or new floor) — visible, never silent |
| `rust-cache` keeps per-matrix entries → double compile across legs | Weekly cadence and the 30-min per-leg timeout absorb it; the floor leg builds only the sync crate (no app bin) so it is the cheaper leg |
| Job/check name changes from the matrix (`nextcloud-it` → `nextcloud-it (leg)`) | This workflow never runs on PRs, so it cannot be a required status check; nothing on `main` protection depends on the name |
| Dispatch plan's stale C4 premise misleads the stage-2/3 agents | D4 records the supersession with evidence (#448/#449 closed via PR #562); this plan is normative over the dispatch prose |

## 7. Out of scope

No compose or setup-script changes; no test-code behavior changes; no new
majors/legs (NC 34/36, Deck 1.18.x); no app-daemon floor surface (D6); no
GHCR mirror; no artifact uploads (D10); no CI lint job (D9); no CHANGELOG
(CI-only, per dispatch); no per-PR wiring (the header's Hub-limit
rationale stands); no CHANGELOG release work; no push, no PR from stage 1.

## 8. Doc truthfulness touch-ups (in this chunk, comment-only)

After this lands, three comments and one strategy sentence become false —
fix them in the same PR so the docs never lie about CI:

1. `crates/taskboard-sync-nextcloud/tests/it_docker.rs` gate-tier doc
   comment: "(CI does not wire the variant)" → wired to the weekly
   workflow's floor leg (link the workflow file).
2. Same file, the `it_nextcloud_docker_deck1175_scheduled_fallback_convergence`
   body comment: "the gate tier runs locally, CI does not wire it" →
   updated likewise.
3. Same file, the deck1175 `#[ignore]` reason strings stay correct
   (they describe the local invocation) — verify, don't rewrite.
4. `docs/testing_strategy.org` §8 gate-tier bullet: "CI wiring
   deliberately not wired as of 2026-10-22" → wired since this chunk
   (date + workflow pointer). Keep the local-invocation snippet (it is
   the validation protocol's floor-leg command).

## 9. Done when (chunk acceptance)

- The workflow defines both legs (stable + floor) with per-leg failure
  visibility (`fail-fast: false`, leg named in the check), schedule and
  dispatch triggers byte-identical.
- Both legs validated locally with exactly the workflow's nextest
  invocations; outcomes recorded per leg; containers torn down.
- `actionlint` + `yamllint` clean (versions recorded).
- Legs parallel-safe (D7 analysis holds; both legs ran concurrently or
  teardown confirms no cross-contamination).
- Zero secrets in the diff; no compose/script changes; §8/doc comment
  touch-ups landed.
- Canary gates green (fmt, clippy, hermetic sync suite).

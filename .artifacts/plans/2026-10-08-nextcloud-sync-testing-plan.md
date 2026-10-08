# [Plan] Nextcloud Sync Testing Environment Build-Out

- **Date**: 2026-10-08
- **Target**: `taskboard-sync-nextcloud` test infrastructure (all four tiers of
  `docs/testing_strategy.org` §8) plus a minimal Deck client slice to prove it
- **Goal**: every tier of the strategy runnable — fixtures + contract tests in
  the default suite, a secretless dockerized Nextcloud tier in CI, and a
  local-only live tier against the private server — with zero Nextcloud
  secrets on GitHub.
- **Status**: READY (awaiting implementation)
- **Strategy source**: `docs/testing_strategy.org` §8 (committed on
  `docs/testing-strategy-nextcloud-tiers`); decisions confirmed by the user in
  chat on 2026-10-08: Deck OCS REST API, app-password Basic auth, docker tier
  in CI, live tier manual/local-only, `.env` secrets.

---

## 1. Context

`taskboard-sync-nextcloud` is a doc-only stub; the workspace has no tests at
all yet. This plan builds the **testing environment** the strategy §8
prescribes and validates it with a **vertical slice**: a `DeckClient` that
can list/create/delete boards over the Deck OCS REST API. The full sync
client surface (stacks, cards, sync actor, conflict resolution) is expressly
out of scope and gets its own future plans.

Known facts established during planning (do not re-derive):

- `reqwest` is already pinned with `json` + `rustls-tls` (root `Cargo.toml`)
  — HTTPS against the private server works without feature changes.
- `.gitignore` already ignores `.env` and `.env.*` — but that pattern also
  swallows the to-be-committed `.env.example`; PR C fixes this with a
  negation.
- `tokio` workspace features lack `test-util`, which
  `tokio::time::pause()`/`start_paused` require — add it as a dev-only
  feature addition in PR A.
- Deck API base path: `{server}/index.php/apps/deck/api/v1.0`; every request
  needs `Authorization: Basic base64(user:app_password)`,
  `OCS-APIRequest: true`, and `Accept: application/json`. Responses are JSON
  in the OCS envelope `{"ocs": {"meta": {...}, "data": ...}}`.

## 2. Deliverable Shape: three PRs, each independently green

| PR | Branch | Contents |
|---|---|---|
| A | `feat/sync-nextcloud-deck-slice` | dev-deps, test harness, `DeckClient` slice, Tier 0 + Tier 1 (default suite stays hermetic) |
| B | `test/sync-nextcloud-docker-tier` | compose file, setup script, Tier 2 tests, CI job |
| C | `test/sync-nextcloud-live-tier` | `.env.example`, Tier 3 live smoke tests, fixture-harvest example, doc touch-ups |

Sequential: B depends on A's harness; C depends on A. Each PR passes the
§10 quality gates on its own and leaves `main` shippable.

## 3. PR A — Harness + Client Slice + Tiers 0–1

### 3.1 Dependencies (root `[workspace.dependencies]`, pinned)

- `wiremock = "0.6"` (Tier 1 mock HTTP server)
- `dotenvy = "0.15"` (Tier 3 `.env` loading)
- `uuid` is already pinned (run-id generation); member crates consume via
  `workspace = true`.
- In `crates/taskboard-sync-nextcloud/Cargo.toml`
  `[dev-dependencies]`: `wiremock`, `dotenvy`, `uuid`, `proptest`, `tokio`
  (all `workspace = true`), plus
  `tokio = { workspace = true, features = ["test-util"] }` semantics via a
  dev-specific entry — feature additions are additive and allowed; no
  unpinned duplicates.

### 3.2 `DeckClient` vertical slice (`src/client.rs` + `src/error.rs`)

- `DeckClient::new(base_url: &str, user: &str, token: &str) ->
  Result<Self, DeckError>` — rejects malformed base URLs (`InvalidBaseUrl`).
- One method per slice operation:
  - `async fn boards(&self) -> Result<Vec<Board>, DeckError>`
    (`GET {base}/index.php/apps/deck/api/v1.0/boards`)
  - `async fn create_board(&self, title, color) -> Result<Board, DeckError>`
    (`POST .../boards`, JSON body `{title, color}`)
  - `async fn delete_board(&self, id) -> Result<(), DeckError>`
    (`DELETE .../boards/{id}`)
- `Board { id: u64 (or string — match what the real server returns in
  Tier 2 and adjust once), title: String, color: String }` with serde.
- OCS envelope types: `OcsEnvelope<T> { ocs: OcsBody<T> }`,
  `OcsBody<T> { meta: OcsMeta, data: T }`; `OcsMeta` keeps `statuscode`,
  ignores unknown fields.
- Typed errors (`thiserror`, no message-text API):

  ```rust
  pub enum DeckError {
      InvalidBaseUrl,
      Transport(reqwest::Error),
      Envelope(serde_json::Error),
      Unauthorized,        // 401
      Forbidden,           // 403
      NotFound,            // 404
      RateLimited,         // 429 after retries exhausted
      Server(u16),         // other 5xx (non-retryable)
      Unavailable,         // 503 after retries exhausted
  }
  ```

- Retry: a **pure** `BackoffPolicy` producing the delay sequence (e.g.
  500 ms doubling, capped at 8 s, max 3 attempts); the retry loop in the
  client retries `429`, `503`, and transport errors only. Policy is
  proptest-able (delays strictly increasing until the cap, correct count).
- Crate keeps `// SPDX-License-Identifier: MIT OR Apache-2.0` and
  `#![forbid(unsafe_code)]`.

### 3.3 Tier 0 — unit tests + fixtures (`src`, `tests/fixtures/deck/`)

- Hand-write `boards_list.json`, `board_create_response.json`,
  `envelope_error.json` from the Deck API docs as the first pass (real
  harvested replacements come with PR C's example).
- Unit tests: envelope decode (ok + unknown-field tolerance + malformed),
  status→`DeckError` mapping table, `BackoffPolicy` proptest, base-URL
  normalization (trailing slash, missing scheme).

### 3.4 Tier 1 — contract tests (`tests/contract.rs`, run by default)

Each test spins its own `wiremock::MockServer`; assertions are on structure
and typed outcomes (no response-text asserts):

1. Happy path: correct method+path, `Authorization` header is Basic-auth of
   the exact `user:token`, `OCS-APIRequest: true` present, `Accept`
   JSON; envelope unwrapped into `Vec<Board>`.
2. `401 → Unauthorized`, `403 → Forbidden`, `404 → NotFound` via `matches!`.
3. `500 → Server(500)` and **no** second request (mock expects exactly 1 hit).
4. Retry: `503, 503, 200` sequence → success after 2 retries; under
   `#[tokio::test(start_paused = true)]` assert the virtual elapsed time
   equals the policy delays. If auto-advance with a real socket proves
   flaky, inject a clock seam into the retry loop — never weaken the test.
5. `429` twice then exhaustion → `RateLimited`; retry delays respected.
6. Malformed JSON / non-JSON content type → `Envelope`.
7. Transport error (mock server dropped) → `Transport`.

### 3.5 Harness (`tests/common/mod.rs`)

- `fn live_docker_config() -> Option<LiveCfg>` — reads
  `TASKBOARD_IT_DOCKER_{URL,USER,TOKEN}`; `None` when any is unset.
- `fn live_config() -> Option<LiveCfg>` — `dotenvy::dotenv()` then reads
  `TASKBOARD_IT_NEXTCLOUD_{URL,USER,TOKEN}`; `None` when unset.
- `fn run_id() -> String` — `taskboard-it-` + 8 chars of a UUIDv4.
- `const LIVE_DEADLINE: Duration = Duration::from_secs(30)` and a
  `with_deadline()` wrapper around every live/docker test body.
- Skip pattern for `#[ignore]` tests: `let Some(cfg) = ... else { return; }`.

### 3.6 Done when

Gates green; `cargo nextest run --workspace` passes with Docker stopped
(hermetic by construction — Tier 1 is in-process only); contract tests run
in the ordinary CI `test` job with no workflow change.

## 4. PR B — Tier 2: Dockerized Nextcloud + CI

### 4.1 `docker/nextcloud-test/compose.yaml`

```yaml
services:
  nextcloud:
    image: nextcloud:<MAJOR>-apache   # pin the current stable major at
                                      # implementation time (hub.docker.com
                                      # /r/library/nextcloud); do not use :latest
    container_name: taskboard-it-nextcloud
    ports:
      - "127.0.0.1:8370:80"           # non-default port; loopback-only bind
    environment:
      NEXTCLOUD_ADMIN_USER: taskboard-admin
      NEXTCLOUD_ADMIN_PASSWORD: taskboard-it-admin-pw   # throwaway, by design
      SQLITE_DATABASE: taskboard-it
      NEXTCLOUD_TRUSTED_DOMAINS: localhost 127.0.0.1
    healthcheck:
      test: ["CMD-SHELL", "curl -fsS http://localhost/status.php | grep -q '\"installed\":true'"]
      interval: 5s
      timeout: 5s
      retries: 36
      start_period: 30s
```

Throwaway credentials in plaintext are deliberate (strategy §8): the
instance is loopback-only and ephemeral. Verify `curl` exists in the image;
if not, switch the healthcheck to a `php`/`wget` probe — the host-side poll
in §4.2 is the authoritative readiness gate either way.

### 4.2 `scripts/nextcloud_it_setup.sh`

Subcommands: `up` (default), `down`. Style follows `scripts/` conventions
(bash + `python3` helpers).

1. `docker compose -f docker/nextcloud-test/compose.yaml up -d --wait`
   (falls back to plain `up -d` + poll if the compose version lacks
   `--wait`).
2. Host-side readiness poll with a 180 s deadline (poll a predicate, never
   sleep): `python3` GET `http://127.0.0.1:8370/status.php` until
   `installed == true`.
3. `docker compose exec -T -u www-data nextcloud php occ app:install deck`
   (guard: if already installed, `occ app:enable deck`; retry once on
   app-store download failure).
4. `occ user:add --password-from-env=TASKBOARD_IT_USER_PASSWORD taskboard-it`
   (idempotent guard via `occ user:info`).
5. Mint the app password — `occ user:add-app-password taskboard-it`
   (newer releases: `occ app-password:assign`); **verify the exact flag
   names against the pinned image** and capture the printed token. Fail
   loudly rather than guessing.
6. Print the export block; with `--ci`, also write the three variables to
   `$GITHUB_ENV`:

   ```sh
   export TASKBOARD_IT_DOCKER_URL=http://127.0.0.1:8370
   export TASKBOARD_IT_DOCKER_USER=taskboard-it
   export TASKBOARD_IT_DOCKER_TOKEN=<minted>
   ```

### 4.3 Tier 2 tests (`tests/it_docker.rs`, `#[ignore]`, names start `it_nextcloud_docker_`)

- `it_nextcloud_docker_lists_boards` — real auth + envelope against a real
  server.
- `it_nextcloud_docker_board_lifecycle` — create board titled
  `taskboard-it-<run-id>` → appears in `boards()` → delete → absent from
  listing. Best-effort delete in a teardown path that also runs on
  assertion failure.

All under `with_deadline()`; skip when `live_docker_config()` is `None`.
Local run:

```sh
eval "$(scripts/nextcloud_it_setup.sh up)" &&
cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
  -E 'test(it_nextcloud_docker)'
```

### 4.4 CI job (`.github/workflows/ci.yml`)

```yaml
  nextcloud-it:
    name: Nextcloud Integration (docker)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@v2
        with:
          tool: nextest
      - name: Start dockerized Nextcloud (Deck, test user, app password)
        run: scripts/nextcloud_it_setup.sh up --ci
      - name: Docker-tier integration tests
        run: cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
             -E 'test(it_nextcloud_docker)' --no-tests=fail
      - name: Teardown
        if: always()
        run: scripts/nextcloud_it_setup.sh down
```

No secrets; not part of the coverage job; expected runtime 3–5 min.
`--no-tests=fail` guards the filter silently matching nothing.

### 4.5 Done when

Local §4.3 run passes from a clean Docker state; the CI job is green on the
PR; `cargo nextest run --workspace` (default suite) still passes with no
Docker running.

## 5. PR C — Tier 3: Live Server + Fixture Harvest

### 5.1 Repo files

- `.gitignore`: add `!.env.example` directly after `.env.*` (the existing
  pattern would otherwise ignore the example file).
- Commit `.env.example` — names and comments only:

  ```
  # Tier 3 live-server tests (docs/testing_strategy.org §8). Copy to .env
  # and fill in; .env is git-ignored. Never commit real values.
  TASKBOARD_IT_NEXTCLOUD_URL=https://your-nextcloud.example
  TASKBOARD_IT_NEXTCLOUD_USER=
  TASKBOARD_IT_NEXTCLOUD_TOKEN=        # app password, not the login password
  ```

### 5.2 Tier 3 tests (`tests/it_live.rs`, `#[ignore]`, names start `it_nextcloud_live_`)

- `it_nextcloud_live_lists_boards` — read-only smoke.
- `it_nextcloud_live_board_lifecycle` — create/delete a
  `taskboard-it-<run-id>` board only; best-effort teardown.

Both load config via `live_config()` (dotenvy) and skip when unset. Run
from the repo root:

```sh
cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
  -E 'test(it_nextcloud_live)'
```

### 5.3 Fixture harvest (`examples/harvest_deck_fixtures.rs`)

Env-configured like Tier 3; creates a run-id board with known content,
fetches `GET /boards`, pretty-prints the JSON to
`tests/fixtures/deck/boards_live.json` (stdout redirect or direct write —
implementer's choice), deletes the board. Manual refresh workflow; harvested
files are reviewed before committing (they must contain only run-id board
data).

### 5.4 Doc touch

`testing_strategy.org` §8: name `dotenvy` alongside `wiremock`, reference
the harvest example as the fixture-refresh path. CHANGELOG `[Unreleased]`
entry for the whole build-out (can ride PR B/C).

### 5.5 Done when

The user runs §5.2 against the private server successfully (manual gate);
`git check-ignore .env` outputs `.env`; `git status` shows `.env.example`
tracked and `.env` ignored.

## 6. YOUR Tasks (manual)

1. **Private server test account** (any time before PR C verification):
   create `taskboard-it` (or similar) on your Nextcloud, mint an app
   password under that account (Settings → Security → Devices & sessions),
   fill your local `.env`. Security rule as in the external-services plan:
   never paste secret *values* into chat or repo files — names only.
2. **Optional, after PR B merges**: add `Nextcloud Integration (docker)` to
   the `protect-main` required status checks (or tell the agent
   `ruleset: agent-task`).
3. Review/merge the three PRs per the double-rebase policy.

## 7. Verification & Acceptance (overall)

1. Default suite hermetic: `cargo nextest run --workspace` green with Docker
   stopped and no `.env` present.
2. All §10 gates green on each PR.
3. CI green including the new job; **no new GitHub secrets added**.
4. Tier 2 reproducible locally from clean Docker state (§4.3 command).
5. Tier 3 passes against the private server (user-verified, once).
6. Nothing secret in the repo: `git check-ignore .env`; review harvested
   fixtures before commit.

## 8. Out of Scope → Backlog (appended with this plan)

- Nextcloud major-version matrix for the docker tier (oldest supported +
  latest stable).
- Scheduled live-tier runs via a self-hosted LAN runner (keeps CI logs
  private; replaces the manual-only rule if ever wanted).
- Full Deck client surface (stacks, cards, attachments), sync actor,
  conflict resolution — future feature plans.

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `occ` flag names drift by Nextcloud version (`user:add-app-password` vs `app-password:assign`) | setup script verifies against the pinned image and fails loudly; version pinned in compose |
| Deck app-store download flaky in CI | retry once in setup script; failure is a loud setup error, not a silent skip |
| Image startup 30–90 s | healthcheck + host poll with 180 s deadline; job budget ~3–5 min |
| `start_paused` + real wiremock sockets (auto-advance) flaky | inject a clock seam into the retry loop; never weaken the timing assertions |
| `curl` missing in the image healthcheck | swap to `php`/`wget` probe; host-side poll is authoritative |
| Port 8370 occupied locally | loopback + non-default port; document override via `TASKBOARD_IT_PORT` if it bites |

# [Plan] Testing Docs & State Doc Comments — Chunk 13, Wave 1 (#265 #295 #246)

- **Date**: 2026-10-18
- **Kind**: stage-1 implementation plan (remediation-wave-1 dispatch,
  chunk 13; three-stage protocol — this plan is the planning stage's
  deliverable, written in the worktree, no implementation in its commit).
- **Origin (normative, binding)**: dispatch plan
  `.artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md` §1
  (row 13) + §14 (chunk 13 acceptance); issues **#265** (F15-08, F15-14),
  **#295** (F14-01 — manual-runbook variant, `workflow_dispatch`
  alternative explicitly deferred), **#246** (F4-04); findings report
  `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md` rows
  F15-08/F15-14/F14-01/F4-04 + agenda rows P1-5 (schedule-now,
  docs-only), P-2 (schedule-now, docs wave), (d)-adjacent (#278 —
  coordination note, §6); owning reports
  `2026-10-16-v0.1.0-audit-theme-docs-drift-report.md` (F15-08/F15-14
  detail), `2026-10-16-v0.1.0-audit-theme-test-quality-report.md`
  (F14-01 detail, §5 row 28), `2026-10-16-v0.1.0-audit-crate-state-report.md`
  (F4-04 detail, §4.3).
- **Branch / worktree**: `docs/testing-docs-runbook` in
  `../taskboard-WT-docs-testing-docs-runbook`, cut from `main` @
  `76838a1` (2026-10-18, in sync with `origin/main`). Dispatch note:
  chunk 13 plans concurrently with chunk 1 but its PR must not merge
  before chunk 1's (no renamed symbols live in this chunk's files, so
  no rebase risk is expected).
- **Status**: READY (every open question settled below with a recorded
  decision; every doc claim carries a tree-verified pointer).

## 1. What this chunk is (and is not)

Three docs-shaped corrections, one PR:

1. **#265 / F15-08** — `docs/testing_strategy.org` §3 prescribes
   `mockall` (+ `async-trait` via `#[automock]`); neither is wired
   anywhere and the project deliberately hand-rolls `BoxFuture` ports.
   Rewrite the paragraph to the actual standard.
2. **#265 / F15-14** — `docs/coding_guidelines.org` §7 "Double-Rebase
   Merge into main" (:143–153) describes a procedure unperformable
   under branch-protected `main`. Replace with the PR-flow reality.
3. **#295 / F14-01** — the keyring gated test
   (`it_keyring_local_round_trip`, guard `TASKBOARD_IT_KEYRING=1`)
   has no CI runner anywhere and its doc comment promises a "tier-3
   runbook" that does not exist. Add the runbook entry to
   `testing_strategy.org` §8 (docs-only decision; the secretless
   `workflow_dispatch` CI variant is **explicitly deferred**).
4. **#246 / F4-04** — stale doc comments in
   `crates/taskboard-state/src/config.rs` name a historical dispatch
   chunk and a nonexistent "interim bootstrap wiring". Comment-only
   code edit (zero code-token changes).

This is a **docs-only** chunk by decision (P1-5, P-2: "schedule-now,
docs-only"). It changes two `.org` files and the *comments* of one
`.rs` file; it adds no tooling, no CI, no tests.

## 2. Verified ground truth (every claim → tree pointer @ 76838a1)

The stage-2 agent works from this table; re-verify pointers only if
the tree moved.

### 2.1 F15-08 premise (mockall unwired, hand-rolled ports real)

| Claim | Evidence |
|---|---|
| `mockall` zero hits in every manifest + `Cargo.lock` | `grep -r mockall Cargo.toml crates/*/Cargo.toml` → none (re-verified 2026-10-18) |
| `async-trait` not a direct dependency of any workspace crate | same grep → none |
| Ports are hand-rolled `BoxFuture` traits | `crates/taskboard-domain/src/persistence.rs:5` ("the same hand-rolled pattern the sync crate uses") and `:25` (`pub type BoxFuture`, "the domain may not depend on `futures`, `tokio`, or `async-trait`") |
| Handwritten fakes are the real standard, with shipped examples | `taskboard_domain::test_support::InMemoryRepository` (`crates/taskboard-domain/src/test_support/mod.rs:19`, proven against the port contract in `test_support/contract.rs:462`); engine-suite fakes (state `tests/engine.rs` etc.); UI forwarder fake `crates/taskboard-ui-slint/tests/support/fake_engine.rs` |
| AGENTS §5 already states the binding rule | "Fakes over mocks: favor handwritten fakes … over pre-programmed mocking libraries" |
| Sole correction site | `mockall`/`automock` appear in exactly one doc location: `docs/testing_strategy.org:46–49` (zero hits elsewhere in `docs/` or `AGENTS.md`) |

### 2.2 F15-14 premise (merge procedure unperformable)

| Claim | Evidence |
|---|---|
| The section text | `docs/coding_guidelines.org:143–153` ("check out `main` and rebase `main` onto `feat/foo`") |
| `main` is branch-protected; PR-only entry; local merges unavailable | `AGENTS.md` §8 (normative; introduced in 3682da7, guidelines section never updated — docs-drift report F15-14 history) |
| `main` is **rebase-merged**; commits land verbatim; hygiene rules; stacked-PR `--force-with-lease` dance | `AGENTS.md` §8 ("Commit hygiene for rebase merges", "Stacked PRs + rebase merges require the rebase dance") |
| Adjacent sections already consistent, untouched | "Git Usage Policy (for agents)" :155–163 and "Release Versioning" :165–177 match AGENTS §8/§9 today |

### 2.3 F14-01 premise (the gated keyring test, walked)

Test: `it_keyring_local_round_trip`,
`crates/taskboard-app/tests/secrets.rs:126–140`:

- `#[ignore = "requires a desktop Secret Service; run with TASKBOARD_IT_KEYRING=1"]`
  + internal guard `secrets.rs:129–131`: env unset ⇒ `return` —
  **skip-never-fail**. Consequence (must be documented): a green run
  *without* the gate exported is vacuous — the `--run-ignored only`
  invocation alone proves nothing.
- Behavior `:132–139`: `KeyringStore.store("https://keyring-test.example.com",
  "roundtrip", AppPassword::new("local-secret"))` → `load` → `delete`
  → assert loaded == stored. **Cleanup runs before the assert**, so any
  in-test failure still self-cleans; only a killed process between
  store and delete can leave the entry behind.
- What it dirties: `KeyringStore` = `Entry::new(KEYRING_SERVICE,
  credential_key(server, username))`
  (`crates/taskboard-app/src/secrets.rs:102/139`), so one entry —
  service `"taskboard"` (`KEYRING_SERVICE`, secrets.rs:23), account
  `roundtrip@keyring-test.example.com` (`credential_key` =
  `username@host`, secrets.rs:194) — written into the **developer's
  own** OS credential store. The secret value is the throwaway literal
  `local-secret`.
- Linux backend: `keyring` 3.6.3, feature `sync-secret-service`
  (+`crypto-rust`) per `crates/taskboard-app/Cargo.toml:68`;
  `secret-service` 4.0.0 in `Cargo.lock` — prerequisite is a running
  Secret Service (e.g. gnome-keyring) on the session D-Bus. macOS leg:
  `apple-native` (app manifest :72); Windows: platform store. A
  headless box fails typed: `CredentialError::Backend` — by design
  (ADR 0008: the signal to set `credential_store = "env"`).
- No CI runner sets the gate: zero `TASKBOARD_IT_KEYRING` hits across
  `.github/workflows/` (all five files, re-verified); the hermetic
  siblings S1–S6 in the same file run in every CI pass.
- Why manual-by-design (the tier-3 philosophy, §8): it dirties a real
  user keyring — exactly what tier-3-manual exists for; CI has no
  desktop credential store and must never touch a user's.

### 2.4 F4-04 premise (stale comments vs today's wiring)

| Old claim | Today's truth |
|---|---|
| `config.rs:6–7`: "chunk 3 wires them from figment's `[reminders]` table" | The wiring landed long ago: `AppConfig::reminder_engine_config()` (`crates/taskboard-app/src/config.rs:659`, doc :649) maps figment's `[reminders]` onto this type; fed to `spawn_state_engine` at `crates/taskboard-app/src/bootstrap.rs:614`. No "chunk 3" exists as a live referent (historical 2026-10-13 dispatch chunk). |
| `config.rs:20–21`: `disabled()` "used by every pre-existing test helper **and the interim bootstrap wiring**" | The bootstrap never calls `disabled()`; call sites at 76838a1 are the state crate's own test suites (`tests/engine.rs:149,692`, `tests/reminders.rs:559`, `tests/snapshots.rs:70`, `tests/proptest.rs:454`, `tests/sqlite_smoke.rs:59`) plus the crate doc example (`src/lib.rs:51`). |
| `config.rs:21–24`: "production defaults … are figment's to encode in chunk 3 — deliberately not encoded here" | They are encoded now, in the app's figment defaults: `DEFAULT_REMINDER_TICK_SECS = 30` and `DEFAULT_REMINDER_BEFORE = "5m"` (`crates/taskboard-app/src/config.rs:56/58`, wired :235–236). The *design intent* ("not encoded here, no hidden production policy") remains true and stays. |

Untouched neighbor: the `tick` field doc `config.rs:13` ("production
default 30 s, design §5.4") is accurate and not part of the finding.

### 2.5 Wired-tooling bar (dispatch §14 acceptance: "name what is wired")

Any tool the rewritten sections name must appear here:

| Tool | Wired at |
|---|---|
| `cargo nextest` | `.github/workflows/ci.yml:162–166` (workspace lane), `:229/236` (coverage lane), `mutation-testing.yml`, `nextcloud-integration.yml:44–53` |
| `cargo-llvm-cov` (three-command quarantine sequence) | `ci.yml:229–249` |
| `cargo-deny` | `ci.yml:128–137` |
| Miri (weekly Mon 03:00 UTC + dispatch) | `miri.yml:8–10` |
| `cargo-mutants` (weekly Mon 04:00 UTC + dispatch) | `mutation-testing.yml:8–10,39–46` |
| `insta`, `proptest`, `wiremock`, `criterion` | root `Cargo.toml:102–114` (`[workspace.dependencies]`) |

The rewrite names none beyond these (it does not need to name any
tool at all beyond nextest in the runbook command).

## 3. Edit outlines

### 3.1 `docs/testing_strategy.org` §3 — replace the mockall bullet (F15-08)

Replace the second bullet (:46–49, "*Using =mockall=:* …") with one
bullet, same list position, along the lines of:

- *No mocking crate:* the workspace deliberately wires no mocking
  library (`mockall` appears in no manifest). Async ports are
  hand-rolled `BoxFuture` traits (`crates/taskboard-domain/src/persistence.rs`)
  precisely so a fake is just an `impl` of the trait — see
  `taskboard_domain::test_support::InMemoryRepository` for the
  repository fake, and the UI suite's `fake_engine` for a fake at a
  module boundary. Behavior is asserted through the port's typed
  results, never through recorded calls.

Rules for the stage-2 agent: keep the section's first bullet
(HashMap-in-`Arc<Mutex>` fake) untouched; do not fix §3-adjacent
drift owned by #268 (the §1 `Box<dyn>`/generics sentence :20–24, the
§2 `Command::TaskCompleted` example :34) — different issue, same
file, out of scope here; after the edit `grep -rn mockall docs/`
must return zero hits.

### 3.2 `docs/coding_guidelines.org` §7 — replace the merge subsection (F15-14)

Retitle "** Double-Rebase Merge into main" (:143) to "** Merging into
main (branch-protected PR flow)" and replace :143–153 with:

1. `main` is branch-protected on GitHub: every change enters through a
   PR that must pass its required CI checks; local merges into `main`
   are not available — the double-rebase procedure this section used
   to describe is no longer performable by anyone.
2. The flow: commits land on the feature branch (freely; house
   subject format above) → push only on explicit instruction → open
   the PR → `gh pr checks` green → hand off. **The user performs the
   merge on GitHub** (rebase-merge).
3. Rebase-merge consequence: commits land on `main` verbatim — every
   commit must stand alone (correct build, meaningful subject, no
   WIP/noise); corrections are amended/reworded before the PR opens,
   never follow-up commits with duplicate subjects.
4. Stacked PRs: after an upstream PR merges, descendant branches
   rebase onto `origin/main` dropping the already-merged commits and
   push `--force-with-lease`. The exact commands live in `AGENTS.md`
   §8 — the normative source; this section is the summary and must
   not drift from it.

Rules: leave "Git Usage Policy (for agents)" (:155–163) and "Release
Versioning" (:165–177) untouched (verified consistent today); do not
touch the foreign-example lines :110–115/:133 (F15-15, owned by
#269); do not add color-eyre material to §3 (#278, agenda row (d) —
that issue owns the error-handling section; this chunk's §7-only edit
must not collide with it); after the edit `grep -rn "double-rebase"
docs/` must return zero hits (the AGENTS.md §8 mention of
"double-rebase style" is a prohibition, in a file this chunk does not
touch).

### 3.3 `docs/testing_strategy.org` §8 — keyring runbook entry (F14-01)

New subsection at the end of §8, after "** App-level E2E
(binary-driving, tier-2 reuse)", titled "** Keyring round trip (local,
manual only)" — paralleling the tier-3 bullet's naming style. Content
outline (each step verified in §2.3 above):

1. *What & why:* `it_keyring_local_round_trip`
   (`crates/taskboard-app/tests/secrets.rs`) drives the production
   `KeyringStore` (ADR 0008) against the **developer's own** OS
   credential store — the only executed proof of the credential
   store's real OS I/O. It is tier-3 for the same reason tier 3 is:
   it dirties a real user keyring by design, and no CI runner sets
   its gate (`TASKBOARD_IT_KEYRING` appears in no workflow). The
   hermetic halves (redaction, error mapping, fake round trip:
   S1–S6) run in every CI pass.
2. *Prerequisite:* a running OS credential store in your session —
   Linux: a Secret Service (gnome-keyring; the app links
   `sync-secret-service`), macOS: Keychain, Windows: Credential
   Manager. Headless boxes fail typed (`CredentialError::Backend`) —
   by design; that is the signal to use `credential_store = "env"`.
3. *Invocation* (copy-paste, mirrors the tier-3 live style at §8):

   ```sh
   TASKBOARD_IT_KEYRING=1 cargo nextest run -p taskboard-app \
     --run-ignored only -E 'test(it_keyring_local_round_trip)'
   ```

4. *The vacuous-pass footgun:* the test is `#[ignore]`d **and**
   internally guarded — without `TASKBOARD_IT_KEYRING=1` it returns
   early and *passes having done nothing*. A green run without the
   gate exported proves nothing; always run the command exactly as
   above.
5. *What it writes:* one throwaway entry into your own store —
   service `taskboard`, account
   `roundtrip@keyring-test.example.com`, value the literal
   `local-secret`. It deletes the entry *before* asserting, so any
   test failure still self-cleans; only a process killed between
   store and delete leaves the entry behind — remove it with your
   desktop's credential manager (e.g. Seahorse on GNOME) looking up
   service `taskboard` / account `roundtrip@keyring-test.example.com`.
   (A `secret-tool` one-liner may be added only if the attribute
   names are proven from the `secret-service` 4.0 storage format —
   never invented.)

Rules: this entry documents the test as it exists — no test edits,
no `.env.example` edit (`TASKBOARD_IT_KEYRING` is a manual gate
flag, not a secret; §8's "secrets live in one place only" rule keeps
.env for actual secrets). The test's own doc comment
(`secrets.rs:124–125`, "run manually in the tier-3 runbook") becomes
true by this entry — no change needed there.

### 3.4 `crates/taskboard-state/src/config.rs` — comment corrections (F4-04)

Comment-only diff; zero code tokens change. Proposed wordings
(stage-2 may polish phrasing; facts are fixed by §2.4):

**Comment 1 — struct doc (:6–7).**

| | Text |
|---|---|
| Old | `Reminder scheduler knobs; chunk 3 wires them from figment's` / `[reminders]` table. Tests inject tiny/paused values.` |
| New | `Reminder scheduler knobs; taskboard-app maps figment's` / `[reminders]` table onto them (`AppConfig::reminder_engine_config`). Tests inject tiny/paused values.` |

**Comment 2 — `disabled()` doc (:20–24).**

| | Text |
|---|---|
| Old | "The off state used by every pre-existing test helper and the interim bootstrap wiring. The `defaults` value is irrelevant while disabled; production defaults (5 m before due) are figment's to encode in chunk 3 — deliberately not encoded here so this type carries no hidden production policy." |
| New | "The off state used by the state crate's test suites and doc example; the production wiring never calls it — it builds the config from figment's `[reminders]` table (`AppConfig::reminder_engine_config`). The `defaults` value is irrelevant while disabled; the production defaults (5 m before due, 30 s tick) live in the app's figment defaults — deliberately not encoded here so this type carries no hidden production policy." |

Formatting constraints for stage-2: mention `AppConfig` and friends in
plain backticks only — **never `[intra-doc link]` syntax**, which
cannot resolve across the app→state dependency edge and would risk
rustdoc warnings; keep lines wrapped to the file's existing width;
`AppConfig` and `figment`'s table names stay in backticks (clippy
pedantic `doc_markdown` runs at `-D warnings`). Verify the diff is
comment-only: `git diff` shows only `///`-line changes.

## 4. Decisions (settled here, binding for stage 2)

- **D1 — rewrite, not delete, the mockall paragraph** (#265's proposed
  approach; a positive statement of the real standard prevents the next
  contributor from re-adding a mocking crate).
- **D2 — §8 gets a dedicated subsection, not a fifth sync tier** — the
  four tiers describe the HTTP-client surface; the keyring test is a
  different manual-only surface in the same tier-3 *philosophy*
  (findings/issue wording: "document as tier-3 manual in §8"), so it
  takes a parallel `**`-level subsection, and its title carries the
  "(local, manual only)" marker to match tier 3's naming.
- **D3 — runbook cleanup instruction stays manager-agnostic**; a CLI
  one-liner is allowed only with proven attribute names (§3.3 step 5).
- **D4 — `.env.example` untouched** (§3.3 rules).
- **D5 — state comments use plain code-font mentions**, no cross-crate
  intra-doc links (§3.4 constraints).
- **D6 — no edits to AGENTS.md** — it is the normative source both
  rewrites point at; drift-fixing flows one way (docs → AGENTS
  reality).

## 5. Non-goals (explicit)

- **No tooling changes**: nothing is wired, unwired, or CI-added. The
  secretless `workflow_dispatch` keyring job (headless gnome-keyring +
  dbus-run-session, mirroring `nextcloud-integration.yml`) is
  **explicitly deferred** per #295's decision — the runbook variant is
  the whole remedy.
- **No new test tiers**; no edits to `tests/secrets.rs` (the runbook
  documents the test as-is; its doc comment already matches).
- **Neighboring findings in the same files stay out** (one issue per
  chunk — fixing them here would blur review and preempt their
  owners):
  - #268 (F15-09/10/11/12/13): testing_strategy §1 :20–24, §2 :34,
    §4 :58–60, §8 :237–239, §13 :460–461;
  - #269 (F15-15 etc.): coding_guidelines commit/branch examples
    :110–115/:133 and the ADR/CHANGELOG items;
  - #267 (F15-04…07): the criterion "task sorting" claim in
    testing_strategy §13 :462–463;
  - #278 (F8-07, agenda row (d)): the color-eyre entry-path paragraph
    destined for coding_guidelines §3 — this chunk's guidelines edit is
    §7-only precisely so the two never collide.
- **No AGENTS.md / architecture.org / ADR edits** (chunk 12 owns
  architecture.org; AGENTS is normative input, not an output).

## 6. Test & verification plan

Docs do not get new tests; verification is claim-audit + gates:

1. **Claim audit (stage-2 self-check, stage-3 re-check)** — every
   assertion in the three rewritten sections maps to a §2 pointer:
   zero `mockall`/`automock` in `docs/`; zero `double-rebase` in
   `docs/`; runbook facts vs `secrets.rs:124–140` + `secrets.rs:23/102/139/194`
   + app manifest :59–72; comment facts vs §2.4 table; no doc claim
   contradicts AGENTS.md.
2. **Comment-only proof**: `git diff main...HEAD -- crates/taskboard-state/src/config.rs`
   touches `///` lines only.
3. **Gates** (AGENTS §10, full list — the `.rs` comment change must
   clear clippy pedantic `-D warnings` incl. `doc_markdown`, and
   `cargo test --doc --workspace` must still compile the lib.rs
   doc example):

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo nextest run --workspace
   cargo test --doc --workspace
   cargo deny check
   cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
   scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
   cargo llvm-cov report --lcov --output-path lcov.info
   ```

4. **Runbook walk-through (recommended, cheap)**: on this dev box
   (Linux + Secret Service) run the §3.3 command once and confirm the
   documented behavior — entry appears and is gone afterwards
   (Seahorse/`secret-tool search` before cleanup lands if inspected
   mid-run); if no desktop keyring is available in the planning
   session, mark the step "verified against test source only" in the
   stage-2 report rather than claiming a live run.

## 7. Commit plan (stage 2; one commit per finding, per dispatch §14)

1. `docs(testing): state the handwritten-fakes standard, drop mockall guidance` (F15-08, #265)
2. `docs(guidelines): replace double-rebase with the protected-main PR flow` (F15-14, #265)
3. `docs(testing): add the keyring tier-3 manual runbook to section 8` (F14-01, #295)
4. `docs(state): correct the stale reminders-config doc comments` (F4-04, #246)

Each commit stands alone (rebase-merge hygiene); the stage-1 plan
commit precedes them on the branch. PR body: `Closes #265` +
`Closes #295` + `Closes #246`.

## 8. Risks

- **Adjacent-scope creep** (the tempting #268/#269 lines sit in the
  same sections): mitigated by §5's explicit line lists and the §3
  post-edit greps; stage-3 checks the diff touches only the four
  agreed regions.
- **Invented runbook steps** — the classic docs-runbook failure: every
  step in §3.3 cites test source (§2.3); the one unverifiable nicety
  (secret-tool flags) is banned unless proven (D3); the vacuous-pass
  warning exists because `--run-ignored only` alone green-lies.
- **Clippy/rustdoc lint trips from the new comment text** (`doc_markdown`,
  cross-crate links): mitigated by §3.4's constraints; caught by gates.
- **Chunk-1 merge-order note**: this PR must not merge before chunk 1
  (#302 rename) per dispatch §1; no file overlap is expected (this
  chunk's `.rs` file names no commands), so a rebase, if needed, is
  trivial.
- **The keyring facts rot silently** (test constants, feature flags):
  accepted — the runbook names the test and the service constant, both
  greppable; a future test change that invalidates a step is a docs
  drift the next audit pass catches (same class as the finding being
  fixed, but now with a single canonical place to fix).

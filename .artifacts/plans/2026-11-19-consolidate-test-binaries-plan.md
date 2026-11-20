# Test Binary Consolidation — Chunk R4 Plan (round 3, #1070)

Branch: `refactor/consolidate-test-binaries`. Scope report §A (normative),
dispatch plan §1/§5, issue #1070, runtime report §A. Status: **GATE-PAUSED —
the binary map below is the reviewable artifact awaiting dispatcher
confirmation before stage 2 runs.**

## 1. Goal & laws

Merge integration-test binaries so the duplicated link work (every app/ui-slint
test binary statically links slint + the full graph; 1471s of the 2104s
`--all-targets` CPU on the pre-lld baseline) shrinks toward the ~300–400s band.
Binding laws:

- **(c) Zero test-case renames.** Test *functions* keep their names verbatim;
  only binaries group. Any gate/tooling path that moves is updated in the same
  change.
- Nextest name filtering must keep working (smoke: `cargo nextest list` +
  a named-filter resolution after the merge).
- Harness distinctions survive merges (the #1016 DriverOptions OnceLock mix is
  the precedent for keeping a split — here encoded as the headless_renderer
  partition below).
- Diffs confined to: test-file moves/merges, tooling-path updates, doc
  updates. **No `[[test]]` Cargo.toml edits needed** — no crate declares
  explicit test targets; auto-discovery covers the new top-level root files,
  and moved subdirectory files are not auto-targets.

## 2. Baseline re-derivation (stage 2, before any move)

The runtime report's numbers pre-date R1's lld landing (`.cargo/config.toml`
now routes dev/test links through lld). Stage 2 re-measures the *before*
state on this branch with an identical method, then re-measures after:

```sh
scripts/memory-gate.sh /usr/bin/time -v cargo build --workspace --all-targets
```

backgrounded + polled, deps warm, run twice (discard the first cold pass),
recording wall + CPU + per-unit times from `--timings` (or a second run with
`cargo build --timings`). The delta is reported against the re-derived
baseline, not the report's pre-lld numbers.

## 3. The binary map (reviewable artifact)

### 3.1 taskboard-ui-slint: 31 → 6 targets

Grouping axis 1: **DriverOptions partition** — `tests/support/driver.rs::run`
asserts one platform per process ("options are per test binary"); a merged
binary must agree on `headless_renderer`. Grouping axis 2: surface family.

| New target (tests/<name>.rs) | Members (moved under tests/probes/, tests/journeys/, tests/inventory/ respectively) | headless | Why grouped |
|---|---|---|---|
| `journeys.rs` | **unchanged** (395 test fns) | true | Evidence hub: 282 `.artifacts` citations + census rows name `journeys.rs`; already one binary; keeping it untouched keeps every existing citation valid |
| `journey_suites.rs` | `focus_law_journeys.rs`, `board_move_journeys.rs`, `habits_journey.rs`, `golden_visuals.rs`, `keymap_case_seam.rs`, `usability_components.rs`, `focus_coverage.rs` | true (all 7 agree) | Journey-driver suites sharing `mod support` + driver; focus_coverage drives `focus_coverage_probe.slint` (same include shape) |
| `probe_composition.rs` | 11 renderer-on probes: `action_menu_bar`, `board_bar`, `dropdown`, `editor_viewport`, `focus_seam`, `overlay_migration`, `plan_window_max`, `slider_grab`, `walk`, `widget_framework`, `window_max` `*_probe_tests.rs` **plus** `virtualisation.rs` | false (all 12 agree) | All geometry/composition probes; `virtualisation.rs` already uses the headless:false option and the probe include shape (#373) — it is a probe suite in all but filename |
| `probe_pixel.rs` | `icon_probe_tests.rs`, `stat_bars_severing_probe_tests.rs`, `task_card_hover_probe_tests.rs`, `task_card_title_probe_tests.rs`, `tooltip_layout_probe_tests.rs` | true (all 5 agree) | Pixel/golden-compare probes (`golden_compare` harness); must stay in the software-renderer group |
| `structural.rs` | `seam_inventory.rs`, `view_mount_inventory.rs`, `keymap_snapshots.rs`, `icon_mapping.rs`, `bench_fixture_contract.rs` | n/a — no platform init | No `support::driver` use; pure data/structural assertions; still each paid a full slint link as standalone binaries |
| `pin_guard.rs` | **unchanged** | n/a | Kept separate deliberately: `docs/adr/0015` §2 and `docs/testing_strategy.org` §200 cite `tests/pin_guard.rs` normatively ("must stay green"); one tiny binary; avoids ADR churn for zero real link savings |

Mechanics: each new root file is a thin root declaring
`mod <member>;` per member; member files `git mv` into
`tests/probes/`, `tests/journeys/`, `tests/inventory/` (history preserved;
subdirectories are not auto-discovered as targets). The generated probe
windows (`include!(OUT_DIR/<probe>.rs)`) have probe-unique component names —
no include collisions across 12 members in one binary (verified as a stage-2
compile gate).

### 3.2 taskboard-app: 34 → 8 targets

Grouping axis: CLI verb family; harness distinctions preserved.

| New target | Members (moved under tests/cli/, tests/core/, tests/services/) | Why |
|---|---|---|
| `cli_sync.rs` | `cli_attach.rs`, `cli_breaks.rs`, `cli_connect.rs`, `cli_standalone.rs`, `cli_verbs.rs` | boards/sync verb family (wiremock ceremonies) |
| `cli_tasks.rs` | `cli_editing.rs`, `cli_undo.rs`, `cli_journal.rs`, `cli_track.rs`, `cli_timers.rs`, `cli_reminders.rs`, `cli_repeat.rs`, `cli_habits.rs` | task-lifecycle verb family |
| `cli_output.rs` | `cli_output.rs`, `cli_tty.rs`, `cli_export_pdf.rs`, `cli_lock.rs`, `cli_login.rs`, `cli_ipc.rs` | output/tty/export/lock/login/ipc-pipe family |
| `app_core.rs` | `bootstrap.rs`, `config.rs`, `daemon_signals.rs`, `dispatch_routing.rs`, `lockfile.rs`, `repl.rs`, `secrets.rs` | boot/config/runtime core |
| `app_services.rs` | `ipc.rs`, `files_sync_convergence.rs`, `ui_settings.rs`, `ui_live_settings.rs`, `web_dashboard.rs` | long-running surfaces |
| `it_cli.rs` | **unchanged** | black-box CLI over the real binary; ADR 0009 cites it by name |
| `it_docker.rs` | **unchanged** | `#[ignore]` docker tier-2 suite |
| `mcp_smoke.rs` | **unchanged** | CI names `--test mcp_smoke` verbatim (`.github/workflows/ci.yml:273`) — moving it would require a CI edit; kept for zero-CI-diff |

Env/cwd locks (`tests/common`) are per-process mutexes — merging is
behavior-neutral under both cargo test (serialized) and nextest
(process-per-test). `tests/common/mod.rs` is shared as-is.

### 3.3 Out of scope (recorded decision)

state (12), storage-sqlite (9+2), sync-nextcloud (7), sync-files-nextcloud
(4), domain (1) binaries are **not** consolidated in this chunk: the dispatch
plan's covers-clause names only the ui-slint probe pairs and the app `cli_*`
family, and those crates' closures are cheap (no slint link). If the
re-measured share justifies it, a follow-up `backlog:` issue carries it.

**Post-map count: 98 → 47 binaries** (ui-slint −25, app −26).

## 4. Tooling-path update list (same change, law c)

Moved basenames referenced by gate/tooling surfaces. Stage 2 runs a
word-boundary `grep -rl` per old name and updates exactly these classes:

1. **bound-test / nomination lines** in committed journey plans the
   ux-coverage gate consumes (`.artifacts/plans/2026-11-18-*ux-plan.md` +
   any plan carrying `bound-test:` or §4 nominations naming a moved file):
   `widget_framework_probe_tests.rs` (6 refs), `habits_journey.rs` (24),
   `focus_law_journeys.rs` (5), `board_move_journeys.rs` (1),
   `overlay_migration_probe_tests.rs` (2), `action_menu_bar_probe_tests.rs`
   (2), `cli_standalone.rs` (3). Full precise list is re-derived at stage 2.
2. **docs**: `docs/ui_testing_instrumentation.org` §55 (the per-probe
   `tests/<name>_probe_tests.rs` convention → grouped-binary description);
   any `docs/*.org` citing a moved basename (`ux_planning.org`, census rows).
   `pin_guard`, `it_cli`, `it_docker`, `mcp_smoke`, `journeys.rs` citations
   need no edit.
3. **scripts**: `scripts/ux-coverage.sh` — no moved-name references (evidence
   resolution is basename + fn-boundary grep; the self-test fixture tree uses
   its own miniature names) — no change. `tests/seam_inventory.rs` reads
   `../src/adapter.rs` via `include_str!` — path unchanged by the move into
   `tests/inventory/`? **Decision: seam_inventory.rs moves to
   `tests/inventory/seam_inventory.rs`; `include_str!("../src/adapter.rs")`
   is relative to the including file → becomes
   `include_str!("../../src/adapter.rs")`** — a one-line mechanical fix in
   the same change.
4. **ci.yml**: no change (mcp_smoke untouched).

**Decision on historical prose**: dated `.artifacts/plans/*.md` prose that
merely narrates past evidence (`secrets.rs:123`, `repl.rs` mentions etc.) is
historical record, not a tooling path — not updated; only gate-consumable
structures (bound-test lines, §4 nomination tables) and normative docs are.
This keeps the churn bounded (~15–25 files) instead of ~300.

## 5. Measurement plan

1. **Before** (stage 2, pre-move): re-derived `--all-targets` wall/CPU +
   per-unit table (§2 method); `cargo nextest list --workspace | sort` saved
   as the test-name-set baseline.
2. **After**: identical method; delta reported vs the re-derived baseline and
   the ~300–400s band.
3. **Name-set identity**: `diff <(before list) <(after list)` — must be
   empty (zero renames, law c).
4. **Nextest filtering smoke**: `cargo nextest run -E 'test(us1_s1)'` and one
   probe-filter + one cli-filter resolve to the same named tests as before.
5. **Gate**: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
   run --workspace` (flock-safe; R4's lane is exclusive; measurements route
   through `scripts/memory-gate.sh`, backgrounded + polled; all landed work
   committed before any wait).

## 6. Risks & mitigations

| Risk | Mitigation |
|---|---|
| DriverOptions mix recreates #1016 (`assert_eq` fires under plain `cargo test --test <merged>`) | Map partitions strictly on headless_renderer; stage 2 runs each merged target under plain `cargo test --test <name>` once |
| Probe include collisions (12 generated modules in one binary) | Component names are probe-unique; compile is an early stage-2 gate |
| ux-coverage INVENTORY-MISS after moves | §4 update list applied in the same commit series; gate re-run on affected journey plans |
| Larger single binaries slow targeted rebuilds | journeys.rs kept separate (largest suite untouched); merged groups are stable low-churn suites |
| `#[ignore]`/docker semantics drift in merges | it_docker/mcp_smoke/it_cli kept separate; no ignore-attributes touched |

## 7. Open questions — resolved decisions

- **Explicit `[[test]]` sections?** None exist; auto-discovery covers the map
  (recorded §1). No Cargo.toml edits.
- **Merge journeys.rs too?** No (evidence hub + rebuild locality, §3.1).
- **Rename merged targets with `_tests` suffix?** No — house style names
  integration files descriptively without the suffix (journeys.rs,
  golden_visuals.rs precedent); the moved member filenames keep their names
  verbatim.
- **state/sqlite/sync consolidation?** Out of scope (§3.3); follow-up issue.
- **Historical .artifacts prose paths?** Not updated (§4 decision).
- **virtualisation.rs placement?** Into `probe_composition.rs` — it matches
  the probe include shape + headless:false (§3.1).

## 8. Estimated diff size

~31 ui-slint files moved (git mv) into 3 subdirs + 4 new root files;
~29 app files moved into 3 subdirs + 5 new root files; ~15–25 tooling/doc
updates; total ≈ 80 files, of which the substantive new content is ~9 thin
root files (the members' bodies are verbatim moves; only `mod common;` /
`mod support;` paths and one `include_str!` depth adjust mechanically).

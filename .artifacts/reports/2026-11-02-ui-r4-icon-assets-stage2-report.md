# [Report] UI R4 chunk 3 — icon assets, stage 2 (implementation)

- **Date**: 2026-11-02
- **Branch**: `feat/ui-r4-icon-assets` (worktree
  `../taskboard-WT-feat-ui-r4-icon-assets`), 5 commits on top of the
  stage-1 plan commit `f3fc7f80`, main untouched, nothing pushed.
- **Plan**: `.artifacts/plans/2026-11-01-ui-r4-icon-assets-plan.md`
  (followed; deviations listed in §6).

## 1. Commit series (each standalone, pre-commit gates green per commit)

| # | Commit | Content |
|---|---|---|
| 1 | `456c5cbe` test(ui-slint): the icon colorize probe | probe composition + build.rs registration + `check.svg` + probe tests (P1–P5) + committed probe golden |
| 2 | `a809467c` chore(ui-slint): vendor the lucide icon set | 18 further SVGs + `LICENSE-lucide` + the probe kinds wall (P6) + regenerated probe golden |
| 3 | `8f127899` feat(ui-slint): icons render vendored svgs via colorize | `icon.slint` (enum + `Icon` + total mapping), `Theme.icon-size`, the icon-button swap, G3 successor, the structural totality test, all 14 goldens regenerated |
| 4 | `219e75f6` feat(ui-slint): distinct dismiss and done notification icons | the six growth kinds + mapping arms, notification dismiss → `IconKind.dismiss`, the notification-surface golden |
| 5 | `198da5ff` docs(ui-slint): d19 amended to vendored svg icons | status-bar stop square → `Icon`, the architecture.org amendment home, CHANGELOG Unreleased |

## 2. The probe evidence (R2 gate — passed)

The software-renderer probe (`tests/icon_probe_tests.rs`, 6 tests,
all green) proves `Image.colorize` tints a vendored SVG at runtime:

- **Tone ladder (P1)**: all 8 `Tone` map outputs render glyph pixels
  of their exact tint on `Theme.background` — measured counts e.g.
  8 tint-weight pixels (12 px check, text-secondary) up to 123
  (36 px); every semantic tint (success/warning/danger/overdue/
  sync-active/offline) renders.
- **Hover-lift pair on both surfaces (P2)**: text-secondary and
  text-primary carry glyph pixels on `Theme.background` and on
  `Theme.surface-raised`.
- **Crispness extremes (P3)**: ink present at 0.5× and 1.0× the icon
  edge; the 3.0× cell has full-tint stroke cores (76 exact-tint
  pixels in the 36 px cell) — the rasterizer is crisp at the scale
  extreme. The 0.5× finding is §5's Tier-4 item.
- **Button-box shape (P4)**: the inlined post-swap icon-box renders
  the glyph in its padded interior.
- **Determinism (P5)**: committed golden
  `tests/goldens/icon-probe-1.0-396.png` byte-stable across runs
  (compare mode green).
- **Kinds wall (P6)**: every one of the 19 vendored assets decodes
  and tints (one cell per file; the only predicate adjustment: the
  hairline `minus.svg` needs the ink floor, not the
  closer-to-tint-than-background test — a 50 %-coverage blend is
  equidistant).
- **Renderer #2 (GL)**: not runnable in this environment — the
  `mcp_smoke` CI lane (femtovg/GL + resvg, `ci.yml:199–216`) is the
  GL-side decode+tint evidence; **stage 3 must confirm a green
  `mcp_smoke` run on the PR** (plan §3's gate: probe green software ∧
  mcp_smoke green GL). Assets are compile-time embedded
  (`@image-url`); no runtime loading exists for stage 3's
  no-runtime-assets grep.

## 3. Per-gate results (final run: `TZ=UTC scripts/run-quality-gates.sh`, exit green, "run-quality-gates: full suite passed")

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean (also enforced by every commit's pre-commit hook) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (idem) |
| `cargo nextest run --workspace` | **2758/2758 passed**, 41 skipped |
| `cargo test --doc --workspace` | green |
| `cargo deny check` | green |
| coverage (llvm-cov nextest + quarantine + lcov) | green — "all 3107 profraw files valid", `lcov.info` written |
| slint token lint (pre-commit) | clean per commit |

**The TZ=UTC note**: the first gate run failed in
`taskboard-app::cli_habits h02_log_receipts_...` — the known
local-vs-UTC calendar-window flake (#560): the run started 23:5x UTC /
00:0x local CET, so the local date was ahead of the UTC date mid-run.
Reproduced standalone, recorded as a watchlist comment on #560
(comment 6565829375), and the suite rerun under `TZ=UTC` (the CI
environment) — green. No production code was touched by this chunk
that could plausibly interact; the failure is environmental and
pre-existing.

## 3b. Commit-level evidence

Each of the 5 commits passed the pre-commit hook chain (fmt check,
slint token lint, workspace clippy `-D warnings`) at commit time; the
icon probe suite was green before the swap commit landed (the R2
order), and the journeys suite ran green unchanged (143/143) straight
after the swap — the no-API-churn evidence.

## 4. Binary size delta (dispatch prompt 3.2)

| Build | Bytes |
|---|---|
| before (HEAD `f3fc7f80` + plan) | 45,437,136 |
| after (commit 5) | 46,625,136 |
| **delta** | **+1,188,000 (≈ +1.13 MiB)** |

The plan expected ≈ +10–20 KB. The measured delta is dominated by
**code, not assets**: the vendored SVGs total ≈7 KB, but this is the
first release link that needs SVG decoding, so the resvg/usvg
decoder stack (present in the lockfile for the testing backend only)
now links into `taskboard-app`. That is the honest cost of the
confirmed decision; the plan's estimate missed the decoder linkage.
Memory: assets are compile-time embedded, decoded on demand — no
additional resident cost beyond rendered icon caches.

## 5. Tier-4 items (dispatch prompt 3.2)

- **Scale 0.5 (6 px glyphs, 0.5 px strokes — the kiosk panel)**:
  glyphs render ink but the strokes are sub-pixel — the probe's 6 px
  cell carries only faint blended pixels (the exact-tint run count is
  0 at that scale). The kiosk-today-0.5 golden shows the outline
  clock/bell still legible; the toolbar hamburger is noticeably
  fainter than the old 1 px drawn bars (old strokes were 1 px solid,
  Lucide's are 2/24 of the edge = 0.5 px at scale 0.5). **Finding,
  not fixed here** (assets are verbatim upstream): if the kiosk
  legibility is judged insufficient, the follow-up is a recorded
  theme decision (e.g. a larger `icon-size` floor at low scales) —
  not asset edits.
- **Scale 3.0 (36 px)**: crisp — 76 exact-tint core pixels in the
  probe's 3.0× cell; goldens at 3.0 show clean outlines.
- **Neutral hover lift (text-secondary → text-primary)**: P2 proves
  both tints render on both surfaces; the G3 successor pins the
  geometry at all five geometries. The lift reads on the Lucide
  stroke weight (both tints differ from both backgrounds by far more
  than the antialiasing band).
- **Status-bar stop square**: filled 10 px square → outlined 12 px
  `IconKind.stop` glyph, pinned by the regenerated board/today/kiosk
  goldens (visible bottom-right chip; compare `today-3.0-1000.png`).
- **Optical centring (plan §2.3)**: the fixed-size `Icon` centres in
  the padded box with no top-bias — verified against the regenerated
  goldens (toolbar menu sits centred at 0.5 and 3.0); no centring
  wrapper needed.

## 6. Deviations from the plan (all recorded, none silent)

1. **Probe composition rides theme tokens** (the slint token lint has
   no waivers): the probe's literal px grid is expressed as
   arithmetic over `Theme` tokens (base-scale 1.0 ⇒ identical
   geometry); the Rust test mirrors the base-px constants.
2. **Probe cell predicates**: the plan's "non-uniform runs of the
   expected tint" is implemented as two floor predicates —
   tint-weighted pixels (closer to tint than to background) and an
   ink floor (max-channel delta ≥ 12) for the sub-pixel cells — after
   measured evidence showed 50 %-coverage blends are equidistant and
   0.5 px strokes never reach tint-majority. Floor-only, per the
   no-ceilings rule.
3. **G3 successor expected run**: the plan guessed "≈ `Theme.icon-size`
   ±1 px"; the Lucide menu bar is 16/24 of the edge plus one stroke
   width of round caps — measured 8 px at scale 1.0, 26 px at 3.0.
   The successor pins that measured token-derived value (±1 px, same
   five geometries); the 0.5 cell pins ink presence (sub-pixel
   strokes have no full-coverage run — the Tier-4 finding).
4. **`unlink.svg`/`timer.svg` availability**: both exist at tag
   1.66.0 — no fallback fired (plan §1 rows 15/17).
5. **Totality test sequencing**: assertion (iii) (no dead vendored
   files) would be red between commits 2 and 3 by construction (19
   files, 13 rows); commit 3 pins the six growth files as the exact
   pending set, commit 4 removes the exception — the test is strict
   from commit 4 on.
6. **Notification-golden fixture determinism**: the notification
   row's occurrence IS the derived effective occurrence (=
   the task's due), so one instant feeds both the due phrase and the
   "fired" phrase. The fixture pins the task due to yesterday's
   midnight (due phrases render date-only; the fired phrase sits in
   the day bucket all day: elapsed ∈ [24 h, 48 h) ⇒ "fired 1d ago")
   and the alarm to −30 min (minute-bucket stable, the today
   fixture's protocol). A −30 min due (the plan's implied shape)
   bakes a wall-clock "· HH:MM" suffix into the today row — rejected.
7. **`Icon` import in status-bar**: compositions import `Icon` from
   `icon.slint` directly (icon-button re-exports only `IconKind`, the
   call-site API). One import line each; no cycles.
8. **mcp_smoke GL evidence**: pending stage 3 (§2 above).

## 7. Gate suite output

See §3 — final line of the successful run:
`run-quality-gates: full suite passed` (the wrapper holds the §10
flock for the whole sequence; the first attempt's failure and the TZ
note are in §3).

## 8. Acceptance greps (dispatch §4 (b), plan §7.6)

- `grep -rn "if (root.icon ==" crates/taskboard-ui-slint/ui/` → empty.
- `grep -rn "source:" crates/taskboard-ui-slint/ui/ | grep -v
  @image-url` → one hit: `task-card.slint:48 dragging-source:` — a
  bool drag-state property, not an Image source. No runtime-loaded
  images anywhere in ui/.

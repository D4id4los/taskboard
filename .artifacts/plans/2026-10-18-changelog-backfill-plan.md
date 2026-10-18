# [Plan] Changelog Backfill & Release-Cut Guard — Chunk 5, Remediation Follow-ups

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 5 of the remediation-followups
  dispatch (`.artifacts/plans/2026-10-18-remediation-followups-dispatch-plan.md`
  §1/§6, prompt 5.1). Planning only — no implementation in this commit.
- **Origin (normative)**: issue **#330** (behavior fixes since v0.1.0
  lack `[Unreleased]` entries; suggested fix: one pass over the commits
  since `v0.1.0` + a release-cut checklist step that greps the commit
  range against the changelog section) — recorded from
  `.artifacts/plans/2026-10-18-arch-org-release-plan.md` §risks
  (bookkeeping item). The dispatch §6 known-missing list (verified
  2026-10-18) is binding as a *lower bound*: stale-binding re-resolve
  (#258), userinfo reject/strip + cleartext warning (#260/#261), typed
  daemon-shutdown tail (#257), subsumed-label fix (#310), release
  overflow-checks (#303), plus every behavior fix this round's chunks
  1–4 land on the branch below this chunk.
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups` (shared single-PR
  round; worktree exists — no recreation, no doctor run needed).
  Sweep base: `v0.1.0` = `a84c421`; sweep head: `999757f` (this plan's
  parent state). 165 commits in the range, no merges.
- **Size**: S (dispatch grade). Docs-only deliverable: `CHANGELOG.md`
  + one release-doc file.

---

## §shape — the two deliverables (and only these)

1. **`CHANGELOG.md` `[Unreleased]` backfill** — nine new entries (7
   under a new `### Fixed` block, 2 appended to the existing
   `### Changed` block), covering the 12 behavior commits the sweep
   judged user-visible. Verbatim text in §entries.
2. **Release-cut checklist step** — one paragraph + one shell block
   in `docs/coding_guidelines.org` §7 "Release Versioning" (the
   release docs PR #332 landed): grep the range since the last tag,
   match every behavior commit against `[Unreleased]`. Verbatim text
   in §checklist.

Nothing else. No code, no script changes, no version bump, no
rewriting of the released `0.1.0` section.

## §sweep — the normative sweep table (v0.1.0..HEAD, 165 commits)

Legend:

- **→ F<n> / → C<n>** — user-visible; entry added by this chunk (text
  in §entries; Fixed entries F1–F7, Changed entries C1–C2).
- **present** — already represented in the current `[Unreleased]`
  section (present-A1 board editing #132/#171/#177; present-A2 GUI
  stack creation #304; present-C1 bare-launch desktop UI #236;
  present-C2 EngineEnvelope rename #302).
- **n/u** — not user-visible, with the why. House rules applied:
  docs-only changes never enter the changelog (established practice,
  recorded in the arch-org-release plan §changelog.4); test-only,
  CI-only, and behavior-preserving refactors are not product
  behavior; release/dev tooling follows the PR #332 precedent (that
  chunk landed with only the #236 product entry).

| # | commit | subject | verdict |
|----|--------|---------|---------|
| 1 | `1e02987` | docs(plan): miri cap raise probe | n/u — docs artefact |
| 2 | `6fe9502` | ci(workflows): miri — raise proptest caps | n/u — CI/test-infra only |
| 3 | `1aefe96` | docs(testing): record the miri cap probe | n/u — docs artefact |
| 4 | `fa987c4` | docs(backlog): miri cap probe done | n/u — docs artefact |
| 5 | `e5ca3e9` | docs(plan): audit baseline metrics scope | n/u — docs artefact |
| 6 | `69827a7` | docs(report): audit baseline metrics | n/u — docs artefact |
| 7 | `aa541e3` | docs(plan): audit flow model scope | n/u — docs artefact |
| 8 | `1a96229` | docs(report): audit phase 1 flow model | n/u — docs artefact |
| 9 | `12a3da1` | docs(plan): audit domain crate scope | n/u — docs artefact |
| 10 | `0eed1cb` | docs(report): audit domain crate findings | n/u — docs artefact |
| 11 | `9a076a7` | docs(plan): audit state crate scope | n/u — docs artefact |
| 12 | `9ae9951` | docs(report): audit state crate | n/u — docs artefact |
| 13 | `95ab582` | docs(plan): audit storage crate scope | n/u — docs artefact |
| 14 | `fafc71e` | docs(report): audit storage crate findings | n/u — docs artefact |
| 15 | `b7f39f7` | docs(plan): kiosk daemon + GUI control design | n/u — docs artefact |
| 16 | `67ba52e` | docs(backlog): dolphin/kiosk/release-guard ideas | n/u — docs artefact |
| 17 | `adc9dde` | docs(plan): accepted kiosk wave-1 decisions | n/u — docs artefact |
| 18 | `5e34a4e` | docs(plan): kiosk wave-1 implementation plan | n/u — docs artefact |
| 19 | `ebde356` | docs(plan): audit sync crate scope | n/u — docs artefact |
| 20 | `44fa1f7` | docs(report): sync crate audited, verdict O1 | n/u — docs artefact |
| 21 | `062ae65` | docs(plan): audit ui crate scope | n/u — docs artefact |
| 22 | `a345652` | docs(report): audit ui-slint crate + axis O | n/u — docs artefact |
| 23 | `0b3c7f0` | docs(plan): audit app crate scope | n/u — docs artefact |
| 24 | `8d7e8a8` | docs(report): audit app crate findings | n/u — docs artefact |
| 25 | `ffdb63f` | docs(plan): audit concurrency theme scope | n/u — docs artefact |
| 26 | `2a1846c` | docs(report): audit concurrency findings | n/u — docs artefact |
| 27 | `1a09102` | docs(plan): audit sync persistence scope | n/u — docs artefact |
| 28 | `149c66f` | docs(report): audit sync persistence findings | n/u — docs artefact |
| 29 | `c4bbcc1` | docs(plan): audit time DST theme scope | n/u — docs artefact |
| 30 | `729ce0d` | docs(report): time, clocks & DST stage-2 | n/u — docs artefact |
| 31 | `ab5a9b5` | docs(plan): audit security theme scope | n/u — docs artefact |
| 32 | `93ba464` | docs(report): audit security findings | n/u — docs artefact |
| 33 | `42fc322` | docs(plan): audit test quality scope | n/u — docs artefact |
| 34 | `6d29155` | docs(report): audit test quality synthesis | n/u — docs artefact |
| 35 | `9e7eaaf` | docs(plan): audit perf budgets scope | n/u — docs artefact |
| 36 | `4cf1349` | docs(report): audit perf budgets findings | n/u — docs artefact |
| 37 | `fd92c57` | docs(plan): audit arch fit scope | n/u — docs artefact |
| 38 | `ae78dc0` | docs(report): audit arch fit walkthroughs | n/u — docs artefact |
| 39 | `9e5c38c` | docs(plan): audit docs drift scope | n/u — docs artefact |
| 40 | `fa9f187` | docs(report): audit docs drift findings | n/u — docs artefact |
| 41 | `070241e` | docs(backlog): e2e_hermetic flake watchlist | n/u — docs artefact |
| 42 | `9eaa2d6` | docs(backlog): backlog migrated to GitHub Issues | n/u — docs artefact |
| 43 | `092246f` | docs(plan): audit phase4 synthesis scope | n/u — docs artefact |
| 44 | `c14cc3f` | docs(report): audit findings + decision agenda | n/u — docs artefact |
| 45 | `e474efe` | docs(report): audit session outcomes | n/u — docs artefact |
| 46 | `7b31c0b` | docs(plan): remediation-wave-1 dispatch | n/u — docs artefact |
| 47 | `dd32c76` | docs(plan): enginecommand envelope rename scope | n/u — docs artefact |
| 48 | `d3eb1d1` | refactor(state): rename EngineCommand to EngineEnvelope | present (present-C2, #302) |
| 49 | `7449c1a` | docs: sweep EngineCommand prose | n/u — docs artefact (companion of the covered rename) |
| 50 | `09ec315` | docs(plan): sync push/pull pinning catalogue | n/u — docs artefact |
| 51 | `a834263` | test(sync): pin defer/same-stack reorder arms | n/u — test-only |
| 52 | `4a74d60` | test(sync): pin pull dead-board arms | n/u — test-only |
| 53 | `a0b1999` | fix(sync): stop double-emitting subsumed label outcomes | → F2 |
| 54 | `36047ca` | test(sync): pin subsumed outcome resolvability | n/u — test-only |
| 55 | `67cb63b` | docs(report): label-rejection shadowing analysis | n/u — docs artefact |
| 56 | `d4a9cda` | chore(deny): ignore unmaintained zbus advisories | n/u — dev/CI advisory posture; runtime deps unchanged (#311 stays the revisit anchor) |
| 57 | `9f1d720` | refactor(sync): name forbidden op classes | n/u — refactor (type-level naming) |
| 58 | `8dab663` | test(sync-nextcloud): bound wait helpers | n/u — test-only |
| 59 | `299f4cf` | test(sync-nextcloud): refused-port precondition | n/u — test-only |
| 60 | `0aeb0bf` | fix(app): REPL history via own 0600 writer | → F6 |
| 61 | `bf1f33a` | docs(agents): coverage gate sequence | n/u — docs artefact |
| 62 | `20dd5b8` | docs(report,plan): flaky watchlist investigation | n/u — docs artefact |
| 63 | `39d2a77` | docs(plan): flaky remediation accepted | n/u — docs artefact |
| 64 | `2884a6a` | docs(report,plan): #289 root cause corrected | n/u — docs artefact (the fix itself is row 60) |
| 65 | `e278a75` | docs(plan): domain gaps + overflow scope | n/u — docs artefact |
| 66 | `1193411` | test(domain): board RemoteMissing/echo arms | n/u — test-only |
| 67 | `bf3a5e1` | test(domain): archived edit/guard conjunctions | n/u — test-only |
| 68 | `c042a1d` | test(domain): apply_sync_report differentials | n/u — test-only |
| 69 | `56a0a4e` | test(domain): intent-quartet arms/boundaries | n/u — test-only |
| 70 | `70eb95a` | test(domain): adversarial proptests | n/u — test-only |
| 71 | `f0dee2f` | chore(workspace): overflow-checks in release profile | → C2 |
| 72 | `913d89a` | docs(plan): state & storage test gap catalogue | n/u — docs artefact |
| 73 | `8f4e959` | test(state): ingestion-equality negatives | n/u — test-only |
| 74 | `380cdb3` | test(storage): sync-phase rejection matrix | n/u — test-only |
| 75 | `d8a1585` | test(storage): :memory: dataset-identity probe | n/u — test-only |
| 76 | `71a9f54` | docs(plan): stale-binding re-resolve scope | n/u — docs artefact |
| 77 | `0297fee` | fix(sync): re-resolve stale-binding fetch 404s | → F1 |
| 78 | `29b1acb` | refactor(sync): inject Clock seam (#259) | n/u — refactor (test seam; production SystemClock reads the identical wall source, no behavior change) |
| 79 | `bf09eca` | docs(adr): ADR 0007 amendments | n/u — docs artefact (policy covered by F1's ADR mention) |
| 80 | `2a4609d` | docs(plan): daemon shutdown typed path scope | n/u — docs artefact |
| 81 | `e7a49ff` | refactor(app): extract daemon shutdown tail | n/u — refactor (pure move; expect carried verbatim, replaced by row 82) |
| 82 | `6070f90` | fix(app): typed failure path at daemon shutdown | → F3 |
| 83 | `6b3e7b3` | docs(plan): security hardening scope | n/u — docs artefact |
| 84 | `7ce3463` | fix(storage): owner-only db/wal/shm modes | → F5 |
| 85 | `373b3ef` | fix(app): owner-only lock sidecar | → F5 |
| 86 | `a809c88` | fix(app): owner-only config overlay writes | → F5 |
| 87 | `200e455` | fix(sync-nextcloud): reject userinfo base urls | → F4 |
| 88 | `96c68c8` | fix(app): reject userinfo server urls in config | → F4 |
| 89 | `53b533c` | fix(app): warn loudly on cleartext http urls | → C1 |
| 90 | `ee69638` | docs(plan): execute_batch exhaustiveness scope | n/u — docs artefact |
| 91 | `1112fb2` | refactor(storage): compiler-exhaustive dispatch | n/u — refactor (compile-time; behavior unchanged) |
| 92 | `c73ee09` | docs(plan): sync client helper scope | n/u — docs artefact |
| 93 | `a000969` | refactor(sync): deck error transport triple | n/u — refactor |
| 94 | `044cc43` | refactor(sync): collapse label-delta arms | n/u — refactor (behavior-preserving at landing; arms reworked by row 148, judged there) |
| 95 | `7b408c1` | refactor(sync): shared request choreography | n/u — refactor |
| 96 | `04a8671` | docs(plan): push_exec module split scope | n/u — docs artefact |
| 97 | `49e8399` | refactor(sync): push_exec.rs → module dir | n/u — refactor |
| 98 | `412b99d` | refactor(sync): extract push_exec::overlay | n/u — refactor |
| 99 | `750a0a9` | refactor(sync): extract push_exec::outcomes | n/u — refactor |
| 100 | `5ede7fc` | refactor(sync): extract push_exec::execute | n/u — refactor |
| 101 | `76838a1` | refactor(sync): push_exec tests to tests.rs | n/u — refactor (test move) |
| 102 | `2d032f9` | docs(plan): testing docs & runbook scope | n/u — docs artefact |
| 103 | `fbe917c` | docs(testing): handwritten-fakes standard | n/u — docs artefact |
| 104 | `1ba3461` | docs(guidelines): protected-main PR flow | n/u — docs artefact (dev-facing workflow doc) |
| 105 | `abee19e` | docs(testing): keyring tier-3 runbook | n/u — docs artefact |
| 106 | `8232a27` | docs(state): stale reminders doc comments | n/u — docs artefact |
| 107 | `2e3896d` | docs(plan): CI guardrails scope | n/u — docs artefact |
| 108 | `2770828` | ci(test): bench targets out of nextest lane | n/u — CI only |
| 109 | `33a9350` | ci(benchmark): bencher cli + loud tracking | n/u — CI only |
| 110 | `72ffb1b` | ci(mutants): upload real mutants output | n/u — CI only |
| 111 | `6b3927c` | fix(ui-slint): deny hand-written unsafe | n/u — lint attribute only; no runtime behavior |
| 112 | `47b2cba` | ci(deny): domain purity + dumb-ui boundaries | n/u — CI only |
| 113 | `82de884` | docs(plan): architecture & release docs scope | n/u — docs artefact |
| 114 | `abe63e4` | docs(adr): formalize ADR 0013 | n/u — docs artefact |
| 115 | `4d73eb4` | docs: promote flow model | n/u — docs artefact |
| 116 | `63f9080` | docs(arch): shipped modes in summary | n/u — docs artefact |
| 117 | `98813c0` | docs(arch): channel-ownership corrections | n/u — docs artefact |
| 118 | `efb31c3` | docs(arch): reminders subsystem | n/u — docs artefact |
| 119 | `404fcb4` | docs(changelog): 0.1.0 corrections/normalization | n/u — released-section correction, not an [Unreleased] candidate |
| 120 | `282a1f7` | docs(plan): release tooling & launch scope | n/u — docs artefact |
| 121 | `aef466f` | fix(release): PR-based release cut (#235) | n/u — release tooling, dev-facing process (PR #332 precedent: only the #236 product entry entered [Unreleased]) |
| 122 | `e52ea40` | ci(release): bash pin for windows guard | n/u — CI only |
| 123 | `3ad16bb` | fix(app): bare no-tty launch with display → gui | present (present-C1, #236) |
| 124 | `09f3fd4` | docs(plan): board/stack editing design | n/u — docs artefact |
| 125 | `721f2f3` | refactor(domain): rename board-push op UpdateBoard | n/u — refactor (internal enum name; feature covered by present-A1) |
| 126 | `b8e2821` | feat(domain): board edit verbs | present (present-A1) |
| 127 | `bb4013e` | test(state): snapshot board edit verbs | n/u — test-only |
| 128 | `bd314de` | test(storage): 0005 upgrade path | n/u — test-only |
| 129 | `73275b4` | test(sync): board delete push path | n/u — test-only |
| 130 | `ba9ebd2` | feat(app): board edit verbs + ceremonies | present (present-A1) |
| 131 | `4304c03` | feat(ui-slint): create stacks on live board | present (present-A2, #304) |
| 132 | `878bb12` | docs(adr): boards complete per-field merge | n/u — docs artefact |
| 133 | `d5ee1db` | docs(plan): multi-board wave framing | n/u — docs artefact |
| 134 | `16b4b3a` | docs(plan): fold PR 334 board shapes | n/u — docs artefact |
| 135 | `e448e1d` | docs(plan): multi-board Stage-0 dispatch | n/u — docs artefact |
| 136 | `62dd839` | docs(template): stage-3 rebase-first | n/u — docs artefact |
| 137 | `361ab0b` | docs(plan): remediation follow-ups dispatch | n/u — docs artefact |
| 138 | `850661e` | docs(plan): multi-board detailed design round | n/u — docs artefact |
| 139 | `b24446e` | docs(plan): multi-board wave detailed design | n/u — docs artefact |
| 140 | `2b1b8b8` | docs(report): worktree disk usage | n/u — docs artefact |
| 141 | `3735122` | chore(scripts): worktree-doctor.sh | n/u — dev tooling |
| 142 | `a8b6b55` | docs(agents): worktree/coverage lifecycle | n/u — docs artefact |
| 143 | `83781ca` | docs(plan): disk-hygiene protocol | n/u — docs artefact |
| 144 | `aeadd9e` | chore(build): line-tables-only dev debug | n/u — dev-profile only; release artefacts unchanged |
| 145 | `b1f35fc` | docs(plan): multi-board implementation round | n/u — docs artefact |
| 146 | `73cc582` | docs(plan): multi-board acceptance finalised | n/u — docs artefact |
| 147 | `0a49fbf` | docs(plan): sync push semantics chunk plan | n/u — docs artefact (this round, chunk 1) |
| 148 | `de3d461` | fix(sync): per-op outcomes in label-delta rejection arms | → F2 |
| 149 | `9989894` | fix(sync): re-resolve delete-path 404s | → F1 |
| 150 | `9b17466` | docs(adr): ADR 0007 delete-path amendment | n/u — docs artefact (covered by F1's ADR mention) |
| 151 | `78b459a` | docs(plan): sync gap tests chunk plan | n/u — docs artefact (chunk 2) |
| 152 | `4f12d69` | test(domain): fail-reads seam in the fake | n/u — test-support only; no production surface |
| 153 | `d97d189` | test(sync): pulled labels reach snapshot | n/u — test-only (chunk 2, F6-04) |
| 154 | `2b5f4a4` | test(sync): board-switch cache clear | n/u — test-only (F6-05) |
| 155 | `f51c3e4` | test(sync): absorber/backoff/tombstone arms | n/u — test-only (F6-06) |
| 156 | `505a43c` | test(sync): archived-twin dedupe + flag | n/u — test-only (F6-23) |
| 157 | `10e9cb6` | test(sync): post-create failure discrimination | n/u — test-only (F6-24) |
| 158 | `e89480f` | docs(report): chunk-2 stage-2 report | n/u — docs artefact |
| 159 | `0dc45c5` | docs(plan): engine exit chunk plan | n/u — docs artefact (chunk 3) |
| 160 | `539de8b` | fix(state): weak emission seam, exit reachable | → F7 |
| 161 | `7eb4008` | docs(report): chunk-3 stage-2 report | n/u — docs artefact |
| 162 | `22ebeaa` | docs(plan): storage test hygiene chunk plan | n/u — docs artefact (chunk 4) |
| 163 | `84fb19a` | feat(storage): close() disposal seam (#313) | n/u — additive, behavior-neutral seam; no app-side call sites (verified: repo.rs impl + connect.rs test fixtures only); the leak symptom was test-side /tmp sidecars |
| 164 | `46d038d` | test(storage): fixtures close-before-drop | n/u — test-only |
| 165 | `999757f` | test(storage): reminder partition roundtrips | n/u — test-only |

**Tally**: 12 commits → entries added (F1–F7, C1–C2); 5 commits
already present; 148 judged not user-visible. Every `feat:`/`fix:`
commit in the range is accounted for; the only `feat:` judged n/u is
row 163 (D5) and the only `chore:` judged user-visible is row 71
(C2) — exactly the caveat the checklist step (§checklist) encodes.

## §entries — the exact `[Unreleased]` additions (verbatim)

Placement: the existing `### Added` and `### Changed` blocks stay
where they are (Added untouched; C1/C2 appended to the end of
Changed's list); a new `### Fixed` block is inserted **after** the
`### Changed` block — canonical Keep-a-Changelog order
(Added → Changed → Fixed). `scripts/changelog.py` is order-agnostic:
only full-line `## ` headings bound a section, and its golden test
exercises a `### Fixed` block directly (D2).

Voice matched from the existing entries (board editing, #236, #302)
and the 0.1.0 Fixed family: full sentences, present tense, concrete
verb/type names, ADR refs in parentheses, issue refs at the entry
tail, no internal file paths, ~72-char wrapping with two-space
continuation indent.

Append to `### Changed` (after the EngineEnvelope entry):

```markdown
- The `connect` ceremony and daemon bootstrap warn once per process
  when the Nextcloud server URL is cleartext `http`, and the connect
  receipt records a `cleartext_transport` flag (#261).
- Release builds enable overflow-checks: arithmetic overflow in the
  shipped binary panics loudly instead of wrapping silently (dev and
  test builds already checked) (#303).
```

New `### Fixed` block after `### Changed`:

```markdown
### Fixed

- A push raced by another client's move no longer loses the local
  intent or re-adopts the card as a twin: a 404 at the stale stack
  binding re-resolves the card once against the fresh listings and
  retries at the fresh location — update/move re-fetch before
  classifying (#258), delete retries at the fresh stack (#319); a
  card absent from every stack still converges `RemoteMissing`, so
  the local tombstone stays correct (ADR 0007).
- Label-delta outcomes on task updates are decided per operation: a
  failing `assignLabel`/`removeLabel` sub-call no longer completes
  the primary edit and its other label ops with the primary's
  result — ops whose write landed stay `Applied`, the failing
  label's ops carry the classified outcome per op, and an
  unresolvable label reaches `Rejected { LocalData }` instead of
  being silently shadowed (#310, #314).
- A stray shared reference to the running core no longer panics the
  daemon mid-shutdown: the shutdown tail logs the advisory refcount,
  runs the ownership-free salvage steps (engine flush, `Shutdown`
  broadcast), and exits through the regular error path (#257).
- Server URLs carrying credentials (`https://user:password@host`)
  are rejected with a typed error at config validation and client
  construction, instead of validating and carrying the credential
  through resolved-config logs and the client's base URL (#260).
- Files taskboard creates are owner-only (`0600`): the SQLite
  database with its WAL/SHM siblings, the daemon lock sidecar, and
  the config file rewritten by `connect` and `logout`; pre-existing
  permissive copies are tightened on the next run (#297).
- REPL history is written through taskboard's own `0600` writer
  instead of rustyline's save, whose process-global umask window
  could strip permission bits from files sibling threads created
  concurrently (#289).
- The state engine exits when every input source is closed: the run
  loop no longer pins its own system-event sender, which had made
  the all-sources-closed exit structurally unreachable (#316).
```

Sweep mapping (normative): F1 ← rows 77, 149; F2 ← rows 53, 148;
F3 ← row 82; F4 ← rows 87, 88; F5 ← rows 84–86; F6 ← row 60;
F7 ← row 160; C1 ← row 89; C2 ← row 71.

## §checklist — the release-cut guard step (verbatim)

**Destination (D1)**: `docs/coding_guidelines.org`, §7 "Release
Versioning" — the release docs PR #332 landed (the section the
chunk-14 plan amended for the PR-based cut). `docs/architecture.org`'s
release note explicitly defers to it ("see
=docs/coding_guidelines.org="), so one file owns the procedure.
Insert **after the section's final paragraph** (the tag paragraph
ending "the script prints these steps)"), **before** the `* 8.
Artefact Management` heading:

```org
Before cutting, sweep the range since the last tag for changelog
misses: a user-visible commit with no =[Unreleased]= entry ships
silently — the promotion step copies the section verbatim and never
invents entries.

#+begin_src sh
LAST="$(git tag --list 'v*' --sort=-v:refname | head -n1)"
git log --oneline "$LAST"..origin/main | grep -E '\b(feat|fix)\('
#+end_src

Check every listed commit against the =[Unreleased]= section by its
=#<issue>= reference; =chore= or =refactor= commits that change
shipped behavior (profile flags, dependency bumps — e.g. the
release-profile overflow-checks) are judged the same way, while
docs-, test-, and CI-only commits are exempt. A miss is fixed by
adding the entry to =[Unreleased]= before running the bump, so
promotion picks it up verbatim.
```

The grep is the issue's "greps the commit range against the changelog
section" step: `feat|fix` is the primary filter, and the prose
carries the `chore`/`refactor` caveat with the in-repo precedent
(#303 landed as `chore(workspace)` and IS user-visible — row 71).

## §decisions — recorded, resolving every open question

- **D1 — checklist destination = `docs/coding_guidelines.org` §7.**
  PR #332 (chunk 14) made it the release-procedure doc;
  architecture.org defers to it; the dispatch acceptance fixes the
  diff to "CHANGELOG.md + the one release-doc file". Embedding the
  sweep in `scripts/bump_version.sh` as an automated guard would
  change a script (outside the accepted diff shape) — recorded in
  §non-goals as a candidate follow-up issue, not filed by this chunk.
- **D2 — section order: Added → Changed → Fixed.** Canonical
  Keep-a-Changelog 1.1.0 order, which the file header declares. The
  0.1.0 section's (Added, Fixed, Changed) order is a normalization
  artifact of merging blocks in place, not a normative claim.
  `changelog.py promote` is order-agnostic (golden test uses a
  `### Fixed` block; only `^## ` headings bind).
- **D3 — grouping: one entry per user-visible behavior family, not
  per commit.** F1 merges #258+#319 (one stale-binding behavior, one
  ADR); F2 merges #310+#314 (one per-op-outcome behavior — #310's
  double-emission and #314's rejection-arm swallowing are two
  symptoms of the same user-visible defect); F4 merges the two #260
  commits (one rejection behavior at two surfaces); F5 merges the
  three #297 commits (one 0600 hardening behavior across three file
  families). Matches the board-verbs entry style (#132, #171, #177).
- **D4 — #316 (weak emission seam) is user-visible → F7.** The run
  loop's exit condition is a real behavior change (the engine task
  can now observe all-sources-closed and join); the acceptance
  requires behavior-relevant commits to be represented or explicitly
  judged. Judged in.
- **D5 — #313's `close()` seam is not user-visible.** Additive,
  behavior-neutral by its own commit body; verified no `taskboard-app`
  call sites (only the storage crate's impl and test fixtures call
  it); the 10,923-file leak symptom was test-side `/tmp` sidecars.
  Chunks 2 and 4's test commits are n/u for the same reason.
- **D6 — release-tooling/CI commits are not user-visible** (row 121
  et al.): they change the release *process*, not the product.
  Precedent: PR #332 shipped the whole release-tooling chunk with
  only the #236 product entry in `[Unreleased]`.
- **D7 — docs-only commits are excluded wholesale** (148-commit
  majority): established practice recorded in the arch-org-release
  plan §changelog.4 (the ADR 0007 amendment and EngineEnvelope
  doc-sweep added none). Includes this round's own plans/reports.
- **D8 — one stage-2 commit** for both files:
  `docs(changelog): backfill [Unreleased] since v0.1.0 (#330)` —
  prompt 5.2's subject shape; the guard paragraph is part of the
  same coherent landing (both halves close #330).
- **D9 — the sweep table is normative at `999757f`.** If stage 3's
  fixes add commits after the backfill, they are review/docs commits
  by construction; stage 3 re-runs the §checklist grep over the
  final range and extends the table if anything behavior-bearing
  slipped in (it should not).

## §stage-2 — implementation steps

1. Read AGENTS.md, #330, dispatch §1/§6, this plan, in the worktree
   (recreate per the round protocol if missing).
2. Edit `CHANGELOG.md`: append C1/C2 to `### Changed`; insert the
   `### Fixed` block after it. Transcribe §entries verbatim.
3. Edit `docs/coding_guidelines.org` §7: insert the §checklist block
   verbatim (org-mode: `=...=` markers and `#+begin_src sh` block as
   written).
4. Proofs (in order):
   - `python3 scripts/changelog.py self-test` — green.
   - Promotion dry-run on a scratch copy: `cp CHANGELOG.md /tmp`,
     `python3 scripts/changelog.py promote 9.9.9 2026-10-18
     /tmp/CHANGELOG.md`, then `extract 9.9.9` on the copy — all nine
     new entries present in the promoted section, `### Fixed` intact.
   - `git diff --stat` — exactly `CHANGELOG.md` +
     `docs/coding_guidelines.org`.
5. Commit once (D8). No push, no PR (shipping is stage-3's).
6. Full AGENTS §10 gate suite per prompt 5.2 (docs-only, but run it;
   delete `target/llvm-cov-target` after `lcov.info` per the disk
   hygiene rule).

## §verification — acceptance mapping (dispatch §6)

| Acceptance criterion | Proof |
|---|---|
| Every behavior-relevant commit represented or judged n/u in the sweep table | §sweep (165 rows, tally; every `feat:`/`fix:` accounted) |
| Checklist step exists in the release docs | §checklist → coding_guidelines.org §7 |
| `bump_version.sh`'s promotion picks the entries up verbatim | changelog.py self-test + scratch-copy promote/extract proof (stage-2 step 4) |
| Gates green | full §10 suite run by stage 2 and stage 3 |
| Diff is CHANGELOG.md + one release-doc file | `git diff --stat` check (stage-2 step 4) |
| No historical section rewritten, no code, no version bump | diff shape alone shows it |

## §non-goals

- No rewriting of the released `0.1.0` section (dispatch non-goal;
  the 0.1.0 corrections already landed with PR #331, chunk 12).
- No code changes, no version bump, no `scripts/bump_version.sh` or
  `scripts/changelog.py` edits.
- No automation of the sweep inside the release script (candidate
  follow-up issue; the dispatcher/user decides — AGENTS §7
  conventions, one `backlog:` issue, `ci` or `dx` label).
- No entries for docs/test/CI/refactor commits beyond §sweep's
  judgments (D4–D7 are the only borderline calls; all recorded).
- No backlog/issue edits; tracker closure rides the round PR body
  (`Closes #330` per dispatch §7).

## §risks

- **n/u verdicts are judgments** — mitigated: every `feat:`/`fix:`
  is individually row'd with a why; the four borderline calls carry
  recorded decisions (D4–D6); stage 3 re-derives the table from the
  grep before approving.
- **The branch may rebase onto a moved `origin/main` before
  shipping** — the table is point-in-time normative; stage 3 re-runs
  the checklist grep over the final `v0.1.0..HEAD` and extends the
  table for any new behavior commit (D9).
- **Entry wording is transcribed, not re-derived** — stage 2 copies
  §entries/§checklist verbatim; improvements go through stage 3's
  amend, not silent drift.
- **`grep -E '\b(feat|fix)\('` is GNU-grep syntax** — the docs
  already assume a GNU userland (the release script uses bash +
  GNU tools); no portability note needed.

## §stage-3 pointers

Per dispatch §1 (amended single-PR protocol) and prompt 5.3: review
the chunk increment, spot-check the round's full diff, re-run the
gates, fix findings by amend, rebase onto `origin/main`, push once,
open the ONE PR (body: five chunks; `Closes #242 #313 #314 #316
#319 #323 #330`), watch `gh pr checks` to green, verify
`mergeStateStatus` CLEAN, then remove the round's worktree as the
final act. Never merge.

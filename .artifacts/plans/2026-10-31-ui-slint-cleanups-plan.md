# [Plan] Chunk 5.2 — UI-slint audit cleanups (#275)

- **Date**: 2026-10-31 (dispatch-scheduled name; written 2026-11-03)
- **Chunk**: 5.2 of backlog round 2
  (dispatch plan: `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §7 "Chunk 5.2 — ui-slint cleanups (#275)")
- **Branch**: `chore/ui-slint-cleanups`, cut off `origin/main` at
  `5d8c0751` (the fetched tip; local `main` was 11 commits behind).
  Worktree: `../taskboard-WT-chore-ui-slint-cleanups`
  (`/home/viktoria/src/` resolves onto the relocated partition, so the
  path is a true sibling of the main checkout; upstream tracking was
  unset after `worktree add -b` auto-pointed it at `origin/main` — a
  new branch must never look push-defaulted at `main`).
- **Covers** (normative): #275 → F7-03, F7-04, F7-07, F7-08, F7-10.
  Originating report:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-ui-slint-report.md`
  (§6 axis G/E findings, §7 axis F, §8 axis H, §9.3 axis C).
- **Gate**: chunk 4.1 merged — the dispatch named PR **#461**, which is
  the chunk's *issue* number; the work landed as **PR #594**
  (`perf/ui-today-virtualisation`, MERGED 2026-11-03T10:36:21Z,
  contained in `origin/main`). Same stale-number pattern chunk 2.2
  recorded (its dispatch named issue #555's PR). Gate satisfied — all
  of batch 4 (#594, #593, #591) is merged, so batch 5 is open.
- **Status**: planning complete; every open question resolved in §3
  (D1–D6). Implementation not started.
- **Hard rule** (dispatch acceptance): **zero behavior changes.** The
  suite is the pin: every test passes unmodified, no snapshot moves,
  no goldens regenerate. A changed UI behavior is a bug in this chunk.

---

## 1. Method

- Re-verification ran against `origin/main` @ `5d8c0751` in this
  worktree (the branch base), finding by finding, with the audited
  v0.1.0 coordinate quoted alongside. Every finding was re-read at its
  current site; the adapter's r1–r4 GUI-review growth (from ~731 lines
  at audit time to 5 570 now) and the render lane (unchanged-refresh
  guard #463, kiosk i16 fix #462, today virtualisation #461) rebuilt
  exactly the seams the audit cites, so no coordinate was trusted from
  the issue text.
- The crate's own growth since the audit means several "counts" in the
  findings are stale even where the *verdicts* hold; the table below
  separates the two.

## 2. Re-verification table (the core)

| Finding | Verdict | Audited @ v0.1.0 | Current @ `5d8c0751` — evidence |
|---|---|---|---|
| **F7-03** (P3, axis G) `current_breakpoint` pub, zero callers; the adapter reads the global directly | **still drifted** | `models.rs:357–363`; adapter read @ `adapter.rs:199–201`; `is_compact` implied the only route | `models.rs:1823–1832` — still `pub fn current_breakpoint(&AppWindow) -> Breakpoint` (the Slint→Rust reverse bridge), **still zero callers**: the repo-wide grep hits only the *Slint global accessors* `get_current_breakpoint`/`set_current_breakpoint` (adapter.rs:932, 3432; lib.rs:1156–1166; tests/driver.rs:512, 521), never the Rust fn. `is_compact` @ `adapter.rs:931–933` still reads the global directly (2 callers: `:1076`, `:1401`) |
| **F7-04** (P3, axis G/E) `set_view_context`/`set_focused_entities` pub but test-only-called | **still drifted — and cfg-gating became structurally impossible** | `adapter.rs:706–731`; callers only in-crate `#[cfg(test)]` (`lib.rs:514–515, 559, 976, 1054`); "the integration `tests/` never call them" | The fns now @ `adapter.rs:5516` / `:5546` on the `impl crate::UiHandle` block (doc @ `:5505–5514` already says "the component tests use this seam now"). In-crate callers all inside `#[cfg(test)]` mods (`keymap_adapter_tests` @ lib.rs:2010, 2011, 2055; `view_component_tests` @ `:2919`, `:3005`). **The audit's caller fact inverted**: `tests/usability_components.rs:704, 783` now drive the settings surface through `set_view_context` ("opens through the component-test seam"). `#[cfg(test)]` items are invisible to an externally-compiled crate, so the audit's gate option would break the integration build |
| **F7-07** (P3, axis F) `UiSettings` mount-once; live re-config needs a post-mount handover channel | **fixed-already** | `lib.rs:44–58, 124–161`; scale pushed at construction, keymap installed one-way at start (`adapter.rs:7–8`); building blocks named: `Keymap::build` pure + `Theme.base-scale` in-out | Both named building blocks now have live plumbing: **scale** — the settings form's five verbs `settings_scale_preview/committed/pressed/released/focus_lost` (`adapter.rs:3047–3172`, callbacks installed `:5460–5486`) write `Theme.base_scale` at runtime; **keymap** — `SetKeymapPreset` replies ride a oneshot into `apply_keymap_update` (`adapter.rs:2112–2113`, `:2434`, `:2580`; "A `KeymapUpdate` swaps the keymap live and refreshes", `:2369`). The re-config channel the finding asked for exists |
| **F7-08** (P3, axis H) 12× seam boilerplate; `install_seam!` only if a 13th seam lands | **trigger met — ~9× over** | "12+ repetitions" @ `adapter.rs:459–699` (of 731 lines); pre-empting rationale @ `:468–470` | **110 `window.on_*` seam installs** across **26 `install_*` fns** in the now-5 570-line adapter (98 `as_weak()` sites). The rationale comment survives verbatim @ `adapter.rs:4288–4290` ("a `Fn(&mut Adapter, &AppWindow, Args)` wrapper shape is over generics' worth of closure types"). No macro exists. The dispatch's "no action now" premise (count ≈ 12) is stale — the 13th seam landed long ago (r1–r4 rounds) |
| **F7-10** (P3, axis C/L) two production clock reads, chunk-11-verified DIRECT-JUSTIFIED | **count stale; verdict stands** | `adapter.rs:193` (refresh's `Utc::now()` into `models::push`) + `lib.rs:211` (wall-minute detector); "none required for v0.1.x" | **9 production reads** now, all display/interaction semantics: `adapter.rs:775` (`day_ctx` — the day window), `:2322` (`push_notice` expiry), `:2359` (`record_system_event` resurrect-notice mint), `:4782` + `:4798` (`install_due_seam` closures — due echo / accepted parse), `:4900` (`begin_date_edit` echo), `:4966` (`open_due_picker` today anchor), `lib.rs:612` (the per-tick clock read feeding both boundaries, r4 decision 5), `lib.rs:739` (`run_alarm_pass` window). The remaining grep hits are `#[cfg(test)]` items (`test_seed_due_buffers` @ `:1050–1059`, lib.rs test mods ≥ `:1073`). No test depends on an absolute wall time (bucket assertions stay relative) — the DIRECT-JUSTIFIED verdict re-verified independently |

Fixed-already candidates: **one** (F7-07). Stage 2 prepares its ✅
text (§8); the other four are split delete / document / defer /
annotate.

## 3. Decisions (every open question resolved)

### D1 — F7-03: **delete** the dead bridge; no caller should exist

The dispatch's decision rule: delete vs. route, decided by whether a
caller *should* exist. **No caller should exist:**

- The adapter owns the *write* side: it computes the breakpoint with
  `responsive::breakpoint_for_width` and pushes it into the
  `Responsive` global (`adapter.rs:3432`), so production read-back is
  redundant — the value it would read back is its own output.
- `is_compact` (`adapter.rs:931–933`) is already the single production
  read-back point and reads the global directly; routing it through the
  bridge would add an enum round-trip (`SlintBreakpoint` →
  `responsive::Breakpoint`) per read to produce the same boolean. The
  tests read the Slint global directly too (`lib.rs:1156–1166`,
  `tests/driver.rs:512, 521`) — they want the *window's* state, not a
  conversion.
- The bridge is the reverse direction of `models::to_slint_breakpoint`
  (`models.rs:171–176`), which stays (it has real call sites on the
  push path).

**Edit**: delete `crates/taskboard-ui-slint/src/models.rs:1823–1832`
(the two doc lines, `#[must_use]`, the fn) plus one adjacent blank line
so exactly one blank line remains between the preceding fn and
`#[cfg(test)] mod tests`. Both imports stay live (`use
crate::responsive::Breakpoint` @ `:25` and the `SlintBreakpoint` alias
@ `:29` are both consumed by `to_slint_breakpoint`). No test
references it; nothing else moves.

**Type**: `refactor(ui-slint)`.

### D2 — F7-04: **document permanent**; cfg-gating is off the table

The audit offered gate-or-document; the re-verification picks
document, and not merely by preference:

- `#[cfg(test)]` on a `pub fn` of a lib crate hides it from *external*
  compilations — and `tests/usability_components.rs` (the integration
  tier) now calls `set_view_context` at `:704` and `:783` to open the
  settings surface. Gating would break `cargo nextest`'s integration
  lane; un-gating it again would be the only fix, so the option is
  structurally closed, not just inconvenient.
- Moving those two integration cases in-crate would change the test
  tiering to enable a gating nicety — scope creep for a
  zero-behavior-change cleanup chunk.
- Production code calls neither method (repo-wide grep: definitions +
  the two tiers above, nothing else) — the "test seam" characterization
  is still true; only its audience widened.

**Edit**: extend the doc block at
`crates/taskboard-ui-slint/src/adapter.rs:5505–5514` with a
permanence paragraph stating the constraint the code can't show:

> This seam is permanent pub surface, deliberately not
> `#[cfg(test)]`-gated (audit F7-04): the integration tier
> (`tests/usability_components.rs`) compiles this crate externally and
> drives the settings surface through it. Production code calls
> neither method (re-verified 2026-11-03 @ `5d8c0751`).

(Stage 2 folds this into the existing paragraph's voice; the exact
wording above is the content contract, not the typography.) The
crate's house pattern for *gated* test seams — the `#[cfg(test)] impl
UiHandle` block at `lib.rs:235–248` — is what these two fns are
deliberately *not*, and the paragraph says why.

**Type**: `docs(ui-slint)`.

### D3 — F7-08: the macro trigger **met, deferred to a dedicated follow-up** — not built in this chunk

The issue's letter says "macro only on a 13th seam"; the count is 110.
The honest reading is not "build it now" but "the premise under which
the dispatch wrote *no action now* has rotted":

- This chunk is sized **S** with a **zero-behavior-change** acceptance.
  A mechanical conversion of 110 callback-install sites is an M/L
  refactor touching the most churned file in the crate (adapter.rs
  grew ~7.6× since the audit, and every GUI-review round edits the
  install fns). Doing it here would bury a large mechanical diff inside
  a cleanup PR and make the "zero behavior changes" review impossible
  to perform hunk-by-hunk.
- The audit's design premise ("three concrete call-shapes exist to
  design against") needs re-derivation: the 110 sites now span at
  least the route-action shape, the raw-send shape
  (`send_toggled_done`, `install_tracking_seams` @ `:5163–5176`), the
  settings-scale verb shape (`:5460–5486`), the due-edit shape
  (`:4753–4800`), and the multi-Rc shapes (`install_board_seams` @
  `:4291–4310` grabs two clones + two weaks for one handler). The macro
  design is real work, deserving its own branch, its own PR, and the
  full journey/golden suite as its safety net.
- The file's own rationale (`adapter.rs:4288–4290`) is the standing
  counter-argument and stays untouched here; the follow-up is what
  re-adjudicates it.

**Action**: no diff hunk. Stage 3 files the follow-up issue
(`backlog: install_seam! macro over the 110 adapter seam installs`;
labels `refactoring` + `dx` + `p3`; body summarizing this decision +
permalink to the #275 close comment and the audit report), then closes
#275 referencing it. Texts in §8.

### D4 — F7-07: **fixed-already, annotate** — no diff hunk

The re-config channel the finding reserved effort-M for landed with
the settings wave (scale: `adapter.rs:3047–3172` + installs at
`:5460–5486`; keymap: the `SetKeymapPreset` reply lane into
`apply_keymap_update`, `:2112–2113`, `:2369`, `:2434`, `:2580`).
Stage 2 prepares the ✅ close text (§8); stage 3 posts it. No code.

### D5 — F7-10: **verdict re-recorded on the refreshed inventory** — no diff hunk

The audit's action was already "none required for v0.1.x"; the round-2
dispatch carries the chunk-11 DIRECT-JUSTIFIED verdict as normative.
Re-verification adds value only as an inventory refresh: **9
production sites** (table, §2), each a display/interaction semantic
(due echo, notice horizons, picker today-anchor, tick boundaries,
alarm windows), none test-load-bearing in absolute time. Lifting a
`now` parameter anywhere would be a behavior-relevant refactor — out
of this chunk by the hard rule. Stage 2 prepares the close text (§8)
carrying the refreshed count so the next audit doesn't re-derive it.

### D6 — chunk mechanics

- **No CHANGELOG** (dispatch stage-2 prompt; nothing user-observable).
- **Commits**: one per finding-with-a-diff → two commits (§4). The
  no-hunk findings (D3/D4/D5) exist only as issue annotations.
- **Gates** run through `scripts/run-quality-gates.sh` (this box hosts
  a live sibling worktree, `feat/ui-r4-datetime-entry` — the flock
  wrapper is mandatory, AGENTS §10).
- **No push, no PR, never touch `main`** — handoff at "implementation
  + gates green"; stage 3 ships.

## 4. Implementation order & commit breakdown

| # | Commit | Finding | Content |
|---|---|---|---|
| 1 | `refactor(ui-slint): the callerless breakpoint bridge deletes` | F7-03 | models.rs:1823–1832 deletion (D1) |
| 2 | `docs(ui-slint): the focus seam documents its permanence` | F7-04 | adapter.rs:5505–5514 doc paragraph (D2) |

F7-07/F7-08/F7-10: no commit (D3/D4/D5). Both commits are standalone,
build-green, rebase-merge clean (AGENTS §8); order 1 → 2 is
independent (either may land alone).

## 5. Verification (stage 2 exit criteria)

- The full §10 block through the wrapper: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo nextest run --workspace`, `cargo test --doc --workspace`,
  `cargo deny check`, the llvm-cov three-command sequence with
  `scripts/coverage-quarantine-profraw.sh` and the trailing
  `rm -rf target/llvm-cov-target`.
- Zero-behavior pins: the entire suite green with **no test file
  touched** (this chunk's diff contains no `.rs` test code, no
  snapshot, no golden). A snapshot diff anywhere is a bug in the chunk.
- Grep audits after the edits:
  - `grep -rn 'current_breakpoint' crates/ tests/` → only the Slint
    global accessors (`get_current_breakpoint`, `set_current_breakpoint`)
    remain; no `models::current_breakpoint`, no bare Rust call.
  - `grep -rn 'set_view_context\|set_focused_entities' crates/
    tests/` → the definitions (adapter.rs), the in-crate `#[cfg(test)]`
    callers (lib.rs), and `tests/usability_components.rs` — zero
    production callers, unchanged from the re-verification census.
- `cargo doc` warning-free on the crate (the new doc paragraph adds no
  broken intra-doc links).

## 6. Risks

1. **Deleting a `pub fn`** — an out-of-workspace consumer would break.
   There is none: `taskboard-ui-slint` is a workspace-internal
   presentation crate consumed only by `taskboard-app` (no
   `current_breakpoint` hit there), and the workspace build is the
   proof. Low.
2. **Doc-wording drift** (D2) — the paragraph must state the
   constraint (permanence + why gating is impossible), not narrate the
   audit. The §3 text is the contract; stage 3 reviews against it. Low.
3. **Scope creep around F7-08** — the 110-site census *invites* a
   macro. The decision (D3) is recorded precisely so neither stage 2
   nor stage 3 "helpfully" starts the conversion; the follow-up issue
   is the home for that appetite. Medium temptation, mitigated.
4. **Plan-file date** (2026-10-31 vs. today's 2026-11-03): kept per
   the dispatch prompt's verbatim path. Noted so nobody reads it as a
   dating error (chunk 2.2 precedent).
5. **Branch tracking** — `worktree add -b` set the new branch to track
   `origin/main`; already unset (header note). Stage 3 pushes with an
   explicit `git push -u origin chore/ui-slint-cleanups`.

## 7. Out of scope (recorded, not done)

- The other five audit findings' homes: F7-01/F7-02 (axis O, P2),
  F7-05 (multi-board dimension), F7-06 (undo correlation), F7-09
  (mutants sweep) — none are #275's; #275 names F7-03/04/07/08/10 only.
- The `install_seam!` macro itself (D3 → follow-up issue).
- Any clock-seam lift (D5), any settings-channel work (D4 — it
  already exists), any test move between tiers (D2 rejected that path).
- adapter.rs's width-drag/tooltip/reset-seam machinery beyond the two
  named hunks — the r4 rounds own it.

## 8. Stage-3 bookkeeping: prepared issue texts

Stage 2 finalizes these with the landed commit SHAs; stage 3 posts.

**Close comment for #275** (one comment, all five findings
self-contained):

> All five findings re-verified against `origin/main` @ `5d8c0751`
> (2026-11-03) before acting; plan:
> `<permalink to .artifacts/plans/2026-10-31-ui-slint-cleanups-plan.md>`
> on `chore/ui-slint-cleanups`.
>
> - **F7-03** — fixed: `models::current_breakpoint` (the zero-caller
>   Slint→Rust bridge) deleted in facd6b34; the adapter's `is_compact`
>   remains the single production read-back, and both bridge imports
>   stay live via `to_slint_breakpoint`.
> - **F7-04** — fixed (documentation): the two `UiHandle` seam fns
>   stay permanently pub — the audit's `#[cfg(test)]` option became
>   structurally impossible once the integration tier
>   (`tests/usability_components.rs`) started driving the settings
>   surface through `set_view_context`; the seam doc now records the
>   permanence + the zero-production-caller fact (d61d5727).
> - **F7-07** — already fixed by the settings wave, no action needed
>   here: scale re-configures live via the settings-scale verbs
>   (`settings_scale_preview/committed/pressed/released/focus_lost`),
>   and the keymap re-installs live via `apply_keymap_update` on the
>   `SetKeymapPreset` reply lane — the post-mount handover channel the
>   finding reserved effort-M for exists.
> - **F7-08** — trigger met and exceeded: the seam census is now 110
>   installs across 26 `install_*` fns (audit counted ~12). The macro
>   is deferred to <new follow-up issue link> — a 110-site mechanical
>   conversion is its own reviewed refactor, not a slot in a
>   zero-behavior-change cleanup chunk. The adapter's standing
>   rationale comment is what that issue re-adjudicates.
> - **F7-10** — verdict stands, inventory refreshed: 9 production
>   clock reads (audit named 2), all display/interaction semantics,
>   none test-load-bearing in absolute time. DIRECT-JUSTIFIED
>   re-confirmed; no seam lift attempted (behavior-relevant, out of
>   this chunk's hard rule).
>
> Zero behavior changes: suite green unmodified, no snapshot/golden
> moved. PR: <link>.

**New follow-up issue body** (F7-08's macro, filed by stage 3 before
closing #275):

> Title: `backlog: install_seam! macro over the 110 adapter seam
> installs` — labels `refactoring`, `dx`, `p3`.
>
> Origin: audit finding F7-08
> (`.artifacts/reports/2026-10-16-v0.1.0-audit-crate-ui-slint-report.md`
> §8) set the macro trigger at "a 13th seam"; chunk 5.2's
> re-verification (#275, 2026-11-03) counted **110 `window.on_*`
> installs across 26 `install_*` fns** in
> `crates/taskboard-ui-slint/src/adapter.rs` — the trigger fired ~9×
> over during the r1–r4 GUI rounds. The chunk deferred the conversion
> (summary + permalink in its close comment): it is a mechanical M/L
> refactor over the crate's most-churned file and needs its own PR
> with the full journey/golden suite as the safety net. Design input:
> the audit's "three concrete call-shapes" premise needs re-derivation
> (route-action, raw-send, settings-scale-verb, due-edit, and
> multi-Rc/weak shapes all exist now); the file's standing rationale
> ("over generics' worth of closure types", `adapter.rs:4288–4290`) is
> the counter-argument to re-adjudicate with 110 sites on the table.

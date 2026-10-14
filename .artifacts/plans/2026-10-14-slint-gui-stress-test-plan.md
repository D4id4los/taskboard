# [Plan] Slint GUI Stress Test — Phase 7 Design Round Input

- **Date**: 2026-10-14
- **Status**: PROPOSED → executed same day (this is the stage-1 research
  plan for chunk 3 of the wave-0 dispatch plan; the report at
  `.artifacts/reports/2026-10-14-slint-gui-stress-test-report.md` is its
  deliverable)
- **Origin**: GUI requirements brief
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md` §4
  (the checklist, normative), R1–R10 for each item's *why*; dispatch
  plan `.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
  §1 (evidence mandate) / §4 (chunk 3, prompt 3.1).
- **Survey target**: the current stable Slint release — established as
  **1.18.1** (crates.io, published 2026-09-21; 1.18.0 2026-09-16).
  The two 2026-10-11 viability reports surveyed exactly 1.18.1, so
  their Slint facts are re-verified, not re-derived.

---

## 1. What will be verified (one row per GUI-brief §4 item)

| # | Item | Gating requirement | Method |
|---|---|---|---|
| 1 | Focus traversal control & programmatic focus | R4 | docs.slint.dev (FocusScope, `forward-focus`, focus-changed/key-pressed APIs) + docs.rs 1.18.1; compile probe if docs ambiguous |
| 2 | Global key-event interception without per-widget handlers | R5 | FocusScope-at-root pattern; KeyEvent struct fields; verify against docs + probe |
| 3 | Runtime theming via singletons/design tokens; text-role abstraction | R2 | `global` singletons, two-way binding, export of globals; text property set from tokens |
| 4 | Scale factor as a user setting | R3 | `slint::Window::set_scale_factor` / `default-font-size` behaviour; docs.rs + changelog search |
| 5 | `slint::testing` API coverage (instantiate, input injection, property reads) | R6 (logical harness) | docs.rs `slint::testing` 1.18.1 module inventory; compile probe of the three capabilities |
| 6 | Accessibility / screen-reader integration | R7 | AccessKit integration status in 1.18.1 (changelog, `accessibility` feature, docs) |
| 7 | Rich-text: `StyledText` rendering / editing upstream status | §3 org rendering, org-mode reports' gate | verify 1.16–1.18 changelog facts still hold; issue #2723 status |
| 8 | Renderer choice for kiosk hardware (skia vs femtovg) | backlog "[2026-10-07] Add the slint dependency"; Phase 7 first commit | feature matrix, software-rendering story (femtovg GL-only vs skia CPU), licensing/dep-weight; sources: crate docs, changelog, upstream docs |
| 9 | Wayland + X11 screenshot/capture tooling | R6 (visual half) | external survey: grim + wlroots protocols, xdg-desktop-portal Screenshot, X11 `xwd`/import; permissions story per compositor |

Plus the **pin proposal** (exact version + feature set for
`[workspace.dependencies]`) and a verdict table entry per item with
gates-mapped (which R-item each verdict constrains).

## 2. Method

1. **Version establishment**: crates.io API (recorded above, command in
   the report's method section).
2. **Documentation evidence**: docs.rs (pinned /1.18.1/ URLs where
   possible), docs.slint.dev element reference, Slint changelogs
   1.16→1.18, upstream GitHub issues. Every verdict cites what the
   source states.
3. **Compile probes** (only where docs are ambiguous — expected for
   items 1, 2, 5): a throwaway cargo project in /tmp depending on
   slint 1.18.1 with the `testing` + renderer features; never
   committed. Probe outcomes recorded as the evidence.
4. **Prior-art verification**: the two 2026-10-11 reports' Slint claims
   (StyledText timeline, IME-via-TextInput-only, FocusScope
   assessment) are checked against the same 1.18.1 sources rather than
   rediscovered; contradictions are recorded.

## 3. Decisions (resolved up front)

- **D1 — Survey version**: 1.18.1 (current stable). 1.19 pre-releases,
  if any, are noted but never the basis of a verdict.
- **D2 — Verdict vocabulary**: supported / partial / unsupported /
  unclear-with-evidence, per the dispatch prompt; "partial" always
  states what works and what doesn't.
- **D3 — Renderer recommendation shape**: one recommendation, options
  named, consequences for CI system packages and kiosk hardware
  stated (decision-grade per the evidence mandate).
- **D4 — Capture tooling scope**: survey only (options + permissions);
  no working capture harness is built; it is R6's visual half, whose
  build decision belongs to the design session, not this research.

## 4. Out of scope

- The Phase 7 design plan itself (prompt 3.2's deliverable).
- Any repo code change beyond the plan + report artefacts.
- Working accessibility or automation prototypes — evidence, not
  implementation.

## 5. Report outline

Context → method (reproducible) → verdict table (all 9 items) →
per-item evidence sections → renderer comparison recommendation →
Wayland/X11 capture survey → pin proposal → surprises → effort
estimate → risks → references.

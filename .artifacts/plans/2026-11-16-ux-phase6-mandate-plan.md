# [Plan] UX Journey Framework — Phase 6 (Mandate Activation)

- **Date**: 2026-11-16
- **Kind**: tiny docs/process chunk — the framework §6 phase-6 mandate
  activation. Flips the trigger rule from advisory to mandatory. No code.
- **Origin**: the framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md` §6
  phase 6 (the activation chunk this plan performs); `docs/ux_planning.org`
  §3 (the activation state machine and its transition law); the census
  status line (currently *phase 5 complete … phase 6 next*). Preconditions
  met and verified 2026-11-16: phases 1–5 complete and merged (PRs #761,
  #835, #837, #841, #844, #847, #848); the phase-5 remediation findings
  resolved (39 home issues closed with PR references, 8 deferrals open
  without milestones); the user directed the activation.
- **Status**: READY — single chunk, no gates.

## 1. Changes

1. `AGENTS.md` §7 — the *Advisory during rollout* parenthetical is
   replaced by the mandatory wording: the trigger rule binds ALL new
   user-visible work from 2026-11-16 (decision: the user's phase-5
   completion + activation instruction). The gate mechanics sentence
   generalizes from "whenever the origin cites a UX plan" to "for every
   UX journey plan" (all new user-visible work now has one). The gate
   stays never-CI.
2. `docs/ux_planning.org` — the activation state machine: the current
   state flips to *mandatory (phase 6, activated 2026-11-16)*; the §3
   phase-6 row completes; the phase-1 row's stale "in flight" note is
   corrected (its PR landed as #761 — a record-keeping drift the
   activation pass fixes).
3. `docs/ux-surface-census.org` — the status line moves to *the framework
   is mandatory (phase 6 complete, 2026-11-16)*, preserving the
   Previous-history convention.

## 2. Acceptance

- The three files carry the activation consistently (same date, same
  authority chain: the user's instruction + the framework §6).
- No other wording changes; the rule's substance (what a UX journey plan
  must contain, the gate, the walkthrough) untouched.
- Full AGENTS §10 suite green via the wrapper before push (docs-only, but
  the house bar holds).
- PR open + green + `CLEAN`.

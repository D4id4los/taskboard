# Theme Support C1-Impl Plan — Skeleton (completed at gate clearance)

- **Date**: 2026-11-25
- **Status**: SKELETON — blocked on the operator gate for the design
  (`2026-11-25-theme-support-design.md`) and on the mandatory UX journey
  plan (docs/ux_planning.org: the theme switch is a user-visible
  setting). Completed and chunked only after both clear.
- **Origin**: #1437; round-8 scope report §C-C1; the design decisions
  D1–D7 referenced below are normative.

## Gate-clearance checklist (before this plan is filled in and dispatched)

- [ ] Operator accepts the design (or amends it; amendments flow back
      into the design doc's decision ledger).
- [ ] UX journey plan written from
      `.artifacts/templates/ux-journey-plan.md` covering the theme-switch
      journey (settings row → live re-render → persistence across
      restart; kiosk operator's config-file path). Stage 2 runs
      `scripts/ux-coverage.sh <plan> --gate`; stage 3 re-runs it plus the
      walkthrough protocol. The gate is never CI.
- [ ] Dispatch plan per `.artifacts/templates/dispatch-plan.md`
      (one branch/PR per chunk; the dispatcher owns full-suite runs, the
      CI watch, teardown; stage-3 scope ends at PR-open).

## Chunk sketch (to be firmed up at clearance; ordering is a dependency order)

1. **Palette data + tests** (D1/D2/D6): `src/palette.rs` (`Palette`,
   `DARK` verbatim, `LIGHT` leuven-derived seeds tuned to the floors);
   `ThemeName` A1 newtype; theme.slint header/comment refresh; the
   .slint-defaults ≡ `DARK` parity test; token_tests parameterized over
   both palettes (pair list + hover-distinctness + completeness);
   construction-time palette push joining `base-scale`. Gate item:
   resolve the #1414 raised/hover state-colour pairs for light (fix
   preferred, carry-forward only with the issue cited).
2. **Config + persistence** (D5): `[ui] theme` field, serde default
   `"dark"`, `ConfigError::InvalidTheme`, `TASKBOARD_UI_THEME` env lane,
   toml_edit writeback via the `SetScale` ceremony's lane; config tests
   (round-trip, invalid value, env precedence, writeback preserves
   unrelated tables).
3. **Switch surface** (D3/D4): `UiRequest::SetTheme { choice, reply }`;
   settings-view theme selector row on the scale-row grammar
   (echo/validity mirror/`theme-committed` callback);
   `settings_form.rs`/`settings_status.rs` wiring; live push on switch.
4. **Light goldens + journeys** (D7): the representative light subset
   (~6–8 captures, manifest-recorded, frozen render clock per
   testing_strategy §6.1); the ux-journey-plan's journey tests; #1415
   cross-link check (chip contrast on light).

## Cross-cutting laws each chunk inherits

- Sub-agent dispatch discipline (docs/agent_dispatch.org): no coverage
  trio inside a subagent; pre-commit hooks + flock-wrapped
  `scripts/run-quality-gates.sh cargo nextest run --workspace`; every
  cargo invocation through `scripts/memory-gate.sh`.
- No text assertions; typed outcomes only (settings_status sentences
  stay unasserted, per the existing settings-view comment law).
- Clock discipline for golden captures (§6.1–6.2 testing_strategy).
- `#![forbid(unsafe_code)]`, AGPL header in the app/UI crate files.

## Open items at clearance time

- `out` + setter vs `in-out` for the 14 palette properties (D2
  footnote) — decides at chunk 1 against the generated-global setter
  reality; either keeps the token lint green.
- Final LIGHT seeds (chunk 1 tunes against the floors; the seeds in the
  design doc are inputs, not contract).

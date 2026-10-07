<!-- Refer to docs/coding_guidelines.org and AGENTS.md before opening. -->

## Summary

<!-- What does this PR change, and why? Link the issue: "Fixes #NN". -->
<!-- If this implements a plan file, reference it by full path, e.g. -->
<!-- "Implements .artifacts/plans/2026-10-07-nextcloud-retry-logic-plan.md". -->

## Type

- [ ] `feat` — new user-facing feature / domain functionality / UI view
- [ ] `fix` — bug fix
- [ ] `refactor` — restructuring without external behavior change
- [ ] `test` — test additions/updates
- [ ] `docs` — documentation
- [ ] `ci` — pipeline / tooling config
- [ ] `chore` — dependency bumps, maintenance

## Checklist

- [ ] Work happened on a feature branch (`<category>/<scope>-<desc>`), not `main`
- [ ] New tests written (TDD); behavior tested at the lowest layer
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo nextest run --workspace` and `cargo test --doc --workspace` pass
- [ ] `cargo deny check --workspace` passes
- [ ] Crate boundaries respected (see `docs/architecture.org`); new `.rs` files carry the correct SPDX header
- [ ] No test was weakened to make it pass; any discovered bugs are reported
- [ ] Any out-of-scope ideas were added to `.artifacts/taskboard-potential-improvements-backlog.md`

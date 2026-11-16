# [Plan] Chunk A — the #731 deny-advisory verdict

- **Date**: 2026-11-16
- **Kind**: verdict plan (compressed chunk — the dispatcher is
  planner and implementer, stated per `docs/agent_dispatch.org` §7).
- **Fork outcome**: **Path A — green with cause.** No PR, no diff.
- **Evidence**: `cargo deny check` on a clean worktree of
  `chore/deny-advisory-verdict` @ dfdf2562 (= `main` 2026-11-16):
  `advisories ok, bans ok, licenses ok, sources ok`, exit 0. A
  pre-existing `warning[advisory-not-detected]` notes RUSTSEC-2026-0206
  no longer matches (harmless).
- **Cause**: #731 was opened 2026-11-11 18:10 from a branch off
  `main` @ ca961b3d — **before** the ride-out landed on `main` as
  `1aef887c` (2026-11-11, PR #727): a dated `deny.toml` ignore of
  RUSTSEC-2026-0459 with a revisit trigger (next sqlx refresh ⇒ swap
  for `libsqlite3-sys >= 0.39`), because sqlx-sqlite 0.9.0 caps
  libsqlite3-sys at <0.38 and no in-range upgrade exists.
- **Register pairing**: the ignore has no entry on register #284 —
  the dispatcher posts the accepted-risk comment (with re-check
  trigger) and closes #731 with the command output. Issue comments
  only; no git refs touched.
- **Carry-over**: the round plan and dispatch plan ride the first
  PR'd chunk (chunk B's branch) per the operator's instruction.

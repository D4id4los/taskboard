# Chunk S6 (compressed) plan — lld prerequisite doc (#1078) + #1103 operator note

- **Date**: 2026-11-21
- **Kind**: compressed dispatcher-owned chunk (agent_dispatch.org §7
  compression: dispatcher executes the whole chunk in its own session).
- **Origin**: round-4 dispatch plan §1 S6 row; issues #1078, #1103;
  the R1 stage-2 report
  (.artifacts/reports/2026-11-19-dev-lld-linker-stage2-report.md).

## Deliverables

1. `docs/lld_prerequisite.org` — the one-time `sudo apt install lld`
   prerequisite note: the loud-failure-by-design contract from the
   `.cargo/config.toml` stanza, the measured why (−42% full-link
   wall, R1 numbers), the version baseline, the CI parity list, the
   Windows/macOS scope note.
2. The #1103 operator note — a comment on issue #1103 recording the
   2026-11-21 verification facts:
   - the directory
     `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-reflection-domain`
     is NOT a registered worktree (`git worktree list` — zero hits);
     it contains only a `target/` remnant (2.2M measured — no 36–55G
     bloat);
   - the branch `feat/reflection-domain` holds 14 commits not on
     `origin/main` — they live in the shared object store, so removing
     the *directory* loses nothing; the *branch's* disposition (land,
     rebase to `-v2`, or drop) is a separate operator decision and is
     NOT proposed here;
   - per AGENTS §8, ad-hoc `rm -rf` of a `*-WT-*` directory is never
     agent-executable — the operator performs the removal; #1103
     closes on the operator's confirmation only.

## Non-goals

- No changes to `.cargo/config.toml` or any workflow file.
- No removal of the directory (operator-owned).
- No branch disposition for `feat/reflection-domain`.

## Acceptance

- Doc states the prerequisite, the why, the CI parity list.
- The #1103 comment is posted with the verification facts.
- PR open + checks green; merge under the round's standing merge
  authority; #1078 closed at the verified merge.

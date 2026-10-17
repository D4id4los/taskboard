# Analysis — `update_task` classified-rejection arms still shadow subsumed label ops

**Date**: 2026-10-17
**Branch**: `test/sync-push-pull-pinning`
**Follow-up to**: #310 (fixed on this branch in `26e97d0`); code under
analysis: `crates/taskboard-sync-nextcloud/src/push_exec.rs`,
`update_task`, the `assignLabel` and `removeLabel` error arms (the two
`classify_push(..).is_some()` early returns in the label-delta phase).

## Scope

#310 fixed the *success tail* of `update_task`. The two mid-delta early
returns were explicitly left untouched. This report analyses whether
they carry the same outcome-shadowing shape and what a correct fix
looks like. Analysis only — no code changed.

## The arms

Both arms sit in the label-delta phase, after the field PUT has
**landed** (`updated` holds its echo). A sub-endpoint call
(`assignLabel` / `removeLabel`) failed with a non-transport error;
`classify_push` returned `Some(_)`, and the arm returns:

```rust
return GroupOutcome::Outcomes(Self::primary_outcome(
    group,
    PushResult::Applied { echo: Some(..) },
));
```

`primary_outcome` completes the primary **and every subsumed op** with
a synthetic `Applied { echo: None }` — and the classified result is
discarded (`is_some()` is all that is checked).

## Findings

### F1 — subsumed label ops are completed as Applied though their write failed (same class as #310, no duplicate signature)

The in-code comment says "let the label ops retry", but
`apply_push_outcomes` (domain `pipeline.rs`) completes an op on its
first `Applied` and emits `PersistenceAction::CompleteOp` — the op
leaves the outbox and never retries. If the failing `assignLabel(3)`
was the write that op 101 stood for, op 101 is completed as applied
while the server never received the binding. Unlike #310 there is no
duplicate-outcome signature (each op id appears once), so a
length-based test cannot catch it; only the *semantics* are wrong.

Local/server label state then diverges silently until some later cycle
happens to re-touch that card's labels.

### F2 — unresolvable label ops are shadowed on this path too (the #310 shape verbatim)

`desired` is built only from resolvable labels; a subsumed op for an
unresolvable label never causes a wire call, yet on this path it still
receives the synthetic `Applied` from `primary_outcome`. Its
`Rejected { LocalData }` — the outcome the fixed success tail now
correctly emits — is swallowed exactly as before. The #310 fix is
therefore **incomplete**: the bug survives on both rejection arms.

### F3 — per-op classification precedent exists and is not followed

`create_task`'s assign loop maps a classified sub-endpoint failure onto
**the op that caused it** (`outcomes.push(PushOutcome { op: *op_id,
result })`, transport → `Aborted { landed: outcomes }` keeping landed
evidence). The update arms instead discard the classification
entirely. The fix shape already exists in the same file.

### F4 — secondary hazard: `RemoteMissing` from a label sub-call is also swallowed

A 404 on `assignLabel`/`removeLabel` classifies as `RemoteMissing`
(`PushCtx::existing`). Discarding it is arguably the *less* bad choice
here (reporting `RemoteMissing` on the primary would tombstone the
task under R9e because of a *label*-level failure), but it means the
arm conflates "card gone" with "label write failed" and reports
neither. A correct fix must route the classification to the label ops,
never to the primary's result slot.

### F5 — the arms are untested

No test mounts a classified (non-transport) failure on
`assignLabel`/`removeLabel` in the update path (checked `push_exec`
suite and the integration tests). Both arms, including their
interaction with subsumed ops, are uncovered.

## Proposed fix shape (for the future bug-fix pass)

1. Keep the primary `Applied { echo: Some(fields echo) }` — the PUT
   genuinely landed.
2. Emit one outcome per subsumed op, the way the (post-#310) success
   tail and `create_task` do:
   - subsumed `UpdateTask`/`MoveTask` → synthetic `Applied { echo:
     None }` (they rode the PUT);
   - subsumed label ops whose label is unresolvable → `Rejected {
     LocalData }`;
   - subsumed label ops whose label **number write** was classified as
     rejected → the classified `PushResult` (or omit them from the
     report so they stay queued — `apply_push_report` leaves ops absent
     from the report untouched; omitting is the smallest-diff option
     and matches "let the label ops retry");
   - label ops whose write succeeded → `Applied { echo: None }`. This
     requires mapping op → label → remote number via
     `overlay.label_by_local` so the failing number's ops can be told
     apart from the succeeding ones.
3. Transport failures keep the current `Aborted` behaviour (correct:
   nothing landed that is not re-sendable).
4. Tests: mount 403 and 404 on `assignLabel`/`removeLabel` in an
   update group carrying subsumed label ops, assert one outcome per op
   with the F1/F2 semantics above; plus a `RemoteMissing` arm test.

## Severity

Low probability (needs a classified sub-endpoint failure mid-delta:
ACL change, remotely deleted label, 400), but silent divergence when it
fires, and the F2 shadowing is reachable with an unresolvable label in
the same group — the same production reachability argument as #310.
Suggested priority p2.

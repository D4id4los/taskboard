# Stage-3 comment texts — chunk 2.3 (audit cleanups storage + sync)

Prepared by stage 2 on 2026-11-02, branch `chore/audit-cleanups-storage-sync`
(tip `f80e9129`). Stage 3 posts these at PR-open/merge; adjust commit refs
if a rebase rewrites them.

## Comment 1 — close #272 (storage small cleanups)

> All findings addressed or verdict-recorded, re-verified at `a3987d11`
> before the work (plan:
> `.artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md` §2):
>
> - **F5-04** fixed — duplicated inline rationale pair deleted
>   (`db06fda8`, arm-local pair kept, fn doc keeps the full rationale).
> - **F5-05** fixed — `spawn_storage_actor` doc names the actual awaiter
>   in present tense (`db06fda8`).
> - **F5-06** fixed — the SELECT-count comment is count-free ("one SELECT
>   per table"); the count had drifted twice (8 → 12 → 19) (`db06fda8`).
> - **F5-07** fixed — `decode::remote_board/stack/card/label` destructure
>   with slice patterns; malformed arity now yields `Corrupted`, never a
>   panic (`02b15b3e`; TDD-pinned in repo.rs's first in-file test module:
>   the pins failed against the indexing shape with the predicted panic /
>   silent-ignore, then passed).
> - **F5-03** — already fixed before this round (`1112fb2d`,
>   `execute_batch` is compiler-exhaustive); recorded so nobody re-finds it.
> - **F5-11** — decline re-affirmed: the reminder family is ≈26 % of
>   repo.rs (2 124 lines), below the ~⅓ trigger. Note for whoever
>   revisits: the growth since the audit came from *new* families
>   (habits, time entries, undo/command journal), so the natural seam if
>   the file ever splits is by family (reminders / habits / tracking /
>   journal), not by moving the reminder block out alone.
> - **F5-08** — accepted-risk verdicts re-verified unchanged; no action.
> - **F5-10** — deferred (M-sized text-form unification); spun off into a
>   dedicated issue.

## Comment 2 — close #273 (sync naming cleanups)

> All findings fixed, re-verified at `a3987d11` before the work (plan:
> `.artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md` §2):
>
> - **F6-12 + F12-07** fixed — crate description now "Nextcloud Deck REST
>   synchronization client actor"; the AGENTS §2 row says "Nextcloud Deck
>   REST API actor" (`e3f55f6a`). Grep audit: `webdav` survives only in
>   the two `client.rs` comments documenting *rejected* WebDAV-style
>   userinfo URLs.
> - **F6-13** fixed — architecture.org's crate row now opens by naming
>   the Deck wire client (roughly two-fifths of the crate, exact file
>   inventory) that the actor half consumes (`e3f55f6a`).
> - **F6-14** fixed — `DeckError::Envelope` renamed to `Json`
>   (`82f96a8a`); decision: rename over re-doc because the misleading
>   half was the Display string too, and the rename is compiler-checked
>   (15 sites). One deliberate user-facing consequence: the
>   connection-error class label in `connections.rs` mirrors the variant
>   and now reads "Json".
> - **F6-15** fixed — doc cross-links at both definitions; sync-side
>   links resolve, the domain side names `GroupOutcome` in plain prose
>   (dependency direction) (`f80e9129`).

## Issue 3 — file before closing #272: F5-10 spin-off

Title: `backlog: storage codec text-form unification (F5-10)`
Labels: `p3`, `refactoring`, `audit-v0.1.0`

Body:

> The storage codec hand-writes TEXT forms mirroring domain serde
> knowledge for op tags, sync phases/error kinds, validator keys — plus
> (new since the audit) importance/urgency/insistence and recur/baseline
> — while `ReminderRef` already uses the delegation pattern
> (domain-owned `text_form`/`parse_text_form`, `codec.rs:397`). Unifying
> collapses codec.rs to delegation and folds the 18× clock-parse helper
> (`decode::clock`). Effort M.
>
> Origin:
> `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md`
> (Axis H, F5-10); re-verified 2026-11-02 at `a3987d11`
> (`.artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md`
> §2, D5).

## Comment 4 — flake observation on #560 (post during this round's gates)

> Observed again 2026-11-02 on `chore/audit-cleanups-storage-sync`
> (and reproduced identically on unmodified `main` in the main checkout):
> failed with window `2026-11-01` vs `Utc::now().date_naive()`
> `2026-11-02` — the local-vs-UTC calendar rollover, same signature as
> the original report. Not touched by that round's work (storage/sync
> cleanups only); left to this issue's dedicated triage.

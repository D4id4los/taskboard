# Type-Driven Design & Formal Methods — Project Suitability for `taskboard`

- **Date:** 2026-10-24
- **Input:** `.artifacts/reports/2026-10-24-type-driven-design-and-formal-methods-survey-report.md`
  (the generic survey; its technique IDs A1–A4, B1–B5, C1–C8 and its §6 open
  questions structure this report). Survey facts are cited as *(survey §n / ID)*.
- **Method:** read-only. Every codebase claim cites `file:line` verified against
  the working tree on 2026-10-24 (`main` @ cb02a38). Project docs
  (`docs/*.org`, ADRs, `AGENTS.md`) are used as context and cited as docs, not
  as code evidence. One decision-critical tool question was re-researched
  (Kani edition-2024 support — §C1); nothing else was re-verified beyond the
  survey's same-day retrieval.
- **Status:** uncommitted draft for maintainer review. Per repo convention,
  committing happens on a `docs/...` feature branch after review.

---

## 1. Executive summary

Shortlist ranked by expected value for THIS codebase (solo-maintained,
LLM-agent-driven, stable-channel, TDD culture already running proptest +
insta + weekly Miri + weekly cargo-mutants):

| Rank | ID | Technique | Verdict | Confidence | Benefit in one line | Cost in one line |
|---|---|---|---|---|---|---|
| 1 | A1 | Newtype + smart constructor | **adopt-selectively** (extend existing practice) | high | Converts the 2 known "validate-and-forget" boundaries (`ConnectorId`, UI scale) into compiler-enforced invariants | Hours; no new deps; small config-API churn |
| 2 | C1 | Kani bounded model checking | **pilot-first** | medium | Escalates the strongest existing proptest laws from 256 samples to exhaustive-within-bounds on the pure core | Harness authoring days; per-dependency compile risk (chrono/tracing); one weekly CI job |
| 3 | C8c | proptest | **adopt-now — already in use; maintain** | high | Already the workhorse (40 `proptest!` blocks); the escalation path to C1 is the survey's §5.4 division of labor | Sunk; zero |
| 4 | C8d | cargo-mutants | **adopt-now — already in use; maintain** | high | Weekly test-strength audit of domain+state already wired | Sunk; optional extension to UI pure modules is one workflow edit |
| 5 | C8a | Miri | **adopt-now — already in use; maintain** | high | Weekly 4-job sweep of domain+state with measured budgets | Sunk; runner walls already 46–111 min (testing_strategy §10) |
| 6 | C4 | Prusti | **monitor** | medium | Default panic-freedom would interrogate the ~313 production `unwrap`/`expect`/`unreachable` sites for free | Revival is one week old; version-coupled rustc vs frozen MSRV — wait for cadence |
| 7 | B1 | Lean 4 parallel model | **monitor** | medium | Only plausible target is the conflict-policy algebra (`merge`+`pipeline`), where a Lean model + the existing differential property = the Cedar pattern | Months of ramp + Mathlib CI tax + proof debt under active domain churn; not amortizable today |
| 8 | A2 | Branded/generational indices | **not-worth-it** (existing typed-id practice already covers the need) | high | — | — |
| 9 | A3 | Typestate | **not-worth-it** | high | — | — |
| 10 | A4 | Phantom units (`uom`) | **not-worth-it** | high | — | — |
| 11 | C2 | Verus | **not-worth-it** | high | — | — |
| 12 | C3 | Creusot | **not-worth-it** | high | — | — |
| 13 | C5 | Aeneas / hax | **not-worth-it** | high | — | — |
| 14 | C6 | Flux | **not-worth-it** | high | — | — |
| 15 | C7 | MIRAI | **not-worth-it** (archived) | high | — | — |
| 16 | C8b | loom | **not-worth-it** | high | — | — |
| 17 | B2–B5 | Lean gap/bridge/cost machinery | **monitor** (rides with B1) | medium | — | — |

**Bottom line (≤10 lines).** This codebase is already a heavy user of the
survey's cheapest rungs: typed ids everywhere, smart-constructor newtypes at
the reminder/color boundaries, a 100%-pure domain core, 40 proptest suites,
insta engine snapshots, weekly Miri and weekly mutation testing. The survey's
"if budget exists for exactly one technique" rung (Family A) is therefore not
a new adoption but a small *extension*: two boundaries still validate-and-
forget (`ConnectorId`, `ui.scale`). Family C's heavy tools are almost entirely
excluded by governance (stable toolchain pin, frozen MSRV) and architecture
(tokio actors everywhere outside the pure core) — Kani is the single
exception whose toolchain model fits, and its bounded proofs are precisely the
escalation of the property laws already written. Family B (Lean) has exactly
one candidate module class (the sync conflict-policy algebra) and its
economics fail today: the core is recoverable-not-catastrophic, actively
churning, and nobody in the project speaks Lean. For the §5.3 agent-
maintainability thesis, the highest-leverage acts are the ones already taken
(types, typed errors, property laws) plus the two cheap extensions above;
machine-checkable proof infrastructure would add spec debt faster than it
adds agent scaffolding at this project's size and cadence.

---

## 2. Current-state map

### 2.1 Frame verification (in passing)

- Toolchain pinned **stable** (`rust-toolchain.toml:2`), edition 2024 +
  `rust-version = "1.96"` in `[workspace.package]` (`Cargo.toml:13-15`);
  the MSRV "does not float" per ADR 0003's 2026-10-12 amendment.
- `#![forbid(unsafe_code)]` in every crate except the UI:
  `taskboard-domain/src/lib.rs:29`, `taskboard-state/src/lib.rs:74`,
  `taskboard-sync-nextcloud/src/lib.rs:12`,
  `taskboard-storage-sqlite/src/lib.rs:67`, `taskboard-app/src/lib.rs:30`,
  `taskboard-app/src/main.rs:14`. The UI crate uses `#![deny(unsafe_code)]`
  (`taskboard-ui-slint/src/lib.rs:2`) because Slint-generated code in
  `OUT_DIR` carries nested `#[allow(unsafe_code)]` attributes that a
  `forbid` would overrule (E0453 ×36) — the crate comment records that the
  lint "fires exactly on hand-written `unsafe`" (`lib.rs:11-16`). Grep
  confirms **zero hand-written `unsafe` in any workspace `src/`** today.
- Src-only sizes (wc -l, 2026-10-24): domain 27,414; app 26,534;
  ui-slint 14,025; sync-nextcloud 11,118; storage-sqlite 3,588; state 1,734.
- License posture: `deny.toml` allow-list is permissive-only, with GPL
  accepted **per crate only** for the Slint runtime family (deny.toml
  `[licenses]` block and the GPL note) — any new verification-adjacent crate
  must pass this list (relevant only if `nutype`/`bounded-integer` were
  adopted; this report recommends hand-rolled instead, §3 A1).

### 2.2 Family A — where the practice already exists

| Pattern | Present? | Evidence |
|---|---|---|
| A1 smart-constructor newtypes (private field, fallible mint) | **Yes, in the two most invariant-dense spots** | `ReminderOffset(chrono::TimeDelta)` — "≤ 0 by construction — `before` is the only non-zero construction path" (`taskboard-domain/src/reminders.rs:27-36`, ctor `before()` at `:39-47`); `DeckColor(String)` in the sync adapter — "Writes are strict (`DeckColor::from_hex`); reads are lenient" (`taskboard-sync-nextcloud/src/color.rs:3-5`, `:19`, `:34`) |
| A1-lite total newtypes (private field, no invariant) | **Yes** | `Effort(u64)` — "unsigned by construction" (`taskboard-domain/src/entities.rs:46-51`) |
| A2 typed handles / composite refs | **Yes, systematically** | `local_id!` macro minting `BoardId`/`StackId`/`TaskId`/`LabelId`/`ReminderRuleId`/`AdhocAlarmId` (`taskboard-domain/src/ids.rs:13-65`); `remote_id!` with `pub u64` (`ids.rs:67-98`); per-board-context composite refs `RemoteCardRef`/`RemoteStackRef`/`RemoteLabelRef` because "Deck card ids are only unique per board" (`ids.rs:144-172`); collections keyed by typed ids (`BTreeMap<TaskId, Task>`, `taskboard-state/src/core.rs:124`) |
| A1 **deliberately not applied** (documented leniency) | **Yes** | `Color(String)` — "Lenient color value… Validation is the sync adapter's job (`DeckColor`)" (`taskboard-domain/src/entities.rs:20-24`) |
| A1 **validate-and-forget** (the gap) | **Yes, two known sites** | `ConnectorId(String)`: "Kebab-case and uniqueness are validated at config load; this constructor cannot fail, so `From` stays total" (`taskboard-domain/src/ids.rs:100-125`) — validation lives in `AppConfig::validate` (`taskboard-app/src/config.rs:853`, predicate `is_kebab_name` at `:1103`, typed error `:649`) but the type thereafter carries no proof. Same shape for `ui.scale`: range-checked once at load (`config.rs:959-960`) then a bare `f64` flows on |
| A3 typestate / A4 phantom units-tags | **Zero** | No `PhantomData` anywhere in workspace `src/` (grep, 2026-10-24). State machines are encoded as enums-with-data + pure transition functions instead: `SyncPhase` (`taskboard-domain/src/state.rs:144-155`) with the dominance fold `phase_rank`/`aggregate_rows` (`state.rs:273-306`); the UI view machine `ViewId`/`Flow`/`ViewState` (`taskboard-ui-slint/src/view_state.rs:26-71`, "at most one — edit XOR move" at `:37-39`); the keymap resolver's prefix-accumulation state machine (`taskboard-ui-slint/src/resolver.rs:18-60`) |

### 2.3 Where B/C candidates are plausible — the pure core

`taskboard-domain` is pure by charter and by manifest: "zero network, zero
UI, zero persistence, and zero asynchronous runtime dependencies"; merge
functions "never read the clock or generate ids — time and identity are
always data passed in" (`taskboard-domain/src/lib.rs:3-8`). The pure,
total, clock-injected modules a verifier could chew:

| Module | Size | Content | Existing machine-checked coverage |
|---|---|---|---|
| `days.rs` | 197 lines | `day_window` — total, deterministic day-boundary math, contract-pinned (D-F6) (`taskboard-domain/src/days.rs:24-36`) | proptest (1 block) |
| `reminders.rs` | 1,728 lines | `ReminderOffset`, occurrence arithmetic, `due_firings` evaluator (`reminders.rs:585`) | proptests incl. `occurrence_of_is_total_and_saturating` (`reminders.rs:1502`), `due_firings_are_sorted_by_occurrence_then_reminder` (`:1166`) |
| `merge.rs` | 2,407 lines | Per-entity conflict-policy algebra R1–R9, pure/total/clock-free (`merge.rs:3-21`) | proptests `merge_is_total_and_strictly_newer_wins` (`merge.rs:2307`), `merge_never_decreases_field_clocks` (`:2284`) |
| `pipeline.rs` | 5,396 lines | Fixed-order composition `apply_sync_report` (`pipeline.rs:408`) | `apply_push_report_equals_the_pipeline_push_phase` (`:3047`) — an existing *differential model-vs-implementation binding*, and `equal_replicas_converge` (`:4308`) |
| `command.rs` | 5,006 lines | `plan_command` — pure, total, clock/id-injected command semantics (`command.rs:6-13`, `:316`) | proptests + engine insta snapshots |
| `push.rs` | 1,638 lines | `plan_pushes` — pure push-coalescing planner with a normative grouping table (`push.rs:3-19`, `:344`) | proptest (1 block) |
| `state.rs` | 1,088 lines | `SyncPhase` fold/dominance (`state.rs:273-306`) | proptests (3 blocks) |

Plus two pure sync-adapter modules: `BackoffPolicy` ("pure retry-backoff
policy… so it can be property-tested", `taskboard-sync-nextcloud/src/backoff.rs:3-5`)
and `DeckColor` (above). On the UI side, `resolver.rs` and `view_state.rs`
are pure Rust with no Slint types (module docs at `resolver.rs:3-8`,
`view_state.rs:3-8`) and are proptested.

Workspace-wide there are **40 `proptest!` blocks** (grep, 2026-10-24), and
`insta` snapshots the engine's `AppState` trees (testing_strategy §5;
`taskboard-state` + `taskboard-app` + UI tiers).

### 2.4 Where actor/async/ArcSwap boundaries rule tools out

- Everything outside `taskboard-domain`'s pure modules is tokio-actor code:
  13 actors/tasks over mpsc/broadcast/oneshot with exactly **one**
  `ArcSwap<AppState>` production write site (`shared.store(Arc::new(core.app()))`,
  `taskboard-state/src/actor.rs:559`, in `publish` at `:553`; the flow model's
  sole-writer enumeration, `docs/flow_model.org` pass 4 / edge E-32).
  Verifiers without async support (C2, C3) and bridges restricted to the
  safe sync subset (C5) are structurally limited to the §2.3 modules.
- Concurrency is message-passing plus ArcSwap; **no hand-rolled atomics or
  locks in production code** — the only `std::sync::atomic` uses are
  test-fake counters (`taskboard-app/src/notifications.rs:314-342`,
  `taskboard-app/src/bootstrap.rs:1196`) and two UI event-loop
  `AtomicBool`s (`taskboard-ui-slint/src/lib.rs:458`, `:494`). There is no
  lock-free data structure of our own to model-check (rules out C8b, §3).
- The engine's central invariant is *encoded, not proven*: "Memory advances
  only by interpreting the very action batch that was persisted… memory ≡
  disk by construction, and a failed `apply` needs no rollback because
  `interpret` never runs" (`taskboard-state/src/core.rs:5-8`, `interpret`
  at `:58`). This is Family-A-style by-construction design already doing
  the work a proof would be asked for.
- Persistence decodes are parse-on-load by design: storage codecs reject
  unknown tags/ids as typed `Corrupted` (`taskboard-storage-sqlite/src/codec.rs:5-10`);
  Deck wire decoding is lenient-toward-server-drift with unknown fields
  ignored (`taskboard-sync-nextcloud/src/model.rs:3-9`); the appdata sidecar
  demotes undecodable modeled keys to verbatim content instead of failing
  the document (`taskboard-domain/src/appdata.rs:9-17`); capability probing
  converges on a conservative flag set "safe by construction"
  (`taskboard-domain/src/capabilities.rs:3-8`).

### 2.5 Existing verification infrastructure (the bar new techniques must beat)

- PR pipeline (`ci.yml`): check, MSRV, deny, test, ui-mcp-smoke, coverage,
  sonarcloud (job ids at `.github/workflows/ci.yml:21,83,122,152,189,221,276`).
- **Miri weekly**, four parallel capped jobs on domain+state with measured
  walls 14–111 min against a 120-min timeout (`.github/workflows/miri.yml:9`;
  testing_strategy §10 table) — the runner budget is already stretched.
- **cargo-mutants weekly** on domain+state
  (`.github/workflows/mutation-testing.yml:9,21,52`).
- cargo-fuzz and ASan are *deferred by ruling* with explicit re-entry
  criteria (testing_strategy §11–12) — the project has a habit of gating
  heavy machinery on demonstrated need.

---

## 3. Per-ID evaluations

Format per ID: applicability → benefit → cost/fit → agent-maintainability →
verdict. Verdict vocabulary: adopt-now / adopt-selectively / pilot-first /
monitor / not-worth-it.

### A1 — Newtype + smart constructor (`ids.rs`-style, hand-rolled)

- **Applicability (extend existing practice).** The house pattern is
  hand-rolled macros/structs, not a derive crate: `local_id!`/`remote_id!`
  (`ids.rs:13-98`), `ReminderOffset` (`reminders.rs:27-60`), `Effort`
  (`entities.rs:46-51`), `DeckColor` (`color.rs:19-47`). The remaining
  un-encoded boundaries are enumerable: (1) `ConnectorId` — validated
  kebab+unique at `config.rs:853` but minted via total `From<String>`
  (`ids.rs:115-125`), so any later `From`-minted value silently bypasses the
  rule; (2) `ui.scale` — range-checked at `config.rs:959-960` then a bare
  `f64` travels through `UiSettings` into the UI crate; (3) smaller: config
  durations already typed (`Duration` + typed `invalid-duration`
  rejections, `config.rs` `[reminders]` handling), `day_start_time` parsed
  once at bootstrap (days.rs doc, `days.rs:28-35`).
- **Expected benefit.** The survey's A1 bug class — "validate and forget",
  re-validation drift, `unreachable!` guards — currently has exactly two
  live instances (ConnectorId, scale). Post-change, an agent (or human)
  editing code to construct a `ConnectorId` from an arbitrary string cannot
  compile it without going through the minting path; the config-load site
  becomes the *only* place the invariant can be satisfied. This is small but
  real: these are the two values every actor and the UI consume.
- **Cost/fit.** Hours, zero new dependencies (hand-rolled matches house
  style; `nutype`'s proc-macro cost and MSRV question — flagged unresolved
  in the survey — are mooted by not adopting it; the project MSRV is current
  stable 1.96 and frozen by ADR 0003). Serde round-trips keep working
  (`Serialize`/`Deserialize` already derived on `ConnectorId`, `ids.rs:104`).
  The one cost is API churn: `AppConfig` fields/consumers that pass
  `String`/`f64` switch to the minted types.
- **Agent-maintainability.** Strictly positive per survey §5.3b: the rule
  becomes visible and enforced instead of documented; agents cannot
  quietly re-widen a constructor. No error-message or bounds-soup risk
  (plain structs, no generics).
- **Verdict: adopt-selectively (high confidence).** Strongest argument
  against: the values already flow through typed config structs and are
  validated at the single load site, so the marginal bugs prevented are
  future-construction-site bugs, which are rare — the payoff is
  agent-scaffolding (compile-time intent), not observed bug classes.

### A2 — Branded/generational indices, typed handles

- **Applicability.** The useful half of A2 is already practiced: distinct
  typed id kinds (`ids.rs:41-98`) plus board-context composite refs where
  remote ids are only per-board-unique (`ids.rs:144-172`), and typed-id-keyed
  `BTreeMap`s instead of arenas (`core.rs:124`). There are **no arena
  containers, no slot recycling, no index reuse** anywhere in the workspace
  (state is `BTreeMap`-of-entities, tombstoned by flag — `entities.rs:16-18`),
  so the ABA/generation bug class A2's runtime half targets does not exist.
  Compile-time branding (`generativity`/`GhostCell`) addresses arena-mixing,
  which has no instance here.
- **Benefit.** None beyond what the existing newtype-index practice already
  provides; the survey itself notes the typed-handle half is the compiler-
  error half (survey §A2, matklad pattern).
- **Cost.** Guard-lifetime ergonomics would actively fight the snapshot-
  sharing architecture (`Arc<AppState>` clones cross threads and outlive
  any scope-guard brand).
- **Agent-maintainability.** Branding's region restructuring would be the
  kind of cascading refactor agents handle worst (survey §5.2).
- **Verdict: not-worth-it (high).** Strongest argument against: none of
  substance — the bug class is already prevented by cheaper means in this
  codebase.

### A3 — Typestate via `PhantomData<S>`

- **Applicability.** The three real state machines (`SyncPhase`, the UI
  `ViewState`/`Flow`, the keymap resolver) are enums-with-data with pure
  transition functions returning outcomes (`view_state.rs:58-66`
  `FlowOutcome` applied by the adapter; `resolver.rs:60` `feed` answering
  `Resolution`). Zero `PhantomData` today.
- **Benefit.** Wrong-state operations here are already compile-time or
  cheaply-runtime errors: `Flow` makes "edit XOR move" unrepresentable as a
  data invariant (`view_state.rs:37-44`); the resolver's `Pending/Aborted`
  are *defined transitions* the adapter must handle (`resolver.rs:22-33`).
  Typestate would add: consume-self APIs incompatible with the
  snapshot/clone-heavy style (`ViewState: Debug, Clone, Default`,
  `view_state.rs:69-71`; `AppState` shared as `Arc` snapshots), and the
  documented bounds-soup/error-message degradation (survey §A3 costs) — the
  exact failure mode that strands LLM agents (survey §5.3d).
- **Cost.** API churn across adapter/router for zero invariant gain;
  proptest/insta suites would need reworking against consumed-self APIs.
- **Agent-maintainability.** Negative: harder diffs, worse diagnostics,
  snapshot flows broken by ownership-moving transitions.
- **Verdict: not-worth-it (high).** Strongest argument against: the current
  enum encoding already achieves "illegal states unrepresentable" where it
  matters, at lower churn.

### A4 — Phantom units / tags (`uom`)

- **Applicability.** The workspace's quantity inventory is tiny: `Effort`
  seconds (`entities.rs:46-51`), stack-column widths in px
  (`view_state.rs:604`, `widths.rs` constants), UI scale as a bare ratio
  (`config.rs:49-53`), durations as `chrono::Duration`. No mixed-unit
  arithmetic exists that has ever been confused — the bug class A4 rules
  out (dimensional misuse) has no surface here.
- **Benefit.** `uom` would buy nothing not already enforced by
  `Effort(u64)` + px-named fields + the config-boundary range check.
- **Cost.** Macro/generics compile-time weight in a workspace whose
  kiosk-budget goals include build sanity; `uom`'s compile-time cost is
  flagged-unmeasured in the survey (do not build on it either way — the
  point is the benefit is ~0).
- **Verdict: not-worth-it (high).** Strongest argument against: none —
  three quantities, each already either newtyped or boundary-checked.

### B1 — Lean 4 parallel model

- **Applicability.** Exactly one module class qualifies under the
  documented-deployment profile (small, pure, stable-ish, high-stakes —
  survey §B1): the conflict-policy algebra, `merge.rs` (2,407 lines) +
  `pipeline.rs` (5,396 lines incl. tests), the analog of Cedar's policy
  semantics. Notably, the binding half of the Cedar pattern *already
  exists here*: `apply_push_report_equals_the_pipeline_push_phase`
  (`pipeline.rs:3047`) is a model-vs-implementation differential property,
  and `equal_replicas_converge` (`:4308`) is the convergence law Cedar's
  Lean spec proves.
- **Expected benefit.** Upgrades those laws from sampled (256-case proptest
  default) to universally proven about the model; gives agents a
  kernel-checked spec to edit against (survey §5.3a). But the marginal
  guarantee over the existing suite is the tail of the input distribution,
  on a policy whose worst-case failure is a suboptimal merge *recoverable
  by design* (write clocks preserve data; `ReconcileByTitle` and
  board-level reconcile verbs exist — `docs/architecture.org` commands
  table) — not the catastrophic/adversarial profile every documented Lean
  deployment targets (survey §B1: "policy interpreters, hashes, field
  arithmetic"; §B5 "proportionate when… errors are catastrophic").
- **Cost/fit.** A second toolchain (Lean + Mathlib pin + `lake exe cache
  get` CI dance, survey §B3), proof maintenance under a domain that is
  *actively churning* (multi-board wave 2026-10-20, reminder rules ADR
  0013, capability seam 2026-10-22 — the amortization window §6 Q8 asks
  about is short), a ~14:1 Lean:Rust ratio at SymCrypt scale (survey §B5),
  and no Lean expertise in the project (solo maintainer + agents).
- **Agent-maintainability.** The §5.3 evidence cuts both ways: AI-written
  kernel-checked proofs are now real (SymCrypt, zkEVM), but specification
  design and loop invariants remain human work, `sorry` debt must be
  CI-tracked, and two of five agents never finished proof projects (survey
  §5.3c). For a solo maintainer this converts domain churn into proof debt.
- **Verdict: monitor (medium).** Strongest argument against adoption: the
  amortization window — the core most worth proving is also the core most
  likely to change quarter-to-quarter, so proofs would be re-broken faster
  than agents could profitably re-prove them. Re-trigger: if the conflict
  policy stabilizes for ~2+ quarters *and* a correctness incident in it
  occurs that the proptests miss, model exactly `merge.rs` first, keeping
  `pipeline.rs:3047` as the differential binding.

### B2 — The proof–implementation gap (extraction fidelity, drift)

- **Applicability.** Only relevant under a B1 adoption; with no extraction
  pipeline, the two structural mitigations that would matter are already in
  place in embryo: the differential property (`pipeline.rs:3047`) and
  vector-pinned decoding fixtures (Deck wire fixtures, testing_strategy §8
  Tier 0). Version-skew and arithmetic-semantics risks (survey §B2) would
  become live only with a model to skew.
- **Verdict: monitor (rides with B1).** One-line rationale: gap-management
  machinery has no host to protect today; the survey's own mitigations
  list is the checklist if B1 ever fires.

### B3 — Lean 4 as a verification substrate

- **Applicability.** Same host question as B1. The standing CI tax (monthly
  Lean × Mathlib lock, cache-gated builds — survey §B3) lands on the same
  runner budget already carrying 46–111-minute weekly Miri walls
  (testing_strategy §10).
- **Verdict: monitor (rides with B1).**

### B4 — Rust↔Lean bridges (Aeneas / hax / Anneal)

- **Applicability.** Aeneas/hax require a pinned nightly + `rustc-dev` in
  *this* repo's build (survey §B4) — incompatible with the stable pin
  (`rust-toolchain.toml:2`) and the MSRV non-floating amendment (ADR 0003);
  `#![forbid(unsafe_code)]` makes Anneal's unsafe-spec niche empty (zero
  hand-written unsafe, §2.1). hax's critique exists but is flagged-unassessed
  in the survey — not relied on here.
- **Verdict: not-worth-it (high).** Strongest argument against: they
  front-load Family B expertise (survey §C5's own framing) that the project
  does not have, to solve a pipeline problem (hand-maintained models) the
  project does not have.

### B5 — Cost model for proof work

- **Applicability.** Applied throughout this report: the only quantified
  ratio (14:1, survey §B5) against a 2.4–5.4-kloc target yields a
  34–75-kloc proof artifact to review and maintain — larger than the
  entire `taskboard-state` + `taskboard-storage-sqlite` crates combined.
  AI leverage reduces the *authoring* cost, not the *review/maintenance*
  cost, and the human remainder (spec design, invariants) is precisely the
  scarce resource here.
- **Verdict: not-worth-it as an adoption item; the model is used as the
  report's arithmetic.**

### C1 — Kani

- **Applicability.** The pure-core modules of §2.3, minus their tokio
  wrappers — Kani's documented gaps (no `await`, partial closures/`Drop`,
  no concurrency — survey §C1 feature table) exclude exactly the actor
  layers that every other heavy tool also excludes, and include exactly the
  modules already written verifier-style: total, injected clocks/ids,
  no IO. Concrete first targets: `days::day_window` (`days.rs:36`),
  `reminders` occurrence/saturation arithmetic (`reminders.rs:495-585`),
  `BackoffPolicy::delay` (`backoff.rs:41`), `aggregate_rows` dominance
  (`state.rs:289`), `ReminderOffset::parse_text_form`/`text_form`
  round-trip (`reminders.rs:69-97`), and bounded-input runs of
  `plan_pushes`' grouping table (`push.rs:344`). The three domain
  `unreachable!`s (`pipeline.rs:154`, `:175`, `merge.rs:376`) are
  structurally-obvious match-exhaustiveness shortcuts, not data invariants
  — not Kani targets in themselves.
- **Expected benefit.** The existing property laws are sampled (256 cases
  by default, capped 2–128 under Miri); a `#[kani::proof]` harness of the
  same law is exhaustive within unwind bounds — the survey's §5.4
  "escalation for a critical property" applied to properties that already
  exist and are already trusted. New bug classes covered: arithmetic
  overflow/panic paths in chrono-adjacent date math on the input
  distribution sampling never reaches (saturating edges are proptested at
  `reminders.rs:1502`, but only on sampled instants).
- **Cost/fit — the decisive facts.** (a) Governance: Kani "pins and manages
  its own nightly per release; it does not require your project to switch
  toolchains" (survey §C1) — the only heavy tool compatible with the
  stable pin + frozen MSRV (ADR 0003). (b) **Edition 2024: supported since
  Kani 0.68.0** ("Updated to Rust edition 2024", model-checking releases
  page, retrieved 2026-10-24 — https://github.com/model-checking/rmc/releases)
  — this was the survey's open tool question that decides the verdict;
  resolved positively. (c) Residual risk is per-dependency: proc-macro and
  foreign-function friction with `chrono`/`serde`/`tracing` under Kani's
  feature-support matrix
  (https://model-checking.github.io/kani/rust-feature-support.html);
  harnesses are `#[cfg(kani)]`-gated so a dependency that refuses to verify
  is contained, not project-blocking. (d) CI: a weekly out-of-band job
  mirroring `miri.yml`'s four-job pattern; the survey's cadence wobble
  (8 months between 0.67 and 0.68) is a monitoring note, not a blocker.
- **Agent-maintainability.** Best-in-family per survey §5.3d: harnesses are
  property-test-shaped (`kani::any()`), counterexamples convert to plain
  tests (`concrete-playback`), failures are ordinary CI failures. The one
  documented silent-failure mode — unwind bounds silently over-approximating
  after loop growth (survey §5.2) — is checkable and must be budgeted in
  the harness docs.
- **Verdict: pilot-first (medium confidence).** Strongest argument against:
  the existing 40-property suite already pins these laws, so the pilot's
  marginal catch is the sampled-vs-exhaustive tail — if chrono/serde
  symbolism proves slow or unsupported in practice, the pilot honestly
  terminates in "proptests were enough", which is an acceptable outcome and
  the reason this is a pilot and not an adoption.

### C2 — Verus

- **Applicability.** Would be limited to the §2.3 pure modules; async is
  unsupported, `Mutex`/`RwLock` unsupported (survey §C2 features page) —
  the actors are out by construction.
- **Cost/fit.** A rustc fork (1.99.1) replaces the stable pin for any code
  it touches — directly against ADR 0003's toolchain governance; proof
  engineering (ghost state, lemma chains, quantifier triggers — survey §C2)
  is a specialist discipline with no practitioner in the project; the
  target modules are under active churn (§B1).
- **Agent-maintainability.** Verus has the largest LLM-assist literature
  (survey §5.3c), which is the one point in its favor — but agent-written
  Verus proofs still need human spec design and review capacity that a
  solo maintainer of a task manager cannot fund for recoverable-risk
  invariants.
- **Verdict: not-worth-it (high).** Strongest argument against: fork-based
  toolchain + specialist proof discipline for a core whose failure profile
  is recoverable, on a stable-pin project — the governance mismatch alone
  is disqualifying.

### C3 — Creusot

- **Applicability.** Same target set as C2; async errors out cleanly
  (survey §C3).
- **Cost/fit.** Pinned `nightly-2026-08-03` + Why3 in *project CI* (the
  you-pin-it regime, survey §5.5) — rejected by the same governance rule;
  self-declared "still very experimental" (survey §C3, the project's own
  README words).
- **Verdict: not-worth-it (high).** Strongest argument against: on every
  axis C1 dominates it for this project — same bounded/functional spectrum,
  managed toolchain instead of a project-carried nightly.

### C4 — Prusti

- **Applicability.** Default panic-freedom checking over the domain crate
  would interrogate the ~313 production-side `unwrap`/`expect`/`unreachable`
  sites (counted outside `#[cfg(test)]` modules, 2026-10-24; the domain's
  production files carry almost none — the density is in app-layer command
  handlers and serde-adjacent code, e.g. `taskboard-app/src/commands/mod.rs`
  and `taskboard-app/src/config.rs`) plus the arithmetic edges C1 targets.
  No async in the target modules, so Prusti's unclear async story doesn't
  bite *there*.
- **Cost/fit.** Precompiled binaries; each version supports a specific Rust
  range (survey §C4) — a live constraint against a frozen-MSRV project, but
  a manageable one *if* cadence re-establishes.
- **Status discipline.** The survey's flagged item — revival is one week
  old (2026-10-20-24 releases) after ~2.5 dormant years — is treated as
  unproven, per the survey's own instruction.
- **Verdict: monitor (medium).** Strongest argument against (today):
  betting CI on a project whose maintenance signal is seven days old.
  Re-evaluate after ~two quarters of sustained releases.

### C5 — Aeneas / hax

Covered under B4 — extraction pipelines only pay in a proof-assistant
workflow that does not exist here; nightly + `rustc-dev` pins rejected by
governance.
**Verdict: not-worth-it (high).**

### C6 — Flux

- **Applicability.** Refinement types would express the few range
  invariants the codebase has (scale bounds, kebab names, non-positive
  offsets) — all of which A1 hand-rolling covers cheaper and stable-only.
- **Cost/fit.** Pinned nightly, source-only build, Z3 on PATH, known ICEs
  with `#[trusted]` escape holes (survey §C6) — the silent-`#[trusted]`
  failure mode (survey §5.2) is precisely the artifact class an
  agent-driven repo must not grow.
- **Verdict: not-worth-it (high).** Strongest argument against: its whole
  value proposition over A1 is checking refinements *through operations*,
  and this codebase has almost no operation-dense refinement surface —
  the quantities are boundary-parsed and then inert.

### C7 — MIRAI

Archived 2024-08 (survey §C7). **Verdict: not-worth-it (high).** No
discussion warranted.

### C8a — Miri

- **Already in use**: weekly four-job sweep on domain+state with measured
  per-family budgets and cap-change discipline (`.github/workflows/miri.yml`;
  testing_strategy §10). The `forbid(unsafe_code)` posture + zero
  hand-written unsafe (§2.1) means its marginal scope here is dependency
  behavior on executed paths — already exactly what the weekly job does.
- **Verdict: adopt-now — already in use; maintain (high).** Extension
  candidates (storage codec tests under Miri) are bounded by sqlx's
  C-FFI, which Miri cannot chase (testing_strategy §10's existing
  `sqlite_smoke` gate). Strongest argument against more spend: none —
  but also no headroom for more scope at current runner walls.

### C8b — loom

- **Applicability.** Nothing to model: no hand-rolled lock-free structures,
  no custom atomics; production sharing is `ArcSwap` (a third-party
  primitive with one write site, `actor.rs:559`) and tokio channels — and
  loom models sync primitives only, not tokio (survey §C8b).
- **Verdict: not-worth-it (high).** Strongest argument against: none —
  the code it protects against writing bugs in does not exist here.

### C8c — proptest

- **Already in use**, heavily: 40 `proptest!` blocks with named laws
  (§2.3); persistence of regression seeds; Miri-interpreted caps with
  recorded budget rulings (testing_strategy §10).
- **Verdict: adopt-now — already in use; maintain (high).** The only
  guidance this report adds: keep the survey's §5.4 division of labor —
  the C1 pilot is the escalation path for whichever law next produces a
  sampled-but-worrying near-miss. Strongest argument against: none.

### C8d — cargo-mutants

- **Already in use**: weekly sweep over domain+state with surviving-mutant
  reports (`.github/workflows/mutation-testing.yml:21,52`;
  testing_strategy §13).
- **Verdict: adopt-now — already in use; maintain (high).** One cheap
  extension worth a maintainer decision: the UI crate's pure modules
  (`resolver.rs`, `view_state.rs`, `presentation.rs`) are proptested
  presentation logic of the same character as the current targets; adding
  them is a one-line workflow change plus a triage pass. Strongest
  argument against: mutant-runtime cost on a crate whose tests include
  component journeys (Tier 2) — scope the target list to the pure modules
  if attempted.

---

## 4. Answers to the survey's §6 open questions

Answerable against the codebase (each numbered as in survey §6):

1. **Boundary inventory.** Untrusted inputs enter at: (a) Deck REST
   responses — lenient serde decode, unknown fields ignored, colors strict-
   on-write/lenient-on-read (`model.rs:3-9`, `color.rs:3-5`); (b) SQLite
   rows — typed `Corrupted` rejections on unknown tags/ids (`codec.rs:5-10`);
   (c) config file/env — strict tables, typed `ConfigError` rejections
   incl. kebab connector names and scale range (`config.rs:10-12`, `:853`,
   `:959-960`); (d) CLI args (clap-typed) and IPC length-framed JSON
   (`docs/flow_model.org` E-25; `taskboard-app/src/ipc/`); (e) the secret
   store (`secrets.rs`, redacting `AppPassword`). Single-value/A1-encodable
   invariants re-checked or assumed at multiple sites: connector-name
   kebabness (checked once, then trusted — `ids.rs:100-103`), UI scale
   range (checked once, `config.rs:959`), reminder text forms
   (`Option`-returning parsers, `reminders.rs:97`, `:166`). Relational
   invariants (not A1-encodable): memory≡disk (`core.rs:5-8`), per-field
   LWW ordering (`merge.rs:8-10`), outbox/pipeline fixed order
   (`pipeline.rs:3-9`), phase dominance in the aggregate fold
   (`state.rs:272-306`).
2. **Persistence/serde policy.** Parse-on-load *is* the policy at both
   persistence boundaries (§2.4). The only "silently trusted" values are
   the deliberately-lenient ones (`Color(String)` at `entities.rs:20-24`,
   lenient `DeckColor` reads, verbatim appdata passthrough
   `appdata.rs:9-17`) — each an explicit ruling, documented at the type.
   Whether parse-on-load of *existing deployed* stored data would fail is
   **resolved by maintainer ruling (2026-10-24): no legacy databases
   exist**, so there is no corpus to audit; the standing parse-on-load
   policy (§2.4) has no historical-debt exposure. By construction the
   remaining failure classes are narrow (unknown enum tags, unparseable
   uuid text — `codec.rs:92`, `:255+`).
3. **Pure-core census, ranked by criticality × stability.** Highest
   criticality × highest churn: `merge`/`pipeline` (proofs a bad trade
   today). Best (criticality × stability × smallness) for Kani-hours:
   `days.rs` (197 lines, contract-pinned), `reminders` occurrence
   arithmetic (totality/saturation properties already written),
   `BackoffPolicy`, `state.rs` fold. Lean-weeks candidate if ever:
   `merge.rs` only. `command.rs`/`pipeline.rs` are too large and too
   churned for proofs; their snapshot+proptest stack is the right tool.
4. **Async surface.** All IO (sync client, storage, IPC, notifier,
   bootstrap) executes inside tokio tasks (`docs/flow_model.org` actor
   inventory, 13 actors); `taskboard-domain` — the only crate worth
   verifying — is 100% sync-pure by manifest and charter
   (`lib.rs:3-8`, `Cargo.toml` deps). So the survey's conditional "if the
   answer is most of it…" resolves *favorably*: the verification-relevant
   fraction is entirely inside C1's supported subset. C1–C6 remain bounded
   to exactly the modules listed in §2.3.
5. **Unsafe inventory.** Zero hand-written `unsafe` in workspace `src/`
   (§2.1); the only sanctioned unsafe is Slint-generated code behind the
   UI crate's `deny`-with-scoped-allows lint configuration
   (`taskboard-ui-slint/src/lib.rs:2-16`). Consequences: Miri's value is
   as-deployed (domain+state sweeps, no FFI reach into sqlx); Kani's
   deref-checking has no hand-written targets; Anneal-style Lean safety
   specs (survey §B2/C5) are irrelevant.
6. **Concurrency inventory.** Message-passing + ArcSwap only; the sole
   production `ArcSwap` write is `actor.rs:559`; atomics exist only in
   test fakes and two UI `AtomicBool`s (§2.4). No loom surface. The
   genuine concurrency-correctness mechanisms are architectural
   (capacity-bounded channels, the serial IPC dispatcher for engine
   fairness — `docs/flow_model.org` E-22) and are covered by the actor
   test stack, not by model checking.
7. **Toolchain tolerance.** Stable-pinned (`rust-toolchain.toml:2`),
   MSRV 1.96 frozen by ADR (ADR 0003 amendment): nightly-pinning tools
   (Creusot, Flux, Aeneas, hax) are incompatible with release engineering
   as governed; Verus's fork likewise; Prusti's per-version Rust coupling
   is tolerable only with a maintained release cadence. Kani's
   managed-toolchain model is compatible — and its bounded-guarantee
   profile is sufficient where heavier tools would be wanted, because the
   existing proptest+mutants stack already supplies sampled behavioral
   coverage; Kani's marginal contribution is exhaustiveness-within-bounds
   on chosen functions, which matches.
8. **Refactoring cadence vs proof budget.** Measured from the repo
   (2026-10-24, `git log --follow`): merge.rs 14 commits, pipeline.rs 30,
   reminders.rs 13, days.rs 1 — but the project is three weeks old
   (earliest commits ~2026-10-07, ADR 0003's date), so the honest reading
   is that *everything* churns because the workspace is new, not that
   these modules are unusually unstable. Consequence unchanged: no stable
   window for a proof investment exists yet, and none can be demonstrated
   this early. The measurement itself is cheap and repo-local — repeat
   the same `git log --follow` count quarterly; a merge/pipeline cadence
   that flattens to near-zero over a quarter is the B1 re-entry signal.
   (This supersedes the draft's initial claim that the question needed
   maintainer history knowledge — the repo answers it.)
9. **Expertise and agent mix.** **Resolved by maintainer ruling
   (2026-10-24): human Lean appetite exists**, and the maintainer reports
   the Lean examples seen so far read as relatively readable. This retires
   the "no expertise exists" objection from the B1 verdict — the standing
   blocker is now only the amortization window (Q8) plus CI tax (Q11,
   itself softened — see below). If merge/pipeline churn flattens, B1
   upgrades naturally from monitor to pilot-first. The kernel-checked +
   CI-visible `sorry`-budget discipline of survey §5.3c remains the
   required shape of any future adoption under the agent-driven workflow.
10. **Where agents get intent today.** Prose docs (`AGENTS.md`,
    `docs/*.org`, ADRs) + typed errors + clippy-pedantic + the CI gates +
    insta snapshots. The codebase already implements the survey's
    recommended experiment for its most important invariants: the
    top invariants *are* types (§2.2) and the engine's by-construction
    rule (`core.rs:5-8`). The residual A1 experiment (mint-typed
    `ConnectorId`/`UiScale`) is Pilot 1 (§5) and doubles as the survey's
    §6.10 measurement: count how many downstream sites the compiler
    forces to update when the mint path changes.
11. **CI budget.** **Resolved by maintainer ruling (2026-10-24): GitHub
    free tier for open source / public projects** — Actions minutes on
    standard runners are free for public repositories, so *credit* budget
    is not the binding constraint. What remains binding is wall-clock
    practicality: runner latency, the Monday-cron collision risk with the
    Miri window (testing_strategy §10's measured 46–111-minute walls), and
    developer attention. Consequence: the Kani pilot (§5, Pilot 2) is
    cheaper to justify than the draft assumed — its cost is harness
    authoring time, not CI minutes; the weekly out-of-band scheduling
    discipline still applies. The Mathlib cache tax (B1) remains a
    latency/pipeline-complexity concern rather than a billing one.
12. **Spec text audit.** The doc-comment contracts are largely *already*
    machine-pinned: `days.rs` D-F6 consumers contract (proptest + fixed-
    stamp discipline), merge R1–R9 (proptests at `merge.rs:2284-2307`),
    pipeline fixed order (`pipeline.rs:3-9`, tested), command semantics
    highlights (`command.rs:12-24`, "pinned per-case in tests"), resolver
    one-answer-per-stroke (`resolver.rs:17`, proptested). What would need
    new machinery to check: totality/universality of the arithmetic laws
    (→ C1 harnesses) and full functional correctness of the policy (→ B1,
    not recommended now). No contract-annotation tool earns its cost on
    this inventory.

**Only-the-maintainer decisions** remaining after the 2026-10-24 rulings:
only §6 Q10's A1 experiment acceptance (now Pilot 1, §5) and the Kani
pilot go/no-go (Q11's budget ruling removed the cost objection; the
remaining cost is authoring time). Q2 is closed (no legacy databases),
Q8 is repo-measurable with a quarterly re-check cadence, Q9 is answered
(Lean appetite exists), Q11 is answered (public-repo free tier).

---

## 5. Pilot proposals (scoped as future plans; no plan files created)

### Pilot 1 — Boundary-mint hardening (A1 extension)

- **Scope/crates:** `taskboard-domain` (`ids.rs` — add a fallible mint,
  e.g. `ConnectorId::from_kebab(&str) -> Result<Self, InvalidConnectorName>`
  reusing `is_kebab_name` semantics; keep the total `From` for
  deserialization of *already-validated* persisted rows, documented),
  `taskboard-app` (`config.rs` — `validate()` mints connector names;
  introduce `UiScale(f64)` minted at the `:959` check and carried in
  `UiSettings`). Downstream signature updates in the UI crate's settings
  hand-over.
- **Effort:** 0.5–1 day incl. test churn.
- **CI wiring:** none new — existing gates (fmt/clippy/nextest/deny/
  coverage) cover it; snapshot diffs should be nil or mechanical.
- **Success criteria:** the only construction paths for `ConnectorId`
  outside deserialization are the minting constructor; `cargo mutants` on
  the touched modules finds no surviving mutant in the validators; all
  existing config tests pass with unchanged accept/reject semantics.
- **Measurement bonus (survey §6.10):** record how many call sites the
  compiler forces to update — that count is this repo's data point for
  the "types as agent scaffolding" thesis.

### Pilot 2 — Kani on the pure core (C1)

- **Scope/crates:** `taskboard-domain` + one sync-adapter module; new
  `#[cfg(kani)]` harness modules (no production-code changes beyond
  gate-compatible refactors if any harness needs one).
- **First harnesses (smallest-to-hardest):** `BackoffPolicy::delay`
  (pure integers, trivial), `ReminderOffset::text_form`/`parse_text_form`
  round-trip, `aggregate_rows` phase-dominance, `days::day_window`
  panic-freedom/totality over symbolic `NaiveTime`/`FixedOffset`, bounded
  `plan_pushes` grouping invariants ("delete subsumes all ops of the
  entity", `push.rs:10-14`).
- **Effort:** 2–4 days authoring + debugging; per-harness unwinding bounds
  documented next to each `#[kani::unwind]`.
- **CI wiring:** new weekly `.github/workflows/kani.yml` mirroring
  `miri.yml`'s structure (scheduled cron + manual dispatch, per-family
  jobs if walls demand); never in the PR pipeline.
- **Success criteria:** ≥3 harnesses report *proven* within their bounds;
  every dependency-induced failure is documented as contained (harness
  dropped or stubbed) rather than blocking; job wall < ~30 min per
  harness family; a counterexample, if found, is converted via
  concrete-playback into a regression test and triaged as a real bug
  (per AGENTS §5: a new test failing can be a production bug). Honest
  termination: if chrono/serde symbolism proves impractical, the pilot
  closes with a written finding that proptests remain the right tool.

### Pilot 3 (optional, tiny) — Mutation-surface extension (C8d)

- **Scope:** `.github/workflows/mutation-testing.yml` target list — add
  the UI pure modules (`taskboard-ui-slint/src/{resolver,view_state,
  presentation}.rs`) to the sweep.
- **Effort:** hours (workflow edit + one triage pass + report under
  `.artifacts/reports/`).
- **Success criteria:** surviving-mutant report produced and triaged;
  if the mutant runtime on the UI crate proves disproportionate, record
  the scoping decision in the report and revert.

---

## 6. Explicit non-recommendations

| Item | Reason (one to three lines) |
|---|---|
| Adopt `nutype`/`bounded-integer`/`refined` for A1 | Hand-rolled matches the house style (`ids.rs` macros, `ReminderOffset`); zero new deps; the crate MSRV questions flagged in the survey become moot; proc-macro compile-time cost buys nothing over a dozen-line struct here |
| A2 machinery (slotmap/generativity/branding) | No arenas, no slot recycling, no index-reuse bug class; typed-id `BTreeMap`s + composite refs already prevent handle confusion at compile time (`ids.rs:144-172`) |
| A3 typestate conversion of `SyncPhase`/`ViewState`/resolver | Enums-with-data already make the illegal states unrepresentable; typestate's consume-self fights `Arc`-snapshot sharing, insta/proptest cloning, and produces the bounds-soup diagnostics that strand agents (survey §5.3d) |
| A4 `uom` | Three quantities total, each newtyped or boundary-checked; no mixed-unit arithmetic exists |
| Family B adoption now (B1–B5) | Recoverable-not-catastrophic failure profile, active domain churn (amortization window too short), zero in-project Lean expertise, runner budget already stretched; the one candidate module (`merge.rs`) and the differential binding (`pipeline.rs:3047`) are recorded as the re-entry point |
| C2 Verus | rustc fork vs frozen-MSRV governance; no async; specialist proof discipline; churn |
| C3 Creusot | Project-CI nightly pin + self-declared experimental; strictly dominated by C1 for this governance model |
| C5 Aeneas/hax | Nightly + `rustc-dev` in this repo; front-loads expertise that does not exist; unsafe-spec tooling (Anneal) has no target — zero hand-written unsafe |
| C6 Flux | Nightly, source-built, ICE-prone with silent `#[trusted]` holes; refinement surface here is nearly empty and A1 covers it stable-only |
| C7 MIRAI | Archived 2024-08 (survey §C7) |
| C8b loom | No hand-rolled sync primitives; message-passing + ArcSwap; loom doesn't model tokio (survey §C8b) |
| cargo-fuzz / ASan | Already deferred by ruling with explicit re-entry criteria (testing_strategy §11–12); this report found no new custom parser or FFI growth that would trigger them |

---

## 7. Decision points — resolved by maintainer rulings (2026-10-24)

All items below were decided by the maintainer in two ruling passes on
2026-10-24; nothing is left open from this analysis.

1. **A1 API churn (Pilot 1): APPROVED.** The maintainer accepts high API
   and general code churn while the project is a single-user passion
   project with a fully internal API surface; blame-churn and multi-file
   edits are explicitly *not* decision variables ("if it improves the
   code, change it freely and change it often"). API stability becomes a
   criterion only for a crate the maintainer explicitly nominates for
   extraction/reuse, at that time. This ruling is now recorded in
   `docs/coding_guidelines.org` (§2, Type-Driven Design Policy) so
   future agent sessions inherit it.
2. **Kani pilot (Pilot 2): APPROVED**, including the proposed triggers —
   opportunistic pull-forward allowed (first sampled near-miss on an
   existing property, or the next substantial touch of `days.rs` /
   reminder occurrence arithmetic); not gated on waiting for production
   bug reports.
3. **Prusti re-evaluation (C4): ACCEPTED** at ≈ 2027 Q2, conditional on
   ~two quarters of sustained release cadence.
4. **Lean ambition (B1): conditional go.** Human Lean appetite exists
   (Q9); the sole remaining blocker is the churn window (Q8). Quarterly
   `git log --follow` re-measure on merge.rs/pipeline.rs; flattening over
   a quarter upgrades B1 from monitor to pilot-first on `merge.rs`, with
   the existing differential property (`pipeline.rs:3047`) as the
   model–code binding.
5. **Legacy-data audit (Q2): CLOSED** — no legacy databases exist.
6. **Churn measurement (Q8): CLOSED as quarterly re-measure** — baseline
   in §4 Q8 (merge 14 / pipeline 30 / reminders 13 / days 1 commits;
   project ~3 weeks old at measurement).
7. **Mutation-target extension (Pilot 3): APPROVED** — add the UI pure
   modules to the weekly cargo-mutants sweep.

Actionable outcomes are tracked as GitHub issues created from this
report on 2026-10-24: #425 (A1 gap-plug: minted `ConnectorId`/`UiScale`,
labels `refactoring`/`p2`), #426 (Kani pilot, `testing`/`p2`), #427
(mutants sweep extension to UI pure modules, `ci`/`p3`), #428 (Prusti
re-evaluation ≈2027 Q2, `testing`/`p3`), #429 (quarterly domain-churn
re-measure as the B1 re-entry signal, `testing`/`p3`). The A1 policy
and churn stance are codified in `docs/coding_guidelines.org` §2.1 and
summarized in `AGENTS.md` §2.

---

*Report ends. Uncommitted by design — committing on a `docs/...` feature
branch per repo convention happens after maintainer review.*

# Type-Driven Design & Formal Verification Techniques for Rust — Survey

- **Date:** 2026-10-24 (all statuses and versions date-stamped as retrieved on this date)
- **Scope:** generic ecosystem survey — deliberately project-agnostic. It makes no
  reference to, and draws no conclusions about, any particular codebase. A separate
  follow-up task will apply it.
- **Method:** primary sources (GitHub repos/releases, official books and guides,
  crates.io/docs.rs APIs, papers) plus practitioner write-ups. Where sources conflict
  or a claim could not be verified, that is flagged inline and in §7. Video sources
  were used via page metadata, descriptions, and transcripts where retrievable (noted
  per entry in the bibliography); none were watched as video.
- **Maturity snapshot caveat:** this ecosystem churns fast. Every release number and
  "last activity" date below was checked against the primary source on 2026-10-24 and
  can be re-verified with the cited URL.

---

## 1. Executive summary

Three families of techniques were surveyed: (A) encoding more domain requirements in
stable Rust's type system with no new tooling; (B) maintaining parallel proofs of pure
algorithms in the Lean 4 proof assistant next to the shipping Rust code; (C) formal
verification tools integrated with the Rust toolchain proper (Kani, Verus, Creusot,
Prusti, Aeneas/hax, Flux, MIRAI) plus the lighter-weight assurance end (Miri, loom,
property testing, mutation testing).

**Family A is production-grade and cheap, and is the clear first rung.** Newtypes with
private fields and smart constructors, branded/generational indices, typestate, and
phantom-unit/tag patterns are all stable-Rust, zero-runtime-cost (or one-time
boundary-cost) techniques with mature supporting crates (`nutype` 0.8.0 2026-09,
`bounded-integer` 0.6.2 2026-07, `slotmap` 1.1.1 2025-12, `uom` 0.38.0 2026-02,
`generativity` 1.2.1 2026-04). Their ceiling is real but low: they prove *local*
invariants (value well-formedness, handle provenance, state-machine shape), not
functional behavior. Costs are compile time (proc macros, heavy generics), API
verbosity, and occasionally poor error messages.

**Family B has crossed from curiosity to industrial practice in 2024–2026**, but only
for small, pure, high-stakes cores. The documented exemplars — AWS Cedar (policy
engine + Lean model + differential testing), Microsoft SymCrypt (16.7 KLOC of Rust
crypto against 237 KLOC of Lean proofs, shipping in Windows insider builds), and a
series of zk-EVM/crypto case studies — all target parsers, policy interpreters,
hashes, and field arithmetic. The economics changed recently: AI proof-writing agents
now close a large share of routine proof obligations (SymCrypt's proofs are
AI-written but kernel-checked; the Runtime Verification zkEVM project reports AI
closing structural and linear-arithmetic lemmas while humans still design specs and
invariants), and one first-hand practitioner report describes validating AI-written
Rust algorithms with AI-written Lean proofs at a cost of "proofs in tens of minutes"
for changes that previously took months. Rust↔Lean bridges exist and are active
(Aeneas, hax) but require pinned nightly toolchains and a safe-Rust-only subset; the
"parallel model" (hand-written second implementation) remains the no-new-tooling
default, with differential/property testing as the glue that keeps model and
implementation from drifting.

**Family C is fragmented and churns — status verification is mandatory before any
adoption decision.** As of 2026-10-24: Kani (bounded model checking; 0.68.0
2026-09-16, pinned nightly) is the most practical "does it ever panic/overflow/UB"
tool, with stabilized function contracts, but a visible release-cadence wobble in
2026 (8 months between 0.67 and 0.68); Verus is the strongest verified-subset
toolchain (weekly releases, rustc fork at 1.99.1, real Microsoft/Asterinas production
use) with high proof-engineering cost and no async support; Creusot is active
(v0.14.0 2026-10-12, pinned nightly-2026-08-03) and self-describes as "still very
experimental"; Prusti was dormant from 2024-02 until a revival in the week of
2026-10-20 — treat its trajectory as unproven; Aeneas and hax are active extraction
pipelines (F*/Coq/HOL4/Lean backends); Flux (refinement types) is active but
source-only with pinned nightly and known ICEs; **MIRAI is archived (2024-08) — off
the table.** The lightweight end (Miri, loom, `proptest`/`quickcheck`, mutation
testing) is mature, cheap, stable-toolchain (Miri: nightly), and complements
everything above.

**For an AI-agent-driven development culture**, the survey finds a consistent thesis
across independent sources: machine-checkable artifacts (types, typestates, contracts,
proofs) are the artifacts agents *cannot quietly rot* — passing tests is not
equivalent to preserved intent, and verifiers turn "agent broke the architecture"
into a CI failure. The same sources also document the failure mode: agents can
discharge easy proof obligations but struggle with hard ones, leaving `sorry`/proof
debt that must be tracked as first-class CI-visible state.

### Comparison table

Guarantee strength: ★☆☆ = local invariant, ★★☆ = bounded/exhaustive behavioral,
★★★ = full functional correctness (relative to a spec). Costs are relative
(−/low/medium/high).

| ID | Technique / tool | Guarantee strength | Authoring cost | Maintenance cost | Toolchain requirement | Maturity (2026-10-24) |
|----|------------------|--------------------|----------------|------------------|----------------------|------------------------|
| A1 | Newtype + smart constructor (`nutype`, `bounded-integer`) | ★☆☆ invalid values cannot exist | low (hand) / low-med (macros) | low | stable | production; active crates |
| A2 | Branded indices / generativity; generational indices (`slotmap`) | ★☆☆ (brands compile-time; generations runtime) | medium | low | stable | production |
| A3 | Typestate `PhantomData<S>` | ★☆☆ wrong-state ops don't compile | medium | medium (API churn) | stable | pattern, hand-rolled; helper crates stalled |
| A4 | Phantom units/tags (`uom`) | ★☆☆ dimensional/unit misuse | medium | low-med | stable | production |
| B | Parallel Lean 4 model (+ Aeneas/hax extraction) | ★★★ for the *model*; unproven gap to Rust | high | high (proof + drift) | stable Rust + Lean toolchain (Mathlib pin); bridges need pinned nightly | industrial at small scale (Cedar, SymCrypt) |
| C1 | Kani | ★★☆ bounded (panics/overflow/UB/contracts within unwind bounds) | medium (harnesses) | medium | its own pinned nightly, managed by `cargo kani` | active; cadence wobble 2026 |
| C2 | Verus | ★★★ verified subset | high | high | rustc fork 1.99.1 (Verus release bundles it) | active weekly; industrial use |
| C3 | Creusot | ★★★ verified subset | high | high | pinned nightly-2026-08-03 + Why3 | active; self-declared experimental |
| C4 | Prusti | ★★☆ panic-freedom + specs; partial correctness | medium-high | ? (revival 2026-10 unproven) | specific Rust versions per release | revival this week; was dormant 2024-02→2026-10 |
| C5 | Aeneas / hax (→ F*/Coq/HOL4/Lean) | ★★☆ model + proofs elsewhere; gap to Rust | high | high | pinned nightly + rustc-dev | active (both repos, 2026-10) |
| C6 | Flux (refinement types) | ★★☆ refinement-checked subset | medium-high | medium | pinned nightly, source-only, Z3 | active research; ICEs known |
| C7 | MIRAI | — | — | — | — | **archived 2024-08; do not adopt** |
| C8a | Miri | ★★☆ UB detection on executed paths | low | low | nightly | official component |
| C8b | loom | ★★☆ concurrency interleavings (models, not real threads) | medium | low | stable | mature, slow-moving (0.7.2, 2024-04) |
| C8c | proptest/quickcheck | ★☆☆ sampled, falsification-only | low | low | stable | mature, active |
| C8d | cargo-mutants | meta: measures test strength | low | low | stable | active (27.1.0, 2026-06) |

---

## 2. Family A — Type-system encoding (stable Rust, no new tooling)

### A1. Newtypes with private fields + smart constructors ("parse, don't validate")

**Mechanism.** A single-field struct with a *private* inner field plus a fallible
constructor means no code outside the defining module can ever produce an instance
that has not passed validation:

```rust
pub struct NonZeroF32(f32);          // field private → unforgeable

impl NonZeroF32 {
    pub fn new(n: f32) -> Option<Self> {
        if n != 0.0 { Some(Self(n)) } else { None }
    }
}
pub fn divide(a: f32, b: NonZeroF32) -> f32 { a / b.0 }  // cannot panic
```

The lineage is Alexis King's "Parse, don't validate" (Haskell, 2019) — a parser
consumes less-structured input and produces more-structured output; validation that
returns the same type proves nothing downstream. The Rust-specific adaptation and the
"make illegal states unrepresentable" maxim are covered in Effective Rust (item 6)
and a February 2026 practitioner post (harudagondi.space), which also demonstrates
`NonEmptyVec<T>` whose `first()` returns `&T` with no `Option`, enums-over-booleans,
and derive-based deserialization guaranteeing field presence.

**Bug class ruled out at compile time:** the *existence* of invalid values anywhere in
the program (through safe code) — and therefore the entire downstream re-validation
class, the `unreachable!()` branches, and the "validate and forget" bug (CVE-2016-0752
is the canonical shotgun-parsing example cited).

**Cannot guarantee:** invariants spanning multiple fields or relations between values;
that third-party trait impls preserve the invariant; that data deserialized
*transparently* (serde `#[serde(transparent)]`, sqlx/diesel transparent mappings)
from a DB or wire format is valid — those paths bypass the constructor unless you
parse on load.

**Machinery: hand-rolled vs derive.** Hand-rolling is a dozen lines per type plus
trait forwarding — fine for a handful of types, noisy at scale. The crate ecosystem
(crates.io API, retrieved 2026-10-24):
- **`nutype` 0.8.0 (2026-09-20; 6.5M downloads; MIT)** — proc-macro generating
  `try_new() -> Result`, an implicit per-type error enum, and a curated derive set
  (Debug/Clone/Ord/Hash/FromStr/Deref/Display/serde…). Sanitizers chain before
  validators (`trim`, `len_char_min`, `regex`, `predicate`, numeric
  `less/greater`/`finite` — `finite` excludes NaN and re-enables `Eq`/`Ord`).
  Documented costs: heavy proc-macro compile time, weak IDE support, owned-`String`-
  only string newtypes, `schemars08` feature does not yet reflect validation rules in
  schemas. Escape hatches: `new_unchecked` (unsafe) and `derive_unchecked` (the
  author's own caveat that generated impls of third-party traits can't be
  invariant-checked).
- **`bounded-integer` 0.6.2 (2026-07-17; 1.4M downloads; ISC)** — two modes: a
  `bounded_integer!` macro generating dedicated structs/enums, or pure const-generic
  types (`BoundedU8<0, 7>`). Feature-gated `serde1` ("making sure all values will
  never be out of bounds" on deserialize), `bytemuck1`, `zerocopy`, `schemars1`
  (bounds → JSON-Schema min/max), `no_std`. Limitation: const-generic variants can
  lack `Default` and get no niche-layout optimizations.
- **`refined` 0.3.1 (2025-03-30)** — composable type-level predicates ("simple
  refinement types; parse, don't validate!"); verifiably present but with small
  adoption (~31k downloads).

**Integration.** Serde: deserialize-as-parse is the point — an invalid payload is a
deserialization error at the boundary. DB: sqlx maps transparent newtypes via
`#[derive(sqlx::Type)] #[sqlx(transparent)]`; diesel via `diesel-derive-newtype`;
both trust the database, so persisted garbage surfaces as either a deserialize error
(good) or a silently-trusted value if the mapping is transparent without validation.
FFI: `#[repr(transparent)]` (RFC 1758) guarantees identical layout/ABI to the inner
type, making validated handle types safe to pass across FFI.

**Costs/benefits.** Runtime cost is one validation at the boundary (this is a boundary
technique, not zero-runtime); compile-time cost comes from proc macros; API cost is
constructor noise. Benefit: the compiler enforces the domain rule at every call site,
forever, for free, and — critically for agent-driven development — the rule is
*visible and enforceable* rather than documented.

### A2. Zero-sized types as proofs/capabilities: branding, generativity, generational indices

**Mechanism (compile-time branding).** An invariant lifetime `'id` acts as an
unforgeable brand: indices branded with `'id` can only be used with the arena that
created that `'id`. The canonical research artifact is the GhostCell paper
(**Yanovski, Dang, Jung, Dreyer — ICFP 2021**; note: earlier drafts/descriptions
circulate an incorrect author list — the verified citation is
[ACM DOI 10.1145/3473597]). `GhostCell<'id, T>` gives zero-cost interior mutability
over shared structures by separating permissions (the token) from data. The same
branding ties indices to arenas: `compact_arena`'s docs state it directly — "the
indices contain an invariant lifetime tag that binds them to their respective arena
so you cannot mix up two arenas by accident" (docs.rs, retrieved 2026-10-24).

```rust
make_guard!(guard);                    // creates a fresh invariant 'id
let mut arena = Arena::with_tag(&guard);
let i: Id<'id> = arena.alloc(x);       // i is branded
// let other = other_arena(i);         // ← does not compile: different 'id
```

Ecosystem (crates.io API, retrieved 2026-10-24): **`generativity` 1.2.1
(2026-04-26; 4.8M downloads)** is the reusable `make_guard!` primitive; **`qcell`
0.5.5 (2025-09-17; 533k; MSRV 1.60)** provides four cell variants, the fastest being
the brand-lifetime `LCell`; **`ghost-cell` 0.2.6 (2024-01-28; 140k)** is the paper's
crate, functional but slow-moving.

**Mechanism (runtime generations).** Catherine West's RustConf 2018 keynote
popularized `(index, generation)` keys: freeing a slot increments its generation, so
a stale handle misses (`get` returns `None`) instead of aliasing the wrong element —
this kills the ABA use-after-free class, but *at runtime*. **`slotmap` 1.1.1
(2025-12-06; 121M downloads; `no_std` since 1.1)** is the standard container family
(`SlotMap`, `HopSlotMap`, `DenseSlotMap`, secondary maps; 32-bit index + 32-bit
generation keys). `generational-arena` 0.2.9 (2023-05; 10.5M) is the simpler
alternative; `slab` 0.4.12 (2026-01; >1B downloads) deliberately has *no*
generations — stale slots can alias, which is the exact bug class this technique
trades away.

**Typed handles/ids.** The newtype-index pattern (matklad, 2018 — rust-analyzer's
`ExprId(u32)`) and rustc's `DefId` (`CrateNum` + `DefIndex`, with `LocalDefId` as a
distinct type) show the pattern at compiler scale: index *type* confusion (using
arena A's handle as arena B's) becomes a compile error.

**Cannot guarantee:** generation checks are runtime; branding constrains the arena's
scope to the guard's lifetime (awkward for long-lived arenas); neither proves
anything about *values*, only about handle provenance. **Costs:** guard-lifetime
ergonomics (branding wants scoped regions), 64-bit keys, `Option`-returning lookups.
**Maturity:** production; slotmap/typenum-tier crates with years of deployment.

### A3. Typestate via `PhantomData<S>`

**Mechanism.** Encode the object's runtime state in a type parameter; transition
methods consume `self` and return the next state, so calling an operation in the
wrong state is a compile error:

```rust
pub struct Conn<S> { sock: UdpSocket, _s: PhantomData<S> }
pub struct Connected; pub struct Bound;

impl Conn<Bound> {
    pub fn connect(self, addr: SocketAddr) -> io::Result<Conn<Connected>> { /* … */ }
}
impl Conn<Connected> {
    pub fn send(self, buf: &[u8]) -> io::Result<Conn<Connected>> { /* … */ }
}
// c.send(..) on a Conn<Bound> does not compile.
```

Cliff Biffle's "The Typestate Pattern in Rust" (2019) is the canonical practitioner
article: wrong-state calls "fail to compile"; state-preserving operations take
`&mut self`; transitions consume `self` and "either produce a new object in a certain
state, or … produce nothing." Historical root: Strom & Yemini, 1986. Real-world
uses: embedded HALs (the Embedded Rust Book's GPIO `Pin<Mode>` chapter — naive APIs
"would allow us to modify certain registers that do not make sense"), protocol and
serializer state (serde's `Serializer`: `serialize_struct` twice or adding fields
after `end` is a compile error), builders, and session types. Will Crichton's
"Type-Driven API Design in Rust" notes cover the API-design framing.

**Bug class ruled out at compile time:** operations in the wrong state
(use-after-close, write-after-body, configuring an input pin as output), and reuse
of a consumed resource. **Cannot guarantee:** sequencing constraints spanning data;
and error paths need design — if a transition can fail, the state must either be
abandoned (`Result<New, E>` drops the old state) or returned in the error variant
(`Result<New, (Self, E)>`) to allow recovery. Biffle's example sidesteps this by
failing *before* the stateful value exists.

**Costs:** API and diagnostics complexity grow with nested typestate parameters —
long generic chains produce poor error messages and trait-bound propagation pain
(widely reported; practitioner consensus — no rigorous study found, flagged). A
dedicated proc-macro DSL exists (`typestate` crate) but is effectively stalled (last
release 0.9.0-rc2, 2021-09-02; 43k downloads) — in practice the pattern is
hand-rolled. Const-generic variants of typestate exist but no canonical reference;
treat them as ad-hoc practice. **Maturity:** pattern-level, stable-Rust, used across
embedded and protocol crates; zero runtime cost.

### A4. Phantom units (quantity types) and phantom tags

**Mechanism.** `PhantomData<T>` associates a struct with a type it doesn't contain —
the vehicle for units, tags, brands, and states. The lineage is Leijen & Meijer's
phantom types (DSL '99, Haskell) and Matt Noonan's "Ghosts of Departed Proofs"
(names that carry proof obligations); King's 2019 post ties these to parse-don't-
validate. Combined with private fields and sealed traits, marker types become
unforgeable capabilities; ZSTs compile to nothing (zero runtime cost). (The
"sealed traits" companion pattern is common practice; the Family A research pass did
not retrieve a dedicated primary citation for it — flagged.)

**Quantity types: `uom`.** Version **0.38.0 (2026-02-14; 14.3M downloads; MSRV
1.68; 525 dependent crates, 203 direct — lib.rs, retrieved 2026-10-24)**. A
`Quantity<Storage, Unit, Kind>` parameterized over unit system and kind;
`Length::new::<kilometer>(5.0) + time` does not compile; values normalize to base
units; the `Kind` parameter prevents cross-quantity arithmetic even at the same
dimension. `no_std`; optional serde; `autoconvert` feature for automatic base-unit
conversion (without it, "zero-cost code is not generated for non-floating point
underlying storage types" — docs.rs). **Compile-time cost:** macro/generics-heavy;
no published uom-specific compile-time benchmark was found (flagged) — the
existence of the `runtime_units` crate, pitched as an escape hatch from exactly this
cost, is indirect evidence it bites. **`typenum` 1.20.1 (2026-05-29; >1B downloads)**
is the type-level-integer substrate under it. `measurements` 0.11.1 (2025-10)
is the lighter alternative with weaker (enum-based) guarantees.

**Phantom tags.** The same mechanism encodes "this `Vec` is sorted," "this id refers
to tenant X," "this handle was validated" — marker-parameterized types with
state-specific method impls. It is the generalization of A1–A3 and inherits their
limits: it proves what the marker encodes, nothing more, and markers must be
introduced by trusted constructors to mean anything.

---

## 3. Family B — Lean 4 proofs alongside the Rust code

### B1. The "parallel model" workflow

**Mechanism.** Keep Rust as the shipping artifact; re-express the pure core (an
algorithm, a policy language, a wire format) as definitions in Lean 4; prove
invariants/theorems about the model; bind model and implementation together with
extraction, differential testing, or both. Layout in practice: a sibling Lean package
(`lake` + `lakefile.toml`, `lean-toolchain` pin, Mathlib dependency) next to the
Cargo workspace; generated or hand-written model files; proof files with theorems;
CI runs `lake build` (with Mathlib cache).

**Documented deployments (all verified against primary sources):**
- **AWS Cedar** (authorization engine): Rust implementation (`cedar-policy/cedar`);
  Lean formalization + proofs in a separate repo (`cedar-policy/cedar-spec`,
  `cedar-lean/`) *plus* `cedar-drt/`, a differential-random-testing harness fuzzing
  the Rust engine against the Lean definitional interpreter. AWS describes the
  workflow as "verification-guided development" (AWS Open Source blog, 2024-04-08).
  Cedar also ships a validated LR(1) parser generator written in Lean
  (`cedar-policy/lean-parser-generator`).
- **Microsoft SymCrypt** (production crypto for Windows): 16.7 KLOC of Rust
  (SHA-3, ML-KEM; AES-GCM, FrodoKEM, ML-DSA in progress) verified against a
  **237 KLOC Lean development** — ~14:1 Lean:Rust — proving safety, panic-freedom,
  and functional correctness on x86-64 and ARM, with the Rust extracted via Aeneas
  and proofs substantially AI-written but checked by the Lean kernel (arXiv 2609.15648,
  2026-09-14; MSR blog 2026-07-13). Shipped in Windows insider builds.
- **Runtime Verification / Ethereum zkEVM**: eight consensus-critical Rust targets
  (Plonky3 FRI folding, Mersenne31/KoalaBear fields, Horner evaluation, RISC Zero
  Merkle verification, a 32-bit ADC via `bv_decide`) extracted into Lean and proved
  against Mathlib-based spec libraries (arXiv 2605.30106, 2026).
- **Axiom / OpenVM**: polynomial constraints extracted from production Rust ZK
  circuits into Lean, proved equivalent to independent NIST specs (FIPS 202 Keccak,
  FIPS 180-4 SHA-256); Axiom's stated trust base explicitly includes "extraction
  fidelity, reference specs, and the Lean kernel/comparator, which still need human
  review" (axiom.xyz, 2026-08-19).
- **Nethermind Halva**: Halo2 circuit constraints extracted from Rust into Lean;
  found a real soundness bug in Scroll's Keccak-256 circuit (`is_final`) — an
  exploitable-in-principle bug found by proof, not by tests (2025-07-02).
- **Cryspen libcrux-iot**: SHA-3 verified via hax's Lean backend "developer-driven"
  with AI assistance (cryspen.com, retrieved 2026-10-24).

**Pattern to note:** every deployment targets a small pure core — parsers, policy
evaluation, hashes, field arithmetic, circuit constraints. None proves a GUI, an
actor runtime, or I/O plumbing.

### B2. The proof–implementation gap

What can drift between the Lean model and the Rust code:
1. **Extraction fidelity** (when a bridge is used): the Runtime Verification paper
   lists extraction gaps and "Lean 4 toolchain drift" among its top engineering
   challenges; gaps were fixed upstream or the Rust was rewritten into monomorphic
   "model" form — i.e., they changed the Rust to make extraction faithful. Unsafe
   Rust is out of scope for current Aeneas/hax pipelines.
2. **Arithmetic semantics**: overflow/panic behavior is real behavior — the FRI
   proof checked "no overflow across 13 operations"; Aeneas embeds results in a
   `Result` monad so edge cases are visible in the model. A hand-written model
   instead inherits whatever arithmetic you *assume*, which may silently differ
   from wrapping/panicking/priority-encoded Rust semantics.
3. **Version skew**: changing one side without the other voids the theorem's
   relevance — a parallel model proves the *model*, as of the last synchronization.

Mitigations in documented practice: differential/property testing of model vs
implementation (Cedar's DRT fuzzes generated policies/requests against both);
executable specs tested against official test vectors (SymCrypt's Lean specs run
against NIST vectors, catching spec-side drift); explicit trust-base review of
extraction fidelity (Axiom); small proven cores so the gap stays narrow.

What remains unproven about the Rust itself: memory safety is *trusted* from Rust's
type system ("Rust provides memory safety; Lean proofs establish functional
correctness" — MSR 2026-07-13); unsafe code, FFI, concurrency, and the compiler
backend are outside the guarantee. Google's `cargo-anneal` (Anneal) — alpha
tooling from the zerocopy team, built on Aeneas — exists precisely to close the
unsafe-Rust gap with Lean safety specifications (crates.io; adoption tracked in
zerocopy issue #3016).

### B3. Lean 4 for software verification, as of 2026-10

- **Toolchain**: monthly releases; latest stable **v4.35.0 (2026-10-12)**;
  v4.30.0 (2026-05-26) added the `sym` tactic and completed the new LCNF compiler
  backend (lean-lang.org release notes).
- **Automation**: built-in `omega` (linear integer arithmetic), `simp`, `aesop`;
  `bv_decide` for bit-vectors (used by the zkEVM proofs); the newer `grind` tactic
  (v4.19.0+) is emerging as unified automation. **`duper`**, a superposition prover
  for dependent type theory "broadly similar to Isabelle's Metis", is maintained
  under leanprover-community and releases in lockstep with Lean (v4.34.1,
  2026-09-26), paired with the `lean-auto` translation layer. **No production
  Sledgehammer-equivalent over Mathlib yet** — closest thing is duper.
- **Mathlib**: >2M lines; a downstream project *must* pin Mathlib's exact
  `lean-toolchain`, and the sanctioned CI path is `lake exe cache get` (~5,000+
  cached files; building from source takes significant time — leanprover-community
  install docs). The monthly Lean cadence × Mathlib version-lock is the standing CI
  tax.
- **Adoption**: AWS (Cedar; Lean FRO investment), Microsoft (SymCrypt), plus
  AI-proof-generation benchmarked at repo scale (VeriSoftBench, arXiv 2602.18307,
  2026-02).
- **Learning curve**: community consensus is that prerequisites (functional
  programming, basic logic/tactics) dominate ramp time — "people who have these
  prerequisites find Lean4 hard but tractable" (community study notes; HN threads).
  Weeks-to-months to productivity is anecdotal only — no controlled study found
  (flagged).

### B4. Rust↔Lean bridges (verified, 2026-10-24)

- **Aeneas** (github.com/AeneasVerif/aeneas; INRIA-origin; POPL 2022, Ho &
  Protzenko, arXiv 2206.07185): Rust → Charon/LLBC → pure functional model →
  **Lean, F*, Coq, HOL4** backends (Lean and HOL4 most mature; **no Isabelle
  backend** — README expresses interest only). Functional correctness is expressed
  via characteristic predicates over the extracted model. Safe-Rust subset; pinned
  nightly toolchain; repo pushed 2026-10-24 with daily nightly releases — very
  active. ICFP 2024 follow-up shows its symbolic translation soundly implements a
  borrow checker.
- **hax** (github.com/cryspen/hax, formerly hacspec; Cryspen): `cargo hax`
  subcommand; backends **F\* and Lean (stable, recommended)**; Rocq, ProVerif,
  SSProve, EasyCrypt experimental. Latest release `cargo-hax-v0.4.2` (2026-10-01);
  repo pushed 2026-10-23. Pinned nightly (`nightly-2025-11-08` + `rustc-dev` per
  its `rust-toolchain.toml`). Users: Cryspen's libcrux (ML-KEM verified with
  hax+F*: panic-freedom, functional correctness, secret independence — ePrint
  2025/142; SHA-3 via the Lean backend). A third-party critique of hax's extraction
  exists ("The Verification Facade", symbolic.software, 2026-04-07); its claims
  were not independently assessed here — flagged.
- **cargo-anneal / Anneal** (Google zerocopy team): unsafe-Rust safety verification
  with Lean specifications, on top of Aeneas; alpha (`anneal expand/generate`);
  adoption in progress in zerocopy (issue #3016).
- **Not verified / do not cite**: no `aws-keccak-verification` repo exists (AWS's
  crypto verification is SAW/Cryptol/s2n-bignum; its Lean flagship is Cedar);
  several company names circulating in informal discussion (Veridis, Arya,
  Aerosync, Prime Intellect, Auxon) have no discoverable Lean+Rust parallel-proof
  work — flagged explicitly since the brief asked to verify, not assume.
- No Lean-FRO-run Rust translator exists beyond the Aeneas ecosystem.

### B5. Cost model

- **Second implementation**: the only quantified ratio is SymCrypt's ~14:1 (Lean
  KLOC : Rust KLOC) for full functional correctness of crypto on two architectures.
  Hand-written parallel models on that model's scale are a second codebase to
  review and keep alive.
- **Proof maintenance under refactoring**: extraction pipelines re-extract and
  replay proofs, so *textual* drift is caught at `lake build`; *semantic* model
  changes invalidate lemma chains in unpredictable patches — the zkEVM repo carries
  tracked `sorry`s as open work. No published quantitative study of proof-breakage
  granularity was found (flagged). Lean+Mathlib monthly drift is a named ongoing
  cost.
- **AI leverage (2026 state)**: SymCrypt's proofs are substantially AI-written,
  kernel-checked; the zkEVM report splits the work — AI provers (Harmonic's
  Aristotle, Logical Intelligence's Aleph) reliably close structural, control-flow,
  and linear-arithmetic lemmas (Aleph autonomously closed two previously-`sorry`
  theorems merged upstream in Feb 2026), while **specification design, loop
  invariants, and algebraic reasoning still need human experts**; four related
  theorems remained open. One practitioner report (Kadie, 2026-09-21 — see §5.3)
  compares five AI agents on real proof work: proofs remained "the weakest link";
  the strongest agent completed weeks-scale proof projects, two of five never
  finished.
- **Expertise**: Lean specialists are scarce (no well-sourced market data found —
  qualitative). **Proportionate when:** the core is small, pure, stable-ish, and
  errors are catastrophic or consensus-critical.

---

## 4. Family C — Formal verification integrated with Rust

### C1. Kani (bounded model checking; CBMC)

- **What it proves**: universal properties *within bounded unwindings* — assertion
  failures, panics (including `unwrap` on `None`), arithmetic overflow,
  out-of-bounds indexing, invalid pointer dereferences, division/modulus by zero,
  plus user-stated properties via harnesses and function contracts. Outcomes per
  check: proven / disproved with counterexample / resource exhaustion (book,
  retrieved 2026-10-24). It checks *safety of all executions* (symbolically), not
  functional correctness unless you write contracts. It deliberately does not
  prioritize all Rust-specific UB: its own feature table marks data races,
  unaligned pointer dereference, pointer-aliasing violations, and invalid-value
  production as **not** checked (docs, retrieved 2026-10-24).
- **Subset/restrictions**: "support for a fair amount of Rust language features,
  but not all (e.g., concurrency)" (book). Feature table: `await` expressions **no**;
  closure types, trait objects, function pointers, `Drop`, `PhantomData`, DSTs,
  patterns — **partial**. Verification time is driven by symbolic exploration:
  even a small `Vec`-using harness yields hundreds of implicit checks; loop
  unwinding bounds (`#[kani::unwind]`) and solver backends dominate runtime
  (debugging-slow-proofs and profiling guides exist in the docs).
- **Toolchain**: Kani pins and *manages its own* recent nightly per release
  ("As part of every release, Kani will synchronize with a recent nightly release
  of Rust" — book); it does not require your project to switch toolchains.
  Installed as the `kani-verifier` cargo crate; `cargo kani` runs verification;
  documented GitHub Actions wiring (install-github-ci.md).
- **Spec/proof organization**: `#[kani::proof]` harnesses live in your code
  (typically `#[cfg(kani)]` modules) with `kani::any()` nondeterministic inputs —
  conceptually a property test that explores *all* values. Function contracts
  (RFC 0009): `#[kani::requires(...)]`, `#[kani::ensures(...)]`,
  `#[kani::proof_for_contract]`; checking a function against its contract is
  separate from proving callers against abstracted contracts. Contracts, loop
  contracts, stubbing (`#[kani::stub]`, incl. trait-method stubbing), quantifiers,
  autoharness, concrete-playback (turn counterexamples into tests) are all listed
  under "experimental features" in the book, with stubbing described as stabilized
  in 0.68 and loop contracts made composable with function contracts in 0.63
  (release notes).
- **Status**: latest release **0.68.0 (2026-09-16**, pins nightly-2026-08-21**)**;
  prior 0.67.0 (2026-01-16) — an **8-month gap** after a monthly cadence through
  2025; still active but cadence wobble is a real signal to monitor (GitHub
  releases, retrieved 2026-10-24). Backing: originally Amazon's model-checking
  team; paper "Kani: A Model Checker for Rust" (arXiv 2607.01504, 2026-07).
  Industrial use: AWS components (Firecracker/s2n-quic verification work is cited
  in Kani materials); specific current deployments beyond AWS were not
  independently verified here — flagged.
- **Costs/benefits**: harness authoring is the cheapest of the heavy tools
  (property-test-like); scaling is the weak point (bounded proofs; large workspaces
  need per-harness budgeting); nightly coupling is managed by the tool itself.

### C2. Verus (SMT-based verified subset)

- **What it proves**: full functional correctness relative to user specifications,
  for the supported subset — "statically verify that executable Rust code will
  always satisfy some user-provided specifications" (guide). Specs and proofs are
  written in Rust syntax inside a `verus!` macro (requires/ensures, forall/exists,
  mathematical `int`/`nat`), type-checked by Rust's own type checker; `spec`,
  `proof`, `tracked` items are erased at compilation — the shipped binary is plain
  Rust.
- **Subset/restrictions** (official features page, updated 2026-05-13, retrieved
  2026-10-24): **not supported**: async/await, `Pin`, user-defined `Drop`,
  `Mutex`/`RwLock` (verified lock implementations exist in `vstd` instead),
  `transmute`, function-pointer types, printing/IO, hardware intrinsics,
  `Debug`/`serde::Serialize` derives. **Partial**: trait objects (`dyn`), exec
  closures (no mutable captures), const generics, iterators, floating point, raw
  pointers (via `vstd`), multi-crate verification, panic-unwinding semantics.
  **Supported**: unsafe blocks, spawn/join, interior mutability patterns via
  `vstd` cells, unions, `Box`/`Rc`/`Arc`, user-defined traits. SMT limits apply:
  "SMT solvers usually cannot automatically perform proofs by induction" — expect
  to write lemma functions and manage quantifier triggers.
- **Toolchain**: Verus is a **rustc fork**, versioned off rustc **1.99.1** (repo
  `rust-toolchain.toml`, retrieved 2026-10-24); users install a Verus release (or
  `vargo`) — your code compiles with Verus's rustc, not upstream stable. Tracking
  upstream Rust is a standing maintenance cost the project pays, not you.
- **Status**: **weekly rolling releases**, latest `0.2026.10.24` (2026-10-24),
  binaries for linux/macos/win × x64/arm64 (GitHub releases, retrieved
  2026-10-24) — very active. Provenance: UCSD/MPI-SWS et al. (OOPSLA 2023 "linear
  ghost types", arXiv 2303.05491; SOSP 2024 "a practical foundation for systems
  verification"; VerusBelt, PLDI 2026).
- **Documented use**: Microsoft (verified storage, VerusBy Example/"verismo",
  proof-synthesis work, a verified mimalloc port), Asterinas OSTD (verified OS
  standard library), Anvil (OSDI 2024, verified cluster controllers), VeriSMo
  (OSDI 2024, verified security module for confidential VMs), verified IronKV,
  node-replication, crash-consistent storage, the verified rlsf TLSF allocator,
  hekate-math (binary tower fields for ZK) (publications-and-projects page,
  retrieved 2026-10-24).
- **Costs/benefits**: highest ceiling among the in-toolchain tools; proof
  engineering is a discipline (separation-logic-flavored reasoning, ghost state,
  lemmas); ergonomic for systems data structures, closed for async — a hard
  exclusion for async-heavy codebases.

### C3. Creusot (+ creusot-contracts; Why3/SMT)

- **What it proves**: functional correctness w.r.t. user-defined contracts and
  logical models, translated to WhyML and discharged by SMT solvers through Why3.
  Contracts are ordinary-looking attributes — `#[requires]`, `#[ensures]`,
  `#[variant]` — written in **pearlite**, a dedicated logical-expression syntax
  (`pearlite!`, snapshots, logical `Int`/`Seq` models), erased at compilation
  (README, retrieved 2026-10-24).
- **Subset/restrictions**: "Creusot is currently best suited for the verification
  of code like data-structures or algorithm implementations, and less so for
  systems which interact heavily with the outside world"; **"Creusot is still very
  experimental software, you should expect some obscure crashes and missing
  features"** (README — the project's own words). Async is unsupported (v0.14.0
  changelog: "Properly error on async" — i.e., a clean diagnostic, not silent
  misbehavior). Closures are partially supported (constructor closures added in
  0.14.0); `Drop`/pointer-heavy code handled via guarded borrows/`PCell`-style
  primitives in `creusot-std`.
- **Toolchain**: pinned **nightly-2026-08-03** (with `rustc-dev`) per repo
  `rust-toolchain`; Why3 + solvers installed via `creusot-setup`; `why3find` for
  CI-grade proof configuration.
- **Status**: releases roughly every 1–2 months — **v0.14.0 (2026-10-12)**, v0.13.0
  (2026-07-30), back to v0.5.0 (2025-10-09) (GitHub releases, retrieved
  2026-10-24) — actively developed (academic team, INRIA-affiliated). Notable
  users: CreuSAT (verified SAT solver), Sprout/cdsat (SMT solver).
- **Costs/benefits**: lower proof-bureaucracy than Verus for functional-correctness
  specs (SMT does more of the work via Why3's reconciliation), but stability and
  subset are weaker; nightly pinning ties your verification CI to specific rustc
  dates.

### C4. Prusti (Viper)

- **What it proves**: by default, **absence of panics** (plus optional overflow
  checks) for all executions; with specifications, pre/postconditions, predicates,
  pure functions, loop-body invariants, pledges, type models, closure specs.
  Explicitly **partial correctness**: "it only verifies that *terminating*
  program executions meet the supplied specification" (user guide, retrieved
  2026-10-24). Spec syntax extends Rust booleans: `result`, `old(...)`, implication
  `==>`, snapshot equality `===`, `forall`/`exists` — attached via the
  `prusti_contracts` crate.
- **Subset/restrictions**: a documented unsupported-features list exists (user
  guide); historically closures were supported, some trait-object and advanced
  generics patterns were not — the guide's list should be re-checked at adoption
  time given the project's just-resumed activity.
- **Toolchain**: precompiled binaries for Ubuntu and Windows/macOS (x64+arm64);
  `cargo prusti` / `prusti-rustc`; Prusti Assistant VS Code extension; each
  Prusti version supports a *specific range of Rust versions* (its own rustc
  coupling) — the operative constraint for CI.
- **Status — verify-before-rely**: the project (now at `prusti/prusti`, formerly
  `viperproject/prusti-dev`; ETH Zurich; Viper/Silicon backend) has a release
  history of **sporadic timestamped releases 2020→2024-02-28, then a ~2.5-year
  gap**, then a **revival in the week of 2026-10-20**: four releases on
  2026-10-23/24 and same-day commits (float casts support, Silicon timeout
  handling, user-guide maintenance) (GitHub API, retrieved 2026-10-24). The task
  brief's suspicion of stalled maintenance was correct until literally this month;
  whether the revival sustains is unknowable today — record as open uncertainty.
- **Costs/benefits**: middle of the ladder — specs cheaper than Verus/Creusot
  (SMT-friendly predicate language, automatic fold/unfold via Viper), ceiling
  lower (no termination; deadlock/concurrency out of scope), toolchain coupling
  medium.

### C5. Aeneas / hax (translation to F*/Coq/HOL4/Lean)

- **What they prove**: not verifiers themselves but extractors: they translate
  (safe-subset) Rust into a pure functional model (via Charon/LLBC) in a proof
  assistant, where you prove functional correctness (Aeneas: characteristic
  predicates; hax+F*: full specs as in libcrux's ML-KEM verification). The
  guarantee lives in the *target* assistant; the Rust↔model link is the extraction
  pipeline (§B2 covers the gap).
- **Requirements**: pinned nightly rustc with `rustc-dev`; safe Rust only (unsafe,
  concurrency, and heavy trait-object code are out of scope today); proof effort
  in the target assistant.
- **Status**: both active on 2026-10-23/24 (repo pushes; hax `cargo-hax-v0.4.2`
  2026-10-01). Aeneas backends: F*, Coq, HOL4, Lean (Lean/HOL4 most mature).
  hax backends: F* and Lean stable; Rocq/ProVerif/EasyCrypt experimental.
  Industrial use: Cryspen/libcrux (F*, Lean), Microsoft SymCrypt (via Aeneas),
  Runtime Verification zkEVM (both, per project reports).
- **Costs/benefits**: the right choice when the team already wants to work in
  F*/Lean/Coq (it turns "parallel model" from hand-maintained to
  pipeline-generated); wrong choice if nobody speaks a proof assistant — it
  *front-loads* Family B expertise rather than removing it.

### C6. Flux (refinement types)

- **What it proves**: refinement-type properties — values of a type constrained by
  logical predicates over the value space (`{v: i32 | v > 0}`-style, refined
  indexes/slices/containers) — checked statically via SMT (Z3 + liquid-fixpoint).
  Stronger than A1's parse-don't-validate (the checker proves the refinements hold,
  including through operations), weaker than full functional correctness.
- **Requirements**: **pinned nightly** rustc (toolchain file; rustup required for
  rustc source access); **source-only build** ("The only way to use Flux is to
  build it from source" — docs, retrieved 2026-10-24; `cargo xtask install`
  produces `flux`/`cargo-flux`); Z3 ≥ 4.15 and liquid-fixpoint on PATH.
- **Status**: active (repo pushed 2026-10-23; no GitHub releases; UCSD group;
  online playground and interactive tutorial). **Known rough edges per official
  docs**: the checker can ICE ("hit an internal error"), with `#[trusted]` as the
  escape hatch; configuration "likely to change".
- **Costs/benefits**: the most ergonomic *static* step beyond the type system
  (specs look like types, not proofs); nightly-only and source-built — a real CI
  and onboarding tax; unresolved-ICE risk on ordinary code.

### C7. MIRAI (abstract interpreter)

**Archived.** `facebookexperimental/mirai` — the repo is **archived, last push
2024-08-22** (GitHub API, retrieved 2026-10-24). MIRAI was Meta's abstract
interpreter for mid-level IR, aimed at taint-like and API-misuse checks
(e.g., string/enum state confusion) with lightweight spec annotations. Any adoption
decision today should exclude it; its niche (cheap static analysis of panic/UB-ish
properties with specs) is now better served by Kani harnesses, Prusti defaults, or
Flux refinements depending on the depth wanted.

### C8. The lighter-weight end of the assurance ladder

- **Miri** (official rustup component, **nightly**): an interpreter for Rust
  *executed* code that detects out-of-bounds access, use-after-free,
  uninitialized reads, invalid values (bad `bool`/enum discriminants), misaligned
  references, intrinsic precondition violations, data races (some, with partial
  weak-memory emulation), experimental Stacked/Tree Borrows aliasing violations,
  and leaks. Its own README is admirably precise about the limit: **"Miri
  fundamentally cannot ensure that your code is sound"** — finding UB proves
  unsoundness, but a clean run only means the executions you ran were clean ("it
  will miss bugs that only occur in a different possible execution"). No FFI, no
  networking, interpreter-slow, `-Zmiri-many-seeds` (default 64) multiplies cost;
  for atomic-heavy code it explicitly defers to loom. Cost: near-zero to adopt
  (`cargo miri test` on the existing test suite) — the highest value-per-dollar
  rung on the whole ladder for unsafe-using codebases.
- **loom** (0.7.2, 2024-04-23; 74M downloads — crates.io API, retrieved
  2026-10-24): exhaustive model checking of *concurrent test code* — explores all
  valid orderings of atomics/locks under the C11 memory model within the test.
  Catches interleaving-dependent races and ordering bugs unit tests can't; does
  not scale to large state spaces (tests must be small models), models rather
  than runs real threads, and does not cover async runtimes (its `loom` future
  support for async remains limited — the crate targets sync primitives).
  Slow-moving but stable; effectively the maintained standard.
- **Property testing — `proptest` 1.12.0 (2026-10-10; 221M downloads) and
  `quickcheck` 1.1.0 (2026-02-10; 76M)** (crates.io API, retrieved 2026-10-24):
  randomized falsification — shrinking counterexamples for stated properties.
  Complements everything above: where Kani proves within bounds and Verus proves
  universally, property tests *sample* and can only falsify — but they run on
  stable, in normal test suites, over full real code (including code no verifier
  supports). `proptest`'s regression-file machinery makes failures sticky.
- **Mutation testing — `cargo-mutants` 27.1.0 (2026-06-02)** (crates.io API,
  retrieved 2026-10-24): generates source mutants and reports which survive the
  test suite — a *meta*-check that measures whether tests (and property tests)
  actually constrain behavior. Catches vacuous tests and silent test rot; says
  nothing about specifications you didn't test. Slow by construction (runs the
  suite per mutant); normally run nightly or on-demand, not per-PR.

---

## 5. Cross-cutting analysis

### 5.1 One assurance ladder

Ranked by guarantee strength vs total cost (authoring + maintenance + CI), with the
project-risk profile where each rung is proportionate:

1. **Types + parse-don't-validate (A1, A4)** — near-zero marginal cost after
   boundary design; rules out invalid-value classes permanently. *Proportionate:
   always.* If budget exists for exactly one technique, it is this one.
2. **Handle provenance (A2), property testing (C8c), Miri on the test suite
   (C8a)** — cheap, orthogonal, catch different classes (handle confusion; logic
   bugs on sampled inputs; UB on executed paths). *Proportionate: always for
   property tests; Miri whenever `unsafe` exists; branding when index/arena
   confusion is a live risk.*
3. **Typestate (A3), loom (C8b), mutation testing (C8d)** — moderate authoring;
   typestate pays off for stateful resources with illegal transitions; loom for
   genuine shared-state concurrency; mutation testing as a periodic audit of test
   strength. *Proportionate: stateful protocol/resource code; concurrent cores.*
4. **Kani harnesses/contracts (C1)** — bounded universal proofs of panic/overflow/
   UB-freedom on chosen functions; the cheapest *exhaustive* guarantee available
   for ordinary (non-`dyn`, non-async) code. *Proportionate: pure algorithmic
   crates, parsers, unsafe glue, and anywhere a single overflow panic is a
   production incident.*
5. **Refinement types (C6)** — static value-range/ordering invariants checked
   across operations. *Proportionate: teams already tolerating nightly, on
   invariant-dense numeric/index-heavy code.* Today's ICE risk caps this rung.
6. **Spec-checking verifiers (C4 Prusti, and C3/C2 in their cheaper modes)** —
   panic-freedom plus functional pre/postconditions. *Proportionate: critical
   pure modules with stable interfaces; Prusti's revival makes even this rung
   provisional until its cadence re-establishes.*
7. **Full functional correctness in-toolchain (C2 Verus, C3 Creusot)** — highest
   in-Rust ceiling; proof engineering is a specialist activity; async is excluded
   (both). *Proportionate: consensus-critical or security-boundary data structures
   with slow churn and expert time available.*
8. **Parallel/extracted Lean proofs (B1, with C5 as the pipeline)** — proofs
   *about a model*, with the gap managed by extraction + differential testing;
   strongest documented ROI when specs are the product (policy languages, crypto,
   circuit constraints) or when AI proof-writing agents are in the loop. The
   Kadie data point (below) is the strongest practitioner evidence that this rung
   got dramatically cheaper in 2025–2026. *Proportionate: small pure cores whose
   failure is catastrophic or adversarially probed.*

### 5.2 Brittleness under refactoring

What breaks, how loudly, and the rework an ordinary feature change causes:

- **Newtypes/units/tags (A1/A4)**: refactors break *loudly at compile time* with
  type errors at every boundary; rework is mechanical (re-parse at new
  boundaries). Ordinary feature change touching an encoded type: minutes-to-hours.
- **Typestate (A3)**: loud compile errors; rework is re-plumbing state parameters
  and trait bounds — an added state or transition touches signatures throughout;
  error messages degrade with nesting. Medium rework; risk of bounds-soup if
  over-generalized.
- **Branded indices (A2)**: loud, but guard-lifetime scoping means restructuring
  *regions*, which can cascade. Medium rework, rare.
- **Property tests (C8c)**: break only when a property is violated or generators
  rot — **silent gaps** are the failure mode (tests keep passing while coverage
  narrows). Mutation testing (C8d) exists precisely to audit this silence.
- **Miri (C8a)**: silent until an executed path hits UB; refactors don't
  invalidate it (it runs the suite), cost is runtime only.
- **Kani (C1)**: harnesses break when signatures change (loud); *bounded* proofs
  can also silently over-approximate after growth in loops/recursion (unwind
  bounds need revisiting — a documented, checkable-but-ignorable hazard). Medium,
  budgetable rework per changed function.
- **Prusti/Creusot/Verus specs (C4/C3/C2)**: loud — a spec violation or failed
  verification is a hard CI failure. But rework is *nonlinear*: adding one
  constructor variant can invalidate quantifier triggers and lemma chains that
  then need expert time. Verus/Creusot proofs are the most expensive to maintain;
  Prusti's default panic-checking is nearly free once set up.
- **Flux (C6)**: refinements update like types (loud) but ICEs can force
  `#[trusted]` — a *silently unchecked* hole; audit `#[trusted]` counts in CI.
- **Parallel Lean model (B1)**: two failure modes. Inside Lean: breakage is loud
  and kernel-checked. Between model and Rust: **silent drift** is the fatal mode —
  the model proves yesterday's algorithm. The documented mitigations are
  structural (extraction pipelines that re-run on every change, differential
  test suites like Cedar DRT, executable specs against known vectors). A parallel
  model *without* one of these bindings is architecture documentation with a
  proof attached — the proof is valid, the relevance decays silently.

### 5.3 Interaction with AI-agent-driven development

This was an explicit focus. Four independent source threads converge:

**a) Machine-checkable artifacts are the artifact class agents cannot quietly
rot.** The LambdaX.1 videos (transcripts, June–July 2026) argue exactly this from
practice: agents "quietly break architecture" while tests stay green — passing
tests are not preserved intent — and the remedy is to make intent *checkable*:
state machines as types (phantom types/ZSTs, "Thinking In Terms Of Types"),
algorithms re-expressed with machine-checked lemmas in Lean ("Explicit Intent vs
Automated Implementation" — well-formedness lemmas, decidability instances,
`sorry`-tracking as explicit debt), and prose specs demoted to *non-load-bearing*
unless each load-bearing sentence is anchored by a checked theorem
("Agents Cannot Handle Markdown Files": free-text spec files accumulate
contradictions and omissions when agents edit them; laws written in Lean + CI
anchoring make agent edits safe). The fourth video (oTDcJcvrfCU) makes the same
point in its title thesis ("why passing tests is not always enough"); only its
description was retrievable, not a transcript — flagged.

**b) Type-driven design as agent scaffolding.** The typestate/newtype patterns of
Family A function as *executable documentation*: an agent navigating a codebase
where `Conn<Bound>` has no `send` method cannot call it; where `NonZeroF32`
exists, no prompt-induced re-validation bug can compile. The harudagondi post
(2026-02-21) and Crichton's notes frame this as API design; for agent-driven
work it doubles as a correctness firewall that operates at compile time, on every
agent edit, with zero CI cost.

**c) AI as proof-writer: now real, with a documented human remainder.** SymCrypt's
237 KLOC of Lean is substantially AI-written with the Lean kernel as the soundness
backstop (MSR, 2026-07-13). The Runtime Verification zkEVM report splits the
labor: AI provers close structural, control-flow, and linear-arithmetic lemmas;
humans design specifications, loop invariants, and algebraic reasoning; four
theorems remained open after AI assistance (arXiv 2605.30106). Verus has an
entire LLM-assisted-verification literature attached (RAG-Verus 2025-02,
AlphaVerus ICML 2025, AutoVerus OOPSLA 2025, "Can LLMs Enable Verification in
Mainstream Programming?" 2025-03). The practitioner deep-dive is Kadie's
"Nine Rules for Vibe Validation of Vibe-Coded Algorithms" (2026-09-21): a
maintainer using AI agents to write both Rust algorithm changes and Lean proofs
for them; year one cost 4,791 lines of Lean to validate 67 lines of Rust (with
AI-written proofs cut by −72% on later cleanup); year two, with better agents, a
10× larger Rust change needed proofs in "tens of minutes" — but across five
agents tried, proofs remained "the weakest link," and two of five agents never
finished the proof work. His rules read as an operating manual for the failure
modes: prove the *model*, bind it to the implementation with differential tests
run in both CI legs, model `BTreeMap` as sorted lists, use `Nat` so
underflow can't enter proofs, track `sorry` count as a first-class metric, and
lint for spec/impl mismatches (a ZST `Monoid`-shaped "lint theorem").

**d) The hindrance side is equally documented**: hard-to-satisfy bounds
(typestate/trait-heavy APIs produce error-message spirals that strand agents);
proof debt agents cannot discharge (`sorry`s that pile up invisibly unless
CI-counted — both the LambdaX.1 and Kadie workflows make the count explicit for
exactly this reason); and verification toolchains (Verus fork, Creusot/Flux/Aeneas
nightly pins, Kani's managed toolchain, Mathlib version locks) that fragment an
agent's otherwise-uniform build/test loop. A pragmatic reading: Family A + Kani
harnesses are agent-*friendly* (compile/test-adjacent signals); Family B/C
full-correctness work is agent-*accelerated* but human-*directed*.

### 5.4 Interaction with a TDD / property-testing culture

Complement, not replace — with a precise division of labor:
- Property tests sample; verifiers prove universally (within bounds or subset).
  The standard escalation for a flaky/critical property is Kani: the same property
  written as `#[kani::proof]` harness with `kani::any()` becomes exhaustive over
  the input domain (Kani's own docs pitch harnesses as property tests that explore
  *all* executions; `concrete-playback` turns counterexamples back into plain
  test cases — i.e., verification failures feed the TDD loop).
- TDD's red-green loop survives verifiers unchanged if specs are written
  test-first: write `requires/ensures`, watch verification fail (red), implement
  until proven (green). Verus/Creusot/Prusti errors are the red light; the
  difference is the green light is a proof, not a sample.
- Mutation testing audits whether the *tests* (property or unit) still constrain
  behavior — the natural quality gate for a TDD culture adopting generated code.
- The Lean parallel model adds a new relationship: the model is an *executable
  spec* that property tests can differentially fuzz against (Cedar DRT is the
  reference implementation of this idea) — property testing becomes the bridge
  that keeps proof and implementation honest.
- What replaces what: nothing replaces the test suite. Even SymCrypt — with full
  Lean proofs — runs its Lean specs against NIST test vectors; even Cedar fuzzes
  Rust against Lean. Proofs change what tests are *for* (regression + binding,
  not primary assurance on proven cores).

### 5.5 Recurring integration concerns (all families)

- **serde round-trips**: deserialize-as-parse (A1 macros, C6 refinements) turns
  invalid payloads into boundary errors — the desirable default; transparent
  derives bypass invariants and silently trust producers. Any adopted encoding
  needs an explicit policy for "trusted" persisted data vs parse-on-load.
- **Database mapping**: same fork in the road — transparent ORM mapping (sqlx
  `transparent`, diesel-derive-newtype) re-imports raw values; verifiers (C2/C3)
  generally cannot chase into SQL, so DB-facing layers stay verified-free and
  must be *wrapped* by proven boundaries.
- **Async runtimes**: the hard wall of Family C. Verus: no async. Creusot:
  errors on async. Kani: no concurrency (`await` unsupported, data races
  unchecked). Flux/Prusti: limited/unclear. In practice verification happens on
  sync cores *called from* async code; the async glue itself is covered only by
  tests, Miri (single-threaded paths), and loom (sync primitives only). Family B
  models the pure logic and ignores the runtime entirely; Family A is
  runtime-agnostic (types compose fine in async fns). This is the single largest
  capability gap in the 2026 Rust verification ecosystem for actor/tokio-style
  architectures.
- **Nightly pinning / toolchain fragmentation**: three distinct regimes — stable
  (Family A, property tests, loom, cargo-mutants), managed-by-tool (Kani installs
  its own pinned nightly; Verus ships its own fork — your tree stays stable),
  and you-pin-it (Creusot, Flux, Aeneas/hax: project CI carries
  `nightly-YYYY-MM-DD` + `rustc-dev`, with rustc-internals churn as recurring
  breakage). Lean adds a second ecosystem clock (monthly Lean × Mathlib lock).
- **Workspace compile times**: Family A costs are real but bounded (proc macros
  in nutype; uom/typenum generics), and amortize poorly in large workspaces only
  at heavy use. Verifiers add a second build dimension: Kani/SMT time scales with
  harness complexity and unwinding bounds, not just LOC; Verus/Creusot
  verification time concentrates in proof-heavy modules; both warrant a separate
  verification CI job (nightly or per-path-filtered) rather than per-commit
  blocking. Mathlib CI needs the cache dance (`lake exe cache get`).

---

## 6. Open questions for the project-specific follow-up evaluation

The next agent should answer these against the real codebase:

1. **Boundary inventory**: where do untrusted inputs enter (wire, disk, user,
   env/config)? Which invariants are currently re-checked or assumed at multiple
   sites, and which of them are single-value (A1-encodable) vs relational
   (needs A2/A3 or a verifier)?
2. **Persistence/serde policy**: how much data is persisted and reloaded? Would
   parse-on-load of existing stored data currently *fail* (i.e., is there
   already-invalid data that transparent mappings are silently trusting)?
3. **Pure-core census**: which modules are pure, deterministic, and
   change-frequency-low enough to be Kani-harnessed (hours of work) or
   Lean-modeled (weeks)? Rank candidates by (criticality × stability) — the
   SymCrypt/Cedar profile — vs (criticality × churn), where proofs are a bad
   trade.
4. **Async surface**: what fraction of non-UI logic executes inside async
   contexts? (This bounds C1–C6 applicability almost completely; if the answer
   is "most of it," the realistic ladder tops out at Family A + Kani on extracted
   sync cores + property/Miri testing.)
5. **Unsafe inventory**: how many `unsafe` blocks, and of what kind (FFI,
   pointer-aliasing tricks, `get_unchecked`)? Determines Miri value, Kani's
   deref-checking value, and whether the Anneal-style Lean-safety-spec approach
   is even relevant.
6. **Concurrency inventory**: genuine shared-state concurrency (needing loom)
   vs message-passing (mostly covered by typed channels + property tests)?
7. **Toolchain tolerance**: what is the project's MSRV/stability policy, and is
   any nightly pinning (Creusot/Flux/Aeneas/hax) or a fork (Verus) compatible
   with its release engineering? Kani's managed-toolchain model sidesteps this —
   is its bounded-guarantee profile sufficient where heavier tools would be
   wanted?
8. **Refactoring cadence vs proof budget**: pick one module and measure — hours
   spent changing it per month? A proof investment needs an expected
   unchanged-period measured in months-to-years to amortize.
9. **Expertise and agent mix**: is there (or will there be) a human who can
   review Lean/Verus proofs, or is the plan agent-written-human-blessed? The 2026
   evidence says agent-written proofs are viable *only* with kernel checking and
   a CI-visible `sorry`/verification-failure budget.
10. **Where do agents currently get intent?** If architecture/invariants live in
    prose docs today, a trial of encoding the top N invariants as types
    (A1/A3) — and counting agent-introduced violations that the compiler then
    catches — is the cheapest possible experiment to quantify the §5.3 thesis on
    this codebase.
11. **CI budget**: what wall-clock/credit budget exists for a verification job?
    (Determines whether Kani harnesses run per-PR or nightly, and whether
    Mathlib caching is operationally acceptable.)
12. **Spec text audit**: which of the existing behavioral specs (doc comments,
    test names) are machine-checkable today, and what would each need to become
    a contract (`requires/ensures`), a refinement, or a theorem?

---

## 7. Bibliography

### Provided seed sources

- harudagondi.space, "Parse, don't validate and type-driven design in Rust"
  (2026-02-21) — A1 mechanism, NonZeroF32/NonEmptyVec patterns, enums-over-bools,
  typenum/Curry-Howard, CVE-2016-0752, ergonomic limitations.
  https://harudagondi.space/blog/parse-dont-validate-and-type-driven-design-in-rust/
- Friedrich "Fred" A. C. Kadie, "Nine Rules for Vibe Validation of Vibe-Coded
  Algorithms" (2026-09-21) — §5.3c: AI-written Rust + AI-written Lean proofs,
  RangeSetBlaze case study, 4,791-Lean-KLOC/67-Rust-KLOC ratio, five-agent
  comparison, differential-testing bridge, `sorry`-count policy.
  https://levelup.gitconnected.com/nine-rules-for-vibe-validation-of-vibe-coded-algorithms-20db019f5583
  (retrieved via reader; Medium blocks the standard fetcher — noted)
- LambdaX.1, "Thinking In Terms Of Types" (2026-06-19, 10:53) — **transcript
  used**: phantom types, ZST state machines, type-level relations; Rust encoding
  becomes cumbersome → motivation for Lean.
  https://www.youtube.com/watch?v=ZGpxQy0f8vg
- LambdaX.1, "Explicit Intent vs Automated Implementation" (2026-06-16, 7:58) —
  **transcript used**: Lean model of lambda calculus, well-formedness lemmas,
  decidability instances, sorry-driven development as tracked debt, explicit
  intent as the contract for agents.
  https://www.youtube.com/watch?v=tL8hudfqko8
- LambdaX.1, "Agents Cannot Handle Markdown Files" (2026-07-04, 20:31) —
  **transcript used**: prose specs accumulate contradictions under agent edits;
  laws written in Lean with CI anchoring make edits safe.
  https://www.youtube.com/watch?v=33mULl7_PjY
- LambdaX.1, "How Agents Quietly Break Architecture" (2026) — **description
  only** (no transcript retrievable; not watched): thesis "passing tests is not
  always enough; formal models in Lean." Cited only for its stated thesis.
  https://www.youtube.com/watch?v=oTDcJcvrfCU
- (All four videos: same channel; worked from page metadata + transcripts via a
  web reader on 2026-10-24, not from viewing.)

### Family A primary sources

- Alexis King, "Parse, don't validate" (2019-11-05) —
  https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/
- Effective Rust, item 6 (newtype pattern) —
  https://www.lurklurk.org/effective-rust/newtype.html
- Softwaremill, "Mastering Rust Patterns vol. 1: Rust Newtypes" (2026-02-18) —
  https://softwaremill.com/mastering-rust-patterns-vol-1-rust-newtypes
- nutype docs/README + crates.io API (0.8.0, 2026-09-20) —
  https://docs.rs/nutype ; https://crates.io/api/v1/crates/nutype
- bounded-integer docs + crates.io API (0.6.2, 2026-07-17) —
  https://docs.rs/bounded-integer ; https://crates.io/api/v1/crates/bounded-integer
- refined (0.3.1, 2025-03-30) — https://crates.io/api/v1/crates/refined
- GhostCell paper: Yanovski, Dang, Jung, Dreyer, ICFP 2021 —
  https://dl.acm.org/doi/abs/10.1145/3473597 ;
  https://plv.mpi-sws.org/rustbelt/ghostcell/paper.pdf
- qcell docs (0.5.5) — https://docs.rs/qcell ; generativity (1.2.1, 2026-04-26) —
  https://crates.io/api/v1/crates/generativity ; compact_arena docs —
  https://docs.rs/compact_arena ; generativity pattern explainer —
  https://arhan.sh/blog/the-generativity-pattern-in-rust/
- Catherine West RustConf 2018 keynote (generational indices) —
  https://kyren.github.io/2018/09/14/rustconf-talk.html
- slotmap (1.1.1, 2025-12-06) — https://crates.io/api/v1/crates/slotmap ;
  https://github.com/orlp/slotmap ; generational-arena (0.2.9) —
  https://crates.io/api/v1/crates/generational-arena ; slab (0.4.12) —
  https://crates.io/api/v1/crates/slab
- matklad, "Newtype Index Pattern" (2018-06-04) —
  https://matklad.github.io/2018/06/04/newtype-index-pattern.html ;
  rustc-dev-guide HIR/DefId — https://rustc-dev-guide.rust-lang.org/hir.html
- Cliff Biffle, "The Typestate Pattern in Rust" (2019-06-05) —
  https://cliffle.com/blog/rust-typestate ; Embedded Rust Book, typestate &
  state-machine chapters —
  https://docs.rust-embedded.org/book/static-guarantees/typestate-programming.html
  ; Will Crichton, "Type-Driven API Design in Rust" —
  https://willcrichton.net/rust-api-type-patterns/typestate.html ;
  `typestate` crate (stalled) — https://crates.io/api/v1/crates/typestate
- uom (0.38.0, 2026-02-14; MSRV 1.68) — https://docs.rs/uom ;
  https://lib.rs/crates/uom ; typenum (1.20.1) —
  https://crates.io/api/v1/crates/typenum ; measurements (0.11.1) —
  https://crates.io/api/v1/crates/measurements ; runtime_units —
  https://crates.io/crates/runtime_units
- Phantom types: Leijen & Meijer, DSL '99 —
  https://www.usenix.org/events/dsl99/full_papers/leijen/leijen_html ;
  Microsoft Rust patterns training ch. 4 —
  https://microsoft.github.io/RustTraining/rust-patterns-book/ch04-phantomdata-types-that-carry-no-data.html
- serde/DB/FFI: repr(transparent) RFC 1758 —
  https://rust-lang.github.io/rfcs/1758-repr-transparent.html ; Rustonomicon —
  https://doc.rust-lang.org/nomicon/other-reprs.html ; sqlx newtype thread —
  https://users.rust-lang.org/t/how-should-one-use-sqlx-with-newtype-pattern/73172 ;
  diesel-derive-newtype — https://docs.rs/diesel-derive-newtype ;
  transparent-bypass caveat —
  https://www.worthe-it.co.za/blog/2020-10-31-newtype-pattern-in.html

### Family B primary sources

- AWS Cedar: cedar-spec repo (cedar-lean, cedar-drt) —
  https://github.com/cedar-policy/cedar-spec ; AWS blog "Lean into verified
  software development" (2024-04-08) —
  https://aws.amazon.com/blogs/opensource/lean-into-verified-software-development ;
  Cedar DRT/automated reasoning —
  https://www.amazon.science/blog/how-we-built-cedar-with-automated-reasoning-and-differential-testing
- Microsoft SymCrypt: paper arXiv 2609.15648 (2026-09-14) —
  https://arxiv.org/abs/2609.15648 ; MSR blog (2026-07-13) —
  https://www.microsoft.com/en-us/research/blog/verifying-rust-cryptography-in-symcrypt-from-standards-to-code
- Runtime Verification zkEVM report, arXiv 2605.30106 (2026) —
  https://arxiv.org/abs/2605.30106
- Axiom/OpenVM FV (2026-08-19) — https://www.axiom.xyz/blog/fv-keccak-sha2
- Nethermind Halva (2025-07-02) —
  https://www.nethermind.io/blog/formal-verification-of-halo2-circuits-in-lean
- Cryspen libcrux-iot SHA-3 — https://cryspen.com/post/sha3-hax-0-4 ; ML-KEM
  verification — https://cryspen.com/post/ml-kem-verification ; ePrint 2025/142
- Lean 4: releases — https://github.com/leanprover/lean4/releases ; v4.30 notes —
  https://lean-lang.org/doc/reference/latest/releases/v4.30.0 ; Lean FRO roadmap —
  https://lean-lang.org/fro/roadmap/y1 ; Mathlib/install —
  https://leanprover-community.github.io/install/project.html ;
  https://lean-lang.org/use-cases/mathlib ; duper —
  https://github.com/leanprover-community/duper ; lean-auto —
  https://github.com/leanprover-community/lean-auto ; use cases —
  https://lean-lang.org/use-cases/aeneas ; Amazon/Lean FRO —
  https://www.amazon.science/news/amazon-is-investing-in-the-lean-focused-research-organization
  ; VeriSoftBench — https://arxiv.org/abs/2602.18307 ; learning-curve notes —
  https://hackmd.io/@manbankat/HkIJPBB3Ze ; practitioner intro —
  https://blog.lambdaclass.com/if-it-compiles-it-is-correct-almost-an-introduction-to-lean-4-for-zk-systems-and-engineering-2
- Aeneas — https://github.com/AeneasVerif/aeneas ; POPL 2022 paper arXiv
  2206.07185. hax — https://github.com/hacspec/hax (org: cryspen/hax).
  cargo-anneal — https://crates.io/crates/cargo-anneal ; zerocopy adoption
  tracker — https://github.com/google/zerocopy/issues/3016
- hax critique (unassessed, flagged): "The Verification Facade" —
  https://symbolic.software/blog/2026-04-07-cryspen-hax (2026-04-07)
- AWS crypto verification (SAW/Cryptol/s2n-bignum — context for the flagged
  non-existent "AWS Keccak in Lean"): https://github.com/awslabs/s2n-bignum ;
  https://aws.amazon.com/blogs/security/automated-reasoning-and-amazon-s2n

### Family C primary sources

- Kani: releases — https://github.com/model-checking/kani/releases ; book —
  https://model-checking.github.io/kani/ ; checks tutorial —
  https://model-checking.github.io/kani/tutorial-kinds-of-failure.html ;
  feature support — https://raw.githubusercontent.com/model-checking/kani/main/docs/src/rust-feature-support.md
  ; contracts RFC 0009 —
  https://model-checking.github.io/kani/rfc/rfcs/0009-function-contracts.html ;
  experimental features (contracts/stubbing/loop-contracts/autoharness) —
  https://github.com/model-checking/kani (docs/src/reference/experimental) ;
  paper arXiv 2607.01504 (2026-07) — https://arxiv.org/abs/2607.01504
- Verus: repo + releases — https://github.com/verus-lang/verus ; guide —
  https://verus-lang.github.io/verus/guide/ ; features page (2026-05-13) —
  https://verus-lang.github.io/verus/guide/features.html ; projects/publications —
  https://verus-lang.github.io/verus/publications-and-projects/ ; OOPSLA 2023
  paper — https://arxiv.org/abs/2303.05491 ; toolchain pin (rustc 1.99.1) — repo
  rust-toolchain.toml via GitHub API
- Creusot: repo/releases — https://github.com/creusot-rs/creusot ; README +
  CHANGELOG v0.14.0 (2026-10-12) via GitHub API; toolchain pin
  (nightly-2026-08-03) — repo rust-toolchain via GitHub API; CreuSAT —
  https://github.com/sarsko/creusat ; cdsat — https://github.com/xldenis/cdsat
- Prusti: repo (prusti/prusti, formerly viperproject/prusti-dev) + release
  history via GitHub API (retrieved 2026-10-24); user guide —
  https://prusti.github.io/prusti/user-guide/ ; verification features —
  https://prusti.github.io/prusti/user-guide/verify/summary.html ; spec syntax —
  https://prusti.github.io/prusti/user-guide/syntax.html ; Viper —
  http://www.pm.inf.ethz.ch/research/viper.html
- Flux: repo — https://github.com/flux-rs/flux ; install guide —
  https://flux-rs.github.io/flux/guide/install.html ; site —
  https://flux-rs.github.io/flux/ ; demo — https://flux.goto.ucsd.edu/
- MIRAI: https://github.com/facebookexperimental/mirai (archived; last push
  2024-08-22; via GitHub API)
- Miri: README — https://github.com/rust-lang/miri
- loom (0.7.2, 2024-04-23), proptest (1.12.0, 2026-10-10), quickcheck (1.1.0,
  2026-02-10), cargo-mutants (27.1.0, 2026-06-02) — crates.io API
  (https://crates.io/api/v1/crates/<name>), retrieved 2026-10-24

### Explicitly unverifiable / flagged items (do not rely on these)

- GhostCell author list circulating as "Yancong Chen et al., 2022" — the verified
  paper is Yanovski/Dang/Jung/Dreyer, ICFP 2021.
- "AWS Keccak verification in Lean" — no such repo/blog found; likely conflation
  with AWS SAW/Cryptol work + Cedar.
- No evidence found connecting Veridis/Arya/Aerosync/Prime Intellect/Auxon to
  Lean+Rust parallel-proof work.
- uom compile-time benchmarks; nutype/bounded-integer MSRVs; sealed-traits
  primary citation; Mathlib cache GB sizes; Lean time-to-productivity studies;
  quantitative proof-breakage granularity — all unresolved.
- Kani non-AWS production deployments; current scale of Firecracker/s2n-quic
  Kani usage — cited from Kani materials, not independently re-verified.
- The hax critique's claims — retrieved, not assessed.
- Prusti's post-2026-10 revival sustainability — unknowable at writing time.

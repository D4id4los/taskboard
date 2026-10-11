# Org-Mode Support for Taskboard — Requirements, Ecosystem Survey, and Build-Viability Report

- **Date**: 2026-10-11
- **Status**: Analysis only — no code, no decisions implemented
- **Scope**: Two candidate features — (A) org-markup rendering inside
  text fields, (B) two-way sync with a local `.org` file
- **Method**: Requirements derived against the current workspace
  (`taskboard-domain`/`-state`/`-sync-nextcloud`/`-app`, ADR 0010);
  ecosystem survey via crates.io API, repository READMEs, GitHub API,
  and Slint upstream sources (changelog, type-mappings, issue tracker);
  build architecture derived from the Org Syntax specification and the
  repo's testing strategy.

---

## 1. Executive summary

**Both features are viable. No blocking technical or licensing obstacle
was found.** The two scenarios share one parser core but stress it
differently: (A) needs a *tolerant, fast, span-oriented* parser fed to a
renderer; (B) needs a *lossless, offset-exact* document model with a
write path whose read→parse→map→write loop is idempotent. A single
two-layer parser design serves both.

Three findings dominate the decision space:

1. **No maintained crate in the Rust ecosystem today both reads and
   writes org files with round-trip fidelity.** The closest is
   **orgize 0.10-alpha** (lossless rowan CST, `replace_range` +
   `to_org()`, MIT) — architecturally exactly right, but the repository
   has had no commits since 2025-01 and never left alpha.
   **organic** (0BSD) is the fidelity champion for *reading* (it
   differential-tests itself against real Emacs) but is explicitly
   read-only. The write-capable or GPL-licensed alternatives are either
   dormant, GPL-3 (incompatible with this workspace's MIT/Apache
   library tier), or application-specific.
2. **Slint's rich-text story landed only in 1.16 (2026-04-16)** — the
   `StyledText` element plus `StyledText::from_markdown()` on the Rust
   side — and rich-text *editing* is still an open upstream issue.
   Scenario A's parsing effort is modest; its UI effort is gated on
   Slint, not on org. An MVP of A is a *rendered view with TAB folding
   plus a plain-source edit mode*; live WYSIWYG decoration during
   editing must wait for upstream.
3. **The connector architecture already anticipates feature B.** ADR
   0010 and the `[sync] backend` config slot were designed for
   additional connectors, and the backlog explicitly names "a plain-file
   backend on a synchronized folder". The main architectural lift is
   that `SyncReport::Completed` currently carries the Deck-specific
   `RemoteBoardSnapshot`; a second connector forces the anticipated
   generalization of that payload.

**Estimated effort** (experienced Rust dev, AI-assisted, repo TDD
norms): parser core ≈ 15–25 dev-days; idempotent writer ≈ 8–15;
org-sync connector ≈ 10–18; org-rendering UI ≈ 10–20. Feature B end to
end ≈ **4–7 weeks**; feature A ≈ **2.5–5 weeks**; both together ≈
**6–10 weeks** because the parser core is shared. Riding orgize (vendored
or forked behind a wrapper) can cut the parser-core line to a 3–5 day
spike plus ongoing fork ownership.

**Recommended sequencing** (§8): spike orgize first; build the shared
parser core with B's read side and A's renderer on top of it; write-back
(surgical edits) last; WYSIWYG editing deferred until Slint ships it.

---

## 2. What the codebase gives us today

Facts that shape the requirements (from the current `main`):

- **Entities** (`taskboard-domain/src/entities.rs`): `Board`, `Stack`
  ("a column within a board"), `Task`, `Label`. `Task` carries
  `title`, `description: String` (doc comment: *"Markdown, as Deck
  stores it"* — the only markup reference in the code), `duedate:
  Option<DateTime<Utc>>`, `done: Option<DateTime<Utc>>` (None = open),
  `stack: StackId`, `order: i64`, `labels: BTreeSet<LabelId>`,
  `archived`, `deleted`, per-field `TaskClocks`.
- **No task-state vocabulary exists.** Stacks are free-form, per-board,
  ordered columns. There is no built-in TODO/STARTED/REVIEW/… list —
  the org keyword↔stack mapping must therefore be *derived or
  configured per board*, not hardcoded.
- **AppState** is flat id-keyed `BTreeMap`s with canonical order
  `(order asc, id asc)`; snapshots via `ArcSwap<AppState>`; deterministic
  serialization chosen for insta.
- **Sync architecture** (ADR 0010): per-connector crate
  (`taskboard-sync-<name>`), `[sync] backend` kebab-case config,
  `SyncCommand`/`SyncReport`/`SyncStateReader` as the connector
  vocabulary, push-before-pull cycle, merge policy (R1–R9 per-field LWW)
  executed by the state engine — connectors never mutate state.
- **Config** (`taskboard-app/src/config.rs`): `AppConfig` with
  `deny_unknown_fields` tables; `SyncBackend { Standalone, Nextcloud }`
  with explicit-or-auto resolution; `StorageSection.db_path` shows the
  file-path field pattern. An `[org]` section slots in cleanly.
- **UI**: `taskboard-ui-slint` is an empty shell — no `slint`
  dependency pinned yet, no text rendering code. Feature A's UI work
  starts from zero, unconstrained by existing widget choices.
- **Testing strategy**: pure-domain proptest, insta state snapshots,
  fakes over mocks, deterministic time, tiered integration (fixtures →
  wiremock → secretless docker → local `#[ignore]`). §11 explicitly
  reserves cargo-fuzz for "when a custom … string tokenizer … appears" —
  an org parser is precisely that trigger.

---

## 3. Scenario A — org-markup text fields with emacs-style rendering

### 3.1 What is being built

A text field (task description first) whose content is org-flavored
markup, rendered in the style of emacs org-mode: emphasis objects
(`*bold*`, `/italic/`, `_underline_`, `+strike+`, `=verbatim=`,
`~code~`), heading typography with indentation by level,
itemized/enumerated/description lists, `#+begin_X … #+end_X` blocks,
property drawers, and TAB-driven visibility cycling (fold/children/
subtree).

### 3.2 Requirements

| # | Requirement | Consequence |
|---|---|---|
| A1 | **Total, tolerant parsing** — any string is valid org; the grammar degrades to paragraphs; the parser never errors | Rules out strict grammars without recovery; no error UI states exist |
| A2 | **Inline object grammar per org rules** — emphasis with `org-emphasis-regexp-components` border rules (opening marker preceded by whitespace/BOL, closing marker followed by whitespace/punct/EOL, no border blanks inside), links, timestamps, escapes (`\_`), entities, statistics cookies | Typed span output `(kind, byte_range)`; the border rules are where naive parsers diverge visibly from Emacs |
| A3 | **Element grammar** — headlines (stars/keyword/priority/title/tags), three list kinds with nesting and checkboxes, blocks, drawers, keywords, comments, fixed-width (`:`), planning lines, tables (later) | Line-oriented block parser feeding A2 per content span |
| A4 | **Frame-budget reparsing** — parse a field-sized input (≤ ~100 KB worst case) well under a frame, or incrementally by construction | Org's grammar is line-based: a line table + per-paragraph inline scan gives cheap incremental reparsing of the edited region only |
| A5 | **Presentation model with visibility** — layout tree of styled blocks + inline style runs, plus a per-node folding state machine (TAB cycles like org) | Folding is UI state over the parse, not persistence |
| A6 | **Slint fit** — rich-text *display* exists since Slint 1.16 (`StyledText`, `StyledText::from_markdown()` constructible from Rust at runtime; inline code got monospace+capsule styling in 1.17/1.18); rich-text *editing* is still open upstream (issue #2723 / #9560) | Inline runs can map org→markdown into `StyledText`; block structure, indentation, drawers, folding = custom Slint layout over our model. Live decorated *editing* is out of our hands until Slint ships it |
| A7 | **No persistence change** — `description` stays a plain `String`; org rendering is a view concern | But note the doc comment calls it "Markdown, as Deck stores it": a decision is needed on the field's canonical markup dialect (see §10 Risks) |
| A8 | **Kiosk-class cost** — pure `&str → Doc` function, allocation-frugal, no async, no IO; usable from the UI crate | Parser must be a dependency-light library crate (UI crate rule: no IO crates — satisfied) |

### 3.3 Shape of the rendering pipeline

```
String (field content)
  │  taskboard-org: parse (element layer + inline layer)
  ▼
LayoutTree (blocks: kind, level, spans; runs: style spans)   ← pure
  │  taskboard-ui-slint: visibility model (TAB) + mapping
  ├─ inline runs → StyledText::from_markdown(org_to_md(runs))  (escape!)
  └─ block structure → VerticalLayout / per-block Slint elements, indented by level
```

Two correctness notes for the `StyledText` path: (1) plain runs must
escape markdown metacharacters (`\ * _ ` [ ]` etc.) or org text like
`a*b` would be re-interpreted as markup; (2) org distinctions that
markdown lacks (verbatim vs code) collapse — acceptable for rendering,
documented as such.

---

## 4. Scenario B — two-way sync with a local `.org` file

### 4.1 The contract, stated precisely

The user requirement "read → parse → deserialize → write should be
idempotent for well-formed org files" decomposes into three testable
properties:

- **P1 — Fixpoint (all inputs)**: `write(parse(write(parse(x)))) ==
  write(parse(x))`. One normalization pass converges; no oscillation.
- **P2 — Byte-stability (managed subset)**: `write(parse(x)) == x` for
  well-formed files within the documented managed subset (golden
  corpus). Untouched bytes never move.
- **P3 — No-op writes nothing**: a sync cycle with no diffs writes
  nothing (mtime stable — important to external tooling like Emacs,
  editors, and file sync clients watching the file).

P2 for *arbitrary* org is not achievable by anything short of Emacs
itself (organic's whole raison d'être is measuring such divergence);
P2 for a well-defined managed subset, plus P1 universally, is the right
target and is what "idempotent for well-formed org files" should mean
contractually.

### 4.2 Document-model strategy

- **S1 — Lossless CST + surgical edits** (orgize's design): parse into
  a tree retaining every byte with ranges; express domain changes as
  `(replace_range, text)` edits; serialize by splicing. Untouched
  regions are byte-identical by construction. Use for **externally
  authored files**.
- **S2 — Canonical serializer**: emit house-style org for content we
  create (new boards/tasks, app-created files). Deterministic field
  order (planning lines in SCHEDULED/DEADLINE/CLOSED order, fixed
  property order, no trailing whitespace, LF endings) makes P1/P2
  properties of the writer's construction.
- **Hybrid (recommended)**: S1 edits anchored on the lossless tree; S2
  emission only for newly inserted headings. First import of an
  id-less external file performs a *one-time, bounded* normalization
  (stamping ids, §4.4) that is visible in the file's git diff —
  honest, reviewable, and convergent (P1).

### 4.3 Mapping org ↔ domain (grounded in the actual entities)

Two levels supported initially: **h1 = Board, h2 = Task**. Deeper
headings, clock logbooks, and unknown drawers are preserved verbatim as
opaque tails — the parser must not flatten them, because nested decks
and subtasks are explicit future features.

| Domain field | Org construct | Notes |
|---|---|---|
| `Board.title` | h1 title | |
| `Stack` (one per TODO keyword) | `#+TODO: STARTED REVIEW \| DONE CANCELLED` vocabulary | Left of `\|` = open stacks, right = done-class; stack order = declaration order, then done stacks. Bare headings (no keyword) → configurable default stack. Keywords present on headings but not declared → auto-added stacks (or config table) |
| `Task.title` | h2 title minus keyword/priority/tags | |
| `Task.description` | h2 section body (paragraphs/lists) as org source | Edit region = body span; content stays org-flavored (feeds scenario A) |
| `Task.duedate` | `DEADLINE: <…>` planning entry | Org timestamps are naive local; domain is `DateTime<Utc>` → needs a configured timezone (figment). Repeater/warning suffixes preserved verbatim |
| `Task.done` | done-class keyword + `CLOSED: […]` | Import heuristic when CLOSED missing: file read time; flagged |
| `Task.labels` | heading tags `:home:errands:` | Labels are per-board entities with colors; auto-create on first sight; **store the canonical tag form in the binding, not in the label title** (org tag charset is restricted; label titles are free strings — round-tripping through the title would break P2) |
| `Task.order` / `Stack.order` | h2 sequence within board / keyword order | Order changes = heading moves (cut/paste region splices) — the trickiest write |
| `Task.archived`/`deleted` | `:taskboard_archived:` tag convention (MVP) | Real org archiving (move to `archive.org`) is a later ceremony |
| `Board.color`, `Label.color` | custom keywords (`#+TASKBOARD_LABEL_home: #ff0000`) or board-level property drawer | Custom keywords are legal org, ignored by Emacs |
| Priority `[#A]`, `SCHEDULED:`, checkboxes, CLOCK logbooks | no domain field | Preserved verbatim; backlog candidates |

### 4.4 Identity, ordering, conflicts

- **Identity**: heading text is not identity. Stamp each managed h2
  with `:TASKBOARD_ID: <uuidv7>` (or reuse `:ID:`) in its property
  drawer — the direct analog of `RemoteCardRef` for files. First
  import adopts by position/title, then stamps ids once.
- **Connector wiring**: new crate `taskboard-sync-org`, `[sync]
  backend = "org"` (+ `[org] path = …` section), reusing
  `SyncCommand`/`SyncReport`/`SyncStateReader` per ADR 0010. Requires
  generalizing the Deck-typed `RemoteBoardSnapshot` payload (small
  ADR: backend-tagged snapshot enum, or promote the connector
  vocabulary to a trait as ADR 0010 anticipated).
- **Change detection**: poll mtime + size/hash at `poll_interval`
  (zero new dependencies; the `notify` crate can come later). Atomic
  writes via temp-file + rename so Emacs never reads a torn file.
- **Conflicts**: org files carry no per-field clocks. External edits
  enter LWW with the file read time as a coarse clock for every field
  (coarser than Deck's per-field `remote_seen`; optional refinement:
  newest CLOCK/timestamp under a heading as a per-task heuristic).
  Documented limitation, not silent behavior.

---

## 5. Shared constraints from the workspace rules

- **New crate shape**: `taskboard-org` — pure parser/writer library,
  `// SPDX-License-Identifier: MIT OR Apache-2.0`,
  `#![forbid(unsafe_code)]`, no async/IO/UI deps (≈ zero-dep; `memchr`
  at most). The domain mapping lives in the connector crate
  (`taskboard-sync-org`), mirroring how `sync-nextcloud` owns
  `model.rs` + `mapping.rs` — the parser stays vocabulary-free.
- **Dependency licensing**: orgize is MIT, organic is 0BSD — both
  compatible with every tier of the dual-licensing model.
  **GPL-3.0+ crates (org-tools, windancer) are disqualified** as
  library dependencies of the MIT/Apache crates (usable, if ever, only
  as external processes — not worth it).
- **Testing**: proptest for parser invariants (P1 fixpoint, emphasis
  border rules); insta golden corpus (seed with this repo's own
  `README.org`/`docs/*.org` — real-world org authored outside our
  bubble — plus synthetic edge cases); cargo-fuzz per testing-strategy
  §11's explicit parser trigger; optionally an organic-style
  differential oracle against pinned Emacs in docker, matching the
  existing Tier-2 pattern.

---

## 6. Rust ecosystem survey

### 6.1 Candidates

| Crate | Read | Write / round-trip | Model | License | State (2026-10) |
|---|---|---|---|---|---|
| **orgize** 0.10.0-alpha.10 | lossless rowan CST, typed AST, traverse events, custom TODO keywords, `ParseConfig` | **yes** — `replace_range()` + `to_org()` surgical splicing; HTML export | CST + AST wrappers | MIT | No commits since 2025-01-27; 0.10 never left alpha; 17 open issues; not archived |
| **organic** 0.1.16 | highest fidelity: differential-tested against pinned Emacs ("any divergence is a bug"); org-element-scope AST | **no** — explicitly read-only, no renderer/serializer | semantic AST | 0BSD | Git active (2026-07), crates.io stale (2024-04), no library docs, unstable API |
| **org-tools-core** | partial (lint-oriented) | formatter (in-place `org fmt`, 9 auto-fix rules, protected blocks) | document model + rules | **GPL-3.0+** | Young (72 commits, 3 stars), active |
| **windancer** | partial (WIP vs worg spec) | no (read → HTML SSG only) | unspecified | **GPL-3.0** | Early, one-author SSG |
| **tree-sitter-org** 1.3.3 | CST w/ error recovery; structure good, **inline markup weak** (README: paragraphs tokenize as expr) | no (CST is editable in principle) | tree-sitter CST | MIT | **Repo archived 2025-02-17**; crate last touched 2022 |
| **org-element** (melito) | tree-sitter-backed, org-element-compatible | unknown | typed AST | **no LICENSE file** | 11 commits, 0 stars — unusable |
| **vissue-core** 0.21 | yes — org-as-database issue tracker | **yes** — "org projection", generation tokens for optimistic concurrency | heading-as-issue + `:PARENT:`/`:BLOCKED_BY:` properties | MIT | Very active (684 commits), but its model *is* the product (DAG issue tracking), not a kanban projection |
| `org` (m-cat) 0.3.1 | basic | unclear | — | — | Abandoned (2019) |
| `orgmode-rs` (rpdeshaies) | event parser | no | — | — | **Repository gone (404)** |
| `tftio-org` / `oak-org-mode` | typed AST / oaks-framework parser | unknown / framework-level | — | MPL-2.0 (oaks; requires **nightly**) | Niche; tftio host unresolvable |
| pandoc / emacs subprocess | reads org | writes org (lossy: drawers, planning, TODO semantics degrade) | external process | GPL (both) | Dismissed: external process + lossy + anti-kiosk |

### 6.2 Assessment against the requirements

- **Scenario A (tolerant span parser)**: organic's AST would work but
  drags in an unstable, undocumented API and far more machinery than a
  field renderer needs; orgize's event traversal fits nicely; a
  purpose-built two-layer parser (§7) is genuinely small for this
  scope because inputs are field-sized and the grammar subset is
  bounded.
- **Scenario B (lossless + write-back)**: **orgize 0.10 is the only
  crate with the right architecture** — lossless CST with byte ranges
  plus splicing write-back is precisely the surgical-edit strategy of
  §4.2. organic is the best *reader* for arbitrary user files but
  offers no write path. Nothing maintained combines both.
- **Prior art worth mining regardless of build/buy**: vissue proves
  the product shape (org file as sole store, write-through tooling,
  Emacs-interop guarantees, generation-based concurrency) — read its
  "Org syntax" and "Org ecosystem" docs before designing our codec;
  organic's compare-mode-in-docker is the pattern for a differential
  oracle; orgize's rowan usage is the reference for our CST if
  self-built.

### 6.3 Ecosystem conclusion

The requirement profile (lossless round-trip + maintained + permissively
licensed + embeddable) is empty in today's ecosystem. The choice is:
adopt-and-own orgize's alpha, or build a scoped parser. There is no
third "just use it" option.

---

## 7. Self-implementation architecture

### 7.1 The key structural insight

Org is a **two-level grammar** (per the Org Syntax spec): a
line-oriented *element* grammar (headlines, lists, blocks, drawers,
keywords) containing paragraph-scoped *object* grammars (emphasis,
links, timestamps). Neither level needs backtracking when parsed with
org's own disambiguation rules (indentation, blank-line separation,
bullet shape). This makes a hand-written scanner — not a parser
generator — the natural fit, and gives incremental reparsing almost
for free: an edit invalidates the edited line's element (and its
neighbors only at element boundaries), then re-scans objects in the
affected paragraphs.

### 7.2 Crate and layer design

```
taskboard-org                     (pure lib, MIT OR Apache-2.0, forbid unsafe)
├── L0  LineTable        memchr '\n' scan → byte offsets per line;
│                        powers incremental reparse + surgical edits
├── L1  elements         line-classifier state machine → ElementTree
│                        (kind, line_span, byte_span); keyword table injected
│                        (TODO vocab is *config*, not syntax)
├── L2  objects          per-span inline scanner → Vec<ObjectSpan>
│                        (emphasis border rules, links, timestamps, escapes)
├── L3  views            borrow-based accessors: headings(), section_of(),
│                        planning(), properties(), list_items()
├── L5  writer           EditPlan (Vec<(Range, String)> splice vs original)
│                        + canonical() emitter for new content
└── (A)  render adapter  Doc → LayoutTree (blocks, styled runs) — UI-agnostic

taskboard-sync-org                (connector crate, mirrors sync-nextcloud)
├── model.rs            org-file view: OrgBoardSnapshot
├── mapping.rs          snapshot ↔ RemoteBoardSnapshot-equivalent, tag bindings
├── file.rs             mtime/hash poll, atomic temp-file+rename writes
└── actor.rs            SyncCommand/SyncReport cycle, push-before-pull
```

Scope discipline: L1/L2 target the *managed subset* (headlines,
planning, drawers/properties, keywords, the three list kinds, blocks,
comments, fixed-width, inline objects, timestamps). Full org-element
parity (org tables.el, footnote definitions, babel headers, affiliated
keyword corner cases) is explicitly out — ×3–4 effort for zero value
to either scenario; unknown constructs parse as opaque regions and are
preserved.

### 7.3 Idempotency by construction

- The writer never reformats what it did not semantically change: edits
  are range splices against the original bytes (P2 for the corpus).
- `canonical()` is a pure function of the typed value with a fixed
  layout contract (planning-line order, property order, one blank line
  between sections, LF, trailing newline) → `parse(canonical(v)) == v`
  is a proptest-able invariant; the fixpoint P1 follows because
  splicing preserves foreign bytes and canonical text is already in
  fixpoint form.

### 7.4 Integration points

- **Config**: `[org] path`, optional keyword overrides, id property
  name, timezone for timestamp conversion; `SyncBackend::Org` variant
  (+ auto-resolution rule: org path present → org).
- **Bootstrap**: one more match arm; under standalone, nothing spawns.
- **Domain**: one small ADR to generalize the `SyncReport::Completed`
  snapshot payload across connectors.
- **UI (A)**: LayoutTree → Slint; inline runs via
  `StyledText::from_markdown` (with escaping), block structure via
  layout elements; `VisibilityModel` for TAB cycling; editing UX =
  plain `TextEdit` source mode ↔ rendered mode toggle.

### 7.5 Testing strategy (per docs/testing_strategy.org)

- **Unit/proptest (L1/L2)**: emphasis border rules, list indent
  disambiguation, keyword case-insensitivity, timestamp grammar; P1
  fixpoint `write(parse(write(parse(x)))) == write(parse(x))`.
- **Insta golden corpus**: this repo's own org docs + synthetic files
  covering CRLF, BOM, missing final newline, deep nesting, drawer
  variants.
- **Fuzzing**: cargo-fuzz target on `parse` + `write∘parse` once the
  crate lands (the §11 trigger fires).
- **Differential (optional, Tier 2-style)**: pinned Emacs in docker as
  an oracle for parse divergence on the corpus — organic's method on
  our subset.
- **Connector**: black-box actor tests over channels with a temp-dir
  fake filesystem, `tokio::time::pause()` for poll cycles, typed
  outcome assertions (no text matching) throughout.

### 7.6 Effort estimate

Assumptions: one experienced Rust developer, AI-assisted, TDD per repo
norms, dev-day ≈ 6 focused hours. Ranges are P50–P85.

| Component | Dev-days | Notes |
|---|---|---|
| Spike: orgize evaluation (parse our corpus, round-trip test, gap list) | 2–4 | Decision gate: ride vs build |
| L0–L3 parser core (managed subset) | 15–25 | The single biggest line; shrinks to ~5 if orgize is adopted and remains sufficient |
| L5 writer + idempotency tests | 8–15 | EditPlan splicing + canonical emitter + P1/P2 suites |
| Scenario B connector end-to-end | 10–18 | model/mapping/file/actor, config, bootstrap, snapshot generalization, actor tests |
| Scenario A rendering | 10–20 | LayoutTree adapter 2–4; Slint block layout + folding 6–12; StyledText spike 1–2; edit-mode UX 2–4 |
| **Feature B total** | **20–38 (~4–7 weeks)** | spike + core + writer + connector |
| **Feature A total** | **12–25 (~2.5–5 weeks)** | core share + render |
| **Both** | **30–50 (~6–10 weeks)** | shared core amortizes |

---

## 8. Recommendation — build vs buy, and sequencing

1. **Do the orgize spike first (2–4 days).** Vendoring a dormant MIT
   alpha behind a `taskboard-org` wrapper is legitimate *if* the spike
   shows it parses our corpus losslessly and its CST covers property
   drawers, planning lines, and list kinds cleanly. Keep the wrapper
   boundary so the dependency is swappable — we already own it either
   way (no upstream to appeal to).
2. **If the spike fails gaps → build L0–L5 per §7.** The architecture
   is low-risk because the grammar subset is bounded, the spec is
   written, and organic/vissue/orgize provide reference behavior to
   diff against.
3. **Deliver B before A's editing ambitions, A's rendering before B's
   writer.** Concretely: Phase 1 = parser core + org *read* (import to
   boards, one-way); Phase 2 = rendering adapter + StyledText view
   with folding (feature A MVP); Phase 3 = surgical write-back
   (feature B complete); Phase 4 = WYSIWYG editing — blocked on Slint
   upstream, re-evaluate per release.
4. **Mine vissue's documented org-mapping decisions** (what it "owns,
   reads, or preserves") before finalizing §4.3 — it has already paid
   for these lessons.

---

## 9. Viability verdict

- **Feature B (two-way org sync): high viability.** The technique
  (lossless CST + splice editing) is proven in orgize; the product
  shape is proven in vissue; the connector slot exists by design
  (ADR 0010). Hard parts are known and bounded: snapshot-payload
  generalization, heading-move writes, coarse LWW for external edits,
  and the one-time id-stamping normalization. 4–7 weeks.
- **Feature A (org-rendered fields): viable with a scoped MVP.**
  Parsing is the *easy* half (tolerant subset, field-sized inputs).
  Display rendering is achievable today on Slint ≥ 1.16 via
  `StyledText` + custom block layout with TAB folding. Live decorated
  *editing* is gated on Slint upstream (rich-text editing still an
  open issue), so the MVP is rendered-view + source-edit toggle, with
  WYSIWYG as a follow-up that requires no rework of the parser layer.
  2.5–5 weeks.
- **The combination is synergistic, not just compatible**: B makes
  task descriptions org-flavored by construction, which is exactly
  what A renders; one parser core serves both.

## 10. Risks and mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| orgize dormancy (no commits since 2025-01, alpha API unstable) | High if adopted | Wrapper crate boundary; vendor/fork policy decided at spike exit; escape hatch = §7 build |
| Slint rich-text editing timeline (open issue) | Medium (A only) | MVP design avoids it; parser/layout investment is editor-agnostic |
| Org grammar corner cases (emphasis borders, list indent rules, keyword case) | Medium | proptest from the spec's rules + golden corpus + optional Emacs differential oracle |
| Description dialect conflict ("Markdown, as Deck stores it") | Medium (product) | Decide per-field markup policy before A ships: org-flavored rendering is a *view*; Deck sync stays verbatim either way |
| Coarse conflict resolution for external edits (file-level clock) | Medium | Document; optional per-heading CLOCK heuristic; generation-token pattern from vissue as future hardening |
| Normalization-on-first-save surprises users' git diffs | Low | One-time, bounded, id-stamping only; changelog + docs |
| CRLF/BOM/final-newline variance breaks P2 | Low | Per-file ending preservation + one-time normalization, all golden-tested |
| Keyword↔stack mapping ambiguity (undeclared keywords) | Low | Config table + auto-add rule; per-board, inspectable in state |

## 11. References

- Org Syntax spec: `https://orgmode.org/worg/dev/org-syntax.html`
- orgize: `https://github.com/PoiScript/orgize` (v0.10 branch; lossless
  rowan CST, `replace_range`/`to_org`)
- organic: `https://code.fizz.buzz/talexander/organic` (Emacs-parity
  differential testing, read-only)
- vissue: `https://github.com/leidarljos/vissue` (org-as-database issue
  tracking; org projection + generation concurrency)
- org-tools: `https://github.com/lino/org-tools` (GPL-3 formatter —
  pattern reference only)
- tree-sitter-org: `https://github.com/milisims/tree-sitter-org`
  (archived 2025-02)
- Slint: 1.16 release blog (StyledText, `@markdown`, NLnet-funded);
  changelog entries 1.16–1.18 (`StyledText::from_markdown` runtime
  parsing, `max-lines`, inline-code styling); issues #9560 (rich text
  rendering design), #2723 (rich text editing — open)
- Workspace: ADR 0010 (standalone-first connectors),
  `docs/testing_strategy.org` §9/§11, `docs/architecture.org`

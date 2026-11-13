# [Plan] UX Journey Plan — fixture (gate self-test, MISSING path)

<!-- Self-test data for scripts/ux-coverage.sh --self-test; never a real
plan. Identical to plan.md plus three rows whose evidence resolves
NOWHERE inside tree/: the W-3 callback, the W-3 command token, and the
US-2/S3 journey step. The gate must report exactly those three MISSING
rows and exit 1 under --gate. -->

- **Date**: 2026-11-13
- **Kind**: UX journey plan fixture.
- **Origin**: scripts/ux-coverage.sh self-test.
- **Status**: ACCEPTED (fixture).

Test-root: tests

## 1. Scope & goals

Fixture scope: exercise every parsed table kind, with deliberate holes.

## 2. Capability × surface matrix

| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |
|---|---|---|---|---|---|---|---|
| fixture.create | C US-1 | — | C US-1 | — | — | — | C US-1 |
| fixture.remove | — | X US-1 | — | — | — | — | — |

**Rulings** (every deliberate absence):

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | fixture.remove absent from Board | creation-first slice; removal ships in P2 | #999 |

## 3. User stories

### US-1 — create a fixture
- **Story**: As a desktop user I want fixture creation so that the gate passes.
- **Platforms**: desktop
- **Entry points**: fixture button; ActionId::FixtureTap
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given the fixture view When the user taps Then StateCommand::DoFixtureThing — verified by journey test.

### US-2 — auxiliary story (uncited by the matrix)
- **Story**: As a desktop user I want a passive badge so that display-only wiring is covered.
- **Platforms**: desktop
- **Entry points**: fixture badge
- **Priority**: P2
- **Acceptance**:
  - AC-1: Given the fixture view When the badge renders Then no command — verified by fold test.

## 4. Journey specs

### US-1 — create a fixture

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-1/S1 | fixture view, tap button | click / ActionId::FixtureTap | StateCommand::DoFixtureThing | fixture list gains a row | Escape cancels |
| US-1/S2 | fixture view, list row | keyboard activate | UiRequest::ShowFixtureToast | toast caption | typed error banner |

### US-2 — auxiliary story (uncited by the matrix)

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-2/S1 | fixture view, badge | passive | — display only | badge renders | — |
| US-2/S3 | fixture view, ghost affordance | click | StateCommand::GhostCommand | nothing (evidence missing) | — |

## 5. Interaction flows

Fixture flow omitted (fixture).

## 6. Wiring inventory

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | fixture-view.slint | FixtureButton | fixture_tap | on_fixture_tap | FixtureTap | DoFixtureThing; ShowFixtureToast | — | US-1/S1; US-1/S2 | fixture_journeys.rs::us1_s1_fixture_create |
| W-2 | fixture-view.slint | FixtureBadge | — | — | — | — | — | US-2/S1: display-only badge, no dispatch | fixture_journeys.rs::us2_s1_badge |
| W-3 | fixture-view.slint | GhostButton | ghost_callback | on_ghost_callback | — | GhostCommand | — | US-2/S3 | (missing on purpose) |

## 7. Command-contract delta

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|
| StateCommand | DoFixtureThing | { title: String } | US-1/S1 |
| UiRequest | ShowFixtureToast | { caption: ToastCaption } | US-1/S2 |
| ActionId | FixtureTap | — | US-1/S1 |

## 8. Verification plan

- **Journey tests** (step-ID-named): us1_s1, us1_s2, us2_s1 (tree/tests).

## 9. Definition of done

- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0

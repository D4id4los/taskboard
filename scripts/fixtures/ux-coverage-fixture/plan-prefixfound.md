# [Plan] UX Journey Plan — fixture (gate self-test, #965 prefix-found)

<!-- Self-test data for scripts/ux-coverage.sh --self-test; never a
real plan. tree/ carries the honest `load` callback, `on_load(` handler and
`DropHabit` variant adjacent to the suffixed spellings (`load_extra`,
`on_load_extra`, `DropHabitExtra`). Boundary-anchored matching (#965)
must still resolve both rows FOUND (the no-miss guard) and exit 0
under --gate. -->

- **Date**: 2026-11-19
- **Kind**: UX journey plan fixture.
- **Origin**: scripts/ux-coverage.sh self-test (#965).
- **Status**: ACCEPTED (fixture).

Test-root: tests

## 1. Scope & goals

Fixture scope: boundary-anchored token matching (prefix-found direction).

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
- **Entry points**: fixture button
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given the fixture view When the user taps Then StateCommand::DropHabit — verified by journey test.

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
| US-1/S1 | fixture view, tap button | click | StateCommand::DropHabit | fixture list gains a row | Escape cancels |
| US-1/S2 | fixture view, list row | keyboard activate | UiRequest::ShowFixtureToast | toast caption | typed error banner |

### US-2 — auxiliary story (uncited by the matrix)

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-2/S1 | fixture view, badge | passive | — display only | badge renders | — |

## 5. Interaction flows

Fixture flow omitted (fixture).

## 6. Wiring inventory

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | collide-view.slint | FixtureButton | load | on_load | — | DropHabit | — | US-1/S1; US-1/S2 | fixture_journeys.rs::us1_s1_fixture_create |
| W-2 | collide-view.slint | FixtureBadge | — | — | — | — | — | US-2/S1: display-only badge, no dispatch | fixture_journeys.rs::us2_s1_badge |

## 7. Command-contract delta

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|
| StateCommand | DropHabit | { title: String } | US-1/S1 |
| UiRequest | ShowFixtureToast | { caption: ToastCaption } | US-1/S2 |

## 8. Verification plan

- **Journey tests** (step-ID-named): us1_s1, us1_s2, us2_s1 (tree/tests).
- **Bound tests** (retro cohort mapping, #768):
  bound-test: US-1/S2 -> fixture_journeys.rs::us1_s2_toast_shown

## 9. Definition of done

- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0

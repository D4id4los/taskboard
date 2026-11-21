# [Plan] UX Journey Plan — fixture (gate self-test, #1073 tier determinism)

<!-- Self-test data for scripts/ux-coverage.sh --self-test; never a
real plan. Run against the fixture tree, which carries BOTH a
production messages twin (src/messages.rs) and a test-support twin
(src/test_support/messages.rs) holding the same TierProbe token. The
command row must resolve FOUND citing the production file —
pre-#1073 the citation picked whichever file find emitted first. -->

- **Date**: 2026-11-21
- **Kind**: UX journey plan fixture.
- **Origin**: scripts/ux-coverage.sh self-test (#1073).
- **Status**: ACCEPTED (fixture).

Test-root: tests

## 1. Scope & goals

Fixture scope: by-name evidence universes are deterministic and
production-preferring.

## 2. Capability × surface matrix

| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |
|---|---|---|---|---|---|---|---|
| fixture.create | C US-1 | — | C US-1 | — | — | — | C US-1 |

**Rulings** (every deliberate absence):

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | no CLI surface | the tier probe is a fixture, not a verb | #1073 |

## 3. User stories

### US-1 — create a fixture

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-1/S1 | fixture view, tap button | click | the probe command | fixture list gains a row | Escape cancels |

## 4. Journey specs

### US-1 — create a fixture

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-1/S1 | fixture view, tap button | click | the probe command | fixture list gains a row | Escape cancels |

## 5. Interaction flows

Fixture flow omitted (fixture).

## 6. Wiring inventory

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | — | — | — | — | — | StateCommand::TierProbe | — | US-1/S1 | — |

## 7. Command-contract delta

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|
| prose | unused delta | — | — |

## 8. Verification plan

- **Bound tests** (retro cohort mapping, #768):
  bound-test: US-1/S1 -> fixture_journeys.rs::us1_s1_fixture_create

## 9. Definition of done

- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0

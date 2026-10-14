## Put a test at the boundary it checks

Unit tests belong beside the code in `src/`, often in `test.rs`. Integration
tests belong in `tests/`. A fixture spanning several modules or app phases is
usually an integration test.

Use [`q_test_harness`](https://github.com/cubething-qproj/q_test_harness) for
shared test machinery, with a small fixture local to the scenario. The fixture
owns an ordinary `App` and relevant entity IDs; its methods arrange the state,
perform actions, and check results. Keep game types out of the harness.

## Choose the schedules deliberately

Use `TestRunnerPlugin` and `add_step` with the plugins being tested to exercise
the normal app lifecycle: loading, screen transitions, entity cleanup, and
interactions between plugins.

`InputTestPlugin` and `AppExt::step` suit precise mechanics. They let a test
supply input and advance only the named schedules. A passing test covers those
phases, not the loading or cleanup phases it omitted.

Use full lifecycle coverage when the behaviour depends on it. For a focused
system test, run just enough of the app to exercise that system's dependencies.
See the `q-test-harness` skill for setup and schedule-stepping details.

## Keep setup proportional to the assertion

Synthetic terrain is sufficient for a step-cost or marker-material test.
Erosion, creek tracing, and layout properties need generated worlds, but their
fixtures can be smaller than the playable default. Keep representative seeds
for the properties that depend on them.

Measure test runtime before optimizing or adjusting deadlines. Local workspace
profiles and standalone CI profiles can differ; see
[`development`](crate::docs::development#build-from-the-workspace-root).
A timeout is a guard against stalled work, not a substitute for understanding
why a test is expensive.

## Assert behaviour

Cover outcomes and edge cases. Avoid assertions tied to tuning constants,
incidental system order, or a particular generated path. If the behaviour
belongs to a library, test it there and open an upstream issue when needed.
Automated tests establish mechanics; the designer's playtesting establishes
whether they feel right.

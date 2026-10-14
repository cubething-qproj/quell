## Choose how work runs

A scheduled system runs at a point chosen by its schedule. An observer is a
system invoked by an event. Messages let systems exchange buffered data.
Choose according to when the result is needed:

- Use scheduled systems for movement, ongoing input, timers, and other work
  evaluated regularly. Put fixed-step simulation in `FixedUpdate`; use
  `Update` for frame-based input and presentation.
- Use observers for discrete reactions such as an entity being selected,
  spawned, or removed. Entity observers and component hooks can react where
  the relevant lifetime change occurs.
- Use messages when producers and readers run at separate schedule points,
  or when a reader should process several inputs together.

Observers run when their event is triggered. `World::trigger` invokes them
directly; `Commands::trigger` queues the event until deferred commands are
applied. Choosing an observer does not remove the need to understand ordering.

Prefer an entity-targeted observer when only one entity should react. A global
observer suits events with application-wide meaning. Avoid replacing a normal
update loop with observers that repeatedly trigger themselves.

## Let the owner choose the scope

A screen schedules the service systems it needs, as described in
[`architecture`](crate::docs::architecture). Its schedules follow the screen's
lifecycle: loading work belongs in `Loading`, ready-state work in `Update`,
and teardown in `OnUnload`.

Global systems and observers retain their own registration and lifetime.
Attach observers to the entities they serve when possible, and make sure
persistent observers can tolerate the absence of screen-owned data.

Use run conditions for additional state within a screen or for changes in
resources. Configure those conditions where that state is owned.

## Make dependencies explicit

Use `.chain()`, `.before()`, and `.after()` when one operation needs another's
result. Otherwise, systems may run in either order or in parallel. Include
deferred commands in that reasoning: an entity queued for spawning is not yet
available to another query.

Libraries expose `SystemSet`s such as `ProcessSystems`, `TerminalSystems`, and
`ShellSystems` for ordering against their phases. Use those public sets instead
of naming private library systems.

Buffered I/O can also cross frame boundaries. Under the default q_os ordering,
keyboard input emitted in `Update` reaches programs after the next `First`;
process output can reach a terminal in the same `Update`. Consult the `q-os`
skill's frame model when adding work across these phases.

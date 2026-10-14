## Ownership and composition

Quell separates the lifetime of a feature from the behaviour it provides:

- [`screen`](crate::screen) composes features and decides when their systems run.
- [`service`](crate::service) implements game behaviour that screens can reuse.
- [`common`](crate::common) holds reusable components and their supporting code.
- [`AppPlugin`](crate::AppPlugin) registers the application and its dependencies.

The source directories are `screen/`, `service/`, and `common/`. Configuration
is currently part of plugin setup; there is no separate `config/` module.

## Screens own a lifetime

A screen is a scene or room in the game, with loading, running, and unloading
phases. It implements [`Screen`](q_screens::prelude::Screen), rather than using
Bevy's serialized `Scene` type to manage those phases.

A screen chooses which systems run in its schedules and creates the entities
those systems need. Its `Loading` systems call `finish_loading()` when assets
are ready. Use that lifecycle instead of adding a second loading state machine.

Cleanup removes entities across the world, including entities spawned at
startup. [`Persistent`](q_screens::prelude::Persistent) and registered persistent
types protect entities that should survive a transition;
[`ScreenScoped`](q_screens::prelude::ScreenScoped) opts entities into cleanup.
Use `Propagate(Persistent)` when an entire hierarchy must survive.

Resources have a separate lifetime. A screen must explicitly remove or reset
resources that belong to one visit; shared settings can remain registered.

Keep setup, teardown, and composition in the screen. Behaviour that becomes
useful to another screen belongs in a service. Dev screens are sandboxes for
trying a feature with its own inspection controls.

## Services provide behaviour

A service registers the types, resources, and observers for one area of the
game. For work that a screen should schedule, return
[`ServiceSystems`](crate::prelude::ServiceSystems):

```rust,ignore
// In the service:
ServiceSystems::new((tick, apply).chain())

// In the screen's builder:
builder.add_systems(ScreenSchedule::Update, service_systems().take());
```

The service exposes its ordering requirements; the screen chooses its active
schedule. This lets the same behaviour run in the game and in a dev sandbox.
Avoid embedding screen-specific run conditions in the service.

Some work is registered globally. Player physics, for example, runs in
`FixedUpdate` and queries player components. External plugins often use the
same model. Control participation through their components and settings; a
screen-local schedule does not automatically gate a plugin's global systems.

Keep per-entity state on entities where practical. Expose tweakable values as
settings resources with defaults so the inspector and panels can edit them.
When work spans many frames, consider whether a
[`q_proc`](https://github.com/cubething-qproj/q_proc) program would express its
lifetime and communication more clearly. See the `q-os` and `q-proc` skills
before introducing another orchestration mechanism.

## Common code supports its callers

Common code supplies mechanics that several features can use without taking
ownership of those features. The billboard module is one example: it exposes
a marker and a facing system; the caller creates the geometry and schedules
the system.

A supporting system does not by itself make a module a service. The distinction
is responsibility: services implement game behaviour; common modules provide
reusable pieces for their callers to assemble.

## Application configuration

Plugin setup configures external crates, registers resources, and establishes
application-wide defaults. Player-facing settings such as audio or controls
belong to game behaviour and need a runtime API as well as initial values.

For UI, prefer theme tokens over colours embedded in widgets. Use Feathers'
tokens for dev tools and a separate namespace for game UI tokens. Both can
live in Feathers' global `UiTheme` without sharing every visual choice.

## Preludes and file layout

Each module exports the public types and systems its callers need through a
`prelude`. The crate prelude collects these and the external-crate re-exports,
so most files can start with `use crate::prelude::*;`. Keep other internals
private.

Within a feature, separate types from Bevy systems:

```text
my_module/
├── mod.rs      # exports and a minimal plugin
├── plugin.rs   # optional separate plugin registration
├── data.rs     # components, resources, and other types
├── systems.rs  # systems and observers
└── test.rs     # unit tests
```

Screens use `screen.rs` for their `Screen` implementation. A large `data.rs`
can become a directory, with complex types and their implementations in their
own modules. Pure algorithms can also live beside their owning types; they
do not need to be written as Bevy systems.

Read a file from definitions to behaviour: module docs, imports, exports,
types and their implementations, free functions, plugin registration, tests.
Use associated methods when an operation belongs to a type.

For event delivery and system ordering, see [`scheduling`](crate::docs::scheduling).
For workspace conventions and documentation, see
[`development`](crate::docs::development).

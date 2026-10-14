## Build from the workspace root

Quell is a crate in the qproj Cargo workspace. The `bin/` and `lib/` checkouts
are separate Git repositories; the org `Cargo.toml` patches library Git
dependencies to those local checkouts.

Run Cargo and `just` commands from the org root so builds share its `target/`
directory. Never build in `/tmp`. Commit Quell changes inside `bin/quell`,
and library changes inside the library's repository.

The usual checks are:

- `just check -p quell` runs Clippy.
- `just test -p quell` runs nextest.

Standalone CI uses Quell's own manifest and remote library dependencies. It
does not inherit the org workspace's patches or build profiles. Land library
changes upstream before relying on them in Quell, and compare profile settings
when local and CI performance differ.

Before adding a reusable abstraction, check whether an existing library owns
it. The `q-os` skill maps the libraries and links to their API guides.

## Features and launch settings

- `dev` enables sandbox screens, inspection tools, and command-line arguments.
- `dylib` enables Bevy dynamic linking to shorten development relinks.
- `jackdaw` enables Jackdaw's play-in-editor integration.

These are default features. `--release` changes the build profile; it does not
disable them. Builds intended to omit dev tooling must select their features
explicitly, starting with `--no-default-features`.

With `dev` enabled, `--screen <name>` selects the initial screen. Use `--help`
for the remaining command-line options.

## Author levels in Jackdaw

Levels are `.bsn` scenes. Register and reflect gameplay components that should
be available in the editor; see the `jackdaw` skill for the integration.
Prefer scene-authored layout to level geometry embedded in Rust. A screen can
load the scene and let its components request the behaviour they need.

## Keep documentation close to its subject

The guides explain project-wide decisions and workflows. Module comments
explain a feature's pipeline and type relationships. Type and method comments
explain how to use the API and why non-obvious operations exist.

Link between those layers instead of repeating an explanation in each one.
Keep a short summary where readers enter the API, then link to the details.
Use short, code-formatted link labels such as
``[`Window::choose`](path::to::Window::choose)``.

Document every public function and any implementation whose intent is not
clear from its signature. Describe the current behaviour; label proposals and
limitations explicitly. Review transcripts and staging history belong with
review artifacts, rather than in maintained API guides.

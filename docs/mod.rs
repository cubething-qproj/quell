//! Project guides for ownership, scheduling, development, and testing.

/// Module responsibilities, entity lifetimes, and composition.
#[doc = include_str!("architecture.md")]
pub mod architecture {}

/// Event delivery, schedule scope, and execution order.
#[doc = include_str!("scheduling.md")]
pub mod scheduling {}

/// Workspace builds, feature selection, and documentation conventions.
#[doc = include_str!("development.md")]
pub mod development {}

/// Test boundaries, fixtures, and schedule coverage.
#[doc = include_str!("testing.md")]
pub mod testing {}

/// How the hex preview connects terrain generation, layout, and rendering.
#[cfg(feature = "dev")]
#[doc = include_str!("hex-paths-overview.md")]
pub mod hex_paths {}

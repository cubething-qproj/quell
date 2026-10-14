//! # Quell
//!
//! [`screen`] composes game features, [`service`] provides their behaviour,
//! and [`common`] holds reusable components and supporting mechanics.
//! [`AppPlugin`] wires them together.
//!
//! For the ownership rules and development workflow, see [`docs`].

/// Architecture, scheduling, development, and testing guides.
#[cfg(doc)]
#[path = "../docs/mod.rs"]
pub mod docs;

/// Screen definitions and registration.
pub mod screen;

/// Components and mechanics shared by features.
pub mod common;
/// Game services and their registration.
pub mod service;

mod plugin;
pub use plugin::{AppPlugin, AppSettings};

/// Everything most modules need: the module preludes plus q_screens.
pub mod prelude {
    pub use super::common::prelude::*;
    pub use super::screen::prelude::*;
    pub use super::service::prelude::*;

    pub use q_screens::prelude::*;
}

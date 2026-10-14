//! Components and mechanics shared by screens and services.
//!
//! Callers choose the entity lifetimes and schedules for these reusable pieces.

pub mod billboard;

/// Common components and systems available through the crate prelude.
pub mod prelude {
    pub use super::billboard::prelude::*;
}

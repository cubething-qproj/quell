//! Reusable components and systems shared between game screens.
//!
//! [`billboard`](crate::common::billboard) provides camera-facing quads while screens retain entity
//! ownership, asset selection, and schedule placement.

pub mod billboard;

/// Shared components and utilities available through the crate prelude.
pub mod prelude {
    pub use super::billboard::prelude::*;
}

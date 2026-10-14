//! Shared billboard data and screen-scheduled camera facing.
//!
//! [`Billboard`](data::Billboard) marks quads; [`face_camera`](systems::face_camera)
//! copies the rotation of a camera selected by its marker type. Screens own
//! creation and scheduling, so this module installs no global update system.

mod data;
mod systems;

/// Shared component and facing system for screen-owned billboards.
pub mod prelude {
    pub use super::{data::Billboard, systems::face_camera};
}

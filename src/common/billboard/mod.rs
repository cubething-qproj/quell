//! Camera-facing orientation for [`Billboard`](data::Billboard) quads.
//!
//! [`face_camera`](systems::face_camera) applies the selected camera's rotation;
//! the marker's docs describe how to use it in a screen.

mod data;
mod systems;

/// Shared component and facing system for screen-owned billboards.
pub mod prelude {
    pub use super::{data::Billboard, systems::face_camera};
}

//! Render layers, camera order, and camera input actions.

use crate::prelude::*;
use bitflags::bitflags;

mod input_ctx;
pub use input_ctx::*;

bitflags! {
    /// Named render layers. Converts into [`RenderLayers`].
    pub struct RenderLayer: u32 {
        /// Used implicitly by all entities without a `RenderLayers` component.
        /// Light source should be on DEFAULT _and_ PLAYER
        const DEFAULT = 1 << 0;
        /// For rendering first-person view
        const PLAYER = 1 << 1;
        /// Indicates this camera can render particles
        const PARTICLES = 1 << 2;
        /// Should only be rendered by a Camera3D
        const GIZMOS_3D = 1 << 3;
        /// Shared by the hex preview camera, terrain, markers, and gizmos.
        /// Keeps the preview separate from normal world and UI cameras.
        const HEX_PREVIEW = 1 << 4;
    }
}
impl From<RenderLayer> for RenderLayers {
    fn from(layer: RenderLayer) -> Self {
        // Render layers are just vectors of ints, so we convert each active bit to its index.
        RenderLayers::from_iter(layer.iter().map(|l| l.bits().trailing_zeros() as usize))
    }
}

/// Camera rendering order: later variants draw after earlier ones.
/// This controls compositing order; [`RenderLayer`] selects which entities a camera sees.
#[derive(Debug)]
pub enum CameraOrder {
    /// World cameras.
    World,
    /// The first-person view.
    Player,
    /// UI, drawn over everything.
    Ui,
}

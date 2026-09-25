use crate::prelude::*;

pub fn flycam_bundle() -> impl Bundle {
    let mut state = FreeCameraState::default();
    state.enabled = false;

    (
        Camera3d::default(),
        Camera {
            order: CameraOrder::World as isize,
            ..default()
        },
        FreeCamera::default(),
        state,
        FreeCameraInput::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::from(RenderLayer::DEFAULT | RenderLayer::GIZMOS_3D | RenderLayer::PARTICLES),
    )
}

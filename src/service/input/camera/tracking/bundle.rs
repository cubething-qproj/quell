use crate::prelude::*;

pub fn tracking_cam_bundle(target: Entity) -> impl Bundle {
    (
        SpringArm::new(target),
        Camera3d::default(),
        Camera {
            order: CameraOrder::World as isize,
            is_active: true,
            ..Default::default()
        },
        Transform::default(),
        RenderLayers::from(RenderLayer::DEFAULT | RenderLayer::GIZMOS_3D | RenderLayer::PARTICLES),
        ContextActivity::<SpringArm>::INACTIVE,
        actions![
            SpringArm[
                (
                    Action::<PARotateCam>::new(),
                    Bindings::spawn((
                        Axial::right_stick().with((
                            DeadZone::default(),
                            Scale::splat(2.0),
                            Negate::x(),
                            DeltaScale::default(),
                        )),
                        Spawn((Binding::mouse_motion(), Scale::splat(0.01), Negate::all()))
                    )),
                ),
                (
                    Action::<PAZoomCam>::new(),
                    DeadZone::default(),
                    SmoothNudge::default(),
                    Scale::splat(PLAYER_CAM_ZOOM_SPD),
                    Bindings::spawn(Spawn((
                        Binding::mouse_wheel(),
                        Scale::splat(0.1),
                        SwizzleAxis::YXZ,
                    )))
                ),
            ]
        ],
    )
}

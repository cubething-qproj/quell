use crate::prelude::*;

/// Creates a tracking camera and its input actions for the spring arm's target.
pub fn tracking_cam_scene(target: Entity) -> impl Scene {
    bsn! {
        template(move |_| Ok(SpringArm::new(target)))
        Camera3d::default()
        Camera {
            order: {CameraOrder::World as isize},
            is_active: true,
        }
        Transform::default()
        RenderLayers::from(RenderLayer::DEFAULT | RenderLayer::GIZMOS_3D | RenderLayer::PARTICLES)
        ContextActivity::<SpringArm>::INACTIVE
        Actions<SpringArm> [
            Action::<PARotateCam>::new()
            Bindings [
                {preset::axial::new(
                    (
                        GamepadAxis::RightStickX,
                        DeadZone::default(),
                        Scale::splat(2.0),
                        Negate::x(),
                        DeltaScale::default(),
                    ),
                    (
                        GamepadAxis::RightStickY,
                        DeadZone::default(),
                        Scale::splat(2.0),
                        Negate::x(),
                        DeltaScale::default(),
                    ),
                )}
                --
                Binding::mouse_motion()
                ~Scale::splat(0.01)
                ~Negate::all()
            ]
            --
            Action::<PAZoomCam>::new()
            DeadZone {
                kind: DeadZoneKind::Radial,
                lower_threshold: 0.,
                upper_threshold: 1.,
            }
            SmoothNudge::default()
            Bindings [
                Binding::mouse_wheel()
                ~Scale::splat(0.1)
                ~SwizzleAxis::YXZ
            ]
        ]
    }
}

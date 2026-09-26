use std::time::Duration;

use crate::prelude::*;
use bevy::{
    ecs::schedule::ScheduleLabel,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use q_test_harness::prelude::{AppExt as _, InputTestPlugin};

struct Fixture {
    app: App,
    camera: Entity,
    window: Entity,
}

impl Fixture {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputTestPlugin,
            TransformPlugin,
            EnhancedInputPlugin,
            SpringArmCameraPlugin,
            super::plugin,
        ));
        app.finish();
        app.cleanup();
        let target = app.world_mut().spawn(Transform::from_xyz(1., 2., 3.)).id();
        let camera = app.world_mut().spawn(tracking_cam_bundle(target)).id();
        let window = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap()
        };
        Self {
            app,
            camera,
            window,
        }
    }

    fn capture(&mut self) {
        self.app
            .world_mut()
            .entity_mut(self.camera)
            .insert(ContextActivity::<SpringArm>::ACTIVE);
        self.app
            .world_mut()
            .get_mut::<CursorOptions>(self.window)
            .unwrap()
            .grab_mode = CursorGrabMode::Locked;
    }

    fn fire<A: InputAction>(&mut self, value: A::Output) {
        let world = self.app.world_mut();
        let action = world
            .query_filtered::<Entity, With<Action<A>>>()
            .single(world)
            .unwrap();
        world.trigger(Fire::<A> {
            context: self.camera,
            action,
            value,
            state: TriggerState::Fired,
            fired_secs: 0.,
            elapsed_secs: 0.,
        });
    }

    fn input(&mut self, dt: Duration) {
        self.app.step(dt, [PreUpdate.intern()]);
    }

    fn transform(&self) -> Transform {
        *self.app.world().get::<Transform>(self.camera).unwrap()
    }

    fn fov(&self) -> f32 {
        let Projection::Perspective(projection) =
            self.app.world().get::<Projection>(self.camera).unwrap()
        else {
            panic!("expected a perspective camera");
        };
        projection.fov
    }
}

#[test]
fn stale_orbit_and_zoom_events_recheck_every_input_gate() {
    for (selected, focused, grab_mode, active) in [
        (false, true, CursorGrabMode::Locked, true),
        (true, false, CursorGrabMode::Locked, true),
        (true, true, CursorGrabMode::None, true),
        (true, true, CursorGrabMode::Confined, true),
        (true, true, CursorGrabMode::Locked, false),
    ] {
        let mut f = Fixture::new();
        f.capture();
        let start = f.transform();
        let fov = f.fov();
        let world = f.app.world_mut();
        world.get_mut::<Camera>(f.camera).unwrap().is_active = selected;
        world.get_mut::<Window>(f.window).unwrap().focused = focused;
        world.get_mut::<CursorOptions>(f.window).unwrap().grab_mode = grab_mode;
        world
            .entity_mut(f.camera)
            .insert(ContextActivity::<SpringArm>::new(active));
        // Bypass evaluation to model a Fire already queued before ownership changed.
        f.fire::<PARotateCam>(Vec2::ONE);
        f.fire::<PAZoomCam>(1.);
        assert_eq!(f.transform(), start);
        assert_eq!(f.fov(), fov);
    }

    let mut f = Fixture::new();
    f.capture();
    // Even a selected camera with the same context activity is not necessarily a SpringArm.
    f.camera = f
        .app
        .world_mut()
        .spawn((Camera3d::default(), ContextActivity::<SpringArm>::ACTIVE))
        .id();
    let start = f.transform();
    let fov = f.fov();
    f.fire::<PARotateCam>(Vec2::ONE);
    f.fire::<PAZoomCam>(1.);
    assert_eq!(f.transform(), start);
    assert_eq!(f.fov(), fov);
}

#[test]
fn orbit_uses_transform_rotation_and_orbit_and_zoom_saturate() {
    let mut f = Fixture::new();
    f.capture();
    f.app
        .world_mut()
        .get_mut::<Transform>(f.camera)
        .unwrap()
        .rotation = Quat::from_euler(EulerRot::YXZ, 0.4, 0.1, 0.2);
    f.fire::<PARotateCam>(Vec2::new(0.3, 0.05));
    let expected = Quat::from_euler(EulerRot::YXZ, 0.7, 0.15, 0.);
    assert!(f.transform().rotation.abs_diff_eq(expected, 1e-5));

    for direction in [1., -1.] {
        f.fire::<PARotateCam>(Vec2::Y * direction * 100.);
        let (_, pitch, roll) = f.transform().rotation.to_euler(EulerRot::YXZ);
        assert!(pitch * direction > 0. && pitch.abs() < std::f32::consts::FRAC_PI_2);
        assert!(roll.abs() < 1e-5);
        f.fire::<PARotateCam>(Vec2::Y * direction * 100.);
        let (_, saturated, _) = f.transform().rotation.to_euler(EulerRot::YXZ);
        assert!((saturated - pitch).abs() < 1e-5);

        f.fire::<PAZoomCam>(direction * 100.);
        let fov = f.fov();
        assert!(fov > 0. && fov < std::f32::consts::PI);
        f.fire::<PAZoomCam>(direction * 100.);
        assert_eq!(f.fov(), fov);
    }
    f.app
        .world_mut()
        .entity_mut(f.camera)
        .insert(Projection::Orthographic(
            OrthographicProjection::default_3d(),
        ));
    f.fire::<PAZoomCam>(100.);
    assert!(matches!(
        f.app.world().get::<Projection>(f.camera),
        Some(Projection::Orthographic(_))
    ));
}

#[test]
fn zoom_speed_edits_apply_to_an_existing_camera() {
    let mut f = Fixture::new();
    f.capture();
    for speed in [0., -1., 2.] {
        f.app
            .world_mut()
            .resource_mut::<SpringArmCameraSettings>()
            .zoom_speed = speed;
        let before = f.fov();
        f.fire::<PAZoomCam>(0.01);
        assert!((f.fov() - before - 0.01 * speed).abs() < 1e-5);
    }
}

#[test]
fn mouse_orbit_is_time_independent_but_stick_orbit_scales_with_delta_and_never_zooms() {
    let sample = |millis, mouse, stick: Vec2| {
        let mut f = Fixture::new();
        f.capture();
        let fov = f.fov();
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadAxis::RightStickX, stick.x);
        gamepad.analog_mut().set(GamepadAxis::RightStickY, stick.y);
        f.app.world_mut().spawn(gamepad);
        f.app
            .world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = mouse;
        f.input(Duration::from_millis(millis));
        assert_eq!(f.fov(), fov);
        f.transform().rotation
    };
    let mouse = sample(10, Vec2::new(3., 2.), Vec2::ZERO);
    assert!(mouse.abs_diff_eq(sample(20, Vec2::new(3., 2.), Vec2::ZERO), 1e-5));
    let (yaw, pitch, _) = mouse.to_euler(EulerRot::YXZ);
    assert!(yaw < 0. && pitch < 0.);

    let (yaw, pitch, _) = sample(10, Vec2::ZERO, Vec2::splat(0.5)).to_euler(EulerRot::YXZ);
    let (double_yaw, double_pitch, _) =
        sample(20, Vec2::ZERO, Vec2::splat(0.5)).to_euler(EulerRot::YXZ);
    assert!(yaw < 0. && pitch > 0.);
    assert!((double_yaw - 2. * yaw).abs() < 1e-5);
    assert!((double_pitch - 2. * pitch).abs() < 1e-5);
    assert_eq!(
        sample(20, Vec2::ZERO, Vec2::splat(f32::EPSILON)),
        Quat::IDENTITY
    );

    let mut f = Fixture::new();
    f.capture();
    let fov = f.fov();
    f.app
        .world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::Y * 4.;
    f.input(Duration::from_millis(16));
    let zoom_speed = f
        .app
        .world()
        .resource::<SpringArmCameraSettings>()
        .zoom_speed;
    assert!((f.fov() - fov) * zoom_speed > 0.);
}

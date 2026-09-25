use crate::prelude::*;
use bevy::window::PrimaryWindow;

const FREE_HELP: &str = "Camera: Free | Tab switch camera\n\
    RMB hold / M toggle capture\n\
    Esc release (again to quit)\n\
    WASD move | Q/E down/up\n\
    Shift faster | Wheel speed";
const TRACKING_HELP: &str = "Camera: Tracking | Tab switch camera\n\
    Left click capture\n\
    Mouse / right stick orbit | Wheel FOV zoom\n\
    Esc release (again to quit)";

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct CameraTestScreen;
impl Screen for CameraTestScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, init);
        builder.add_systems(ScreenSchedule::Update, (update, update_help));
        builder
    }
    fn name() -> String {
        "camera_test".into()
    }
}

/// spawn the scene.
/// this is temp, ideally load the scene from file
/// then spawn it
fn init(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut screen: ScreenInfoMut<CameraTestScreen>,
) {
    // spawn everything
    let cube = meshes.add(Cuboid::default());
    let plane = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(100.)));
    let wall = meshes.add(Cuboid::new(100., 100., 1.));
    let material = materials.add(StandardMaterial::default());

    let cube_entity = commands
        .spawn((
            Cube,
            Transform::default(),
            Mesh3d(cube),
            MeshMaterial3d(material.clone()),
            Collider::cuboid(1., 1., 1.),
        ))
        .id();
    commands.spawn((
        Transform::default(),
        Mesh3d(plane),
        MeshMaterial3d(material.clone()),
        Collider::half_space(Vec3::Y),
    ));
    commands.spawn((
        Transform::from_xyz(0., 0., -10.),
        Mesh3d(wall),
        MeshMaterial3d(material),
        Collider::cuboid(100., 100., 1.),
    ));
    commands.spawn((PointLight::default(), Transform::from_xyz(0., 3., 0.)));
    commands.trigger(SpawnGlobalCtx);
    commands.trigger(SpawnCursorCapture);
    commands.spawn((flycam_bundle(), ScreenScoped, Name::new("Free Camera")));
    commands.spawn(tracking_cam_bundle(cube_entity)).insert((
        Camera {
            order: CameraOrder::World as isize,
            is_active: false,
            ..default()
        },
        ScreenScoped,
        Name::new("Tracking Camera"),
    ));
    commands.spawn(CameraTestInput::bundle());
    commands.spawn((
        Name::new("Camera Help"),
        ScreenScoped,
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            padding: UiRect::all(px(12)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        BackgroundColor(Color::srgba(0., 0., 0., 0.7)),
        children![(
            CameraTestHelp,
            Text::new(FREE_HELP),
            TextFont::default().with_font_size(16.),
        )],
    ));

    screen.finish_loading();
}

pub(super) fn switch_camera(
    _: On<Start<PASwitchCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut tracking: Single<&mut Camera, (With<SpringArm>, Without<FreeCamera>)>,
    mut free: Single<&mut Camera, (With<FreeCamera>, Without<SpringArm>)>,
    mut commands: Commands,
) {
    if !window.focused {
        return;
    }

    let tracking_active = free.is_active;
    free.is_active = !tracking_active;
    tracking.is_active = tracking_active;
    commands.trigger(ReleaseCursor);
}

fn update_help(
    mut help: Single<&mut Text, With<CameraTestHelp>>,
    free: Single<&Camera, (With<FreeCamera>, Without<SpringArm>)>,
    tracking: Single<&Camera, (With<SpringArm>, Without<FreeCamera>)>,
) {
    let text = if free.is_active {
        FREE_HELP
    } else if tracking.is_active {
        TRACKING_HELP
    } else {
        return;
    };
    if help.0 != text {
        help.0 = text.into();
    }
}

fn update(mut query: Query<&mut Transform, With<Cube>>, time: Res<Time>) {
    let mut tf = r!(query.single_mut());
    *tf = tf.with_translation(Vec3::new(
        3. * f32::cos(time.elapsed_secs()) - 1.5,
        1.,
        3. * f32::sin(time.elapsed_secs()) - 1.5,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        ecs::schedule::ScheduleLabel,
        input::mouse::AccumulatedMouseScroll,
        window::{CursorGrabMode, CursorOptions},
    };
    use q_test_harness::prelude::{AppExt as _, InputTestPlugin};
    use std::time::Duration;

    #[derive(Resource, Default)]
    struct ReleaseRequests(usize);

    struct Fixture {
        app: App,
        window: Entity,
        free: Entity,
        tracking: Entity,
        ui: Entity,
        help: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                InputTestPlugin,
                EnhancedInputPlugin,
                crate::service::input_plugin,
            ))
            .add_input_context::<CameraTestInput>()
            .add_observer(switch_camera)
            .add_observer(
                |_: On<ReleaseCursor>, mut requests: ResMut<ReleaseRequests>| {
                    requests.0 += 1;
                },
            )
            .add_systems(Update, update_help)
            .init_resource::<ReleaseRequests>();
            app.finish();
            app.cleanup();
            let world = app.world_mut();
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.trigger(SpawnGlobalCtx);
            world.trigger(SpawnCursorCapture);
            world.flush();
            let target = world.spawn(Transform::default()).id();
            let free = world.spawn(flycam_bundle()).id();
            let tracking = world
                .spawn(tracking_cam_bundle(target))
                .insert((
                    Camera {
                        order: CameraOrder::World as isize,
                        is_active: false,
                        ..default()
                    },
                    Transform::from_xyz(3., 2., 1.).with_rotation(Quat::from_rotation_y(0.5)),
                ))
                .id();
            let ui = world.spawn(Camera2d).id();
            let help = world.spawn((CameraTestHelp, Text::new(FREE_HELP))).id();
            world.spawn(CameraTestInput::bundle());
            Self {
                app,
                window,
                free,
                tracking,
                ui,
                help,
            }
        }

        fn tick(&mut self) {
            // Omit PostUpdate so target following cannot change the saved poses.
            self.app.step(
                Duration::from_millis(16),
                [
                    PreUpdate.intern(),
                    RunFixedMainLoop.intern(),
                    Update.intern(),
                ],
            );
        }

        fn key(&mut self, key: KeyCode, pressed: bool) {
            self.app.key(key, pressed);
        }

        fn tab(&mut self, pressed: bool) {
            self.key(KeyCode::Tab, pressed);
            self.tick();
        }

        fn mouse(&mut self, button: MouseButton, pressed: bool) {
            self.app.mouse(button, pressed);
        }

        fn left_click(&mut self) {
            self.mouse(MouseButton::Left, true);
            self.tick();
            self.mouse(MouseButton::Left, false);
            self.tick();
        }

        fn motion(&mut self) {
            self.app.motion(Vec2::new(12., 6.));
        }

        fn transform(&self, entity: Entity) -> Transform {
            *self.app.world().get::<Transform>(entity).unwrap()
        }

        fn fov(&self) -> f32 {
            let Projection::Perspective(projection) =
                self.app.world().get::<Projection>(self.tracking).unwrap()
            else {
                panic!("expected perspective camera");
            };
            projection.fov
        }

        fn scroll(&mut self) {
            self.app
                .world_mut()
                .resource_mut::<AccumulatedMouseScroll>()
                .delta = Vec2::Y * 3.;
        }

        fn assert_capture(&self, captured: bool) {
            let world = self.app.world();
            let cursor = world.get::<CursorOptions>(self.window).unwrap();
            assert_eq!(cursor.visible, !captured);
            assert_eq!(
                cursor.grab_mode,
                if captured {
                    CursorGrabMode::Locked
                } else {
                    CursorGrabMode::None
                }
            );
            assert_eq!(
                world.get::<FreeCameraState>(self.free).unwrap().enabled,
                captured && world.get::<Camera>(self.free).unwrap().is_active,
            );
            assert_eq!(
                **world
                    .get::<ContextActivity<SpringArm>>(self.tracking)
                    .unwrap(),
                captured && world.get::<Camera>(self.tracking).unwrap().is_active,
            );
        }

        fn assert_selected(&self, tracking: bool, release_requests: usize) {
            let world = self.app.world();
            assert_eq!(world.get::<Camera>(self.free).unwrap().is_active, !tracking);
            assert_eq!(
                world.get::<Camera>(self.tracking).unwrap().is_active,
                tracking
            );
            assert!(world.get::<Camera>(self.ui).unwrap().is_active);
            assert_eq!(world.resource::<ReleaseRequests>().0, release_requests);
            assert!(
                world
                    .get::<Text>(self.help)
                    .unwrap()
                    .0
                    .starts_with(if tracking {
                        "Camera: Tracking"
                    } else {
                        "Camera: Free"
                    })
            );
        }
    }

    #[test]
    fn tab_switches_once_per_press_preserving_poses_and_ui() {
        let mut f = Fixture::new();
        let poses = [f.free, f.tracking, f.ui]
            .map(|entity| *f.app.world().get::<Transform>(entity).unwrap());
        f.assert_selected(false, 0);
        f.tick();
        f.tab(true);
        f.assert_selected(true, 1);
        f.tick();
        f.assert_selected(true, 1);
        f.tab(false);
        f.assert_selected(true, 1);
        f.tab(true);
        f.assert_selected(false, 2);
        for (entity, pose) in [f.free, f.tracking, f.ui].into_iter().zip(poses) {
            assert_eq!(*f.app.world().get::<Transform>(entity).unwrap(), pose);
        }
    }

    #[test]
    fn tab_held_on_entry_requires_release_before_switching() {
        let mut f = Fixture::new();
        f.tab(true);
        f.tick();
        f.assert_selected(false, 0);
        f.tab(false);
        f.tab(true);
        f.assert_selected(true, 1);
    }

    #[test]
    fn unfocused_window_does_not_switch_or_request_release() {
        let mut f = Fixture::new();
        f.tick();
        f.app
            .world_mut()
            .get_mut::<Window>(f.window)
            .unwrap()
            .focused = false;
        f.tab(true);
        f.assert_selected(false, 0);
    }

    #[test]
    fn captured_handoffs_require_fresh_gestures_and_clear_player_movement() {
        let mut f = Fixture::new();
        f.tick();
        f.mouse(MouseButton::Right, true);
        f.tick();
        f.assert_capture(true);
        f.mouse(MouseButton::Left, true);
        f.tick();
        f.tab(true);
        f.assert_selected(true, 1);
        f.assert_capture(false);
        f.tab(false);
        f.mouse(MouseButton::Left, false);
        f.tick();
        f.assert_capture(false);
        f.tick();
        f.assert_capture(false);
        f.left_click();
        f.assert_capture(true);

        let mut controller = PlayerTnuaController::default();
        controller.basis.desired_motion = Vec3::X;
        controller.basis.desired_forward = Some(Dir3::X);
        let player = f
            .app
            .world_mut()
            .spawn((
                ICtxDefault,
                ContextActivity::<ICtxDefault>::ACTIVE,
                PlayerController {
                    last_move: Some(Vec3::X),
                },
                controller,
            ))
            .id();
        let free_pose = f.transform(f.free);
        let tracking_pose = f.transform(f.tracking);
        f.motion();
        f.tick();
        assert_eq!(f.transform(f.free), free_pose);
        assert_ne!(f.transform(f.tracking).rotation, tracking_pose.rotation);
        assert!(
            **f.app
                .world()
                .get::<ContextActivity<ICtxDefault>>(player)
                .unwrap()
        );
        assert!(
            f.app
                .world()
                .get::<PlayerController>(player)
                .unwrap()
                .last_move
                .is_some()
        );

        let tracking_pose = f.transform(f.tracking);
        // Orbit input evaluated before the switch must not reach either camera.
        f.motion();
        f.tab(true);
        f.assert_selected(false, 2);
        f.assert_capture(false);
        assert_eq!(f.transform(f.free), free_pose);
        assert_eq!(f.transform(f.tracking), tracking_pose);
        let world = f.app.world();
        assert!(!**world.get::<ContextActivity<ICtxDefault>>(player).unwrap());
        assert!(
            world
                .get::<PlayerController>(player)
                .unwrap()
                .last_move
                .is_none()
        );
        let controller = world.get::<PlayerTnuaController>(player).unwrap();
        assert_eq!(controller.basis.desired_motion, Vec3::ZERO);
        assert!(controller.basis.desired_forward.is_none());

        // RMB stayed held throughout both switches; it cannot recapture Free.
        f.tab(false);
        f.tick();
        f.assert_capture(false);
        f.mouse(MouseButton::Right, false);
        f.tick();
        f.assert_capture(false);
        f.mouse(MouseButton::Right, true);
        f.tick();
        f.assert_capture(true);
    }

    #[test]
    fn tracking_escape_releases_before_a_second_escape_quits() {
        let mut f = Fixture::new();
        f.tick();
        f.tab(true);
        f.tab(false);
        f.left_click();
        f.assert_capture(true);

        f.key(KeyCode::Escape, true);
        f.tick();
        let pose = f.transform(f.tracking);
        let fov = f.fov();
        f.motion();
        f.scroll();
        f.key(KeyCode::Escape, false);
        f.tick();
        f.assert_capture(false);
        assert_eq!(f.transform(f.tracking), pose);
        assert_eq!(f.fov(), fov);
        assert!(f.app.world().resource::<Messages<AppExit>>().is_empty());

        f.key(KeyCode::Escape, true);
        f.tick();
        f.key(KeyCode::Escape, false);
        f.tick();
        assert!(!f.app.world().resource::<Messages<AppExit>>().is_empty());
    }

    #[test]
    fn recapturing_tracking_does_not_resume_old_zoom() {
        let mut f = Fixture::new();
        f.tick();
        f.tab(true);
        f.tab(false);
        f.left_click();
        let initial_fov = f.fov();
        f.scroll();
        f.tick();
        assert_ne!(f.fov(), initial_fov);

        f.key(KeyCode::Escape, true);
        f.tick();
        f.key(KeyCode::Escape, false);
        f.tick();
        f.assert_capture(false);
        let released_fov = f.fov();
        f.tick();
        f.left_click();
        f.assert_capture(true);
        f.tick();
        assert_eq!(f.fov(), released_fov);
    }

    #[test]
    fn tracking_focus_loss_disarms_a_held_click_until_a_fresh_gesture() {
        for captured in [false, true] {
            let mut f = Fixture::new();
            f.tick();
            f.tab(true);
            f.tab(false);
            if captured {
                f.left_click();
            }
            f.assert_capture(captured);
            f.mouse(MouseButton::Left, true);
            f.tick();
            f.app
                .world_mut()
                .get_mut::<Window>(f.window)
                .unwrap()
                .focused = false;
            f.tick();
            f.assert_capture(false);

            f.app
                .world_mut()
                .get_mut::<Window>(f.window)
                .unwrap()
                .focused = true;
            f.tick();
            f.assert_capture(false);
            f.mouse(MouseButton::Left, false);
            f.tick();
            f.assert_capture(false);
            f.tick();
            f.left_click();
            f.assert_capture(true);
        }
    }
}

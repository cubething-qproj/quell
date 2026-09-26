use bevy::window::PrimaryWindow;
use bevy_tnua_avian3d::TnuaAvian3dSensorShape;

use crate::prelude::*;

// TODO: Split this out into a bundle
fn spawn_player_root(
    _: On<SpawnPlayerRoot>,
    mut commands: Commands,
    player_assets: Res<PlayerAssets>,
    settings: Res<PlayerSettings>,
) {
    let player_entt = commands
        .spawn((
            PlayerController::default(),
            ScreenScoped,
            WorldAssetRoot(player_assets.model.clone()),
            (
                RigidBody::Dynamic,
                Collider::capsule(settings.capsule_radius, settings.capsule_height),
                CollisionLayers::new(CollisionLayer::Player, LayerMask::ALL),
                LockedAxes::ROTATION_LOCKED.unlock_rotation_y(),
                Friction::ZERO,
            ),
            (
                PlayerTnuaController::default(),
                TnuaAvian3dSensorShape(Collider::cylinder(settings.capsule_radius + 0.1, 0.)),
                ICtxDefault,
                ContextActivity::<ICtxDefault>::ACTIVE,
                actions!(
                    ICtxDefault[(
                        Action::<PAMove>::new(),
                        DeadZone::default(),
                        SmoothNudge::default(),
                        Negate::y(),
                        SwizzleAxis::XZY,
                        Bindings::spawn((Cardinal::wasd_keys(), Axial::left_stick())),
                    )]
                ),
            ),
        ))
        .id();

    commands.spawn((
        Name::new("PlayerCam"),
        ScreenScoped,
        (LockedAxes::new().lock_rotation_z(),),
        (
            #[cfg(feature = "dev")]
            ShowLightGizmo::default(),
            PointLight::default(),
        ),
        tracking_cam_bundle(player_entt),
    ));
}

fn on_move(
    trigger: On<Fire<PAMove>>,
    settings: Res<PlayerSettings>,
    mut controller: Single<&mut PlayerController>,
    camera: Single<&Camera, With<SpringArm>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    controller.last_move =
        (camera.is_active && window.focused).then_some(trigger.value * settings.default_speed);
}

pub fn plugin(app: &mut App) {
    app.add_observer(on_move).add_observer(spawn_player_root);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::ScheduleLabel;
    use q_test_harness::prelude::{AppExt as _, InputTestPlugin};
    use std::time::Duration;

    struct Fixture {
        app: App,
        player: Entity,
        camera: Entity,
        window: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                InputTestPlugin,
                EnhancedInputPlugin,
                crate::service::input::plugin,
                super::super::plugin,
            ))
            .init_resource::<PlayerAssets>()
            .add_systems(Update, player_systems().take());
            app.finish();
            app.cleanup();
            let world = app.world_mut();
            world.trigger(SpawnPlayerRoot);
            world.flush();
            let player = world
                .query_filtered::<Entity, With<PlayerController>>()
                .single(world)
                .unwrap();
            let camera = world
                .query_filtered::<Entity, With<SpringArm>>()
                .single(world)
                .unwrap();
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            Self {
                app,
                player,
                camera,
                window,
            }
        }

        fn tick(&mut self, frames: usize) {
            for _ in 0..frames {
                self.app.step(
                    Duration::from_millis(16),
                    [
                        PreUpdate.intern(),
                        RunFixedMainLoop.intern(),
                        Update.intern(),
                    ],
                );
            }
        }

        fn desired_motion(&self) -> Vec3 {
            self.app
                .world()
                .get::<PlayerTnuaController>(self.player)
                .unwrap()
                .basis
                .desired_motion
        }

        fn warm_move(&mut self) {
            self.tick(2);
            self.app.key(KeyCode::KeyW, true);
            self.tick(8);
            assert_ne!(self.desired_motion(), Vec3::ZERO);
        }

        fn release_while_inactive(&mut self) {
            self.tick(2);
            assert!(
                !**self
                    .app
                    .world()
                    .get::<ContextActivity<ICtxDefault>>(self.player)
                    .unwrap()
            );
            self.app.key(KeyCode::KeyW, false);
            self.tick(8);
            self.assert_stopped();
        }

        fn assert_stopped(&self) {
            assert_eq!(self.desired_motion(), Vec3::ZERO);
            assert!(
                self.app
                    .world()
                    .get::<PlayerController>(self.player)
                    .unwrap()
                    .last_move
                    .is_none()
            );
        }

        fn assert_restored_without_motion(&mut self) {
            // Context activation follows input evaluation; check the next evaluation too.
            self.tick(2);
            assert!(
                **self
                    .app
                    .world()
                    .get::<ContextActivity<ICtxDefault>>(self.player)
                    .unwrap()
            );
            self.assert_stopped();
        }
    }

    #[test]
    fn movement_speed_edits_apply_to_an_existing_player() {
        let mut f = Fixture::new();
        f.warm_move();
        f.app
            .world_mut()
            .resource_mut::<PlayerSettings>()
            .default_speed = 0.;
        f.tick(2);
        assert_eq!(f.desired_motion(), Vec3::ZERO);
        f.app
            .world_mut()
            .resource_mut::<PlayerSettings>()
            .default_speed = 2.;
        f.tick(2);
        assert!(f.desired_motion().length() > 0.);
    }

    #[test]
    fn spawned_player_keeps_world_contacts_with_player_membership() {
        let mut app = App::new();
        let dt = Duration::from_millis(16);
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .add_message::<AssetEvent<Mesh>>()
            .init_resource::<PlayerAssets>()
            .insert_resource(PlayerSettings {
                capsule_height: 1.5,
                capsule_radius: 0.75,
                ..default()
            })
            .add_plugins((PhysicsPlugins::default(), super::plugin))
            .insert_resource(Time::<Fixed>::from_duration(dt))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(dt));
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        world.trigger(SpawnPlayerRoot);
        world.flush();
        let player = world
            .query_filtered::<Entity, With<PlayerController>>()
            .single(world)
            .unwrap();
        let settings = world.resource::<PlayerSettings>();
        let resting_height = settings.capsule_height / 2. + settings.capsule_radius;
        let sensor = &world.get::<TnuaAvian3dSensorShape>(player).unwrap().0;
        assert!(sensor.shape().as_cylinder().unwrap().radius > settings.capsule_radius);
        let start_height = resting_height + 4.;
        world
            .entity_mut(player)
            .insert(Transform::from_xyz(0., start_height, 0.));
        world.spawn((
            RigidBody::Static,
            Collider::half_space(Vec3::Y),
            Transform::default(),
        ));
        // Exercise actual collision response, not just a layer-mask comparison.
        for _ in 0..180 {
            app.update();
        }
        let world = app.world_mut();
        assert_eq!(world.get::<ColliderOf>(player).unwrap().body, player);
        for (owner, layers) in world.query::<(&ColliderOf, &CollisionLayers)>().iter(world) {
            if owner.body == player {
                assert_ne!(layers.memberships.0 & CollisionLayer::Player.to_bits(), 0);
            }
        }
        let height = world.get::<Position>(player).unwrap().y;
        assert!((height - resting_height).abs() < 0.1, "height: {height}");
        assert!(world.get::<LinearVelocity>(player).unwrap().y.abs() < 0.1);
    }

    #[test]
    fn focus_restore_does_not_resume_movement_released_while_unfocused() {
        let mut f = Fixture::new();
        f.warm_move();
        f.app
            .world_mut()
            .get_mut::<Window>(f.window)
            .unwrap()
            .focused = false;
        f.release_while_inactive();
        f.app
            .world_mut()
            .get_mut::<Window>(f.window)
            .unwrap()
            .focused = true;
        f.assert_restored_without_motion();
    }

    #[test]
    fn camera_reselection_does_not_resume_movement_released_while_deselected() {
        let mut f = Fixture::new();
        f.warm_move();
        f.app
            .world_mut()
            .get_mut::<Camera>(f.camera)
            .unwrap()
            .is_active = false;
        f.release_while_inactive();
        f.app
            .world_mut()
            .get_mut::<Camera>(f.camera)
            .unwrap()
            .is_active = true;
        f.assert_restored_without_motion();
    }
}

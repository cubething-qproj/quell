use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};
use bevy::window::PrimaryWindow;

use crate::prelude::*;

fn spawn_player_root(
    trigger: On<SpawnPlayerRoot>,
    mut commands: Commands,
    settings: Res<PlayerSettings>,
) {
    commands.spawn((
        PlayerController::default(),
        ScreenScoped,
        // Spawn resting on the ground: move-and-slide can't resolve deep
        // initial penetration.
        trigger
            .transform
            .with_translation(trigger.transform.translation + Vec3::Y * settings.resting_height()),
    ));
}

/// Adds the parts of a [`PlayerController`] that need resources or other entities.
pub(super) fn on_add_player_controller(mut world: DeferredWorld, ctx: HookContext) {
    let (Some(assets), Some(settings)) = (
        world.get_resource::<PlayerAssets>(),
        world.get_resource::<PlayerSettings>(),
    ) else {
        warn!("PlayerController added without PlayerAssets and PlayerSettings; skipping setup");
        return;
    };
    let model = assets.model.clone();
    let collider = Collider::capsule(settings.capsule_radius, settings.capsule_height);
    let mut commands = world.commands();
    commands.entity(ctx.entity).insert((
        WorldAssetRoot(model),
        collider,
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
    ));
    commands.spawn((
        Name::new("PlayerCam"),
        PlayerCameraOf(ctx.entity),
        (LockedAxes::new().lock_rotation_z(),),
        (PointLight::default()),
        tracking_cam_bundle(ctx.entity),
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

fn on_player_spawn(
    trigger: On<Insert, PlayerSpawn>,
    spawns: Query<&GlobalTransform>,
    players: Query<(), With<PlayerController>>,
    mut commands: Commands,
) {
    // Scene reloads re-insert the spawn; only replace a missing player.
    if !players.is_empty() {
        return;
    }
    let Ok(spawn) = spawns.get(trigger.entity) else {
        return;
    };
    commands.trigger(SpawnPlayerRoot {
        transform: spawn.compute_transform().with_scale(Vec3::ONE),
    });
}

pub fn plugin(app: &mut App) {
    app.add_observer(on_move)
        .add_observer(spawn_player_root)
        .add_observer(on_player_spawn);
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
                TransformPlugin,
                EnhancedInputPlugin,
                crate::service::input::plugin,
            ))
            .init_resource::<Assets<Mesh>>()
            .add_message::<AssetEvent<Mesh>>()
            .add_plugins((PhysicsPlugins::default(), super::super::plugin))
            .init_resource::<PlayerAssets>()
            .add_systems(Update, player_systems().take());
            app.finish();
            app.cleanup();
            let world = app.world_mut();
            world.trigger(SpawnPlayerRoot::default());
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
                .get::<PlayerMotor>(self.player)
                .unwrap()
                .desired_velocity
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
    fn spawned_player_lands_on_the_ground_with_player_membership() {
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
            .add_plugins((
                PhysicsPlugins::default(),
                super::plugin,
                super::super::movement::plugin,
            ))
            .insert_resource(Time::<Fixed>::from_duration(dt))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(dt));
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        world.trigger(SpawnPlayerRoot::default());
        world.flush();
        let player = world
            .query_filtered::<Entity, With<PlayerController>>()
            .single(world)
            .unwrap();
        let resting_height = world.resource::<PlayerSettings>().resting_height();
        let start_height = resting_height + 4.;
        world
            .entity_mut(player)
            .insert(Transform::from_xyz(0., start_height, 0.));
        world.spawn((
            RigidBody::Static,
            Collider::half_space(Vec3::Y),
            Transform::default(),
        ));
        // Let the player fall and land through `move_player`, not just compare layer masks.
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
    fn spawn_point_spawns_one_player_and_replaces_a_missing_one() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .add_message::<AssetEvent<Mesh>>()
            .init_resource::<PlayerAssets>()
            .init_resource::<PlayerSettings>()
            .add_plugins((PhysicsPlugins::default(), super::plugin));
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        let at = Transform::from_xyz(3., 1., -2.).with_rotation(Quat::from_rotation_y(1.));
        // Jackdaw's loader sets `GlobalTransform` before user components go in.
        let spawn = world
            .spawn((at, GlobalTransform::from(at), PlayerSpawn))
            .id();
        world.flush();
        let players = |world: &mut World| {
            world
                .query_filtered::<(Entity, &Transform), With<PlayerController>>()
                .iter(world)
                .map(|(entity, transform)| (entity, *transform))
                .collect::<Vec<_>>()
        };
        let [(player, transform)] = players(world)[..] else {
            panic!("expected one player");
        };
        let resting_height = world.resource::<PlayerSettings>().resting_height();
        assert_eq!(
            transform.translation,
            at.translation + Vec3::Y * resting_height
        );
        assert_eq!(transform.rotation, at.rotation);

        // A scene reload re-inserts the spawn.
        world.entity_mut(spawn).insert(PlayerSpawn);
        world.flush();
        assert_eq!(players(world).len(), 1);

        world.entity_mut(player).despawn();
        world.entity_mut(spawn).insert(PlayerSpawn);
        world.flush();
        assert_eq!(players(world).len(), 1);
        let cameras = world
            .query_filtered::<(), With<SpringArm>>()
            .iter(world)
            .count();
        assert_eq!(cameras, 1, "the old camera despawns with its player");
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

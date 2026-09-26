use std::time::Duration;

use crate::prelude::*;
use bevy::{camera::visibility::VisibilitySystems, time::TimeUpdateStrategy};

const DT: Duration = Duration::from_millis(16);
const CLEARANCE: f32 = 0.02;
const TOLERANCE: f32 = 1e-3;

#[derive(Resource, Default)]
struct FrustumPose(Option<(Transform, GlobalTransform)>);

struct Fixture {
    app: App,
    target: Entity,
    camera: Entity,
}

impl Fixture {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .add_message::<AssetEvent<Mesh>>()
            .add_plugins((PhysicsPlugins::default(), SpringArmCameraPlugin))
            .insert_resource(SpringArmCollisionFilter {
                included: LayerMask(CollisionLayer::Default.to_bits()),
                excluded_memberships: LayerMask(CollisionLayer::Player.to_bits()),
            })
            .insert_resource(Gravity(Vec3::ZERO))
            .insert_resource(Time::<Fixed>::from_duration(DT))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .init_resource::<FrustumPose>()
            .add_systems(
                PostUpdate,
                (|camera: Single<(&Transform, &GlobalTransform), With<SpringArm>>,
                  mut pose: ResMut<FrustumPose>| {
                    pose.0 = Some((*camera.0, *camera.1));
                })
                .in_set(VisibilitySystems::UpdateFrusta),
            );
        app.finish();
        app.cleanup();
        let target = app.world_mut().spawn(Transform::from_xyz(1., 2., 3.)).id();
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), SpringArm::new(target)))
            .id();
        // Bevy's first time update has zero delta; physics assertions start next tick.
        app.update();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(DT));
        Self {
            app,
            target,
            camera,
        }
    }

    fn blocker(&mut self, position: Vec3, size: Vec3) -> Entity {
        self.app
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
                CollisionLayers::new(CollisionLayer::Default, LayerMask::ALL),
                Transform::from_translation(position),
            ))
            .id()
    }

    fn assert_pose(&self, expected: Vec3) {
        let world = self.app.world();
        let local = world.get::<Transform>(self.camera).unwrap();
        let global = world.get::<GlobalTransform>(self.camera).unwrap();
        let (frustum_local, frustum_global) = world.resource::<FrustumPose>().0.unwrap();
        assert!(
            local.translation.abs_diff_eq(expected, TOLERANCE),
            "expected {expected:?}, got {:?}",
            local.translation
        );
        assert!(global.to_matrix().abs_diff_eq(local.to_matrix(), TOLERANCE));
        assert_eq!(frustum_local, *local);
        assert!(
            frustum_global
                .to_matrix()
                .abs_diff_eq(global.to_matrix(), TOLERANCE)
        );
    }
}

#[test]
fn filtering_skips_target_body_sensors_player_membership_and_camera_but_keeps_nearest_wall() {
    let mut f = Fixture::new();
    let arm = f.app.world().get::<SpringArm>(f.camera).unwrap();
    let (length, radius) = (arm.length, arm.probe_radius);
    let pivot = f
        .app
        .world()
        .get::<GlobalTransform>(f.target)
        .unwrap()
        .translation();
    f.app
        .world_mut()
        .entity_mut(f.target)
        .insert((RigidBody::Static, Collider::sphere(radius)));
    let child = f
        .app
        .world_mut()
        .spawn((
            ChildOf(f.target),
            Collider::sphere(radius),
            Transform::default(),
        ))
        .id();
    f.app.world_mut().spawn((
        ChildOf(f.target),
        Collider::sphere(radius),
        Transform::from_xyz(0., 0., length * 0.2),
    ));
    let size = Vec3::splat(radius);
    let sensor = f.blocker(pivot + Vec3::Z * (length * 0.3), size);
    f.app.world_mut().entity_mut(sensor).insert(Sensor);
    let mixed = f.blocker(pivot + Vec3::Z * (length * 0.4), size);
    f.app
        .world_mut()
        .entity_mut(mixed)
        .insert(CollisionLayers::new(
            CollisionLayer::Default | CollisionLayer::Player,
            LayerMask::ALL,
        ));
    let non_default = f.blocker(pivot + Vec3::Z * (length * 0.5), size);
    f.app
        .world_mut()
        .entity_mut(non_default)
        .insert(CollisionLayers::new(CollisionLayer::Camera, LayerMask::ALL));
    f.app.world_mut().entity_mut(f.camera).insert((
        RigidBody::Static,
        Collider::sphere(radius),
        Transform::from_translation(pivot + Vec3::Z * (length * 0.6)),
    ));
    // Spawn the farther blocker first so selecting an arbitrary hit cannot pass.
    f.blocker(pivot + Vec3::Z * (length * 0.95), size);
    let wall_center = length * 0.8;
    f.blocker(pivot + Vec3::Z * wall_center, size);
    for target in [f.target, child] {
        f.app
            .world_mut()
            .get_mut::<SpringArm>(f.camera)
            .unwrap()
            .target = target;
        f.app.update();
        f.assert_pose(pivot + Vec3::Z * (wall_center - size.z / 2. - radius - CLEARANCE));
    }
}

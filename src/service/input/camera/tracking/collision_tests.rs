use std::time::Duration;

use crate::prelude::*;
use bevy::{camera::visibility::VisibilitySystems, time::TimeUpdateStrategy};

const DT: Duration = Duration::from_millis(16);
const CLEARANCE: f32 = 0.02;
const TOLERANCE: f32 = 1e-3;

#[derive(Component)]
struct MoveInFixed(Vec3);

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
            .add_plugins((PhysicsPlugins::default(), super::systems::plugin))
            .insert_resource(Gravity(Vec3::ZERO))
            .insert_resource(Time::<Fixed>::from_duration(DT))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .init_resource::<FrustumPose>()
            .add_systems(
                FixedUpdate,
                |mut movers: Query<(&MoveInFixed, &mut Transform)>| {
                    for (destination, mut transform) in &mut movers {
                        transform.translation = destination.0;
                    }
                },
            )
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
            .spawn((
                Camera3d::default(),
                SpringArm {
                    target_offset: Vec3::new(0.25, 0.75, -0.5),
                    ..SpringArm::new(target)
                },
            ))
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

    fn arm(&self) -> &SpringArm {
        self.app.world().get::<SpringArm>(self.camera).unwrap()
    }

    fn pivot(&self) -> Vec3 {
        self.app
            .world()
            .get::<GlobalTransform>(self.arm().target)
            .unwrap()
            .translation()
            + self.arm().target_offset
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

    fn move_in_fixed(&mut self, entity: Entity, position: Vec3) {
        self.app
            .world_mut()
            .entity_mut(entity)
            .insert(MoveInFixed(position));
    }

    fn tick(&mut self) {
        self.app.update();
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
fn fixed_step_obstruction_and_recovery_reach_frusta_in_the_same_frame() {
    let mut f = Fixture::new();
    let (length, radius) = (f.arm().length, f.arm().probe_radius);
    let pivot = f.pivot();
    let size = Vec3::new(4. * radius, 4. * radius, radius);
    let away = pivot + Vec3::X * length;
    let wall = f.blocker(away, size);
    f.tick();
    f.assert_pose(pivot + Vec3::Z * length);

    let rotation = f.app.world().get::<Transform>(f.camera).unwrap().rotation;
    let target_shift = Vec3::new(2., -1., 3.);
    let target_position = f
        .app
        .world()
        .get::<Transform>(f.target)
        .unwrap()
        .translation;
    f.move_in_fixed(f.target, target_position + target_shift);
    let pivot = pivot + target_shift;
    let wall_center = length * 0.6;
    f.move_in_fixed(wall, pivot + Vec3::Z * wall_center);
    f.tick();
    f.assert_pose(pivot + Vec3::Z * (wall_center - size.z / 2. - radius - CLEARANCE));
    assert_eq!(
        f.app.world().get::<Transform>(f.camera).unwrap().rotation,
        rotation
    );

    f.move_in_fixed(wall, away);
    f.tick();
    f.assert_pose(pivot + Vec3::Z * length);
}

#[test]
fn sweep_reaches_hits_beyond_length_minus_radius() {
    let mut f = Fixture::new();
    let (length, radius) = (f.arm().length, f.arm().probe_radius);
    let pivot = f.pivot();
    let face = length + radius / 2.;
    f.blocker(
        pivot + Vec3::Z * (face + radius / 2.),
        Vec3::new(4. * radius, 4. * radius, radius),
    );
    f.tick();
    let distance = face - radius - CLEARANCE;
    assert!(distance > length - radius && distance < length);
    f.assert_pose(pivot + Vec3::Z * distance);
}

#[test]
fn sphere_probe_catches_a_corner_off_the_centerline() {
    let mut f = Fixture::new();
    let (length, radius) = (f.arm().length, f.arm().probe_radius);
    let pivot = f.pivot();
    let half_size = radius * 0.1;
    let corner_offset = radius * 0.6;
    let face = length / 2. - half_size;
    f.blocker(
        pivot
            + Vec3::new(
                corner_offset + half_size,
                corner_offset + half_size,
                length / 2.,
            ),
        Vec3::splat(2. * half_size),
    );
    f.tick();
    // The center ray misses both side faces, but the sphere reaches their near corner.
    let reach = ((radius + CLEARANCE).powi(2) - 2. * corner_offset.powi(2)).sqrt();
    f.assert_pose(pivot + Vec3::Z * (face - reach));
}

#[test]
fn initial_overlap_and_tiny_arms_collapse_to_the_pivot() {
    let mut f = Fixture::new();
    let (length, radius) = (f.arm().length, f.arm().probe_radius);
    let pivot = f.pivot();
    f.blocker(pivot, Vec3::splat(4. * radius));
    for length in [length, 0., f32::EPSILON] {
        f.app
            .world_mut()
            .get_mut::<SpringArm>(f.camera)
            .unwrap()
            .length = length;
        f.tick();
        f.assert_pose(pivot);
    }
}

#[test]
fn filtering_skips_target_body_sensors_player_membership_and_camera_but_keeps_nearest_wall() {
    let mut f = Fixture::new();
    let (length, radius) = (f.arm().length, f.arm().probe_radius);
    // Put the pivot at the compound body's origin for both body and collider targets.
    f.app
        .world_mut()
        .get_mut::<SpringArm>(f.camera)
        .unwrap()
        .target_offset = Vec3::ZERO;
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
    let player = f.blocker(pivot + Vec3::Z * (length * 0.4), size);
    f.app
        .world_mut()
        .entity_mut(player)
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
        f.tick();
        f.assert_pose(pivot + Vec3::Z * (wall_center - size.z / 2. - radius - CLEARANCE));
    }
}

#[test]
fn invalid_probe_geometry_leaves_the_previous_pose_unchanged() {
    let mut f = Fixture::new();
    let (length, radius) = (f.arm().length, f.arm().probe_radius);
    f.tick();
    let original = *f.app.world().get::<Transform>(f.camera).unwrap();
    let target_position = f
        .app
        .world()
        .get::<Transform>(f.target)
        .unwrap()
        .translation;
    f.move_in_fixed(f.target, target_position + Vec3::ONE);
    for (length, radius) in [
        (-1., radius),
        (f32::NAN, radius),
        (f32::INFINITY, radius),
        (f32::NEG_INFINITY, radius),
        (length, 0.),
        (length, -radius),
        (length, f32::NAN),
        (length, f32::INFINITY),
        (length, f32::NEG_INFINITY),
    ] {
        {
            let mut arm = f.app.world_mut().get_mut::<SpringArm>(f.camera).unwrap();
            arm.length = length;
            arm.probe_radius = radius;
        }
        f.tick();
        f.assert_pose(original.translation);
        assert_eq!(*f.app.world().get::<Transform>(f.camera).unwrap(), original);
    }
}

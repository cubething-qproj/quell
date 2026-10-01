use std::{f32::consts::FRAC_PI_2, time::Duration};

use bevy::time::TimeUpdateStrategy;
use q_test_harness::prelude::InputTestPlugin;
use quell::{prelude::*, service::player};

const DT: Duration = Duration::from_millis(16);
/// Height gain that counts as having climbed.
const CLIMB: f32 = 0.1;
const FOOT: f32 = 2.;

fn steep_angle() -> f32 {
    (PlayerSettings::default().max_slope + FRAC_PI_2) / 2.
}

struct Fixture {
    app: App,
    player: Entity,
}

impl Fixture {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputTestPlugin,
            TransformPlugin,
            EnhancedInputPlugin,
        ))
        .init_resource::<Assets<Mesh>>()
        .add_message::<AssetEvent<Mesh>>()
        .init_resource::<PlayerAssets>()
        .add_plugins((PhysicsPlugins::default(), player::plugin))
        .insert_resource(Time::<Fixed>::from_duration(DT))
        .insert_resource(TimeUpdateStrategy::ManualDuration(DT));
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        world.trigger(SpawnPlayerRoot::default());
        world.flush();
        let player = world
            .query_filtered::<Entity, With<PlayerMotor>>()
            .single(world)
            .unwrap();
        world.spawn((
            RigidBody::Static,
            Collider::half_space(Vec3::Y),
            Transform::default(),
        ));
        Self { app, player }
    }

    /// Spawns a static wall whose near face is the plane `x = face`.
    fn wall(&mut self, face: f32) {
        self.app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(1., 10., 10.),
            Transform::from_xyz(face + 0.5, 5., 0.),
        ));
    }

    /// Spawns a static ramp rising along +X at `angle` whose surface meets the
    /// ground at `x = foot`.
    fn ramp(&mut self, foot: f32, angle: f32) {
        let (length, thickness) = (20., 1.);
        let rotation = Quat::from_rotation_z(angle);
        let surface_center = Vec3::X * foot + rotation * Vec3::X * (length / 2.);
        self.app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(length, thickness, 10.),
            Transform::from_translation(surface_center - rotation * Vec3::Y * (thickness / 2.))
                .with_rotation(rotation),
        ));
    }

    fn walk(&mut self, velocity: Vec3) {
        self.app
            .world_mut()
            .get_mut::<PlayerMotor>(self.player)
            .unwrap()
            .desired_velocity = velocity;
    }

    fn update(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    /// Highest height reached over `frames` updates.
    fn max_height(&mut self, frames: usize) -> f32 {
        (0..frames)
            .map(|_| {
                self.app.update();
                self.position().y
            })
            .fold(f32::MIN, f32::max)
    }

    fn position(&self) -> Vec3 {
        self.app.world().get::<Position>(self.player).unwrap().0
    }

    fn place(&mut self, position: Vec3) {
        self.app
            .world_mut()
            .entity_mut(self.player)
            .insert(Transform::from_translation(position));
    }

    fn settings(&self) -> &PlayerSettings {
        self.app.world().resource::<PlayerSettings>()
    }

    fn assert_resting(&self) {
        let resting_height = self.settings().resting_height();
        let height = self.position().y;
        assert!((height - resting_height).abs() < 0.05, "height: {height}");
    }
}

#[test]
fn walks_along_the_ground_until_a_wall_stops_it() {
    let mut f = Fixture::new();
    let face = 5.;
    f.wall(face);
    f.update(10);
    f.assert_resting();
    let start = f.position();

    f.walk(Vec3::X * 5.);
    f.update(30);
    assert!(f.position().x > start.x + 1., "x: {}", f.position().x);
    f.assert_resting();

    f.update(120);
    let stop = face - f.settings().capsule_radius;
    let x = f.position().x;
    assert!(x <= stop && x > stop - 0.1, "x: {x}, stop: {stop}");
    f.assert_resting();
}

#[test]
fn climbs_walkable_slopes_but_not_steep_ones() {
    let climb = |angle: f32| {
        let mut f = Fixture::new();
        f.ramp(FOOT, angle);
        f.update(10);
        f.walk(Vec3::X * 5.);
        let gain = f.max_height(120) - f.settings().resting_height();
        (gain, f.position().x)
    };
    let (gentle, _) = climb(PlayerSettings::default().max_slope / 2.);
    assert!(gentle > CLIMB, "gentle climb: {gentle}");
    let (steep, x) = climb(steep_angle());
    assert!(steep < CLIMB, "steep climb: {steep}");
    let reach = FOOT - PlayerSettings::default().capsule_radius;
    assert!(x > reach, "never reached the steep ramp: x = {x}");
}

#[test]
fn slides_down_steep_slopes_to_the_ground() {
    let mut f = Fixture::new();
    let angle = steep_angle();
    f.ramp(FOOT, angle);
    let x = FOOT + 1.;
    let surface = (x - FOOT) * angle.tan();
    f.place(Vec3::new(
        x,
        surface + f.settings().resting_height() + 1.,
        0.,
    ));
    // Even without the slope rule, depenetration ratchets the player down slowly;
    // this budget requires the rule's faster slide.
    f.update(300);
    f.assert_resting();
    assert!(f.position().x < FOOT, "x: {}", f.position().x);
}

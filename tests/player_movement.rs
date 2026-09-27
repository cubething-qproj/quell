#![feature(register_tool)]
#![register_tool(bevy)]
#![allow(bevy::panicking_methods)]

use std::time::Duration;

use bevy::time::TimeUpdateStrategy;
use q_test_harness::prelude::InputTestPlugin;
use quell::{prelude::*, service::player};

const DT: Duration = Duration::from_millis(16);

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
        world.trigger(SpawnPlayerRoot);
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

    fn position(&self) -> Vec3 {
        self.app.world().get::<Position>(self.player).unwrap().0
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

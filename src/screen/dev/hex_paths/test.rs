use super::{data::*, systems};
use crate::prelude::*;
use bevy::{
    ecs::schedule::ScheduleLabel, platform::collections::HashMap, transform::TransformPlugin,
};
use hexx::{EdgeDirection, Hex};
use q_test_harness::prelude::{AppExt as _, InputTestPlugin};
use std::time::Duration;

/// Exercises board replacement and transform propagation without rendering or UI.
struct Fixture {
    app: App,
    panel: Entity,
}

impl Fixture {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputTestPlugin, TransformPlugin))
            .insert_resource(HexPathsSettings {
                seed: Some(7),
                ..default()
            })
            .init_resource::<HexPathsView>()
            .init_resource::<PanelState>()
            .add_message::<WalkTo>()
            .add_systems(
                Update,
                (
                    systems::spawn_board.run_if(resource_changed::<HexPathsSettings>),
                    systems::plan_walk,
                    systems::walk,
                )
                    .chain(),
            );
        app.finish();
        app.cleanup();
        let panel = app.world_mut().spawn(SettingsPanel).id();
        let mut fixture = Self { app, panel };
        fixture.tick();
        fixture
    }

    fn tick(&mut self) {
        self.app
            .step(Duration::ZERO, [Update.intern(), PostUpdate.intern()]);
    }

    /// Requests a walk to `to`, then advances exactly one step duration.
    fn walk_to(&mut self, to: Hex) {
        self.app.world_mut().write_message(WalkTo(to));
        let step = self.app.world().resource::<HexPathsView>().step_duration;
        self.app.step(
            Duration::from_secs_f32(step),
            [Update.intern(), PostUpdate.intern()],
        );
    }

    fn player(&mut self) -> Hex {
        let world = self.app.world_mut();
        world
            .query_filtered::<&HexPos, With<HexPathsPlayer>>()
            .single(world)
            .unwrap()
            .0
    }

    fn revealed(&self, hex: Hex) -> bool {
        let world = self.app.world();
        world
            .get::<Revealed>(world.resource::<Board>().tiles[&hex])
            .is_some()
    }

    fn layout(&self) -> HashMap<Hex, (TileKind, Spokes)> {
        let world = self.app.world();
        world
            .resource::<Board>()
            .tiles
            .iter()
            .map(|(&hex, &entity)| {
                (
                    hex,
                    (
                        *world.get::<TileKind>(entity).unwrap(),
                        *world.get::<Spokes>(entity).unwrap(),
                    ),
                )
            })
            .collect()
    }

    fn node(&self) -> Entity {
        let world = self.app.world();
        *world
            .resource::<Board>()
            .tiles
            .values()
            .find(|&&entity| matches!(world.get::<TileKind>(entity), Some(TileKind::Node(_))))
            .unwrap()
    }

    fn assert_board_present(&mut self) {
        let world = self.app.world_mut();
        assert!(world.contains_resource::<Board>());
        assert!(world.resource::<BoardStats>().error.is_none());
        assert_eq!(
            world
                .query_filtered::<Entity, With<BoardRoot>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<HexPathsPlayer>>()
                .iter(world)
                .count(),
            1
        );
        assert!(world.get::<SettingsPanel>(self.panel).is_some());
    }

    fn assert_player_transform_propagated(&mut self) {
        let world = self.app.world_mut();
        let (transform, global, parent) = world
            .query_filtered::<(&Transform, &GlobalTransform, &ChildOf), With<HexPathsPlayer>>()
            .single(world)
            .unwrap();
        assert_eq!(global.translation(), transform.translation);
        assert!(world.get::<GlobalTransform>(parent.parent()).is_some());
    }
}

#[test]
fn view_edits_preserve_reveals_and_regeneration_resets_them() {
    let mut fixture = Fixture::new();
    let layout = fixture.layout();
    let node = fixture.node();
    fixture.app.world_mut().entity_mut(node).insert(Revealed);
    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsView>()
        .show_paths = true;
    fixture.tick();
    assert!(fixture.app.world().get::<Revealed>(node).is_some());
    assert_eq!(fixture.layout(), layout);

    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsSettings>()
        .set_changed();
    fixture.tick();
    assert!(fixture.app.world().get_entity(node).is_err());
    assert_eq!(fixture.layout(), layout);
    let world = fixture.app.world_mut();
    assert!(
        world
            .query_filtered::<&TileKind, With<Revealed>>()
            .iter(world)
            .all(|kind| *kind == TileKind::Spawn)
    );
    fixture.assert_board_present();
}

#[test]
fn invalid_settings_clear_the_board_and_valid_settings_restore_it() {
    let mut fixture = Fixture::new();
    let node = fixture.node();
    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsSettings>()
        .columns = 0;
    fixture.tick();
    let world = fixture.app.world_mut();
    assert!(!world.contains_resource::<Board>());
    assert!(world.resource::<BoardStats>().error.is_some());
    assert!(world.get_entity(node).is_err());
    assert_eq!(
        world
            .query_filtered::<Entity, With<HexPathsPlayer>>()
            .iter(world)
            .count(),
        0
    );

    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsSettings>()
        .columns = 10;
    fixture.tick();
    fixture.assert_board_present();
}

#[test]
fn player_transform_propagates_on_entry_and_regeneration() {
    let mut fixture = Fixture::new();
    fixture.assert_player_transform_propagated();
    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsSettings>()
        .seed = Some(11);
    fixture.tick();
    fixture.assert_player_transform_propagated();
}

#[test]
fn leaving_the_screen_restores_the_ui_cameras_clear_color() {
    let mut fixture = Fixture::new();
    let color = Color::srgb(0.2, 0.3, 0.4);
    let clear_color = ClearColorConfig::Custom(color);
    let camera = fixture
        .app
        .world_mut()
        .spawn(Camera {
            clear_color: ClearColorConfig::None,
            ..default()
        })
        .id();
    let world = fixture.app.world_mut();
    world.insert_resource(UiCameraState {
        entity: camera,
        clear_color,
    });
    world.run_system_cached(systems::unload).unwrap();
    let ClearColorConfig::Custom(restored) = world.get::<Camera>(camera).unwrap().clear_color
    else {
        panic!("the previous custom clear color should be restored");
    };
    assert_eq!(restored, color);
    assert!(!world.contains_resource::<UiCameraState>());
    assert!(!world.contains_resource::<Board>());
    assert!(!world.contains_resource::<BoardStats>());
}

#[test]
fn walking_reveals_a_tile_on_arrival_and_ignores_unreachable_targets() {
    let mut fixture = Fixture::new();
    let layout = fixture.layout();
    let spawn = fixture.player();

    // Nodes start at least two steps away, behind unrevealed tiles.
    let (&node, _) = layout
        .iter()
        .find(|(_, (kind, _))| matches!(kind, TileKind::Node(_)))
        .unwrap();
    fixture.walk_to(node);
    assert_eq!(fixture.player(), spawn);

    // A tile across one of spawn's crossings is one step into the unknown.
    let next = EdgeDirection::ALL_DIRECTIONS
        .into_iter()
        .find_map(|dir| {
            let (_, spokes) = layout.get(&spawn.neighbor(dir))?;
            layout[&spawn]
                .1
                .crosses(*spokes, dir)
                .then_some(spawn.neighbor(dir))
        })
        .unwrap();
    assert!(!fixture.revealed(next));
    fixture.walk_to(next);
    assert_eq!(fixture.player(), next);
    assert!(fixture.revealed(next));
}

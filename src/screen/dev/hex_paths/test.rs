//! Selected-schedule checks for board replacement and explicit unload behaviour.
//!
//! [`Fixture`] runs [`spawn_board`](systems::spawn_board) on small seeded worlds without
//! the rendered screen or panel. It steps Update and PostUpdate explicitly;
//! unload is called directly. These tests do not exercise the full screen
//! lifecycle, picking, rendered camera behaviour, or asset readiness.

use super::{data::*, screen, systems};
use crate::prelude::*;
use bevy::ecs::schedule::ScheduleLabel;
use q_test_harness::prelude::{AppExt as _, InputTestPlugin};
use std::time::Duration;

/// A small headless app for board replacement and unload-resource assertions.
///
/// Owns ordinary settings, tuning, view, and cache resources so the generation
/// system runs with its normal parameters. Selected schedule stepping uses
/// [`InputTestPlugin`] and the test harness's `AppExt` helpers.
struct Fixture {
    app: App,
}

impl Fixture {
    /// Creates and steps a minimal app to generate its first small seeded board.
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputTestPlugin))
            .insert_resource(HexPathsSettings {
                seed: Some(7),
                columns: 12,
                rows: 12,
                ..default()
            })
            // A small region, quick enough for debug builds.
            .insert_resource(HexTerrainTuning {
                region_hexes: 50,
                iterations: 120,
                candidates: 8,
                margin: 4,
                ..default()
            })
            .init_resource::<PanelState>()
            .init_resource::<RegionCache>()
            .init_resource::<HexPathsView>()
            .add_systems(
                Update,
                systems::spawn_board.run_if(resource_changed::<HexPathsSettings>),
            );
        app.finish();
        app.cleanup();
        let mut fixture = Self { app };
        fixture.tick();
        fixture
    }

    /// Runs only Update and PostUpdate without advancing time or simulating a full app frame.
    fn tick(&mut self) {
        self.app
            .step(Duration::ZERO, [Update.intern(), PostUpdate.intern()]);
    }

    /// Checks that successful generation installed one root, a Board, and no generation error.
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
    }
}

#[test]
fn invalid_settings_clear_the_board_and_valid_settings_restore_it() {
    let mut fixture = Fixture::new();
    fixture.assert_board_present();
    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsSettings>()
        .columns = 0;
    fixture.tick();
    let world = fixture.app.world_mut();
    assert!(!world.contains_resource::<Board>());
    assert!(world.resource::<BoardStats>().error.is_some());
    assert_eq!(
        world
            .query_filtered::<Entity, With<BoardRoot>>()
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
    world.run_system_cached(screen::unload).unwrap();
    let ClearColorConfig::Custom(restored) = world.get::<Camera>(camera).unwrap().clear_color
    else {
        panic!("the previous custom clear color should be restored");
    };
    assert_eq!(restored, color);
    assert!(!world.contains_resource::<UiCameraState>());
    assert!(!world.contains_resource::<Board>());
    assert!(!world.contains_resource::<BoardStats>());
}

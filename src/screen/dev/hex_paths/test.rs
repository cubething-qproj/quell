//! Selected-schedule checks for board replacement and explicit unload behaviour.
//!
//! [`Fixture`] runs [`spawn_board`](systems::spawn_board) on small seeded worlds without
//! a renderer or panel, but includes surface/marker observers and reveal
//! updates. Pointer events are injected directly. It steps Update and PostUpdate;
//! unload is called directly. These tests do not exercise the full screen
//! lifecycle, ray casting, UI occlusion, rendered camera behaviour, or asset readiness.

use super::{data::*, screen, systems};
use bevy::{
    camera::NormalizedRenderTarget,
    ecs::schedule::ScheduleLabel,
    picking::{
        backend::HitData,
        events::Press,
        pointer::{Location, PointerId},
    },
    prelude::*,
};
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
            .init_resource::<HexPathsAppearance>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_observer(systems::spawn_surface)
            .add_observer(systems::spawn_markers)
            .add_systems(
                Update,
                (
                    systems::spawn_board.run_if(resource_changed::<HexPathsSettings>),
                    systems::refresh_preview.run_if(
                        resource_exists_and_changed::<Board>
                            .or_else(resource_changed::<HexPathsView>)
                            .or_else(resource_changed::<HexPathsAppearance>),
                    ),
                    systems::paint_markers.run_if(resource_changed::<HexPathsView>),
                )
                    .chain(),
            );
        let quad = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(1., 2.));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let mut material = || materials.add(StandardMaterial::default());
        let billboards = Billboards {
            quad,
            spawn: material(),
            player: material(),
            bench: material(),
            empty: material(),
            key: material(),
            npc: material(),
            fear: material(),
            unknown: material(),
        };
        app.insert_resource(billboards);
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

    /// Returns a displayed POI entity, its tile, and its current material.
    fn poi(&mut self) -> (Entity, usize, Handle<StandardMaterial>) {
        let world = self.app.world_mut();
        world
            .query::<(Entity, &Marker, &Pad, &MeshMaterial3d<StandardMaterial>)>()
            .iter(world)
            .find_map(|(entity, marker, pad, material)| {
                matches!(marker, Marker::Poi(_)).then_some((entity, pad.0, material.0.clone()))
            })
            .expect("the board has a displayed POI")
    }

    /// Checks that every marker already has its image for the current reveal state.
    fn assert_marker_images(&mut self) {
        let world = self.app.world_mut();
        let markers: Vec<_> = world
            .query::<(&Marker, &MeshMaterial3d<StandardMaterial>)>()
            .iter(world)
            .map(|(marker, material)| (*marker, material.0.clone()))
            .collect();
        let view = world.resource::<HexPathsView>();
        let billboards = world.resource::<Billboards>();
        for (marker, material) in markers {
            assert_eq!(material, marker.material(view, billboards));
        }
    }

    /// Injects the event delivered after picking, without exercising the ray-casting backend.
    fn press_marker(&mut self, marker: Entity, camera: Entity, button: PointerButton) {
        self.app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::None {
                    width: 800,
                    height: 600,
                },
                position: Vec2::ZERO,
            },
            Press {
                button,
                count: 1,
                hit: HitData::new(camera, 1., None, None),
            },
            marker,
        ));
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

#[test]
fn reveal_and_layout_rebuilds_keep_marker_images_current() {
    let mut fixture = Fixture::new();
    fixture.assert_marker_images();
    let (marker, _, image) = fixture.poi();
    assert_eq!(image, fixture.app.world().resource::<Billboards>().unknown);
    fixture
        .app
        .world_mut()
        .resource_mut::<HexPathsView>()
        .reveal_nodes = true;
    fixture.tick();
    fixture.assert_marker_images();
    assert!(
        fixture.app.world().get_entity(marker).is_ok(),
        "reveal retains marker entities"
    );
    assert_ne!(fixture.poi().2, image);

    fixture.app.world_mut().resource_mut::<Board>().regenerated = true;
    fixture.tick();
    assert!(
        fixture.app.world().get_entity(marker).is_err(),
        "a new layout replaces old markers"
    );
    fixture.assert_marker_images();
    let world = fixture.app.world_mut();
    assert!(
        world
            .query::<&Marker>()
            .iter(world)
            .any(|marker| *marker == Marker::Player)
    );
    assert_eq!(
        world
            .query_filtered::<Entity, With<TerrainSurface>>()
            .iter(world)
            .count(),
        1
    );
}

#[test]
fn marker_pointer_presses_only_focus_from_the_board_camera() {
    let mut fixture = Fixture::new();
    let camera = fixture.app.world_mut().spawn(HexPathsCamera).id();
    let other_camera = fixture.app.world_mut().spawn_empty().id();
    let (marker, tile, _) = fixture.poi();
    fixture.press_marker(marker, camera, PointerButton::Secondary);
    fixture.press_marker(marker, other_camera, PointerButton::Primary);
    assert_eq!(
        fixture.app.world().resource::<HexPathsView>().focus,
        CameraFocus::Terrain
    );
    fixture.press_marker(marker, camera, PointerButton::Primary);
    assert_eq!(
        fixture.app.world().resource::<HexPathsView>().focus,
        CameraFocus::Node(tile)
    );

    let world = fixture.app.world_mut();
    let spawn = world
        .query::<(Entity, &Marker)>()
        .iter(world)
        .find_map(|(entity, marker)| (*marker == Marker::Spawn).then_some(entity))
        .unwrap();
    fixture.press_marker(spawn, camera, PointerButton::Primary);
    assert_eq!(
        fixture.app.world().resource::<HexPathsView>().focus,
        CameraFocus::Player
    );
}

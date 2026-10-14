//! Lifecycle and update ordering for [`HexPathsScreen`].
//!
//! Loading creates the camera, billboard assets, and panel and saves the UI
//! camera's clear colour. Readiness marks settings changed; Update is the sole
//! full-generation path. Chained systems generate [`Board`] before replacing
//! its surface and markers, then handle view, focus, camera, and overlays.
//!
//! Unload restores the UI camera and removes play-time resources, including
//! the board and billboard handles; it clears the cached region and panel
//! errors. Screen-scoped entity cleanup removes the preview hierarchy.
//! Settings, tuning, appearance, and view resources remain registered.

use super::{data::*, panel, systems};
use crate::prelude::*;
use bevy::feathers::{dark_theme::create_dark_theme, theme::UiTheme};
use bevy_editor_cam::prelude::{EditorCam, EnabledMotion};

/// The dev [`Screen`] combining generation, inspection controls, and a 3D preview.
///
/// The dev plugin registers it to own loading, cleanup, and the update order
/// of the `systems` and `panel` modules. The internal `Board` resource stores
/// generated terrain and layout; the camera and UI are scoped entities,
/// separate from the pure generator.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct HexPathsScreen;

impl Screen for HexPathsScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, init);
        builder.add_systems(ScreenSchedule::OnReady, request_board);
        builder.add_systems(
            ScreenSchedule::Update,
            (
                systems::spawn_board.run_if(
                    resource_changed::<HexPathsSettings>.or_else(resource_changed::<HexTerrainTuning>),
                ),
                systems::spawn_surface.run_if(
                    resource_exists_and_changed::<Board>
                        .or_else(resource_changed::<HexPathsView>)
                        .or_else(resource_changed::<HexPathsAppearance>),
                ),
                systems::spawn_markers.run_if(
                    resource_exists_and_changed::<Board>
                        .or_else(resource_exists::<Board>.and_then(resource_changed::<HexPathsAppearance>)),
                ),
                systems::paint_markers,
                systems::fit_viewport,
                systems::pick_focus,
                systems::aim_camera,
                systems::orbit_camera,
                systems::face_camera,
                systems::draw_board,
                panel::sync_settings,
                panel::update_stats,
                panel::resize_panel,
            )
                .chain(),
        );
        builder.add_systems(ScreenSchedule::OnUnload, unload);
        builder
    }

    fn name() -> String {
        "hex_paths".into()
    }
}

/// Creates preview assets and UI, then finishes the screen's loading phase.
///
/// Saves the default UI camera's clear colour in [`UiCameraState`] and disables
/// its clear so the board camera remains visible beneath it. Billboard asset
/// handles are requested here; this does not wait for image loading. Full
/// board generation is requested later by [`request_board`].
fn init(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    appearance: Res<HexPathsAppearance>,
    view: Res<HexPathsView>,
    mut ui_camera: Single<(Entity, &mut Camera), With<IsDefaultUiCamera>>,
    mut screen: ScreenInfoMut<HexPathsScreen>,
) {
    commands.insert_resource(UiTheme(create_dark_theme()));
    commands.insert_resource(UiCameraState {
        entity: ui_camera.0,
        clear_color: ui_camera.1.clear_color,
    });
    ui_camera.1.clear_color = ClearColorConfig::None;
    commands.spawn((
        Name::new("Hex Board Camera"),
        ScreenScoped,
        HexPathsCamera,
        Camera3d::default(),
        // Panning will follow the player or terrain instead.
        EditorCam {
            enabled_motion: EnabledMotion {
                pan: false,
                orbit: true,
                zoom: true,
            },
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-view.initial_tilt.to_radians())),
        Camera {
            order: CameraOrder::World as isize,
            ..default()
        },
        RenderLayers::layer(31),
    ));
    let height = appearance.billboard_height;
    let mut billboard = |name: &str| {
        materials.add(StandardMaterial {
            base_color_texture: Some(server.load(format!(
                "textures/hex-prototype/{name}-billboard.png"
            ))),
            alpha_mode: AlphaMode::Mask(0.5),
            unlit: true,
            cull_mode: None,
            ..default()
        })
    };
    commands.insert_resource(Billboards {
        quad: meshes.add(
            Mesh::from(Rectangle::new(height / 2., height)).translated_by(Vec3::Y * height / 2.),
        ),
        spawn: billboard("spawn"),
        player: billboard("player"),
        bench: billboard("bench"),
        empty: billboard("empty"),
        key: billboard("key"),
        npc: billboard("npc"),
        fear: billboard("fear"),
        unknown: billboard("unknown"),
    });
    commands.queue_spawn_scene(panel::scene(appearance.panel_width));
    screen.finish_loading();
}

/// Requests a board on each screen entry; Update is the sole generation path.
fn request_board(mut settings: ResMut<HexPathsSettings>) {
    settings.set_changed();
}

/// Restores UI compositing state and removes resources belonging to this screen entry.
///
/// Removes [`Board`], [`BoardStats`], [`Billboards`], and [`UiCameraState`], then
/// clears [`RegionCache`] and [`PanelState`]. Screen cleanup despawns scoped
/// entities separately; persistent settings and view resources are retained.
pub(super) fn unload(
    mut commands: Commands,
    mut panel: ResMut<PanelState>,
    mut cache: ResMut<RegionCache>,
    ui_state: Option<Res<UiCameraState>>,
    mut cameras: Query<&mut Camera>,
) {
    if let Some(state) = ui_state
        && let Ok(mut camera) = cameras.get_mut(state.entity)
    {
        camera.clear_color = state.clear_color;
    }
    commands.remove_resource::<UiCameraState>();
    commands.remove_resource::<Board>();
    commands.remove_resource::<BoardStats>();
    commands.remove_resource::<Billboards>();
    cache.0 = None;
    *panel = PanelState::default();
}

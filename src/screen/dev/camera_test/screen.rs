use crate::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Reflect)]
pub struct CameraTestScreen;
impl Screen for CameraTestScreen {
    fn builder(mut builder: ScreenScopeBuilder<Self>) -> ScreenScopeBuilder<Self> {
        builder.add_systems(ScreenSchedule::Loading, init);
        builder.add_systems(ScreenSchedule::Update, update);
        builder
    }
    fn name() -> String {
        "camera_test".into()
    }
}

/// spawn the scene.
/// this is temp, ideally load the scene from file
/// then spawn it
fn init(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut screen: ScreenInfoMut<CameraTestScreen>,
) {
    // spawn everything
    let cube = meshes.add(Cuboid::default());
    let plane = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(100.)));
    let wall = meshes.add(Cuboid::new(100., 100., 1.));
    let material = materials.add(StandardMaterial::default());

    commands.spawn((
        Cube,
        Transform::default(),
        Mesh3d(cube),
        MeshMaterial3d(material.clone()),
        Collider::cuboid(0., 0., 0.),
    ));
    commands.spawn((
        Transform::default(),
        Mesh3d(plane),
        MeshMaterial3d(material.clone()),
        Collider::half_space(Vec3::Y),
    ));
    commands.spawn((
        Transform::from_xyz(0., 0., -10.),
        Mesh3d(wall),
        MeshMaterial3d(material),
        Collider::half_space(Vec3::Z),
    ));
    commands.spawn((PointLight::default(), Transform::from_xyz(0., 3., 0.)));
    commands.trigger(SpawnGlobalCtx);
    commands.trigger(SpawnCursorCapture);
    commands.spawn((flycam_bundle(), ScreenScoped, Name::new("Free Camera")));
    commands.spawn((
        Name::new("Camera Help"),
        ScreenScoped,
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            padding: UiRect::all(px(12)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        BackgroundColor(Color::srgba(0., 0., 0., 0.7)),
        children![
            (
                Text::new("Camera: Free"),
                TextFont::default().with_font_size(20.),
            ),
            (
                Text::new(
                    "RMB hold / M toggle capture\n\
                     Esc release (again to quit)\n\
                     WASD move | Q/E down/up\n\
                     Shift faster | Wheel speed",
                ),
                TextFont::default().with_font_size(16.),
            ),
        ],
    ));

    screen.finish_loading();
}

fn update(mut query: Query<&mut Transform, With<Cube>>, time: Res<Time>) {
    let mut tf = r!(query.single_mut());
    *tf = tf.with_translation(Vec3::new(
        3. * f32::cos(time.elapsed_secs()) - 1.5,
        1.,
        3. * f32::sin(time.elapsed_secs()) - 1.5,
    ));
}

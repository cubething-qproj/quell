use crate::prelude::*;
use bevy::{camera::visibility::VisibilitySystems, transform::TransformSystems};

fn apply(
    targets: Query<&GlobalTransform, Without<SpringArm>>,
    owners: Query<&ColliderOf>,
    solids: Query<&CollisionLayers, Without<Sensor>>,
    // Compatibility with AppSettings::use_physics = false and focused input tests.
    spatial_query: Option<SpatialQuery>,
    mut cameras: Query<
        (Entity, &SpringArm, &mut Transform, &mut GlobalTransform),
        (With<Camera>, Without<ChildOf>, Without<Children>),
    >,
) {
    let filter = SpatialQueryFilter::from_mask(CollisionLayer::Default);
    for (camera, arm, mut transform, mut global_transform) in &mut cameras {
        let Ok(target) = targets.get(arm.target) else {
            continue;
        };
        // Fields are editable through reflection, so reject invalid live configuration too.
        if !arm.length.is_finite()
            || arm.length < 0.
            || !arm.probe_radius.is_finite()
            || arm.probe_radius <= 0.
        {
            warn_once!(
                "SpringArm requires a finite nonnegative length and finite positive probe radius"
            );
            continue;
        }
        let pivot = target.translation() + arm.target_offset;
        let direction = transform.back();
        let mut distance = 0.;
        if arm.length > f32::EPSILON {
            distance = arm.length;
            if let Some(spatial_query) = &spatial_query {
                let body = owners
                    .get(arm.target)
                    .map_or(arm.target, |owner| owner.body);
                let hit = spatial_query.cast_shape_predicate(
                    &Collider::sphere(arm.probe_radius),
                    pivot,
                    Quat::IDENTITY,
                    direction,
                    &ShapeCastConfig::from_max_distance(arm.length).with_target_distance(0.02),
                    &filter,
                    &|entity| {
                        entity != camera
                            && entity != arm.target
                            && owners.get(entity).map_or(entity, |owner| owner.body) != body
                            && solids.get(entity).is_ok_and(|layers| {
                                // A mask alone would still admit mixed Default | Player membership.
                                layers.memberships.0 & CollisionLayer::Player.to_bits() == 0
                            })
                    },
                );
                if let Some(hit) = hit {
                    // Center travel already includes probe clearance. A zero hit collapses
                    // to the pivot; it cannot depenetrate an embedded pivot.
                    distance = hit.distance.clamp(0., arm.length);
                }
            }
        }
        transform.translation = pivot + direction * distance;
        // Target globals are current because this runs after transform propagation.
        // Moving the camera now leaves its global stale. It has no parent or children,
        // so copying its local transform updates world space without another tree pass.
        *global_transform = GlobalTransform::from(*transform);
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        apply
            .after(TransformSystems::Propagate)
            .before(VisibilitySystems::UpdateFrusta),
    );
}

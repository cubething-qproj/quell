use crate::prelude::*;
use bevy::{camera::visibility::VisibilitySystems, transform::TransformSystems};

fn apply(
    targets: Query<&GlobalTransform, Without<SpringArm>>,
    mut cameras: Query<
        (&SpringArm, &mut Transform, &mut GlobalTransform),
        (With<Camera>, Without<ChildOf>, Without<Children>),
    >,
) {
    for (arm, mut transform, mut global_transform) in &mut cameras {
        let Ok(target) = targets.get(arm.target) else {
            continue;
        };
        transform.translation =
            target.translation() + arm.target_offset + transform.back() * arm.length.max(0.);
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

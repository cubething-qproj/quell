use std::f32::consts::{FRAC_PI_2, FRAC_PI_8, PI};

use crate::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

fn input_enabled(
    camera: &Camera,
    activity: &ContextActivity<SpringArm>,
    window: &Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
) -> bool {
    // Recheck ownership when applying events evaluated before a switch or release.
    camera.is_active
        && **activity
        && window.single().is_ok_and(|(window, cursor)| {
            window.focused && cursor.grab_mode == CursorGrabMode::Locked
        })
}

fn on_rotate(
    trigger: On<Fire<PARotateCam>>,
    mut cameras: Query<(&Camera, &ContextActivity<SpringArm>, &mut Transform), With<SpringArm>>,
    window: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((camera, activity, mut transform)) = cameras.get_mut(trigger.context) else {
        return;
    };
    if !input_enabled(camera, activity, &window) {
        return;
    }
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    let yaw = (yaw + trigger.value.x) % (2. * PI);
    let pitch = (pitch + trigger.value.y).clamp(-FRAC_PI_8, FRAC_PI_8);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.);
}

fn on_zoom(
    trigger: On<Fire<PAZoomCam>>,
    mut cameras: Query<(&Camera, &ContextActivity<SpringArm>, &mut Projection), With<SpringArm>>,
    window: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((camera, activity, mut projection)) = cameras.get_mut(trigger.context) else {
        return;
    };
    if !input_enabled(camera, activity, &window) {
        return;
    }
    if let Projection::Perspective(projection) = &mut *projection {
        projection.fov = (projection.fov + trigger.value).clamp(FRAC_PI_8, FRAC_PI_2);
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(on_rotate).add_observer(on_zoom);
}

use crate::prelude::*;
use bevy::window::PrimaryWindow;

fn update_controller(
    mut query: Single<(&mut PlayerMotor, &mut PlayerController)>,
    camera: Single<(&Camera, &Transform), With<SpringArm>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let (motor, controller) = &mut *query;

    if !camera.0.is_active || !window.focused {
        controller.last_move = None;
        motor.desired_velocity = Vec3::ZERO;
        motor.desired_forward = None;
        return;
    }

    let yaw = camera.1.rotation.to_euler(EulerRot::YXZ).0;
    let yaw_quat = Quat::from_axis_angle(Vec3::Y, yaw);
    let moved = controller.last_move.is_some();
    let last_move = controller.last_move.take().unwrap_or_default();
    let desired_velocity = yaw_quat * last_move;
    let desired_forward = moved
        .then_some(Dir3::new(-desired_velocity.normalize()).ok())
        .flatten();

    motor.desired_forward = desired_forward;
    motor.desired_velocity = desired_velocity;
}

pub fn systems() -> ServiceSystems {
    ServiceSystems::new(update_controller)
}

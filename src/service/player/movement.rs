//! Kinematic player movement built on Avian's [`MoveAndSlide`].
//!
//! Movement policy (see quell#13):
//!
//! - **Grounding:** before each move, the player's collider is cast down by
//!   [`PlayerSettings::ground_snap`]. If it hits a surface no steeper than
//!   [`PlayerSettings::max_slope`], the player is grounded: gravity is skipped,
//!   and the player is snapped back onto the ground after moving.
//! - **Steps:** there is no explicit step-up. The capsule's rounded base rides
//!   over small ledges. The ground snap handles step-down.
//! - **Moving surfaces:** not supported. No platform velocity is inherited.
//! - **Dynamic bodies:** not handled. The kinematic player isn't pushed by them.

use crate::prelude::*;

fn move_player(
    mut players: Query<(
        Entity,
        &PlayerMotor,
        &Collider,
        &CollisionLayers,
        &mut Transform,
        &mut LinearVelocity,
    )>,
    move_and_slide: MoveAndSlide,
    settings: Res<PlayerSettings>,
    gravity: Res<Gravity>,
    time: Res<Time>,
) {
    let config = MoveAndSlideConfig::default();
    let skin_width = move_and_slide.length_unit.0 * config.skin_width;
    for (entity, motor, collider, layers, mut transform, mut velocity) in &mut players {
        let filter = SpatialQueryFilter::from_mask(layers.filters).with_excluded_entities([entity]);
        let rotation = transform.rotation;
        let ground = |position: Vec3| {
            move_and_slide
                .cast_move(
                    collider,
                    position,
                    rotation,
                    Vec3::NEG_Y * settings.ground_snap,
                    skin_width,
                    &filter,
                )
                .filter(|hit| hit.normal1.angle_between(Vec3::Y) <= settings.max_slope)
        };

        let grounded = ground(transform.translation).is_some();
        let fall = if grounded {
            0.
        } else {
            velocity.y + gravity.0.y * time.delta_secs()
        };
        let MoveAndSlideOutput {
            mut position,
            mut projected_velocity,
        } = move_and_slide.move_and_slide(
            collider,
            transform.translation,
            rotation,
            motor.desired_velocity.with_y(fall),
            time.delta(),
            &config,
            &filter,
            |_| MoveAndSlideHitResponse::Accept,
        );
        if grounded && let Some(hit) = ground(position) {
            position.y -= hit.distance;
            projected_velocity.y = 0.;
        }

        transform.translation = position;
        velocity.0 = projected_velocity;
        if let Some(forward) = motor.desired_forward {
            let target = Transform::default().looking_to(forward, Vec3::Y).rotation;
            transform.rotation =
                rotation.rotate_towards(target, settings.turn_speed * time.delta_secs());
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, move_player.in_set(PlayerSystems));
}

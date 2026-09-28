//! Kinematic player movement built on Avian's [`MoveAndSlide`].
//!
//! Movement policy (see quell#13):
//!
//! - **Grounding:** before each move, the player's collider is cast down by
//!   [`PlayerSettings::ground_snap`]. If it hits a surface no steeper than
//!   [`PlayerSettings::max_slope`], the player is grounded: gravity is skipped,
//!   and the player is snapped back onto the ground after moving.
//! - **Slopes:** surfaces steeper than `max_slope` block horizontal motion into
//!   them like walls, while falling still follows them, so they can't be climbed
//!   but can be slid down. Known limitation: only vertical velocity carries over
//!   between ticks, so the slide is a slow, timestep-dependent crawl.
//! - **Steps:** there is no explicit step-up. Ledge edges are subject to the
//!   slope rule, so the capsule rides over ledges only up to
//!   `capsule_radius * (1 - cos(max_slope))` high (about 0.15 m by default).
//!   The ground snap handles step-down.
//! - **Moving surfaces:** not supported. No platform velocity is inherited.
//! - **Dynamic bodies:** not handled. The kinematic player isn't pushed by them.

use crate::prelude::*;

/// Tolerance for treating velocity as tangent to a surface; matches Avian's
/// internal `DOT_EPSILON`.
const DOT_EPSILON: f32 = 0.005;

/// Velocity after touching a too-steep, upward-facing surface with outward
/// `normal`. Velocity already tangent to or leaving the surface is unchanged.
/// Otherwise, horizontal motion into the surface is blocked as if by a vertical
/// wall, so pushing can't climb it. Falling is handled separately, projected
/// onto the slope, so pushing into the slope can't cancel the fall and the
/// player doesn't cling. The result is tangent or separating, so Avian's own
/// projection leaves it alone and repeated hits don't compound.
fn steep_slope_velocity(v: Vec3, normal: Vec3) -> Vec3 {
    if v.dot(normal) >= -DOT_EPSILON {
        return v;
    }
    let wall = normal.with_y(0.).normalize_or_zero();
    let horizontal = v.with_y(0.);
    horizontal - wall * horizontal.dot(wall).min(0.)
        + Vec3::Y * v.y.max(0.)
        + (Vec3::Y * v.y.min(0.)).reject_from_normalized(normal)
}

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
                .filter(|hit| settings.walkable(hit.normal1))
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
            |hit| {
                let normal = **hit.normal;
                if normal.y > 0. && !settings.walkable(normal) {
                    *hit.velocity = steep_slope_velocity(*hit.velocity, normal);
                }
                MoveAndSlideHitResponse::Accept
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Outward normal of a 60° slope rising toward +X.
    fn steep() -> Vec3 {
        let angle = 60f32.to_radians();
        Vec3::new(-angle.sin(), angle.cos(), 0.)
    }

    #[test]
    fn pushing_into_a_steep_slope_neither_climbs_nor_penetrates() {
        let v = steep_slope_velocity(Vec3::new(5., 0., 2.), steep());
        assert!(v.y <= 0., "{v}");
        assert!(v.dot(steep()) >= 0., "{v}");
        assert_eq!(v.z, 2.);
    }

    #[test]
    fn falling_onto_a_steep_slope_slides_down_it() {
        let v = steep_slope_velocity(Vec3::NEG_Y * 5., steep());
        assert!(v.y < 0. && v.x < 0., "{v}");
        assert!(v.dot(steep()).abs() < 1e-5, "{v}");
    }

    #[test]
    fn leaving_velocity_is_kept_and_results_are_stable() {
        let leaving = Vec3::new(-3., 4., 0.);
        assert_eq!(steep_slope_velocity(leaving, steep()), leaving);
        let v = steep_slope_velocity(Vec3::new(5., -5., 0.), steep());
        assert_eq!(steep_slope_velocity(v, steep()), v);
    }
}

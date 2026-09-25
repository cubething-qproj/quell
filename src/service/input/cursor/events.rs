use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::prelude::*;

fn on_capture_cursor(
    trigger: On<Complete<PACaptureCursor>>,
    mut window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    controls: Query<&ICtxCaptureCursor>,
    tracking: Query<(Entity, &Camera), With<SpringArm>>,
    mut commands: Commands,
) {
    let (window, cursor) = &mut *window;
    // A click held across a switch must not capture on its eventual completion.
    if window.focused
        && controls
            .get(trigger.context)
            .is_ok_and(|controls| controls.armed)
        && let Ok((entity, camera)) = tracking.single()
        && camera.is_active
    {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
        commands
            .entity(entity)
            .insert(ContextActivity::<SpringArm>::ACTIVE);
    }
}
fn on_release_cursor(_: On<Complete<PAReleaseCursor>>, mut commands: Commands) {
    commands.trigger(ReleaseCursor);
}

fn release_cursor<E: Event>(
    _: On<E>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut controls: Query<&mut ICtxCaptureCursor>,
    tracking: Query<(Entity, &ContextActivity<SpringArm>)>,
    mut commands: Commands,
    #[cfg(feature = "dev")] mut free_cameras: Query<(&mut FreeCameraState, &mut FreeCameraInput)>,
) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
    for mut controls in &mut controls {
        controls.armed = false;
    }
    for (entity, activity) in &tracking {
        if **activity {
            commands
                .entity(entity)
                .insert(ContextActivity::<SpringArm>::INACTIVE);
        }
    }
    #[cfg(feature = "dev")]
    for (mut state, mut input) in &mut free_cameras {
        input.reset(&mut state);
    }
}

fn spawn_cursor_capture(_trigger: On<SpawnCursorCapture>, mut commands: Commands) {
    debug!("spawn_capture_cursor_actions");
    commands.spawn((
        Name::new("Cursor capture"),
        ICtxCaptureCursor::default(),
        ContextActivity::<ICtxCaptureCursor>::ACTIVE,
        // Quit (1000) checks capture first; release then precedes camera input (0).
        ContextPriority::<ICtxCaptureCursor>::new(500),
        // todo: state scope?
        actions![
            ICtxCaptureCursor[
                (
                    Action::<PACaptureCursor>::new(),
                    bindings![MouseButton::Left]
                ),
                (
                    Action::<PAReleaseCursor>::new(),
                    bindings![KeyCode::Escape],
                    ActionSettings {
                        consume_input: true,
                        require_reset: true,
                        ..Default::default()
                    }
                ),
           ]
        ],
    ));
}

fn sync_contexts(
    window: Single<(&Window, &CursorOptions), With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut controls: Query<&mut ICtxCaptureCursor>,
    tracking: Query<(Entity, &Camera, &ContextActivity<SpringArm>)>,
    mut players: Query<(
        Entity,
        &ContextActivity<ICtxDefault>,
        &mut PlayerController,
        &mut PlayerTnuaController,
    )>,
    mut commands: Commands,
) {
    let (window, cursor) = *window;
    let tracking_selected = tracking.iter().any(|(_, camera, _)| camera.is_active);
    for mut controls in &mut controls {
        if !window.focused || !tracking_selected {
            controls.armed = false;
        } else if !mouse.pressed(MouseButton::Left) && !mouse.just_released(MouseButton::Left) {
            // Capture fires on click completion. Wait past the release frame so a
            // click begun before switching/focus loss cannot recapture the cursor.
            controls.armed = true;
        }
    }
    for (entity, camera, activity) in &tracking {
        let enabled =
            camera.is_active && window.focused && cursor.grab_mode == CursorGrabMode::Locked;
        if **activity != enabled {
            commands
                .entity(entity)
                .insert(ContextActivity::<SpringArm>::new(enabled));
        }
    }
    for (entity, activity, mut player, mut controller) in &mut players {
        let enabled = tracking_selected && window.focused;
        if **activity != enabled {
            commands
                .entity(entity)
                .insert(ContextActivity::<ICtxDefault>::new(enabled));
        }
        if !enabled {
            player.last_move = None;
            controller.basis.desired_motion = Vec3::ZERO;
            controller.basis.desired_forward = None;
        }
    }
    if !window.focused && cursor.grab_mode != CursorGrabMode::None {
        commands.trigger(ReleaseCursor);
    }
}

fn on_remove_tracking(
    event: On<Remove, SpringArm>,
    cameras: Query<&Camera>,
    mut commands: Commands,
) {
    if cameras
        .get(event.entity)
        .is_ok_and(|camera| camera.is_active)
    {
        commands.trigger(ReleaseCursor);
    }
}

fn reset_smoothing<C: Component>(
    event: On<Insert, ContextActivity<C>>,
    contexts: Query<(&ContextActivity<C>, &Actions<C>)>,
    mut modifiers: Query<&mut SmoothNudge>,
) {
    let Ok((activity, actions)) = contexts.get(event.entity) else {
        return;
    };
    if **activity {
        return;
    }
    // Inactive contexts stop evaluating modifiers, freezing any retained value.
    // Clear that history without changing the configured response or clock.
    for action in actions {
        if let Ok(mut smooth) = modifiers.get_mut(action) {
            *smooth = SmoothNudge::new(smooth.decay_rate).with_time_kind(smooth.time_kind);
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(on_capture_cursor)
        .add_observer(on_release_cursor)
        .add_observer(release_cursor::<ReleaseCursor>)
        .add_observer(spawn_cursor_capture)
        // Keep q_screens lifecycle cleanup shared between both camera modes.
        .add_observer(release_cursor::<ScreenChanged>)
        .add_observer(on_remove_tracking)
        .add_observer(reset_smoothing::<ICtxDefault>)
        .add_observer(reset_smoothing::<SpringArm>)
        .add_systems(PreUpdate, sync_contexts.after(EnhancedInputSystems::Apply));
}

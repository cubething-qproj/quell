use crate::prelude::*;
use bevy::{
    app::RunFixedMainLoopSystems,
    camera_controller::free_camera::run_freecamera_controller,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

fn gate_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut camera: Single<(
        &Camera,
        &FreeCamera,
        &mut FreeCameraState,
        &mut FreeCameraInput,
    )>,
) {
    let (window, cursor) = &mut *window;
    let (camera, config, state, input) = &mut *camera;
    let held = mouse.pressed(config.mouse_key_cursor_grab);
    let toggle_held = keys.pressed(config.keyboard_key_toggle_cursor_grab);

    if !camera.is_active || !window.focused {
        if state.enabled {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
        input.reset(state);
        return;
    }

    // Always give a newly spawned/reset controller a disabled tick to clear its
    // private capture flags, and wait for held capture controls to be released.
    if !input.armed {
        input.armed = !held && !toggle_held && !keys.pressed(KeyCode::Escape);
        FreeCameraInput::stop(state);
        return;
    }

    if state.enabled && cursor.grab_mode != CursorGrabMode::Locked {
        input.reset(state);
        return;
    }

    if keys.just_pressed(config.keyboard_key_toggle_cursor_grab) {
        input.toggled = !input.toggled;
    }
    state.enabled = input.toggled || held;
    if !state.enabled {
        // Upstream applies movement before releasing its cursor. Predicting the
        // release here also prevents its subsequent snap-rotation system running.
        FreeCameraInput::stop(state);
    }
}

fn on_screen_changed(
    _: On<ScreenChanged>,
    mut cameras: Query<(&Camera, &mut FreeCameraState, &mut FreeCameraInput)>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    for (camera, mut state, mut input) in &mut cameras {
        if camera.is_active {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
        input.reset(&mut state);
    }
}

fn on_remove(
    event: On<Remove, FreeCamera>,
    cameras: Query<&Camera>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if cameras
        .get(event.entity)
        .is_ok_and(|camera| camera.is_active)
    {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        RunFixedMainLoop,
        gate_input
            .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop)
            .before(run_freecamera_controller),
    )
    .add_observer(on_screen_changed)
    .add_observer(on_remove);
}

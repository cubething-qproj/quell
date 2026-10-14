//! Observers for the global input context.

use crate::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

/// Exits on completion of [`PAQuit`] when the cursor is free.
/// Ignores the event while captured; [`PAReleaseCursor`] handles that case.
fn exit_app(
    _: On<Complete<PAQuit>>,
    mut commands: Commands,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
) {
    debug!("exit_app");
    if matches!(cursor.grab_mode, CursorGrabMode::None) {
        commands.write_message(AppExit::Success);
    }
}

/// Spawns the global context with its Escape-to-quit binding.
fn spawn_global_ctx(_: On<SpawnGlobalCtx>, mut commands: Commands) {
    commands.spawn((
        ICtxGlobal,
        ScreenScoped,
        ContextActivity::<ICtxGlobal>::ACTIVE,
        ContextPriority::<ICtxGlobal>::new(1000),
        actions![
            ICtxGlobal[(
                Action::<PAQuit>::new(),
                bindings![KeyCode::Escape],
                ActionSettings {
                    consume_input: false,
                    require_reset: true,
                    ..Default::default()
                }
            )]
        ],
    ));
}

/// Registers the quit and spawn observers.
pub fn plugin(app: &mut App) {
    app.add_observer(exit_app);
    app.add_observer(spawn_global_ctx);
}

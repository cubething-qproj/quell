//! Components and input for the camera sandbox.

use crate::prelude::*;

/// The moving target the tracking camera follows.
#[derive(Component)]
pub struct Cube;

/// Input context for switching cameras with Tab. Also used by other sandboxes
/// that offer a free camera.
#[derive(Component)]
pub struct CameraTestInput;

impl CameraTestInput {
    /// A screen-scoped entity carrying this context and its Tab binding.
    ///
    /// High priority, so switching cameras releases the cursor before quit,
    /// capture, or orbit actions see the press.
    pub(crate) fn bundle() -> impl Bundle {
        (
            Self,
            ScreenScoped,
            Name::new("Camera Test Input"),
            ContextActivity::<Self>::ACTIVE,
            // Switch and release capture before quit, capture, or orbit actions run.
            ContextPriority::<Self>::new(2000),
            actions![
                Self[(
                    Action::<PASwitchCamera>::new(),
                    bindings![KeyCode::Tab],
                    ActionSettings {
                        require_reset: true,
                        ..default()
                    },
                )]
            ],
        )
    }
}

/// Swaps the active camera between free and tracking.
#[derive(InputAction)]
#[action_output(bool)]
pub struct PASwitchCamera;

/// The help text, rewritten to match whichever camera is active.
#[derive(Component)]
pub struct CameraTestHelp;

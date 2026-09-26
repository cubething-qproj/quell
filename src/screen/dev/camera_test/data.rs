use crate::prelude::*;

#[derive(Component)]
pub struct Cube;

#[derive(Component)]
pub struct CameraTestInput;

impl CameraTestInput {
    pub(super) fn bundle() -> impl Bundle {
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

#[derive(InputAction)]
#[action_output(bool)]
pub struct PASwitchCamera;

#[derive(Component)]
pub struct CameraTestHelp;

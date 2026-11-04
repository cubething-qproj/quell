use crate::prelude::*;

#[derive(Component)]
pub struct Cube;

#[derive(Component, Clone, Default)]
pub struct CameraTestInput;

impl CameraTestInput {
    pub(crate) fn bundle() -> impl Scene {
        bsn! {
            Self
            ScreenScoped
            Name::new("Camera Test Input")
            ContextActivity::<Self>::ACTIVE
            // Switch and release capture before quit, capture, or orbit actions run.
            ContextPriority::<Self>::new(2000)
            Actions<Self> [
                Action::<PASwitchCamera>::new()
                Bindings [Binding(KeyCode::Tab)]
                ActionSettings {
                    require_reset: true,
                }
            ]
        }
    }
}

#[derive(InputAction)]
#[action_output(bool)]
pub struct PASwitchCamera;

#[derive(Component)]
pub struct CameraTestHelp;

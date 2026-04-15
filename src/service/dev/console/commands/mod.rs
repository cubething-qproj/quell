mod screen;

use crate::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_plugins(screen::plugin);
}

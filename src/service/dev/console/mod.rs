mod commands;
mod ui;

use crate::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_plugins(q_term::ConsolePlugin);
    app.add_plugins((ui::plugin, commands::plugin));
}

mod query;
mod ui;

use crate::prelude::*;

pub fn plugin(app: &mut App) {
    app.add_plugins(q_shell::prelude::ShellPlugin::default());
    app.add_plugins((ui::plugin, query::plugin));
}

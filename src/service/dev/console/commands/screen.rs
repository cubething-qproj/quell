use clap::Parser;
use q_term::prelude::*;

use crate::prelude::*;

#[derive(Parser, Debug, Message, Clone)]
#[command(name = "screen")]
struct ScreenCmd {
    target: Option<String>,
    #[arg(short, long)]
    list: bool,
    #[arg(short, long)]
    show: bool,
}

fn on_msg(
    mut reader: MessageReader<CommandMsg<ScreenCmd>>,
    mut commands: Commands,
    screens: Screens,
    data: Res<ScreenData>,
    current_screen: Res<CurrentScreen>,
) {
    for msg in reader.read() {
        if msg.command.list {
            let list = data
                .iter()
                .filter_map(|i| i.as_ref().map(|i| format!("{}\n", i.name())))
                .collect::<String>();
            msg.println(&mut commands, list);
        }
        if msg.command.show
            && let Some(screen) = &**current_screen.as_ref()
        {
            let screen = c!(screens.get_by_id(*screen));
            msg.println(&mut commands, screen.name().to_owned());
        }
        if let Some(target) = msg.command.target.as_ref() {
            match screens.get_by_name(target) {
                Ok(screen) => {
                    commands.write_message(SwitchToScreenMsg(screen.screen_id()));
                    msg.println(&mut commands, format!("Switching to {}", screen.name()));
                }
                Err(err) => {
                    msg.println(&mut commands, err.to_string());
                }
            }
        }
    }
}
pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, on_msg);
    app.add_console_command::<ScreenCmd>();
}

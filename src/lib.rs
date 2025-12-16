mod game;
mod main_menu;

use avian2d::PhysicsPlugins;
use bevy::prelude::*;

#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
enum GameState {
    #[default]
    Menu,
    Game,
}

pub struct GameAppPlugin;

impl Plugin for GameAppPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((DefaultPlugins, PhysicsPlugins::default()))
            .init_state::<GameState>()
            .add_systems(Startup, setup)
            .add_plugins((main_menu::plugin, game::plugin));
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.insert_resource(ClearColor(Color::BLACK));
}

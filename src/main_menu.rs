use super::*;

use bevy::{
    color::palettes::css::{BLACK, WHITE},
    prelude::*,
};

#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, States)]
enum MenuState {
    Main,
    #[default]
    Disabled,
}

#[derive(Component)]
struct OnMainMenuScreen;

const TEXT_COLOR: Color = Color::Srgba(WHITE);
const HOVER_TEXT_COLOR: Color = Color::Srgba(BLACK);
const NORMAL_BUTTON: Color = Color::Srgba(BLACK);
const HOVERED_BUTTON: Color = Color::Srgba(WHITE);
const HOVERED_PRESSED_BUTTON: Color = Color::Srgba(WHITE);
const PRESSED_BUTTON: Color = Color::Srgba(WHITE);

#[derive(Component)]
struct SelectedOption;

#[derive(Component)]
enum MenuButtonAction {
    Play,
    Quit,
}

pub(crate) fn plugin(app: &mut App) {
    app.init_state::<MenuState>()
        .add_systems(OnEnter(GameState::Menu), menu_setup)
        .add_systems(OnEnter(MenuState::Main), main_menu_setup)
        .add_systems(
            Update,
            (menu_action, button_system).run_if(in_state(GameState::Menu)),
        )
        .add_systems(OnExit(MenuState::Main), cleanup_main_menu_screen)
        .add_systems(OnExit(GameState::Menu), cleanup_menu_screen);
}

fn button_system(
    mut interaction_query: Query<
        (
            &Interaction,
            &Children,
            &mut BackgroundColor,
            &mut BorderColor,
            Option<&SelectedOption>,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut texts: Query<&mut TextColor>,
) {
    for (interaction, children, mut background_color, mut border_color, selected) in
        &mut interaction_query
    {
        (*background_color, *border_color) = match (*interaction, selected) {
            (Interaction::Pressed, _) | (Interaction::None, Some(_)) => {
                for &child in children {
                    if let Ok(mut text_color) = texts.get_mut(child) {
                        text_color.0 = HOVER_TEXT_COLOR;
                    }
                }

                (PRESSED_BUTTON.into(), BorderColor::all(HOVER_TEXT_COLOR))
            }
            (Interaction::Hovered, Some(_)) => {
                for &child in children {
                    if let Ok(mut text_color) = texts.get_mut(child) {
                        text_color.0 = HOVER_TEXT_COLOR;
                    }
                }

                (
                    HOVERED_PRESSED_BUTTON.into(),
                    BorderColor::all(HOVER_TEXT_COLOR),
                )
            }
            (Interaction::Hovered, None) => {
                for &child in children {
                    if let Ok(mut text_color) = texts.get_mut(child) {
                        text_color.0 = HOVER_TEXT_COLOR;
                    }
                }

                (HOVERED_BUTTON.into(), BorderColor::all(HOVER_TEXT_COLOR))
            }
            (Interaction::None, None) => {
                for &child in children {
                    if let Ok(mut text_color) = texts.get_mut(child) {
                        text_color.0 = TEXT_COLOR;
                    }
                }

                (NORMAL_BUTTON.into(), BorderColor::all(TEXT_COLOR))
            }
        }
    }
}

fn menu_setup(mut menu_state: ResMut<NextState<MenuState>>) {
    menu_state.set(MenuState::Main);
}

fn cleanup_menu_screen(mut _commands: Commands) {
    // Additional cleanup can be added here if needed
}

fn cleanup_main_menu_screen(mut _commands: Commands, mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::BLACK;
}

fn main_menu_setup(
    mut commands: Commands,
    mut clear_color: ResMut<ClearColor>,
    _asset_server: Res<AssetServer>,
    font_family: Res<MainFont>,
) {
    let font_family = &font_family.0;

    let button_node = Node {
        width: px(300),
        height: px(65),
        margin: UiRect::all(px(20)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(2)),
        ..default()
    };
    let button_text_font = TextFont {
        font_size: 33.0,
        font: font_family.clone(),
        ..default()
    };

    clear_color.0 = BLACK.into();

    commands.spawn((
        DespawnOnExit(MenuState::Main),
        Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        OnMainMenuScreen,
        children![(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            children![
                (
                    Text::new("Moon Landr"),
                    TextFont {
                        font_size: 67.0,
                        font: font_family.clone(),
                        ..default()
                    },
                    TextColor(TEXT_COLOR),
                    Node {
                        margin: UiRect::all(px(50)),
                        ..default()
                    },
                ),
                (
                    Button,
                    button_node.clone(),
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor::all(TEXT_COLOR),
                    MenuButtonAction::Play,
                    children![(
                        Text::new("New Game"),
                        button_text_font.clone(),
                        TextColor(TEXT_COLOR),
                    ),]
                ),
                /*(
                    Button,
                    button_node.clone(),
                    BackgroundColor(NORMAL_BUTTON),
                    MenuButtonAction::Settings,
                    children![
                        (
                            Text::new("Settings"),
                            button_text_font.clone(),
                            TextColor(TEXT_COLOR),
                        ),
                    ]
                ),*/
                (
                    Button,
                    button_node,
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor::all(TEXT_COLOR),
                    MenuButtonAction::Quit,
                    children![(Text::new("Quit"), button_text_font, TextColor(TEXT_COLOR),),]
                ),
            ]
        )],
    ));
}

fn menu_action(
    interaction_query: Query<
        (&Interaction, &MenuButtonAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut app_exit_writer: MessageWriter<AppExit>,
    mut menu_state: ResMut<NextState<MenuState>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    for (interaction, menu_button_action) in &interaction_query {
        if *interaction == Interaction::Pressed {
            match menu_button_action {
                MenuButtonAction::Quit => {
                    app_exit_writer.write(AppExit::Success);
                }
                MenuButtonAction::Play => {
                    game_state.set(GameState::Game);
                    menu_state.set(MenuState::Disabled);
                }
            }
        }
    }
}

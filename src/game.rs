use crate::*;

use avian2d::prelude::*;
use bevy::{camera::ScalingMode, prelude::*};
//use bevy_math::prelude::*;
use noiz::prelude::*;
use std::{
    fmt::Debug,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(SubStates, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[source(GameState = GameState::Game)]
enum GamePhase {
    #[default]
    Running,
    Win,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Player;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerState {
    Idle,
    Firing,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Fuel(u32);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerLandingState {
    OutsideLandingZone,
    InLandingZone,
    Landed,
}

#[derive(Resource)]
struct WinTimer(Timer);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct PlayerFuelText;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct PlayerVelocityText;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Terrain;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct LandPad;

pub(crate) fn plugin(app: &mut App) {
    app.add_sub_state::<GamePhase>()
        .add_systems(OnEnter(GameState::Game), setup_level)
        .add_systems(
            Update,
            (
                control_system.run_if(in_state(GamePhase::Running)),
                win_input_system.run_if(in_state(GamePhase::Win)),
                animation_system,
                hud_system,
            )
                .chain()
                .run_if(in_state(GameState::Game)),
        )
        .add_systems(
            Update,
            (
                player_landing_state_transition_system,
                win_timer_system.run_if(player_is_landed),
            )
                .chain()
                .run_if(in_state(GameState::Game)),
        )
        .add_systems(OnExit(GameState::Game), cleanup_level)
        .add_systems(OnEnter(GamePhase::Win), setup_win_screen)
        .add_systems(OnExit(GamePhase::Win), cleanup_win_screen);
}

fn setup_level(
    mut commands: Commands,
    mut clear_color: ResMut<ClearColor>,
    asset_server: Res<AssetServer>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    clear_color.0 = Color::BLACK;

    let Projection::Orthographic(perspective) = camera.1.as_mut() else {
        return;
    };

    perspective.scaling_mode = ScalingMode::AutoMax {
        max_width: 1600.0,
        max_height: 900.0,
    };

    camera.0.translation = Vec2::new(800.0, 450.0).extend(camera.0.translation.z);

    let texture = asset_server.load("sprites/lander.png");

    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 16), 2, 1, None, None);

    let layout_handle = layouts.add(layout);

    commands.spawn((
        DespawnOnExit(GameState::Game),
        Player,
        PlayerLandingState::OutsideLandingZone,
        RigidBody::Dynamic,
        Collider::rectangle(16.0, 16.0),
        Mass(1.0),
        Sprite::from_atlas_image(
            texture,
            TextureAtlas {
                layout: layout_handle,
                index: 0,
            },
        ),
        PlayerState::Idle,
        Fuel(1000),
        Transform::from_translation(Vec3::new(400.0, 500.0, 0.0)),
        LinearVelocity {
            0: Vec2::new(12.0, 0.0),
        },
    ));

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;

    let mut noise_generator = Noise::from(common_noise::Perlin::default());
    noise_generator.set_seed(seed);
    noise_generator.set_period(1600.0);

    let terrain_points: Vec<Vec2> = (0..1600)
        .map(|x| {
            let height =
                noise_generator.sample_for::<f32>(Vec2::new(x as f32, 0.0)) * 200.0 + 300.0;
            Vec2::new(x as f32, height)
        })
        .collect();

    let terrain_mesh = meshes.add(Polyline2d::new(terrain_points.clone()));

    commands.spawn((
        DespawnOnExit(GameState::Game),
        Terrain,
        RigidBody::Static,
        Collider::polyline(terrain_points, None),
        Mesh2d(terrain_mesh),
        MeshMaterial2d(materials.add(Color::WHITE)),
    ));

    commands
        .spawn((
            DespawnOnExit(GameState::Game),
            LandPad,
            RigidBody::Static,
            Sensor,
            CollisionEventsEnabled,
            Collider::rectangle(64.0, 16.0),
            Transform::from_translation(Vec3::new(700.0, 500.0, 0.0)),
            Mesh2d(meshes.add(Rectangle::new(64.0, 16.0))),
            MeshMaterial2d(materials.add(Color::srgb(1.0, 0.0, 0.0))),
            children![(
                RigidBody::Static,
                Collider::rectangle(64.0, 2.0),
                Transform::from_translation(Vec3::new(0.0, -8.0, 0.0)),
                Mesh2d(meshes.add(Rectangle::new(64.0, 2.0))),
                MeshMaterial2d(materials.add(Color::WHITE)),
            )],
        ))
        .observe(player_entered_landing_zone)
        .observe(player_exited_landing_zone);

    commands.spawn((
        DespawnOnExit(GameState::Game),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(5.0),
            ..Default::default()
        },
        children![
            (
                PlayerFuelText,
                Text::new("Fuel: 100"),
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Left),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
            ),
            (
                PlayerVelocityText,
                Text::new("Horizontal velocity: 0.0 m/s, Vertical velocity: 0.0 m/s"),
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Left),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
            )
        ],
    ));

    commands.insert_resource(WinTimer(Timer::from_seconds(3.0, TimerMode::Once)));
}

fn cleanup_level(
    mut commands: Commands,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera>>,
) {
    let Projection::Orthographic(perspective) = camera.1.as_mut() else {
        return;
    };

    perspective.scaling_mode = ScalingMode::WindowSize;

    camera.0.translation = Vec2::new(0.0, 0.0).extend(camera.0.translation.z);

    commands.remove_resource::<WinTimer>();
}

fn win_input_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut game_phase: ResMut<NextState<GamePhase>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        game_phase.set(GamePhase::Running);
        game_state.set(GameState::Menu);
    }
}

fn control_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut player: Single<(&Transform, Forces, &mut PlayerState, &mut Fuel), With<Player>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if keyboard_input.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        player.1.apply_angular_acceleration(2.0);
    }
    if keyboard_input.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        player.1.apply_angular_acceleration(-2.0);
    }

    if player.3.0 > 0 {
        if keyboard_input.just_pressed(KeyCode::Space) {
            *player.2 = PlayerState::Firing;
        }
        if keyboard_input.pressed(KeyCode::Space) {
            let acceleration_vector = (player.0.rotation * Vec3::Y * 100.0).truncate();

            player.1.apply_linear_acceleration(acceleration_vector);
            player.3.0 = player.3.0.saturating_sub(1);
        }
    }
    if (keyboard_input.just_released(KeyCode::Space) && *player.2 == PlayerState::Firing)
        || player.3.0 == 0
    {
        *player.2 = PlayerState::Idle;
    }

    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
    }
}

fn animation_system(
    mut player: Single<(&PlayerState, &mut Sprite), (With<Player>, Changed<PlayerState>)>,
) {
    match player.0 {
        PlayerState::Idle => {
            player.1.texture_atlas.as_mut().unwrap().index = 0;
        }
        PlayerState::Firing => {
            player.1.texture_atlas.as_mut().unwrap().index = 1;
        }
    }
}

fn hud_system(
    player: Single<(&LinearVelocity, &Fuel), With<Player>>,
    mut fuel_text: Single<&mut Text, With<PlayerFuelText>>,
    mut velocity_text: Single<&mut Text, (With<PlayerVelocityText>, Without<PlayerFuelText>)>,
) {
    fuel_text.0 = format!("Fuel: {}", player.1.0);

    let horizontal_velocity = player.0.0.x;
    let vertical_velocity = player.0.0.y;

    velocity_text.0 = format!(
        "Horizontal velocity: {:.1} m/s, Vertical velocity: {:.1} m/s",
        horizontal_velocity, vertical_velocity
    );
}

fn player_is_landed(player_landing_state: Single<&PlayerLandingState, With<Player>>) -> bool {
    **player_landing_state == PlayerLandingState::Landed
}

fn player_entered_landing_zone(
    event: On<CollisionStart>,
    player_query: Query<&mut PlayerLandingState, With<Player>>,
) {
    let other_entity = event.collider2;
    if player_query.contains(other_entity) {
        let mut landing_state = player_query
            .single_inner()
            .expect("Player should have PlayerLandingState");
        if *landing_state == PlayerLandingState::OutsideLandingZone {
            *landing_state = PlayerLandingState::InLandingZone;
        }
    }
}

fn player_exited_landing_zone(
    event: On<CollisionEnd>,
    player_query: Query<&mut PlayerLandingState, With<Player>>,
) {
    let other_entity = event.collider2;
    if player_query.contains(other_entity) {
        let mut landing_state = player_query
            .single_inner()
            .expect("Player should have PlayerLandingState");
        *landing_state = PlayerLandingState::OutsideLandingZone;
    }
}

fn win_timer_system(
    time: Res<Time>,
    mut win_timer: ResMut<WinTimer>,
    mut game_phase: ResMut<NextState<GamePhase>>,
    player_landing_state: Single<&mut PlayerLandingState, With<Player>>,
) {
    if **player_landing_state != PlayerLandingState::Landed {
        return;
    }

    win_timer.0.tick(time.delta());

    if win_timer.0.just_finished() {
        game_phase.set(GamePhase::Win);
    }
}

fn player_landing_state_transition_system(
    mut player: Single<
        (&mut PlayerLandingState, &LinearVelocity, &AngularVelocity),
        (
            With<Player>,
            Changed<LinearVelocity>,
            Changed<AngularVelocity>,
        ),
    >,
    mut win_timer: ResMut<WinTimer>,
) {
    match *player.0 {
        PlayerLandingState::InLandingZone => {
            if player.1.0.length() < 5.0 && player.2.0.abs() < 0.1 {
                win_timer.0.reset();
                *player.0 = PlayerLandingState::Landed;
            }
        }
        PlayerLandingState::Landed => {
            if player.1.0.length() >= 5.0 || player.2.0.abs() >= 0.1 {
                *player.0 = PlayerLandingState::InLandingZone;
            }
        }
        _ => {}
    };
}

fn setup_win_screen(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(GamePhase::Win),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..Default::default()
        },
        children![(
            Text::new("You Landed Successfully!\nPress SPACE to return to menu."),
            TextColor(Color::WHITE),
            TextLayout::new_with_justify(Justify::Center),
            TextFont {
                font_size: 48.0,
                ..default()
            },
        )],
    ));
}

fn cleanup_win_screen(mut _commands: Commands) {}

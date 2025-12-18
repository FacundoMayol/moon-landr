use crate::*;

use avian2d::{math::PI, prelude::*};
use bevy::{camera::ScalingMode, prelude::*};
//use bevy_math::prelude::*;
use noiz::prelude::*;
use std::{
    fmt::Debug,
    time::{SystemTime, UNIX_EPOCH},
};

//// TODO
/// Win condition should be to land, landing pads should only add multipliers to score. Currently, win is only on pads (that's wrong).
/// Losing condition should be to crash into ground at high speed or bad angle.
/// Should add more animation, background stars, parallax scrolling, sound effects, etc.
/// Should add a scoring system based on fuel used, landing accuracy, time taken, etc.
/// Should restrict player rotation
/// Should make ground generation more interesting
/// Should make lander more realistic with physics
/// Should have walls on sides of screen to prevent flying offscreen.

/// CONSTANTS:
/// Lander weight: 15.100 kg tonnes (8.200 kg for fuel)
/// Moon gravity: 1.62 m/s2
/// Lander main engine thrust: 45 kN

#[derive(SubStates, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[source(GameState = GameState::Game)]
enum GamePhase {
    #[default]
    Running,
    Win,
    Lose,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Player;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerState {
    Idle,
    Firing,
    Crashed,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Fuel(u32);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct ScoreMultiplier(f32);

#[derive(Resource)]
struct WinTimer(Timer);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct PlayerFuelText;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct PlayerVelocityText;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Ground;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Grounded(bool);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct LandPad {
    score_multiplier: f32,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_sub_state::<GamePhase>()
        .add_systems(OnEnter(GameState::Game), setup_level)
        .add_systems(
            Update,
            (
                ((
                    control_system,
                    ground_detection_system,
                    start_win_timer_system,
                    reset_win_timer_system,
                    tick_win_timer_system,
                )
                    .chain(),)
                    .run_if(in_state(GamePhase::Running)),
                (end_input_system).run_if(not(in_state(GamePhase::Running))),
                animation_system,
                hud_system,
            )
                .run_if(in_state(GameState::Game)),
        )
        .add_systems(OnExit(GameState::Game), cleanup_level)
        .add_systems(OnEnter(GamePhase::Lose), setup_lose_screen)
        .add_systems(OnExit(GamePhase::Lose), cleanup_lose_screen)
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

    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 16), 3, 1, None, None);

    let layout_handle = layouts.add(layout);

    commands
        .spawn((
            DespawnOnExit(GameState::Game),
            Player,
            Grounded(false),
            ScoreMultiplier(1.0),
            RigidBody::Dynamic,
            CollisionEventsEnabled,
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
                0: Vec2::new(0.0, 0.0),
            },
        ))
        .observe(player_crash_observer);

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;

    let mut noise_generator = Noise::from(LayeredNoise::new(
        Normed::<f32>::default(),
        Persistence(0.7),
        FractalLayers {
            layer: Octave::<MixCellGradients<OrthoGrid, Smoothstep, QuickGradients>>::default(),
            lacunarity: 2.0,
            amount: 12,
        },
    ));
    noise_generator.set_seed(seed);
    noise_generator.set_period(800.0);

    let ground_points: Vec<Vec2> = (0..800)
        .map(|x| {
            let height =
                noise_generator.sample_for::<f32>(Vec2::new(x as f32, 0.0)) * 500.0 + 300.0;
            Vec2::new(x as f32 * 2.0, height)
        })
        .collect();

    let ground_mesh = meshes.add(Polyline2d::new(ground_points.clone()));

    commands.spawn((
        DespawnOnExit(GameState::Game),
        Ground,
        RigidBody::Static,
        Collider::polyline(ground_points, None),
        Mesh2d(ground_mesh),
        MeshMaterial2d(materials.add(Color::WHITE)),
    ));

    commands
        .spawn((
            DespawnOnExit(GameState::Game),
            LandPad {
                score_multiplier: 3.0,
            },
            RigidBody::Static,
            Sensor,
            CollisionEventsEnabled,
            Collider::rectangle(64.0, 16.0),
            Transform::from_translation(Vec3::new(700.0, 500.0, 0.0)),
            Mesh2d(meshes.add(Rectangle::new(64.0, 16.0))),
            MeshMaterial2d(materials.add(Color::srgb(1.0, 0.0, 0.0))),
        ))
        .observe(player_entered_landing_zone)
        .observe(player_exited_landing_zone)
        .with_children(|parent| {
            parent.spawn((
                Ground,
                RigidBody::Static,
                CollisionEventsEnabled,
                Collider::rectangle(64.0, 2.0),
                Transform::from_translation(Vec3::new(0.0, -8.0, 0.0)),
                Mesh2d(meshes.add(Rectangle::new(64.0, 2.0))),
                MeshMaterial2d(materials.add(Color::WHITE)),
            ));
        });

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

fn end_input_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
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
        PlayerState::Crashed => {
            player.1.texture_atlas.as_mut().unwrap().index = 2;
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

fn player_entered_landing_zone(
    event: On<CollisionStart>,
    landpads: Query<&LandPad>,
    mut player: Single<(&mut ScoreMultiplier, Entity), With<Player>>,
) {
    let this_entity = event.collider1;
    let other_entity = event.collider2;

    let Ok(land_pad) = landpads.get(this_entity) else {
        return;
    };

    if player.1 != other_entity {
        return;
    };

    player.0.0 = land_pad.score_multiplier;
}

fn player_exited_landing_zone(
    event: On<CollisionEnd>,
    mut player: Single<(&mut ScoreMultiplier, Entity), With<Player>>,
) {
    let other_entity = event.collider2;

    if player.1 != other_entity {
        return;
    };

    player.0.0 = 1.0;
}

fn ground_detection_system(
    mut collision_started: MessageReader<CollisionStart>,
    mut collision_ended: MessageReader<CollisionEnd>,
    ground_query: Query<(), With<Ground>>,
    mut grounded_query: Query<&mut Grounded /*, With<Player>*/>,
) {
    for event in collision_started.read() {
        let (a, b) = (event.collider1, event.collider2);

        let grounded_entity: Entity = if grounded_query.get(a).is_ok() {
            a
        } else if grounded_query.get(b).is_ok() {
            b
        } else {
            continue;
        };

        let other = if grounded_entity == a { b } else { a };
        if ground_query.get(other).is_ok() {
            if let Ok(mut grounded) = grounded_query.get_mut(grounded_entity) {
                grounded.0 = true;
            }
        }
    }

    for event in collision_ended.read() {
        let (a, b) = (event.collider1, event.collider2);

        let grounded_entity: Entity = if grounded_query.get(a).is_ok() {
            a
        } else if grounded_query.get(b).is_ok() {
            b
        } else {
            continue;
        };

        let other = if grounded_entity == a { b } else { a };
        if ground_query.get(other).is_ok() {
            if let Ok(mut grounded) = grounded_query.get_mut(grounded_entity) {
                grounded.0 = false;
            }
        }
    }
}

fn player_crash_observer(
    event: On<CollisionStart>,
    player: Single<Entity, With<Player>>,
    ground_query: Query<Entity, With<Ground>>,
    collisions: Collisions,
    mut game_phase: ResMut<NextState<GamePhase>>,
) {
    let (a, b) = (event.collider1, event.collider2);

    let player_entity = if a == *player {
        a
    } else if b == *player {
        b
    } else {
        return;
    };

    let other_entity = if player_entity == a { b } else { a };

    if ground_query.get(other_entity).is_err() {
        return;
    }

    let mut impact_impulse_magnitude = 0.0;
    for contact_pair in collisions.collisions_with(player_entity) {
        impact_impulse_magnitude += contact_pair.total_normal_impulse_magnitude();
    }

    if impact_impulse_magnitude > 50.0 {
        game_phase.set(GamePhase::Lose);
    }
}

fn tick_win_timer_system(
    time: Res<Time>,
    mut win_timer: ResMut<WinTimer>,
    mut game_phase: ResMut<NextState<GamePhase>>,
) {
    win_timer.0.tick(time.delta());
    if win_timer.0.just_finished() {
        game_phase.set(GamePhase::Win);
    }
}

fn start_win_timer_system(
    player: Single<(&Grounded, &LinearVelocity, &AngularVelocity, &Transform), With<Player>>,
    mut win_timer: ResMut<WinTimer>,
) {
    if win_timer.0.is_paused()
        && (player.0.0
            && player.1.0.length() < 5.0
            && player.2.0.abs() < 0.1
            && player.3.rotation.to_euler(EulerRot::XYZ).2.abs() < PI / 2.0)
    {
        win_timer.0.reset();
        win_timer.0.unpause();
    }
}

fn reset_win_timer_system(
    player: Single<(&Grounded, &LinearVelocity, &AngularVelocity, &Transform), With<Player>>,
    mut win_timer: ResMut<WinTimer>,
) {
    if !win_timer.0.is_paused()
        && (!player.0.0
            || player.1.0.length() >= 5.0
            || player.2.0.abs() >= 0.1
            || player.3.rotation.to_euler(EulerRot::XYZ).2.abs() >= PI / 2.0)
    {
        win_timer.0.pause();
    }
}

fn setup_lose_screen(
    mut commands: Commands,
    mut player: Single<
        (
            Entity,
            &mut PlayerState,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<Player>,
    >,
) {
    *player.1 = PlayerState::Crashed;
    player.2.0 = Vec2::ZERO;
    player.3.0 = 0.0;
    commands.entity(player.0).insert(LockedAxes::ALL_LOCKED);

    commands.spawn((
        DespawnOnExit(GamePhase::Lose),
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
            Text::new("You Lost!\nPress SPACE to return to menu."),
            TextColor(Color::WHITE),
            TextLayout::new_with_justify(Justify::Center),
            TextFont {
                font_size: 48.0,
                ..default()
            },
        )],
    ));
}

fn cleanup_lose_screen(mut _commands: Commands) {}

fn setup_win_screen(mut commands: Commands, player: Single<&ScoreMultiplier, With<Player>>) {
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
            Text::new(format!(
                "You Landed Successfully!\nPress SPACE to return to menu.\nScore Multiplier: {:.2}",
                player.0
            )),
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

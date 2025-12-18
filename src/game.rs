use crate::*;

use avian2d::{math::PI, prelude::*};
use bevy::{camera::ScalingMode, prelude::*};
use noiz::prelude::*;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::{
    fmt::Debug,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

//// TODO
/// Terrain should be infinite, generated as the player moves. Also the camera should follow the player.
/// Should have landing pads working correctly.
/// Should add more animation, background stars, parallax scrolling, sound effects, etc.
/// Should add a scoring system based on fuel used, landing accuracy, time taken, etc.
/// Should make ground generation more interesting

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

#[derive(Resource)]
struct TimePassed(Duration);

#[derive(Component)]
enum HudText {
    Fuel,
    XVelocity,
    YVelocity,
    TimePassed,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Ground;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Grounded(bool);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct TerrainChunk {
    x_origin: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct LandPad {
    score_multiplier: f32,
}

type TerrainNoiseType = Noise<
    LayeredNoise<
        Normed<f32>,
        Persistence,
        FractalLayers<Octave<MixCellGradients<OrthoGrid, Smoothstep, QuickGradients>>>,
    >,
>;

#[derive(Resource)]
struct TerrainNoiseGenerator(TerrainNoiseType);

#[derive(Resource)]
struct TerrainMaterial(Handle<ColorMaterial>);

#[derive(Resource)]
struct GameSounds {
    thrust_sound: Handle<AudioSource>,
    crash_sound: Handle<AudioSource>,
    landing_sound: Handle<AudioSource>,
}

#[derive(Component)]
enum GameSound {
    Thrust,
    Crash,
    Landing,
}

const GRAVITY: Vec2 = Vec2::new(0.0, -1.62);
const THRUST: f32 = 12000.0;
const ROTATION_THRUST: f32 = 3.0;
const FUEL_CONSUMPTION_RATE: u32 = 1;
const SAFE_LANDING_IMPULSE_MAGNITUDE: f32 = 15000.0;
const FUEL_MASS_FACTOR: f32 = 1.0;
const DRY_LANDER_MASS: f32 = 800.0;
const MAX_FUEL: u32 = 1000;

const CHUNK_BUFFER_OUTSIDE_VIEWPORT_COUNT: i32 = 3;
const CHUNK_WIDTH: f32 = 400.0;
const CHUNK_GRANULARITY: u32 = 2; // units per sample point
const CHUNK_NOISE_LAYERS: u32 = 12;
const CHUNK_NOISE_PERSISTENCE: f32 = 0.6;
const CHUNK_NOISE_LACUNARITY: f32 = 2.0;
//const CHUNK_NOISE_PERIOD: f32 = CHUNK_WIDTH / CHUNK_GRANULARITY as f32;
const CHUNK_NOISE_FREQUENCY: f32 = CHUNK_GRANULARITY as f32 / CHUNK_WIDTH;
const CHUNK_HEIGHT_AMPLITUDE: f32 = 300.0;
const CHUNK_BASE_HEIGHT: f32 = 300.0;

const CAMERA_VIEWPORT_WIDTH: f32 = 1600.0;
const CAMERA_VIEWPORT_HEIGHT: f32 = 900.0;

const LANDER_SIZE: UVec2 = UVec2::new(16, 16);
const LAND_PAD_WIDTH: u32 = 24; // in world units

const INITIAL_HORIZONTAL_SPEED: f32 = 50.0;

const WIN_TIMER_DURATION: f32 = 3.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_sub_state::<GamePhase>()
        .add_systems(OnEnter(GameState::Game), setup_level)
        .add_systems(
            Update,
            (
                (
                    (
                        control_system,
                        audio_system,
                        terrain_chunk_system,
                        camera_follow_system,
                        ground_detection_system,
                        start_win_timer_system,
                        reset_win_timer_system,
                        tick_win_timer_system,
                    )
                        .chain(),
                    fuel_weight_system,
                    playtime_system,
                )
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
    font: Res<MainFont>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    /*mut meshes: ResMut<Assets<Mesh>>,*/
) {
    let font = &font.0;

    clear_color.0 = Color::BLACK;

    let Projection::Orthographic(perspective) = camera.1.as_mut() else {
        return;
    };

    perspective.scaling_mode = ScalingMode::Fixed {
        width: CAMERA_VIEWPORT_WIDTH,
        height: CAMERA_VIEWPORT_HEIGHT,
    };

    camera.0.translation = Vec2::new(CAMERA_VIEWPORT_WIDTH / 2.0, CAMERA_VIEWPORT_HEIGHT / 2.0)
        .extend(camera.0.translation.z);
    let texture = asset_server.load("sprites/lander.png");

    let layout = TextureAtlasLayout::from_grid(LANDER_SIZE, 3, 1, None, None);

    let layout_handle = layouts.add(layout);

    commands
        .spawn((
            DespawnOnExit(GameState::Game),
            Player,
            Grounded(false),
            ScoreMultiplier(1.0),
            RigidBody::Dynamic,
            CollisionEventsEnabled,
            Collider::rectangle(LANDER_SIZE.x as f32, LANDER_SIZE.y as f32),
            Mass(DRY_LANDER_MASS + (MAX_FUEL as f32 * FUEL_MASS_FACTOR)),
            Sprite::from_atlas_image(
                texture,
                TextureAtlas {
                    layout: layout_handle,
                    index: 0,
                },
            ),
            PlayerState::Idle,
            Fuel(MAX_FUEL),
            Transform {
                rotation: Quat::from_rotation_z(PI / 2.0),
                translation: Vec3::new(0.0, 850.0, 0.0),
                ..Default::default()
            },
            LinearVelocity {
                0: Vec2::new(INITIAL_HORIZONTAL_SPEED, 0.0),
            },
        ))
        .observe(player_crash_observer);

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;

    let mut terrain_noise_generator: TerrainNoiseType = Noise::from(LayeredNoise::new(
        Normed::<f32>::default(),
        Persistence(CHUNK_NOISE_PERSISTENCE),
        FractalLayers {
            layer: Octave::<MixCellGradients<OrthoGrid, Smoothstep, QuickGradients>>::default(),
            lacunarity: CHUNK_NOISE_LACUNARITY,
            amount: CHUNK_NOISE_LAYERS,
        },
    ));
    terrain_noise_generator.set_seed(seed);
    //noise_generator.set_period(CHUNK_NOISE_PERIOD);
    terrain_noise_generator.set_frequency(CHUNK_NOISE_FREQUENCY);

    commands.insert_resource(TerrainNoiseGenerator(terrain_noise_generator));

    let terrain_material = materials.add(Color::WHITE);

    commands.insert_resource(TerrainMaterial(terrain_material));

    /*let ground_points: Vec<Vec2> = (0..800)
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
        Collider::polyline(ground_points, None), // TODO: should use heightfield or similar for performance
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

    // TODO: this works, but terrain should be infinite and generated as the player moves
    commands.spawn((
        DespawnOnExit(GameState::Game),
        Ground,
        RigidBody::Static,
        Collider::compound(vec![
            (Vec2::new(0.0, 0.0), 0.0, Collider::half_space(Vec2::X)),
            (Vec2::new(1600.0, 0.0), 0.0, Collider::half_space(-Vec2::X)),
            (Vec2::new(0.0, 0.0), 0.0, Collider::half_space(Vec2::Y)),
            (Vec2::new(0.0, 900.0), 0.0, Collider::half_space(-Vec2::Y)),
        ]),
    ));*/

    commands.spawn((
        DespawnOnExit(GameState::Game),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            column_gap: Val::Px(5.0),
            ..Default::default()
        },
        children![
            (
                HudText::TimePassed,
                Text::new("TIME PASSED: 0.0 s"),
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Right),
                TextFont {
                    font_size: 16.0,
                    font: font.clone(),
                    ..default()
                },
            ),
            (
                HudText::Fuel,
                Text::new("FUEL: 100"),
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Right),
                TextFont {
                    font_size: 16.0,
                    font: font.clone(),
                    ..default()
                },
            ),
            (
                HudText::XVelocity,
                Text::new("HORIZONTAL VELOCITY: 0.0 m/s"),
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Right),
                TextFont {
                    font_size: 16.0,
                    font: font.clone(),
                    ..default()
                },
            ),
            (
                HudText::YVelocity,
                Text::new("VERTICAL VELOCITY: 0.0 m/s"),
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Right),
                TextFont {
                    font_size: 16.0,
                    font: font.clone(),
                    ..default()
                },
            ),
        ],
    ));

    commands.insert_resource(WinTimer(Timer::from_seconds(
        WIN_TIMER_DURATION,
        TimerMode::Once,
    )));

    commands.insert_resource(TimePassed(Duration::ZERO));

    commands.insert_resource(GameSounds {
        thrust_sound: asset_server.load("sounds/engine.wav"),
        crash_sound: asset_server.load("sounds/explosion.wav"),
        landing_sound: asset_server.load("sounds/win.wav"),
    });

    commands.insert_resource(Gravity(GRAVITY));
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

    commands.remove_resource::<TimePassed>();

    commands.remove_resource::<TerrainNoiseGenerator>();

    commands.remove_resource::<TerrainMaterial>();

    commands.remove_resource::<GameSounds>();

    commands.insert_resource(Gravity(Vec2::NEG_Y * 9.81));
}

fn create_terrain_chunk(
    commands: &mut Commands,
    x_origin: f32,
    terrain_noise_generator: &TerrainNoiseGenerator,
    terrain_material: &Handle<ColorMaterial>,
    font: &Handle<Font>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let mut ground_heights: Vec<f32> = (0..=CHUNK_WIDTH as i32)
        .step_by(CHUNK_GRANULARITY as usize)
        .map(|x| {
            terrain_noise_generator
                .0
                .sample_for::<f32>(Vec2::new(x_origin + x as f32, 0.0))
                * CHUNK_HEIGHT_AMPLITUDE
                + CHUNK_BASE_HEIGHT
        })
        .collect();

    let seed = x_origin;
    let mut rng = StdRng::seed_from_u64(seed as u64);

    let mut land_pad: Option<Vec2> = None;

    const LAND_PAD_WINDOW: usize = (LAND_PAD_WIDTH / CHUNK_GRANULARITY) as usize;

    if rng.random_bool(0.7) {
        for i in 1..(ground_heights.len() - LAND_PAD_WINDOW) {
            let x_0 = i;
            let x_1 = i + LAND_PAD_WINDOW;

            if (ground_heights[x_0] - ground_heights[x_1]).abs() <= 4.0 {
                let pad_height = (ground_heights[x_0] + ground_heights[x_1]) / 2.0;
                for x in x_0..=x_1 {
                    ground_heights[x] = pad_height;
                }
                let pad_x = (x_0 as f32 + x_1 as f32) * CHUNK_GRANULARITY as f32 / 2.0;
                land_pad = Some(Vec2::new(pad_x, pad_height));
                break;
            }
        }
    }

    let ground_points: Vec<Vec2> = ground_heights
        .iter()
        .enumerate()
        .map(|(x, &height)| Vec2::new((x * CHUNK_GRANULARITY as usize) as f32, height))
        .collect();

    let ground_mesh = meshes.add(Polyline2d::new(ground_points.clone()));

    let mut chunk = commands.spawn((
        DespawnOnExit(GameState::Game),
        Ground,
        TerrainChunk { x_origin },
        RigidBody::Static,
        //Collider::heightfield(ground_heights, Vec2::new(1.0, 1.0)),
        Collider::polyline(ground_points, None), // TODO: should use heightfield or similar for performance
        Mesh2d(ground_mesh),
        MeshMaterial2d(terrain_material.clone()),
        Transform::from_translation(Vec3::new(x_origin, 0.0, 0.0)),
    ));

    if let Some(pad_pos) = land_pad {
        chunk.with_children(|parent| {
            parent
                .spawn((
                    LandPad {
                        score_multiplier: 3.0,
                    },
                    RigidBody::Static,
                    Sensor,
                    CollisionEventsEnabled,
                    Collider::rectangle(LAND_PAD_WIDTH as f32, 16.0),
                    Transform::from_translation(Vec3::new(pad_pos.x, pad_pos.y + 8.0, 0.0)),
                    Visibility::default(),
                ))
                .observe(player_entered_landing_zone)
                .observe(player_exited_landing_zone)
                .with_child((
                    Text2d::new(format!("x{:.1}", 3.0)),
                    TextFont {
                        font_size: 14.0,
                        font: font.clone(),
                        ..default()
                    },
                    TextLayout::new_with_justify(Justify::Center),
                    TextColor(Color::WHITE),
                    Transform::from_translation(Vec3::new(0.0, 16.0, 0.0)),
                ));
        });
    }
}

fn terrain_chunk_system(
    mut commands: Commands,
    player: Single<&Transform, With<Player>>,
    existing_chunks: Query<(Entity, &TerrainChunk)>,
    terrain_noise_generator: Res<TerrainNoiseGenerator>,
    terrain_material: Res<TerrainMaterial>,
    font: Res<MainFont>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let player_x = player.translation.x;
    let current_chunk_x_origin: i32 = ((player_x / CHUNK_WIDTH).floor() * CHUNK_WIDTH) as i32;

    const CHUNKS_IN_CAMERA_VIEWPORT: i32 = (CAMERA_VIEWPORT_WIDTH / CHUNK_WIDTH).ceil() as i32 + 2; // +2 for buffer on each side

    let needed_chunk_origins: Vec<i32> = ((-CHUNK_BUFFER_OUTSIDE_VIEWPORT_COUNT
        - CHUNKS_IN_CAMERA_VIEWPORT / 2)
        ..(CHUNKS_IN_CAMERA_VIEWPORT / 2 + CHUNK_BUFFER_OUTSIDE_VIEWPORT_COUNT))
        .map(|i| current_chunk_x_origin + (i * CHUNK_WIDTH as i32))
        .collect::<Vec<i32>>();

    // Remove chunks that are no longer needed
    for (entity, chunk) in existing_chunks.iter() {
        let chunk_x_origin_i32 = chunk.x_origin as i32;
        if !needed_chunk_origins.contains(&chunk_x_origin_i32) {
            commands.entity(entity).despawn();
        }
    }

    let exisitng_chunk_origins: Vec<i32> = existing_chunks
        .iter()
        .map(|(_, chunk)| chunk.x_origin as i32)
        .collect::<Vec<i32>>();

    let chunks_to_add: Vec<f32> = needed_chunk_origins
        .iter()
        .cloned()
        .filter_map(|x_origin| {
            if !exisitng_chunk_origins.contains(&x_origin) {
                Some(x_origin as f32)
            } else {
                None
            }
        })
        .collect::<Vec<f32>>();

    for x_origin in chunks_to_add {
        create_terrain_chunk(
            &mut commands,
            x_origin,
            &terrain_noise_generator,
            &terrain_material.0,
            &font.0,
            &mut meshes,
        );
    }
}

fn camera_follow_system(
    player: Single<&Transform, With<Player>>,
    mut camera: Single<(&mut Transform, &Projection), (With<Camera>, Without<Player>)>,
    window: Single<&Window>,
) {
    let Projection::Orthographic(perspective) = camera.1 else {
        return;
    };

    let viewport_size = Vec2::new(window.width(), window.height()) * perspective.scale;

    let center = camera.0.translation.truncate();
    let quarter_size = viewport_size / 4.0;

    let min = center - quarter_size;
    let max = center + quarter_size;

    if player.translation.x < min.x {
        camera.0.translation.x = player.translation.x + quarter_size.x;
    } else if player.translation.x > max.x {
        camera.0.translation.x = player.translation.x - quarter_size.x;
    }
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
        player.1.apply_angular_acceleration(ROTATION_THRUST);
    }
    if keyboard_input.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        player.1.apply_angular_acceleration(-ROTATION_THRUST);
    }

    if player.3.0 > 0 {
        if keyboard_input.just_pressed(KeyCode::Space) {
            *player.2 = PlayerState::Firing;
        }
        if keyboard_input.pressed(KeyCode::Space) {
            let force_vector = (player.0.rotation * Vec3::Y * THRUST).truncate();

            player.1.apply_force(force_vector);
            player.3.0 = player.3.0.saturating_sub(FUEL_CONSUMPTION_RATE);
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

fn audio_system(
    mut commands: Commands,
    player: Single<&PlayerState, (With<Player>, Changed<PlayerState>)>,
    game_sounds: Res<GameSounds>,
    sounds_query: Query<(Entity, &AudioSink, &GameSound)>,
) {
    match *player {
        PlayerState::Firing => {
            commands.spawn((
                DespawnOnExit(GamePhase::Running),
                GameSound::Thrust,
                AudioPlayer::new(game_sounds.thrust_sound.clone()),
                PlaybackSettings::LOOP,
            ));
        }
        _ => {
            for (entity, sink, sound) in &sounds_query {
                match sound {
                    GameSound::Thrust => {
                        sink.stop();
                        commands.entity(entity).despawn();
                    }
                    _ => {}
                }
            }
        }
    }
}

fn playtime_system(time: Res<Time>, mut time_passed: ResMut<TimePassed>) {
    time_passed.0 += time.delta();
}

fn hud_system(
    player: Single<(&LinearVelocity, &Fuel), With<Player>>,
    time_passed: Res<TimePassed>,
    mut texts_query: Query<(&HudText, &mut Text)>,
) {
    for (kind, mut text) in &mut texts_query {
        match kind {
            HudText::Fuel => {
                text.0 = format!("FUEL: {}", player.1.0);
            }
            HudText::XVelocity => {
                let horizontal_velocity = player.0.0.x;
                text.0 = format!("HORIZONTAL VELOCITY: {:.1} m/s", horizontal_velocity);
            }
            HudText::YVelocity => {
                let vertical_velocity = player.0.0.y;
                text.0 = format!("VERTICAL VELOCITY: {:.1} m/s", vertical_velocity);
            }
            HudText::TimePassed => {
                let total_secs = time_passed.0.as_secs();
                let minutes = total_secs / 60;
                let seconds = total_secs % 60;
                text.0 = format!("TIME PASSED: {:02}:{:02}", minutes, seconds);
            }
        }
    }
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

    if impact_impulse_magnitude > SAFE_LANDING_IMPULSE_MAGNITUDE {
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

fn fuel_weight_system(mut player: Single<(&mut Mass, &Fuel), (With<Player>, Changed<Fuel>)>) {
    let empty_mass = DRY_LANDER_MASS;
    let fuel_mass = player.1.0 as f32 * FUEL_MASS_FACTOR;
    player.0.0 = empty_mass + fuel_mass;
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
    font: Res<MainFont>,
    game_sounds: Res<GameSounds>,
) {
    let font = &font.0;

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
                font: font.clone(),
                ..default()
            },
        )],
    ));

    commands.spawn((
        DespawnOnExit(GamePhase::Lose),
        GameSound::Crash,
        AudioPlayer::new(game_sounds.crash_sound.clone()),
        PlaybackSettings::DESPAWN,
    ));
}

fn cleanup_lose_screen(mut _commands: Commands) {}

fn setup_win_screen(
    mut commands: Commands,
    player: Single<&ScoreMultiplier, With<Player>>,
    font: Res<MainFont>,
    game_sounds: Res<GameSounds>,
) {
    let font = &font.0;

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
                font: font.clone(),
                ..default()
            },
        )],
    ));

    commands.spawn((
        DespawnOnExit(GamePhase::Win),
        GameSound::Landing,
        AudioPlayer::new(game_sounds.landing_sound.clone()),
        PlaybackSettings::DESPAWN,
    ));
}

fn cleanup_win_screen(mut _commands: Commands) {}

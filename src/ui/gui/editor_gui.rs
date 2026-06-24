use crate::{
    physics::{predictions::Prediction, time::SIMTICKS_PER_TICK},
    prelude::*,
    ui::{
        gui::SelectionRadius,
        screen::editor::{
            editor_backend::{
                ChangeNodeTick, ChangePredictionsNumber, ConfirmThrust, NumberOfPredictions,
                ReloadPredictions, TempPrediction, UpdateThrust,
            },
            ClearOnEditorExit, EditorContext, EditorEvents,
        },
        widget::space_map::SpaceMap,
        RenderSet,
    },
    utils::algebra::relative_axes,
};
use bevy::{
    color::palettes::css::{BLUE, DARK_BLUE, DARK_GREEN, DARK_RED, GREEN, ORANGE, RED, WHITE},
    input::{
        common_conditions::{input_just_released, input_pressed},
        mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        ButtonState,
    },
    prelude::*,
    window::PrimaryWindow,
};

use super::{draw_all_ships_predictions, draw_gizmos, AdaptiveTranslation, SelectObjectEvent, MAX_HEIGHT};

const GIZMO_COLORS: [Color; 6] = [
    Color::Srgba(RED),
    Color::Srgba(DARK_RED),
    Color::Srgba(GREEN),
    Color::Srgba(DARK_GREEN),
    Color::Srgba(BLUE),
    Color::Srgba(DARK_BLUE),
];

pub fn plugin(app: &mut App) {
    app.init_resource::<CurrentGizmo>()
        .add_systems(
            PostUpdate,
            (draw_predictions, draw_maneuver_node)
                .in_set(RenderSet)
                .after(draw_all_ships_predictions)
                .after(draw_gizmos)
                .run_if(resource_exists::<EditorContext>),
        )
        .add_systems(
            Update,
            (despawn_arrows, spawn_arrows)
                .chain()
                .after(EventHandling)
                .run_if(on_event::<EditorEvents>()),
        )
        .add_systems(
            Update,
            (
                handle_drag_gizmo
                    .run_if(input_pressed(MouseButton::Left).and_then(on_event::<MouseMotion>())),
                handle_click_gizmo.run_if(on_event::<SelectObjectEvent>()),
                handle_release_gizmo.run_if(input_just_released(MouseButton::Left)),
                (despawn_arrows, spawn_arrows)
                    .chain()
                    .run_if(on_event::<ReloadPredictions>()),
            )
                .run_if(resource_exists::<EditorContext>),
        )
        .add_systems(
            PreUpdate,
            (
                send_change_predictions_number.run_if(input_pressed(KeyCode::ShiftLeft)),
                send_change_node_tick.run_if(input_pressed(KeyCode::ControlLeft)),
            )
                .run_if(resource_exists::<EditorContext>.and_then(on_event::<MouseWheel>())),
        );
}
#[derive(Resource, PartialEq, Default)]
/// Contains the direction of the gizmo (unit 3vector) and the initial position of the mouse
pub(super) struct CurrentGizmo(pub Option<GizmoDraggingState>);

#[derive(PartialEq)]
pub(super) struct GizmoDraggingState {
    gizmo: Entity,
    direction: Vec3,
    initial_mouse_pos: Vec2,
}

#[derive(Component, Clone, Debug)]
pub(super) struct ArrowGizmo {
    pub color: Color,
    /// The unit vector pointing in the direction of the arrow, in absolute coordinates
    pub global_direction: Vec3,
    /// The relative coordinates of the gizmo (should be Vec3::X or similar)
    pub local_direction: Vec3,
}

fn draw_predictions(
    mut gizmos: Gizmos,
    predictions: Query<(&Transform, &Prediction, Option<&TempPrediction>)>,
    space_map: Res<SpaceMap>,
    predictions_number: Res<NumberOfPredictions>,
) {
    for (t, p, temp) in predictions.iter() {
        if p.index % SIMTICKS_PER_TICK as usize != 0 {
            continue;
        }
        let color = if temp.is_some() {
            Color::Srgba(ORANGE).with_alpha(0.5)
        } else {
            Color::Srgba(BLUE).with_alpha(0.8)
        };
        gizmos.circle_2d(
            t.translation.xy(),
            (1. - p.index as f32 / predictions_number.0 as f32) * MAX_HEIGHT
                / (500. * space_map.zoom_level as f32),
            color,
        );
    }
}

fn spawn_arrows(
    context: Res<EditorContext>,
    space_map: Res<SpaceMap>,
    mut commands: Commands,
    positions: Query<&Transform>,
    speeds: Query<&Velocity>,
) {
    if let Some(e) = context.selected_prediction_entity() {
        let scale = MAX_HEIGHT as f64 / space_map.system_size;
        let speed = (speeds.get(e).unwrap().0 * scale).as_vec3();
        let pos = positions.get(e).unwrap().translation;
        let focus = space_map
            .focus_body
            .map_or(Vec3::ZERO, |e| positions.get(e).unwrap().translation);
        let [forward, right, down] = relative_axes(pos - focus, speed);
        let directions = [forward, -forward, right, -right, down, -down];
        let local_directions = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ];
        commands
            .spawn(TransformBundle::from_transform(
                Transform::from_translation(pos),
            ))
            .with_children(|parent| {
                let radius = MAX_HEIGHT / 30.;
                for (i, d) in directions.iter().zip(local_directions).enumerate() {
                    let global_direction = d.0.as_vec3();
                    let local_direction = d.1;
                    parent.spawn((
                        ArrowGizmo {
                            color: GIZMO_COLORS[i],
                            global_direction,
                            local_direction,
                        },
                        ClearOnEditorExit,
                        SelectionRadius {
                            min_radius: 1.5 * radius,
                            ..default()
                        },
                        TransformBundle::default(),
                        AdaptiveTranslation(global_direction * 2.5 * radius),
                    ));
                }
            });
    }
}

fn despawn_arrows(mut commands: Commands, entities: Query<Entity, With<ArrowGizmo>>) {
    entities.iter().for_each(|e| commands.entity(e).despawn());
}

fn draw_maneuver_node(
    mut gizmos: Gizmos,
    space_map: Res<SpaceMap>,
    context: Res<EditorContext>,
    positions: Query<&Transform>,
    current_gizmo: Res<CurrentGizmo>,
    arrows: Query<(Entity, &ArrowGizmo, &GlobalTransform)>,
) {
    if let Some(e) = context.selected_prediction_entity() {
        let pos = positions.get(e).unwrap().translation;
        let size = MAX_HEIGHT / (30. * space_map.zoom_level as f32);
        gizmos.circle_2d(pos.xy(), size, WHITE);
        for (gizmo, arrow, arrow_pos) in arrows.iter() {
            let mut color = arrow.color;
            if current_gizmo.0.as_ref().is_some_and(|g| g.gizmo == gizmo) {
                color = color.mix(&Color::WHITE, 0.5);
            }
            let pos = arrow_pos.translation();
            gizmos.arrow(
                pos - 1.5 * size * arrow.global_direction,
                pos + 0.5 * size * arrow.global_direction,
                color,
            );
        }
    }
}

fn handle_click_gizmo(
    mut select_event: EventReader<SelectObjectEvent>,
    arrows: Query<&ArrowGizmo>,
    mut current_gizmo: ResMut<CurrentGizmo>,
) {
    for event in select_event.read() {
        if let Ok(arrow) = arrows.get(event.entity) {
            current_gizmo.0 = Some(GizmoDraggingState {
                gizmo: event.entity,
                direction: arrow.local_direction,
                initial_mouse_pos: event.cursor_pos,
            })
        }
    }
}

fn handle_release_gizmo(
    mut input: EventReader<MouseButtonInput>,
    mut current_gizmo: ResMut<CurrentGizmo>,
    mut confirm_thrust: EventWriter<ConfirmThrust>,
) {
    for event in input.read() {
        if matches!(event.state, ButtonState::Released) && current_gizmo.0.is_some() {
            current_gizmo.0 = None;
            confirm_thrust.send(ConfirmThrust);
        }
    }
}

fn handle_drag_gizmo(
    window: Query<&Window, With<PrimaryWindow>>,
    current_gizmo: Res<CurrentGizmo>,
    space_map: Res<SpaceMap>,
    mut update_thrust: EventWriter<UpdateThrust>,
) {
    if let Some(cursor_pos) = window.single().cursor_position() {
        if let Some(GizmoDraggingState {
            direction,
            initial_mouse_pos,
            ..
        }) = current_gizmo.0
        {
            let zoom = space_map.zoom_level;
            update_thrust.send(UpdateThrust(
                direction.as_dvec3() * (cursor_pos - initial_mouse_pos).length() as f64 * 1e6
                    / zoom,
            ));
        }
    }
}

fn send_change_predictions_number(
    mut events: EventWriter<ChangePredictionsNumber>,
    mut scroll: EventReader<MouseWheel>,
) {
    for event in scroll.read() {
        let is_step = match event.unit {
            MouseScrollUnit::Line => true,
            MouseScrollUnit::Pixel => false,
        };
        events.send(ChangePredictionsNumber {
            is_step,
            amount: event.y,
        });
    }
}

fn send_change_node_tick(
    mut events: EventWriter<ChangeNodeTick>,
    mut scroll: EventReader<MouseWheel>,
) {
    for event in scroll.read() {
        let is_step = match event.unit {
            MouseScrollUnit::Line => true,
            MouseScrollUnit::Pixel => false,
        };
        events.send(ChangeNodeTick {
            is_step,
            amount: event.y,
        });
    }
}

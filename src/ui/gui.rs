use bevy::{
    color::palettes::css::{BLACK, DARK_GRAY, GOLD, GREEN, TEAL},
    core_pipeline::bloom::BloomSettings,
    input::{
        common_conditions::input_pressed,
        mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        ButtonState,
    },
    math::{DVec2, DVec3},
    prelude::*,
    render::camera::ScalingMode,
    window::PrimaryWindow,
};

use crate::{
    objects::{
        orbiting_obj::{OrbitalObjID, OrbitingObjects},
        // ships::HostBody,
    }, physics::{influence::HillRadius, orbit::SystemSize}, prelude::*, utils::{
        algebra::{center_to_periapsis_direction, ellipse_half_sizes},
        ui::EllipseBuilder,
    }
};

use self::editor_gui::CurrentGizmo;

use super::{
    widget::space_map::{SpaceMap, ZOOM_STEP},
    RenderSet, UiUpdate,
};

pub mod editor_gui;

pub const MAX_HEIGHT: f32 = 100000.;
const MIN_RADIUS: f32 = 1e-4;
const SCROLL_SENSITIVITY: f32 = 10.;


use std::fs::OpenOptions;
use std::io::Write;

pub fn debug_to_file<T: std::fmt::Display>(msg: &str, value: T) {
    let mut file = OpenOptions::new()
        .create(true)             
        .append(true)             
        .open("logs.txt")           
        .unwrap();

    let full_msg = format!("{} - value: {}", msg, value);
    writeln!(file, "{}", full_msg).unwrap();
} 

pub struct GuiPlugin;

impl Plugin for GuiPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "debug_display")]
        app.init_resource::<DebugDisplay>();
        app.add_plugins(editor_gui::plugin)
            .insert_resource(ClearColor(Color::Srgba(BLACK)))
            .add_event::<SelectObjectEvent>()
            .add_systems(Startup, (camera_setup, color_setup))
            .add_systems(
                OnEnter(Loaded),
                (insert_display_components, update_transform)
                    .chain()
                    .in_set(GUIUpdate),
            )
            .add_systems(
                PostUpdate,
                (
                    (update_transform, update_camera_pos)
                        .chain()
                        .in_set(UiUpdate),
                    draw_gizmos.in_set(RenderSet),
                    (debug_print, draw_selection_spheres).run_if(resource_exists::<DebugDisplay>),
                )
                    .run_if(resource_exists::<SpaceMap>)
                    .run_if(in_state(Loaded)),
            )
            .add_systems(
                Update,
                (
                    zoom_with_scroll.run_if(
                        on_event::<MouseWheel>().and_then(not(input_pressed(KeyCode::ShiftLeft)
                            .or_else(input_pressed(KeyCode::ControlLeft)))),
                    ),
                    (adaptive_scale, adaptive_translation)
                        .after(zoom_with_scroll)
                        .after(EventHandling),
                    pan_when_dragging.run_if(
                        input_pressed(MouseButton::Left)
                            .and_then(resource_exists_and_equals(CurrentGizmo(None))),
                    ),
                )
                    .run_if(resource_exists::<SpaceMap>),
            )
            .add_systems(
                PreUpdate,
                send_select_object_event
                    .run_if(on_event::<MouseButtonInput>().and_then(resource_exists::<SpaceMap>)),
            );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct GUIUpdate;

#[derive(Resource, Default)]
struct DebugDisplay;

#[derive(Event)]
pub struct SelectObjectEvent {
    pub entity: Entity,
    pub cursor_pos: Vec2,
}

#[derive(Component)]
pub struct AdaptiveScaling(f32);

#[derive(Component)]
pub struct AdaptiveTranslation(Vec3);

#[derive(Component, Copy, Clone, Debug, Default)]
/// A selectable object. The actual radius is the radius of the object in transform coordinates,
/// but since it can seem too small when zooming out, we can provide a minimum radius that is independent of zoom level
pub struct SelectionRadius {
    pub min_radius: f32,
    pub actual_radius: f32,
}

impl SelectionRadius {
    pub fn radius(&self, zoom_level: f64) -> f32 {
        self.actual_radius.max(self.min_radius / zoom_level as f32)
    }
}

#[derive(Resource)]
pub struct Colors {
    stars: Handle<StandardMaterial>,
    planets: Handle<StandardMaterial>,
    other: Handle<StandardMaterial>,
}

pub fn camera_setup(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle {
            camera: Camera {
                hdr: true,
                ..default()
            },
            transform: Transform::from_xyz(0., 0., MAX_HEIGHT).looking_at(Vec3::ZERO, Vec3::Y),
            projection: Projection::Orthographic(OrthographicProjection {
                far: 2. * MAX_HEIGHT,
                scaling_mode: ScalingMode::FixedVertical(MAX_HEIGHT),
                ..default()
            }),
            ..default()
        },
        BloomSettings::NATURAL,
    ));
}

pub fn color_setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let colors = Colors {
        stars: materials.add(StandardMaterial {
            base_color: Color::Srgba(GOLD),
            emissive: LinearRgba::from(GOLD) * 50.,
            ..default()
        }),
        planets: materials.add(Color::Srgba(TEAL)),
        other: materials.add(Color::Srgba(DARK_GRAY)),
    };
    commands.insert_resource(colors);
} 

fn insert_display_components(
    mut commands: Commands,
    bodies: Query<(Entity, &BodyInfo)>,
    ships: Query<Entity, With<ShipInfo>>,
    mut meshes: ResMut<Assets<Mesh>>,
    colors: Res<Colors>,
    system_size: Res<SystemSize>,
) {
    let scale = MAX_HEIGHT as f64 / system_size.0;

    bodies.iter().for_each(|(e, BodyInfo(data))| {
        let material = match data.body_type {
            BodyType::Star => colors.stars.clone(),
            BodyType::Planet => colors.planets.clone(),
            _ => colors.other.clone(),
        };
        commands.entity(e).insert((
            PbrBundle {
                mesh: meshes.add(
                    Sphere {
                        radius: MIN_RADIUS.max((data.radius * scale) as f32),
                    }
                    .mesh(),
                ),
                material,
                ..default()
            },
            SelectionRadius {
                min_radius: MAX_HEIGHT / 100.,
                actual_radius: (data.radius * scale) as f32,
            },
        ));
        if matches!(data.body_type, BodyType::Star) {
            commands.entity(e).with_children(|builder| {
                builder.spawn(PointLightBundle {
                    point_light: PointLight {
                        intensity: (data.radius * scale * 1e3).powi(3) as f32,
                        color: Color::WHITE,
                        shadows_enabled: true,
                        radius: (data.radius * scale) as f32,
                        range: MAX_HEIGHT,
                        ..default()
                    },
                    ..default()
                });
            });
        }
    });
    for e in ships.iter() {
        commands.entity(e).insert(TransformBundle::default());
    }
}

fn adaptive_scale(mut query: Query<(&mut Transform, &AdaptiveScaling)>, space_map: Res<SpaceMap>) {
    query
        .par_iter_mut()
        .for_each(|(mut t, s)| t.scale = Vec3::ONE * s.0 * space_map.zoom_level as f32)
}

fn adaptive_translation(
    mut query: Query<(&mut Transform, &AdaptiveTranslation)>,
    space_map: Res<SpaceMap>,
) {
    query
        .par_iter_mut()
        .for_each(|(mut t, init)| t.translation = init.0 / space_map.zoom_level as f32)
}

fn zoom_with_scroll(mut events: EventReader<MouseWheel>, mut space_map: ResMut<SpaceMap>) {
    for event in events.read() {
        space_map.zoom_level *= ZOOM_STEP.powf(match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y * SCROLL_SENSITIVITY,
        } as f64);
    }
}

fn pan_when_dragging(mut motions: EventReader<MouseMotion>, mut map: ResMut<SpaceMap>) {
    for event in motions.read() {
        let scale = map.system_size / (500. * map.zoom_level);
        map.offset_amount += scale * event.delta.as_dvec2() * DVec2::new(-1., 1.);
    }
}

fn send_select_object_event(
    mut clicks: EventReader<MouseButtonInput>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut writer: EventWriter<SelectObjectEvent>,
    objects: Query<(Entity, &GlobalTransform, &SelectionRadius)>,
    map: Res<SpaceMap>,
) {
    let Ok((cam, cam_transform)) = cam.get_single() else {
        return;
    };
    for event in clicks.read() {
        if matches!(
            (event.state, event.button),
            (ButtonState::Pressed, MouseButton::Left)
        ) {
            if let Some(cursor_pos) = window.single().cursor_position() {
                if let Some(translation) = cam.viewport_to_world_2d(cam_transform, cursor_pos) {
                    objects
                        .iter()
                        .find(|(_, pos, rad)| {
                            (pos.translation().xy() - translation).length()
                                < rad.radius(map.zoom_level)
                        })
                        .map(|(entity, _, _)| {
                            writer.send(SelectObjectEvent { entity, cursor_pos })
                        });
                }
            }
        }
    }
}

fn update_camera_pos(
    space_map: Res<SpaceMap>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
    positions: Query<&Position>,
) {
    let scale = MAX_HEIGHT as f64 / space_map.system_size;
    let Ok((mut cam_pos, mut proj)) = cam.get_single_mut() else {
        return;
    };
    let focus_pos = space_map
        .focus_body
        .map_or(DVec3::default(), |f| positions.get(f).unwrap().0);
    cam_pos.translation = ((focus_pos
        + DVec3::new(space_map.offset_amount.x, space_map.offset_amount.y, 0.))
        * scale)
        .as_vec3()
        + MAX_HEIGHT * Vec3::Z;
    if let Projection::Orthographic(ortho) = proj.as_mut() {
        ortho.scale = (1. / space_map.zoom_level) as f32;
    }
}

fn update_transform(system_size: Res<SystemSize>, mut query: Query<(&mut Transform, &Position)>) {
    let scale = MAX_HEIGHT as f64 / system_size.0;
    for (mut transform, Position(pos)) in query.iter_mut() {
        transform.translation = (*pos * scale).as_vec3();
    }
}

#[allow(non_snake_case)]
#[allow(clippy::too_many_arguments)]
fn draw_gizmos(
    space_map: Res<SpaceMap>,
    mut gizmos: Gizmos,
    bodies: Query<(
        &Transform,
        &Velocity,
        &BodyInfo,
        &OrbitingObjects,
        &HillRadius,
    ), With<BodyInfo>>,
    influence_query: Query<(&Transform, &HillRadius)>,
    orbit_query: Query<&EllipticalOrbit>,
    // orbital_ships: Query<(&Transform, &Velocity, &EllipticalOrbit, &HostBody), With<ShipInfo>>,
    ships: Query<(&Transform, &Velocity, &Influenced), With<ShipInfo>>,
    bodies_mapping: Res<BodiesMapping>,
    ships_mapping: Res<ShipsMapping>,
) {
    let scale = MAX_HEIGHT as f64 / space_map.system_size;
    if let &SpaceMap {
        zoom_level,
        selected: Some(s),
        ..
    } = space_map.as_ref()
    {
        if let Ok((pos, _, info, orbiting_obj, _)) = bodies.get(s) {
            // Display selection circle
            gizmos.circle_2d(
                pos.translation.xy(),
                (MAX_HEIGHT as f64 / (100. * zoom_level))
                    .max(info.0.radius * scale + MAX_HEIGHT as f64 / (70. * zoom_level))
                    as f32,
                Color::srgba(1., 1., 1., 0.1),
            );

            // Display children orbits
            let parent_translation = pos.translation;
            for &obj in orbiting_obj
                .0
                .iter()
                .filter_map(|obj_id| {
                    match obj_id {
                        OrbitalObjID::Body(body_id) => {bodies_mapping.0.get(body_id)},
                        OrbitalObjID::Ship(ship_id) => {ships_mapping.0.get(ship_id)},
                    }
                })
            {
                if let Ok(&EllipticalOrbit {
                    semimajor_axis: a,
                    inclination: I,
                    long_asc_node: O,
                    arg_periapsis: o,
                    eccentricity: e,
                    eccentric_anomaly: E,
                    revolution_period,
                    ..
                }) = orbit_query.get(obj) 
                {
                    let (o, O, I, E) = (
                    o.to_radians(),
                    O.to_radians(),
                    I.to_radians(),
                    E.to_radians(),
                    );
                    let peri = (1. - e) * a;
                    // if let Some(radius) = viewable_radius(cam) {
                    //     let distance_to_parent = (cam_pos.translation() - parent_translation).length();
                    //     if distance_to_parent + radius < (peri * scale) as f32
                    //         || distance_to_parent - radius > (apo * scale) as f32
                    //     {
                    //         continue;
                    //     }
                    // }
                    let position =
                        (scale * (peri - a) * center_to_periapsis_direction(o, O, I).normalize())
                            .as_vec3()
                            + parent_translation;
                        
                    let resolution = ((zoom_level * 100.) as usize).min(1000);
                    EllipseBuilder {
                        position,
                        rotation: Quat::from_rotation_z(O as f32)
                            * Quat::from_rotation_x(I as f32)
                            * Quat::from_rotation_z(o as f32),
                        half_size: (ellipse_half_sizes(a, e) * scale).as_vec2(),
                        color: Color::WHITE.with_alpha(0.1),
                        resolution,
                        initial_angle: E as f32,
                        sign: -revolution_period.signum() as f32,
                    }
                    .draw(&mut gizmos);
                }       
            }
            // Display sphere of influence
            for (pos, radius) in influence_query.iter() {
                gizmos.circle_2d(
                    pos.translation.xy(),
                    (radius.0 * scale) as f32,
                    Color::srgba(1., 0.1, 0.1, 0.1),
                );
            }
            // // Display orbital ships
            // for (transform, speed, _, HostBody(host_body_id)) in orbital_ships.iter() {
            //     let host_body = bodies_mapping.0.get(host_body_id).unwrap();
            //     let ref_speed = bodies.get(*host_body).unwrap().1 .0;
            //     let speed = ((speed.0 - ref_speed).normalize_or(DVec3::X) * MAX_HEIGHT as f64 
            //         / (30. * zoom_level))
            //         .xy()
            //         .as_vec2();
            //     let t = transform.translation.xy() - speed / 3.;
            //     let perp = speed.perp() / 3.;
            //     gizmos.linestrip_2d(
            //         [t + speed, t + perp, t - perp, t + speed],
            //         Color::Srgba(RED),
            //     );
            // }
            // Display ships
            for (t, speed, influence) in ships.iter() {
                let ref_speed = influence
                    .main_influencer
                    .map_or(DVec3::ZERO, |e| bodies.get(e).unwrap().1 .0);
                let speed = ((speed.0 - ref_speed).normalize_or(DVec3::X) * MAX_HEIGHT as f64
                    / (30. * zoom_level))
                    .xy()
                    .as_vec2();
                let t = t.translation.xy() - speed / 3.;
                let perp = speed.perp() / 3.;
                gizmos.linestrip_2d(
                    [t + speed, t + perp, t - perp, t + speed],
                    Color::Srgba(GOLD),
                );
            }
        }
    }
}

fn debug_print(
    mut keys: EventReader<bevy_ratatui::event::KeyEvent>,
    influence: Query<&Influenced>,
) {
    for event in keys.read() {
        if event.code == crossterm::event::KeyCode::Char('p') {
            influence.iter().for_each(|i| eprintln!("{:?}", i));
        }
    }
}

fn draw_selection_spheres(
    mut gizmos: Gizmos,
    spheres: Query<(&SelectionRadius, &GlobalTransform)>,
    space_map: Res<SpaceMap>,
) {
    spheres.iter().for_each(|(r, pos)| {
        gizmos.circle_2d(
            pos.translation().xy(),
            r.radius(space_map.zoom_level),
            GREEN,
        );
    });
}

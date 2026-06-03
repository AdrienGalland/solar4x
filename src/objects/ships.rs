//! A "Ship" is an object whose movement is governed by the gravitationnal
//! attraction of the celestial bodies, along with custom trajectories

use arrayvec::ArrayString;
use bevy::{math::DVec3, prelude::*, utils::HashMap};
use std::f64::consts::PI;

use crate::game::{ClearOnUnload, Loaded};
use crate::physics::influence::{HillRadius};
use crate::physics::{leapfrog::get_acceleration, G};
use crate::physics::prelude::*;
use crate::objects::{
    orbiting_obj::{OrbitingObjects, OrbitalObjID},
    bodies::BodyID,
};

use super::id::MAX_ID_LENGTH;
use super::prelude::{BodiesMapping, BodyInfo, PrimaryBody};
use super::ObjectsUpdate;

pub mod trajectory;
pub mod scheduler;

// pub(crate) struct ShipID(u64);

// #[derive(Resource, Default)]
// struct ShipIDBuilder(NumberIncrementer);

// impl IDBuilder for ShipIDBuilder {
//     type ID = ShipID;

//     fn incrementer(&mut self) -> &mut NumberIncrementer {
//         &mut self.0
//     }

//     fn id_from_u64(u: u64) -> Self::ID {
//         ShipID(u)
//     }
// }
pub struct ShipsPlugin;

impl Plugin for ShipsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(trajectory::plugin)
            .add_event::<ShipEvent>()
            .add_systems(Update, 
                (
                    handle_ship_create,
                    handle_ship_remove,
                    // handle_switch_to_orbital,
                    // handle_switch_to_free_motion,
                ).in_set(ObjectsUpdate))
            .add_systems(OnEnter(Loaded), create_ships.in_set(ObjectsUpdate));
            // .add_systems(
            //     PostUpdate,
            //     check_ship_orbits.run_if(|r: Option<Res<DisableShipOrbitCheck>>| {
            //         !r.is_some_and(|r| r.0)
            //     }),
            // );
    }
}
#[derive(Component)]
pub(crate) struct HostBody(pub BodyID);

pub type ShipID = ArrayString<MAX_ID_LENGTH>;

#[derive(Component, Clone, Default, PartialEq, Debug)]
pub struct ShipInfo {
    pub id: ShipID,
    pub spawn_pos: DVec3,
    pub spawn_speed: DVec3,
}

#[derive(Resource, Default)]
pub struct ShipsMapping(pub HashMap<ShipID, Entity>);

#[derive(Event, Debug)]
pub enum ShipEvent {
    Create(ShipInfo),
    Remove(ShipID),
    SwitchToOrbital{ship_id: ShipID, r_vec: DVec3, v_vec: DVec3, host_mass: Mass},
    SwitchToFreeMotion(ShipID),
}

fn create_ships(mut commands: Commands) {
    commands.insert_resource(ShipsMapping::default());
}

fn handle_ship_remove(
    mut reader: EventReader<ShipEvent>,
    mut commands: Commands,
    mut ships_mapping: ResMut<ShipsMapping>,
) {
    for event in reader.read() {
        if let ShipEvent::Remove(id) = event {
            if let Some(e) = ships_mapping.0.remove(id) {
                commands.entity(e).despawn();
            }
        }
    }      
}

fn handle_ship_create(
    mut reader: EventReader<ShipEvent>, 
    mut commands: Commands, 
    mut ships_mapping: ResMut<ShipsMapping>,
    query: Query<(&Position, &HillRadius, &OrbitingObjects)>,
    query_with_mass: Query<(&Position, &HillRadius, &OrbitingObjects, &Mass)>, 
    bodies_mapping:Res<BodiesMapping>,
    main_body: Query<&BodyInfo, With<PrimaryBody>>,
) {
    for event in reader.read() {
        if let ShipEvent::Create(info) = event {
            let pos = Position(info.spawn_pos);
            ships_mapping.0.entry(info.id).or_insert({
                let influence = Influenced::new(
                    &pos,
                    &query,
                    bodies_mapping.as_ref(),
                    main_body.single().0.id,
                );
                commands
                    .spawn((
                        info.clone(),
                        Acceleration::new(get_acceleration(
                            info.spawn_pos,
                            query_with_mass 
                                .iter_many(&influence.influencers)
                                .map(|(p, _, _, m)| (p.0, m.0)),
                        )),
                        influence,
                        pos,
                        Velocity(info.spawn_speed),
                        TransformBundle::from_transform(Transform::from_xyz(0., 0., 1.)),
                        ClearOnUnload,
                    ))
                    .id()
            
            });
        }
    }
}

fn handle_switch_to_orbital(
    mut reader: EventReader<ShipEvent>,
    mut commands: Commands,
    mut orbiting: Query<&mut OrbitingObjects>,
    query_influenced: Query<&Influenced>,
    bodies: Query<&BodyInfo>,
    ships_mapping: Res<ShipsMapping>,
    bodies_mapping: Res<BodiesMapping>,
) {
    for event in reader.read() {
        if let ShipEvent::SwitchToOrbital { 
                ship_id, 
                r_vec, 
                v_vec, 
                host_mass 
            } = event 
        {
            if let Some(ship) = ships_mapping.0.get(ship_id) {
                if query_influenced.get(*ship).is_err() {
                    continue;
                }
                let orbit = calc_elliptical_orbit(*r_vec, *v_vec, *host_mass);
                let orbiting_obj: OrbitingObjects = OrbitingObjects(Vec::new());
                let host_body_id = get_host_body(ship, &query_influenced, &bodies);
                let host_entity = bodies_mapping.0.get(&host_body_id).unwrap();
                let mut host_orbiting_obj = orbiting.get_mut(*host_entity).unwrap(); 
                host_orbiting_obj.0.push(OrbitalObjID::Ship(*ship_id));
                commands
                    .entity(*ship)
                    .insert((orbit, orbiting_obj.clone(), HostBody(host_body_id)));
                commands.entity(*ship).remove::<(Acceleration, Influenced)>();
            };
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_switch_to_free_motion(
    mut reader: EventReader<ShipEvent>,
    mut commands: Commands,
    ships_mapping: Res<ShipsMapping>,
    ship_positions :  Query<&Position, With<ShipInfo>>,
    query: Query<(&Position, &HillRadius, &OrbitingObjects)>,
    query_with_mass: Query<(&Position, &HillRadius, &OrbitingObjects, &Mass)>, 
    bodies_mapping:Res<BodiesMapping>,
    main_body: Query<&BodyInfo, With<PrimaryBody>>,
) {
    for event in reader.read() {
        if let ShipEvent::SwitchToFreeMotion(ship_id) = event {
            if let Some(ship) = ships_mapping.0.get(ship_id) {
                if let Ok(pos) = ship_positions.get(*ship) {

                    let influence = Influenced::new(
                    pos,
                    &query,
                    bodies_mapping.as_ref(),
                    main_body.single().0.id,
                    );
                    let acc = Acceleration::new(get_acceleration(
                            pos.0,
                            query_with_mass 
                                .iter_many(&influence.influencers)
                                .map(|(p, _, _, m)| (p.0, m.0)),
                    ));
                    commands.insert_resource(DisableShipOrbitCheck(true));
                    commands.entity(*ship).remove::<(HostBody, OrbitingObjects, EllipticalOrbit)>();
                    commands.entity(*ship).insert((acc, influence));
                }
            }
        }
    }
}

fn get_host_body(ship: &Entity, query_influenced: &Query<&Influenced>, bodies: &Query<&BodyInfo>) -> BodyID {
    let influenced = query_influenced.get(*ship).unwrap();
    let host_body = influenced.main_influencer.unwrap();
    bodies.get(host_body).unwrap().0.id
}

#[allow(non_snake_case)]
fn calc_elliptical_orbit(
    r_vec: DVec3, 
    v_vec: DVec3, 
    mass: Mass,
    ) -> EllipticalOrbit {
    let mu = G*mass.0;
    let v = v_vec.length();
    let r = r_vec.length();
    let h = r_vec.cross(v_vec);
    let e_vec = v_vec.cross(h)/mu - r_vec/r;
    let e = e_vec.length();
    let epsilon = v.powf(2.)/2. - mu/r;
    let semimajor_axis = -mu / (2. * epsilon);
    let inclination = (h.z/h.length()).acos();
    let n_vec = DVec3::new(-h.y, h.x, 0.);
    let mut long_asc_node;
    let mut arg_periapsis;
    if n_vec == DVec3::ZERO {
        long_asc_node = 0.;
        arg_periapsis = 0.;
    } else {
        long_asc_node = (n_vec.x/n_vec.length()).acos();
        if n_vec.y < 0. {
            long_asc_node = 2.*PI - long_asc_node;
        }
        arg_periapsis = (n_vec.dot(e_vec) / (n_vec.length() * e)).acos();
        if e_vec.z < 0. {
            arg_periapsis = 2.*PI - arg_periapsis;
        }
    }
    
    let mut true_anomaly = (e_vec.dot(r_vec) / (e * r)).acos();
    if r_vec.dot(v_vec) < 0.0 {
        true_anomaly = 2.0 * PI - true_anomaly;
    }
    let tan_half_E = ((1.0 - e) / (1.0 + e)).sqrt() * (true_anomaly / 2.0).tan();
    let E = 2.0 * tan_half_E.atan();

    let initial_mean_anomaly = E - e * E.sin();

    let revolution_period = 2.*PI*(semimajor_axis.powf(3.)/mu).powf(0.5);
    EllipticalOrbit {
        eccentricity: e,
        semimajor_axis, 
        inclination,
        long_asc_node,
        arg_periapsis,
        initial_mean_anomaly,
        revolution_period,
        mean_anomaly: initial_mean_anomaly,
        ..Default::default()
    }
} 


pub(crate) fn check_ship_orbits(
    ships: Query<(&ShipInfo, &Position, &Velocity, &Influenced), Without<EllipticalOrbit>>,
    influencers: Query<(&Position, &Velocity, &Mass), With<HillRadius>>,
    mut writer: EventWriter<ShipEvent>,
) {
    for (info, pos, vel, influenced) in ships.iter() {
        if let Some(main_influencer) = influenced.main_influencer {
            if let Ok((inf_pos, inf_vel, inf_mass)) = influencers.get(main_influencer) {
                let r = pos.0 - inf_pos.0;
                let v = vel.0 - inf_vel.0;
                let mu = G*inf_mass.0;
                let epsilon = v.length().powf(2.)/2. - mu/r.length();
                if epsilon < 0. {
                    writer.send(ShipEvent::SwitchToOrbital{ship_id: info.id, r_vec: r, v_vec: v, host_mass: *inf_mass});
                }
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct DisableShipOrbitCheck(pub bool);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{
        bodies::body_data::{BodyData, BodyType},
        orbiting_obj::{OrbitingObjects, OrbitalObjID},
        id::id_from,
    };
    use bevy::ecs::system::SystemState;
    use crate::physics::SECONDS_PER_DAY;
    const TWO_PI: f64 = std::f64::consts::TAU; 

    
    fn setup(app: &mut App, info: &ShipInfo) -> Entity {

        app.add_event::<ShipEvent>();

        let sun_data = BodyData {
            id: id_from("soleil"),
            name: "Sun".into(),
            body_type: BodyType::Star,
            host_body: None,
            semimajor_axis: 0.,
            eccentricity: 0.,
            inclination: 0.,
            long_asc_node: 0.,
            arg_periapsis: 0.,
            initial_mean_anomaly: 0.,
            periapsis: 0.,
            apoapsis: 0.,
            revolution_period: 0.,
            rotation_period: 0.,
            radius: 695508.,
            mass: 1.989e30
        };
        let earth_data = BodyData {
            id: id_from("terre"),
            name: "Earth".into(),
            body_type: BodyType::Planet,
            host_body: Some(id_from("terre")),
            semimajor_axis: 149598023.,
            eccentricity: 0.01670,
            inclination: 0.,
            long_asc_node: 18.272,
            arg_periapsis: 85.901,
            initial_mean_anomaly: 358.617,
            periapsis: 147095000.,
            apoapsis: 152100000.,
            revolution_period: 365.256,
            rotation_period: 23.9345,
            radius: 6371.00840,
            mass: 5.97237e24
        };
        let primary_body = app.world_mut().spawn( (
            Position::default(),
            EllipticalOrbit::from(&sun_data),
            Mass(sun_data.mass),
            OrbitingObjects(vec![OrbitalObjID::Body(id_from("terre"))]),
            BodyInfo(sun_data.clone()),
            Velocity::default(),
            ClearOnUnload,
            PrimaryBody,
            HillRadius(f64::INFINITY)
        )).id();

        let hill_radius_earth = (earth_data.semimajor_axis
            * (1. - earth_data.eccentricity)
            * (earth_data.mass / (3. * (sun_data.mass + earth_data.mass))).powf(1. / 3.))
            .max(earth_data.radius);    
        
        let body = app.world_mut().spawn( (
            Position::default(),
            EllipticalOrbit::from(&earth_data),
            Mass(earth_data.mass),
            OrbitingObjects(Vec::new()),
            BodyInfo(earth_data),
            Velocity::default(),
            ClearOnUnload,
            HillRadius(hill_radius_earth)
        )).id();

        let mut mapping = HashMap::new();
        mapping.insert(id_from("soleil"), primary_body);
        mapping.insert(id_from("terre"), body);
        app.insert_resource(ShipsMapping::default());
        app.insert_resource(BodiesMapping(mapping));

        let mut state_mapping: SystemState<Res<BodiesMapping>> = SystemState::new(app.world_mut());
        let mut state_query: SystemState<Query<(&Position, &HillRadius, &OrbitingObjects)>> = SystemState::new(app.world_mut());
        let (bodies_mapping, bodies) = {
        let world = app.world();
            (
                state_mapping.get(world),
                state_query.get(world)
            )
        };

        let pos = Position(info.spawn_pos); 
        let influence = Influenced::new(&pos, &bodies, bodies_mapping.as_ref(), id_from("soleil"));
        let acceleration = Acceleration{current: DVec3::ZERO, previous: DVec3::ZERO};

        let ship_entity = app.world_mut().spawn((
            info.clone(),
            acceleration,
            influence,
            pos,
            Velocity(info.spawn_speed),
            TransformBundle::from_transform(Transform::from_xyz(0., 0., 1.)),
            ClearOnUnload,
        )).id();

        app.world_mut().resource_mut::<ShipsMapping>().0.insert(info.id, ship_entity);

        ship_entity
    }

    #[test]
    fn test_handle_ship_remove() {
        let mut app = App::new();
        
        app.insert_resource(ShipsMapping::default());
        app.add_event::<ShipEvent>();
        
        app.add_systems(Update, handle_ship_remove);

        let ship_id = ShipID::from("ship").unwrap();
        let info: ShipInfo = ShipInfo::default();
        let entity = app.world_mut().spawn(info).id();
        app.world_mut().resource_mut::<ShipsMapping>().0.insert(ship_id, entity);
        
        app.world_mut().send_event(ShipEvent::Remove(ship_id));
        
        app.update();
        
        assert!(app.world().get_entity(entity).is_none());
        assert!(!app.world().resource::<ShipsMapping>().0.contains_key(&ship_id));
    }

    #[test]
    fn test_handle_ship_create() {
        let mut app = App::new();
        
        app.insert_resource(ShipsMapping::default());
        app.insert_resource(BodiesMapping::default());
        
        app.world_mut().spawn((BodyInfo::default(), PrimaryBody));
        
        app.add_systems(Update, handle_ship_create);
        app.add_event::<ShipEvent>();

        app.world_mut().send_event(ShipEvent::Create(ShipInfo {
            id: ShipID::from("s").unwrap(),
            spawn_pos: DVec3::new(1e6, 0., 0.),
            spawn_speed: DVec3::new(0., 1e6, 0.),
        }));
                
        app.update();
        
        let created_ships = app
            .world_mut()
            .query_filtered::<Entity, (With<Position>, With<Velocity>, With<Acceleration>, With<Influenced>)>()
            .iter(app.world())
            .count();
        assert_eq!(created_ships, 1);
    }

    #[test]
    fn test_check_ship_orbits_handle_switch_to_orbital() {

        let mut app = App::new();

        let info = ShipInfo {
            id: ShipID::from("s").unwrap(),
            spawn_pos: DVec3 { x: -32501208.838173263, y: 143561259.9263618, z: 0.},
            spawn_speed: DVec3 { x: -2696715.3893552525, y: -672187.3782865074, z: 0. } 
        };

        let ship_entity = setup(&mut app, &info);

        app.add_systems(Update, handle_switch_to_orbital);
        app.add_systems(Update, check_ship_orbits);

        app.update();

        let orbit = app.world().get::<EllipticalOrbit>(ship_entity);
        let orbiting_obj = app.world().get::<OrbitingObjects>(ship_entity);
        let host_body = app.world().get::<HostBody>(ship_entity);
        assert!(orbit.is_some(), "Ship should have a EllipticalOrbit Component");
        assert!(orbiting_obj.is_some(), "Ship should have a HostBody Component");
        assert!(host_body.is_some(), "Ship should have a HostBody Component");
    }

    #[test]
    fn test_switch_to_orbital_impossible() {

        let mut app = App::new();
        
        // Vaisseau dans le rayon de Hill mais pas en orbite 
        let info = ShipInfo {
            id: ShipID::from("s2").unwrap(),
            spawn_pos: DVec3 { x: -2522401.726568888, y: 142515717.88224745, z: 0.},
            spawn_speed: DVec3 { x: -25224010.7265688, y: -544246.5886227646, z: 0. } 
        };
        let ship_entity = setup(&mut app, &info);

        app.add_systems(Update, handle_ship_create);
        app.add_systems(Update, check_ship_orbits);

        app.update();

        let orbit = app.world().get::<EllipticalOrbit>(ship_entity);
        let orbiting_obj = app.world().get::<OrbitingObjects>(ship_entity);
        let host_body = app.world().get::<HostBody>(ship_entity);
        assert!(orbit.is_none(), "Ship should have no EllipticalOrbit Component");
        assert!(orbiting_obj.is_none(), "Ship should have no OrbitingObjects Component");
        assert!(host_body.is_none(), "Ship should have no HostBody Component");
    }

    #[test]
    fn test_handle_switch_to_free_motion() {

        let mut app = App::new();

        let info = ShipInfo {
            id: ShipID::from("s").unwrap(),
            spawn_pos: DVec3 { x: -32501208.838173263, y: 143561259.9263618, z: 0.},
            spawn_speed: DVec3 { x: -2696715.3893552525, y: -672187.3782865074, z: 0. } 
        };

        let ship_entity = setup(&mut app, &info);

        app.add_systems(Update, handle_switch_to_free_motion);
        app.add_systems(Update, handle_switch_to_orbital);

        
        app.world_mut().send_event(ShipEvent::SwitchToOrbital { 
            ship_id: ShipID::from("s").unwrap(), 
            r_vec: DVec3 { x: -32501208.838173263, y: 143561259.9263618, z: 0.}, 
            v_vec: DVec3 { x: -2696715.38935525, y: -672187.3782865074, z: 0. },
            host_mass: Mass(1e20),
        });

        app.update();

        app.world_mut().send_event(ShipEvent::SwitchToFreeMotion(ShipID::from("s").unwrap()));

        app.update();

        let orbit = app.world().get::<EllipticalOrbit>(ship_entity);
        let orbiting_obj = app.world().get::<OrbitingObjects>(ship_entity);
        let host_body: Option<&HostBody> = app.world().get::<HostBody>(ship_entity);
        assert!(orbit.is_none(), "Ship shouldn't have a EllipticalOrbit Component");
        assert!(orbiting_obj.is_none(), "Ship shouldn't have a HostBody Component");
        assert!(host_body.is_none(), "Ship shouldn't have a HostBody Component");

        let influenced = app.world().get::<Influenced>(ship_entity);
        let acc = app.world().get::<Acceleration>(ship_entity);
        assert!(influenced.is_some(), "Ship should have an Influenced Component");
        assert!(acc.is_some(), "Ship should have an Acceleration Component");
    }

    #[test]
    fn test_calc_elliptical_orbit() {
        // r and v found at https://ssd.jpl.nasa.gov/api/horizons.api?format=json&COMMAND=399&CENTER=%27@sun%27&EPHEM_TYPE=VECTORS&START_TIME=%272000-01-01%27&STOP_TIME=%272000-01-02%27&STEP_SIZE=%271%20d%27&OUT_UNITS=KM-S
        let r_vec = DVec3::new(
        -2.521092855899356e7,
        1.449279195838006e8,
        -6.164165719002485e2,
        );
        let v_vec = DVec3::new(
            -29.83983333677879,
            -5.207633902410673,
            6.16844118423998e-05,
        )* SECONDS_PER_DAY;

        let earth_mass = 1.9885e30;

        let orbit = calc_elliptical_orbit(
            r_vec,
            v_vec,
            Mass(earth_mass),
        );
        let semimajor =  149598023.;
        let eccentricity = 0.01670;
        let inclination = 0.;
        let long_asc_node = 18.272;
        let arg_periapsis = 85.901;
        // let initial_mean_anomaly = 358.617;
        let revolution_period = 365.256;

        let tolerance_prct = 0.2;
        let epsilon = 1e-5;

        fn normalize_angle(angle: f64) -> f64 {
            let mut a = angle % TWO_PI;
            if a < 0.0 {
                a += TWO_PI;
            }
            a
        }

        assert!((orbit.semimajor_axis - semimajor).abs() < tolerance_prct * semimajor);
        assert!((orbit.eccentricity - eccentricity).abs() < eccentricity * tolerance_prct);
        assert!((orbit.inclination - inclination).abs() < epsilon);
        assert!((normalize_angle(orbit.long_asc_node) - normalize_angle(long_asc_node)).abs() < long_asc_node * tolerance_prct);
        assert!((normalize_angle(orbit.arg_periapsis) - normalize_angle(arg_periapsis)).abs() < arg_periapsis * tolerance_prct);
        assert!((orbit.revolution_period - revolution_period).abs() < revolution_period * tolerance_prct);
        }
}
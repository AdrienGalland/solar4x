use std::{
    collections::{btree_map, BTreeMap},
    fs::{read_dir, remove_file, File},
    io::{Read, Write},
    iter::Peekable,
    path::{Path, PathBuf},
    sync::Arc,
};

// use arrayvec::ArrayString;
use vectorize;

use bevy::{math::DVec3, prelude::*};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::{
    game::{Authoritative, GameFiles},
    objects::prelude::{BodiesMapping, BodyID},
    physics::{prelude::*, time::TickEvent},
    prelude::{exit_on_error_if_app, GameStage},
    utils::algebra::orbital_to_global_matrix,
};

use super::{ShipID, ShipInfo, ShipsMapping};

pub const TRAJECTORIES_PATH: &str = "trajectories";

pub fn plugin(app: &mut App) {
    app.add_event::<TrajectoryEvent>()
        .add_event::<VelocityUpdate>()
        // This system set is currently configured in the [physics] module
        .add_systems(
            FixedUpdate,
            (
                follow_trajectory.run_if(on_event::<TickEvent>()),
                handle_thrusts,
            )
                .chain()
                .in_set(TrajectoryUpdate),
        )
        .add_systems(
            OnEnter(GameStage::Action),
            dispatch_trajectories.run_if(in_state(Authoritative)),
        )
        .add_systems(
            OnEnter(GameStage::Preparation),
            remove_old_nodes.run_if(in_state(Authoritative)),
        )
        .add_systems(Update, handle_trajectory_event.pipe(exit_on_error_if_app));
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TrajectoryUpdate;

#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ManeuverNode {
    pub name: String,
    pub thrust: DVec3,
    pub origin: BodyID,
}

/// A succession of maneuver nodes sorted by order of time, with a single node per server tick
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Trajectory {
    #[serde(with = "vectorize")]
    pub nodes: BTreeMap<u64, ManeuverNode>,
}

/// A trajectory taken by an object, storing a peekable queue of all remaining maneuver nodes
#[derive(Component, Debug)]
pub struct CurrentTrajectory {
    queue: Peekable<btree_map::IntoIter<u64, ManeuverNode>>,
}

impl CurrentTrajectory {
    pub fn new(trajectory: Trajectory) -> Self {
        Self {
            queue: trajectory.nodes.into_iter().peekable(),
        }
    }

    pub fn new_from_tick(mut trajectory: Trajectory, tick: u64) -> Self {
        trajectory.nodes.retain(|t, _| *t >= tick);
        Self::new(trajectory)
    }
}

#[derive(Event, Debug, Clone, PartialEq)]
pub enum TrajectoryEvent {
    Create {
        ship: ShipID,
        trajectory: Trajectory,
    },
    Delete(ShipID),
    AddNode {
        ship: ShipID,
        node: ManeuverNode,
        tick: u64,
    },
    RemoveNode {
        ship: ShipID,
        tick: u64,
    },
}

#[derive(Event, Debug)]
pub struct VelocityUpdate {
    pub ship_id: ShipID,
    pub thrust: DVec3,
}

#[derive(Debug)]
pub enum TrajectoryError {
    Io(std::io::Error),
    De(toml::de::Error),
    Ser(toml::ser::Error),
}

impl From<std::io::Error> for TrajectoryError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<toml::de::Error> for TrajectoryError {
    fn from(value: toml::de::Error) -> Self {
        Self::De(value)
    }
}

impl From<toml::ser::Error> for TrajectoryError {
    fn from(value: toml::ser::Error) -> Self {
        Self::Ser(value)
    }
}

impl std::fmt::Display for TrajectoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrajectoryError::Io(err) => write!(f, "Error when reading trajectory: {}", err),
            TrajectoryError::De(err) => write!(f, "Error when deserializing trajectory: {}", err),
            TrajectoryError::Ser(err) => write!(f, "Error when serializing trajectory: {}", err),
        }
    }
}

impl std::error::Error for TrajectoryError {}

fn read_trajectory(path: impl AsRef<Path>) -> Result<Trajectory, TrajectoryError> {
    let mut file = File::open(&path)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(toml::from_str::<Trajectory>(&buf)?)
}

fn build_path(dir: impl AsRef<Path>, id: ShipID) -> PathBuf {
    dir.as_ref().join(id.to_string())
}

pub fn read_ship_trajectory(
    dir: impl AsRef<Path>,
    id: ShipID,
) -> Result<Trajectory, TrajectoryError> {
    read_trajectory(build_path(dir, id))
}

pub fn write_trajectory(path: impl AsRef<Path>, t: &Trajectory) -> Result<(), TrajectoryError> {
    let s = toml::to_string_pretty(t)?;
    Ok(File::create(path)?.write_all(s.as_bytes())?)
}

fn follow_trajectory(
    mut velocity_event_writer: EventWriter<VelocityUpdate>,
    mapping: Res<BodiesMapping>,
    coords: Query<(&Position, &Velocity)>,
    mut trajectories: Query<(Entity, &mut CurrentTrajectory, &ShipInfo)>,
    time: Res<GameTime>,
    // mut ship_event_writer: EventWriter<ShipEvent>,
) {
    let velocity_updates = Arc::new(Mutex::new(Vec::new()));
    // let ship_events = Arc::new(Mutex::new(Vec::new()));
    trajectories.par_iter_mut().for_each(|(e, mut t, info)| {
        if let Some((tick, n)) = t.queue.peek() {
            if *tick <= time.tick() {
                // let id = ArrayString::from(&info.id).unwrap();
                // ship_events.lock().unwrap().push(
                //     ShipEvent::SwitchToFreeMotion(id)
                // );
                if let Some(origin) = mapping.0.get(&n.origin) {
                    let (&Position(o_pos), &Velocity(o_speed)) = coords.get(*origin).unwrap();
                    let (&Position(pos), &Velocity(speed)) = coords.get(e).unwrap();
                    let thrust: DVec3 = orbital_to_global_matrix(o_pos, o_speed, pos, speed) * n.thrust;
                    velocity_updates.lock().unwrap().push(VelocityUpdate {
                        ship_id: info.id,
                        thrust,
                    });
                }
                t.queue.next();
            }
        }
    });
    // ship_event_writer.send_batch(
    // Arc::try_unwrap(ship_events)
    //     .unwrap()
    //     .into_inner()
    //     .unwrap(),
    // );

    velocity_event_writer.send_batch(
        Arc::try_unwrap(velocity_updates)
            .unwrap()
            .into_inner()
            .unwrap(),
    );
}

pub fn handle_thrusts(
    mut velocity_events: EventReader<VelocityUpdate>,
    mut speeds: Query<&mut Velocity>,
    mapping: Res<ShipsMapping>,
) {
    for event in velocity_events.read() {
        if let Some(entity) = mapping.0.get(&event.ship_id) {
            speeds.get_mut(*entity).unwrap().0 += event.thrust;
        }
    }
}

pub fn dispatch_trajectories(
    mut commands: Commands,
    dir: Res<GameFiles>,
    mapping: Res<ShipsMapping>,
) {
    if let Ok(dir) = read_dir(&dir.trajectories) {
        for entry in dir.flatten() {
            let path = entry.path();
            if let Ok(traj) = read_trajectory(&path) {
                if let Some(e) = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|s| ShipID::from(s).ok())
                    .and_then(|id| mapping.0.get(&id))
                {
                    commands.entity(*e).insert(CurrentTrajectory::new(traj));
                }
            }
        }
    }
}

pub fn remove_old_nodes(dir: Res<GameFiles>, time: Res<GameTime>) {
    if let Ok(dir) = read_dir(&dir.trajectories) {
        for entry in dir.flatten() {
            let path = entry.path();
            if let Ok(mut traj) = read_trajectory(&path) {
                traj.nodes.retain(|t, _| *t >= time.tick());
                write_trajectory(path, &traj).expect("Could not write trajectory");
            }
        }
    }
}

pub fn handle_trajectory_event(
    mut reader: EventReader<TrajectoryEvent>,
    dir: Res<GameFiles>,
    time: Res<GameTime>,
    mapping: Option<Res<ShipsMapping>>,
    mut commands: Commands,
) -> color_eyre::Result<()> {
    use TrajectoryEvent::*;
    for event in reader.read() {
        let ship = *match event {
            Create { ship, .. } => ship,
            Delete(s) => s,
            AddNode { ship, .. } => ship,
            RemoveNode { ship, .. } => ship,
        };
        let path = build_path(&dir.trajectories, ship);
        match event {
            Create { trajectory, .. } => {
                write_trajectory(path, trajectory)?;
                if let Some(entity) = mapping.as_ref().and_then(|mapping| mapping.0.get(&ship)) {
                    commands
                        .entity(*entity)
                        .insert(CurrentTrajectory::new_from_tick(trajectory.clone(), time.tick()));
                }
            }
            Delete(_) => {
                remove_file(path).unwrap_or_default();
                if let Some(entity) = mapping.as_ref().and_then(|mapping| mapping.0.get(&ship)) {
                    commands.entity(*entity).remove::<CurrentTrajectory>();
                }
            }
            AddNode { node, tick, .. } => {
                let mut t = read_trajectory(&path).unwrap_or_default();
                t.nodes.insert(*tick, node.clone());
                write_trajectory(path, &t)?;
                if let Some(entity) = mapping.as_ref().and_then(|mapping| mapping.0.get(&ship)) {
                    commands
                        .entity(*entity)
                        .insert(CurrentTrajectory::new_from_tick(t, time.tick()));
                }
            }
            RemoveNode { tick, .. } => {
                let mut t = read_trajectory(&path)?;
                t.nodes.remove(tick);
                write_trajectory(path, &t)?;
                if let Some(entity) = mapping.as_ref().and_then(|mapping| mapping.0.get(&ship)) {
                    commands
                        .entity(*entity)
                        .insert(CurrentTrajectory::new_from_tick(t, time.tick()));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{read_dir, File},
        io::Read,
    };

    use bevy::{
        app::{App, FixedMain},
        math::DVec3,
        state::state::NextState,
    };

    use crate::{objects::ships::{ShipEvent, DisableShipOrbitCheck}, physics::time::SIMTICKS_PER_TICK, prelude::*};

    use super::*;

    fn new_app_with(mut app: App) -> App {
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Singleplayer));
        app.update();
        app
    }

    fn new_trajectory() -> Trajectory {
        Trajectory {
            nodes: BTreeMap::from([(
                1,
                ManeuverNode {
                    name: "1".to_owned(),
                    thrust: DVec3::new(1e4, 0., 0.),
                    origin: id_from("soleil"),
                },
            )]),
        }
    }

    #[test]
    fn test_handle_trajectory_event() -> color_eyre::Result<()> {
        let mut app = App::new();
        app = new_app_with(app);
        let trajectory = new_trajectory();
        app.world_mut().send_event(TrajectoryEvent::Create {
            ship: ShipID::from("s")?,
            trajectory: trajectory.clone(),
        });
        app.update();
        let path = app.world_mut().resource::<GameFiles>().trajectories.clone();
        let mut files = read_dir(path).unwrap();
        let file_path = files.next().unwrap()?.path();
        let mut buf = String::new();
        File::open(&file_path)?.read_to_string(&mut buf)?;
        let traj = read_trajectory(file_path)?;
        assert_eq!(
            traj.nodes.into_iter().collect::<Vec<_>>(),
            trajectory.nodes.into_iter().collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn test_dispatch_trajectory() {
        let mut app = App::new();
        app = new_app_with(app);
        let id = id_from("s");
        app.world_mut().send_event(ShipEvent::Create(ShipInfo {
            id,
            spawn_pos: DVec3::new(1e6, 0., 0.),
            spawn_speed: DVec3::new(0., 1e6, 0.),
        }));
        let trajectory = new_trajectory();
        app.world_mut().send_event(TrajectoryEvent::Create {
            ship: id,
            trajectory: trajectory.clone(),
        });
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameStage>>()
            .set(GameStage::Action);
        app.update();
        let world = app.world_mut();
        let mut query = world.query::<&mut CurrentTrajectory>();
        let queue = &mut query.single_mut(world).queue;
        assert_eq!(
            queue.collect::<Vec<_>>(),
            trajectory.nodes.into_iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_follow_trajectory() {
        let mut app = App::new();
        app.insert_resource(DisableShipOrbitCheck(true));
        app = new_app_with(app);
        let id = id_from("s");
        app.world_mut().send_event(ShipEvent::Create(ShipInfo {
            id,
            spawn_pos: DVec3::new(0., 0., 1e10),
            spawn_speed: DVec3::new(0., 1e4, 0.),
        }));
        let trajectory = new_trajectory();
        app.world_mut().send_event(TrajectoryEvent::Create {
            ship: id,
            trajectory: trajectory.clone(),
        });
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameStage>>()
            .set(GameStage::Action);
        app.update();
        let mut simtick = 0;

        while simtick < SIMTICKS_PER_TICK {
            app.update();
            simtick = app.world().resource::<GameTime>().simtick;
        }
        FixedMain::run_fixed_main(app.world_mut());
        assert_eq!(app.world().resource::<Events<VelocityUpdate>>().len(), 1);
        let ship_speed = app
            .world_mut()
            .query_filtered::<&Velocity, With<ShipInfo>>()
            .single(app.world());
        assert!((ship_speed.0 - DVec3::new(0., 2e4, 0.)).length() < 10., "Erreur trop grande : {}, ship_speed.0 : {}", (ship_speed.0 - DVec3::new(0., 2e4, 0.)).length(), ship_speed.0);
    }
}

pub mod event_bus;

use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use bevy::{math::DVec3, prelude::*};
use mlua::{Lua, Table, Value};

use crate::{
    game::Loaded,
    objects::{
        prelude::{BodiesMapping, BodyID, BodyInfo},
        ships::{
            trajectory::{TrajectoryUpdate, VelocityUpdate},
            ShipID, ShipInfo,
        },
    },
    physics::prelude::{Position, Velocity},
};

pub const DEFAULT_SHIP_SCRIPTS_PATH: &str = "src/scripts/ships";

pub struct ScriptingPlugin;

impl Plugin for ScriptingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(event_bus::EventBusPlugin)
            .init_resource::<ScriptSettings>()
            .add_systems(
                FixedUpdate,
                run_ship_scripts
                    .before(TrajectoryUpdate)
                    .run_if(in_state(Loaded)),
            );
    }
}

#[derive(Resource, Debug, Clone)]
pub struct ScriptSettings {
    pub ship_scripts_dir: PathBuf,
}

impl Default for ScriptSettings {
    fn default() -> Self {
        Self {
            ship_scripts_dir: DEFAULT_SHIP_SCRIPTS_PATH.into(),
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct ShipScript {
    pub path: PathBuf,
}

impl ShipScript {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ScriptCommand {
    ApplyGlobalThrust { ship: ShipID, thrust: DVec3 },
}

fn run_ship_scripts(
    settings: Res<ScriptSettings>,
    ships: Query<(&ShipInfo, &Position, &Velocity, Option<&ShipScript>)>,
    bodies: Query<(&BodyInfo, &Position, &Velocity)>,
    bodies_mapping: Res<BodiesMapping>,
    mut velocity_updates: EventWriter<VelocityUpdate>,
) {
    for (ship, pos, speed, script) in ships.iter() {
        let Some(path) = script_path_for_ship(&settings.ship_scripts_dir, ship, script) else {
            continue;
        };
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        match run_ship_script(ship, pos.0, speed.0, &source, &bodies, &bodies_mapping) {
            Ok(commands) => {
                velocity_updates.send_batch(commands.into_iter().map(|command| match command {
                    ScriptCommand::ApplyGlobalThrust { ship, thrust } => {
                        VelocityUpdate { ship_id: ship, thrust }
                    }
                }));
            }
            Err(err) => {
                warn!("Lua script {} failed: {err}", path.display());
            }
        }
    }
}

fn script_path_for_ship(
    scripts_dir: &Path,
    ship: &ShipInfo,
    script: Option<&ShipScript>,
) -> Option<PathBuf> {
    let explicit = script.map(|script| script.path.clone());
    let default = scripts_dir.join(format!("{}.lua", ship.id));
    explicit.or_else(|| default.exists().then_some(default))
}

fn run_ship_script(
    ship: &ShipInfo,
    ship_pos: DVec3,
    ship_speed: DVec3,
    source: &str,
    bodies: &Query<(&BodyInfo, &Position, &Velocity)>,
    bodies_mapping: &BodiesMapping,
) -> mlua::Result<Vec<ScriptCommand>> {
    let lua = Lua::new();
    let commands = Rc::new(RefCell::new(Vec::new()));
    install_ship_api(
        &lua,
        commands.clone(),
        ship,
        ship_pos,
        ship_speed,
        bodies,
        bodies_mapping,
    )?;
    lua.load(source).set_name(&ship.id.to_string()).exec()?;
    Ok(Rc::try_unwrap(commands)
        .map(|commands| commands.into_inner())
        .unwrap_or_default())
}

fn install_ship_api(
    lua: &Lua,
    commands: Rc<RefCell<Vec<ScriptCommand>>>,
    ship: &ShipInfo,
    ship_pos: DVec3,
    ship_speed: DVec3,
    bodies: &Query<(&BodyInfo, &Position, &Velocity)>,
    bodies_mapping: &BodiesMapping,
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("ship", ship_table(lua, ship, ship_pos, ship_speed)?)?;

    let body_positions = bodies
        .iter()
        .map(|(info, pos, speed)| (info.0.id, (pos.0, speed.0)))
        .collect::<Vec<_>>();
    let body_entities = bodies_mapping.0.clone();
    globals.set(
        "body",
        lua.create_function(move |lua, id: String| {
            let id = BodyID::from(id.as_str()).map_err(mlua::Error::external)?;
            let Some(_) = body_entities.get(&id) else {
                return Ok(Value::Nil);
            };
            let Some((_, (pos, speed))) = body_positions.iter().find(|(body_id, _)| *body_id == id)
            else {
                return Ok(Value::Nil);
            };
            Ok(Value::Table(body_table(lua, id, *pos, *speed)?))
        })?,
    )?;

    globals.set(
        "vec3",
        lua.create_function(|lua, (x, y, z): (f64, f64, f64)| vec3_table(lua, DVec3::new(x, y, z)))?,
    )?;
    globals.set(
        "length",
        lua.create_function(|_, value: Table| Ok(table_to_vec3(value)?.length()))?,
    )?;
    globals.set(
        "distance",
        lua.create_function(|_, (a, b): (Table, Table)| {
            Ok((table_to_vec3(a)? - table_to_vec3(b)?).length())
        })?,
    )?;
    globals.set(
        "normalize",
        lua.create_function(|lua, value: Table| {
            vec3_table(lua, table_to_vec3(value)?.normalize_or_zero())
        })?,
    )?;

    let ship_id = ship.id;
    globals.set(
        "apply_global_thrust",
        lua.create_function(move |_, thrust: Table| {
            commands.borrow_mut().push(ScriptCommand::ApplyGlobalThrust {
                ship: ship_id,
                thrust: table_to_vec3(thrust)?,
            });
            Ok(())
        })?,
    )?;

    Ok(())
}

fn ship_table(lua: &Lua, ship: &ShipInfo, pos: DVec3, speed: DVec3) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", ship.id.to_string())?;
    table.set("position", vec3_table(lua, pos)?)?;
    table.set("velocity", vec3_table(lua, speed)?)?;
    Ok(table)
}

fn body_table(lua: &Lua, id: BodyID, pos: DVec3, speed: DVec3) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", id.to_string())?;
    table.set("position", vec3_table(lua, pos)?)?;
    table.set("velocity", vec3_table(lua, speed)?)?;
    Ok(table)
}

fn vec3_table(lua: &Lua, value: DVec3) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("x", value.x)?;
    table.set("y", value.y)?;
    table.set("z", value.z)?;
    Ok(table)
}

fn table_to_vec3(table: Table) -> mlua::Result<DVec3> {
    Ok(DVec3::new(table.get("x")?, table.get("y")?, table.get("z")?))
}

#[cfg(test)]
mod tests {
    use bevy::utils::HashMap;

    use crate::objects::prelude::{id_from, BodiesMapping, BodyData};

    use super::*;

    fn body_query_app() -> App {
        let mut app = App::new();
        let earth = app
            .world_mut()
            .spawn((
                BodyInfo(BodyData {
                    id: id_from("terre"),
                    ..Default::default()
                }),
                Position(DVec3::new(10., 0., 0.)),
                Velocity(DVec3::new(0., 1., 0.)),
            ))
            .id();
        app.insert_resource(BodiesMapping(HashMap::from([(id_from("terre"), earth)])));
        app
    }

    #[test]
    fn test_ship_script_can_apply_global_thrust() {
        let mut app = body_query_app();
        let ship = ShipInfo {
            id: id_from("ship"),
            ..Default::default()
        };
        let mut query = app.world_mut().query::<(&BodyInfo, &Position, &Velocity)>();
        let commands = run_ship_script(
            &ship,
            DVec3::ZERO,
            DVec3::ZERO,
            "apply_global_thrust(vec3(1.0, 2.0, 3.0))",
            &query,
            app.world().resource::<BodiesMapping>(),
        )
        .unwrap();

        assert_eq!(
            commands,
            vec![ScriptCommand::ApplyGlobalThrust {
                ship: id_from("ship"),
                thrust: DVec3::new(1., 2., 3.),
            }]
        );
    }

    #[test]
    fn test_ship_script_can_read_body_distance() {
        let mut app = body_query_app();
        let ship = ShipInfo {
            id: id_from("ship"),
            ..Default::default()
        };
        let mut query = app.world_mut().query::<(&BodyInfo, &Position, &Velocity)>();
        let commands = run_ship_script(
            &ship,
            DVec3::ZERO,
            DVec3::ZERO,
            "\
                local earth = body('terre')\n\
                if distance(ship.position, earth.position) < 20.0 then\n\
                    apply_global_thrust(normalize(earth.position))\n\
                end\n\
            ",
            &query,
            app.world().resource::<BodiesMapping>(),
        )
        .unwrap();

        assert_eq!(
            commands,
            vec![ScriptCommand::ApplyGlobalThrust {
                ship: id_from("ship"),
                thrust: DVec3::X,
            }]
        );
    }
}

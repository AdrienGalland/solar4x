use bevy::{math::DVec3, prelude::*};
use mlua::Value;

use crate::{
    game::Loaded,
    objects::{
        prelude::BodyInfo,
        ships::{ShipID, ShipInfo, trajectory::VelocityUpdate},
    },
    physics::{prelude::{Position, ToggleTime}, time::TimeEvent},
    scripting::event_bus::{LuaEventBus, ScriptingSet},
};

// ── ship_tick broadcast ───────────────────────────────────────────────────────

/// Fires one `ship_tick` event per ship, per frame, while the simulation is running.
///
/// Lua event data:
///   data.ship_id  : string
///   data.bodies   : table  { body_id  → distance (km) }
///   data.ships    : table  { ship_id  → distance (km) }
fn ship_tick_broadcast(
    ships: Query<(&ShipInfo, &Position)>,
    bodies: Query<(&BodyInfo, &Position)>,
    bus: NonSendMut<LuaEventBus>,
) {
    let ships_list: Vec<(&ShipInfo, &Position)> = ships.iter().collect();

    for (ship, ship_pos) in ships_list.iter() {
        let Ok(bodies_table) = bus.lua.create_table() else { continue };
        for (body_info, body_pos) in bodies.iter() {
            let d = (ship_pos.0 - body_pos.0).length();
            let _ = bodies_table.set(body_info.0.id.to_string(), d);
        }

        let Ok(ships_table) = bus.lua.create_table() else { continue };
        for (other_ship, other_pos) in ships_list.iter() {
            if other_ship.id == ship.id { continue }
            let d = (ship_pos.0 - other_pos.0).length();
            let _ = ships_table.set(other_ship.id.to_string(), d);
        }

        let Ok(event_table) = bus.lua.create_table() else { continue };
        let _ = event_table.set("ship_id", ship.id.to_string());
        let _ = event_table.set("bodies", bodies_table);
        let _ = event_table.set("ships", ships_table);
        bus.fire("ship_tick", Value::Table(event_table));
    }
}

// ── Lua → Bevy bridge ─────────────────────────────────────────────────────────

/// Reads events emitted by the Lua bus and translates them into Bevy actions.
fn lua_to_bevy_bridge(
    mut bus: NonSendMut<LuaEventBus>,
    mut time_events: EventWriter<TimeEvent>,
    mut velocity_updates: EventWriter<VelocityUpdate>,
) {
    for (event_name, data) in bus.emitted.drain(..) {
        match event_name.as_str() {
            "pause_game" => {
                time_events.send(TimeEvent::PauseTime);
            }
            "resume_game" => {
                time_events.send(TimeEvent::StartTime);
            }
            "apply_thrust" => {
                if let Value::Table(t) = &data {
                    let ship_id_str: mlua::Result<String> = t.get("ship_id");
                    let dx: mlua::Result<f64> = t.get("dx");
                    let dy: mlua::Result<f64> = t.get("dy");
                    let dz: mlua::Result<f64> = t.get("dz");
                    if let (Ok(id), Ok(x), Ok(y), Ok(z)) = (ship_id_str, dx, dy, dz) {
                        if let Ok(ship_id) = ShipID::from(id.as_str()) {
                            velocity_updates.send(VelocityUpdate { ship_id, thrust: DVec3::new(x, y, z) });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Script loading ────────────────────────────────────────────────────────────

/// Loads all `.lua` files from `src/scripts/events/` into the event bus.
/// Files are loaded in alphabetical order so that libraries prefixed with `_`
/// are guaranteed to be available before behaviour scripts.
/// Runs once when the game enters the `Loaded` state.
fn load_event_scripts(mut bus: NonSendMut<LuaEventBus>) {
    let dir = std::path::Path::new("src/scripts/events");
    if !dir.exists() {
        warn!("[scripting] scripts directory '{}' not found", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "lua"))
        .collect();
    paths.sort();

    for path in paths {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if let Err(e) = bus.load_script(&source, &name) {
                    warn!("[scripting] Failed to load '{}': {e}", path.display());
                }
            }
            Err(e) => warn!("[scripting] Cannot read '{}': {e}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use bevy::math::DVec3;
    use mlua::Value;
    use crate::{
        objects::{
            bodies::body_data::{BodyData},
            prelude::{id_from, BodyInfo},
            ships::{ShipID, ShipInfo, trajectory::VelocityUpdate},
        },
        physics::{prelude::Position, time::TimeEvent},
        scripting::event_bus::{EventBusPlugin, LuaEventBus, ScriptingSet},
    };

    fn setup() -> App {
        let mut app = App::new();
        app.add_plugins(EventBusPlugin);
        app.add_event::<TimeEvent>();
        app.add_event::<VelocityUpdate>();
        app.insert_resource(crate::physics::prelude::ToggleTime(true));
        app
    }

    #[test]
    fn test_ship_tick_includes_bodies_and_ships() {
        let mut app = setup();
        {
            let mut bus = app.world_mut().non_send_resource_mut::<LuaEventBus>();
            bus.load_script(
                r#"
                on("ship_tick", function(data)
                    if data.ship_id == "shp1" then
                        _G.has_body = data.bodies["terre"] ~= nil
                        _G.has_ship = data.ships["shp2"] ~= nil
                    end
                end)
                "#,
                "test",
            ).unwrap();
        }
        app.world_mut().spawn((
            BodyInfo(BodyData { id: id_from("terre"), ..Default::default() }),
            Position(DVec3::new(1e5, 0., 0.)),
        ));
        app.world_mut().spawn((
            ShipInfo { id: ShipID::from("shp1").unwrap(), ..Default::default() },
            Position(DVec3::ZERO),
        ));
        app.world_mut().spawn((
            ShipInfo { id: ShipID::from("shp2").unwrap(), ..Default::default() },
            Position(DVec3::new(5e4, 0., 0.)),
        ));
        app.add_systems(Update, ship_tick_broadcast.in_set(ScriptingSet::FireEvents));
        app.update();
        let bus = app.world().non_send_resource::<LuaEventBus>();
        assert!(
            bus.lua.globals().get::<bool>("has_body").unwrap_or(false),
            "ship_tick.bodies should contain distances to celestial bodies",
        );
        assert!(
            bus.lua.globals().get::<bool>("has_ship").unwrap_or(false),
            "ship_tick.ships should contain distances to other ships",
        );
    }

    #[test]
    fn test_ship_tick_excludes_self_from_ships_table() {
        let mut app = setup();
        {
            let mut bus = app.world_mut().non_send_resource_mut::<LuaEventBus>();
            bus.load_script(
                r#"
                on("ship_tick", function(data)
                    if data.ship_id == "shp1" then
                        _G.self_in_ships = data.ships["shp1"] ~= nil
                    end
                end)
                "#,
                "test",
            ).unwrap();
        }
        app.world_mut().spawn((
            ShipInfo { id: ShipID::from("shp1").unwrap(), ..Default::default() },
            Position(DVec3::ZERO),
        ));
        app.add_systems(Update, ship_tick_broadcast.in_set(ScriptingSet::FireEvents));
        app.update();
        let bus = app.world().non_send_resource::<LuaEventBus>();
        let self_in_ships: bool = bus.lua.globals().get("self_in_ships").unwrap_or(true);
        assert!(!self_in_ships, "a ship should not appear in its own ships table");
    }

    #[test]
    fn test_bridge_pause_game_sends_time_event() {
        let mut app = setup();
        app.add_systems(Update, lua_to_bevy_bridge.in_set(ScriptingSet::BridgeEvents));
        {
            let bus = app.world_mut().non_send_resource_mut::<LuaEventBus>();
            bus.fire("pause_game", Value::Nil);
        }
        app.update();
        let events = app.world().resource::<Events<TimeEvent>>();
        assert_eq!(events.len(), 1, "pause_game should send one TimeEvent");
    }

    #[test]
    fn test_bridge_resume_game_sends_time_event() {
        let mut app = setup();
        app.add_systems(Update, lua_to_bevy_bridge.in_set(ScriptingSet::BridgeEvents));
        {
            let bus = app.world_mut().non_send_resource_mut::<LuaEventBus>();
            bus.fire("resume_game", Value::Nil);
        }
        app.update();
        let events = app.world().resource::<Events<TimeEvent>>();
        assert_eq!(events.len(), 1, "resume_game should send one TimeEvent");
    }

    #[test]
    fn test_bridge_apply_thrust_sends_velocity_update() {
        let mut app = setup();
        app.add_systems(Update, lua_to_bevy_bridge.in_set(ScriptingSet::BridgeEvents));
        {
            let mut bus = app.world_mut().non_send_resource_mut::<LuaEventBus>();
            let t = bus.lua.create_table().unwrap();
            t.set("ship_id", "shp").unwrap();
            t.set("dx", 2.0_f64).unwrap();
            t.set("dy", 0.0_f64).unwrap();
            t.set("dz", 0.0_f64).unwrap();
            bus.fire("apply_thrust", Value::Table(t));
        }
        app.update();
        let events = app.world().resource::<Events<VelocityUpdate>>();
        assert_eq!(events.len(), 1, "apply_thrust should send one VelocityUpdate");
    }

    #[test]
    fn test_bridge_apply_thrust_missing_fields_is_ignored() {
        let mut app = setup();
        app.add_systems(Update, lua_to_bevy_bridge.in_set(ScriptingSet::BridgeEvents));
        {
            let mut bus = app.world_mut().non_send_resource_mut::<LuaEventBus>();
            // Table without dx/dy/dz fields
            let t = bus.lua.create_table().unwrap();
            t.set("ship_id", "shp").unwrap();
            bus.fire("apply_thrust", Value::Table(t));
        }
        app.update();
        let events = app.world().resource::<Events<VelocityUpdate>>();
        assert_eq!(events.len(), 0, "apply_thrust with missing fields should be silently ignored");
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct BridgePlugin;

impl Plugin for BridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Loaded), load_event_scripts)
            .add_systems(
                Update,
                ship_tick_broadcast
                    .in_set(ScriptingSet::FireEvents)
                    .run_if(in_state(Loaded))
                    .run_if(|t: Res<ToggleTime>| t.0),
            )
            .add_systems(
                Update,
                lua_to_bevy_bridge
                    .in_set(ScriptingSet::BridgeEvents)
                    .run_if(in_state(Loaded)),
            );
    }
}

use bevy::prelude::*;
use mlua::Value;

use crate::{
    game::Loaded,
    objects::{prelude::BodyInfo, ships::ShipInfo},
    physics::{prelude::{Position, ToggleTime}, time::TimeEvent},
    scripting::event_bus::{LuaEventBus, ScriptingSet},
};

// ── ship_tick broadcast ───────────────────────────────────────────────────────

/// Fires one `ship_tick` event per ship, per frame, while the simulation is running.
///
/// Lua event data:
///   data.ship_id           : string
///   data.distances         : table  { body_id → distance (km) }
fn ship_tick_broadcast(
    ships: Query<(&ShipInfo, &Position)>,
    bodies: Query<(&BodyInfo, &Position)>,
    bus: NonSendMut<LuaEventBus>,
) {
    for (ship, ship_pos) in ships.iter() {
        let Ok(distances) = bus.lua.create_table() else { continue };
        for (body_info, body_pos) in bodies.iter() {
            let d = (ship_pos.0 - body_pos.0).length();
            let _ = distances.set(body_info.0.id.to_string(), d);
        }

        let Ok(event_table) = bus.lua.create_table() else { continue };
        let _ = event_table.set("ship_id", ship.id.to_string());
        let _ = event_table.set("distances", distances);
        bus.fire("ship_tick", Value::Table(event_table));
    }
}

// ── Lua → Bevy bridge ─────────────────────────────────────────────────────────

/// Reads events emitted by the Lua bus and translates them into Bevy actions.
fn lua_to_bevy_bridge(
    mut bus: NonSendMut<LuaEventBus>,
    mut time_events: EventWriter<TimeEvent>,
) {
    for event_name in bus.emitted.drain(..) {
        match event_name.as_str() {
            "pause_game" => {
                time_events.send(TimeEvent::PauseTime);
            }
            "resume_game" => {
                time_events.send(TimeEvent::StartTime);
            }
            _ => {}
        }
    }
}

// ── Script loading ────────────────────────────────────────────────────────────

/// Loads all `.lua` files from `src/scripts/events/` into the event bus.
/// Runs once when the game enters the `Loaded` state.
fn load_event_scripts(mut bus: NonSendMut<LuaEventBus>) {
    let dir = std::path::Path::new("src/scripts/events");
    if !dir.exists() {
        warn!("[scripting] scripts directory '{}' not found", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "lua") {
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

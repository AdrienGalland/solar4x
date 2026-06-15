use std::{cell::RefCell, collections::HashMap, rc::Rc};

use bevy::prelude::*;
use mlua::{Function, Lua, Thread, Value};

// ── System ordering ──────────────────────────────────────────────────────────

/// Ordering within the scripting pipeline (all in `Update`):
///   FireEvents → ProcessEvents → BridgeEvents
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScriptingSet {
    /// Rust systems that fire events into the bus (e.g. proximity check).
    FireEvents,
    /// Drains the pending queue, runs Lua handlers/coroutines.
    ProcessEvents,
    /// Reads `emitted` and translates Lua events into Bevy actions.
    BridgeEvents,
}

// ── Waiter ───────────────────────────────────────────────────────────────────

struct Waiter {
    thread: Thread,
    waiting_for: String,
}

// ── LuaEventBus ──────────────────────────────────────────────────────────────

/// Persistent Lua state and event bus.
///
/// Stored as a NonSend resource because `mlua::Lua` is `!Send`.
/// To fire an event from a Rust system, add `NonSendMut<LuaEventBus>` as a
/// parameter and call `bus.fire(event, data)`.
pub struct LuaEventBus {
    /// Exposed so bridge systems can build Lua tables via `bus.lua.create_table()`.
    pub(crate) lua: Lua,
    handlers: HashMap<String, Vec<Function>>,
    waiters: Vec<Waiter>,
    /// Filled by `fire()` calls (from Lua and Rust); drained by `process_lua_events`.
    pending: Rc<RefCell<Vec<(String, Value)>>>,
    /// Every event name processed this frame — read by `BridgeEvents` systems.
    /// Cleared at the start of each `process_lua_events` run.
    pub emitted: Vec<String>,
}

impl LuaEventBus {
    fn new() -> mlua::Result<Self> {
        let lua = Lua::new();
        let pending: Rc<RefCell<Vec<(String, Value)>>> = Default::default();

        let pending_clone = pending.clone();
        lua.globals().set(
            "fire",
            lua.create_function(move |_, (event, data): (String, Value)| {
                pending_clone.borrow_mut().push((event, data));
                Ok(())
            })?,
        )?;

        lua.load("function wait_for(event) return coroutine.yield(event) end")
            .exec()?;

        Ok(Self {
            lua,
            handlers: HashMap::new(),
            waiters: vec![],
            pending,
            emitted: vec![],
        })
    }

    /// Load a Lua script and register all `on("event", fn)` handlers declared in it.
    /// Call once per script (e.g. when a ship spawns or at startup).
    pub fn load_script(&mut self, source: &str, name: &str) -> mlua::Result<()> {
        let collected: Rc<RefCell<Vec<(String, Function)>>> = Default::default();
        let collected_clone = collected.clone();

        let on_fn = self.lua.create_function(move |_, (event, handler): (String, Function)| {
            collected_clone.borrow_mut().push((event, handler));
            Ok(())
        })?;
        self.lua.globals().set("on", on_fn)?;

        self.lua.load(source).set_name(name).exec()?;

        for (event, handler) in collected.borrow_mut().drain(..) {
            self.handlers.entry(event).or_default().push(handler);
        }
        Ok(())
    }

    /// Fire an event from Rust.
    pub fn fire(&self, event: &str, data: Value) {
        self.pending.borrow_mut().push((event.to_string(), data));
    }

    /// Fire an event with no data payload.
    pub fn fire_empty(&self, event: &str) {
        self.fire(event, Value::Nil);
    }
}

// ── Processing system ─────────────────────────────────────────────────────────

fn process_lua_events(mut bus: NonSendMut<LuaEventBus>) {
    bus.emitted.clear();
    let events: Vec<(String, Value)> = bus.pending.borrow_mut().drain(..).collect();

    for (event_name, data) in events {
        bus.emitted.push(event_name.clone());

        // Resume coroutines waiting for this event
        let waiters = std::mem::take(&mut bus.waiters);
        let mut remaining = Vec::new();
        for waiter in waiters {
            if waiter.waiting_for == event_name {
                if let Some(next) = resume_thread(&waiter.thread, data.clone()) {
                    remaining.push(Waiter { thread: waiter.thread, waiting_for: next });
                }
            } else {
                remaining.push(waiter);
            }
        }
        bus.waiters = remaining;

        // Spawn new coroutines for registered handlers
        if let Some(handlers) = bus.handlers.get(&event_name).cloned() {
            for handler in handlers {
                match bus.lua.create_thread(handler) {
                    Ok(thread) => {
                        if let Some(next) = resume_thread(&thread, data.clone()) {
                            bus.waiters.push(Waiter { thread, waiting_for: next });
                        }
                    }
                    Err(e) => warn!("Failed to create Lua thread for '{event_name}': {e}"),
                }
            }
        }
    }
}

fn resume_thread(thread: &Thread, data: Value) -> Option<String> {
    match thread.resume::<Value>(data) {
        Ok(Value::String(s)) => s.to_str().ok().map(|b| b.to_string()),
        Ok(_) => None,
        Err(e) => {
            warn!("Lua coroutine error: {e}");
            None
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct EventBusPlugin;

impl Plugin for EventBusPlugin {
    fn build(&self, app: &mut App) {
        let bus = LuaEventBus::new().expect("Failed to initialize Lua event bus");
        app.insert_non_send_resource(bus)
            .configure_sets(
                Update,
                (ScriptingSet::FireEvents, ScriptingSet::ProcessEvents, ScriptingSet::BridgeEvents)
                    .chain(),
            )
            .add_systems(Update, process_lua_events.in_set(ScriptingSet::ProcessEvents));
    }
}

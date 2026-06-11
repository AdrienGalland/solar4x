use std::{cell::RefCell, collections::HashMap, rc::Rc};

use bevy::prelude::*;
use mlua::{Function, Lua, Thread, Value};

// ── Waiter ──────────────────────────────────────────────────────────────────

/// A coroutine suspended at a `wait_for(event)` call.
struct Waiter {
    thread: Thread,
    waiting_for: String,
}

// ── LuaEventBus ─────────────────────────────────────────────────────────────

/// Persistent Lua state and event bus.
///
/// Stored as a NonSend resource because `mlua::Lua` is `!Send`.
/// Other systems can fire events via `bus.fire(event, data)` by adding
/// `NonSendMut<LuaEventBus>` as a system parameter.
pub struct LuaEventBus {
    lua: Lua,
    /// handlers registered with `on("event", fn)` in Lua scripts
    handlers: HashMap<String, Vec<Function>>,
    /// coroutines suspended on `wait_for(...)`
    waiters: Vec<Waiter>,
    /// events pending processing this frame (filled by Lua `fire()` and `LuaEventBus::fire`)
    pending: Rc<RefCell<Vec<(String, Value)>>>,
}

impl LuaEventBus {
    fn new() -> mlua::Result<Self> {
        let lua = Lua::new();
        let pending: Rc<RefCell<Vec<(String, Value)>>> = Default::default();

        // `fire("event", data)` — callable from Lua scripts
        let pending_clone = pending.clone();
        lua.globals().set(
            "fire",
            lua.create_function(move |_, (event, data): (String, Value)| {
                pending_clone.borrow_mut().push((event, data));
                Ok(())
            })?,
        )?;

        // `wait_for("event")` — yields the current coroutine; resumes with event data
        lua.load("function wait_for(event) return coroutine.yield(event) end")
            .exec()?;

        Ok(Self {
            lua,
            handlers: HashMap::new(),
            waiters: vec![],
            pending,
        })
    }

    /// Load a Lua script and register all `on("event", fn)` handlers declared in it.
    ///
    /// Call this once per script file (e.g., when a ship is spawned).
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

    /// Fire an event from Rust (e.g., from a bridge system reacting to a Bevy event).
    pub fn fire(&self, event: &str, data: Value) {
        self.pending.borrow_mut().push((event.to_string(), data));
    }

    /// Convenience: fire an event with no data payload.
    pub fn fire_empty(&self, event: &str) {
        self.fire(event, Value::Nil);
    }

    /// Build a Lua table from key-value string pairs, useful for Rust-side `fire` calls.
    pub fn make_table<'a>(
        &'a self,
        fields: &[(&str, Value<'a>)],
    ) -> mlua::Result<Value<'a>> {
        let table = self.lua.create_table()?;
        for (k, v) in fields {
            table.set(*k, v.clone())?;
        }
        Ok(Value::Table(table))
    }
}

// ── Processing system ────────────────────────────────────────────────────────

fn process_lua_events(mut bus: NonSendMut<LuaEventBus>) {
    let events: Vec<(String, Value)> = bus.pending.borrow_mut().drain(..).collect();

    for (event_name, data) in events {
        // Resume coroutines that were waiting for this event
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

/// Resume a coroutine with `data`. Returns the next event name if the coroutine
/// yielded again (i.e., called `wait_for`), or `None` if it finished.
fn resume_thread(thread: &Thread, data: Value) -> Option<String> {
    match thread.resume::<Value>(data) {
        Ok(Value::String(s)) => Some(s.to_str().unwrap_or_default().to_string()),
        Ok(_) => None,
        Err(e) => {
            warn!("Lua coroutine error: {e}");
            None
        }
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────────

pub struct EventBusPlugin;

impl Plugin for EventBusPlugin {
    fn build(&self, app: &mut App) {
        let bus = LuaEventBus::new().expect("Failed to initialize Lua event bus");
        app.insert_non_send_resource(bus)
            .add_systems(Update, process_lua_events);
    }
}

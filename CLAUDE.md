# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build

# Run client (TUI + Bevy window)
cargo run --bin client

# Run server
cargo run --bin server

# Run with optional features
cargo run --bin client --features asteroids     # includes asteroids/comets
cargo run --bin client --features debug_display # enable debug overlay

# Run tests
cargo test

# Run a single test
cargo test test_handle_ship_create
cargo test -- --nocapture   # show println output
```

The client binary accepts an optional path to a custom keymap file (parsed via `src/utils/args.rs`). Default keybindings are in `keymap.toml`.

## Architecture

Solar4X is a 4X space game with a **split architecture**: a Bevy-based 2D GUI (`GuiPlugin`) renders the space simulation, while a Ratatui TUI handles all menus and interaction.

### Crate structure

The crate is a single library (`src/lib.rs`) with two binaries:
- `src/bin/client.rs` — composes `ClientPlugin + TuiPlugin + GuiPlugin`
- `src/bin/server.rs` — headless, runs `ServerPlugin` with a schedule runner

### Plugin hierarchy

```
ClientPlugin
└── GamePlugin
    ├── PhysicsPlugin
    │   ├── orbit::plugin      — elliptical orbit propagation
    │   ├── influence::plugin  — Hill sphere / gravitational influence
    │   ├── leapfrog::plugin   — Leapfrog integrator for free-motion ships
    │   └── time::plugin       — GameTime, ToggleTime, SimStepSize
    ├── BodiesPlugin           — celestial body loading from main_objects.json
    ├── ShipsPlugin
    │   └── trajectory::plugin — ManeuverNode dispatch & thrust application
    └── ScriptingPlugin        — Lua scripting for ships (src/scripting.rs)
        ├── EventBusPlugin     — Lua event bus with coroutine support
        └── BridgePlugin       — built-in events & Lua→Bevy bridge (src/scripting/bridge.rs)
TuiPlugin                      — Ratatui screens & input system sets
GuiPlugin                      — Bevy 2D rendering, camera, gizmos
```

### State machine

Key Bevy states:
- `ClientMode` (None / Singleplayer / Multiplayer / Explorer) — top-level mode
- `Loaded` (computed from `ClientMode`) — true when bodies are in the world
- `InGame` (computed from `ClientMode + Loaded`) — true in singleplayer or multiplayer when loaded
- `GameStage` (Preparation / Action, sub-state of `InGame`) — time is paused in Preparation, running in Action
- `Authoritative` (computed) — true when the instance owns simulation (singleplayer or server)
- `AppScreen` (StartMenu / Explorer / Fleet / Editor(ShipID) / Scheduler(ShipID)) — which TUI screen is shown

### Physics pipeline (FixedUpdate, gated by `ToggleTime`)

Order: `TimeUpdate → OrbitsUpdate → InfluenceUpdate → TrajectoryUpdate → LeapfrogUpdate`

- **Orbits** (`src/physics/orbit.rs`): propagates `EllipticalOrbit` components for all bodies and orbital ships
- **Influence** (`src/physics/influence.rs`): computes which bodies fall in each ship's Hill sphere (`Influenced` component)
- **Trajectory** (`src/objects/ships/trajectory.rs`): dispatches `ManeuverNode` thrusts at the right game-tick; reads from `gamefiles/trajectories/`
- **Leapfrog** (`src/physics/leapfrog.rs`): integrates free-motion ships using velocity Verlet

Lua scripts run in `FixedUpdate` **before** `TrajectoryUpdate`, gated by `in_state(Loaded)`.

Time is measured in **simulation ticks** (`GameTime.simtick`). `GAMETIME_PER_SIMTICK = 1e-3` days/tick; `STPS = 64` fixed updates/second. Changing `SimStepSize` speeds up simulation but changes outcomes; changing the update rate via `Time<Virtual>` does not.

### Objects

- **Bodies** (`src/objects/bodies/`): loaded from `main_objects.json` (or a custom JSON), filtered by `BodiesConfig` (by `BodyType` threshold or explicit IDs). The `PrimaryBody` marker identifies the star.
- **Ships** (`src/objects/ships.rs`): spawned via `ShipEvent::Create`. Free-motion ships carry `Influenced + Acceleration + Velocity`. Ships with negative specific orbital energy can optionally switch to `EllipticalOrbit` (orbital mode), losing `Influenced`/`Acceleration`. Trajectories (sequences of `ManeuverNode`) are serialised to `gamefiles/trajectories/<ship_id>.json`.
- **Predictions** (`src/physics/predictions.rs`): computed on the client to draw trajectory previews in the GUI.

### Lua scripting (`src/scripting.rs`)

Ships can be controlled by Lua scripts. Each tick, `run_ship_scripts` looks for a script for each ship:
1. Explicit path via the `ShipScript` component on the ship entity.
2. Default path `src/scripts/ships/<ship_id>.lua` if the file exists.

**Globals exposed to ship scripts:**
- `ship` — table with `id`, `position`, `velocity` (vec3)
- `body(id)` — returns a table with `id`, `position`, `velocity` for a celestial body, or `nil`
- `vec3(x, y, z)` — construct a 3D vector table
- `length(v)`, `distance(a, b)`, `normalize(v)` — vector math
- `apply_global_thrust(v)` — queue a `VelocityUpdate` for this tick

Scripts are stateless per tick (a fresh `Lua` VM is created each call). For stateful/event-driven logic, use the event bus instead.

### Lua event bus (`src/scripting/event_bus.rs`)

`LuaEventBus` is a persistent `NonSend` resource (Lua is `!Send`) that supports event-driven scripts with coroutine suspension:

- `on("event", fn)` — register a handler (called as a coroutine)
- `wait_for("event")` — yield the current coroutine until the named event fires
- `fire("event", data)` — queue an event from Lua
- `LuaEventBus::fire` / `fire_empty` — fire events from Rust systems

Events are processed each frame by the `process_lua_events` system (`Update` schedule). Rust systems can access the bus via `NonSendMut<LuaEventBus>`.

System ordering within `Update`: `FireEvents → ProcessEvents → BridgeEvents`.

**Event scripts** are loaded once from `src/scripts/events/` on `OnEnter(Loaded)`. Each `.lua` file in that directory is executed immediately (registering `on(...)` handlers); the handlers then run as coroutines each time the matching event fires.

**Built-in events (fired by `BridgePlugin`):**
- `ship_tick` — fired once per ship per frame while the simulation is running; data: `{ ship_id: string, distances: { body_id → number } }`

**Events handled by `BridgePlugin` (fire from Lua to control the game):**
- `pause_game` — sends `TimeEvent::PauseTime`
- `resume_game` — sends `TimeEvent::StartTime`

### UI layers

Both the TUI and the simulation now render in a **single Bevy window** — no terminal window.

- **TUI overlay** (`src/ui/tui_overlay.rs`): Ratatui renders to a `TestBackend` buffer (80×30 chars, top-left of the window). Each frame the buffer is converted to Bevy `Text` entities and displayed as a Bevy UI overlay (`ZIndex::Global(100)`). Font: `assets/fonts/mono.ttf`.
- **TUI screens** (`src/ui/screen/`): `StartMenu`, `Explorer`, `Fleet`, `Editor`, `Scheduler`. Each screen renders to the fixed `TUI_COLS×TUI_ROWS` area (top-left). Screens with popups (currently only `Fleet`) have a separate widget (`FleetPopup`) that renders centered on the full window.
- **TUI input** (`src/ui/tui_input.rs`): Bevy `KeyboardInput` events are converted to `bevy_ratatui::event::KeyEvent` (crossterm) and dispatched as before — no terminal raw mode, no crossterm event polling.
- **Bevy GUI** (`src/ui/gui/`): 2D Bevy window showing orbits, ship positions and prediction gizmos. Mouse pan (hold LMB) and click-to-select are handled here.
- **Widgets** (`src/ui/widget/`): reusable Ratatui widgets — `SpaceMap` (ASCII minimap), `SearchWidget`, `TreeWidget`, `InfoWidget`.

### Networking

`bevy_quinnet` (QUIC) is used for client/server. The server sends `BodiesConfig` and periodic `UpdateTime` ticks. Client channels are currently one-way (server → client only for game state sync). Server runs headlessly at `127.0.0.1:6000`.

### Game files

Runtime data lives under `gamefiles/` (constant `GAME_FILES_PATH`). In tests, a `TempDirectory` is used instead so tests are isolated and don't touch disk.

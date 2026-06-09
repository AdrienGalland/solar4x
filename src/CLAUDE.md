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
    └── ShipsPlugin
        └── trajectory::plugin — ManeuverNode dispatch & thrust application
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

Time is measured in **simulation ticks** (`GameTime.simtick`). `GAMETIME_PER_SIMTICK = 1e-3` days/tick; `STPS = 64` fixed updates/second. Changing `SimStepSize` speeds up simulation but changes outcomes; changing the update rate via `Time<Virtual>` does not.

### Objects

- **Bodies** (`src/objects/bodies/`): loaded from `main_objects.json` (or a custom JSON), filtered by `BodiesConfig` (by `BodyType` threshold or explicit IDs). The `PrimaryBody` marker identifies the star.
- **Ships** (`src/objects/ships.rs`): spawned via `ShipEvent::Create`. Free-motion ships carry `Influenced + Acceleration + Velocity`. Ships with negative specific orbital energy can optionally switch to `EllipticalOrbit` (orbital mode), losing `Influenced`/`Acceleration`. Trajectories (sequences of `ManeuverNode`) are serialised to `gamefiles/trajectories/<ship_id>.json`.
- **Predictions** (`src/physics/predictions.rs`): computed on the client to draw trajectory previews in the GUI.

### UI layers

- **TUI screens** (`src/ui/screen/`): `StartMenu`, `Explorer`, `Fleet`, `Editor`, `Scheduler`. Each screen has a `*Context` resource and a `*Screen` widget. Screens are rendered in `PostUpdate/RenderSet`.
- **Bevy GUI** (`src/ui/gui/`): 2D Bevy window showing orbits, ship positions and prediction gizmos. Mouse pan (hold LMB) and click-to-select are handled here.
- **Widgets** (`src/ui/widget/`): reusable Ratatui widgets — `SpaceMap` (ASCII minimap), `SearchWidget`, `TreeWidget`, `InfoWidget`.

### Networking

`bevy_quinnet` (QUIC) is used for client/server. The server sends `BodiesConfig` and periodic `UpdateTime` ticks. Client channels are currently one-way (server → client only for game state sync). Server runs headlessly at `127.0.0.1:6000`.

### Game files

Runtime data lives under `gamefiles/` (constant `GAME_FILES_PATH`). In tests, a `TempDirectory` is used instead so tests are isolated and don't touch disk.

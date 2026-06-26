use bevy::prelude::*;

use crate::utils::Direction2;

/// Number of server updates (ticks) per real time second
// pub const TPS: f32 = 1.; comment
/// Number of simulation updates (simticks) per real time second
pub const STPS: f64 = 64.;

/// Game time that is added at each client update (in days, multiplied by simstepsize)
pub const GAMETIME_PER_SIMTICK: f64 = 1e-3;

pub const SIMTICKS_PER_TICK: u64 = 10;

pub fn plugin(app: &mut App) {
    app.insert_resource(Time::<Fixed>::from_hz(STPS))
        .init_resource::<ToggleTime>()
        .init_resource::<GameTime>()
        .init_resource::<SimStepSize>()
        .add_event::<TimeEvent>()
        .add_event::<TickEvent>()
        .add_systems(
            FixedUpdate,
            (update_simtick, update_tick).in_set(TimeUpdate),
        )
        .add_systems(Update, handle_time_events);
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TimeUpdate;

#[derive(Resource, PartialEq, Default)]
pub struct ToggleTime(pub bool);

/// The elapsed time in game (stores simulation ticks)
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct GameTime {
    pub simtick: u64,
}

impl GameTime {
    pub fn time(&self) -> f64 {
        self.simtick as f64 * GAMETIME_PER_SIMTICK
    }

    pub fn tick(&self) -> u64 {
        self.simtick / SIMTICKS_PER_TICK
    }
}

/// The number of simticks that are added at each update
#[derive(Resource)]
pub struct SimStepSize(pub u64);

impl Default for SimStepSize {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Event, Default)]
pub struct TickEvent;

#[derive(Event, Clone, Copy)]
pub enum TimeEvent {
    /// Change the number of simticks that are simulated per update.
    ///
    /// **THIS CAN CHANGE SIMULATION OUTCOME**
    ChangeStepSize(Direction2),
    /// Change the number of updates that are processed per real time second.
    ///
    /// This does not change simulation outcome, but leads to heavier CPU load.
    ChangeUpdateRate(Direction2),
    ToggleTime,
    /// Force the simulation to start.
    StartTime,
    /// Force the simulation to pause.
    PauseTime,
}

fn update_tick(mut writer: EventWriter<TickEvent>, game_time: Res<GameTime>) {
    if game_time.simtick % SIMTICKS_PER_TICK == 0 {
        writer.send_default();
    }
}

pub(crate) fn update_simtick(mut game_time: ResMut<GameTime>, step: Res<SimStepSize>) {
    game_time.simtick += step.0;
}

fn handle_time_events(
    mut reader: EventReader<TimeEvent>,
    mut toggle_time: ResMut<ToggleTime>,
    mut time: ResMut<Time<Virtual>>,
    mut step_size: ResMut<SimStepSize>,
) {
    use TimeEvent::*;
    for event in reader.read() {
        match event {
            ChangeUpdateRate(d) => {
                let speed = time.relative_speed_f64();
                time.set_relative_speed_f64(match d {
                    Direction2::Up => speed * 2.,
                    Direction2::Down => speed / 2.,
                })
            }
            ChangeStepSize(d) => match d {
                Direction2::Up => step_size.0 *= 2,
                Direction2::Down => step_size.0 /= 2,
            },
            ToggleTime => toggle_time.0 = !toggle_time.0,
            StartTime => toggle_time.0 = true,
            PauseTime => toggle_time.0 = false,
        }
    }
}

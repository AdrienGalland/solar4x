use std::collections::BTreeMap;

use bevy::{math::DVec3, prelude::*};
use bevy_ratatui::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    widgets::{Block, List, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::{
    objects::{
        orbiting_obj::OrbitingObjects, ships::{trajectory::ManeuverNode,
            // DisableShipOrbitCheck, HostBody
            }
}, 
    physics::
    {
        // influence::HillRadius, leapfrog::get_acceleration,
        time::{SIMTICKS_PER_TICK, TimeEvent}},
    prelude::*,
};

use super::AppScreen;

pub mod editor_backend;

pub fn plugin(app: &mut App) {
    app.add_plugins(editor_backend::plugin)
        .add_computed_state::<InEditor>()
        .add_event::<EditorEvents>()
        .add_systems(
            Update,
            (
                read_input.in_set(InputReading),
                ((
                    handle_select_prediction.run_if(resource_exists::<Events<SelectObjectEvent>>),
                    handle_editor_events,
                )
                    .chain(),)
                    .in_set(EventHandling),
            )
                .run_if(in_state(InEditor))
                .run_if(resource_exists::<EditorContext>),
        )
        .add_systems(OnEnter(InEditor), create_screen)
        .add_systems(Update, update_editor_context.run_if(in_state(InEditor)).run_if(resource_exists::<EditorContext>))
        .add_systems(OnExit(InEditor), clear_screen);
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InEditor;

impl ComputedStates for InEditor {
    type SourceStates = AppScreen;

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            AppScreen::Editor(_) => Some(Self),
            _ => None,
        }
    }
}

#[derive(Resource)]
pub struct EditorContext {
    pub ship: Entity,
    pub ship_info: ShipInfo,
    pub pos: DVec3,
    pub speed: DVec3,
    pub simtick: u64,
    pub current_simtick: u64,
    pub game_time: f64,
    pub time_running: bool,
    list_state: ListState,
    /// Each maneuver node is stored here along with the associated tick, and corresponds to a prediction.
    /// Since there is a prediction for each tick, the index of the prediction is simply the number of ticks
    /// that separate the start from the maneuver node
    nodes: BTreeMap<u64, ManeuverNode>,
    predictions: Vec<Entity>,
    /// These predictions start from a maneuver node that is currently being edited. At the end of edition,
    /// the true predictions after the node are replaced by these temporary ones
    temp_predictions: Vec<Entity>,
    /// This field stores the thrust that will be added to a node when we are editing one
    editing_data: Option<DVec3>,
}

impl EditorContext {
    pub fn new(
        ship: Entity,
        ship_info: ShipInfo,
        &Position(pos): &Position,
        &Velocity(speed): &Velocity,
        tick: u64,
        time: &GameTime,
        toggle_time: &ToggleTime,
    ) -> Self {
        Self {
            ship,
            ship_info,
            pos,
            speed,
            simtick: tick,
            current_simtick: time.simtick,
            game_time: time.time(),
            time_running: toggle_time.0,
            list_state: ListState::default(),
            nodes: BTreeMap::new(),
            predictions: Vec::new(),
            temp_predictions: Vec::new(),
            editing_data: None,
        }
    }

    pub fn selected_node(&self) -> Option<&ManeuverNode> {
        self.selected_entry().map(|(_, n)| n)
    }
    pub fn selected_node_mut(&mut self) -> Option<&mut ManeuverNode> {
        self.selected_entry_mut().map(|(_, n)| n)
    }

    pub fn selected_tick(&self) -> Option<u64> {
        self.selected_entry().map(|(t, _)| *t)
    }

    /// Attempts to select the node at the provided tick, returning the index if successful
    pub fn select_tick(&mut self, tick: u64) -> Option<usize> {
        self.index_of_tick(tick)
            .inspect(|&i| self.list_state.select(Some(i)))
    }

    pub fn index_of_tick(&self, tick: u64) -> Option<usize> {
        self.nodes.keys().position(|t| *t == tick)
    }

    pub fn selected_entry(&self) -> Option<(&u64, &ManeuverNode)> {
        self.list_state
            .selected()
            .and_then(|i| self.nodes.iter().nth(i))
    }

    pub fn selected_entry_mut(&mut self) -> Option<(&u64, &mut ManeuverNode)> {
        self.list_state
            .selected()
            .and_then(|i| self.nodes.iter_mut().nth(i))
    }

    fn index_of_prediction_at_simtick(&self, simtick: u64) -> usize {
        simtick.saturating_sub(self.simtick) as usize
    }

    fn prediction_at_simtick(&self, tick: u64) -> Option<Entity> {
        self.predictions
            .get(self.index_of_prediction_at_simtick(tick))
            .cloned()
    }

    pub fn selected_prediction_entity(&self) -> Option<Entity> {
        self.selected_tick()
            .and_then(|t| self.prediction_at_simtick(SIMTICKS_PER_TICK * t))
    }

    pub fn get_node(&self, tick: u64) -> Option<&ManeuverNode> {
        self.nodes.get(&tick)
    }

    pub fn select_or_insert(&mut self, tick: u64, default: ManeuverNode) {
        self.nodes.entry(tick).or_insert(default);
        self.select_tick(tick);
    }

    pub fn change_tick(&mut self, tick: u64, newtick: u64) {
        self.nodes
            .remove(&tick)
            .map(|val| self.nodes.insert(newtick, val));
    }
}
impl ClampedList for EditorContext {
    fn list_state(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }
}

pub struct EditorScreen;

#[allow(clippy::too_many_arguments)]
fn create_screen(
    mut commands: Commands,
    // mut writer: EventWriter<ShipEvent>,
    screen: Res<State<AppScreen>>,
    ships: Query<(&ShipInfo, &Position, &Velocity)>,
    ships_mapping: Res<ShipsMapping>,
    bodies_mapping: Res<BodiesMapping>,
    bodies: Query<(&BodyInfo, &OrbitingObjects)>,
    // pos_mass: Query<(&Position, &Mass), With<OrbitingObjects>>,
    // influencing_bodies: Query<(&Position, &HillRadius, &OrbitingObjects)>,
    system_size: Res<SystemSize>,
    influenced: Query<&Influenced>,
    // host_bodies: Query<(&HostBody, &Position)>,
    // primary_body: Query<&BodyInfo, With<PrimaryBody>>,
    time: Res<GameTime>,
    mut toggle_time: ResMut<ToggleTime>,
) {
    // let main_body = primary_body.get_single().unwrap().0.id;
    if let AppScreen::Editor(id) = screen.get() {
        if let Some(e) = ships_mapping.0.get(id) {

            let host_body = if let Ok(influence) = influenced.get(*e) {
                influence.main_influencer

            } 
            // else if let Ok((host_body, position)) = host_bodies.get(*e) {
            //     let influence = Influenced::new(position, &influencing_bodies, &bodies_mapping, main_body);
            //     let acc = Acceleration::new(get_acceleration(
            //                     position.0,
            //                     pos_mass
            //                         .iter_many(&influence.influencers)
            //                         .map(|(p, m)| (p.0, m.0)),
            //     ));
            //     commands.entity(*e).insert((influence, acc));
            //     let host_body = bodies_mapping.0.get(&host_body.0).copied();
            //     commands.entity(*e).remove::<(HostBody, EllipticalOrbit, OrbitingObjects)>();
            //     host_body
            // } 
            else {
                return;
            };

            let (
                info,
                pos,
                speed,
            ) = ships.get(*e).unwrap();

            toggle_time.0 = false;
            commands.insert_resource(EditorContext::new(
                *e,
                info.clone(),
                pos,
                speed,
                time.simtick,
                &time,
                &toggle_time,
            ));
            let mut map = SpaceMap::new(system_size.0, host_body, host_body);
            map.autoscale(&bodies_mapping.0, &bodies);
            commands.insert_resource(map);
            // writer.send(ShipEvent::SwitchToFreeMotion(*id));
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct ClearOnEditorExit;

fn clear_screen(
    mut commands: Commands, 
    query: Query<Entity, With<ClearOnEditorExit>>,
    // mut disable_orbit_check: ResMut<DisableShipOrbitCheck>, 
) {
    commands.remove_resource::<EditorContext>();
    commands.remove_resource::<SpaceMap>();
    query.iter().for_each(|e| commands.entity(e).despawn());
    // disable_orbit_check.0 = false;
}

fn read_input(
    mut key_event: EventReader<KeyEvent>,
    keymap: Res<Keymap>,
    mut internal_event: EventWriter<EditorEvents>,
    mut time_event_writer: EventWriter<TimeEvent>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    use Direction2::*;
    use EditorEvents::*;
    let keymap = &keymap.editor;
    for event in key_event.read() {
        if event.kind == KeyEventKind::Release {
            return;
        }
        if keymap.run_time.matches(event) {
            time_event_writer.send(TimeEvent::StartTime);
        } else if keymap.pause_time.matches(event) {
            time_event_writer.send(TimeEvent::PauseTime);
        } else if keymap.select_next.matches(event) {
            internal_event.send(SelectAdjacent(Down));
        } else if keymap.select_previous.matches(event) {
            internal_event.send(SelectAdjacent(Up));
        } else if keymap.open_scheduler.matches(event) {
            // next_screen.set(AppScreen::Scheduler(context.ship_info.id));
        } else if keymap.back.matches(event) {
            next_screen.set(AppScreen::Fleet);
        }
    }
}

#[derive(Event, Clone, Copy)]
pub enum EditorEvents {
    SelectAdjacent(Direction2),
    SelectNearestOrInsert(u64),
    CreateSchedule(ShipID),
}

fn update_editor_context(
    time: Res<GameTime>,
    toggle_time: Res<ToggleTime>,
    ships: Query<(&Position, &Velocity), With<ShipInfo>>,
    mut context: ResMut<EditorContext>,
    mut reload: EventWriter<editor_backend::ReloadPredictions>,
) {
    if context.current_simtick != time.simtick {
        if let Ok((&Position(pos), &Velocity(speed))) = ships.get(context.ship) {
            context.pos = pos;
            context.speed = speed;
            context.simtick = time.simtick;p
            reload.send_default();
        }
    }
    context.game_time = time.time();
    context.time_running = toggle_time.0;
    context.current_simtick = time.simtick;
}

fn handle_editor_events(
    mut context: ResMut<EditorContext>,
    mut events: EventReader<EditorEvents>,
    bodies: Query<&BodyInfo>,
    primary: Query<&BodyInfo, With<PrimaryBody>>,
    space_map: Res<SpaceMap>,
) {
    for event in events.read() {
        match *event {
            EditorEvents::SelectAdjacent(d) => context.select_adjacent(d),
            EditorEvents::SelectNearestOrInsert(simtick) => {
                let origin = space_map
                    .focus_body
                    .map_or(primary.single().0.id, |e| bodies.get(e).unwrap().0.id);
                context.select_or_insert(
                    simtick / SIMTICKS_PER_TICK,
                    ManeuverNode {
                        name: "Node".into(),
                        thrust: DVec3::ZERO,
                        origin,
                    },
                );
            }
            _=> return,
        }
    }
}

fn handle_select_prediction(
    mut select_events: EventReader<SelectObjectEvent>,
    mut editor_events: EventWriter<EditorEvents>,
    predictions: Query<&Prediction>,
) {
    for event in select_events.read() {
        if let Ok(p) = predictions.get(event.entity) {
            editor_events.send(EditorEvents::SelectNearestOrInsert(p.simtick));
        }
    }
}

impl StatefulWidget for EditorScreen {
    type State = EditorContext;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let outer_chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
        Paragraph::new("Trajectory editor — n: new node  ·  s: scheduler  ·  r: run  ·  p: pause  ·  Esc: back")
            .alignment(Alignment::Center)
            .render(outer_chunks[0], buf);

        let chunks = Layout::horizontal([Constraint::Percentage(30), Constraint::Fill(1)])
            .split(outer_chunks[1]);
        let list = List::new(state.nodes.values().map(|n| &n.name[..]))
            .highlight_symbol(">")
            .block(Block::bordered().title_top(format!(
                "Maneuver nodes — {:.3} d ({})",
                state.game_time,
                if state.time_running { "Running" } else { "Paused" }
            )));
        StatefulWidget::render(list, chunks[0], buf, &mut state.list_state);

        if let Some((tick, node)) = state.selected_entry() {
            Paragraph::new(format!(
                "Tick: {}\nTime: {:.3} d\nStatus: {}\nThrust: {}\nOrigin: {}\n\nKeys: Up/Down move  ·  n new node  ·  s scheduler  ·  r run  ·  p pause  ·  Esc back",
                tick,
                state.game_time,
                if state.time_running { "Running" } else { "Paused" },
                node.thrust,
                node.origin
            ))
            .render(chunks[1], buf);
        } else {
            Paragraph::new("No node selected. Use n to create a new node or select one in the list.")
                .render(chunks[1], buf);
        }
    }
}

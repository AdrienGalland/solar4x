use std::{error::Error, num::ParseFloatError, path::PathBuf};

use arrayvec::CapacityError;
use bevy::prelude::*;
use bevy_ratatui::event::KeyEvent;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::Stylize,
    widgets::{Block, Clear, List, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};

use crate::{
    game::GameFiles,
    objects::{
        id::MAX_ID_LENGTH,
        ships::{
            config::{list_ship_configs, load_ship_config, ShipConfig},
            trajectory::TrajectoryEvent,
        },
    },
    physics::time::TimeEvent,
    prelude::*,
    scripting::bridge::InjectComponentsEvent,
    ui::UiUpdate,
    utils::{algebra::circular_orbit_around_body, list::OptionsList},
};

pub fn plugin(app: &mut App) {
    app.add_event::<FleetScreenEvent>()
        .add_systems(
            Update,
            (
                read_input.in_set(InputReading),
                handle_fleet_events
                    .pipe(exit_on_error_if_app)
                    .in_set(EventHandling),
            )
                .run_if(in_loaded_screen::<FleetContext>(AppScreen::Fleet)),
        )
        .add_systems(
            PostUpdate,
            update_fleet_context
                .run_if(state_exists::<GameStage>)
                .run_if(
                    state_changed::<GameStage>.or_else(resource_exists_and_changed::<ShipsMapping>),
                )
                .in_set(UiUpdate),
        )
        .add_systems(OnEnter(InGame), (create_screen, create_fleet_space_map).chain())
        .add_systems(
            OnExit(InGame),
            (
                clear_screen.run_if(not(in_state(AppScreen::Fleet))),
                |mut commands: Commands| commands.remove_resource::<SpaceMap>(),
            ),
        )
        .add_systems(
            OnEnter(AppScreen::Fleet),
            create_fleet_space_map.run_if(in_state(Loaded)),
        )
        .add_systems(
            Update,
            handle_fleet_focus
                .run_if(in_loaded_screen::<FleetContext>(AppScreen::Fleet))
                .run_if(on_event::<SelectObjectEvent>())
                .run_if(resource_exists::<SpaceMap>),
        );
}

fn create_fleet_space_map(
    mut commands: Commands,
    system_size: Res<SystemSize>,
    primary: Query<Entity, With<PrimaryBody>>,
    bodies_mapping: Res<BodiesMapping>,
    bodies: Query<(&BodyInfo, &OrbitingObjects)>,
) {
    let primary_entity = primary.get_single().ok();
    let mut map = SpaceMap::new(system_size.0, primary_entity, primary_entity);
    map.autoscale(&bodies_mapping.0, &bodies);
    commands.insert_resource(map);
}

fn handle_fleet_focus(
    mut events: EventReader<SelectObjectEvent>,
    bodies: Query<&BodyInfo>,
    mut space_map: ResMut<SpaceMap>,
) {
    for event in events.read() {
        if bodies.get(event.entity).is_ok() {
            space_map.focus(event.entity);
            space_map.selected = Some(event.entity);
        }
    }
}

fn create_screen(
    mut commands: Commands,
    mut next_screen: ResMut<NextState<AppScreen>>,
    ships: Query<&ShipInfo>,
) {
    commands.insert_resource(FleetContext::new(ships.iter().cloned()));
    next_screen.set(AppScreen::Fleet);
}

fn clear_screen(mut commands: Commands) {
    commands.remove_resource::<FleetContext>();
}

#[derive(Clone)]
pub struct LoadShipContext {
    configs: Vec<(String, PathBuf)>,
    list_state: ListState,
    error: Option<String>,
}

impl LoadShipContext {
    fn new(dir: &std::path::Path) -> Self {
        Self {
            configs: list_ship_configs(dir),
            list_state: ListState::default(),
            error: None,
        }
    }
    fn selected(&self) -> Option<&PathBuf> {
        self.list_state.selected().map(|i| &self.configs[i].1)
    }
}

impl ClampedList for LoadShipContext {
    fn list_state(&mut self) -> &mut ListState { &mut self.list_state }
    fn len(&self) -> usize { self.configs.len() }
}

#[derive(Clone)]
pub enum FleetPopupKind {
    CreateShip(CreateShipContext),
    LoadShip(LoadShipContext),
}

#[derive(Resource, Default)]
pub struct FleetContext {
    list_state: ListState,
    ships: Vec<ShipInfo>,
    popup: Option<FleetPopupKind>,
    stage: GameStage,
    game_time: f64,
    time_running: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Event, Clone)]
pub enum FleetScreenEvent {
    Select(Direction2),
    TryNewShip(CreateShipContext),
    LoadShip(ShipConfig),
    EditTrajectory,
    EditComponents,
    EnterExplorer,
    Back,
}

#[derive(Clone, Debug)]
pub enum ShipCreationError {
    ParseError(ParseFloatError),
    IDTooLong,
    ShipAlreadyExists(ShipID),
}

impl From<ParseFloatError> for ShipCreationError {
    fn from(value: ParseFloatError) -> Self {
        Self::ParseError(value)
    }
}

impl From<CapacityError> for ShipCreationError {
    fn from(_value: CapacityError) -> Self {
        Self::IDTooLong
    }
}

impl Error for ShipCreationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ShipCreationError::ParseError(e) => Some(e),
            _ => None,
        }
    }
}

impl std::fmt::Display for ShipCreationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShipCreationError::ParseError(e) => {
                write!(f, "Parsing error while creating ship: {}", e)
            }
            ShipCreationError::ShipAlreadyExists(id) => write!(
                f,
                "Couldn't create ship with id \"{}\" because it already exists",
                id
            ),
            ShipCreationError::IDTooLong => write!(
                f,
                "Couldn't create ship because id is too long (max length = {})",
                MAX_ID_LENGTH
            ),
        }
    }
}

impl ClampedList for FleetContext {
    fn list_state(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    fn len(&self) -> usize {
        self.ships.len()
    }
}

#[derive(Default, Clone)]
pub struct CreateShipContext {
    id_text: String,
    host_body: String,
    altitude: String,
    pos_x: String,
    pos_y: String,
    pos_z: String,
    speed_x: String,
    speed_y: String,
    speed_z: String,
    selected: usize,
}

impl OptionsList<9> for CreateShipContext {
    fn current_index(&mut self) -> &mut usize {
        &mut self.selected
    }

    fn fields_list(&mut self) -> [(&mut String, String); 9] {
        [
            (&mut self.id_text, "Ship ID".into()),
            // TODO: add search or tree widget instead of plain id
            (&mut self.host_body, "Host body id".into()),
            (&mut self.altitude, "Spawn Altitude".into()),
            (&mut self.pos_x, "Spawn x".into()),
            (&mut self.pos_y, "Spawn y".into()),
            (&mut self.pos_z, "Spawn z".into()),
            (&mut self.speed_x, "Velocity x".into()),
            (&mut self.speed_y, "Velocity y".into()),
            (&mut self.speed_z, "Velocity z".into()),
        ]
    }
}

impl CreateShipContext {
    fn to_info<'a>(
        &self,
        mut ships: impl Iterator<Item = &'a ShipInfo>,
        bodies: &Query<(&Mass, &Position, &Velocity)>,
        mapping: &BodiesMapping,
    ) -> Result<ShipInfo, ShipCreationError> {
        let CreateShipContext {
            id_text,
            host_body,
            altitude,
            pos_x,
            pos_y,
            pos_z,
            speed_x,
            speed_y,
            speed_z,
            ..
        } = self;
        let (spawn_pos, spawn_speed) =
            if let Some(body) = BodyID::from(host_body).ok().and_then(|i| mapping.0.get(&i)) {
                let (Mass(m), Position(p), Velocity(v)) = bodies.get(*body).unwrap();
                circular_orbit_around_body(altitude.parse()?, *m, *p, *v)
            } else {
                (
                    (pos_x.parse()?, pos_y.parse()?, pos_z.parse()?).into(),
                    (speed_x.parse()?, speed_y.parse()?, speed_z.parse()?).into(),
                )
            };
        let id = ShipID::from(id_text).map_err(CapacityError::simplify)?;
        if ships.any(|s| s.id == id) {
            Err(ShipCreationError::ShipAlreadyExists(id))
        } else {
            Ok(ShipInfo {
                id,
                spawn_pos,
                spawn_speed,
            })
        }
    }
}

impl FleetContext {
    pub fn new(ships: impl Iterator<Item = ShipInfo>) -> Self {
        Self {
            ships: ships.collect(),
            ..Default::default()
        }
    }
    pub fn has_popup(&self) -> bool {
        self.popup.is_some()
    }
    fn selected_ship(&self) -> Option<&ShipInfo> {
        self.list_state.selected().map(|i| &self.ships[i])
    }
}

pub struct FleetScreen;

/// Renders only the popup, centered on the full window area.
pub struct FleetPopup;

fn read_input(
    mut context: ResMut<FleetContext>,
    mut key_event: EventReader<KeyEvent>,
    keymap: Res<Keymap>,
    mut internal_event: EventWriter<FleetScreenEvent>,
    mut time_events: EventWriter<TimeEvent>,
    game_files: Res<GameFiles>,
) {
    use Direction2::*;
    use FleetScreenEvent::*;
    let keymap = &keymap.fleet_screen;
    for KeyEvent(event) in key_event.read() {
        if event.kind == KeyEventKind::Release {
            return;
        }
        match &mut context.popup {
            None => match event {
                e if keymap.select_next.matches(e) => {
                    internal_event.send(Select(Down));
                }
                e if keymap.select_previous.matches(e) => {
                    internal_event.send(Select(Up));
                }
                e if keymap.edit_trajectory.matches(e) => {
                    internal_event.send(EditTrajectory);
                }
                e if keymap.run_time.matches(e) => {
                    time_events.send(TimeEvent::StartTime);
                }
                e if keymap.pause_time.matches(e) => {
                    time_events.send(TimeEvent::PauseTime);
                }
                e if keymap.new_ship.matches(e) => {
                    context.popup = Some(FleetPopupKind::CreateShip(CreateShipContext::default()));
                }
                e if keymap.load_ship.matches(e) => {
                    context.popup = Some(FleetPopupKind::LoadShip(LoadShipContext::new(&game_files.ships)));
                }
                e if keymap.edit_components.matches(e) => {
                    internal_event.send(EditComponents);
                }
                e if keymap.back.matches(e) => {
                    internal_event.send(Back);
                }
                e if keymap.enter_explorer.matches(e) => {
                    internal_event.send(EnterExplorer);
                }
                _ => {}
            },
            Some(FleetPopupKind::CreateShip(ctx)) => match event {
                e if keymap.cycle_options.matches(e) => ctx.select_next(),
                e if keymap.cycle_options_back.matches(e) => ctx.select_previous(),
                e if keymap.back.matches(e) => context.popup = None,
                e if keymap.validate_new_ship.matches(e) => {
                    internal_event.send(TryNewShip(ctx.clone()));
                }
                e if keymap.delete_char.matches(e) => {
                    ctx.selected_field().pop();
                }
                crossterm::event::KeyEvent {
                    code: KeyCode::Char(c),
                    ..
                } => ctx.selected_field().push(*c),
                _ => {}
            },
            Some(FleetPopupKind::LoadShip(ctx)) => match event {
                e if keymap.select_next.matches(e) => ctx.select_adjacent(Down),
                e if keymap.select_previous.matches(e) => ctx.select_adjacent(Up),
                e if keymap.back.matches(e) => context.popup = None,
                e if keymap.validate_new_ship.matches(e) => {
                    if let Some(path) = ctx.selected().cloned() {
                        match load_ship_config(&path) {
                            Ok(config) => { internal_event.send(LoadShip(config)); }
                            Err(e) => {
                                if let Some(FleetPopupKind::LoadShip(ctx)) = &mut context.popup {
                                    ctx.error = Some(format!("Erreur: {e}"));
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
        }
    }
}

fn handle_fleet_events(
    mut context: ResMut<FleetContext>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut next_mode: ResMut<NextState<ClientMode>>,
    mut events: EventReader<FleetScreenEvent>,
    mut ship_events: EventWriter<ShipEvent>,
    mut trajectory_events: EventWriter<TrajectoryEvent>,
    mut inject_events: EventWriter<InjectComponentsEvent>,
    bodies: Query<(&Mass, &Position, &Velocity)>,
    mapping: Res<BodiesMapping>,
) -> color_eyre::eyre::Result<()> {
    for event in events.read() {
        match event {
            FleetScreenEvent::Select(d) => context.select_adjacent(*d),
            FleetScreenEvent::TryNewShip(ctx) => {
                let info = ctx.to_info(context.ships.iter(), &bodies, mapping.as_ref())?;
                context.ships.push(info.clone());
                ship_events.send(ShipEvent::Create(info.clone()));
                context.popup = None;
            }
            FleetScreenEvent::LoadShip(config) => {
                if let Some(id) = config.ship_id() {
                    let info = ShipInfo {
                        id,
                        spawn_pos: config.spawn_pos.into(),
                        spawn_speed: config.spawn_speed.into(),
                    };
                    if !context.ships.iter().any(|s| s.id == id) {
                        context.ships.push(info.clone());
                        ship_events.send(ShipEvent::Create(info));
                        if !config.components.tanks.is_empty()
                            || !config.components.thrusters.is_empty()
                            || !config.components.sensors.is_empty()
                        {
                            inject_events.send(InjectComponentsEvent {
                                ship_id: id,
                                components: config.components.clone(),
                            });
                        }
                        if !config.trajectory.nodes.is_empty() {
                            trajectory_events.send(TrajectoryEvent::Create {
                                ship: id,
                                trajectory: config.trajectory.clone(),
                            });
                        }
                    }
                    context.popup = None;
                }
            }
            FleetScreenEvent::EditComponents => {
                if let Some(ship) = context.selected_ship() {
                    next_screen.set(AppScreen::Components(ship.id));
                }
            }
            FleetScreenEvent::EditTrajectory => {
                if let Some(ship) = context.selected_ship() {
                    next_screen.set(AppScreen::Editor(ship.id));
                }
            }
            FleetScreenEvent::Back => next_mode.set(ClientMode::None),
            FleetScreenEvent::EnterExplorer => next_screen.set(AppScreen::Explorer),
        }
    }
    Ok(())
}

fn update_fleet_context(
    stage: Res<State<GameStage>>,
    ships: Query<&ShipInfo>,
    time: Res<GameTime>,
    toggle_time: Res<ToggleTime>,
    mut ctx: ResMut<FleetContext>,
) {
    ctx.stage = stage.get().clone();
    ctx.game_time = time.time();
    ctx.time_running = toggle_time.0;
    ctx.ships.retain(|i| ships.iter().any(|j| i == j));
    let diff = ships
        .iter()
        .find(|i| !ctx.ships.iter().any(|j| *i == j))
        .cloned();
    ctx.ships.extend(diff);
}

impl StatefulWidget for FleetScreen {
    type State = FleetContext;

    fn render(
        self,
        _full_area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        use crate::ui::tui_overlay::{TUI_COLS, TUI_ROWS};
        let area = ratatui::prelude::Rect {
            x: 0,
            y: 0,
            width: TUI_COLS,
            height: TUI_ROWS,
        };

        let chunks =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).split(area);

        // Ship list
        let entries = state.ships.iter().map(|s| s.id.to_string());
        let list = List::new(entries).highlight_symbol(">").block(
            Block::bordered()
                .title_top(format!(
                    "Fleet — Stage: {}  ·  Time: {:.3} d ({})",
                    state.stage,
                    state.game_time,
                    if state.time_running { "Running" } else { "Paused" }
                ))
                .title_bottom("Ships"),
        );
        <List as StatefulWidget>::render(list, chunks[0], buf, &mut state.list_state);

        // Ship info or help panel
        if let Some(info) = state.selected_ship() {
            Paragraph::new(format!(
                "ID: {}\nSpawn position: {}\nSpawn velocity: {}\n\nspace: edit traj  c: components  n: new  l: load  e: explorer  r: run  p: pause  esc: back",
                info.id,
                info.spawn_pos,
                info.spawn_speed
            ))
            .block(Block::bordered().title_top("Ship info"))
            .wrap(Wrap { trim: true })
            .render(chunks[1], buf);
        } else {
            Paragraph::new(
                "No ship selected. Use Up/Down to choose a ship.\n\nn: new ship  l: load ship  e: explorer  r: run  p: pause  esc: back",
            )
            .block(Block::bordered().title_top("Fleet help"))
            .wrap(Wrap { trim: true })
            .render(chunks[1], buf);
        }
    }
}

impl StatefulWidget for FleetPopup {
    type State = FleetContext;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        match &mut state.popup {
            Some(FleetPopupKind::CreateShip(ctx)) => render_create_ship(ctx, area, buf),
            Some(FleetPopupKind::LoadShip(ctx)) => render_load_ship(ctx, area, buf),
            None => {}
        }
    }
}

fn render_create_ship(ctx: &mut CreateShipContext, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
    Clear.render(area, buf);
    let block = Block::bordered().title_top(" Ship creation ".bold());
    let inner = block.inner(area);
    block.render(area, buf);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .split(inner);

    Paragraph::new("Esc: cancel  ·  Tab: next field  ·  Shift+Tab: prev  ·  Enter: validate")
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .render(chunks[0], buf);

    Paragraph::new("Fill host body + altitude for orbit, or all six coords for free spawn.")
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .render(chunks[1], buf);

    let body =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).split(chunks[2]);

    let mut lc = [Constraint::Percentage(100 / 3)].repeat(3);
    lc.push(Constraint::Fill(1));
    let left = Layout::vertical(lc).split(body[0]);
    for i in 0..3 {
        ctx.paragraph(i).render(left[i], buf);
    }

    let mut rc = [Constraint::Percentage(100 / 6)].repeat(6);
    rc.push(Constraint::Fill(1));
    let coords = Layout::vertical(rc).split(body[1]);
    for i in 3..9 {
        ctx.paragraph(i).render(coords[i - 3], buf);
    }
}

fn render_load_ship(ctx: &mut LoadShipContext, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
    Clear.render(area, buf);
    let block = Block::bordered().title_top(" Load ship ".bold());
    let inner = block.inner(area);
    block.render(area, buf);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(inner);

    Paragraph::new("Up/Down: select  ·  Enter: load  ·  Esc: cancel")
        .alignment(Alignment::Center)
        .render(chunks[0], buf);

    if ctx.configs.is_empty() {
        Paragraph::new("No saved ships found in gamefiles/ships/")
            .alignment(Alignment::Center)
            .render(chunks[1], buf);
    } else {
        let items: Vec<String> = ctx.configs.iter().map(|(name, _)| name.clone()).collect();
        let list = List::new(items).highlight_symbol("> ");
        <List as StatefulWidget>::render(list, chunks[1], buf, &mut ctx.list_state);
    }

    if let Some(err) = &ctx.error {
        Paragraph::new(err.as_str())
            .alignment(Alignment::Center)
            .render(chunks[2], buf);
    }
}

#[cfg(test)]
mod tests {
    use bevy::{app::App, prelude::default, state::state::NextState};

    use crate::prelude::*;

    use super::{CreateShipContext, FleetContext, FleetScreenEvent};

    fn new_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            ClientPlugin::testing().in_mode(ClientMode::Singleplayer),
            TuiPlugin::testing(),
        ));
        app.update();
        app.update();
        app
    }

    #[test]
    fn test_create_ship() {
        let mut app = new_app();
        let popup = CreateShipContext {
            selected: 0,
            host_body: "terre".into(),
            altitude: "1e4".into(),
            ..Default::default()
        };
        app.world_mut()
            .send_event(FleetScreenEvent::TryNewShip(popup));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<ShipsMapping>().0.len(), 1)
    }

    #[test]
    fn test_update_context() {
        let mut app = new_app();
        let ctx = app.world().resource::<FleetContext>();
        assert_eq!(ctx.ships.len(), 0);
        assert_eq!(ctx.stage, GameStage::Preparation);
        app.world_mut().send_event(ShipEvent::Create(ShipInfo {
            id: id_from("s"),
            ..default()
        }));
        app.world_mut()
            .resource_mut::<NextState<GameStage>>()
            .set(GameStage::Action);
        // One update to set the stage
        app.update();
        // One update to update the context
        app.update();
        let ctx = app.world().resource::<FleetContext>();
        assert_eq!(ctx.ships.len(), 1);
        assert_eq!(ctx.stage, GameStage::Action);
    }
}

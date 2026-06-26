use bevy::prelude::*;
use bevy_ratatui::event::KeyEvent;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::Stylize,
    widgets::{Block, List, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};

use crate::{
    game::GameFiles,
    objects::ships::{
        config::{
            save_ship_config, SensorConfig, ShipComponents, ShipComponentsStore, ShipConfig,
            TankConfig, ThrusterConfig,
        },
        trajectory::Trajectory,
        ShipID, ShipInfo,
    },
    prelude::*,
    scripting::bridge::InjectComponentsEvent,
    ui::UiUpdate,
};

use super::AppScreen;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InComponents;

impl ComputedStates for InComponents {
    type SourceStates = AppScreen;
    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            AppScreen::Components(_) => Some(Self),
            _ => None,
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

pub fn plugin(app: &mut App) {
    app.add_computed_state::<InComponents>()
        .add_event::<ComponentsScreenEvent>()
        .add_systems(
            Update,
            (
                read_input.in_set(InputReading),
                handle_events.in_set(EventHandling),
            )
                .run_if(in_state(InComponents))
                .run_if(resource_exists::<ComponentsContext>),
        )
        .add_systems(
            PostUpdate,
            update_context
                .run_if(resource_exists::<ComponentsContext>)
                .run_if(resource_exists_and_changed::<ShipsMapping>)
                .in_set(UiUpdate),
        )
        .add_systems(OnEnter(InComponents), create_screen)
        .add_systems(OnExit(InComponents), (persist_components, clear_screen).chain());
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct ComponentsContext {
    pub ship_id: ShipID,
    pub components: ShipComponents,
    pub mode: ComponentsMode,
    list_state: ListState,
    save_status: Option<String>,
}

pub enum ComponentsMode {
    Viewing,
    ChoosingType(usize),
    AddingComponent(AddComponentContext),
}

pub struct AddComponentContext {
    pub kind: ComponentKind,
    pub fields: Vec<(String, String)>,
    pub selected: usize,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ComponentKind {
    Tank,
    Thruster,
    Sensor,
}

impl ComponentKind {
    fn label(self) -> &'static str {
        match self {
            Self::Tank => "Tank",
            Self::Thruster => "Thruster",
            Self::Sensor => "Sensor",
        }
    }

    fn field_names(self) -> &'static [&'static str] {
        match self {
            Self::Tank => &["ID", "Capacite", "Carburant initial"],
            Self::Thruster => &["ID", "Force max", "Consommation (L/force)", "Reservoir ID"],
            Self::Sensor => &["ID", "Portee (km)"],
        }
    }
}

const KINDS: [ComponentKind; 3] = [ComponentKind::Tank, ComponentKind::Thruster, ComponentKind::Sensor];

impl AddComponentContext {
    fn new(kind: ComponentKind) -> Self {
        let fields = kind
            .field_names()
            .iter()
            .map(|name| (name.to_string(), String::new()))
            .collect();
        Self { kind, fields, selected: 0, error: None }
    }

    fn selected_value(&mut self) -> &mut String {
        &mut self.fields[self.selected].1
    }
}

impl ComponentsContext {
    fn new(ship_id: ShipID, components: ShipComponents) -> Self {
        Self {
            ship_id,
            components,
            mode: ComponentsMode::Viewing,
            list_state: ListState::default(),
            save_status: None,
        }
    }

    fn component_entries(&self) -> Vec<String> {
        let mut entries = Vec::new();
        for (id, t) in &self.components.tanks {
            entries.push(format!("[Tank]     {}  cap={:.0} fuel={:.0}", id, t.capacite, t.carburant));
        }
        for (id, t) in &self.components.thrusters {
            entries.push(format!("[Thruster] {}  F={:.0} conso={:.3} res={}", id, t.force_max, t.consommation, t.reservoir));
        }
        for (id, s) in &self.components.sensors {
            entries.push(format!("[Sensor]   {}  portee={:.0} km", id, s.portee));
        }
        entries
    }

    fn selected_component_id(&self) -> Option<(String, &'static str)> {
        let i = self.list_state.selected()?;
        let mut idx = 0usize;
        for (id, _) in &self.components.tanks {
            if idx == i { return Some((id.clone(), "tank")); }
            idx += 1;
        }
        for (id, _) in &self.components.thrusters {
            if idx == i { return Some((id.clone(), "thruster")); }
            idx += 1;
        }
        for (id, _) in &self.components.sensors {
            if idx == i { return Some((id.clone(), "sensor")); }
            idx += 1;
        }
        None
    }

    fn delete_selected(&mut self) {
        if let Some((id, kind)) = self.selected_component_id() {
            match kind {
                "tank" => { self.components.tanks.remove(&id); }
                "thruster" => { self.components.thrusters.remove(&id); }
                "sensor" => { self.components.sensors.remove(&id); }
                _ => {}
            }
            self.list_state.select(None);
        }
    }

    fn total_components(&self) -> usize {
        self.components.tanks.len()
            + self.components.thrusters.len()
            + self.components.sensors.len()
    }

    fn select_next(&mut self) {
        let n = self.total_components();
        if n == 0 { return; }
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % n);
        self.list_state.select(Some(i));
    }

    fn select_previous(&mut self) {
        let n = self.total_components();
        if n == 0 { return; }
        let i = self.list_state.selected().map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Event)]
enum ComponentsScreenEvent {
    SelectNext,
    SelectPrevious,
    StartAdd,
    DeleteSelected,
    Save,
    Back,
    Validate,
    CycleType(i32),
    CycleField(i32),
    CharInput(char),
    DeleteChar,
}

// ── Systems ───────────────────────────────────────────────────────────────────

fn create_screen(
    screen: Res<State<AppScreen>>,
    mut commands: Commands,
    store: Res<ShipComponentsStore>,
) {
    if let AppScreen::Components(ship_id) = screen.get() {
        let components = store.0.get(ship_id).cloned().unwrap_or_default();
        commands.insert_resource(ComponentsContext::new(*ship_id, components));
    }
}

fn persist_components(ctx: Res<ComponentsContext>, mut store: ResMut<ShipComponentsStore>) {
    store.0.insert(ctx.ship_id, ctx.components.clone());
}

fn clear_screen(mut commands: Commands) {
    commands.remove_resource::<ComponentsContext>();
}

fn update_context(
    screen: Res<State<AppScreen>>,
    mut ctx: ResMut<ComponentsContext>,
    ships: Query<&ShipInfo>,
) {
    if let AppScreen::Components(id) = screen.get() {
        if ships.iter().all(|s| &s.id != id) {
            ctx.ship_id = ShipID::default();
        }
    }
}

fn read_input(
    ctx: Res<ComponentsContext>,
    mut key_events: EventReader<KeyEvent>,
    keymap: Res<Keymap>,
    mut events: EventWriter<ComponentsScreenEvent>,
) {
    let km = &keymap.components;
    for KeyEvent(event) in key_events.read() {
        if event.kind == KeyEventKind::Release {
            return;
        }
        match &ctx.mode {
            ComponentsMode::Viewing => match event {
                e if km.select_next.matches(e) => { events.send(ComponentsScreenEvent::SelectNext); }
                e if km.select_previous.matches(e) => { events.send(ComponentsScreenEvent::SelectPrevious); }
                e if km.add_component.matches(e) => { events.send(ComponentsScreenEvent::StartAdd); }
                e if km.delete_component.matches(e) => { events.send(ComponentsScreenEvent::DeleteSelected); }
                e if km.save.matches(e) => { events.send(ComponentsScreenEvent::Save); }
                e if km.back.matches(e) => { events.send(ComponentsScreenEvent::Back); }
                _ => {}
            },
            ComponentsMode::ChoosingType(_) => match event {
                e if km.cycle_options.matches(e) => { events.send(ComponentsScreenEvent::CycleType(1)); }
                e if km.cycle_options_back.matches(e) => { events.send(ComponentsScreenEvent::CycleType(-1)); }
                e if km.validate.matches(e) => { events.send(ComponentsScreenEvent::Validate); }
                e if km.back.matches(e) => { events.send(ComponentsScreenEvent::Back); }
                _ => {}
            },
            ComponentsMode::AddingComponent(_) => match event {
                e if km.cycle_options.matches(e) => { events.send(ComponentsScreenEvent::CycleField(1)); }
                e if km.cycle_options_back.matches(e) => { events.send(ComponentsScreenEvent::CycleField(-1)); }
                e if km.validate.matches(e) => { events.send(ComponentsScreenEvent::Validate); }
                e if km.back.matches(e) => { events.send(ComponentsScreenEvent::Back); }
                e if km.delete_char.matches(e) => { events.send(ComponentsScreenEvent::DeleteChar); }
                crossterm::event::KeyEvent { code: KeyCode::Char(c), .. } => {
                    events.send(ComponentsScreenEvent::CharInput(*c));
                }
                _ => {}
            },
        }
    }
}

fn handle_events(
    mut ctx: ResMut<ComponentsContext>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut events: EventReader<ComponentsScreenEvent>,
    mut store: ResMut<ShipComponentsStore>,
    mut inject_events: EventWriter<InjectComponentsEvent>,
    game_files: Res<GameFiles>,
    ships: Query<&ShipInfo>,
) {
    for event in events.read() {
        match event {
            ComponentsScreenEvent::SelectNext => ctx.select_next(),
            ComponentsScreenEvent::SelectPrevious => ctx.select_previous(),
            ComponentsScreenEvent::DeleteSelected => {
                ctx.delete_selected();
                store.0.insert(ctx.ship_id, ctx.components.clone());
            }
            ComponentsScreenEvent::StartAdd => {
                ctx.mode = ComponentsMode::ChoosingType(0);
                ctx.save_status = None;
            }
            ComponentsScreenEvent::Save => {
                let info = ships.iter().find(|s| s.id == ctx.ship_id);
                let (spawn_pos, spawn_speed) = info
                    .map(|i| ([i.spawn_pos.x, i.spawn_pos.y, i.spawn_pos.z],
                               [i.spawn_speed.x, i.spawn_speed.y, i.spawn_speed.z]))
                    .unwrap_or_default();
                let traj_path = game_files.trajectories.join(ctx.ship_id.to_string());
                let trajectory = std::fs::read_to_string(&traj_path)
                    .ok()
                    .and_then(|s| toml::from_str::<Trajectory>(&s).ok())
                    .unwrap_or_default();
                let config = ShipConfig {
                    id: ctx.ship_id.to_string(),
                    spawn_pos,
                    spawn_speed,
                    components: ctx.components.clone(),
                    trajectory,
                };
                ctx.save_status = Some(match save_ship_config(&game_files.ships, &config) {
                    Ok(()) => {
                        inject_events.send(InjectComponentsEvent {
                            ship_id: ctx.ship_id,
                            components: ctx.components.clone(),
                        });
                        format!("Saved to gamefiles/ships/{}.json", ctx.ship_id)
                    }
                    Err(e) => format!("Save failed: {e}"),
                });
            }
            ComponentsScreenEvent::Back => {
                let is_viewing = matches!(ctx.mode, ComponentsMode::Viewing);
                if is_viewing {
                    next_screen.set(AppScreen::Fleet);
                } else {
                    ctx.mode = ComponentsMode::Viewing;
                }
            }
            ComponentsScreenEvent::CycleType(dir) => {
                if let ComponentsMode::ChoosingType(i) = &mut ctx.mode {
                    let n = KINDS.len() as i32;
                    *i = ((*i as i32 + dir).rem_euclid(n)) as usize;
                }
            }
            ComponentsScreenEvent::CycleField(dir) => {
                if let ComponentsMode::AddingComponent(add_ctx) = &mut ctx.mode {
                    let n = add_ctx.fields.len() as i32;
                    add_ctx.selected = ((add_ctx.selected as i32 + dir).rem_euclid(n)) as usize;
                }
            }
            ComponentsScreenEvent::Validate => {
                let chosen_kind = match &ctx.mode {
                    ComponentsMode::ChoosingType(i) => Some(KINDS[*i]),
                    _ => None,
                };
                let is_adding = matches!(ctx.mode, ComponentsMode::AddingComponent(_));
                if let Some(kind) = chosen_kind {
                    ctx.mode = ComponentsMode::AddingComponent(AddComponentContext::new(kind));
                } else if is_adding {
                    try_commit_component(&mut ctx);
                    if matches!(ctx.mode, ComponentsMode::Viewing) {
                        store.0.insert(ctx.ship_id, ctx.components.clone());
                    }
                }
            }
            ComponentsScreenEvent::CharInput(c) => {
                if let ComponentsMode::AddingComponent(add_ctx) = &mut ctx.mode {
                    add_ctx.selected_value().push(*c);
                }
            }
            ComponentsScreenEvent::DeleteChar => {
                if let ComponentsMode::AddingComponent(add_ctx) = &mut ctx.mode {
                    add_ctx.selected_value().pop();
                }
            }
        }
    }
}

fn parse_f64(values: &[String], i: usize) -> Result<f64, String> {
    values
        .get(i)
        .ok_or_else(|| format!("Missing field {i}"))?
        .parse::<f64>()
        .map_err(|e| e.to_string())
}

fn try_commit_component(ctx: &mut ComponentsContext) {
    // Extract all needed data from the mode borrow first, then drop it.
    let (kind, values) = match &ctx.mode {
        ComponentsMode::AddingComponent(ac) => (
            ac.kind,
            ac.fields.iter().map(|(_, v)| v.trim().to_owned()).collect::<Vec<_>>(),
        ),
        _ => return,
    };

    let id = values.first().cloned().unwrap_or_default();
    if id.is_empty() {
        if let ComponentsMode::AddingComponent(ac) = &mut ctx.mode {
            ac.error = Some("ID cannot be empty".into());
        }
        return;
    }

    // Parse and commit — ctx.mode borrow already released above.
    let err: Option<String> = match kind {
        ComponentKind::Tank => {
            match (parse_f64(&values, 1), parse_f64(&values, 2)) {
                (Ok(capacite), Ok(carburant)) => {
                    ctx.components.tanks.insert(id, TankConfig { capacite, carburant });
                    None
                }
                (Err(e), _) | (Ok(_), Err(e)) => Some(e),
            }
        }
        ComponentKind::Thruster => {
            match (parse_f64(&values, 1), parse_f64(&values, 2)) {
                (Ok(force_max), Ok(consommation)) => {
                    let reservoir = values.get(3).cloned().unwrap_or_default();
                    ctx.components.thrusters.insert(id, ThrusterConfig { force_max, consommation, reservoir });
                    None
                }
                (Err(e), _) | (Ok(_), Err(e)) => Some(e),
            }
        }
        ComponentKind::Sensor => {
            match parse_f64(&values, 1) {
                Ok(portee) => {
                    ctx.components.sensors.insert(id, SensorConfig { portee });
                    None
                }
                Err(e) => Some(e),
            }
        }
    };

    match err {
        None => ctx.mode = ComponentsMode::Viewing,
        Some(e) => {
            if let ComponentsMode::AddingComponent(ac) = &mut ctx.mode {
                ac.error = Some(format!("Parse error: {e}"));
            }
        }
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

pub struct ComponentsScreen;

impl StatefulWidget for ComponentsScreen {
    type State = ComponentsContext;

    fn render(
        self,
        _full_area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        use crate::ui::tui_overlay::{TUI_COLS, TUI_ROWS};
        let area = ratatui::prelude::Rect { x: 0, y: 0, width: TUI_COLS, height: TUI_ROWS };

        // Extract a copy of the mode discriminant before the mutable call to avoid borrow conflicts.
        enum ModeKind { Viewing, Choosing(usize), Adding }
        let kind = match &state.mode {
            ComponentsMode::Viewing => ModeKind::Viewing,
            ComponentsMode::ChoosingType(i) => ModeKind::Choosing(*i),
            ComponentsMode::AddingComponent(_) => ModeKind::Adding,
        };
        match kind {
            ModeKind::Viewing => render_viewing(state, area, buf),
            ModeKind::Choosing(i) => render_choose_type(i, area, buf),
            ModeKind::Adding => render_adding(state, area, buf),
        }
    }
}

fn render_viewing(ctx: &mut ComponentsContext, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
    let chunks = Layout::horizontal([Constraint::Percentage(65), Constraint::Fill(1)]).split(area);

    let entries = ctx.component_entries();
    let n = entries.len();
    let ship_id = ctx.ship_id;
    let save_status = ctx.save_status.clone();
    let list = List::new(entries)
        .highlight_symbol("> ")
        .block(Block::bordered().title_top(format!(" Components — {} ", ship_id)));
    <List as StatefulWidget>::render(list, chunks[0], buf, &mut ctx.list_state);

    let help = format!(
        "Ship: {}\n{} component(s)\n\na: add component\nd: delete selected\ns: save to file\nesc: back to fleet{}",
        ship_id,
        n,
        save_status.as_deref().map(|s| format!("\n\n{s}")).unwrap_or_default()
    );
    Paragraph::new(help)
        .block(Block::bordered().title_top("Help"))
        .wrap(Wrap { trim: true })
        .render(chunks[1], buf);
}

fn render_choose_type(selected: usize, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
    let block = Block::bordered().title_top(" Choose component type ".bold());
    let inner = block.inner(area);
    block.render(area, buf);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    Paragraph::new("Tab/Shift+Tab: cycle  ·  Enter: confirm  ·  Esc: cancel")
        .alignment(Alignment::Center)
        .render(chunks[0], buf);

    let items: Vec<String> = KINDS
        .iter()
        .enumerate()
        .map(|(i, k)| {
            if i == selected { format!("> {}", k.label()) } else { format!("  {}", k.label()) }
        })
        .collect();
    Widget::render(List::new(items), chunks[1], buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;

    fn make_adding_ctx(kind: ComponentKind, field_values: &[&str]) -> ComponentsContext {
        let mut ctx = ComponentsContext::new(ShipID::from("shp").unwrap(), ShipComponents::default());
        let mut add_ctx = AddComponentContext::new(kind);
        for (i, val) in field_values.iter().enumerate() {
            if let Some((_, v)) = add_ctx.fields.get_mut(i) {
                *v = val.to_string();
            }
        }
        ctx.mode = ComponentsMode::AddingComponent(add_ctx);
        ctx
    }

    #[test]
    fn test_commit_tank_valid() {
        let mut ctx = make_adding_ctx(ComponentKind::Tank, &["reserv", "500", "300"]);
        try_commit_component(&mut ctx);
        assert!(matches!(ctx.mode, ComponentsMode::Viewing), "mode should return to Viewing");
        assert!(ctx.components.tanks.contains_key("reserv"));
        let t = &ctx.components.tanks["reserv"];
        assert_eq!(t.capacite, 500.0);
        assert_eq!(t.carburant, 300.0);
    }

    #[test]
    fn test_commit_thruster_valid() {
        let mut ctx = make_adding_ctx(ComponentKind::Thruster, &["moteur", "10.0", "0.5", "reserv"]);
        try_commit_component(&mut ctx);
        assert!(matches!(ctx.mode, ComponentsMode::Viewing));
        let th = &ctx.components.thrusters["moteur"];
        assert_eq!(th.force_max, 10.0);
        assert_eq!(th.consommation, 0.5);
        assert_eq!(th.reservoir, "reserv");
    }

    #[test]
    fn test_commit_sensor_valid() {
        let mut ctx = make_adding_ctx(ComponentKind::Sensor, &["radar", "1e6"]);
        try_commit_component(&mut ctx);
        assert!(matches!(ctx.mode, ComponentsMode::Viewing));
        assert_eq!(ctx.components.sensors["radar"].portee, 1e6);
    }

    #[test]
    fn test_commit_empty_id_sets_error() {
        let mut ctx = make_adding_ctx(ComponentKind::Sensor, &["", "1e6"]);
        try_commit_component(&mut ctx);
        assert!(matches!(ctx.mode, ComponentsMode::AddingComponent(_)), "mode should stay AddingComponent");
        if let ComponentsMode::AddingComponent(ac) = &ctx.mode {
            assert!(ac.error.is_some(), "error should be set for empty ID");
        }
        assert!(ctx.components.sensors.is_empty());
    }

    #[test]
    fn test_commit_invalid_number_sets_error() {
        let mut ctx = make_adding_ctx(ComponentKind::Tank, &["reserv", "not_a_number", "100"]);
        try_commit_component(&mut ctx);
        assert!(matches!(ctx.mode, ComponentsMode::AddingComponent(_)));
        if let ComponentsMode::AddingComponent(ac) = &ctx.mode {
            assert!(ac.error.is_some(), "error should mention parse failure");
        }
        assert!(ctx.components.tanks.is_empty());
    }

    #[test]
    fn test_commit_does_not_overwrite_existing_on_error() {
        let mut ctx = ComponentsContext::new(ShipID::from("shp").unwrap(), ShipComponents::default());
        ctx.components.tanks.insert("existing".into(), TankConfig { capacite: 99.0, carburant: 50.0 });

        let mut add_ctx = AddComponentContext::new(ComponentKind::Tank);
        add_ctx.fields[0].1 = "existing".into();
        add_ctx.fields[1].1 = "bad".into(); // parse error
        add_ctx.fields[2].1 = "50".into();
        ctx.mode = ComponentsMode::AddingComponent(add_ctx);

        try_commit_component(&mut ctx);

        // Original value must be untouched
        assert_eq!(ctx.components.tanks["existing"].capacite, 99.0);
    }

    fn setup_app_for_components() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin));
        app.init_state::<AppScreen>();
        app.init_resource::<ShipComponentsStore>();
        app.add_systems(Update, (persist_components, create_screen));
        app
    }

    #[test]
    fn test_persist_components_writes_to_store() {
        let mut app = setup_app_for_components();
        let ship_id = ShipID::from("shp").unwrap();
        let mut components = ShipComponents::default();
        components.sensors.insert("radar".into(), SensorConfig { portee: 5000.0 });
        app.insert_resource(ComponentsContext::new(ship_id, components));

        app.update();

        let store = app.world().resource::<ShipComponentsStore>();
        assert!(store.0.contains_key(&ship_id), "store should have the ship after persist");
        assert!(store.0[&ship_id].sensors.contains_key("radar"));
    }

    #[test]
    fn test_create_screen_reads_from_store() {
        let mut app = setup_app_for_components();
        let ship_id = ShipID::from("shp").unwrap();

        {
            let mut store = app.world_mut().resource_mut::<ShipComponentsStore>();
            let mut components = ShipComponents::default();
            components.tanks.insert("main".into(), TankConfig { capacite: 200.0, carburant: 150.0 });
            store.0.insert(ship_id, components);
        }

        // StateTransition runs before Update, so create_screen sees the new state in the same update.
        app.world_mut().resource_mut::<NextState<AppScreen>>().set(AppScreen::Components(ship_id));
        app.update();

        let ctx = app.world().get_resource::<ComponentsContext>().expect("ComponentsContext should exist");
        assert_eq!(ctx.ship_id, ship_id);
        assert!(ctx.components.tanks.contains_key("main"), "components should be loaded from store");
    }

    #[test]
    fn test_create_screen_empty_when_not_in_store() {
        let mut app = setup_app_for_components();
        let ship_id = ShipID::from("fresh").unwrap();

        app.world_mut().resource_mut::<NextState<AppScreen>>().set(AppScreen::Components(ship_id));
        app.update();

        let ctx = app.world().get_resource::<ComponentsContext>().expect("ComponentsContext should exist");
        assert!(ctx.components.tanks.is_empty());
        assert!(ctx.components.thrusters.is_empty());
        assert!(ctx.components.sensors.is_empty());
    }
}

fn render_adding(ctx: &ComponentsContext, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
    let ComponentsMode::AddingComponent(add_ctx) = &ctx.mode else { return };

    let block = Block::bordered().title_top(format!(" Add {} ", add_ctx.kind.label()).bold());
    let inner = block.inner(area);
    block.render(area, buf);

    let n_fields = add_ctx.fields.len();
    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1)]; // hint row
    for _ in 0..n_fields {
        constraints.push(Constraint::Length(3));
    }
    constraints.push(Constraint::Fill(1));
    let chunks = Layout::vertical(constraints).split(inner);

    Paragraph::new("Tab/Shift+Tab: next field  ·  Enter: confirm  ·  Esc: cancel")
        .alignment(Alignment::Center)
        .render(chunks[0], buf);

    for (i, (label, value)) in add_ctx.fields.iter().enumerate() {
        let is_selected = i == add_ctx.selected;
        let title = if is_selected {
            format!(" > {} ", label)
        } else {
            format!("   {} ", label)
        };
        Paragraph::new(value.as_str())
            .block(Block::bordered().title_top(title))
            .render(chunks[i + 1], buf);
    }

    if let Some(err) = &add_ctx.error {
        let last = chunks[n_fields + 1];
        Paragraph::new(err.as_str())
            .alignment(Alignment::Center)
            .render(last, buf);
    }
}


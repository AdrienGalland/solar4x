use bevy::{prelude::*, utils::HashMap};
use bevy_ratatui::event::KeyEvent;
use crossterm::event::{KeyCode, KeyEvent as CKeyEvent, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    widgets::{StatefulWidget, StatefulWidgetRef, WidgetRef},
};

use crate::{
    client::ClientMode,
    game::GameStage,
    physics::{orbit::SystemSize, time::TimeEvent},
    ui::{
        gui::SelectObjectEvent,
        widget::{
            info::InfoWidget,
            search::{SearchPlugin, SearchState, SearchWidget},
            space_map::{SpaceMap, SpaceMapWidget},
            tree::{TreeState, TreeWidget},
        },
        UiUpdate,
    },
    utils::list::ClampedList,
};
use crate::{input::prelude::Keymap, objects::prelude::*};
use crate::{
    physics::Position,
    ui::{
        prelude::*,
        widget::{
            search::{SearchEvent, SearchMatcher},
            space_map::SpaceMapEvent,
            tree::TreeEvent,
        },
    },
};
use crate::objects::orbiting_obj::OrbitingObjects;

use super::PreviousScreen;

pub fn plugin(app: &mut App) {
    app.add_event::<ExplorerEvent>()
        .add_plugins(SearchPlugin)
        .add_systems(
            Update,
            (
                read_input.in_set(InputReading),
                (
                    handle_explorer_events,
                    focus_on_select_body.run_if(
                        resource_exists::<Events<SelectObjectEvent>>
                            .and_then(on_event::<SelectObjectEvent>()),
                    ),
                )
                    .in_set(EventHandling),
                update_space_map.in_set(UiUpdate),
            )
                .run_if(in_loaded_screen::<ExplorerContext>(AppScreen::Explorer)),
        )
        .add_systems(
            OnEnter(AppScreen::Explorer),
            (create_screen, update_space_map).chain(),
        )
        .add_systems(OnExit(AppScreen::Explorer), clear_screen);
}

fn create_screen(
    mut commands: Commands,
    primary: Query<Entity, With<PrimaryBody>>,
    bodies: Query<(&BodyInfo, &OrbitingObjects)>,
    system_size: Res<SystemSize>,
    mapping: Res<BodiesMapping>,
    query: Query<&OrbitingObjects>
) {
    let primary = primary.single();
    commands.insert_resource(SpaceMap::new(system_size.0, Some(primary), Some(primary)));
    commands.insert_resource(ExplorerContext::new(primary, &bodies,mapping, query));
}

fn clear_screen(mut commands: Commands) {
    commands.remove_resource::<ExplorerContext>();
    commands.remove_resource::<SpaceMap>();
}

#[derive(Default, Debug, Clone, Copy)]
pub enum SidePaneMode {
    #[default]
    Tree,
    Search,
}

#[derive(Resource)]
pub struct ExplorerContext {
    pub side_pane_mode: SidePaneMode,
    pub info_toggle: bool,
    pub tree_state: TreeState,
    pub search_state: SearchState,
    pub info: InfoWidget,
    pub space_map: SpaceMapWidget,
}

impl ExplorerContext {
    pub fn new(primary: Entity, bodies: &Query<(&BodyInfo, &OrbitingObjects)>, mapping: Res<BodiesMapping>, query: Query<&OrbitingObjects>) -> ExplorerContext {
        let (primary_info, primary_orbiting) = bodies.get(primary).unwrap();
        let primary_data = primary_info.clone().0;
        let infos: Vec<_> = bodies.iter().map(|(i,_)| &i.0).collect();
        ExplorerContext {
            side_pane_mode: SidePaneMode::default(),
            info_toggle: false,
            tree_state: TreeState::new(&primary_data, Some(&primary_data), infos.clone().into_iter(), mapping, query),
            search_state: SearchState::new(infos.into_iter()),
            info: InfoWidget {
                body_info: primary_data,
                orbiting_obj: primary_orbiting.clone(),
            },
            space_map: SpaceMapWidget::default(),
        }
    }
    fn update_info(&mut self, mapping: &HashMap<BodyID, Entity>, bodies: &Query<&BodyInfo>) {
        let id = self.tree_state.selected_body_id();
        if let Ok(body_info) = bodies.get(mapping[&id]) {
            self.info.body_info = body_info.0.clone();
        }
    }
}

#[derive(Event)]
pub(crate) enum ExplorerEvent {
    Tree(TreeEvent),
    Search(SearchEvent),
    SpaceMap(SpaceMapEvent),
    View(ViewEvent),
    Time(TimeEvent),
}

impl ExplorerContext {
    pub fn selected_body(&self) -> BodyID {
        let tree_selection = self.tree_state.selected_body_id();
        match self.side_pane_mode {
            SidePaneMode::Tree => tree_selection,
            SidePaneMode::Search => self
                .search_state
                .selected_body_id()
                .unwrap_or(tree_selection),
        }
    }
}

#[derive(Debug, Event)]
pub enum ViewEvent {
    ChangeSidePaneMode(SidePaneMode),
    ToggleInfo,
    Back,
}

fn read_input(
    context: Res<ExplorerContext>,
    mut key_event: EventReader<KeyEvent>,
    keymap: Res<Keymap>,
    mut internal_event: EventWriter<ExplorerEvent>,
) {
    use crate::prelude::Direction2::*;
    use ExplorerEvent::*;
    use ViewEvent::*;
    for KeyEvent(event) in key_event.read() {
        if event.kind == KeyEventKind::Release {
            return;
        }
        let keymap = &keymap.explorer;
        internal_event.send(match context.side_pane_mode {
            SidePaneMode::Tree => {
                let codes = &keymap.tree;
                use crate::prelude::Direction4::*;
                use SpaceMapEvent::*;
                use TimeEvent::*;
                use TreeEvent::*;
                match event {
                    e if codes.select_next.matches(e) => Tree(Select(Down)),
                    e if codes.select_previous.matches(e) => Tree(Select(Up)),
                    e if codes.toggle_expand.matches(e) => Tree(ToggleTreeExpansion),
                    e if codes.zoom_in.matches(e) => SpaceMap(Zoom(Up)),
                    e if codes.zoom_out.matches(e) => SpaceMap(Zoom(Down)),
                    e if codes.map_offset_up.matches(e) => SpaceMap(MapOffset(Front)),
                    e if codes.map_offset_left.matches(e) => SpaceMap(MapOffset(Left)),
                    e if codes.map_offset_down.matches(e) => SpaceMap(MapOffset(Back)),
                    e if codes.map_offset_right.matches(e) => SpaceMap(MapOffset(Right)),
                    e if codes.map_offset_reset.matches(e) => SpaceMap(MapOffsetReset),
                    e if codes.focus.matches(e) => SpaceMap(FocusBody),
                    e if codes.autoscale.matches(e) => SpaceMap(Autoscale),
                    e if codes.enter_search.matches(e) => {
                        View(ChangeSidePaneMode(SidePaneMode::Search))
                    }
                    e if codes.toggle_info.matches(e) => View(ToggleInfo),
                    e if codes.back.matches(e) => View(ViewEvent::Back),
                    e if codes.speed_up.matches(e) => Time(ChangeStepSize(Up)),
                    e if codes.slow_down.matches(e) => Time(ChangeStepSize(Down)),
                    e if codes.toggle_time.matches(e) => Time(ToggleTime),
                    _ => return,
                }
            }
            SidePaneMode::Search => {
                use SearchEvent::*;
                let codes = &keymap.search;
                match event {
                    e if codes.delete_char.matches(e) => Search(DeleteChar),
                    e if codes.validate_search.matches(e) => Search(ValidateSearch),
                    e if codes.select_next.matches(e) => Search(Select(Down)),
                    e if codes.select_previous.matches(e) => Search(Select(Up)),
                    e if codes.leave_search.matches(e) => {
                        View(ChangeSidePaneMode(SidePaneMode::Tree))
                    }
                    CKeyEvent {
                        code: KeyCode::Char(char),
                        ..
                    } => Search(WriteChar(*char)),
                    _ => return,
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_explorer_events(
    mut ctx: ResMut<ExplorerContext>,
    mut space_map: ResMut<SpaceMap>,

    client_mode: Res<State<ClientMode>>,
    mut next_mode: ResMut<NextState<ClientMode>>,

    previous_screen: Res<PreviousScreen>,
    mut next_screen: ResMut<NextState<AppScreen>>,

    game_stage: Option<Res<State<GameStage>>>,
    mut next_game_stage: Option<ResMut<NextState<GameStage>>>,

    mut events: EventReader<ExplorerEvent>,
    mapping: Res<BodiesMapping>,
    bodies: Query<&BodyInfo>,
    bodies_orbiting: Query<(&BodyInfo, &OrbitingObjects)>,
    mut time_events: ResMut<Events<TimeEvent>>,
    fuzzy_matcher: Res<SearchMatcher>,
) {
    for event in events.read() {
        match event {
            ExplorerEvent::Tree(event) => {
                use TreeEvent::*;
                match event {
                    Select(d) => {
                        ctx.tree_state.select_adjacent(*d);
                        ctx.update_info(&mapping.0, &bodies);
                    }
                    ToggleTreeExpansion => ctx.tree_state.toggle_selection_expansion(),
                }
            }
            ExplorerEvent::Search(event) => {
                use SearchEvent::*;
                match event {
                    DeleteChar => {
                        ctx.search_state.delete_char();
                        ctx.search_state
                            .update_search_entries(bodies.iter(), &fuzzy_matcher.0);
                    }
                    Select(d) => ctx.search_state.select_adjacent(*d),
                    ValidateSearch => {
                        if let Some(id) = ctx.search_state.selected_body_id() {
                            ctx.tree_state.select_body(id);
                            ctx.update_info(&mapping.0, &bodies);
                        }
                        ctx.side_pane_mode = SidePaneMode::Tree;
                    }
                    WriteChar(char) => {
                        ctx.search_state.enter_char(*char);
                        ctx.search_state
                            .update_search_entries(bodies.iter(), &fuzzy_matcher.0);
                    }
                }
            }
            ExplorerEvent::SpaceMap(event) => {
                use SpaceMapEvent::*;
                match event {
                    Zoom(d) => space_map.zoom(*d),
                    MapOffset(d) => space_map.offset(*d),
                    MapOffsetReset => space_map.reset_offset(),
                    FocusBody => {
                        if let Some(entity) = mapping.0.get(&ctx.tree_state.selected_body_id()) {
                            space_map.focus(*entity);
                            ctx.tree_state.focus_body(bodies.get(*entity).unwrap().0.id)
                        }
                    }
                    Autoscale => space_map.autoscale(&mapping.0, &bodies_orbiting),
                }
            }
            ExplorerEvent::View(event) => match *event {
                ViewEvent::ChangeSidePaneMode(new_focus) => {
                    ctx.search_state.reset_search();
                    ctx.side_pane_mode = new_focus
                }

                ViewEvent::ToggleInfo => ctx.info_toggle = !ctx.info_toggle,
                ViewEvent::Back => match client_mode.get() {
                    ClientMode::Explorer => next_mode.set(ClientMode::None),
                    _ => {
                        next_screen.set(previous_screen.0);
                    }
                },
            },
            ExplorerEvent::Time(event) => {
                if let (Some(game_stage), Some(next_game_stage)) =
                    (game_stage.as_ref(), next_game_stage.as_mut())
                {
                    match event {
                        TimeEvent::ChangeStepSize(d) => {
                            time_events.send(TimeEvent::ChangeUpdateRate(*d));
                        }
                        TimeEvent::ToggleTime => next_game_stage.set(match game_stage.get() {
                            GameStage::Preparation => GameStage::Action,
                            GameStage::Action => GameStage::Preparation,
                        }),
                        _ => {}
                    }
                } else {
                    time_events.send(*event);
                }
            }
        }
    }
}

fn update_space_map(
    mut ctx: ResMut<ExplorerContext>,
    mut space_map: ResMut<SpaceMap>,
    query: Query<(Entity, &Position, &BodyInfo)>,
    mapping: Res<BodiesMapping>,
) {
    space_map.selected = mapping.0.get(&ctx.selected_body()).cloned();
    ctx.space_map.update_map(space_map.as_ref(), &query);
}

fn focus_on_select_body(
    mut events: EventReader<SelectObjectEvent>,
    info: Query<&BodyInfo>,
    mut space_map: ResMut<SpaceMap>,
    mut ctx: ResMut<ExplorerContext>,
) {
    for event in events.read() {
        if let Ok(info) = info.get(event.entity) {
            space_map.focus(event.entity);
            ctx.tree_state.focus_body(info.0.id)
        }
    }
}
pub struct ExplorerScreen<'a> {
    pub map: &'a mut SpaceMap,
}

impl StatefulWidget for ExplorerScreen<'_> {
    type State = ExplorerContext;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let outer_chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
        Paragraph::new("Explorer — /: search  ·  i: toggle info  ·  f: focus  ·  t: toggle time  ·  >/<: speed  ·  Esc: back")
            .alignment(Alignment::Center)
            .render(outer_chunks[0], buf);

        let mut c = vec![Constraint::Percentage(25), Constraint::Fill(1)];
        if state.info_toggle {
            c.push(Constraint::Percentage(25));
        }
        let chunks = Layout::horizontal(c).split(outer_chunks[1]);

        match state.side_pane_mode {
            SidePaneMode::Tree => TreeWidget.render(chunks[0], buf, &mut state.tree_state),
            SidePaneMode::Search => {
                SearchWidget.render(chunks[0], buf, &mut state.search_state);
            }
        }
        state.space_map.render_ref(chunks[1], buf, self.map);
        if state.info_toggle {
            state.info.render_ref(chunks[2], buf);
        }
    }
}

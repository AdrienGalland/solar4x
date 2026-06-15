use bevy::prelude::*;
use bevy_ratatui::event::KeyEvent;
use editor::{EditorContext, EditorScreen};
use explorer::{ExplorerContext, ExplorerScreen};
use fleet::{FleetContext, FleetPopup, FleetScreen};
use start::{StartMenu, StartMenuContext};

use crate::ui::tui_overlay::{PopupOverlayMarker, PopupTuiContext, TuiContext};

use crate::{
    client::ClientMode,
    objects::ships::ShipID,
    prelude::{exit_on_error_if_app, Loaded},
};

use super::{widget::space_map::SpaceMap, InputReading, RenderSet};

pub mod editor;
pub mod explorer;
pub mod fleet;
pub mod start;
pub mod schedule_screen;

/// A resource storing the current screen
/// Set this to change screen, the appropriate context is automatically generated when the app is ready
/// (for example when the bodies have been imported)
#[derive(States, Debug, PartialEq, Eq, Clone, Copy, Hash, Default)]
pub enum AppScreen {
    #[default]
    StartMenu,
    Explorer,
    Fleet,
    Editor(ShipID),
    Scheduler(ShipID),
}

#[derive(Resource, Default, Debug)]
pub struct PreviousScreen(pub AppScreen);

pub fn plugin(app: &mut App) {
    app.add_plugins((
        start::plugin,
        explorer::plugin,
        fleet::plugin,
        editor::plugin,
    ))
    .init_state::<AppScreen>()
    .init_resource::<PreviousScreen>()
    .add_systems(
        PreUpdate,
        update_previous_screen.run_if(resource_changed::<NextState<AppScreen>>),
    )
    .add_systems(
        Update,
        clear_key_events
            .before(InputReading)
            .run_if(state_changed::<AppScreen>),
    )
    .add_systems(
        OnEnter(ClientMode::Explorer),
        move |mut next_screen: ResMut<NextState<AppScreen>>| next_screen.set(AppScreen::Explorer),
    )
    .add_systems(
        OnEnter(ClientMode::Singleplayer),
        move |mut next_screen: ResMut<NextState<AppScreen>>| next_screen.set(AppScreen::Fleet),
    )
    .add_systems(
        Update,
        toggle_popup_visibility.run_if(resource_exists::<TuiContext>),
    )
    .add_systems(
        PostUpdate,
        render
            .pipe(exit_on_error_if_app)
            .run_if(resource_exists::<TuiContext>)
            .in_set(RenderSet),
    );
}

fn update_previous_screen(
    next: Res<NextState<AppScreen>>,
    current: Res<State<AppScreen>>,
    mut previous: ResMut<PreviousScreen>,
) {
    if let NextState::Pending(_) = next.as_ref() {
        previous.0 = *current.get();
    }
}

fn clear_key_events(mut events: ResMut<Events<KeyEvent>>) {
    events.clear();
}

fn toggle_popup_visibility(
    fleet: Option<Res<FleetContext>>,
    mut query: Query<&mut Visibility, With<PopupOverlayMarker>>,
) {
    let has_popup = fleet.as_deref().map(|f| f.has_popup()).unwrap_or(false);
    for mut vis in query.iter_mut() {
        *vis = if has_popup {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn render(
    mut ctx: ResMut<TuiContext>,
    mut popup_ctx: Option<ResMut<PopupTuiContext>>,
    screen: Res<State<AppScreen>>,
    mut start_menu: Option<ResMut<StartMenuContext>>,
    mut explorer: Option<ResMut<ExplorerContext>>,
    mut fleet: Option<ResMut<FleetContext>>,
    mut editor: Option<ResMut<EditorContext>>,
    mut space_map: Option<ResMut<SpaceMap>>,
) -> color_eyre::Result<()> {
    let fleet_has_popup = fleet.as_deref().map(|f| f.has_popup()).unwrap_or(false);

    ctx.0.draw(|f| {
        let full_area = f.size();
        match screen.get() {
            AppScreen::StartMenu => {
                if let Some(sm) = start_menu.as_deref_mut() {
                    f.render_stateful_widget(StartMenu, full_area, sm);
                }
            }
            AppScreen::Explorer => {
                if let (Some(exp), Some(map)) =
                    (explorer.as_deref_mut(), space_map.as_deref_mut())
                {
                    f.render_stateful_widget(ExplorerScreen { map }, full_area, exp);
                }
            }
            AppScreen::Fleet => {
                if let Some(fl) = fleet.as_deref_mut() {
                    f.render_stateful_widget(FleetScreen, full_area, fl);
                }
            }
            AppScreen::Editor(_) => {
                if let Some(ed) = editor.as_deref_mut() {
                    f.render_stateful_widget(EditorScreen, full_area, ed);
                }
            }
            AppScreen::Scheduler(_) => {}
        }
    })?;

    // Render popup into its own buffer (displayed as a separate centered Bevy overlay).
    if fleet_has_popup {
        if let (Some(fl), Some(popup)) = (fleet.as_deref_mut(), popup_ctx.as_mut()) {
            popup.0.draw(|f| {
                let area = f.size();
                f.render_stateful_widget(FleetPopup, area, fl);
            })?;
        }
    }

    Ok(())
}

/// Helper function to reduce boilerplate
pub fn in_loaded_screen<Context: Resource>(screen: AppScreen) -> impl Condition<()> {
    in_state(screen)
        .and_then(resource_exists::<Context>)
        .and_then(in_state(Loaded))
}

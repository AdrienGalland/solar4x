use bevy::prelude::*;
use bevy_ratatui::event::KeyEvent;

use crate::input::prelude::Keymap;

pub mod gui;
pub mod screen;
pub mod tui_input;
pub mod tui_overlay;
pub mod widget;

pub mod prelude {
    pub use super::{
        gui::{SelectObjectEvent, MAX_HEIGHT},
        screen::{in_loaded_screen, AppScreen},
        tui_overlay::TuiContext,
        widget::space_map::SpaceMap,
        EventHandling, InputReading, TuiPlugin,
    };
}

#[derive(Default)]
pub struct TuiPlugin {
    pub headless: bool,
    pub keymap: Keymap,
}

impl TuiPlugin {
    pub fn testing() -> Self {
        Self {
            headless: true,
            ..default()
        }
    }
}

impl Plugin for TuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<KeyEvent>()
            .add_plugins(screen::plugin)
            .insert_resource(self.keymap.clone())
            .configure_sets(PostUpdate, (UiUpdate, RenderSet).chain())
            .configure_sets(Update, (InputReading, EventHandling).chain());

        if !self.headless {
            app.add_plugins(tui_overlay::plugin).add_systems(
                PreUpdate,
                tui_input::bevy_keyboard_to_key_event,
            );

            // Initialize TuiContext once the window is available.
            app.add_systems(Startup, init_tui_context);
        }
    }
}

fn init_tui_context(mut commands: Commands, windows: Query<&Window, With<bevy::window::PrimaryWindow>>) {
    let (cols, rows) = windows
        .get_single()
        .map(|w| {
            let font_w = tui_overlay::FONT_SIZE * 0.6;
            let font_h = tui_overlay::FONT_SIZE * 1.2;
            (
                (w.width() / font_w).floor() as u16,
                (w.height() / font_h).floor() as u16,
            )
        })
        .unwrap_or((180, 50));
    commands.insert_resource(tui_overlay::TuiContext::new(cols, rows));
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputReading;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventHandling;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UiUpdate;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderSet;

#[cfg(test)]
mod tests {
    use bevy::{app::App, state::state::State};

    use crate::prelude::*;

    #[test]
    fn test_change_screen() {
        let mut app = App::new();
        app.add_plugins((
            ClientPlugin::testing().in_mode(ClientMode::Explorer),
            TuiPlugin::testing(),
        ));
        // One update to enter the explorer mode
        app.update();
        // One update to create the body system
        app.update();
        // One update to enter the screen
        app.update();
        let world = app.world_mut();
        assert!(matches!(
            *world.resource::<State<AppScreen>>().get(),
            AppScreen::Explorer
        ));
    }
}

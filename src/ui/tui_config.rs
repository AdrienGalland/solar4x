use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Configuration file path
pub const CONFIG_FILE_PATH: &str = "tui_config.toml";

/// TUI Window Mode - determines how the TUI is displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TuiWindowMode {
    /// TUI is integrated in the main Bevy window (top-left overlay)
    Integrated,
    /// TUI is in a separate Bevy window
    SeparateWindow,
}

impl Default for TuiWindowMode {
    fn default() -> Self {
        TuiWindowMode::Integrated
    }
}

impl std::fmt::Display for TuiWindowMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TuiWindowMode::Integrated => write!(f, "Integrated"),
            TuiWindowMode::SeparateWindow => write!(f, "Separate Window"),
        }
    }
}

/// Configuration for TUI window display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiWindowConfig {
    /// Mode: Integrated or Separate Window
    pub mode: TuiWindowMode,
    /// Width of the separate TUI window (in pixels) - only used in SeparateWindow mode
    pub window_width: f32,
    /// Height of the separate TUI window (in pixels) - only used in SeparateWindow mode
    pub window_height: f32,
    /// X position of the separate TUI window
    pub window_x: f32,
    /// Y position of the separate TUI window
    pub window_y: f32,
}

impl Default for TuiWindowConfig {
    fn default() -> Self {
        Self {
            mode: TuiWindowMode::default(),
            window_width: 800.0,
            window_height: 600.0,
            window_x: 100.0,
            window_y: 100.0,
        }
    }
}

impl TuiWindowConfig {
    /// Load configuration from file, or create default if not exists
    pub fn load() -> Self {
        let path = PathBuf::from(CONFIG_FILE_PATH);
        
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
            eprintln!("Warning: Could not parse {}, using defaults", CONFIG_FILE_PATH);
        }
        
        // Create default config file
        let config = Self::default();
        if let Err(e) = config.save() {
            eprintln!("Warning: Could not create config file: {}", e);
        }
        config
    }

    /// Save configuration to file
    pub fn save(&self) -> color_eyre::Result<()> {
        let path = PathBuf::from(CONFIG_FILE_PATH);
        let content = toml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Toggle between Integrated and SeparateWindow modes
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            TuiWindowMode::Integrated => TuiWindowMode::SeparateWindow,
            TuiWindowMode::SeparateWindow => TuiWindowMode::Integrated,
        };
    }

    /// Set the mode
    pub fn set_mode(&mut self, mode: TuiWindowMode) {
        self.mode = mode;
    }

    /// Get the current mode
    pub fn get_mode(&self) -> TuiWindowMode {
        self.mode
    }
}

/// Resource to hold the TUI window configuration in the Bevy app
#[derive(Resource, Debug, Clone)]
pub struct TuiWindowSettings(pub TuiWindowConfig);

impl Default for TuiWindowSettings {
    fn default() -> Self {
        Self(TuiWindowConfig::default())
    }
}

/// Resource to track if TUI should be rendered in the main window
/// This is true in Integrated mode, false in SeparateWindow mode
#[derive(Resource, Debug, Default)]
pub struct RenderTuiInMainWindow(pub bool);

impl TuiWindowSettings {
    pub fn new(config: TuiWindowConfig) -> Self {
        Self(config)
    }

    pub fn load() -> Self {
        Self(TuiWindowConfig::load())
    }

    pub fn save(&self) -> color_eyre::Result<()> {
        self.0.save()
    }

    pub fn toggle_mode(&mut self) {
        self.0.toggle_mode();
    }

    pub fn set_mode(&mut self, mode: TuiWindowMode) {
        self.0.set_mode(mode);
    }

    pub fn get_mode(&self) -> TuiWindowMode {
        self.0.get_mode()
    }

    pub fn get_config(&self) -> &TuiWindowConfig {
        &self.0
    }

    pub fn get_config_mut(&mut self) -> &mut TuiWindowConfig {
        &mut self.0
    }
}

/// Event to trigger mode change
#[derive(Event, Debug, Clone, Default)]
pub struct ToggleTuiWindowModeEvent;

/// Plugin to manage TUI window configuration
pub struct TuiWindowConfigPlugin;

impl Plugin for TuiWindowConfigPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ToggleTuiWindowModeEvent>()
            .init_resource::<TuiWindowSettings>()
            .add_systems(Startup, load_tui_config)
            .add_systems(Update, handle_toggle_mode_event);
    }
}

fn load_tui_config(mut commands: Commands) {
    commands.insert_resource(TuiWindowSettings::load());
}

fn handle_toggle_mode_event(
    mut events: EventReader<ToggleTuiWindowModeEvent>,
    mut settings: ResMut<TuiWindowSettings>,
) {
    for _ in events.read() {
        settings.toggle_mode();
        if let Err(e) = settings.save() {
            eprintln!("Error saving TUI config: {}", e);
        }
    }
}

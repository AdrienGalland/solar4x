use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use bevy::{prelude::*, state::app::StatesPlugin};
use tempfile::{tempdir, TempDir};

use crate::{
    client::ClientMode,
    objects::{
        bodies::BodiesPlugin,
        prelude::BodiesMapping,
        ships::{trajectory::TRAJECTORIES_PATH, ShipsMapping, ShipsPlugin},
        ObjectsUpdate,
    },
    physics::{
        influence::InfluenceUpdate, orbit::OrbitsUpdate, prelude::ToggleTime, PhysicsPlugin,
        PhysicsUpdate,
    },
    scripting::ScriptingPlugin,
    ui::gui::GUIUpdate,
};

pub mod prelude {
    pub use super::{GameStage, InGame, Loaded};
}

pub const GAME_FILES_PATH: &str = "gamefiles";

/// This plugin's role is to handle everything that is about the main game, and that is common to both the server and the client
#[derive(Default)]
pub struct GamePlugin {
    pub testing: bool,
}

impl GamePlugin {
    pub fn testing() -> Self {
        Self { testing: true }
    }
}

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        let path = if self.testing {
            let dir = TempDirectory::default();
            let path = dir.0.path().to_owned();
            app.insert_resource(dir);
            path
        } else {
            GAME_FILES_PATH.into()
        };
        if self.testing {
            app.add_plugins((MinimalPlugins, StatesPlugin))
        } else {
            app.add_plugins(DefaultPlugins)
        }
        .add_plugins((PhysicsPlugin, BodiesPlugin, ShipsPlugin, ScriptingPlugin))
        .add_computed_state::<InGame>()
        .add_computed_state::<Authoritative>()
        .add_sub_state::<GameStage>()
        .add_computed_state::<Loaded>()
        .insert_resource(GameFiles::new(path).unwrap())
        .configure_sets(
            OnEnter(Loaded),
            (ObjectsUpdate, OrbitsUpdate, InfluenceUpdate, GUIUpdate).chain(),
        )
        .configure_sets(Update, ObjectsUpdate.run_if(in_state(Loaded)))
        .configure_sets(FixedUpdate, PhysicsUpdate.run_if(in_state(Loaded)))
        .add_systems(OnExit(Loaded), clear_loaded)
        .add_systems(OnEnter(GameStage::Action), enable_time)
        .add_systems(OnEnter(GameStage::Preparation), disable_time);
    }
}

#[derive(Resource)]
pub struct TempDirectory(pub TempDir);

impl Default for TempDirectory {
    fn default() -> Self {
        Self(tempdir().unwrap())
    }
}

#[derive(Resource)]
pub struct GameFiles {
    pub root: PathBuf,
    pub trajectories: PathBuf,
}

impl GameFiles {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let root: PathBuf = path.as_ref().into();
        let trajectories = root.join(TRAJECTORIES_PATH);
        create_dir_all(trajectories)?;
        Ok(Self {
            trajectories: root.join(TRAJECTORIES_PATH),
            root,
        })
    }
}

/// This state represents whether the app is running the main game (singleplayer or multiplayer) or not, and is loaded
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InGame;

impl ComputedStates for InGame {
    type SourceStates = (Option<ClientMode>, Loaded);

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        if !matches!(sources.1, Loaded) {
            None
        } else {
            match sources.0 {
                Some(ClientMode::Singleplayer | ClientMode::Multiplayer) | None => Some(InGame),
                _ => None,
            }
        }
    }
}

#[derive(SubStates, Debug, Hash, PartialEq, Eq, Clone, Default)]
#[source(InGame = InGame)]
pub enum GameStage {
    #[default]
    Preparation,
    Action,
}

impl std::fmt::Display for GameStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                GameStage::Preparation => "Preparation",
                GameStage::Action => "Action",
            }
        )
    }
}

/// This state represents whether or not bodies and ships are loaded in game.
/// For server, is is automatically the case, but for a client a system is loaded only if one is connected to a server,
/// or if the singleplayer or explore modes have been launched
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Loaded;

impl ComputedStates for Loaded {
    type SourceStates = ClientMode;

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            ClientMode::None => None,
            _ => Some(Loaded),
        }
    }
}

#[derive(Component)]
pub struct ClearOnUnload;

/// This state represents whether or not the running instance is authoritative or not,
/// i.e. if it is the server or it runs in singleplayer.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Authoritative;

impl ComputedStates for Authoritative {
    type SourceStates = Option<ClientMode>;

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            Some(ClientMode::Singleplayer) | None => Some(Self),
            _ => None,
        }
    }
}

fn clear_loaded(mut commands: Commands, query: Query<Entity, With<ClearOnUnload>>) {
    for e in query.iter() {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<BodiesMapping>();
    commands.remove_resource::<ShipsMapping>();
}

fn enable_time(mut toggle: ResMut<ToggleTime>) {
    toggle.0 = true;
}
fn disable_time(mut toggle: ResMut<ToggleTime>) {
    toggle.0 = false;
}

#[cfg(test)]
mod tests {
    use bevy::{app::App, state::state::State};

    use crate::prelude::*;

    fn new_app() -> App {
        let mut app = App::new();
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Singleplayer));
        app.update();
        app.update();
        app
    }

    #[test]
    fn test_states() {
        let app = new_app();
        assert!(app.world().contains_resource::<State<InGame>>());
        assert_eq!(
            *app.world().resource::<State<GameStage>>(),
            GameStage::Preparation
        );
    }
}

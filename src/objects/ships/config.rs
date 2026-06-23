use std::{collections::HashMap, path::Path};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{trajectory::Trajectory, ShipID};

pub const SHIPS_PATH: &str = "ships";

/// Stores ship component declarations in memory for the duration of the game session.
/// Survives screen transitions so components are not lost when leaving the Components screen.
#[derive(Resource, Default)]
pub struct ShipComponentsStore(pub HashMap<ShipID, ShipComponents>);

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct TankConfig {
    pub capacite: f64,
    pub carburant: f64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ThrusterConfig {
    pub force_max: f64,
    pub consommation: f64,
    pub reservoir: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SensorConfig {
    pub portee: f64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ShipComponents {
    pub tanks: HashMap<String, TankConfig>,
    pub thrusters: HashMap<String, ThrusterConfig>,
    pub sensors: HashMap<String, SensorConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ShipConfig {
    pub id: String,
    pub spawn_pos: [f64; 3],
    pub spawn_speed: [f64; 3],
    #[serde(default)]
    pub components: ShipComponents,
    #[serde(default)]
    pub trajectory: Trajectory,
}

impl ShipConfig {
    pub fn ship_id(&self) -> Option<ShipID> {
        ShipID::from(self.id.as_str()).ok()
    }
}

pub fn save_ship_config(dir: &Path, config: &ShipConfig) -> std::io::Result<()> {
    let path = dir.join(format!("{}.json", config.id));
    let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

pub fn load_ship_config(path: &Path) -> std::io::Result<ShipConfig> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(std::io::Error::other)
}

pub fn list_ship_configs(dir: &Path) -> Vec<(String, std::path::PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut result: Vec<(String, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .map(|e| {
            let name = e
                .path()
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (name, e.path())
        })
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_config(id: &str) -> ShipConfig {
        let mut tanks = HashMap::new();
        tanks.insert("principal".into(), TankConfig { capacite: 500.0, carburant: 300.0 });
        let mut thrusters = HashMap::new();
        thrusters.insert("moteur1".into(), ThrusterConfig {
            force_max: 10.0,
            consommation: 0.5,
            reservoir: "principal".into(),
        });
        let mut sensors = HashMap::new();
        sensors.insert("radar".into(), SensorConfig { portee: 1_000_000.0 });

        ShipConfig {
            id: id.into(),
            spawn_pos: [1.0, 2.0, 3.0],
            spawn_speed: [4.0, 5.0, 6.0],
            components: ShipComponents { tanks, thrusters, sensors },
            trajectory: Trajectory::default(),
        }
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let dir = tempdir().unwrap();
        let config = sample_config("explorer");

        save_ship_config(dir.path(), &config).unwrap();

        let path = dir.path().join("explorer.json");
        let loaded = load_ship_config(&path).unwrap();

        assert_eq!(loaded.id, "explorer");
        assert_eq!(loaded.spawn_pos, [1.0, 2.0, 3.0]);
        assert_eq!(loaded.spawn_speed, [4.0, 5.0, 6.0]);
        assert!(loaded.components.tanks.contains_key("principal"));
        assert!(loaded.components.thrusters.contains_key("moteur1"));
        assert!(loaded.components.sensors.contains_key("radar"));

        let tank = &loaded.components.tanks["principal"];
        assert_eq!(tank.capacite, 500.0);
        assert_eq!(tank.carburant, 300.0);

        let thruster = &loaded.components.thrusters["moteur1"];
        assert_eq!(thruster.force_max, 10.0);
        assert_eq!(thruster.consommation, 0.5);
        assert_eq!(thruster.reservoir, "principal");

        let sensor = &loaded.components.sensors["radar"];
        assert_eq!(sensor.portee, 1_000_000.0);
    }

    #[test]
    fn test_save_creates_file_named_after_id() {
        let dir = tempdir().unwrap();
        save_ship_config(dir.path(), &sample_config("voyager")).unwrap();
        assert!(dir.path().join("voyager.json").exists());
    }

    #[test]
    fn test_load_nonexistent_file_returns_error() {
        let dir = tempdir().unwrap();
        let result = load_ship_config(&dir.path().join("missing.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_ship_configs_sorted_and_json_only() {
        let dir = tempdir().unwrap();
        save_ship_config(dir.path(), &sample_config("zulu")).unwrap();
        save_ship_config(dir.path(), &sample_config("alpha")).unwrap();
        save_ship_config(dir.path(), &sample_config("mike")).unwrap();
        std::fs::write(dir.path().join("ignore.toml"), "not json").unwrap();

        let list = list_ship_configs(dir.path());

        assert_eq!(list.len(), 3);
        assert_eq!(list[0].0, "alpha");
        assert_eq!(list[1].0, "mike");
        assert_eq!(list[2].0, "zulu");
        // Non-JSON file is excluded
        assert!(list.iter().all(|(name, _)| name != "ignore"));
    }

    #[test]
    fn test_list_ship_configs_empty_dir() {
        let dir = tempdir().unwrap();
        assert!(list_ship_configs(dir.path()).is_empty());
    }

    #[test]
    fn test_list_ship_configs_nonexistent_dir() {
        let result = list_ship_configs(std::path::Path::new("/nonexistent/path"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_ship_id_valid() {
        let config = sample_config("explorer");
        assert!(config.ship_id().is_some());
    }

    #[test]
    fn test_ship_id_too_long() {
        let long_id = "a".repeat(256);
        let config = ShipConfig {
            id: long_id,
            spawn_pos: [0.0; 3],
            spawn_speed: [0.0; 3],
            components: ShipComponents::default(),
            trajectory: Trajectory::default(),
        };
        assert!(config.ship_id().is_none());
    }

    #[test]
    fn test_components_default_empty() {
        let c = ShipComponents::default();
        assert!(c.tanks.is_empty());
        assert!(c.thrusters.is_empty());
        assert!(c.sensors.is_empty());
    }
}

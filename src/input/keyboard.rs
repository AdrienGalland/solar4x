use std::{
    fs::File,
    io::{Read, Result, Write},
    path::Path,
};

use bevy::ecs::system::Resource;
use serde::{Deserialize, Serialize};

use super::key::Key;

#[derive(Resource, Default, Clone, Serialize, Deserialize, Debug)]
pub struct Keymap {
    pub explorer: ExplorerKeymap,
    pub start_menu: StartMenuKeymap,
    pub fleet_screen: FleetScreenKeymap,
    pub editor: EditorKeymap,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerKeymap {
    pub tree: TreeViewKeymap,
    pub search: SearchViewKeymap,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StartMenuKeymap {
    pub select_next: Key,
    pub select_previous: Key,
    pub quit: Key,
    pub validate: Key,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FleetScreenKeymap {
    pub select_next: Key,
    pub select_previous: Key,
    pub back: Key,
    pub edit_trajectory: Key,
    pub new_ship: Key,
    pub cycle_options: Key,
    pub cycle_options_back: Key,
    pub validate_new_ship: Key,
    pub delete_char: Key,
    pub enter_explorer: Key,
    pub run_time: Key,
    pub pause_time: Key,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EditorKeymap {
    pub select_next: Key,
    pub select_previous: Key,
    pub back: Key,
    pub new_node: Key,
    pub open_scheduler: Key,
    pub run_time: Key,
    pub pause_time: Key,
}

impl Keymap {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        toml::from_str(&buf).map_err(std::io::Error::other)
    }

    pub fn write_to_file(&self, path: impl AsRef<Path>, overwrite: bool) -> Result<()> {
        let mut file = if overwrite {
            File::create(path)?
        } else {
            File::create_new(path)?
        };
        file.write_all(
            toml::to_string_pretty(self)
                .map_err(std::io::Error::other)?
                .as_bytes(),
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TreeViewKeymap {
    pub select_next: Key,
    pub select_previous: Key,
    pub zoom_in: Key,
    pub zoom_out: Key,
    pub toggle_expand: Key,
    pub map_offset_up: Key,
    pub map_offset_down: Key,
    pub map_offset_left: Key,
    pub map_offset_right: Key,
    pub map_offset_reset: Key,
    pub enter_search: Key,
    pub focus: Key,
    pub autoscale: Key,
    pub back: Key,
    pub speed_up: Key,
    pub slow_down: Key,
    pub toggle_time: Key,
    pub toggle_info: Key,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchViewKeymap {
    pub move_cursor_right: Key,
    pub move_cursor_left: Key,
    pub select_next: Key,
    pub select_previous: Key,
    pub leave_search: Key,
    pub validate_search: Key,
    pub delete_char: Key,
}

impl Default for TreeViewKeymap {
    fn default() -> Self {
        Self {
            select_next: Key::from_str_unchecked("down"),
            select_previous: Key::from_str_unchecked("up"),
            zoom_in: Key::from_str_unchecked("+"),
            zoom_out: Key::from_str_unchecked("-"),
            toggle_info: Key::from_str_unchecked("i"),
            toggle_expand: Key::from_str_unchecked("space"),
            map_offset_up: Key::from_str_unchecked("w"),
            map_offset_down: Key::from_str_unchecked("s"),
            map_offset_left: Key::from_str_unchecked("a"),
            map_offset_right: Key::from_str_unchecked("d"),
            map_offset_reset: Key::from_str_unchecked("0"),
            enter_search: Key::from_str_unchecked("/"),
            focus: Key::from_str_unchecked("f"),
            autoscale: Key::from_str_unchecked("x"),
            back: Key::from_str_unchecked("esc"),
            speed_up: Key::from_str_unchecked(">"),
            slow_down: Key::from_str_unchecked("<"),
            toggle_time: Key::from_str_unchecked("t"),
        }
    }
}

impl Default for SearchViewKeymap {
    fn default() -> Self {
        Self {
            move_cursor_right: Key::from_str_unchecked("right"),
            move_cursor_left: Key::from_str_unchecked("left"),
            select_next: Key::from_str_unchecked("down"),
            select_previous: Key::from_str_unchecked("up"),
            leave_search: Key::from_str_unchecked("esc"),
            validate_search: Key::from_str_unchecked("enter"),
            delete_char: Key::from_str_unchecked("backspace"),
        }
    }
}

impl Default for StartMenuKeymap {
    fn default() -> Self {
        Self {
            select_next: Key::from_str_unchecked("down"),
            select_previous: Key::from_str_unchecked("up"),
            quit: Key::from_str_unchecked("esc"),
            validate: Key::from_str_unchecked("space"),
        }
    }
}

impl Default for FleetScreenKeymap {
    fn default() -> Self {
        Self {
            select_next: Key::from_str_unchecked("down"),
            select_previous: Key::from_str_unchecked("up"),
            back: Key::from_str_unchecked("esc"),
            edit_trajectory: Key::from_str_unchecked("space"),
            new_ship: Key::from_str_unchecked("n"),
            cycle_options: Key::from_str_unchecked("tab"),
            cycle_options_back: Key::from_str_unchecked("S backtab"),
            validate_new_ship: Key::from_str_unchecked("enter"),
            delete_char: Key::from_str_unchecked("backspace"),
            enter_explorer: Key::from_str_unchecked("e"),
            run_time: Key::from_str_unchecked("r"),
            pause_time: Key::from_str_unchecked("p"),
        }
    }
}

impl Default for EditorKeymap {
    fn default() -> Self {
        Self {
            select_next: Key::from_str_unchecked("down"),
            select_previous: Key::from_str_unchecked("up"),
            back: Key::from_str_unchecked("esc"),
            new_node: Key::from_str_unchecked("n"),
            open_scheduler: Key::from_str_unchecked("s"),
            run_time: Key::from_str_unchecked("r"),
            pause_time: Key::from_str_unchecked("p"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Keymap;

    #[test]
    fn test_default_keymap() {
        Keymap::default();
    }
}

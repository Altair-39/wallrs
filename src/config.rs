use std::env;
use std::fs;
use std::path::PathBuf;
use toml::Value;

use crate::tui::Tab;

#[derive(Clone)]
pub struct CustomKeybindings {
    pub search: char,
    pub favorite: char,
    pub multi_select: char,
}

#[derive(Clone)]
pub struct Config {
    pub wallpaper_dir: PathBuf,
    pub session: Session,
    pub vim_motion: bool,
    pub enable_mouse_support: bool,
    pub keybindings: CustomKeybindings,
    pub tabs: Vec<TabConfig>,
    pub list_position: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Session {
    X11,
    Wayland,
}

#[derive(Debug, Clone, Copy)]
pub struct TabConfig {
    pub tab: Tab,
    pub enabled: bool,
}

impl TabConfig {
    pub fn default_tabs() -> Vec<Self> {
        vec![
            Self {
                tab: Tab::Wallpapers,
                enabled: true,
            },
            Self {
                tab: Tab::History,
                enabled: true,
            },
            Self {
                tab: Tab::Favorites,
                enabled: true,
            },
        ]
    }
}

impl Config {
    pub fn load() -> Self {
        // Detect session type
        let session = if env::var("WAYLAND_DISPLAY").is_ok() {
            Session::Wayland
        } else {
            Session::X11
        };

        // Resolve config paths
        let xdg_config = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".config"));

        let config_file = xdg_config.join("wallrs/config.toml");
        let keybindings_file = xdg_config.join("wallrs/keybindings.toml");

        // Default values
        let default_dir = dirs::home_dir().unwrap().join("Pictures/Wallpapers");
        let mut wallpaper_dir = default_dir;
        let mut vim_motion = false;
        let mut enable_mouse_support = false;
        let mut keybindings = CustomKeybindings::default();
        let mut tabs = TabConfig::default_tabs();
        let mut list_position = String::from("left"); // Default: list on the left

        // Load main config.toml
        if config_file.exists() {
            let contents = fs::read_to_string(&config_file).expect("Failed to read config file");
            let value: Value = toml::from_str(&contents).expect("Invalid TOML in config file");

            if let Some(path_str) = value.get("wallpaper_dir").and_then(|v| v.as_str()) {
                wallpaper_dir = PathBuf::from(path_str);
            }

            if let Some(v) = value.get("vim_motion").and_then(|v| v.as_bool()) {
                vim_motion = v;
            }

            if let Some(v) = value.get("enable_mouse_support").and_then(|v| v.as_bool()) {
                enable_mouse_support = v;
            }

            if let Some(v) = value.get("list_position").and_then(|v| v.as_str()) {
                let lower = v.to_lowercase();
                if lower == "left" || lower == "right" || lower == "top" || lower == "bottom" {
                    list_position = lower;
                }
            }

            // Load tab configuration
            if let Some(tab_val) = value.get("tabs") {
                if let Some(arr) = tab_val.as_array() {
                    let mut parsed: Vec<TabConfig> = Vec::new();
                    for item in arr {
                        match item {
                            Value::String(s) => {
                                if let Some(tab) = Tab::from_name(s) {
                                    parsed.push(TabConfig { tab, enabled: true });
                                }
                            }
                            Value::Table(tbl) => {
                                if let Some(name) = tbl.get("name").and_then(|v| v.as_str()) {
                                    if let Some(tab) = Tab::from_name(name) {
                                        let enabled = tbl
                                            .get("enabled")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(true);
                                        parsed.push(TabConfig { tab, enabled });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    if !parsed.is_empty() {
                        tabs = parsed;
                    }
                } else if let Some(tbl) = tab_val.as_table() {
                    let mut by_name = std::collections::HashMap::new();
                    for (k, v) in tbl {
                        if let Some(enabled) = v.as_bool() {
                            if let Some(tab) = Tab::from_name(k) {
                                by_name.insert(tab, enabled);
                            }
                        }
                    }
                    let mut merged: Vec<TabConfig> = Vec::new();
                    for def in TabConfig::default_tabs() {
                        let enabled = by_name.get(&def.tab).copied().unwrap_or(def.enabled);
                        merged.push(TabConfig {
                            tab: def.tab,
                            enabled,
                        });
                    }
                    tabs = merged;
                }
            }
        }

        // Load keybindings.toml if present
        if keybindings_file.exists() {
            let contents =
                fs::read_to_string(&keybindings_file).expect("Failed to read keybindings.toml");
            let value: Value = toml::from_str(&contents).expect("Invalid TOML in keybindings.toml");

            if let Some(c) = value.get("search").and_then(|v| v.as_str()) {
                if let Some(ch) = c.chars().next() {
                    keybindings.search = ch;
                }
            }

            if let Some(c) = value.get("favorite").and_then(|v| v.as_str()) {
                if let Some(ch) = c.chars().next() {
                    keybindings.favorite = ch;
                }
            }

            if let Some(c) = value.get("multi_select").and_then(|v| v.as_str()) {
                if let Some(ch) = c.chars().next() {
                    keybindings.multi_select = ch;
                }
            }
        }

        Self {
            wallpaper_dir,
            session,
            vim_motion,
            enable_mouse_support,
            keybindings,
            tabs,
            list_position,
        }
    }
}

impl Default for CustomKeybindings {
    fn default() -> Self {
        Self {
            search: '/',
            favorite: 'f',
            multi_select: 'v',
        }
    }
}

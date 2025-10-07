use std::{env, fs, path::PathBuf};
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
    pub transition_type: String,
    pub commands: CommandConfig,
}

#[derive(Debug, Clone, Copy)]
pub enum Session {
    X11,
    Wayland,
}

#[derive(Clone)]
pub struct CommandConfig {
    pub wal: Vec<String>,
    pub swww: Vec<String>,
    pub feh: Vec<String>,
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
        let mut list_position = String::from("left");
        let mut transition_type = String::from("fade");

        // Default command arguments
        let default_commands = CommandConfig {
            wal: vec![
                "-i".into(),
                "{path}".into(),
                "-n".into(),
                "--backend".into(),
                "wal".into(),
            ],
            swww: vec![
                "img".into(),
                "{path}".into(),
                "--transition-fps".into(),
                "60".into(),
                "--transition-type".into(),
                "{transition}".into(),
            ],
            feh: vec!["--bg-scale".into(), "{path}".into()],
        };
        let mut commands = default_commands.clone();

        // Load main config.toml if it exists
        let value: Option<Value> = if config_file.exists() {
            let contents = fs::read_to_string(&config_file).expect("Failed to read config.toml");
            Some(toml::from_str(&contents).expect("Invalid TOML in config.toml"))
        } else {
            None
        };

        if let Some(value) = &value {
            // General settings
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
                if ["left", "right", "top", "bottom"].contains(&lower.as_str()) {
                    list_position = lower;
                }
            }

            if let Some(v) = value.get("transition_type").and_then(|v| v.as_str()) {
                let valid = ["fade", "wipe", "grow", "outer", "any", "none", "random"];
                let lower = v.to_lowercase();
                if valid.contains(&lower.as_str()) {
                    transition_type = lower;
                }
            }

            // --- Load commands safely (merge with defaults) ---
            if let Some(cmds) = value.get("commands").and_then(|v| v.as_table()) {
                let merge = |default: &Vec<String>, custom: Option<&Vec<Value>>| -> Vec<String> {
                    match custom {
                        Some(arr) if !arr.is_empty() => {
                            // Convert TOML values to strings
                            let custom_args: Vec<String> = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();

                            let mut merged = Vec::new();

                            // Always start with the default command prefix (like "img")
                            if !custom_args
                                .first()
                                .map_or(false, |a| a == "img" || a == "-i")
                            {
                                merged.push(default[0].clone());
                            }

                            // Add user args
                            merged.extend(custom_args);

                            // Ensure `{path}` exists somewhere
                            if !merged.iter().any(|a| a.contains("{path}")) {
                                merged.push("{path}".into());
                            }

                            merged
                        }
                        _ => default.clone(),
                    }
                };

                commands.wal = merge(
                    &default_commands.wal,
                    cmds.get("wal").and_then(|v| v.as_array()),
                );
                commands.swww = merge(
                    &default_commands.swww,
                    cmds.get("swww").and_then(|v| v.as_array()),
                );
                commands.feh = merge(
                    &default_commands.feh,
                    cmds.get("feh").and_then(|v| v.as_array()),
                );
            }

            // --- Load tab configuration ---
            if let Some(tab_val) = value.get("tabs") {
                if let Some(arr) = tab_val.as_array() {
                    let mut parsed = Vec::new();
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
                }
            }
        }

        // Load keybindings.toml if present
        if keybindings_file.exists() {
            let contents =
                fs::read_to_string(&keybindings_file).expect("Failed to read keybindings.toml");
            let value: Value = toml::from_str(&contents).expect("Invalid TOML in keybindings.toml");

            if let Some(c) = value
                .get("search")
                .and_then(|v| v.as_str())
                .and_then(|s| s.chars().next())
            {
                keybindings.search = c;
            }
            if let Some(c) = value
                .get("favorite")
                .and_then(|v| v.as_str())
                .and_then(|s| s.chars().next())
            {
                keybindings.favorite = c;
            }
            if let Some(c) = value
                .get("multi_select")
                .and_then(|v| v.as_str())
                .and_then(|s| s.chars().next())
            {
                keybindings.multi_select = c;
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
            transition_type,
            commands,
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

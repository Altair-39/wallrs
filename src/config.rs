use std::env;
use std::fs;
use std::path::PathBuf;

pub struct Config {
    pub wallpaper_dir: PathBuf,
    pub session: Session,
    pub vim_motion: bool,
    pub enable_mouse_support: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Session {
    X11,
    Wayland,
}

impl Config {
    pub fn load() -> Self {
        // Detect session type
        let session = if env::var("WAYLAND_DISPLAY").is_ok() {
            Session::Wayland
        } else {
            Session::X11
        };

        // XDG config path
        let xdg_config = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".config"));

        let config_file = xdg_config.join("wallrs/config.toml");

        // Default wallpaper directory
        let default_dir = dirs::home_dir().unwrap().join("Pictures/Wallpapers");

        // Defaults
        let mut wallpaper_dir = default_dir.clone();
        let mut vim_motion = false;
        let mut enable_mouse_support = false;

        // Read TOML config if exists
        if config_file.exists() {
            let contents = fs::read_to_string(&config_file).expect("Failed to read config file");
            let value: toml::Value =
                toml::from_str(&contents).expect("Invalid TOML in config file");

            if let Some(path_str) = value.get("wallpaper_dir").and_then(|v| v.as_str()) {
                wallpaper_dir = PathBuf::from(path_str);
            }

            if let Some(v) = value.get("vim_motion").and_then(|v| v.as_bool()) {
                vim_motion = v;
            }

            if let Some(v) = value.get("enable_mouse_support").and_then(|v| v.as_bool()) {
                enable_mouse_support = v;
            }
        }

        Config {
            wallpaper_dir,
            session,
            vim_motion,
            enable_mouse_support,
        }
    }
}

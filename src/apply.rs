use crate::config::Config;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

pub fn apply_wallpaper(path: &Path, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Always run wal to generate colors
    Command::new("wal")
        .args(&["-i", path.to_str().unwrap(), "-n", "--backend", "wal"])
        .stdout(Stdio::null()) // discard stdout
        .stderr(Stdio::null()) // discard stderr
        .status()?;

    match config.session {
        crate::config::Session::Wayland => {
            let transition = if !config.transition_type.is_empty() {
                config.transition_type.as_str()
            } else {
                "fade"
            };

            Command::new("swww")
                .args(&[
                    "img",
                    path.to_str().unwrap(),
                    "--transition-fps",
                    "60",
                    "--transition-type",
                    transition,
                ])
                .status()?;
        }

        crate::config::Session::X11 => {
            if Command::new("which").arg("nitrogen").status()?.success() {
                Command::new("nitrogen")
                    .args(&["--set-zoom-fill", path.to_str().unwrap(), "--save"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?;
            } else {
                Command::new("feh")
                    .args(&["--bg-scale", path.to_str().unwrap()])
                    .status()?;
            }
        }
    }

    // Reload waybar (if running)
    Command::new("pkill")
        .args(&["-USR2", "waybar"])
        .status()
        .ok();

    // Copy current wallpaper to rofi preview folder
    fs::copy(
        path,
        dirs::home_dir()
            .unwrap()
            .join(".config/rofi/images/current.jpg"),
    )?;

    Ok(())
}

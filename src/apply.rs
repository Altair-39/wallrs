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
    // Apply wallpaper depending on session
    match config.session {
        crate::config::Session::Wayland => {
            Command::new("swww")
                .args(&[
                    "img",
                    path.to_str().unwrap(),
                    "--transition-fps",
                    "60",
                    "--transition-type",
                    "fade",
                ])
                .status()?;
        }
        crate::config::Session::X11 => {
            Command::new("feh")
                .args(&["--bg-scale", path.to_str().unwrap()])
                .status()?;
        }
    }

    Command::new("pkill").args(&["-USR2", "waybar"]).status()?;

    // Copy current wallpaper to rofi folder
    fs::copy(
        path,
        dirs::home_dir()
            .unwrap()
            .join(".config/rofi/images/current.jpg"),
    )?;

    Ok(())
}

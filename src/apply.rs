use crate::config::Config;
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub fn apply_wallpaper(path: &Path, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = path.to_str().unwrap();
    let transition = if !config.transition_type.is_empty() {
        config.transition_type.as_str()
    } else {
        "fade"
    };

    // Replace placeholders in args
    let expand_args = |args: &[String]| -> Vec<String> {
        args.iter()
            .map(|arg| {
                arg.replace("{path}", path_str)
                    .replace("{transition}", transition)
            })
            .collect()
    };
    if config.pywal {
        // Run wal
        Command::new("wal")
            .args(expand_args(&config.commands.wal))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    if config.hellwal {
        // Run hellwal
        Command::new("hellawal")
            .args(expand_args(&config.commands.wal))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }

    match config.session {
        crate::config::Session::Wayland => {
            Command::new("swww")
                .args(expand_args(&config.commands.swww))
                .status()?;
        }
        crate::config::Session::X11 => {
            Command::new("feh")
                .args(expand_args(&config.commands.feh))
                .status()?;
        }
    }

    // Reload waybar
    Command::new("pkill")
        .args(["-USR2", "waybar"])
        .status()
        .ok();

    Ok(())
}

use crate::config::Config;
use std::{
    fs,
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

    if config.telegram {
        // Run telegram-palette-gen
        Command::new("telegram-palette-gen")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }

    if config.pywal {
        // Run wal
        Command::new("wal")
            .args(expand_args(&config.commands.wal))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        // Update niri config if wm is niri
        if config.wm == "niri" {
            update_niri_colors()?;
        }
    }

    if config.hellwal {
        // Run hellwal
        Command::new("hellwal")
            .args(expand_args(&config.commands.wal))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        // Update niri config if wm is niri
        if config.wm == "niri" {
            update_niri_colors()?;
        }
    }

    match config.session {
        crate::config::Session::Wayland => {
            if config.mpvpaper {
                Command::new("mpvpaper")
                    .args(expand_args(&config.commands.mpvpaper))
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?;
            } else {
                Command::new("swww")
                    .args(expand_args(&config.commands.swww))
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?;
            }
            // Reload waybar
            Command::new("pkill")
                .args(["-USR2", "waybar"])
                .status()
                .ok();
        }
        crate::config::Session::X11 => {
            Command::new("feh")
                .args(expand_args(&config.commands.feh))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
        }
    }

    Ok(())
}

fn update_niri_colors() -> Result<(), Box<dyn std::error::Error>> {
    let home_dir = std::env::var("HOME")?;
    let niri_config_path = Path::new(&home_dir).join(".config/niri/config.kdl");

    if !niri_config_path.exists() {
        return Ok(());
    }

    // Read the current niri config
    let config_content = fs::read_to_string(&niri_config_path)?;

    let wal_colors = get_wal_colors()?;
    let active_color = &wal_colors[2];
    let inactive_color = &wal_colors[8];

    // Update the focus-ring section in the config
    let updated_content = update_focus_ring_colors(&config_content, active_color, inactive_color);

    // Write the updated config back
    fs::write(&niri_config_path, updated_content)?;

    Ok(())
}

fn get_wal_colors() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let home_dir = std::env::var("HOME")?;
    let colors_file = Path::new(&home_dir).join(".cache/wal/colors");

    if !colors_file.exists() {
        return Err("Wal colors file not found".into());
    }

    let colors_content = fs::read_to_string(colors_file)?;
    let colors: Vec<String> = colors_content
        .lines()
        .map(|line| line.trim().to_string())
        .collect();

    Ok(colors)
}

fn update_focus_ring_colors(
    config_content: &str,
    active_color: &str,
    inactive_color: &str,
) -> String {
    let mut in_focus_ring_section = false;
    let mut updated_lines = Vec::new();

    for line in config_content.lines() {
        let trimmed = line.trim();

        // Check if we're entering the focus-ring section
        if trimmed.starts_with("focus-ring {") {
            in_focus_ring_section = true;
            updated_lines.push(line.to_string());
            continue;
        }

        // Check if we're leaving the focus-ring section
        if in_focus_ring_section && trimmed == "}" {
            in_focus_ring_section = false;
            updated_lines.push(line.to_string());
            continue;
        }

        // Update colors within the focus-ring section
        if in_focus_ring_section {
            if trimmed.starts_with("active-color") {
                updated_lines.push(format!("    active-color \"{}\"", active_color));
                continue;
            } else if trimmed.starts_with("inactive-color") {
                updated_lines.push(format!("    inactive-color \"{}\"", inactive_color));
                continue;
            }
        }

        updated_lines.push(line.to_string());
    }

    updated_lines.join("\n")
}

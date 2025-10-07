mod apply;
mod config;
mod input;
mod mouse;
mod persistence;
mod tui;
mod wallpapers;

use apply::apply_wallpaper;
use config::Config;
use crossterm::event::EnableFocusChange;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tui::run_tui;
use wallpapers::load_wallpapers;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI flags
    let args: Vec<String> = env::args().collect();
    let print_only = args.iter().any(|a| a == "--print");

    // Check if --path flag is provided (directory)
    let path_arg = args
        .windows(2)
        .find(|w| w[0] == "--path")
        .map(|w| PathBuf::from(&w[1]));

    // Load config
    let mut cfg = Config::load();

    cfg.pywal = args.iter().any(|a| a == "--pywal");
    // If --path is set, override wallpaper_dir
    if let Some(path) = path_arg {
        if !path.is_dir() {
            eprintln!(
                "Error: specified path is not a directory: {}",
                path.display()
            );
            return Ok(());
        }
        cfg.wallpaper_dir = path;
    }

    // Load wallpapers
    let wallpapers = load_wallpapers(&cfg.wallpaper_dir)?;
    if wallpapers.is_empty() {
        eprintln!("No wallpapers found in {}", cfg.wallpaper_dir.display());
        return Ok(());
    }

    // Run TUI to select a wallpaper
    let selected_wallpaper = run_tui(&wallpapers, &cfg)?;

    if print_only {
        Command::new("wal")
            .args(&[
                "-i",
                selected_wallpaper.to_str().unwrap(),
                "-n",
                "--backend",
                "wal",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        // Save selected wallpaper to cache as current.<ext>
        let cache_dir: PathBuf = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("wallrs");
        fs::create_dir_all(&cache_dir)?;

        let ext = selected_wallpaper
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let cache_file = cache_dir.join(format!("current.{}", ext));

        fs::copy(&selected_wallpaper, &cache_file)?;
        Command::new("pkill").args(&["-USR2", "waybar"]).status()?;

        println!("Saved selection to {}", cache_file.display());
    } else {
        // Apply wallpaper normally
        apply_wallpaper(&selected_wallpaper, &cfg)?;
    }

    Ok(())
}

mod apply;
mod config;
mod input;
mod mouse;
mod persistence;
mod tui;
mod wallpapers;

use apply::apply_wallpaper;
use clap::Parser;
use config::Config;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tui::run_tui;
use wallpapers::load_wallpapers;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the wallpaper directory
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Only print wallpaper info instead of applying
    #[arg(short, long)]
    print: bool,

    /// Generate colors using pywal
    #[arg(long)]
    pywal: Option<bool>,

    /// Generate colors using hellwal
    #[arg(long)]
    hellwal: Option<bool>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI flags
    let args = Args::parse();

    // Load config
    let mut cfg = Config::load();

    if let Some(pywal_flag) = args.pywal {
        cfg.pywal = pywal_flag; // only override if user passed --pywal
    }
    if let Some(hellwal_flag) = args.hellwal {
        cfg.hellwal = hellwal_flag; // only override if user passed --pywal
    }
    // If --path is set, override wallpaper_dir
    if let Some(path) = args.path {
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
    let selected_wallpaper = run_tui(&wallpapers, &cfg).await?;

    if args.print {
        if cfg.pywal {
            Command::new("wal")
                .args([
                    "-i",
                    selected_wallpaper.to_str().unwrap(),
                    "-n",
                    "--backend",
                    "wal",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
        }
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
        Command::new("pkill").args(["-USR2", "waybar"]).status()?;

        println!("Saved selection to {}", cache_file.display());
    } else {
        // Apply wallpaper normally
        apply_wallpaper(&selected_wallpaper, &cfg)?;
    }

    Ok(())
}

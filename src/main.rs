mod apply;
mod config;
mod input;
mod mouse;
mod persistence;
mod tui;
mod wallpapers;

use apply::apply_wallpaper;
use config::Config;
use tui::run_tui;
use wallpapers::load_wallpapers;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load config
    let cfg = Config::load();

    // Load wallpapers
    let wallpapers = load_wallpapers(&cfg.wallpaper_dir)?;
    if wallpapers.is_empty() {
        eprintln!("No wallpapers found!");
        return Ok(());
    }

    let selected_wallpaper = run_tui(&wallpapers, &cfg)?;
    apply_wallpaper(&selected_wallpaper, &cfg)?;

    Ok(())
}

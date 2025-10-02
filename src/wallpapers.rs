use std::path::PathBuf;
use walkdir::WalkDir;

pub fn load_wallpapers(dir: &PathBuf) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut wallpapers: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ["jpg", "jpeg", "png"].contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    wallpapers.sort_by_key(|p| p.file_name().unwrap().to_string_lossy().to_lowercase());

    Ok(wallpapers)
}

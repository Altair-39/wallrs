use std::path::PathBuf;
use walkdir::WalkDir;

pub fn load_wallpapers(
    dir: &PathBuf,
    video: &bool,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut wallpapers: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| {
                    let ext_lower = ext.to_lowercase();
                    if *video {
                        // Include both images and mp4 when video is true
                        ["jpg", "jpeg", "png", "mp4"].contains(&ext_lower.as_str())
                    } else {
                        // Only include images when video is false
                        ["jpg", "jpeg", "png"].contains(&ext_lower.as_str())
                    }
                })
                .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    wallpapers.sort_by_key(|p| p.file_name().unwrap().to_string_lossy().to_lowercase());

    Ok(wallpapers)
}

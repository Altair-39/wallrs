use std::fs;
use std::path::PathBuf;

// ------------------------
// Persistence helpers
// ------------------------
pub fn load_list(name: &str) -> Vec<PathBuf> {
    let path = dirs::home_dir().unwrap().join(".config/wallrs").join(name);
    if let Ok(data) = fs::read_to_string(path) {
        data.lines().map(PathBuf::from).collect()
    } else {
        Vec::new()
    }
}

pub fn save_list(name: &str, list: &[PathBuf]) {
    let path = dirs::home_dir().unwrap().join(".config/wallrs").join(name);
    let _ = fs::write(
        path,
        list.iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

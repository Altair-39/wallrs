use crate::config::{Config as AppConfig, CustomKeybindings, Session};
use crate::input::{Input, handle_input};
use crate::mouse::{MouseInput, handle_mouse};
use crate::persistence::{load_list, save_list};
use crossterm::event::{self, EnableMouseCapture};
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::execute;
use image::DynamicImage;
use ratatui::layout::Alignment;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    text::Text,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use strum_macros::Display;
use tempfile::NamedTempFile;
use tokio::sync::mpsc;
#[derive(Debug, Clone)]
struct ConfigItem {
    name: String,
    value: String,
    field_type: ConfigFieldType,
    category: ConfigCategory,
}

#[derive(Debug, Clone)]
enum ConfigFieldType {
    Boolean,
    Number,
    ListPosition,
    Session,
    TransitionType,
    Keybinding,
    CommandList,
}

#[derive(Debug, Clone, PartialEq)]
enum ConfigCategory {
    General,
    Keybindings,
    Tabs,
    Commands,
}

#[derive(Clone)]
struct ConfigEditState {
    items: Vec<ConfigItem>,
    selected: usize,
    editing: bool,
    current_input: String,
    categories: Vec<ConfigCategory>,
    selected_category: usize,
    keybindings: CustomKeybindings, // Add this
}
// ---------------------------
// Image Cache
// ---------------------------
struct ImageCache {
    cache: HashMap<PathBuf, CachedImage>,
    max_size: usize,
}

impl ImageCache {
    fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            max_size,
        }
    }

    fn get(&mut self, path: &PathBuf) -> Option<&CachedImage> {
        self.cache.get(path)
    }

    fn insert(&mut self, path: PathBuf, image: CachedImage) {
        // Simple LRU-like eviction: remove oldest entries if cache is full
        if self.cache.len() >= self.max_size
            && let Some(key) = self.cache.keys().next().cloned()
        {
            self.cache.remove(&key);
        }

        self.cache.insert(path, image);
    }
}
#[derive(PartialEq, Eq, Clone)]
enum Bool {
    True,
    False,
}

#[derive(Clone)]
struct CachedImage {
    image: Arc<DynamicImage>,
    is_video: Bool,
}

impl CachedImage {
    fn new(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let image = if ["mp4", "avi", "mov", "mkv", "webm"].contains(&extension.as_str()) {
            // Extract thumbnail from video
            Self::extract_video_thumbnail(path)?
        } else {
            // Load regular image
            image::ImageReader::open(path)?
                .with_guessed_format()?
                .decode()?
        };

        Ok(Self {
            image: Arc::new(image),
            is_video: if ["mp4", "avi", "mov", "mkv", "webm"].contains(&extension.as_str()) {
                Bool::True
            } else {
                Bool::False
            },
        })
    }

    fn extract_video_thumbnail(
        path: &PathBuf,
    ) -> Result<DynamicImage, Box<dyn std::error::Error + Send + Sync>> {
        // Create a temporary file for the thumbnail
        let temp_file = NamedTempFile::new()?;
        let temp_path = temp_file.path().with_extension("jpg");

        // Use ffmpeg to extract a frame from the video (at 1 second)
        let output = Command::new("ffmpeg")
            .args([
                "-i",
                path.to_str().unwrap(),
                "-ss",
                "00:00:01", // Seek to 1 second
                "-vframes",
                "1", // Extract 1 frame
                "-q:v",
                "2", // High quality
                temp_path.to_str().unwrap(),
                "-y", // Overwrite output file
            ])
            .output()?;

        if !output.status.success() {
            return Err(
                format!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr)).into(),
            );
        }

        // Load the extracted frame as an image
        let image = image::open(&temp_path)?;

        // Clean up the temporary file (ignore errors)
        let _ = std::fs::remove_file(&temp_path);

        Ok(image)
    }

    fn create_video_placeholder() -> DynamicImage {
        // Create a placeholder image for videos when thumbnail extraction fails
        DynamicImage::ImageRgba8(image::RgbaImage::from_fn(100, 100, |x, y| {
            if (x / 10 + y / 10) % 2 == 0 {
                image::Rgba([70, 70, 70, 255]) // Dark gray
            } else {
                image::Rgba([50, 50, 50, 255]) // Darker gray
            }
        }))
    }
}
// ---------------------------
// Tab Enum
// ---------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize)]
pub enum Tab {
    #[strum(serialize = "Wallpapers")]
    Wallpapers,
    #[strum(serialize = "History")]
    History,
    #[strum(serialize = "Favorites")]
    Favorites,
    #[strum(serialize = "Config")]
    Config,
}

impl Tab {
    pub fn title(self) -> String {
        self.to_string()
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "wallpapers" | "wallpaper" | "wall" => Some(Tab::Wallpapers),
            "history" | "recent" | "recents" => Some(Tab::History),
            "favorites" | "favourites" | "favorite" | "favourite" | "favs" => Some(Tab::Favorites),
            "config" | "configs" => Some(Tab::Config),
            _ => None,
        }
    }
}

impl FromStr for Tab {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Tab::from_name(s).ok_or(())
    }
}

// ---------------------------
// Rename State
// ---------------------------

pub struct RenameState {
    pub original_path: PathBuf,
    pub current_input: String,
    pub error: Option<String>,
}

// ---------------------------
// TUI Application
// ---------------------------

pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    config: AppConfig, // Change to mutable reference
    wallpapers: Vec<PathBuf>,
    history: Vec<PathBuf>,
    favorites: Vec<PathBuf>,
    configs: Vec<PathBuf>,
    selected: usize,
    list_state: ListState,
    search_query: String,
    in_search: bool,
    current_tab: Tab,
    last_preview: Option<PathBuf>,
    multi_select: bool,
    selected_items: Vec<usize>,
    dirty: bool,
    // Image rendering
    picker: Picker,
    preview_state: Option<StatefulProtocol>,
    image_cache: ImageCache,
    preview_tx: mpsc::Sender<(
        PathBuf,
        Result<CachedImage, Box<dyn std::error::Error + Send + Sync>>,
    )>,
    preview_rx: mpsc::Receiver<(
        PathBuf,
        Result<CachedImage, Box<dyn std::error::Error + Send + Sync>>,
    )>,
    rename_state: Option<RenameState>,
    config_edit_state: Option<ConfigEditState>, // Add this
}
impl<'a> TuiApp {
    pub fn new(
        wallpapers: &[PathBuf],
        config: AppConfig, // This should already be mutable
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if config.mouse_support {
            execute!(io::stdout(), EnableMouseCapture)?;
        }

        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let first_tab = config
            .tabs
            .iter()
            .find(|t| t.enabled)
            .map(|t| t.tab)
            .unwrap_or(Tab::Wallpapers);

        let picker = Picker::from_query_stdio()?;

        // Initialize image cache with reasonable default size
        let cache_size = config.image_cache_size.unwrap_or(50);
        let image_cache = ImageCache::new(cache_size);
        let (preview_tx, preview_rx) = mpsc::channel(10);

        Ok(Self {
            terminal,
            config,
            wallpapers: wallpapers.to_vec(),
            history: load_list("history.txt"),
            favorites: load_list("favorites.txt"),
            configs: vec![],
            selected: 0,
            list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            search_query: String::new(),
            in_search: false,
            current_tab: first_tab,
            last_preview: None,
            multi_select: false,
            selected_items: Vec::new(),
            dirty: true,
            picker,
            preview_state: None,
            image_cache,
            preview_tx,
            preview_rx,
            rename_state: None,
            config_edit_state: None, // Add this line
        })
    }
    pub async fn run(&mut self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        // Preload images
        let filtered = self.filter_items();
        let preload_paths: Vec<PathBuf> = filtered.iter().take(10).cloned().collect();
        self.preload_images(&preload_paths);

        loop {
            // Check for completed previews asynchronously
            while let Ok((path, result)) = self.preview_rx.try_recv() {
                if let Ok(cached_image) = result {
                    self.image_cache.insert(path.clone(), cached_image.clone());

                    if Some(&path) == self.filter_items().get(self.selected) {
                        self.preview_state = Some(
                            self.picker
                                .new_resize_protocol(cached_image.image.as_ref().clone()),
                        );
                        self.dirty = true;
                    }
                }
            }

            let filtered = self.filter_items();
            self.adjust_selection(&filtered);

            if self.dirty {
                self.draw_ui(&filtered)?;
                self.dirty = false;
            }

            if event::poll(std::time::Duration::from_millis(16))? {
                if let Some(selected) = self.handle_event(&filtered)? {
                    return Ok(selected);
                }

                self.dirty = true;
            }

            tokio::task::yield_now().await;
        }
    }

    fn create_config_items(&self) -> Vec<ConfigItem> {
        let mut items = Vec::new();

        // General settings
        items.push(ConfigItem {
            name: "Mouse Support".to_string(),
            value: self.config.mouse_support.to_string(),
            field_type: ConfigFieldType::Boolean,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Vim Motion".to_string(),
            value: self.config.vim_motion.to_string(),
            field_type: ConfigFieldType::Boolean,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Session".to_string(),
            value: format!("{:?}", self.config.session),
            field_type: ConfigFieldType::Session,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "List Position".to_string(),
            value: self.config.list_position.clone(),
            field_type: ConfigFieldType::ListPosition,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Transition Type".to_string(),
            value: self.config.transition_type.clone(),
            field_type: ConfigFieldType::TransitionType,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Image Cache Size".to_string(),
            value: self.config.image_cache_size.unwrap_or(50).to_string(),
            field_type: ConfigFieldType::Number,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Telegram".to_string(),
            value: self.config.telegram.to_string(),
            field_type: ConfigFieldType::Boolean,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Pywal".to_string(),
            value: self.config.pywal.to_string(),
            field_type: ConfigFieldType::Boolean,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Hellwal".to_string(),
            value: self.config.hellwal.to_string(),
            field_type: ConfigFieldType::Boolean,
            category: ConfigCategory::General,
        });

        items.push(ConfigItem {
            name: "Mpvpaper".to_string(),
            value: self.config.mpvpaper.to_string(),
            field_type: ConfigFieldType::Boolean,
            category: ConfigCategory::General,
        });

        // Keybindings
        items.push(ConfigItem {
            name: "Search Key".to_string(),
            value: self.config.keybindings.search.to_string(),
            field_type: ConfigFieldType::Keybinding,
            category: ConfigCategory::Keybindings,
        });

        items.push(ConfigItem {
            name: "Favorite Key".to_string(),
            value: self.config.keybindings.favorite.to_string(),
            field_type: ConfigFieldType::Keybinding,
            category: ConfigCategory::Keybindings,
        });

        items.push(ConfigItem {
            name: "Multi-select Key".to_string(),
            value: self.config.keybindings.multi_select.to_string(),
            field_type: ConfigFieldType::Keybinding,
            category: ConfigCategory::Keybindings,
        });

        items.push(ConfigItem {
            name: "Rename Key".to_string(),
            value: self.config.keybindings.rename.to_string(),
            field_type: ConfigFieldType::Keybinding,
            category: ConfigCategory::Keybindings,
        });

        items.push(ConfigItem {
            name: "Quit Key".to_string(),
            value: self.config.keybindings.quit.to_string(),
            field_type: ConfigFieldType::Keybinding,
            category: ConfigCategory::Keybindings,
        });

        // Tabs
        for tab_config in &self.config.tabs {
            items.push(ConfigItem {
                name: format!("{} Tab", tab_config.tab),
                value: tab_config.enabled.to_string(),
                field_type: ConfigFieldType::Boolean,
                category: ConfigCategory::Tabs,
            });
        }

        // Commands
        items.push(ConfigItem {
            name: "Wal Command".to_string(),
            value: self.config.commands.wal.join(" "),
            field_type: ConfigFieldType::CommandList,
            category: ConfigCategory::Commands,
        });

        items.push(ConfigItem {
            name: "Swww Command".to_string(),
            value: self.config.commands.swww.join(" "),
            field_type: ConfigFieldType::CommandList,
            category: ConfigCategory::Commands,
        });

        items.push(ConfigItem {
            name: "Feh Command".to_string(),
            value: self.config.commands.feh.join(" "),
            field_type: ConfigFieldType::CommandList,
            category: ConfigCategory::Commands,
        });

        items.push(ConfigItem {
            name: "Mpvpaper Command".to_string(),
            value: self.config.commands.mpvpaper.join(" "),
            field_type: ConfigFieldType::CommandList,
            category: ConfigCategory::Commands,
        });

        items
    }

    fn start_config_edit(&mut self) {
        // Reset selection to safe state
        self.selected = 0;
        self.list_state.select(Some(0));

        let items = self.create_config_items();
        let categories = vec![
            ConfigCategory::General,
            ConfigCategory::Keybindings,
            ConfigCategory::Tabs,
            ConfigCategory::Commands,
        ];

        // Start with General category items
        let general_items: Vec<ConfigItem> = items
            .into_iter()
            .filter(|item| item.category == ConfigCategory::General)
            .collect();

        // Ensure we have items before creating edit state
        if !general_items.is_empty() {
            self.config_edit_state = Some(ConfigEditState {
                items: general_items,
                selected: 0,
                editing: false,
                current_input: String::new(),
                categories,
                selected_category: 0, // This should match the category of the items (General)
                keybindings: self.config.keybindings.clone(),
            });
        }
    }
    fn apply_config_change(&mut self, item: &ConfigItem, new_value: &str) {
        match item.name.as_str() {
            "Mouse Support" => self.config.mouse_support = new_value.parse().unwrap_or(false),
            "Vim Motion" => self.config.vim_motion = new_value.parse().unwrap_or(false),
            "Session" => {
                self.config.session = if new_value.to_lowercase().contains("wayland") {
                    Session::Wayland
                } else {
                    Session::X11
                };
            }
            "List Position" => self.config.list_position = new_value.to_string(),
            "Transition Type" => self.config.transition_type = new_value.to_string(),
            "Image Cache Size" => {
                if let Ok(size) = new_value.parse() {
                    self.config.image_cache_size = Some(size);
                }
            }
            "Telegram" => self.config.telegram = new_value.parse().unwrap_or(false),
            "Pywal" => self.config.pywal = new_value.parse().unwrap_or(false),
            "Hellwal" => self.config.hellwal = new_value.parse().unwrap_or(false),
            "Mpvpaper" => self.config.mpvpaper = new_value.parse().unwrap_or(false),
            "Search Key" => {
                if let Some(c) = new_value.chars().next() {
                    self.config.keybindings.search = c;
                }
            }
            "Favorite Key" => {
                if let Some(c) = new_value.chars().next() {
                    self.config.keybindings.favorite = c;
                }
            }
            "Multi-select Key" => {
                if let Some(c) = new_value.chars().next() {
                    self.config.keybindings.multi_select = c;
                }
            }
            "Rename Key" => {
                if let Some(c) = new_value.chars().next() {
                    self.config.keybindings.rename = c;
                }
            }
            "Quit Key" => {
                if let Some(c) = new_value.chars().next() {
                    self.config.keybindings.quit = c;
                }
            }
            name if name.ends_with(" Tab") => {
                if let Some(tab_name) = name.strip_suffix(" Tab") && let Ok(tab) = tab_name.parse::<Tab>() && let Some(tab_config) =
                            self.config.tabs.iter_mut().find(|tc| tc.tab == tab)
                        {
                            tab_config.enabled = new_value.parse().unwrap_or(false);
                        }
            }
            "Wal Command" => {
                self.config.commands.wal = new_value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect()
            }
            "Swww Command" => {
                self.config.commands.swww = new_value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect()
            }
            "Feh Command" => {
                self.config.commands.feh = new_value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect()
            }
            "Mpvpaper Command" => {
                self.config.commands.mpvpaper = new_value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect()
            }
            _ => {}
        }
    }
    fn save_config_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wallust")
            .join("config.toml");

        // Create directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a simplified config for serialization without complex types
        #[derive(Serialize)]
        struct SerializableConfig {
            wallpaper_dirs: Vec<PathBuf>,
            vim_motion: bool,
            mouse_support: bool,
            image_cache_size: Option<usize>,
            list_position: String,
            transition_type: String,
            telegram: bool,
            pywal: bool,
            hellwal: bool,
            mpvpaper: bool,
        }

        let serializable_config = SerializableConfig {
            wallpaper_dirs: self.config.wallpaper_dirs.clone(),
            vim_motion: self.config.vim_motion,
            mouse_support: self.config.mouse_support,
            image_cache_size: self.config.image_cache_size,
            list_position: self.config.list_position.clone(),
            transition_type: self.config.transition_type.clone(),
            telegram: self.config.telegram,
            pywal: self.config.pywal,
            hellwal: self.config.hellwal,
            mpvpaper: self.config.mpvpaper,
        };

        let toml_string = toml::to_string(&serializable_config)?;
        fs::write(config_path, toml_string)?;
        Ok(())
    }
    fn request_preview(&self, path: PathBuf) {
        let tx = self.preview_tx.clone();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let extension = path_clone
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                // Check if it's a video file
                if ["mp4", "avi", "mov", "mkv", "webm"].contains(&extension.as_str()) {
                    match CachedImage::new(&path_clone) {
                        Ok(cached_image) => Ok(cached_image),
                        Err(_) => {
                            // Fallback to video placeholder if extraction fails
                            Ok(CachedImage {
                                image: Arc::new(CachedImage::create_video_placeholder()),
                                is_video: Bool::True,
                            })
                        }
                    }
                } else {
                    // Regular image file
                    CachedImage::new(&path_clone)
                }
            })
            .await
            .unwrap_or_else(|e| Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>));

            let _ = tx.send((path, result)).await;
        });
    }
    // --------------------
    // Tab management
    // --------------------

    fn active_tabs(&self) -> Vec<Tab> {
        if !self.config.tabs.is_empty() {
            let out: Vec<Tab> = self
                .config
                .tabs
                .iter()
                .filter(|t| t.enabled)
                .map(|t| t.tab)
                .collect();
            if !out.is_empty() {
                return out;
            }
        }
        vec![Tab::Wallpapers, Tab::History, Tab::Favorites]
    }

    fn current_tab_index(&self) -> usize {
        self.active_tabs()
            .iter()
            .position(|&t| t == self.current_tab)
            .unwrap_or(0)
    }

    // --------------------
    // Filtering & selection
    // --------------------

    fn filter_items(&self) -> Vec<PathBuf> {
        match self.current_tab {
            Tab::Wallpapers => {
                if self.search_query.is_empty() {
                    self.wallpapers.clone()
                } else {
                    let q = self.search_query.to_lowercase();
                    self.wallpapers
                        .iter()
                        .filter(|p| {
                            p.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&q)
                        })
                        .cloned()
                        .collect()
                }
            }
            Tab::History => self.history.clone(),
            Tab::Favorites => self.favorites.clone(),
            Tab::Config => self.configs.clone(),
        }
    }

    fn adjust_selection(&mut self, filtered: &[PathBuf]) {
        if filtered.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
            self.dirty = true;
        } else if self.selected >= filtered.len() {
            self.selected = filtered.len() - 1;
            self.list_state.select(Some(self.selected));
            self.dirty = true;
        }
    }
    fn draw_config_editor(&mut self, edit_state: &ConfigEditState) -> Result<(), Box<dyn std::error::Error>> {
    self.terminal.draw(|f| {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Length(3), // Categories
                Constraint::Min(3),    // Config items
                Constraint::Length(3), // Help
            ])
            .split(area);

        // Title with current category
        let current_category_name = match edit_state.categories[edit_state.selected_category] {
            ConfigCategory::General => "General",
            ConfigCategory::Keybindings => "Keybindings", 
            ConfigCategory::Tabs => "Tabs",
            ConfigCategory::Commands => "Commands",
        };
        
        let title = Paragraph::new(format!("Configuration Editor - {}", current_category_name))
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);

        // Categories - Use Tabs instead of horizontal List
        let category_names: Vec<&str> = edit_state.categories.iter().map(|cat| match cat {
            ConfigCategory::General => "General",
            ConfigCategory::Keybindings => "Keybindings",
            ConfigCategory::Tabs => "Tabs",
            ConfigCategory::Commands => "Commands",
        }).collect();

        // Create tabs for categories
        let tabs = category_names.iter().enumerate().map(|(i, name)| {
            let tab_text = if i == edit_state.selected_category {
                format!("▶ {} ◀", name)
            } else {
                format!("  {}  ", name)
            };
            Line::from(tab_text)
                .style(if i == edit_state.selected_category {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                })
        }).collect::<Vec<_>>();

        let tabs_widget = ratatui::widgets::Tabs::new(tabs)
            .block(Block::default().borders(Borders::ALL).title("Categories"))
            .select(edit_state.selected_category)
            .divider("|");
        f.render_widget(tabs_widget, chunks[1]);

        // Config items
        let items: Vec<ListItem> = edit_state.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let value_display = match item.field_type {
                    ConfigFieldType::Boolean => {
                        if item.value.to_lowercase() == "true" {
                            "✓".to_string()
                        } else {
                            "✗".to_string()
                        }
                    }
                    _ => item.value.clone(),
                };

                let content = if i == edit_state.selected {
                    if edit_state.editing {
                        format!(">> {}: [{}]", item.name, edit_state.current_input)
                    } else {
                        format!(">> {}: {}", item.name, value_display)
                    }
                } else {
                    format!("   {}: {}", item.name, value_display)
                };

                ListItem::new(content)
                    .style(if i == edit_state.selected {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    })
            })
            .collect();

        let list_title = format!("Settings ({})", edit_state.items.len());
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(list_title))
            .highlight_symbol(">> ");
        f.render_widget(list, chunks[2]);

        // Help
        let help_text = if edit_state.editing {
            "Enter: Confirm | Esc: Cancel"
        } else {
            &format!("↑↓/jk: Navigate | Enter/Space: Edit | Tab/Shift+Tab: Switch Categories | {}: Exit | Ctrl+S: Save", 
                    edit_state.keybindings.quit)
        };
        let help = Paragraph::new(help_text)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[3]);

        // Set cursor position if editing
        if edit_state.editing && let Some(item) = edit_state.items.get(edit_state.selected) {
                let cursor_x = chunks[2].x + 6 + item.name.len() as u16 + 2 + edit_state.current_input.len() as u16 + 1;
                let cursor_y = chunks[2].y + 1 + edit_state.selected as u16;
                f.set_cursor_position(ratatui::prelude::Position::new(cursor_x, cursor_y));
            
        }
    })?;

    Ok(())
}
    // --------------------
    // File Operations
    // --------------------

    fn rename_wallpaper(&mut self, old_path: &Path, new_name: &str) -> io::Result<PathBuf> {
        let parent_dir = old_path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?;

        let mut new_path = parent_dir.join(new_name);

        // Add file extension if missing
        if let Some(ext) = old_path.extension()
            && new_path.extension().is_none()
        {
            new_path.set_extension(ext);
        }

        // Check if new name already exists
        if new_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "File with that name already exists",
            ));
        }

        fs::rename(old_path, &new_path)?;

        // Update all references to the old path
        self.update_path_references(old_path, &new_path);

        Ok(new_path)
    }

    fn update_path_references(&mut self, old_path: &Path, new_path: &PathBuf) {
        // Update wallpapers list
        if let Some(pos) = self.wallpapers.iter().position(|p| p == old_path) {
            self.wallpapers[pos] = new_path.clone();
        }

        // Update history
        if let Some(pos) = self.history.iter().position(|p| p == old_path) {
            self.history[pos] = new_path.clone();
        }

        // Update favorites
        if let Some(pos) = self.favorites.iter().position(|p| p == old_path) {
            self.favorites[pos] = new_path.clone();
            save_list("favorites.txt", &self.favorites);
        }

        // Update image cache
        if let Some(image) = self.image_cache.cache.remove(old_path) {
            self.image_cache.cache.insert(new_path.clone(), image);
        }

        // Update last_preview if it was the renamed file
        if self.last_preview.as_ref() == Some(&PathBuf::from(old_path)) {
            self.last_preview = Some(new_path.clone());
        }
    }

    // --------------------
    // UI Rendering
    // --------------------

    fn draw_ui(&mut self, filtered: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
        // Handle config edit state first
        if self.config_edit_state.is_some() {
            // Clone the edit state to avoid borrowing issues
            let edit_state = self.config_edit_state.clone().unwrap();
            self.draw_config_editor(&edit_state)?;
            return Ok(());
        }
        let size = self.terminal.size()?;
        let area_rect = Rect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        };

        // Tabs
        let active_tabs = self.active_tabs();
        let tab_titles: Vec<String> = active_tabs.iter().map(|t| t.title()).collect();
        let selected_index = self.current_tab_index();

        let title = match self.current_tab {
            Tab::Wallpapers => {
                if self.in_search {
                    format!("Search: {} ", self.search_query)
                } else {
                    "Wallpapers".into()
                }
            }
            Tab::History => "History".into(),
            Tab::Favorites => "Favorites".into(),
            Tab::Config => "Configs".into(),
        };

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut name = p.file_name().unwrap().to_string_lossy().to_string();

                let extension = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ["mp4", "avi", "mov", "mkv"].contains(&extension.as_str()) {
                    name.push_str(" 🎥");
                }

                if self.favorites.contains(p) {
                    name.push_str(" ★");
                }
                if self.multi_select && self.selected_items.contains(&i) {
                    name = format!("[x] {}", name);
                }
                ListItem::new(name)
            })
            .collect();

        // Split screen vertically for tabs + main area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area_rect);

        // HORIZONTAL TABS LAYOUT - Split the tab area into equal columns
        let tab_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                active_tabs
                    .iter()
                    .map(|_| Constraint::Ratio(1, active_tabs.len() as u32))
                    .collect::<Vec<_>>(),
            )
            .split(chunks[0]);

        // Determine list and preview layout based on config
        let (list_area, preview_area) = match self.config.list_position.to_lowercase().as_str() {
            "right" => {
                let halves = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);
                (halves[1], halves[0])
            }
            "top" => {
                let halves = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);
                (halves[0], halves[1])
            }
            "bottom" => {
                let halves = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);
                (halves[1], halves[0])
            }
            _ => {
                // default "left"
                let halves = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);
                (halves[0], halves[1])
            }
        };

        // Update preview if selection changed
        if !filtered.is_empty() && Some(&filtered[self.selected]) != self.last_preview.as_ref() {
            let path = filtered[self.selected].clone();
            self.last_preview = Some(path.clone());
            self.request_preview(path);
        }

        // Compute scrollbar for list
        let total = filtered.len() as u16;
        let height = list_area.height;
        let scroll_ratio = (self.selected as f32 / total.max(1) as f32).min(1.0);
        let scroll_pos = (scroll_ratio * (height - 1) as f32).round() as u16;

        let rename_state = self.rename_state.as_ref();

        // Draw UI
        self.terminal.draw(|f| {
            // HORIZONTAL TABS - Render each tab in its own column
            for (i, tab_chunk) in tab_chunks.iter().enumerate() {
                let is_selected = selected_index == i;
                let tab_content = if is_selected {
                    format!("▶ {} ◀", tab_titles[i])
                } else {
                    tab_titles[i].clone()
                };

                let tab_block = Paragraph::new(tab_content)
                    .block(Block::default().borders(Borders::ALL))
                    .alignment(Alignment::Center)
                    .style(if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    });

                f.render_widget(tab_block, *tab_chunk);
            }

            // Scrollbar
            for y in 0..height {
                let symbol = if y == scroll_pos { "█" } else { "│" };
                let p = Paragraph::new(symbol)
                    .style(Style::default().fg(Color::Yellow))
                    .block(Block::default());
                f.render_widget(p, Rect::new(list_area.x, list_area.y + y, 1, 1));
            }

            // List
            let list = List::new(items)
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .style(Style::default()),
                )
                .highlight_style(Style::default().fg(Color::Yellow))
                .highlight_symbol(">> ");
            f.render_stateful_widget(
                list,
                Rect {
                    x: list_area.x + 1,
                    y: list_area.y,
                    width: list_area.width - 1,
                    height: list_area.height,
                },
                &mut self.list_state,
            );

            if let Some(state) = &mut self.preview_state {
                // For all layouts, center the preview
                let centered_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Min(0),                             // Left spacer
                        Constraint::Length(preview_area.width.min(80)), // Centered content
                        Constraint::Min(0),                             // Right spacer
                    ])
                    .split(preview_area);

                let vertical_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(0),                              // Top spacer
                        Constraint::Length(preview_area.height.min(40)), // Centered content
                        Constraint::Min(0),                              // Bottom spacer
                    ])
                    .split(centered_chunks[1]);

                let centered_area = vertical_chunks[1];

                // CLEAR the entire preview area first to avoid artifacts
                f.render_widget(Clear, preview_area);

                let widget = StatefulImage::new();
                f.render_stateful_widget(widget.resize(Resize::Fit(None)), centered_area, state);

                // Overlay video indicator if this is a video
                if let Some(current_path) = self.last_preview.as_ref() {
                    let extension = current_path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if ["mp4", "avi", "mov", "mkv"].contains(&extension.as_str()) {
                        let video_text = Paragraph::new("🎥 VIDEO")
                            .style(Style::default().fg(Color::Yellow).bg(Color::Black));
                        let overlay_area =
                            Rect::new(centered_area.x + 2, centered_area.y + 2, 10, 1);
                        f.render_widget(video_text, overlay_area);
                    }
                }
            } else if self.last_preview.is_some() {
                // Show centered loading indicator
                let loading_text = Paragraph::new("Loading preview...")
                    .style(Style::default().fg(Color::Gray))
                    .alignment(ratatui::layout::Alignment::Center);

                let centered_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Min(0),
                        Constraint::Length(20),
                        Constraint::Min(0),
                    ])
                    .split(preview_area);

                // CLEAR the loading area too
                f.render_widget(Clear, preview_area);
                f.render_widget(loading_text, centered_chunks[1]);
            } else {
                // CLEAR the preview area when there's no preview at all
                f.render_widget(Clear, preview_area);
            } // Draw rename dialog if active
            if let Some(rename_state) = rename_state {
                Self::draw_rename_dialog(f, area_rect, rename_state);
            }
        })?;

        Ok(())
    }
    fn draw_rename_dialog(f: &mut Frame, area: Rect, rename_state: &RenameState) {
        // Create a centered dialog area
        let width = 50;
        let height = 10;
        let x = (area.width - width) / 2;
        let y = (area.height - height) / 2;
        let dialog_area = Rect::new(x, y, width, height);

        // Dialog background
        let block = Block::default()
            .title(" Rename Wallpaper ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        f.render_widget(Clear, dialog_area);
        f.render_widget(block, dialog_area);

        // Content area inside the dialog
        let inner_area = dialog_area.inner(Margin::new(1, 1));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Original name
                Constraint::Length(3), // Input field
                Constraint::Length(1), // Error message
                Constraint::Min(1),    // Spacer
                Constraint::Length(1), // Instructions
            ])
            .split(inner_area);

        // Original file name
        let original_name = Text::raw(format!(
            "Original: {}",
            rename_state
                .original_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        f.render_widget(Paragraph::new(original_name), chunks[0]);

        // Input field
        let input = Paragraph::new(rename_state.current_input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("New Name"));
        f.render_widget(input, chunks[1]);

        // Error message
        if let Some(error) = &rename_state.error {
            let error_text = Text::styled(error, Style::default().fg(Color::Red));
            f.render_widget(Paragraph::new(error_text), chunks[2]);
        }

        // Instructions
        let instructions = Text::raw("Enter: Confirm | Esc: Cancel");
        f.render_widget(Paragraph::new(instructions), chunks[4]);

        // Set cursor position in input field
        f.set_cursor_position(ratatui::prelude::Position::new(
            chunks[1].x + rename_state.current_input.len() as u16 + 1,
            chunks[1].y + 1,
        ));
    }

    // --------------------
    // Cache management methods
    // --------------------

    fn preload_images(&mut self, paths: &[PathBuf]) {
        for path in paths.iter().take(self.image_cache.max_size) {
            if self.image_cache.get(path).is_none()
                && let Ok(cached_image) = CachedImage::new(path)
            {
                self.image_cache.insert(path.clone(), cached_image);
            }
        }
    }

    // --------------------
    // Event Handling
    // --------------------

    fn handle_event(
    &mut self,
    filtered: &[PathBuf],
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    self.dirty = true;

    let event = event::read()?;

    // Handle config editing mode first
    if self.config_edit_state.is_some() {
        // Take ownership temporarily
        if let Some(edit_state) = self.config_edit_state.take() {
            // Process the event and get the updated state
            let (result, updated_edit_state) = self.handle_config_edit_event(edit_state, &event);
            // Put the updated state back
            self.config_edit_state = updated_edit_state;
            return result;
        }
    }

    if self.rename_state.is_some() {
        // Take ownership temporarily to avoid borrowing conflicts
        if let Some(mut rename_state) = self.rename_state.take() {
            let result = self.handle_rename_event(&mut rename_state, &event);
            // Put it back
            self.rename_state = Some(rename_state);
            return result;
        }
    }

    // Normal event handling
    self.handle_normal_event(filtered, &event)
}
fn update_config_items_in_state(
    &mut self,
    edit_state: &mut ConfigEditState,
    category: ConfigCategory,
) {
    let all_items = self.create_config_items();
    
    let filtered_items: Vec<ConfigItem> = all_items
        .into_iter()
        .filter(|item| item.category == category)
        .collect();
    
    edit_state.items = filtered_items;
}
fn move_to_next_tab(&mut self) {
    let active_tabs = self.active_tabs();
    
    if let Some(current_pos) = active_tabs.iter().position(|&t| t == self.current_tab) {
        let next_pos = (current_pos + 1) % active_tabs.len();
        self.current_tab = active_tabs[next_pos];
        self.selected = 0;
        self.list_state.select(Some(0));
        self.selected_items.clear();
        self.multi_select = false;
        self.dirty = true;
    }
}

    fn handle_config_edit_event(
    &mut self,
    mut edit_state: ConfigEditState,
    event: &event::Event,
) -> (Result<Option<PathBuf>, Box<dyn std::error::Error>>, Option<ConfigEditState>) {
    match event {
        event::Event::Key(key) => {
                // Handle editing mode
                if edit_state.editing {
                    match key.code {
                        KeyCode::Enter => {
                            // Apply the change
                            let current_input = edit_state.current_input.clone();
                            let selected_index = edit_state.selected;

                            if let Some(item) = edit_state.items.get(selected_index).cloned() {
                                self.apply_config_change(&item, &current_input);
                            }

                            edit_state.editing = false;
                            edit_state.current_input.clear();

                            // Refresh items
                            let category = edit_state.categories[edit_state.selected_category].clone();
                            self.update_config_items_in_state(&mut edit_state, category);
                            self.dirty = true;
                        }
                        KeyCode::Esc => {
                            edit_state.editing = false;
                            edit_state.current_input.clear();
                            self.dirty = true;
                        }
                        KeyCode::Char(c) => {
                            edit_state.current_input.push(c);
                            self.dirty = true;
                        }
                        KeyCode::Backspace => {
                            edit_state.current_input.pop();
                            self.dirty = true;
                        }
                        _ => {}
                    }
                } else {
                    // Not in editing mode - handle navigation and actions
                    match key.code {
                        KeyCode::Char(c) => {
                            // Check for quit key
                            if c == edit_state.keybindings.quit {
                                self.dirty = true;
                                // Move to next tab after quitting config
                                self.move_to_next_tab();
                                return (Ok(None), None);
                            }

                            // Check for vim keys
                            match c {
                                'j' | 'J' => {
                                    // Move down
                                    if edit_state.selected < edit_state.items.len().saturating_sub(1) {
                                        edit_state.selected += 1;
                                    } else {
                                        edit_state.selected = 0;
                                    }
                                    self.dirty = true;
                                }
                                'k' | 'K' => {
                                    // Move up
                                    if edit_state.selected > 0 {
                                        edit_state.selected -= 1;
                                    } else {
                                        edit_state.selected = edit_state.items.len().saturating_sub(1);
                                    }
                                    self.dirty = true;
                                }
                                ' ' => {
                                    // Space bar - edit or toggle
                                    if let Some(item) = edit_state.items.get(edit_state.selected).cloned() {
                                        match item.field_type {
                                            ConfigFieldType::Boolean => {
                                                let new_value = if item.value.to_lowercase() == "true" {
                                                    "false"
                                                } else {
                                                    "true"
                                                };
                                                self.apply_config_change(&item, new_value);
                                                let category = edit_state.categories[edit_state.selected_category].clone();
                                                self.update_config_items_in_state(&mut edit_state, category);
                                                self.dirty = true;
                                            }
                                            _ => {
                                                edit_state.editing = true;
                                                edit_state.current_input = item.value;
                                                self.dirty = true;
                                            }
                                        }
                                    }
                                }
                                's' | 'S' if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    // Save configuration
                                    if let Err(e) = self.save_config_to_file() {
                                        eprintln!("Failed to save config: {}", e);
                                    }
                                    self.dirty = true;
                                    // Move to next tab after saving
                                    self.move_to_next_tab();
                                    return (Ok(None), None);
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Down => {
                            if edit_state.selected < edit_state.items.len().saturating_sub(1) {
                                edit_state.selected += 1;
                            } else {
                                edit_state.selected = 0;
                            }
                            self.dirty = true;
                        }
                        KeyCode::Up => {
                            if edit_state.selected > 0 {
                                edit_state.selected -= 1;
                            } else {
                                edit_state.selected = edit_state.items.len().saturating_sub(1);
                            }
                            self.dirty = true;
                        }
                        KeyCode::Tab => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                // Previous category with wrap
                                edit_state.selected_category = if edit_state.selected_category == 0 {
                                    edit_state.categories.len() - 1
                                } else {
                                    edit_state.selected_category - 1
                                };
                            } else {
                                // Next category with wrap
                                edit_state.selected_category = (edit_state.selected_category + 1) % edit_state.categories.len();
                            }
                            let category = edit_state.categories[edit_state.selected_category].clone();
                            self.update_config_items_in_state(&mut edit_state, category);
                            self.dirty = true;
                        }
                        KeyCode::Enter => {
                            if let Some(item) = edit_state.items.get(edit_state.selected).cloned() {
                                match item.field_type {
                                    ConfigFieldType::Boolean => {
                                        let new_value = if item.value.to_lowercase() == "true" {
                                            "false"
                                        } else {
                                            "true"
                                        };
                                        self.apply_config_change(&item, new_value);
                                        let category = edit_state.categories[edit_state.selected_category].clone();
                                        self.update_config_items_in_state(&mut edit_state, category);
                                        self.dirty = true;
                                    }
                                    _ => {
                                        edit_state.editing = true;
                                        edit_state.current_input = item.value;
                                        self.dirty = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Esc => {
                            self.dirty = true;
                            // Move to next tab after escaping config
                            self.move_to_next_tab();
                            return (Ok(None), None);
                        }
                        _ => {}
                    }
                }
            }
        event::Event::FocusGained => todo!(),
        event::Event::FocusLost => todo!(),
        event::Event::Mouse(_) => todo!(),
        event::Event::Paste(_) => todo!(),
        event::Event::Resize(_, _) => todo!(),
    }

    (Ok(None), Some(edit_state))
}    fn handle_rename_event(
        &mut self,
        rename_state: &mut RenameState,
        event: &event::Event,
    ) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        if let event::Event::Key(key) = event {
            match key.code {
                KeyCode::Enter => {
                    let new_name = rename_state.current_input.trim().to_string();
                    if new_name.is_empty() {
                        rename_state.error = Some("Name cannot be empty".to_string());
                        return Ok(None);
                    }

                    match self.rename_wallpaper(&rename_state.original_path, &new_name) {
                        Ok(new_path) => {
                            self.rename_state = None;

                            // Update preview if needed
                            if self.last_preview.as_ref() == Some(&rename_state.original_path) {
                                self.last_preview = Some(new_path.clone());
                                self.request_preview(new_path);
                            }
                        }
                        Err(e) => {
                            rename_state.error = Some(e.to_string());
                        }
                    }
                }
                KeyCode::Esc => {
                    self.rename_state = None;
                }
                KeyCode::Char(c) => {
                    rename_state.current_input.push(c);
                    rename_state.error = None;
                }
                KeyCode::Backspace => {
                    rename_state.current_input.pop();
                    rename_state.error = None;
                }
                _ => {}
            }
        }
        Ok(None)
    }

    // Helper method for normal events
    fn handle_normal_event(
        &mut self,
        filtered: &[PathBuf],
        event: &event::Event,
    ) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        match event {
            event::Event::Key(key) => {
                let active_tabs = self.active_tabs();
                let mut filtered_vec = filtered.to_vec();

                let mut input = Input {
                    current_tab: &mut self.current_tab,
                    in_search: &mut self.in_search,
                    search_query: &mut self.search_query,
                    selected: &mut self.selected,
                    list_state: &mut self.list_state,
                    filtered: &mut filtered_vec,
                    history: &mut self.history,
                    favorites: &mut self.favorites,
                    vim_motion: self.config.vim_motion,
                    mouse_support: self.config.mouse_support,
                    keybindings: &self.config.keybindings,
                    active_tabs: &active_tabs,
                };

                // Store previous tab to detect changes
                let previous_tab = *input.current_tab;

                if let Some(sel) = handle_input(
                    &mut input,
                    &mut self.multi_select,
                    &mut self.selected_items,
                    *key,
                ) {
                    match sel.to_str() {
                        Some("__rename__") => {
                            // Safe rename initialization
                            if !filtered.is_empty() && self.selected < filtered.len() {
                                self.rename_state = Some(RenameState {
                                    original_path: filtered[self.selected].clone(),
                                    current_input: String::new(),
                                    error: None,
                                });
                            }
                            return Ok(None);
                        }
                        Some("__config_edit__") => {
                            self.start_config_edit();
                            return Ok(None);
                        }
                        _ => return Ok(Some(sel)),
                    }
                }

                // Auto-open config editor when switching to Config tab
                if *input.current_tab == Tab::Config && previous_tab != Tab::Config {
                    self.start_config_edit();
                }
            }
            event::Event::Mouse(me) if self.config.mouse_support => {
                let previous_tab = self.current_tab;
                let mut mouse_input = MouseInput {
                    me: *me,
                    selected: &mut self.selected,
                    list_state: &mut self.list_state,
                    filtered,
                    list_area: &Rect::new(0, 3, 40, 20),
                    tabs_area: &Rect::new(0, 0, 80, 3),
                    current_tab: &mut self.current_tab,
                };
                handle_mouse(&mut mouse_input);

                // Auto-open config editor when switching to Config tab via mouse
                if self.current_tab == Tab::Config && previous_tab != Tab::Config {
                    self.start_config_edit();
                }
            }
            _ => {}
        }

        Ok(None)
    }
}

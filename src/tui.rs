use crate::config::Config as AppConfig;
use crate::input::{Input, handle_input};
use crate::mouse::{MouseInput, handle_mouse};
use crate::persistence::{load_list, save_list};
use crossterm::event::KeyCode;
use crossterm::event::{self, EnableMouseCapture};
use crossterm::execute;
use image::DynamicImage;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    text::Text,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use strum_macros::Display;
use tokio::sync::mpsc;

// ---------------------------
// Image Cache
// ---------------------------

#[derive(Clone)]
struct CachedImage {
    image: Arc<DynamicImage>,
}

impl CachedImage {
    fn new(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let image = image::ImageReader::open(path)?
            .with_guessed_format()?
            .decode()?;
        Ok(Self {
            image: Arc::new(image),
        })
    }
}

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

// ---------------------------
// Tab Enum
// ---------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum Tab {
    #[strum(serialize = "Wallpapers")]
    Wallpapers,
    #[strum(serialize = "History")]
    History,
    #[strum(serialize = "Favorites")]
    Favorites,
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

pub struct TuiApp<'a> {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    config: &'a AppConfig,
    wallpapers: Vec<PathBuf>,
    history: Vec<PathBuf>,
    favorites: Vec<PathBuf>,
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
}

impl<'a> TuiApp<'a> {
    pub fn new(
        wallpapers: &[PathBuf],
        config: &'a AppConfig,
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
    fn request_preview(&self, path: PathBuf) {
        let tx = self.preview_tx.clone();
        let path_clone = path.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || CachedImage::new(&path_clone))
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
        };

        // List items
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut name = p.file_name().unwrap().to_string_lossy().to_string();

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

        // Store rename_state in a local variable to avoid borrowing issues
        let rename_state = self.rename_state.as_ref();

        // Draw UI
        self.terminal.draw(|f| {
            // Tabs
            let tabs = Tabs::new(tab_titles.clone())
                .select(selected_index)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(tabs, chunks[0]);

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

            // Preview
            if let Some(state) = &mut self.preview_state {
                let widget = StatefulImage::new();
                f.render_stateful_widget(widget.resize(Resize::Fit(None)), preview_area, state);
            }

            // Draw rename dialog if active
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

        if self.rename_state.is_some() {
            match event {
                event::Event::Key(key) => {
                    match key.code {
                        KeyCode::Enter => {
                            let (original_path, new_name) = {
                                let rename_state = self.rename_state.as_mut().unwrap();
                                let new_name = rename_state.current_input.trim().to_string();
                                if new_name.is_empty() {
                                    rename_state.error = Some("Name cannot be empty".to_string());
                                    return Ok(None);
                                }
                                (rename_state.original_path.clone(), new_name)
                            };

                            match self.rename_wallpaper(&original_path, &new_name) {
                                Ok(new_path) => {
                                    self.rename_state = None;

                                    if self.last_preview.as_ref() == Some(&original_path) {
                                        self.last_preview = Some(new_path.clone());
                                        self.request_preview(new_path);
                                    } else {
                                        let current_filtered = self.filter_items();
                                        if let Some(current_selected) =
                                            current_filtered.get(self.selected)
                                            && current_selected == &new_path
                                        {
                                            self.last_preview = Some(new_path.clone());
                                            self.request_preview(new_path);
                                        }
                                    }

                                    return Ok(None);
                                }
                                Err(e) => {
                                    if let Some(rs) = self.rename_state.as_mut() {
                                        rs.error = Some(e.to_string());
                                    }
                                }
                            }
                        }
                        KeyCode::Esc => {
                            self.rename_state = None;
                            return Ok(None);
                        }
                        KeyCode::Char(c) => {
                            if let Some(rs) = self.rename_state.as_mut() {
                                rs.current_input.push(c);
                                rs.error = None;
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(rs) = self.rename_state.as_mut() {
                                rs.current_input.pop();
                                rs.error = None;
                            }
                        }
                        _ => {}
                    }
                    return Ok(None);
                }
                _ => {}
            }
        } else {
            match event {
                event::Event::Key(key) => {
                    let active_tabs = self.active_tabs();
                    let mut filtered_vec = filtered.to_vec();
                    let mut input = Input {
                        key: key.code,
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

                    if let Some(sel) =
                        handle_input(&mut input, &mut self.multi_select, &mut self.selected_items)
                    {
                        if sel == PathBuf::from("__rename__") {
                            if !filtered.is_empty() {
                                self.rename_state = Some(RenameState {
                                    original_path: filtered[self.selected].clone(),
                                    current_input: String::new(),
                                    error: None,
                                });
                            }
                            return Ok(None);
                        }
                        return Ok(Some(sel));
                    }
                }
                event::Event::Mouse(me) if self.config.mouse_support => {
                    let mut mouse_input = MouseInput {
                        me,
                        selected: &mut self.selected,
                        list_state: &mut self.list_state,
                        filtered,
                        list_area: &Rect::new(0, 3, 40, 20),
                        tabs_area: &Rect::new(0, 0, 80, 3),
                        current_tab: &mut self.current_tab,
                    };
                    handle_mouse(&mut mouse_input);
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

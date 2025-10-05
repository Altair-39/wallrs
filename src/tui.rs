use crate::config::Config as AppConfig;
use crate::input::{Input, handle_input};
use crate::mouse::{MouseInput, handle_mouse};
use crate::persistence::load_list;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use image::ImageReader;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use strum_macros::Display;

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
// TUI Entry Point
// ---------------------------

pub fn run_tui(
    wallpapers: &[PathBuf],
    config: &AppConfig,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut tui = TuiApp::new(wallpapers, config)?;
    tui.run()
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

    // Image rendering
    picker: Picker,
    preview_state: Option<StatefulProtocol>,
}

impl<'a> TuiApp<'a> {
    pub fn new(
        wallpapers: &[PathBuf],
        config: &'a AppConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if config.enable_mouse_support {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        enable_raw_mode()?;

        execute!(io::stdout(), EnterAlternateScreen)?;
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
            picker,
            preview_state: None,
        })
    }

    pub fn run(&mut self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        loop {
            let filtered = self.filter_items();
            self.adjust_selection(&filtered);
            self.draw_ui(&filtered)?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Some(selected) = self.handle_event(&filtered)? {
                    self.cleanup()?;
                    return Ok(selected);
                }
            }
        }
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
        } else if self.selected >= filtered.len() {
            self.selected = filtered.len() - 1;
            self.list_state.select(Some(self.selected));
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
                    format!("Search: {}", self.search_query)
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
            let img = ImageReader::open(&filtered[self.selected])?
                .with_guessed_format()?
                .decode()?;
            self.preview_state = Some(self.picker.new_resize_protocol(img));
            self.last_preview = Some(filtered[self.selected].clone());
        }

        // Compute scrollbar for list
        let total = filtered.len() as u16;
        let height = list_area.height;
        let scroll_ratio = (self.selected as f32 / total.max(1) as f32).min(1.0);
        let scroll_pos = (scroll_ratio * (height - 1) as f32).round() as u16;

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
        })?;

        Ok(())
    }
    // --------------------
    // Event Handling
    // --------------------

    fn handle_event(
        &mut self,
        filtered: &[PathBuf],
    ) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        match event::read()? {
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
                    enable_mouse_support: self.config.enable_mouse_support,
                    keybindings: &self.config.keybindings,
                    active_tabs: &active_tabs,
                };
                if let Some(sel) =
                    handle_input(&mut input, &mut self.multi_select, &mut self.selected_items)
                {
                    return Ok(Some(sel));
                }
            }
            event::Event::Mouse(me) if self.config.enable_mouse_support => {
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
        Ok(None)
    }

    // --------------------
    // Cleanup
    // --------------------

    fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.config.enable_mouse_support {
            execute!(io::stdout(), DisableMouseCapture).ok();
        }
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }
}

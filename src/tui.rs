use crate::config::Config as AppConfig;
use crate::input::{handle_input, Input};
use crate::mouse::{handle_mouse, MouseInput};
use crate::persistence::load_list;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, LeaveAlternateScreen};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Tabs},
    Terminal,
};
use std::io::{self, Write};
use std::path::PathBuf;
use std::str::FromStr;
use strum_macros::Display;
use viuer::Config as ViuerConfig;

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
        let s = s.trim().to_lowercase();
        match s.as_str() {
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

pub fn run_tui(
    wallpapers: &[PathBuf],
    config: &AppConfig,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut tui = TuiApp::new(wallpapers, config)?;
    tui.run()
}

// ---------------------------
// Core TUI Application Struct
// ---------------------------

struct TuiApp<'a> {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    config: &'a AppConfig,
    wallpapers: Vec<PathBuf>,
    history: Vec<PathBuf>,
    favorites: Vec<PathBuf>,
    selected: usize,
    list_state: ratatui::widgets::ListState,
    search_query: String,
    in_search: bool,
    current_tab: Tab,
    last_preview: Option<PathBuf>,
}

impl<'a> TuiApp<'a> {
    fn new(
        wallpapers: &[PathBuf],
        config: &'a AppConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if config.enable_mouse_support {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        enable_raw_mode()?;

        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // pick first enabled tab from config, fallback to Wallpapers
        let first_tab = config
            .tabs
            .iter()
            .find(|t| t.enabled)
            .map(|t| t.tab)
            .unwrap_or(Tab::Wallpapers);

        Ok(Self {
            terminal,
            config,
            wallpapers: wallpapers.to_vec(),
            history: load_list("history.txt"),
            favorites: load_list("favorites.txt"),
            selected: 0,
            list_state: {
                let mut s = ratatui::widgets::ListState::default();
                s.select(Some(0));
                s
            },
            search_query: String::new(),
            in_search: false,
            current_tab: first_tab,
            last_preview: None,
        })
    }

    fn run(&mut self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        loop {
            let filtered = self.filter_items();
            self.adjust_selection(&filtered);
            self.draw_ui(&filtered)?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Some(result) = self.handle_event(&filtered)? {
                    self.cleanup()?;
                    return Ok(result);
                }
            }
        }
    }

    // --------------------
    // Tab management helpers
    // --------------------

    fn default_tab_order() -> Vec<Tab> {
        vec![Tab::Wallpapers, Tab::History, Tab::Favorites]
    }

    fn active_tabs(&self) -> Vec<Tab> {
        if !self.config.tabs.is_empty() {
            let mut out: Vec<Tab> = self
                .config
                .tabs
                .iter()
                .filter(|t| t.enabled)
                .map(|t| t.tab)
                .collect();
            if out.is_empty() {
                return Self::default_tab_order();
            }
            out
        } else {
            Self::default_tab_order()
        }
    }

    fn current_tab_index(&self) -> usize {
        self.active_tabs()
            .iter()
            .position(|&t| t == self.current_tab)
            .unwrap_or(0)
    }

    fn next_tab(&mut self) {
        let active = self.active_tabs();
        if let Some(pos) = active.iter().position(|&t| t == self.current_tab) {
            self.current_tab = active.get(pos + 1).copied().unwrap_or(active[0]);
        }
    }

    fn previous_tab(&mut self) {
        let active = self.active_tabs();
        if let Some(pos) = active.iter().position(|&t| t == self.current_tab) {
            if pos == 0 {
                self.current_tab = *active.last().unwrap();
            } else {
                self.current_tab = active[pos - 1];
            }
        }
    }

    // --------------------
    // UI + State Management
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

    fn draw_ui(&mut self, filtered: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
        let size = self.terminal.size()?;
        let area = Rect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        };
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(vertical_chunks[1]);

        let list_area = chunks[0];
        let preview_area = chunks[1];

        // Precompute everything that borrows `self` so the closure only uses locals
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

        // clone favorites list locally so closure doesn't borrow self
        let favorites_clone = self.favorites.clone();
        // move a mutable reference to list_state into a local
        let list_state_ref = &mut self.list_state;

        let items: Vec<ListItem> = filtered
            .iter()
            .map(|p| {
                let mut name = p.file_name().unwrap().to_string_lossy().to_string();
                if favorites_clone.contains(p) {
                    name.push_str(" ★");
                }
                ListItem::new(name)
            })
            .collect();

        self.terminal.draw(|f| {
            let tabs = Tabs::new(tab_titles.clone())
                .select(selected_index)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(tabs, vertical_chunks[0]);

            let list = List::new(items.clone())
                .block(Block::default().title(title.clone()).borders(Borders::ALL))
                .highlight_style(Style::default().fg(Color::Yellow))
                .highlight_symbol(">> ");
            f.render_stateful_widget(list, list_area, list_state_ref);
        })?;

        self.update_preview(filtered, preview_area)?;
        Ok(())
    }

    fn update_preview(
        &mut self,
        filtered: &[PathBuf],
        area: Rect,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if filtered.is_empty() {
            return Ok(());
        }
        let path = &filtered[self.selected];
        if Some(path.clone()) == self.last_preview {
            return Ok(());
        }

        let mut stdout = io::stdout();
        for y in area.y..area.y + area.height {
            crossterm::queue!(stdout, crossterm::cursor::MoveTo(area.x, y))?;
            write!(stdout, "{}", " ".repeat(area.width as usize))?;
        }
        stdout.flush()?;

        let conf = ViuerConfig {
            x: area.x,
            y: area.y as i16,
            width: Some(area.width as u32 / 2),
            height: Some(area.height as u32),
            ..Default::default()
        };
        let _ = viuer::print_from_file(path, &conf);
        self.last_preview = Some(path.clone());
        Ok(())
    }

    fn handle_event(
        &mut self,
        filtered: &[PathBuf],
    ) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        match event::read()? {
            event::Event::Key(key) => {
                let active_tabs = self.active_tabs();
                let mut input = Input {
                    key: key.code,
                    current_tab: &mut self.current_tab,
                    in_search: &mut self.in_search,
                    search_query: &mut self.search_query,
                    selected: &mut self.selected,
                    list_state: &mut self.list_state,
                    filtered: &filtered,
                    history: &mut self.history,
                    favorites: &mut self.favorites,
                    vim_motion: self.config.vim_motion,
                    enable_mouse_support: self.config.enable_mouse_support,
                    keybindings: &self.config.keybindings,
                    active_tabs: &active_tabs,
                };

                Ok(handle_input(&mut input))
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
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.config.enable_mouse_support {
            execute!(io::stdout(), DisableMouseCapture).ok();
        }
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }
}

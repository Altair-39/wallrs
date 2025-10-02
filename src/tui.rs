use crossterm::event::{self, Event, KeyCode, MouseEvent, MouseEventKind};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Tabs},
};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use viuer::Config;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Wallpapers,
    History,
    Favorites,
}

pub fn run_tui(
    wallpapers: &[PathBuf],
    vim_motion: bool,
    enable_mouse_support: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if enable_mouse_support {
        execute!(io::stdout(), EnableMouseCapture)?;
    }
    enable_raw_mode()?;
    let mut terminal = {
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend)?
    };
    terminal.clear()?;

    // Load history + favorites
    let mut history = load_list("history.txt");
    let mut favorites = load_list("favorites.txt");

    let mut selected = 0;
    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(selected));

    let mut search_query = String::new();
    let mut in_search = false;
    let mut current_tab = Tab::Wallpapers;
    let mut last_preview: Option<PathBuf> = None;

    loop {
        // Filter items depending on tab
        let filtered: Vec<PathBuf> = match current_tab {
            Tab::Wallpapers => {
                if search_query.is_empty() {
                    wallpapers.to_vec()
                } else {
                    wallpapers
                        .iter()
                        .filter(|p| {
                            p.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&search_query.to_lowercase())
                        })
                        .cloned()
                        .collect()
                }
            }
            Tab::History => history.clone(),
            Tab::Favorites => favorites.clone(),
        };

        if filtered.is_empty() {
            selected = 0;
            list_state.select(None);
        } else if selected >= filtered.len() {
            selected = filtered.len() - 1;
            list_state.select(Some(selected));
        }

        // Precompute layout areas

        let size = terminal.size()?;
        let area = ratatui::layout::Rect {
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

        // Draw UI
        terminal.draw(|f| {
            // Tabs
            let tab_titles = ["Wallpapers", "History", "Favorites"]
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>();
            let tabs = Tabs::new(tab_titles)
                .select(match current_tab {
                    Tab::Wallpapers => 0,
                    Tab::History => 1,
                    Tab::Favorites => 2,
                })
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(tabs, vertical_chunks[0]);

            // Left list
            let title = match current_tab {
                Tab::Wallpapers => {
                    if in_search {
                        format!("Search: {}", search_query)
                    } else {
                        "Wallpapers".to_string()
                    }
                }
                Tab::History => "History".to_string(),
                Tab::Favorites => "Favorites".to_string(),
            };

            let items: Vec<ListItem> = filtered
                .iter()
                .map(|p| {
                    let mut name = p.file_name().unwrap().to_string_lossy().to_string();
                    if favorites.contains(p) {
                        name.push_str(" ★");
                    }
                    ListItem::new(name)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().title(title).borders(Borders::ALL))
                .highlight_style(Style::default().fg(Color::Yellow))
                .highlight_symbol(">> ");
            f.render_stateful_widget(list, list_area, &mut list_state);

            // Right preview
            if !filtered.is_empty() && Some(filtered[selected].clone()) != last_preview {
                let path = &filtered[selected];
                let mut stdout = io::stdout();

                for y in chunks[1].y..chunks[1].y + chunks[1].height {
                    crossterm::queue!(stdout, crossterm::cursor::MoveTo(chunks[1].x, y)).unwrap();
                    write!(stdout, "{}", " ".repeat(chunks[1].width as usize)).unwrap();
                }
                stdout.flush().unwrap();

                let conf = Config {
                    x: chunks[1].x,
                    y: chunks[1].y as i16,
                    width: Some(chunks[1].width as u32 / 2),
                    height: Some(chunks[1].height as u32),
                    ..Default::default()
                };
                let _ = viuer::print_from_file(path, &conf);
                last_preview = Some(path.clone());
            }
        })?;

        // Event handling
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(path) = handle_input(
                        key.code,
                        &mut current_tab,
                        &mut in_search,
                        &mut search_query,
                        &mut selected,
                        &mut list_state,
                        &filtered,
                        &mut history,
                        &mut favorites,
                        vim_motion,
                        enable_mouse_support,
                    ) {
                        if enable_mouse_support {
                            execute!(io::stdout(), DisableMouseCapture).ok();
                        }
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        return Ok(path);
                    }
                }
                Event::Mouse(me) if enable_mouse_support => {
                    handle_mouse(
                        me,
                        &mut selected,
                        &mut list_state,
                        &filtered,
                        &list_area,
                        &vertical_chunks[0],
                        &mut current_tab,
                    );
                }
                _ => {}
            }
        }
    }
}

// ------------------------
// Persistence helpers
// ------------------------
fn load_list(name: &str) -> Vec<PathBuf> {
    let path = dirs::home_dir().unwrap().join(".config/wallrs").join(name);
    if let Ok(data) = fs::read_to_string(path) {
        data.lines().map(PathBuf::from).collect()
    } else {
        Vec::new()
    }
}

fn save_list(name: &str, list: &[PathBuf]) {
    let path = dirs::home_dir().unwrap().join(".config/wallrs").join(name);
    let _ = fs::write(
        path,
        list.iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

// ------------------------
// Input handler
// ------------------------
fn handle_input(
    key: KeyCode,
    current_tab: &mut Tab,
    in_search: &mut bool,
    search_query: &mut String,
    selected: &mut usize,
    list_state: &mut ratatui::widgets::ListState,
    filtered: &[PathBuf],
    history: &mut Vec<PathBuf>,
    favorites: &mut Vec<PathBuf>,
    vim_motion: bool,
    enable_mouse_support: bool,
) -> Option<PathBuf> {
    match key {
        // Search
        KeyCode::Char('/') if *current_tab == Tab::Wallpapers && !*in_search => {
            *in_search = true;
            search_query.clear();
            *selected = 0;
            list_state.select(Some(*selected));
        }
        KeyCode::Esc if *in_search => *in_search = false,
        KeyCode::Char(c) if *in_search => {
            search_query.push(c);
            *selected = 0;
            list_state.select(Some(*selected));
        }
        KeyCode::Backspace if *in_search => {
            search_query.pop();
            *selected = 0;
            list_state.select(Some(*selected));
        }
        KeyCode::Enter if *in_search => *in_search = false,

        // Navigation
        KeyCode::Down | KeyCode::Char('j') if vim_motion => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
                list_state.select(Some(*selected));
            }
        }
        KeyCode::Up | KeyCode::Char('k') if vim_motion => {
            if *selected > 0 {
                *selected -= 1;
                list_state.select(Some(*selected));
            }
        }

        KeyCode::PageDown => {
            if *selected < filtered.len().saturating_sub(5) {
                *selected += 5;
                list_state.select(Some(*selected));
            }
        }
        KeyCode::PageUp => {
            if *selected > 5 {
                *selected -= 5;
                list_state.select(Some(*selected));
            }
        }

        // Switch tab
        KeyCode::Tab | KeyCode::Char('l') if vim_motion => {
            *current_tab = match current_tab {
                Tab::Wallpapers => Tab::History,
                Tab::History => Tab::Favorites,
                Tab::Favorites => Tab::Wallpapers,
            };
            *selected = 0;
            list_state.select(Some(*selected));
        }
        KeyCode::Char('h') if vim_motion => {
            *current_tab = match current_tab {
                Tab::Wallpapers => Tab::Favorites,
                Tab::History => Tab::Wallpapers,
                Tab::Favorites => Tab::History,
            };
            *selected = 0;
            list_state.select(Some(*selected));
        }

        // Toggle favorite
        KeyCode::Char('f') if !filtered.is_empty() => {
            let sel = filtered[*selected].clone();
            if favorites.contains(&sel) {
                favorites.retain(|p| p != &sel);
            } else {
                favorites.insert(0, sel.clone());
            }
            save_list("favorites.txt", favorites);
        }

        // Select item
        KeyCode::Enter if !*in_search && !filtered.is_empty() => {
            let sel = filtered[*selected].clone();

            if *current_tab == Tab::Wallpapers {
                history.retain(|p| p != &sel);
                history.insert(0, sel.clone());
                save_list("history.txt", history);
            }
            return Some(sel);
        }

        // Quit
        KeyCode::Esc if !*in_search => {
            if enable_mouse_support {
                execute!(io::stdout(), DisableMouseCapture).ok();
            }
            disable_raw_mode().unwrap();
            execute!(io::stdout(), LeaveAlternateScreen).unwrap();
            std::process::exit(0);
        }

        _ => {}
    }
    None
}

// ------------------------
// Mouse handler
// ------------------------
pub fn handle_mouse(
    me: MouseEvent,
    selected: &mut usize,
    list_state: &mut ratatui::widgets::ListState,
    filtered: &[PathBuf],
    list_area: &Rect,
    tabs_area: &Rect,
    current_tab: &mut Tab,
) {
    match me.kind {
        // Click inside the list
        MouseEventKind::Down(_) => {
            // List selection
            if me.column >= list_area.x
                && me.column < list_area.x + list_area.width
                && me.row >= list_area.y
                && me.row < list_area.y + list_area.height
            {
                let index = (me.row - list_area.y) as usize;
                if index < filtered.len() {
                    *selected = index;
                    list_state.select(Some(*selected));
                }
            }

            // Tab click
            if me.row >= tabs_area.y && me.row < tabs_area.y + tabs_area.height {
                let tab_width = tabs_area.width / 3;
                let tab_index = ((me.column - tabs_area.x) / tab_width) as usize;
                *current_tab = match tab_index {
                    0 => Tab::Wallpapers,
                    1 => Tab::History,
                    2 => Tab::Favorites,
                    _ => *current_tab,
                };
                *selected = 0;
                list_state.select(Some(*selected));
            }
        }

        // Scroll up/down
        MouseEventKind::ScrollUp => {
            if *selected > 0 {
                *selected -= 1;
                list_state.select(Some(*selected));
            }
        }
        MouseEventKind::ScrollDown => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
                list_state.select(Some(*selected));
            }
        }

        _ => {}
    }
}

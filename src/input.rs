use crate::config::CustomKeybindings;
use crate::persistence::save_list;
use crate::tui::Tab;
use crossterm::event::{DisableMouseCapture, KeyCode};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use ratatui::widgets::ListState;
use std::io;
use std::path::PathBuf;

pub struct Input<'a> {
    pub key: KeyCode,
    pub current_tab: &'a mut Tab,
    pub in_search: &'a mut bool,
    pub search_query: &'a mut String,
    pub selected: &'a mut usize,
    pub list_state: &'a mut ListState,
    pub filtered: &'a [PathBuf],
    pub history: &'a mut Vec<PathBuf>,
    pub favorites: &'a mut Vec<PathBuf>,
    pub vim_motion: bool,
    pub enable_mouse_support: bool,
    pub keybindings: &'a CustomKeybindings,
    pub active_tabs: &'a [Tab],
}

pub fn handle_input(input: &mut Input) -> Option<PathBuf> {
    let Input {
        key,
        current_tab,
        in_search,
        search_query,
        selected,
        list_state,
        filtered,
        history,
        favorites,
        vim_motion,
        enable_mouse_support,
        keybindings,
        active_tabs,
    } = input;

    // Dereference mutable references for convenience
    let current_tab = &mut **current_tab;
    let in_search = &mut **in_search;
    let selected = &mut **selected;

    match key {
        // Start search
        KeyCode::Char(c)
            if *c == keybindings.search && *current_tab == Tab::Wallpapers && !*in_search =>
        {
            *in_search = true;
            search_query.clear();
            *selected = 0;
            list_state.select(Some(*selected));
        }

        // Exit search
        KeyCode::Esc if *in_search => *in_search = false,
        KeyCode::Enter if *in_search => *in_search = false,

        // Input search query
        KeyCode::Char(c) if *in_search => {
            search_query.push(*c);
            *selected = 0;
            list_state.select(Some(*selected));
        }
        KeyCode::Backspace if *in_search => {
            search_query.pop();
            *selected = 0;
            list_state.select(Some(*selected));
        }

        // Navigation (vim motion)
        KeyCode::Down | KeyCode::Char('j') if *vim_motion => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
                list_state.select(Some(*selected));
            }
        }
        KeyCode::Up | KeyCode::Char('k') if *vim_motion => {
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

        // Tab switching (respect active_tabs)
        KeyCode::Tab | KeyCode::Char('l') if *vim_motion => {
            if let Some(pos) = active_tabs.iter().position(|t| *t == *current_tab) {
                let next = (pos + 1) % active_tabs.len();
                *current_tab = active_tabs[next];
                *selected = 0;
                list_state.select(Some(*selected));
            }
        }
        KeyCode::Char('h') if *vim_motion => {
            if let Some(pos) = active_tabs.iter().position(|t| *t == *current_tab) {
                let prev = if pos == 0 {
                    active_tabs.len() - 1
                } else {
                    pos - 1
                };
                *current_tab = active_tabs[prev];
                *selected = 0;
                list_state.select(Some(*selected));
            }
        }

        // Toggle favorite (custom keybinding)
        KeyCode::Char(c) if *c == keybindings.favorite && !filtered.is_empty() => {
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
            if *enable_mouse_support {
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

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
    pub mouse_support: bool,
    pub keybindings: &'a CustomKeybindings,
    pub active_tabs: &'a [Tab],
}

pub fn handle_input(
    input: &mut Input,
    multi_select: &mut bool,
    selected_items: &mut Vec<usize>,
) -> Option<PathBuf> {
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
        mouse_support,
        keybindings,
        active_tabs,
    } = input;

    let current_tab = &mut **current_tab;
    let in_search = &mut **in_search;
    let selected = &mut **selected;

    match key {
        // Toggle multi-select mode, only outside search
        KeyCode::Char(c) if *c == keybindings.multi_select && !*in_search => {
            *multi_select = !*multi_select;
            if !*multi_select {
                selected_items.clear();
            } else if !selected_items.contains(selected) {
                selected_items.push(*selected);
            }
        }

        // Tab switching
        KeyCode::Tab if !*in_search => {
            if let Some(pos) = active_tabs.iter().position(|&t| t == *current_tab) {
                *current_tab = active_tabs[(pos + 1) % active_tabs.len()];
                *selected = 0;
                list_state.select(Some(*selected));
                selected_items.clear();
                *multi_select = false;
            }
        }

        // Vim-style tab switching
        KeyCode::Char('l') if *vim_motion && !*in_search => {
            if let Some(pos) = active_tabs.iter().position(|&t| t == *current_tab) {
                *current_tab = active_tabs[(pos + 1) % active_tabs.len()];
                *selected = 0;
                list_state.select(Some(*selected));
                selected_items.clear();
                *multi_select = false;
            }
        }

        KeyCode::Char('h') if *vim_motion && !*in_search => {
            if let Some(pos) = active_tabs.iter().position(|&t| t == *current_tab) {
                let new_pos = if pos == 0 {
                    active_tabs.len() - 1
                } else {
                    pos - 1
                };
                *current_tab = active_tabs[new_pos];
                *selected = 0;
                list_state.select(Some(*selected));
                selected_items.clear();
                *multi_select = false;
            }
        }
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

        // Search input
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

        // Navigation
        KeyCode::Down => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            } else {
                *selected -= filtered.len().saturating_sub(1);
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            }
        }
        KeyCode::PageDown => {
            if *selected < filtered.len().saturating_sub(5) {
                *selected += 5;
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            } else {
                *selected -= filtered.len().saturating_sub(1);
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            }
        }
        KeyCode::Char('j') if *vim_motion => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            } else {
                *selected -= filtered.len().saturating_sub(1);
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            }
        }
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            } else {
                *selected += filtered.len();
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            }
        }
        KeyCode::PageUp => {
            if *selected > 4 {
                *selected -= 5;
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            } else {
                *selected += filtered.len();
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            }
        }
        KeyCode::Char('k') if *vim_motion => {
            if *selected > 0 {
                *selected -= 1;
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            } else {
                *selected += filtered.len();
                list_state.select(Some(*selected));
                if *multi_select && !selected_items.contains(selected) {
                    selected_items.push(*selected);
                }
            }
        }

        // Toggle favorite
        KeyCode::Char(c) if *c == keybindings.favorite && !filtered.is_empty() => {
            if *multi_select && !selected_items.is_empty() {
                for &i in selected_items.iter() {
                    let item = filtered[i].clone();
                    if favorites.contains(&item) {
                        favorites.retain(|p| p != &item);
                    } else {
                        favorites.insert(0, item);
                    }
                }
            } else {
                let item = filtered[*selected].clone();
                if favorites.contains(&item) {
                    favorites.retain(|p| p != &item);
                } else {
                    favorites.insert(0, item);
                }
            }
            save_list("favorites.txt", favorites);
        }
        KeyCode::Char(c)
            if *c == keybindings.rename
                && !filtered.is_empty()
                && !*in_search
                && *current_tab == Tab::Wallpapers =>
        {
            return Some(PathBuf::from("__rename__"));
        }

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
        KeyCode::Char(c) if *c == keybindings.quit && !filtered.is_empty() && !*in_search => {
            if *mouse_support {
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

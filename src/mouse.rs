use crate::tui::Tab;
use crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::path::PathBuf;

pub struct MouseInput<'a> {
    pub me: MouseEvent,
    pub selected: &'a mut usize,
    pub list_state: &'a mut ListState,
    pub filtered: &'a [PathBuf],
    pub list_area: &'a Rect,
    pub tabs_area: &'a Rect,
    pub current_tab: &'a mut Tab,
}

pub fn handle_mouse(input: &mut MouseInput) {
    let MouseInput {
        me,
        selected,
        list_state,
        filtered,
        list_area,
        tabs_area,
        current_tab,
    } = input;

    match me.kind {
        // Click inside the list
        crossterm::event::MouseEventKind::Down(_) => {
            // List selection
            if me.column >= list_area.x
                && me.column < list_area.x + list_area.width
                && me.row >= list_area.y
                && me.row < list_area.y + list_area.height
            {
                let index = (me.row - list_area.y) as usize;
                if index < filtered.len() {
                    **selected = index;
                    list_state.select(Some(**selected));
                }
            }

            // Tab click
            if me.row >= tabs_area.y && me.row < tabs_area.y + tabs_area.height {
                let tab_width = tabs_area.width / 3;
                let tab_index = ((me.column - tabs_area.x) / tab_width) as usize;
                **current_tab = match tab_index {
                    0 => Tab::Wallpapers,
                    1 => Tab::History,
                    2 => Tab::Favorites,
                    _ => **current_tab,
                };
                **selected = 0;
                list_state.select(Some(**selected));
            }
        }

        // Scroll up/down
        crossterm::event::MouseEventKind::ScrollUp => {
            if **selected > 0 {
                **selected -= 1;
                list_state.select(Some(**selected));
            }
        }
        crossterm::event::MouseEventKind::ScrollDown => {
            if **selected < filtered.len().saturating_sub(1) {
                **selected += 1;
                list_state.select(Some(**selected));
            }
        }

        _ => {}
    }
}

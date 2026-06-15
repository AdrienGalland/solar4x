use ratatui::{
    style::{Color, Style, Stylize},
    widgets::{Block, ListState, Paragraph},
};

use super::{ui::cycle_add, Direction2};

pub fn select_next_clamp(list_state: &mut ListState, max: usize) {
    list_state.select(match list_state.selected() {
        Some(i) if i == max => Some(i),
        Some(i) => Some(i + 1),
        None => Some(max),
    })
}

pub fn select_previous_clamp(list_state: &mut ListState, min: usize) {
    list_state.select(match list_state.selected() {
        Some(i) if i == min => Some(i),
        Some(i) => Some(i - 1),
        None => Some(min),
    })
}

pub trait ClampedList {
    fn list_state(&mut self) -> &mut ListState;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn select_next(&mut self) {
        let len = self.len();
        if len > 0 {
            select_next_clamp(self.list_state(), len - 1);
        }
    }

    fn select_previous(&mut self) {
        select_previous_clamp(self.list_state(), 0);
    }

    fn select_adjacent(&mut self, direction: Direction2) {
        match direction {
            Direction2::Up => self.select_previous(),
            Direction2::Down => self.select_next(),
        }
    }

    fn select_last(&mut self) {
        let len = self.len();
        self.list_state().select(len.checked_sub(1));
    }
}
pub trait OptionsList<const SIZE: usize> {
    fn current_index(&mut self) -> &mut usize;
    fn selected_field(&mut self) -> &mut String {
        let i = *self.current_index();
        self.nth(i)
    }
    fn nth(&mut self, n: usize) -> &mut String {
        self.fields_list()[n].0
    }

    fn nth_title(&mut self, n: usize) -> String {
        self.fields_list()[n].1.clone()
    }
    fn fields_list(&mut self) -> [(&mut String, String); SIZE];

    fn select_next(&mut self) {
        cycle_add(self.current_index(), SIZE, 1);
    }
    fn select_previous(&mut self) {
        cycle_add(self.current_index(), SIZE, -1);
    }
    fn paragraph(&mut self, i: usize) -> Paragraph {
        let selected = i == *self.current_index();
        let (fg, title_str) = if selected {
            (Color::Yellow, format!("▶ {}", self.nth_title(i)))
        } else {
            (Color::Reset, self.nth_title(i))
        };
        let style = Style::new().fg(fg);
        Paragraph::new(self.nth(i).clone())
            .style(style)
            .block(Block::bordered().border_style(style).title_top(title_str))
    }
}

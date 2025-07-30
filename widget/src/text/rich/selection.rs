use iced_widget::{graphics::text::Paragraph, text_input::Value};
use std::cmp::Ordering;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Direction {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Selection {
    pub start: SelectionEnd,
    pub end: SelectionEnd,
    pub direction: Direction,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SelectionEnd {
    pub line: usize,
    pub index: usize,
}

impl PartialOrd for SelectionEnd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SelectionEnd {
    fn cmp(&self, other: &Self) -> Ordering {
        self.line
            .cmp(&other.line)
            .then(self.index.cmp(&other.index))
    }
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn select_range(&mut self, start: SelectionEnd, end: SelectionEnd) {
        self.start = start.min(end);
        self.end = end.max(start);
    }

    pub(crate) fn change_selection(&mut self, new_end: SelectionEnd) {
        let (start, end) = if self.direction == Direction::Right {
            if new_end < self.start {
                self.direction = Direction::Left;
            }

            (self.start, new_end)
        } else {
            if new_end > self.end {
                self.direction = Direction::Right;
            }

            (new_end, self.end)
        };

        self.select_range(start, end);
    }

    pub(crate) fn select_word(&mut self, line: usize, index: usize, paragraph: &Paragraph) {
        let value = Value::new(paragraph.buffer().lines[line].text());

        let start = SelectionEnd {
            line,
            index: value.previous_start_of_word(index),
        };

        let end = SelectionEnd {
            line,
            index: value.next_end_of_word(index),
        };

        self.select_range(start, end);
    }

    pub(crate) fn select_left(&mut self, paragraph: &Paragraph) {
        let mut new_end = if self.direction == Direction::Right {
            self.end
        } else {
            self.start
        };

        if new_end.index > 0 {
            new_end.index -= 1;

            self.change_selection(new_end);
        } else if new_end.line > 0 {
            new_end.line -= 1;

            let value = Value::new(paragraph.buffer().lines[new_end.line].text());
            new_end.index = value.len();

            self.change_selection(new_end);
        }
    }

    pub(crate) fn select_right(&mut self, paragraph: &Paragraph) {
        let mut new_end = if self.direction == Direction::Right {
            self.end
        } else {
            self.start
        };

        let lines = &paragraph.buffer().lines;
        let value = Value::new(lines[new_end.line].text());

        if new_end.index < value.len() {
            new_end.index += 1;

            self.change_selection(new_end);
        } else if new_end.line < lines.len() - 1 {
            new_end.line += 1;
            new_end.index = 0;

            self.change_selection(new_end);
        }
    }

    pub(crate) fn select_left_by_words(&mut self, paragraph: &Paragraph) {
        let mut new_end = if self.direction == Direction::Right {
            self.end
        } else {
            self.start
        };

        if new_end.index == 1 {
            new_end.index = 0;

            self.change_selection(new_end);
        } else if new_end.index > 1 {
            let value = Value::new(paragraph.buffer().lines[new_end.line].text());
            new_end.index = value.previous_start_of_word(new_end.index);

            self.change_selection(new_end);
        } else if new_end.line > 0 {
            new_end.line -= 1;

            let value = Value::new(paragraph.buffer().lines[new_end.line].text());
            new_end.index = value.previous_start_of_word(value.len());

            self.change_selection(new_end);
        }
    }

    pub(crate) fn select_right_by_words(&mut self, paragraph: &Paragraph) {
        let mut new_end = if self.direction == Direction::Right {
            self.end
        } else {
            self.start
        };

        let lines = &paragraph.buffer().lines;
        let value = Value::new(lines[new_end.line].text());

        if value.len() - new_end.index == 1 {
            new_end.index = value.len();

            self.change_selection(new_end);
        } else if new_end.index < value.len() {
            new_end.index = value.next_end_of_word(new_end.index);

            self.change_selection(new_end);
        } else if new_end.line < lines.len() - 1 {
            new_end.line += 1;

            let value = Value::new(lines[new_end.line].text());
            new_end.index = value.next_end_of_word(0);

            self.change_selection(new_end);
        }
    }

    pub(crate) fn select_line_left(&mut self) {
        let mut start = self.start;

        if start.index > 0 {
            start.index = 0;

            self.select_range(start, self.end);
        }
    }

    pub(crate) fn select_line_right(&mut self, paragraph: &Paragraph) {
        let mut end = self.end;

        let value = Value::new(paragraph.buffer().lines[end.line].text());

        if end.index < value.len() {
            end.index = value.len();

            self.select_range(self.start, end);
        }
    }

    pub(crate) fn select_all(&mut self, paragraph: &Paragraph) {
        let line = paragraph.buffer().lines.len() - 1;
        let index = Value::new(paragraph.buffer().lines[line].text()).len();

        let end = SelectionEnd { line, index };

        self.select_range(SelectionEnd::default(), end);
    }
}

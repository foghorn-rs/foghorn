use iced_widget::{graphics::text::Paragraph, text_input::Value};
use std::cmp::Ordering;

#[cfg(windows)]
const LINE_ENDING: &str = "\r\n";
#[cfg(not(windows))]
const LINE_ENDING: &str = "\n";

/// The direction of a selection.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Direction {
    Left,
    #[default]
    Right,
}

/// A text selection.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Selection {
    /// The start of the selection.
    pub start: SelectionEnd,
    /// The end of the selection.
    pub end: SelectionEnd,
    /// The last direction of the selection.
    pub direction: Direction,
    moving_line_index: Option<usize>,
}

/// One of the ends of a [`Selection`].
///
/// Note that the index refers to [`graphemes`], not glyphs or bytes.
///
/// [`graphemes`]: unicode_segmentation::UnicodeSegmentation::graphemes
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SelectionEnd {
    pub line: usize,
    pub index: usize,
}

impl SelectionEnd {
    /// Creates a new [`SelectionEnd`].
    pub fn new(line: usize, index: usize) -> Self {
        Self { line, index }
    }
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
    /// Creates a new empty [`Selection`].
    pub fn new() -> Self {
        Self::default()
    }

    /// A selection is empty when the start and end are the same.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns the selected text from the given [`Paragraph`].
    ///
    /// Uses `\r\n` on Windows and `\n` on other systems as newline.
    pub fn text(&self, paragraph: &Paragraph) -> String {
        let Selection { start, end, .. } = *self;

        let mut value = String::new();
        let buffer_lines = &paragraph.buffer().lines;
        let lines_total = end.line - start.line + 1;

        for (idx, line) in buffer_lines.iter().enumerate().take(lines_total) {
            if idx == 0 {
                if lines_total == 1 {
                    value.push_str(&line.text()[start.index..end.index]);
                } else {
                    value.push_str(&line.text()[start.index..]);
                    value.push_str(LINE_ENDING);
                }
            } else if idx == lines_total - 1 {
                value.push_str(&line.text()[..end.index]);
            } else {
                value.push_str(line.text());
                value.push_str(LINE_ENDING);
            }
        }

        value
    }

    /// Returns the currently active [`SelectionEnd`].
    ///
    /// `self.end` if `self.direction` is [`Right`], `self.start` otherwise.
    ///
    /// [`Right`]: Direction::Right
    pub fn active_end(&self) -> SelectionEnd {
        if self.direction == Direction::Right {
            self.end
        } else {
            self.start
        }
    }

    /// Select a new range.
    ///
    /// `self.start` will be set to the smaller value, `self.end` to the larger.
    ///
    /// # Example
    ///
    /// ```
    /// use foghorn_widgets::text::rich::{Selection, SelectionEnd};
    ///
    /// let mut selection = Selection::default();
    ///
    /// let start = SelectionEnd::new(5, 17);
    /// let end = SelectionEnd::new(2, 8);
    ///
    /// selection.select_range(start, end);
    ///
    /// assert_eq!(selection.start, end);
    /// assert_eq!(selection.end, start);
    /// ```
    pub fn select_range(&mut self, start: SelectionEnd, end: SelectionEnd) {
        self.start = start.min(end);
        self.end = end.max(start);
    }

    /// Updates the current selection by setting a new end point.
    ///
    /// This method adjusts the selection range based on the provided `new_end` position. The
    /// current [`Direction`] is used to determine the new values:
    ///
    /// - If the current direction is [`Right`] (i.e., the selection goes from `start` to `end`), the
    ///   range becomes `(start, new_end)`. If `new_end` is before `start`, the direction is flipped to [`Left`].
    ///
    /// - If it's [`Left`], the range becomes `(new_end, end)`. If `new_end` is after `end`, the
    ///   direction is flipped to [`Right`].
    ///
    /// # Example
    ///
    /// ```
    /// use foghorn_widgets::text::rich::selection::{Direction, Selection, SelectionEnd};
    ///
    /// let mut selection = Selection::default();
    ///
    /// let start = SelectionEnd::new(5, 17);
    /// let end = SelectionEnd::new(2, 8);
    ///
    /// selection.select_range(start, end);
    ///
    /// assert_eq!(selection.start, end);
    /// assert_eq!(selection.end, start);
    /// assert_eq!(selection.direction, Direction::Right);
    ///
    /// let new_end = SelectionEnd::new(2, 2);
    ///
    /// selection.change_selection(new_end);
    ///
    /// assert_eq!(selection.start, new_end);
    /// assert_eq!(selection.end, end);
    /// assert_eq!(selection.direction, Direction::Left);
    /// ```
    ///
    /// [`Left`]: Direction::Left
    /// [`Right`]: Direction::Right
    pub fn change_selection(&mut self, new_end: SelectionEnd) {
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

        self.moving_line_index = None;
        self.select_range(start, end);
    }

    /// Selects the word around the given grapheme position.
    pub fn select_word(&mut self, line: usize, index: usize, paragraph: &Paragraph) {
        let value = Value::new(paragraph.buffer().lines[line].text());

        let start = SelectionEnd::new(line, value.previous_start_of_word(index));
        let end = SelectionEnd::new(line, value.next_end_of_word(index));

        self.select_range(start, end);
    }

    /// Moves the active [`SelectionEnd`] to the left by one, wrapping to the previous line if
    /// possible and required.
    pub fn select_left(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        if active_end.index > 0 {
            active_end.index -= 1;

            self.change_selection(active_end);
        } else if active_end.line > 0 {
            active_end.line -= 1;

            let value = Value::new(paragraph.buffer().lines[active_end.line].text());
            active_end.index = value.len();

            self.change_selection(active_end);
        }
    }

    /// Moves the active [`SelectionEnd`] to the right by one, wrapping to the next line if
    /// possible and required.
    pub fn select_right(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        let lines = &paragraph.buffer().lines;
        let value = Value::new(lines[active_end.line].text());

        if active_end.index < value.len() {
            active_end.index += 1;

            self.change_selection(active_end);
        } else if active_end.line < lines.len() - 1 {
            active_end.line += 1;
            active_end.index = 0;

            self.change_selection(active_end);
        }
    }

    /// Moves the active [`SelectionEnd`] up by one, keeping track of the original grapheme index.
    pub fn select_up(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        if active_end.line == 0 {
            active_end.index = 0;

            self.change_selection(active_end);
        } else {
            active_end.line -= 1;

            let mut moving_line_index = None;

            if let Some(index) = self.moving_line_index.take() {
                active_end.index = index;
            }

            let value = Value::new(paragraph.buffer().lines[active_end.line].text());
            if active_end.index > value.len() {
                moving_line_index = Some(active_end.index);
                active_end.index = value.len();
            }

            self.change_selection(active_end);
            self.moving_line_index = moving_line_index;
        }
    }

    /// Moves the active [`SelectionEnd`] down by one, keeping track of the original grapheme index.
    pub fn select_down(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        let lines = &paragraph.buffer().lines;
        let value = Value::new(lines[active_end.line].text());

        if active_end.line == lines.len() - 1 {
            active_end.index = value.len();

            self.change_selection(active_end);
        } else {
            active_end.line += 1;

            let mut moving_line_index = None;

            if let Some(index) = self.moving_line_index.take() {
                active_end.index = index;
            }

            let value = Value::new(paragraph.buffer().lines[active_end.line].text());
            if active_end.index > value.len() {
                moving_line_index = Some(active_end.index);
                active_end.index = value.len();
            }

            self.change_selection(active_end);
            self.moving_line_index = moving_line_index;
        }
    }

    /// Moves the active [`SelectionEnd`] to the previous start of a word on its current line, or
    /// the previous line if it exists and `index == 0`.
    pub fn select_left_by_words(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        if active_end.index == 1 {
            active_end.index = 0;

            self.change_selection(active_end);
        } else if active_end.index > 1 {
            let value = Value::new(paragraph.buffer().lines[active_end.line].text());
            active_end.index = value.previous_start_of_word(active_end.index);

            self.change_selection(active_end);
        } else if active_end.line > 0 {
            active_end.line -= 1;

            let value = Value::new(paragraph.buffer().lines[active_end.line].text());
            active_end.index = value.previous_start_of_word(value.len());

            self.change_selection(active_end);
        }
    }

    /// Moves the active [`SelectionEnd`] to the next end of a word on its current line, or
    /// the next line if it exists and `index == line.len()`.
    pub fn select_right_by_words(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        let lines = &paragraph.buffer().lines;
        let value = Value::new(lines[active_end.line].text());

        if value.len() - active_end.index == 1 {
            active_end.index = value.len();

            self.change_selection(active_end);
        } else if active_end.index < value.len() {
            active_end.index = value.next_end_of_word(active_end.index);

            self.change_selection(active_end);
        } else if active_end.line < lines.len() - 1 {
            active_end.line += 1;

            let value = Value::new(lines[active_end.line].text());
            active_end.index = value.next_end_of_word(0);

            self.change_selection(active_end);
        }
    }

    /// Moves the active [`SelectionEnd`] to the beginning of its current line.
    pub fn select_line_beginning(&mut self) {
        let mut active_end = self.active_end();

        if active_end.index > 0 {
            active_end.index = 0;

            self.change_selection(active_end);
        }
    }

    /// Moves the active [`SelectionEnd`] to the end of its current line.
    pub fn select_line_end(&mut self, paragraph: &Paragraph) {
        let mut active_end = self.active_end();

        let value = Value::new(paragraph.buffer().lines[active_end.line].text());

        if active_end.index < value.len() {
            active_end.index = value.len();

            self.change_selection(active_end);
        }
    }

    /// Moves the active [`SelectionEnd`] to the beginning of the [`Paragraph`].
    pub fn select_beginning(&mut self) {
        self.change_selection(SelectionEnd::new(0, 0));
    }

    /// Moves the active [`SelectionEnd`] to the end of the [`Paragraph`].
    pub fn select_end(&mut self, paragraph: &Paragraph) {
        let lines = &paragraph.buffer().lines;
        let value = Value::new(lines[lines.len() - 1].text());

        let new_end = SelectionEnd::new(lines.len() - 1, value.len());

        self.change_selection(new_end);
    }

    /// Selects an entire line.
    pub fn select_line(&mut self, line: usize, paragraph: &Paragraph) {
        let value = Value::new(paragraph.buffer().lines[line].text());

        let start = SelectionEnd::new(line, 0);
        let end = SelectionEnd::new(line, value.len());

        self.select_range(start, end);
    }

    /// Selects the entire [`Paragraph`].
    pub fn select_all(&mut self, paragraph: &Paragraph) {
        let line = paragraph.buffer().lines.len() - 1;
        let index = Value::new(paragraph.buffer().lines[line].text()).len();

        let end = SelectionEnd::new(line, index);

        self.select_range(SelectionEnd::new(0, 0), end);
    }
}

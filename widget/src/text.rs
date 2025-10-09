pub mod rich;
pub mod span;
use iced_widget::{
    Renderer,
    core::{
        self, Color, Element, Event, Font, Layout, Length, Pixels, Point, Renderer as _, Size,
        Theme, Widget, alignment, clipboard,
        keyboard::{self, key},
        layout,
        mouse::{self, click},
        renderer,
        text::{self, Paragraph as _, Renderer as _},
        touch,
        widget::{
            Operation,
            text::Format,
            tree::{self, Tree},
        },
    },
    graphics::text::Paragraph,
};
pub use rich::SignalRich;
use rich::selection::{Selection, SelectionEnd};
pub use span::SignalSpan;
pub use text::{Alignment, LineHeight, Shaping, Wrapping};

/// A bunch of text.
pub struct Text<'a, Theme = iced_widget::Theme>
where
    Theme: Catalog,
{
    fragment: text::Fragment<'a>,
    format: Format<Font>,
    class: Theme::Class<'a>,
}

impl<'a, Theme> Text<'a, Theme>
where
    Theme: Catalog,
{
    /// Create a new fragment of [`Text`] with the given contents.
    pub fn new(fragment: impl text::IntoFragment<'a>) -> Self {
        Text {
            fragment: fragment.into_fragment(),
            format: Format::default(),
            class: Theme::default(),
        }
    }

    /// Sets the size of the [`Text`].
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.format.size = Some(size.into());
        self
    }

    /// Sets the [`LineHeight`] of the [`Text`].
    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.format.line_height = line_height.into();
        self
    }

    /// Sets the [`Font`] of the [`Text`].
    ///
    /// [`Font`]: crate::text::Renderer::Font
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.format.font = Some(font.into());
        self
    }

    /// Sets the width of the [`Text`] boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.format.width = width.into();
        self
    }

    /// Sets the height of the [`Text`] boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.format.height = height.into();
        self
    }

    /// Centers the [`Text`], both horizontally and vertically.
    pub fn center(self) -> Self {
        self.align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
    }

    /// Sets the [`alignment::Horizontal`] of the [`Text`].
    pub fn align_x(mut self, alignment: impl Into<text::Alignment>) -> Self {
        self.format.align_x = alignment.into();
        self
    }

    /// Sets the [`alignment::Vertical`] of the [`Text`].
    pub fn align_y(mut self, alignment: impl Into<alignment::Vertical>) -> Self {
        self.format.align_y = alignment.into();
        self
    }

    /// Sets the [`Shaping`] strategy of the [`Text`].
    pub fn shaping(mut self, shaping: Shaping) -> Self {
        self.format.shaping = shaping;
        self
    }

    /// Sets the [`Wrapping`] strategy of the [`Text`].
    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.format.wrapping = wrapping;
        self
    }

    /// Sets the style of the [`Text`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the [`Text`].
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

/// The internal state of a [`Text`] widget.
#[derive(Debug, Default, Clone)]
pub struct State {
    paragraph: Paragraph,
    content: String,
    is_hovered: bool,
    selection: Selection,
    is_dragging: bool,
    last_click: Option<mouse::Click>,
    keyboard_modifiers: keyboard::Modifiers,
}

impl State {
    fn grapheme_line_and_index(&self, point: Point) -> Option<(usize, usize)> {
        let cursor = self.paragraph.buffer().hit(point.x, point.y)?;
        let value = self.paragraph.buffer().lines[cursor.line].text();

        Some((
            cursor.line,
            unicode_segmentation::UnicodeSegmentation::graphemes(
                &value[..cursor.index.min(value.len())],
                true,
            )
            .count(),
        ))
    }

    fn selection_end_points(&self) -> [Point; 2] {
        let Selection { start, end, .. } = self.selection;

        let start_position = self
            .paragraph
            .grapheme_position(start.line, start.index)
            .unwrap_or(Point::ORIGIN);

        let end_position = self
            .paragraph
            .grapheme_position(end.line, end.index)
            .unwrap_or(Point::ORIGIN);

        [start_position, end_position]
    }

    fn update(&mut self, text: text::Text<&str, Font>) {
        if self.content != text.content {
            text.content.clone_into(&mut self.content);
            self.paragraph = Paragraph::with_text(text);
            return;
        }

        match self.paragraph.compare(text.with_content(())) {
            text::Difference::None => {}
            text::Difference::Bounds => self.paragraph.resize(text.bounds),
            text::Difference::Shape => self.paragraph = Paragraph::with_text(text),
        }
    }
}

impl<Message, Theme> Widget<Message, Theme, Renderer> for Text<'_, Theme>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.format.width,
            height: self.format.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout(
            tree.state.downcast_mut::<State>(),
            renderer,
            limits,
            &self.fragment,
            self.format,
        )
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn core::Clipboard,
        shell: &mut core::Shell<'_, Message>,
        viewport: &core::Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        let bounds = layout.bounds();
        let click_position = cursor.position_over(bounds);

        if viewport.intersection(&bounds).is_none()
            && state.selection == Selection::default()
            && !state.is_dragging
        {
            return;
        }

        let was_hovered = state.is_hovered;
        let selection_before = state.selection;
        state.is_hovered = click_position.is_some();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if let Some(cursor_position) = click_position {
                    let target = cursor_position - core::Vector::new(bounds.x, bounds.y);

                    let click =
                        mouse::Click::new(cursor_position, mouse::Button::Left, state.last_click);

                    match click.kind() {
                        click::Kind::Single => {
                            let (line, index) = if target != Point::ORIGIN {
                                state.grapheme_line_and_index(target)
                            } else {
                                None
                            }
                            .unwrap_or((0, 0));

                            let new_end = SelectionEnd { line, index };

                            if state.keyboard_modifiers.shift() {
                                state.selection.change_selection(new_end);
                            } else {
                                state.selection.select_range(new_end, new_end);
                            }

                            state.is_dragging = true;
                        }
                        click::Kind::Double => {
                            let (line, index) =
                                state.grapheme_line_and_index(target).unwrap_or((0, 0));

                            state.selection.select_word(line, index, &state.paragraph);
                            state.is_dragging = false;
                        }
                        click::Kind::Triple => {
                            let (line, _) = state.grapheme_line_and_index(target).unwrap_or((0, 0));

                            state.selection.select_line(line, &state.paragraph);
                            state.is_dragging = false;
                        }
                    }

                    state.last_click = Some(click);

                    shell.capture_event();
                } else {
                    state.selection = Selection::default();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                state.is_dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { position })
            | Event::Touch(touch::Event::FingerMoved { position, .. }) => {
                if state.is_dragging {
                    let target = *position - core::Vector::new(bounds.x, bounds.y);
                    let (line, index) = state.grapheme_line_and_index(target).unwrap_or((0, 0));

                    let new_end = SelectionEnd { line, index };

                    state.selection.change_selection(new_end);
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key.as_ref() {
                keyboard::Key::Character("c")
                    if state.keyboard_modifiers.command() && !state.selection.is_empty() =>
                {
                    clipboard.write(
                        clipboard::Kind::Standard,
                        state.selection.text(&state.paragraph),
                    );

                    shell.capture_event();
                }
                keyboard::Key::Character("a")
                    if state.keyboard_modifiers.command()
                        && state.selection != Selection::default() =>
                {
                    state.selection.select_all(&state.paragraph);

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::Home)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    if state.keyboard_modifiers.jump() {
                        state.selection.select_beginning();
                    } else {
                        state.selection.select_line_beginning();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::End)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    if state.keyboard_modifiers.jump() {
                        state.selection.select_end(&state.paragraph);
                    } else {
                        state.selection.select_line_end(&state.paragraph);
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowLeft)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_line_beginning();
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_left_by_words(&state.paragraph);
                    } else {
                        state.selection.select_left(&state.paragraph);
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowRight)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_line_end(&state.paragraph);
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_right_by_words(&state.paragraph);
                    } else {
                        state.selection.select_right(&state.paragraph);
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowUp)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_beginning();
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_line_beginning();
                    } else {
                        state.selection.select_up(&state.paragraph);
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowDown)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_end(&state.paragraph);
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_line_end(&state.paragraph);
                    } else {
                        state.selection.select_down(&state.paragraph);
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::Escape) => {
                    state.is_dragging = false;
                    state.selection = Selection::default();

                    state.keyboard_modifiers = keyboard::Modifiers::default();

                    shell.capture_event();
                }
                _ => {}
            },
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
            }
            _ => {}
        }

        if state.is_hovered != was_hovered || state.selection != selection_before {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor_position: mouse::Cursor,
        viewport: &core::Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State>();
        let style = theme.style(&self.class);

        if !state.selection.is_empty() {
            let bounds = layout.bounds();

            let [start, end] = state
                .selection_end_points()
                .map(|pos| pos + core::Vector::new(bounds.x, bounds.y));

            let line_height = self
                .format
                .line_height
                .to_absolute(self.format.size.unwrap_or_else(|| renderer.default_size()))
                .0;

            let baseline_y = bounds.y + ((start.y - bounds.y) / line_height).floor() * line_height;

            // The correct code, uncomment when glyphs report a correct `y` value.
            //
            // let height = end.y - baseline_y - 0.5;
            // let rows = (height / line_height).ceil() as usize;
            //
            // Temporary solution
            let rows = state.selection.end.line - state.selection.start.line + 1;

            for row in 0..rows {
                let (x, width) = if row == 0 {
                    (
                        start.x,
                        if rows == 1 {
                            end.x.min(bounds.x + bounds.width) - start.x
                        } else {
                            bounds.x + bounds.width - start.x
                        },
                    )
                } else if row == rows - 1 {
                    (bounds.x, end.x - bounds.x)
                } else {
                    (bounds.x, bounds.width)
                };
                let y = baseline_y + row as f32 * line_height;

                renderer.fill_quad(
                    renderer::Quad {
                        bounds: core::Rectangle {
                            x,
                            y,
                            width,
                            height: line_height,
                        },
                        snap: true,
                        ..Default::default()
                    },
                    style.selection,
                );
            }
        }

        draw(
            renderer,
            defaults,
            layout.bounds(),
            &state.paragraph,
            style,
            viewport,
        );
    }

    fn operate(
        &mut self,
        _state: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &self.fragment);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &core::Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();

        if state.is_hovered {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Produces the [`layout::Node`] of a [`Text`] widget.
pub fn layout(
    state: &mut State,
    renderer: &Renderer,
    limits: &layout::Limits,
    content: &str,
    format: Format<Font>,
) -> layout::Node {
    layout::sized(limits, format.width, format.height, |limits| {
        let bounds = limits.max();

        let size = format.size.unwrap_or_else(|| renderer.default_size());
        let font = format.font.unwrap_or_else(|| renderer.default_font());

        state.update(text::Text {
            content,
            bounds,
            size,
            line_height: format.line_height,
            font,
            align_x: format.align_x,
            align_y: format.align_y,
            shaping: format.shaping,
            wrapping: format.wrapping,
        });

        state.paragraph.min_bounds()
    })
}

/// Draws text using the same logic as the [`Text`] widget.
pub fn draw<Renderer>(
    renderer: &mut Renderer,
    style: &renderer::Style,
    bounds: core::Rectangle,
    paragraph: &Renderer::Paragraph,
    appearance: Style,
    viewport: &core::Rectangle,
) where
    Renderer: text::Renderer,
{
    let anchor = bounds.anchor(
        paragraph.min_bounds(),
        paragraph.align_x(),
        paragraph.align_y(),
    );

    renderer.fill_paragraph(
        paragraph,
        anchor,
        appearance.color.unwrap_or(style.text_color),
        *viewport,
    );
}

impl<'a, Message, Theme> From<Text<'a, Theme>> for Element<'a, Message, Theme, Renderer>
where
    Theme: Catalog + 'a,
{
    fn from(text: Text<'a, Theme>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(text)
    }
}

impl<'a, Theme> From<&'a str> for Text<'a, Theme>
where
    Theme: Catalog + 'a,
{
    fn from(content: &'a str) -> Self {
        Self::new(content)
    }
}

/// The appearance of some text.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Style {
    /// The [`Color`] of the text.
    ///
    /// The default, `None`, means using the inherited color.
    pub color: Option<Color>,
    /// The [`Color`] of text selections.
    pub selection: Color,
}

/// The theme catalog of a [`Text`].
pub trait Catalog: Sized {
    /// The item class of this [`Catalog`].
    type Class<'a>;

    /// The default class produced by this [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, item: &Self::Class<'_>) -> Style;
}

/// A styling function for a [`Text`].
///
/// This is just a boxed closure: `Fn(&Theme, Status) -> Style`.
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

/// The default text styling; color is inherited.
pub fn default(theme: &Theme) -> Style {
    Style {
        color: None,
        selection: theme.extended_palette().primary.weak.color,
    }
}

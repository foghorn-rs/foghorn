pub mod selection;
use super::SignalSpan;
use iced_widget::{
    Renderer,
    core::{
        Clipboard, Color, Element, Event, Font, Layout, Length, Pixels, Point, Rectangle,
        Renderer as _, Shell, Size, Text, Theme, Vector, Widget, alignment, border, clipboard,
        keyboard::{self, key},
        layout,
        mouse::{self, click},
        renderer,
        renderer::Quad,
        text::{self, Paragraph as _, Renderer as _, Span},
        touch,
        widget::{
            text::{Alignment, LineHeight, Shaping, Wrapping},
            tree::{self, Tree},
        },
    },
    graphics::text::Paragraph,
};
pub use selection::{Selection, SelectionEnd};

/// A bunch of [`SignalRich`] text.
#[expect(missing_debug_implementations)]
pub struct SignalRich<'a, Link, Message> {
    spans: &'a [SignalSpan<'a, Link>],
    size: Option<Pixels>,
    line_height: LineHeight,
    width: Length,
    height: Length,
    font: Option<Font>,
    align_x: Alignment,
    align_y: alignment::Vertical,
    wrapping: Wrapping,
    style: StyleFn<'a, Theme>,
    hovered_link: Option<usize>,
    hovered_mention: Option<usize>,
    hovered_spoiler: Option<usize>,
    on_link_click: Option<Box<dyn Fn(Link) -> Message + 'a>>,
    on_mention_click: Option<Box<dyn Fn(String) -> Message + 'a>>,
}

impl<'a, Link, Message> SignalRich<'a, Link, Message>
where
    Link: Clone + 'static,
{
    /// Creates a new empty [`SignalRich`] text.
    pub fn new() -> Self {
        Self {
            spans: &[],
            size: None,
            line_height: LineHeight::default(),
            width: Length::Shrink,
            height: Length::Shrink,
            font: None,
            align_x: Alignment::Default,
            align_y: alignment::Vertical::Top,
            wrapping: Wrapping::default(),
            style: Box::new(default),
            hovered_link: None,
            hovered_mention: None,
            hovered_spoiler: None,
            on_link_click: None,
            on_mention_click: None,
        }
    }

    /// Sets the spans of the [`SignalRich`] text.
    pub fn with_spans(mut self, spans: impl Into<&'a [SignalSpan<'a, Link>]>) -> Self {
        self.spans = spans.into();
        self
    }

    #[expect(clippy::same_name_method)]
    /// Sets the default size of the [`SignalRich`] text.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets the default [`LineHeight`] of the [`SignalRich`] text.
    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    /// Sets the default font of the [`SignalRich`] text.
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the width of the [`SignalRich`] text boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`SignalRich`] text boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Centers the [`SignalRich`] text, both horizontally and vertically.
    pub fn center(self) -> Self {
        self.align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
    }

    /// Sets the [`alignment::Horizontal`] of the [`SignalRich`] text.
    pub fn align_x(mut self, alignment: impl Into<Alignment>) -> Self {
        self.align_x = alignment.into();
        self
    }

    /// Sets the [`alignment::Vertical`] of the [`SignalRich`] text.
    pub fn align_y(mut self, alignment: impl Into<alignment::Vertical>) -> Self {
        self.align_y = alignment.into();
        self
    }

    /// Sets the [`Wrapping`] strategy of the [`SignalRich`] text.
    pub fn wrapping(mut self, wrapping: impl Into<Wrapping>) -> Self {
        self.wrapping = wrapping.into();
        self
    }

    /// Sets the message that will be produced when a link of the [`SignalRich`] text
    /// is clicked.
    pub fn on_link_click(mut self, on_link_clicked: impl Fn(Link) -> Message + 'a) -> Self {
        self.on_link_click = Some(Box::new(on_link_clicked));
        self
    }

    /// Sets the message that will be produced when a mention of the [`SignalRich`] text
    /// is clicked.
    pub fn on_mention_click(mut self, on_mention_clicked: impl Fn(String) -> Message + 'a) -> Self {
        self.on_mention_click = Some(Box::new(on_mention_clicked));
        self
    }

    /// Sets the style of the [`SignalRich`] text.
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self {
        self.style = Box::new(style);
        self
    }
}

impl<Link, Message> Default for SignalRich<'_, Link, Message>
where
    Link: Clone + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

struct State<Link> {
    spans: Vec<SignalSpan<'static, Link>>,
    span_pressed: Option<usize>,
    revealed_spoilers: Vec<usize>,
    paragraph: Paragraph,
    selection: Selection,
    is_dragging: bool,
    last_click: Option<mouse::Click>,
    keyboard_modifiers: keyboard::Modifiers,
}

impl<Link: Clone> State<Link> {
    fn find_grapheme_line_and_index(&self, point: Point) -> Option<(usize, usize)> {
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
}

impl<Link, Message> Widget<Message, Theme, Renderer> for SignalRich<'_, Link, Message>
where
    Link: Clone + 'static,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Link>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Link> {
            spans: vec![],
            span_pressed: None,
            revealed_spoilers: vec![],
            paragraph: Paragraph::default(),
            selection: Selection::default(),
            is_dragging: false,
            last_click: None,
            keyboard_modifiers: keyboard::Modifiers::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout(
            tree.state.downcast_mut::<State<Link>>(),
            renderer,
            limits,
            self.width,
            self.height,
            self.spans,
            self.line_height,
            self.size,
            self.font,
            self.align_x,
            self.align_y,
            self.wrapping,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State<Link>>();

        let style = (self.style)(theme);

        let mut current_spoiler: Option<(usize, Rectangle)> = None;
        let draw_spoiler = |renderer: &mut Renderer, rectangle, spoiler_hovered| {
            renderer.fill_quad(
                Quad {
                    bounds: rectangle,
                    border: border::rounded(5),
                    ..Default::default()
                },
                if spoiler_hovered {
                    style.hovered_spoiler
                } else {
                    style.spoiler
                },
            );
        };

        for (index, span) in self.spans.iter().enumerate() {
            let link_hovered = self.on_link_click.is_some() && Some(index) == self.hovered_link;
            let mention_hovered =
                self.on_mention_click.is_some() && Some(index) == self.hovered_mention;
            let spoiler_hovered = span
                .spoiler_tag
                .is_some_and(|tag| Some(tag) == self.hovered_spoiler);
            let spoiler_revealed = span
                .spoiler_tag
                .is_some_and(|tag| state.revealed_spoilers.contains(&tag));

            if span.strikethrough() || span.spoiler() || span.mention() || link_hovered {
                let translation = layout.position() - Point::ORIGIN;
                let regions = state.paragraph.span_bounds(index);

                if span.spoiler() && !spoiler_revealed {
                    for bounds in &regions {
                        let bounds = bounds.shrink(2) + translation;

                        if let Some((tag, rectangle)) = current_spoiler.as_mut() {
                            if Some(*tag) == span.spoiler_tag && rectangle.y == bounds.y {
                                *rectangle = rectangle.union(&bounds);
                            } else {
                                draw_spoiler(renderer, *rectangle, spoiler_hovered);

                                current_spoiler = span.spoiler_tag.zip(Some(bounds));
                            }
                        } else {
                            current_spoiler = span.spoiler_tag.zip(Some(bounds));
                        }
                    }

                    if let Some((_, rectangle)) = current_spoiler.as_ref()
                        && self.spans.get(index + 1).is_none_or(|next_span| {
                            next_span
                                .spoiler_tag
                                .is_none_or(|tag| Some(tag) != span.spoiler_tag)
                        })
                    {
                        draw_spoiler(renderer, *rectangle, spoiler_hovered);

                        current_spoiler = None;
                    }
                }

                if span.mention() {
                    for bounds in &regions {
                        renderer.fill_quad(
                            Quad {
                                bounds: bounds.shrink([2, 0]) + translation,
                                border: border::rounded(5),
                                ..Default::default()
                            },
                            if mention_hovered {
                                style.hovered_mention
                            } else {
                                style.mention
                            },
                        );
                    }
                }

                if span.strikethrough() || link_hovered {
                    let size = self.size.unwrap_or_else(|| renderer.default_size());
                    let line_height = self.line_height.to_absolute(size);
                    let color = style.color.unwrap_or(defaults.text_color);
                    let baseline =
                        translation + Vector::new(0.0, size.0 + (line_height.0 - size.0) / 2.0);

                    if link_hovered {
                        for bounds in &regions {
                            renderer.fill_quad(
                                Quad {
                                    bounds: Rectangle::new(
                                        bounds.position() + baseline
                                            - Vector::new(0.0, size.0 * 0.08),
                                        Size::new(bounds.width, 1.0),
                                    ),
                                    ..Default::default()
                                },
                                color,
                            );
                        }
                    }

                    if span.strikethrough() {
                        for bounds in &regions {
                            renderer.fill_quad(
                                Quad {
                                    bounds: Rectangle::new(
                                        bounds.position() + baseline
                                            - Vector::new(0.0, size.0 / 2.0),
                                        Size::new(bounds.width, 1.0),
                                    ),
                                    ..Default::default()
                                },
                                color,
                            );
                        }
                    }
                }
            }
        }

        if !state.selection.is_empty() {
            let bounds = layout.bounds();

            let [start, end] = state
                .selection_end_points()
                .map(|pos| pos + Vector::new(bounds.x, bounds.y));

            let line_height = self
                .line_height
                .to_absolute(self.size.unwrap_or_else(|| renderer.default_size()))
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
                    Quad {
                        bounds: Rectangle {
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

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<Link>>();

        let bounds = layout.bounds();
        let click_position = cursor.position_over(bounds);

        if viewport.intersection(&bounds).is_none()
            && state.selection == Selection::default()
            && !state.is_dragging
        {
            return;
        }

        let link_was_hovered = self.hovered_link;
        let mention_was_hovered = self.hovered_mention;
        let spoiler_was_hovered = self.hovered_spoiler;

        self.hovered_link = None;
        self.hovered_mention = None;
        self.hovered_spoiler = None;

        if let Some(position) = click_position.map(|p| p - Vector::new(bounds.x, bounds.y))
            && let Some(index) = state.paragraph.hit_span(position)
            && let Some(span) = self.spans.get(index)
        {
            if span.spoiler()
                && span
                    .spoiler_tag
                    .is_some_and(|tag| !state.revealed_spoilers.contains(&tag))
            {
                self.hovered_spoiler = span.spoiler_tag;
            } else if span.link.is_some() {
                self.hovered_link = Some(index);
            } else if span.mention() {
                self.hovered_mention = Some(index);
            }
        }

        if link_was_hovered != self.hovered_link
            || spoiler_was_hovered != self.hovered_spoiler
            || mention_was_hovered != self.hovered_mention
        {
            shell.request_redraw();
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                let selection_before = state.selection;

                if self.hovered_link.is_some() {
                    state.span_pressed = self.hovered_link;
                    shell.capture_event();
                } else if self.hovered_mention.is_some() {
                    state.span_pressed = self.hovered_mention;
                    shell.capture_event();
                } else if self.hovered_spoiler.is_some() {
                    state.span_pressed = self.hovered_spoiler;
                    shell.capture_event();
                }

                if let Some(cursor_position) = click_position {
                    let target = cursor_position - Vector::new(bounds.x, bounds.y);

                    let click =
                        mouse::Click::new(cursor_position, mouse::Button::Left, state.last_click);

                    match click.kind() {
                        click::Kind::Single => {
                            let (line, index) = if target != Point::ORIGIN {
                                state.find_grapheme_line_and_index(target)
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
                                state.find_grapheme_line_and_index(target).unwrap_or((0, 0));

                            state.selection.select_word(line, index, &state.paragraph);
                            state.is_dragging = false;
                        }
                        click::Kind::Triple => {
                            let (line, _) =
                                state.find_grapheme_line_and_index(target).unwrap_or((0, 0));

                            state.selection.select_line(line, &state.paragraph);
                            state.is_dragging = false;
                        }
                    }

                    state.last_click = Some(click);

                    shell.capture_event();
                } else {
                    state.selection = Selection::default();
                }

                if selection_before != state.selection {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                state.is_dragging = false;

                if !matches!(event, Event::Touch(touch::Event::FingerLost { .. })) {
                    match state.span_pressed {
                        Some(span) if Some(span) == self.hovered_link => {
                            if let Some((link, on_link_clicked)) = self
                                .spans
                                .get(span)
                                .and_then(|span| span.link.clone())
                                .zip(self.on_link_click.as_deref())
                            {
                                shell.publish(on_link_clicked(link));
                            }
                        }
                        Some(span) if Some(span) == self.hovered_mention => {
                            if let Some((mention, on_mention_clicked)) = self
                                .spans
                                .get(span)
                                .map(|span| span.text.clone().into_owned())
                                .zip(self.on_mention_click.as_deref())
                            {
                                shell.publish(on_mention_clicked(mention));
                            }
                        }
                        Some(tag) if Some(tag) == self.hovered_spoiler => {
                            state.revealed_spoilers.push(tag);

                            refresh_spans(
                                state,
                                layout.bounds().size(),
                                self.spans,
                                self.line_height,
                                self.size.unwrap_or_else(|| renderer.default_size()),
                                self.font.unwrap_or_else(|| renderer.default_font()),
                                self.align_x,
                                self.align_y,
                                self.wrapping,
                            );

                            shell.request_redraw();
                        }
                        _ => {}
                    }

                    state.span_pressed = None;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position })
            | Event::Touch(touch::Event::FingerMoved { position, .. }) => {
                if state.is_dragging {
                    let target = *position - Vector::new(bounds.x, bounds.y);

                    let (line, index) =
                        state.find_grapheme_line_and_index(target).unwrap_or((0, 0));

                    let new_end = SelectionEnd { line, index };

                    let selection_before = state.selection;

                    state.selection.change_selection(new_end);

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }
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
                    let selection_before = state.selection;

                    state.selection.select_all(&state.paragraph);

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::Home)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    let selection_before = state.selection;

                    if state.keyboard_modifiers.jump() {
                        state.selection.select_beginning();
                    } else {
                        state.selection.select_line_beginning();
                    }

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::End)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    let selection_before = state.selection;

                    if state.keyboard_modifiers.jump() {
                        state.selection.select_end(&state.paragraph);
                    } else {
                        state.selection.select_line_end(&state.paragraph);
                    }

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowLeft)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    let selection_before = state.selection;

                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_line_beginning();
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_left_by_words(&state.paragraph);
                    } else {
                        state.selection.select_left(&state.paragraph);
                    }

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowRight)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    let selection_before = state.selection;

                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_line_end(&state.paragraph);
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_right_by_words(&state.paragraph);
                    } else {
                        state.selection.select_right(&state.paragraph);
                    }

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowUp)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    let selection_before = state.selection;

                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_beginning();
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_line_beginning();
                    } else {
                        state.selection.select_up(&state.paragraph);
                    }

                    if selection_before != state.selection {
                        shell.request_redraw();
                    }

                    shell.capture_event();
                }
                keyboard::Key::Named(key::Named::ArrowDown)
                    if state.keyboard_modifiers.shift()
                        && state.selection != Selection::default() =>
                {
                    let selection_before = state.selection;

                    if state.keyboard_modifiers.macos_command() {
                        state.selection.select_end(&state.paragraph);
                    } else if state.keyboard_modifiers.jump() {
                        state.selection.select_line_end(&state.paragraph);
                    } else {
                        state.selection.select_down(&state.paragraph);
                    }

                    if selection_before != state.selection {
                        shell.request_redraw();
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
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.hovered_link.is_some()
            || self.hovered_mention.is_some()
            || self.hovered_spoiler.is_some()
        {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

#[expect(clippy::too_many_arguments)]
fn layout<Link>(
    state: &mut State<Link>,
    renderer: &Renderer,
    limits: &layout::Limits,
    width: Length,
    height: Length,
    spans: &[SignalSpan<'_, Link>],
    line_height: LineHeight,
    size: Option<Pixels>,
    font: Option<Font>,
    align_x: Alignment,
    align_y: alignment::Vertical,
    wrapping: Wrapping,
) -> layout::Node
where
    Link: Clone,
{
    layout::sized(limits, width, height, |limits| {
        let bounds = limits.max();

        let size = size.unwrap_or_else(|| renderer.default_size());
        let font = font.unwrap_or_else(|| renderer.default_font());

        if state.spans == spans {
            match state.paragraph.compare(Text {
                content: (),
                bounds,
                size,
                line_height,
                font,
                align_x,
                align_y,
                shaping: Shaping::Advanced,
                wrapping,
            }) {
                text::Difference::None => {}
                text::Difference::Bounds => {
                    state.paragraph.resize(bounds);
                }
                text::Difference::Shape => {
                    refresh_spans(
                        state,
                        limits.max(),
                        spans,
                        line_height,
                        size,
                        font,
                        align_x,
                        align_y,
                        wrapping,
                    );
                }
            }
        } else {
            refresh_spans(
                state,
                limits.max(),
                spans,
                line_height,
                size,
                font,
                align_x,
                align_y,
                wrapping,
            );

            state.selection = Selection::default();
        }

        state.paragraph.min_bounds()
    })
}

#[expect(clippy::too_many_arguments)]
fn refresh_spans<Link>(
    state: &mut State<Link>,
    bounds: Size,
    spans: &[SignalSpan<'_, Link>],
    line_height: LineHeight,
    size: Pixels,
    font: Font,
    align_x: Alignment,
    align_y: alignment::Vertical,
    wrapping: Wrapping,
) where
    Link: Clone,
{
    let iced_spans: Vec<_> = spans
        .iter()
        .cloned()
        .map(|span| {
            let is_revealed = span
                .spoiler_tag
                .as_ref()
                .is_some_and(|tag| state.revealed_spoilers.contains(tag));

            let mut iced_span = Span::from(span);

            if is_revealed {
                iced_span.color = None;
            }

            iced_span
        })
        .collect();

    let text_with_spans = Text {
        content: iced_spans.as_slice(),
        bounds,
        size,
        line_height,
        font,
        align_x,
        align_y,
        shaping: Shaping::Advanced,
        wrapping,
    };

    state.paragraph = Paragraph::with_spans(text_with_spans);
    state.spans = spans.iter().cloned().map(SignalSpan::into_static).collect();
}

fn draw(
    renderer: &mut Renderer,
    style: &renderer::Style,
    bounds: Rectangle,
    paragraph: &Paragraph,
    appearance: Style,
    viewport: &Rectangle,
) {
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

impl<'a, Link, Message> From<SignalRich<'a, Link, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Link: Clone + 'static,
    Message: 'a,
{
    fn from(text: SignalRich<'a, Link, Message>) -> Self {
        Element::new(text)
    }
}

/// The appearance of some rich text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Style {
    /// The [`Color`] of the text.
    ///
    /// The default, `None`, means using the inherited color.
    pub color: Option<Color>,
    /// The [`Color`] of spoilers.
    pub spoiler: Color,
    /// The [`Color`] of hovered spoilers.
    pub hovered_spoiler: Color,
    /// The [`Color`] of mentions.
    pub mention: Color,
    /// The [`Color`] of hovered mentions.
    pub hovered_mention: Color,
    /// The [`Color`] of text selections.
    pub selection: Color,
}

/// A styling function for a [`SignalRich`].
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

pub fn default(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        color: None,
        spoiler: palette.background.weak.color,
        hovered_spoiler: palette.background.weakest.color,
        mention: palette.background.strong.color,
        hovered_mention: palette.background.strongest.color,
        selection: palette.primary.weak.color,
    }
}

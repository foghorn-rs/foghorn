use iced::{
    Background, Border, Color, Padding, Pixels,
    advanced::text::Highlight,
    widget::text::{Fragment, IntoFragment, LineHeight},
};
use std::borrow::Cow;

/// A span of text.
#[derive(Clone, Debug)]
pub struct Span<'a, Link = (), Font = iced::Font> {
    /// The [`Fragment`] of text.
    pub text: Fragment<'a>,
    /// The size of the [`Span`] in [`Pixels`].
    pub size: Option<Pixels>,
    /// The [`LineHeight`] of the [`Span`].
    pub line_height: Option<LineHeight>,
    /// The font of the [`Span`].
    pub font: Option<Font>,
    /// The [`Color`] of the [`Span`].
    pub color: Option<Color>,
    /// The link of the [`Span`].
    pub link: Option<Link>,
    /// The [`Highlight`] of the [`Span`].
    pub highlight: Option<Highlight>,
    /// The [`Padding`] of the [`Span`].
    ///
    /// Currently, it only affects the bounds of the [`Highlight`].
    pub padding: Padding,
    /// Whether the [`Span`] should be underlined or not.
    pub underline: bool,
    /// Whether the [`Span`] should be struck through or not.
    pub strikethrough: bool,
    /// Whether the [`Span`] contains a spoiler.
    pub spoiler: bool,
}

impl<'a, Link, Font> Span<'a, Link, Font> {
    /// Creates a new [`Span`] of text with the given text fragment.
    pub fn new(fragment: impl IntoFragment<'a>) -> Self {
        Self {
            text: fragment.into_fragment(),
            ..Self::default()
        }
    }

    /// Sets the size of the [`Span`].
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets the [`LineHeight`] of the [`Span`].
    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.line_height = Some(line_height.into());
        self
    }

    /// Sets the font of the [`Span`].
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the font of the [`Span`], if any.
    pub fn font_maybe(mut self, font: Option<impl Into<Font>>) -> Self {
        self.font = font.map(Into::into);
        self
    }

    /// Sets the [`Color`] of the [`Span`].
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the [`Color`] of the [`Span`], if any.
    pub fn color_maybe(mut self, color: Option<impl Into<Color>>) -> Self {
        self.color = color.map(Into::into);
        self
    }

    /// Sets the link of the [`Span`].
    pub fn link(mut self, link: impl Into<Link>) -> Self {
        self.link = Some(link.into());
        self
    }

    /// Sets the link of the [`Span`], if any.
    pub fn link_maybe(mut self, link: Option<impl Into<Link>>) -> Self {
        self.link = link.map(Into::into);
        self
    }

    /// Sets the [`Background`] of the [`Span`].
    pub fn background(self, background: impl Into<Background>) -> Self {
        self.background_maybe(Some(background))
    }

    /// Sets the [`Background`] of the [`Span`], if any.
    pub fn background_maybe(mut self, background: Option<impl Into<Background>>) -> Self {
        let Some(background) = background else {
            return self;
        };

        match &mut self.highlight {
            Some(highlight) => {
                highlight.background = background.into();
            }
            None => {
                self.highlight = Some(Highlight {
                    background: background.into(),
                    border: Border::default(),
                });
            }
        }

        self
    }

    /// Sets the [`Border`] of the [`Span`].
    pub fn border(self, border: impl Into<Border>) -> Self {
        self.border_maybe(Some(border))
    }

    /// Sets the [`Border`] of the [`Span`], if any.
    pub fn border_maybe(mut self, border: Option<impl Into<Border>>) -> Self {
        let Some(border) = border else {
            return self;
        };

        match &mut self.highlight {
            Some(highlight) => {
                highlight.border = border.into();
            }
            None => {
                self.highlight = Some(Highlight {
                    border: border.into(),
                    background: Background::Color(Color::TRANSPARENT),
                });
            }
        }

        self
    }

    /// Sets the [`Padding`] of the [`Span`].
    ///
    /// It only affects the [`background`] and [`border`] of the
    /// [`Span`], currently.
    ///
    /// [`background`]: Self::background
    /// [`border`]: Self::border
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets whether the [`Span`] should be underlined or not.
    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }

    /// Sets whether the [`Span`] should be struck through or not.
    pub fn strikethrough(mut self, strikethrough: bool) -> Self {
        self.strikethrough = strikethrough;
        self
    }

    /// Sets whether the [`Span`] contains a spoiler or not.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }

    #[expect(clippy::wrong_self_convention)]
    /// Turns the [`Span`] into a static one.
    pub fn to_static(self) -> Span<'static, Link, Font> {
        Span {
            text: Cow::Owned(self.text.into_owned()),
            size: self.size,
            line_height: self.line_height,
            font: self.font,
            color: self.color,
            link: self.link,
            highlight: self.highlight,
            padding: self.padding,
            underline: self.underline,
            strikethrough: self.strikethrough,
            spoiler: self.spoiler,
        }
    }
}

impl<Link, Font> Default for Span<'_, Link, Font> {
    fn default() -> Self {
        Self {
            text: Cow::default(),
            size: None,
            line_height: None,
            font: None,
            color: None,
            link: None,
            highlight: None,
            padding: Padding::default(),
            underline: false,
            strikethrough: false,
            spoiler: false,
        }
    }
}

impl<'a, Link, Font> From<&'a str> for Span<'a, Link, Font> {
    fn from(value: &'a str) -> Self {
        Span::new(value)
    }
}

impl<Link, Font: PartialEq> PartialEq for Span<'_, Link, Font> {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.size == other.size
            && self.line_height == other.line_height
            && self.font == other.font
            && self.color == other.color
    }
}

impl<'a, Link, Font> From<Span<'a, Link, Font>> for iced::widget::text::Span<'a, Link, Font> {
    fn from(value: Span<'a, Link, Font>) -> Self {
        iced::widget::text::Span {
            text: value.text,
            size: value.size,
            line_height: value.line_height,
            font: value.font,
            color: value.color,
            link: value.link,
            highlight: value.highlight,
            padding: value.padding,
            underline: value.underline,
            strikethrough: value.strikethrough,
        }
    }
}

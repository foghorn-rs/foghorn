use iced::{
    Color, Padding,
    widget::text::{Fragment, IntoFragment},
};
use std::borrow::Cow;

/// A span of text.
#[derive(Clone, Debug)]
pub struct Span<'a, Link = (), Font = iced::Font> {
    /// The [`Fragment`] of text.
    pub text: Fragment<'a>,
    /// The font of the [`Span`].
    pub font: Option<Font>,
    /// The link of the [`Span`].
    pub link: Option<Link>,
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

    /// Turns the [`Span`] into a static one.
    pub fn into_static(self) -> Span<'static, Link, Font> {
        Span {
            text: Cow::Owned(self.text.into_owned()),
            font: self.font,
            link: self.link,
            strikethrough: self.strikethrough,
            spoiler: self.spoiler,
        }
    }
}

impl<Link, Font> Default for Span<'_, Link, Font> {
    fn default() -> Self {
        Self {
            text: Cow::default(),
            font: None,
            link: None,
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
        self.text == other.text && self.font == other.font
    }
}

impl<'a, Link, Font> From<Span<'a, Link, Font>> for iced::widget::text::Span<'a, Link, Font> {
    fn from(value: Span<'a, Link, Font>) -> Self {
        iced::widget::text::Span {
            text: value.text,
            size: None,
            line_height: None,
            font: value.font,
            color: value.spoiler.then_some(Color::TRANSPARENT),
            link: value.link,
            highlight: None,
            padding: Padding::default(),
            underline: false,
            strikethrough: value.strikethrough,
        }
    }
}

use iced_widget::core::{
    Color, Font, font,
    font::{Family, Style, Weight},
    text::{self, Fragment, IntoFragment, Shaping},
};
use std::borrow::Cow;
use uuid::Uuid;

pub const MENTION: u8 = 1 << 0;
pub const BOLD: u8 = 1 << 1;
pub const ITALIC: u8 = 1 << 2;
pub const SPOILER: u8 = 1 << 3;
pub const STRIKETHROUGH: u8 = 1 << 4;
pub const MONOSPACE: u8 = 1 << 5;

/// A span of text.
#[derive(Clone, Debug, Eq, Hash)]
pub struct SignalSpan<'a, Link = ()> {
    /// The [`Fragment`] of text.
    pub text: Fragment<'a>,
    /// The flags of the [`SignalSpan`].
    pub flags: u8,
    /// The link of the [`SignalSpan`].
    pub link: Option<Link>,
    /// The mention of the [`SignalSpan`].
    pub mention: Option<Uuid>,
    /// Allows spoiler [`SignalSpan`]s to be rendered as one.
    pub spoiler_tag: Option<usize>,
}

impl<'a, Link> SignalSpan<'a, Link> {
    /// Creates a new [`SignalSpan`] of text with the given text fragment.
    pub fn new(fragment: impl IntoFragment<'a>) -> Self {
        Self {
            text: fragment.into_fragment(),
            ..Self::default()
        }
    }

    /// Sets the flags of the [`SignalSpan`].
    pub fn flags(mut self, flags: impl Into<u8>) -> Self {
        self.flags = flags.into();
        self
    }

    /// Sets the link of the [`SignalSpan`].
    pub fn link(mut self, link: impl Into<Link>) -> Self {
        self.link = Some(link.into());
        self
    }

    /// Sets the link of the [`SignalSpan`], if any.
    pub fn link_maybe(mut self, link: Option<impl Into<Link>>) -> Self {
        self.link = link.map(Into::into);
        self
    }

    /// Sets the mention of the [`SignalSpan`].
    pub fn set_mention(mut self, mention: impl Into<Uuid>) -> Self {
        self.mention = Some(mention.into());
        self
    }

    /// Sets the mention of the [`SignalSpan`], if any.
    pub fn set_mention_maybe(mut self, mention: Option<impl Into<Uuid>>) -> Self {
        self.mention = mention.map(Into::into);
        self
    }

    /// Sets the spoiler tag of the [`SignalSpan`].
    pub fn spoiler_tag(mut self, tag: usize) -> Self {
        self.spoiler_tag = Some(tag);
        self
    }

    /// Sets the spoiler tag of the [`SignalSpan`], if any.
    pub fn spoiler_tag_maybe(mut self, tag: Option<usize>) -> Self {
        self.spoiler_tag = tag;
        self
    }

    /// Gets whether the [`SignalSpan`] has the `mention` flag set.
    pub fn mention(&self) -> bool {
        self.flags & MENTION != 0
    }

    /// Gets whether the [`SignalSpan`] has the `bold` flag set.
    pub fn bold(&self) -> bool {
        self.flags & BOLD != 0
    }

    /// Gets whether the [`SignalSpan`] has the `italic` flag set.
    pub fn italic(&self) -> bool {
        self.flags & ITALIC != 0
    }

    /// Gets whether the [`SignalSpan`] has the `spoiler` flag set.
    pub fn spoiler(&self) -> bool {
        self.flags & SPOILER != 0
    }

    /// Gets whether the [`SignalSpan`] has the `strikethrough` flag set.
    pub fn strikethrough(&self) -> bool {
        self.flags & STRIKETHROUGH != 0
    }

    /// Gets whether the [`SignalSpan`] has the `monospace` flag set.
    pub fn monospace(&self) -> bool {
        self.flags & MONOSPACE != 0
    }

    /// Turns the [`SignalSpan`] into a static one.
    pub fn into_static(self) -> SignalSpan<'static, Link> {
        SignalSpan {
            text: Cow::Owned(self.text.into_owned()),
            flags: self.flags,
            link: self.link,
            mention: self.mention,
            spoiler_tag: self.spoiler_tag,
        }
    }

    /// Produces a [`Text`] widget from the given [`SignalSpan`].
    ///
    /// Only the [`monospace`], [`italic`] and [`bold`] styles are applied.
    ///
    /// [`Text`]: iced_widget::Text
    /// [`monospace`]: SignalSpan::monospace
    /// [`italic`]: SignalSpan::italic
    /// [`bold`]: SignalSpan::bold
    pub fn as_text_widget(&'a self) -> super::Text<'a> {
        let font = Font {
            family: if self.monospace() {
                Family::Monospace
            } else {
                Family::default()
            },
            weight: if self.bold() {
                Weight::Bold
            } else {
                Weight::Normal
            },
            style: if self.italic() {
                Style::Italic
            } else {
                Style::Normal
            },
            ..Font::DEFAULT
        };

        super::Text::new(&self.text)
            .font(font)
            .shaping(Shaping::Auto)
    }

    pub fn is_simple_text(&self) -> bool {
        !self.spoiler() && !self.mention() && !self.strikethrough() && self.link.is_none()
    }
}

impl<Link> Default for SignalSpan<'_, Link> {
    fn default() -> Self {
        Self {
            text: Cow::Borrowed(""),
            flags: 0,
            link: None,
            mention: None,
            spoiler_tag: None,
        }
    }
}

impl<'a, Link> From<&'a str> for SignalSpan<'a, Link> {
    fn from(value: &'a str) -> Self {
        SignalSpan::new(value)
    }
}

impl<Link> PartialEq for SignalSpan<'_, Link> {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.flags == other.flags
    }
}

impl<'a, Link> From<SignalSpan<'a, Link>> for text::Span<'a, Link, Font> {
    fn from(value: SignalSpan<'a, Link>) -> Self {
        text::Span {
            font: Some(Font {
                family: if value.monospace() {
                    Family::Monospace
                } else {
                    Family::SansSerif
                },
                weight: if value.bold() {
                    Weight::Bold
                } else {
                    Weight::Normal
                },
                style: if value.italic() {
                    font::Style::Italic
                } else {
                    font::Style::Normal
                },
                ..Font::default()
            }),
            color: value.spoiler().then_some(Color::TRANSPARENT),
            strikethrough: value.strikethrough(),
            text: value.text,
            link: value.link,
            ..text::Span::default()
        }
    }
}

use super::{Chat, Message};
use crate::widget::Rich;
use iced::{
    Alignment, Element, Fill, Shrink,
    border::{self, radius},
    widget::{column, container, horizontal_space, image, row, text, text::Wrapping},
};
use jiff::{Span, Timestamp, Unit, fmt::friendly::SpanPrinter, tz::TimeZone};

impl Chat {
    pub fn as_iced_widget<'a, M: 'a>(&'a self) -> Element<'a, M> {
        let name = match self {
            Self::Contact(contact) => &contact.name,
            Self::Group(group) => &group.title,
        };

        let avatar = match self {
            Self::Contact(contact) => &contact.avatar,
            Self::Group(group) => &group.avatar,
        };

        let content = [
            avatar
                .clone()
                .map(|handle| container(image(handle).height(50)).into()),
            Some(horizontal_space().into()),
            Some(text(name).into()),
        ];

        row(content.into_iter().flatten())
            .align_y(Alignment::Center)
            .height(Shrink)
            .into()
    }
}

impl Message {
    pub fn as_iced_widget<'a, M: 'a>(&'a self, now: Timestamp, tz: &TimeZone) -> Element<'a, M> {
        let timestamp = self.timestamp.to_zoned(tz.clone());
        let now = now.to_zoned(tz.clone());

        let timestamp = if timestamp.date() == now.date() {
            let diff = timestamp.since(&now).unwrap().round(Unit::Minute).unwrap();

            if diff.is_zero() {
                "now".to_owned()
            } else {
                SpanPrinter::new().span_to_string(&diff)
            }
        } else if timestamp.date() == now.date() - Span::new().days(1) {
            timestamp.strftime("yesterday at %H:%M").to_string()
        } else {
            timestamp.strftime("%d.%m.%Y at %H:%M").to_string()
        };

        let head = self.sender.name.clone() + ", " + &timestamp;

        let content = [
            Some(text(head).size(10).into()),
            self.body.as_ref().map(|body| {
                Rich::with_spans(body)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into()
            }),
        ];

        let content = column(content.into_iter().flatten());

        let content = container(content).max_width(650).padding(10).style(|t| {
            container::primary(t).border({
                border::rounded(if self.sender.is_self {
                    radius(15).top_right(5)
                } else {
                    radius(15).top_left(5)
                })
            })
        });

        let mut items = [
            self.sender.avatar.clone().map(|handle| {
                container(image(handle).height(50))
                    .height(Fill)
                    .align_y(Alignment::Start)
                    .into()
            }),
            Some(content.into()),
            Some(horizontal_space().into()),
        ];

        if self.sender.is_self {
            items.reverse();
        }

        row(items.into_iter().flatten())
            .align_y(Alignment::Center)
            .height(Shrink)
            .spacing(5)
            .into()
    }
}

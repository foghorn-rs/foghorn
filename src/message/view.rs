use super::{Chat, Message, Quote};
use crate::{app, icons::reply, widget::SignalRich};
use iced::{
    Alignment, Element, Fill, Shrink,
    border::{self, radius},
    widget::{button, column, container, image, row, space, text, text::Wrapping},
};
use jiff::{Span, Unit, Zoned, fmt::friendly::SpanPrinter, tz::TimeZone};
use std::sync::Arc;

impl Chat {
    pub fn as_iced_widget(&self) -> Element<'_, app::Message> {
        let name = match self {
            Self::Contact(contact) => {
                if contact.is_self {
                    "Note to Self"
                } else {
                    &contact.name
                }
            }
            Self::Group(group) => &group.title,
        };

        let avatar = match self {
            Self::Contact(contact) => &contact.avatar,
            Self::Group(group) => &group.avatar,
        };

        row![
            avatar.clone().map(|handle| image(handle).height(50)),
            space::horizontal(),
            text(name)
        ]
        .align_y(Alignment::Center)
        .height(Shrink)
        .into()
    }
}

impl Quote {
    pub fn as_iced_widget(&self, now: &Zoned, tz: &TimeZone) -> Element<'_, app::Message> {
        let timestamp = format_zoned(&self.timestamp.to_zoned(tz.clone()), now);

        let head = self
            .sender
            .as_ref()
            .map(|sender| sender.name.clone() + ", ")
            .unwrap_or_default()
            + &timestamp;

        let content = row![
            column![
                text(head).size(10),
                self.body.as_deref().map(|body| {
                    // TODO: add selectable Text widget
                    if body.len() == 1 && body[0].is_simple_text() {
                        Element::from(body[0].as_text_widget())
                    } else {
                        Element::from(
                            SignalRich::new()
                                .with_spans(body)
                                .wrapping(Wrapping::WordOrGlyph),
                        )
                    }
                })
            ],
            self.attachments
                .first()
                .and_then(|image| image.image.clone())
                .map(|handle| container(image(handle)).max_height(50)),
        ]
        .align_y(Alignment::Center)
        .spacing(5);

        container(content)
            .padding(10)
            .style(|t: &iced::Theme| {
                let pair = t.extended_palette().primary.weak;
                container::Style {
                    background: Some(pair.color.into()),
                    text_color: Some(pair.text),
                    border: border::rounded(5),
                    ..Default::default()
                }
            })
            .into()
    }
}

impl Message {
    pub fn as_iced_widget(
        self: &Arc<Self>,
        now: &Zoned,
        tz: &TimeZone,
    ) -> Element<'_, app::Message> {
        let timestamp = format_zoned(&self.timestamp.to_zoned(tz.clone()), now);

        let head = self.sender.name.clone() + ", " + &timestamp;

        let content = column![
            self.quote
                .as_ref()
                .map(|quote| quote.as_iced_widget(now, tz)),
            (!self.attachments.is_empty()).then_some(column(
                self.attachments
                    .iter()
                    .filter_map(|attachment| attachment.image.clone())
                    .map(|handle| image(handle).width(325).into()),
            )),
            column![
                text(head).size(10),
                self.body.as_deref().map(|body| {
                    // TODO: add selectable Text widget
                    if body.len() == 1 && body[0].is_simple_text() {
                        Element::from(body[0].as_text_widget())
                    } else {
                        Element::from(
                            SignalRich::new()
                                .with_spans(body)
                                .wrapping(Wrapping::WordOrGlyph)
                                .on_mention_click(app::Message::Mention),
                        )
                    }
                })
            ]
        ]
        .spacing(10);

        let content = container(content)
            .max_width(if self.attachments.is_empty() {
                650
            } else {
                335
            })
            .padding(10)
            .style(|t| {
                container::primary(t).border({
                    border::rounded(if self.sender.is_self {
                        radius(15).top_right(5)
                    } else {
                        radius(15).top_left(5)
                    })
                })
            })
            .into();

        let mut items = [
            self.sender
                .avatar
                .clone()
                .map(|handle| image(handle).height(50).into()),
            Some(content),
            Some(
                row![
                    button(reply())
                        .style(button::text)
                        .on_press(app::Message::Quote(Some(self.clone()))),
                ]
                .height(Fill)
                .align_y(Alignment::Center)
                .into(),
            ),
            Some(space::horizontal().into()),
        ];

        if self.sender.is_self {
            items.reverse();
        }

        row(items.into_iter().flatten())
            .align_y(Alignment::Start)
            .height(Shrink)
            .spacing(5)
            .into()
    }
}

fn format_zoned(timestamp: &Zoned, now: &Zoned) -> String {
    if timestamp.date() == now.date() {
        let diff = timestamp.since(now).unwrap().round(Unit::Minute).unwrap();

        if diff.is_zero() {
            "now".to_owned()
        } else {
            SpanPrinter::new().span_to_string(&diff)
        }
    } else if timestamp.date() == now.date() - Span::new().days(1) {
        timestamp.strftime("yesterday at %H:%M").to_string()
    } else {
        timestamp.strftime("%d.%m.%Y at %H:%M").to_string()
    }
}

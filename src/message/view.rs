use super::Message;
use iced::{
    Alignment, Element, Fill, Shrink,
    border::{self, radius},
    widget::{Column, Row, container, horizontal_space, image, text},
};
use jiff::{Span, Timestamp, fmt::friendly::SpanPrinter, tz::TimeZone};

impl Message {
    pub fn as_iced_widget<'a, M: 'a>(&'a self, now: Timestamp, tz: &TimeZone) -> Element<'a, M> {
        let timestamp = self.timestamp.to_zoned(tz.clone());
        let now = now.to_zoned(tz.clone());

        let timestamp = if timestamp.date() == now.date() {
            SpanPrinter::new().span_to_string(&timestamp.since(&now).unwrap())
        } else if timestamp.date() == now.date() - Span::new().days(1) {
            timestamp.strftime("yesterday at %H:%M").to_string()
        } else {
            timestamp.strftime("%d.%m.%Y at %H:%M").to_string()
        };

        let head = self.sender.name.clone() + ", " + &timestamp;

        let content = [
            Some(text(head).size(10).into()),
            self.body.as_deref().map(|body| text(body).into()),
        ];

        let content = content.into_iter().flatten().collect::<Column<'_, _>>();

        let content = container(content).max_width(500).padding(10).style(|t| {
            container::primary(t).border({
                border::rounded(if self.sender.is_me() {
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

        if self.sender.is_me() {
            items.reverse();
        }

        items
            .into_iter()
            .flatten()
            .collect::<Row<'_, _>>()
            .height(Shrink)
            .spacing(5)
            .align_y(Alignment::Center)
            .into()
    }
}

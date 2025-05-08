use super::Message;
use iced::{
    Alignment, Element, Fill, Shrink,
    border::{self, radius},
    widget::{Row, column, container, horizontal_space, image, text},
};
use jiff::{Timestamp, fmt::friendly::SpanPrinter, tz::TimeZone};

impl Message {
    pub fn as_iced_widget<'a, M: 'a>(&'a self, now: Timestamp, tz: &TimeZone) -> Element<'a, M> {
        let mut content = column![text(&self.sender.name).size(10)].spacing(2);

        if let Some(body) = self.body.as_deref() {
            content = content.push(text(body));
        }

        let timestamp = self.timestamp.to_zoned(tz.clone());
        let now = now.to_zoned(tz.clone());
        let diff = timestamp.since(&now).unwrap();

        let span = if timestamp.day() == now.day() {
            SpanPrinter::new().span_to_string(&diff)
        } else {
            timestamp.to_string()
        };

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
            Some(text(span).into()),
            Some(horizontal_space().into()),
        ];

        if self.sender.is_me() {
            items.reverse();
        }

        items
            .into_iter()
            .flatten()
            .collect::<Row<'a, _>>()
            .height(Shrink)
            .spacing(5)
            .align_y(Alignment::Center)
            .into()
    }
}

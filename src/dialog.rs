use crate::app::Message;
use iced::{
    Element, Font,
    widget::{center_x, column, qr_code, text},
};
use iced_dialog::button;
use std::borrow::Cow;

#[derive(Debug, Default)]
pub struct Dialog {
    is_open: bool,
    title: Cow<'static, str>,
    content: Cow<'static, str>,
    qr_code: Option<qr_code::Data>,
    message: Option<Message>,
    font: Font,
}

impl Dialog {
    pub fn new(
        title: impl Into<Cow<'static, str>>,
        content: impl Into<Cow<'static, str>>,
        qr_code: Option<qr_code::Data>,
        action: Option<Message>,
    ) -> Self {
        Self {
            is_open: true,
            title: title.into(),
            content: content.into(),
            qr_code,
            message: action,
            font: Font::DEFAULT,
        }
    }

    pub fn monospace(mut self) -> Self {
        self.font = Font::MONOSPACE;
        self
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn as_iced_dialog<'a>(
        &'a self,
        base: impl Into<Element<'a, Message>>,
    ) -> iced_dialog::Dialog<'a, Message> {
        let content = column![text(&*self.content).font(self.font)]
            .push_maybe(self.qr_code.as_ref().map(qr_code).map(center_x))
            .spacing(8);

        iced_dialog::Dialog::with_buttons(
            self.is_open,
            base,
            content,
            self.message
                .clone()
                .map_or(vec![], |m| vec![button("Close", m).into()]),
        )
        .title(&*self.title)
    }
}

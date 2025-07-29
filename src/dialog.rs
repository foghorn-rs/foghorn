use crate::app::Message;
use iced::{
    Element, Font,
    widget::{center_x, column, qr_code, text},
};
use iced_dialog::button;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default)]
pub enum Action {
    #[default]
    None,
    Close,
    RetryLinking,
}

impl From<Action> for Vec<Element<'_, Message>> {
    fn from(action: Action) -> Self {
        match action {
            Action::None => vec![],
            Action::Close => vec![button("Close", Message::CloseDialog).into()],
            Action::RetryLinking => vec![button("Retry Linking", Message::LinkSecondary).into()],
        }
    }
}

#[derive(Debug, Default)]
pub struct Dialog {
    is_open: bool,
    title: Cow<'static, str>,
    content: Cow<'static, str>,
    qr_code: Option<qr_code::Data>,
    action: Action,
    font: Font,
}

impl Dialog {
    pub fn new(
        title: impl Into<Cow<'static, str>>,
        content: impl Into<Cow<'static, str>>,
        qr_code: Option<qr_code::Data>,
        action: Action,
    ) -> Self {
        Self {
            is_open: true,
            title: title.into(),
            content: content.into(),
            qr_code,
            action,
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
        let content = column![
            text(&*self.content).font(self.font),
            self.qr_code.as_ref().map(qr_code).map(center_x)
        ]
        .spacing(8);

        iced_dialog::Dialog::with_buttons(self.is_open, base, content, self.action.into())
            .title(&*self.title)
            .font(self.font)
    }
}

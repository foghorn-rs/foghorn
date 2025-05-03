use crate::app::Message;
use iced::{
    Element,
    widget::{center_x, column, qr_code},
};
use iced_dialog::button;

#[derive(Clone, Copy, Debug, Default)]
pub enum Action {
    #[default]
    None,
    Close,
}

impl From<Action> for Vec<Element<'_, Message>> {
    fn from(action: Action) -> Self {
        match action {
            Action::None => vec![],
            Action::Close => vec![button("Close", Message::CloseDialog).into()],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Dialog {
    is_open: bool,
    title: &'static str,
    content: String,
    action: Action,
}

impl Dialog {
    pub fn new(title: &'static str, content: impl Into<String>, action: Action) -> Self {
        Self {
            is_open: true,
            title,
            content: content.into(),
            action,
        }
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn as_iced_dialog<'a>(
        &'a self,
        base: impl Into<Element<'a, Message>>,
        qr_code_data: Option<&'a qr_code::Data>,
    ) -> iced_dialog::Dialog<'a, Message> {
        let content = column![self.content.as_str()]
            .push_maybe(qr_code_data.map(|data| center_x(qr_code(data))))
            .spacing(8);

        iced_dialog::Dialog::with_buttons(self.is_open, base, content, self.action.into())
            .title(self.title)
    }
}

use crate::{
    dialog::{Action, Dialog},
    manager_manager::{ManagerError, ManagerManager},
    message,
    widget::vsplit::{self, VSplit},
};
use iced::{
    Element,
    Length::Fill,
    Subscription, Task,
    futures::channel::oneshot,
    keyboard, padding,
    time::every,
    widget::{
        button, column, container, horizontal_rule, horizontal_space, qr_code, scrollable, text,
        text_editor,
    },
};
use jiff::{Timestamp, tz::TimeZone};
use notify_rust::Notification;
use presage::libsignal_service::provisioning::ProvisioningError;
use std::{cmp::Reverse, collections::HashMap, mem::take, sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub enum Message {
    ManagerError(Option<Arc<ManagerError>>),
    QrCode(String),
    LinkSecondary,
    Received(Box<(message::Chat, message::Message)>),
    CloseDialog,
    Now(Timestamp),
    Tz(TimeZone),
    OpenChat(Box<message::Chat>),
    NextChat,
    PreviousChat,
    SplitAt(f32),
    ContentEdit(text_editor::Action),
    Send,
}

pub struct App {
    manager_manager: ManagerManager,
    dialog: Dialog,
    chats: HashMap<message::Chat, Vec<message::Message>>,
    now: Option<Timestamp>,
    tz: Option<TimeZone>,
    open_chat: Option<message::Chat>,
    message_content: text_editor::Content,
    split_at: f32,
}

impl App {
    pub fn create() -> (Self, Task<Message>) {
        let manager_manager = ManagerManager::default();
        let register = manager_manager.clone().load_registered();

        (
            Self {
                manager_manager,
                dialog: Dialog::default(),
                chats: HashMap::new(),
                now: None,
                tz: None,
                open_chat: None,
                message_content: text_editor::Content::new(),
                split_at: 270.0,
            },
            Task::batch([
                Task::perform(async { TimeZone::system() }, Message::Tz),
                Task::perform(async { Timestamp::now() }, Message::Now),
                Task::perform(register, |err| Message::ManagerError(err.map(Arc::new))),
            ]),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ManagerError(manager_error) => {
                if let Some(error) = manager_error {
                    return match &*error {
                        &ManagerError::NotYetRegisteredError
                        | &ManagerError::NoProvisioningMessageReceived
                        | &ManagerError::ProvisioningError(ProvisioningError::MissingMessage) => {
                            self.update(Message::LinkSecondary)
                        }
                        err => {
                            self.dialog = Dialog::new(
                                "Oops! Something went wrong.",
                                err.to_string(),
                                None,
                                Action::RetryLinking,
                            )
                            .monospace();
                            Task::none()
                        }
                    };
                }

                self.dialog.close();

                return Task::future(self.manager_manager.clone().stream_mesages())
                    .then(Task::stream)
                    .map(Box::new)
                    .map(Message::Received);
            }
            Message::LinkSecondary => {
                let (tx, rx) = oneshot::channel();

                return Task::batch([
                    Task::perform(self.manager_manager.clone().link_secondary(tx), |err| {
                        Message::ManagerError(err.map(Arc::new))
                    }),
                    Task::perform(rx, |url| Message::QrCode(url.unwrap())),
                ]);
            }
            Message::QrCode(url) => {
                self.dialog = Dialog::new(
                    "Link your device",
                    "Scan the QR code below to link your device.",
                    Some(qr_code::Data::new(url).unwrap()),
                    Action::None,
                );
            }
            Message::Received(message) => {
                let (chat, message) = *message;

                self.chats
                    .entry(chat)
                    .and_modify(|m| {
                        m.insert(
                            m.partition_point(|m| m.timestamp <= message.timestamp),
                            message.clone(),
                        );
                    })
                    .or_insert_with(|| vec![message.clone()]);

                if !message.sender.is_self && !message.is_from_store {
                    return Task::future(async move {
                        _ = Notification::new()
                            .summary(&message.sender.name)
                            .body(
                                message
                                    .body
                                    .map(|vec| {
                                        vec.iter()
                                            .map(|span| span.text.as_ref())
                                            .collect::<Vec<&str>>()
                                            .join("")
                                    })
                                    .unwrap_or_default()
                                    .as_str(),
                            )
                            .show_async()
                            .await;
                    })
                    .discard();
                }
            }
            Message::CloseDialog => self.dialog.close(),
            Message::OpenChat(open_chat) => self.open_chat = Some(*open_chat),
            Message::SplitAt(split_at) => self.split_at = split_at.clamp(170.0, 370.0),
            Message::Now(now) => self.now = Some(now),
            Message::Tz(tz) => self.tz = Some(tz),
            Message::NextChat | Message::PreviousChat => {
                let mut contacts = self.chats.keys().collect::<Vec<_>>();
                contacts.sort_by_key(|c| Reverse(self.chats[c].last().map(|c| c.timestamp)));

                if let Some(open_chat) = self.open_chat.as_ref() {
                    if let Some(index) = contacts.iter().position(|chat| chat == &open_chat) {
                        self.open_chat = if matches!(message, Message::NextChat) {
                            Some(contacts[(index + 1) % contacts.len()].clone())
                        } else if index == 0 {
                            Some((*contacts.last().expect("Contacts must not be empty")).clone())
                        } else {
                            Some(contacts[index - 1].clone())
                        }
                    }
                } else if !contacts.is_empty() {
                    self.open_chat = if matches!(message, Message::NextChat) {
                        Some(contacts[0].clone())
                    } else {
                        Some((*contacts.last().expect("Contacts must not be empty")).clone())
                    }
                }
            }
            Message::ContentEdit(action) => self.message_content.perform(action),
            Message::Send => {
                let content = take(&mut self.message_content).text();

                let manager_manager = self.manager_manager.clone();
                return Task::future(
                    manager_manager.send(content, self.open_chat.clone().unwrap()),
                )
                .and_then(Task::done)
                .map(Box::new)
                .map(Message::Received);
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut contacts = self.chats.keys().collect::<Vec<_>>();
        contacts.sort_by_key(|c| Reverse(self.chats[c].last().map(|c| c.timestamp)));
        let contacts = column![
            "Chats",
            horizontal_rule(11),
            scrollable(
                column(contacts.into_iter().map(|c| {
                    button(c.as_iced_widget())
                        .on_press(Message::OpenChat(c.clone().into()))
                        .padding(5)
                        .style(button::secondary)
                        .into()
                }))
                .spacing(5)
            )
            .spacing(5)
        ]
        .padding(padding::all(5).right(0));

        let chat = if let Some(((tz, now), open_chat)) =
            self.tz.as_ref().zip(self.now).zip(self.open_chat.as_ref())
        {
            let now = now.to_zoned(tz.clone());

            column![
                text(open_chat.name()),
                horizontal_rule(11),
                scrollable(
                    column(
                        self.chats[open_chat]
                            .iter()
                            .map(|chat| chat.as_iced_widget(&now, tz)),
                    )
                    .spacing(5),
                )
                .height(Fill)
                .anchor_bottom()
                .spacing(0),
                horizontal_rule(11),
                text_editor(&self.message_content)
                    .min_height(20)
                    .on_action(Message::ContentEdit)
                    .key_binding(|key_press| {
                        let modifiers = key_press.modifiers;
                        match text_editor::Binding::from_key_press(key_press) {
                            Some(text_editor::Binding::Enter) if !modifiers.shift() => {
                                Some(text_editor::Binding::Custom(Message::Send))
                            }
                            binding => binding,
                        }
                    }),
            ]
            .padding(padding::all(5).left(0))
            .into()
        } else {
            Element::new(horizontal_space())
        };

        let base = VSplit::new(contacts, chat, self.split_at, Message::SplitAt)
            .strategy(vsplit::Strategy::Left);

        let dialog = self
            .dialog
            .as_iced_dialog(container(base).width(Fill).height(Fill))
            .height(320);

        dialog.into()
    }

    #[expect(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            every(Duration::from_secs(1)).map(|_| Message::Now(Timestamp::now())),
            keyboard::on_key_press(|key, modifiers| match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Tab) if modifiers.command() => {
                    if modifiers.shift() {
                        Some(Message::PreviousChat)
                    } else {
                        Some(Message::NextChat)
                    }
                }
                _ => None,
            }),
        ])
    }
}

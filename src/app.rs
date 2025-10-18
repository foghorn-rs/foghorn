use crate::{
    dialog::{Action, Dialog},
    log::warn,
    manager_manager::{ManagerError, ManagerManager},
    message::{self, SignalAction},
};
use iced::{
    Element,
    Length::Fill,
    Subscription, Task,
    futures::channel::oneshot,
    keyboard, padding,
    time::every,
    widget::{button, column, container, qr_code, rule, scrollable, space, text, text_editor},
};
use iced_split::{Split, Strategy};
use jiff::{Timestamp, tz::TimeZone};
use notify_rust::Notification;
use presage::libsignal_service::{prelude::Uuid, provisioning::ProvisioningError};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    mem::take,
    sync::Arc,
    time::Duration,
};
use tokio::task::spawn_blocking;

#[derive(Clone, Debug)]
pub enum Message {
    ManagerError(Option<Arc<ManagerError>>),
    QrCode(String),
    LinkSecondary,
    Received((message::Chat, SignalAction)),
    CloseDialog,
    Now(Timestamp),
    Tz(TimeZone),
    OpenChat(message::Chat),
    NextChat,
    PreviousChat,
    Mention(Uuid),
    Quote(Option<Arc<message::Message>>),
    SplitAt(f32),
    ContentEdit(text_editor::Action),
    Send,
}

pub struct App {
    manager_manager: ManagerManager,
    dialog: Dialog,
    chats: HashMap<message::Chat, BTreeMap<Timestamp, Arc<message::Message>>>,
    now: Option<Timestamp>,
    tz: Option<TimeZone>,
    open_chat: Option<message::Chat>,
    message_content: text_editor::Content,
    quote: Option<message::Quote>,
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
                quote: None,
                split_at: 313.5,
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
            Message::Received((chat, message)) => match message {
                SignalAction::Contact => {
                    self.chats.entry(chat).or_insert_with(|| [].into());
                }
                SignalAction::Message(message, notif) => {
                    self.chats
                        .entry(chat)
                        .and_modify(|m| {
                            m.insert(message.timestamp, message.clone());
                        })
                        .or_insert_with(|| [(message.timestamp, message.clone())].into());

                    if notif {
                        return Task::future(async move {
                            let body = message
                                .body
                                .as_deref()
                                .map(|spans| {
                                    spans
                                        .iter()
                                        .map(|span| span.text.as_ref())
                                        .collect::<String>()
                                })
                                .unwrap_or_default();

                            if let Ok(Err(err)) = spawn_blocking(move || {
                                Notification::new()
                                    .summary(&message.sender.name)
                                    .body(&body)
                                    .show()
                            })
                            .await
                            {
                                warn!("{err}");
                            }
                        })
                        .discard();
                    }
                }
                SignalAction::Replace(old_ts, message) => {
                    self.chats.get_mut(&chat).unwrap().remove(&old_ts);
                    self.chats
                        .get_mut(&chat)
                        .unwrap()
                        .insert(message.timestamp, message);
                }
                SignalAction::Delete(timestamp) => {
                    self.chats.get_mut(&chat).unwrap().remove(&timestamp);
                }
            },
            Message::CloseDialog => self.dialog.close(),
            Message::OpenChat(open_chat) => {
                self.open_chat = Some(open_chat);
                self.message_content = text_editor::Content::new();
                self.quote = None;
            }
            Message::NextChat => {
                let mut contacts = self.chats.keys().collect::<Vec<_>>();
                contacts.sort_by_key(|c| Reverse(self.chats[c].last_key_value().map(|(k, _)| k)));

                let mut iter = contacts.iter().chain(contacts.iter());
                if let Some(open_chat) = self.open_chat.as_ref() {
                    iter.by_ref().find(|chat| **chat == open_chat);
                }

                if let Some(chat) = iter.next().copied().cloned() {
                    return self.update(Message::OpenChat(chat));
                }
            }
            Message::PreviousChat => {
                let mut contacts = self.chats.keys().collect::<Vec<_>>();
                contacts.sort_by_key(|c| Reverse(self.chats[c].last_key_value().map(|(k, _)| k)));

                let mut iter = contacts.iter().chain(contacts.iter());
                if let Some(open_chat) = self.open_chat.as_ref() {
                    iter.by_ref().rfind(|chat| **chat == open_chat);
                }

                if let Some(chat) = iter.next_back().copied().cloned() {
                    return self.update(Message::OpenChat(chat));
                }
            }
            Message::Mention(uuid) => {
                if let Some(chat) = self.chats.keys().find(|chat| chat.uuid() == Some(uuid)) {
                    return self.update(Message::OpenChat(chat.clone()));
                }
            }
            Message::Quote(quote) => self.quote = quote.as_deref().cloned().map(Into::into),
            Message::SplitAt(split_at) => self.split_at = split_at.clamp(153.0, 313.5),
            Message::Now(now) => self.now = Some(now),
            Message::Tz(tz) => self.tz = Some(tz),
            Message::ContentEdit(action) => self.message_content.perform(action),
            Message::Send => {
                let content = take(&mut self.message_content).text().trim().to_owned();

                let manager_manager = self.manager_manager.clone();
                return Task::future(manager_manager.send(
                    self.open_chat.clone().unwrap(),
                    content,
                    self.quote.take(),
                ))
                .and_then(Task::done)
                .map(Message::Received);
            }
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut contacts = self.chats.keys().collect::<Vec<_>>();
        contacts.sort_by_key(|c| Reverse(self.chats[c].last_key_value().map(|(k, _)| k)));
        let contacts = column![
            "Chats",
            rule::horizontal(1),
            scrollable(
                column(contacts.into_iter().map(|c| {
                    button(c.as_iced_widget())
                        .on_press(Message::OpenChat(c.clone()))
                        .padding(5)
                        .style(button::subtle)
                        .into()
                }))
                .spacing(5)
            )
            .spacing(5)
        ]
        .spacing(5)
        .padding(padding::all(5).right(0));

        let chat = if let Some(tz) = self.tz.as_ref()
            && let Some(now) = self.now
            && let Some(open_chat) = self.open_chat.as_ref()
        {
            let now = now.to_zoned(tz.clone());

            column![
                text(open_chat.name()),
                rule::horizontal(1),
                scrollable(
                    column(
                        self.chats[open_chat]
                            .values()
                            .map(|message| message.as_iced_widget(&now, tz)),
                    )
                    .spacing(5),
                )
                .height(Fill)
                .anchor_bottom()
                .spacing(5),
                self.quote
                    .as_ref()
                    .map(|quote| quote.as_iced_widget(&now, tz)),
                rule::horizontal(1),
                text_editor(&self.message_content)
                    .min_height(20)
                    .on_action(Message::ContentEdit)
                    .key_binding(|key_press| {
                        let modifiers = key_press.modifiers;
                        let binding = text_editor::Binding::from_key_press(key_press)?;

                        Some(match binding {
                            text_editor::Binding::Enter if !modifiers.shift() => {
                                text_editor::Binding::Custom(Message::Send)
                            }
                            text_editor::Binding::Backspace
                                if modifiers.command()
                                    && self.message_content.selection().is_none() =>
                            {
                                text_editor::Binding::Sequence(vec![
                                    text_editor::Binding::Select(text_editor::Motion::WordLeft),
                                    text_editor::Binding::Backspace,
                                ])
                            }
                            text_editor::Binding::Delete
                                if modifiers.command()
                                    && self.message_content.selection().is_none() =>
                            {
                                text_editor::Binding::Sequence(vec![
                                    text_editor::Binding::Select(text_editor::Motion::WordRight),
                                    text_editor::Binding::Delete,
                                ])
                            }
                            binding => binding,
                        })
                    }),
            ]
            .spacing(5)
            .padding(padding::all(5).left(0))
            .into()
        } else {
            Element::new(space::horizontal())
        };

        let base =
            Split::new(contacts, chat, self.split_at, Message::SplitAt).strategy(Strategy::Start);

        let dialog = self
            .dialog
            .as_iced_dialog(container(base).width(Fill).height(Fill))
            .max_height(320)
            .max_width(iced_dialog::dialog::DEFAULT_MAX_WIDTH);

        dialog.into()
    }

    #[expect(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            every(Duration::from_secs(1)).map(|_| Message::Now(Timestamp::now())),
            keyboard::on_key_press(|key, modifiers| match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Tab) if modifiers.command() => {
                    Some(if modifiers.shift() {
                        Message::PreviousChat
                    } else {
                        Message::NextChat
                    })
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::Quote(None)),
                _ => None,
            }),
        ])
    }
}

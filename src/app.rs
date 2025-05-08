use crate::{
    dialog::{Action, Dialog},
    manager_manager::{ManagerError, ManagerManager},
    message,
};
use iced::{
    Element,
    Length::Fill,
    Subscription, Task,
    futures::channel::oneshot,
    time::every,
    widget::{column, container, qr_code, scrollable},
};
use jiff::{Timestamp, Unit, tz::TimeZone};
use presage::libsignal_service::provisioning::ProvisioningError;
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub enum Message {
    Now(Timestamp),
    Tz(TimeZone),
    ManagerError(Option<Arc<ManagerError>>),
    QrCode(String),
    LinkSecondary,
    Received((message::Chat, message::Message)),
    CloseDialog,
}

pub struct App {
    now: Option<Timestamp>,
    tz: Option<TimeZone>,
    manager_manager: ManagerManager,
    registered: bool,
    dialog: Dialog,
    chats: HashMap<message::Chat, Vec<message::Message>>,
}

impl App {
    pub fn create() -> (Self, Task<Message>) {
        let manager_manager = ManagerManager::default();
        let register = manager_manager.clone().load_registered();

        (
            Self {
                now: None,
                tz: None,
                manager_manager,
                registered: false,
                dialog: Dialog::default(),
                chats: HashMap::new(),
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
            Message::Now(now) => self.now = Some(now.round(Unit::Minute).unwrap()),
            Message::Tz(tz) => self.tz = Some(tz),
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

                self.registered = true;
                self.dialog.close();

                return Task::future(self.manager_manager.clone().stream_mesages())
                    .then(Task::stream)
                    .map(Message::Received);
            }
            Message::LinkSecondary => {
                if self.registered {
                    self.dialog.close();
                } else {
                    let (tx, rx) = oneshot::channel();

                    return Task::batch([
                        Task::perform(self.manager_manager.clone().link_secondary(tx), |err| {
                            Message::ManagerError(err.map(Arc::new))
                        }),
                        Task::perform(rx, |url| Message::QrCode(url.unwrap())),
                    ]);
                }
            }
            Message::QrCode(url) => {
                self.dialog = Dialog::new(
                    "Link your device",
                    "Scan the QR code below to link your device.",
                    Some(qr_code::Data::new(url).unwrap()),
                    Action::None,
                );
            }
            Message::Received((chat, message)) => {
                self.chats
                    .entry(chat)
                    .and_modify(|m| {
                        m.insert(
                            m.partition_point(|m| m.timestamp < message.timestamp),
                            message.clone(),
                        );
                    })
                    .or_insert_with(|| vec![message]);
            }
            Message::CloseDialog => self.dialog.close(),
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = if self.registered {
            if let Some((tz, now)) = self.tz.as_ref().zip(self.now) {
                scrollable(
                    column(self.chats.values().map(|c| {
                        column(c.iter().map(|m| m.as_iced_widget(now, tz)))
                            .spacing(5)
                            .into()
                    }))
                    .padding(5)
                    .spacing(5),
                )
                .into()
            } else {
                "registered".into()
            }
        } else {
            "not registered".into()
        };

        let dialog: iced_dialog::Dialog<'_, Message> = self
            .dialog
            .as_iced_dialog(container(base).width(Fill).height(Fill))
            .height(320);

        dialog.into()
    }

    #[expect(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<Message> {
        every(Duration::from_secs(60)).map(|_| Message::Now(Timestamp::now()))
    }
}

use crate::{
    dialog::Dialog,
    manager_manager::{ManagerError, ManagerManager},
    message,
};
use iced::{
    Element,
    Length::Fill,
    Task,
    futures::channel::oneshot,
    widget::{container, qr_code},
};
use presage::libsignal_service::provisioning::ProvisioningError;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug)]
pub enum Message {
    ManagerError(Option<Arc<ManagerError>>),
    QrCode(String),
    LinkSecondary,
    Received((message::Chat, message::Message)),
}

pub struct App {
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
                manager_manager,
                registered: false,
                dialog: Dialog::default(),
                chats: HashMap::new(),
            },
            Task::perform(register, |err| Message::ManagerError(err.map(Arc::new))),
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
                                Some(Message::LinkSecondary),
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
                    None,
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
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = if self.registered {
            "registered".into()
        } else {
            "not registered".into()
        };

        let dialog: iced_dialog::Dialog<'_, Message> = self
            .dialog
            .as_iced_dialog(container(base).width(Fill).height(Fill))
            .height(320);

        dialog.into()
    }
}

use crate::{
    dialog::{Action, Dialog},
    manager_manager::{ManagerError, ManagerManager},
};
use iced::{
    Element,
    Length::Fill,
    Task,
    futures::channel::oneshot,
    widget::{container, qr_code},
};
use presage::{libsignal_service::provisioning::ProvisioningError, model::messages::Received};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Message {
    ManagerError(Option<Arc<ManagerError>>),
    QrCode(String),
    LinkSecondary,
    OpenDialog(Dialog),
    CloseDialog,
    Received(Arc<Received>),
}

pub struct App {
    manager_manager: ManagerManager,
    manager_error: Option<Arc<ManagerError>>,
    registered: bool,
    qr_code: Option<qr_code::Data>,
    dialog: Dialog,
}

impl App {
    pub fn create() -> (Self, Task<Message>) {
        let manager_manager = ManagerManager::default();
        let register = manager_manager.clone().load_registered();

        (
            Self {
                manager_manager,
                manager_error: None,
                registered: false,
                qr_code: None,
                dialog: Dialog::default(),
            },
            Task::perform(register, |err| Message::ManagerError(err.map(Arc::new))),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ManagerError(manager_error) => {
                self.manager_error = manager_error;
                self.qr_code = None;

                if let Some(error) = &self.manager_error {
                    return self.update(match &**error {
                        &ManagerError::NotYetRegisteredError
                        | &ManagerError::NoProvisioningMessageReceived
                        | &ManagerError::ProvisioningError(ProvisioningError::MissingMessage) => {
                            Message::LinkSecondary
                        }
                        err => Message::OpenDialog(Dialog::new(
                            "Oops! Something went wrong.",
                            err.to_string(),
                            Action::Close,
                        )),
                    });
                }

                self.registered = true;
                self.dialog.close();

                return Task::future(self.manager_manager.clone().stream_mesages())
                    .then(Task::stream)
                    .map(|r| Message::Received(Arc::new(r)));
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
                self.qr_code = Some(qr_code::Data::new(url).unwrap());
                return self.update(Message::OpenDialog(Dialog::new(
                    "Link your device",
                    "Scan the QR code below to link your device.",
                    Action::None,
                )));
            }
            Message::OpenDialog(dialog) => self.dialog = dialog,
            Message::CloseDialog => self.dialog.close(),
            #[expect(clippy::dbg_macro)]
            Message::Received(received) => {
                dbg!(received);
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
            .as_iced_dialog(
                container(base).width(Fill).height(Fill),
                self.qr_code.as_ref(),
            )
            .height(320);

        dialog.into()
    }
}

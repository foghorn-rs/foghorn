use crate::{
    dialog::{Action, Dialog},
    no_debug::NoDebug,
};
use iced::{
    Element,
    Length::Fill,
    Task,
    futures::channel::oneshot,
    widget::{container, qr_code},
};
use presage::{
    libsignal_service::configuration::SignalServers,
    manager::{Linking, Registered},
    model::identity::OnNewIdentity,
    store::Store,
};
use presage_store_sled::{MigrationConflictStrategy, SledStore};
use std::sync::Arc;
use tokio::{runtime::Runtime, task::spawn_blocking};

type RegisteredManager = presage::Manager<SledStore, Registered>;
type LinkingManager = presage::Manager<SledStore, Linking>;
type ManagerError = presage::Error<<SledStore as Store>::Error>;

#[derive(Clone, Debug)]
pub enum Message {
    LoadManager(ManagerStatus),
    LinkSecondary,
    QrCode(String),
    OpenDialog(Dialog),
    CloseDialog,
}

#[derive(Clone, Debug, Default)]
pub enum ManagerStatus {
    Loaded(Box<RegisteredManager>),
    ManagerError(NoDebug<SledStore>, Arc<ManagerError>),
    #[default]
    Unloaded,
}

#[derive(Default)]
pub struct App {
    manager_status: ManagerStatus,
    qr_code: Option<qr_code::Data>,
    dialog: Dialog,
}

impl App {
    pub fn create() -> (Self, Task<Message>) {
        let load_manager = async || {
            let store = SledStore::open(
                "",
                MigrationConflictStrategy::BackupAndDrop,
                OnNewIdentity::Trust,
            )
            .await
            .unwrap();

            match RegisteredManager::load_registered(store.clone()).await {
                Ok(manager) => ManagerStatus::Loaded(Box::new(manager)),
                Err(err) => ManagerStatus::ManagerError(store.into(), Arc::new(err)),
            }
        };

        (
            Self::default(),
            Task::perform(non_send_fut(move || load_manager), Message::LoadManager),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoadManager(manager_status) => {
                self.manager_status = manager_status.clone();
                self.qr_code = None;

                if let ManagerStatus::ManagerError(_, error) = manager_status {
                    return match error.as_ref() {
                        &presage::Error::NotYetRegisteredError
                        | &presage::Error::NoProvisioningMessageReceived
                        | &presage::Error::ProvisioningError(presage::libsignal_service::provisioning::ProvisioningError::MissingMessage) => {
                            Task::done(Message::LinkSecondary)
                        }
                        err => Dialog::new(
                            "Oops! Something went wrong.",
                            err.to_string(),
                            Action::Close,
                        )
                        .into(),
                    };
                }

                self.dialog.close();
            }
            Message::LinkSecondary => {
                let ManagerStatus::ManagerError(store, _) = self.manager_status.clone() else {
                    panic!()
                };

                let (tx, rx) = oneshot::channel();

                let load_manager = async move || match LinkingManager::link_secondary_device(
                    store.clone().0,
                    SignalServers::Production,
                    "foghorn".to_owned(),
                    tx,
                )
                .await
                {
                    Ok(manager) => ManagerStatus::Loaded(Box::new(manager)),
                    Err(err) => ManagerStatus::ManagerError(store.clone(), Arc::new(err)),
                };

                return Task::batch([
                    Task::perform(non_send_fut(|| load_manager), Message::LoadManager),
                    Task::perform(rx, |url| Message::QrCode(url.unwrap().to_string())),
                ]);
            }
            Message::QrCode(url) => {
                self.qr_code = Some(qr_code::Data::new(url).unwrap());
                return Dialog::new(
                    "Link your device",
                    "You can scan the QR code below to link your device.",
                    Action::None,
                )
                .into();
            }
            Message::OpenDialog(dialog) => self.dialog = dialog,
            Message::CloseDialog => self.dialog.close(),
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = if let ManagerStatus::Loaded(_) = self.manager_status {
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

async fn non_send_fut<F: AsyncFnOnce() -> R, R: Send + 'static>(
    f: impl FnOnce() -> F + Send + 'static,
) -> R {
    spawn_blocking(|| Runtime::new().unwrap().block_on(f()()))
        .await
        .unwrap()
}

use crate::no_debug::NoDebug;
use iced::{
    Element, Task,
    futures::channel::oneshot,
    widget::{button, qr_code},
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
    ManagerStatus(ManagerStatus),
    LinkSecondary,
    QrCode(String),
}

#[expect(dead_code)]
#[derive(Clone, Debug, Default)]
pub enum ManagerStatus {
    Loaded(RegisteredManager),
    ManagerError(NoDebug<SledStore>, Arc<ManagerError>),
    #[default]
    Unloaded,
}

#[derive(Default)]
pub struct App {
    manager_status: ManagerStatus,
    qr_code: Option<qr_code::Data>,
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
                Ok(manager) => ManagerStatus::Loaded(manager),
                Err(err) => ManagerStatus::ManagerError(store.into(), Arc::new(err)),
            }
        };

        (
            Self::default(),
            Task::perform(non_send_fut(move || load_manager), Message::ManagerStatus),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ManagerStatus(manager_status) => {
                self.manager_status = manager_status;
                self.qr_code = None;
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
                    Ok(manager) => ManagerStatus::Loaded(manager),
                    Err(err) => ManagerStatus::ManagerError(store.clone(), Arc::new(err)),
                };

                return Task::batch([
                    Task::perform(non_send_fut(|| load_manager), Message::ManagerStatus),
                    Task::perform(rx, |url| Message::QrCode(url.unwrap().to_string())),
                ]);
            }
            Message::QrCode(url) => self.qr_code = Some(qr_code::Data::new(url).unwrap()),
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        if let ManagerStatus::Loaded(_) = self.manager_status {
            "registered".into()
        } else if let Some(url) = &self.qr_code {
            qr_code(url).into()
        } else {
            button("link").on_press(Message::LinkSecondary).into()
        }
    }
}

async fn non_send_fut<F: AsyncFnOnce() -> R, R: Send + 'static>(
    f: impl FnOnce() -> F + Send + 'static,
) -> R {
    spawn_blocking(|| Runtime::new().unwrap().block_on(f()()))
        .await
        .unwrap()
}

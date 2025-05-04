use iced::futures::{
    SinkExt as _, StreamExt as _,
    channel::{mpsc, oneshot},
};
use presage::{
    libsignal_service::configuration::SignalServers,
    manager::{Linking, Registered},
    model::identity::OnNewIdentity,
    store::Store,
};
use presage_store_sled::{MigrationConflictStrategy, SledStore};
use std::sync::Arc;
use tokio::{
    runtime::Builder,
    task::{self, LocalSet},
};

type RegisteredManager = presage::Manager<SledStore, Registered>;
type LinkingManager = presage::Manager<SledStore, Linking>;
pub type ManagerError = presage::Error<<SledStore as Store>::Error>;

enum Message {
    RegisteredManager(RegisteredManager),
    LoadRegistered(oneshot::Sender<ManagerError>),
    LinkSecondary(oneshot::Sender<ManagerError>, oneshot::Sender<String>),
    Shutdown,
}

#[derive(Clone)]
pub struct ManagerManager {
    sender: mpsc::Sender<Message>,
    _shutdown: Arc<Shutdown>,
}

pub struct Shutdown {
    sender: mpsc::Sender<Message>,
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        while self.sender.try_send(Message::Shutdown).is_err() {}
    }
}

impl Default for ManagerManager {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel(100);

        let self_sender = sender.clone();
        std::thread::spawn(move || {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(LocalSet::new().run_until(manager_manager(self_sender, receiver)));
        });

        let shutdown = Arc::new(Shutdown {
            sender: sender.clone(),
        });

        Self {
            sender,
            _shutdown: shutdown,
        }
    }
}

impl ManagerManager {
    pub async fn load_registered(mut self) -> Option<ManagerError> {
        let (tx, rx) = oneshot::channel();

        self.sender.send(Message::LoadRegistered(tx)).await.unwrap();

        rx.await.ok()
    }

    pub async fn link_secondary(mut self, url: oneshot::Sender<String>) -> Option<ManagerError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(Message::LinkSecondary(tx, url))
            .await
            .unwrap();

        rx.await.ok()
    }
}

async fn manager_manager(
    self_sender: mpsc::Sender<Message>,
    mut receiver: mpsc::Receiver<Message>,
) {
    let store = SledStore::open(
        "",
        MigrationConflictStrategy::BackupAndDrop,
        OnNewIdentity::Trust,
    )
    .await
    .unwrap();

    #[expect(unused_variables, clippy::collection_is_never_read)]
    let mut manager = None;

    while let Some(message) = receiver.next().await {
        match message {
            #[expect(unused_assignments)]
            Message::RegisteredManager(ok) => manager = Some(ok),
            Message::LoadRegistered(c) => {
                let store = store.clone();
                let mut self_sender = self_sender.clone();
                task::spawn_local(async move {
                    match RegisteredManager::load_registered(store).await {
                        Ok(ok) => self_sender
                            .send(Message::RegisteredManager(ok))
                            .await
                            .unwrap(),
                        Err(err) => c.send(err).unwrap(),
                    }
                });
            }
            Message::LinkSecondary(c, url) => {
                let (tx, rx) = oneshot::channel();

                let store = store.clone();
                let mut self_sender = self_sender.clone();
                task::spawn_local(async move {
                    match LinkingManager::link_secondary_device(
                        store,
                        SignalServers::Production,
                        "foghorn".to_owned(),
                        tx,
                    )
                    .await
                    {
                        Ok(ok) => self_sender
                            .send(Message::RegisteredManager(ok))
                            .await
                            .unwrap(),
                        Err(err) => c.send(err).unwrap(),
                    }
                });

                task::spawn_local(async { url.send(rx.await.unwrap().to_string()) });
            }
            Message::Shutdown => return,
        }
    }
}

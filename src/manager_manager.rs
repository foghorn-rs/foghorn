use crate::backoff::retry_fib;
use async_lock::OnceCell;
use iced::futures::{
    SinkExt as _, Stream, StreamExt as _,
    channel::{mpsc, oneshot},
};
use presage::{
    libsignal_service::{configuration::SignalServers, prelude::Content},
    manager::{Linking, Registered},
    model::{identity::OnNewIdentity, messages::Received},
    store::{ContentsStore as _, Store, Thread},
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
    LoadRegistered(oneshot::Sender<ManagerError>),
    LinkSecondary(oneshot::Sender<ManagerError>, oneshot::Sender<String>),
    StreamMessages(mpsc::Sender<Content>),
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

        std::thread::spawn(move || {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(LocalSet::new().run_until(manager_manager(receiver)));
        });

        Self {
            sender: sender.clone(),
            _shutdown: Arc::new(Shutdown { sender }),
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

    pub async fn stream_mesages(mut self) -> impl Stream<Item = Content> {
        let (tx, rx) = mpsc::channel(100);

        self.sender.send(Message::StreamMessages(tx)).await.unwrap();

        rx
    }
}

async fn manager_manager(mut receiver: mpsc::Receiver<Message>) {
    let store = SledStore::open(
        "",
        MigrationConflictStrategy::BackupAndDrop,
        OnNewIdentity::Trust,
    )
    .await
    .unwrap();

    let manager = Arc::new(OnceCell::new());
    let who_am_i = Arc::new(OnceCell::new());

    while let Some(message) = receiver.next().await {
        match message {
            Message::LoadRegistered(c) => {
                let store = store.clone();
                let manager = manager.clone();
                task::spawn_local(async move {
                    match RegisteredManager::load_registered(store).await {
                        Ok(ok) => _ = manager.set(ok).await,
                        Err(err) => c.send(err).unwrap(),
                    }
                });
            }
            Message::LinkSecondary(c, url) => {
                let (tx, rx) = oneshot::channel();

                let store = store.clone();
                let manager = manager.clone();
                task::spawn_local(async move {
                    match LinkingManager::link_secondary_device(
                        store,
                        SignalServers::Production,
                        "foghorn".to_owned(),
                        tx,
                    )
                    .await
                    {
                        Ok(ok) => _ = manager.set(ok).await,
                        Err(err) => c.send(err).unwrap(),
                    }
                });

                task::spawn_local(async { url.send(rx.await.unwrap().to_string()) });
            }
            Message::StreamMessages(mut c) => {
                let store = store.clone();
                let manager = manager.clone();
                let who_am_i = who_am_i.clone();
                task::spawn_local(async move {
                    _ = who_am_i
                        .get_or_init(async || {
                            retry_fib(async || manager.wait().await.whoami().await.ok()).await
                        })
                        .await;

                    for thread in store
                        .contacts()
                        .await
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|c| Thread::Contact(c.uuid))
                        .chain(
                            store
                                .groups()
                                .await
                                .into_iter()
                                .flatten()
                                .flatten()
                                .map(|g| Thread::Group(g.0)),
                        )
                    {
                        for message in store
                            .messages(&thread, ..)
                            .await
                            .into_iter()
                            .flatten()
                            .flatten()
                        {
                            c.send(message).await.unwrap();
                        }
                    }

                    let mut stream = manager
                        .wait()
                        .await
                        .clone()
                        .receive_messages()
                        .await
                        .unwrap()
                        .boxed_local();

                    while let Some(next) = stream.next().await {
                        if let Received::Content(message) = next {
                            c.send(*message).await.unwrap();
                        }
                    }
                });
            }
            Message::Shutdown => return,
        }
    }
}

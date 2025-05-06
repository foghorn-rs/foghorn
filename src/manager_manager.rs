use crate::{
    backoff::retry_fib,
    message::{Chat, Message, decode_content},
};
use async_lock::OnceCell;
use iced::futures::{
    SinkExt as _, Stream, StreamExt as _,
    channel::{mpsc, oneshot},
};
use presage::{
    libsignal_service::configuration::SignalServers,
    manager::{Linking, Registered},
    model::{identity::OnNewIdentity, messages::Received},
    store::{ContentsStore as _, Store, Thread},
};
use presage_store_sled::{MigrationConflictStrategy, SledStore};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::{
    runtime::Builder,
    task::{self, LocalSet},
};

pub type RegisteredManager = presage::Manager<SledStore, Registered>;
pub type LinkingManager = presage::Manager<SledStore, Linking>;
pub type ManagerError = presage::Error<<SledStore as Store>::Error>;

enum Event {
    LoadRegistered(oneshot::Sender<ManagerError>),
    LinkSecondary(oneshot::Sender<ManagerError>, oneshot::Sender<String>),
    StreamMessages(mpsc::Sender<(Chat, Message)>),
    Shutdown,
}

#[derive(Clone)]
pub struct ManagerManager {
    sender: mpsc::Sender<Event>,
    _shutdown: Arc<Shutdown>,
}

struct Shutdown {
    sender: mpsc::Sender<Event>,
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        while self.sender.try_send(Event::Shutdown).is_err() {}
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

        self.sender.send(Event::LoadRegistered(tx)).await.unwrap();

        rx.await.ok()
    }

    pub async fn link_secondary(mut self, url: oneshot::Sender<String>) -> Option<ManagerError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(Event::LinkSecondary(tx, url))
            .await
            .unwrap();

        rx.await.ok()
    }

    pub async fn stream_mesages(mut self) -> impl Stream<Item = (Chat, Message)> {
        let (tx, rx) = mpsc::channel(100);

        self.sender.send(Event::StreamMessages(tx)).await.unwrap();

        rx
    }
}

async fn manager_manager(mut receiver: mpsc::Receiver<Event>) {
    let store = SledStore::open(
        "",
        MigrationConflictStrategy::BackupAndDrop,
        OnNewIdentity::Trust,
    )
    .await
    .unwrap();

    let manager = Rc::new(RefCell::new(None));
    let who_am_i = Rc::new(OnceCell::new());

    while let Some(message) = receiver.next().await {
        match message {
            Event::LoadRegistered(c) => {
                let store = store.clone();
                let manager = manager.clone();
                task::spawn_local(async move {
                    match RegisteredManager::load_registered(store).await {
                        Ok(ok) => *manager.borrow_mut() = Some(ok),
                        Err(err) => c.send(err).unwrap(),
                    }
                });
            }
            Event::LinkSecondary(c, url) => {
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
                        Ok(ok) => *manager.borrow_mut() = Some(ok),
                        Err(err) => c.send(err).unwrap(),
                    }
                });

                task::spawn_local(async { url.send(rx.await.unwrap().to_string()) });
            }
            Event::StreamMessages(c) => {
                let store = store.clone();
                let manager = manager.borrow().clone().unwrap();
                let who_am_i = who_am_i.clone();
                task::spawn_local(async move {
                    who_am_i
                        .get_or_init(async || retry_fib(async || manager.whoami().await.ok()).await)
                        .await;

                    let chats = Rc::new(RefCell::new(HashMap::new()));

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
                            let mut store = store.clone();
                            let mut manager = manager.clone();
                            let who_am_i = who_am_i.clone();
                            let chats = chats.clone();
                            let mut c = c.clone();
                            task::spawn_local(async move {
                                if let Some(message) = decode_content(
                                    message,
                                    &mut manager,
                                    &mut store,
                                    who_am_i.wait().await,
                                    &chats,
                                )
                                .await
                                {
                                    c.send(message).await.unwrap();
                                }
                            });
                        }
                    }

                    let mut stream = manager
                        .clone()
                        .receive_messages()
                        .await
                        .unwrap()
                        .boxed_local();

                    while let Some(next) = stream.next().await {
                        if let Received::Content(message) = next {
                            let mut store = store.clone();
                            let mut manager = manager.clone();
                            let who_am_i = who_am_i.clone();
                            let chats = chats.clone();
                            let mut c = c.clone();
                            task::spawn_local(async move {
                                if let Some(message) = decode_content(
                                    *message,
                                    &mut manager,
                                    &mut store,
                                    who_am_i.wait().await,
                                    &chats,
                                )
                                .await
                                {
                                    c.send(message).await.unwrap();
                                }
                            });
                        }
                    }
                });
            }
            Event::Shutdown => return,
        }
    }
}

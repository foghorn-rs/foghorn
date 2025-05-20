use crate::{
    message::{Chat, Quote, SignalAction, decode_content, sync_contacts, sync_messages},
    parse::markdown_to_body_ranges,
};
use iced::futures::{
    SinkExt as _, Stream, StreamExt as _,
    channel::{mpsc, oneshot},
};
use jiff::Timestamp;
use presage::{
    libsignal_service::{
        configuration::SignalServers, content::Metadata, prelude::Content, protocol::Aci,
    },
    manager::{Linking, Registered},
    model::{identity::OnNewIdentity, messages::Received},
    proto::{DataMessage, SyncMessage, sync_message::Sent},
    store::{ContentsStore as _, Store},
};
use presage_store_sled::{MigrationConflictStrategy, SledStore};
use std::{cell::RefCell, collections::HashMap, pin::pin, rc::Rc, sync::Arc};
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
    StreamMessages(mpsc::Sender<(Chat, SignalAction)>),
    SendMessage(
        Chat,
        String,
        Option<Quote>,
        oneshot::Sender<(Chat, SignalAction)>,
    ),
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

    pub async fn stream_mesages(mut self) -> impl Stream<Item = (Chat, SignalAction)> {
        let (tx, rx) = mpsc::channel(100);

        self.sender.send(Event::StreamMessages(tx)).await.unwrap();

        rx
    }

    pub async fn send(
        mut self,
        chat: Chat,
        content: String,
        quote: Option<Quote>,
    ) -> Option<(Chat, SignalAction)> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(Event::SendMessage(chat, content, quote, tx))
            .await
            .unwrap();

        rx.await.ok()
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
    let cache = Rc::new(RefCell::new(HashMap::new()));

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
            Event::StreamMessages(mut c) => {
                let mut manager = manager.borrow().clone().unwrap();
                let cache = cache.clone();
                task::spawn_local(async move {
                    let mut synced = false;

                    {
                        let mut manager = manager.clone();
                        task::spawn_local(async move { manager.request_contacts().await });
                    }

                    sync_contacts(&mut manager, &cache, &mut c).await;
                    sync_messages(&mut manager, &cache, &mut c).await;

                    let mut stream = pin!(manager.receive_messages().await.unwrap());

                    while let Some(next) = stream.next().await {
                        match next {
                            Received::Content(content) => {
                                if let Some(message) =
                                    decode_content(*content, &mut manager, &cache, synced).await
                                {
                                    c.send(message).await.unwrap();
                                }
                            }
                            Received::QueueEmpty => synced = true,
                            Received::Contacts => sync_contacts(&mut manager, &cache, &mut c).await,
                        }
                    }
                });
            }
            Event::SendMessage(chat, content, quote, c) => {
                let mut manager = manager.borrow().clone().unwrap();
                let cache = cache.clone();
                task::spawn_local(async move {
                    let (body, body_ranges) = markdown_to_body_ranges(content.trim());
                    if body.trim().is_empty() {
                        return;
                    }

                    let metadata = Metadata {
                        sender: manager.registration_data().service_ids.aci().into(),
                        destination: manager.registration_data().service_ids.aci().into(),
                        sender_device: manager.registration_data().device_id.unwrap_or_default(),
                        timestamp: Timestamp::now().as_millisecond() as u64,
                        needs_receipt: true,
                        unidentified_sender: false,
                        was_plaintext: true,
                        server_guid: None,
                    };

                    let message = DataMessage {
                        body: Some(body.clone()),
                        attachments: Vec::new(),
                        group_v2: chat.group_context(),
                        profile_key: chat.profile_key().map(Into::into),
                        quote: quote.map(Into::into),
                        body_ranges: body_ranges.clone(),
                        ..Default::default()
                    };

                    match &chat {
                        Chat::Contact(contact) => manager
                            .send_message(
                                Aci::from(contact.uuid),
                                message.clone(),
                                metadata.timestamp,
                            )
                            .await
                            .unwrap(),
                        Chat::Group(group) => {
                            manager
                                .send_message_to_group(
                                    &group.key,
                                    message.clone(),
                                    metadata.timestamp,
                                )
                                .await
                                .unwrap();
                        }
                    }

                    let message = Content {
                        metadata,
                        body: SyncMessage {
                            sent: Some(Sent {
                                destination_service_id: chat.uuid().map(|uuid| uuid.to_string()),
                                message: Some(message),
                                ..Sent::default()
                            }),
                            ..SyncMessage::default()
                        }
                        .into(),
                    };

                    manager
                        .store()
                        .save_message(&chat.thread(), message.clone())
                        .await
                        .unwrap();

                    c.send(
                        decode_content(message, &mut manager, &cache, false)
                            .await
                            .unwrap(),
                    )
                    .unwrap();
                });
            }
            Event::Shutdown => return,
        }
    }
}

use crate::{
    log, manager_manager::RegisteredManager, parse::body_ranges_to_signal_spans, widget::SignalSpan,
};
use iced::{
    futures::{SinkExt as _, StreamExt as _, channel::mpsc, stream::FuturesOrdered},
    widget::image,
};
use jiff::Timestamp;
use mime::Mime;
use presage::{
    libsignal_service::{
        content::{ContentBody, Metadata},
        prelude::{Content, ProfileKey, Uuid},
        zkgroup::{GroupMasterKeyBytes, ProfileKeyBytes},
    },
    proto::{
        AttachmentPointer, BodyRange, DataMessage, EditMessage, GroupContextV2, SyncMessage,
        data_message::{
            self, Delete,
            quote::{self, QuotedAttachment},
        },
        sync_message::Sent,
    },
    store::{ContentsStore as _, Thread},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

mod view;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Chat {
    Contact(Arc<Contact>),
    Group(Arc<Group>),
}

impl Chat {
    pub fn name(&self) -> &str {
        match self {
            Self::Contact(contact) => &contact.name,
            Self::Group(group) => &group.title,
        }
    }

    pub fn thread(&self) -> Thread {
        match self {
            Self::Contact(contact) => Thread::Contact(contact.uuid),
            Self::Group(group) => Thread::Group(group.key),
        }
    }

    pub fn uuid(&self) -> Option<Uuid> {
        match self {
            Self::Contact(contact) => Some(contact.uuid),
            Self::Group(_) => None,
        }
    }

    pub fn profile_key(&self) -> Option<ProfileKeyBytes> {
        match self {
            Self::Contact(contact) => Some(contact.key),
            Self::Group(_) => None,
        }
    }

    pub fn group_context(&self) -> Option<GroupContextV2> {
        match self {
            Self::Contact(_) => None,
            Self::Group(group) => Some(GroupContextV2 {
                master_key: Some(group.key.into()),
                revision: Some(group.revision),
                group_change: None,
            }),
        }
    }

    fn contact(&self) -> Option<Arc<Contact>> {
        match self {
            Self::Contact(contact) => Some(contact.clone()),
            Self::Group(_) => None,
        }
    }
}

#[derive(Debug, Eq)]
pub struct Contact {
    pub key: ProfileKeyBytes,
    pub uuid: Uuid,
    pub name: String,
    pub avatar: Option<image::Handle>,
    pub is_self: bool,
}

impl PartialEq for Contact {
    fn eq(&self, other: &Self) -> bool {
        self.uuid.eq(&other.uuid)
    }
}

impl Hash for Contact {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

#[derive(Debug, Eq)]
pub struct Group {
    pub key: GroupMasterKeyBytes,
    pub revision: u32,
    pub title: String,
    pub avatar: Option<image::Handle>,
    pub members: Vec<Arc<Contact>>,
}

impl PartialEq for Group {
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl Hash for Group {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub ptr: AttachmentPointer,
    pub mime: Mime,
    pub image: Option<image::Handle>,
}

impl Attachment {
    async fn new(ptr: AttachmentPointer, manager: &RegisteredManager) -> Self {
        let mime = ptr.content_type().parse::<Mime>().unwrap();
        let image = if mime.type_() == mime::IMAGE {
            manager
                .get_attachment(&ptr)
                .await
                .ok()
                .map(image::Handle::from_bytes)
        } else {
            None
        };

        Self { ptr, mime, image }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub timestamp: Timestamp,
    pub body: Option<Vec<SignalSpan<'static>>>,
    pub attachments: Vec<Attachment>,
    pub sticker: Option<Attachment>,
    pub sender: Arc<Contact>,
    pub quote: Option<Quote>,
}

impl Message {
    pub async fn new(
        timestamp: u64,
        body: Option<String>,
        attachments: Vec<AttachmentPointer>,
        sender: Uuid,
        sticker: Option<data_message::Sticker>,
        quote: Option<data_message::Quote>,
        cache: &RefCell<HashMap<Thread, Chat>>,
        body_ranges: Vec<BodyRange>,
        manager: &RegisteredManager,
    ) -> Self {
        let sticker = if let Some(ptr) = sticker.and_then(|sticker| sticker.data) {
            Some(Attachment::new(ptr, manager).await)
        } else {
            None
        };

        let quote = if let Some(quote) = quote {
            Some(Quote::new(quote, cache, manager).await)
        } else {
            None
        };

        Self {
            timestamp: Timestamp::from_millisecond(timestamp as i64).unwrap(),
            body: body_ranges_to_signal_spans(body, body_ranges),
            attachments: attachments
                .into_iter()
                .map(|ptr| Attachment::new(ptr, manager))
                .collect::<FuturesOrdered<_>>()
                .collect()
                .await,
            sender: cache.borrow()[&Thread::Contact(sender)].contact().unwrap(),
            sticker,
            quote,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Quote {
    pub timestamp: Timestamp,
    pub body: Option<Vec<SignalSpan<'static>>>,
    pub attachments: Vec<Attachment>,
    pub sender: Option<Arc<Contact>>,
}

impl Quote {
    async fn new(
        quote: data_message::Quote,
        cache: &RefCell<HashMap<Thread, Chat>>,
        manager: &RegisteredManager,
    ) -> Self {
        Self {
            timestamp: Timestamp::from_millisecond(quote.id.unwrap_or_default() as i64).unwrap(),
            body: body_ranges_to_signal_spans(quote.text, quote.body_ranges),
            attachments: quote
                .attachments
                .into_iter()
                .filter_map(|a| a.thumbnail)
                .map(|ptr| Attachment::new(ptr, manager))
                .collect::<FuturesOrdered<_>>()
                .collect()
                .await,
            sender: quote
                .author_aci
                .and_then(|sender| sender.parse().ok())
                .and_then(|sender| cache.borrow().get(&Thread::Contact(sender))?.contact()),
        }
    }
}

impl From<Message> for Quote {
    fn from(value: Message) -> Self {
        Self {
            timestamp: value.timestamp,
            body: value.body,
            attachments: value.attachments,
            sender: Some(value.sender),
        }
    }
}

impl From<Quote> for data_message::Quote {
    fn from(value: Quote) -> Self {
        Self {
            id: Some(value.timestamp.as_millisecond() as u64),
            author_aci: value.sender.map(|sender| sender.uuid.to_string()),
            text: value
                .body
                .as_ref()
                .map(|body| body.iter().map(|x| &*x.text).collect::<String>()),
            attachments: value
                .attachments
                .into_iter()
                .map(|attachment| QuotedAttachment {
                    content_type: Some(attachment.mime.to_string()),
                    file_name: None,
                    thumbnail: Some(attachment.ptr),
                })
                .collect(),
            body_ranges: vec![],
            r#type: Some(quote::Type::Normal as i32),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SignalAction {
    Contact,
    Message(Arc<Message>, bool),
    Replace(Timestamp, Arc<Message>),
    Delete(Timestamp),
}

pub async fn sync_contacts(
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
    c: &mut mpsc::Sender<(Chat, SignalAction)>,
) {
    let me = get_contact_cached(
        manager.registration_data().service_ids.aci,
        manager.registration_data().profile_key().bytes,
        manager,
        cache,
    )
    .await
    .unwrap();
    c.send((me, SignalAction::Contact)).await.unwrap();

    for contact in manager
        .store()
        .contacts()
        .await
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Some(contact) =
            get_contact_cached(contact.uuid, contact.profile_key, manager, cache).await
        {
            c.send((contact, SignalAction::Contact)).await.unwrap();
        }
    }

    for group in manager
        .store()
        .groups()
        .await
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Some(group) = get_group_cached(
            GroupContextV2 {
                revision: Some(group.1.revision),
                master_key: Some(group.0.into()),
                group_change: None,
            },
            manager,
            cache,
        )
        .await
        {
            c.send((group, SignalAction::Contact)).await.unwrap();
        }
    }
}

pub async fn sync_messages(
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
    c: &mut mpsc::Sender<(Chat, SignalAction)>,
) {
    for thread in manager
        .store()
        .contacts()
        .await
        .into_iter()
        .flatten()
        .flatten()
        .map(|c| Thread::Contact(c.uuid))
        .chain(
            manager
                .store()
                .groups()
                .await
                .into_iter()
                .flatten()
                .flatten()
                .map(|g| Thread::Group(g.0)),
        )
    {
        for message in manager
            .store()
            .messages(&thread, ..)
            .await
            .into_iter()
            .flatten()
            .flatten()
        {
            let message_log = format!("{}, {}", message.metadata, message.body);

            if let Some(message) = decode_content(message, manager, cache, false).await {
                c.send(message).await.unwrap();
            } else {
                log::warn!("Decoding of message failed: {}", message_log);
            }
        }
    }
}

pub async fn decode_content(
    content: Content,
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
    synced: bool,
) -> Option<(Chat, SignalAction)> {
    match (content.metadata, content.body) {
        (
            Metadata {
                sender, timestamp, ..
            },
            ContentBody::EditMessage(EditMessage {
                target_sent_timestamp,
                data_message:
                    Some(DataMessage {
                        body,
                        attachments,
                        group_v2,
                        profile_key,
                        quote,
                        sticker,
                        body_ranges,
                        ..
                    }),
            }),
        ) => {
            // a message edited not by us

            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                get_contact_cached(sender.raw_uuid(), profile_key?, manager, cache).await?
            };

            let message = Message::new(
                timestamp,
                body,
                attachments,
                sender.raw_uuid(),
                sticker,
                quote,
                cache,
                body_ranges,
                manager,
            )
            .await;

            debug_assert!(!message.sender.is_self);

            // delete the old message, so we don't load it again when starting up the next time
            manager
                .store()
                .clone()
                .delete_message(&chat.thread(), target_sent_timestamp?)
                .await
                .ok()?;

            Some((
                chat,
                SignalAction::Replace(
                    Timestamp::from_millisecond(target_sent_timestamp? as i64).unwrap(),
                    message.into(),
                ),
            ))
        }
        (
            Metadata {
                sender, timestamp, ..
            },
            ContentBody::SynchronizeMessage(SyncMessage {
                sent:
                    Some(Sent {
                        destination_service_id,
                        edit_message:
                            Some(EditMessage {
                                target_sent_timestamp,
                                data_message:
                                    Some(DataMessage {
                                        body,
                                        attachments,
                                        group_v2,
                                        profile_key,
                                        quote,
                                        sticker,
                                        body_ranges,
                                        ..
                                    }),
                            }),
                        ..
                    }),
                ..
            }),
        ) => {
            // a message edited by us

            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                let uuid = destination_service_id?.parse().ok()?;
                get_contact_cached(uuid, profile_key?, manager, cache).await?
            };

            let message = Message::new(
                timestamp,
                body,
                attachments,
                sender.raw_uuid(),
                sticker,
                quote,
                cache,
                body_ranges,
                manager,
            )
            .await;

            debug_assert!(message.sender.is_self);

            // delete the old message, so we don't load it again when starting up the next time
            manager
                .store()
                .clone()
                .delete_message(&chat.thread(), target_sent_timestamp?)
                .await
                .ok()?;

            Some((
                chat,
                SignalAction::Replace(
                    Timestamp::from_millisecond(target_sent_timestamp? as i64).unwrap(),
                    message.into(),
                ),
            ))
        }
        (
            Metadata { sender, .. },
            ContentBody::DataMessage(DataMessage {
                group_v2,
                profile_key,
                delete: Some(Delete {
                    target_sent_timestamp,
                }),
                ..
            }),
        ) => {
            // a message deleted not by us

            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                get_contact_cached(sender.raw_uuid(), profile_key?, manager, cache).await?
            };

            Some((
                chat,
                SignalAction::Delete(
                    Timestamp::from_millisecond(target_sent_timestamp? as i64).unwrap(),
                ),
            ))
        }
        (
            _,
            ContentBody::SynchronizeMessage(SyncMessage {
                sent:
                    Some(Sent {
                        destination_service_id,
                        message:
                            Some(DataMessage {
                                group_v2,
                                profile_key,
                                delete:
                                    Some(Delete {
                                        target_sent_timestamp,
                                    }),
                                ..
                            }),
                        ..
                    }),
                ..
            }),
        ) => {
            // a message deleted by us

            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                let uuid = destination_service_id?.parse().ok()?;
                get_contact_cached(uuid, profile_key?, manager, cache).await?
            };

            Some((
                chat,
                SignalAction::Delete(
                    Timestamp::from_millisecond(target_sent_timestamp? as i64).unwrap(),
                ),
            ))
        }
        (
            Metadata {
                sender, timestamp, ..
            },
            ContentBody::DataMessage(DataMessage {
                body,
                attachments,
                group_v2,
                profile_key,
                quote,
                sticker,
                body_ranges,
                ..
            }),
        ) => {
            // a message sent not by us, or previously edited by us

            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                get_contact_cached(sender.raw_uuid(), profile_key?, manager, cache).await?
            };

            let message = Message::new(
                timestamp,
                body,
                attachments,
                sender.raw_uuid(),
                sticker,
                quote,
                cache,
                body_ranges,
                manager,
            )
            .await;

            Some((chat, SignalAction::Message(message.into(), synced)))
        }
        (
            Metadata {
                sender, timestamp, ..
            },
            ContentBody::SynchronizeMessage(SyncMessage {
                sent:
                    Some(Sent {
                        destination_service_id,
                        message:
                            Some(DataMessage {
                                body,
                                attachments,
                                group_v2,
                                profile_key,
                                quote,
                                sticker,
                                body_ranges,
                                ..
                            }),
                        ..
                    }),
                ..
            }),
        ) => {
            // a message sent by us

            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                let uuid = destination_service_id?.parse().ok()?;
                get_contact_cached(uuid, profile_key?, manager, cache).await?
            };

            let message = Message::new(
                timestamp,
                body,
                attachments,
                sender.raw_uuid(),
                sticker,
                quote,
                cache,
                body_ranges,
                manager,
            )
            .await;

            debug_assert!(message.sender.is_self);

            Some((chat, SignalAction::Message(message.into(), false)))
        }
        _ => None,
    }
}

async fn get_group_cached(
    context: GroupContextV2,
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
) -> Option<Chat> {
    let key = context.master_key().try_into().ok()?;
    let revision = context.revision();

    let chat = Thread::Group(key);
    let group = manager.store().group(key).await.ok()??;

    if group.revision == revision {
        if let Some(chat) = cache.borrow().get(&chat) {
            return Some(chat.clone());
        }
    }

    let mut members = vec![];

    for member in group.members {
        if let Some(member) =
            get_contact_cached(member.uuid, member.profile_key.bytes, manager, cache).await
        {
            members.push(member.contact()?);
        }
    }

    let group = Group {
        key,
        revision,
        title: group.title,
        avatar: manager
            .retrieve_group_avatar(context)
            .await
            .ok()?
            .map(image::Handle::from_bytes),
        members,
    };

    cache
        .borrow_mut()
        .insert(chat.clone(), Chat::Group(group.into()));

    Some(cache.borrow()[&chat].clone())
}

async fn get_contact_cached(
    uuid: Uuid,
    profile_key: impl TryInto<ProfileKeyBytes>,
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
) -> Option<Chat> {
    let chat = Thread::Contact(uuid);

    if let Some(chat) = cache.borrow().get(&chat) {
        return Some(chat.clone());
    }

    let profile_key = ProfileKey::create(profile_key.try_into().ok()?);

    let contact = Contact {
        key: profile_key.bytes,
        uuid,
        name: manager
            .retrieve_profile_by_uuid(uuid, profile_key)
            .await
            .ok()?
            .name?
            .to_string(),
        avatar: manager
            .retrieve_profile_avatar_by_uuid(uuid, profile_key)
            .await
            .ok()?
            .map(image::Handle::from_bytes),
        is_self: uuid == manager.registration_data().service_ids.aci,
    };

    cache
        .borrow_mut()
        .insert(chat.clone(), Chat::Contact(contact.into()));

    Some(cache.borrow()[&chat].clone())
}

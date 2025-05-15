use crate::{
    manager_manager::RegisteredManager, parse::body_ranges_to_signal_spans, widget::SignalSpan,
};
use iced::widget::image;
use jiff::Timestamp;
use presage::{
    libsignal_service::{
        content::{ContentBody, Metadata},
        prelude::{Content, ProfileKey, Uuid},
        zkgroup::{GroupMasterKeyBytes, ProfileKeyBytes},
    },
    proto::{
        AttachmentPointer, BodyRange, DataMessage, GroupContextV2, SyncMessage, data_message,
        sync_message::Sent,
    },
    store::{ContentsStore as _, Thread},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
};

mod view;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Chat {
    Contact(Contact),
    Group(Group),
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

    fn contact(&self) -> Option<Contact> {
        match self {
            Self::Contact(contact) => Some(contact.clone()),
            Self::Group(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
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

impl Eq for Contact {}

impl Hash for Contact {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct Group {
    pub key: GroupMasterKeyBytes,
    pub revision: u32,
    pub title: String,
    pub avatar: Option<image::Handle>,
    pub members: Vec<Contact>,
}

impl PartialEq for Group {
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl Eq for Group {}

impl Hash for Group {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub ptr: AttachmentPointer,
    pub data: Option<Vec<u8>>,
    pub image: Option<image::Handle>,
}

impl Attachment {
    async fn new(ptr: AttachmentPointer, manager: &RegisteredManager) -> Self {
        let data = manager.get_attachment(&ptr).await.ok();
        let image = data.clone().map(image::Handle::from_bytes);
        Self { ptr, data, image }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub timestamp: Timestamp,
    pub body: Option<Vec<SignalSpan<'static>>>,
    pub attachments: Vec<Attachment>,
    pub sticker: Option<Attachment>,
    pub sender: Contact,
    pub quote: Option<Quote>,
    pub is_from_store: bool,
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
        is_from_store: bool,
        manager: &RegisteredManager,
    ) -> Self {
        let mut a = Vec::new();
        for ptr in attachments {
            a.push(Attachment::new(ptr, manager).await);
        }

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
            attachments: a,
            sender: cache.borrow()[&Thread::Contact(sender)].contact().unwrap(),
            sticker,
            quote,
            is_from_store,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Quote {
    pub timestamp: Timestamp,
    pub body: Option<Vec<SignalSpan<'static>>>,
    pub attachments: Vec<Attachment>,
    pub sender: Option<Contact>,
}

impl Quote {
    async fn new(
        quote: data_message::Quote,
        cache: &RefCell<HashMap<Thread, Chat>>,
        manager: &RegisteredManager,
    ) -> Self {
        let mut a = Vec::new();
        for ptr in quote.attachments.iter().filter_map(|a| a.thumbnail.clone()) {
            a.push(Attachment::new(ptr, manager).await);
        }

        Self {
            timestamp: Timestamp::from_millisecond(quote.id() as i64).unwrap(),
            body: body_ranges_to_signal_spans(quote.text, quote.body_ranges),
            attachments: a,
            sender: quote
                .author_aci
                .and_then(|sender| sender.parse().ok())
                .and_then(|sender| cache.borrow().get(&Thread::Contact(sender))?.contact()),
        }
    }
}

pub async fn decode_content(
    content: Content,
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
    is_from_store: bool,
) -> Option<(Chat, Message)> {
    match (content.metadata, content.body) {
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
            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                let uuid = destination_service_id?.parse().ok()?;
                get_contact_cached(uuid, profile_key.as_deref(), manager, cache).await?
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
                is_from_store,
                manager,
            )
            .await;

            Some((chat, message))
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
            let chat = if let Some(context) = group_v2 {
                get_group_cached(context, manager, cache).await?
            } else {
                get_contact_cached(sender.raw_uuid(), profile_key.as_deref(), manager, cache)
                    .await?
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
                is_from_store,
                manager,
            )
            .await;

            Some((chat, message))
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

    for member in &group.members {
        let member =
            get_contact_cached(member.uuid, Some(member.profile_key.bytes), manager, cache)
                .await?
                .contact()?;

        members.push(member);
    }

    let avatar = manager
        .retrieve_group_avatar(context)
        .await
        .ok()?
        .map(image::Handle::from_bytes);

    cache.borrow_mut().insert(
        chat.clone(),
        Chat::Group(Group {
            key,
            revision,
            title: group.title,
            avatar,
            members,
        }),
    );

    Some(cache.borrow()[&chat].clone())
}

pub async fn ensure_self_exists(
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
) {
    get_contact_cached(
        manager.registration_data().service_ids.aci,
        Some(manager.registration_data().profile_key().bytes),
        manager,
        cache,
    )
    .await
    .unwrap();
}

async fn get_contact_cached(
    uuid: Uuid,
    profile_key: Option<impl TryInto<ProfileKeyBytes>>,
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
) -> Option<Chat> {
    let chat = Thread::Contact(uuid);

    if let Some(chat) = cache.borrow().get(&chat) {
        return Some(chat.clone());
    }

    let profile_key = ProfileKey::create(profile_key?.try_into().ok()?);

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
        .insert(chat.clone(), Chat::Contact(contact));

    Some(cache.borrow()[&chat].clone())
}

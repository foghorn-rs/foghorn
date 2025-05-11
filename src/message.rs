use crate::manager_manager::RegisteredManager;
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
    fn contact(&self) -> Option<Contact> {
        match self {
            Self::Contact(contact) => Some(contact.clone()),
            Self::Group(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Contact {
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
    pub preview: Option<image::Handle>,
}

impl From<AttachmentPointer> for Attachment {
    fn from(ptr: AttachmentPointer) -> Self {
        Self {
            preview: ptr.thumbnail.clone().map(image::Handle::from_bytes),
            ptr,
            data: None,
            image: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub timestamp: Timestamp,
    pub body: Option<String>,
    pub attachments: Vec<Attachment>,
    pub sticker: Option<Attachment>,
    pub sender: Contact,
    pub quote: Option<Quote>,
    pub body_ranges: Vec<BodyRange>,
}

impl Message {
    fn new(
        timestamp: u64,
        body: Option<String>,
        attachments: Vec<AttachmentPointer>,
        sender: Uuid,
        sticker: Option<data_message::Sticker>,
        quote: Option<data_message::Quote>,
        body_ranges: Vec<BodyRange>,
        cache: &RefCell<HashMap<Thread, Chat>>,
    ) -> Option<Self> {
        Some(Self {
            timestamp: Timestamp::from_millisecond(timestamp as i64).unwrap(),
            body,
            attachments: attachments.into_iter().map(Attachment::from).collect(),
            sender: cache.borrow().get(&Thread::Contact(sender))?.contact()?,
            sticker: sticker
                .and_then(|sticker| sticker.data)
                .map(Attachment::from),
            quote: quote.map(|quote| Quote::new(quote, cache)),
            body_ranges,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Quote {
    pub timestamp: Timestamp,
    pub body: Option<String>,
    pub sender: Option<Contact>,
    pub body_ranges: Vec<BodyRange>,
}

impl Quote {
    fn new(quote: data_message::Quote, cache: &RefCell<HashMap<Thread, Chat>>) -> Self {
        Self {
            timestamp: Timestamp::from_millisecond(quote.id() as i64).unwrap(),
            body: quote.text,
            sender: quote
                .author_aci
                .and_then(|sender| sender.parse().ok())
                .and_then(|sender| cache.borrow().get(&Thread::Contact(sender))?.contact()),
            body_ranges: quote.body_ranges,
        }
    }
}

pub async fn decode_content(
    content: Content,
    manager: &mut RegisteredManager,
    cache: &RefCell<HashMap<Thread, Chat>>,
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
                body_ranges,
                cache,
            )?;

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
                body_ranges,
                cache,
            )?;

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

    let mut members = Vec::new();

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
            title: group.title,
            avatar,
            members,
        }),
    );

    Some(cache.borrow()[&chat].clone())
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

use crate::manager_manager::{ManagerError, RegisteredManager};
use iced::widget::image;
use jiff::{Timestamp, Unit};
use presage::{
    libsignal_service::{
        content::{ContentBody, Metadata},
        prelude::{Content, ProfileKey, Uuid},
        zkgroup::{GROUP_MASTER_KEY_LEN, GroupMasterKeyBytes, PROFILE_KEY_LEN},
    },
    proto::{AttachmentPointer, BodyRange, DataMessage, SyncMessage, sync_message::Sent},
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

#[derive(Clone, Debug)]
pub struct Contact {
    pub uuid: Uuid,
    pub name: String,
    pub avatar: Option<image::Handle>,
    me: Uuid,
}

impl Contact {
    pub fn is_me(&self) -> bool {
        self.me == self.uuid
    }
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

#[derive(Clone, Debug)]
pub struct Quote {
    pub timestamp: Timestamp,
    pub body: Option<String>,
    pub sender: Option<Contact>,
    pub body_ranges: Vec<BodyRange>,
}

pub async fn decode_content(
    content: Content,
    manager: &mut RegisteredManager,
    chats: &RefCell<HashMap<Thread, Chat>>,
) -> Option<(Chat, Message)> {
    match (content.metadata, content.body) {
        // incoming direct/group message
        (
            Metadata {
                timestamp,
                sender,
                destination,
                ..
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
            })
            | ContentBody::SynchronizeMessage(SyncMessage {
                sent:
                    Some(Sent {
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
                let master_key = context.master_key();
                let revision = context.revision();

                let mut key = [0; GROUP_MASTER_KEY_LEN];
                key.copy_from_slice(master_key.get(..GROUP_MASTER_KEY_LEN)?);
                let chat = Thread::Group(key);

                let group = manager.store().group(key).await.ok()??;

                if group.revision != revision || !chats.borrow().contains_key(&chat) {
                    let mut members = Vec::new();

                    for member in &group.members {
                        let chat = Thread::Contact(member.uuid);

                        if let Some(contact) = chats.borrow().get(&chat) {
                            let Chat::Contact(contact) = contact else {
                                return None;
                            };

                            members.push(contact.clone());
                        } else {
                            let contact =
                                retrieve_contact_by_uuid(member.uuid, member.profile_key, manager)
                                    .await
                                    .ok()?;

                            chats
                                .borrow_mut()
                                .insert(chat, Chat::Contact(contact.clone()));

                            members.push(contact);
                        }
                    }

                    let avatar = manager
                        .retrieve_group_avatar(context)
                        .await
                        .ok()?
                        .map(image::Handle::from_bytes);

                    chats.borrow_mut().insert(
                        chat.clone(),
                        Chat::Group(Group {
                            key,
                            title: group.title,
                            avatar,
                            members,
                        }),
                    );
                }

                chat
            } else {
                let uuid = if sender.raw_uuid() == manager.registration_data().service_ids.aci {
                    destination.raw_uuid()
                } else {
                    sender.raw_uuid()
                };
                let chat = Thread::Contact(uuid);

                if !chats.borrow().contains_key(&chat) {
                    let mut bytes = [0; PROFILE_KEY_LEN];
                    bytes.copy_from_slice(profile_key?.get(..PROFILE_KEY_LEN)?);

                    let contact = retrieve_contact_by_uuid(uuid, ProfileKey { bytes }, manager)
                        .await
                        .ok()?;

                    chats
                        .borrow_mut()
                        .insert(chat.clone(), Chat::Contact(contact));
                }

                chat
            };
            let chat = chats.borrow()[&chat].clone();

            let quote_sender = if let Some(quote) = &quote {
                let chat = Thread::Contact(quote.author_aci().parse().ok()?);

                if let Some(Chat::Contact(sender)) = chats.borrow().get(&chat) {
                    Some(sender.clone())
                } else {
                    None
                }
            } else {
                None
            };

            let Chat::Contact(sender) = &chats.borrow()[&Thread::Contact(sender.raw_uuid())] else {
                return None;
            };
            let sender = sender.clone();

            let message = Message {
                timestamp: Timestamp::from_millisecond(timestamp as i64)
                    .unwrap()
                    .round(Unit::Minute)
                    .unwrap(),
                body,
                attachments: attachments.into_iter().map(Attachment::from).collect(),
                sender,
                sticker: sticker.and_then(|sticker| sticker.data.map(Attachment::from)),
                quote: quote.map(|quote| Quote {
                    timestamp: Timestamp::from_millisecond(quote.id() as i64)
                        .unwrap()
                        .round(Unit::Minute)
                        .unwrap(),
                    body: quote.text,
                    sender: quote_sender,
                    body_ranges: quote.body_ranges,
                }),
                body_ranges,
            };

            Some((chat, message))
        }
        _ => None,
    }
}

async fn retrieve_contact_by_uuid(
    uuid: Uuid,
    profile_key: ProfileKey,
    manager: &mut RegisteredManager,
) -> Result<Contact, ManagerError> {
    Ok(Contact {
        uuid,
        name: manager
            .retrieve_profile_by_uuid(uuid, profile_key)
            .await?
            .name
            .map(|name| name.to_string())
            .unwrap_or_default(),
        avatar: manager
            .retrieve_profile_avatar_by_uuid(uuid, profile_key)
            .await?
            .map(image::Handle::from_bytes),
        me: manager.registration_data().service_ids.aci,
    })
}

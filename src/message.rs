use crate::manager_manager::RegisteredManager;
use iced::{Color, widget::image};
use presage::{
    libsignal_service::{
        content::{ContentBody, Metadata},
        prelude::{Content, ProfileKey, Uuid},
        push_service::WhoAmIResponse,
        zkgroup::{GROUP_MASTER_KEY_LEN, GroupMasterKeyBytes, PROFILE_KEY_LEN},
    },
    proto::{AttachmentPointer, DataMessage, GroupContextV2, SyncMessage, sync_message::Sent},
    store::{ContentsStore as _, Thread},
};
use presage_store_sled::SledStore;
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Chat {
    Contact(Contact),
    Group(Group),
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub uuid: Uuid, // hash this only
    pub name: String,
    pub avatar: Option<image::Handle>,
    pub color: Option<Color>,
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
    pub key: GroupMasterKeyBytes, // hash this only
    pub title: String,
    pub avatar: image::Handle,
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

#[derive(Clone, Debug)]
pub struct Message {
    pub timestamp: u64,
    pub body: Option<String>,
    pub attachments: Vec<Attachment>,
    pub sticker: Option<Attachment>,
    pub sender: Contact,
}

pub async fn decode_content(
    content: Content,
    manager: &mut RegisteredManager,
    store: &mut SledStore,
    who_am_i: &WhoAmIResponse,
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
                group_v2,
                attachments,
                sticker,
                profile_key,
                ..
            })
            | ContentBody::SynchronizeMessage(SyncMessage {
                sent:
                    Some(Sent {
                        message:
                            Some(DataMessage {
                                body,
                                group_v2,
                                attachments,
                                sticker,
                                profile_key,
                                ..
                            }),
                        ..
                    }),
                ..
            }),
        ) => {
            let chat = if let Some(GroupContextV2 {
                master_key: Some(master_key),
                ..
            }) = group_v2
            {
                let mut bytes = [0; GROUP_MASTER_KEY_LEN];
                bytes.copy_from_slice(master_key.get(..GROUP_MASTER_KEY_LEN)?);
                let group = store.group(bytes).await.ok()??;

                if !chats.borrow().contains_key(&Thread::Group(bytes)) {
                    let mut members = Vec::new();
                    for member in &group.members {
                        let profile = store
                            .profile(member.uuid, member.profile_key)
                            .await
                            .ok()??;

                        members.push(
                            if let Chat::Contact(contact) = chats
                                .borrow_mut()
                                .entry(Thread::Contact(member.uuid))
                                .or_insert_with(|| {
                                    Chat::Contact(Contact {
                                        uuid: member.uuid,
                                        name: profile
                                            .name
                                            .map(|name| name.given_name)
                                            .unwrap_or_default(),
                                        avatar: profile.avatar.map(|avatar| {
                                            image::Handle::from_bytes(avatar.into_bytes())
                                        }),
                                        color: None,
                                    })
                                })
                            {
                                contact.clone()
                            } else {
                                return None;
                            },
                        );
                    }

                    chats.borrow_mut().insert(
                        Thread::Group(bytes),
                        Chat::Group(Group {
                            key: bytes,
                            title: group.title,
                            avatar: image::Handle::from_bytes(group.avatar.into_bytes()),
                            members,
                        }),
                    );
                }

                Thread::Group(bytes)
            } else {
                let uuid = if [who_am_i.aci, who_am_i.pni].contains(&sender.raw_uuid()) {
                    destination.raw_uuid()
                } else {
                    sender.raw_uuid()
                };

                if !chats.borrow().contains_key(&Thread::Contact(uuid)) {
                    let mut bytes = [0; PROFILE_KEY_LEN];
                    bytes.copy_from_slice(profile_key?.get(..PROFILE_KEY_LEN)?);

                    let profile = if let Some(profile) =
                        store.profile(uuid, ProfileKey { bytes }).await.ok()?
                    {
                        profile
                    } else {
                        let profile = manager
                            .retrieve_profile_by_uuid(uuid, ProfileKey { bytes })
                            .await
                            .ok()?;

                        store
                            .save_profile(uuid, ProfileKey { bytes }, profile.clone())
                            .await
                            .ok()?;

                        profile
                    };

                    chats.borrow_mut().insert(
                        Thread::Contact(uuid),
                        Chat::Contact(Contact {
                            uuid,
                            name: profile.name.map(|name| name.given_name).unwrap_or_default(),
                            avatar: profile.avatar.map(image::Handle::from_bytes),
                            color: None,
                        }),
                    );
                }

                Thread::Contact(uuid)
            };

            let chat = chats.borrow()[&chat].clone();

            let Chat::Contact(sender) = &chats.borrow()[&Thread::Contact(sender.raw_uuid())] else {
                return None;
            };
            let sender = sender.clone();

            let message = Message {
                timestamp,
                body,
                attachments: attachments
                    .into_iter()
                    .map(|ptr| Attachment {
                        preview: ptr.thumbnail.clone().map(image::Handle::from_bytes),
                        ptr,
                        data: None,
                        image: None,
                    })
                    .collect(),
                sender,
                sticker: sticker.and_then(|sticker| {
                    sticker.data.map(|ptr| Attachment {
                        preview: ptr.thumbnail.clone().map(image::Handle::from_bytes),
                        ptr,
                        data: None,
                        image: None,
                    })
                }),
            };

            Some((chat, message))
        }
        _ => None,
    }
}

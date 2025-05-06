use crate::manager_manager::RegisteredManager;
use iced::{Color, widget::image};
use presage::{
    libsignal_service::{
        Profile,
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
    pub uuid: Uuid,
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
            let chat = if let Some(context) = group_v2 {
                let master_key = context.master_key();
                let revision = context.revision();

                let mut key = [0; GROUP_MASTER_KEY_LEN];
                key.copy_from_slice(master_key.get(..GROUP_MASTER_KEY_LEN)?);
                let group = store.group(key).await.ok()??;

                if group.revision != revision || !chats.borrow().contains_key(&Thread::Group(key)) {
                    let mut members = Vec::new();
                    for member in &group.members {
                        let contact =
                            get_contact_cached(member.uuid, member.profile_key, manager, store)
                                .await?;

                        members.push(
                            if let Chat::Contact(contact) = chats
                                .borrow_mut()
                                .entry(Thread::Contact(member.uuid))
                                .or_insert_with(|| Chat::Contact(contact()))
                            {
                                contact.clone()
                            } else {
                                return None;
                            },
                        );
                    }

                    let avatar = get_group_avatar_cached(key, revision, manager, store).await;

                    chats.borrow_mut().insert(
                        Thread::Group(key),
                        Chat::Group(Group {
                            key,
                            revision,
                            title: group.title,
                            avatar: avatar.map(image::Handle::from_bytes),
                            members,
                        }),
                    );
                }

                Thread::Group(key)
            } else {
                let uuid = if [who_am_i.aci, who_am_i.pni].contains(&sender.raw_uuid()) {
                    destination.raw_uuid()
                } else {
                    sender.raw_uuid()
                };

                if !chats.borrow().contains_key(&Thread::Contact(uuid)) {
                    let mut bytes = [0; PROFILE_KEY_LEN];
                    bytes.copy_from_slice(profile_key?.get(..PROFILE_KEY_LEN)?);

                    let contact =
                        get_contact_cached(uuid, ProfileKey { bytes }, manager, store).await?();

                    chats
                        .borrow_mut()
                        .insert(Thread::Contact(uuid), Chat::Contact(contact));
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

async fn get_contact_cached(
    uuid: Uuid,
    profile_key: ProfileKey,
    manager: &mut RegisteredManager,
    store: &mut SledStore,
) -> Option<impl FnOnce() -> Contact> {
    let profile = get_profile_cached(uuid, profile_key, manager, store).await?;
    let avatar = get_profile_avatar_cached(uuid, profile_key, manager, store).await;

    Some(move || Contact {
        uuid,
        name: profile
            .name
            .map(|name| name.to_string())
            .unwrap_or_default(),
        avatar: avatar.map(image::Handle::from_bytes),
        color: None,
    })
}

async fn get_profile_cached(
    uuid: Uuid,
    profile_key: ProfileKey,
    manager: &mut RegisteredManager,
    store: &mut SledStore,
) -> Option<Profile> {
    if let Some(profile) = store.profile(uuid, profile_key).await.ok()? {
        Some(profile)
    } else {
        let profile = manager
            .retrieve_profile_by_uuid(uuid, profile_key)
            .await
            .ok()?;

        store
            .save_profile(uuid, profile_key, profile.clone())
            .await
            .ok()?;

        Some(profile)
    }
}

async fn get_profile_avatar_cached(
    uuid: Uuid,
    profile_key: ProfileKey,
    manager: &mut RegisteredManager,
    store: &mut SledStore,
) -> Option<Vec<u8>> {
    if let Some(avatar) = store.profile_avatar(uuid, profile_key).await.ok()? {
        Some(avatar)
    } else {
        let avatar = manager
            .retrieve_profile_avatar_by_uuid(uuid, profile_key)
            .await
            .ok()??;

        store
            .save_profile_avatar(uuid, profile_key, &avatar)
            .await
            .ok()?;

        Some(avatar)
    }
}

async fn get_group_avatar_cached(
    master_key: GroupMasterKeyBytes,
    revision: u32,
    manager: &mut RegisteredManager,
    store: &SledStore,
) -> Option<Vec<u8>> {
    if let Some(avatar) = store.group_avatar(master_key).await.ok()? {
        Some(avatar)
    } else {
        let avatar = manager
            .retrieve_group_avatar(GroupContextV2 {
                master_key: Some(master_key.into()),
                revision: Some(revision),
                group_change: None,
            })
            .await
            .ok()??;

        store.save_group_avatar(master_key, &avatar).await.ok()?;

        Some(avatar)
    }
}

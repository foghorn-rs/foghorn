use iced::{Element, Task};
use presage::{manager::Registered, model::identity::OnNewIdentity, store::Store};
use presage_store_sled::{MigrationConflictStrategy, SledStore};
use std::sync::Arc;
use tokio::{runtime::Runtime, task::spawn_blocking};

type Manager = presage::Manager<SledStore, Registered>;
type SledStoreError = presage::Error<<SledStore as Store>::Error>;

#[derive(Clone, Debug)]
pub enum Message {
    ManagerLoaded(Result<Manager, Arc<SledStoreError>>),
}

pub struct App {
    manager: Option<Manager>,
}

impl App {
    pub fn create() -> (Self, Task<Message>) {
        (
            Self { manager: None },
            Task::perform(
                spawn_blocking(|| {
                    Runtime::new().unwrap().block_on(async {
                        Box::pin(Manager::load_registered(
                            SledStore::open(
                                "",
                                MigrationConflictStrategy::BackupAndDrop,
                                OnNewIdentity::Trust,
                            )
                            .await
                            .unwrap(),
                        ))
                        .await
                        .map_err(Arc::new)
                    })
                }),
                |manager| Message::ManagerLoaded(manager.unwrap()),
            ),
        )
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ManagerLoaded(manager) => {
                if let Ok(manager) = manager {
                    self.manager = Some(manager);
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.manager.is_none() {
            "not registered".into()
        } else {
            "registered".into()
        }
    }
}

use app::App;
use iced::{Result, application};
use presage as _;
use presage_store_sled as _;

mod app;
mod backoff;
mod dialog;
mod manager_manager;

fn main() -> Result {
    application(App::create, App::update, App::view)
        .antialiasing(true)
        .run()
}

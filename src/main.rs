use app::App;
use iced::{Result, application};

mod app;
mod dialog;
mod manager_manager;
mod message;

fn main() -> Result {
    application(App::create, App::update, App::view)
        .antialiasing(true)
        .run()
}

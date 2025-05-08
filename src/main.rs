use app::App;
use iced::{Result, application};

mod app;
mod dialog;
mod manager_manager;
mod message;
mod widget;

fn main() -> Result {
    application(App::create, App::update, App::view)
        .subscription(App::subscription)
        .antialiasing(true)
        .run()
}

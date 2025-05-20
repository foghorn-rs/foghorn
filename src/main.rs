use app::App;
use iced::{Result, application};
use icons::LUCIDE_BYTES;

mod app;
mod dialog;
mod icons;
mod manager_manager;
mod message;
mod parse;
mod widget;

fn main() -> Result {
    application(App::create, App::update, App::view)
        .subscription(App::subscription)
        .antialiasing(true)
        .font(LUCIDE_BYTES)
        .run()
}

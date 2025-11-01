use app::App;
use foghorn_widgets as widget;
use iced::{Result, application};
use icons::LUCIDE_BYTES;

mod app;
mod dialog;
mod icons;
mod log;
mod manager_manager;
mod message;
mod parse;

fn main() -> Result {
    #[expect(clippy::print_stderr)]
    if let Err(error) = log::init() {
        eprintln!("Foghorn: failed to initialize logger: {error}");
    }

    application(App::create, App::update, App::view)
        .subscription(App::subscription)
        .antialiasing(true)
        .font(LUCIDE_BYTES)
        .run()
}

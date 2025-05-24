#![expect(unused_imports)]
mod logger {
    use std::{fs::File, sync::Arc};
    use tracing::Level;
    pub use tracing::{debug, error, info, trace, warn};
    use tracing_subscriber::{
        filter::{EnvFilter, LevelFilter, Targets},
        fmt,
        prelude::*,
    };

    pub fn init() -> std::io::Result<()> {
        let stdout_log = fmt::layer().compact();

        let file = File::create("debug_log.json")?;
        let debug_log = fmt::layer().with_writer(Arc::new(file)).json();

        tracing_subscriber::registry()
            .with(
                stdout_log.with_filter(
                    Targets::default()
                        .with_target("foghorn::widget", Level::DEBUG)
                        .with_default(Level::INFO),
                ),
            )
            .with(debug_log)
            .with(
                Targets::default()
                    .with_target("foghorn", Level::TRACE)
                    .with_target("iced", Level::WARN)
                    .with_target("iced_wgpu", Level::WARN)
                    .with_target("iced_tiny_skia", Level::WARN)
                    .with_target("wgpu_core", LevelFilter::OFF),
            )
            .init();

        Ok(())
    }
}

pub use logger::{debug, error, info, init, trace, warn};

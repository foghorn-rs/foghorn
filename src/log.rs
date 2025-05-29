mod logger {
    use std::fs::File;
    use tracing::Level;
    pub use tracing::{debug, error, info, trace, warn};
    use tracing_subscriber::{
        filter::{LevelFilter, Targets},
        fmt,
        prelude::*,
    };

    pub fn init() -> std::io::Result<()> {
        tracing_subscriber::registry()
            .with(fmt::layer().compact())
            .with(
                fmt::layer()
                    .with_writer(File::create("debug_log.json")?)
                    .json(),
            )
            .with(
                Targets::default()
                    .with_target("foghorn", Level::TRACE)
                    .with_target("iced", Level::WARN)
                    .with_target("wgpu", LevelFilter::OFF),
            )
            .init();

        Ok(())
    }
}

#[expect(unused_imports)]
pub use logger::{debug, error, info, init, trace, warn};

mod logger {
    use std::{env, fs::File};
    use tracing::Level;
    pub use tracing::{debug, error, info, trace, warn};
    use tracing_subscriber::{
        filter::{LevelFilter, Targets},
        fmt,
        prelude::*,
    };

    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        let env_rust_log = env::var("RUST_LOG")
            .ok()
            .as_deref()
            .map(str::parse::<Level>)
            .transpose()?;

        tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .compact()
                    .with_filter(LevelFilter::from_level(env_rust_log.unwrap_or(Level::INFO))),
            )
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

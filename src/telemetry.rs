use crate::configuration::ApplicationSettings;
use tracing::subscriber::set_global_default;
use tracing::Subscriber;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub fn get_tracing_subscriber<Sink>(
    name: &str,
    default_log_level: &str,
    sink: Sink,
) -> impl Subscriber + Send + Sync
where
    // for<'a> is HRTB (aka Higher-Ranked Trait Bound)
    // use this to specify lifetime to specific type
    Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    // Get RUST_LOG environment variable
    // If not set, default value is "info"
    // RUST_LOG=info cargo <command> <args>
    let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(default_log_level));

    // Format Span with Bunyan format and output to stdout
    let formatting_layer = BunyanFormattingLayer::new(name.into(), sink);

    // Setup Span with Layers
    // use with to chain Layers pipeline
    Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
}

pub fn init_tracing_subscriber(subscriber: impl Subscriber + Send + Sync) {
    // actix-web Logger middleware use log crate for logging
    // Redirect all log events that use 'log crate' to Subscriber
    LogTracer::init().expect("Failed to init LogTracer");
    set_global_default(subscriber).expect("Failed to set tracing subscriber");
}

pub fn config_tracing(app_config: &ApplicationSettings) {
    init_tracing_subscriber(get_tracing_subscriber(
        &app_config.name,
        &app_config.rust_log,
        std::io::stdout,
    ));
}

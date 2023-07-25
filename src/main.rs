use sqlx::PgPool;
use std::net::TcpListener;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};
use zero2prod::configuration::Settings;
use zero2prod::startup::run;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // actix-web Logger middleware use log crate for logging
    // Redirect all log events that use 'log crate' to Subscriber
    LogTracer::init().expect("Failed to init LogTracer");

    // Get RUST_LOG environment variable
    // If not set, default value is "info"
    // RUST_LOG=info cargo <command> <args>
    let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"));

    // Format Span with Bunyan format and output to stdout
    let formatting_layer = BunyanFormattingLayer::new("zero2prod".into(), std::io::stdout);

    // Setup Span with Layers
    // use with to chain Layers pipeline
    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    set_global_default(subscriber).expect("Failed to set tracing subscriber");

    let settings = Settings::get_configuration().expect("Failed to read configuration");
    // Use Pool to handle queue of connections rather than single connection like PgConnection
    // Allow to work with multithreading actix-web runtime
    let db_connection_pool = PgPool::connect(&settings.database.get_database_url())
        .await
        .expect("Failed to connect to Postgres");
    let listener =
        TcpListener::bind(settings.application.get_url()).expect("Failed to bind address");

    run(listener, db_connection_pool)?.await
}

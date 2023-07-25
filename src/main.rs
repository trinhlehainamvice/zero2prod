use sqlx::PgPool;
use std::net::TcpListener;
use zero2prod::configuration::Settings;
use zero2prod::startup::run;
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::get_configuration().expect("Failed to read configuration");

    init_tracing_subscriber(get_tracing_subscriber(
        settings.application.name.clone(),
        settings.application.default_log_level.clone(),
        std::io::stdout,
    ));

    // Use Pool to handle queue of connections rather than single connection like PgConnection
    // Allow to work with multithreading actix-web runtime
    let db_connection_pool = PgPool::connect(&settings.database.get_database_url())
        .await
        .expect("Failed to connect to Postgres");
    let listener =
        TcpListener::bind(settings.application.get_url()).expect("Failed to bind address");

    run(listener, db_connection_pool)?.await
}

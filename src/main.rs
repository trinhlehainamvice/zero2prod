use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use zero2prod::configuration::Settings;
use zero2prod::email_client::EmailClient;
use zero2prod::routes::SubscriberEmail;
use zero2prod::startup::run;
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::get_configuration().expect("Failed to read configuration");

    init_tracing_subscriber(get_tracing_subscriber(
        &settings.application.name,
        &settings.application.default_log_level,
        std::io::stdout,
    ));

    // Use Pool to handle queue of connections rather than single connection like PgConnection
    // Allow to work with multithreading actix-web runtime
    // Use lazy connect to connect to database when needed
    let db_connection_pool = PgPoolOptions::new()
        // Limit connection timeout to avoid long wait times
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(settings.database.get_pg_database_options());
    let listener =
        TcpListener::bind(settings.application.get_url()).expect("Failed to bind address");

    let email_client = EmailClient::new(
        settings.email_client.api_base_url,
        SubscriberEmail::parse(settings.email_client.sender_email)
            .expect("Failed to parse sender email"),
        settings.email_client.auth_header,
        settings.email_client.auth_token,
        settings.email_client.request_timeout_millis
    );

    run(listener, db_connection_pool, email_client)?.await
}

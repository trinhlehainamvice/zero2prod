use crate::configuration::{DatabaseSettings, EmailClientSettings, Settings};
use crate::email_client::EmailClient;
use crate::routes::{check_health, subscribe, SubscriberEmail};
use actix_web::dev::Server;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub async fn build(pg_pool: PgPool, settings: Settings) -> Result<(Server, u16), std::io::Error> {
    let listener =
        TcpListener::bind(settings.application.get_url()).expect("Failed to bind address");

    let port = listener.local_addr().unwrap().port();

    let email_client = get_email_client(settings.email_client);
    // So to share data between threads, actix-web provide web::Data<T>(Arc<T>)
    // which is a thread-safe reference counting pointer to a value of type T
    let pg_pool = Data::new(pg_pool);
    let email_client = Data::new(email_client);

    // Actix-web runtime that have multiple threads
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default()) // logger middleware
            .route("/health", web::get().to(check_health))
            .route("/subscriptions", web::post().to(subscribe))
            // Application Context, that store state of application
            .app_data(pg_pool.clone())
            .app_data(email_client.clone())
    })
    .listen(listener)?
    .run();

    Ok((server, port))
}

pub async fn run_until_terminated(server: Server) -> Result<(), std::io::Error> {
    server.await
}

// Use Pool to handle queue of connections rather than single connection like PgConnection
// Allow to work with multithreading actix-web runtime
// Use lazy connect to connect to database when needed
pub fn get_pg_pool(database_config: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        // Limit connection timeout to avoid long wait times
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(database_config.get_pg_database_options())
}

fn get_email_client(email_client_config: EmailClientSettings) -> EmailClient {
    EmailClient::new(
        email_client_config.api_base_url,
        SubscriberEmail::parse(email_client_config.sender_email)
            .expect("Failed to parse sender email"),
        email_client_config.auth_header,
        email_client_config.auth_token,
        email_client_config.request_timeout_millis,
    )
}

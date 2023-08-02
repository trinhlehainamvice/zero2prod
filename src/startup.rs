use crate::configuration::{DatabaseSettings, EmailClientSettings, Settings};
use crate::email_client::EmailClient;
use crate::routes::{
    check_health, home, login, login_form, publish_newsletter, subscriptions, SubscriberEmail,
};
use actix_web::cookie::Key;
use actix_web::dev::Server;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use actix_web_flash_messages::storage::CookieMessageStore;
use actix_web_flash_messages::FlashMessagesFramework;
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(pg_pool: PgPool, settings: Settings) -> Result<Self, std::io::Error> {
        let listener =
            TcpListener::bind(settings.application.get_url()).expect("Failed to bind address");

        let port = listener.local_addr().unwrap().port();

        let email_client = get_email_client(settings.email_client);
        // So to share data between threads, actix-web provide web::Data<T>(Arc<T>)
        // which is a thread-safe reference counting pointer to a value of type T
        let pg_pool = Data::new(pg_pool);
        let email_client = Data::new(email_client);
        let app_base_url = Data::new(settings.application.base_url);

        let key = Key::from(settings.application.hmac_secret.expose_secret().as_bytes());
        let message_store = CookieMessageStore::builder(key).build();
        let message_framework = FlashMessagesFramework::builder(message_store).build();

        // Actix-web runtime that have multiple threads
        let server = HttpServer::new(move || {
            App::new()
                .wrap(TracingLogger::default()) // logger middleware
                .wrap(message_framework.clone())
                .route("/", web::get().to(home))
                .route("/login", web::get().to(login_form))
                .route("/login", web::post().to(login))
                .route("/health", web::get().to(check_health))
                .route("/subscriptions", web::post().to(subscriptions::subscribe))
                .route(
                    "/subscriptions/confirm",
                    web::get().to(subscriptions::confirm),
                )
                .route("/newsletters", web::post().to(publish_newsletter))
                // Application Context, that store state of application
                .app_data(pg_pool.clone())
                .app_data(email_client.clone())
                .app_data(app_base_url.clone())
        })
        .listen(listener)?
        .run();

        Ok(Self { server, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_terminated(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// Use Pool to handle queue of connections rather than single connection like PgConnection
// Allow to work with multithreading actix-web runtime
// Use lazy connect to connect to database when needed
pub fn get_pg_pool(database_config: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        // Limit connection timeout to avoid long wait times
        .acquire_timeout(std::time::Duration::from_secs(
            database_config.query_timeout_secs,
        ))
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

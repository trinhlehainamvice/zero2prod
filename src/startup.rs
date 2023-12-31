use crate::authentication::reject_anonymous_users;
use crate::configuration::{DatabaseSettings, EmailClientSettings, Settings};
use crate::email_client::EmailClient;
use crate::routes::{admin, check_health, home, login, login_form, subscriptions, SubscriberEmail};
use actix_session::storage::RedisSessionStore;
use actix_session::SessionMiddleware;
use actix_web::cookie::Key;
use actix_web::dev::Server;
use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use actix_web_flash_messages::storage::CookieMessageStore;
use actix_web_flash_messages::FlashMessagesFramework;
use actix_web_lab::middleware;
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::net::TcpListener;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing_actix_web::TracingLogger;

pub struct ApplicationBuilder {
    settings: Settings,
    notify: Arc<Notify>,
    pg_pool: Option<PgPool>,
}

impl ApplicationBuilder {
    fn new(settings: Settings, notify: Arc<Notify>) -> Self {
        Self {
            settings,
            notify,
            pg_pool: None,
        }
    }

    pub fn set_pg_pool(mut self, pg_pool: PgPool) -> Self {
        self.pg_pool = Some(pg_pool);
        self
    }

    pub async fn build(self) -> Result<Application, anyhow::Error> {
        let listener = TcpListener::bind(self.settings.application.get_url())?;

        let port = listener.local_addr().unwrap().port();

        let email_client = build_email_client(self.settings.email_client.clone())?;
        // So to share data between threads, actix-web provide web::Data<T>(Arc<T>)
        // which is a thread-safe reference counting pointer to a value of type T
        let pg_pool = Data::new(match self.pg_pool {
            Some(pool) => pool,
            None => get_pg_pool(&self.settings.database),
        });
        let email_client = Data::new(email_client);
        let app_base_url = Data::new(self.settings.application.base_url.clone());

        let message_key = Key::from(
            self.settings
                .application
                .flash_msg_key
                .expose_secret()
                .as_bytes(),
        );
        let message_store = CookieMessageStore::builder(message_key).build();
        let message_framework = FlashMessagesFramework::builder(message_store).build();

        let session_key = Key::from(
            self.settings
                .application
                .redis_session_key
                .expose_secret()
                .as_bytes(),
        );
        let session_store =
            RedisSessionStore::builder(self.settings.application.redis_url.expose_secret())
                .build()
                .await
                .expect("Failed to build RedisSessionStore");

        let notify = Data::from(self.notify);

        // Actix-web runtime that have multiple threads
        let server = HttpServer::new(move || {
            App::new()
                .wrap(TracingLogger::default()) // logger middleware
                .wrap(message_framework.clone())
                .wrap(SessionMiddleware::new(
                    session_store.clone(),
                    session_key.clone(),
                ))
                .route("/", web::get().to(home))
                .route("/login", web::get().to(login_form))
                .route("/login", web::post().to(login))
                .route("/health", web::get().to(check_health))
                .route("/subscriptions", web::post().to(subscriptions::subscribe))
                .route(
                    "/subscriptions/confirm",
                    web::get().to(subscriptions::confirm),
                )
                .service(
                    web::scope("/admin")
                        .wrap(middleware::from_fn(reject_anonymous_users))
                        .route("/dashboard", web::get().to(admin::admin_dashboard))
                        .route("/newsletters", web::get().to(admin::get_newsletters_form))
                        .route("/newsletters", web::post().to(admin::publish_newsletters))
                        .route("/logout", web::get().to(admin::logout))
                        .route("/password", web::get().to(admin::change_password_form))
                        .route("/password", web::post().to(admin::change_password))
                        .app_data(notify.clone()),
                )
                // Application Context, that store state of application
                .app_data(pg_pool.clone())
                .app_data(email_client.clone())
                .app_data(app_base_url.clone())
        })
        .listen(listener)?
        .run();

        Ok(Application { server, port })
    }
}

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub fn builder(settings: Settings, notify: Arc<Notify>) -> ApplicationBuilder {
        ApplicationBuilder::new(settings, notify)
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

pub fn build_email_client(
    email_client_config: EmailClientSettings,
) -> Result<EmailClient, anyhow::Error> {
    EmailClient::new(
        email_client_config.host,
        SubscriberEmail::parse(email_client_config.sender_email).map_err(|e| anyhow::anyhow!(e))?,
        email_client_config.username,
        email_client_config.password,
        email_client_config.port,
        email_client_config.require_tls,
        email_client_config.request_timeout_millis,
    )
}

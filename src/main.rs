use sqlx::PgPool;
use std::net::TcpListener;
use zero2prod::configuration::Settings;
use zero2prod::startup::run;

#[tokio::main]
async fn main() -> std::io::Result<()> {
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

use once_cell::sync::Lazy;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::configuration::{DatabaseSettings, Settings};
use zero2prod::startup::Application;
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

pub struct TestApp {
    pub addr: String,
    pub db_connection_pool: PgPool,
    pub email_client: MockServer,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(&format!("{}/subscriptions", self.addr))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }
}

static TRACING: Lazy<()> = Lazy::new(|| {
    let test_name = "test";
    let default_log_level = "debug";
    if std::env::var("TEST_LOG").is_ok() {
        init_tracing_subscriber(get_tracing_subscriber(
            test_name,
            default_log_level,
            std::io::stdout,
        ));
    } else {
        init_tracing_subscriber(get_tracing_subscriber(
            test_name,
            default_log_level,
            std::io::sink,
        ));
    }
});

pub async fn spawn_app() -> std::io::Result<TestApp> {
    // Lazy mean only run when it is called
    // once_cell make sure it is only run once on entire program lifetime
    Lazy::force(&TRACING);

    let email_client = MockServer::start().await;

    let settings = {
        let mut settings = Settings::get_configuration().expect("Failed to read configuration");

        // Use port 0 to ask the OS to pick a random free port
        settings.application.port = 0;
        // Use mock server to as email server for testing
        settings.email_client.api_base_url = email_client.uri();
        settings
    };

    let db_connection_pool = get_test_database(&settings.database).await;
    let app = Application::build(db_connection_pool.clone(), settings)
        .await
        .expect("Failed to build Server");

    let addr = format!("http://127.0.0.1:{}", app.port());

    // tokio spawn background thread an run app
    // We want to hold thread instance until tests finish (or end of tokio::test)
    // tokio::test manage background threads and terminate them when tests finish
    tokio::spawn(app.run_until_terminated());

    Ok(TestApp {
        addr,
        db_connection_pool,
        email_client,
    })
}

// Test will cause unexpected result if do same test multiple times to the same database
// So we need to create a branch new test database for each test for isolation
// Need to manually clean up test database
async fn get_test_database(database: &DatabaseSettings) -> PgPool {
    let database_name = Uuid::new_v4().to_string();

    let mut pg_options = database.get_pg_options();
    // Create test database
    let mut connection = PgConnection::connect_with(&pg_options)
        .await
        .expect("Failed to connect to Postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, database_name).as_str())
        .await
        .expect("Failed to create database");

    pg_options = pg_options.database(&database_name);

    // Migrate database
    let connection_pool = PgPool::connect_with(pg_options)
        .await
        .expect("Failed to connect to Postgres");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}

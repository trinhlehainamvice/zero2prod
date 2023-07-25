use once_cell::sync::Lazy;
use secrecy::ExposeSecret;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use uuid::Uuid;
use zero2prod::configuration::{DatabaseSettings, Settings};
use zero2prod::startup::run;
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

#[tokio::test]
async fn check_health_check() {
    // Arrange
    let TestApp { addr, .. } = spawn_app().await.unwrap();

    // Act
    let response = reqwest::Client::new()
        .get(&format!("{}/health", addr))
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
} // _app_thread is dropped here after all tests are successful

#[tokio::test]
async fn test_200_success_post_subscribe_in_urlencoded_format() {
    // Arrange
    let TestApp { addr, .. } = spawn_app().await.unwrap();

    // Act
    let body = "name=Foo%20Bar&email=foobar%40example.com";
    let response = reqwest::Client::new()
        .post(&format!("{}/subscriptions", addr))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_400_fail_post_subscribe_in_urlencoded_format_when_missing_data() {
    // Arrange
    let TestApp { addr, .. } = spawn_app().await.unwrap();
    let test_cases = vec![
        ("email=foobar%40example.com", "Missing the name"),
        ("name=Foo%20Bar", "Missing the email"),
        ("", "Missing both name and email aka data form is empty"),
    ];

    // Act
    let req_builder = reqwest::Client::new()
        .post(&format!("{}/subscriptions", addr))
        .header("Content-Type", "application/x-www-form-urlencoded");
    for (body, error) in test_cases {
        let response = req_builder
            .try_clone()
            .unwrap()
            .body(body)
            .send()
            .await
            .expect("Failed to execute request");

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail 400 Bad Request with payload {}",
            error
        );
    }
}

#[tokio::test]
async fn test_200_success_connect_to_database_and_subscribe_valid_data_in_urlencoded_format() {
    // Arrange
    let TestApp { addr, .. } = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    // Act
    let response = reqwest::Client::new()
        .post(&format!("{}/subscriptions", addr))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_query_subscriptions_name_from_database() {
    // Arrange
    let TestApp {
        addr,
        db_connection_pool,
    } = spawn_app().await.unwrap();
    let body = "name=Foo%20Bar&email=foobar%40example.com";

    // Act
    let response = reqwest::Client::new()
        .post(&format!("{}/subscriptions", addr))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request");

    // Assert
    assert!(response.status().is_success());

    // Act
    let subscriber = sqlx::query!("SELECT email, name FROM subscriptions")
        .fetch_one(&db_connection_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    // Assert
    assert_eq!("foobar@example.com", subscriber.email);
    assert_eq!("Foo Bar", subscriber.name);
}

struct TestApp {
    addr: String,
    db_connection_pool: PgPool,
}

static TRACING: Lazy<()> = Lazy::new(|| {
    let test_name = "test".to_string();
    let default_log_level = "debug".to_string();
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

async fn spawn_app() -> std::io::Result<TestApp> {
    // Lazy mean only run when it is called
    // once_cell make sure it is only run once on entire program lifetime
    Lazy::force(&TRACING);

    // Use port 0 to ask the OS to pick a random free port
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    // Then query allocated port by local_addr
    let addr = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());

    let config = Settings::get_configuration().expect("Failed to read configuration");
    let db_connection_pool = get_test_database(&config.database).await;
    let app = run(listener, db_connection_pool.clone()).expect("Failed to bind address");

    // tokio spawn background thread an run app
    // We want to hold thread instance until tests finish (or end of tokio::test)
    // tokio::test manage background threads and terminate them when tests finish
    tokio::spawn(app);

    Ok(TestApp {
        addr,
        db_connection_pool,
    })
}

// Test will cause unexpected result if do same test multiple times to the same database
// So we need to create a branch new test database for each test for isolation
// Need to manually clean up test database
async fn get_test_database(database: &DatabaseSettings) -> PgPool {
    let database_name = Uuid::new_v4().to_string();

    // Create test database
    let mut connection = PgConnection::connect(&database.get_base_url())
        .await
        .expect("Failed to connect to Postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, database_name).as_str())
        .await
        .expect("Failed to create database");

    let url = format!(
        "{}://{}:{}@{}:{}/{}",
        database.protocol,
        database.username,
        database.password.expose_secret(),
        database.host,
        database.port,
        database_name
    );

    // Migrate database
    let connection_pool = PgPool::connect(&url)
        .await
        .expect("Failed to connect to Postgres");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}

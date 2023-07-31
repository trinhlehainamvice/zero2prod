use argon2::password_hash::SaltString;
use argon2::{Algorithm, Params, PasswordHasher, Version};
use fake::faker::internet::en::Password;
use fake::faker::name::en::Name;
use fake::Fake;
use once_cell::sync::Lazy;
use rand::rngs::OsRng;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zero2prod::configuration::{DatabaseSettings, Settings};
use zero2prod::startup::Application;
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

pub struct TestApp {
    pub addr: String,
    pub port: u16,
    pub pg_pool: PgPool,
    pub email_client: MockServer,
    pub test_user: TestUser,
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

    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(&format!("{}/newsletters", self.addr))
            .json(&body)
            .basic_auth(
                self.test_user.username.clone(),
                Some(self.test_user.password.clone()),
            )
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn create_unconfirmed_subscriber(&self, body: &str) -> ConfirmationLinks {
        // Arrange
        let _scoped_mock = Mock::given(path("/email"))
            .and(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount_as_scoped(&self.email_client)
            .await;

        // Act
        self.post_subscriptions(body.into()).await;
        let email_request = &self.email_client.received_requests().await.unwrap()[0];

        ConfirmationLinks::get_confirmation_link(email_request)

        // Assert when scoped_mock drop
    }

    pub async fn create_confirmed_subscriber(&self, body: &str) {
        // Arrange
        let confirmation_links = self.create_unconfirmed_subscriber(body).await;
        let mut link = reqwest::Url::parse(&confirmation_links.html).unwrap();
        link.set_port(Some(self.port)).unwrap();

        // Act
        reqwest::get(link)
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }
}

static TRACING: Lazy<()> = Lazy::new(|| {
    let test_name = "test_app";
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

    let pg_pool = get_test_database(&settings.database).await;
    let app = Application::build(pg_pool.clone(), settings)
        .await
        .expect("Failed to build Server");

    let port = app.port();
    let addr = format!("http://127.0.0.1:{}", port);

    let test_user = TestUser::generate();
    test_user.create_user(&pg_pool).await;

    // tokio spawn background thread an run app
    // We want to hold thread instance until tests finish (or end of tokio::test)
    // tokio::test manage background threads and terminate them when tests finish
    tokio::spawn(app.run_until_terminated());

    Ok(TestApp {
        addr,
        port,
        pg_pool,
        email_client,
        test_user,
    })
}

pub struct ConfirmationLinks {
    pub html: String,
    pub plain_text: String,
}

fn get_link(s: &str) -> String {
    let links: Vec<_> = linkify::LinkFinder::new()
        .links(s)
        .filter(|l| *l.kind() == linkify::LinkKind::Url)
        .collect();
    assert_eq!(links.len(), 1);
    links[0].as_str().to_owned()
}

impl ConfirmationLinks {
    pub fn get_confirmation_link(req: &wiremock::Request) -> Self {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        let html = get_link(body["HtmlBody"].as_str().unwrap());
        let plain_text = get_link(body["TextBody"].as_str().unwrap());
        Self { html, plain_text }
    }
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

pub struct TestUser {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Name().fake(),
            password: Password(8..20).fake(),
        }
    }

    pub async fn create_user(&self, pg_pool: &PgPool) {
        let salt = SaltString::generate(&mut OsRng);
        let params = Params::new(15000, 2, 1, None).expect("Fail to create Argon Params");
        let hash = argon2::Argon2::new(Algorithm::Argon2d, Version::V0x13, params);
        let password_hash = hash
            .hash_password(self.password.as_bytes(), salt.as_salt())
            .expect("Failed to hash password with Argon");
        // password_hash contains array of 8 bytes generated by sha3
        // Need to convert integer to hex string
        sqlx::query!(
            r#"INSERT INTO users (user_id, username, password_hash, salt)
            VALUES ($1, $2, $3, $4)
            "#,
            self.user_id,
            self.username,
            password_hash.to_string(),
            salt.to_string()
        )
        .execute(pg_pool)
        .await
        .expect("Failed to create user to test database");
    }
}

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Params, PasswordHasher, Version};
use fake::faker::internet::en::SafeEmail;
use fake::faker::name::en::Name;
use fake::Fake;
use once_cell::sync::Lazy;
use rand::rngs::OsRng;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::sync::Arc;
use tokio::sync::Notify;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zero2prod::configuration::{DatabaseSettings, Settings};
use zero2prod::email_client::EmailClient;
use zero2prod::newsletters_issues::{build_worker, try_execute_task, ExecutionResult};
use zero2prod::startup::{get_email_client, Application};
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

pub struct TestApp {
    pub client: reqwest::Client,
    pub addr: String,
    pub port: u16,
    pub pg_pool: PgPool,
    pub email_server: MockServer,
    pub email_client: EmailClient,
    pub test_user: TestUser,
}

impl TestApp {
    pub async fn send_remaining_emails(&self) -> anyhow::Result<()> {
        loop {
            if let ExecutionResult::EmptyQueue =
                try_execute_task(&self.pg_pool, &self.email_client).await?
            {
                return Ok(());
            }
        }
    }

    pub async fn login(&self) -> reqwest::Response {
        self.post_login(serde_json::json!({
            "username": &self.test_user.username,
            "password": &self.test_user.password
        }))
        .await
    }

    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        self.client
            .post(&format!("{}/subscriptions", self.addr))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(&format!("{}{}", self.addr, path))
            .send()
            .await
            .unwrap()
    }

    pub async fn get_html(&self, path: &str) -> String {
        self.client
            .get(&format!("{}{}", self.addr, path))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    }

    pub async fn post_newsletters(&self, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .post(&format!("{}/admin/newsletters", self.addr))
            .form(&body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub async fn post_login(&self, login_form: serde_json::Value) -> reqwest::Response {
        self.client
            .post(&format!("{}/login", self.addr))
            .form(&login_form)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_form(&self, path: &str, form: serde_json::Value) -> reqwest::Response {
        self.client
            .post(&format!("{}{}", self.addr, path))
            .form(&form)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn get_login_html(&self) -> String {
        self.client
            .get(&format!("{}/login", self.addr))
            .send()
            .await
            .expect("Failed to execute request.")
            .text()
            .await
            .expect("Failed to read response body.")
    }

    pub async fn create_unconfirmed_subscriber(&self, body: &str) -> ConfirmationLinks {
        // Arrange
        let _scoped_mock = Mock::given(path("/email"))
            .and(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount_as_scoped(&self.email_server)
            .await;

        // Act
        self.post_subscriptions(body.into()).await;
        // Because Mock Server Instance stack ups all incoming requests
        let requests = self.email_server.received_requests().await.unwrap();
        // Need to get the last request in received_requests (latest one) from Mock Server
        let email_request = requests.last().unwrap();

        ConfirmationLinks::get_confirmation_link(email_request)

        // Assert when scoped_mock drop
    }

    pub async fn create_confirmed_subscriber(&self, body: &str) {
        // Arrange
        let confirmation_links = self.create_unconfirmed_subscriber(body).await;
        let mut link = reqwest::Url::parse(&confirmation_links.html).unwrap();
        link.set_port(Some(self.port)).unwrap();

        // Act
        reqwest::Client::new()
            .get(link)
            .send()
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

    let email_server = MockServer::start().await;

    let settings = {
        let mut settings = Settings::get_configuration().expect("Failed to read configuration");

        // Use port 0 to ask the OS to pick a random free port
        settings.application.port = 0;
        // Use mock server to as email server for testing
        settings.email_client.api_base_url = email_server.uri();
        settings
    };

    let notify = Arc::new(Notify::new());
    let email_client = get_email_client(settings.email_client.clone());
    let pg_pool = get_test_database(&settings.database).await;
    let app = Application::builder(settings.clone(), notify.clone())
        .set_pg_pool(pg_pool.clone())
        .build()
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
    tokio::spawn(
        build_worker(settings, notify)
            .set_pg_pool(pg_pool.clone())
            .run_worker_until_stopped(),
    );

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    Ok(TestApp {
        client,
        addr,
        port,
        pg_pool,
        email_server,
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
            // password: Password(8..20).fake(),
            password: "4ll0v3f0rR_$t".to_string(),
        }
    }

    pub async fn create_user(&self, pg_pool: &PgPool) {
        let salt = SaltString::generate(&mut OsRng);
        let params = Params::new(15000, 2, 1, None).expect("Fail to create Argon Params");
        let hasher = argon2::Argon2::new(Algorithm::Argon2d, Version::V0x13, params);
        let password_hash = hasher
            .hash_password(self.password.as_bytes(), salt.as_salt())
            .expect("Failed to hash password into PCH format");
        // password_hash contains array of 8 bytes generated by sha3
        // Need to convert integer to hex string
        sqlx::query!(
            r#"INSERT INTO users (user_id, username, password_hash)
            VALUES ($1, $2, $3)
            "#,
            self.user_id,
            self.username,
            password_hash.to_string(),
        )
        .execute(pg_pool)
        .await
        .expect("Failed to create user to test database");
    }
}

pub fn assert_redirects_to(response: &reqwest::Response, location: &str) {
    assert_eq!(response.status().as_u16(), 303);
    assert_eq!(response.headers().get("location").unwrap(), location);
}

pub async fn create_confirmed_subscriber(app: &TestApp) {
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_urlencoded::to_string(serde_json::json!({
        "name": name,
        "email": email
    }))
    .expect("Failed to subscriber json form to urlencoded");

    app.create_confirmed_subscriber(&body).await;
}

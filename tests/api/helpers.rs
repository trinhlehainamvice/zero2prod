use argon2::password_hash::SaltString;
use argon2::{Algorithm, Params, PasswordHasher, Version};
use fake::faker::internet::en::SafeEmail;
use fake::faker::name::en::Name;
use fake::Fake;
use once_cell::sync::Lazy;
use rand::rngs::OsRng;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use uuid::Uuid;
use zero2prod::configuration::{DatabaseSettings, Settings};
use zero2prod::email_client::EmailClient;
use zero2prod::newsletters_issues::{
    DeleteExpiredIdempotencyWorker, NewslettersIssuesDeliveryWorker,
};
use zero2prod::startup::{build_email_client, Application};
use zero2prod::telemetry::{get_tracing_subscriber, init_tracing_subscriber};

#[cfg(not(feature = "pool"))]
pub struct TestApp {
    pub client: reqwest::Client,
    pub addr: String,
    pub port: u16,
    pub pg_pool: PgPool,
    pub email_client: EmailClient,
    pub test_user: TestUser,
}

impl TestApp {
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

    pub async fn wait_until_completed_newsletters_issue_count_matches(&self, n_issues: usize) {
        loop {
            let completed_n_issues = sqlx::query!(
                r#"
                SELECT COUNT(*)
                FROM newsletters_issues
                WHERE status = 'COMPLETED'
                "#,
            )
            .fetch_one(&self.pg_pool)
            .await
            .expect("Failed to fetch number of completed newsletters_issues")
            .count
            .expect("Expect number of completed newsletters_issues");

            if completed_n_issues == n_issues as i64 {
                break;
            }

            tokio::time::sleep(Duration::from_millis(10)).await
        }
    }

    pub async fn get_email_messages_json(&self) -> serde_json::Value {
        let response = reqwest::Client::new()
            .get("http://localhost:1080/api/messages")
            .send()
            .await
            .expect("Fail to get email messages");

        assert_eq!(response.status().as_u16(), 200);

        response.json().await.expect("Fail to parse email messages")
    }

    pub async fn get_confirmation_links(&self, email: &str) -> ConfirmationLinks {
        let messages = self.get_email_messages_json().await;

        let message_id = messages
            .as_array()
            .unwrap()
            .iter()
            .find(|msg| {
                msg["from"]["email"].as_str() == Some(self.email_client.sender_email())
                    && msg["to"][0]["email"].as_str() == Some(email)
            })
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://localhost:1080/api/message/{}", message_id))
            .send()
            .await
            .expect("Fail to get confirm email message");
        assert_eq!(response.status().as_u16(), 200);

        let message_json: serde_json::Value = response
            .json()
            .await
            .expect("Fail to parse confirm email message to json");

        ConfirmationLinks::get_confirmation_links(message_json)
    }

    pub async fn click_confirmation_link(&self, confirmation_links: &ConfirmationLinks) {
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

    pub async fn create_confirmed_subscriber(&self, body: serde_json::Value) {
        // Arrange
        let urlencoded_body = serde_urlencoded::to_string(&body).unwrap();
        self.post_subscriptions(urlencoded_body).await;

        let confirmation_links = self
            .get_confirmation_links(body["email"].as_str().unwrap())
            .await;

        self.click_confirmation_link(&confirmation_links).await;
    }
}

impl TestApp {
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder::default()
    }
}

#[derive(Default, Clone)]
pub struct TestAppBuilder {
    spawn_newsletters_issues_delivery_worker: bool,
    spawn_delete_expired_idempotency_worker: bool,
    idempotency_expiration_time_millis: Option<u64>,
}

impl TestAppBuilder {
    pub fn spawn_newsletters_issues_delivery_worker(mut self) -> Self {
        self.spawn_newsletters_issues_delivery_worker = true;
        self
    }

    pub fn spawn_delete_expired_idempotency_worker(mut self) -> Self {
        self.spawn_delete_expired_idempotency_worker = true;
        self
    }

    pub fn idempotency_expiration_time_millis(mut self, time_millis: u64) -> Self {
        self.idempotency_expiration_time_millis = Some(time_millis);
        self
    }

    pub async fn build(self) -> anyhow::Result<TestApp> {
        // Lazy mean only run when it is called
        // once_cell make sure it is only run once on entire program lifetime
        Lazy::force(&TRACING);

        let settings = {
            let mut settings = Settings::get_configuration().expect("Failed to read configuration");

            // Use port 0 to ask the OS to pick a random free port
            settings.application.port = 0;

            if let Some(time_millis) = self.idempotency_expiration_time_millis {
                settings.application.idempotency_expiration_millis = time_millis;
            }

            // Increase uniqueness of each test case
            settings.email_client.sender_email = SafeEmail().fake();

            settings
        };

        let notify = Arc::new(Notify::new());
        let email_client = build_email_client(settings.email_client.clone())?;
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

        if self.spawn_newsletters_issues_delivery_worker {
            tokio::spawn(
                NewslettersIssuesDeliveryWorker::builder(settings.clone(), notify)
                    .set_pg_pool(pg_pool.clone())
                    .run_until_terminated(),
            );
        }
        if self.spawn_delete_expired_idempotency_worker {
            tokio::spawn(
                DeleteExpiredIdempotencyWorker::builder(settings)
                    .set_pg_pool(pg_pool.clone())
                    .run_until_terminated(),
            );
        }

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
            email_client,
            test_user,
        })
    }
}

static TRACING: Lazy<()> = Lazy::new(|| {
    const TEST_NAME: &str = "test_app";
    const DEFAULT_LOG_LEVEL: &str = "debug";
    if std::env::var("TEST_LOG").is_ok() {
        init_tracing_subscriber(get_tracing_subscriber(
            TEST_NAME,
            DEFAULT_LOG_LEVEL,
            std::io::stdout,
        ));
    } else {
        init_tracing_subscriber(get_tracing_subscriber(
            TEST_NAME,
            DEFAULT_LOG_LEVEL,
            std::io::sink,
        ));
    }
});

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
    pub fn get_confirmation_links(message_json: serde_json::Value) -> Self {
        let html = get_link(message_json["html"].as_str().unwrap());
        let plain_text = get_link(message_json["text"].as_str().unwrap());
        assert_eq!(html.len(), plain_text.len());
        assert_eq!(html, plain_text);
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
    let body = serde_json::json!({
        "name": name,
        "email": email
    });

    app.create_confirmed_subscriber(body).await;
}

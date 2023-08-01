use crate::email_client::EmailClient;
use crate::routes::{error_chain_fmt, SubscriberEmail};
use actix_web::http::header::HeaderMap;
use actix_web::http::{header, StatusCode};
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::Engine;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct NewsletterPayload {
    title: String,
    content: NewsletterContent,
}

#[derive(serde::Deserialize)]
pub struct NewsletterContent {
    text: String,
    html: String,
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[derive(thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
    #[error("Authorization failed.")]
    AuthFailed(#[source] anyhow::Error),
}

impl ResponseError for PublishError {
    fn status_code(&self) -> StatusCode {
        match self {
            PublishError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
            PublishError::AuthFailed(_) => StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            PublishError::Unexpected(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
            PublishError::AuthFailed(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
        }
    }
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

// Some tasks are CPU-intensive, they should be handled in separate threads to avoid blocking event loop thread
fn spawn_blocking_task_with_tracing<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // Spawn a new thread to handle this task, also new span will be created in this new thread
    // Need to pass span of thread than spawned task to block thread, for that thread can subscribe to parent span
    let current_span = tracing::Span::current();
    // Hash password algorithm consuming a lot of CPU power, may cause blocking event loop thread handle current request
    // spawn blocking task to another thread to let current event loop thread to continue to process non-blocking tasks (another requests)
    tokio::task::spawn_blocking(move || current_span.in_scope(f))
}

#[tracing::instrument(
    name = "Publish a newsletter letter",
    skip_all,
    fields(
        username = tracing::field::Empty,
        user_id = tracing::field::Empty
    )
)]
pub async fn publish_newsletter(
    web::Json(payload): web::Json<NewsletterPayload>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    request: HttpRequest,
) -> Result<HttpResponse, PublishError> {
    let credentials =
        get_credentials_from_basic_auth(request.headers()).map_err(PublishError::AuthFailed)?;
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));

    let user_id = validate_credentials(&pg_pool, credentials).await?;
    tracing::Span::current().record("user_id", tracing::field::display(&user_id));

    let confirmed_subscribers = get_confirmed_subscribers(&pg_pool).await.unwrap();
    for subscriber in confirmed_subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &payload.title,
                        &payload.content.text,
                        &payload.content.html,
                    )
                    .await
                    .with_context(|| {
                        format!("Failed to send newsletters to {}", subscriber.email)
                    })?;
            }
            Err(error) => {
                tracing::warn!(
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscriber with \
                    an invalid email address in current version"
                )
            }
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Get confirmed subscribers", skip_all)]
async fn get_confirmed_subscribers(
    pg_pool: &PgPool,
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    struct Row {
        email: String,
    }

    let confirmed_subscribers = sqlx::query_as!(
        Row,
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pg_pool)
    .await?
    .into_iter()
    // Parse confirmed email from the database again
    // Because validation will be updated or changed in new version
    .map(|Row { email }| match SubscriberEmail::parse(email) {
        Ok(email) => Ok(ConfirmedSubscriber { email }),
        Err(error) => Err(anyhow::anyhow!(error)),
    })
    .collect();

    Ok(confirmed_subscribers)
}

struct Credentials {
    username: String,
    password: Secret<String>,
}

fn get_credentials_from_basic_auth(header: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    // Get the `Authorization` header with valid UTF8 string
    let header_value = header
        .get("Authorization")
        .context("No `Authorization` header found")?
        .to_str()
        .context("`Authorization` header's value is not valid UTF8")?;

    let base64encoded_segment = header_value
        // Remove `Basic` and Padding ` ` -> `Basic `
        .strip_prefix("Basic ")
        .context("`Authorization` header's does not start with `Basic `")?;

    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to base64 decode value `Basic Authorization` header's value to bytes")?;

    let decoded_credentials =
        String::from_utf8(decoded_bytes).context("decoded bytes is not valid UTF8")?;

    let mut credentials = decoded_credentials.splitn(2, ':');
    let username = credentials
        .next()
        .context("Decoded credentials do not contain a username")?
        .to_string();
    let password = credentials
        .next()
        .context("Decoded credentials do not contain a password")?
        .to_string();

    Ok(Credentials {
        username,
        password: Secret::new(password),
    })
}

#[tracing::instrument(name = "Get credentials from database", skip_all)]
async fn get_credentials_from_database(
    pg_pool: &PgPool,
    username: &str,
) -> Result<Option<(Uuid, Secret<String>)>, anyhow::Error> {
    let credentials = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
        username
    )
    .fetch_optional(pg_pool)
    .await
    .context("Failed to fetch credentials from database")?
    .map(|row| (row.user_id, Secret::new(row.password_hash)));

    Ok(credentials)
}

#[tracing::instrument(name = "Validate credentials from database", skip_all)]
async fn validate_credentials(
    pg_pool: &PgPool,
    credentials: Credentials,
) -> Result<Uuid, PublishError> {
    const HASHED_PASSWORD_IF_INVALID_USERNAME: &str = "$argon2d$v=19$m=15000,t=2,p=1\
        $QhQyHN2/VvKTi5QYqo+VZA\
        $JkXwR/rdESxDi2DfcCf8lk2U4+ShyN3CXZATJQvP0lg";
    let mut user_id = None;
    let mut expected_password_hash = Secret::new(HASHED_PASSWORD_IF_INVALID_USERNAME.to_string());

    if let Some((stored_user_id, stored_password_hash)) =
        get_credentials_from_database(pg_pool, &credentials.username)
            .await
            .map_err(PublishError::Unexpected)?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    // Always verify password hash even if username is invalid
    // Prevent timing attack to guest valid username from database
    spawn_blocking_task_with_tracing(move || {
        verify_password_hash(credentials.password, expected_password_hash)
    })
    .await
    .context("Failed to spawn blocking task")
    .map_err(PublishError::Unexpected)??;

    // Validation is satisfied when both user_id and password hash_are valid
    user_id.ok_or_else(|| PublishError::AuthFailed(anyhow::anyhow!("Invalid username or password")))
}

#[tracing::instrument(name = "Verify password hash", skip_all)]
fn verify_password_hash(
    password: Secret<String>,
    expected_password_hash: Secret<String>,
) -> Result<(), PublishError> {
    let parsed_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .map_err(|e| PublishError::Unexpected(anyhow::anyhow!(e)))?;

    Argon2::default()
        .verify_password(password.expose_secret().as_bytes(), &parsed_hash)
        .context("Failed to verify password hash")
        .map_err(PublishError::AuthFailed)
}

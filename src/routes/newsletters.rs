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

#[tracing::instrument(name = "Publish a newsletter letter", skip_all)]
pub async fn publish_newsletter(
    web::Json(payload): web::Json<NewsletterPayload>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    request: HttpRequest,
) -> Result<HttpResponse, PublishError> {
    let credentials =
        get_credentials_from_basic_auth(request.headers()).map_err(PublishError::AuthFailed)?;
    let _user_id = validate_credentials(&pg_pool, &credentials).await?;

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
    credentials: &Credentials,
) -> Result<Uuid, PublishError> {
    let (user_id, expected_password_hash) =
        get_credentials_from_database(pg_pool, &credentials.username)
            .await
            .map_err(PublishError::Unexpected)?
            .ok_or(PublishError::AuthFailed(anyhow::anyhow!(
                "Unknown username"
            )))?;

    let parsed_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .map_err(|e| PublishError::Unexpected(anyhow::anyhow!(e)))?;

    tracing::info_span!("Verify password hash")
        .in_scope(|| {
            Argon2::default().verify_password(
                credentials.password.expose_secret().as_bytes(),
                &parsed_hash,
            )
        })
        .context("Failed to verify password hash")
        .map_err(PublishError::AuthFailed)?;

    Ok(user_id)
}

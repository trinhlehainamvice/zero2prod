use crate::authentication::{get_credentials_from_basic_auth, validate_credentials, AuthError};
use crate::email_client::EmailClient;
use crate::error_chain_fmt;
use crate::routes::SubscriberEmail;
use actix_web::http::{header, StatusCode};
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use anyhow::Context;
use reqwest::header::HeaderValue;
use sqlx::PgPool;

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
    UnexpectedError(#[from] anyhow::Error),
    #[error("Authorization failed.")]
    AuthFailed(#[source] anyhow::Error),
}

impl ResponseError for PublishError {
    fn status_code(&self) -> StatusCode {
        match self {
            PublishError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            PublishError::AuthFailed(_) => StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            PublishError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
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

    let user_id = validate_credentials(&pg_pool, credentials)
        .await
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => PublishError::AuthFailed(e.into()),
            AuthError::UnexpectedError(_) => PublishError::UnexpectedError(e.into()),
        })?;

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

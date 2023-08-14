use crate::email_client::EmailClient;
use crate::routes::domain::{NewSubscriber, SubscriberEmail, SubscriberName, SubscriptionStatus};
use crate::utils::error_chain_fmt;
use actix_web::{web, HttpResponse, ResponseError};
use anyhow::Context;
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use std::fmt::{Debug, Display, Formatter};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct NewSubscriberForm {
    name: String,
    email: String,
}

impl TryInto<NewSubscriber> for NewSubscriberForm {
    type Error = String;
    fn try_into(self) -> Result<NewSubscriber, Self::Error> {
        Ok(NewSubscriber {
            name: SubscriberName::parse(self.name)?,
            email: SubscriberEmail::parse(self.email)?,
        })
    }
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    InvalidSubscriptionForm(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            SubscribeError::InvalidSubscriptionForm(_) => actix_web::http::StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_) => {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl Debug for SubscribeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

// Instrument wrap function into a Span
// Instrument can capture arguments of function, but CAN'T capture local variables
#[tracing::instrument(
    name = "Add a new subscriber",
    skip(subscriber, pg_pool, email_client, app_base_url),
    fields(
        name = %subscriber.name,
        email = %subscriber.email,
    )
)]
pub async fn subscribe(
    web::Form(subscriber): web::Form<NewSubscriberForm>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    app_base_url: web::Data<String>,
) -> Result<HttpResponse, SubscribeError> {
    let mut transaction = pg_pool
        .begin()
        .await
        .context("Failed to begin a database transaction")?;

    let subscriber: NewSubscriber = subscriber
        .try_into()
        .map_err(SubscribeError::InvalidSubscriptionForm)?;

    let subscription_id = insert_pending_subscriber(&subscriber, &mut transaction)
        .await
        .context("Failed to insert new subscriber")?;

    let subscription_token = generate_subscription_token();
    insert_subscription_token(&subscription_id, &subscription_token, &mut transaction)
        .await
        .context("Failed to insert subscription token into database")?;

    // Use Transaction to guarantee all database queries in one request is failed or success all together
    // To avoid fault states in database
    // Usually use when there are multiple `INSERT` or `UPDATE` queries
    transaction
        .commit()
        .await
        .context("Failed to commit a database transaction")?;

    // Need to insert subscription token into database before sending confirmation email
    send_confirmation_email(
        &app_base_url,
        email_client,
        &subscriber.email,
        &subscription_token,
    )
    .await
    .context("Failed to send confirmation email")?;

    Ok(HttpResponse::Ok().finish())
}

// Separate sql query into separate function (separation of concerns)
// This function not dependent on actix-web framework
#[tracing::instrument(
    name = "Insert a new subscriber to database with pending status",
    skip(subscriber, transaction)
)]
async fn insert_pending_subscriber(
    subscriber: &NewSubscriber,
    transaction: &mut Transaction<'_, Postgres>,
) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        id,
        subscriber.email.as_ref(),
        subscriber.name.as_ref(),
        Utc::now(),
        SubscriptionStatus::Pending.as_ref()
    )
    .execute(transaction)
    .await?;

    Ok(id)
}

pub struct InsertSubscriptionError(sqlx::Error);

impl ResponseError for InsertSubscriptionError {}

impl std::error::Error for InsertSubscriptionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl Display for InsertSubscriptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to insert subscription token into database")
    }
}

impl Debug for InsertSubscriptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[tracing::instrument(
    name = "Insert new subscription token map to a subscription id into database",
    skip(subscription_id, subscription_token, transaction)
)]
async fn insert_subscription_token(
    subscription_id: &Uuid,
    subscription_token: &str,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), InsertSubscriptionError> {
    sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_id, subscription_token)
        VALUES ($1, $2)
        "#,
        subscription_id,
        subscription_token
    )
    .execute(transaction)
    .await
    .map_err(InsertSubscriptionError)?;

    Ok(())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(app_base_url, email_client, subscriber_email, subscription_token)
)]
async fn send_confirmation_email(
    app_base_url: &str,
    email_client: web::Data<EmailClient>,
    subscriber_email: &SubscriberEmail,
    subscription_token: &str,
) -> Result<(), anyhow::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        app_base_url, subscription_token
    );
    // TODO: make better form
    let subject = "Confirmation";
    let html_body = format!(
        "<p>\
        Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.\
        </p>",
        confirmation_link,
    );
    let text_body = format!(
        "Welcome to our newsletter!\nGo to this link: {} to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_multipart_email(subscriber_email, subject, &text_body, &html_body)
        .await?;

    Ok(())
}

// Generate Alphanumeric (A-Z, a-z, 0-9) 25-characters-long case-sensitive subscriptions token
fn generate_subscription_token() -> String {
    let mut rng = rand::thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

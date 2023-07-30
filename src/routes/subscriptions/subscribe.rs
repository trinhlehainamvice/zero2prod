use crate::email_client::EmailClient;
use crate::routes::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
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

pub enum SubscribeError {
    Validation(String),
    TransactionBegin(sqlx::Error),
    InsertPendingSubscriber(sqlx::Error),
    InsertSubscriptionToken(InsertSubscriptionError),
    TransactionCommit(sqlx::Error),
    SendConfirmationEmail(reqwest::Error),
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::Validation(_) => StatusCode::BAD_REQUEST,
            SubscribeError::InsertSubscriptionToken(_)
            | SubscribeError::TransactionBegin(_)
            | SubscribeError::SendConfirmationEmail(_)
            | SubscribeError::InsertPendingSubscriber(_)
            | SubscribeError::TransactionCommit(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Display for SubscribeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SubscribeError::Validation(_) => write!(f, "Invalid subscription form"),
            SubscribeError::TransactionBegin(_) => {
                write!(f, "Failed to begin a transaction")
            }
            SubscribeError::InsertSubscriptionToken(_) => {
                write!(f, "Failed to insert subscription token")
            }
            SubscribeError::SendConfirmationEmail(_) => {
                write!(f, "Failed to send confirmation email")
            }
            SubscribeError::InsertPendingSubscriber(_) => {
                write!(f, "Failed to insert new pending subscriber")
            }
            SubscribeError::TransactionCommit(_) => {
                write!(f, "Failed to commit a transaction")
            }
        }
    }
}

impl Debug for SubscribeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::error::Error for SubscribeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SubscribeError::Validation(_) => None,
            SubscribeError::TransactionBegin(e) => Some(e),
            SubscribeError::InsertSubscriptionToken(e) => Some(e),
            SubscribeError::InsertPendingSubscriber(e) => Some(e),
            SubscribeError::TransactionCommit(e) => Some(e),
            SubscribeError::SendConfirmationEmail(e) => Some(e),
        }
    }
}

impl From<String> for SubscribeError {
    fn from(e: String) -> Self {
        SubscribeError::Validation(e)
    }
}

impl From<InsertSubscriptionError> for SubscribeError {
    fn from(e: InsertSubscriptionError) -> Self {
        SubscribeError::InsertSubscriptionToken(e)
    }
}

impl From<reqwest::Error> for SubscribeError {
    fn from(e: reqwest::Error) -> Self {
        SubscribeError::SendConfirmationEmail(e)
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
        .map_err(SubscribeError::TransactionBegin)?;

    let subscriber: NewSubscriber = subscriber.try_into()?;

    let subscription_id = insert_pending_subscriber(&subscriber, &mut transaction)
        .await
        .map_err(SubscribeError::InsertPendingSubscriber)?;

    let subscription_token = generate_subscription_token();
    insert_subscription_token(&subscription_id, &subscription_token, &mut transaction)
        .await
        .map_err(SubscribeError::InsertSubscriptionToken)?;

    // Use Transaction to guarantee all database queries in one request is failed or success all together
    // To avoid fault states in database
    // Usually use when there are multiple `INSERT` or `UPDATE` queries
    transaction
        .commit()
        .await
        .map_err(SubscribeError::TransactionCommit)?;

    // Need to insert subscription token into database before sending confirmation email
    send_confirmation_email(
        &app_base_url,
        email_client,
        &subscriber.email,
        &subscription_token,
    )
    .await?;

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
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        id,
        subscriber.email.as_ref(),
        subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(transaction)
    .await?;

    Ok(id)
}

pub struct InsertSubscriptionError(sqlx::Error);

impl ResponseError for InsertSubscriptionError {}

fn error_chain_fmt(e: &impl std::error::Error, f: &mut Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

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
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        app_base_url, subscription_token
    );
    // TODO: make better form
    let subject = "Confirmation";
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link,
    );
    let text_body = format!(
        "Welcome to our newsletter!\nGo to this link: {} to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(subscriber_email, subject, &text_body, &html_body)
        .await
}

// Generate Alphanumeric (A-Z, a-z, 0-9) 25-characters-long case-sensitive subscriptions token
fn generate_subscription_token() -> String {
    let mut rng = rand::thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

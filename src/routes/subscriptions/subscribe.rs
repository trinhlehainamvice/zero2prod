use crate::email_client::EmailClient;
use crate::routes::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use actix_web::{web, HttpResponse, Responder};
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
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
) -> impl Responder {
    let mut transaction = match pg_pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber: NewSubscriber = match subscriber.try_into() {
        Ok(subscriber) => subscriber,
        // TODO: handle better error
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let subscription_id = match insert_pending_subscriber(&subscriber, &mut transaction).await {
        Ok(id) => id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscription_token = generate_subscription_token();
    if insert_subscription_token(&subscription_id, &subscription_token, &mut transaction)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    // Use Transaction to guarantee all database queries in one request is failed or success all together
    // To avoid fault states in database
    // Usually use when there are multiple `INSERT` or `UPDATE` queries
    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    // Need to insert subscription token into database before sending confirmation email
    match send_confirmation_email(
        &app_base_url,
        email_client,
        &subscriber.email,
        &subscription_token,
    )
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(id)
}

#[tracing::instrument(
    name = "Insert new subscription token map to a subscription id into database",
    skip(subscription_id, subscription_token, transaction)
)]
async fn insert_subscription_token(
    subscription_id: &Uuid,
    subscription_token: &str,
    transaction: &mut Transaction<'_, Postgres>,
) -> sqlx::Result<()> {
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
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

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

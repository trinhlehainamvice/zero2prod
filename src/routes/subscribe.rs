use crate::email_client::EmailClient;
use crate::routes::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use actix_web::{web, HttpResponse, Responder};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct NewSubscriberForm {
    name: String,
    email: String,
}

// Instrument wrap function into a Span
// Instrument can capture arguments of function, but CAN'T capture local variables
#[tracing::instrument(
    name = "Add a new subscriber",
    skip(subscriber, pg_pool, email_client),
    fields(
        name = %subscriber.name,
        email = %subscriber.email,
    )
)]
pub async fn subscribe(
    web::Form(subscriber): web::Form<NewSubscriberForm>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> impl Responder {
    let subscriber: NewSubscriber = match subscriber.try_into() {
        Ok(subscriber) => subscriber,
        // TODO: handle better error
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    if send_confirmation_email(email_client, &subscriber.email)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    match insert_subscriber(&subscriber, &pg_pool).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

// Separate sql query into separate function (separation of concerns)
// This function not dependent on actix-web framework
#[tracing::instrument(
    name = "Inserting a new subscriber to database"
    skip(subscriber, pg_pool)
)]
async fn insert_subscriber(subscriber: &NewSubscriber, pg_pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'confirmed')
        "#,
        Uuid::new_v4(),
        subscriber.email.as_ref(),
        subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(pg_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}

async fn send_confirmation_email(
    email_client: web::Data<EmailClient>,
    subscriber_email: &SubscriberEmail,
) -> Result<(), reqwest::Error> {
    let confirmation_link = "http://localhost:3000/subscribe/confirm?email=";
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

impl TryInto<NewSubscriber> for NewSubscriberForm {
    type Error = String;
    fn try_into(self) -> Result<NewSubscriber, Self::Error> {
        Ok(NewSubscriber {
            name: SubscriberName::parse(self.name)?,
            email: SubscriberEmail::parse(self.email)?,
        })
    }
}

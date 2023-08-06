use crate::authentication::UserId;
use crate::email_client::EmailClient;
use crate::idempotency::{
    get_idempotency_response_record_from_database, insert_idempotency_response_record_into_database,
};
use crate::routes::SubscriberEmail;
use crate::utils::{e400, e500, see_other};
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct NewsletterPayload {
    title: String,
    text_content: String,
    html_content: String,
    idempotency_key: String,
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(
    name = "Publish a newsletter letter",
    skip_all,
    fields(
        username = tracing::field::Empty,
        user_id = tracing::field::Empty
    )
)]
pub async fn publish_newsletters(
    web::Form(NewsletterPayload {
        title,
        text_content,
        html_content,
        idempotency_key,
    }): web::Form<NewsletterPayload>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let idempotency_key = idempotency_key.try_into().map_err(e400)?;
    let user_id = user_id.into_inner();
    if let Some(saved_response) =
        get_idempotency_response_record_from_database(&pg_pool, &idempotency_key, &user_id)
            .await
            .map_err(e500)?
    {
        return Ok(saved_response);
    }

    let confirmed_subscribers = get_confirmed_subscribers(&pg_pool)
        .await
        .context("Failed to fetch confirmed subscribers.")
        .map_err(e500)?;

    for subscriber in confirmed_subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(&subscriber.email, &title, &text_content, &html_content)
                    .await
                    .with_context(|| format!("Failed to send newsletters to {}", subscriber.email))
                    .map_err(e500)?;
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

    FlashMessage::success("Published newsletter successfully!").send();
    let response = see_other("/admin/newsletters");
    let response = insert_idempotency_response_record_into_database(
        &pg_pool,
        &idempotency_key,
        &user_id,
        response,
    )
    .await
    .map_err(e500)?;
    Ok(response)
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
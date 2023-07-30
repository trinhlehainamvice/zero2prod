use crate::email_client::EmailClient;
use crate::routes::{error_chain_fmt, SubscriberEmail};
use actix_web::{web, HttpResponse, Responder};
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
enum PublishError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
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
) -> impl Responder {
    HttpResponse::Ok().finish()
}

#[tracing::instrument(name = "Get confirmed subscribers", skip_all)]
async fn get_confirmed_subscribers(pg_pool: &PgPool) -> Result<(), sqlx::Error> {
    todo!()
}

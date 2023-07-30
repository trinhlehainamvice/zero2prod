use actix_web::{web, HttpResponse, Responder};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct ConfirmTokenParam {
    pub subscription_token: String,
}

#[tracing::instrument(
    name = "Confirm a pending subscriber",
    skip(subscription_token, pg_pool)
)]
pub async fn confirm(
    web::Query(ConfirmTokenParam { subscription_token }): web::Query<ConfirmTokenParam>,
    pg_pool: web::Data<PgPool>,
) -> impl Responder {
    let subscription_id =
        match get_subscription_id_from_subscription_tokens(&subscription_token, &pg_pool).await {
            Ok(id) => id,
            Err(_) => return HttpResponse::InternalServerError().finish(),
        };

    match get_subscription_status(&subscription_id, &pg_pool).await {
        Ok(status) => {
            if status == "pending_confirmation"
                && update_subscriber_status_to_confirmed(&subscription_id, &pg_pool)
                    .await
                    .is_err()
            {
                return HttpResponse::InternalServerError().finish();
            }
            HttpResponse::Ok().finish()
        }
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[tracing::instrument(
    name = "Get subscription_id from the subscription_tokens by subscription_token"
    skip(subscription_token, pg_pool)
)]
async fn get_subscription_id_from_subscription_tokens(
    subscription_token: &str,
    pg_pool: &PgPool,
) -> Result<Uuid, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT subscription_id
        FROM subscription_tokens
        WHERE subscription_token = $1
        "#,
        subscription_token
    )
    .fetch_one(pg_pool)
    .await
    .map_err(|e| {
        tracing::error!(
            "Failed to get subscription id from subscription tokens: {}",
            e
        );
        e
    })?;

    Ok(result.subscription_id)
}

#[tracing::instrument(
name = "Get the subscription status from the subscriptions by subscription id"
skip(subscription_id, pg_pool)
)]
async fn get_subscription_status(
    subscription_id: &Uuid,
    pg_pool: &PgPool,
) -> Result<String, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT status
        FROM subscriptions
        WHERE id = $1
        "#,
        subscription_id
    )
    .fetch_one(pg_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get subscription status: {}", e);
        e
    })?;

    Ok(result.status)
}

#[tracing::instrument(
    name = "Update subscriber status to confirmed",
    skip(subscription_id, pg_pool)
)]
async fn update_subscriber_status_to_confirmed(
    subscription_id: &Uuid,
    pg_pool: &PgPool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE subscriptions
        SET status = 'confirmed'
        WHERE id = $1
        "#,
        subscription_id
    )
    .execute(pg_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update subscriber status to confirmed: {}", e);
        e
    })?;

    Ok(())
}

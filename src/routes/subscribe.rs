use actix_web::{web, HttpResponse, Responder};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Subscriber {
    name: String,
    email: String,
}

pub async fn subscribe(
    web::Form(subscriber): web::Form<Subscriber>,
    connection: web::Data<PgPool>,
) -> impl Responder {
    // When request happens in concurrently, it's hard to find out which request even each request has timestamp
    // Solve this by attach uuid to each request
    let req_id = Uuid::new_v4();

    // Tracing span will scope code structure from enter to exit
    let request_span = tracing::info_span!(
        "Register a new subscriber.",
        %req_id,
        subscriber_name = %subscriber.name,
        subscriber_email = %subscriber.email
    );
    // Tracing span enter
    let _enter_span = request_span.enter();

    // Pass Span to Instrument
    // Instrument handle to enter Span when Future is polled successfully
    let query_span = tracing::info_span!("Inserting a new subscriber to database",);
    match sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        subscriber.email,
        subscriber.name,
        Utc::now() // need to use timestamptz instead of TIMESTAMP in sql database table
    )
    .execute(connection.as_ref())
    // Attach span to instrument before await
    .instrument(query_span)
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(e) => {
            tracing::error!(
                "req_id: {} - Failed to register a new subscriber when querying database: {:?}",
                req_id,
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
}
// Tracing spans automatically exit when dropped
// NOTE: to see TRACE message when span dropped, we should set RUST_LOG=trace

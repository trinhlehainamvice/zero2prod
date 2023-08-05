use actix_web::http::header::LOCATION;
use actix_web::HttpResponse;
use sqlx::PgPool;
use std::fmt::Formatter;
use uuid::Uuid;

pub fn error_chain_fmt(e: &impl std::error::Error, f: &mut Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    // Retrieve all underlying layers errors
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

// Some tasks are CPU-intensive, they should be handled in separate threads to avoid blocking event loop thread
pub fn spawn_blocking_task_with_tracing<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // Spawn a new thread to handle this task, also new span will be created in this new thread
    // Need to pass span of thread than spawned task to block thread, for that thread can subscribe to parent span
    let current_span = tracing::Span::current();
    // Hash password algorithm consuming a lot of CPU power, may cause blocking event loop thread handle current request
    // spawn blocking task to another thread to let current event loop thread to continue to process non-blocking tasks (another requests)
    tokio::task::spawn_blocking(move || current_span.in_scope(f))
}

pub fn e500<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}

#[tracing::instrument(name = "Get username from database with user_id", skip(pg_pool))]
pub async fn get_username_from_database(
    pg_pool: &PgPool,
    user_id: &Uuid,
) -> Result<String, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT username
        FROM users
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_one(pg_pool)
    .await?;
    Ok(result.username)
}

pub fn see_other(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, location))
        .finish()
}

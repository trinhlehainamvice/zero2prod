use actix_session::Session;
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use sqlx::PgPool;
use uuid::Uuid;

fn e500<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}

pub async fn dashboard(
    session: Session,
    pg_pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = session.get::<Uuid>("user_id").map_err(e500)?;

    let username = match user_id {
        Some(id) => get_username_from_database(&pg_pool, &id)
            .await
            .map_err(e500)?,
        None => return Err(e500(anyhow::anyhow!("User ID not found in session"))),
    };

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Dashboard</title>
</head>
<body>
<p>Hello {}</p>
</body>
</html>
           "#,
            username
        )))
}

async fn get_username_from_database(
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

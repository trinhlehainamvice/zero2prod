use crate::authentication::UserId;
use crate::utils::{e500, get_username_from_database};
use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};
use sqlx::PgPool;

pub async fn admin_dashboard(
    user_id: web::ReqData<UserId>,
    pg_pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let username = get_username_from_database(&pg_pool, &user_id.into_inner())
        .await
        .map_err(e500)?;

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
<br>
<a href="/admin/newsletters">Publish Newsletter</a>
<br>
<a href="/admin/password">Change Password</a>
<br>
<a href="/admin/logout">Logout</a>
</body>
</html>
           "#,
            username
        )))
}

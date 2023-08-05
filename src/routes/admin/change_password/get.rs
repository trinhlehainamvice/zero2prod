use actix_web::http::header::ContentType;
use actix_web::HttpResponse;
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

pub async fn change_password(
    messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    let mut flash_msg = "".to_string();
    for msg in messages.iter() {
        let _ = writeln!(flash_msg, "<p><i>{}</i></p>", msg.content());
    }

    Ok(HttpResponse::Ok()
        .insert_header(ContentType::html())
        .body(format!(
            r#"
               <!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Login</title>
</head>
<body>
<form action="/admin/password" method="POST">
    {flash_msg}
    <label>Current password
        <input
                type="password"
                placeholder="Current password"
                name="current_password"
        >
    </label>
    <br>
    <label>New password
        <input
                type="password"
                placeholder="New password"
                name="new_password"
        >
    </label>
    <br>
    <label>Confirm password
        <input
                type="password"
                placeholder="Confirm password"
                name="confirm_password"
        >
    </label>
    <br>
    <button type="submit">Confirm</button>
    <br>
    <a href="/admin/dashboard">Back</a> 
</form>
</body>
</html>
            "#
        )))
}

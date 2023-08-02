use actix_web::http::header::ContentType;
use actix_web::HttpResponse;
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

pub async fn login_form(messages: IncomingFlashMessages) -> HttpResponse {
    let mut error = "".to_string();
    for msg in messages.iter() {
        let _ = writeln!(error, "<p><i>{}</i></p>", msg.content());
    }

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"
               <!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Login</title>
</head>
<body>
<form action="/login" method="POST">
    {error}
    <label>Username
        <input
                type="text"
                placeholder="Enter username"
                name="username"
        >
    </label>
    <label>Password
        <input
                type="password"
                placeholder="Enter password"
                name="password"
        >
    </label>
    <button type="submit">Login</button>
</form>
</body>
</html>"#
        ))
}

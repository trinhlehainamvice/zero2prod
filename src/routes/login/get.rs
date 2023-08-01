use actix_web::http::header::ContentType;
use actix_web::{web, HttpResponse};

#[derive(serde::Deserialize)]
pub struct ErrorParam {
    // Make a param optional, mean query still passed even if this param is missing
    error: Option<String>,
}

#[tracing::instrument(name = "Login Page", skip_all)]
pub async fn login_form(web::Query(ErrorParam { error }): web::Query<ErrorParam>) -> HttpResponse {
    let error_html = match error {
        None => String::new(),
        Some(error) => format!(
            "<p><i>{}</i></p>",
            // Remove untrusted characters that can be used in HTML content and executed in browser
            // Known as XSS (Cross Site Scripting)
            // For example: https://www.example.com/search?q=<script>alert('XSS Attack!')</script>
            // Just like SQL injection, attacker injects untrusted characters into the query that lead to unexpected query execution
            htmlescape::encode_minimal(&error)
        ),
    };

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
    {error_html}
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
